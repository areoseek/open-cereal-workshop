//! Built-in instruments. Like the effects (`fx.rs`), each one is our own sound source that reads and writes the
//! stored settings of the matching generator in a project file. Settings stay in file units; the voice code
//! converts them. Field meanings come from comparing many project files and are approximations until measured
//! against the reference (docs/PARITY.md, "Instruments").

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Three mixed oscillators.
    Osc3,
    /// Harmonics shaped by a timbre, mask, pluck, filter and phaser (`additive.rs`).
    Additive,
    /// Six operators through a modulation matrix and three filters (`fm.rs`).
    Fm,
    /// A swept sine drum (`minis.rs`).
    Kick,
    /// A carrier and two modulators (`minis.rs`).
    Fm2,
    /// A plucked bass string (`minis.rs`).
    Bass,
    /// Sampled keys read from set files (`keys.rs`).
    Keys,
    /// The larger additive instrument, played by the additive engine (`additive.rs`).
    Forge,
    /// The pack player: its sounds live in protected pack files, so a stand-in voice is chosen by the preset's name.
    Packs,
    /// The metallic additive instrument (`metal.rs`).
    Metal,
    /// The same instrument behind a preset picker.
    MetalBank,
    /// Synths we cannot read in full yet: a voice of our own shaped by the few settings we can place (`SUBS`).
    Sub,
    /// Drum synths we cannot read in full yet: played by the swept sine drum.
    Thump,
    /// One drum patch per key (`drumbank.rs`).
    DrumBank,
    /// A picture swept from left to right, each row a sine wave (`beep.rs`).
    Image,
    /// Units that make no sound of their own: they send control values or notes elsewhere.
    Quiet,
}

impl Kind {
    /// The small instruments that share the voice code of `minis.rs`.
    pub fn mini(self) -> bool {
        !matches!(self, Kind::Osc3 | Kind::Additive | Kind::Fm | Kind::Forge | Kind::Packs | Kind::Metal | Kind::MetalBank | Kind::Sub)
    }
}

pub struct Param {
    pub name: &'static str,
    /// Byte offset in the stored block.
    pub off: usize,
    /// Bytes the value takes (1 or 4).
    pub width: u8,
    pub min: i32,
    pub max: i32,
    pub def: i32,
}

pub struct Def {
    pub kind: Kind,
    /// Identifier used in project files.
    pub ident: &'static str,
    /// Our own name for the instrument.
    pub title: &'static str,
    pub size: usize,
    pub fixed: &'static [(usize, i32)],
    /// Bytes a fresh block carries after the fixed-size part.
    pub tail: &'static [u8],
    pub params: &'static [Param],
}

impl Def {
    /// Instruments we can play from a stored block but cannot yet make a fresh block for.
    pub fn arrives_only(&self) -> bool {
        matches!(self.kind, Kind::Fm | Kind::Forge | Kind::Packs | Kind::Metal | Kind::MetalBank | Kind::Sub | Kind::Thump | Kind::DrumBank | Kind::Image | Kind::Quiet)
    }
}

const fn p(name: &'static str, off: usize, min: i32, max: i32, def: i32) -> Param {
    Param { name, off, width: 4, min, max, def }
}

const fn b(name: &'static str, off: usize) -> Param {
    Param { name, off, width: 1, min: 0, max: 1, def: 0 }
}

/// Settings per oscillator of the three-oscillator instrument, in table order.
pub const OSC_FIELDS: usize = 7;
pub const SHAPES: [&str; 7] = ["Sine", "Triangle", "Square", "Saw", "Round saw", "Noise", "Custom"];

pub static DEFS: &[Def] = &[Def {
    kind: Kind::Osc3,
    ident: "3x Osc",
    title: "Triple Osc",
    size: 92,
    fixed: &[(0, 12)],
    tail: &[],
    params: &[
        p("Osc 1 pan", 4, -64, 64, 0),
        p("Osc 1 shape", 8, 0, 6, 3),
        p("Osc 1 coarse", 12, -24, 24, 0),
        p("Osc 1 fine", 16, -100, 100, 0),
        p("Osc 1 phase", 20, 0, 64, 0),
        p("Osc 1 detune", 24, 0, 64, 0),
        p("Osc 2 level", 28, 0, 128, 0),
        p("Osc 2 pan", 32, -64, 64, 0),
        p("Osc 2 shape", 36, 0, 6, 3),
        p("Osc 2 coarse", 40, -24, 24, -12),
        p("Osc 2 fine", 44, -100, 100, 0),
        p("Osc 2 phase", 48, 0, 64, 0),
        p("Osc 2 detune", 52, 0, 64, 0),
        p("Osc 3 level", 56, 0, 128, 0),
        p("Osc 3 pan", 60, -64, 64, 0),
        p("Osc 3 shape", 64, 0, 6, 3),
        p("Osc 3 coarse", 68, -24, 24, -24),
        p("Osc 3 fine", 72, -100, 100, 0),
        p("Osc 3 phase", 76, 0, 64, 0),
        p("Osc 3 detune", 80, 0, 64, 0),
        p("Phase random", 84, 0, 64, 0),
        b("Osc 1 invert", 88),
        b("Osc 2 invert", 89),
        b("Osc 3 invert", 90),
        b("Osc 3 AM", 91),
    ],
}, Def {
    kind: Kind::Additive,
    ident: "Harmless",
    title: "Harmonic Stack",
    size: 432,
    fixed: &[(0, 12), (424, 1)],
    tail: &[0, 0, 0, 0, 1],
    // order matters: `additive::ix` indexes this list
    params: &[
        p("Phase", 4, -1000, 1000, 1000),
        p("Low harmonic protection", 8, 0, 1024, 256),
        p("Sub 1", 12, 0, 2048, 0),
        p("Sub 2", 16, 0, 2048, 0),
        p("Sub 3", 20, 0, 2048, 0),
        p("EQ 1", 24, -1024, 1024, 0),
        p("EQ 2", 28, -1024, 1024, 0),
        p("EQ 3", 32, -1024, 1024, 0),
        p("EQ 4", 36, -1024, 1024, 0),
        p("EQ 5", 40, -1024, 1024, 0),
        p("EQ 6", 44, -1024, 1024, 0),
        p("Mask 1", 48, 0, 1000, 1000),
        p("Mask 2", 52, 0, 1000, 0),
        p("Mask 3", 56, 0, 1000, 1000),
        p("Mask 4", 60, 0, 1000, 0),
        p("Mask 5", 64, 0, 1000, 1000),
        p("Mask 6", 68, 0, 1000, 0),
        p("Mask 7", 72, 0, 1000, 1000),
        p("Mask 8", 76, 0, 1000, 0),
        p("Mask 9", 80, 0, 1000, 1000),
        p("Mask 10", 84, 0, 1000, 0),
        p("Mask 11", 88, 0, 1000, 1000),
        p("Mask 12", 92, 0, 1000, 0),
        p("Mask mix", 96, 0, 1000, 0),
        p("Mask LFO", 100, -1000, 1000, 0),
        p("Tremolo", 104, 0, 1000, 0),
        p("Attack", 108, 0, 1000, 0),
        p("Decay", 112, 0, 1000, 1000),
        p("Release", 116, 0, 1000, 250),
        p("Pluck", 128, 0, 1000, 1000),
        p("Pluck masked", 132, 0, 1, 0),
        p("Pluck alternative", 136, 0, 1, 0),
        p("Pluck on release", 140, 0, 1, 0),
        p("Volume", 148, 0, 2000, 1000),
        p("Velocity to volume", 152, 0, 1, 1),
        p("Pitch", 156, -2400, 2400, 0),
        p("Harmonic detune", 160, -2400, 2400, 0),
        p("Grit", 164, -1000, 1000, 0),
        p("Vibrato", 168, 0, 1000, 0),
        p("Pitch LFO", 172, -1000, 1000, 0),
        p("Portamento", 176, 0, 1, 0),
        p("Legato", 180, 0, 1, 0),
        p("Slide time", 184, 0, 1000, 400),
        p("Filter on", 192, 0, 1, 1),
        p("Filter attack", 196, 0, 1000, 0),
        p("Filter decay", 200, 0, 1000, 500),
        p("Attack slope", 204, -1000, 1000, 0),
        p("Decay slope", 208, -1000, 1000, -300),
        p("Envelope amount", 212, -1000, 1000, 500),
        p("Velocity to envelope", 216, 0, 1, 0),
        p("Cutoff", 220, 0, 8192, 4096),
        p("Adaptive envelope", 224, 0, 1, 0),
        p("Filter width", 228, 0, 1000, 500),
        p("Key tracking", 232, -1000, 1000, 0),
        p("Cutoff LFO", 236, -1000, 1000, 0),
        p("Resonance", 240, 0, 1000, 0),
        p("Resonance in Hz", 244, 0, 1, 0),
        p("Resonance offset", 248, -2400, 2400, 0),
        p("Self oscillation", 252, 0, 2048, 0),
        p("Motion only", 256, 0, 1, 0),
        p("Adaptive width", 260, 0, 1, 0),
        p("Unison order", 264, 1, 9, 1),
        p("Unison pan", 268, 0, 128, 128),
        p("Unison pitch", 272, 0, 256, 128),
        p("Unison phase", 276, 0, 128, 64),
        p("Unison variation", 280, 0, 128, 0),
        p("Unison type", 284, 0, 2, 2),
        p("Phaser amount", 288, 0, 1000, 0),
        p("Phaser scale", 292, 0, 2, 0),
        p("Phaser width", 296, 0, 10000, 5000),
        p("Phaser width LFO", 300, -1000, 1000, 0),
        p("Phaser offset", 304, 0, 1000, 500),
        p("Phaser offset LFO", 308, -1000, 1000, 0),
        p("Phaser speed", 312, -8000, 8000, 3000),
        p("Phaser key tracking", 316, -2000, 2000, 0),
        p("Harmonizer amount", 320, 0, 1000, 0),
        p("Harmonizer width", 324, 0, 1000, 500),
        p("Harmonizer LFO", 328, -1000, 1000, 0),
        p("Velocity to harmonizer", 332, 0, 1, 0),
        p("Harmonizer strength", 336, 0, 1024, 256),
        p("Harmonizer type", 340, 0, 4, 0),
        p("Harmonizer position", 344, 0, 3, 3),
        p("LFO shape", 348, 0, 13, 0),
        p("LFO global", 352, 0, 1, 1),
        p("LFO attack", 356, 0, 1000, 0),
        p("LFO speed", 360, -8000, 8000, 3000),
        p("Chorus type", 364, 0, 9, 0),
        p("Chorus mix", 368, 0, 1000, 500),
        p("Delay type", 372, 0, 3, 0),
        p("Delay volume", 376, 0, 1000, 500),
        p("Delay feedback", 380, 0, 1000, 300),
        p("Delay time", 384, 1, 384, 144),
        p("Reverb type", 388, 0, 6, 0),
        p("Reverb volume", 392, 0, 1000, 500),
        p("Compressor mix", 396, 0, 1000, 500),
        p("Compressor type", 404, 0, 16, 4),
        p("Phaser type", 408, 0, 8, 0),
        p("Resonance type", 412, 0, 9, 0),
        p("Filter type", 416, 0, 4, 0),
        p("Timbre", 420, 0, 11, 0),
    ],
}, Def {
    kind: Kind::Fm,
    ident: "Sytrus",
    title: "Six Op",
    // a packed block of no fixed size; it is read by `fm::Patch` and stored back untouched
    size: 0,
    fixed: &[],
    tail: &[],
    params: &[],
},
    Def {
        kind: Kind::Kick,
        ident: "Fruit Kick",
        title: "Sweep Kick",
        size: 32,
        fixed: &[(28, 4504291)],
        tail: &[],
        params: &[
            p("Start", 4, -2400, 3600, 0),
            p("End", 8, -2400, 0, -600),
            p("Sweep", 12, 0, 256, 64),
            p("Decay", 16, 0, 256, 128),
            p("Click", 20, 0, 64, 32),
            p("Drive", 24, 0, 128, 0),
        ],
    },
    Def {
        kind: Kind::Fm2,
        ident: "Fruity DX10",
        title: "Duo FM",
        size: 92,
        fixed: &[(0, 1)],
        tail: &[],
        params: &[
            p("Attack", 4, 0, 65535, 0),
            p("Decay", 8, 0, 65535, 42598),
            p("Release", 12, 0, 65535, 28901),
            p("Mod 1 coarse", 16, 0, 65535, 55181),
            p("Mod 1 fine", 20, 0, 65535, 21561),
            p("Mod 1 start", 24, 0, 65535, 15073),
            p("Mod 1 time", 28, 0, 65535, 52429),
            p("Mod 1 hold", 32, 0, 65535, 3277),
            p("Mod 1 release", 36, 0, 65535, 52429),
            p("Mod touch", 40, 0, 65535, 58982),
            p("Vibrato", 44, 0, 65535, 0),
            p("Fine tune", 48, 0, 65535, 29295),
            p("Brightness", 52, 0, 65535, 0),
            p("Mod 1 through", 56, 0, 65535, 27132),
            p("Vibrato rate", 60, 0, 65535, 0),
            p("Mod 2 coarse", 64, 0, 65535, 0),
            p("Mod 2 fine", 68, 0, 65535, 0),
            p("Mod 2 start", 72, 0, 65535, 0),
            p("Mod 2 time", 76, 0, 65535, 0),
            p("Mod 2 hold", 80, 0, 65535, 0),
            p("Mod 2 release", 84, 0, 65535, 0),
            p("Octave", 88, -3, 3, 0),
        ],
    },
    Def {
        kind: Kind::Bass,
        ident: "BooBass",
        title: "String Bass",
        size: 16,
        fixed: &[(0, 1)],
        tail: &[],
        params: &[p("Bass", 4, 0, 65535, 32767), p("Mid", 8, 0, 65535, 32767), p("Treble", 12, 0, 65535, 32767)],
    },
    Def {
        kind: Kind::Keys,
        ident: "FL Keys",
        title: "Sample Keys",
        // the block starts with the set's name, so the settings sit at a distance that depends on it
        size: 0,
        fixed: &[],
        tail: &[1, 0, 0, 0, 9, b'm', b'd', b'a', b' ', b'P', b'i', b'a', b'n', b'o', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        params: &[
            p("Decay", 0, -1000, 1000, 0),
            p("Release", 4, -1000, 1000, 0),
            p("Hardness", 8, -1000, 1000, 0),
            p("Touch to hardness", 12, -1000, 1000, -1000),
            p("Muffle", 16, 0, 1000, 784),
            p("Touch to muffle", 20, 0, 2000, 502),
            p("Touch", 24, -1000, 1000, -248),
            p("Stereo", 28, 0, 2000, 1000),
            p("Tune", 32, -1000, 1000, 0),
            p("Detune", 36, 0, 500, 123),
            p("Stretch", 40, -500, 500, 0),
            p("Treble", 44, -1000, 1000, 0),
            p("Pan / tremolo", 48, -1000, 1000, 0),
            p("Wobble rate", 52, 0, 65535, 0),
            p("Drive", 56, 0, 1000, 0),
        ],
    },
    Def {
        kind: Kind::Forge,
        ident: "Harmor",
        title: "Harmonic Forge",
        // only the leading settings of part A are read; everything after them stays as stored
        size: 344,
        fixed: &[],
        tail: &[],
        // order matters: `forge` below indexes this list
        params: &[
            p("Timbre mix", 28, 0, 1000, 0),
            p("Phase randomness", 16, -1000, 1000, 0),
            p("Prism", 116, -1000, 1000, 0),
            p("Harmonizer", 120, -1000, 1000, 0),
            p("Harmonizer width", 124, 0, 1000, 500),
            p("Harmonizer strength", 128, 0, 1024, 256),
            p("Unison order", 148, 1, 9, 1),
            p("Unison pan", 160, 0, 256, 192),
            p("Unison pitch", 164, 0, 256, 128),
            p("Unison phase", 168, -1000, 1000, 500),
            p("Filter 1 cutoff", 220, 0, 8192, 8192),
            p("Filter 1 width", 228, 0, 1000, 500),
            p("Filter 1 key tracking", 232, -1000, 1000, 0),
            p("Filter 1 resonance", 240, 0, 1000, 0),
            p("Filter 2 cutoff", 268, 0, 8192, 8192),
            p("Filter 2 resonance", 288, 0, 1000, 0),
            p("Pluck", 312, 0, 1000, 1000),
            p("Phaser mix", 316, 0, 1000, 0),
            p("Phaser scale", 320, 0, 2, 0),
            p("Phaser width", 324, 0, 10000, 5000),
            p("Phaser offset", 328, 0, 1000, 500),
            p("Phaser speed", 332, -8000, 8000, 3000),
        ],
    },
    Def { kind: Kind::Metal, ident: "Ogun", title: "Metal Stack", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::MetalBank, ident: "Autogun", title: "Metal Bank", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "PoiZone", title: "Duo Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "Sawer", title: "Saw Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "Sakura", title: "String Model", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "Toxic Biohazard", title: "Hybrid Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "Kepler", title: "Vintage Poly", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "MiniSynth", title: "Pocket Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "GMS", title: "Triple Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "Morphine", title: "Morph Additive", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Sub, ident: "SimSynth", title: "Analog Trio", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Thump, ident: "BassDrum", title: "Kick Designer", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Thump, ident: "Drumpad", title: "Membrane Drum", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Image, ident: "BeepMap", title: "Picture Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Keyboard Controller", title: "Key Controller", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Envelope Controller", title: "Envelope Sender", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Voltage Controller", title: "Voltage Sender", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "MIDI Out", title: "MIDI Sender", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::DrumBank, ident: "Fruity DrumSynth Live", title: "Drum Bank", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Thump, ident: "Drumaxx", title: "Drum Modeler", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Packs, ident: "FLEX", title: "Pack Player", size: 1105, fixed: &[], tail: &[], params: &[] },
];

/// What we can place in the float lists of the synths played by a stand-in: where the list starts, the four
/// envelope settings of the level (attack, decay, sustain, release), the filter cutoff, and the kind of voice.
struct SubMap {
    ident: &'static str,
    base: usize,
    adsr: Option<[usize; 4]>,
    cutoff: Option<usize>,
    voice: &'static str,
}

const SUBS: &[SubMap] = &[
    SubMap { ident: "PoiZone", base: 12, adsr: Some([22, 23, 24, 25]), cutoff: Some(5), voice: "lead" },
    SubMap { ident: "Sawer", base: 12, adsr: Some([2, 3, 4, 5]), cutoff: None, voice: "lead" },
    SubMap { ident: "Sakura", base: 12, adsr: Some([2, 3, 4, 5]), cutoff: None, voice: "pluck" },
];

/// The stand-in voice of one of the `Kind::Sub` synths.
fn sub_patch(ident: &str, state: &[u8]) -> crate::additive::Patch {
    use crate::additive::ix;
    let Some(m) = SUBS.iter().find(|m| m.ident.eq_ignore_ascii_case(ident)) else { return stand_in("synth") };
    let mut patch = stand_in(m.voice);
    let at = |i: usize| {
        let o = m.base + i * 4;
        state.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0))
    };
    if let Some([a, d, su, r]) = m.adsr {
        if let (Some(a), Some(d), Some(su), Some(r)) = (at(a), at(d), at(su), at(r)) {
            let p = &mut patch.p;
            p[ix::ATTACK] = a * 1000.0;
            p[ix::RELEASE] = r * 1000.0;
            // the engine has no sustain level: a low one becomes a note that dies away
            if su < 0.5 {
                p[ix::DECAY] = 300.0 + d * 600.0;
                p[ix::PLUCK] = 400.0 + d * 500.0;
            } else {
                p[ix::DECAY] = 1000.0;
                p[ix::PLUCK] = 1000.0;
            }
        }
    }
    if let Some(c) = m.cutoff.and_then(at) {
        patch.p[ix::CUTOFF] = 2600.0 + c * 5592.0;
    }
    patch
}

/// Settings for the swept sine drum from a drum synth's block.
fn thump(ident: &str, state: &[u8]) -> Vec<f32> {
    let at = |i: usize| state.get(1292 + i * 4..1296 + i * 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0));
    if ident.eq_ignore_ascii_case("BassDrum") {
        if let (Some(dur), Some(base), Some(peak), Some(slide)) = (at(1), at(2), at(3), at(4)) {
            let end = -1800.0 + base * 1800.0;
            return vec![(end + peak * 3000.0).min(3600.0), end, slide * 128.0, 40.0 + dur * 216.0, 32.0, 0.0];
        }
        return vec![1200.0, -900.0, 48.0, 140.0, 32.0, 0.0];
    }
    // a struck skin: a short fall onto the played key
    vec![700.0, 0.0, 40.0, 140.0, 16.0, 0.0]
}

/// A voice of our own for a pack preset we cannot read, picked from the words in its name.
pub fn stand_in(name: &str) -> crate::additive::Patch {
    use crate::additive::ix;
    let n = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    let mut p: Vec<f32> = def(Kind::Additive).params.iter().map(|p| p.def as f32).collect();
    p[ix::F_AMOUNT] = 0.0;
    p[ix::CUTOFF] = 8192.0;
    // (timbre, attack, decay, release, pluck, cutoff, unison voices)
    let (timbre, attack, decay, release, pluck, cut, uni) = if has(&["808", "sub"]) {
        (3.0, 0.0, 780.0, 200.0, 1000.0, 3600.0, 1.0)
    } else if has(&["bas", "bss"]) {
        (0.0, 0.0, 1000.0, 150.0, 520.0, 5200.0, 1.0)
    } else if has(&["organ", "accord", "harmonica"]) {
        (9.0, 0.0, 1000.0, 120.0, 1000.0, 8192.0, 1.0)
    } else if has(&["pad", "string", "choir", "voice", "atmos", "drone", "sphere", "sweep", "ensemble", "orchestra", "violin", "cello", "viola"]) {
        (2.0, 420.0, 1000.0, 520.0, 1000.0, 5600.0, 5.0)
    } else if has(&["brass", "horn", "trumpet", "trombone", "tuba", "sax"]) {
        (0.0, 120.0, 1000.0, 180.0, 1000.0, 5400.0, 1.0)
    } else if has(&["flute", "whistle", "ocarina", "pipe", "recorder", "piccolo", "clarinet", "oboe", "bassoon"]) {
        (3.0, 150.0, 1000.0, 180.0, 1000.0, 7000.0, 1.0)
    } else if has(&["bell", "glock", "celesta", "chime", "kalimba", "music box", "vibra", "tinke"]) {
        (7.0, 0.0, 640.0, 520.0, 1000.0, 8192.0, 1.0)
    } else if has(&["piano", "key", "clav", "harp", "pluck", "guitar", "marimba", "xylo", "koto", "banjo", "sitar", "pizz", "dulcimer"]) {
        (2.0, 0.0, 620.0, 300.0, 560.0, 7000.0, 1.0)
    } else if has(&["lead", "saw", "sync", "synth"]) {
        (0.0, 0.0, 1000.0, 200.0, 1000.0, 6400.0, 3.0)
    } else {
        (2.0, 0.0, 800.0, 300.0, 700.0, 6400.0, 1.0)
    };
    p[ix::TIMBRE] = timbre;
    p[ix::ATTACK] = attack;
    p[ix::DECAY] = decay;
    p[ix::RELEASE] = release;
    p[ix::PLUCK] = pluck;
    p[ix::CUTOFF] = cut;
    p[ix::KEY_TRACK] = 500.0;
    p[ix::UNI_ORDER] = uni;
    crate::additive::Patch { p, custom: None }
}

pub fn def(kind: Kind) -> &'static Def {
    DEFS.iter().find(|d| d.kind == kind).expect("every kind has a table entry")
}

pub fn title_for(ident: &str) -> Option<&'static str> {
    // instruments that are not settings tables: sample maps and the loop slicer
    const OTHER: &[(&str, &str)] = &[("FPC", "Pad Machine"), ("DirectWave", "Zone Sampler"), ("Fruity Slicer", "Loop Slicer"), ("Slicex", "Marker Slicer")];
    DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(ident)).map(|d| d.title).or_else(|| OTHER.iter().find(|o| o.0.eq_ignore_ascii_case(ident)).map(|o| o.1))
}

/// One instrument's settings, in file units, in the order of its table.
#[derive(Debug, Clone)]
pub struct Gen {
    pub kind: Kind,
    pub p: Vec<f32>,
    /// The stored block these settings came from (or a fresh one); bytes we do not understand live on here.
    pub raw: Vec<u8>,
    /// The read patch of the six-operator instrument, shared by its notes.
    pub fm: Option<std::sync::Arc<crate::fm::Patch>>,
    /// A patch worked out once at load, for the instruments whose settings we only play and never edit.
    pub pre: Option<std::sync::Arc<crate::additive::Patch>>,
    /// The picture of the picture instrument.
    pub img: Option<std::sync::Arc<crate::beep::Image>>,
}

/// Two sets of settings are the same when they would be stored as the same block.
impl PartialEq for Gen {
    fn eq(&self, o: &Self) -> bool {
        self.kind == o.kind && self.p == o.p && (self.raw == o.raw || self.encode(None) == o.encode(None))
    }
}

impl Gen {
    pub fn new(kind: Kind) -> Self {
        let mut g = Self { kind, p: def(kind).params.iter().map(|p| p.def as f32).collect(), raw: Vec::new(), fm: None, pre: None, img: None };
        g.raw = g.encode(None);
        if kind == Kind::Fm {
            g.fm = Some(Default::default());
        }
        g
    }

    pub fn decode(ident: &str, state: &[u8]) -> Option<Self> {
        let d = DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(ident))?;
        if state.is_empty() {
            return Some(Self::new(d.kind));
        }
        if d.kind == Kind::Fm {
            // layouts we cannot read stay with the project as an unknown instrument
            let patch = crate::fm::Patch::parse(state)?;
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: Some(std::sync::Arc::new(patch)), pre: None, img: None });
        }
        if d.kind == Kind::Sub {
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: Some(std::sync::Arc::new(sub_patch(ident, state))), img: None });
        }
        if d.kind == Kind::Image {
            // a block without a readable picture stays with the project and plays nothing
            let img = crate::beep::Image::parse(state).map(std::sync::Arc::new);
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: None, img });
        }
        if d.kind == Kind::Quiet {
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: None, img: None });
        }
        if d.kind == Kind::DrumBank {
            return Some(Self { kind: d.kind, p: crate::drumbank::parse(state), raw: state.to_vec(), fm: None, pre: None, img: None });
        }
        if d.kind == Kind::Thump {
            return Some(Self { kind: d.kind, p: thump(ident, state), raw: state.to_vec(), fm: None, pre: None, img: None });
        }
        if matches!(d.kind, Kind::Metal | Kind::MetalBank) {
            let patch = crate::metal::patch(state)?;
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: Some(std::sync::Arc::new(patch)), img: None });
        }
        if d.kind == Kind::Osc3 && state.len() == 44 {
            return Some(Self::old_osc3(state));
        }
        let widened;
        let state = if d.kind == Kind::Additive && state.len() == 428 {
            // the first layout had no timbre choice and no custom wave
            let mut v = state[..420].to_vec();
            v.extend_from_slice(&[0; 4]);
            v.extend_from_slice(&state[420..]);
            v.extend_from_slice(&[0; 4]);
            widened = v;
            &widened[..]
        } else {
            state
        };
        let shift = Self::shift(d.kind, state);
        if state.len() < d.size || (d.kind == Kind::Keys && state.len() < shift + 60) {
            return None;
        }
        if d.kind == Kind::Keys {
            // read the set now, away from the sound thread
            crate::keys::set(&crate::keys::name_in(state));
        }
        let p = d
            .params
            .iter()
            .map(|p| {
                let off = p.off + shift;
                let v = if p.width == 1 { state[off] as i32 } else { i32::from_le_bytes(state[off..off + 4].try_into().unwrap()) };
                v.clamp(p.min, p.max) as f32
            })
            .collect();
        let mut g = Self { kind: d.kind, p, raw: state.to_vec(), fm: None, pre: None, img: None };
        if d.kind == Kind::Packs {
            g.pre = Some(std::sync::Arc::new(stand_in(&g.preset_name())));
        }
        Some(g)
    }

    /// The pack and preset a pack player block names.
    pub fn pack_and_preset(&self) -> (String, String) {
        let text = |at: usize| {
            let b = self.raw.get(at..).unwrap_or(&[]);
            let b = &b[..b.len().min(1024)];
            String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned()
        };
        (text(16), text(1041))
    }

    fn preset_name(&self) -> String {
        self.pack_and_preset().1
    }

    /// The larger additive instrument as a patch of the additive engine: the settings both share carry over, the
    /// saw to square mix becomes a harmonic table, and the second layer, the picture part and the drawn envelopes
    /// are not played yet.
    fn forge(&self) -> crate::additive::Patch {
        use crate::additive::ix;
        let v = |i: usize| self.p[i];
        let mut p: Vec<f32> = def(Kind::Additive).params.iter().map(|p| p.def as f32).collect();
        p[ix::PHASE] = 1000.0 - v(1).abs();
        p[ix::HZ_AMOUNT] = v(3).abs();
        p[ix::HZ_WIDTH] = v(4);
        p[ix::HZ_STRENGTH] = v(5);
        p[ix::UNI_ORDER] = v(6);
        p[ix::UNI_PAN] = v(7) / 2.0;
        p[ix::UNI_PITCH] = v(8);
        p[ix::UNI_PHASE] = v(9).abs() / 1000.0 * 128.0;
        // the lower of the two filters is the one heard; no envelope moves it yet
        let (cut, res) = if v(14) < v(10) { (v(14), v(15)) } else { (v(10), v(13)) };
        p[ix::FILTER_ON] = 1.0;
        p[ix::F_AMOUNT] = 0.0;
        // a low setting is nearly always opened by a drawn envelope we cannot read yet, so it is kept from closing fully
        p[ix::CUTOFF] = 2048.0 + cut * 0.75;
        p[ix::KEY_TRACK] = v(12);
        p[ix::RESONANCE] = res;
        p[ix::PLUCK] = v(16);
        p[ix::PH_AMOUNT] = v(17);
        p[ix::PH_SCALE] = v(18);
        p[ix::PH_WIDTH] = v(19);
        p[ix::PH_OFFSET] = v(20);
        p[ix::PH_SPEED] = v(21);
        let mix = v(0) / 1000.0;
        let mut custom = None;
        if mix >= 1.0 {
            p[ix::TIMBRE] = 1.0;
        } else if mix > 0.0 || v(2) != 0.0 {
            // prism leans the levels toward the high or the low harmonics
            let tilt = v(2) / 1000.0;
            let amp: Vec<f32> = (1..=512).map(|k| (if k % 2 == 1 { 1.0 } else { 1.0 - mix }) / k as f32 * (k as f32).powf(tilt * 0.5)).collect();
            custom = Some((amp, vec![0.0; 512]));
            p[ix::TIMBRE] = 11.0;
        }
        crate::additive::Patch { p, custom }
    }

    /// Where the settings start in a block that opens with a name.
    fn shift(kind: Kind, state: &[u8]) -> usize {
        if kind == Kind::Keys {
            5 + state.get(4).copied().unwrap_or(0) as usize
        } else {
            0
        }
    }

    /// The first layout of the three-oscillator block: shape, coarse and fine of oscillator 1, then mix level,
    /// shape, coarse and fine of the other two.
    fn old_osc3(state: &[u8]) -> Self {
        let mut g = Self::new(Kind::Osc3);
        let at = |i: usize| i32::from_le_bytes(state[i * 4..i * 4 + 4].try_into().unwrap());
        let d = def(Kind::Osc3);
        let mut set = |name: &str, v: i32| {
            if let Some(k) = d.params.iter().position(|p| p.name == name) {
                g.p[k] = v.clamp(d.params[k].min, d.params[k].max) as f32;
            }
        };
        set("Osc 1 shape", at(0));
        set("Osc 2 level", at(3));
        set("Osc 2 shape", at(4));
        set("Osc 3 level", at(7));
        set("Osc 3 shape", at(8));
        set("Osc 1 coarse", at(1));
        set("Osc 1 fine", at(2));
        set("Osc 2 coarse", at(5));
        set("Osc 2 fine", at(6));
        set("Osc 3 coarse", at(9));
        set("Osc 3 fine", at(10));
        g
    }

    /// The stored block. Bytes we do not understand are kept from `base`, and a value that still reads the same
    /// as `base` is left exactly as it was.
    pub fn encode(&self, base: Option<&[u8]>) -> Vec<u8> {
        let d = def(self.kind);
        if self.kind == Kind::Fm {
            return if self.raw.is_empty() { base.map(|b| b.to_vec()).unwrap_or_default() } else { self.raw.clone() };
        }
        let mut out = match base {
            _ if !self.raw.is_empty() && self.raw.len() >= d.size => self.raw.clone(),
            Some(b) if b.len() >= d.size => b.to_vec(),
            _ => {
                let mut v = vec![0u8; d.size];
                for &(off, val) in d.fixed {
                    v[off..off + 4].copy_from_slice(&val.to_le_bytes());
                }
                v.extend_from_slice(d.tail);
                v
            }
        };
        let shift = Self::shift(self.kind, &out);
        for (p, &v) in d.params.iter().zip(&self.p) {
            let v = (v.round() as i32).clamp(p.min, p.max);
            let off = p.off + shift;
            if p.width == 1 {
                if (out[off] as i32).clamp(p.min, p.max) != v {
                    out[off] = v as u8;
                }
            } else {
                let was = i32::from_le_bytes(out[off..off + 4].try_into().unwrap());
                if was.clamp(p.min, p.max) != v {
                    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
                    if self.kind == Kind::Kick && p.off == 8 {
                        // the block also carries the end pitch as a step per frame
                        let step = (6370028.0 * 2f64.powf(v as f64 / 1200.0)).round() as i32;
                        out[28..32].copy_from_slice(&step.to_le_bytes());
                    }
                }
            }
        }
        out
    }

    /// What a voice needs to sound this instrument.
    pub fn tone(&self) -> Tone {
        match self.kind {
            Kind::Osc3 => {
                let mut t = Tone::default();
                for k in 0..3 {
                    let f = &self.p[k * OSC_FIELDS..];
                    t.osc[k] = Osc {
                        pan: (f[0] / 64.0).clamp(-1.0, 1.0),
                        shape: f[1] as u8,
                        ratio: 2f32.powf((f[2] + f[3] / 100.0) / 12.0),
                        phase: f[4] / 64.0,
                        // the knob spreads the two sides by up to half a semitone
                        spread: 2f32.powf(f[5] / 64.0 * 0.5 / 12.0),
                        invert: self.p[21 + k] != 0.0,
                    };
                }
                t.mix2 = self.p[6] / 128.0;
                t.mix3 = self.p[13] / 128.0;
                t.random = self.p[20] / 64.0;
                t.am = self.p[24] != 0.0;
                t
            }
            Kind::Additive => Tone { add: Some(std::sync::Arc::new(crate::additive::Patch::new(&self.p, &self.raw))), ..Default::default() },
            Kind::Fm => Tone { fm: self.fm.clone(), ..Default::default() },
            Kind::Packs | Kind::Metal | Kind::MetalBank | Kind::Sub => Tone { add: self.pre.clone().or_else(|| Some(std::sync::Arc::new(stand_in("")))), ..Default::default() },
            Kind::Thump => {
                let p = if self.p.len() == 6 { self.p.clone() } else { thump("", &[]) };
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: Kind::Kick, p, set: None, img: None })), ..Default::default() }
            }
            Kind::Image => Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p: Vec::new(), set: None, img: self.img.clone() })), ..Default::default() },
            Kind::Forge => Tone { add: Some(std::sync::Arc::new(self.forge())), ..Default::default() },
            Kind::Keys => {
                let u = self.p.iter().zip(crate::keys::RANGES).map(|(&v, (lo, hi))| (v - lo as f32) / (hi - lo) as f32).collect();
                let set = Some(crate::keys::set(&crate::keys::name_in(&self.raw)));
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p: u, set, img: None })), ..Default::default() }
            }
            k => Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: k, p: self.p.clone(), set: None, img: None })), ..Default::default() },
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Osc {
    pub pan: f32,
    pub shape: u8,
    /// Frequency against the note.
    pub ratio: f32,
    /// Start phase, 0..1.
    pub phase: f32,
    /// Frequency ratio between the right and left copies.
    pub spread: f32,
    pub invert: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Tone {
    pub osc: [Osc; 3],
    pub mix2: f32,
    pub mix3: f32,
    pub random: f32,
    pub am: bool,
    /// Set for the additive instrument, which has its own voice.
    pub add: Option<std::sync::Arc<crate::additive::Patch>>,
    /// Set for the six-operator instrument.
    pub fm: Option<std::sync::Arc<crate::fm::Patch>>,
    /// Set for the small instruments.
    pub mini: Option<std::sync::Arc<crate::minis::Patch>>,
}

/// Running state of one sounding note.
#[derive(Debug, Clone, Default)]
pub struct ToneState {
    ph: [[f32; 2]; 3],
    seed: u32,
    hold: [f32; 3],
    add: Option<Box<crate::additive::Voice>>,
    fm: Option<Box<crate::fm::Voice>>,
    mini: Option<crate::minis::Voice>,
}

#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
fn wave(shape: u8, t: f32, dt: f32) -> f32 {
    match shape {
        1 => {
            let x = t + 0.25;
            let x = x - x.floor();
            1.0 - 4.0 * (x - 0.5).abs()
        }
        2 => {
            let mut v = if t < 0.5 { 1.0 } else { -1.0 };
            v += blep(t, dt);
            let t2 = t + 0.5;
            v -= blep(t2 - t2.floor(), dt);
            v
        }
        3 => 2.0 * t - 1.0 - blep(t, dt),
        4 => {
            // a saw with the edge rounded off: the rising half of a parabola
            let x = 2.0 * t - 1.0;
            x * (2.0 - x.abs()) - blep(t, dt)
        }
        _ => (t * TAU).sin(),
    }
}

impl ToneState {
    pub fn start(tone: &Tone, seed: u32) -> Self {
        let mut s = Self { seed: seed | 1, ..Default::default() };
        if tone.add.is_some() || tone.fm.is_some() || tone.mini.is_some() {
            return s;
        }
        for k in 0..3 {
            let r = if tone.random > 0.0 { s.rand() * 0.5 + 0.5 } else { 0.0 };
            let ph = (tone.osc[k].phase + r * tone.random).fract();
            s.ph[k] = [ph, ph];
        }
        s
    }

    #[inline]
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Tell the voice about its note; the additive instrument needs this before the first frame.
    pub fn note(&mut self, tone: &Tone, sr: f32, key: u8, vel: f32, dphase: f32) {
        if let Some(p) = &tone.add {
            self.add = Some(Box::new(crate::additive::Voice::start(p, sr, key, vel, dphase, self.seed)));
        }
        if let Some(p) = &tone.fm {
            self.fm = Some(Box::new(crate::fm::Voice::start(p, sr, key, vel, dphase, self.seed)));
        }
        if let Some(p) = &tone.mini {
            self.mini = Some(crate::minis::Voice::start(p, sr, key, vel, dphase, self.seed));
        }
    }

    /// The key was let go. Returns the seconds the instrument's own envelopes still need.
    pub fn release(&mut self, tone: &Tone) -> f32 {
        if let (Some(p), Some(v)) = (&tone.mini, &mut self.mini) {
            return v.release(p);
        }
        match (&tone.fm, &mut self.fm) {
            (Some(p), Some(v)) => {
                v.release();
                p.tail_s(v.seconds())
            }
            _ => 0.0,
        }
    }

    /// One frame. `dphase` is the note's cycles per frame.
    #[inline]
    pub fn frame(&mut self, tone: &Tone, dphase: f32) -> [f32; 2] {
        if let (Some(p), Some(v)) = (&tone.add, &mut self.add) {
            return v.frame(p, dphase);
        }
        if let (Some(p), Some(v)) = (&tone.fm, &mut self.fm) {
            return v.frame(p, dphase);
        }
        if let (Some(p), Some(v)) = (&tone.mini, &mut self.mini) {
            return v.frame(p, dphase);
        }
        let mut o = [[0.0f32; 2]; 3];
        for k in 0..3 {
            let osc = &tone.osc[k];
            let base = dphase * osc.ratio;
            if osc.shape == 5 {
                // noise, stepped at the oscillator's pitch so the note still has a colour
                let p = &mut self.ph[k][0];
                *p += (base * 64.0).min(1.0);
                if *p >= 1.0 {
                    *p -= 1.0;
                    self.hold[k] = self.rand();
                }
                o[k] = [self.hold[k]; 2];
            } else {
                let half = osc.spread.sqrt();
                for (c, d) in [(0usize, base / half), (1usize, base * half)] {
                    let d = d.min(0.49);
                    let p = &mut self.ph[k][c];
                    o[k][c] = wave(osc.shape, *p, d);
                    *p += d;
                    if *p >= 1.0 {
                        *p -= 1.0;
                    }
                }
            }
            if osc.invert {
                o[k] = [-o[k][0], -o[k][1]];
            }
            let (l, r) = (1.0 - osc.pan.max(0.0), 1.0 + osc.pan.min(0.0));
            o[k][0] *= l;
            o[k][1] *= r;
        }
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let a = o[0][c] + (o[1][c] - o[0][c]) * tone.mix2;
            out[c] = if tone.am {
                // the third oscillator shapes the level of the other two
                a * (1.0 + (o[2][c] - 1.0) * tone.mix3)
            } else {
                a + (o[2][c] - a) * tone.mix3
            } * 0.4;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_round_trips() {
        let g = Gen::new(Kind::Osc3);
        let fresh = g.encode(None);
        assert_eq!(fresh.len(), 92);
        assert_eq!(Gen::decode("3x Osc", &fresh).unwrap(), g);
        let mut odd = fresh.clone();
        odd.extend_from_slice(&[9; 368]);
        let mut g = Gen::decode("3x Osc", &odd).unwrap();
        g.p[1] = 2.0;
        g.p[6] = 64.0;
        g.p[24] = 1.0;
        let out = g.encode(None);
        assert_eq!(out.len(), 460);
        assert_eq!(&out[92..], &odd[92..]);
        assert_eq!(Gen::decode("3x Osc", &out).unwrap(), g);
    }

    #[test]
    fn additive_blocks_of_every_layout_are_read_and_kept() {
        let g = Gen::new(Kind::Additive);
        let fresh = g.encode(None);
        assert_eq!(fresh.len(), 437);
        assert_eq!(Gen::decode("Harmless", &fresh).unwrap(), g);
        // the first layout lacks the timbre choice and the custom wave count
        let mut old = fresh[..420].to_vec();
        old.extend_from_slice(&fresh[424..432]);
        assert_eq!(old.len(), 428);
        let o = Gen::decode("Harmless", &old).unwrap();
        assert_eq!(o.p, g.p);
        // a block with a custom wave keeps it through a settings change
        let mut big = fresh[..432].to_vec();
        big.extend_from_slice(&2i32.to_le_bytes());
        big.extend_from_slice(&[7; 16]);
        let mut c = Gen::decode("Harmless", &big).unwrap();
        c.p[crate::additive::ix::CUTOFF] = 1000.0;
        let out = c.encode(None);
        assert_eq!(&out[432..], &big[432..]);
        assert_eq!(i32::from_le_bytes(out[220..224].try_into().unwrap()), 1000);
    }

    #[test]
    fn oscillators_sound_at_pitch() {
        let mut g = Gen::new(Kind::Osc3);
        g.p[1] = 0.0; // sine
        let tone = g.tone();
        let mut st = ToneState::start(&tone, 7);
        let dphase = 441.0 / 44100.0;
        let x: Vec<f32> = (0..44100).map(|_| st.frame(&tone, dphase)[0]).collect();
        let crossings = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((440..=442).contains(&crossings), "{crossings}");
        // every shape stays in range
        for shape in 0..7 {
            g.p[1] = shape as f32;
            g.p[6] = 64.0;
            g.p[13] = 40.0;
            g.p[5] = 30.0;
            let tone = g.tone();
            let mut st = ToneState::start(&tone, 3);
            let peak = (0..20000).map(|_| st.frame(&tone, 0.01)).fold(0.0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()));
            assert!(peak > 0.05 && peak < 1.0, "shape {shape} peak {peak}");
        }
    }
    #[test]
    fn the_larger_additive_block_becomes_an_additive_patch() {
        use crate::additive::ix;
        let mut b = vec![0u8; 2000];
        let mut put = |off: usize, v: i32| b[off..off + 4].copy_from_slice(&v.to_le_bytes());
        put(28, 500);
        put(148, 5);
        put(160, 192);
        put(220, 4096);
        put(268, 8192);
        put(240, 300);
        put(312, 600);
        let g = Gen::decode("Harmor", &b).unwrap();
        assert_eq!(g.kind, Kind::Forge);
        let t = g.tone();
        let p = t.add.expect("played by the additive engine");
        assert_eq!(p.p[ix::UNI_ORDER], 5.0);
        assert_eq!(p.p[ix::UNI_PAN], 96.0);
        assert_eq!(p.p[ix::CUTOFF], 5120.0);
        assert_eq!(p.p[ix::RESONANCE], 300.0);
        assert_eq!(p.p[ix::PLUCK], 600.0);
        let (amp, _) = p.custom.as_ref().expect("a half square has its own table");
        assert!((amp[0] - 1.0).abs() < 1e-6 && (amp[1] - 0.25).abs() < 1e-6);
        // untouched settings are stored back byte for byte
        assert_eq!(g.encode(None), b);
    }
    #[test]
    fn a_pack_player_block_gets_a_stand_in_voice_by_name() {
        use crate::additive::ix;
        let mut b = vec![0u8; 2707];
        b[16..20].copy_from_slice(b"Pack");
        b[1041..1049].copy_from_slice(b"Warm Pad");
        let g = Gen::decode("FLEX", &b).unwrap();
        assert_eq!(g.pack_and_preset(), ("Pack".to_string(), "Warm Pad".to_string()));
        let p = g.tone().add.unwrap();
        assert!(p.p[ix::ATTACK] > 0.0 && p.p[ix::UNI_ORDER] > 1.0);
        assert_eq!(stand_in("808 Deep").p[ix::UNI_ORDER], 1.0);
        assert_eq!(g.encode(None), b);
    }
    #[test]
    fn stand_in_synths_take_their_envelope_and_drums_their_sweep() {
        use crate::additive::ix;
        let mut b = vec![0u8; 700];
        let mut put = |i: usize, v: f32| b[12 + i * 4..16 + i * 4].copy_from_slice(&v.to_le_bytes());
        put(22, 0.5);
        put(24, 0.9);
        put(25, 0.4);
        put(5, 1.0);
        let g = Gen::decode("PoiZone", &b).unwrap();
        let p = g.tone().add.unwrap();
        assert_eq!((p.p[ix::ATTACK], p.p[ix::RELEASE], p.p[ix::DECAY], p.p[ix::CUTOFF]), (500.0, 400.0, 1000.0, 8192.0));
        assert_eq!(g.encode(None), b);
        let mut k = vec![0u8; 1384];
        for (i, v) in [(1usize, 0.5f32), (2, 0.5), (3, 1.0), (4, 0.5)] {
            k[1292 + i * 4..1296 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        let g = Gen::decode("BassDrum", &k).unwrap();
        assert_eq!(g.p, vec![2100.0, -900.0, 64.0, 148.0, 32.0, 0.0]);
        assert!(g.tone().mini.is_some());
        assert!(Gen::decode("drumpad", &[0u8; 300]).unwrap().tone().mini.is_some());
        assert_eq!(Gen::decode("Kepler", &[1u8; 178]).unwrap().encode(None), vec![1u8; 178]);
    }
}
