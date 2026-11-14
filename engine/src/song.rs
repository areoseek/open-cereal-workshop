//! The project as the engine plays it: channels, patterns of notes, the playlist and the mixer.
//!
//! A `Song` is a plain value. The UI edits its own copy and publishes a snapshot to the audio thread, which only
//! ever reads.

use std::path::Path;
use std::sync::Arc;

use crate::flp::{self, ChannelKind, ClipSource, Flp};
use crate::sample::{Library, Sample};

pub type Color = [u8; 3];

#[derive(Debug, Clone)]
pub struct Chan {
    pub name: String,
    pub kind: ChannelKind,
    /// Generator name for plugin channels (empty for the sampler).
    pub plugin: String,
    pub sample_path: String,
    pub sample: Option<Arc<Sample>>,
    pub color: Color,
    pub enabled: bool,
    /// 0..=1 of the knob (file value / 12800).
    pub volume: f32,
    /// -1..=1.
    pub pan: f32,
    pub pitch_cents: f32,
    pub root_key: u8,
    /// Cents the root note is tuned by.
    pub root_fine: f32,
    /// Mixer insert, 0 = master.
    pub insert: usize,
    /// When the volume envelope is on, notes stop at their end; otherwise one-shots play out.
    pub env_on: bool,
    /// The rack group it is filed under (an index into the song's groups; anything past the end means the default one).
    pub group: usize,
    /// Drawn as a thin strip in the rack.
    pub zipped: bool,
    /// Which channel of the file this came from (None for one added since the last load or save).
    pub src: Option<u16>,
    /// The curve and what it drives, for an automation clip channel.
    pub auto: Option<Arc<Automation>>,
    /// Channels a layer channel plays.
    pub children: Vec<u16>,
    /// Sampler settings.
    pub smp: Sampler,
    /// The sample as loaded, when `sample` is a treated copy (reversed, faded, ...).
    pub raw: Option<Arc<Sample>>,
    /// Settings of a built-in instrument, when the channel's generator is one we can run.
    pub gen: Option<crate::synth::Gen>,
    /// When the channel's generator is a patch bay: the bay its instrument plays through, and which of the
    /// bay's held units that instrument is. Read through [`Channel::bay`].
    pub bay: Option<Bay>,
    /// The cuts of a loop slicer channel; its loop is in `sample`.
    pub slices: Option<Arc<crate::slicer::Slices>>,
    /// The sample map of a pad machine or multi-sample player.
    pub zones: Option<Arc<crate::zones::Map>>,
    /// The live plug-in of a channel that plays one from another maker.
    pub ext: Option<crate::host::Hosted>,
}

/// One envelope of a sampler channel. Times are seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Env {
    pub on: bool,
    pub delay: f32,
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    /// 0..=1.
    pub sustain: f32,
    pub release: f32,
    /// Depth: cents for pitch, -1..=1 of the knob for the filter.
    pub amount: f32,
    /// Bend of the attack, decay and release lines, -1..=1.
    pub tension: [f32; 3],
    /// Times follow the tempo: they are as set at 120 and stretch or shrink with it.
    pub tempo: bool,
}

impl Default for Env {
    fn default() -> Self {
        Self { on: false, delay: 0.0, attack: 0.0, hold: 0.0, decay: 0.0, sustain: 1.0, release: 0.0, amount: 0.0, tension: [0.0; 3], tempo: false }
    }
}

/// The wobble that goes with one envelope of a channel: pan, volume, the two filter knobs or pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lfo {
    /// Seconds before it starts, and seconds it takes to reach its full depth.
    pub delay: f32,
    pub attack: f32,
    /// Depth: pan offset (1 = the whole way), share of the level, share of the knob for the filter, cents for pitch.
    pub amount: f32,
    /// The speed knob, 0 (fastest) to 1.
    pub speed: f32,
    /// 0 sine, 1 triangle, 2 pulse.
    pub shape: u8,
    /// Times and speed follow the tempo.
    pub tempo: bool,
    /// The wave runs from the start of the song instead of from each note.
    pub global: bool,
}

impl Sampler {
    /// The filter as it sits by default passes everything untouched; any other setting colours the sound.
    pub fn filter_idle(&self) -> bool {
        self.filter == 0 && self.cutoff + self.adjust_x >= 0.999 && self.resonance + self.adjust_y <= 0.0 && !self.cut_env.on && !self.res_env.on && self.lfos[2].amount == 0.0 && self.lfos[3].amount == 0.0
    }
}

impl Default for Lfo {
    fn default() -> Self {
        Self { delay: 0.0, attack: crate::law::env_seconds(20000), amount: 0.0, speed: 32950.0 / 65536.0, shape: 0, tempo: false, global: false }
    }
}

/// What the file's wobble amount counts in, per target.
pub const LFO_SCALE: [f32; 5] = [64.0, 128.0, 128.0, 128.0, 1.0];

impl Lfo {
    fn from_flp(e: &flp::Envelope, scale: f32) -> Self {
        Self {
            delay: crate::law::env_seconds(e.lfo[0]),
            attack: crate::law::env_seconds(e.lfo[1]),
            amount: e.lfo[2] as f32 / scale,
            speed: (e.lfo[3] as f32 / 65536.0).clamp(0.0, 1.0),
            shape: e.lfo[4].clamp(0, 2) as u8,
            tempo: e.flags & 2 != 0,
            global: e.flags & 32 != 0,
        }
    }

    /// Seconds one turn takes.
    pub fn period(&self) -> f32 {
        crate::law::knob_seconds(self.speed).max(1e-4)
    }

    /// The wave at a phase in turns: -1..=1, starting at 0 on the way up.
    pub fn wave(&self, turns: f32) -> f32 {
        let ph = turns - turns.floor();
        match self.shape {
            1 => {
                if ph < 0.25 {
                    ph * 4.0
                } else if ph < 0.75 {
                    2.0 - ph * 4.0
                } else {
                    ph * 4.0 - 4.0
                }
            }
            2 => {
                if ph < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            _ => (ph * std::f32::consts::TAU).sin(),
        }
    }
}

impl Env {
    fn from_flp(e: &flp::Envelope, amount_scale: f32) -> Self {
        let t = |v: i32| (v as f32 / 128.0).clamp(-1.0, 1.0);
        Self {
            on: e.enabled,
            delay: crate::law::env_seconds(e.predelay),
            attack: crate::law::env_seconds(e.attack),
            hold: crate::law::env_seconds(e.hold),
            decay: crate::law::env_seconds(e.decay),
            sustain: (e.sustain as f32 / 128.0).clamp(0.0, 1.0),
            release: crate::law::env_seconds(e.release),
            amount: e.amount as f32 * amount_scale,
            tension: [t(e.tension[0]), t(e.tension[1]), t(e.tension[2])],
            tempo: e.flags & 1 != 0,
        }
    }

    /// Level `t` seconds after the note began, while it is held.
    pub fn held(&self, t: f32) -> f32 {
        let mut t = t - self.delay;
        if t < 0.0 {
            return 0.0;
        }
        if t < self.attack {
            return env_bend(t / self.attack, self.tension[0]);
        }
        t -= self.attack;
        if t < self.hold {
            return 1.0;
        }
        t -= self.hold;
        if t < self.decay {
            return self.sustain + (1.0 - self.sustain) * env_bend(1.0 - t / self.decay, -self.tension[1]);
        }
        self.sustain
    }

    /// Level `t` seconds after the note was let go at level `from`; None once it has died away.
    pub fn released(&self, from: f32, t: f32) -> Option<f32> {
        if t >= self.release {
            return None;
        }
        Some(from * env_bend(1.0 - t / self.release.max(1e-6), -self.tension[2]))
    }
}

/// What the sampler does to a note beyond level, pan and pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sampler {
    pub vol_env: Env,
    pub cut_env: Env,
    pub pitch_env: Env,
    /// The envelope on the second filter knob (resonance).
    pub res_env: Env,
    /// The wobbles: pan, volume, the two filter knobs, pitch.
    pub lfos: [Lfo; 5],
    /// Filter cutoff and resonance, 0..=1 of the knobs, and the filter type (0 and 1 low pass, 2 band pass,
    /// 3 high pass, 4 band stop, 5 to 7 steeper low passes).
    pub cutoff: f32,
    pub resonance: f32,
    pub filter: u8,
    /// Fades as a share of the sample, 0..=1.
    pub fade_in: f32,
    pub fade_out: f32,
    pub reverse: bool,
    pub swap_stereo: bool,
    pub normalize: bool,
    pub invert: bool,
    /// Play the loop stored in the sample for as long as the note is held.
    pub use_loop: bool,
    /// The loop goes back and forth instead of jumping to its start.
    pub ping_pong: bool,
    /// A note here silences sounding notes of channels whose `cut_by` matches. 0 = none.
    pub cut: u16,
    pub cut_by: u16,
    pub mono: bool,
    pub porta: bool,
    /// Glide time in seconds.
    pub slide: f32,
    /// Share of the project swing this channel takes, 0..=1.
    pub swing: f32,
    /// Echoes: how many, level kept from one to the next (0 = no echoes), pan and pitch moved per echo, gap in steps.
    pub echoes: u8,
    pub echo_feedback: f32,
    pub echo_pan: f32,
    pub echo_cents: f32,
    /// What each echo adds to the note's two free controls, -1..1.
    pub echo_mod_x: f32,
    pub echo_mod_y: f32,
    /// Treatments older projects carry, kept as stored: one side late (2048 = neither, above it
    /// the left by that many frames), a pitch sweep along the sample (256 = none), ring
    /// modulation (amount low, rate high), reverb (kind in the high half, length in the low),
    /// boost, tone (128 = flat), cut (1024 = open) and the cut's peak.
    pub old_fx: [u32; 8],
    pub echo_steps: f32,
    /// Beats the sample is fitted to (0 = its own length), extra pitch in cents, length multiplier, and whether
    /// pitch follows speed (plain resampling) or is kept.
    pub stretch_beats: f32,
    pub stretch_cents: f32,
    pub stretch_mul: f32,
    pub keep_pitch: bool,
    /// Extra gain from the level adjustment.
    pub gain: f32,
    /// Arpeggiator: direction (0 off, 1 up, 2 down, 3 up and down, 4 up and down with the ends twice,
    /// 5 random), octaves covered, chord (-1 the held notes, -2 the same but a lone note just holds, 0 the
    /// note alone, then [`ARP_CHORDS`]), steps between notes, share of that time each note lasts, glide
    /// between notes, and how often each octave is played before the next.
    pub arp_dir: u8,
    pub arp_range: u8,
    pub arp_chord: i16,
    pub arp_steps: f32,
    pub arp_gate: f32,
    pub arp_slide: bool,
    pub arp_repeat: u8,
    /// Longest a note may last, in steps; [`GATE_OFF`] and up = no limit.
    pub gate_steps: f32,
    /// Notes start this many steps late, 0..=1.
    pub shift: f32,
    /// Portamento notes ignore the gate.
    pub full_porta: bool,
    /// The root key's shift is added to the note's key instead of to its pitch.
    pub add_to_key: bool,
    /// The project's main pitch moves this channel.
    pub main_pitch: bool,
    /// Most notes that may sound at once (0 = no limit).
    pub max_poly: u32,
    /// Standing offsets from the levels box: pan (-1..=1) and the two filter knobs (-1..=1 of their travel).
    pub adjust_pan: f32,
    pub adjust_x: f32,
    pub adjust_y: f32,
    /// Lowest and highest key the channel answers to (0 and 256 = every key).
    pub key_lo: u32,
    pub key_hi: u32,
    /// Echo pan turns round at the sides; fat mode swaps the echoes from side to side.
    pub echo_ping: bool,
    pub echo_fat: bool,
    /// What the hit strength and the key move on each note: the middle value where nothing moves (0..=127),
    /// then pan, mod x and mod y, -1..=1.
    pub track_vel: [f32; 4],
    pub track_key: [f32; 4],
    /// Take the constant offset out of the sample, and pan it from the left to the right along its length.
    pub remove_dc: bool,
    pub fade_stereo: bool,
    /// Share of the sample cut from its start, and share of the rest that is kept, 0..=1.
    pub smp_start: f32,
    pub smp_len: f32,
    /// The end is blended into the start over this share of half the sample, 0..=1.
    pub crossfade: f32,
    /// Quiet ends are cut off below this level, 0..=1 (1 = 256 steps of a 16 bit sample).
    pub trim: f32,
    /// Share of the sample skipped when a note starts, 0..=1.
    pub start_offset: f32,
    /// How note starts and ends are smoothed, see [`DECLICK`].
    pub declick: u8,
    /// Semitones the pitch knob and the pitch wheel reach each way.
    pub bend_range: u8,
    /// The way of stretching as the file numbers it, see [`STRETCH_MODES`]; `keep_pitch` is what is heard.
    pub stretch_mode: i8,
    /// How the sample is held: streamed from disk, resampled to the project rate when loaded, and whether
    /// the regions and slice markers stored in it are read.
    pub keep_on_disk: bool,
    pub resample: bool,
    pub load_regions: bool,
    pub load_slices: bool,
}

/// The ways of smoothing note starts and ends, in file order.
pub const DECLICK: [&str; 6] = ["Out only (no bleeding)", "Transient (no bleeding)", "Transient (bleeding)", "Generic (bleeding)", "Smooth (bleeding)", "Crossfade (bleeding)"];

/// The ways of stretching and the number each has in the file, in the order they are listed.
pub const STRETCH_MODES: [(&str, i8); 11] = [
    ("Resample", 0),
    ("E3 generic", 1),
    ("E3 mono", 2),
    ("Stretch", -1),
    ("Slice stretch", 3),
    ("Slice map", 4),
    ("Auto", 5),
    ("E2 generic", 6),
    ("E2 transient", 7),
    ("E2 mono", 8),
    ("E2 speech", 9),
];

/// The gate knob at its right end: notes keep their length.
pub const GATE_OFF: f32 = 7.99;

/// Chords the arpeggiator can spell from each note, after "the held notes" and "the note alone".
pub const ARP_CHORDS: [(&str, &[u8]); 88] = [
    ("Major", &[0, 4, 7]),
    ("sus2", &[0, 2, 7]),
    ("sus4", &[0, 5, 7]),
    ("Majb5", &[0, 4, 6]),
    ("minor", &[0, 3, 7]),
    ("mb5", &[0, 3, 6]),
    ("aug", &[0, 4, 8]),
    ("augsus4", &[0, 5, 8]),
    ("tri", &[0, 3, 6, 9]),
    ("6", &[0, 4, 7, 9]),
    ("6sus4", &[0, 5, 7, 9]),
    ("6add9", &[0, 4, 7, 9, 14]),
    ("m6", &[0, 3, 7, 9]),
    ("m6add9", &[0, 3, 7, 9, 14]),
    ("7", &[0, 4, 7, 10]),
    ("7sus4", &[0, 5, 7, 10]),
    ("7#5", &[0, 4, 8, 10]),
    ("7b5", &[0, 4, 6, 10]),
    ("7#9", &[0, 4, 7, 10, 15]),
    ("7b9", &[0, 4, 7, 10, 13]),
    ("7#5#9", &[0, 4, 8, 10, 15]),
    ("7#5b9", &[0, 4, 8, 10, 13]),
    ("7b5b9", &[0, 4, 6, 10, 13]),
    ("7add11", &[0, 4, 7, 10, 17]),
    ("7add13", &[0, 4, 7, 10, 21]),
    ("7#11", &[0, 4, 7, 10, 18]),
    ("Maj7", &[0, 4, 7, 11]),
    ("Maj7b5", &[0, 4, 6, 11]),
    ("Maj7#5", &[0, 4, 8, 11]),
    ("Maj7#11", &[0, 4, 7, 11, 18]),
    ("Maj7add13", &[0, 4, 7, 11, 21]),
    ("m7", &[0, 3, 7, 10]),
    ("m7b5", &[0, 3, 6, 10]),
    ("m7b9", &[0, 3, 7, 10, 13]),
    ("m7add11", &[0, 3, 7, 10, 17]),
    ("m7add13", &[0, 3, 7, 10, 21]),
    ("m-Maj7", &[0, 3, 7, 11]),
    ("m-Maj7add11", &[0, 3, 7, 11, 17]),
    ("m-Maj7add13", &[0, 3, 7, 11, 21]),
    ("9", &[0, 4, 7, 10, 14]),
    ("9sus4", &[0, 5, 7, 10, 14]),
    ("add9", &[0, 4, 7, 14]),
    ("9#5", &[0, 4, 8, 10, 14]),
    ("9b5", &[0, 4, 6, 10, 14]),
    ("9#11", &[0, 4, 7, 10, 14, 18]),
    ("9b13", &[0, 4, 7, 10, 14, 20]),
    ("Maj9", &[0, 4, 7, 11, 14]),
    ("Maj9sus4", &[0, 5, 7, 11, 14]),
    ("Maj9#5", &[0, 4, 8, 11, 14]),
    ("Maj9#11", &[0, 4, 7, 11, 14, 18]),
    ("m9", &[0, 3, 7, 10, 14]),
    ("madd9", &[0, 3, 7, 14]),
    ("m9b5", &[0, 3, 6, 10, 14]),
    ("m9-Maj7", &[0, 3, 7, 11, 14]),
    ("11", &[0, 4, 7, 10, 14, 17]),
    ("11b9", &[0, 4, 7, 10, 13, 17]),
    ("Maj11", &[0, 4, 7, 11, 14, 17]),
    ("m11", &[0, 3, 7, 10, 14, 17]),
    ("m-Maj11", &[0, 3, 7, 11, 14, 17]),
    ("13", &[0, 4, 7, 10, 14, 21]),
    ("13#9", &[0, 4, 7, 10, 15, 21]),
    ("13b9", &[0, 4, 7, 10, 13, 21]),
    ("13b5b9", &[0, 4, 6, 10, 13, 21]),
    ("Maj13", &[0, 4, 7, 11, 14, 21]),
    ("m13", &[0, 3, 7, 10, 14, 21]),
    ("m-Maj13", &[0, 3, 7, 11, 14, 21]),
    ("Major", &[0, 2, 4, 5, 7, 9, 11]),
    ("Major pentatonic", &[0, 2, 4, 7, 9]),
    ("Major bebop", &[0, 2, 4, 5, 7, 8, 9, 11]),
    ("Minor Harmonic", &[0, 2, 3, 5, 7, 8, 11]),
    ("Minor Melodic", &[0, 2, 3, 5, 7, 9, 11]),
    ("Minor pentatonic", &[0, 3, 5, 7, 10]),
    ("Minor Natural (Aeolian)", &[0, 2, 3, 5, 7, 8, 10]),
    ("Minor Neapolitan", &[0, 1, 3, 5, 7, 8, 11]),
    ("Minor Hungarian", &[0, 2, 3, 6, 7, 8, 11]),
    ("Whole tone", &[0, 2, 4, 6, 8, 10]),
    ("Diminished", &[0, 2, 3, 5, 6, 8, 9, 11]),
    ("Dominant bebop", &[0, 2, 4, 5, 7, 9, 10, 11]),
    ("Jap in sen", &[0, 1, 5, 7, 10]),
    ("Blues", &[0, 3, 5, 6, 7, 10]),
    ("Arabic", &[0, 1, 4, 5, 7, 8, 11]),
    ("Enigmatic", &[0, 1, 4, 6, 8, 10, 11]),
    ("Neapolitan", &[0, 1, 3, 5, 7, 9, 11]),
    ("Dorian", &[0, 2, 3, 5, 7, 9, 10]),
    ("Phrygian", &[0, 1, 3, 5, 7, 8, 10]),
    ("Lydian", &[0, 2, 4, 6, 7, 9, 11]),
    ("Mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
    ("Locrian", &[0, 1, 3, 5, 6, 8, 10]),
];

/// Where the set chord list is ruled off when shown: after these many entries. Scales start at the last one.
pub const ARP_CHORD_BREAKS: [usize; 6] = [9, 14, 39, 54, 59, 66];

/// The arpeggiator's time knob fully right: the first note is held and nothing follows.
pub const ARP_HOLD: f32 = 8.0;

/// The arpeggiator's time knob against steps between notes, before it is cut down to whole ticks: fitted to
/// renders of the reference at eleven settings from a 24th of a step to four steps (1024 = one step).
pub fn arp_knob_steps(raw: i32) -> f32 {
    if raw >= 1447 {
        return ARP_HOLD;
    }
    knob_steps(raw).max(1.0 / 24.0)
}

pub fn arp_steps_knob(steps: f32) -> i32 {
    if steps >= ARP_HOLD {
        return 1447;
    }
    steps_knob(steps)
}

/// The time knobs of a channel (arpeggiator time, gate, shift): the stored value against steps, before the
/// player cuts it down to whole ticks. 1024 is one step; the curve is the one the arpeggiator fit gave, and
/// renders with the gate and shift knobs fall on it too.
pub fn knob_steps(raw: i32) -> f32 {
    let x = raw.clamp(0, 1446) as f32 / 1024.0;
    24.2 / 27.7 * (28f32.powf(x) - 0.3) / 24.0
}

pub fn steps_knob(steps: f32) -> i32 {
    (((steps.max(0.0) * 24.0 * 27.7 / 24.2 + 0.3).ln() / 28f32.ln() * 1024.0).round() as i32).clamp(0, 1446)
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            vol_env: Env::default(),
            cut_env: Env::default(),
            pitch_env: Env::default(),
            res_env: Env::default(),
            lfos: [Lfo::default(); 5],
            cutoff: 1.0,
            resonance: 0.0,
            filter: 0,
            fade_in: 0.0,
            fade_out: 0.0,
            reverse: false,
            swap_stereo: false,
            normalize: false,
            invert: false,
            use_loop: false,
            ping_pong: false,
            cut: 0,
            cut_by: 0,
            mono: false,
            porta: false,
            slide: 0.1,
            swing: 1.0,
            echoes: 4,
            echo_feedback: 0.0,
            echo_pan: 0.0,
            echo_cents: 0.0,
            echo_mod_x: 0.0,
            echo_mod_y: 0.0,
            old_fx: OLD_FX_OFF,
            echo_steps: 3.0,
            stretch_beats: 0.0,
            stretch_cents: 0.0,
            stretch_mul: 1.0,
            keep_pitch: false,
            remove_dc: false,
            fade_stereo: false,
            smp_start: 0.0,
            smp_len: 1.0,
            crossfade: 0.0,
            trim: 0.0,
            start_offset: 0.0,
            declick: 0,
            bend_range: 2,
            stretch_mode: 0,
            keep_on_disk: false,
            resample: true,
            load_regions: true,
            load_slices: false,
            gain: 1.0,
            arp_dir: 0,
            arp_range: 1,
            arp_chord: -1,
            arp_steps: arp_knob_steps(1024),
            arp_gate: 1.0,
            arp_slide: false,
            arp_repeat: 1,
            gate_steps: 8.0,
            shift: 0.0,
            full_porta: true,
            add_to_key: false,
            main_pitch: true,
            max_poly: 0,
            adjust_pan: 0.0,
            adjust_x: 0.0,
            adjust_y: 0.0,
            key_lo: 0,
            key_hi: 256,
            echo_ping: false,
            echo_fat: false,
            track_vel: [100.0, 0.0, 0.0, 0.0],
            track_key: [60.0, 0.0, 0.0, 0.0],
        }
    }
}

fn track(t: Option<&[i32; 4]>, mid: f32) -> [f32; 4] {
    match t {
        Some(t) => [t[0].clamp(0, 127) as f32, t[1] as f32 / 256.0, t[2] as f32 / 256.0, t[3] as f32 / 256.0],
        None => [mid, 0.0, 0.0, 0.0],
    }
}

/// The older treatments at rest.
/// The sample rate a project works at (where samples with Resample on are brought when loaded).
pub const PROJECT_RATE: u32 = 44_100;

pub const OLD_FX_OFF: [u32; 8] = [2048, 256, 0x0080_0000, 0x0001_0000, 0, 128, 1024, 0];

impl Sampler {
    fn from_flp(c: &flp::FlpChannel) -> Self {
        let env = |i: usize, scale: f32| c.envelopes.get(i).map(|e| Env::from_flp(e, scale)).unwrap_or_default();
        Self {
            vol_env: env(1, 1.0),
            cut_env: env(2, 1.0 / 128.0),
            pitch_env: env(4, 1.0),
            res_env: env(3, 1.0 / 128.0),
            lfos: std::array::from_fn(|k| c.envelopes.get(k).map(|e| Lfo::from_flp(e, LFO_SCALE[k])).unwrap_or_default()),
            cutoff: (c.filter.0 as f32 / 256.0).clamp(0.0, 1.0),
            resonance: (c.filter.1 as f32 / 256.0).clamp(0.0, 1.0),
            filter: c.filter.2.min(7) as u8,
            fade_in: (c.fade_in as f32 / 1024.0).min(1.0),
            fade_out: (c.fade_out as f32 / 1024.0).min(1.0),
            reverse: c.fx_flags & 2 != 0,
            swap_stereo: c.fx_flags & 256 != 0,
            normalize: c.normalize,
            invert: c.invert,
            use_loop: c.sample_flags & 8 != 0,
            ping_pong: c.ping_pong,
            cut: c.cut_group.0,
            cut_by: c.cut_group.1,
            mono: c.poly.2 & 1 != 0,
            porta: c.poly.2 & 2 != 0,
            slide: c.poly.1 as f32 / 5000.0,
            swing: (c.swing as f32 / 128.0).clamp(0.0, 1.0),
            echoes: c.echo[3].clamp(0, 10) as u8,
            echo_feedback: (c.echo[0] as f32 / 12800.0).clamp(0.0, 1.0),
            echo_pan: (c.echo[1] as f32 - 6400.0) / 6400.0,
            echo_cents: c.echo[2] as f32,
            old_fx: c.old_fx,
            echo_mod_x: ((c.echo_mod.1 as f32 - 128.0) / 128.0).clamp(-1.0, 1.0),
            echo_mod_y: ((c.echo_mod.0 as f32 - 128.0) / 128.0).clamp(-1.0, 1.0),
            echo_steps: c.echo[4] as f32 / 48.0,
            stretch_beats: (c.stretch.0 / STRETCH_UNITS).max(0.0),
            stretch_cents: c.stretch.1 as f32,
            stretch_mul: (c.stretch.2 as f32 / 10000.0).exp2(),
            keep_pitch: c.stretch.3 != 0,
            remove_dc: c.remove_dc,
            fade_stereo: c.fx_flags & 1 != 0,
            smp_start: c.smp_start as f32,
            smp_len: c.smp_len as f32,
            crossfade: c.crossfade as f32 / 1024.0,
            trim: c.trim as f32 / 256.0,
            start_offset: c.start_offset as f32,
            declick: c.declick.min(5),
            bend_range: c.bend_range.clamp(0, 48) as u8,
            stretch_mode: c.stretch.3.clamp(-1, 9) as i8,
            keep_on_disk: c.sample_flags & 256 != 0,
            resample: c.sample_flags & 1 != 0,
            load_regions: c.sample_flags & 2 != 0,
            load_slices: c.sample_flags & 4 != 0,
            gain: (c.adjust[1] as f32 / 12800.0).clamp(0.0, 4.0),
            arp_dir: c.arp[0].clamp(0, 5) as u8,
            arp_range: c.arp[1].clamp(1, 10) as u8,
            arp_chord: c.arp[2].clamp(-2, ARP_CHORDS.len() as i32) as i16,
            arp_steps: arp_knob_steps(c.arp[3]),
            arp_gate: (c.arp[4] as f32 / 48.0).clamp(0.0, 1.0),
            arp_slide: c.arp_slide,
            arp_repeat: c.arp[5].clamp(1, 16) as u8,
            gate_steps: if c.gate >= 1447 { 8.0 } else { knob_steps(c.gate as i32) },
            shift: (c.time_shift as f32 / 1024.0).clamp(0.0, 1.0),
            full_porta: c.full_porta,
            add_to_key: c.add_to_key,
            main_pitch: c.main_pitch,
            max_poly: c.poly.0.min(999),
            adjust_pan: (c.adjust[0] as f32 / 6400.0).clamp(-1.0, 1.0),
            adjust_x: (c.adjust[3] as f32 / 256.0).clamp(-1.0, 1.0),
            adjust_y: (c.adjust[4] as f32 / 256.0).clamp(-1.0, 1.0),
            key_lo: c.key_range.0,
            key_hi: c.key_range.1,
            echo_ping: c.echo_flags & 2 != 0,
            echo_fat: c.echo_flags & 4 != 0,
            track_vel: track(c.tracking.first(), 100.0),
            track_key: track(c.tracking.get(1), 60.0),
        }
    }

    /// The sweep's amount, -1..1: above nothing the pitch starts high and falls.
    fn pogo(&self) -> f64 {
        (self.old_fx[1] as f64 - 256.0).clamp(-256.0, 256.0) / 256.0
    }

    fn treats_sample(&self) -> bool {
        self.reverse
            || self.swap_stereo
            || self.normalize
            || self.invert
            || self.fade_in > 0.0
            || self.fade_out > 0.0
            || self.remove_dc
            || self.fade_stereo
            || self.smp_start > 0.0
            || self.smp_len < 1.0
            || self.crossfade > 0.0
            || self.trim > 0.0
            || self.old_fx != OLD_FX_OFF
    }

    /// The stretch mode number to store: the one chosen, unless what is heard was switched apart from it.
    pub fn mode_number(&self) -> i8 {
        match (self.keep_pitch, self.stretch_mode != 0) {
            (true, false) => -1,
            (false, true) => 0,
            _ => self.stretch_mode,
        }
    }
}

impl Chan {
    /// A hosted plug-in this machine does not have: silent, or playing a voice of our own in its place.
    pub fn missing_plugin(&self) -> bool {
        self.kind == ChannelKind::Plugin && self.ext.is_none() && self.plugin.eq_ignore_ascii_case(WRAPPER)
    }

    /// The patch bay this channel's instrument plays through, while the channel is still a bay.
    pub fn bay(&self) -> Option<&Bay> {
        self.bay.as_ref().filter(|_| self.kind == ChannelKind::Plugin && self.plugin.eq_ignore_ascii_case(BAY))
    }
    /// Set the sample this channel plays, applying the treatments that are worked out ahead of time.
    /// A new channel playing one of the built-in instruments.
    pub fn instrument(kind: crate::synth::Kind) -> Self {
        let d = crate::synth::def(kind);
        Self { name: d.title.into(), kind: ChannelKind::Plugin, plugin: d.ident.into(), gen: Some(crate::synth::Gen::new(kind)), bay: None, ..Default::default() }
    }

    /// A channel playing a plug-in from another maker.
    pub fn hosted(inst: Arc<crate::host::Instance>) -> Self {
        Self { name: inst.found.name.clone(), kind: ChannelKind::Plugin, plugin: WRAPPER.into(), ext: Some(crate::host::Hosted(inst)), ..Default::default() }
    }

    pub fn set_sample(&mut self, raw: Option<Arc<Sample>>) {
        self.raw = raw;
        self.bake();
    }

    /// Redo the treated copy after changing reverse, fades, normalise or stereo swap.
    pub fn bake(&mut self) {
        if self.raw.is_none() {
            self.raw = self.sample.clone();
        }
        let Some(raw) = self.raw.clone() else {
            self.sample = None;
            return;
        };
        let f = &self.smp;
        // Resample on: the sample is held at the project's rate, and every treatment below works on that
        // (a sliced loop keeps its own frames, the cuts count in them)
        let own = matches!(self.kind, ChannelKind::Sampler | ChannelKind::AudioClip) && self.slices.is_none();
        let raw = if f.resample && own { raw.to_rate(PROJECT_RATE) } else { raw };
        if !f.treats_sample() {
            self.sample = Some(raw);
            return;
        }
        let mut frames = raw.frames.clone();
        let mut loop_pts = raw.loop_pts;
        // the cut comes first: a share off the start, then a share of what is left
        if f.smp_start > 0.0 || f.smp_len < 1.0 {
            let n = frames.len();
            let a = ((f.smp_start.clamp(0.0, 1.0) as f64 * n as f64).round() as usize).min(n);
            let keep = ((f.smp_len.clamp(0.0, 1.0) as f64 * (n - a) as f64).round() as usize).min(n - a);
            frames = frames[a..a + keep].to_vec();
            // (measured: a cut edge is eased over half a millisecond)
            let e = ((raw.rate as f32 * 0.0005) as usize).min(keep / 2);
            for i in 0..e {
                let g = 0.5 - 0.5 * (std::f32::consts::PI * i as f32 / e as f32).cos();
                if a > 0 {
                    frames[i][0] *= g;
                    frames[i][1] *= g;
                }
                if a + keep < n {
                    frames[keep - 1 - i][0] *= g;
                    frames[keep - 1 - i][1] *= g;
                }
            }
            loop_pts = loop_pts.and_then(|(s, e)| {
                let (s, e) = (s.saturating_sub(a as u32).min(keep as u32), e.saturating_sub(a as u32).min(keep as u32));
                (e > s).then_some((s, e))
            });
        }
        // one side held back (measured: by the stored distance from the middle, in frames, ahead of the turning round)
        let late = f.old_fx[0] as i64 - 2048;
        if late != 0 && !frames.is_empty() {
            let n = frames.len();
            // (measured: the sample grows by the wait, so the late side still plays to its end)
            let (side, d) = (if late > 0 { 0 } else { 1 }, late.unsigned_abs() as usize);
            frames.resize(n + d, [0.0; 2]);
            for i in (0..n + d).rev() {
                frames[i][side] = if i >= d { frames[i - d][side] } else { 0.0 };
            }
        }
        if f.reverse {
            frames.reverse();
        }
        // the pitch sweep reads the sample along a bent line (measured: a blend of the straight
        // line with a quarter sine one way, and a quarter arc from the far end the other way)
        let a = f.pogo();
        if a != 0.0 && frames.len() > 1 {
            let n = frames.len();
            let src = std::mem::take(&mut frames);
            // (measured: read through a four point curve)
            let get = |i: i64, c: usize| if i < 0 || i >= n as i64 { 0.0 } else { src[i as usize][c] };
            let at = |p: f64| {
                let p = p.clamp(0.0, (n - 1) as f64);
                let (i, t) = (p.floor() as i64, (p - p.floor()) as f32);
                let mut o = [0f32; 2];
                for (c, o) in o.iter_mut().enumerate() {
                    let (a, b, d, e) = (get(i - 1, c), get(i, c), get(i + 1, c), get(i + 2, c));
                    *o = b + 0.5 * t * (d - a + t * (2.0 * a - 5.0 * b + 4.0 * d - e + t * (3.0 * (b - d) + e - a)));
                }
                o
            };
            frames = (0..n)
                .map(|i| {
                    let x = i as f64 / (n - 1) as f64;
                    let bent = if a > 0.0 { (x * std::f64::consts::FRAC_PI_2).sin() } else { (1.0 - x).acos() / std::f64::consts::FRAC_PI_2 };
                    at((n - 1) as f64 * (x + a.abs() * (bent - x)))
                })
                .collect();
            loop_pts = None;
        }
        // fades (measured: the knob is a slope over the whole sample, a quarter turn spans all
        // of it, further turns reach silence sooner and leave the rest silent)
        if f.fade_in > 0.0 || f.fade_out > 0.0 {
            let n = frames.len().max(1) as f32;
            let (ki, ko) = (f.fade_in * 4.0, f.fade_out * 4.0);
            for (i, s) in frames.iter_mut().enumerate() {
                let t = i as f32 / n;
                let g = (1.0 - ki * (1.0 - t)).clamp(0.0, 1.0) * (1.0 - ko * t).clamp(0.0, 1.0);
                s[0] *= g;
                s[1] *= g;
            }
        }
        // (measured: the offset is taken off and the level raised after the fades)
        if f.remove_dc && !frames.is_empty() {
            let n = frames.len() as f64;
            let (l, r) = frames.iter().fold((0f64, 0f64), |m, s| (m.0 + s[0] as f64, m.1 + s[1] as f64));
            let (l, r) = ((l / n) as f32, (r / n) as f32);
            for s in &mut frames {
                s[0] -= l;
                s[1] -= r;
            }
        }
        if f.normalize {
            let peak = frames.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()));
            if peak > 1e-6 {
                let g = 1.0 / peak;
                for s in &mut frames {
                    s[0] *= g;
                    s[1] *= g;
                }
            }
        }
        // quiet ends go (measured: after levelling and fades): everything before the first and after the last frame at the trim level
        // (measured: the level is the knob's 0..256 counted in steps of a 16 bit sample)
        if f.trim > 0.0 {
            let thr = f.trim * 256.0 / 32768.0;
            let loud = |s: &[f32; 2]| s[0].abs() >= thr || s[1].abs() >= thr;
            match (frames.iter().position(loud), frames.iter().rposition(loud)) {
                (Some(a), Some(b)) => {
                    frames = frames[a..=b].to_vec();
                    loop_pts = loop_pts.and_then(|(s, e)| {
                        let (s, e) = (s.saturating_sub(a as u32).min((b + 1 - a) as u32), e.saturating_sub(a as u32).min((b + 1 - a) as u32));
                        (e > s).then_some((s, e))
                    });
                }
                _ => {
                    frames.clear();
                    loop_pts = None;
                }
            }
        }
        let n = frames.len();
        // the end is blended into the start so the sample can go round without a step
        // (measured: at full the second half fades out as the first half fades in over it)
        if f.crossfade > 0.0 {
            let x = ((f.crossfade.min(1.0) as f64 * n as f64 / 2.0).round() as usize).min(n / 2);
            for i in 0..x {
                let g = i as f32 / x as f32;
                let from = frames[i];
                let s = &mut frames[n - x + i];
                s[0] = s[0] * (1.0 - g) + from[0] * g;
                s[1] = s[1] * (1.0 - g) + from[1] * g;
            }
        }
        if f.fade_stereo && n > 1 {
            for (i, s) in frames.iter_mut().enumerate() {
                // (measured: the two sides keep equal power between them)
                let g = i as f32 / (n - 1) as f32 * std::f32::consts::FRAC_PI_2;
                s[0] *= g.cos();
                s[1] *= g.sin();
            }
        }
        // boost (measured: sixteen steps of the stored number double the level)
        if f.old_fx[4] != 0 {
            let g = 1.0 + f.old_fx[4] as f32 / 16.0;
            for s in &mut frames {
                s[0] *= g;
                s[1] *= g;
            }
        }
        // ring modulation (measured: a sine on the left and a cosine on the right, both the sine
        // for a one sided sample, from 61.25 Hz up in steps of an eighth of that)
        let (ring, rate) = ((f.old_fx[2] & 0xffff).min(256) as f32 / 256.0, (f.old_fx[2] >> 16).min(256) as f64);
        if ring > 0.0 {
            let w = std::f64::consts::TAU / 720.0 * (1.0 + rate / 8.0);
            for (i, s) in frames.iter_mut().enumerate() {
                let (sin, cos) = (w * i as f64).sin_cos();
                s[0] *= 1.0 - ring + ring * sin as f32;
                s[1] *= 1.0 - ring + ring * if raw.mono { sin } else { cos } as f32;
            }
        }
        old_tone(&mut frames, f.old_fx[5], f.old_fx[6], f.old_fx[7]);
        old_reverb(&mut frames, f.old_fx[3]);
        for s in &mut frames {
            if f.swap_stereo {
                s.swap(0, 1);
            }
            if f.invert {
                s[0] = -s[0];
                s[1] = -s[1];
            }
        }
        self.sample = Some(Arc::new(Sample { name: raw.name.clone(), rate: raw.rate, mono: raw.mono && !f.swap_stereo && !f.fade_stereo && f.old_fx[0] == 2048 && f.old_fx[3] < 0x1_0001, frames, loop_pts: if f.reverse { None } else { loop_pts } }));
    }
}

/// The older tone knobs worked into the sample: EQ, then the low-pass with its resonance. All measured:
/// the EQ is one pole at 0.75 x (stored - 128) / 128 (below the middle it brightens, above it darkens and
/// lifts), the low-pass is `y += f (x - y) + fb (y - y before)` where f climbs as e^(6.4 x) from the knob
/// and the resonance is the same curve read backwards, stopping at 640.
fn old_tone(frames: &mut [[f32; 2]], eq: u32, cut: u32, res: u32) {
    if eq != 128 {
        let p = 0.75 * (eq.min(256) as f64 - 128.0) / 128.0;
        let mut y = [0.0f64; 2];
        for s in frames.iter_mut() {
            for c in 0..2 {
                y[c] = s[c] as f64 + p * y[c];
                s[c] = y[c] as f32;
            }
        }
    }
    if cut < 1024 || res > 0 {
        let curve = |v: f64| (((6.4 * v / 1024.0).exp() - 1.0) / (6.4f64.exp() - 1.0)).max(0.00023);
        let f = curve(cut.min(1024) as f64);
        let fb = if res == 0 { 0.0 } else { 1.0 - curve(1024.0 - res.min(640) as f64) };
        let (mut y1, mut y2) = ([0.0f64; 2], [0.0f64; 2]);
        for s in frames.iter_mut() {
            for c in 0..2 {
                let y = y1[c] + f * (s[c] as f64 - y1[c]) + fb * (y1[c] - y2[c]);
                y2[c] = y1[c];
                y1[c] = y;
                s[c] = y as f32;
            }
        }
    }
}

/// The older reverb, worked into the sample (measured from what one click comes back as: a
/// bank of repeating delays that die away over the stored length, 256 = one second, then a chain
/// of spreading delays, softened, under seven tenths of the sample itself).
fn old_reverb(frames: &mut Vec<[f32; 2]>, stored: u32) {
    let (second, amount) = (stored >= 0x1_0000, (stored & 0xffff).min(256));
    if amount == 0 || stored > 0x1_0100 {
        return;
    }
    let ring = 44100.0 * amount as f64 / 256.0;
    // (measured: the sample grows to hold the tail, and ends at the last frame above two thousandths)
    frames.resize(frames.len() + 22050, [0.0; 2]);
    let bank: &[usize] = if second { &[2467, 2767, 3217, 3533, 3877, 4127] } else { &[1777, 1847, 1993, 2137] };
    for c in 0..2 {
        let chain: &[usize] = match (second, c) {
            (false, _) => &[43, 127, 389],
            (true, 0) => &[67, 97, 101, 197, 599],
            (true, _) => &[67, 79, 101, 197, 599],
        };
        let n = frames.len();
        let dry: Vec<f32> = frames.iter().map(|s| s[c]).collect();
        let mut wet = vec![0f32; n];
        for &d in bank {
            let g = 10f64.powf(-3.0 * d as f64 / ring) as f32;
            let mut y = vec![0f32; n];
            for i in 0..n {
                y[i] = dry[i] + if i >= d { g * y[i - d] } else { 0.0 };
                wet[i] += y[i];
            }
        }
        let share = 1.0 / bank.len() as f32;
        wet.iter_mut().for_each(|v| *v *= share);
        for &d in chain {
            let x = std::mem::replace(&mut wet, vec![0f32; n]);
            for i in 0..n {
                wet[i] = -0.7 * x[i] + if i >= d { x[i - d] + 0.7 * wet[i - d] } else { 0.0 };
            }
        }
        if second {
            let mut y = 0f32;
            for (s, w) in frames.iter_mut().zip(&wet) {
                y = 0.3 * w + 0.7 * y;
                s[c] = 0.7 * s[c] + 0.541 * y;
            }
        } else {
            for i in 0..n {
                let (a, b) = (if i >= 2 { wet[i - 2] } else { 0.0 }, if i >= 1 { wet[i - 1] } else { 0.0 });
                frames[i][c] = 0.7 * dry[i] + 1.2 * (0.25 * a + 0.5 * b + 0.25 * wet[i]);
            }
        }
    }
    let keep = frames.iter().rposition(|s| s[0].abs() > 0.002 || s[1].abs() > 0.002).map_or(0, |i| i + 1);
    frames.truncate(keep);
}

/// A control an automation clip can drive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    ChanVolume(usize),
    ChanPan(usize),
    /// The channel's pitch knob (taken as two semitones either way).
    ChanPitch(usize),
    InsertVolume(usize),
    InsertPan(usize),
    Tempo,
    /// Dry/wet of an effect slot: insert, slot.
    SlotMix(usize, u16),
    /// On/off switch of an effect slot.
    SlotOn(usize, u16),
    /// One of the effect's own settings, counted in the order the unit lists them: insert, slot, setting.
    SlotParam(usize, u16, u16),
    /// A setting of a channel's own instrument: channel and the setting's place in its table.
    ChanParam(usize, u16),
    /// Something not played back yet (plugin controls, slot levels, ...): raw control number and destination.
    Other { param: u16, dest: u16 },
}

/// The run of a hosted plug-in's setting as events count it.
const HOSTED_RANGE: (i32, i32) = (0, 65536);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoPt {
    /// Ticks from the start of the clip.
    pub tick: f64,
    /// 0..=1 of the control's travel.
    pub value: f32,
    /// Bend of the line arriving at this point, -1..=1.
    pub tension: f32,
    /// Shape of the line arriving at this point.
    pub mode: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Automation {
    pub points: Vec<AutoPt>,
    pub targets: Vec<Target>,
}

/// The bend of a channel envelope's stage. Positive tension starts slowly and finishes fast.
/// (Approximate: see docs/PARITY.md.)
fn env_bend(t: f32, tension: f32) -> f32 {
    if tension.abs() < 1e-4 {
        t
    } else {
        t.powf((tension.clamp(-1.0, 1.0) * 3.0).exp2())
    }
}

/// How far along a drawn line's piece has come at `t` (0..=1) of its width, for the piece's kind and
/// tension. The same line editor draws automation clips and the lines inside units, so both read this.
/// Measured against renders of automation clips and of drawn tables.
/// How many levels a stepped line takes for a tension (measured at forty tensions: 0 -> 1002,
/// 0.25 -> 128, 0.5 -> 18, 0.75 -> 4, 1 -> 2).
fn step_count(k: f32) -> f32 {
    (2.0 + 10f64.powf(3.0 - 3.6 * (k.abs().min(1.0) as f64))).round().max(2.0) as f32
}

/// Strength of the ratio curve for a tension: 1 is the straight line, smaller rises sooner.
fn ratio_strength(k: f32) -> f32 {
    const T: [f32; 9] = [0.0, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 1.0];
    const L: [f32; 9] = [0.0, -0.4142, -1.109, -1.938, -2.594, -3.368, -4.878, -7.202, -9.97];
    let x = k.abs().min(1.0);
    let i = (0..8).rfind(|&i| x >= T[i]).unwrap_or(0);
    let slope = |a: usize, b: usize| (L[b] - L[a]) / (T[b] - T[a]);
    let m0 = if i == 0 { slope(0, 1) } else { 0.5 * (slope(i - 1, i) + slope(i, i + 1)) };
    let m1 = if i == 7 { slope(7, 8) } else { 0.5 * (slope(i, i + 1) + slope(i + 1, i + 2)) };
    let h = T[i + 1] - T[i];
    let u = (x - T[i]) / h;
    let (u2, u3) = (u * u, u * u * u);
    let l = (2.0 * u3 - 3.0 * u2 + 1.0) * L[i]
        + (u3 - 2.0 * u2 + u) * h * m0
        + (-2.0 * u3 + 3.0 * u2) * L[i + 1]
        + (u3 - u2) * h * m1;
    if k >= 0.0 {
        l.exp2()
    } else {
        (-l).exp2()
    }
}

pub fn line_shape(mode: u8, t: f32, k: f32) -> f32 {
    use crate::fm::bend;
    match mode {
        // hold: stay on the earlier value until this point
        2 => {
            if t >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        1 | 8 | 12 => {
            let single = mode - 1;
            if t < 0.5 {
                0.5 * line_shape(single, 2.0 * t, k)
            } else {
                1.0 - 0.5 * line_shape(single, 2.0 - 2.0 * t, k)
            }
        }
        // the second single curve: a sixth power, mixed with the straight line by the tension
        7 => {
            let w = k.abs().min(1.0);
            let c = if k >= 0.0 { 1.0 - (1.0 - t).powi(6) } else { t.powi(6) };
            t + (c - t) * w
        }
        // stairs: the tension sets how many levels the piece takes. A positive tension spends an equal time
        // on each level and ends on the end value; a negative one leaves the start value at once and only
        // reaches the end value at the point itself
        3 => {
            let n = step_count(k);
            if t >= 1.0 {
                1.0
            } else if k >= 0.0 {
                ((t * n + 1e-4).floor().min(n - 1.0) / (n - 1.0)).clamp(0.0, 1.0)
            } else {
                (((t * (n - 1.0) + 1e-4).floor() + 1.0) / n).clamp(0.0, 1.0)
            }
        }
        // smooth stairs: one level fewer, joined by an S that takes the whole of each period
        4 => {
            // a negative tension starts and ends half way through a rise
            let periods = step_count(k) - 1.0;
            let half = if k >= 0.0 { 0.0 } else { 0.5 };
            let at = t.clamp(0.0, 1.0) * periods + half;
            let j = at.floor();
            let u = at - j;
            let sm = if u < 0.5 { 4.0 * u * u * u } else { 1.0 - 4.0 * (1.0 - u).powi(3) };
            ((j + sm - half) / periods).clamp(0.0, 1.0)
        }
        // pulse: flips between the two values, a positive tension starting low
        5 => {
            if t >= 1.0 {
                return 1.0;
            }
            let at = t.max(0.0) * (step_count(k) - 1.0) + 1e-4;
            let late = at - at.floor() >= 0.5;
            if late == (k >= 0.0) {
                1.0
            } else {
                0.0
            }
        }
        // wave: a triangle for a positive tension, a sine for a negative one, ending on the end value
        6 => {
            let halves = 2.0 * (step_count(k) - 1.0) - 1.0;
            let at = t.clamp(0.0, 1.0) * halves;
            if k >= 0.0 {
                let j = at.floor().min(halves - 1.0);
                let u = at - j;
                if j as i64 % 2 == 0 {
                    u
                } else {
                    1.0 - u
                }
            } else {
                0.5 - 0.5 * (std::f32::consts::PI * at).cos()
            }
        }
        // the third single curve: a ratio curve whose strength follows a measured table
        11 => {
            let c = ratio_strength(k);
            let d = t + c * (1.0 - t);
            if d.abs() < 1e-12 {
                t
            } else {
                (t / d).clamp(0.0, 1.0)
            }
        }
        // a quarter of a sine, mixed with the straight line by the tension; negative tension turns it over
        9 => {
            let w = k.abs().min(1.0);
            let x = std::f32::consts::FRAC_PI_2 * t;
            let c = if k >= 0.0 { x.sin() } else { 1.0 - x.cos() };
            t + (c - t) * w
        }
        // smooth: an S at no tension, straightening as the tension grows either way
        10 => {
            let c = t * t * (3.0 - 2.0 * t);
            c + (t - c) * k.abs().min(1.0)
        }
        _ => bend(t, k),
    }
}

impl Automation {
    /// Value at `tick` from the start of the clip; holds the first and last values outside the points.
    pub fn value_at(&self, tick: f64) -> f32 {
        let Some(first) = self.points.first() else { return 0.0 };
        if tick <= first.tick {
            return first.value;
        }
        let i = self.points.partition_point(|p| p.tick <= tick);
        if i >= self.points.len() {
            return self.points[self.points.len() - 1].value;
        }
        let (a, b) = (self.points[i - 1], self.points[i]);
        let span = b.tick - a.tick;
        if span <= 0.0 {
            return b.value;
        }
        let t = ((tick - a.tick) / span) as f32;
        let shaped = line_shape(b.mode, t, b.tension);
        a.value + (b.value - a.value) * shaped
    }

    /// Ticks from the first point to the last.
    pub fn length(&self) -> f64 {
        self.points.last().map(|p| p.tick).unwrap_or(0.0)
    }
}

impl Chan {
    /// An automation clip's curve fitted between its lowest and highest setting. The channel's
    /// pan slot holds the lowest and its volume slot the highest (measured against renders).
    pub fn auto_range(&self, v: f32) -> f32 {
        let lo = ((self.pan + 1.0) * 0.5).clamp(0.0, 1.0);
        let hi = self.volume.clamp(0.0, 1.0);
        lo + (hi - lo) * v
    }

    /// A fresh automation clip channel: full range.
    pub fn automation() -> Self {
        Self { kind: ChannelKind::AutomationClip, volume: 1.0, pan: -1.0, ..Default::default() }
    }
}

/// Where the clips of one automation channel sit in the playlist; built by `Song::prepare`.
#[derive(Debug, Clone, Default)]
pub struct AutoPlan {
    pub chan: usize,
    /// (position, length, offset) of every sounding clip, by position.
    pub clips: Vec<(u32, u32, u32)>,
}

impl Default for Chan {
    fn default() -> Self {
        Self { name: "Sampler".into(), kind: ChannelKind::Sampler, plugin: String::new(), sample_path: String::new(), sample: None, color: [92, 101, 106], enabled: true, volume: 10000.0 / 12800.0, pan: 0.0, pitch_cents: 0.0, root_key: 60, root_fine: 0.0, insert: 0, env_on: false, group: usize::MAX, zipped: false, src: None, auto: None, children: vec![], smp: Sampler::default(), raw: None, gen: None, bay: None, slices: None, zones: None, ext: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub pos: u32,
    /// Ticks; 0 = a step (plays the whole sample).
    pub len: u32,
    pub chan: u16,
    /// 60 = C5.
    pub key: u8,
    /// 1.0 = the default velocity (100 of 128).
    pub vel: f32,
    /// -1..=1.
    pub pan: f32,
    pub fine_cents: f32,
    pub muted: bool,
    /// Release velocity, 0..=1, 0.5 = the default.
    pub release: f32,
    /// The two free per-note controls, -1..=1; on a sampler they move the filter's cutoff and resonance.
    pub mod_x: f32,
    pub mod_y: f32,
    /// A slide note makes no sound: it bends the notes of its colour already sounding to its own pitch.
    pub slide: bool,
    /// A portamento note glides in from the pitch of the note before it.
    pub porta: bool,
    /// Note colour, 0..=15; also the MIDI channel it goes out on.
    pub color: u8,
    /// Notes that share a number other than 0 are grouped and are picked and moved as one.
    pub group: u16,
}

impl Default for Note {
    fn default() -> Self {
        Self { pos: 0, len: 0, chan: 0, key: 60, vel: 1.0, pan: 0.0, fine_cents: 0.0, muted: false, release: 0.5, mod_x: 0.0, mod_y: 0.0, slide: false, porta: false, color: 0, group: 0 }
    }
}

impl Note {
    pub fn step(pos: u32, chan: u16) -> Self {
        Self { pos, chan, ..Default::default() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pattern {
    pub name: String,
    pub color: Option<Color>,
    /// Kept sorted by position.
    pub notes: Vec<Note>,
    /// Recorded control movements, kept sorted by position.
    pub events: Vec<PatEvent>,
    /// Markers on the pattern's own time line, as the piano roll shows them.
    pub markers: Vec<TimeMarker>,
}

/// One control movement inside a pattern, as the file holds it: which control of which destination, and
/// the control's own raw value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatEvent {
    pub tick: u32,
    pub param: u16,
    pub dest: u16,
    pub raw: i32,
}

impl Pattern {
    pub fn sort(&mut self) {
        self.notes.sort_by_key(|n| (n.pos, n.chan, n.key));
    }
    /// Length in ticks: the notes rounded up to whole bars, at least one bar.
    pub fn length(&self, bar: u32) -> u32 {
        let end = self.notes.iter().map(|n| n.pos + n.len.max(1)).max().unwrap_or(0).max(self.events.last().map(|e| e.tick + 1).unwrap_or(0));
        end.div_ceil(bar).max(1) * bar
    }
    pub fn has_step(&self, chan: u16, pos: u32) -> bool {
        self.notes.iter().any(|n| n.chan == chan && n.pos == pos)
    }
    /// Toggle a step-sequencer step. Returns the new state.
    pub fn toggle_step(&mut self, chan: u16, pos: u32) -> bool {
        let before = self.notes.len();
        self.notes.retain(|n| !(n.chan == chan && n.pos == pos));
        if self.notes.len() == before {
            self.notes.push(Note::step(pos, chan));
            self.sort();
            true
        } else {
            false
        }
    }
    /// A step counts as lit when the channel has a note anywhere inside it: a shifted step starts late.
    pub fn has_step_in(&self, chan: u16, pos: u32, step: u32) -> bool {
        self.notes.iter().any(|n| n.chan == chan && n.pos >= pos && n.pos < pos + step)
    }
    pub fn set_step_in(&mut self, chan: u16, pos: u32, step: u32, on: bool) {
        if !on {
            self.notes.retain(|n| !(n.chan == chan && n.pos >= pos && n.pos < pos + step));
        } else if !self.has_step_in(chan, pos, step) {
            self.notes.push(Note::step(pos, chan));
            self.sort();
        }
    }
    pub fn set_step(&mut self, chan: u16, pos: u32, on: bool) {
        if self.has_step(chan, pos) != on {
            self.toggle_step(chan, pos);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipSrc {
    /// Index into `Song::patterns`.
    Pattern(usize),
    /// Index into `Song::channels` (an audio clip channel).
    Audio(usize),
    /// Index into `Song::channels` (an automation clip channel).
    Automation(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub pos: u32,
    pub len: u32,
    /// 0-based from the top.
    pub track: u16,
    pub src: ClipSrc,
    /// Ticks into the source at which the clip starts.
    pub offset: u32,
    pub muted: bool,
    /// Clips that share a number other than 0 are grouped and are picked and moved as one.
    pub group: u16,
    /// A clip of sound starts this much further into its recording than `offset` says, in milliseconds of the
    /// recording (less than a tick's worth; files keep the start in milliseconds).
    pub fine: f32,
    /// Fades and level; only clips of sound use it.
    pub fx: ClipFx,
}

/// How many milliseconds of a channel's recording pass in one tick of a clip of it, at a tempo. Files keep the
/// start and end of a clip of sound in these (seen across every project looked at, and measured).
pub fn clip_ms_per_tick(ch: &Chan, tempo: f32, ppq: u32) -> f32 {
    let line = 60_000.0 / tempo.max(1.0) / ppq.max(1) as f32;
    let f = &ch.smp;
    let mul = f.stretch_mul.max(1e-6);
    if f.stretch_beats > 0.0 {
        match &ch.sample {
            Some(s) => s.seconds() * 1000.0 / (f.stretch_beats * mul * ppq.max(1) as f32),
            None => line,
        }
    } else if f.keep_pitch {
        line / mul
    } else {
        line * 2f32.powf((ch.pitch_cents + f.stretch_cents) / 1200.0) / mul
    }
}

pub use crate::flp::ClipFx;

/// How many of the file's units of stretched length make a beat. (Clips that show the whole of a stretched
/// sound are this long in every project looked at.)
pub const STRETCH_UNITS: f32 = 192.0;

#[derive(Debug, Clone)]
pub struct Track {
    pub name: String,
    pub color: Color,
    pub enabled: bool,
    pub perf: Perf,
    /// Joined to the track above it (they are muted and triggered as one).
    pub grouped: bool,
    /// Locked to its content: drawing on the track puts down the clip that already lies on it.
    pub lock_content: bool,
    /// Height on the playlist, 1 being the usual one.
    pub size: f32,
}

impl Default for Track {
    fn default() -> Self {
        Self { name: String::new(), color: [72, 81, 86], enabled: true, perf: Perf::default(), grouped: false, lock_content: false, size: 1.0 }
    }
}

/// How a playlist track behaves when its clips are played by hand in performance mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perf {
    /// 0 retrigger, 1 hold and stop, 2 hold and motion, 3 latch.
    pub press: u8,
    /// What follows a clip that has played through: 0 stay, 1 one shot, 2 march and wrap, 3 march and
    /// stay, 4 march and stop, 5 random, 6 random without repeats.
    pub motion: u8,
    /// The grid a trigger waits for: 0 off, 1 quarter beat, 2 half beat, 3 beat, 4 two beats, 5 four beats,
    /// 6 automatic (the clip's own length, four beats at most).
    pub sync: u8,
    /// Where in the clip a trigger starts: 0 its beginning, 1..=5 the grid as above, 6 the song position.
    pub pos_sync: u8,
    pub queue: bool,
    /// A trigger that comes a little late starts at once, inside the clip, in place of waiting a whole grid step.
    pub tolerant: bool,
}

impl Default for Perf {
    fn default() -> Self {
        Self { press: 0, motion: 0, sync: 5, pos_sync: 0, queue: false, tolerant: true }
    }
}

pub const PERF_PRESS: [&str; 4] = ["Retrigger", "Hold & stop", "Hold & motion", "Latch"];
pub const PERF_MOTION: [&str; 7] = ["Stay", "One shot", "March & wrap", "March & stay", "March & stop", "Random", "Exclusive random"];
pub const PERF_SYNC: [&str; 7] = ["Off", "1/4 beat", "1/2 beat", "1 beat", "2 beats", "4 beats", "Auto"];

#[derive(Debug, Clone, Default)]
pub struct Slot {
    pub index: u16,
    pub plugin: String,
    pub name: String,
    pub enabled: bool,
    /// Dry/wet, 0..=1.
    pub mix: f32,
    /// The effect's settings when we have a unit for it; None passes the sound through untouched.
    pub fx: Option<crate::fx::Fx>,
    /// Where the slot sits in the file the project came from (insert, slot); None for one added since.
    pub src: Option<(u16, u16)>,
}

static STAND_IN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether channels whose plug-in is not on this machine play a voice of our own (from the next project opened).
pub fn set_stand_in_missing(on: bool) {
    STAND_IN.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn stand_in_missing() -> bool {
    STAND_IN.load(std::sync::atomic::Ordering::Relaxed)
}

/// A patch bay standing in a channel as its instrument.
#[derive(Debug, Clone)]
pub struct Bay {
    /// The bay itself, with the first instrument's cables starting at the bay's input.
    pub fx: crate::fx::Fx,
    /// Which held unit the channel's own instrument is.
    pub lead: usize,
    /// The other held instruments that play the same notes.
    pub layers: crate::fxpatch::Layers,
}

/// The stored identifier of the unit that holds a plug-in from another maker.
pub const WRAPPER: &str = "Fruity Wrapper";

/// The stored identifier of the patch bay.
pub const BAY: &str = "Patcher";

impl Slot {
    /// A slot running a plug-in from another maker.
    pub fn hosted(index: u16, inst: Arc<crate::host::Instance>) -> Self {
        let name = inst.found.name.clone();
        let fx = crate::fx::Fx { ext: Some(crate::host::Hosted(inst)), ..crate::fx::Fx::new(crate::fx::Kind::Hosted) };
        Self { index, plugin: WRAPPER.to_string(), name, enabled: true, mix: 1.0, fx: Some(fx), src: None }
    }
    /// A new slot holding one of our units at its default settings.
    pub fn with_fx(index: u16, kind: crate::fx::Kind) -> Self {
        let d = crate::fx::def(kind);
        Self { index, plugin: d.ident.to_string(), name: String::new(), enabled: true, mix: 1.0, fx: Some(crate::fx::Fx::new(kind)), src: None }
    }
    /// What to call the slot on screen: the user's own name when they gave one, else our name for the unit.
    pub fn title(&self) -> String {
        // a wrapped unit has the wrapper's identifier, so its title comes from the unit itself
        if let Some(h) = self.fx.as_ref().and_then(|f| f.ext.as_ref()) {
            return if self.name.is_empty() || self.name == self.plugin { h.0.found.name.clone() } else { self.name.clone() };
        }
        let own = crate::fx::title_for(&self.plugin).or_else(|| self.fx.as_ref().map(|f| crate::fx::def(f.kind).title));
        match own {
            Some(t) if self.name.is_empty() || self.name == self.plugin => t.to_string(),
            _ if self.name.is_empty() => self.plugin.clone(),
            _ => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Insert {
    pub name: String,
    pub color: Option<Color>,
    /// File value / 12800; 1.0 = 0 dB.
    pub volume: f32,
    pub pan: f32,
    pub enabled: bool,
    /// Inserts this one feeds. Empty on the master.
    pub sends: Vec<usize>,
    pub slots: Vec<Slot>,
    /// Stereo separation, -1 (wider) ..= 1 (mono).
    pub sep: f32,
    /// The built-in equaliser: low shelf, mid peak, high shelf.
    pub eq: [Band; 3],
    /// Send level per destination insert (index = destination), 1.0 = 100 %. Missing entries are 1.0.
    pub levels: Vec<f32>,
    pub invert: bool,
    pub swap: bool,
    /// The sound card input heard on this track: -1 for none, else stereo pairs first and then single inputs.
    pub audio_in: i32,
    /// Where the strip stands in the mixer: 0 held at the left, 1 in the scrolling middle, 2 held at the right.
    pub dock: u8,
    /// A gap drawn after the strip.
    pub separator: bool,
    /// The effect slots as a whole, on or off.
    pub fx_on: bool,
    /// How late the track is said to be by hand, in milliseconds, on top of what its plug-ins report.
    pub latency_ms: f32,
    /// Projects of earlier versions: the figure is a hold the track really has (heard when the lining-up is off,
    /// made up for when it is on), not a statement about its plug-ins.
    pub latency_real: bool,
}

/// One band of an insert's equaliser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub gain_db: f32,
    /// 0..=1 of the frequency knob.
    pub freq: f32,
    /// 0..=1 of the width knob.
    pub q: f32,
}

impl Band {
    /// Where the band stands (measured on noise through the reference: about 10 Hz to 16 kHz).
    pub fn hz(&self) -> f32 {
        crate::fm::tone_hz(self.freq)
    }
    /// How sharp the band is; `band` is 0 for the low shelf, 1 for the peak, 2 for the high shelf.
    pub fn sharp(&self, band: usize) -> f32 {
        crate::fm::tone_q(if band == 1 { 1 } else { 0 }, self.q)
    }
}

pub const EQ_DEFAULT: [Band; 3] = [
    Band { gain_db: 0.0, freq: 5777.0 / 65536.0, q: 17500.0 / 65536.0 },
    Band { gain_db: 0.0, freq: 33145.0 / 65536.0, q: 17500.0 / 65536.0 },
    Band { gain_db: 0.0, freq: 55825.0 / 65536.0, q: 17500.0 / 65536.0 },
];

impl Insert {
    pub fn send_level(&self, dest: usize) -> f32 {
        self.levels.get(dest).copied().unwrap_or(1.0)
    }
    pub fn set_send_level(&mut self, dest: usize, v: f32) {
        if self.levels.len() <= dest {
            self.levels.resize(dest + 1, 1.0);
        }
        self.levels[dest] = v;
    }
}

impl Default for Insert {
    fn default() -> Self {
        Self { name: String::new(), color: None, volume: 1.0, pan: 0.0, enabled: true, sends: vec![], slots: vec![], sep: 0.0, eq: EQ_DEFAULT, levels: vec![], invert: false, swap: false, audio_in: -1, dock: 1, separator: false, fx_on: true, latency_ms: 0.0, latency_real: false }
    }
}

/// A named point on the playlist's time line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimeMarker {
    pub pos: u32,
    pub name: String,
    /// 0 is a plain marker, 8 a time signature change; other kinds are kept as they arrive.
    pub kind: u8,
    pub sig: (u8, u8),
}

/// One of the project's playlists. The one on show lives in the song's own `clips`, `tracks` and `markers`;
/// its entry here keeps only the name until another arrangement is turned to.
#[derive(Debug, Clone, Default)]
pub struct Arrangement {
    pub name: String,
    pub clips: Vec<Clip>,
    pub tracks: Vec<Track>,
    pub markers: Vec<TimeMarker>,
    /// Which arrangement of the opened file this one is, counted in file order.
    pub src: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub comment: String,
    pub ppq: u32,
    pub tempo: f32,
    pub time_sig: (u8, u8),
    /// The playlist's clips ahead of the start marker are played by hand.
    pub performance: bool,
    /// Clips that start part-way into their pattern sound the notes already running at that point.
    pub truncated: bool,
    /// Late plug-ins are lined up with everything else; a project that does not say so plays them as they come.
    pub auto_pdc: bool,
    /// The panning law: 0 circular, 2 triangular.
    pub pan_law: u8,
    /// Main pitch: every instrument is moved by this many cents.
    pub main_pitch: i16,
    /// The transport plays the whole song, not one pattern: stored with the project.
    pub song_mode: bool,
    /// Project swing, 0..=1.
    pub swing: f32,
    pub channels: Vec<Chan>,
    /// patterns[0] is "Pattern 1".
    pub patterns: Vec<Pattern>,
    pub tracks: Vec<Track>,
    /// The channel rack's named groups, in name order, and the one on show (None for all).
    pub groups: Vec<String>,
    pub cur_group: Option<usize>,
    /// Kept sorted by position.
    pub clips: Vec<Clip>,
    /// Kept sorted by position.
    pub markers: Vec<TimeMarker>,
    /// Every playlist of the project, and which one is on show.
    pub arrangements: Vec<Arrangement>,
    pub cur_arr: usize,
    /// inserts[0] is the master.
    pub inserts: Vec<Insert>,
    /// Order in which the mixer processes inserts so every sender runs before what it feeds.
    /// Where mixer inserts are numbered from in control addresses (depends on the version that saved the file).
    pub mixer_base: usize,
    pub mix_order: Vec<usize>,
    /// Automation clips that drive something, with their playlist positions (built by `prepare`).
    pub auto_plan: Vec<AutoPlan>,
    /// Controls driven by a controller unit sitting in an effect slot.
    pub ctl_links: Vec<CtlLink>,
    /// Controls driven by a knob or slider of a MIDI device.
    pub midi_links: Vec<MidiLink>,
    /// Mapping formulas of links: what arrives at the control is the formula's answer for what was sent.
    pub link_maps: Vec<LinkMap>,
}

/// The mapping formula of whatever drives one control.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkMap {
    pub target: Target,
    pub text: String,
    f: Option<crate::formula::Formula>,
}

impl LinkMap {
    pub fn new(target: Target, text: &str) -> Self {
        Self { target, text: text.to_string(), f: crate::formula::compile(text).ok() }
    }

    pub fn run(&self, v: f32) -> f32 {
        match &self.f {
            Some(f) => {
                let out = f.value(&crate::formula::Vars { a: v as f64, ..Default::default() });
                if out.is_finite() { out.clamp(0.0, 1.0) as f32 } else { v }
            }
            None => v,
        }
    }
}

/// One control tied to a controller number of a MIDI device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiLink {
    pub ctrl: u16,
    pub port: u16,
    pub target: Target,
}

/// One control driven by an output of a controller unit (insert, slot, which output).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CtlLink {
    pub ins: usize,
    pub slot: u16,
    pub out: u16,
    pub target: Target,
}

impl CtlLink {
    /// The `ins` of a link whose sender is a channel of the rack; `slot` then holds the channel.
    pub const CHAN: usize = usize::MAX;

    pub fn from_chan(chan: usize, out: u16, target: Target) -> Self {
        Self { ins: Self::CHAN, slot: chan as u16, out, target }
    }

    pub fn chan(&self) -> Option<usize> {
        (self.ins == Self::CHAN).then_some(self.slot as usize)
    }
}

pub const DEFAULT_TRACKS: usize = 500;
pub const DEFAULT_INSERTS: usize = 127;

fn color(v: u32) -> Color {
    [v as u8, (v >> 8) as u8, (v >> 16) as u8]
}

impl Song {
    /// Every hosted plug-in the song holds, on channels and in effect slots.
    pub fn hosted(&self) -> Vec<Arc<crate::host::Instance>> {
        let chans = self.channels.iter().filter_map(|c| c.ext.as_ref());
        let slots = self.inserts.iter().flat_map(|i| i.slots.iter()).filter_map(|s| s.fx.as_ref().and_then(|f| f.ext.as_ref()));
        chans.chain(slots).map(|h| h.0.clone()).collect()
    }

    /// Get every hosted plug-in ready to run at this rate. Call from the window thread before the song is played.
    pub fn activate_hosted(&self, rate: f32) {
        for h in self.hosted() {
            h.activate(rate);
        }
    }
    /// A new, empty project: four empty sampler channels, one pattern, the default mixer.
    pub fn new() -> Self {
        let names = ["Kick", "Clap", "Hat", "Snare"];
        // the last track is the "current" one: it hears whatever is selected, so it must not feed the master as well
        let plain = |i: usize| i != 0 && i != DEFAULT_INSERTS - 1;
        let mut inserts: Vec<Insert> = (0..DEFAULT_INSERTS).map(|i| Insert { sends: if plain(i) { vec![0] } else { vec![] }, dock: plain(i) as u8, ..Default::default() }).collect();
        inserts[0].name = "Master".into();
        let mut s = Self {
            title: String::new(),
            artist: String::new(),
            genre: String::new(),
            comment: String::new(),
            ppq: 96,
            tempo: 130.0,
            time_sig: (4, 4),
            performance: false,
            truncated: true,
            auto_pdc: true,
            pan_law: 0,
            main_pitch: 0,
            song_mode: false,
            swing: 0.0,
            channels: names.iter().map(|n| Chan { name: n.to_string(), ..Default::default() }).collect(),
            patterns: vec![Pattern::default()],
            tracks: vec![Track::default(); DEFAULT_TRACKS],
            groups: vec!["Unsorted".into()],
            cur_group: None,
            clips: vec![],
            markers: vec![],
            inserts,
            mixer_base: 448,
            mix_order: vec![],
            auto_plan: vec![],
            arrangements: vec![Arrangement { name: "Arrangement".into(), src: None, ..Default::default() }],
            cur_arr: 0,
            ctl_links: vec![],
            midi_links: vec![],
            link_maps: vec![],
        };
        s.prepare();
        s
    }

    /// Ticks in one bar.
    pub fn bar(&self) -> u32 {
        (self.beat_ticks() * self.time_sig.0.max(1) as u32).max(1)
    }
    /// Ticks in one beat of the tempo. The project's second time signature number counts steps in a beat, a step
    /// being a quarter of the file's time base, so 4/8 has beats twice as long as 4/4 and plays twice as fast.
    pub fn beat_ticks(&self) -> u32 {
        (self.ppq * self.time_sig.1.max(1) as u32 / 4).max(1)
    }
    /// The stretches of the time line that each keep one time signature: where it starts, the
    /// number of its first bar (from 0) and the ticks in one of its bars. Time signature markers
    /// start a new bar where they stand.
    pub fn bar_spans(&self) -> Vec<(u32, u32, u32)> {
        let mut out = vec![(0u32, 0u32, self.bar())];
        let mut sigs: Vec<&TimeMarker> = self.markers.iter().filter(|m| m.kind == 8 && m.sig.0 > 0 && m.sig.1 > 0).collect();
        sigs.sort_by_key(|m| m.pos);
        for m in sigs {
            let len = (self.ppq * 4 * m.sig.0 as u32 / m.sig.1 as u32).max(1);
            let (start, number, was) = *out.last().unwrap();
            if m.pos == start {
                out.last_mut().unwrap().2 = len;
            } else {
                out.push((m.pos, number + (m.pos - start).div_ceil(was), len));
            }
        }
        out
    }
    /// The time signature in force at a tick of the song: where it took hold, its bar number there, and the signature.
    pub fn sig_at(&self, tick: u32) -> (u32, u32, (u8, u8)) {
        let mut out = (0u32, 0u32, (self.time_sig.0.max(1), self.time_sig.1.max(1)));
        let mut sigs: Vec<&TimeMarker> = self.markers.iter().filter(|m| m.kind == 8 && m.sig.0 > 0 && m.sig.1 > 0 && m.pos <= tick).collect();
        sigs.sort_by_key(|m| m.pos);
        let mut was = self.bar();
        for m in sigs {
            out = (m.pos, out.1 + (m.pos - out.0).div_ceil(was), m.sig);
            was = (self.ppq * 4 * m.sig.0 as u32 / m.sig.1 as u32).max(1);
        }
        out
    }
    /// The next signature change after a tick, if there is one.
    pub fn next_sig(&self, tick: u32) -> Option<u32> {
        self.markers.iter().filter(|m| m.kind == 8 && m.sig.0 > 0 && m.sig.1 > 0 && m.pos > tick).map(|m| m.pos).min()
    }
    /// The bar lines from the one at or before `from` up to `to`: tick, bar number from 0, ticks to the next line.
    pub fn bar_lines(&self, from: u32, to: u32) -> Vec<(u32, u32, u32)> {
        let spans = self.bar_spans();
        let mut out = vec![];
        for (i, &(start, number, len)) in spans.iter().enumerate() {
            let end = spans.get(i + 1).map(|s| s.0).unwrap_or(u32::MAX);
            if end <= from {
                continue;
            }
            if start > to {
                break;
            }
            let mut k = from.saturating_sub(start) / len;
            loop {
                let t = start as u64 + k as u64 * len as u64;
                if t >= end as u64 || t > to as u64 || out.len() > 20_000 {
                    break;
                }
                let t = t as u32;
                out.push((t, number + k, len.min(end - t)));
                k += 1;
            }
        }
        out
    }
    /// Turn to another arrangement: the playlist on show is put away and the chosen one takes its place.
    pub fn set_arrangement(&mut self, to: usize) {
        if to == self.cur_arr || to >= self.arrangements.len() {
            return;
        }
        let was = self.cur_arr.min(self.arrangements.len() - 1);
        let a = &mut self.arrangements[was];
        a.clips = std::mem::take(&mut self.clips);
        a.tracks = std::mem::take(&mut self.tracks);
        a.markers = std::mem::take(&mut self.markers);
        let b = &mut self.arrangements[to];
        self.clips = std::mem::take(&mut b.clips);
        self.tracks = std::mem::take(&mut b.tracks);
        self.markers = std::mem::take(&mut b.markers);
        self.cur_arr = to;
        self.prepare();
    }

    /// A new arrangement after the others: empty, or a copy of the one on show. It becomes the one on show.
    pub fn add_arrangement(&mut self, name: &str, copy: bool) {
        let a = Arrangement {
            name: name.to_string(),
            clips: if copy { self.clips.clone() } else { vec![] },
            tracks: if copy { self.tracks.clone() } else { vec![Track::default(); self.tracks.len().max(DEFAULT_TRACKS)] },
            markers: if copy { self.markers.clone() } else { vec![] },
            src: None,
        };
        self.arrangements.push(a);
        self.set_arrangement(self.arrangements.len() - 1);
    }

    /// Take an arrangement out of the project; the last one stays.
    pub fn remove_arrangement(&mut self, i: usize) {
        if self.arrangements.len() < 2 || i >= self.arrangements.len() {
            return;
        }
        if i == self.cur_arr {
            self.set_arrangement(if i == 0 { 1 } else { i - 1 });
        }
        self.arrangements.remove(i);
        if self.cur_arr > i {
            self.cur_arr -= 1;
        }
    }

    /// The clips of every arrangement, the one on show first.
    pub fn all_clips(&self) -> impl Iterator<Item = &Clip> {
        self.clips.iter().chain(self.arrangements.iter().flat_map(|a| a.clips.iter()))
    }

    /// The clips, tracks and markers of arrangement `i`, wherever they are kept just now.
    pub fn arrangement(&self, i: usize) -> (&[Clip], &[Track], &[TimeMarker]) {
        match self.arrangements.get(i) {
            Some(a) if i != self.cur_arr => (&a.clips, &a.tracks, &a.markers),
            _ => (&self.clips, &self.tracks, &self.markers),
        }
    }

    /// Change the timebase: every position and length in the project is counted afresh in ticks of the new
    /// resolution, so the song sounds as before on a finer or coarser grid.
    pub fn set_ppq(&mut self, ppq: u32) {
        let (new, old) = (ppq.clamp(24, 960) as u64, self.ppq.max(1) as u64);
        if new == old {
            return;
        }
        let m = |t: u32| ((t as u64 * new + old / 2) / old) as u32;
        let len = |t: u32| if t == 0 { 0 } else { m(t).max(1) };
        for p in &mut self.patterns {
            for n in &mut p.notes {
                let end = m(n.pos + n.len);
                n.pos = m(n.pos);
                n.len = if n.len == 0 { 0 } else { end.saturating_sub(n.pos).max(1) };
            }
            p.events.iter_mut().for_each(|e| e.tick = m(e.tick));
            p.markers.iter_mut().for_each(|k| k.pos = m(k.pos));
        }
        // a clip of sound starts at a place in its recording, which is counted out again in the new ticks
        let per: Vec<(f32, f32)> = self.channels.iter().map(|ch| (clip_ms_per_tick(ch, self.tempo, old as u32), clip_ms_per_tick(ch, self.tempo, new as u32))).collect();
        for c in self.clips.iter_mut().chain(self.arrangements.iter_mut().flat_map(|a| a.clips.iter_mut())) {
            let end = m(c.pos + c.len);
            c.pos = m(c.pos);
            c.len = end.saturating_sub(c.pos).max(1);
            match (c.src, c.offset > 0 || c.fine > 0.0) {
                (ClipSrc::Audio(ch), true) if per.get(ch).is_some_and(|p| p.0 > 0.0 && p.1 > 0.0) => {
                    let (was, now) = per[ch];
                    let ms = c.offset as f32 * was + c.fine;
                    let ticks = (ms / now + 1e-3).floor().clamp(0.0, 4.0e9);
                    c.offset = ticks as u32;
                    c.fine = (ms - ticks * now).max(0.0);
                }
                _ => c.offset = len(c.offset),
            }
        }
        self.markers.iter_mut().chain(self.arrangements.iter_mut().flat_map(|a| a.markers.iter_mut())).for_each(|k| k.pos = m(k.pos));
        for ch in &mut self.channels {
            if let Some(a) = ch.auto.as_mut() {
                Arc::make_mut(a).points.iter_mut().for_each(|p| p.tick = p.tick * new as f64 / old as f64);
            }
        }
        self.ppq = new as u32;
        self.prepare();
    }

    /// Ticks in one step (a sixteenth).
    pub fn step(&self) -> u32 {
        (self.ppq / 4).max(1)
    }
    /// Playlist length in ticks, rounded up to a whole bar (at least one).
    pub fn length(&self) -> u32 {
        let bar = self.bar();
        let end = self.clips.iter().map(|c| c.pos + c.len).max().unwrap_or(0);
        end.div_ceil(bar).max(1) * bar
    }
    /// The group new channels land in: the one on show, else the catch-all.
    pub fn default_group(&self) -> usize {
        self.cur_group.filter(|g| *g < self.groups.len()).or_else(|| self.groups.iter().position(|g| g == "Unsorted")).unwrap_or(0)
    }
    pub fn group_of(&self, chan: usize) -> usize {
        match self.channels.get(chan) {
            Some(c) if c.group < self.groups.len() => c.group,
            _ => self.default_group(),
        }
    }
    /// Whether the rack shows this channel with the group that is on show.
    pub fn in_view(&self, chan: usize) -> bool {
        self.cur_group.filter(|g| *g < self.groups.len()).is_none_or(|g| self.group_of(chan) == g)
    }
    fn group_members(&self, g: usize) -> Vec<usize> {
        (0..self.channels.len()).filter(|&i| self.group_of(i) == g).collect()
    }
    /// Give a rack group another name (taking the name of another group joins the two). Returns where it now is.
    pub fn rename_group(&mut self, g: usize, name: &str) -> Option<usize> {
        if g >= self.groups.len() || name.is_empty() {
            return None;
        }
        let (members, shown) = (self.group_members(g), self.cur_group == Some(g));
        if members.is_empty() {
            self.groups[g] = name.to_string();
            return Some(g);
        }
        let at = self.group_channels(&members, name);
        if shown {
            self.cur_group = Some(at);
        }
        Some(at)
    }
    /// Take a rack group away; its channels go to the catch-all group.
    pub fn delete_group(&mut self, g: usize) {
        if g >= self.groups.len() || self.groups[g] == "Unsorted" {
            return;
        }
        let members = self.group_members(g);
        self.cur_group = None;
        if members.is_empty() {
            self.groups.remove(g);
            for c in &mut self.channels {
                if c.group > g {
                    c.group -= 1;
                }
            }
        } else {
            self.group_channels(&members, "Unsorted");
        }
    }
    /// File the given channels under a group of this name, made if it is not there yet; the list stays in name order.
    pub fn group_channels(&mut self, chans: &[usize], name: &str) -> usize {
        // settle everyone's group first, so adding a name cannot move a channel that had none
        for i in 0..self.channels.len() {
            self.channels[i].group = self.group_of(i);
        }
        let at = match self.groups.iter().position(|g| g == name) {
            Some(i) => i,
            None => {
                let at = self.groups.iter().position(|g| g.to_lowercase().as_str() > name.to_lowercase().as_str()).unwrap_or(self.groups.len());
                self.groups.insert(at, name.to_string());
                for c in &mut self.channels {
                    if c.group >= at {
                        c.group += 1;
                    }
                }
                if let Some(g) = self.cur_group.as_mut().filter(|g| **g >= at) {
                    *g += 1;
                }
                at
            }
        };
        for &c in chans {
            if let Some(c) = self.channels.get_mut(c) {
                c.group = at;
            }
        }
        // a group nobody is in goes away, except the catch-all
        let mut g = 0;
        while g < self.groups.len() {
            if self.groups[g] != "Unsorted" && g != at && !self.channels.iter().any(|c| c.group == g) {
                self.groups.remove(g);
                for c in &mut self.channels {
                    if c.group > g {
                        c.group -= 1;
                    }
                }
                self.cur_group = match self.cur_group {
                    Some(x) if x == g => None,
                    Some(x) if x > g => Some(x - 1),
                    x => x,
                };
                if at > g {
                    return self.groups.iter().position(|n| n == name).unwrap_or(0);
                }
            } else {
                g += 1;
            }
        }
        at
    }
    /// Two playlist tracks change places, clips and all.
    pub fn swap_tracks(&mut self, a: usize, b: usize) {
        if a == b || a >= self.tracks.len() || b >= self.tracks.len() {
            return;
        }
        self.tracks.swap(a, b);
        for c in &mut self.clips {
            if c.track as usize == a {
                c.track = b as u16;
            } else if c.track as usize == b {
                c.track = a as u16;
            }
        }
    }
    /// Where the performance zone ends (the start marker) when the playlist is in performance mode: the clips
    /// ahead of it are played by hand, the song itself runs from it.
    pub fn perf_start(&self) -> Option<u32> {
        if !self.performance {
            return None;
        }
        self.markers.iter().filter(|m| m.kind == 5).map(|m| m.pos).max().filter(|&p| p > 0)
    }
    /// Where the song goes back to when it reaches its end: the loop marker if there is one, else the start marker.
    pub fn loop_start(&self) -> u32 {
        let of = |kind: u8| self.markers.iter().rev().find(|m| m.kind == kind).map(|m| m.pos);
        of(4).or(of(5)).unwrap_or(0)
    }
    /// The next time marker action after `from`: (where it fires, kind, where play goes for the jumping kinds).
    /// Kinds: 1 loop back to the marker when the next one is reached, 2 skip to the next marker, 3 pause,
    /// 6 and 7 punch recording in and out.
    pub fn marker_action(&self, from: f64) -> Option<(f64, u8, f64)> {
        let mut ms: Vec<(u32, u8)> = self.markers.iter().map(|m| (m.pos, m.kind)).collect();
        ms.sort();
        let end = self.length();
        let mut best: Option<(f64, u8, f64)> = None;
        let mut take = |c: (f64, u8, f64)| {
            if c.0 > from && best.map(|b| c.0 < b.0).unwrap_or(true) {
                best = Some(c);
            }
        };
        for (i, &(pos, kind)) in ms.iter().enumerate() {
            let next = ms[i + 1..].iter().find(|m| m.0 > pos).map(|m| m.0).unwrap_or(end);
            match kind {
                1 if pos as f64 <= from && (next as f64) > from => take((next as f64, 1, pos as f64)),
                2 if next > pos => take((pos as f64, 2, next as f64)),
                3 | 6 | 7 => take((pos as f64, kind, pos as f64)),
                _ => {}
            }
        }
        best
    }
    /// Every sampler and instrument channel becomes a MIDI sender on port 0, each on the next MIDI channel
    /// (the drum channel is passed over), so the song can be written as a MIDI file or played on outside
    /// gear. Names, colours, levels and notes stay. Returns how many channels were changed.
    pub fn prepare_midi_export(&mut self) -> usize {
        let mut n = 0;
        for c in self.channels.iter_mut() {
            let is_sender = c.plugin.eq_ignore_ascii_case(crate::midiout::IDENT);
            if !matches!(c.kind, ChannelKind::Sampler | ChannelKind::Plugin) || is_sender {
                continue;
            }
            let mut m = Chan::instrument(crate::synth::Kind::Quiet);
            m.plugin = crate::midiout::IDENT.into();
            let mut set = crate::midiout::Set::fresh();
            set.chan = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15][n % 15];
            if let Some(g) = m.gen.as_mut() {
                g.raw = set.write();
            }
            m.name = std::mem::take(&mut c.name);
            (m.color, m.enabled, m.volume, m.pan, m.group, m.insert) = (c.color, c.enabled, c.volume, c.pan, c.group, c.insert);
            *c = m;
            n += 1;
        }
        n
    }

    pub fn pattern_length(&self, p: usize) -> u32 {
        self.patterns.get(p).map(|p| p.length(self.bar())).unwrap_or(self.bar())
    }
    pub fn pattern_name(&self, p: usize) -> String {
        match self.patterns.get(p) {
            Some(pat) if !pat.name.is_empty() => pat.name.clone(),
            _ => format!("Pattern {}", p + 1),
        }
    }
    pub fn insert_name(&self, i: usize) -> String {
        match self.inserts.get(i) {
            Some(ins) if !ins.name.is_empty() => ins.name.clone(),
            _ if i == 0 => "Master".into(),
            _ => format!("Insert {i}"),
        }
    }
    pub fn ensure_pattern(&mut self, p: usize) {
        while self.patterns.len() <= p {
            self.patterns.push(Pattern::default());
        }
    }

    /// Remove a pattern; its clips go with it and later patterns move up one.
    pub fn remove_pattern(&mut self, pi: usize) {
        if pi >= self.patterns.len() {
            return;
        }
        self.patterns.remove(pi);
        if self.patterns.is_empty() {
            self.patterns.push(Pattern::default());
        }
        self.clips.retain(|c| c.src != ClipSrc::Pattern(pi));
        self.arrangements.iter_mut().for_each(|a| a.clips.retain(|c| c.src != ClipSrc::Pattern(pi)));
        for c in self.clips.iter_mut().chain(self.arrangements.iter_mut().flat_map(|a| a.clips.iter_mut())) {
            if let ClipSrc::Pattern(k) = &mut c.src {
                if *k > pi {
                    *k -= 1;
                }
            }
        }
    }

    /// Swap a pattern with the one above or below it; the clips follow. Returns where it now sits.
    pub fn move_pattern(&mut self, pi: usize, up: bool) -> usize {
        let to = if up { pi.checked_sub(1) } else { Some(pi + 1) };
        let Some(to) = to.filter(|t| pi < self.patterns.len() && *t < self.patterns.len()) else { return pi };
        self.patterns.swap(pi, to);
        for c in self.clips.iter_mut().chain(self.arrangements.iter_mut().flat_map(|a| a.clips.iter_mut())) {
            if let ClipSrc::Pattern(k) = &mut c.src {
                if *k == pi {
                    *k = to;
                } else if *k == to {
                    *k = pi;
                }
            }
        }
        to
    }

    /// Give every channel playing in a pattern a pattern of its own: the first keeps the pattern, the others
    /// move to new ones, placed on the playlist wherever the pattern is. Returns the new patterns.
    pub fn split_pattern(&mut self, pi: usize) -> Vec<usize> {
        let Some(p) = self.patterns.get(pi) else { return vec![] };
        let mut chans: Vec<u16> = p.notes.iter().map(|n| n.chan).collect();
        chans.sort_unstable();
        chans.dedup();
        if chans.len() < 2 {
            return vec![];
        }
        let base = self.pattern_name(pi);
        let name = |s: &Self, c: u16| format!("{base} - {}", s.channels.get(c as usize).map(|c| c.name.as_str()).unwrap_or("?"));
        let len = self.pattern_length(pi);
        let all = std::mem::take(&mut self.patterns[pi].notes);
        let placed: Vec<Clip> = self.clips.iter().filter(|c| c.src == ClipSrc::Pattern(pi)).cloned().collect();
        let mut made = Vec::new();
        for (k, &ch) in chans.iter().enumerate() {
            let notes: Vec<Note> = all.iter().filter(|n| n.chan == ch).cloned().collect();
            let at = if k == 0 {
                pi
            } else {
                self.patterns.push(Pattern { color: self.patterns[pi].color, ..Default::default() });
                made.push(self.patterns.len() - 1);
                self.patterns.len() - 1
            };
            self.patterns[at].notes = notes;
            self.patterns[at].name = name(self, ch);
            if k > 0 {
                // the song sounds as before: the new pattern lies wherever the old one does
                self.clips.extend(placed.iter().map(|c| Clip { src: ClipSrc::Pattern(at), len: if c.len == 0 { len } else { c.len }, group: 0, ..c.clone() }));
            }
        }
        made
    }

    /// Copy a pattern to the end of the list; returns the copy's index.
    pub fn clone_pattern(&mut self, pi: usize) -> usize {
        self.ensure_pattern(pi);
        let mut p = self.patterns[pi].clone();
        if !p.name.is_empty() {
            p.name = format!("{} #2", p.name);
        }
        self.patterns.push(p);
        self.patterns.len() - 1
    }

    /// The first pattern with nothing in it and nothing placed, adding one if there is none.
    pub fn next_empty_pattern(&mut self) -> usize {
        let used: std::collections::HashSet<usize> = self.all_clips().filter_map(|c| if let ClipSrc::Pattern(p) = c.src { Some(p) } else { None }).collect();
        match (0..self.patterns.len()).find(|i| self.patterns[*i].notes.is_empty() && !used.contains(i)) {
            Some(i) => i,
            None => {
                self.patterns.push(Pattern::default());
                self.patterns.len() - 1
            }
        }
    }

    /// Swap two mixer tracks, and with them everything that refers to either by number. The master stays put.
    pub fn swap_inserts(&mut self, a: usize, b: usize) {
        let n = self.inserts.len();
        if a == b || a == 0 || b == 0 || a >= n || b >= n {
            return;
        }
        let f = |k: usize| if k == a { b } else if k == b { a } else { k };
        let ft = |t: &mut Target| match t {
            Target::InsertVolume(k) | Target::InsertPan(k) | Target::SlotMix(k, _) | Target::SlotOn(k, _) | Target::SlotParam(k, _, _) => *k = f(*k),
            _ => {}
        };
        // recorded movements name their track by address: read each one, move it, write it back
        let mut moved: Vec<(usize, usize, u16, u16)> = vec![];
        for (pi, p) in self.patterns.iter().enumerate() {
            for (ei, e) in p.events.iter().enumerate() {
                let mut t = self.event_target(e.param, e.dest);
                let was = t;
                ft(&mut t);
                if t != was {
                    let (param, dest) = self.event_address(t);
                    moved.push((pi, ei, param, dest));
                }
            }
        }
        for (pi, ei, param, dest) in moved {
            let e = &mut self.patterns[pi].events[ei];
            e.param = param;
            e.dest = dest;
        }
        self.inserts.swap(a, b);
        for ins in &mut self.inserts {
            for d in &mut ins.sends {
                *d = f(*d);
            }
            ins.sends.sort_unstable();
            if ins.levels.len() <= a.max(b) && ins.levels.iter().any(|&v| v != 1.0) {
                ins.levels.resize(a.max(b) + 1, 1.0);
            }
            if ins.levels.len() > a.max(b) {
                ins.levels.swap(a, b);
            }
        }
        for c in &mut self.channels {
            c.insert = f(c.insert);
            if let Some(au) = &c.auto {
                if au.targets.iter().any(|t| { let mut u = *t; ft(&mut u); u != *t }) {
                    let mut a2 = (**au).clone();
                    a2.targets.iter_mut().for_each(ft);
                    c.auto = Some(Arc::new(a2));
                }
            }
        }
        for l in &mut self.ctl_links {
            if l.ins != CtlLink::CHAN {
                l.ins = f(l.ins);
            }
            ft(&mut l.target);
        }
        for l in &mut self.midi_links {
            ft(&mut l.target);
        }
        for l in &mut self.link_maps {
            ft(&mut l.target);
        }
    }

    /// Swap two channels of the rack, and with them everything that refers to either by number.
    pub fn swap_channels(&mut self, a: usize, b: usize) {
        if a == b || a >= self.channels.len() || b >= self.channels.len() {
            return;
        }
        let f = |k: usize| if k == a { b } else if k == b { a } else { k };
        let ft = |t: &mut Target| match t {
            Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) => *k = f(*k),
            _ => {}
        };
        self.channels.swap(a, b);
        for p in &mut self.patterns {
            for n in &mut p.notes {
                n.chan = f(n.chan as usize) as u16;
            }
            for e in &mut p.events {
                if e.dest < 0x2000 {
                    e.dest = f(e.dest as usize) as u16;
                }
            }
            p.sort();
        }
        for c in self.clips.iter_mut().chain(self.arrangements.iter_mut().flat_map(|a| a.clips.iter_mut())) {
            match &mut c.src {
                ClipSrc::Audio(k) | ClipSrc::Automation(k) => *k = f(*k),
                _ => {}
            }
        }
        for l in &mut self.ctl_links {
            if let Some(k) = l.chan() {
                l.slot = f(k) as u16;
            }
            ft(&mut l.target);
        }
        for l in &mut self.midi_links {
            ft(&mut l.target);
        }
        for l in &mut self.link_maps {
            ft(&mut l.target);
        }
        for ch in &mut self.channels {
            for k in &mut ch.children {
                *k = f(*k as usize) as _;
            }
            if let Some(au) = &ch.auto {
                if au.targets.iter().any(|t| matches!(t, Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k == a || *k == b)) {
                    let mut a2 = (**au).clone();
                    a2.targets.iter_mut().for_each(ft);
                    ch.auto = Some(Arc::new(a2));
                }
            }
        }
    }

    /// Remove a channel and renumber everything that refers to the ones after it.
    pub fn remove_channel(&mut self, ci: usize) {
        if ci >= self.channels.len() {
            return;
        }
        self.channels.remove(ci);
        for p in &mut self.patterns {
            p.notes.retain(|n| n.chan as usize != ci);
            for n in &mut p.notes {
                if n.chan as usize > ci {
                    n.chan -= 1;
                }
            }
            // recorded movements of the channel's own controls go with it
            p.events.retain(|e| e.dest >= 0x2000 || e.dest as usize != ci);
            for e in &mut p.events {
                if e.dest < 0x2000 && e.dest as usize > ci {
                    e.dest -= 1;
                }
            }
        }
        self.clips.retain(|c| !matches!(c.src, ClipSrc::Audio(k) | ClipSrc::Automation(k) if k == ci));
        self.arrangements.iter_mut().for_each(|a| a.clips.retain(|c| !matches!(c.src, ClipSrc::Audio(k) | ClipSrc::Automation(k) if k == ci)));
        for c in self.clips.iter_mut().chain(self.arrangements.iter_mut().flat_map(|a| a.clips.iter_mut())) {
            match &mut c.src {
                ClipSrc::Audio(k) | ClipSrc::Automation(k) if *k > ci => *k -= 1,
                _ => {}
            }
        }
        self.ctl_links.retain(|l| l.chan() != Some(ci) && !matches!(l.target, Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if k == ci));
        for l in &mut self.ctl_links {
            if l.chan().is_some_and(|k| k > ci) {
                l.slot -= 1;
            }
            match &mut l.target {
                Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k > ci => *k -= 1,
                _ => {}
            }
        }
        self.midi_links.retain(|l| !matches!(l.target, Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if k == ci));
        self.link_maps.retain(|l| !matches!(l.target, Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if k == ci));
        for l in &mut self.link_maps {
            match &mut l.target {
                Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k > ci => *k -= 1,
                _ => {}
            }
        }
        for l in &mut self.midi_links {
            match &mut l.target {
                Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k > ci => *k -= 1,
                _ => {}
            }
        }
        for ch in &mut self.channels {
            ch.children.retain(|&k| k as usize != ci);
            for k in &mut ch.children {
                if *k as usize > ci {
                    *k -= 1;
                }
            }
            if let Some(a) = &ch.auto {
                if a.targets.iter().any(|t| matches!(t, Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k >= ci)) {
                    let mut a2 = (**a).clone();
                    a2.targets.retain(|t| !matches!(t, Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k == ci));
                    for t in &mut a2.targets {
                        match t {
                            Target::ChanVolume(k) | Target::ChanPan(k) | Target::ChanPitch(k) | Target::ChanParam(k, _) if *k > ci => *k -= 1,
                            _ => {}
                        }
                    }
                    ch.auto = Some(Arc::new(a2));
                }
            }
        }
    }

    /// Sort what must be sorted and work out the mixer order. Call after editing, before publishing.
    pub fn prepare(&mut self) {
        self.clips.sort_by_key(|c| (c.pos, c.track));
        let mut plan: Vec<AutoPlan> = vec![];
        let mut slot: Vec<Option<usize>> = vec![None; self.channels.len()];
        for c in &self.clips {
            let ClipSrc::Automation(ch) = c.src else { continue };
            if c.muted || !self.tracks.get(c.track as usize).map(|t| t.enabled).unwrap_or(true) {
                continue;
            }
            let Some(chan) = self.channels.get(ch) else { continue };
            if !chan.enabled || !chan.auto.as_ref().map(|a| !a.targets.is_empty() && !a.points.is_empty()).unwrap_or(false) {
                continue;
            }
            let k = *slot[ch].get_or_insert_with(|| {
                plan.push(AutoPlan { chan: ch, clips: vec![] });
                plan.len() - 1
            });
            plan[k].clips.push((c.pos, c.len, c.offset));
        }
        self.auto_plan = plan;
        if self.inserts.is_empty() {
            self.inserts.push(Insert { name: "Master".into(), dock: 0, ..Default::default() });
        }
        let n = self.inserts.len();
        for c in &mut self.channels {
            if c.insert >= n {
                c.insert = 0;
            }
        }
        for (i, ins) in self.inserts.iter_mut().enumerate() {
            ins.sends.retain(|&d| d < n && d != i);
            if i == 0 {
                ins.sends.clear();
            }
        }
        // depth-first topological order: an insert is placed after everything that feeds it
        let mut order = Vec::with_capacity(n);
        let mut state = vec![0u8; n]; // 0 new, 1 in progress, 2 done
        let mut feeders: Vec<Vec<usize>> = vec![vec![]; n];
        for (i, ins) in self.inserts.iter().enumerate() {
            for &d in &ins.sends {
                feeders[d].push(i);
            }
        }
        fn visit(i: usize, feeders: &[Vec<usize>], state: &mut [u8], order: &mut Vec<usize>) {
            if state[i] != 0 {
                return; // done, or a feedback loop: break it here
            }
            state[i] = 1;
            for &f in &feeders[i] {
                visit(f, feeders, state, order);
            }
            state[i] = 2;
            order.push(i);
        }
        for i in 1..n {
            visit(i, &feeders, &mut state, &mut order);
        }
        visit(0, &feeders, &mut state, &mut order);
        // the master always goes last
        order.retain(|&i| i != 0);
        order.push(0);
        self.mix_order = order;
    }

    /// Build a song from a parsed project file, loading the samples it names through `lib`.
    pub fn from_flp(f: &Flp, lib: &mut Library, project_dir: Option<&Path>) -> Self {
        Self::from_flp_with(f, Some(lib), project_dir)
    }

    /// As `from_flp`; without a library no samples are loaded (enough to compare two versions of a project).
    pub fn from_flp_with(f: &Flp, mut lib: Option<&mut Library>, project_dir: Option<&Path>) -> Self {
        let ppq = f.header.ppq.max(1) as u32;
        let n_inserts = f.inserts.len().max(1);
        let n_chans = f.channels.len();

        let base = flp::mixer_base(f.version_tuple());
        let target = |l: &flp::FlpLink| -> Target {
            use flp::param::*;
            let raw = (l.dest >> 6) as usize;
            if l.dest < 0x2000 {
                match l.param {
                    CHAN_VOLUME => Target::ChanVolume(l.dest as usize),
                    CHAN_PAN => Target::ChanPan(l.dest as usize),
                    CHAN_PITCH => Target::ChanPitch(l.dest as usize),
                    p if p & PLUGIN_FLAG != 0 => Target::ChanParam(l.dest as usize, p & !PLUGIN_FLAG),
                    _ => Target::Other { param: l.param, dest: l.dest },
                }
            } else if l.dest == flp::DEST_GLOBAL {
                match l.param {
                    GLOBAL_TEMPO => Target::Tempo,
                    _ => Target::Other { param: l.param, dest: l.dest },
                }
            } else if raw >= base && raw - base < n_inserts {
                let slot = l.dest & 63;
                match l.param {
                    INSERT_VOLUME if slot == 0 => Target::InsertVolume(raw - base),
                    INSERT_PAN if slot == 0 => Target::InsertPan(raw - base),
                    SLOT_MIX => Target::SlotMix(raw - base, slot),
                    SLOT_ENABLED => Target::SlotOn(raw - base, slot),
                    p if p & PLUGIN_FLAG != 0 => Target::SlotParam(raw - base, slot, p & !PLUGIN_FLAG),
                    _ => Target::Other { param: l.param, dest: l.dest },
                }
            } else {
                Target::Other { param: l.param, dest: l.dest }
            }
        };

        let channels: Vec<Chan> = f
            .channels
            .iter()
            .enumerate()
            .map(|(pos, c)| {
                let (slices, loop_smp) = match (c.kind == ChannelKind::Plugin).then(|| crate::slicer::open(&c.plugin, &c.state, lib.as_deref_mut(), project_dir)).flatten() {
                    Some((sl, smp)) => (Some(Arc::new(sl)), smp),
                    None => (None, None),
                };
                let sample = match (c.kind, lib.as_deref_mut()) {
                    (ChannelKind::Sampler | ChannelKind::AudioClip, Some(lib)) => lib.load_stored(&c.sample_path, project_dir),
                    // the grain player works on the channel's own sample
                    (ChannelKind::Plugin, Some(lib)) if crate::synth::sample_named_in(&c.plugin, &c.state).is_some() => lib.load_stored(&crate::synth::sample_named_in(&c.plugin, &c.state).unwrap(), project_dir),
                    (ChannelKind::Plugin, Some(lib)) if c.plugin.eq_ignore_ascii_case("Fruity Granulizer") && !c.sample_path.is_empty() => lib.load_stored(&c.sample_path, project_dir),
                    _ => loop_smp,
                };
                let bank = (c.kind == ChannelKind::Plugin).then(|| crate::sf2::open(&c.plugin, &c.state, lib.as_deref_mut(), project_dir)).flatten().map(Arc::new);
                let zones = bank.is_none().then(|| (c.kind == ChannelKind::Plugin).then(|| crate::zones::Map::parse(&c.plugin, &c.state)).flatten()).flatten().map(|mut m| {
                    if let Some(lib) = lib.as_deref_mut() {
                        m.load(lib, project_dir);
                    }
                    Arc::new(m)
                });
                let mut ch = Chan {
                    name: if !c.name.is_empty() {
                        c.name.clone()
                    } else if !c.plugin.is_empty() {
                        c.plugin.clone()
                    } else {
                        // a nameless sampler goes by its sound's file name
                        let file = c.sample_path.rsplit(['/', '\\']).next().unwrap_or("");
                        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
                        if stem.is_empty() { "Sampler".into() } else { stem.to_string() }
                    },
                    kind: c.kind,
                    plugin: c.plugin.clone(),
                    sample_path: c.sample_path.clone(),
                    sample,
                    color: color(c.color),
                    enabled: c.enabled,
                    volume: c.volume as f32 / 12800.0,
                    pan: (c.pan as f32 - 6400.0) / 6400.0,
                    pitch_cents: c.pitch_cents as f32,
                    root_key: c.root_key.min(131) as u8,
                    root_fine: c.fine_tune as f32,
                    insert: if c.insert < 0 || c.insert as usize >= n_inserts { 0 } else { c.insert as usize },
                    env_on: c.envelopes.get(1).map(|e| e.enabled).unwrap_or(false),
                    src: Some(pos as u16),
                    auto: (c.kind == ChannelKind::AutomationClip).then(|| {
                        let mut tick = 0.0;
                        let points = flp::parse_automation(&c.automation)
                            .iter()
                            .map(|p| {
                                tick += p.delta * ppq as f64;
                                AutoPt { tick, value: p.value as f32, tension: p.tension, mode: p.mode }
                            })
                            .collect();
                        let targets = f.links.iter().filter(|l| l.from_clip() && l.source as usize == pos).map(target).collect();
                        Arc::new(Automation { points, targets })
                    }),
                    children: c.children.clone(),
                    group: c.rack_group as usize,
                    zipped: c.zipped,
                    smp: Sampler::from_flp(c),
                    raw: None,
                    gen: if c.kind == ChannelKind::Plugin { crate::synth::Gen::decode(&c.plugin, &c.state) } else { None },
                    bay: None,
                    slices,
                    zones: bank.or(zones),
                    ext: None,
                };
                if ch.kind == ChannelKind::Plugin && ch.gen.is_none() && ch.plugin.eq_ignore_ascii_case(WRAPPER) {
                    ch.ext = crate::host::from_block(&c.state).map(crate::host::Hosted);
                    if ch.ext.is_none() && stand_in_missing() {
                        if let Some(n) = crate::host::block_name(&c.state) {
                            ch.gen = Some(crate::synth::missing_gen(&format!("{} {n}", ch.name), &c.state));
                        }
                    }
                }
                if ch.kind == ChannelKind::Plugin && ch.gen.is_none() && ch.plugin.eq_ignore_ascii_case(BAY) {
                    if let Some((p, lead, g, layers)) = crate::fxpatch::decode_gen(&c.state) {
                        ch.gen = Some(g);
                        ch.bay = Some(Bay { fx: crate::fx::Fx { kind: crate::fx::Kind::Patch, p: vec![], ir: None, sub: Some(Arc::new(p)), raw: None, ext: None }, lead, layers });
                    }
                }
                if let Some(g) = ch.gen.as_mut().filter(|g| matches!(g.kind, crate::synth::Kind::Grain | crate::synth::Kind::Travel)) {
                    g.smp = ch.sample.clone();
                }
                ch.bake();
                ch
            })
            .collect();

        let max_pat = f
            .patterns
            .iter()
            .map(|p| p.index as usize)
            .chain(f.arrangements.iter().flat_map(|a| a.items.iter()).filter_map(|i| match i.source {
                ClipSource::Pattern(p) => Some(p as usize),
                _ => None,
            }))
            .chain(std::iter::once(f.current_pattern as usize))
            .max()
            .unwrap_or(1)
            .max(1);
        let mut patterns = vec![Pattern::default(); max_pat];
        for p in &f.patterns {
            if p.index == 0 {
                continue;
            }
            let out = &mut patterns[p.index as usize - 1];
            out.name = p.name.clone();
            out.color = p.color.map(color);
            out.events = p.ctrl.iter().map(|&(tick, param, dest, raw)| PatEvent { tick, param, dest, raw }).collect();
            out.markers = p.markers.iter().map(|m| TimeMarker { pos: m.position, name: m.name.clone(), kind: m.kind, sig: m.sig }).collect();
            out.events.sort_by_key(|e| e.tick);
            out.notes = p
                .notes
                .iter()
                .map(|n| Note {
                    pos: n.position,
                    len: n.length,
                    chan: n.rack_channel,
                    key: n.key.min(131) as u8,
                    vel: n.velocity as f32 / 100.0,
                    pan: (n.pan as f32 - 64.0) / 64.0,
                    fine_cents: (n.fine_pitch as f32 - 120.0) * 10.0,
                    muted: n.flags & 0x2000 != 0,
                    release: n.release as f32 / 128.0,
                    mod_x: (n.mod_x as f32 - 128.0) / 128.0,
                    mod_y: (n.mod_y as f32 - 128.0) / 128.0,
                    slide: n.flags & 0x0008 != 0,
                    porta: n.flags & 0x0010 != 0,
                    color: n.midi_channel & 15,
                    group: n.group,
                })
                .collect();
            out.sort();
        }

        let read_arr = |a: &flp::FlpArrangement| {
        let mut tracks = vec![Track::default(); DEFAULT_TRACKS];
        let mut clips = vec![];
        {
            if !a.tracks.is_empty() {
                tracks = a.tracks.iter().map(|t| Track { name: t.name.clone(), color: color(t.color), enabled: t.enabled, perf: Perf { press: t.press.min(3) as u8, motion: t.motion.min(6) as u8, sync: t.trigger_sync.min(6) as u8, pos_sync: t.position_sync.min(6) as u8, queue: t.queued, tolerant: t.tolerant }, grouped: t.grouped, lock_content: t.content_locked, size: if t.height > 0.0 { t.height } else { 1.0 } }).collect();
            }
            for it in &a.items {
                let mut fine = 0.0;
                let (src, offset) = match it.source {
                    ClipSource::Pattern(p) => {
                        let off = it.start_offset as i32;
                        (ClipSrc::Pattern(p.max(1) as usize - 1), if off > 0 { off as u32 } else { 0 })
                    }
                    ClipSource::Channel(c) => {
                        let c = c as usize;
                        let at = f32::from_bits(it.start_offset);
                        let at = if at.is_finite() && at > 0.0 { at } else { 0.0 };
                        match f.channels.get(c).map(|c| c.kind) {
                            // an automation clip counts its start in beats
                            Some(ChannelKind::AutomationClip) => (ClipSrc::Automation(c), (at * ppq as f32) as u32),
                            // a clip of sound in milliseconds of its recording
                            _ => {
                                let per = channels.get(c).map_or(60_000.0 / f.tempo.max(1.0) / ppq as f32, |ch| clip_ms_per_tick(ch, f.tempo, ppq));
                                let ticks = (at / per + 1e-3).floor().clamp(0.0, 4.0e9);
                                fine = (at - ticks * per).max(0.0);
                                (ClipSrc::Audio(c), ticks as u32)
                            }
                        }
                    }
                };
                clips.push(Clip { pos: it.position, len: it.length, track: it.track, src, offset, muted: it.muted, group: it.group, fine, fx: it.fx });
            }
        }
        let mut markers: Vec<TimeMarker> = a.markers.iter().map(|m| TimeMarker { pos: m.position, name: m.name.clone(), kind: m.kind, sig: m.sig }).collect();
        markers.sort_by_key(|m| m.pos);
        Arrangement { name: a.name.trim_end_matches('\0').to_string(), clips, tracks, markers, src: None }
        };
        let mut arrangements: Vec<Arrangement> = f.arrangements.iter().map(read_arr).collect();
        if arrangements.is_empty() {
            arrangements.push(Arrangement { name: "Arrangement".into(), tracks: vec![Track::default(); DEFAULT_TRACKS], ..Default::default() });
        }
        arrangements.iter_mut().enumerate().for_each(|(i, a)| a.src = Some(i));
        let want = f.file.events.iter().find_map(|e| match (e.id, &e.value) {
            (flp::id::CURRENT_ARRANGEMENT, flp::Value::Word(v)) => Some(*v),
            _ => None,
        });
        let cur_arr = want.and_then(|w| f.arrangements.iter().position(|a| a.index == w)).unwrap_or(0);
        let (tracks, clips, markers) = {
            let a = &mut arrangements[cur_arr];
            (std::mem::take(&mut a.tracks), std::mem::take(&mut a.clips), std::mem::take(&mut a.markers))
        };

        let inserts: Vec<Insert> = if f.inserts.is_empty() {
            Song::new().inserts
        } else {
            f.inserts
                .iter()
                .enumerate()
                .map(|(i, ins)| {
                    let mut sends: Vec<usize> = ins.routes.iter().enumerate().filter(|(_, r)| **r).map(|(k, _)| k).collect();
                    if ins.routes.is_empty() && i != 0 {
                        sends.push(0);
                    }
                    Insert {
                        name: ins.name.clone(),
                        color: ins.color.map(color),
                        volume: ins.volume as f32 / 12800.0,
                        pan: ins.pan as f32 / 6400.0,
                        enabled: ins.flags & flp::INSERT_FLAG_ENABLED != 0 || ins.flags == 0,
                        sends,
                        slots: ins.slots.iter().map(|s| Slot { src: Some((ins.index as u16, s.index)), index: s.index, plugin: s.plugin.clone(), name: s.name.clone(), enabled: s.enabled, mix: (s.mix as f32 / 12800.0).clamp(0.0, 1.0), fx: crate::fx::Fx::decode(&s.plugin, &s.state).map(|mut fx| {
                            // a unit that names its sound gets it loaded here
                            if fx.ir.is_none() {
                                if let (Some(path), Some(lib)) = (crate::fx::sample_named_in(&s.plugin, &s.state), lib.as_deref_mut()) {
                                    fx.ir = lib.load_stored(&path, project_dir).map(|smp| crate::fx::Ir { smp, path });
                                }
                            }
                            fx
                        }) }).collect(),
                        sep: (ins.stereo_separation as f32 / 64.0).clamp(-1.0, 1.0),
                        eq: ins.eq.map(|b| Band { gain_db: b[0] as f32 / 100.0, freq: b[1] as f32 / 65536.0, q: b[2] as f32 / 65536.0 }),
                        levels: {
                            let mut l = vec![];
                            for &(d, v) in &ins.route_levels {
                                if ins.routes.get(d).copied().unwrap_or(false) && v != 12800 {
                                    if l.len() <= d {
                                        l.resize(d + 1, 1.0);
                                    }
                                    l[d] = v as f32 / 12800.0;
                                }
                            }
                            l
                        },
                        invert: ins.flags & flp::INSERT_FLAG_INVERT != 0,
                        swap: ins.flags & flp::INSERT_FLAG_SWAP != 0,
                        audio_in: ins.input,
                        dock: if ins.flags == 0 { (i != 0) as u8 } else if ins.flags & flp::INSERT_FLAG_DOCK_RIGHT != 0 { 2 } else if ins.flags & flp::INSERT_FLAG_DOCK_MIDDLE != 0 { 1 } else { 0 },
                        separator: ins.flags & flp::INSERT_FLAG_SEPARATOR != 0,
                        fx_on: ins.flags & flp::INSERT_FLAG_FX != 0 || ins.flags == 0,
                        latency_ms: ins.latency_ms,
                        latency_real: ins.latency_real,
                    }
                })
                .collect()
        };

        let mut song = Song {
            title: f.title.clone(),
            artist: f.artist.clone(),
            genre: f.genre.clone(),
            comment: f.comment.clone(),
            ppq,
            tempo: if f.tempo > 0.0 { f.tempo } else { 130.0 },
            time_sig: (f.time_sig.0.max(1), f.time_sig.1.max(1)),
            performance: f.performance,
            truncated: f.truncated,
            auto_pdc: f.auto_pdc,
            pan_law: f.pan_law,
            main_pitch: f.main_pitch,
            song_mode: f.song_mode,
            swing: (f.shuffle as f32 / 128.0).min(1.0),
            channels,
            patterns,
            tracks,
            groups: f.groups.clone(),
            cur_group: usize::try_from(f.cur_group).ok().filter(|g| *g < f.groups.len()),
            clips,
            markers,
            inserts,
            mix_order: vec![],
            auto_plan: vec![],
            arrangements,
            cur_arr,
            mixer_base: base,
            midi_links: f
                .file
                .events
                .iter()
                .filter(|e| e.id == flp::id::MIDI_LINK)
                .filter_map(|e| match &e.value {
                    flp::Value::Data(d) if d.len() >= 12 => {
                        let w = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
                        Some(MidiLink { ctrl: w(0), port: w(2), target: target(&flp::FlpLink { kind: 0, source: 0, extra: 0, param: w(8), dest: w(10) }) })
                    }
                    _ => None,
                })
                .collect(),
            link_maps: f.link_formulas.iter().filter_map(|(k, text)| f.links.get(*k).map(|l| LinkMap::new(target(l), text))).collect(),
            ctl_links: f
                .links
                .iter()
                .filter(|l| l.kind & 0x8000 != 0 && l.source >= 0x2000 && l.source != flp::DEST_GLOBAL)
                .filter_map(|l| {
                    let raw = (l.source >> 6) as usize;
                    (raw >= base && raw - base < n_inserts).then(|| CtlLink { ins: raw - base, slot: l.source & 63, out: l.kind & 0x7fff, target: target(l) })
                })
                .chain(f.links.iter().filter(|l| l.kind & 0xe000 == 0x2000 && (l.source as usize) < n_chans).map(|l| CtlLink::from_chan(l.source as usize, l.kind & 0x1fff, target(l))))
                .filter(|c| !matches!(c.target, Target::Other { .. }))
                .collect(),
        };
        // bank players: have their sounds ready before the first note asks for them
        for fx in song.inserts.iter().flat_map(|i| i.slots.iter()).filter_map(|s| s.fx.as_ref()).filter(|f| f.kind == crate::fx::Kind::Gm) {
            for mc in 0..16 {
                crate::gm::voice(crate::gm::patch_of(&fx.p, mc, -1));
            }
        }
        song.prepare();
        song
    }
}

impl Default for Song {
    fn default() -> Self {
        Self::new()
    }
}

impl Song {
    /// What a control number and destination from the file point at.
    pub fn event_target(&self, param: u16, dest: u16) -> Target {
        use flp::param::*;
        let raw = (dest >> 6) as usize;
        let other = Target::Other { param, dest };
        if dest < 0x2000 {
            match param {
                CHAN_VOLUME => Target::ChanVolume(dest as usize),
                CHAN_PAN => Target::ChanPan(dest as usize),
                CHAN_PITCH => Target::ChanPitch(dest as usize),
                p if p & PLUGIN_FLAG != 0 => Target::ChanParam(dest as usize, p & !PLUGIN_FLAG),
                _ => other,
            }
        } else if dest == flp::DEST_GLOBAL {
            if param == GLOBAL_TEMPO {
                Target::Tempo
            } else {
                other
            }
        } else if raw >= self.mixer_base && raw - self.mixer_base < self.inserts.len() {
            let (ins, slot) = (raw - self.mixer_base, dest & 63);
            match param {
                INSERT_VOLUME if slot == 0 => Target::InsertVolume(ins),
                INSERT_PAN if slot == 0 => Target::InsertPan(ins),
                SLOT_MIX => Target::SlotMix(ins, slot),
                SLOT_ENABLED => Target::SlotOn(ins, slot),
                p if p & PLUGIN_FLAG != 0 => Target::SlotParam(ins, slot, p & !PLUGIN_FLAG),
                _ => other,
            }
        } else {
            other
        }
    }

    /// The control number and destination the file uses for a target.
    pub fn event_address(&self, t: Target) -> (u16, u16) {
        use flp::param::*;
        let ins = |i: usize, slot: u16| (((i + self.mixer_base) as u16) << 6) | (slot & 63);
        match t {
            Target::ChanVolume(c) => (CHAN_VOLUME, c as u16),
            Target::ChanPan(c) => (CHAN_PAN, c as u16),
            Target::ChanPitch(c) => (CHAN_PITCH, c as u16),
            Target::InsertVolume(i) => (INSERT_VOLUME, ins(i, 0)),
            Target::InsertPan(i) => (INSERT_PAN, ins(i, 0)),
            Target::Tempo => (GLOBAL_TEMPO, flp::DEST_GLOBAL),
            Target::SlotMix(i, s) => (SLOT_MIX, ins(i, s)),
            Target::SlotOn(i, s) => (SLOT_ENABLED, ins(i, s)),
            Target::SlotParam(i, s, k) => (k | PLUGIN_FLAG, ins(i, s)),
            Target::ChanParam(c, k) => (k | PLUGIN_FLAG, c as u16),
            Target::Other { param, dest } => (param, dest),
        }
    }

    /// Put a control at a point of its travel, 0..=1, as turning it by hand would.
    pub fn set_travel(&mut self, t: Target, v: f32) {
        let v = v.clamp(0.0, 1.0);
        fn slot(s: &mut Song, i: usize, k: u16) -> Option<&mut Slot> {
            s.inserts.get_mut(i).and_then(|i| i.slots.iter_mut().find(|x| x.index == k))
        }
        match t {
            Target::ChanVolume(c) => {
                if let Some(c) = self.channels.get_mut(c) {
                    c.volume = v;
                }
            }
            Target::ChanPan(c) => {
                if let Some(c) = self.channels.get_mut(c) {
                    c.pan = v * 2.0 - 1.0;
                }
            }
            Target::ChanPitch(c) => {
                if let Some(c) = self.channels.get_mut(c) {
                    c.pitch_cents = (v - 0.5) * 400.0;
                }
            }
            Target::InsertVolume(i) => {
                if let Some(i) = self.inserts.get_mut(i) {
                    i.volume = v * 1.25;
                }
            }
            Target::InsertPan(i) => {
                if let Some(i) = self.inserts.get_mut(i) {
                    i.pan = v * 2.0 - 1.0;
                }
            }
            Target::Tempo => self.tempo = 60.0 + v * 120.0,
            Target::SlotMix(i, k) => {
                if let Some(x) = slot(self, i, k) {
                    x.mix = v;
                }
            }
            Target::SlotOn(i, k) => {
                if let Some(x) = slot(self, i, k) {
                    x.enabled = v >= 0.5;
                }
            }
            Target::SlotParam(i, k, p) => {
                if let Some(f) = slot(self, i, k).and_then(|x| x.fx.as_mut()) {
                    f.set_travel(p as usize, v);
                }
            }
            Target::ChanParam(c, p) => {
                if let Some(h) = self.channels.get(c).and_then(|c| c.ext.as_ref()) {
                    h.0.set_travel(p as usize, v);
                }
                if let Some(g) = self.channels.get_mut(c).and_then(|c| c.gen.as_mut()) {
                    g.set_travel(p as usize, v);
                }
            }
            Target::Other { .. } => {}
        }
    }

    /// Where a control stands now, in the raw values its events carry; None for one we cannot read.
    pub fn event_raw(&self, t: Target) -> Option<i32> {
        let slot = |i: usize, k: u16| self.inserts.get(i)?.slots.iter().find(|x| x.index == k);
        Some(match t {
            Target::ChanVolume(c) => (self.channels.get(c)?.volume * 12800.0).round() as i32,
            Target::ChanPan(c) => ((self.channels.get(c)?.pan + 1.0) * 6400.0).round() as i32,
            Target::ChanPitch(c) => self.channels.get(c)?.pitch_cents.round() as i32,
            Target::InsertVolume(i) => (self.inserts.get(i)?.volume / 1.25 * 16000.0).round() as i32,
            Target::InsertPan(i) => (self.inserts.get(i)?.pan * 6400.0).round() as i32,
            Target::Tempo => (self.tempo * 1000.0).round() as i32,
            Target::SlotMix(i, k) => (slot(i, k)?.mix * 12800.0).round() as i32,
            Target::SlotOn(i, k) => slot(i, k)?.enabled as i32,
            Target::SlotParam(i, k, p) => {
                let fx = slot(i, k)?.fx.as_ref()?;
                if fx.ext.is_some() {
                    return None;
                }
                fx.p.get(p as usize)?.round() as i32
            }
            Target::ChanParam(c, p) => self.channels.get(c)?.gen.as_ref()?.p.get(p as usize)?.round() as i32,
            Target::Other { .. } => return None,
        })
    }

    /// What reaches a control when `v` is sent to it: the answer of its mapping formula, when it has one.
    pub fn mapped(&self, t: Target, v: f32) -> f32 {
        if self.link_maps.is_empty() {
            return v;
        }
        match self.link_maps.iter().find(|m| m.target == t) {
            Some(m) => m.run(v),
            None => v,
        }
    }

    /// The raw values a target's control runs between; None for one we do not play.
    pub fn event_range(&self, t: Target) -> Option<(i32, i32)> {
        Some(match t {
            Target::ChanVolume(_) | Target::ChanPan(_) => (0, 12800),
            // the pitch knob in cents, two semitones either way
            Target::ChanPitch(_) => (-200, 200),
            Target::InsertVolume(_) => (0, 16000),
            Target::InsertPan(_) => (-6400, 6400),
            Target::Tempo => (60_000, 180_000),
            Target::SlotMix(..) => (0, 12800),
            Target::SlotOn(..) => (0, 1),
            Target::SlotParam(i, s, k) => {
                let fx = self.inserts.get(i)?.slots.iter().find(|x| x.index == s)?.fx.as_ref()?;
                if let Some(h) = &fx.ext {
                    return ((k as usize) < h.0.param_count()).then_some(HOSTED_RANGE);
                }
                let q = crate::fx::def(fx.kind).params.get(k as usize)?;
                if fx.kind == crate::fx::Kind::Deck {
                    // the turntable's settings are counted in thousandths (seen in a project that scratches)
                    return Some((0, 1000));
                }
                (q.min, q.max)
            }
            Target::ChanParam(c, k) => {
                if let Some(h) = &self.channels.get(c)?.ext {
                    return ((k as usize) < h.0.param_count()).then_some(HOSTED_RANGE);
                }
                let g = self.channels.get(c)?.gen.as_ref()?;
                let q = crate::synth::def(g.kind).params.get(k as usize)?;
                (q.min, q.max)
            }
            Target::Other { .. } => return None,
        })
    }

    /// A short name for a target, for lists and titles.
    pub fn target_name(&self, t: Target) -> String {
        let chan = |c: usize| self.channels.get(c).map(|c| c.name.clone()).unwrap_or_else(|| format!("Channel {}", c + 1));
        let ins = |i: usize| match self.inserts.get(i) {
            Some(x) if !x.name.is_empty() => x.name.clone(),
            _ if i == 0 => "Master".into(),
            _ => format!("Insert {i}"),
        };
        let slot = |i: usize, s: u16| self.inserts.get(i).and_then(|x| x.slots.iter().find(|x| x.index == s)).map(|x| if x.name.is_empty() { x.plugin.clone() } else { x.name.clone() }).unwrap_or_else(|| format!("slot {}", s + 1));
        match t {
            Target::ChanVolume(c) => format!("{} - Volume", chan(c)),
            Target::ChanPan(c) => format!("{} - Pan", chan(c)),
            Target::ChanPitch(c) => format!("{} - Pitch", chan(c)),
            Target::InsertVolume(i) => format!("{} - Volume", ins(i)),
            Target::InsertPan(i) => format!("{} - Pan", ins(i)),
            Target::Tempo => "Tempo".into(),
            Target::SlotMix(i, s) => format!("{} - {} - Mix", ins(i), slot(i, s)),
            Target::SlotOn(i, s) => format!("{} - {} - On", ins(i), slot(i, s)),
            Target::SlotParam(i, s, k) => {
                let name = self.inserts.get(i).and_then(|x| x.slots.iter().find(|x| x.index == s)).and_then(|x| x.fx.as_ref()).and_then(|f| match &f.ext {
                    Some(h) => h.0.param_name(k as usize),
                    None => crate::fx::def(f.kind).params.get(k as usize).map(|q| q.name.to_string()),
                }).unwrap_or_else(|| format!("control {}", k + 1));
                format!("{} - {} - {}", ins(i), slot(i, s), name)
            }
            Target::ChanParam(c, k) => {
                let name = self.channels.get(c).and_then(|x| match &x.ext {
                    Some(h) => h.0.param_name(k as usize),
                    None => x.gen.as_ref().and_then(|g| crate::synth::def(g.kind).params.get(k as usize)).map(|q| q.name.to_string()),
                }).unwrap_or_else(|| format!("control {}", k + 1));
                format!("{} - {}", chan(c), name)
            }
            Target::Other { param, dest } => format!("Control {param:#06x} of {dest:#06x}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_second_time_signature_number_is_steps_in_a_beat() {
        let mut s = Song::default();
        s.ppq = 96;
        s.time_sig = (4, 4);
        assert_eq!((s.beat_ticks(), s.bar()), (96, 384));
        s.time_sig = (4, 8);
        assert_eq!((s.beat_ticks(), s.bar()), (192, 768));
        s.time_sig = (3, 4);
        assert_eq!((s.beat_ticks(), s.bar()), (96, 288));
    }

    #[test]
    fn a_new_timebase_keeps_every_time_in_place() {
        let mut s = Song::new();
        s.patterns[0].notes.push(Note { pos: 36, len: 12, key: 60, ..Default::default() });
        s.patterns[0].notes.push(Note { pos: 96, len: 0, key: 62, ..Default::default() });
        s.clips.push(Clip { pos: 384, len: 100, track: 0, src: ClipSrc::Pattern(0), offset: 48, muted: false, group: 0, fine: 0.0, fx: Default::default() });
        s.markers.push(TimeMarker { pos: 768, name: "x".into(), kind: 0, sig: (4, 4) });
        s.set_ppq(192);
        assert_eq!((s.ppq, s.bar()), (192, 768));
        let n = &s.patterns[0].notes;
        assert_eq!((n[0].pos, n[0].len, n[1].pos, n[1].len), (72, 24, 192, 0));
        assert_eq!((s.clips[0].pos, s.clips[0].len, s.clips[0].offset, s.markers[0].pos), (768, 200, 96, 1536));
        s.set_ppq(96);
        assert_eq!((s.patterns[0].notes[0].pos, s.clips[0].len), (36, 100));
    }

    #[test]
    fn patterns_move_and_split_without_changing_the_song() {
        let mut s = Song::new();
        s.channels.push(Chan::default());
        s.ensure_pattern(1);
        s.patterns[0].notes.push(Note { pos: 0, chan: 0, ..Default::default() });
        s.patterns[0].notes.push(Note { pos: 96, chan: 1, ..Default::default() });
        s.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
        assert_eq!(s.move_pattern(0, true), 0);
        assert_eq!(s.move_pattern(0, false), 1);
        assert_eq!(s.clips[0].src, ClipSrc::Pattern(1));
        assert_eq!(s.patterns[1].notes.len(), 2);
        let made = s.split_pattern(1);
        assert_eq!(made.len(), 1);
        assert!(s.patterns[1].notes.iter().all(|n| n.chan == 0) && s.patterns[made[0]].notes.iter().all(|n| n.chan == 1));
        assert_eq!(s.clips.iter().filter(|c| c.src == ClipSrc::Pattern(made[0]) && c.pos == 0).count(), 1);
        assert!(s.split_pattern(1).is_empty());
    }

    #[test]
    fn swapping_channels_and_tracks_takes_what_refers_to_them_along() {
        let mut s = Song::new();
        s.channels = vec![Chan { name: "a".into(), insert: 3, ..Chan::default() }, Chan { name: "b".into(), insert: 5, ..Chan::default() }, Chan { name: "c".into(), ..Chan::default() }];
        s.ensure_pattern(0);
        s.patterns[0].set_step(0, 0, true);
        s.patterns[0].set_step(2, 24, true);
        let (param, dest) = s.event_address(Target::InsertVolume(3));
        s.patterns[0].events.push(PatEvent { tick: 0, param, dest, raw: 100 });
        s.midi_links.push(MidiLink { ctrl: 7, port: 0, target: Target::ChanVolume(0) });
        s.inserts[3].sends = vec![5];
        s.inserts[3].set_send_level(5, 0.5);

        s.swap_channels(0, 2);
        assert_eq!(s.channels[2].name, "a");
        assert!(s.patterns[0].has_step(2, 0) && s.patterns[0].has_step(0, 24));
        assert_eq!(s.midi_links[0].target, Target::ChanVolume(2));

        s.swap_inserts(3, 4);
        assert_eq!(s.channels[2].insert, 4);
        assert_eq!(s.channels[1].insert, 5);
        assert_eq!(s.inserts[4].sends, vec![5]);
        assert_eq!(s.inserts[4].send_level(5), 0.5);
        let e = s.patterns[0].events[0];
        assert_eq!(s.event_target(e.param, e.dest), Target::InsertVolume(4));
        s.swap_inserts(4, 5);
        assert_eq!(s.inserts[5].sends, vec![4]);
        assert_eq!(s.inserts[5].send_level(4), 0.5);
        // the master never moves
        s.swap_inserts(0, 5);
        assert_eq!(s.channels[2].insert, 5);
    }

    #[test]
    fn new_song_is_sane() {
        let s = Song::new();
        assert_eq!(s.bar(), 384);
        assert_eq!(s.step(), 24);
        assert_eq!(s.length(), 384);
        assert_eq!(s.mix_order.len(), DEFAULT_INSERTS);
        assert_eq!(*s.mix_order.last().unwrap(), 0);
    }

    #[test]
    fn steps_toggle_and_length_rounds_to_bars() {
        let mut p = Pattern::default();
        assert!(p.toggle_step(0, 0));
        assert!(p.toggle_step(0, 400));
        assert!(p.has_step(0, 400));
        assert_eq!(p.length(384), 768);
        assert!(!p.toggle_step(0, 400));
        assert_eq!(p.length(384), 384);
    }

    #[test]
    fn mix_order_puts_senders_first() {
        let mut s = Song::new();
        s.inserts[5].sends = vec![3]; // 5 feeds 3, which feeds the master
        s.prepare();
        let at = |i: usize| s.mix_order.iter().position(|&x| x == i).unwrap();
        assert!(at(5) < at(3));
        assert!(at(3) < at(0));
    }

    #[test]
    fn time_signature_markers_change_the_bars() {
        let mut s = Song::default();
        s.ppq = 96;
        s.time_sig = (4, 4);
        assert_eq!(s.bar_lines(400, 800), vec![(384, 1, 384), (768, 2, 384)]);
        // 3/4 from the third bar on, and a change that cuts a bar short
        s.markers.push(TimeMarker { pos: 768, name: String::new(), kind: 8, sig: (3, 4) });
        s.markers.push(TimeMarker { pos: 768 + 288 + 96, name: String::new(), kind: 8, sig: (4, 4) });
        let lines = s.bar_lines(0, 1700);
        assert_eq!(lines, vec![(0, 0, 384), (384, 1, 384), (768, 2, 288), (1056, 3, 96), (1152, 4, 384), (1536, 5, 384)]);
    }

    #[test]
    fn marker_actions_fire_in_order() {
        let mut s = Song::default();
        let mk = |pos, kind| TimeMarker { pos, name: String::new(), kind, sig: (4, 4) };
        s.markers = vec![mk(100, 1), mk(300, 0), mk(500, 2), mk(700, 3), mk(900, 0)];
        // inside the looped section: back to its marker when the next one comes up
        assert_eq!(s.marker_action(150.0), Some((300.0, 1, 100.0)));
        // before it nothing loops; the first thing ahead is still that section's end only once inside
        assert_eq!(s.marker_action(0.0), Some((500.0, 2, 700.0)));
        assert_eq!(s.marker_action(300.0), Some((500.0, 2, 700.0)));
        assert_eq!(s.marker_action(500.0), Some((700.0, 3, 700.0)));
        assert_eq!(s.marker_action(700.0), None);
    }
}
