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
    /// Sixteen modelled drums on pads with a step runner (`drumkit.rs`).
    Kit,
    /// One drum patch per key (`drumbank.rs`).
    DrumBank,
    /// A picture swept from left to right, each row a sine wave (`beep.rs`).
    Image,
    /// The channel's sample played as short grains (`grain.rs`).
    Grain,
    /// A sample named in the block, played straight for now (the drawn travel paths are not read).
    Travel,
    /// The small three-oscillator analog-style synth and its wider sibling (`analog.rs`).
    Buzz,
    /// The six-voice vintage poly: its stored values, 0 to 1, in file order (`analog::poly`).
    Poly,
    /// The acid bass line synth (`analog::acid`).
    Acid,
    /// The larger vintage poly (`analog::poly2`).
    Poly2,
    /// The small touch-screen synth: its stored values, 0 to 1, in file order (`analog::pocket`).
    Pocket,
    /// The saw synth: its first ninety-six stored values in file order (`analog::saw`).
    SawSyn,
    /// The two-oscillator synth with sync, ring, a gate and its own echo: its stored values in file order (`duo.rs`).
    Duo,
    /// The two-string model with its touch, links, body, echo, chorus and room: its stored values in file order (`strmodel.rs`).
    StrModel,
    Triple,
    /// Six oscillators that bend each other, with a filter: its stored list of numbers (`hybrid.rs`).
    Hybrid,
    /// Four generators of sine overtones blended along a path.
    Morph,
    KickD,
    Skin,
    /// The three-oscillator analogue: seventy stored values, 0 to 1, in file order, then the switch word (`trio::set`).
    Trio,
    /// The electric guitar (`guitar.rs`).
    Guitar,
    /// A plucked string made from a wearing loop of noise (`minis.rs`).
    String,
    BuzzXt,
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
        matches!(self.kind, Kind::Forge | Kind::Packs | Kind::Metal | Kind::MetalBank | Kind::Sub | Kind::DrumBank | Kind::Quiet | Kind::Poly2 | Kind::Acid)
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

/// The settings both analog-style synths share, in stored order (the wider one keeps eleven more we cannot place).
const BUZZ: &[Param] = &[
    p("Osc 1 shape", 4, 0, 3, 0),
    p("Osc 1 coarse", 8, 0, 128, 64),
    p("Osc 1 fine", 12, 0, 128, 64),
    p("Osc 2 shape", 16, 0, 3, 0),
    p("Osc 2 coarse", 20, 0, 128, 64),
    p("Osc 2 fine", 24, 0, 128, 64),
    p("Osc 3 shape", 28, 0, 1, 0),
    p("Osc 3 amount", 32, 0, 128, 0),
    p("Osc mix", 36, 0, 128, 64),
    p("Pulse width", 40, 0, 128, 0),
    p("FM amount", 44, 0, 128, 0),
    p("Ring mod", 48, 0, 1, 0),
    p("Amp attack", 52, 0, 128, 0),
    p("Amp decay", 56, 0, 128, 64),
    p("Amp sustain", 60, 0, 128, 128),
    p("Amp release", 64, 0, 128, 16),
    p("Filter attack", 68, 0, 128, 0),
    p("Filter decay", 72, 0, 128, 64),
    p("Filter sustain", 76, 0, 128, 64),
    p("Filter release", 80, 0, 128, 16),
    p("Key tracking", 84, 0, 1, 0),
    p("Filter type", 88, 0, 5, 0),
    p("Cutoff", 92, 0, 512, 512),
    p("Resonance", 96, 0, 128, 0),
    p("Envelope amount", 100, 0, 128, 0),
    p("LFO 1 shape", 104, 0, 3, 2),
    p("LFO 1 target", 108, 0, 2, 1),
    p("LFO 1 amount", 112, 0, 128, 0),
    p("LFO 1 speed", 116, 0, 128, 64),
    p("LFO 1 sync", 120, 0, 1, 0),
    p("LFO 1 reset", 124, 0, 1, 0),
    p("LFO 2 shape", 128, 0, 3, 2),
    p("LFO 2 target", 132, 0, 2, 0),
    p("LFO 2 amount", 136, 0, 128, 0),
    p("LFO 2 speed", 140, 0, 128, 64),
    p("LFO 2 sync", 144, 0, 1, 0),
    p("LFO 2 reset", 148, 0, 1, 0),
    p("Drive on", 152, 0, 1, 0),
    p("Drive", 156, 0, 128, 0),
    p("Tone", 160, 0, 128, 64),
    p("Dual voice", 164, 0, 1, 0),
];

/// The later version of the same synth: the same settings, then eleven more. Which of the first two is which,
/// and what the last two are, is read off how the stored sounds use them.
const BUZZ_XT: &[Param] = &[
    p("Osc 1 shape", 4, 0, 3, 0),
    p("Osc 1 coarse", 8, 0, 128, 64),
    p("Osc 1 fine", 12, 0, 128, 64),
    p("Osc 2 shape", 16, 0, 3, 0),
    p("Osc 2 coarse", 20, 0, 128, 64),
    p("Osc 2 fine", 24, 0, 128, 64),
    p("Osc 3 shape", 28, 0, 1, 0),
    p("Osc 3 amount", 32, 0, 128, 0),
    p("Osc mix", 36, 0, 128, 64),
    p("Pulse width", 40, 0, 128, 0),
    p("FM amount", 44, 0, 128, 0),
    p("Ring mod", 48, 0, 1, 0),
    p("Amp attack", 52, 0, 128, 0),
    p("Amp decay", 56, 0, 128, 64),
    p("Amp sustain", 60, 0, 128, 128),
    p("Amp release", 64, 0, 128, 16),
    p("Filter attack", 68, 0, 128, 0),
    p("Filter decay", 72, 0, 128, 64),
    p("Filter sustain", 76, 0, 128, 64),
    p("Filter release", 80, 0, 128, 16),
    p("Key tracking", 84, 0, 1, 0),
    p("Filter type", 88, 0, 5, 0),
    p("Cutoff", 92, 0, 512, 512),
    p("Resonance", 96, 0, 128, 0),
    p("Envelope amount", 100, 0, 128, 0),
    p("LFO 1 shape", 104, 0, 3, 2),
    p("LFO 1 target", 108, 0, 2, 1),
    p("LFO 1 amount", 112, 0, 128, 0),
    p("LFO 1 speed", 116, 0, 128, 64),
    p("LFO 1 sync", 120, 0, 1, 0),
    p("LFO 1 reset", 124, 0, 1, 0),
    p("LFO 2 shape", 128, 0, 3, 2),
    p("LFO 2 target", 132, 0, 2, 0),
    p("LFO 2 amount", 136, 0, 128, 0),
    p("LFO 2 speed", 140, 0, 128, 64),
    p("LFO 2 sync", 144, 0, 1, 0),
    p("LFO 2 reset", 148, 0, 1, 0),
    p("Drive on", 152, 0, 1, 0),
    p("Drive", 156, 0, 128, 0),
    p("Tone", 160, 0, 128, 64),
    p("Dual voice", 164, 0, 1, 0),
    p("True noise", 168, 0, 1, 0),
    p("Analogue drift", 172, 0, 1, 1),
    p("Mod attack", 176, 0, 128, 11),
    p("Mod decay", 180, 0, 128, 64),
    p("Mod amount", 184, 0, 128, 64),
    p("Mod to pulse width", 188, 0, 1, 0),
    p("Mod to LFO 1 amount", 192, 0, 1, 0),
    p("Mod to osc 1 level", 196, 0, 1, 0),
    p("Mod to osc 2 pitch", 200, 0, 1, 0),
    p("Velocity to level", 204, 0, 128, 64),
    p("Velocity to filter", 208, 0, 128, 0),
];

/// The electric guitar's settings; `guitar.rs` reads them by position in this list.
const GUITAR: &[Param] = &[
    p("String type", 4, 0, 6, 1),
    p("Pickup", 8, 0, 2, 1),
    p("Tone", 12, 0, 65535, 32768),
    p("Slap", 16, 0, 65535, 0),
    p("Fret noise", 20, 0, 65535, 8000),
    p("Harmonics", 24, 0, 65535, 40000),
    p("Velocity", 28, 0, 65535, 32768),
    p("Damping", 32, 0, 65535, 16000),
    p("Damping by velocity", 36, 0, 65535, 0),
    p("Amp type", 52, 0, 3, 1),
    p("Drive", 56, 0, 65535, 20000),
    p("Presence", 60, 0, 65535, 32768),
    p("Feedback", 64, 0, 65535, 0),
    p("Low", 68, 0, 65535, 32768),
    p("Mid", 72, 0, 65535, 32768),
    p("High", 76, 0, 65535, 32768),
    p("Cabinet", 80, 0, 3, 2),
    p("Effect", 84, 0, 15, 0),
    p("Effect setting 1", 88, 0, 65535, 32768),
    p("Effect setting 2", 92, 0, 65535, 32768),
    p("Play mode", 100, 0, 5, 0),
    p("Speed", 104, 0, 65535, 0),
    p("Hold", 108, 0, 1, 0),
];

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
        p("Velocity to attack", 120, 0, 1, 0),
        p("Velocity to release", 124, 0, 1, 0),
        p("Pluck", 128, 0, 1000, 1000),
        p("Velocity to pluck", 132, 0, 1, 0),
        p("Pluck alternative", 136, 0, 1, 0),
        p("Pluck masked", 140, 0, 1, 0),
        p("Pluck on release", 144, 0, 1, 0),
        p("Volume", 148, 0, 2000, 1000),
        p("Velocity to volume", 152, 0, 1, 1),
        p("Pitch", 156, -2400, 2400, 0),
        p("Harmonic detune", 160, -2400, 2400, 0),
        p("Grit", 164, -1000, 1000, 0),
        p("Vibrato", 168, 0, 1000, 0),
        p("Pitch LFO", 172, -1000, 1000, 0),
        p("Portamento", 176, 0, 1, 0),
        p("Legato", 180, 0, 1, 0),
        p("Slide time", 184, -1000, 1000, 400),
        p("Velocity to slide", 188, 0, 1, 0),
        p("Adaptive envelope", 192, 0, 1, 1),
        p("Filter attack", 196, 0, 1000, 0),
        p("Filter decay", 200, 0, 1000, 500),
        p("Attack slope", 204, -1000, 1000, 0),
        p("Decay slope", 208, -1000, 1000, -300),
        p("Envelope amount", 212, -1000, 1000, 500),
        p("Velocity to envelope", 216, 0, 1, 0),
        p("Cutoff", 220, 0, 8192, 4096),
        p("Cutoff in Hz", 224, 0, 1, 0),
        p("Filter width", 228, 0, 1000, 500),
        p("Key tracking", 232, -2000, 2000, 0),
        p("Cutoff LFO", 236, -1000, 1000, 0),
        p("Resonance", 240, 0, 1000, 0),
        p("Resonance in Hz", 244, 0, 1, 0),
        p("Resonance offset", 248, -2400, 2400, 0),
        p("Self oscillation", 252, 0, 2048, 0),
        p("Adaptive width", 256, 0, 1, 0),
        p("Motion only", 260, 0, 1, 0),
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
        p("Delay time", 384, 0, 384, 144),
        p("Reverb type", 388, 0, 6, 0),
        p("Reverb volume", 392, 0, 1000, 500),
        p("Compressor mix", 396, 0, 1000, 500),
        p("Velocity to unison pitch", 400, 0, 1, 0),
        p("Filter type", 404, 0, 16, 4),
        p("Phaser type", 408, 0, 9, 0),
        p("Resonance type", 412, 0, 9, 0),
        p("Compressor type", 416, 0, 4, 0),
        p("Timbre", 420, 0, 11, 0),
        p("High quality", 428, 0, 1, 0),
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
            p("Brightness", 48, 0, 65535, 29295),
            p("Mod 1 through", 52, 0, 65535, 0),
            p("Vibrato rate", 56, 0, 65535, 27132),
            p("Spare", 60, 0, 65535, 0),
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
    Def { kind: Kind::Duo, ident: "PoiZone", title: "Duo Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::SawSyn, ident: "Sawer", title: "Saw Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::StrModel, ident: "Sakura", title: "String Model", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Hybrid, ident: "Toxic Biohazard", title: "Hybrid Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Buzz, ident: "Wasp", title: "Hornet", size: 169, fixed: &[(0, 1)], tail: &[], params: BUZZ },
    Def { kind: Kind::BuzzXt, ident: "Wasp XT", title: "Hornet XT", size: 213, fixed: &[(0, 11)], tail: &[], params: BUZZ_XT },
    Def {
        kind: Kind::String,
        ident: "Plucked!",
        title: "Pluck String",
        size: 20,
        fixed: &[],
        tail: &[],
        params: &[p("Decay", 0, 0, 128, 128), p("Colour", 4, 0, 128, 64), p("Even decay", 8, 0, 1, 1), p("Gate", 12, 0, 1, 0), p("Widen", 16, 0, 1, 1)],
    },
    Def { kind: Kind::Guitar, ident: "FL Slayer", title: "Six String", size: 116, fixed: &[], tail: &[], params: GUITAR },
    Def { kind: Kind::Poly, ident: "Kepler", title: "Vintage Poly", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Poly2, ident: "Kepler Exo", title: "Vintage Poly II", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Acid, ident: "Transistor Bass", title: "Acid Line", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Dashboard", title: "Control Panel", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Dance", title: "Dancer", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Video Player", title: "Video Player", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "ReWired", title: "Rewire Link", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "FL Studio Mobile", title: "Mobile Rack", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "FL Studio Mobile Rack", title: "Mobile Modules", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Slicer 2", title: "Loop Slicer 2", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Pocket, ident: "MiniSynth", title: "Pocket Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Triple, ident: "GMS", title: "Triple Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Morph, ident: "Morphine", title: "Morph Additive", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Trio, ident: "SimSynth", title: "Analog Trio", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::KickD, ident: "BassDrum", title: "Kick Designer", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Skin, ident: "Drumpad", title: "Membrane Drum", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Image, ident: "BeepMap", title: "Picture Synth", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Keyboard Controller", title: "Key Controller", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Envelope Controller", title: "Envelope Sender", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "Fruity Voltage Controller", title: "Voltage Sender", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Quiet, ident: "MIDI Out", title: "MIDI Sender", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Grain, ident: "Fruity Granulizer", title: "Grain Player", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Travel, ident: "Wave Traveller", title: "Path Scratcher", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::DrumBank, ident: "Fruity DrumSynth Live", title: "Drum Bank", size: 0, fixed: &[], tail: &[], params: &[] },
    Def { kind: Kind::Kit, ident: "Drumaxx", title: "Drum Modeler", size: 0, fixed: &[], tail: &[], params: &[] },
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
    SubMap { ident: "Sawer", base: 12, adsr: Some([2, 3, 4, 5]), cutoff: None, voice: "lead" },
];

/// The values of the vintage polys: a name with its length in front at `at`, a version, a count, then that many
/// numbers from 0 to 1.
fn poly_values(state: &[u8], at: usize) -> Vec<f32> {
    let word = |o: usize| state.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let Some(name) = word(at).filter(|&n| n < 64) else { return Vec::new() };
    let from = at + 4 + name + 8;
    let Some(count) = word(from - 4).filter(|&n| n < 4096) else { return Vec::new() };
    let Some(data) = state.get(from..from + count * 4) else { return Vec::new() };
    data.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }).collect()
}

/// The values of the small touch-screen synth: a tagged file whose data part holds a short head and then one
/// number from 0 to 1 per control.
/// The analogue trio's block: seventy eight-byte numbers and a word of switches. Empty when it is not that.
fn trio_values(state: &[u8]) -> Vec<f32> {
    if state.len() < 564 {
        return Vec::new();
    }
    let mut p: Vec<f32> = state[..560].chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap()) as f32).collect();
    p.push(u32::from_le_bytes([state[560], state[561], state[562], state[563]]) as f32);
    // nineteen on/off words sit where numbers 54 to 63 would be
    for k in 0..19 {
        let o = 436 + k * 4;
        p.push(u32::from_le_bytes([state[o], state[o + 1], state[o + 2], state[o + 3]]).min(1) as f32);
    }
    p
}

/// The values of a block that opens with a four-letter mark, its length and a version mark: `count` numbers
/// from the twelfth byte. Empty when the block is not of that make or too short.
fn tagged_values(state: &[u8], count: usize) -> Vec<f32> {
    if state.len() < 12 + count * 4 || &state[..4] != b"SSLF" {
        return Vec::new();
    }
    state[12..12 + count * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn pocket_values(state: &[u8]) -> Vec<f32> {
    let Some(at) = state.windows(4).position(|w| w == b"DATA") else { return Vec::new() };
    let Some(n) = state.get(at + 4..at + 8).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize) else { return Vec::new() };
    let data = state.get(at + 16..(at + 8 + n).min(state.len())).unwrap_or(&[]);
    data.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }).collect()
}

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

/// The voice that plays for a hosted plug-in this machine does not have: picked from the words of the channel
/// and plug-in names, with the stored block kept as it is.
pub fn missing_gen(name: &str, state: &[u8]) -> Gen {
    Gen { kind: Kind::Sub, p: Vec::new(), raw: state.to_vec(), fm: None, pre: Some(std::sync::Arc::new(stand_in(name))), img: None, smp: None }
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
    crate::additive::Patch { p, custom: None, rise: 0.0 }
}

pub fn def(kind: Kind) -> &'static Def {
    DEFS.iter().find(|d| d.kind == kind).expect("every kind has a table entry")
}

pub fn title_for(ident: &str) -> Option<&'static str> {
    // instruments that are not settings tables: sample maps and the loop slicer
    const OTHER: &[(&str, &str)] = &[("FPC", "Pad Machine"), ("DirectWave", "Zone Sampler"), ("Fruity Slicer", "Loop Slicer"), ("Slicex", "Marker Slicer"), ("Soundfont Player", "Sound Bank Player"), ("Fruity Soundfont Player", "Sound Bank Player")];
    DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(ident)).map(|d| d.title).or_else(|| OTHER.iter().find(|o| o.0.eq_ignore_ascii_case(ident)).map(|o| o.1))
}

/// One instrument's settings, in file units, in the order of its table.
/// The file name of the key sender, the silent channel that turns notes into control movements.
pub const KEY_SENDER: &str = "Fruity Keyboard Controller";

/// What the key sender gives for a key: its stored table has a value per key after two leading words;
/// without a table the keys run evenly from the bottom of the keyboard to the top.
pub fn key_sender_value(raw: &[u8], key: u8) -> f32 {
    let at = 8 + 4 * key as usize;
    if key < 120 && raw.len() >= 8 + 480 {
        return f32::from_le_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]]).clamp(0.0, 1.0);
    }
    (key as f32 / 119.0).min(1.0)
}

/// Writes a key's value into the key sender's table, making the table first when there is none.
pub fn key_sender_set(raw: &mut Vec<u8>, key: u8, v: f32) {
    if raw.len() < 501 {
        let mut b = vec![0u8; 501];
        for k in 0..120u8 {
            b[8 + 4 * k as usize..12 + 4 * k as usize].copy_from_slice(&(k as f32 / 119.0).to_le_bytes());
        }
        b[488] = 0;
        b[492] = 119;
        *raw = b;
    }
    if key < 120 {
        let at = 8 + 4 * key as usize;
        raw[at..at + 4].copy_from_slice(&v.clamp(0.0, 1.0).to_le_bytes());
    }
}

/// The key sender's smoothing and note-off value: (seconds to reach a pressed key's value, seconds to reach
/// the note-off value, the note-off value when its switch is on). No stored example sets any of these, so
/// their places are our guess: the two leading words as the times, a value and a switch byte at the end.
pub fn key_sender_extra(raw: &[u8]) -> (f32, f32, Option<f32>) {
    if raw.len() < 501 {
        return (0.0, 0.0, None);
    }
    let f = |o: usize| {
        let v = f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
        if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
    };
    // the knobs run to two seconds
    (f(0) * 2.0, f(4) * 2.0, (raw[500] != 0).then(|| f(496)))
}

/// Writes the smoothing times (as knob positions 0..1) and the note-off value with its switch.
pub fn key_sender_set_extra(raw: &mut Vec<u8>, attack: f32, release: f32, off: Option<f32>) {
    key_sender_set(raw, 255, 0.0);
    raw[0..4].copy_from_slice(&attack.clamp(0.0, 1.0).to_le_bytes());
    raw[4..8].copy_from_slice(&release.clamp(0.0, 1.0).to_le_bytes());
    if let Some(v) = off {
        raw[496..500].copy_from_slice(&v.clamp(0.0, 1.0).to_le_bytes());
    }
    raw[500] = off.is_some() as u8;
}

/// The key range the key sender's automatic mappings work over.
pub fn key_sender_range(raw: &[u8]) -> (u8, u8) {
    if raw.len() < 496 {
        return (0, 119);
    }
    let lo = u32::from_le_bytes([raw[488], raw[489], raw[490], raw[491]]).min(119) as u8;
    let hi = u32::from_le_bytes([raw[492], raw[493], raw[494], raw[495]]).min(119) as u8;
    (lo.min(hi), hi.max(lo))
}

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
    /// The channel's sample, for the grain player.
    pub smp: Option<std::sync::Arc<crate::sample::Sample>>,
}

/// Two sets of settings are the same when they would be stored as the same block.
impl PartialEq for Gen {
    fn eq(&self, o: &Self) -> bool {
        self.kind == o.kind && self.p == o.p && (self.raw == o.raw || self.encode(None) == o.encode(None))
    }
}

/// The sample a block names for itself, for the units that play one.
pub fn sample_named_in(ident: &str, state: &[u8]) -> Option<String> {
    if !ident.eq_ignore_ascii_case("Wave Traveller") {
        return None;
    }
    let s = state.get(20..)?;
    // the name's length sits just ahead of it; some blocks close the name with a zero as well
    let len = state.get(16..20).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).filter(|&n| n <= s.len());
    let s = &s[..len.unwrap_or(s.len())];
    let s = &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())];
    let name = String::from_utf8_lossy(s).into_owned();
    (name.len() > 4 && name.contains('.') && s.iter().all(|&b| b >= 0x20)).then_some(name)
}

impl Gen {
    pub fn new(kind: Kind) -> Self {
        let mut g = Self { kind, p: def(kind).params.iter().map(|p| p.def as f32).collect(), raw: Vec::new(), fm: None, pre: None, img: None, smp: None };
        g.raw = g.encode(None);
        if kind == Kind::Travel {
            g.raw = crate::travel::fresh();
        }
        if kind == Kind::Grain {
            g.raw = crate::grain::fresh();
        }
        if kind == Kind::Image {
            g.raw = crate::beep::fresh();
            g.img = crate::beep::Image::parse(&g.raw).map(std::sync::Arc::new);
        }
        if matches!(kind, Kind::Trio | Kind::Pocket | Kind::SawSyn) {
            g.p = crate::analog::plain(kind);
            g.raw = crate::analog::fresh(kind);
        }
        if kind == Kind::Poly {
            g.p = crate::analog::poly_defaults();
            g.raw = crate::analog::poly_fresh();
        }
        if kind == Kind::Duo {
            g.p = crate::duo::defaults();
            g.raw = crate::duo::fresh();
        }
        if kind == Kind::StrModel {
            g.p = crate::strmodel::defaults();
            g.raw = crate::strmodel::fresh();
        }
        if kind == Kind::Triple {
            g.p = crate::triple::defaults();
            g.raw = crate::triple::fresh();
        }
        if kind == Kind::Hybrid {
            g.p = crate::hybrid::defaults();
            g.raw = crate::hybrid::fresh();
        }
        if kind == Kind::Kit {
            g.p = crate::drumkit::defaults();
            g.raw = crate::drumkit::fresh();
        }
        if kind == Kind::Skin {
            g.p = crate::membrane::defaults();
            g.raw = crate::membrane::fresh();
        }
        if kind == Kind::KickD {
            g.p = crate::kickd::defaults();
            g.raw = crate::kickd::fresh();
        }
        if kind == Kind::Morph {
            g.p = crate::morph::defaults();
            g.raw = crate::morph::fresh();
        }
        if kind == Kind::Fm {
            // a new one carries a full block of its own, so every page of its window can be edited and saved
            g.raw = crate::fm::Patch::blank_state();
            g.fm = Some(crate::fm::Patch::parse(&g.raw).map(std::sync::Arc::new).unwrap_or_default());
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
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: Some(std::sync::Arc::new(patch)), pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Sub {
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: Some(std::sync::Arc::new(sub_patch(ident, state))), img: None, smp: None });
        }
        if d.kind == Kind::Image {
            // a block without a readable picture stays with the project and plays nothing
            let img = crate::beep::Image::parse(state).map(std::sync::Arc::new);
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: None, img, smp: None });
        }
        if d.kind == Kind::Poly2 {
            return Some(Self { kind: d.kind, p: poly_values(state, 28), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Poly {
            return Some(Self { kind: d.kind, p: poly_values(state, 12), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Duo {
            return Some(Self { kind: d.kind, p: tagged_values(state, crate::duo::COUNT), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::StrModel {
            return Some(Self { kind: d.kind, p: tagged_values(state, crate::strmodel::COUNT), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Hybrid {
            return Some(Self { kind: d.kind, p: crate::hybrid::values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Skin {
            return Some(Self { kind: d.kind, p: crate::membrane::values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::KickD {
            return Some(Self { kind: d.kind, p: crate::kickd::values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Morph {
            return Some(Self { kind: d.kind, p: crate::morph::values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Triple {
            return Some(Self { kind: d.kind, p: crate::triple::values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::SawSyn {
            return Some(Self { kind: d.kind, p: tagged_values(state, 96), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Trio {
            return Some(Self { kind: d.kind, p: trio_values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Pocket {
            return Some(Self { kind: d.kind, p: pocket_values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if matches!(d.kind, Kind::Quiet | Kind::Grain | Kind::Travel | Kind::Acid) {
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::DrumBank {
            return Some(Self { kind: d.kind, p: crate::drumbank::parse(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if d.kind == Kind::Kit {
            return Some(Self { kind: d.kind, p: crate::drumkit::values(state), raw: state.to_vec(), fm: None, pre: None, img: None, smp: None });
        }
        if matches!(d.kind, Kind::Metal | Kind::MetalBank) {
            let patch = crate::metal::patch(state)?;
            return Some(Self { kind: d.kind, p: Vec::new(), raw: state.to_vec(), fm: None, pre: Some(std::sync::Arc::new(patch)), img: None, smp: None });
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
        let mut g = Self { kind: d.kind, p, raw: state.to_vec(), fm: None, pre: None, img: None, smp: None };
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
        p[ix::PHASE] = v(1).abs();
        p[ix::HZ_AMOUNT] = v(3).abs();
        p[ix::HZ_WIDTH] = v(4);
        p[ix::HZ_STRENGTH] = v(5);
        p[ix::UNI_ORDER] = v(6);
        p[ix::UNI_PAN] = v(7) / 2.0;
        p[ix::UNI_PITCH] = v(8);
        p[ix::UNI_PHASE] = v(9).abs() / 1000.0 * 128.0;
        // the lower of the two filters is the one heard; no envelope moves it yet
        let (cut, res) = if v(14) < v(10) { (v(14), v(15)) } else { (v(10), v(13)) };
        p[ix::F_AMOUNT] = 0.0;
        // a low setting is nearly always opened by a drawn envelope we cannot read yet, so it is kept from closing fully
        p[ix::CUTOFF] = if cut >= 4096.0 { cut + 321.0 } else { 2048.0 + cut * 0.75 };
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
        p[ix::VOLUME] *= 0.943;
        crate::additive::Patch { p, custom, rise: 0.5 }
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
                // a setting that cannot be nothing starts at its rest value (left at nothing, the original
                // falls over on a voice count of none)
                if Self::shift(self.kind, &v) == 0 {
                    let len = v.len();
                    for q in d.params.iter().filter(|q| q.min > 0 && q.off + q.width as usize <= len) {
                        if q.width == 1 {
                            v[q.off] = q.def as u8;
                        } else {
                            v[q.off..q.off + 4].copy_from_slice(&q.def.to_le_bytes());
                        }
                    }
                }
                v
            }
        };
        if self.kind == Kind::Trio {
            // seventy long numbers and the word of switches
            if out.len() >= 564 && self.p.len() >= 71 {
                for (i, &v) in self.p[..70].iter().enumerate() {
                    let o = i * 8;
                    if (54..64).contains(&i) {
                        continue;
                    }
                    if f64::from_le_bytes(out[o..o + 8].try_into().unwrap()) as f32 != v {
                        // the stored numbers are long ones; a short one is written as its nearest decimal
                        out[o..o + 8].copy_from_slice(&(((v as f64) * 1e6).round() / 1e6).to_le_bytes());
                    }
                }
                for (k, &v) in self.p.iter().skip(71).take(19).enumerate() {
                    let o = 436 + k * 4;
                    let word = u32::from(v >= 0.5);
                    if u32::from_le_bytes(out[o..o + 4].try_into().unwrap()).min(1) != word {
                        out[o..o + 4].copy_from_slice(&word.to_le_bytes());
                    }
                }
                let word = self.p[70] as u32;
                if u32::from_le_bytes(out[560..564].try_into().unwrap()) != word {
                    out[560..564].copy_from_slice(&word.to_le_bytes());
                }
            }
            return out;
        }
        if matches!(self.kind, Kind::Duo | Kind::StrModel | Kind::Triple | Kind::Hybrid | Kind::Morph | Kind::KickD | Kind::Skin | Kind::Kit | Kind::Poly | Kind::Pocket | Kind::SawSyn) {
            // a list of numbers (from the twelfth byte for most): only the ones that were moved are written
            let first = match self.kind {
                Kind::Triple => crate::triple::base(&out).unwrap_or(usize::MAX / 8),
                Kind::Hybrid => crate::hybrid::base(&out).unwrap_or(usize::MAX / 8),
                Kind::Morph => crate::morph::base(&out).unwrap_or(usize::MAX / 8),
                Kind::KickD => crate::kickd::FIRST,
                Kind::Skin => crate::membrane::FIRST,
                Kind::Kit => crate::drumkit::FIRST,
                Kind::Pocket => out.windows(4).position(|w| w == b"DATA").map_or(usize::MAX / 8, |at| at + 16),
                // past the name, its version and the count
                Kind::Poly => out.get(12..16).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).filter(|&n| n < 64).map_or(usize::MAX / 8, |n| 24 + n),
                _ => 12,
            };
            for (i, &v) in self.p.iter().enumerate() {
                let o = first + i * 4;
                if out.len() >= o + 4 && f32::from_le_bytes(out[o..o + 4].try_into().unwrap()) != v {
                    out[o..o + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
            return out;
        }
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

    /// Put setting `k` at `v` of its travel (0..=1).
    pub fn set_travel(&mut self, k: usize, v: f32) {
        if let (Some(q), Some(slot)) = (def(self.kind).params.get(k), self.p.get_mut(k)) {
            *slot = q.min as f32 + (q.max as f32 - q.min as f32) * v.clamp(0.0, 1.0);
        }
    }

    /// The effects this instrument carries after its voices.
    pub fn tail(&self) -> crate::tailfx::Set {
        match self.kind {
            Kind::Duo => crate::duo::tail(&self.p),
            Kind::Additive => crate::additive::tail(&self.p),
            Kind::Fm => self.fm.as_ref().map(|f| f.tail).unwrap_or_default(),
            Kind::SawSyn => crate::analog::saw_tail(&self.p),
            Kind::Trio => crate::tailfx::Set { wide: self.p.get(89).is_some_and(|v| *v >= 0.5), ..Default::default() },
            Kind::StrModel => crate::strmodel::tail(&self.p),
            Kind::Triple => crate::triple::tail(&self.p),
            Kind::Hybrid => crate::hybrid::tail(&self.p),
            Kind::Morph => crate::morph::tail(&self.p),
            Kind::Guitar => crate::guitar::tail(&self.p),
            _ => Default::default(),
        }
    }

    /// For an instrument whose slides close in on the goal instead of running straight to it: the time
    /// constant in seconds for a note of this velocity over 128, whether fresh notes slide in too, and
    /// whether a bent note takes the new note's loudness.
    pub fn slide_law(&self, touch: f32) -> Option<(f32, bool, bool)> {
        match self.kind {
            Kind::Additive => Some(crate::additive::Patch::new(&self.p, &[]).slide(touch)),
            _ => None,
        }
    }

    /// For an instrument set to one voice at a time: the seconds a held note takes to slide to the next.
    pub fn mono(&self) -> Option<f32> {
        match self.kind {
            Kind::Duo => crate::duo::set(&self.p).mono,
            Kind::Additive => crate::additive::Patch::new(&self.p, &[]).mono(),
            Kind::Hybrid => crate::hybrid::set(&self.p).mono,
            Kind::Morph => crate::morph::set(&self.p).mono,
            Kind::Triple => (self.p.len() >= crate::triple::COUNT && self.p[40] >= 0.5).then(|| if self.p[11] >= 0.5 { 0.005 + self.p[10].clamp(0.0, 1.0).powi(2) * 2.0 } else { 0.0 }),
            _ => None,
        }
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
                        // the keyboard's C5 is 523 Hz here: an octave above the note number
                        ratio: 2.0 * 2f32.powf((f[2] + f[3] / 100.0) / 12.0),
                        // measured: the knob moves only the right side, up to half a cycle ahead
                        phase: f[4].max(0.0) / 128.0,
                        // measured: the knob raises only the right side, one cent a step
                        spread: 2f32.powf(f[5].max(0.0) / 1200.0),
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
            Kind::Kit => {
                // the cut groups and the step patterns ride along after the list
                let p = if self.p.len() >= crate::drumkit::COUNT { crate::drumkit::voice_list(&self.p, &self.raw) } else { crate::drumkit::voice_list(&crate::drumkit::defaults(), &crate::drumkit::fresh()) };
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p, set: None, img: None, smp: None, path: None })), ..Default::default() }
            }
            Kind::Image => Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p: Vec::new(), set: None, img: self.img.clone(), smp: None, path: None })), ..Default::default() },
            Kind::Grain => Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p: Vec::new(), set: None, img: None, smp: self.smp.clone().map(|s| (s, crate::grain::Set::parse(&self.raw))), path: None })), ..Default::default() },
            Kind::Travel => Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p: Vec::new(), set: None, img: None, smp: None, path: self.smp.clone().map(|s| (s, std::sync::Arc::new(crate::travel::Set::parse(&self.raw)))) })), ..Default::default() },
            Kind::Forge => Tone { add: Some(std::sync::Arc::new(self.forge())), ..Default::default() },
            Kind::Keys => {
                let u = self.p.iter().zip(crate::keys::RANGES).map(|(&v, (lo, hi))| (v - lo as f32) / (hi - lo) as f32).collect();
                let set = Some(crate::keys::set(&crate::keys::name_in(&self.raw)));
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p: u, set, img: None, smp: None, path: None })), ..Default::default() }
            }
            Kind::KickD => {
                // the layered sample rides along as one number after the list
                let mut p = self.p.clone();
                if p.len() >= crate::kickd::COUNT {
                    p.truncate(crate::kickd::COUNT);
                    p.push(crate::kickd::sample_id(&self.raw));
                }
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p, set: None, img: None, smp: None, path: None })), ..Default::default() }
            }
            Kind::Morph => {
                // the snapshots ride along as one number after the list
                let mut p = self.p.clone();
                if p.len() >= crate::morph::COUNT {
                    p.truncate(crate::morph::COUNT);
                    p.push(crate::morph::body_id(&self.raw));
                }
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p, set: None, img: None, smp: None, path: None })), ..Default::default() }
            }
            Kind::Hybrid => {
                let mut p = self.p.clone();
                if p.len() >= crate::hybrid::COUNT {
                    p.truncate(crate::hybrid::COUNT);
                    let ids = crate::hybrid::wave_ids(&self.raw, &p);
                    p.extend(ids);
                }
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p, set: None, img: None, smp: None, path: None })), ..Default::default() }
            }
            Kind::Triple => {
                // the three waves ride along as numbers after the list
                let mut p = self.p.clone();
                if p.len() >= crate::triple::COUNT {
                    p.truncate(crate::triple::COUNT);
                    p.extend(crate::triple::wave_ids(&self.raw));
                }
                Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: self.kind, p, set: None, img: None, smp: None, path: None })), ..Default::default() }
            }
            k => Tone { mini: Some(std::sync::Arc::new(crate::minis::Patch { kind: k, p: self.p.clone(), set: None, img: None, smp: None, path: None })), ..Default::default() },
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

impl Tone {
    /// This instrument sends a share of each note to its own effects, apart from the note's plain sound.
    pub fn sends(&self) -> bool {
        self.fm.as_ref().is_some_and(|p| p.send.iter().any(|&s| s != 0.0))
    }
}

/// Running state of one sounding note.
#[derive(Debug, Clone, Default)]
pub struct ToneState {
    ph: [[f64; 2]; 3],
    seed: u32,
    add: Option<Box<crate::additive::Voice>>,
    fm: Option<Box<crate::fm::Voice>>,
    mini: Option<crate::minis::Voice>,
    /// played live, not rendered: instruments with a quality setting for each use the live one
    pub live: bool,
    /// seconds of the song gone by when the note starts (free-running wobbles keep their place by it)
    pub at: f32,
    /// the key the channel played before this note (the operator synth slides in from it)
    pub from: Option<u8>,
}

/// One plain reading of a shape at `t` of its cycle (measured from single readings of low notes).
#[inline]
fn wave(shape: u8, t: f32) -> f32 {
    match shape {
        0 => (t * TAU).sin(),
        1 => {
            if t < 0.25 {
                4.0 * t
            } else if t < 0.75 {
                2.0 - 4.0 * t
            } else {
                4.0 * t - 4.0
            }
        }
        2 => {
            if t < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        // a fast climb from the bottom that settles at the top, then the last sixth of the cycle at the bottom
        4 => {
            if t < 5.0 / 6.0 {
                1.0 - 2.0 * (-49.26 * t).exp()
            } else {
                -1.0
            }
        }
        // the drawn shape is a saw until a wave is loaded into it
        _ => 2.0 * t - 1.0,
    }
}

impl ToneState {
    pub fn start(tone: &Tone, seed: u32) -> Self {
        let mut s = Self { seed: seed | 1, ..Default::default() };
        if tone.add.is_some() || tone.fm.is_some() || tone.mini.is_some() {
            return s;
        }
        for k in 0..3 {
            // measured: the two sides start at places of their own
            let mut r = || if tone.random > 0.0 { (s.rand() * 0.5 + 0.5) * tone.random } else { 0.0 };
            let (a, b) = (r(), r());
            s.ph[k] = [a.fract() as f64, (b + tone.osc[k].phase).fract() as f64];
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

    /// The tempo the note starts at, for instruments with lines drawn in beats.
    pub fn set_bpm(&mut self, bpm: f32) {
        if let Some(v) = &mut self.fm {
            v.beat = (bpm / 60.0).clamp(0.1, 20.0);
        }
        if let Some(v) = &mut self.add {
            v.beat = (bpm / 60.0).clamp(0.1, 20.0);
        }
    }

    /// Where in the song (seconds from its start) the note begins, for instruments whose voices drift with the clock.
    pub fn set_start(&mut self, tone: &Tone, hz: f32, secs: f64) {
        if let (Some(v), Some(p)) = (&mut self.fm, &tone.fm) {
            v.started_at(p, hz, secs);
        }
    }

    /// Tell a sampled keys voice how long after its instrument's first note it starts.
    pub fn set_joined(&mut self, secs: f64) {
        if let Some(crate::minis::Voice::Keys(v)) = &mut self.mini {
            v.joined(secs);
        }
    }

    /// How long a sampled keys voice held for `hold` seconds counts as sounding (see the voice).
    pub fn keys_sounding(&self, hold: f64, step: f64) -> Option<f64> {
        match &self.mini {
            Some(crate::minis::Voice::Keys(v)) => Some(v.sounding(hold, step)),
            _ => None,
        }
    }

    /// Tell the voice about its note; the additive instrument needs this before the first frame.
    pub fn note(&mut self, tone: &Tone, sr: f32, key: u8, vel: f32, dphase: f32) {
        self.note_at(tone, sr, key, vel, [vel, 0.5, 0.5], dphase)
    }

    /// The same, with the note's place on the velocity, X and Y axes of the operator synth's maps given apart from
    /// its loudness (measured: the first is the velocity over 128, so a default note sits at 0.78; the other two
    /// sit in the middle whatever the instrument's own settings say, so only the note can move them).
    pub fn note_at(&mut self, tone: &Tone, sr: f32, key: u8, vel: f32, axis: [f32; 3], dphase: f32) {
        if let Some(p) = &tone.add {
            let mut v = crate::additive::Voice::start(p, sr, key, vel, dphase, self.seed);
            v.set_touch(p, axis[0]);
            v.at = self.at as f64;
            self.add = Some(Box::new(v));
        }
        if let Some(p) = &tone.fm {
            self.fm = Some(Box::new(crate::fm::Voice::start_as(p, sr, key, axis, dphase, self.seed, self.live, self.from)));
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
        if let (Some(p), Some(v)) = (&tone.add, &mut self.add) {
            return v.release(p);
        }
        match (&tone.fm, &mut self.fm) {
            (Some(p), Some(v)) => {
                v.release();
                p.tail_s(v.seconds(), v.beat)
            }
            _ => 0.0,
        }
    }

    /// What the last frame sent to the instrument's effects, for the instruments that keep that apart.
    #[inline]
    pub fn sent(&self) -> [f32; 2] {
        self.fm.as_ref().map_or([0.0; 2], |v| v.sent())
    }

    /// The same share on the finer frames of an instrument that works several times over (see the voice).
    #[inline]
    pub fn sent_fine(&self) -> &[[f32; 2]] {
        self.fm.as_ref().map_or(&[], |v| v.sent_fine())
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
                // noise (measured: a fresh even-spread value every frame on each side, whatever the pitch)
                o[k] = [self.rand(), self.rand()];
            } else {
                for (c, d) in [(0usize, base), (1usize, base * osc.spread)] {
                    let d = d.min(0.49) as f64;
                    let p = &mut self.ph[k][c];
                    // measured: the cycle is 1024 places long, and a frame that crosses several of them is the
                    // mean of that many plain readings spread evenly over the frame, nothing rounded off; the
                    // place is kept in double precision and moved reading by reading, which puts every edge on
                    // the same frame as the reference for the whole note. The sine is read once, and so is the third
                    // oscillator while it shapes the level of the others.
                    if osc.shape == 0 || (k == 2 && tone.am) {
                        o[k][c] = wave(osc.shape, *p as f32);
                        *p += d;
                        if *p >= 1.0 {
                            *p -= 1.0;
                        }
                    } else {
                        let n = ((d * 1024.0) as usize).max(1);
                        let sub = d / n as f64;
                        let mut a = 0.0f32;
                        for _ in 0..n {
                            a += wave(osc.shape, *p as f32);
                            *p += sub;
                            if *p >= 1.0 {
                                *p -= 1.0;
                            }
                        }
                        o[k][c] = a / n as f32;
                    }
                }
            }
            if osc.invert {
                o[k] = [-o[k][0], -o[k][1]];
            }
            // measured: equal power, level with the plain sound in the middle
            let th = (osc.pan + 1.0) * std::f32::consts::FRAC_PI_4;
            o[k][0] *= th.cos() * std::f32::consts::SQRT_2;
            o[k][1] *= th.sin() * std::f32::consts::SQRT_2;
        }
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let a = o[0][c] + (o[1][c] - o[0][c]) * tone.mix2;
            out[c] = if tone.am {
                // the third oscillator shapes the level of the other two (measured: its top takes the level
                // down, and each side follows its own share of the oscillator's place across the field)
                a * (1.0 - o[2][c] * std::f32::consts::FRAC_1_SQRT_2 * tone.mix3)
            } else {
                a + (o[2][c] - a) * tone.mix3
            } * 0.5; // measured level
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A setting moved while a note sounds: the note goes on with the new settings and stays sound.
    #[test]
    fn settings_can_move_under_a_sounding_note() {
        let mut bad = vec![];
        for d in DEFS {
            let r = std::panic::catch_unwind(|| {
                let mut g = Gen::new(d.kind);
                let tone = g.tone();
                let mut st = ToneState::start(&tone, 12345);
                st.note(&tone, 44100.0, 60, 0.8, 261.6 / 44100.0);
                for _ in 0..600 {
                    st.frame(&tone, 261.6 / 44100.0);
                }
                let mut seed = 99u32;
                for k in 0..d.params.len() {
                    for round in 0..3 {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let v = match round {
                            0 => 0.0,
                            1 => 1.0,
                            _ => (seed >> 8) as f32 / (1u32 << 24) as f32,
                        };
                        g.set_travel(k, v);
                        let t = g.tone();
                        for _ in 0..200 {
                            let f = st.frame(&t, 261.6 / 44100.0);
                            assert!(f[0].is_finite() && f[1].is_finite(), "setting {k} at {v}");
                        }
                    }
                }
            });
            if r.is_err() {
                bad.push(d.ident);
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn the_triple_oscillator_follows_the_measured_voice() {
        let run = |set: &[(usize, f32)], n: usize| {
            let mut g = Gen::new(Kind::Osc3);
            for &(i, v) in set {
                g.p[i] = v;
            }
            let t = g.tone();
            let mut s = ToneState::start(&t, 1);
            // the keyboard's C5 on the default saw: 12 places of the 1024 a frame
            (0..n).map(|_| s.frame(&t, 261.625_58 / 44100.0)).collect::<Vec<_>>()
        };
        // a frame is the mean of twelve readings spread over it
        let step = 523.251_16f64 / 44100.0;
        let saw = run(&[], 200);
        let want: f64 = (0..12).map(|j| 2.0 * (j as f64 * step / 12.0) - 1.0).sum::<f64>() / 12.0 * 0.5;
        assert!((saw[0][0] as f64 - want).abs() < 1e-6, "{} {want}", saw[0][0]);
        // the edge frame sits on a twelfth of the jump: nothing rounded off
        let edge = saw.windows(2).map(|w| w[1][0] - w[0][0]).fold(0.0f32, f32::min);
        assert!((-1.0..-0.5).contains(&edge), "{edge}");
        assert_eq!(saw[50][0], saw[50][1]);
        // the phase and detune knobs move only the right side
        let ph = run(&[(4, 32.0)], 4);
        assert_eq!(ph[0][0], saw[0][0]);
        assert!((ph[0][1] - (saw[0][0] + 0.25)).abs() < 1e-4, "{}", ph[0][1]);
        let de = run(&[(5, 64.0)], 3000);
        assert!((0..200).all(|i| de[i][0] == saw[i][0]) && de[150][1] != saw[150][1]);
        // equal-power placing, level in the middle
        let pan = run(&[(0, 32.0), (1, 0.0)], 30);
        let mid = run(&[(1, 0.0)], 30);
        assert!((pan[20][0] / mid[20][0] - 0.5412).abs() < 1e-3 && (pan[20][1] / mid[20][1] - 1.3066).abs() < 1e-3);
        // the third oscillator's top takes the level of the others down
        let am = run(&[(13, 128.0), (24, 1.0), (15, 2.0)], 30);
        let k = 1.0 - std::f32::consts::FRAC_1_SQRT_2;
        assert!((am[10][0] - saw[10][0] * k).abs() < 1e-5, "{} {}", am[10][0], saw[10][0] * k);
        // the last shape climbs fast and rests at the bottom for the final sixth; the drawn one is a saw
        assert!(wave(4, 0.0) == -1.0 && wave(4, 0.5) > 0.9999 && wave(4, 0.9) == -1.0);
        assert_eq!(run(&[(1, 6.0)], 50), saw[..50].to_vec());
        // noise is fresh on every frame and on each side
        let nz = run(&[(1, 5.0)], 50);
        assert!(nz.iter().all(|f| f[0] != f[1]) && nz[3][0] != nz[4][0]);
    }

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
    fn analog_synths_make_fresh_blocks_and_sound() {
        for (ident, size, version) in [("Wasp", 169, 1), ("Wasp XT", 213, 11)] {
            let g = Gen::decode(ident, &[]).unwrap();
            assert_eq!(g.raw.len(), size);
            assert_eq!(g.raw[0], version);
            let mut again = Gen::decode(ident, &g.raw).unwrap();
            assert_eq!(again.encode(None), g.raw);
            again.p[22] = 300.0;
            assert_eq!(&again.encode(None)[92..96], &300i32.to_le_bytes());
            let tone = again.tone();
            let patch = tone.mini.as_ref().unwrap();
            let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 1.0, 261.63 / 44100.0, 5);
            let peak = (0..6000).map(|_| v.frame(patch, 261.63 / 44100.0)[0].abs()).fold(0.0, f32::max);
            assert!(peak > 0.05 && peak < 1.2, "{ident} {peak}");
        }
    }

    #[test]
    fn the_plucked_string_rings_at_pitch_and_wears_down() {
        let g = Gen::decode("Plucked!", &[]).unwrap();
        assert_eq!(g.raw.len(), 20);
        let tone = g.tone();
        let patch = tone.mini.as_ref().unwrap();
        let dp = 220.0 / 44100.0;
        let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, dp, 9);
        let out: Vec<f32> = (0..44100).map(|_| v.frame(patch, dp)[0]).collect();
        let peak = |r: std::ops::Range<usize>| out[r].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak(0..4000) > 0.1 && peak(0..4000) < 1.2);
        assert!(peak(40000..44100) < peak(0..4000) * 0.9);
        // one period is about 200 samples: the sound matches itself there and not half a period on
        let corr = |lag: usize| (20000..30000).map(|i| out[i] * out[i + lag]).sum::<f32>();
        assert!(corr(200) > 0.8 * corr(0) && corr(100) < 0.5 * corr(0), "{} {} {}", corr(0), corr(200), corr(100));
    }

    #[test]
    fn the_guitar_sounds_in_every_amp_and_cabinet() {
        let fresh = Gen::decode("FL Slayer", &[]).unwrap();
        assert_eq!(fresh.raw.len(), 116);
        assert_eq!(Gen::decode("FL Slayer", &fresh.raw).unwrap().encode(None), fresh.raw);
        for amp in 0..4 {
            for cab in 0..4 {
                for mode in [0, 2] {
                    let mut g = fresh.clone();
                    g.p[9] = amp as f32;
                    g.p[16] = cab as f32;
                    g.p[20] = mode as f32;
                    g.p[1] = (amp % 3) as f32;
                    g.p[3] = 40000.0;
                    let tone = g.tone();
                    let patch = tone.mini.as_ref().unwrap();
                    let dp = 110.0 / 44100.0;
                    let mut v = crate::minis::Voice::start(patch, 44100.0, 45, 1.0, dp, 3);
                    let peak = (0..12000).map(|_| v.frame(patch, dp)[0].abs()).fold(0.0, f32::max);
                    assert!(peak > 0.02 && peak < 2.0, "amp {amp} cab {cab} mode {mode}: {peak}");
                    let tail = v.release(patch);
                    let n = (tail * 44100.0 * 3.0) as usize;
                    let end = (0..n).map(|_| v.frame(patch, dp)[0].abs()).skip(n - 400).fold(0.0, f32::max);
                    assert!(end < 0.01, "amp {amp} cab {cab}: {end}");
                }
            }
        }
    }

    #[test]
    fn the_analog_trio_reads_its_values_and_sounds_in_every_wave() {
        for wave in 0..5 {
            let mut v = [0.0f64; 70];
            for o in 0..3 {
                v[o * 7..o * 7 + 7].copy_from_slice(&[0.5, 0.5, 0.5, 0.8, 0.5, 0.5, wave as f64 / 10.0]);
            }
            v[26] = 0.6; // cutoff
            v[28] = 0.5;
            v[29] = 0.5;
            v[36] = 0.3;
            v[37] = 1.0;
            v[39] = 0.5;
            let mut b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
            b.extend_from_slice(&4u32.to_le_bytes());
            // the switches as a fresh one has them
            b[436..512].copy_from_slice(&Gen::new(Kind::Trio).raw[436..512]);
            let g = Gen::decode("SimSynth", &b).unwrap();
            assert_eq!(g.p.len(), 90);
            assert_eq!(g.encode(None), b);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 220.0 / 44100.0;
            let mut voice = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, dp, 3);
            let peak = (0..8000).map(|_| voice.frame(patch, dp)[0].abs()).fold(0.0, f32::max);
            assert!(peak > 0.03 && peak < 1.5, "wave {wave}: {peak}");
        }
        let tone = Gen::decode("SimSynth", &[]).unwrap().tone();
        let patch = tone.mini.as_ref().unwrap();
        let mut voice = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, 0.005, 3);
        assert!((0..4000).map(|_| voice.frame(patch, 0.005)[0].abs()).fold(0.0, f32::max) > 0.03);
    }

    #[test]
    fn the_small_analogue_synths_are_made_new_and_written_back() {
        for (kind, ident, len, moved_at) in [(Kind::Trio, "SimSynth", 564, 26 * 8..27 * 8), (Kind::Pocket, "MiniSynth", 0, 0..0), (Kind::SawSyn, "Sawer", 620, 12 + 26 * 4..12 + 27 * 4)] {
            let g = Gen::new(kind);
            assert!(len == 0 || g.raw.len() == len, "{ident}");
            assert!(!def(kind).arrives_only());
            let mut back = Gen::decode(ident, &g.raw).unwrap();
            assert_eq!(back.kind, kind);
            assert_eq!(back.p, crate::analog::plain(kind), "{ident}");
            assert_eq!(back.encode(None), g.raw, "{ident}");
            let (_, page) = crate::analog::page(kind);
            assert_eq!(page.len(), back.p.len());
            back.p[26] = 0.125;
            let out = back.encode(None);
            let moved: Vec<usize> = (0..out.len()).filter(|&i| out[i] != g.raw[i]).collect();
            assert!(!moved.is_empty() && (moved_at.is_empty() || moved.iter().all(|i| moved_at.contains(i))), "{ident} {moved:?}");
            assert_eq!(Gen::decode(ident, &out).unwrap().p[26], 0.125);
        }
        // the saw synth carries its effects, and two saws make its pulse
        let mut g = Gen::new(Kind::SawSyn);
        assert!(!g.tail().any());
        (g.p[57], g.p[65], g.p[19], g.p[22], g.p[18]) = (1.0, 1.0, 1.0, 1.0, 0.5);
        let t = g.tail();
        assert!(t.delay.is_some() && t.reverb.is_some() && t.chorus.is_none());
        let set = crate::analog::saw(&g.p);
        assert!(set.osc[0].shape == crate::analog::Shape::Pulse && set.width == 0.0 && set.osc[1].level == 0.0);
    }

    #[test]
    fn the_vintage_poly_is_made_new_and_written_back() {
        let g = Gen::new(Kind::Poly);
        assert_eq!(g.raw.len(), 178);
        assert!(!def(Kind::Poly).arrives_only());
        let mut back = Gen::decode("Kepler", &g.raw).unwrap();
        assert_eq!(back.kind, Kind::Poly);
        assert_eq!(back.p, crate::analog::poly_defaults());
        assert_eq!(back.encode(None), g.raw);
        back.p[19] = 0.25;
        let out = back.encode(None);
        let moved: Vec<usize> = (0..out.len()).filter(|&i| out[i] != g.raw[i]).collect();
        assert!(!moved.is_empty() && moved.iter().all(|&i| (33 + 19 * 4..33 + 20 * 4).contains(&i)), "{moved:?}");
        assert_eq!(Gen::decode("Kepler", &out).unwrap().p[19], 0.25);
        // its own arpeggio: off as made, then up and down over two octaves, locked to the tempo
        assert!(crate::analog::poly_arp(&back.p).is_none());
        (back.p[0], back.p[1], back.p[2], back.p[3], back.p[4]) = (1.0, 0.5, 0.5, 0.75, 1.0);
        assert_eq!(crate::analog::poly_arp(&back.p), Some((3, 2, 1.0)));
    }

    #[test]
    fn the_duo_synth_keeps_its_block_and_writes_only_what_moved() {
        let mut b = crate::duo::fresh();
        // bytes of ours after the numbers must come back untouched
        b[660..664].copy_from_slice(b"Lead");
        let mut g = Gen::decode("PoiZone", &b).unwrap();
        assert_eq!(g.kind, Kind::Duo);
        assert_eq!(g.p, crate::duo::defaults());
        assert_eq!(g.encode(None), b);
        g.p[18] = 0.25;
        let out = g.encode(None);
        let moved: Vec<usize> = (0..b.len()).filter(|&i| out[i] != b[i]).collect();
        assert!(!moved.is_empty() && moved.iter().all(|&i| (12 + 18 * 4..12 + 19 * 4).contains(&i)), "{moved:?}");
        assert_eq!(Gen::decode("PoiZone", &out).unwrap().p[18], 0.25);
        // a new one carries a whole block and sounds
        let g = Gen::new(Kind::Duo);
        assert_eq!(g.raw.len(), 700);
        assert!(!def(Kind::Duo).arrives_only());
        let tone = g.tone();
        let patch = tone.mini.as_ref().unwrap();
        let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, 0.005, 3);
        assert!((0..4000).map(|_| v.frame(patch, 0.005)[0].abs()).fold(0.0, f32::max) > 0.03);
        // a block of another make is kept and plays the default sound
        let odd = vec![7u8; 40];
        let g = Gen::decode("PoiZone", &odd).unwrap();
        assert!(g.p.is_empty() && g.encode(None) == odd && g.mono().is_none() && !g.tail().any());
    }

    #[test]
    fn the_string_model_keeps_its_block_and_writes_only_what_moved() {
        let mut b = crate::strmodel::fresh();
        b[468..476].copy_from_slice(b"My pluck");
        let mut g = Gen::decode("Sakura", &b).unwrap();
        assert_eq!(g.kind, Kind::StrModel);
        assert_eq!(g.p, crate::strmodel::defaults());
        assert_eq!(g.encode(None), b);
        g.p[29] = 0.75;
        let out = g.encode(None);
        let moved: Vec<usize> = (0..b.len()).filter(|&i| out[i] != b[i]).collect();
        assert!(!moved.is_empty() && moved.iter().all(|&i| (12 + 29 * 4..12 + 30 * 4).contains(&i)), "{moved:?}");
        let g = Gen::new(Kind::StrModel);
        assert_eq!(g.raw.len(), 596);
        assert!(!def(Kind::StrModel).arrives_only());
        let tone = g.tone();
        let patch = tone.mini.as_ref().unwrap();
        let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, 0.005, 3);
        assert!((0..8000).map(|_| v.frame(patch, 0.005)[0].abs()).fold(0.0, f32::max) > 0.01);
        // a block of another make is kept and plays the default sound
        let odd = vec![7u8; 40];
        let g = Gen::decode("Sakura", &odd).unwrap();
        assert!(g.p.is_empty() && g.encode(None) == odd && !g.tail().any());
    }

    /// With CEREAL_FLP_CORPUS set: every stored sound of the string model there reads, plays and stays in range.
    #[test]
    fn every_stored_string_model_sound_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let dir = std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Sakura");
        let (mut seen, mut quiet, mut top, mut loud, mut worst) = (0, Vec::new(), 0.0f32, 0, String::new());
        let mut stack = vec![dir];
        let mut files = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                    files.push(path);
                }
            }
        }
        let mut peaks = Vec::new();
        for path in files {
            let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
            assert_eq!(g.p.len(), crate::strmodel::COUNT, "{path:?}");
            assert_eq!(g.encode(None), g.raw);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 261.63 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
            let mut buf: Vec<[f32; 2]> = (0..66150).map(|_| v.frame(patch, dp)).collect();
            let set = g.tail();
            if set.any() {
                crate::tailfx::State::new(44100.0).process(&set, &mut buf, 5512.0, None);
            }
            let peak = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            assert!(peak.is_finite(), "{path:?}: {peak}");
            if peak > top {
                top = peak;
                worst = path.file_name().unwrap().to_string_lossy().into_owned();
            }
            peaks.push(peak);
            if peak > 1.5 {
                loud += 1;
            }
            // a few are stored with the volume all the way down
            if peak < 0.003 && g.p[0] > 0.05 {
                quiet.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            seen += 1;
        }
        if seen > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            eprintln!("string model sounds: {seen}, loudest {top}, middle {}, tenth {}, over 1.5: {loud}, quiet {}: {:?}", peaks[seen / 2], peaks[seen / 10], quiet.len(), &quiet[..quiet.len().min(12)]);
            assert!(top < 4.0, "{worst}: {top}");
            assert!(quiet.len() * 10 < seen, "{} of {seen} stay silent: {quiet:?}", quiet.len());
        }
    }

    /// With CEREAL_FLP_CORPUS set: every stored sound of the triple synth there reads, plays and stays in range.
    #[test]
    fn every_stored_triple_synth_sound_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let root = std::path::Path::new(&root);
        if let Some(install) = root.parent() {
            crate::keys::add_root(install);
        }
        let mut stack = vec![root.join("Patches/Plugin presets/Generators/GMS")];
        let mut files = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("gmsynth")) {
                    files.push(path);
                }
            }
        }
        let (mut seen, mut quiet, mut top, mut worst, mut peaks) = (0, Vec::new(), 0.0f32, String::new(), Vec::new());
        for path in files {
            let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
            assert_eq!(g.kind, Kind::Triple);
            assert_eq!(g.p.len(), crate::triple::COUNT, "{path:?}");
            assert_eq!(g.encode(None), g.raw);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 261.63 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
            let mut buf: Vec<[f32; 2]> = (0..66150).map(|_| v.frame(patch, dp)).collect();
            let set = g.tail();
            if set.any() {
                crate::tailfx::State::new(44100.0).process(&set, &mut buf, 5512.0, None);
            }
            let peak = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            assert!(peak.is_finite(), "{path:?}: {peak}");
            if peak > top {
                top = peak;
                worst = path.file_name().unwrap().to_string_lossy().into_owned();
            }
            peaks.push(peak);
            if std::env::var("CEREAL_TRIPLE_DUMP").is_ok() && !(0.05..1.5).contains(&peak) {
                let on: Vec<usize> = (0..10).filter(|k| g.p[147 + k] >= 0.5).collect();
                eprintln!("{peak:.3} {:?} fx{on:?} x{:?} y{:?} cut {} res {} eg {:?} lfo {:?} amp {:?} eq {:?} mode {} amt {}", path.file_name().unwrap(), on.iter().map(|k| g.p[117 + k]).collect::<Vec<_>>(), on.iter().map(|k| g.p[127 + k]).collect::<Vec<_>>(), g.p[12], g.p[13], &g.p[16..26], &g.p[26..40], &g.p[4..9], &g.p[177..182], g.p[51], g.p[50]);
            }
            if peak < 0.003 && g.p[0] > 0.05 && g.p[8] > 0.05 {
                quiet.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            seen += 1;
        }
        if seen > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            eprintln!("triple synth sounds: {seen}, loudest {top} ({worst}), middle {}, tenth {}, quiet {}: {:?}", peaks[seen / 2], peaks[seen / 10], quiet.len(), &quiet[..quiet.len().min(12)]);
            assert!(crate::triple::wave_names().len() > 4 || !root.parent().is_some_and(|r| r.join("Plugins").is_dir()));
            assert!(top < 4.0, "{worst}: {top}");
            assert!(quiet.len() * 10 < seen, "{} of {seen} stay silent: {quiet:?}", quiet.len());
        }
    }

    /// With CEREAL_FLP_CORPUS set: every stored membrane drum there reads, plays and stays in range.
    #[test]
    fn every_stored_membrane_drum_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Drumpad")];
        let (mut seen, mut peaks, mut odd) = (0, Vec::new(), Vec::new());
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                    continue;
                }
                let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
                assert_eq!(g.kind, Kind::Skin);
                assert_eq!(g.p.len(), crate::membrane::COUNT, "{path:?}");
                assert_eq!(g.encode(None), g.raw);
                let tone = g.tone();
                let patch = tone.mini.as_ref().unwrap();
                let dp = 261.63 / 44100.0;
                let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
                let buf: Vec<[f32; 2]> = (0..44100).map(|_| v.frame(patch, dp)).collect();
                let peak = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
                assert!(peak.is_finite(), "{path:?}: {peak}");
                if !(0.02..2.0).contains(&peak) {
                    odd.push(format!("{} {peak:.3}", path.file_stem().unwrap().to_string_lossy()));
                }
                peaks.push(peak);
                seen += 1;
            }
        }
        if seen > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            eprintln!("membrane drums: {seen}, loudest {}, ninth tenth {}, middle {}, tenth {}, softest {}, out of range {}: {:?}", peaks[seen - 1], peaks[seen * 9 / 10], peaks[seen / 2], peaks[seen / 10], peaks[0], odd.len(), &odd[..odd.len().min(12)]);
            assert!(peaks[seen - 1] < 4.0);
            assert!(odd.len() * 10 < seen, "{} of {seen} are out of range", odd.len());
        }
    }

    /// With CEREAL_FLP_CORPUS set: every stored drum kit there reads, writes back the same, and its pads and
    /// patterns play.
    #[test]
    fn every_stored_drum_kit_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Drumaxx")];
        let (mut kits, mut pads, mut runs, mut peaks) = (0, 0, 0, Vec::new());
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                    continue;
                }
                let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
                assert_eq!(g.kind, Kind::Kit);
                assert_eq!(g.p.len(), crate::drumkit::COUNT, "{path:?}");
                assert_eq!(g.encode(None), g.raw);
                let tone = g.tone();
                let patch = tone.mini.as_ref().unwrap();
                let dp = 261.63 / 44100.0;
                let mut keys: Vec<u8> = (0..crate::drumkit::PADS).map(|k| g.p[k * crate::drumkit::REC + 4] as u8).filter(|&k| k > 0).collect();
                keys.dedup();
                for key in keys {
                    let mut v = crate::minis::Voice::start(patch, 44100.0, key, 0.8, dp, 3);
                    let peak = (0..22050).map(|_| v.frame(patch, dp)).fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                    assert!(peak.is_finite() && peak < 6.0, "{path:?} key {key}: {peak}");
                    peaks.push(peak);
                    pads += 1;
                }
                for pat in 0..crate::drumkit::PATTERNS {
                    let (Some(key), Some(_)) = (crate::drumkit::pat_key(&g.raw, pat), crate::drumkit::pat_len(&g.raw, pat)) else { continue };
                    let mut v = crate::minis::Voice::start(patch, 44100.0, key, 0.8, dp, 3);
                    let peak = (0..44100).map(|_| v.frame(patch, dp)).fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                    assert!(peak.is_finite() && peak < 12.0, "{path:?} pattern {pat}: {peak}");
                    runs += 1;
                }
                kits += 1;
            }
        }
        if kits > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            let n = peaks.len();
            eprintln!("drum kits: {kits}, pads {pads}, patterns {runs}; pad peaks: loudest {}, ninth tenth {}, middle {}, tenth {}, softest {}", peaks[n - 1], peaks[n * 9 / 10], peaks[n / 2], peaks[n / 10], peaks[0]);
            assert!(peaks[n / 2] > 0.05 && peaks[n / 2] < 1.5);
        }
    }

    /// With CEREAL_FLP_CORPUS set: every stored kick designer sound there reads, finds its layered sample and plays.
    #[test]
    fn every_stored_kick_designer_sound_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let root = std::path::Path::new(&root);
        if let Some(up) = root.parent() {
            crate::keys::add_root(up);
        }
        let mut stack = vec![root.join("Patches/Plugin presets/Generators/BassDrum")];
        let (mut seen, mut layered, mut peaks) = (0, 0, Vec::new());
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                    continue;
                }
                let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
                assert_eq!(g.kind, Kind::KickD);
                assert_eq!(g.p.len(), crate::kickd::COUNT, "{path:?}");
                assert_eq!(g.encode(None), g.raw);
                let tone = g.tone();
                let patch = tone.mini.as_ref().unwrap();
                if patch.p[crate::kickd::COUNT] >= 1.0 {
                    layered += 1;
                }
                let dp = 261.63 / 44100.0;
                let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
                let buf: Vec<[f32; 2]> = (0..44100).map(|_| v.frame(patch, dp)).collect();
                let peak = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                assert!(peak.is_finite() && peak > 0.05 && peak < 2.5, "{path:?}: {peak}");
                peaks.push(peak);
                seen += 1;
            }
        }
        if seen > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            eprintln!("kick designer sounds: {seen}, with a layered sample found {layered}, loudest {}, middle {}, softest {}", peaks[seen - 1], peaks[seen / 2], peaks[0]);
        }
    }

    /// With CEREAL_FLP_CORPUS set: every stored sound of the hybrid synth there reads, plays and stays in range.
    #[test]
    fn every_stored_morph_additive_sound_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let root = std::path::Path::new(&root);
        let mut stack = vec![root.join("Patches/Plugin presets/Generators/Morphine")];
        let mut files = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("mrp")) {
                    files.push(path);
                }
            }
        }
        let (mut seen, mut quiet, mut top, mut worst, mut peaks) = (0, Vec::new(), 0.0f32, String::new(), Vec::new());
        for path in files {
            let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
            assert_eq!(g.kind, Kind::Morph);
            assert_eq!(g.p.len(), crate::morph::COUNT, "{path:?}");
            assert_eq!(g.encode(None), g.raw);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            assert!(patch.p[crate::morph::COUNT] >= 1.0, "{path:?}: the snapshots do not read");
            let dp = 261.63 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
            let mut buf: Vec<[f32; 2]> = (0..44100).map(|_| v.frame(patch, dp)).collect();
            let dry = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            let set = g.tail();
            if set.any() {
                crate::tailfx::State::new(44100.0).process(&set, &mut buf, 5512.0, None);
            }
            let peak = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            assert!(peak.is_finite(), "{path:?}: {peak}");
            if peak > top {
                top = peak;
                worst = path.file_name().unwrap().to_string_lossy().into_owned();
            }
            peaks.push(peak);
            if std::env::var("CEREAL_MORPH_DUMP").is_ok() && !(0.05..1.5).contains(&peak) {
                eprintln!("{peak:.3} dry {dry:.3} {:?} lvl {} drive {} env {:?}", path.file_name().unwrap(), g.p[0], g.p[1], &g.p[9..13]);
            }
            if peak < 0.003 {
                quiet.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            seen += 1;
        }
        if seen > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            eprintln!("morph additive sounds: {seen}, loudest {top} ({worst}), middle {}, tenth {}, quiet {}: {:?}", peaks[seen / 2], peaks[seen / 10], quiet.len(), &quiet[..quiet.len().min(8)]);
            assert!(top < 4.0, "{worst} peaks at {top}");
            assert!(quiet.len() * 10 < seen, "{} of {seen} are silent", quiet.len());
        }
    }

    #[test]
    fn every_stored_hybrid_synth_sound_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let root = std::path::Path::new(&root);
        let mut stack = vec![root.join("Patches/Plugin presets/Generators/Toxic Biohazard")];
        let mut files = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("tbio")) {
                    files.push(path);
                }
            }
        }
        let (mut seen, mut quiet, mut top, mut worst, mut peaks) = (0, Vec::new(), 0.0f32, String::new(), Vec::new());
        for path in files {
            let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
            assert_eq!(g.kind, Kind::Hybrid);
            assert_eq!(g.p.len(), crate::hybrid::COUNT, "{path:?}");
            assert_eq!(g.encode(None), g.raw);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 261.63 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
            let mut buf: Vec<[f32; 2]> = (0..66150).map(|_| v.frame(patch, dp)).collect();
            let dry = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            let set = g.tail();
            if set.any() {
                crate::tailfx::State::new(44100.0).process(&set, &mut buf, 5512.0, None);
            }
            let peak = buf.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            assert!(peak.is_finite(), "{path:?}: {peak}");
            if peak > top {
                top = peak;
                worst = path.file_name().unwrap().to_string_lossy().into_owned();
            }
            peaks.push(peak);
            if std::env::var("CEREAL_HYBRID_DUMP").is_ok() && !(0.03..1.5).contains(&peak) {
                eprint!("dry {dry:.2} ");
                let on: Vec<usize> = (0..6).filter(|k| g.p[25 + k] >= 0.5).collect();
                eprintln!("{peak:.3} {:?} on{on:?} lvl {:?} wave {:?} filt {} cut {} res {} eg {} feg {:?} amp {:?} vol {} drive {} eq {:?}", path.file_name().unwrap(), &g.p[103..109], &g.p[306..312], g.p[300], g.p[15], g.p[16], g.p[23], &g.p[19..23], &g.p[3..7], g.p[0], g.p[8], &g.p[227..236]);
            }
            if peak < 0.003 && g.p[0] > 0.05 {
                quiet.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            seen += 1;
        }
        if seen > 0 {
            peaks.sort_by(|a, b| a.total_cmp(b));
            eprintln!("hybrid synth sounds: {seen}, loudest {top} ({worst}), middle {}, tenth {}, quiet {}: {:?}", peaks[seen / 2], peaks[seen / 10], quiet.len(), &quiet[..quiet.len().min(12)]);
            assert!(top < 4.0, "{worst}: {top}");
            assert!(quiet.len() * 10 < seen, "{} of {seen} stay silent: {quiet:?}", quiet.len());
        }
    }

    /// With CEREAL_FLP_CORPUS set: every stored sound of the duo synth there reads, plays and stays in range.
    #[test]
    fn every_stored_duo_sound_plays() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let dir = std::path::Path::new(&root).join("Patches/Plugin presets/Generators/PoiZone");
        let (mut seen, mut quiet, mut top) = (0, Vec::new(), 0.0f32);
        let mut stack = vec![dir];
        let mut files = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                    files.push(path);
                }
            }
        }
        for path in files {
            let Some(crate::preset::Unit::Gen(g)) = crate::preset::read(&path).ok().and_then(|p| p.unit()) else { panic!("{path:?} does not read") };
            assert_eq!(g.p.len(), crate::duo::COUNT, "{path:?}");
            assert_eq!(g.encode(None), g.raw);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 261.63 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 60, 0.8, dp, 3);
            let peak = (0..44100).map(|_| v.frame(patch, dp)).fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            assert!(peak.is_finite() && peak < 3.0, "{path:?}: {peak}");
            top = top.max(peak);
            if peak < 0.003 {
                quiet.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            seen += 1;
        }
        if seen > 0 {
            eprintln!("duo sounds: {seen}, loudest {top}, quiet {}: {:?}", quiet.len(), &quiet[..quiet.len().min(12)]);
            assert!(quiet.len() * 20 < seen, "{} of {seen} stay silent: {quiet:?}", quiet.len());
        }
    }

    #[test]
    fn the_saw_synth_reads_its_values_and_follows_its_cutoff() {
        let bright = |cut: f32| {
            let mut b = b"SSLF\x64\x02\0\x001rwS".to_vec();
            for i in 0..152 {
                let x: f32 = match i {
                    0 | 6 | 7 => 0.5,
                    4 => 1.0,
                    17 => 0.8,
                    27 => cut,
                    _ => 0.0,
                };
                b.extend_from_slice(&x.to_le_bytes());
            }
            let g = Gen::decode("Sawer", &b).unwrap();
            assert_eq!(g.p.len(), 96);
            assert_eq!(g.encode(None), b);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 220.0 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, dp, 3);
            let out: Vec<f32> = (0..8000).map(|_| v.frame(patch, dp)[0]).collect();
            let level: f32 = out.iter().map(|x| x.abs()).sum();
            let edge: f32 = out.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
            assert!(level > 50.0, "{level}");
            edge / level
        };
        let (open, shut) = (bright(1.0), bright(0.3));
        assert!(open > shut * 1.3, "{open} {shut}");
        let tone = Gen::decode("Sawer", &[]).unwrap().tone();
        let patch = tone.mini.as_ref().unwrap();
        let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, 0.005, 3);
        assert!((0..4000).map(|_| v.frame(patch, 0.005)[0].abs()).fold(0.0, f32::max) > 0.03);
    }

    #[test]
    fn the_pocket_synth_reads_its_values_and_sounds_in_every_wave() {
        let mut block = b"SSLF....HEAD\x04\0\0\0\x01\0\x01\0DATA".to_vec();
        block.extend_from_slice(&(8u32 + 42 * 4).to_le_bytes());
        block.extend_from_slice(b"0rev\0\0\0\0");
        for wave in 0..10 {
            let mut b = block.clone();
            for i in 0..42 {
                let x: f32 = match i {
                    4 => wave as f32 / 9.0,
                    5 => 0.6,
                    7 => 0.8,
                    21 | 22 => 1.0,
                    23 => 0.1,
                    _ => 0.0,
                };
                b.extend_from_slice(&x.to_le_bytes());
            }
            let g = Gen::decode("MiniSynth", &b).unwrap();
            assert_eq!(g.p.len(), 42);
            assert_eq!(g.encode(None), b);
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 220.0 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, dp, 3);
            let peak = (0..8000).map(|_| v.frame(patch, dp)[0].abs()).fold(0.0, f32::max);
            assert!(peak > 0.05 && peak < 1.5, "wave {wave}: {peak}");
        }
        // an empty block still plays
        let tone = Gen::decode("MiniSynth", &[]).unwrap().tone();
        let patch = tone.mini.as_ref().unwrap();
        let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, 0.005, 3);
        assert!((0..4000).map(|_| v.frame(patch, 0.005)[0].abs()).fold(0.0, f32::max) > 0.05);
    }

    #[test]
    fn the_vintage_poly_reads_its_values_and_sounds() {
        let mut b = vec![0u8; 12];
        b.extend_from_slice(&9u32.to_le_bytes());
        b.extend_from_slice(b"Kepler-1\0");
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&34u32.to_le_bytes());
        for i in 0..34 {
            let x: f32 = match i {
                13..=15 => 1.0,
                16 => 0.8,
                11 | 19 | 28 | 33 => 0.5,
                30 => 0.33,
                _ => 0.0,
            };
            b.extend_from_slice(&x.to_le_bytes());
        }
        b.extend_from_slice(&[0; 9]);
        let g = Gen::decode("Kepler", &b).unwrap();
        assert_eq!(g.p.len(), 34);
        assert_eq!(g.encode(None), b);
        for g in [g, Gen::decode("Kepler", &[]).unwrap(), Gen::decode("Kepler Exo", &[]).unwrap(), Gen::decode("Transistor Bass", &[7; 40]).unwrap()] {
            let tone = g.tone();
            let patch = tone.mini.as_ref().unwrap();
            let dp = 220.0 / 44100.0;
            let mut v = crate::minis::Voice::start(patch, 44100.0, 57, 1.0, dp, 3);
            let out: Vec<[f32; 2]> = (0..12000).map(|_| v.frame(patch, dp)).collect();
            let peak = out.iter().map(|f| f[0].abs()).fold(0.0, f32::max);
            assert!(peak > 0.05 && peak < 1.5, "{peak}");
        }
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
        // an octave above the note it is given, as the original sounds
        assert!((880..=884).contains(&crossings), "{crossings}");
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
        assert_eq!(p.p[ix::CUTOFF], 4417.0);
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
        // a block that cannot be read is kept as it came
        let b = vec![3u8; 700];
        let g = Gen::decode("Morphine", &b).unwrap();
        assert!(g.tone().mini.is_some());
        assert_eq!(g.encode(None), b);
        let k = vec![0u8; 1384];
        let g = Gen::decode("BassDrum", &k).unwrap();
        assert!(g.p.is_empty());
        assert!(g.tone().mini.is_some());
        assert_eq!(g.encode(None), k);
        assert!(Gen::decode("drumpad", &[0u8; 300]).unwrap().tone().mini.is_some());
        assert_eq!(Gen::decode("Kepler", &[1u8; 178]).unwrap().encode(None), vec![1u8; 178]);
    }
}
