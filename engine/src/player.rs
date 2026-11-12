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
    /// The pitch wheel: bend everything on a channel by this many octaves.
    Bend { chan: u16, octaves: f32 },
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
    recording: AtomicBool,
    countdown: AtomicBool,
    /// Notes played by hand are written down one grid step at a time while the song stands still.
    step_edit: AtomicBool,
    /// Play is pressed, and the song starts with the first note played by hand.
    waiting: AtomicBool,
    /// The stretch of time picked out to play round and round: start in the high half, end in the low; 0 = none.
    range: AtomicU64,
    /// Notes played by hand while recording is armed and the song runs, for the window to write down.
    live: Mutex<Vec<Live>>,
    /// MIDI messages the sender channels made, for the window's sender thread to put on the wire.
    midi: Mutex<Vec<crate::midiout::Msg>>,
    /// The effect slot whose sound the window wants to look at (insert * 64 + slot), or all ones for none.
    tap_want: AtomicU32,
    tap: Mutex<Tap>,
    /// The slot whose passing sound is being recorded, and what has gathered since the window last took it.
    rec_want: AtomicU32,
    rec: Mutex<Vec<[f32; 2]>>,
    /// Sound arriving from the sound card inputs, interleaved over `input_ch` channels, waiting for the mixer.
    input: Mutex<std::collections::VecDeque<f32>>,
    input_ch: AtomicUsize,
    /// Whether any mixer track listens to an input, so the window knows to keep the inputs open.
    input_wanted: AtomicBool,
}

/// The latest stretch of sound through the watched slot, kept going round.
struct Tap {
    buf: Vec<[f32; 2]>,
    at: usize,
    /// Frames written so far, so the reader can tell fresh sound from stale.
    count: u64,
    rate: f32,
}

const TAP_LEN: usize = 16_384;

/// A key going down or up while recording, stamped with the play position.
/// A note an arpeggiator holds: frames from now until it starts and ends (no end for a key still down).
struct ArpHeld {
    note: Note,
    from: i64,
    to: Option<i64>,
    live: bool,
    /// Already sounded whole, as a lone note under the holding chord choice.
    sus: bool,
}

/// What a MIDI sender channel last sent: its block and its volume, pan and bend.
struct MidiSeen {
    chan: u16,
    raw: Vec<u8>,
    fresh: bool,
    mix: [i32; 3],
}

/// One arpeggiator run: all held notes of a channel, or one note spelling its own chord.
/// A note a script was handed and has not been told the end of yet.
struct ScriptHeld {
    hid: u32,
    key: u8,
    live: bool,
    /// Frames until it ends, for a note of known length.
    left: Option<u64>,
}

/// A note script at work in front of one instrument of a bay.
struct ScriptRun {
    serial: u32,
    chan: u16,
    li: usize,
    /// Where in the chain of note units it stands, and the socket this instrument's cable leaves it by.
    at: usize,
    port: u32,
    /// Which held unit of the bay the script is.
    node: Option<usize>,
    def: Arc<crate::script::ScriptDef>,
    inst: crate::script::Inst,
    /// Note lines not yet due, with the frames each still waits.
    wait: Vec<(u32, String)>,
    held: Vec<ScriptHeld>,
    /// The part of a tick left over from the last block.
    acc: f64,
    next: u32,
}

struct ArpRun {
    chan: u16,
    key: Option<u8>,
    own: bool,
    held: Vec<ArpHeld>,
    i: u32,
    /// Frames until the next note.
    wait: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub tick: f64,
    pub on: bool,
    pub chan: u16,
    pub key: u8,
    pub vel: f32,
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
            recording: AtomicBool::new(false),
            countdown: AtomicBool::new(false),
            step_edit: AtomicBool::new(false),
            range: AtomicU64::new(0),
            waiting: AtomicBool::new(false),
            live: Mutex::new(Vec::new()),
            midi: Mutex::new(Vec::with_capacity(256)),
            tap_want: AtomicU32::new(u32::MAX),
            tap: Mutex::new(Tap { buf: vec![[0.0; 2]; TAP_LEN], at: 0, count: 0, rate: 48_000.0 }),
            rec_want: AtomicU32::new(u32::MAX),
            rec: Mutex::new(Vec::new()),
            input: Mutex::new(std::collections::VecDeque::new()),
            input_ch: AtomicUsize::new(0),
            input_wanted: AtomicBool::new(false),
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
    pub fn recording(&self) -> bool {
        self.recording.load(Relaxed)
    }
    pub fn set_recording(&self, on: bool) {
        self.recording.store(on, Relaxed);
    }
    /// Whether a bar of clicks is counted in before recording starts.
    pub fn countdown(&self) -> bool {
        self.countdown.load(Relaxed)
    }
    pub fn set_countdown(&self, on: bool) {
        self.countdown.store(on, Relaxed);
    }
    /// The stretch of time that plays round and round in place of the whole song or pattern.
    pub fn range(&self) -> Option<(u32, u32)> {
        let r = self.range.load(Relaxed);
        let (a, b) = ((r >> 32) as u32, r as u32);
        (b > a).then_some((a, b))
    }
    pub fn set_range(&self, r: Option<(u32, u32)>) {
        let (a, b) = r.unwrap_or((0, 0));
        self.range.store((a as u64) << 32 | b as u64, Relaxed);
    }
    pub fn step_edit(&self) -> bool {
        self.step_edit.load(Relaxed)
    }
    pub fn set_step_edit(&self, on: bool) {
        self.step_edit.store(on, Relaxed);
    }
    /// Whether the song is held back until the first note is played by hand.
    pub fn waiting(&self) -> bool {
        self.waiting.load(Relaxed)
    }
    pub fn set_waiting(&self, on: bool) {
        self.waiting.store(on, Relaxed);
    }
    /// Puts one message of the window's own in the queue (a reset button, say).
    pub fn send_midi(&self, m: crate::midiout::Msg) {
        if let Ok(mut q) = self.midi.lock() {
            q.push(m);
        }
    }
    /// The MIDI messages made since the last call, in order.
    pub fn take_midi(&self, into: &mut Vec<crate::midiout::Msg>) {
        if let Ok(mut m) = self.midi.lock() {
            into.extend(m.drain(..));
        }
    }
    /// The hand-played notes logged since the last call.
    pub fn take_live(&self) -> Vec<Live> {
        self.live.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default()
    }
    fn log(&self, c: &Cmd) {
        if self.waiting() && matches!(c, Cmd::NoteOn { .. }) {
            self.set_waiting(false);
            self.set_playing(true);
        }
        let stepping = self.step_edit() && !self.playing();
        if !(self.recording() && self.playing()) && !stepping {
            return;
        }
        let ev = match *c {
            Cmd::NoteOn { chan, key, vel } => Live { tick: self.tick(), on: true, chan, key, vel },
            Cmd::NoteOff { chan, key } => Live { tick: self.tick(), on: false, chan, key, vel: 0.0 },
            _ => return,
        };
        if let Ok(mut l) = self.live.lock() {
            l.push(ev);
        }
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
    /// Choose the effect slot whose sound is kept for the pictures of the meter units, or none.
    /// Start or stop gathering the sound that passes a mixer slot, for the wave recorder.
    pub fn record_slot(&self, at: Option<(usize, u16)>) {
        if let Ok(mut r) = self.rec.lock() {
            *r = Vec::with_capacity(if at.is_some() { 96_000 } else { 0 });
        }
        self.rec_want.store(at.map(|(i, s)| (i as u32) << 6 | (s as u32 & 63)).unwrap_or(u32::MAX), Relaxed);
    }
    /// Hand over sound from the sound card inputs, interleaved over `channels`, at the engine's own rate.
    /// At most a second is kept waiting; older sound is let go.
    pub fn feed_input(&self, data: &[f32], channels: usize) {
        if channels == 0 {
            return;
        }
        if let Ok(mut q) = self.input.lock() {
            if self.input_ch.swap(channels, Relaxed) != channels {
                q.clear();
            }
            q.extend(data.iter().copied());
            let cap = 48_000 * channels;
            if q.len() > cap {
                let over = (q.len() - cap).div_ceil(channels) * channels;
                q.drain(..over);
            }
        }
    }
    pub fn input_wanted(&self) -> bool {
        self.input_wanted.load(Relaxed)
    }
    pub fn recording_slot(&self) -> Option<(usize, u16)> {
        let v = self.rec_want.load(Relaxed);
        (v != u32::MAX).then_some(((v >> 6) as usize, (v & 63) as u16))
    }
    /// What has been recorded since the last call. Room for the next stretch is made here, away from the audio
    /// thread.
    pub fn take_recorded(&self) -> Vec<[f32; 2]> {
        let fresh = Vec::with_capacity(96_000);
        self.rec.lock().map(|mut r| std::mem::replace(&mut *r, fresh)).unwrap_or_default()
    }
    pub fn watch_slot(&self, at: Option<(usize, u16)>) {
        self.tap_want.store(at.map(|(i, s)| (i as u32) << 6 | (s as u32 & 63)).unwrap_or(u32::MAX), Relaxed);
    }
    /// The last `n` frames through the watched slot, oldest first, with the count of frames seen so far
    /// (a count that stands still means the track has gone quiet).
    pub fn watched(&self, n: usize) -> (Vec<[f32; 2]>, u64, f32) {
        let Ok(t) = self.tap.lock() else { return (Vec::new(), 0, 48_000.0) };
        let n = n.min(TAP_LEN);
        let from = (t.at + TAP_LEN - n) % TAP_LEN;
        ((0..n).map(|i| t.buf[(from + i) % TAP_LEN]).collect(), t.count, t.rate)
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
        self.transport.log(&c);
        if let Ok(mut q) = self.cmds.lock() {
            q.push(c);
        }
    }

    /// A second way in for commands, for threads other than the window's (MIDI input).
    pub fn sender(&self) -> CmdSender {
        CmdSender(self.cmds.clone(), self.transport.clone())
    }
}

/// Sends commands to the audio thread from anywhere.
#[derive(Clone)]
pub struct CmdSender(Arc<Mutex<Vec<Cmd>>>, Arc<Transport>);

impl CmdSender {
    pub fn send(&self, c: Cmd) {
        self.1.log(&c);
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
    let handle = Handle { transport, song, cmds, graveyard: vec![] };
    let player = handle.player(sample_rate);
    (handle, player)
}

impl Handle {
    /// Another player on the same song, transport and commands: for when the sound card is reopened,
    /// perhaps at another rate. The one before it must be dropped first.
    pub fn player(&self, sample_rate: f32) -> Player {
        let (song, transport, cmds) = (self.song.clone(), self.transport.clone(), self.cmds.clone());
        Player {
            song,
            transport,
            cmds,
            inbox: Vec::with_capacity(64),
            sr: sample_rate,
            voices: (0..MAX_VOICES).map(|_| Voice::default()).collect(),
            wheel: Vec::new(),
            buses: vec![],
            eq: vec![],
            fx: vec![],
            driven: Vec::with_capacity(64),
            env_runs: Vec::new(),
            arps: Vec::new(),
            scripts: Vec::new(),
            script_emit: None,
            script_serial: 0,
            script_ctl: Vec::new(),
            layer_sets: Vec::new(),
            midi_wait: Vec::with_capacity(64),
            midi_live: Vec::new(),
            host_wait: Vec::with_capacity(64),
            host_live: Vec::new(),
            offline: false,
            host_evs: Vec::with_capacity(64),
            host_buf: vec![[0.0; 2]; BLOCK],
            pad_turn: Vec::with_capacity(128),
            midi_seen: Vec::new(),
            midi_ran: false,
            seed: 0x2545_f491,
            shape_turn: Vec::new(),
            ring: vec![],
            chan_fx: vec![],
            scratch: Vec::with_capacity(BLOCK),
            cap: Vec::new(),
            cap_at: Vec::new(),
            inbuf: Vec::new(),
            blockno: 0,
            dirty: vec![],
            pos: 0.0,
            was_playing: false,
            chase: false,
            count_in: 0.0,
            born: 0,
            ov: Overrides::default(),
            tpf: 130.0 * 96.0 / 60.0 / sample_rate as f64,
            clicks: [click(sample_rate, 1760.0), click(sample_rate, 880.0)],
        }
    }
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
    /// Instrument settings being driven: channel, the setting's place in its table, and its 0..=1 value.
    gen: Vec<(usize, u16, f32)>,
}

impl Overrides {
    fn clear(&mut self) {
        self.tempo = None;
        self.any_chan = false;
        self.slot.clear();
        self.gen.clear();
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
            Target::ChanParam(c, k) => match self.gen.iter_mut().find(|e| e.0 == c && e.1 == k) {
                Some(e) => e.2 = v,
                None => self.gen.push((c, k, v)),
            },
            Target::Other { .. } => {}
        }
    }
}

/// A slot's switch and mix for the block being run, after automation.
/// A note of an envelope sender channel that is still sending.
struct EnvRun {
    chan: u16,
    key: u8,
    /// The key the sender was handed, which note units in a bay may have moved.
    sent: u8,
    /// The held unit of the channel's bay that is the sender, when it is not the channel itself.
    node: Option<usize>,
    vel: f32,
    rand: f32,
    /// Seconds since the note began, and when it was let go.
    t: f32,
    rel: Option<f32>,
    /// Frames until a note from the song is let go.
    left: Option<u64>,
    set: crate::fm::Sender,
}

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
/// A script's velocity against ours: its 0..1 is the stored 0..128, of which our 1.0 is 100.
const SCRIPT_VEL: f32 = 100.0 / 128.0;

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
    /// Octaves the pitch wheel bends it by.
    hand: f32,
    /// Octaves slide and portamento notes have bent it by, where that is heading and how fast per frame.
    slide: f32,
    slide_to: f32,
    slide_inc: f32,
    color: u8,
    /// A note mover on the way to this voice's instrument, the length of a beat for it, and what it last asked
    /// for: loudness taken off, octaves and pan.
    motion: Option<Arc<crate::noteshape::Motion>>,
    mo_beat: f32,
    mo: [f32; 3],
    mo_inc: f32,
    /// The script note this voice is, 0 for none, and what the script has since done to it: octaves, loudness over what it started at less one, pan.
    script: u64,
    sc: [f32; 3],
    /// The pitch a script's note started at, in keys.
    sc_key: f32,
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
        if self.slide != self.slide_to {
            let d = self.slide_inc * CTL as f32;
            self.slide = if (self.slide_to - self.slide).abs() <= d { self.slide_to } else { self.slide + d * (self.slide_to - self.slide).signum() };
        }
        if let Some(m) = &self.motion {
            let rel = self.releasing.then(|| self.t.saturating_sub(self.rel_t) as f32 / self.sr);
            let mv = m.at(self.color, self.key, t, rel, self.mo_beat);
            let cut = 1.0 - mv.gain;
            if self.t == 0 {
                self.mo[0] = cut;
                self.mo_inc = 0.0;
            } else {
                self.mo_inc = (cut - self.mo[0]) / CTL as f32;
            }
            self.mo[1] = mv.octaves;
            self.mo[2] = mv.pan.clamp(-1.0, 1.0);
        }
        if self.bend != 0.0 || self.hand != 0.0 || self.slide != 0.0 || self.mo[1] != 0.0 || self.sc[0] != 0.0 {
            self.step_now *= ((self.bend + self.hand + self.slide + self.mo[1] + self.sc[0]) as f64).exp2();
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
            let g = self.level * self.env_now * (1.0 - self.mo[0]) * (1.0 + self.sc[1]);
            self.env_now += self.env_inc;
            self.mo[0] = (self.mo[0] + self.mo_inc).clamp(0.0, 1.0);
            let side = self.mo[2] + self.sc[2];
            out[0] += s[0] * self.gl * g * (1.0 - side).clamp(0.0, 1.0);
            out[1] += s[1] * self.gr * g * (1.0 + side).clamp(0.0, 1.0);
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
    /// Pitch wheel position per channel, in octaves.
    wheel: Vec<f32>,
    buses: Vec<Vec<[f32; 2]>>,
    /// Equaliser memory per insert and band: two values per side.
    eq: Vec<[[f32; 4]; 3]>,
    /// Running effect units per insert, in slot order.
    fx: Vec<Vec<(u16, fx::Unit)>>,
    /// Controller outputs waiting to be applied.
    driven: Vec<(Target, f32)>,
    /// Envelope sender notes still running.
    env_runs: Vec<EnvRun>,
    /// Arpeggiators at work: the notes each holds and where it is in its run.
    arps: Vec<ArpRun>,
    /// Note scripts at work.
    scripts: Vec<ScriptRun>,
    /// Set while a script's own note is being started: the instrument it is for, the first note unit after the script, the voice's tag.
    script_emit: Option<(usize, usize, u64)>,
    script_serial: u32,
    /// Output values note scripts sent, waiting for their bays: the channel, the script's unit, the output, the value.
    script_ctl: Vec<(u16, usize, u16, f32)>,
    /// What bay cables last sent to the settings of layered instruments: the channel, the unit, the setting, the value.
    layer_sets: Vec<(usize, usize, u16, f32)>,
    /// MIDI messages on their way out, with the frames each still waits.
    midi_wait: Vec<(u64, crate::midiout::Msg)>,
    /// Hand-held notes of sender channels: channel, key, and the message that ends the note.
    midi_live: Vec<(u16, u8, crate::midiout::Msg)>,
    /// Notes on their way to hosted plug-ins: frames still to wait, channel, and the note.
    host_wait: Vec<(u64, u16, crate::host::Ev)>,
    /// Hand-held notes of hosted plug-ins: channel and key.
    host_live: Vec<(u16, u8)>,
    host_evs: Vec<crate::host::Ev>,
    offline: bool,
    host_buf: Vec<[f32; 2]>,
    /// The layer each pad last played, for pads whose layers take turns: channel, key, layer.
    pad_turn: Vec<(u16, u8, usize)>,
    /// What each sender channel last put on the wire, so only changes are sent.
    midi_seen: Vec<MidiSeen>,
    midi_ran: bool,
    seed: u32,
    /// Whose turn it is at the note units that hand their targets out one at a time.
    shape_turn: crate::noteshape::Turns,
    /// Frames an insert keeps running after its input stops, for effect tails.
    ring: Vec<usize>,
    /// The patch bay of each channel whose generator is one, with the sound its notes made this block.
    chan_fx: Vec<Option<ChanFx>>,
    scratch: Vec<[f32; 2]>,
    /// What each insert put out this block, kept for units on other inserts that listen to it.
    cap: Vec<Vec<[f32; 2]>>,
    cap_at: Vec<u64>,
    /// This block's share of the sound card inputs.
    inbuf: Vec<f32>,
    blockno: u64,
    dirty: Vec<bool>,
    pos: f64,
    was_playing: bool,
    /// Set after a start or a seek: clips already under the play head must be picked up mid-way.
    chase: bool,
    /// Ticks of the count-in bar still to click through before the song moves.
    count_in: f64,
    born: u64,
    ov: Overrides,
    /// Ticks per frame at the tempo of the block being played.
    tpf: f64,
    /// Metronome sounds: bar, beat.
    clicks: [Arc<Sample>; 2],
}

/// How many keys a channel's root key moves the notes it is sent: none for channels that play a recording
/// (their root key sets the pitch instead) and for layers, which hand the note on as it came.
fn root_shift(ch: &crate::song::Chan) -> i32 {
    match ch.kind {
        ChannelKind::Sampler | ChannelKind::AudioClip | ChannelKind::Layer | ChannelKind::AutomationClip => 0,
        _ => 60 - ch.root_key as i32,
    }
}

/// The channel's note settings, with the instrument's own arpeggio standing in when the channel's is off.
fn own_arp(ch: &crate::song::Chan) -> crate::song::Sampler {
    let mut f = ch.smp;
    if f.arp_dir == 0 {
        if let Some((way, range, steps)) = ch.gen.as_ref().filter(|g| g.kind == crate::synth::Kind::Poly).and_then(|g| crate::analog::poly_arp(&g.p)) {
            f.arp_dir = way;
            f.arp_range = range;
            f.arp_chord = -1;
            f.arp_steps = steps;
            f.arp_gate = 0.6;
            f.arp_repeat = 1;
            f.arp_slide = false;
        }
    }
    f
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

    /// Start one note as the channel's time settings shape it: shifted late, cut at the gate, or handed to the
    /// channel's arpeggiator. `len_frames` None means "no note-off" (a step, or a live key still held).
    fn note_on(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool) {
        self.note_on_at(song, n, delay, len_frames, live, 0);
    }

    fn note_on_at(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool, depth: u8) {
        let Some(ch) = song.channels.get(n.chan as usize) else { return };
        crate::analog::set_beat(60.0 / self.transport.tempo().max(1.0) as f32);
        let f = &own_arp(ch);
        if ch.kind == ChannelKind::Layer {
            // a layer hands the note to its children, and each shapes it by its own time settings and arpeggiator
            if ch.enabled && !n.muted && depth < 4 && (f.key_lo..=f.key_hi).contains(&(n.key as u32)) {
                for &k in &ch.children {
                    if k != n.chan {
                        self.note_on_at(song, &Note { chan: k, ..n.clone() }, delay, len_frames, live, depth + 1);
                    }
                }
            }
            return;
        }
        let step = song.step() as f64 / self.now_tpf(song);
        let delay = if live { delay } else { delay + (f.shift as f64 * step) as u32 };
        let mut len_frames = len_frames;
        if !live && f.gate_steps < crate::song::GATE_OFF && !(n.porta && f.full_porta) && !n.slide {
            let g = ((f.gate_steps as f64 * step) as u64).max(1);
            len_frames = Some(len_frames.map_or(g, |l| l.min(g)));
        }
        if f.arp_dir != 0 && !n.slide && !n.muted && ch.enabled && !matches!(ch.kind, ChannelKind::Layer | ChannelKind::AutomationClip | ChannelKind::AudioClip) {
            let own = f.arp_chord >= 0;
            let from = delay as i64;
            // a step has no length of its own: it holds for one step
            let to = if live { None } else { Some(from + len_frames.unwrap_or(step as u64).max(1) as i64) };
            let held = ArpHeld { note: n.clone(), from, to, live, sus: false };
            match self.arps.iter_mut().find(|r| r.chan == n.chan && (!own || r.key == Some(n.key)) && r.own == own) {
                Some(r) => {
                    r.held.retain(|h| h.note.key != n.key || h.to.is_some() != to.is_some());
                    r.held.push(held);
                }
                None => self.arps.push(ArpRun { chan: n.chan, key: own.then_some(n.key), own, held: vec![held], i: 0, wait: from as f64 }),
            }
            return;
        }
        self.note_on_plain(song, n, delay, len_frames, live);
    }

    /// Ticks per frame, whether or not the song is running.
    fn now_tpf(&self, song: &Song) -> f64 {
        if self.was_playing && self.tpf > 0.0 {
            self.tpf
        } else {
            (song.tempo.max(1.0) as f64 * song.ppq as f64 / 60.0 / self.sr as f64).max(1e-9)
        }
    }

    /// The running script in front of this instrument, started if it is not running yet. `None` when it may not or cannot run.
    fn script_run(&mut self, chan: u16, li: usize, at: usize, port: u32, node: Option<usize>, def: &Arc<crate::script::ScriptDef>) -> Option<usize> {
        if let Some(i) = self.scripts.iter().position(|r| r.chan == chan && r.li == li && r.at == at && (Arc::ptr_eq(&r.def, def) || r.def == *def)) {
            self.scripts[i].port = port;
            return Some(i);
        }
        let inst = crate::script::Inst::new(def)?;
        // one script to a place: whatever stood here before has been taken out or changed
        let mut gone = Vec::new();
        self.scripts.retain(|r| {
            let stay = !(r.chan == chan && r.li == li);
            if !stay {
                gone.push(r.serial);
            }
            stay
        });
        for v in self.voices.iter_mut().filter(|v| v.active && v.script != 0 && gone.contains(&((v.script >> 32) as u32))) {
            v.let_go();
        }
        self.script_serial += 1;
        self.scripts.push(ScriptRun { serial: self.script_serial, chan, li, at, port, node, def: def.clone(), inst, wait: Vec::new(), held: Vec::new(), acc: 0.0, next: 0 });
        Some(self.scripts.len() - 1)
    }

    /// Every script forgets the notes it holds and lets its own go.
    fn hush_scripts(&mut self) {
        for r in self.scripts.iter_mut() {
            r.wait.clear();
            r.held.clear();
            r.inst.step("x");
        }
        for v in self.voices.iter_mut().filter(|v| v.active && v.script != 0) {
            v.let_go();
        }
    }

    /// Let every note script live through the next `n` frames: it hears the notes that start and end in them
    /// and each tick that passes, and the notes it plays, bends and ends are done to the instrument behind it.
    fn run_scripts(&mut self, song: &Song, n: usize) {
        if self.scripts.is_empty() {
            return;
        }
        use crate::script::Out;
        let tpf = self.now_tpf(song);
        let tempo = self.transport.tempo();
        let mut runs = std::mem::take(&mut self.scripts);
        let mut text = String::with_capacity(256);
        runs.retain_mut(|r| {
            // a script taken out of the song, or changed, stops and takes its notes with it
            let here = song.channels.get(r.chan as usize).and_then(|ch| {
                let b = ch.bay()?;
                let node = if r.li == 0 { b.lead } else { b.layers.get(r.li - 1)?.0 };
                let chain = &b.fx.sub.as_ref()?.shape.iter().find(|s| s.0 == node)?.1;
                match &chain.get(r.at)?.shaper {
                    crate::noteshape::Shaper::Script(d) => Some(Arc::ptr_eq(d, &r.def) || **d == *r.def),
                    _ => None,
                }
            });
            if here != Some(true) {
                let tag = r.serial;
                for v in self.voices.iter_mut().filter(|v| v.active && v.script != 0 && (v.script >> 32) as u32 == tag) {
                    v.let_go();
                }
                return false;
            }
            let before = r.acc;
            r.acc += n as f64 * tpf;
            let whole = r.acc.floor();
            r.acc -= whole;
            text.clear();
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("c {} {} {:?} -", song.ppq.max(1), self.was_playing as u8, tempo));
            r.wait.retain_mut(|(d, line)| {
                if (*d as usize) < n {
                    text.push('\n');
                    text.push_str(line);
                    false
                } else {
                    *d -= n as u32;
                    true
                }
            });
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("\nt {}", whole as u64));
            r.held.retain_mut(|h| match h.left {
                Some(l) if l < n as u64 => {
                    let _ = std::fmt::Write::write_fmt(&mut text, format_args!("\nr {}", h.hid));
                    false
                }
                Some(l) => {
                    h.left = Some(l - n as u64);
                    true
                }
                None => true,
            });
            let top = (r.serial as u64) << 32;
            // a tick of the block, as the frame of it that tick falls on
            let frame = |k: u32| if k == 0 { 0 } else { (((k as f64 - before) / tpf).max(0.0) as usize).min(n.saturating_sub(1)) };
            for o in r.inst.step(&text) {
                match o {
                    Out::On(k, vid, v) => {
                        if v.output.max(0) as u32 != r.port {
                            continue;
                        }
                        let pitch = (v.note + v.fine).clamp(0.0, 131.0);
                        let key = pitch.round();
                        let note = Note { chan: r.chan, key: key as u8, fine_cents: (pitch - key) * 100.0, vel: (v.vel / SCRIPT_VEL).max(0.0), pan: v.pan.clamp(-1.0, 1.0), mod_x: v.fcut.clamp(-1.0, 1.0), mod_y: v.fres.clamp(-1.0, 1.0), color: v.color, ..Default::default() };
                        self.script_emit = Some((r.li, r.at + 1, top | vid as u64));
                        self.note_on_depth(song, &note, frame(k) as u32, None, false, 0);
                        self.script_emit = None;
                    }
                    Out::Change(_, vid, v) => {
                        let tag = top | vid as u64;
                        for x in self.voices.iter_mut().filter(|x| x.active && x.script == tag) {
                            x.sc[0] = (v.note + v.fine - x.sc_key) / 12.0;
                            let to = law::velocity_gain((v.vel / SCRIPT_VEL).max(0.0));
                            x.sc[1] = if x.note_gain > 1e-6 { to / x.note_gain - 1.0 } else { 0.0 };
                            x.sc[2] = v.pan.clamp(-1.0, 1.0) - x.note_pan;
                        }
                    }
                    Out::Off(k, vid) => {
                        let tag = top | vid as u64;
                        let at = frame(k) as u64;
                        for x in self.voices.iter_mut().filter(|x| x.active && x.script == tag && !x.releasing) {
                            x.remaining = Some(x.delay as u64 + at);
                        }
                    }
                    Out::Ctl(k, v) => {
                        // the script's own outputs move whatever the bay cables them to
                        if let (Some(node), true) = (r.node, self.script_ctl.len() < 1024) {
                            self.script_ctl.push((r.chan, node, k as u16, v));
                        }
                    }
                    Out::Err(_) => {}
                }
            }
            true
        });
        // a script may have been started by a note another script played
        runs.append(&mut self.scripts);
        self.scripts = runs;
    }

    /// Let every arpeggiator play the notes that fall in the next `n` frames.
    fn run_arps(&mut self, song: &Song, n: usize) {
        if self.arps.is_empty() {
            return;
        }
        let step = song.step() as f64 / self.now_tpf(song);
        let mut arps = std::mem::take(&mut self.arps);
        let mut keys: Vec<u8> = Vec::with_capacity(32);
        let mut seq: Vec<u8> = Vec::with_capacity(128);
        for r in arps.iter_mut() {
            let Some(ch) = song.channels.get(r.chan as usize) else {
                r.held.clear();
                continue;
            };
            let f = own_arp(ch);
            if f.arp_dir == 0 {
                r.held.clear();
                continue;
            }
            let period = (f.arp_steps as f64 * step).max(self.sr as f64 * 0.004);
            while r.wait < n as f64 {
                let t = r.wait as i64;
                let on = |h: &ArpHeld| h.from <= t && h.to.map_or(true, |e| t < e);
                if !r.held.iter().any(on) {
                    // nothing held right now: wait for the next note, and start the run over with it
                    r.i = 0;
                    match r.held.iter().filter(|h| h.from > t).map(|h| h.from).min() {
                        Some(next) => {
                            r.wait = next as f64;
                            continue;
                        }
                        None => break,
                    }
                }
                keys.clear();
                if r.own {
                    let root = r.key.unwrap_or(60);
                    let shape: &[u8] = if f.arp_chord <= 0 { &[0] } else { crate::song::ARP_CHORDS[(f.arp_chord as usize - 1).min(crate::song::ARP_CHORDS.len() - 1)].1 };
                    keys.extend(shape.iter().map(|&s| root.saturating_add(s)));
                } else {
                    keys.extend(r.held.iter().filter(|h| on(h)).map(|h| h.note.key));
                    keys.sort_unstable();
                    keys.dedup();
                }
                if f.arp_chord == -2 && keys.len() == 1 && f.arp_range <= 1 {
                    // a lone note is left to sound as written
                    for h in r.held.iter_mut().filter(|h| !h.sus && h.from <= t && h.to.map_or(true, |e| t < e)) {
                        h.sus = true;
                        let len = h.to.map(|e| (e - t).max(1) as u64);
                        self.note_on_plain(song, &h.note, t.max(0) as u32, len, h.live);
                    }
                    r.wait += period;
                    continue;
                }
                seq.clear();
                for o in 0..f.arp_range.max(1) {
                    for _ in 0..f.arp_repeat.max(1) {
                        seq.extend(keys.iter().map(|&k| k as u32 + 12 * o as u32).filter(|&k| k <= 131).map(|k| k as u8));
                    }
                }
                if seq.is_empty() {
                    seq.push(keys[0]);
                }
                let len = seq.len() as u32;
                let at = match f.arp_dir {
                    2 => len - 1 - r.i % len,
                    3 if len > 1 => {
                        let k = r.i % (2 * len - 2);
                        if k < len { k } else { 2 * len - 2 - k }
                    }
                    4 => {
                        let k = r.i % (2 * len);
                        if k < len { k } else { 2 * len - 1 - k }
                    }
                    5 => {
                        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        (self.seed >> 8) % len
                    }
                    _ => r.i % len,
                };
                // how hard, where and in which colour comes from the newest held note
                let Some(tpl) = r.held.iter().filter(|h| on(h)).last().map(|h| h.note.clone()) else { break };
                let note = Note { key: seq[at as usize], porta: tpl.porta || (f.arp_slide && r.i > 0), slide: false, ..tpl };
                let hold = ((period * f.arp_gate.clamp(0.02, 1.0) as f64) as u64).max(1);
                self.note_on_plain(song, &note, t.max(0) as u32, Some(hold), false);
                r.i = r.i.wrapping_add(1);
                r.wait += period;
            }
            r.wait -= n as f64;
            for h in r.held.iter_mut() {
                h.from -= n as i64;
                if let Some(e) = h.to.as_mut() {
                    *e -= n as i64;
                }
            }
            r.held.retain(|h| h.to.map_or(true, |e| e > 0));
        }
        arps.retain(|r| !r.held.is_empty());
        arps.append(&mut self.arps);
        self.arps = arps;
    }

    /// Start one note with what the channel adds to it: the offsets its hit strength and key bring, and its
    /// echoes, each a note of its own.
    fn note_on_plain(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool) {
        let Some(ch) = song.channels.get(n.chan as usize) else { return };
        let f = &ch.smp;
        let mut n = n.clone();
        if !n.slide {
            for (t, at) in [(&f.track_vel, n.vel * 100.0), (&f.track_key, n.key as f32)] {
                let off = (at - t[0]) / 64.0;
                n.pan = (n.pan + t[1] * off).clamp(-1.0, 1.0);
                n.mod_x = (n.mod_x + t[2] * off).clamp(-1.0, 1.0);
                n.mod_y = (n.mod_y + t[3] * off).clamp(-1.0, 1.0);
            }
        }
        let echoes = if f.echo_feedback > 0.0 && !n.slide && ch.kind != ChannelKind::AudioClip { f.echoes as u32 } else { 0 };
        if echoes == 0 {
            return self.note_on_one(song, &n, delay, len_frames, live);
        }
        let gap = f.echo_steps as f64 * song.step() as f64 / self.now_tpf(song);
        for k in 0..=echoes {
            let mut pan = n.pan + f.echo_pan * k as f32;
            if f.echo_fat {
                // fat mode: the echoes sit on alternate sides
                pan = n.pan + if k == 0 { 0.0 } else if k % 2 == 1 { f.echo_pan } else { -f.echo_pan };
            } else if f.echo_ping {
                // ping pong: the pan turns round at each side
                let x = (pan + 1.0).rem_euclid(4.0);
                pan = if x > 2.0 { 3.0 - x } else { x - 1.0 };
            }
            let e = Note {
                vel: n.vel * f.echo_feedback.powi(k as i32).sqrt(),
                pan: pan.clamp(-1.0, 1.0),
                fine_cents: n.fine_cents + f.echo_cents * k as f32,
                porta: n.porta && k == 0,
                ..n.clone()
            };
            self.note_on_one(song, &e, delay + (k as f64 * gap) as u32, len_frames, live);
        }
    }

    fn note_on_one(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool) {
        let mark = self.born;
        // the note a portamento glides in from: the newest one still sounding on the channel
        let from = if n.porta {
            self.voices.iter().filter(|v| v.active && !v.preview && v.chan == n.chan && v.color == n.color).max_by_key(|v| v.born).map(|v| v.key as f32 / 12.0 + v.slide_to)
        } else {
            None
        };
        self.note_on_depth(song, n, delay, len_frames, live, 0);
        let glide = 1.0 / (self.sr * 0.12);
        for v in self.voices.iter_mut().filter(|v| v.active && v.born > mark) {
            v.color = n.color;
            if v.fon {
                v.fcut = (v.fcut + n.mod_x * 0.5).clamp(0.0, 1.0);
                v.fres = (v.fres + n.mod_y * 0.5).clamp(0.0, 1.0);
            }
            if let Some(from) = from {
                v.slide = from - v.key as f32 / 12.0;
                v.slide_to = 0.0;
                v.slide_inc = v.slide.abs() * glide;
            }
        }
    }

    fn note_on_depth(&mut self, song: &Song, n: &Note, delay: u32, len_frames: Option<u64>, live: bool, depth: u8) {
        let Some(ch) = song.channels.get(n.chan as usize) else { return };
        if !ch.enabled || n.muted {
            return;
        }
        // a script's own note has been through all this already
        let emit = self.script_emit;
        // keys outside the channel's region are not for it
        if emit.is_none() && ((n.key as u32) < ch.smp.key_lo || n.key as u32 > ch.smp.key_hi) {
            return;
        }
        // an instrument has no recording to repitch: its root key moves the keys it is sent
        let moved;
        let n = match if emit.is_some() { 0 } else { root_shift(ch) } {
            0 => n,
            by => {
                moved = Note { key: (n.key as i32 + by).clamp(0, 131) as u8, ..n.clone() };
                &moved
            }
        };
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
        if ch.plugin.eq_ignore_ascii_case(crate::fm::ENV_SENDER) {
            // the envelope sender makes no sound either: the note sets its envelopes running, and they are sent block by block
            if let Some(set) = ch.gen.as_ref().and_then(|g| crate::fm::Sender::parse(&g.raw)) {
                self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let rand = (self.seed >> 8) as f32 / (1u32 << 24) as f32;
                self.env_runs.retain(|r| r.chan != n.chan || r.node.is_some());
                self.env_runs.push(EnvRun { chan: n.chan, key: n.key, sent: n.key, node: None, vel: n.vel, rand, t: 0.0, rel: None, left: len_frames.map(|f| f + delay as u64), set });
            }
            return;
        }
        if ch.ext.is_some() {
            // a hosted plug-in is handed the note and its end; the sound is made when the block is mixed
            use crate::host::Ev;
            let key = n.key.min(127);
            let vel = (n.vel * 100.0 / 127.0).clamp(1.0 / 127.0, 1.0);
            let off = Ev::NoteOff { at: 0, key, chan: 0 };
            self.host_wait.push((delay as u64, n.chan, Ev::NoteOn { at: 0, key, chan: 0, vel }));
            match len_frames {
                Some(f) => self.host_wait.push((delay as u64 + f.max(1), n.chan, off)),
                None if live => self.host_live.push((n.chan, n.key.min(127))),
                None => self.host_wait.push((delay as u64 + (self.sr * 0.25) as u64, n.chan, off)),
            }
            return;
        }
        if ch.plugin.eq_ignore_ascii_case(crate::midiout::IDENT) {
            // the MIDI sender makes no sound: the note goes out as a message, and its end follows when it is due
            let h = crate::midiout::Head::read(ch.gen.as_ref().map(|g| &g.raw[..]).unwrap_or(&[]));
            let mc = if h.by_color { (h.chan + n.color) & 15 } else { h.chan };
            let key = n.key.min(127);
            let vel = ((n.vel * 100.0).round() as i32).clamp(1, 127) as u8;
            let off = crate::midiout::Msg::new(h.port, &[0x80 | mc, key, 0]);
            self.midi_wait.push((delay as u64, crate::midiout::Msg::new(h.port, &[0x90 | mc, key, vel])));
            match len_frames {
                Some(f) => self.midi_wait.push((delay as u64 + f.max(1), off)),
                None if live => self.midi_live.push((n.chan, n.key, off)),
                None => self.midi_wait.push((delay as u64 + (self.sr * 0.25) as u64, off)),
            }
            return;
        }
        if ch.plugin.eq_ignore_ascii_case(crate::synth::KEY_SENDER) {
            // the key sender makes no sound: the key and how hard it was hit go to whatever is linked to it
            let outs = [crate::synth::key_sender_value(ch.gen.as_ref().map(|g| &g.raw[..]).unwrap_or(&[]), n.key), n.vel.clamp(0.0, 1.0)];
            let mut fitted = false;
            for l in song.ctl_links.iter().filter(|l| l.chan() == Some(n.chan as usize)) {
                if let Some(&v) = outs.get(l.out as usize) {
                    if !fitted {
                        self.ov.fit(song);
                        fitted = true;
                    }
                    self.ov.set(l.target, v);
                }
            }
            return;
        }
        if n.slide {
            // a slide note bends what is sounding in its colour to its own pitch over its length
            let frames = len_frames.unwrap_or(1).max(1) as f32;
            for v in self.voices.iter_mut().filter(|v| v.active && !v.preview && v.chan == n.chan && v.color == n.color) {
                v.slide_to = (n.key as f32 - v.key as f32) / 12.0 + n.fine_cents / 1200.0;
                v.slide_inc = (v.slide_to - v.slide).abs() / frames;
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
                {
                    let note_gain = law::velocity_gain(n.vel) * f.gain;
                    let note_pan = n.pan.clamp(-1.0, 1.0);
                    let (pl, pr) = law::pan((chan_pan + note_pan).clamp(-1.0, 1.0));
                    let gain = chan_vol * note_gain;
                    self.start(Voice {
                        sample: Some(smp.clone()),
                        step,
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
                        delay,
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
                // layers that share the note either sound together or take turns, as the pad asks
                let mode = map.hit(n.key, vel).next().map_or(0, |z| z.pick);
                let among = map.hit(n.key, vel).count();
                let only = (mode != 0 && among > 1).then(|| self.pad_pick(n.chan, n.key, mode, among));
                for (i, z) in map.hit(n.key, vel).enumerate() {
                    if only.is_some_and(|o| o != i) {
                        continue;
                    }
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
                        // the zone's own level rides with the note's, so later volume moves keep it
                        note_gain: note_gain * z.gain,
                        note_pan: n.pan,
                        follow: z.track,
                        venv: z.env.unwrap_or_default(),
                        fon: z.filter.is_some(),
                        fcut: z.filter.map_or(1.0, |f| f.0),
                        fres: z.filter.map_or(0.0, |f| f.1),
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
                // a script ends its own notes
                let remaining = if live || emit.is_some() { len_frames } else { Some(len_frames.unwrap_or(min_len).max(1)) };
                let layers = ch.bay().map_or(&[][..], |b| &b.layers[..]);
                if let Some(glide) = ch.gen.as_ref().filter(|_| layers.is_empty()).and_then(|g| g.mono()) {
                    // one voice at a time: a note that arrives while another is held bends that one to its
                    // own pitch; anything already fading is cut short
                    let mut held = false;
                    let cut = ms(4.0);
                    for v in self.voices.iter_mut().filter(|v| v.active && !v.preview && v.chan == n.chan) {
                        if held || v.releasing {
                            v.kill = true;
                            v.release = v.release.max(cut);
                            v.let_go();
                            if v.delay > 0 {
                                v.active = false;
                            }
                            continue;
                        }
                        let now = v.key as f32 / 12.0 + v.slide;
                        v.key = n.key;
                        v.dphase = hz / sr;
                        v.slide = now - n.key as f32 / 12.0;
                        v.slide_to = 0.0;
                        v.slide_inc = if glide <= 0.0 { f32::MAX } else { v.slide.abs() / (glide * sr) };
                        v.remaining = remaining.map(|r| r + delay as u64);
                        v.live = live;
                        held = true;
                    }
                    if held {
                        return;
                    }
                }
                for li in 0..=layers.len() {
                let g = if li == 0 { ch.gen.as_ref() } else { Some(&layers[li - 1].1) };
                // note units on the cables to this instrument have their say first
                let chain = ch.bay().and_then(|b| {
                    let node = if li == 0 { b.lead } else { b.layers[li - 1].0 };
                    b.fx.sub.as_ref()?.shape.iter().find(|s| s.0 == node).map(|s| &s.1[..])
                });
                let plain = crate::noteshape::Ev::of(n);
                let chain = match emit {
                    Some((to, from, _)) => {
                        if to != li {
                            continue;
                        }
                        chain.map(|c| &c[from.min(c.len())..])
                    }
                    None => {
                        if let Some((si, def)) = chain.and_then(crate::noteshape::script_at) {
                            let c = chain.unwrap_or(&[]);
                            if let Some(ri) = self.script_run(n.chan, li, si, c[si].port, Some(c[si].node), &def) {
                                // the script is handed the note, and what sounds is up to it
                                let before = crate::noteshape::apply(&c[..si], plain, n.chan, &mut self.seed, &mut self.shape_turn);
                                let r = &mut self.scripts[ri];
                                for ev in before {
                                    r.next += 1;
                                    let vel = (ev.vel * ev.gain * SCRIPT_VEL).clamp(0.0, 1.0);
                                    r.wait.push((delay, format!("n {} {:?} {:?} {:?} {:?} {:?} {} 0", r.next, ev.key as f32 + ev.fine_cents / 100.0, vel, ev.pan, ev.mod_x, ev.mod_y, ev.color)));
                                    let left = match len_frames {
                                        Some(f) => Some(f.max(1) + delay as u64),
                                        None if live => None,
                                        None => Some(delay as u64 + (sr * 0.25) as u64),
                                    };
                                    r.held.push(ScriptHeld { hid: r.next, key: n.key, live: live && len_frames.is_none(), left });
                                }
                                continue;
                            }
                        }
                        chain
                    }
                };
                let evs = match chain {
                    Some(c) => crate::noteshape::apply(c, plain, n.chan, &mut self.seed, &mut self.shape_turn),
                    None => vec![plain],
                };
                let motion = chain.and_then(crate::noteshape::motion_of);
                // a sender held in the bay makes no sound: the notes that reach it set its outputs, which the bay's cables carry
                let held_sender = ch.bay().and_then(|b| {
                    let node = if li == 0 { b.lead } else { b.layers[li - 1].0 };
                    let ident = &b.fx.sub.as_ref()?.nodes.get(node)?.ident;
                    let env = ident.eq_ignore_ascii_case(crate::fm::ENV_SENDER);
                    (env || ident.eq_ignore_ascii_case(crate::synth::KEY_SENDER)).then_some((node, env))
                });
                if let Some((node, env)) = held_sender {
                    for ev in evs {
                        let key = ev.key.clamp(0, 131) as u8;
                        let vel = (ev.vel * ev.gain).clamp(0.0, 1.0);
                        let raw = g.map(|g| &g.raw[..]).unwrap_or(&[]);
                        if !env {
                            if self.script_ctl.len() < 1024 {
                                self.script_ctl.push((n.chan, node, 0, crate::synth::key_sender_value(raw, key)));
                                self.script_ctl.push((n.chan, node, 1, vel));
                            }
                        } else if let Some(set) = crate::fm::Sender::parse(raw) {
                            self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                            let rand = (self.seed >> 8) as f32 / (1u32 << 24) as f32;
                            self.env_runs.retain(|r| r.chan != n.chan || r.node != Some(node));
                            let left = match len_frames {
                                Some(f) => Some(f.max(1) + delay as u64),
                                None if live => None,
                                None => Some(delay as u64 + (sr * 0.25) as u64),
                            };
                            self.env_runs.push(EnvRun { chan: n.chan, key: n.key, sent: key, node: Some(node), vel, rand, t: 0.0, rel: None, left, set });
                        }
                    }
                    continue;
                }
                for ev in evs {
                let shaped = ev != plain;
                let (note_gain, gain, pl, pr, hz) = if shaped {
                    let note_gain = law::velocity_gain(ev.vel) * ev.gain;
                    let (pl, pr) = law::pan((chan_pan + ev.pan).clamp(-1.0, 1.0));
                    (note_gain, chan_vol * note_gain, pl, pr, 440.0 * 2f32.powf((ev.key as f32 - 69.0) / 12.0 + (ch.pitch_cents + ev.fine_cents) / 1200.0))
                } else {
                    (note_gain, gain, pl, pr, hz)
                };
                let key = ev.key.clamp(0, 131) as u8;
                // the voice goes by the key that was pressed, so that letting go of it finds the voice
                let mut v = Voice { dphase: hz / sr, level: 0.0, attack: ms(4.0), release: ms(90.0), remaining, gl: gain * pl, gr: gain * pr, insert: ch.insert, delay, chan: n.chan, key: n.key, live, note_gain, note_pan: ev.pan, follow: true, ..Default::default() };
                if let Some((from, steps)) = ev.glide {
                    v.slide = (from - ev.key) as f32 / 12.0;
                    v.slide_to = 0.0;
                    v.slide_inc = v.slide.abs() / (steps * song.ppq.max(1) as f32 / 4.0 / self.tpf as f32).max(1.0);
                }
                v.feed = li as u8;
                v.color = ev.color;
                v.mo_beat = 60.0 / self.transport.tempo().max(1.0);
                v.motion = motion.clone();
                if let Some((_, _, tag)) = emit {
                    v.script = tag;
                    v.sc_key = n.key as f32 + n.fine_cents / 100.0;
                }
                if let Some(g) = g {
                    // a built-in instrument: its own oscillators, shaped by the channel's envelopes and filter
                    // settings a clip or an event is driving stand in for the stored ones from this note on
                    let driven = li == 0 && self.ov.gen.iter().any(|e| e.0 == n.chan as usize);
                    // a layered instrument of a bay: what the bay's cables last sent to its settings
                    let node = if li > 0 { song.channels.get(n.chan as usize).and_then(|c| c.bay()).and_then(|b| b.layers.get(li - 1)).map(|l| l.0) } else { None };
                    let cabled = node.is_some_and(|nd| self.layer_sets.iter().any(|e| e.0 == n.chan as usize && e.1 == nd));
                    let tone = if driven {
                        let mut g = g.clone();
                        for e in self.ov.gen.iter().filter(|e| e.0 == n.chan as usize) {
                            g.set_travel(e.1 as usize, e.2);
                        }
                        g.tone()
                    } else if cabled {
                        let mut g = g.clone();
                        for e in self.layer_sets.iter().filter(|e| e.0 == n.chan as usize && Some(e.1) == node) {
                            g.set_travel(e.2 as usize, e.3);
                        }
                        g.tone()
                    } else {
                        g.tone()
                    };
                    v.tstate = crate::synth::ToneState::start(&tone, (self.born as u32).wrapping_mul(2654435761).wrapping_add(key as u32));
                    v.tstate.note(&tone, sr, key, note_gain.min(1.0), hz / sr);
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
            }
            ChannelKind::Layer | ChannelKind::AutomationClip => {}
        }
    }

    fn note_off(&mut self, song: &Song, chan: u16, key: u8) {
        let shift = song.channels.get(chan as usize).map_or(0, root_shift);
        let sent = key;
        let key = (key as i32 + shift).clamp(0, 131) as u8;
        for r in self.arps.iter_mut().filter(|r| r.chan == chan) {
            r.held.retain(|h| !(h.live && h.note.key == sent));
        }
        for r in self.env_runs.iter_mut().filter(|r| r.chan == chan && r.key == key && r.rel.is_none()) {
            r.rel = Some(r.t);
        }
        for h in self.scripts.iter_mut().filter(|r| r.chan == chan).flat_map(|r| r.held.iter_mut()).filter(|h| h.live && h.key == key) {
            h.live = false;
            h.left = Some(0);
        }
        if let Some(ch) = song.channels.get(chan as usize) {
            if ch.kind == ChannelKind::Layer {
                for &k in &ch.children {
                    if song.channels.get(k as usize).map(|c| c.kind != ChannelKind::Layer).unwrap_or(false) {
                        self.note_off(song, k, sent);
                    }
                }
                return;
            }
        }
        while let Some(i) = self.host_live.iter().position(|m| m.0 == chan && m.1 == key.min(127)) {
            self.host_live.swap_remove(i);
            self.host_wait.push((0, chan, crate::host::Ev::NoteOff { at: 0, key: key.min(127), chan: 0 }));
        }
        while let Some(i) = self.midi_live.iter().position(|m| m.0 == chan && m.1 == key) {
            let m = self.midi_live.swap_remove(i);
            self.midi_wait.push((0, m.2));
        }
        for v in self.voices.iter_mut().filter(|v| v.active && v.live && v.chan == chan && v.key == key) {
            v.let_go();
        }
    }

    /// Ends every note a sender channel has out, now.
    fn midi_hush(&mut self) {
        self.midi_wait.retain_mut(|(left, m)| {
            *left = 0;
            m.data[0] & 0xf0 == 0x80
        });
        for (_, _, off) in self.midi_live.drain(..) {
            self.midi_wait.push((0, off));
        }
        self.host_wait.retain_mut(|(left, _, e)| {
            *left = 0;
            matches!(e, crate::host::Ev::NoteOff { .. })
        });
        for (chan, key) in self.host_live.drain(..) {
            self.host_wait.push((0, chan, crate::host::Ev::NoteOff { at: 0, key, chan: 0 }));
        }
    }

    /// Which of `among` layers a pad plays this time: in turn (1), at random (2), or at random but not the
    /// one it played last (3).
    fn pad_pick(&mut self, chan: u16, key: u8, mode: u8, among: usize) -> usize {
        let at = match self.pad_turn.iter().position(|p| p.0 == chan && p.1 == key) {
            Some(i) => i,
            None => {
                self.pad_turn.push((chan, key, usize::MAX));
                self.pad_turn.len() - 1
            }
        };
        let last = self.pad_turn[at].2;
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let roll = (self.seed >> 8) as usize;
        let now = match mode {
            1 if last >= among => 0,
            1 => (last + 1) % among,
            3 if last < among => (last + 1 + roll % (among - 1)) % among,
            _ => roll % among,
        };
        self.pad_turn[at].2 = now;
        now
    }

    /// Sends what the sender channels have due in the next `n` frames, and any of their settings that changed.
    fn run_midi(&mut self, song: &Song, n: usize) {
        use crate::midiout::{knob_msgs, patch_msgs, Head, Msg, IDENT, KNOBS};
        let playing = self.transport.playing();
        if playing && !self.midi_ran {
            // starting the song sends every setting afresh
            self.midi_seen.clear();
        }
        self.midi_ran = playing;
        let mut out: Vec<(u64, Msg)> = Vec::new();
        for (i, ch) in song.channels.iter().enumerate() {
            if !ch.plugin.eq_ignore_ascii_case(IDENT) {
                continue;
            }
            let raw = ch.gen.as_ref().map(|g| &g.raw[..]).unwrap_or(&[]);
            let mix = [(ch.volume * 127.0).round() as i32, ((ch.pan * 0.5 + 0.5) * 127.0).round() as i32, (self.wheel.get(i).copied().unwrap_or(0.0) * 12.0 / 2.0 * 8192.0) as i32];
            let at = match self.midi_seen.iter().position(|s| s.chan == i as u16) {
                Some(at) => at,
                None => {
                    self.midi_seen.push(MidiSeen { chan: i as u16, raw: Vec::new(), fresh: true, mix: [i32::MIN; 3] });
                    self.midi_seen.len() - 1
                }
            };
            let seen = &mut self.midi_seen[at];
            if !seen.fresh && seen.raw == raw && seen.mix == mix {
                continue;
            }
            let h = Head::read(raw);
            let old = Head::read(&seen.raw);
            let all = seen.fresh || old.port != h.port || old.chan != h.chan;
            if all || old.patch != h.patch || old.bank != h.bank {
                patch_msgs(&h, |m| out.push((0, m)));
            }
            h.each(raw, |slot, num, on, kind, lo, hi, chan| {
                if !on || slot >= KNOBS {
                    return;
                }
                let v = h.value(raw, slot);
                let mut was = None;
                if !all {
                    old.each(&seen.raw, |s, num2, on2, kind2, _, _, chan2| {
                        if s == slot && on2 && (num2, kind2, chan2) == (num, kind, chan) {
                            was = Some(old.value(&seen.raw, slot));
                        }
                    });
                }
                if was != Some(v) {
                    let v = if hi > lo { v.clamp(lo, hi) } else { v };
                    knob_msgs(h.port, if chan >= 0 { chan as u8 } else { h.chan }, kind, num, v, |m| out.push((0, m)));
                }
            });
            if h.send_mix {
                if all || seen.mix[0] != mix[0] {
                    out.push((0, Msg::new(h.port, &[0xb0 | h.chan, 7, mix[0].clamp(0, 127) as u8])));
                }
                if all || seen.mix[1] != mix[1] {
                    out.push((0, Msg::new(h.port, &[0xb0 | h.chan, 10, mix[1].clamp(0, 127) as u8])));
                }
                if all || seen.mix[2] != mix[2] {
                    let b = (mix[2] + 8192).clamp(0, 16383);
                    out.push((0, Msg::new(h.port, &[0xe0 | h.chan, (b & 127) as u8, (b >> 7) as u8])));
                }
            }
            seen.fresh = false;
            seen.mix = mix;
            seen.raw.clear();
            seen.raw.extend_from_slice(raw);
        }
        if self.midi_wait.is_empty() && out.is_empty() {
            return;
        }
        // settings go ahead of the notes that need them
        let n = n as u64;
        let mut i = 0;
        while i < self.midi_wait.len() {
            if self.midi_wait[i].0 < n {
                out.push(self.midi_wait.remove(i));
            } else {
                self.midi_wait[i].0 -= n;
                i += 1;
            }
        }
        if out.is_empty() {
            return;
        }
        let head = out.iter().take_while(|m| m.0 == 0 && m.1.data[0] & 0xe0 != 0x80).count();
        out[head..].sort_by_key(|m| m.0);
        if let Ok(mut q) = self.transport.midi.lock() {
            if q.len() < 65_536 {
                q.extend(out.iter().map(|m| m.1));
            }
        }
    }

    fn silence(&mut self, fast: bool) {
        self.arps.clear();
        self.hush_scripts();
        self.midi_hush();
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
                    let n = Note { pos: 0, len: 0, chan, key, vel, pan: 0.0, fine_cents: 0.0, muted: false, ..Default::default() };
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
                Cmd::Bend { chan, octaves } => {
                    let c = chan as usize;
                    if self.wheel.len() <= c {
                        self.wheel.resize(c + 1, 0.0);
                    }
                    self.wheel[c] = octaves.clamp(-4.0, 4.0);
                }
                Cmd::Panic => {
                    self.arps.clear();
                    self.hush_scripts();
                    self.midi_hush();
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
        // recorded control movements: each sets its control, which then holds
        for e in &p.events[p.events.partition_point(|e| e.tick < first)..] {
            if e.tick as f64 >= b {
                break;
            }
            let t = song.event_target(e.param, e.dest);
            if let Some((lo, hi)) = song.event_range(t) {
                self.ov.fit(song);
                self.ov.set(t, (e.raw - lo) as f32 / (hi - lo).max(1) as f32);
            }
        }
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
            if let Some((a, b)) = tr.range() {
                if self.pos < a as f64 || self.pos >= b as f64 {
                    self.pos = a as f64;
                }
            }
            self.chase = true;
            self.ov.clear();
            self.count_in = if tr.recording() && tr.countdown() { song.bar() as f64 } else { 0.0 };
        }
        if !playing && self.was_playing {
            self.silence(false);
        }
        self.was_playing = playing;
        if !playing {
            return n;
        }
        if self.count_in > 0.0 {
            // one bar of clicks while the song waits where it is
            let tpf = (song.tempo.max(1.0) as f64 * song.ppq as f64 / 60.0 / self.sr as f64).max(1e-9);
            let n = n.min(((self.count_in / tpf).ceil() as usize).max(1));
            let beat = (song.ppq * 4 / song.time_sig.1.max(1) as u32).max(1) as f64;
            let bar = song.bar() as f64;
            let t0 = bar - self.count_in;
            let t1 = t0 + n as f64 * tpf;
            let mut b = (t0 / beat - 1e-9).ceil().max(0.0) * beat;
            while b < t1 && b < bar - 0.5 {
                let delay = (((b - t0) / tpf) - 1e-6).ceil().max(0.0) as u32;
                let smp = self.clicks[if b < 0.5 { 0 } else { 1 }].clone();
                self.start(Voice { sample: Some(smp), step: 1.0, level: 1.0, attack: 1.0, release: 1.0, gl: 1.0, gr: 1.0, insert: 0, delay, preview: true, ..Default::default() });
                b += beat;
            }
            self.count_in -= n as f64 * tpf;
            if self.count_in < 1e-6 {
                self.count_in = 0.0;
            }
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
        crate::minis::set_tempo(tempo);
        let tpf = (tempo as f64 * song.ppq as f64 / 60.0 / self.sr as f64).max(1e-9);
        self.tpf = tpf;
        let loop_len = if song_mode { song.length() } else { song.pattern_length(pat) } as f64;
        let loop_from = if song_mode { song.loop_start().min((loop_len as u32).saturating_sub(1)) as f64 } else { 0.0 };
        let (loop_from, loop_len) = tr.range().map(|(a, b)| (a as f64, b as f64)).unwrap_or((loop_from, loop_len));
        if self.pos >= loop_len {
            if tr.looping() {
                self.pos = loop_from;
                self.chase = true;
            } else {
                tr.set_playing(false);
                self.was_playing = false;
                return n;
            }
        }
        let to_end = (((loop_len - self.pos) / tpf).ceil() as usize).max(1);
        let mut n = n.min(to_end);
        let t0 = self.pos;
        // time markers with an action: the block ends on the marker so the action lands on it
        let mut act: Option<(f64, u8, f64)> = None;
        if song_mode && tr.range().is_none() {
            act = song.marker_action(t0);
            if let Some((at, _, _)) = act {
                n = n.min((((at - t0) / tpf).ceil() as usize).max(1));
            }
        }
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
            // in the song the clicks follow the time signature changes on the time line
            let (from, sig, until) = if song_mode {
                let (from, _, sig) = song.sig_at(t0.max(0.0) as u32);
                (from as f64, sig, song.next_sig(t0.max(0.0) as u32).map(|x| x as f64).unwrap_or(f64::MAX))
            } else {
                (0.0, (song.time_sig.0.max(1), song.time_sig.1.max(1)), f64::MAX)
            };
            let beat = (song.ppq * 4 / sig.1 as u32).max(1) as f64;
            let bar = beat * sig.0 as f64;
            let mut b = from + ((t0 - from) / beat).ceil() * beat;
            let click = |me: &mut Self, at: f64, first: bool| {
                let delay = (((at - t0) / tpf) - 1e-6).ceil().max(0.0) as u32;
                let smp = me.clicks[if first { 0 } else { 1 }].clone();
                me.start(Voice { sample: Some(smp), step: 1.0, level: 1.0, attack: 1.0, release: 1.0, gl: 1.0, gr: 1.0, insert: 0, delay, preview: true, ..Default::default() });
            };
            while b < t1 && b < until - 0.5 {
                click(self, b, ((b - from) % bar) < 0.5);
                b += beat;
            }
            if until > t0 && until < t1 {
                click(self, until, true);
            }
        }

        self.pos = t0 + n as f64 * tpf;
        if let Some((at, kind, to)) = act.filter(|a| self.pos >= a.0) {
            match kind {
                1 | 2 => {
                    self.pos = to;
                    self.chase = true;
                }
                3 => {
                    self.pos = at;
                    tr.set_playing(false);
                    self.was_playing = false;
                    self.silence(true);
                }
                6 => tr.set_recording(true),
                7 => tr.set_recording(false),
                _ => {}
            }
        }
        if self.pos >= loop_len && tr.looping() {
            self.pos = loop_from;
            self.chase |= loop_from > 0.0;
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
    /// The patch bay the channel plays through.
    unit: Option<fx::Unit>,
    /// The effects the channel's own instrument carries after its voices.
    tail: Option<(crate::tailfx::Set, crate::tailfx::State)>,
    buf: Vec<[f32; 2]>,
    dirty: bool,
    ring: usize,
}

impl Player {
    /// Mark this player as rendering to a file: hosted plug-ins lent to the render answer only to it.
    pub fn set_offline(&mut self, on: bool) {
        self.offline = on;
    }

    fn mix(&mut self, song: &Song, out: &mut [f32], channels: usize) {
        let n = out.len() / channels;
        let ni = song.inserts.len().max(1);
        if self.buses.len() != ni {
            self.buses = vec![vec![[0.0; 2]; BLOCK]; ni];
            self.dirty = vec![false; ni];
            self.fx = (0..ni).map(|_| vec![]).collect();
            self.ring = vec![0; ni];
        }
        let clock = crate::host::Clock { playing: self.transport.playing(), tempo: self.transport.tempo() as f64, beats: self.pos / song.ppq.max(1) as f64, sig: song.time_sig, offline: self.offline };
        let cx = fx::Ctx { sr: self.sr, step: (song.step() as f64 / self.tpf.max(1e-9)) as f32, clock };
        if self.chan_fx.len() != song.channels.len() {
            self.chan_fx.resize_with(song.channels.len(), || None);
        }
        for (ch, slot) in song.channels.iter().zip(self.chan_fx.iter_mut()) {
            let bay = ch.bay();
            let tail = if bay.is_none() && ch.kind == ChannelKind::Plugin { ch.gen.as_ref().map(|g| g.tail()).filter(|t| t.any()) } else { None };
            match (bay, tail, slot.as_mut()) {
                (None, None, None) => {}
                (None, None, Some(_)) => *slot = None,
                (None, Some(t), Some(ChanFx { tail: Some(have), .. })) => have.0 = t,
                (None, Some(t), _) => *slot = Some(ChanFx { unit: None, tail: Some((t, crate::tailfx::State::new(self.sr))), buf: vec![[0.0; 2]; BLOCK], dirty: false, ring: 0 }),
                (Some(b), _, Some(ChanFx { unit: Some(u), .. })) if u.fx.sub.as_ref().zip(b.fx.sub.as_ref()).is_some_and(|(a, b)| std::sync::Arc::ptr_eq(a, b) || a == b) => {}
                // only settings were turned: the running units take them and keep their sound
                (Some(b), _, Some(ChanFx { unit: Some(u), .. })) if u.fx.sub.as_ref().zip(b.fx.sub.as_ref()).is_some_and(|(a, b)| a.same_wiring(b)) => u.fx.sub = b.fx.sub.clone(),
                (Some(b), _, _) => *slot = Some(ChanFx { unit: Some(fx::Unit::new(b.fx.clone(), self.sr)), tail: None, buf: vec![[0.0; 2]; BLOCK], dirty: false, ring: 0 }),
            }
        }
        let mut live = 0;
        let follow = self.ov.any_chan;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            live += 1;
            if !v.preview {
                v.hand = self.wheel.get(v.chan as usize).copied().unwrap_or(0.0);
            }
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
                let into = if v.feed == 0 { Some(&mut cf.buf[..n]) } else { cf.unit.as_mut().and_then(|u| u.bay_feed(v.feed as usize - 1, n)) };
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
        // where the song stands in sixteenth notes, for effects that keep step with it
        let at_step = self.transport.playing().then(|| self.pos / song.step().max(1) as f64);
        // what the note scripts sent from their outputs since the last block
        for (c, node, k, v) in self.script_ctl.drain(..) {
            let Some(u) = self.chan_fx.get_mut(c as usize).and_then(|c| c.as_mut()).and_then(|c| c.unit.as_mut()) else { continue };
            if let Some(sub) = u.fx.sub.clone() {
                if let Some(st) = u.bay_state() {
                    st.drive(&sub, (node, k), v);
                }
            }
        }
        for (c, cf) in self.chan_fx.iter_mut().enumerate() {
            let Some(cf) = cf else { continue };
            if !cf.dirty && cf.ring == 0 {
                continue;
            }
            let mut ring = 0;
            if let Some(u) = &mut cf.unit {
                u.song_at = (self.pos / song.ppq.max(1) as f64, (self.tpf * 60.0 * self.sr as f64 / song.ppq.max(1) as f64) as f32);
                u.process(&mut cf.buf[..n], &cx);
                ring = u.tail(&cx);
                // settings the bay's cables sent to the instrument the notes play
                if let (Some(st), Some(lead)) = (u.bay_state(), song.channels[c].bay().map(|b| b.lead)) {
                    if !st.gen_sets.is_empty() {
                        self.ov.fit(song);
                        for (node, k, v) in st.gen_sets.drain(..) {
                            if node == lead {
                                self.ov.set(Target::ChanParam(c, k), v);
                            } else {
                                let at = self.layer_sets.iter().position(|e| e.0 == c && e.1 == node && e.2 == k);
                                match at {
                                    Some(i) => self.layer_sets[i].3 = v,
                                    None if self.layer_sets.len() < 4096 => self.layer_sets.push((c, node, k, v)),
                                    None => {}
                                }
                            }
                        }
                    }
                }
            }
            if let Some((set, st)) = &mut cf.tail {
                st.process(set, &mut cf.buf[..n], cx.step, at_step);
                ring = set.tail(cx.sr, cx.step);
            }
            cf.ring = if cf.dirty { ring } else { cf.ring.saturating_sub(n) };
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

        // hosted instruments: each is handed the notes due in this block and gives its sound to its track
        for (c, ch) in song.channels.iter().enumerate() {
            let Some(h) = &ch.ext else { continue };
            self.host_evs.clear();
            for (left, chan, e) in &self.host_wait {
                if *chan as usize == c && *left < n as u64 {
                    let at = *left as u32;
                    self.host_evs.push(match *e {
                        crate::host::Ev::NoteOn { key, chan, vel, .. } => crate::host::Ev::NoteOn { at, key, chan, vel },
                        crate::host::Ev::NoteOff { key, chan, .. } => crate::host::Ev::NoteOff { at, key, chan },
                        crate::host::Ev::Midi { data, .. } => crate::host::Ev::Midi { at, data },
                    });
                }
            }
            self.host_evs.sort_by_key(|e| match e {
                crate::host::Ev::NoteOn { at, .. } | crate::host::Ev::NoteOff { at, .. } | crate::host::Ev::Midi { at, .. } => *at,
            });
            // settings a clip or an event is driving
            for e in self.ov.gen.iter().filter(|e| e.0 == c) {
                h.0.set_travel(e.1 as usize, e.2);
            }
            let buf = &mut self.host_buf[..n];
            buf.fill([0.0; 2]);
            if !h.0.process(buf, &self.host_evs, &cx.clock, self.sr) || !ch.enabled {
                continue;
            }
            let g = law::channel_gain(self.ov.chan_volume(song, c));
            let (pl, pr) = law::pan(self.ov.chan_pan(song, c).clamp(-1.0, 1.0));
            let i = if (ch.insert as usize) < ni { ch.insert as usize } else { 0 };
            for (o, f) in self.buses[i][..n].iter_mut().zip(buf.iter()) {
                o[0] += f[0] * g * pl;
                o[1] += f[1] * g * pr;
            }
            self.dirty[i] = true;
        }
        let n64 = n as u64;
        self.host_wait.retain_mut(|(left, _, _)| {
            if *left < n64 {
                false
            } else {
                *left -= n64;
                true
            }
        });

        // envelope senders: move every running note on and hand its outputs to what is linked
        if !self.env_runs.is_empty() {
            let dt = n as f32 / self.sr;
            let mut fitted = false;
            for r in self.env_runs.iter_mut() {
                if let Some(node) = r.node {
                    // held in a bay: the outputs go down the bay's cables with the next block
                    for a in 0..r.set.arts.len() {
                        if let (Some(v), true) = (r.set.value(a, r.sent, r.vel, r.rand, r.t, r.rel), self.script_ctl.len() < 1024) {
                            self.script_ctl.push((r.chan, node, a as u16, v));
                        }
                    }
                }
                for l in song.ctl_links.iter().filter(|l| r.node.is_none() && l.chan() == Some(r.chan as usize)) {
                    if let Some(v) = r.set.value(l.out as usize, r.key, r.vel, r.rand, r.t, r.rel) {
                        if !fitted {
                            self.ov.fit(song);
                            fitted = true;
                        }
                        self.ov.set(l.target, v);
                    }
                }
                r.t += dt;
                if let Some(left) = r.left.as_mut() {
                    *left = left.saturating_sub(n as u64);
                    if *left == 0 && r.rel.is_none() {
                        r.rel = Some(r.t);
                    }
                }
            }
            // a note is done once it is let go and every one of its lines has run out
            self.env_runs.retain(|r| r.rel.is_none_or(|rel| r.t - rel <= (0..r.set.arts.len()).map(|a| r.set.tail(a, rel)).fold(0.0, f32::max) + dt));
        }

        let master = self.transport.master();
        self.blockno += 1;
        if self.cap.len() != ni {
            self.cap = vec![Vec::new(); ni];
            self.cap_at = vec![0; ni];
        }
        // sound card inputs join the tracks that listen to them, ahead of the effects
        let listening = song.inserts.iter().any(|i| i.audio_in >= 0);
        self.transport.input_wanted.store(listening, Relaxed);
        if listening {
            let ch = self.transport.input_ch.load(Relaxed);
            self.inbuf.clear();
            if let (true, Ok(mut q)) = (ch > 0, self.transport.input.try_lock()) {
                // hold no more than a few blocks back, so what is heard stays close to what is played
                let keep = (n + 2048) * ch;
                if q.len() > keep {
                    let over = (q.len() - keep).div_ceil(ch) * ch;
                    q.drain(..over);
                }
                let take = (n * ch).min(q.len() / ch * ch);
                self.inbuf.extend(q.drain(..take));
            }
            let got = if ch > 0 { self.inbuf.len() / ch } else { 0 };
            if got > 0 {
                let pairs = ch.div_ceil(2);
                for (i, ins) in song.inserts.iter().enumerate() {
                    if ins.audio_in < 0 || i >= ni {
                        continue;
                    }
                    let want = ins.audio_in as usize;
                    // stereo pairs come first in the list, single inputs after them
                    let (l, r) = if want < pairs { (want * 2, (want * 2 + 1).min(ch - 1)) } else { ((want - pairs).min(ch - 1), (want - pairs).min(ch - 1)) };
                    for (k, f) in self.buses[i][..got].iter_mut().enumerate() {
                        f[0] += self.inbuf[k * ch + l];
                        f[1] += self.inbuf[k * ch + r];
                    }
                    self.dirty[i] = true;
                }
            }
        }
        for &i in &song.mix_order {
            // a sender that needs no sound to work keeps its track running
            let sender = i < ni && song.inserts[i].slots.iter().any(|s| s.fx.as_ref().is_some_and(|f| matches!(f.kind, fx::Kind::Formula | fx::Kind::Xy | fx::Kind::Xyz)));
            // a track being recorded runs through silence too, or the pauses would fall out of the take
            let taken = self.transport.rec_want.load(Relaxed);
            let sender = sender || (taken != u32::MAX && (taken >> 6) as usize == i);
            if i >= ni || (!self.dirty[i] && self.ring[i] == 0 && !sender) {
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
                    if u.1.fx.kind == fx::Kind::Patch {
                        u.1.song_at = (self.pos / song.ppq.max(1) as f64, (self.tpf * 60.0 * self.sr as f64 / song.ppq.max(1) as f64) as f32);
                    }
                    if matches!(u.1.fx.kind, fx::Kind::Peak | fx::Kind::Xy | fx::Kind::Xyz | fx::Kind::Formula | fx::Kind::Surface) {
                        // a controller: run it, then hand its outputs to whatever is linked to them
                        u.1.song_at = (self.pos / song.ppq.max(1) as f64, (self.tpf * 60.0 * self.sr as f64 / song.ppq.max(1) as f64) as f32);
                        u.1.process(&mut bus[..n], &cx);
                        let slot = slots_index;
                        for l in song.ctl_links.iter().filter(|l| l.ins == i && l.slot == slot) {
                            if let Some(v) = u.1.sends(l.out as usize) {
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
                    if u.1.fx.kind == fx::Kind::Split {
                        // its gate is there for linked controls to follow
                        for l in song.ctl_links.iter().filter(|l| l.ins == i && l.slot == slots_index) {
                            if let Some(&v) = u.1.out.get(l.out as usize) {
                                self.driven.push((l.target, v));
                            }
                        }
                    }
                    if self.transport.rec_want.load(Relaxed) == (i as u32) << 6 | (slots_index as u32 & 63) {
                        // the window only ever holds this for the moment it swaps the buffer out
                        if let Ok(mut r) = self.transport.rec.lock() {
                            r.extend_from_slice(&bus[..n]);
                        }
                    }
                    if self.transport.tap_want.load(Relaxed) == (i as u32) << 6 | (slots_index as u32 & 63) {
                        // never wait for the window here: if it is reading, this block is simply not shown
                        if let Ok(mut t) = self.transport.tap.try_lock() {
                            for f in &bus[..n] {
                                let at = t.at;
                                t.buf[at] = *f;
                                t.at = (at + 1) % TAP_LEN;
                            }
                            t.count += n as u64;
                            t.rate = self.sr;
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
            self.run_arps(&song, n);
            self.run_scripts(&song, n);
            self.run_midi(&song, n);
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
    fn played_notes_are_logged_only_while_recording_runs() {
        let (h, mut p) = new_player(click_song(), 48_000.0);
        let other = h.sender();
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        assert!(h.transport.take_live().is_empty(), "nothing is logged unless armed and playing");
        h.transport.set_recording(true);
        h.transport.set_playing(true);
        let mut buf = vec![0.0; 4800 * 2];
        p.render(&mut buf, 2);
        other.send(Cmd::NoteOn { chan: 0, key: 62, vel: 0.5 });
        p.render(&mut buf, 2);
        h.send(Cmd::NoteOff { chan: 0, key: 62 });
        let got = h.transport.take_live();
        assert_eq!(got.len(), 2);
        assert!(got[0].on && !got[1].on && got[0].key == 62);
        assert!(got[0].tick > 0.0 && got[1].tick > got[0].tick);
        assert!(h.transport.take_live().is_empty());
    }

    #[test]
    fn the_pitch_wheel_bends_held_notes() {
        let mut song = Song::new();
        song.channels = vec![crate::song::Chan::instrument(crate::synth::Kind::Osc3)];
        let (h, mut p) = new_player(song, 48_000.0);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        let mut buf = vec![0.0; 24_000 * 2];
        p.render(&mut buf, 2);
        let rises = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        p.render(&mut buf, 2);
        let flat = rises(&buf);
        h.send(Cmd::Bend { chan: 0, octaves: 1.0 });
        p.render(&mut buf, 2);
        p.render(&mut buf, 2);
        let up = rises(&buf);
        assert!(flat > 50.0 && (up / flat - 2.0).abs() < 0.2, "{flat} rises, then {up}");
    }

    #[test]
    fn a_key_remapper_in_a_bay_moves_the_notes_of_the_instrument_behind_it() {
        let rises = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        let run = |up: bool| {
            let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
            ch.plugin = crate::song::BAY.to_string();
            let mut patch = crate::fxpatch::decode(&crate::fxpatch::fresh()).unwrap();
            if up {
                let mut rows: [Vec<i16>; 12] = Default::default();
                for (k, r) in rows.iter_mut().enumerate() {
                    r.push(k as i16 + 12);
                }
                let shaper = crate::noteshape::Shaper::Keys { on: true, offset: 0, mode: 0, glide: 0.0, rows };
                patch.shape.push((0, vec![crate::noteshape::Link { shaper, port: 0, node: 1 }]));
            }
            ch.bay = Some(crate::song::Bay { fx: fx::Fx { kind: fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(patch)), raw: None, ext: None }, lead: 0, layers: vec![] });
            let mut song = Song::new();
            song.channels = vec![ch];
            let (h, mut p) = new_player(song, 48_000.0);
            h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
            let mut buf = vec![0.0; 24_000 * 2];
            p.render(&mut buf, 2);
            p.render(&mut buf, 2);
            let held = rises(&buf);
            // letting go of the key that was pressed ends the voice that went out under another key
            h.send(Cmd::NoteOff { chan: 0, key: 60 });
            p.render(&mut buf, 2);
            p.render(&mut buf, 2);
            (held, buf.iter().fold(0f32, |m, v| m.max(v.abs())))
        };
        let (plain, _) = run(false);
        let (moved, after) = run(true);
        assert!(plain > 50.0 && (moved / plain - 2.0).abs() < 0.2, "{plain} rises, then {moved}");
        assert!(after < 1e-3, "still sounding at {after}");
    }

    #[test]
    fn a_script_in_a_bay_plays_its_own_notes_for_the_ones_it_is_handed() {
        if crate::pyrt::get().is_none() {
            eprintln!("no Python on this machine: skipped");
            return;
        }
        let rises = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        let peak = |b: &[f32]| b.iter().fold(0f32, |m, v| m.max(v.abs()));
        // an octave up while the key is down; `bend` ticks in, the note is bent a further octave up; the second socket stays silent
        let source = |bend: u32, socket: u32| format!("import flvfx\nt0 = {{}}\ndef onTriggerVoice(v):\n    o = flvfx.Voice(v)\n    o.note += 12\n    o.output = {socket}\n    v.o = o\n    o.trigger()\ndef onTick():\n    for o in flvfx.context.voices:\n        t0[id(o)] = t0.get(id(o), 0) + 1\n        if {bend} and t0[id(o)] == {bend}:\n            o.note += 12\ndef onReleaseVoice(v):\n    v.o.release()\n");
        let run = |src: Option<String>| {
            let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
            ch.plugin = crate::song::BAY.to_string();
            let mut patch = crate::fxpatch::decode(&crate::fxpatch::fresh()).unwrap();
            if let Some(src) = src {
                use crate::noteshape::{Link, Shaper};
                crate::script::allow(&src);
                let def = crate::script::ScriptDef { source: src, params: String::new() };
                patch.shape.push((0, vec![Link { shaper: Shaper::Script(Arc::new(def)), port: 0, node: 1 }]));
            }
            ch.bay = Some(crate::song::Bay { fx: fx::Fx { kind: fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(patch)), raw: None, ext: None }, lead: 0, layers: vec![] });
            let mut song = Song::new();
            song.channels = vec![ch];
            let (h, mut p) = new_player(song, 48_000.0);
            h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
            let mut a = vec![0.0; 12_000 * 2];
            p.render(&mut a, 2);
            let mut b = vec![0.0; 12_000 * 2];
            p.render(&mut b, 2);
            h.send(Cmd::NoteOff { chan: 0, key: 60 });
            let mut c = vec![0.0; 24_000 * 2];
            p.render(&mut c, 2);
            p.render(&mut c, 2);
            (rises(&a), rises(&b), peak(&b), peak(&c))
        };
        let plain = run(None);
        let up = run(Some(source(0, 0)));
        assert!((up.0 / plain.0 - 2.0).abs() < 0.1 && (up.1 / plain.1 - 2.0).abs() < 0.1, "{plain:?} {up:?}");
        // the script's note lasts as long as the key, and no longer
        assert!(up.2 > plain.2 * 0.5 && up.3 < 1e-4, "{plain:?} {up:?}");
        // 140 a minute at 96 to the beat is 224 ticks a second: 56 ticks is the end of the first quarter second
        let bent = run(Some(source(56, 0)));
        assert!((bent.0 / plain.0 - 2.0).abs() < 0.15 && (bent.1 / plain.1 - 4.0).abs() < 0.2, "{plain:?} {bent:?}");
        // a note sent out of a socket nothing is plugged into is not heard
        let other = run(Some(source(0, 1)));
        assert!(other.2 < 1e-6, "{other:?}");
    }

    #[test]
    fn a_script_output_moves_the_setting_it_is_cabled_to() {
        if crate::pyrt::get().is_none() {
            eprintln!("no Python on this machine: skipped");
            return;
        }
        let peak = |b: &[f32]| b.iter().fold(0f32, |m, v| m.max(v.abs()));
        let run = |to: f32| {
            use crate::fxpatch::{Cable, End, Node, SetCable};
            use crate::noteshape::{Link, Shaper};
            let src = format!("import flvfx\nflvfx.addOutputController('Out', 1.0)\ndef onTriggerVoice(v):\n    o = flvfx.Voice(v)\n    v.o = o\n    o.trigger()\n    flvfx.setOutputController('Out', {to})\ndef onReleaseVoice(v):\n    v.o.release()\n");
            crate::script::allow(&src);
            let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
            ch.plugin = crate::song::BAY.to_string();
            let node = |fx| Node { ident: String::new(), name: String::new(), fx };
            let c = |a, b| Cable { from: a, to: b, level: 1.0 };
            let patch = crate::fxpatch::Patch {
                nodes: vec![node(None), node(None), node(Some(fx::Fx::new(fx::Kind::Balance)))],
                cables: vec![c(End::Bay, End::Node(2)), c(End::Node(2), End::Bay)],
                order: vec![0, 1, 2],
                shape: vec![(0, vec![Link { shaper: Shaper::Script(Arc::new(crate::script::ScriptDef { source: src, params: String::new() })), port: 0, node: 1 }])],
                sets: vec![SetCable { from: (1, 0), to: (2, 1) }],
                ..Default::default()
            };
            ch.bay = Some(crate::song::Bay { fx: fx::Fx { kind: fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(patch)), raw: None, ext: None }, lead: 0, layers: vec![] });
            let mut song = Song::new();
            song.channels = vec![ch];
            let (h, mut p) = new_player(song, 48_000.0);
            h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
            let mut a = vec![0.0; 12_000 * 2];
            p.render(&mut a, 2);
            p.render(&mut a, 2);
            peak(&a)
        };
        let (open, shut) = (run(0.8), run(0.0));
        assert!(open > 0.01 && shut < open * 0.01, "{open} {shut}");
    }

    #[test]
    fn a_cable_reaches_the_setting_of_a_layered_instrument() {
        if crate::pyrt::get().is_none() {
            eprintln!("no Python on this machine: skipped");
            return;
        }
        let rises = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        let run = |to: f32| {
            use crate::fxpatch::{Cable, End, Node, SetCable};
            use crate::noteshape::{Link, Shaper};
            let src = format!("import flvfx\nflvfx.addOutputController('Out', 0.5)\ndef onTriggerVoice(v):\n    o = flvfx.Voice(v)\n    v.o = o\n    o.trigger()\n    flvfx.setOutputController('Out', {to})\ndef onReleaseVoice(v):\n    v.o.release()\n");
            crate::script::allow(&src);
            let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
            ch.plugin = crate::song::BAY.to_string();
            let node = |fx| Node { ident: String::new(), name: String::new(), fx };
            // only the layered instrument (unit 2) is cabled to the way out; the script's output goes to its first coarse tuning
            let patch = crate::fxpatch::Patch {
                nodes: vec![node(None), node(None), node(None)],
                cables: vec![Cable { from: End::Node(2), to: End::Bay, level: 1.0 }],
                order: vec![0, 1, 2],
                shape: vec![(0, vec![Link { shaper: Shaper::Script(Arc::new(crate::script::ScriptDef { source: src, params: String::new() })), port: 0, node: 1 }])],
                sets: vec![SetCable { from: (1, 0), to: (2, 2) }],
                feeds: vec![2],
                ..Default::default()
            };
            let layer = crate::synth::Gen::new(crate::synth::Kind::Osc3);
            ch.bay = Some(crate::song::Bay { fx: fx::Fx { kind: fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(patch)), raw: None, ext: None }, lead: 0, layers: vec![(2, layer)] });
            let mut song = Song::new();
            song.channels = vec![ch];
            let (h, mut p) = new_player(song, 48_000.0);
            let mut a = vec![0.0; 12_000 * 2];
            h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
            p.render(&mut a, 2);
            p.render(&mut a, 2);
            h.send(Cmd::NoteOff { chan: 0, key: 60 });
            for _ in 0..8 {
                p.render(&mut a, 2);
            }
            h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
            p.render(&mut a, 2);
            rises(&a)
        };
        let (same, up) = (run(0.5), run(1.0));
        assert!(same > 20.0 && (up / same - 4.0).abs() < 0.4, "{same} {up}");
    }

    #[test]
    fn an_envelope_sender_held_in_a_bay_runs_down_its_cables() {
        use crate::fxpatch::{Cable, End, Node, SetCable};
        let peak = |b: &[f32]| b.iter().fold(0f32, |m, v| m.max(v.abs()));
        let raw = crate::fm::tests::sender_block();
        let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
        ch.plugin = crate::song::BAY.to_string();
        let node = |ident: &str, fx| Node { ident: ident.to_string(), name: String::new(), fx };
        let c = |a, b| Cable { from: a, to: b, level: 1.0 };
        // the instrument sounds through a level unit, whose level the sender's third output holds
        let patch = crate::fxpatch::Patch {
            nodes: vec![node("", None), node(crate::fm::ENV_SENDER, None), node("", Some(fx::Fx::new(fx::Kind::Balance)))],
            cables: vec![c(End::Bay, End::Node(2)), c(End::Node(2), End::Bay)],
            order: vec![0, 1, 2],
            sets: vec![SetCable { from: (1, 2), to: (2, 1) }],
            feeds: vec![1],
            ..Default::default()
        };
        let sender = crate::synth::Gen::decode(crate::fm::ENV_SENDER, &raw).unwrap();
        ch.bay = Some(crate::song::Bay { fx: fx::Fx { kind: fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(patch)), raw: None, ext: None }, lead: 0, layers: vec![(1, sender)] });
        let mut song = Song::new();
        song.channels = vec![ch];
        let (h, mut p) = new_player(song, 48_000.0);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        let mut a = vec![0.0; 512];
        p.render(&mut a, 2);
        p.render(&mut a, 2);
        // its envelope starts from nothing
        let early = peak(&a);
        let mut long = vec![0.0f32; 96_000];
        p.render(&mut long, 2);
        p.render(&mut a, 2);
        let late = peak(&a);
        assert!(early < 0.02 && late > 0.05 && late > early * 5.0, "{early} {late}");
    }

    #[test]
    fn a_note_mover_in_a_bay_keeps_moving_the_voice_while_it_sounds() {
        let rises = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        let run = |part: usize| {
            let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
            ch.plugin = crate::song::BAY.to_string();
            let mut patch = crate::fxpatch::decode(&crate::fxpatch::fresh()).unwrap();
            if part < 5 {
                use crate::noteshape::{Link, Motion, MotionArt, MotionPart, Shaper};
                let line = |pts: &[(f32, f32)]| {
                    let mut c = crate::fm::Curve::flat(0.5);
                    c.pts = pts.iter().map(|&(x, y)| crate::fm::Pt { x, y, ..Default::default() }).collect();
                    c.prepare([1.0; 3]);
                    c
                };
                let mut parts: Vec<MotionPart> = (0..5).map(|i| MotionPart { base: if i == 0 { 0.0 } else { 0.5 }, amount: 1.0, wave: 1.0, env: line(&[(0.0, 0.5)]), env_on: false, lfo: line(&[(0.0, 0.5)]), lfo_on: false, lfo_hz: 5.0 }).collect();
                parts[part].env_on = true;
                // loudness: silent for a quarter second, then full; pitch: an octave up all along
                parts[part].env = if part == 0 { line(&[(0.0, 0.0), (0.25, 0.0), (0.01, 1.0)]) } else { line(&[(0.0, 1.0)]) };
                parts[1].amount = 0.25;
                let m = Motion { arts: vec![MotionArt { colour: 0, lo: 0, hi: 131, parts }] };
                patch.shape.push((0, vec![Link { shaper: Shaper::Motion(Arc::new(m)), port: 0, node: 1 }]));
            }
            ch.bay = Some(crate::song::Bay { fx: fx::Fx { kind: fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(patch)), raw: None, ext: None }, lead: 0, layers: vec![] });
            let mut song = Song::new();
            song.channels = vec![ch];
            let (h, mut p) = new_player(song, 48_000.0);
            h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
            let mut buf = vec![0.0; 9_600 * 2];
            p.render(&mut buf, 2);
            let early = buf.iter().fold(0f32, |m, v| m.max(v.abs()));
            let mut buf = vec![0.0; 24_000 * 2];
            p.render(&mut buf, 2);
            p.render(&mut buf, 2);
            (early, buf.iter().fold(0f32, |m, v| m.max(v.abs())), rises(&buf))
        };
        let plain = run(9);
        let faded = run(0);
        let up = run(1);
        assert!(plain.0 > 0.01 && faded.0 < plain.0 * 0.01, "{plain:?} {faded:?}");
        assert!((faded.1 / plain.1 - 1.0).abs() < 0.05, "{plain:?} {faded:?}");
        assert!((up.2 / plain.2 - 2.0).abs() < 0.2, "{} rises, then {}", plain.2, up.2);
    }

    #[test]
    fn a_pattern_control_event_moves_its_control() {
        let run = |quiet: bool| {
            let mut song = Song::new();
            song.channels = vec![crate::song::Chan::instrument(crate::synth::Kind::Osc3)];
            song.patterns[0].notes.push(Note { pos: 0, len: song.ppq * 4, ..Default::default() });
            if quiet {
                song.patterns[0].events.push(crate::song::PatEvent { tick: 0, param: 0, dest: 0, raw: 0 });
            }
            let (h, mut p) = new_player(song, 48_000.0);
            h.transport.set_playing(true);
            let mut buf = vec![0.0; 16_000 * 2];
            p.render(&mut buf, 2);
            p.render(&mut buf, 2);
            buf.iter().fold(0.0f32, |m, x| m.max(x.abs()))
        };
        let (loud, quiet) = (run(false), run(true));
        assert!(loud > 0.05 && quiet < loud * 0.01, "{loud} then {quiet}");
    }

    #[test]
    fn a_slide_note_bends_the_sounding_note_to_its_pitch() {
        let run = |slide: bool| {
            let mut song = Song::new();
            song.channels = vec![crate::song::Chan::instrument(crate::synth::Kind::Osc3)];
            let q = song.ppq / 4;
            song.patterns[0].notes.push(Note { pos: 0, len: song.ppq * 4, ..Default::default() });
            if slide {
                song.patterns[0].notes.push(Note { pos: q, len: q, key: 72, slide: true, ..Default::default() });
            }
            let (h, mut p) = new_player(song, 48_000.0);
            h.transport.set_playing(true);
            let mut buf = vec![0.0; 16_000 * 2];
            p.render(&mut buf, 2);
            p.render(&mut buf, 2);
            buf.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32
        };
        let (flat, up) = (run(false), run(true));
        assert!(flat > 50.0 && (up / flat - 2.0).abs() < 0.2, "{flat} rises, then {up}");
    }

    fn duo_song(set: &[(usize, f32)]) -> Song {
        let mut song = Song::new();
        let mut ch = crate::song::Chan::instrument(crate::synth::Kind::Duo);
        let g = ch.gen.as_mut().unwrap();
        for &(i, v) in set {
            g.p[i] = v;
        }
        g.raw = g.encode(None);
        song.channels = vec![ch];
        song
    }

    #[test]
    fn a_one_voice_instrument_bends_the_held_note_to_the_next() {
        let (h, mut p) = new_player(duo_song(&[(44, 1.0), (46, 0.1)]), 48_000.0);
        let rises = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        let mut buf = vec![0.0; 24_000 * 2];
        p.render(&mut buf, 2);
        p.render(&mut buf, 2);
        let low = rises(&buf);
        h.send(Cmd::NoteOn { chan: 0, key: 72, vel: 1.0 });
        p.render(&mut buf, 2);
        assert_eq!(h.transport.voices.load(Relaxed), 1);
        p.render(&mut buf, 2);
        let high = rises(&buf);
        assert!(low > 50.0 && (high / low - 2.0).abs() < 0.2, "{low} rises, then {high}");
        // letting the second key go ends the one voice
        h.send(Cmd::NoteOff { chan: 0, key: 72 });
        p.render(&mut buf, 2);
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|x| x.abs() < 1e-3));
    }

    #[test]
    fn an_instruments_own_gate_and_echo_run_after_its_voices() {
        let run = |set: &[(usize, f32)], len: u32| {
            let mut song = duo_song(set);
            let q = song.ppq / 4;
            song.patterns[0].notes.push(Note { pos: 0, len: q * len, ..Default::default() });
            let (h, mut p) = new_player(song, 48_000.0);
            h.transport.set_playing(true);
            // 130 beats a minute: a sixteenth is 5538 frames
            let mut buf = vec![0.0; 5538 * 8 * 2];
            p.render(&mut buf, 2);
            let level = |k: usize| (buf[k * 5538 * 2 + 2000..(k + 1) * 5538 * 2 - 2000].iter().map(|x| x * x).sum::<f32>() / 7076.0).sqrt();
            (0..8).map(level).collect::<Vec<f32>>()
        };
        // the default pattern lets steps 1, 3, 5, 6, 7 through
        let open = run(&[], 8);
        let gated = run(&[(47, 1.0)], 8);
        assert!(open[1] > 0.01 && gated[0] > open[0] * 0.8 && gated[1] < open[1] * 0.02 && gated[4] > open[4] * 0.8, "{open:?} {gated:?}");
        // a short note comes back four sixteenths later
        let dry = run(&[], 1);
        let wet = run(&[(31, 1.0), (34, 0.5), (38, 1.0)], 1);
        assert!(dry[4] < 1e-4 && wet[4] > dry[0] * 0.3 && wet[2] < wet[4] * 0.2, "{dry:?} {wet:?}");
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

    fn run(s: Song, frames: usize) -> Vec<f32> {
        let (h, mut p) = new_player(s, 48_000.0);
        h.transport.set_playing(true);
        let mut buf = vec![0.0f32; frames * 2];
        for block in buf.chunks_mut(2 * 333) {
            p.render(block, 2);
        }
        buf
    }

    #[test]
    fn a_sender_channel_turns_notes_and_knobs_into_messages() {
        use crate::midiout::{Ctl, Msg, Set};
        let mut s = click_song();
        let mut set = Set::fresh();
        set.chan = 3;
        set.port = 9;
        set.patch = 17;
        set.send_mix = false;
        *set.ctl_mut(0) = Ctl { slot: 0, num: 74, on: true, kind: 0, lo: 0, hi: 127, chan: -1, name: String::new(), short: String::new() };
        set.values[0] = 40;
        s.channels[0].kind = ChannelKind::Plugin;
        s.channels[0].plugin = crate::midiout::IDENT.into();
        s.channels[0].gen = crate::synth::Gen::decode(crate::midiout::IDENT, &set.write());
        assert!(s.channels[0].gen.is_some());
        s.patterns[0].notes.push(Note { pos: 0, len: 24, chan: 0, key: 64, ..Default::default() });
        let (mut h, mut p) = new_player(s.clone(), 48_000.0);
        h.transport.set_playing(true);
        let mut buf = vec![0.0f32; 2 * 333];
        let mut got: Vec<Msg> = vec![];
        let mut when = vec![];
        for i in 0..40 {
            p.render(&mut buf, 2);
            let had = got.len();
            h.transport.take_midi(&mut got);
            when.extend((had..got.len()).map(|_| i));
            assert!(buf.iter().all(|x| *x == 0.0));
        }
        let bytes: Vec<Vec<u8>> = got.iter().map(|m| m.bytes().to_vec()).collect();
        assert_eq!(bytes, vec![vec![0xc3, 17], vec![0xb3, 74, 40], vec![0x93, 64, 100], vec![0x83, 64, 0]], "{when:?}");
        assert!(got.iter().all(|m| m.port == 9));
        // the note is one step long: 6000 frames, so its end leaves in the nineteenth block
        assert_eq!(when, vec![0, 0, 0, 18]);
        // a moved knob is sent alone; a held key ends when it is let go
        set.values[0] = 41;
        s.channels[0].gen = crate::synth::Gen::decode(crate::midiout::IDENT, &set.write());
        h.transport.set_playing(false);
        p.render(&mut buf, 2);
        h.publish(s);
        h.send(Cmd::NoteOn { chan: 0, key: 50, vel: 0.5 });
        p.render(&mut buf, 2);
        h.send(Cmd::NoteOff { chan: 0, key: 50 });
        p.render(&mut buf, 2);
        got.clear();
        h.transport.take_midi(&mut got);
        let bytes: Vec<Vec<u8>> = got.iter().map(|m| m.bytes().to_vec()).collect();
        assert_eq!(bytes, vec![vec![0xb3, 74, 41], vec![0x93, 50, 50], vec![0x83, 50, 0]]);
    }

    #[test]
    fn the_arpeggiator_spells_held_notes_in_time() {
        // one note four steps long; a step at 120 bpm is 6000 frames
        let mut s = click_song();
        s.patterns[0].notes.push(Note { pos: 0, len: 96, chan: 0, key: 60, ..Default::default() });
        s.channels[0].smp.arp_dir = 1;
        s.channels[0].smp.arp_chord = 1;
        let got = onsets(&run(s.clone(), 40_000), 200);
        assert_eq!(got.len(), 4, "{got:?}");
        for (i, g) in got.iter().enumerate() {
            assert!(g.abs_diff(i * 6000) <= 2, "{got:?}");
        }
        // half the time: twice the notes; a quarter step late: all of them moved
        s.channels[0].smp.arp_steps = 0.5;
        s.channels[0].smp.shift = 0.25;
        let got = onsets(&run(s.clone(), 40_000), 200);
        assert_eq!(got.len(), 8, "{got:?}");
        assert!(got[0].abs_diff(1500) <= 2 && got[1].abs_diff(4500) <= 2, "{got:?}");
        // the gate cuts the held note to one step, leaving two notes
        s.channels[0].smp.shift = 0.0;
        s.channels[0].smp.gate_steps = 1.0;
        assert_eq!(onsets(&run(s.clone(), 40_000), 200).len(), 2);
        // two notes held together are walked up and back down; a lone one just sounds under the holding choice
        let mut s = click_song();
        s.patterns[0].notes.push(Note { pos: 0, len: 96, chan: 0, key: 60, ..Default::default() });
        s.channels[0].smp.arp_dir = 3;
        s.channels[0].smp.arp_chord = -2;
        assert_eq!(onsets(&run(s.clone(), 40_000), 200).len(), 1);
        s.patterns[0].notes.push(Note { pos: 0, len: 96, chan: 0, key: 72, ..Default::default() });
        assert_eq!(onsets(&run(s, 40_000), 200).len(), 4);
    }

    #[test]
    fn tracking_pans_by_key_and_echoes_turn_round_at_the_sides() {
        let mut s = click_song();
        s.channels[0].smp.track_key = [60.0, 1.0, 0.0, 0.0];
        s.patterns[0].notes.push(Note { pos: 0, len: 0, chan: 0, key: 124, ..Default::default() });
        let buf = run(s.clone(), 2000);
        let side = |b: &[f32], c: usize| b.chunks(2).map(|f| f[c].abs()).fold(0.0f32, f32::max);
        assert!(side(&buf, 0) < 1e-3 && side(&buf, 1) > 0.5);
        // three echoes a step apart, each 0.8 further right: right, back toward the middle, then left
        let mut s = click_song();
        s.patterns[0].notes.push(Note { pos: 0, len: 0, chan: 0, key: 60, ..Default::default() });
        let f = &mut s.channels[0].smp;
        (f.echo_feedback, f.echoes, f.echo_steps, f.echo_pan, f.echo_ping) = (1.0, 3, 1.0, 0.8, true);
        let buf = run(s, 24_000);
        let lean: Vec<f32> = (0..4).map(|k| {
            let b = &buf[k * 12_000..k * 12_000 + 400];
            side(b, 1) - side(b, 0)
        }).collect();
        assert!(lean[0].abs() < 1e-3 && lean[1] > 0.5 && lean[2] > 0.0 && lean[2] < lean[1] && lean[3] < -0.2, "{lean:?}");
    }

    #[test]
    fn a_held_key_arpeggiates_until_it_is_let_go() {
        let mut s = click_song();
        s.channels[0].smp.arp_dir = 5;
        s.channels[0].smp.arp_range = 2;
        let (h, mut p) = new_player(s, 48_000.0);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        let mut buf = vec![0.0f32; 2 * 20_000];
        p.render(&mut buf, 2);
        assert_eq!(onsets(&buf, 200).len(), 4);
        h.send(Cmd::NoteOff { chan: 0, key: 60 });
        p.render(&mut buf, 2);
        assert!(onsets(&buf[2 * 400..], 200).is_empty());
    }

    #[test]
    fn song_mode_plays_clips_and_respects_mutes() {
        let mut s = click_song();
        s.patterns[0].toggle_step(0, 0);
        s.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0 });
        s.clips.push(Clip { pos: 384, len: 384, track: 1, src: ClipSrc::Pattern(0), offset: 0, muted: true, group: 0 });
        s.clips.push(Clip { pos: 768, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0 });
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
        s.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0 });
        s.clips.push(Clip { pos: 0, len: 384, track: 1, src: ClipSrc::Automation(ac), offset: 0, muted: false, group: 0 });
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
    fn a_formula_sender_drives_a_linked_control() {
        // on a silent track of its own, the formula still runs and holds the master level down
        let mut s = click_song();
        let mut slot = crate::song::Slot::with_fx(0, crate::fx::Kind::Formula);
        slot.fx.as_mut().unwrap().set_formula("a*0");
        s.inserts[5].slots.push(slot);
        s.ctl_links.push(crate::song::CtlLink { ins: 5, slot: 0, out: 0, target: Target::InsertVolume(0) });
        let (h, mut p) = new_player(s.clone(), 48_000.0);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        let mut buf = vec![0.0f32; 8192];
        p.render(&mut buf, 2);
        assert!(buf[4096..].iter().all(|x| x.abs() < 1e-6));
        // and lets it through when the formula says so
        s.inserts[5].slots[0].fx.as_mut().unwrap().set_formula("0.5+a");
        let (h, mut p) = new_player(s, 48_000.0);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        assert!(buf.iter().any(|x| x.abs() > 0.01));
    }

    #[test]
    fn a_key_sender_moves_a_linked_control() {
        let mut s = click_song();
        s.channels.push(crate::song::Chan { name: "Keys".into(), kind: ChannelKind::Plugin, plugin: crate::synth::KEY_SENDER.into(), ..Default::default() });
        let k = s.channels.len() - 1;
        s.ctl_links.push(crate::song::CtlLink::from_chan(k, 0, Target::InsertVolume(0)));
        let (h, mut p) = new_player(s.clone(), 48_000.0);
        let mut buf = vec![0.0f32; 4096];
        // the bottom key sends nothing: the master fader goes down
        h.send(Cmd::NoteOn { chan: k as u16, key: 0, vel: 1.0 });
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|x| x.abs() < 1e-6));
        // a high key opens it again
        h.send(Cmd::NoteOn { chan: k as u16, key: 110, vel: 1.0 });
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        assert!(buf.iter().any(|x| x.abs() > 0.01));
        // a stored table decides what each key sends
        let mut raw = vec![];
        crate::synth::key_sender_set(&mut raw, 110, 0.0);
        assert_eq!(crate::synth::key_sender_value(&raw, 110), 0.0);
        assert!((crate::synth::key_sender_value(&raw, 60) - 60.0 / 119.0).abs() < 1e-6);
        assert_eq!(crate::synth::key_sender_range(&raw), (0, 119));
    }

    #[test]
    fn an_envelope_sender_runs_into_a_linked_control() {
        let mut s = click_song();
        let raw = crate::fm::tests::sender_block();
        s.channels.push(crate::song::Chan { name: "Env".into(), kind: ChannelKind::Plugin, plugin: crate::fm::ENV_SENDER.into(), gen: crate::synth::Gen::decode(crate::fm::ENV_SENDER, &raw), ..Default::default() });
        let k = s.channels.len() - 1;
        assert!(s.channels[k].gen.is_some());
        s.ctl_links.push(crate::song::CtlLink::from_chan(k, 2, Target::InsertVolume(0)));
        let (h, mut p) = new_player(s, 48_000.0);
        let mut buf = vec![0.0f32; 512];
        // its envelope starts from nothing, so the master fader is down at first
        h.send(Cmd::NoteOn { chan: k as u16, key: 60, vel: 1.0 });
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        let early = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        // and has opened it a second later
        let mut long = vec![0.0f32; 96_000];
        p.render(&mut long, 2);
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        let late = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(early < 0.02, "{early}");
        assert!(late > 0.1 && late > early * 5.0, "{late}");
    }

    #[test]
    fn a_sound_card_input_is_heard_on_its_track() {
        let mut s = click_song();
        let (mut h, mut p) = new_player(s.clone(), 48_000.0);
        let mut buf = vec![0.0f32; 512];
        // nobody listens: what arrives is not heard
        h.transport.feed_input(&vec![0.5; 3 * 256], 3);
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|v| v.abs() < 1e-6) && !h.transport.input_wanted());
        // three inputs make two pairs and three singles; entry 3 is the second single, on both sides
        s.inserts[2].audio_in = 3;
        h.publish(s);
        p.render(&mut buf, 2);
        assert!(h.transport.input_wanted());
        let frames: Vec<f32> = (0..256).flat_map(|_| [0.1, 0.25, 0.4]).collect();
        h.transport.feed_input(&frames, 3);
        p.render(&mut buf, 2);
        assert!((buf[0] - 0.25).abs() < 0.02 && (buf[1] - 0.25).abs() < 0.02, "{} {}", buf[0], buf[1]);
        // and it stops when the input runs dry
        p.render(&mut buf, 2);
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|v| v.abs() < 1e-3));
    }

    #[test]
    fn a_watched_slot_is_shown_and_recorded() {
        let mut s = click_song();
        let mut slot = crate::song::Slot::with_fx(2, crate::fx::Kind::Thru);
        slot.plugin = "Edison".into();
        s.inserts[0].slots.push(slot);
        let (h, mut p) = new_player(s, 48_000.0);
        let mut buf = vec![0.0f32; 2048];
        // nothing is gathered until someone asks
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        assert!(h.transport.take_recorded().is_empty());
        assert_eq!(h.transport.watched(64).1, 0);
        h.transport.record_slot(Some((0, 2)));
        h.transport.watch_slot(Some((0, 2)));
        h.send(Cmd::NoteOn { chan: 0, key: 60, vel: 1.0 });
        p.render(&mut buf, 2);
        p.render(&mut buf, 2);
        let got = h.transport.take_recorded();
        assert_eq!(got.len(), 2048);
        assert!(got.iter().any(|f| f[0].abs() > 0.1));
        let (seen, count, rate) = h.transport.watched(512);
        assert_eq!((seen.len(), count, rate), (512, 2048, 48_000.0));
        // the pause after the click is part of the take
        assert!(got[1800..].iter().all(|f| f[0].abs() < 1e-3));
        // taking empties it, and stopping ends the gathering
        assert!(h.transport.take_recorded().is_empty());
        h.transport.record_slot(None);
        p.render(&mut buf, 2);
        assert!(h.transport.take_recorded().is_empty());
        assert_eq!(h.transport.recording_slot(), None);
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
        s.channels[0].zones = Some(Arc::new(crate::zones::Map { zones: vec![pad(36, 0, 110, -1), pad(46, 0, 127, 0), pad(42, 0, 127, 0)], bank: None, kit: None }));
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
    fn the_key_region_and_an_instruments_root_key_are_kept() {
        let mut s = tone_song(1.0);
        s.patterns[0].toggle_step(0, 0);
        // a sampler outside its key region stays silent
        s.channels[0].smp.key_lo = 61;
        assert!(loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.0, 0.5) < 1e-6);
        s.channels[0].smp.key_lo = 0;
        assert!(loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.0, 0.5) > 0.1);
        // an instrument's root key moves the key it is sent: the pad on key 48 answers key 60 once the root is an octave up
        let tone = s.channels[0].sample.take().unwrap();
        s.channels[0].kind = ChannelKind::Plugin;
        let pad = crate::zones::Zone { key_lo: 48, key_hi: 48, root: 48, track: false, sample: Some(tone), one_shot: true, ..Default::default() };
        s.channels[0].zones = Some(Arc::new(crate::zones::Map { zones: vec![pad], bank: None, kit: None }));
        assert!(loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.0, 0.5) < 1e-6);
        s.channels[0].root_key = 72;
        assert!(loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.0, 0.5) > 0.1);
    }

    #[test]
    fn a_layers_child_runs_its_own_arpeggiator() {
        let mut s = tone_song(0.05);
        s.channels.push(crate::song::Chan { kind: ChannelKind::Layer, children: vec![0], ..Default::default() });
        let layer = s.channels.len() as u16 - 1;
        s.patterns[0].notes.push(Note { len: s.ppq * 2, ..Note::step(0, layer) });
        let plain = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&plain, 0.0, 0.04) > 0.1 && loud(&plain, 0.2, 0.9) < 1e-6, "one short hit {} {}", loud(&plain, 0.0, 0.04), loud(&plain, 0.2, 0.9));
        s.channels[0].smp.arp_dir = 1;
        s.channels[0].smp.arp_steps = 1.0;
        let arp = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&arp, 0.2, 0.9) > 0.1, "the child repeats the note while the layer's note is held");
    }

    #[test]
    fn a_hosted_plugin_plays_a_channel_and_treats_a_track() {
        let Some(path) = crate::host::test_plugin() else {
            eprintln!("test plug-in not built; skipped");
            return;
        };
        // as an instrument: the pattern's note is heard for as long as it lasts
        let mut s = tone_song(2.0);
        s.channels[0].set_sample(None);
        s.channels[0].kind = ChannelKind::Plugin;
        s.channels[0].plugin = crate::song::WRAPPER.to_string();
        s.channels[0].ext = Some(crate::host::Hosted(crate::host::Instance::open(&path, "").expect("opens")));
        let quarter = s.ppq as u32;
        s.patterns[0].toggle_step(0, quarter);
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.0, 0.45) < 1e-4, "silent before the note");
        assert!(loud(&buf, 0.52, 0.6) > 0.05, "the note sounds");
        assert!(loud(&buf, 1.0, 1.5) < 1e-3, "and ends");
        // turned down on the channel, it is quieter
        let full = loud(&buf, 0.52, 0.6);
        s.channels[0].volume *= 0.25;
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.52, 0.6) < full * 0.5);

        // as an effect: its gain setting scales what the track carries
        let mut s = tone_song(2.0);
        s.patterns[0].toggle_step(0, 0);
        let plain = loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.1, 0.4);
        let inst = crate::host::Instance::open(&path, "").expect("opens");
        let gain = inst.params()[0].id;
        s.inserts[0].slots.push(crate::song::Slot::hosted(0, inst.clone()));
        inst.set(gain, 2.0);
        let twice = loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.1, 0.4);
        assert!((twice / plain - 2.0).abs() < 0.1, "{twice} against {plain}");
        inst.set(gain, 0.0);
        assert!(loud(&crate::render_song(&s, 48_000.0, false, 0, 0.0), 0.1, 0.4) < 1e-4);

        // a clip drives that setting by its place: held at the top it doubles, at the bottom it silences
        inst.set(gain, 1.0);
        let len = s.ppq as u32 * 4;
        for (value, want) in [(1.0f32, 2.0f32), (0.0, 0.0)] {
            let mut s = s.clone();
            let pts = vec![crate::song::AutoPt { tick: 0.0, value, tension: 0.0, mode: 0 }, crate::song::AutoPt { tick: len as f64, value, tension: 0.0, mode: 0 }];
            s.channels.push(Chan { kind: ChannelKind::AutomationClip, auto: Some(Arc::new(crate::song::Automation { points: pts, targets: vec![Target::SlotParam(0, 0, 0)] })), ..Default::default() });
            let ac = s.channels.len() - 1;
            s.clips.push(Clip { pos: 0, len, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0 });
            s.clips.push(Clip { pos: 0, len, track: 1, src: ClipSrc::Automation(ac), offset: 0, muted: false, group: 0 });
            let got = loud(&crate::render_song(&s, 48_000.0, true, 0, 0.0), 0.1, 0.4);
            assert!((got / plain - want).abs() < 0.1, "{got} against {plain}, wanted {want} times");
            assert_eq!(s.target_name(Target::SlotParam(0, 0, 0)).rsplit(" - ").next(), Some("Gain"));
        }
    }

    #[test]
    fn a_second_player_carries_on_with_the_same_song() {
        let mut s = tone_song(2.0);
        s.patterns[0].toggle_step(0, 0);
        let (mut handle, first) = new_player(Song::new(), 44_100.0);
        handle.publish(s);
        // the sound card is reopened at another rate: the old player goes, a new one takes over
        drop(first);
        let mut p = handle.player(96_000.0);
        assert_eq!(p.sample_rate(), 96_000.0);
        handle.transport.set_playing(true);
        let mut out = vec![0.0f32; 96_000];
        p.render(&mut out, 2);
        assert!(out.iter().any(|v| v.abs() > 0.01), "the song published before the change is what plays");
    }

    #[test]
    fn layers_that_share_a_note_take_turns_when_the_pad_asks() {
        let mut s = tone_song(2.0);
        let tone = s.channels[0].sample.take().unwrap();
        s.channels[0].kind = ChannelKind::Plugin;
        let layer = |gain: f32, pick: u8| crate::zones::Zone { key_lo: 36, key_hi: 36, root: 36, track: false, sample: Some(tone.clone()), one_shot: true, gain, pick, ..Default::default() };
        s.patterns[0].toggle_step(0, 0);
        let second = s.ppq as u32 * 2;
        s.patterns[0].toggle_step(0, second);
        for n in &mut s.patterns[0].notes {
            n.key = 36;
        }
        // together: the loud layer is heard from the first note
        s.channels[0].zones = Some(Arc::new(crate::zones::Map { zones: vec![layer(0.0, 0), layer(1.0, 0)], bank: None, kit: None }));
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.0, 0.3) > 0.1);
        // in turn: the first note plays the silent layer, the second the loud one
        s.channels[0].zones = Some(Arc::new(crate::zones::Map { zones: vec![layer(0.0, 1), layer(1.0, 1)], bank: None, kit: None }));
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        assert!(loud(&buf, 0.0, 0.9) < 1e-6);
        assert!(loud(&buf, 1.1, 1.8) > 0.1);
    }

    #[test]
    fn a_count_in_bar_clicks_before_the_song_moves() {
        let s = tone_song(2.0);
        let mut s2 = s.clone();
        s2.patterns[0].toggle_step(0, 0);
        let (handle, mut player) = crate::new_player(s2.clone(), 48_000.0);
        let tr = handle.transport.clone();
        tr.set_recording(true);
        tr.set_countdown(true);
        tr.set_playing(true);
        // 140 beats a minute by default or not, a bar is four beats: run half a bar and the song has not moved
        let bar_secs = 4.0 * 60.0 / s2.tempo as f64;
        let mut buf = vec![0.0f32; ((bar_secs * 0.5 * 48_000.0) as usize) * 2];
        player.render(&mut buf, 2);
        assert_eq!(tr.tick(), 0.0);
        assert!(buf.iter().any(|x| x.abs() > 0.01), "the clicks are heard");
        let mut buf = vec![0.0f32; ((bar_secs * 0.75 * 48_000.0) as usize) * 2];
        player.render(&mut buf, 2);
        assert!(tr.tick() > 0.0, "the song runs once the bar is counted");
    }

    #[test]
    fn a_zone_follows_its_own_envelope() {
        let mut s = tone_song(2.0);
        let tone = s.channels[0].sample.take().unwrap();
        s.channels[0].kind = ChannelKind::Plugin;
        let env = crate::song::Env { on: true, decay: 0.3, sustain: 0.0, release: 0.01, ..Default::default() };
        let zone = crate::zones::Zone { root: 60, sample: Some(tone), one_shot: true, env: Some(env), ..Default::default() };
        s.channels[0].zones = Some(Arc::new(crate::zones::Map { zones: vec![zone], bank: None, kit: None }));
        s.patterns[0].toggle_step(0, 0);
        let buf = crate::render_song(&s, 48_000.0, false, 0, 0.0);
        // loud at the start, gone once the decay has run to its silent sustain
        assert!(loud(&buf, 0.0, 0.1) > 0.1);
        assert!(loud(&buf, 0.5, 1.5) < 1e-4);
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
        s.clips.push(Clip { pos: 0, len: 96 * 8, track: 0, src: ClipSrc::Audio(0), offset: 0, muted: false, group: 0 });
        let buf = crate::render_song(&s, 48_000.0, true, 0, 0.0);
        assert!(loud(&buf, 1.5, 1.9) > 0.1, "still sounding in its second second");
        assert!(loud(&buf, 2.1, 3.0) < 1e-3);
    }
}
