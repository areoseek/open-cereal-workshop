//! The audio side: plays a `Song` snapshot, sample-accurately, into a stereo mixer.
//!
//! The UI owns a `Handle`; the audio callback owns the `Player`. They share the song through an atomic pointer
//! swap, the transport through atomics, and live notes through a queue the audio thread only ever try-locks.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::flp::ChannelKind;
use crate::law;
use crate::sample::Sample;
use crate::song::{ClipSrc, Note, Song, Target};

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
        dirty: vec![],
        pos: 0.0,
        was_playing: false,
        chase: false,
        born: 0,
        ov: Overrides::default(),
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
}

impl Overrides {
    fn clear(&mut self) {
        self.tempo = None;
        self.any_chan = false;
        self.chan.iter_mut().for_each(|c| *c = (None, None));
        self.ins.iter_mut().for_each(|c| *c = (None, None));
    }

    fn fit(&mut self, song: &Song) {
        if self.chan.len() != song.channels.len() {
            self.chan.resize(song.channels.len(), (None, None));
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
            Target::Other { .. } => {}
        }
    }
}

/// A short decaying blip for the metronome.
fn click(rate: f32, hz: f32) -> Arc<Sample> {
    let n = (rate * 0.03) as usize;
    let data: Vec<f32> = (0..n).map(|i| (i as f32 / rate * hz * std::f32::consts::TAU).sin() * (1.0 - i as f32 / n as f32).powi(2) * 0.6).collect();
    Arc::new(Sample::from_mono("click", rate as u32, &data))
}

#[derive(Default)]
struct Voice {
    active: bool,
    sample: Option<Arc<Sample>>,
    pos: f64,
    step: f64,
    phase: f32,
    dphase: f32,
    lp: f32,
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
    chan: u16,
    key: u8,
    live: bool,
    preview: bool,
    born: u64,
    /// Gain and pan the note itself brought, kept so channel automation can move a sounding voice.
    note_gain: f32,
    note_pan: f32,
    follow: bool,
}

impl Voice {
    fn let_go(&mut self) {
        self.releasing = true;
        self.remaining = None;
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
                    self.releasing = true;
                    self.remaining = None;
                } else {
                    *r -= 1;
                }
            }
            if self.releasing {
                self.level -= self.release;
                if self.level <= 0.0 {
                    self.active = false;
                    break;
                }
            } else if self.level < 1.0 {
                self.level = (self.level + self.attack).min(1.0);
            }
            let s = match &self.sample {
                Some(smp) => {
                    if self.pos >= smp.frames.len() as f64 {
                        self.active = false;
                        break;
                    }
                    let s = smp.at(self.pos);
                    self.pos += self.step;
                    s
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
            out[0] += s[0] * self.gl * self.level;
            out[1] += s[1] * self.gr * self.level;
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
    dirty: Vec<bool>,
    pos: f64,
    was_playing: bool,
    /// Set after a start or a seek: clips already under the play head must be picked up mid-way.
    chase: bool,
    born: u64,
    ov: Overrides,
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
        let note_gain = law::velocity_gain(n.vel);
        let gain = law::channel_gain(self.ov.chan_volume(song, n.chan as usize)) * note_gain;
        let (pl, pr) = law::pan((self.ov.chan_pan(song, n.chan as usize) + n.pan).clamp(-1.0, 1.0));
        let cents = (n.key as f32 - ch.root_key as f32) * 100.0 + ch.pitch_cents + n.fine_cents;
        let sr = self.sr;
        let ms = |m: f32| 1.0 / (sr * m / 1000.0).max(1.0);
        let base = Voice { gl: gain * pl, gr: gain * pr, insert: ch.insert, delay, chan: n.chan, key: n.key, live, note_gain, note_pan: n.pan, follow: true, ..Default::default() };
        match ch.kind {
            ChannelKind::Sampler | ChannelKind::AudioClip => {
                let Some(smp) = ch.sample.clone() else { return };
                let step = smp.rate as f64 / sr as f64 * 2f64.powf(cents as f64 / 1200.0);
                // without a volume envelope a sampler plays its sample out whatever the note length
                let remaining = if ch.env_on || live { len_frames } else { None };
                self.start(Voice { sample: Some(smp), step, level: 1.0, attack: 1.0, release: ms(if ch.env_on { 30.0 } else { 8.0 }), remaining, ..base });
            }
            ChannelKind::Plugin | ChannelKind::Other(_) => {
                let hz = 440.0 * 2f32.powf((n.key as f32 - 69.0) / 12.0 + (ch.pitch_cents + n.fine_cents) / 1200.0);
                let min_len = (sr * 0.12) as u64;
                let remaining = if live { len_frames } else { Some(len_frames.unwrap_or(min_len).max(1)) };
                self.start(Voice { dphase: hz / sr, level: 0.0, attack: ms(4.0), release: ms(90.0), remaining, ..base });
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
            let delay = ((((n.pos as f64 + shift) - t0) / tpf) - 1e-6).ceil().max(0.0) as u32;
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
                        let gain = law::channel_gain(self.ov.chan_volume(song, chi));
                        let (pl, pr) = law::pan(self.ov.chan_pan(song, chi));
                        let step = smp.rate as f64 / self.sr as f64 * 2f64.powf(ch.pitch_cents as f64 / 1200.0);
                        let pos = secs * smp.rate as f64;
                        let remaining = Some(((end - begin) / tpf) as u64);
                        let rel = 1.0 / (self.sr * 0.005);
                        self.start(Voice { sample: Some(smp), pos, step, level: 1.0, attack: 1.0, release: rel, gl: gain * pl, gr: gain * pr, insert: ch.insert, delay, remaining, chan: chi as u16, note_gain: 1.0, follow: true, ..Default::default() });
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

    fn mix(&mut self, song: &Song, out: &mut [f32], channels: usize) {
        let n = out.len() / channels;
        let ni = song.inserts.len().max(1);
        if self.buses.len() != ni {
            self.buses = vec![vec![[0.0; 2]; BLOCK]; ni];
            self.dirty = vec![false; ni];
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
            }
            let i = if v.insert < ni { v.insert } else { 0 };
            if v.render(&mut self.buses[i][..n]) {
                self.dirty[i] = true;
            }
        }
        self.transport.voices.store(live, Relaxed);

        let master = self.transport.master();
        for &i in &song.mix_order {
            if i >= ni || !self.dirty[i] {
                continue;
            }
            self.dirty[i] = false;
            let ins = &song.inserts[i];
            let mut bus = std::mem::take(&mut self.buses[i]);
            if ins.enabled {
                let (ov, op) = self.ov.ins.get(i).copied().unwrap_or((None, None));
                let g = law::fader_gain(ov.unwrap_or(ins.volume)) * if i == 0 { master } else { 1.0 };
                let (pl, pr) = law::pan(op.unwrap_or(ins.pan));
                let (gl, gr) = (g * pl, g * pr);
                let mut peak = [0f32; 2];
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
                            for (o, f) in self.buses[d][..n].iter_mut().zip(&bus[..n]) {
                                o[0] += f[0];
                                o[1] += f[1];
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
    use crate::song::{Clip, ClipSrc};

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
        use crate::song::{AutoPt, Automation, Chan};
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
}
