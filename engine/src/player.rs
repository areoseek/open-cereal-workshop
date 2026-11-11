//! The audio side: plays a `Song` snapshot, sample-accurately, into a stereo mixer.
//!
//! The UI owns a `Handle`; the audio callback owns the `Player`. They share the song through an atomic pointer
//! swap, the transport through atomics, and live notes through a queue the audio thread only ever try-locks.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::flp::ChannelKind;
use crate::fx;
use crate::law;
use crate::sample::Sample;
use crate::song::{Env, ClipSrc, Note, Song, Target};

pub const MAX_VOICES: usize = 256;
pub const MAX_METERS: usize = 256;
const BLOCK: usize = 256;

/// Things the UI asks of the audio thread outside the song itself.
pub enum Cmd {
    NoteOn { chan: u16, key: u8, vel: f32 },
    NoteOff { chan: u16, key: u8 },
    /// Audition a sample (browser preview).
    Preview(Arc<Sample>),
    StopPreview,
    /// Silence everything now.
    Panic,
}

/// Play state both threads can see.
pub struct Transport {
    playing: AtomicBool,
    song_mode: AtomicBool,
    looping: AtomicBool,
    pattern: AtomicUsize,
    tick: AtomicU64,
    seek: AtomicI64,
    master: AtomicU32,
    peaks: Vec<[AtomicU32; 2]>,
    voices: AtomicUsize,
    tempo: AtomicU32,
    metronome: AtomicBool,
}

impl Transport {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            playing: AtomicBool::new(false),
            song_mode: AtomicBool::new(false),
            looping: AtomicBool::new(true),
            pattern: AtomicUsize::new(0),
            tick: AtomicU64::new(0f64.to_bits()),
            seek: AtomicI64::new(-1),
            master: AtomicU32::new(1f32.to_bits()),
            peaks: (0..MAX_METERS).map(|_| [AtomicU32::new(0), AtomicU32::new(0)]).collect(),
            voices: AtomicUsize::new(0),
            tempo: AtomicU32::new(0f32.to_bits()),
            metronome: AtomicBool::new(false),
        })
    }
    pub fn playing(&self) -> bool {
        self.playing.load(Relaxed)
    }
    pub fn set_playing(&self, p: bool) {
        self.playing.store(p, Relaxed);
    }
    pub fn song_mode(&self) -> bool {
        self.song_mode.load(Relaxed)
    }
    pub fn set_song_mode(&self, s: bool) {
        self.song_mode.store(s, Relaxed);
    }
    pub fn looping(&self) -> bool {
        self.looping.load(Relaxed)
    }
    pub fn set_looping(&self, l: bool) {
        self.looping.store(l, Relaxed);
    }
    /// The pattern that plays in pattern mode (0-based).
    pub fn pattern(&self) -> usize {
        self.pattern.load(Relaxed)
    }
    pub fn set_pattern(&self, p: usize) {
        self.pattern.store(p, Relaxed);
    }
    /// Play position in ticks.
    pub fn tick(&self) -> f64 {
        f64::from_bits(self.tick.load(Relaxed))
    }
    /// Move the play position.
    pub fn seek(&self, tick: u32) {
        self.seek.store(tick as i64, Relaxed);
        self.tick.store((tick as f64).to_bits(), Relaxed);
    }
    pub fn master(&self) -> f32 {
        f32::from_bits(self.master.load(Relaxed))
    }
    pub fn set_master(&self, v: f32) {
        self.master.store(v.clamp(0.0, 1.25).to_bits(), Relaxed);
    }
    /// Read and reset the peak meter of an insert (0 = master).
    pub fn take_peak(&self, insert: usize) -> (f32, f32) {
        match self.peaks.get(insert) {
            Some(p) => (f32::from_bits(p[0].swap(0, Relaxed)), f32::from_bits(p[1].swap(0, Relaxed))),
            None => (0.0, 0.0),
        }
    }
    /// Tempo in force while playing (automation can move it); 0 before anything has played.
    pub fn tempo(&self) -> f32 {
        f32::from_bits(self.tempo.load(Relaxed))
    }
    pub fn metronome(&self) -> bool {
        self.metronome.load(Relaxed)
    }
    pub fn set_metronome(&self, on: bool) {
        self.metronome.store(on, Relaxed)
    }
    pub fn voices(&self) -> usize {
        self.voices.load(Relaxed)
    }
}

/// The UI's end.
pub struct Handle {
    pub transport: Arc<Transport>,
    song: Arc<ArcSwap<Song>>,
    cmds: Arc<Mutex<Vec<Cmd>>>,
    /// Old snapshots are kept a little while so they are freed here and not on the audio thread.
    graveyard: Vec<Arc<Song>>,
}

impl Handle {
    /// Give the audio thread a new version of the song.
    pub fn publish(&mut self, mut song: Song) {
        song.prepare();
        let old = self.song.swap(Arc::new(song));
        self.graveyard.push(old);
        if self.graveyard.len() > 8 {
            self.graveyard.remove(0);
        }
    }
    pub fn current(&self) -> Arc<Song> {
        self.song.load_full()
    }
    pub fn send(&self, c: Cmd) {
        if let Ok(mut q) = self.cmds.lock() {
            q.push(c);
        }
    }

    /// A second way in for commands, for threads other than the window's (MIDI input).
    pub fn sender(&self) -> CmdSender {
        CmdSender(self.cmds.clone())
    }
}

/// Sends commands to the audio thread from anywhere.
#[derive(Clone)]
pub struct CmdSender(Arc<Mutex<Vec<Cmd>>>);

impl CmdSender {
    pub fn send(&self, c: Cmd) {
        if let Ok(mut q) = self.0.lock() {
            q.push(c);
        }
    }
}

/// Make the two ends.
pub fn new_player(mut song: Song, sample_rate: f32) -> (Handle, Player) {
    song.prepare();
    let song = Arc::new(ArcSwap::from_pointee(song));
    let transport = Transport::new();
    let cmds = Arc::new(Mutex::new(Vec::with_capacity(64)));
    let handle = Handle { transport: transport.clone(), song: song.clone(), cmds: cmds.clone(), graveyard: vec![] };
    let player = Player {
        song,
        transport,
        cmds,
        inbox: Vec::with_capacity(64),
        sr: sample_rate,
        voices: (0..MAX_VOICES).map(|_| Voice::default()).collect(),
        buses: vec![],
        eq: vec![],
        fx: vec![],
        driven: Vec::with_capacity(64),
        ring: vec![],
        chan_fx: vec![],
        scratch: Vec::with_capacity(BLOCK),
        cap: Vec::new(),
        cap_at: Vec::new(),
        blockno: 0,
        dirty: vec![],
        pos: 0.0,
        was_playing: false,
        chase: false,
        born: 0,
        ov: Overrides::default(),
        tpf: 130.0 * 96.0 / 60.0 / sample_rate as f64,
        clicks: [click(sample_rate, 1760.0), click(sample_rate, 880.0)],
    };
    (handle, player)
}

/// Control values set by automation clips; they stand in for the song's own values until playback restarts.
#[derive(Default)]
struct Overrides {
    tempo: Option<f32>,
    /// Per channel: volume (0..=1) and pan (-1..=1).
    chan: Vec<(Option<f32>, Option<f32>)>,
    /// Per insert: volume (0..=1.25) and pan.
    ins: Vec<(Option<f32>, Option<f32>)>,
    any_chan: bool,
    /// Per channel: pitch knob in cents.
    pitch: Vec<Option<f32>>,
    /// Effect slot controls being driven: the target and its 0..=1 value.
    slot: Vec<(Target, f32)>,
}

impl Overrides {
    fn clear(&mut self) {
        self.tempo = None;
        self.any_chan = false;
        self.slot.clear();
        self.pitch.iter_mut().for_each(|c| *c = None);
        self.chan.iter_mut().for_each(|c| *c = (None, None));
        self.ins.iter_mut().for_each(|c| *c = (None, None));
    }

    fn fit(&mut self, song: &Song) {
        if self.chan.len() != song.channels.len() {
            self.chan.resize(song.channels.len(), (None, None));
        }
        if self.pitch.len() != song.channels.len() {
            self.pitch.resize(song.channels.len(), None);
        }
        if self.ins.len() != song.inserts.len() {
            self.ins.resize(song.inserts.len(), (None, None));
        }
    }

    fn chan_volume(&self, song: &Song, c: usize) -> f32 {
        self.chan.get(c).and_then(|o| o.0).unwrap_or_else(|| song.channels[c].volume)
    }

    fn chan_pan(&self, song: &Song, c: usize) -> f32 {
        self.chan.get(c).and_then(|o| o.1).unwrap_or_else(|| song.channels[c].pan)
    }

    fn set(&mut self, t: Target, v: f32) {
        let v = v.clamp(0.0, 1.0);
        match t {
            Target::ChanVolume(c) => {
                if let Some(o) = self.chan.get_mut(c) {
                    o.0 = Some(v);
                    self.any_chan = true;
                }
            }
            Target::ChanPan(c) => {
                if let Some(o) = self.chan.get_mut(c) {
                    o.1 = Some(v * 2.0 - 1.0);
                    self.any_chan = true;
                }
            }
            Target::ChanPitch(c) => {
                if let Some(o) = self.pitch.get_mut(c) {
                    *o = Some((v * 2.0 - 1.0) * 200.0);
                    self.any_chan = true;
                }
            }
            Target::InsertVolume(i) => {
                if let Some(o) = self.ins.get_mut(i) {
                    o.0 = Some(v * 1.25);
                }
            }
            Target::InsertPan(i) => {
                if let Some(o) = self.ins.get_mut(i) {
                    o.1 = Some(v * 2.0 - 1.0);
                }
            }
            // the tempo control runs from 60 to 180 bpm
            Target::Tempo => self.tempo = Some(60.0 + v * 120.0),
            Target::SlotMix(..) | Target::SlotOn(..) | Target::SlotParam(..) => match self.slot.iter_mut().find(|e| e.0 == t) {
                Some(e) => e.1 = v,
                None => self.slot.push((t, v)),
            },
            Target::Other { .. } => {}
        }
    }
}

/// A slot's switch and mix for the block being run, after automation.
struct SlotNow {
    enabled: bool,
    mix: f32,
}

/// A short decaying blip for the metronome.
fn click(rate: f32, hz: f32) -> Arc<Sample> {
    let n = (rate * 0.03) as usize;
    let data: Vec<f32> = (0..n).map(|i| (i as f32 / rate * hz * std::f32::consts::TAU).sin() * (1.0 - i as f32 / n as f32).powi(2) * 0.6).collect();
    Arc::new(Sample::from_mono("click", rate as u32, &data))
}

/// Frames between envelope and filter updates.
const CTL: u32 = 32;

#[derive(Default)]
struct Voice {
    active: bool,
    sample: Option<Arc<Sample>>,
    pos: f64,
    /// Frames the reading wraps between while the note sounds.
    loop_pts: Option<(f64, f64)>,
    /// Source frames per output frame at the note's own pitch, and where a glide is taking it.
    step: f64,
    step_to: f64,
    glide: f64,
    /// `step` with the pitch envelope applied.
    step_now: f64,
    phase: f32,
    dphase: f32,
    lp: f32,
    /// A built-in instrument's settings and running state.
    tone: Option<crate::synth::Tone>,
    tstate: crate::synth::ToneState,
    /// Frames the instrument's own envelopes still need after the key is let go.
    tail: u32,
    gl: f32,
    gr: f32,
    insert: usize,
    /// Frames to wait before sounding (places the start inside a block).
    delay: u32,
    /// Frames until the note is let go; None plays the sample out.
    remaining: Option<u64>,
    level: f32,
    attack: f32,
    release: f32,
    releasing: bool,
    /// Stop with the short linear fade whatever the envelope says.
    kill: bool,
    chan: u16,
    key: u8,
    live: bool,
    preview: bool,
    born: u64,
    /// Gain and pan the note itself brought, kept so channel automation can move a sounding voice.
    note_gain: f32,
    note_pan: f32,
    follow: bool,
    /// For a channel that is a patch bay: 0 for its own instrument, else one more than the layer played.
    feed: u8,
    /// Octaves the channel's pitch automation has moved the note by.
    bend: f32,
    sr: f32,
    /// Frames sounded, and frames since the note was let go.
    t: u32,
    rel_t: u32,
    rel_from: f32,
    ctl: u32,
    venv: Env,
    penv: Env,
    cenv: Env,
    env_now: f32,
    env_inc: f32,
    /// Filter: type, knob positions, coefficients and state (two stages, two channels).
    ftype: u8,
    fcut: f32,
    fres: f32,
    fon: bool,
    fg: f32,
    fk: f32,
    fs: [[f32; 2]; 4],
    /// Time stretch: source frames used per output frame, while `step` stays the pitch.
    stretch: bool,
    rate: f64,
    hop: u32,
    hop_t: u32,
    a_pos: f64,
    b_pos: f64,
}

impl Voice {
    fn let_go(&mut self) {
        if !self.releasing {
            self.releasing = true;
            self.rel_from = self.env_now;
            self.rel_t = 0;
            self.ctl = 0;
            if let Some(tone) = &self.tone {
                self.tail = (self.tstate.release(tone) * self.sr) as u32;
            }
        }
        self.remaining = None;
    }

    /// Work out the envelopes and the filter for the next few frames.
    fn control(&mut self) {
        let t = self.t as f32 / self.sr;
        if self.venv.on {
            let target = if self.releasing {
                self.venv.released(self.rel_from, self.rel_t as f32 / self.sr).unwrap_or(-1.0)
            } else {
                self.venv.held(t)
            };
            if target < 0.0 {
                if !self.kill {
                    self.active = false;
                }
                self.env_inc = -self.env_now / CTL as f32;
            } else if self.t == 0 {
                self.env_now = target;
                self.env_inc = 0.0;
            } else {
                self.env_inc = (target - self.env_now) / CTL as f32;
            }
        } else {
            self.env_now = 1.0;
            self.env_inc = 0.0;
        }
        if self.step_to != self.step {
            self.step += (self.step_to - self.step) * self.glide;
            if (self.step_to - self.step).abs() < self.step_to * 1e-4 {
                self.step = self.step_to;
            }
        }
        self.step_now = if self.penv.on && self.penv.amount != 0.0 {
            self.step * ((self.penv.amount * self.penv.held(t)) as f64 / 1200.0).exp2()
        } else {
            self.step
        };
        if self.bend != 0.0 {
            self.step_now *= (self.bend as f64).exp2();
        }
        if self.fon {
            let mut x = self.fcut;
            if self.cenv.on {
                x += self.cenv.amount * self.cenv.held(t);
            }
            let hz = (20.0 * 1000f32.powf(x.clamp(0.0, 1.0))).min(self.sr * 0.45);
            self.fg = (std::f32::consts::PI * hz / self.sr).tan();
            self.fk = 1.0 / (0.707 + self.fres * 10.0);
        }
    }

    #[inline]
    fn filter(&mut self, x: [f32; 2]) -> [f32; 2] {
        let (g, k) = (self.fg, self.fk);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let stages = if matches!(self.ftype, 5 | 7) { 2 } else { 1 };
        let mut x = x;
        for st in 0..stages {
            for c in 0..2 {
                let [ic1, ic2] = self.fs[st * 2 + c];
                let v3 = x[c] - ic2;
                let v1 = a1 * ic1 + a2 * v3;
                let v2 = ic2 + a2 * ic1 + a3 * v3;
                self.fs[st * 2 + c] = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
                x[c] = match self.ftype {
                    2 => v1 * k,
                    3 => x[c] - k * v1 - v2,
                    4 => x[c] - k * v1,
                    _ => v2,
                };
            }
        }
        x
    }

    fn render(&mut self, bus: &mut [[f32; 2]]) -> bool {
        let mut wrote = false;
        for out in bus.iter_mut() {
            if self.delay > 0 {
                self.delay -= 1;
                continue;
            }
            if let Some(r) = self.remaining.as_mut() {
                if *r == 0 {
                    self.let_go();
                } else {
                    *r -= 1;
                }
            }
            if self.ctl == 0 {
                self.control();
                self.ctl = CTL;
                if !self.active {
                    break;
                }
            }
            self.ctl -= 1;
            if self.releasing && self.tail > 0 && !self.kill {
                self.tail -= 1;
            } else if self.releasing && (self.kill || !self.venv.on) {
                self.level -= self.release;
                if self.level <= 0.0 {
                    self.active = false;
                    break;
                }
            } else if self.level < 1.0 {
                self.level = (self.level + self.attack).min(1.0);
            }
            let mut s = match &self.sample {
                Some(smp) => {
                    if self.pos >= smp.frames.len() as f64 {
                        self.active = false;
                        break;
                    }
                    if self.stretch {
                        // two overlapping grains read at the note's pitch while the read point moves at `rate`
                        let w = self.hop_t as f32 / self.hop as f32;
                        let wb = 0.5 - 0.5 * (std::f32::consts::PI * w).cos();
                        let (sa, sb) = (smp.at(self.a_pos), smp.at(self.b_pos));
                        self.a_pos += self.step_now;
                        self.b_pos += self.step_now;
                        self.pos += self.rate;
                        self.hop_t += 1;
                        if self.hop_t >= self.hop {
                            self.hop_t = 0;
                            self.a_pos = self.b_pos;
                            self.b_pos = self.pos;
                        }
                        [sa[0] + (sb[0] - sa[0]) * wb, sa[1] + (sb[1] - sa[1]) * wb]
                    } else {
                        let s = smp.at(self.pos);
                        self.pos += self.step_now;
                        if let Some((a, b)) = self.loop_pts {
                            if self.pos >= b {
                                self.pos = a + (self.pos - b) % (b - a).max(1.0);
                            }
                        }
                        s
                    }
                }
                None if self.tone.is_some() => {
                    let tone = self.tone.as_ref().unwrap();
                    self.tstate.frame(tone, self.dphase * self.step_now as f32)
                }
                None => {
                    // stand-in voice for generators we cannot run yet: a softened saw
                    self.phase += self.dphase;
                    if self.phase >= 1.0 {
                        self.phase -= 1.0;
                    }
                    let saw = self.phase * 2.0 - 1.0;
                    let tri = 2.0 * saw.abs() - 1.0;
                    let x = saw * 0.5 + tri * 0.5;
                    self.lp += (x - self.lp) * 0.25;
                    [self.lp * 0.3, self.lp * 0.3]
                }
            };
            if self.fon {
                s = self.filter(s);
            }
            let g = self.level * self.env_now;
            self.env_now += self.env_inc;
            out[0] += s[0] * self.gl * g;
            out[1] += s[1] * self.gr * g;
            self.t += 1;
            if self.releasing {
                self.rel_t += 1;
            }
            wrote = true;
        }
        if !self.active {
            self.sample = None;
        }
        wrote
    }
}

/// Lives on the audio thread.
pub struct Player {
    song: Arc<ArcSwap<Song>>,
    transport: Arc<Transport>,
    cmds: Arc<Mutex<Vec<Cmd>>>,
    inbox: Vec<Cmd>,
    sr: f32,
    voices: Vec<Voice>,
    buses: Vec<Vec<[f32; 2]>>,
    /// Equaliser memory per insert and band: two values per side.
    eq: Vec<[[f32; 4]; 3]>,
    /// Running effect units per insert, in slot order.
    fx: Vec<Vec<(u16, fx::Unit)>>,
    /// Controller outputs waiting to be applied.
    driven: Vec<(Target, f32)>,
    /// Frames an insert keeps running after its input stops, for effect tails.
    ring: Vec<usize>,
    /// The patch bay of each channel whose generator is one, with the sound its notes made this block.
    chan_fx: Vec<Option<ChanFx>>,
    scratch: Vec<[f32; 2]>,
    /// What each insert put out this block, kept for units on other inserts that listen to it.
    cap: Vec<Vec<[f32; 2]>>,
    cap_at: Vec<u64>,
    blockno: u64,
    dirty: Vec<bool>,
    pos: f64,
    was_playing: bool,
    /// Set after a start or a seek: clips already under the play head must be picked up mid-way.
    chase: bool,
    born: u64,
    ov: Overrides,
    /// Ticks per frame at the tempo of the block being played.
    tpf: f64,
    /// Metronome sounds: bar, beat.
    clicks: [Arc<Sample>; 2],
}

impl Player {
    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    fn start(&mut self, mut v: Voice) {
        self.born += 1;
        v.born = self.born;
        v.active = true;
        v.sr = self.sr;
        if v.step_to == 0.0 {
            v.step_to = v.step;
        }
        v.step_now = v.step;
        if !v.venv.on {
            v.env_now = 1.0;
        }
        let slot = match self.voices.iter().position(|x| !x.active) {
            Some(i) => i,
            None => self.voices.iter().enumerate().min_by_key(|(_, x)| x.born).map(|(i, _)| i).unwrap_or(0),
        };
        self.voices[slot] = v;
    }

    /// Start one note. `len_frames` None means "no note-off" (a step, or a live key still held).
    fn note_on(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool) {
        self.note_on_depth(song, n, delay, len_frames, live, 0);
    }

    fn note_on_depth(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool, depth: u8) {
        let Some(ch) = song.channels.get(n.chan as usize) else { return };
        if !ch.enabled || n.muted {
            return;
        }
        if ch.kind == ChannelKind::Layer {
            // a layer plays its children with the same note
            if depth < 4 {
                for &k in &ch.children {
                    if k != n.chan {
                        self.note_on_depth(song, &Note { chan: k, ..n.clone() }, delay, len_frames, live, depth + 1);
                    }
                }
            }
            return;
        }
        let f = ch.smp;
        let sr = self.sr;
        let ms = |m: f32| 1.0 / (sr * m / 1000.0).max(1.0);
        let chan_vol = law::channel_gain(self.ov.chan_volume(song, n.chan as usize));
        let chan_pan = self.ov.chan_pan(song, n.chan as usize);
        match ch.kind {
            ChannelKind::Sampler | ChannelKind::AudioClip => {
                let Some(smp) = ch.sample.clone() else { return };
                let cents = (n.key as f32 - ch.root_key as f32) * 100.0 + ch.pitch_cents + n.fine_cents + f.stretch_cents;
                let step = smp.rate as f64 / sr as f64 * 2f64.powf(cents as f64 / 1200.0);
                let venv = Env { on: ch.env_on, ..f.vol_env };
                // without a volume envelope a sampler plays its sample out whatever the note length
                let remaining = if ch.env_on || live { len_frames } else { None };
                // a note in a cut group, or on a one-voice channel, stops what it replaces at the moment it starts
                let fade = ms(4.0);
                let mut glided = false;
                for v in self.voices.iter_mut().filter(|v| v.active && !v.preview && !v.releasing && v.sample.is_some()) {
                    let same = v.chan == n.chan;
                    let cut = f.cut != 0 && song.channels.get(v.chan as usize).map(|c| c.smp.cut_by == f.cut).unwrap_or(false);
                    if same && f.mono && f.porta && !glided && v.delay == 0 {
                        // portamento: the sounding voice slides to the new note instead of starting again
                        v.step_to = step;
                        v.glide = 1.0 - (-(CTL as f64) / (f.slide.max(0.001) as f64 * sr as f64 / 3.0)).exp();
                        v.key = n.key;
                        v.remaining = remaining.map(|r| r + delay as u64);
                        glided = true;
                    } else if (same && f.mono) || cut {
                        v.kill = true;
                        v.release = v.release.max(fade);
                        v.remaining = Some(v.remaining.unwrap_or(u64::MAX).min(delay as u64));
                    }
                }
                if glided {
                    return;
                }
                let filter_on = !(matches!(f.filter, 0 | 1 | 5 | 6 | 7) && f.cutoff >= 0.995 && !f.cut_env.on);
                for k in 0..=(if f.echo_feedback > 0.0 { f.echoes as u32 } else { 0 }) {
                    let fall = f.echo_feedback.powi(k as i32);
                    let note_gain = law::velocity_gain(n.vel) * f.gain * fall;
                    let note_pan = (n.pan + f.echo_pan * k as f32).clamp(-1.0, 1.0);
                    let (pl, pr) = law::pan((chan_pan + note_pan).clamp(-1.0, 1.0));
                    let gain = chan_vol * note_gain;
                    let late = (k as f64 * f.echo_steps as f64 * song.step() as f64 / self.tpf.max(1e-9)) as u32;
                    self.start(Voice {
                        sample: Some(smp.clone()),
                        step: step * 2f64.powf((f.echo_cents * k as f32) as f64 / 1200.0),
                        level: 1.0,
                        attack: 1.0,
                        release: ms(8.0),
                        remaining,
                        venv,
                        penv: f.pitch_env,
                        cenv: f.cut_env,
                        ftype: f.filter,
                        fcut: f.cutoff,
                        fres: f.resonance,
                        fon: filter_on,
                        gl: gain * pl,
                        gr: gain * pr,
                        insert: ch.insert,
                        delay: delay + late,
                        chan: n.chan,
                        key: n.key,
                        live,
                        note_gain,
                        note_pan,
                        follow: true,
                        ..Default::default()
                    });
                }
            }
            ChannelKind::Plugin | ChannelKind::Other(_) if ch.zones.is_some() => {
                // a sample map: every zone the key and velocity fall in sounds its recording
                let Some(map) = ch.zones.clone() else { return };
                let vel = (n.vel * 100.0).round().clamp(0.0, 127.0) as u8;
                let fade = ms(4.0);
                for v in self.voices.iter_mut().filter(|v| v.active && !v.preview && !v.releasing && v.chan == n.chan && v.sample.is_some()) {
                    if map.cuts(n.key, v.key) {
                        v.kill = true;
                        v.release = v.release.max(fade);
                        v.remaining = Some(v.remaining.unwrap_or(u64::MAX).min(delay as u64));
                    }
                }
                let last = song.inserts.len().saturating_sub(1);
                for z in map.hit(n.key, vel) {
                    let Some(smp) = z.sample.clone() else { continue };
                    let keys = if z.track { (n.key as i32 - z.root) as f32 * 100.0 } else { 0.0 };
                    let step = smp.rate as f64 / sr as f64 * 2f64.powf((keys + z.tune_cents + ch.pitch_cents + n.fine_cents) as f64 / 1200.0);
                    let loop_pts = z.looped.filter(|l| l.1 > l.0 && l.1 <= smp.frames.len()).map(|l| (l.0 as f64, l.1 as f64));
                    // a held zone needs a length; a step with none gets a short one
                    let held = if live { len_frames } else { Some(len_frames.unwrap_or((sr * 0.12) as u64).max(1)) };
                    let note_gain = if z.vel_scale { law::velocity_gain(n.vel) } else { 1.0 };
                    let (pl, pr) = law::pan((chan_pan + n.pan + z.pan).clamp(-1.0, 1.0));
                    let gain = chan_vol * note_gain * z.gain;
                    self.start(Voice {
                        sample: Some(smp),
                        loop_pts,
                        step,
                        step_to: step,
                        step_now: step,
                        level: 0.0,
                        attack: ms(z.attack * 1000.0),
                        release: ms(z.release * 1000.0),
                        remaining: if z.one_shot { None } else { held },
                        gl: gain * pl,
                        gr: gain * pr,
                        insert: if z.out > 0 { (ch.insert + z.out).min(last) } else { ch.insert },
                        delay,
                        chan: n.chan,
                        key: n.key,
                        live: live && !z.one_shot,
                        note_gain,
                        note_pan: n.pan,
                        follow: z.track,
                        ..Default::default()
                    });
                }
            }
            ChannelKind::Plugin | ChannelKind::Other(_) if ch.slices.is_some() => {
                // a loop slicer: the key picks a slice of the loop, which plays through at the channel's pitch
                let (Some(sl), Some(smp)) = (&ch.slices, ch.sample.clone()) else { return };
                let scale = smp.rate as f64 / sl.rate.max(1) as f64;
                let total = (smp.frames.len() as f64 / scale) as usize;
                let Some((start, len, _reverse)) = sl.for_key(n.key, total) else { return };
                let step = smp.rate as f64 / sr as f64 * 2f64.powf((ch.pitch_cents + n.fine_cents) as f64 / 1200.0);
                let note_gain = law::velocity_gain(n.vel);
                let note_pan = n.pan;
                let (pl, pr) = law::pan((chan_pan + note_pan).clamp(-1.0, 1.0));
                let gain = chan_vol * note_gain;
                let frames = (len as f64 * scale / step) as u64;
                self.start(Voice {
                    sample: Some(smp),
                    pos: start as f64 * scale,
                    step,
                    step_to: step,
                    step_now: step,
                    level: 0.0,
                    attack: ms(1.0),
                    release: ms(6.0),
                    remaining: Some(frames.saturating_sub((sr * 0.006) as u64).max(1)),
                    gl: gain * pl,
                    gr: gain * pr,
                    insert: ch.insert,
                    delay,
                    chan: n.chan,
                    key: n.key,
                    live: false,
                    note_gain,
                    note_pan,
                    follow: true,
                    ..Default::default()
                });
            }
            ChannelKind::Plugin | ChannelKind::Other(_) => {
                let note_gain = law::velocity_gain(n.vel);
                let gain = chan_vol * note_gain;
                let (pl, pr) = law::pan((chan_pan + n.pan).clamp(-1.0, 1.0));
                let hz = 440.0 * 2f32.powf((n.key as f32 - 69.0) / 12.0 + (ch.pitch_cents + n.fine_cents) / 1200.0);
                let min_len = (sr * 0.12) as u64;
                let remaining = if live { len_frames } else { Some(len_frames.unwrap_or(min_len).max(1)) };
                let layers = ch.bay().map_or(&[][..], |b| &b.layers[..]);
                for li in 0..=layers.len() {
                let g = if li == 0 { ch.gen.as_ref() } else { Some(&layers[li - 1].1) };
                let mut v = Voice { dphase: hz / sr, level: 0.0, attack: ms(4.0), release: ms(90.0), remaining, gl: gain * pl, gr: gain * pr, insert: ch.insert, delay, chan: n.chan, key: n.key, live, note_gain, note_pan: n.pan, follow: true, ..Default::default() };
                v.feed = li as u8;
                if let Some(g) = g {
                    // a built-in instrument: its own oscillators, shaped by the channel's envelopes and filter
                    let tone = g.tone();
                    v.tstate = crate::synth::ToneState::start(&tone, (self.born as u32).wrapping_mul(2654435761).wrapping_add(n.key as u32));
                    v.tstate.note(&tone, sr, n.key, note_gain.min(1.0), hz / sr);
                    v.attack = ms(1.5);
                    v.release = ms(12.0);
                    if let Some(p) = &tone.add {
                        // this instrument brings its own rise and fall
                        v.attack = ms(p.attack_s() * 1000.0);
                        v.release = ms(p.release_s() * 1000.0);
                    }
                    v.tone = Some(tone);
                    v.step = 1.0;
                    v.step_to = 1.0;
                    v.step_now = 1.0;
                    v.venv = Env { on: ch.env_on, ..f.vol_env };
                    if ch.env_on {
                        v.level = 1.0;
                        v.attack = 1.0;
                    }
                    v.penv = f.pitch_env;
                    v.cenv = f.cut_env;
                    v.ftype = f.filter;
                    v.fcut = f.cutoff;
                    v.fres = f.resonance;
                    v.fon = !(matches!(f.filter, 0 | 1 | 5 | 6 | 7) && f.cutoff >= 0.995 && !f.cut_env.on);
                }
                self.start(v);
                }
            }
            ChannelKind::Layer | ChannelKind::AutomationClip => {}
        }
    }

    fn note_off(&mut self, song: &Song, chan: u16, key: u8) {
        if let Some(ch) = song.channels.get(chan as usize) {
            if ch.kind == ChannelKind::Layer {
                for &k in &ch.children {
                    if song.channels.get(k as usize).map(|c| c.kind != ChannelKind::Layer).unwrap_or(false) {
                        self.note_off(song, k, key);
                    }
                }
                return;
            }
        }
        for v in self.voices.iter_mut().filter(|v| v.active && v.live && v.chan == chan && v.key == key) {
            v.let_go();
        }
    }

    fn silence(&mut self, fast: bool) {
        let rel = 1.0 / (self.sr * if fast { 0.004 } else { 0.03 });
        for v in self.voices.iter_mut().filter(|v| v.active && !v.preview) {
            v.kill = true;
            v.release = v.release.max(rel);
            v.let_go();
            if v.delay > 0 {
                v.active = false;
            }
        }
    }

    fn take_commands(&mut self, song: &Song) {
        if let Ok(mut q) = self.cmds.try_lock() {
            if q.is_empty() {
                return;
            }
            std::mem::swap(&mut *q, &mut self.inbox);
        } else {
            return;
        }
        let mut inbox = std::mem::take(&mut self.inbox);
        for c in inbox.drain(..) {
            match c {
                Cmd::NoteOn { chan, key, vel } => {
                    let n = Note { pos: 0, len: 0, chan, key, vel, pan: 0.0, fine_cents: 0.0, muted: false };
                    self.note_on(song, &n, 0, None, true);
                }
                Cmd::NoteOff { chan, key } => self.note_off(song, chan, key),
                Cmd::Preview(smp) => {
                    for v in self.voices.iter_mut().filter(|v| v.preview) {
                        v.let_go();
                    }
                    let step = smp.rate as f64 / self.sr as f64;
                    let rel = 1.0 / (self.sr * 0.01);
                    self.start(Voice { sample: Some(smp), step, level: 1.0, attack: 1.0, release: rel, gl: 0.8, gr: 0.8, insert: 0, preview: true, ..Default::default() });
                }
                Cmd::StopPreview => {
                    for v in self.voices.iter_mut().filter(|v| v.preview) {
                        v.let_go();
                    }
                }
                Cmd::Panic => {
                    for v in self.voices.iter_mut() {
                        v.active = false;
                        v.sample = None;
                    }
                }
            }
        }
        self.inbox = inbox;
    }

    /// Trigger the notes of one pattern whose pattern-local tick lies in [a, b). `t0` is the song tick at the
    /// start of the segment and `shift` maps pattern ticks onto song ticks. Notes are cut at `cut` (pattern
    /// ticks) when given.
    #[allow(clippy::too_many_arguments)]
    fn fire_pattern(&mut self, song: &Song, pat: usize, a: f64, b: f64, shift: f64, t0: f64, tpf: f64, cut: Option<u32>) {
        let Some(p) = song.patterns.get(pat) else { return };
        let first = a.ceil().max(0.0) as u32;
        let start = p.notes.partition_point(|n| n.pos < first);
        for n in &p.notes[start..] {
            if n.pos as f64 >= b {
                break;
            }
            // swing pushes every second step late, up to a third of a step
            let mut at = n.pos as f64;
            if song.swing > 0.0 && (n.pos / song.step().max(1)) % 2 == 1 {
                let share = song.channels.get(n.chan as usize).map(|c| c.smp.swing).unwrap_or(1.0);
                at += (song.swing * share) as f64 * song.step() as f64 / 3.0;
            }
            let delay = ((((at + shift) - t0) / tpf) - 1e-6).ceil().max(0.0) as u32;
            let len = if n.len == 0 {
                None
            } else {
                let end = match cut {
                    Some(c) => (n.pos + n.len).min(c),
                    None => n.pos + n.len,
                };
                Some((end.saturating_sub(n.pos) as f64 / tpf) as u64)
            };
            self.note_on(song, n, delay, len, false);
        }
    }

    /// Start whatever begins in the next `n` frames; returns how many frames to render before calling again.
    fn advance(&mut self, song: &Song, n: usize) -> usize {
        let tr = self.transport.clone();
        let playing = tr.playing();
        let sought = tr.seek.swap(-1, Relaxed);
        if sought >= 0 {
            self.pos = sought as f64;
            self.chase = true;
            if playing {
                self.silence(true);
            }
        }
        if playing && !self.was_playing {
            self.chase = true;
            self.ov.clear();
        }
        if !playing && self.was_playing {
            self.silence(false);
        }
        self.was_playing = playing;
        if !playing {
            return n;
        }
        let song_mode = tr.song_mode();
        let pat = tr.pattern();
        self.ov.fit(song);
        if song_mode {
            for plan in &song.auto_plan {
                let Some(auto) = song.channels.get(plan.chan).and_then(|c| c.auto.as_ref()) else { continue };
                // the clip under the play head, or the last one before it (its end value holds)
                let k = plan.clips.partition_point(|c| c.0 as f64 <= self.pos);
                if k == 0 {
                    continue;
                }
                let (pos, len, offset) = plan.clips[k - 1];
                let local = (self.pos - pos as f64).min(len as f64) + offset as f64;
                let v = auto.value_at(local);
                for &t in &auto.targets {
                    self.ov.set(t, v);
                }
            }
        }
        let tempo = self.ov.tempo.unwrap_or(song.tempo).max(1.0);
        tr.tempo.store(tempo.to_bits(), Relaxed);
        let tpf = (tempo as f64 * song.ppq as f64 / 60.0 / self.sr as f64).max(1e-9);
        self.tpf = tpf;
        let loop_len = if song_mode { song.length() } else { song.pattern_length(pat) } as f64;
        if self.pos >= loop_len {
            if tr.looping() {
                self.pos = 0.0;
                self.chase = true;
            } else {
                tr.set_playing(false);
                self.was_playing = false;
                return n;
            }
        }
        let to_end = (((loop_len - self.pos) / tpf).ceil() as usize).max(1);
        let n = n.min(to_end);
        let t0 = self.pos;
        let t1 = (t0 + n as f64 * tpf).min(loop_len);
        let chase = std::mem::take(&mut self.chase);

        if song_mode {
            for ci in 0..song.clips.len() {
                let c = &song.clips[ci];
                if c.pos as f64 >= t1 {
                    break;
                }
                let end = (c.pos + c.len) as f64;
                if end <= t0 || c.muted || !song.tracks.get(c.track as usize).map(|t| t.enabled).unwrap_or(true) {
                    continue;
                }
                match c.src {
                    ClipSrc::Pattern(p) => {
                        // pattern time = song time - clip start + clip offset
                        let shift = c.pos as f64 - c.offset as f64;
                        let a = t0.max(c.pos as f64) - shift;
                        let b = t1.min(end) - shift;
                        self.fire_pattern(song, p, a, b, shift, t0, tpf, Some(c.len + c.offset));
                    }
                    ClipSrc::Audio(chi) => {
                        let starts_here = c.pos as f64 >= t0;
                        if !starts_here && !chase {
                            continue;
                        }
                        let Some(ch) = song.channels.get(chi) else { continue };
                        let Some(smp) = ch.sample.clone() else { continue };
                        if !ch.enabled {
                            continue;
                        }
                        let begin = t0.max(c.pos as f64);
                        let delay = (((begin - t0) / tpf) - 1e-6).ceil().max(0.0) as u32;
                        // ticks into the audio at which we begin
                        let into = begin - c.pos as f64 + c.offset as f64;
                        let secs = into / song.ppq as f64 * 60.0 / tempo as f64;
                        let f = &ch.smp;
                        let gain = law::channel_gain(self.ov.chan_volume(song, chi)) * f.gain;
                        let (pl, pr) = law::pan(self.ov.chan_pan(song, chi));
                        // the pitch the clip sounds at, and how fast the read point crosses the sample
                        let step = smp.rate as f64 / self.sr as f64 * 2f64.powf((ch.pitch_cents + f.stretch_cents) as f64 / 1200.0);
                        let rate = if f.stretch_beats > 0.0 {
                            let want = f.stretch_beats as f64 * 60.0 / tempo as f64 * f.stretch_mul as f64;
                            smp.frames.len() as f64 / (want * self.sr as f64).max(1.0)
                        } else if f.keep_pitch {
                            smp.rate as f64 / self.sr as f64 / f.stretch_mul as f64
                        } else {
                            step / f.stretch_mul as f64
                        };
                        let stretch = f.keep_pitch && (rate / step - 1.0).abs() > 0.002;
                        let step = if f.keep_pitch { step } else { rate };
                        let pos = secs * self.sr as f64 * rate;
                        let remaining = Some(((end - begin) / tpf) as u64);
                        let rel = 1.0 / (self.sr * 0.005);
                        self.start(Voice { sample: Some(smp), pos, a_pos: pos, b_pos: pos, step, rate, stretch, hop: (self.sr * 0.03) as u32, level: 1.0, attack: 1.0, release: rel, gl: gain * pl, gr: gain * pr, insert: ch.insert, delay, remaining, chan: chi as u16, note_gain: f.gain, follow: true, ..Default::default() });
                    }
                    ClipSrc::Automation(_) => {}
                }
            }
        } else {
            self.fire_pattern(song, pat, t0, t1, 0.0, t0, tpf, None);
        }

        if tr.metronome() {
            let beat = (song.ppq * 4 / song.time_sig.1.max(1) as u32).max(1) as f64;
            let bar = song.bar() as f64;
            let mut b = (t0 / beat).ceil() * beat;
            while b < t1 {
                let delay = (((b - t0) / tpf) - 1e-6).ceil().max(0.0) as u32;
                let smp = self.clicks[if (b % bar) < 0.5 { 0 } else { 1 }].clone();
                self.start(Voice { sample: Some(smp), step: 1.0, level: 1.0, attack: 1.0, release: 1.0, gl: 1.0, gr: 1.0, insert: 0, delay, preview: true, ..Default::default() });
                b += beat;
            }
        }

        self.pos = t0 + n as f64 * tpf;
        if self.pos >= loop_len && tr.looping() {
            self.pos = 0.0;
        }
        tr.tick.store(self.pos.min(loop_len).to_bits(), Relaxed);
        n
    }

}

/// Biquad coefficients (b0, b1, b2, a1, a2) for band 0 (low shelf), 1 (peak) or 2 (high shelf).
fn eq_coeffs(band: usize, b: &crate::song::Band, rate: f32) -> [f32; 5] {
    let a = 10f32.powf(b.gain_db / 40.0);
    let w = 2.0 * std::f32::consts::PI * (b.hz() / rate).min(0.49);
    let (sn, cs) = w.sin_cos();
    let alpha = sn / (2.0 * b.width());
    let (b0, b1, b2, a0, a1, a2);
    match band {
        1 => {
            b0 = 1.0 + alpha * a;
            b1 = -2.0 * cs;
            b2 = 1.0 - alpha * a;
            a0 = 1.0 + alpha / a;
            a1 = -2.0 * cs;
            a2 = 1.0 - alpha / a;
        }
        0 => {
            let k = 2.0 * a.sqrt() * alpha;
            b0 = a * ((a + 1.0) - (a - 1.0) * cs + k);
            b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cs);
            b2 = a * ((a + 1.0) - (a - 1.0) * cs - k);
            a0 = (a + 1.0) + (a - 1.0) * cs + k;
            a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cs);
            a2 = (a + 1.0) + (a - 1.0) * cs - k;
        }
        _ => {
            let k = 2.0 * a.sqrt() * alpha;
            b0 = a * ((a + 1.0) + (a - 1.0) * cs + k);
            b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cs);
            b2 = a * ((a + 1.0) + (a - 1.0) * cs - k);
            a0 = (a + 1.0) - (a - 1.0) * cs + k;
            a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cs);
            a2 = (a + 1.0) - (a - 1.0) * cs - k;
        }
    }
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

struct ChanFx {
    unit: fx::Unit,
    buf: Vec<[f32; 2]>,
    dirty: bool,
    ring: usize,
}

impl Player {
    fn mix(&mut self, song: &Song, out: &mut [f32], channels: usize) {
        let n = out.len() / channels;
        let ni = song.inserts.len().max(1);
        if self.buses.len() != ni {
            self.buses = vec![vec![[0.0; 2]; BLOCK]; ni];
            self.dirty = vec![false; ni];
            self.fx = (0..ni).map(|_| vec![]).collect();
            self.ring = vec![0; ni];
        }
        let cx = fx::Ctx { sr: self.sr, step: (song.step() as f64 / self.tpf.max(1e-9)) as f32 };
        if self.chan_fx.len() != song.channels.len() {
            self.chan_fx.resize_with(song.channels.len(), || None);
        }
        for (ch, slot) in song.channels.iter().zip(self.chan_fx.iter_mut()) {
            match (ch.bay(), slot.as_mut()) {
                (None, None) => {}
                (None, Some(_)) => *slot = None,
                (Some(b), Some(cf)) if cf.unit.fx.sub.as_ref().zip(b.fx.sub.as_ref()).is_some_and(|(a, b)| std::sync::Arc::ptr_eq(a, b) || a == b) => {}
                (Some(b), _) => *slot = Some(ChanFx { unit: fx::Unit::new(b.fx.clone(), self.sr), buf: vec![[0.0; 2]; BLOCK], dirty: false, ring: 0 }),
            }
        }
        let mut live = 0;
        let follow = self.ov.any_chan;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            live += 1;
            if follow && v.follow {
                if let Some(&(vol, pan)) = self.ov.chan.get(v.chan as usize) {
                    if vol.is_some() || pan.is_some() {
                        let c = v.chan as usize;
                        let g = law::channel_gain(self.ov.chan_volume(song, c)) * v.note_gain;
                        let (pl, pr) = law::pan((self.ov.chan_pan(song, c) + v.note_pan).clamp(-1.0, 1.0));
                        v.gl = g * pl;
                        v.gr = g * pr;
                    }
                }
                if let Some(Some(cents)) = self.ov.pitch.get(v.chan as usize) {
                    v.bend = (cents - song.channels[v.chan as usize].pitch_cents) / 1200.0;
                }
            }
            if let Some(Some(cf)) = self.chan_fx.get_mut(v.chan as usize) {
                let into = if v.feed == 0 { Some(&mut cf.buf[..n]) } else { cf.unit.bay_feed(v.feed as usize - 1, n) };
                if into.is_some_and(|b| v.render(b)) {
                    cf.dirty = true;
                }
                continue;
            }
            let i = if v.insert < ni { v.insert } else { 0 };
            if v.render(&mut self.buses[i][..n]) {
                self.dirty[i] = true;
            }
        }
        for (c, cf) in self.chan_fx.iter_mut().enumerate() {
            let Some(cf) = cf else { continue };
            if !cf.dirty && cf.ring == 0 {
                continue;
            }
            cf.unit.process(&mut cf.buf[..n], &cx);
            cf.ring = if cf.dirty { cf.unit.tail(&cx) } else { cf.ring.saturating_sub(n) };
            let i = song.channels[c].insert as usize;
            let i = if i < ni { i } else { 0 };
            for (o, f) in self.buses[i][..n].iter_mut().zip(&mut cf.buf[..n]) {
                o[0] += f[0];
                o[1] += f[1];
                *f = [0.0; 2];
            }
            self.dirty[i] = true;
            cf.dirty = false;
        }
        self.transport.voices.store(live, Relaxed);

        let master = self.transport.master();
        self.blockno += 1;
        if self.cap.len() != ni {
            self.cap = vec![Vec::new(); ni];
            self.cap_at = vec![0; ni];
        }
        for &i in &song.mix_order {
            if i >= ni || (!self.dirty[i] && self.ring[i] == 0) {
                continue;
            }
            let ins = &song.inserts[i];
            let mut bus = std::mem::take(&mut self.buses[i]);
            // effect slots, top to bottom, ahead of the insert's own controls
            if ins.enabled && (!ins.slots.is_empty() || !self.fx[i].is_empty()) {
                let units = &mut self.fx[i];
                let want = ins.slots.iter().filter(|s| s.fx.is_some());
                if want.clone().count() != units.len() || want.clone().zip(units.iter()).any(|(s, u)| s.index != u.0 || s.fx.as_ref().map(|f| f.kind) != Some(u.1.fx.kind) || s.fx.as_ref().is_some_and(|f| f.ir != u.1.fx.ir)) {
                    *units = want.map(|s| (s.index, fx::Unit::new(s.fx.clone().unwrap(), self.sr))).collect();
                }
                let mut tail = 0;
                for (s, u) in ins.slots.iter().filter(|s| s.fx.is_some()).zip(units.iter_mut()) {
                    if let Some(f) = &s.fx {
                        if u.1.fx != *f {
                            if u.1.fx.kind == f.kind && u.1.fx.p.len() == f.p.len() {
                                u.1.fx.p.copy_from_slice(&f.p);
                                if u.1.fx.sub != f.sub {
                                    u.1.fx.sub = f.sub.clone();
                                }
                            } else {
                                u.1.fx = f.clone();
                            }
                        }
                    }
                    let (mut on, mut mix) = (s.enabled, s.mix);
                    for &(t, v) in &self.ov.slot {
                        match t {
                            Target::SlotMix(a, b) if a == i && b == s.index => mix = v,
                            Target::SlotOn(a, b) if a == i && b == s.index => on = v >= 0.5,
                            Target::SlotParam(a, b, k) if a == i && b == s.index => u.1.fx.set_travel(k as usize, v),
                            _ => {}
                        }
                    }
                    let slots_index = s.index;
                    let s = SlotNow { enabled: on, mix };
                    if !s.enabled || s.mix <= 0.0 {
                        continue;
                    }
                    tail += u.1.tail(&cx);
                    if let Some(sel) = u.1.side_wants() {
                        for (which, &k) in sel.iter().enumerate() {
                            if k < 1 {
                                continue;
                            }
                            let src = song.inserts.iter().enumerate().filter(|(j, x)| *j != i && x.sends.contains(&i)).map(|(j, _)| j).nth(k as usize - 1);
                            match src {
                                Some(j) if self.cap_at[j] == self.blockno && self.cap[j].len() >= n => u.1.side_set(which, &self.cap[j][..n]),
                                _ => {}
                            }
                        }
                    }
                    if matches!(u.1.fx.kind, fx::Kind::Peak | fx::Kind::Xy) {
                        // a controller: run it, then hand its outputs to whatever is linked to them
                        u.1.process(&mut bus[..n], &cx);
                        let slot = slots_index;
                        for l in song.ctl_links.iter().filter(|l| l.ins == i && l.slot == slot) {
                            if let Some(&v) = u.1.out.get(l.out as usize) {
                                self.driven.push((l.target, v));
                            }
                        }
                    } else if u.1.fx.kind == fx::Kind::SendTap {
                        // takes the sound off at this point of the chain and adds it to one of the tracks this one is routed to
                        u.1.process(&mut bus[..n], &cx);
                        let k = u.1.fx.p[3] as i32;
                        let ok = |d: usize| d != 0 && d != i && d < ni;
                        let to = ins.sends.iter().copied().find(|&d| ok(d) && ins.sends.iter().filter(|&&e| ok(e) && e < d).count() as i32 == k);
                        if let Some(d) = to {
                            if u.1.tap.len() >= n {
                                for (o, f) in self.buses[d][..n].iter_mut().zip(&u.1.tap[..n]) {
                                    o[0] += f[0];
                                    o[1] += f[1];
                                }
                                self.dirty[d] = true;
                            }
                        }
                    } else if s.mix >= 1.0 {
                        u.1.process(&mut bus[..n], &cx);
                    } else {
                        self.scratch.clear();
                        self.scratch.extend_from_slice(&bus[..n]);
                        u.1.process(&mut bus[..n], &cx);
                        for (w, d) in bus[..n].iter_mut().zip(&self.scratch) {
                            w[0] = d[0] + (w[0] - d[0]) * s.mix;
                            w[1] = d[1] + (w[1] - d[1]) * s.mix;
                        }
                    }
                }
                if !self.driven.is_empty() {
                    self.ov.fit(song);
                    for k in 0..self.driven.len() {
                        let (t, v) = self.driven[k];
                        self.ov.set(t, v);
                    }
                    self.driven.clear();
                }
                if self.dirty[i] {
                    self.ring[i] = tail;
                } else {
                    self.ring[i] = self.ring[i].saturating_sub(n);
                }
            } else {
                self.ring[i] = 0;
            }
            self.dirty[i] = false;
            if ins.enabled {
                let (ov, op) = self.ov.ins.get(i).copied().unwrap_or((None, None));
                let g = law::fader_gain(ov.unwrap_or(ins.volume)) * if i == 0 { master } else { 1.0 };
                let (pl, pr) = law::pan(op.unwrap_or(ins.pan));
                let (gl, gr) = (g * pl, g * pr);
                let mut peak = [0f32; 2];
                // the insert's own equaliser, then separation, polarity and swap, then the fader
                if self.eq.len() != ni {
                    self.eq = vec![[[0.0; 4]; 3]; ni];
                }
                for (b, band) in ins.eq.iter().enumerate() {
                    if band.gain_db.abs() < 0.005 {
                        continue;
                    }
                    let c = eq_coeffs(b, band, self.sr);
                    let st = &mut self.eq[i][b];
                    for f in bus[..n].iter_mut() {
                        for ch in 0..2 {
                            let x = f[ch];
                            let y = c[0] * x + st[ch * 2];
                            st[ch * 2] = c[1] * x - c[3] * y + st[ch * 2 + 1];
                            st[ch * 2 + 1] = c[2] * x - c[4] * y;
                            f[ch] = y;
                        }
                    }
                    for v in st.iter_mut() {
                        if !v.is_finite() || v.abs() < 1e-20 {
                            *v = 0.0;
                        }
                    }
                }
                if ins.sep != 0.0 {
                    let side = 1.0 - ins.sep;
                    for f in bus[..n].iter_mut() {
                        let m = (f[0] + f[1]) * 0.5;
                        let s = (f[0] - f[1]) * 0.5 * side;
                        *f = [m + s, m - s];
                    }
                }
                if ins.swap {
                    for f in bus[..n].iter_mut() {
                        f.swap(0, 1);
                    }
                }
                let (gl, gr) = if ins.invert { (-gl, -gr) } else { (gl, gr) };
                for f in bus[..n].iter_mut() {
                    f[0] *= gl;
                    f[1] *= gr;
                    peak[0] = peak[0].max(f[0].abs());
                    peak[1] = peak[1].max(f[1].abs());
                }
                if let Some(m) = self.transport.peaks.get(i) {
                    for c in 0..2 {
                        if peak[c] > f32::from_bits(m[c].load(Relaxed)) {
                            m[c].store(peak[c].to_bits(), Relaxed);
                        }
                    }
                }
                if i == 0 {
                    for (f, o) in bus[..n].iter().zip(out.chunks_mut(channels)) {
                        let l = f[0].clamp(-1.0, 1.0);
                        let r = f[1].clamp(-1.0, 1.0);
                        if channels == 1 {
                            o[0] = (l + r) * 0.5;
                        } else {
                            o[0] = l;
                            o[1] = r;
                            for x in &mut o[2..] {
                                *x = 0.0;
                            }
                        }
                    }
                } else {
                    for &d in &ins.sends {
                        if d < ni {
                            if song.inserts[d].slots.iter().any(|s| s.fx.as_ref().is_some_and(|f| f.kind == fx::Kind::Voc)) {
                                // a unit over there may listen to this track on its own, whatever the send level
                                let cap = &mut self.cap[i];
                                cap.clear();
                                cap.extend_from_slice(&bus[..n]);
                                self.cap_at[i] = self.blockno;
                                self.dirty[d] = true;
                            }
                            let lv = law::fader_gain(ins.send_level(d));
                            for (o, f) in self.buses[d][..n].iter_mut().zip(&bus[..n]) {
                                o[0] += f[0] * lv;
                                o[1] += f[1] * lv;
                            }
                            self.dirty[d] = true;
                        }
                    }
                }
            }
            for f in bus[..n].iter_mut() {
                *f = [0.0; 2];
            }
            self.buses[i] = bus;
        }
    }

    /// Fill an interleaved buffer with `channels` output channels.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let song = self.song.load_full();
        self.take_commands(&song);
        for x in out.iter_mut() {
            *x = 0.0;
        }
        let total = out.len() / channels;
        let mut done = 0;
        while done < total {
            let want = (total - done).min(BLOCK);
            let n = self.advance(&song, want).clamp(1, want);
            self.mix(&song, &mut out[done * channels..(done + n) * channels], channels);
            done += n;
        }
    }

    /// True while anything is still sounding.
    pub fn busy(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::{Chan, Clip, ClipSrc};

    fn click_song() -> Song {
        let mut s = Song::new();
        s.tempo = 120.0;
        // a 100-frame burst at 48 kHz
        let data: Vec<f32> = (0..100).map(|_| 0.9).collect();
        s.channels[0].sample = Some(Arc::new(Sample::from_mono("click", 48_000, &data)));
        s.channels[0].volume = 1.0;
        s
    }

    fn onsets(buf: &[f32], window: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut quiet = window;
        for (i, f) in buf.chunks(2).enumerate() {
            if f[0].abs() > 1e-4 {
                if quiet >= window {
                    out.push(i);
                }
                quiet = 0;
            } else {
                quiet += 1;
            }
        }
        out
    }

    #[test]
    fn silent_when_stopped() {
        let (_h, mut p) = new_player(click_song(), 48_000.0);
        let mut buf = vec![1.0f32; 4800];
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn steps_land_on_their_frames_and_the_pattern_loops() {
        let mut s = click_song();
        for step in [0u32, 4, 8, 12] {
            s.patterns[0].toggle_step(0, step * 24);
        }
        let (h, mut p) = new_player(s, 48_000.0);
        h.transport.set_playing(true);
        // two bars at 120 bpm = 4 s; odd block size on purpose
        let mut buf = vec![0.0f32; 48_000 * 4 * 2];
        for block in buf.chunks_mut(2 * 333) {
            p.render(block, 2);
        }
        let got = onsets(&buf, 200);
        let want: Vec<usize> = (0..8).map(|i| i * 24_000).collect();
        assert_eq!(got.len(), want.len(), "{got:?}");
        for (g, w) in got.iter().zip(&want) {
            assert!(g.abs_diff(*w) <= 2, "onset at {g}, wanted {w}");
        }
    }

    #[test]
    fn song_mode_plays_clips_and_respects_mutes() {
        let mut s = click_song();
        s.patterns[0].toggle_step(0, 0);
        s.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false });
        s.clips.push(Clip { pos: 384, len: 384, track: 1, src: ClipSrc::Pattern(0), offset: 0, muted: true });
        s.clips.push(Clip { pos: 768, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false });
        let (h, mut p) = new_player(s, 48_000.0);
        h.transport.set_song_mode(true);
        h.transport.set_looping(false);
        h.transport.set_playing(true);
        let mut buf = vec![0.0f32; 48_000 * 7 * 2];
        for block in buf.chunks_mut(2 * 512) {
            p.render(block, 2);
        }
        // bars are 2 s long: clicks at 0 s and 4 s, nothing at 2 s, and playback stops at the end
        let got = onsets(&buf, 200);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0] == 0 && got[1].abs_diff(192_000) <= 1, "{got:?}");
        assert!(!h.transport.playing());
    }

    #[test]
    fn automation_moves_volume_and_tempo_and_layers_play_children() {
        use crate::song::{AutoPt, Automation};
        let mut s = click_song();
        // a long tone so the fade can be heard
        let data: Vec<f32> = (0..96_000).map(|_| 0.5).collect();
        s.channels[0].sample = Some(Arc::new(Sample::from_mono("tone", 48_000, &data)));
        s.patterns[0].toggle_step(0, 0);
        let ramp = |a: f32, b: f32, t: Target| {
            Some(Arc::new(Automation { points: vec![AutoPt { tick: 0.0, value: a, tension: 0.0, mode: 0 }, AutoPt { tick: 384.0, value: b, tension: 0.0, mode: 0 }], targets: vec![t] }))
        };
        s.channels.push(Chan { kind: ChannelKind::AutomationClip, auto: ramp(1.0, 0.0, Target::ChanVolume(0)), ..Default::default() });
        let ac = s.channels.len() - 1;
        s.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false });
        s.clips.push(Clip { pos: 0, len: 384, track: 1, src: ClipSrc::Automation(ac), offset: 0, muted: false });
        let (h, mut p) = new_player(s.clone(), 48_000.0);
        h.transport.set_song_mode(true);
        h.transport.set_looping(false);
        h.transport.set_playing(true);
        let mut buf = vec![0.0f32; 48_000 * 2 * 2];
        for block in buf.chunks_mut(2 * 512) {
            p.render(block, 2);
        }
        let at = |sec: f32| buf[(sec * 48_000.0) as usize * 2].abs();
        assert!(at(0.05) > 0.4, "{}", at(0.05));
        assert!(at(1.0) < at(0.05) * 0.5 && at(1.0) > 0.001, "{}", at(1.0));
        assert!(at(1.9) < 0.01, "{}", at(1.9));

        // tempo: half way up the control is 120 bpm, the top is 180
        s.channels[ac].auto = ramp(1.0, 1.0, Target::Tempo);
        let (h, mut p) = new_player(s.clone(), 48_000.0);
        h.transport.set_song_mode(true);
        h.transport.set_playing(true);
        p.render(&mut buf[..2048], 2);
        assert_eq!(h.transport.tempo(), 180.0);

        // an effect setting: the balance unit's volume held at the bottom silences the insert; so does its mix
        // at zero not, since the dry sound then passes
        s.channels[ac].auto = ramp(0.0, 0.0, Target::SlotParam(0, 0, 1));
        s.inserts[0].slots.push(crate::song::Slot::with_fx(0, crate::fx::Kind::Balance));
        let (h, mut p) = new_player(s.clone(), 48_000.0);
        h.transport.set_song_mode(true);
        h.transport.set_playing(true);
        p.render(&mut buf[..8192], 2);
        assert!(buf[4096..8192].iter().all(|x| x.abs() < 1e-6));
        s.channels[ac].auto = ramp(0.0, 0.0, Target::SlotOn(0, 0));
        let (h, mut p) = new_player(s.clone(), 48_000.0);
        h.transport.set_song_mode(true);
        h.transport.set_playing(true);
        p.render(&mut buf[..8192], 2);
        assert!(buf[4096..8192].iter().any(|x| x.abs() > 0.1));

        // a level follower in a slot, set to duck, pulls the linked fader down while sound passes through it
        let mut s2 = s.clone();
        s2.channels[ac].auto = None;
        s2.inserts[0].slots.clear();
        let mut duck = crate::song::Slot::with_fx(0, crate::fx::Kind::Peak);
        if let Some(f) = duck.fx.as_mut() {
            f.p[1] = -256.0;
            f.p[10] = 65536.0;
        }
        s2.inserts[0].slots.push(duck);
        s2.ctl_links.push(crate::song::CtlLink { ins: 0, slot: 0, out: 1, target: Target::InsertVolume(0) });
        let (h, mut p) = new_player(s2, 48_000.0);
        h.transport.set_song_mode(true);
        h.transport.set_playing(true);
        p.render(&mut buf[..8192], 2);
        assert!(buf[4096..8192].iter().all(|x| x.abs() < 1e-6));

        // a layer plays its children
        let mut s = click_song();
        s.channels.push(Chan { kind: ChannelKind::Layer, children: vec![0], ..Default::default() });
        let layer = (s.channels.len() - 1) as u16;
        let (h, mut p) = new_player(s, 48_000.0);
        h.send(Cmd::NoteOn { chan: layer, key: 60, vel: 1.0 });
        p.render(&mut buf[..2048], 2);
        assert!(buf[..2048].iter().any(|x| x.abs() > 0.1));
    }

    #[test]
    fn live_notes_and_mixer_routing() {
        let mut s = click_song();
        s.channels[0].insert = 3;
        s.inserts[3].volume = 0.0; // fader down: silence
        let (mut h, mut p) = new_player(s.clone(), 48_000.0);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        let mut buf = vec![0.0f32; 2048];
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|x| x.abs() < 1e-6));
        s.inserts[3].volume = 1.0;
        h.publish(s);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        assert!(buf.iter().any(|x| x.abs() > 0.1));
        assert!(h.transport.take_peak(3).0 > 0.1);
        assert!(h.transport.take_peak(0).0 > 0.1);
    }

    fn tone_song(secs: f32) -> Song {
        let mut s = Song::new();
        s.tempo = 120.0;
        let data: Vec<f32> = (0..(48_000.0 * secs) as usize).map(|i| (i as f32 * 0.06).sin() * 0.5).collect();
        s.channels[0].set_sample(Some(Arc::new(Sample::from_mono("tone", 48_000, &data))));
        s.channels[0].volume = 1.0;
        s
    }

    fn loud(buf: &[f32], from: f32, to: f32) -> f32 {
        buf[(from * 48_000.0) as usize * 2..(to * 48_000.0) as usize * 2].iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    #[test]
    fn volume_envelope_shapes_and_releases_a_note() {
        let mut s = tone_song(2.0);
        s.channels[0].env_on = true;
        s.channels[0].smp.vol_env = Env { on: true, decay: 0.1, sustain: 0.0, ..Env::default() };
        s.patterns[0].toggle_step(0, 0);
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.0, 0.05) > 0.2);
        assert!(loud(&buf, 0.2, 1.0) < 1e-3, "the decay to a zero sustain silences the note");

        // a held note stops through its release when the note ends
        s.channels[0].smp.vol_env = Env { on: true, release: 0.05, ..Env::default() };
        s.patterns[0].notes[0].len = 96; // one beat = 0.5 s
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.3, 0.45) > 0.2);
        assert!(loud(&buf, 0.6, 1.0) < 1e-3);
    }

    #[test]
    fn a_slicer_key_plays_its_slice_and_no_more() {
        let mut s = tone_song(2.0);
        s.channels[0].kind = ChannelKind::Plugin;
        let cut = |pos| crate::slicer::Cut { pos, key: -1, reverse: false, name: String::new() };
        s.channels[0].slices = Some(Arc::new(crate::slicer::Slices { beats: 4.0, bpm: 120.0, path: String::new(), cuts: vec![cut(0), cut(24_000), cut(48_000)], base: 60, rate: 48_000 }));
        s.patterns[0].toggle_step(0, 0);
        s.patterns[0].notes[0].key = 61;
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.05, 0.45) > 0.2);
        assert!(loud(&buf, 0.52, 1.5) < 1e-3, "the slice ends at the next cut");
        // a key past the last slice is silent
        s.patterns[0].notes[0].key = 70;
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.0, 1.0) < 1e-6);
    }

    #[test]
    fn a_pad_machine_plays_layers_by_velocity_and_cuts_by_group() {
        let mut s = tone_song(2.0);
        let tone = s.channels[0].sample.take().unwrap();
        s.channels[0].kind = ChannelKind::Plugin;
        let pad = |key: u8, lo: u8, hi: u8, cut: i32| crate::zones::Zone { key_lo: key, key_hi: key, vel_lo: lo, vel_hi: hi, root: key as i32, track: false, sample: Some(tone.clone()), one_shot: true, cut, cut_by: cut, ..Default::default() };
        s.channels[0].zones = Some(Arc::new(crate::zones::Map { zones: vec![pad(36, 0, 110, -1), pad(46, 0, 127, 0), pad(42, 0, 127, 0)] }));
        s.patterns[0].toggle_step(0, 0);
        s.patterns[0].notes[0].key = 36;
        // a short note still plays the whole recording
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 1.0, 1.8) > 0.1);
        // a velocity outside every layer is silent
        s.patterns[0].notes[0].vel = 1.2;
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.0, 1.8) < 1e-6);
        // an open hat is stopped by a closed one in its group; the closed one is a different key, so check the level drop
        s.patterns[0].notes[0].vel = 1.0;
        s.patterns[0].notes[0].key = 46;
        let alone = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        s.patterns[0].toggle_step(0, 4);
        let k = s.patterns[0].notes.len() - 1;
        s.patterns[0].notes[k].key = 42;
        s.patterns[0].notes[k].vel = 0.05;
        let cut = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&alone, 1.0, 1.8) > 0.1);
        assert!(loud(&cut, 1.0, 1.8) < 0.01, "the open hat was stopped");
    }

    #[test]
    fn cut_groups_reverse_and_stretch() {
        let mut s = tone_song(2.0);
        s.channels.push(Chan { name: "quiet".into(), volume: 0.0, ..s.channels[0].clone() });
        let last = s.channels.len() - 1;
        s.channels[0].smp.cut_by = 1;
        s.channels[last].smp.cut = 1;
        s.patterns[0].toggle_step(0, 0);
        s.patterns[0].toggle_step(last as u16, 96);
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.1, 0.45) > 0.2);
        assert!(loud(&buf, 0.6, 1.5) < 1e-3, "the second channel cuts the first");

        // a fade-in over the whole sample starts from nothing; reversed, it ends in nothing
        let mut s = tone_song(1.0);
        s.channels[0].smp.fade_in = 1.0;
        s.channels[0].bake();
        let smp = s.channels[0].sample.clone().unwrap();
        assert!(smp.frames[..480].iter().all(|f| f[0].abs() < 0.01));
        s.channels[0].smp.reverse = true;
        s.channels[0].bake();
        let smp = s.channels[0].sample.clone().unwrap();
        assert!(smp.frames[..480].iter().all(|f| f[0].abs() < 0.01), "fades are applied after reversing");

        // a one second clip fitted to four beats at 120 bpm lasts two seconds
        let mut s = tone_song(1.0);
        s.channels[0].kind = ChannelKind::AudioClip;
        s.channels[0].smp.stretch_beats = 4.0;
        s.channels[0].smp.keep_pitch = true;
        s.clips.push(Clip { pos: 0, len: 96 * 8, track: 0, src: ClipSrc::Audio(0), offset: 0, muted: false });
        let buf = crate::render_song(&s, 48_000.0, true, 0, 0.0);
        assert!(loud(&buf, 1.5, 1.9) > 0.1, "still sounding in its second second");
        assert!(loud(&buf, 2.1, 3.0) < 1e-3);
    }
}
