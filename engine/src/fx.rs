//! Insert effects. Each unit is our own processor that reads and writes the stored settings of the matching
//! effect in a project file, so a project keeps its mix when opened here and keeps its settings when saved.
//!
//! An effect is described by a table (`DEFS`): the identifier found in project files, our own title, and where
//! each setting lives in the stored block. The settings stay in file units; the processors convert them. The
//! conversions are approximations until measured against the reference (docs/PARITY.md, "Effects").

use std::f32::consts::{PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Balance,
    SoftClip,
    FastDist,
    Enhancer,
    Loud,
    Warp,
    Bank,
    FreeFilter,
    BassBoost,
    Phaser,
    FreqShift,
    Finisher,
    LowLift,
    MorphEq,
    FastLp,
    Overdrive,
    Filter,
    AutoPan,
    Delay,
    Comp,
    Shaper,
    Eq,
    Reverb,
    Limiter,
    Flanger,
    Chorus,
    Echo,
    Spread,
    Transient,
    Swarm,
    MultiComp,
    Maxi,
    BucketChorus,
    GrainShift,
    BandSplit,
    EchoBank,
    OldEq,
    OldReverb,
    OldFlanger,
    Stack,
    Plush,
    Pad,
    Tuner,
    Peak,
    Xy,
    Xyz,
    Formula,
    Curve,
    /// Units that leave the sound alone: displays, notes, recorders and senders of control values.
    Thru,
    /// The bank player: makes no change to the sound passing through; MIDI senders on its port sound here.
    Gm,
    /// A board of controls that only sends their positions out.
    Surface,
    /// A plug-in from another maker, run through the host.
    Hosted,
    /// Pages of notes or a web address; makes no sound.
    Notes,
    Conv,
    Rack,
    Patch,
    Voc,
    OldVoc,
    OldDelay,
    SendTap,
    Squeeze,
    DcBlock,
    Eq7,
    Mute,
    Flip,
    Emph,
    StonePhaser,
    Amp,
    Relooper,
    Deck,
    Multi,
    Split,
}

pub struct Param {
    pub name: &'static str,
    /// Byte offset of the 32-bit value in the stored block.
    pub off: usize,
    pub min: i32,
    pub max: i32,
    pub def: i32,
}

pub struct Def {
    pub kind: Kind,
    /// Identifier used in project files.
    pub ident: &'static str,
    /// Our own name for the unit.
    pub title: &'static str,
    /// Size of a freshly written block.
    pub size: usize,
    /// Fixed values of a freshly written block (version markers and the like).
    pub fixed: &'static [(usize, i32)],
    pub params: &'static [Param],
}

const fn p(name: &'static str, off: usize, min: i32, max: i32, def: i32) -> Param {
    Param { name, off, min, max, def }
}

pub static DEFS: &[Def] = &[
    Def { kind: Kind::Balance, ident: "Fruity Balance", title: "Balance", size: 8, fixed: &[], params: &[p("Pan", 0, -128, 127, 0), p("Volume", 4, 0, 320, 256)] },
    Def { kind: Kind::SoftClip, ident: "Fruity Soft Clipper", title: "Soft Clip", size: 8, fixed: &[], params: &[p("Threshold", 0, 1, 127, 100), p("Post gain", 4, 0, 160, 128)] },
    Def {
        kind: Kind::FastDist,
        ident: "Fruity Fast Dist",
        title: "Quick Distortion",
        size: 20,
        fixed: &[],
        params: &[p("Pre", 0, 64, 192, 128), p("Threshold", 4, 1, 10, 10), p("Type", 8, 0, 1, 0), p("Mix", 12, 0, 128, 128), p("Post", 16, 0, 128, 128)],
    },
    Def {
        kind: Kind::Enhancer,
        ident: "Fruity Stereo Enhancer",
        title: "Stereo Tool",
        size: 24,
        fixed: &[],
        params: &[p("Pan", 0, -128, 127, 0), p("Volume", 4, 0, 320, 256), p("Separation", 8, -96, 96, 0), p("Phase offset", 12, -500, 500, 0), p("Position", 16, 0, 1, 1), p("Invert", 20, 0, 2, 0)],
    },
    Def { kind: Kind::Loud, ident: "Soundgoodizer", title: "Loudener", size: 12, fixed: &[(0, 3)], params: &[p("Mode", 4, 0, 3, 0), p("Amount", 8, 0, 1000, 600)] },
    Def { kind: Kind::FastLp, ident: "Fruity Fast LP", title: "Quick Low-pass", size: 12, fixed: &[(0, 1)], params: &[p("Cutoff", 4, 0, 1024, 1024), p("Resonance", 8, 0, 1024, 0)] },
    Def {
        kind: Kind::Overdrive,
        ident: "Fruity Blood Overdrive",
        title: "Overdrive",
        size: 36,
        fixed: &[(0, 3)],
        params: &[p("Pre band", 4, 0, 10000, 0), p("Colour", 8, 0, 10000, 5000), p("Pre amp", 12, 0, 10000, 0), p("x100", 16, 0, 1, 0), p("Post filter", 20, 0, 10000, 0), p("Post gain", 24, 0, 10000, 10000)],
    },
    Def {
        kind: Kind::Filter,
        ident: "Fruity Filter",
        title: "Filter",
        size: 29,
        fixed: &[(0, 2)],
        params: &[p("Cutoff", 4, 0, 1024, 1024), p("Resonance", 8, 0, 1024, 0), p("Low pass", 12, 0, 1024, 1024), p("Band pass", 16, 0, 1024, 0), p("High pass", 20, 0, 1024, 0), p("x2", 24, 0, 1, 0)],
    },
    Def {
        kind: Kind::AutoPan,
        ident: "Fruity PanOMatic",
        title: "Auto Pan",
        size: 24,
        fixed: &[],
        params: &[p("Pan", 0, 0, 256, 128), p("Volume", 4, 0, 256, 256), p("Shape", 8, 0, 2, 0), p("Target", 12, 0, 2, 0), p("Amount", 16, 0, 128, 64), p("Speed", 20, 27500, 55000, 27500)],
    },
    Def {
        kind: Kind::Delay,
        ident: "Fruity Delay 2",
        title: "Delay",
        size: 32,
        fixed: &[],
        params: &[
            p("Input pan", 0, -128, 127, 0),
            p("Input volume", 4, 0, 160, 128),
            p("Dry", 8, 0, 160, 128),
            p("Feedback", 12, 0, 128, 64),
            p("Time", 16, 1, 768, 192),
            p("Stereo offset", 20, -512, 512, 0),
            p("Mode", 24, 0, 2, 0),
            p("Feedback cut", 28, 0, 128, 128),
        ],
    },
    Def {
        kind: Kind::Comp,
        ident: "Fruity Compressor",
        title: "Compressor",
        size: 36,
        fixed: &[(0, 3), (28, 1)],
        params: &[p("Threshold", 4, -600, 0, 0), p("Ratio", 8, 4, 300, 10), p("Gain", 12, -300, 300, 0), p("Attack", 16, 0, 4000, 10), p("Release", 20, 1, 4000, 200), p("Type", 24, 0, 7, 0)],
    },
    Def {
        kind: Kind::Shaper,
        ident: "Fruity Stereo Shaper",
        title: "Stereo Matrix",
        size: 36,
        fixed: &[(32, 1)],
        params: &[p("Right to left", 4, -12800, 12800, 0), p("Left", 8, -12800, 12800, 12800), p("Right", 12, -12800, 12800, 12800), p("Left to right", 16, -12800, 12800, 0), p("Delay", 20, -4096, 4096, 0), p("Phase", 24, -4096, 4096, 0)],
    },
    Def {
        kind: Kind::Eq,
        ident: "Fruity Parametric EQ 2",
        title: "Seven Band EQ",
        size: 305,
        fixed: &[(0, 2)],
        params: &[
            p("Level 1", 4, -1800, 1800, 0),
            p("Level 2", 8, -1800, 1800, 0),
            p("Level 3", 12, -1800, 1800, 0),
            p("Level 4", 16, -1800, 1800, 0),
            p("Level 5", 20, -1800, 1800, 0),
            p("Level 6", 24, -1800, 1800, 0),
            p("Level 7", 28, -1800, 1800, 0),
            p("Freq 1", 32, 0, 65536, 10923),
            p("Freq 2", 36, 0, 65536, 18204),
            p("Freq 3", 40, 0, 65536, 25486),
            p("Freq 4", 44, 0, 65536, 32768),
            p("Freq 5", 48, 0, 65536, 40050),
            p("Freq 6", 52, 0, 65536, 47332),
            p("Freq 7", 56, 0, 65536, 54613),
            p("Width 1", 60, 0, 65536, 40124),
            p("Width 2", 64, 0, 65536, 25412),
            p("Width 3", 68, 0, 65536, 25412),
            p("Width 4", 72, 0, 65536, 25412),
            p("Width 5", 76, 0, 65536, 25412),
            p("Width 6", 80, 0, 65536, 25412),
            p("Width 7", 84, 0, 65536, 40124),
            p("Type 1", 88, 0, 7, 5),
            p("Type 2", 92, 0, 7, 6),
            p("Type 3", 96, 0, 7, 6),
            p("Type 4", 100, 0, 7, 6),
            p("Type 5", 104, 0, 7, 6),
            p("Type 6", 108, 0, 7, 6),
            p("Type 7", 112, 0, 7, 7),
            p("Slope 1", 116, -3, 3, 0),
            p("Slope 2", 120, -3, 3, 0),
            p("Slope 3", 124, -3, 3, 0),
            p("Slope 4", 128, -3, 3, 0),
            p("Slope 5", 132, -3, 3, 0),
            p("Slope 6", 136, -3, 3, 0),
            p("Slope 7", 140, -3, 3, 0),
            p("Main level", 144, -1800, 1800, 0),
            p("High quality", 292, 0, 1, 1),
        ],
    },
    Def {
        kind: Kind::Reverb,
        ident: "Fruity Reeverb 2",
        title: "Room Reverb",
        size: 66,
        fixed: &[(0, 10001)],
        params: &[
            p("Low cut", 4, 19, 2500, 75),
            p("High cut", 8, 5, 221, 40),
            p("Predelay", 12, 0, 1000, 0),
            p("Room size", 16, 1, 100, 50),
            p("Diffusion", 20, 0, 100, 100),
            p("Decay", 24, 1, 200, 15),
            p("High damping", 28, 5, 221, 40),
            p("Bass", 32, 20, 300, 100),
            p("Crossover", 36, 25, 1590, 500),
            p("Separation", 40, -64, 64, 0),
            p("Dry", 44, 0, 160, 128),
            p("Early", 48, 0, 160, 64),
            p("Wet", 52, 0, 160, 38),
            p("Mod speed", 56, 0, 2048, 684),
            p("Mod amount", 60, 0, 2048, 0),
        ],
    },
    Def {
        kind: Kind::Limiter,
        ident: "Fruity Limiter",
        title: "Limiter",
        size: 169,
        // the second run of numbers is carried by every stored block, at the values a new unit starts with
        fixed: &[
            (0, 7),
            (76, 1000),
            (80, 1000),
            (84, 1000),
            (88, 1115),
            (92, 3),
            (96, 5000),
            (100, 3),
            (104, 2882),
            (108, 1000),
            (124, 6666),
            (128, 1),
            (132, 650),
            (136, 1000),
            (144, 4187),
            (148, 50),
            (156, 0x0101_0101),
        ],
        params: &[
            p("Gain", 4, 0, 2000, 1000),
            p("Saturation", 8, 0, 1000, 1000),
            p("Ceiling", 12, 0, 2000, 1000),
            p("Attack", 16, 0, 10000, 1115),
            p("Release", 24, 0, 10000, 5000),
            p("Sustain", 32, 0, 10000, 2882),
            p("Comp threshold", 36, 0, 1000, 1000),
            p("Comp ratio", 40, 0, 1000, 0),
            p("Comp knee", 44, 0, 1000, 0),
            p("Comp attack", 48, 0, 10000, 0),
            p("Comp release", 52, 0, 10000, 6666),
            p("Comp sustain", 60, 0, 10000, 650),
            p("Gate gain", 64, 0, 1000, 1000),
            p("Gate threshold", 68, 0, 1000, 0),
            p("Gate release", 72, 0, 10000, 4187),
            p("Attack curve", 20, 1, 8, 3),
            p("Release curve", 28, 1, 8, 3),
            p("Comp curve", 56, 1, 8, 1),
            p("Sidechain", 164, 0, 64, 0),
        ],
    },
    Def {
        kind: Kind::Stack,
        ident: "Fruity Flangus",
        title: "Flanger",
        size: 32,
        fixed: &[],
        params: &[p("Order", 0, 1, 16, 4), p("Depth", 4, 0, 1024, 512), p("Speed", 8, 0, 1024, 768), p("Delay", 12, 0, 1024, 8), p("Spread", 16, 0, 128, 128), p("Cross", 20, -128, 128, -32), p("Dry", 24, -128, 128, 96), p("Wet", 28, -128, 128, 96)],
    },
    // the processor the classic flanger runs on; no file names it
    Def {
        kind: Kind::Flanger,
        ident: "(feedback flanger)",
        title: "Feedback flanger",
        size: 32,
        fixed: &[],
        params: &[p("Delay", 0, 1, 1024, 4), p("Depth", 4, 0, 1024, 512), p("Rate", 8, 0, 1024, 768), p("Phase", 12, 0, 360, 8), p("Damp", 16, 0, 128, 128), p("Feedback", 20, -128, 128, -32), p("Dry", 24, -128, 128, 128), p("Wet", 28, -128, 128, 96)],
    },
    Def {
        kind: Kind::Chorus,
        ident: "Fruity Chorus",
        title: "Chorus",
        size: 52,
        fixed: &[(0, 1)],
        params: &[
            p("Delay", 4, 0, 1024, 512),
            p("Depth", 8, 0, 5000, 2250),
            p("Stereo", 12, 0, 1024, 341),
            p("LFO 1 rate", 16, 0, 5000, 1500),
            p("LFO 2 rate", 20, 0, 5000, 2500),
            p("LFO 3 rate", 24, 0, 5000, 3500),
            p("LFO 1 wave", 28, 0, 1024, 0),
            p("LFO 2 wave", 32, 0, 1024, 0),
            p("LFO 3 wave", 36, 0, 1024, 0),
            p("Cross type", 40, 0, 1024, 0),
            p("Cross cutoff", 44, 0, 5000, 2500),
            p("Wet only", 48, 0, 1024, 0),
        ],
    },
    Def {
        kind: Kind::Echo,
        ident: "Fruity Delay 3",
        title: "Echo Lab",
        size: 108,
        fixed: &[(0, 1), (32, 6000), (36, 3000), (56, 1)],
        params: &[
            p("Input level", 4, 0, 6000, 6000),
            p("Model", 8, 0, 2, 2),
            p("Tempo sync", 12, 0, 1, 1),
            p("Keep pitch", 16, 0, 1, 0),
            p("Time", 20, 0, 768, 192),
            p("Stereo offset", 24, 0, 6000, 3000),
            p("Smoothing", 28, 0, 1000, 500),
            p("Mod rate", 40, 0, 6000, 0),
            p("Mod time", 44, 0, 6000, 0),
            p("Mod cutoff", 48, 0, 6000, 0),
            p("Diffusion", 52, 0, 6000, 0),
            p("Feedback", 60, 0, 6000, 3000),
            p("Cutoff", 68, 0, 6000, 6000),
            p("Resonance", 72, 0, 6000, 0),
            p("Distortion", 76, 0, 6000, 0),
            p("Sample rate", 88, 0, 60000, 60000),
            p("Bits", 92, 0, 6000, 6000),
            p("Wet", 96, 0, 6000, 6000),
            p("Tone", 100, -6000, 6000, 0),
            p("Dry", 104, 0, 6000, 6000),
        ],
    },
    Def {
        kind: Kind::Spread,
        ident: "Spreader",
        title: "Widener",
        size: 30,
        fixed: &[(0, 65539), (20, 1), (26, 0x0101_0000)],
        params: &[p("Spread", 4, 0, 10000, 5000), p("Separation", 8, -10000, 10000, 0), p("Low bypass", 12, 1, 1_000_000, 1), p("Mono lows", 16, 0, 1, 0)],
    },
    Def {
        kind: Kind::Transient,
        ident: "Transient Processor",
        title: "Transient Shaper",
        size: 49,
        fixed: &[(0, 1), (36, 1), (40, 5), (44, 12800)],
        params: &[
            p("Attack", 4, 0, 160, 80),
            p("Drive", 8, 0, 160, 0),
            p("Release", 12, 0, 160, 80),
            p("Gain", 16, 0, 160, 80),
            p("Attack shape", 20, 0, 2, 1),
            p("Release shape", 24, 0, 2, 1),
            p("Split frequency", 28, 0, 160, 80),
            p("Split balance", 32, 0, 160, 80),
        ],
    },
    Def {
        kind: Kind::Swarm,
        ident: "Hyper Chorus",
        title: "Swarm Chorus",
        size: 52,
        fixed: &[(0, 2)],
        params: &[
            p("Delay", 4, 0, 1000, 500),
            p("Feedback", 8, 0, 1000, 500),
            p("Modulation", 12, 0, 1000, 500),
            p("Mod rate", 16, 0, 1000, 500),
            p("Movement", 20, 0, 1000, 500),
            p("Move rate", 24, 0, 1000, 500),
            p("Eight lines", 28, 0, 1, 0),
            p("Low pass", 32, 20, 22000, 22000),
            p("High pass", 36, 20, 22000, 120),
            p("Phase spread", 40, 0, 1000, 500),
            p("Mix", 44, 0, 1000, 500),
        ],
    },
    Def {
        kind: Kind::MultiComp,
        ident: "Fruity Multiband Compressor",
        title: "Three Band Compressor",
        size: 124,
        fixed: &[(120, 50)],
        params: &[
            p("Gain", 4, 0, 65536, 32768),
            p("Linear phase", 12, 0, 1, 0),
            p("High state", 16, 0, 2, 0),
            p("High from", 20, 20, 20000, 17000),
            p("High gain", 24, 0, 65536, 21845),
            p("High threshold", 28, 0, 65536, 45875),
            p("High ratio", 32, 0, 65536, 32768),
            p("High attack", 36, 1, 1000, 100),
            p("High release", 40, 100, 10000, 1000),
            p("High knee", 44, 0, 65536, 52429),
            p("Mid state", 48, 0, 2, 0),
            p("Mid to", 52, 20, 20000, 17000),
            p("Mid from", 56, 20, 20000, 6500),
            p("Mid gain", 60, 0, 65536, 21845),
            p("Mid threshold", 64, 0, 65536, 45875),
            p("Mid ratio", 68, 0, 65536, 32768),
            p("Mid attack", 72, 1, 1000, 100),
            p("Mid release", 76, 100, 10000, 1000),
            p("Mid knee", 80, 0, 65536, 52429),
            p("Low state", 84, 0, 2, 0),
            p("Low to", 88, 20, 20000, 6500),
            p("Low gain", 92, 0, 65536, 21845),
            p("Low threshold", 96, 0, 65536, 45875),
            p("Low ratio", 100, 0, 65536, 32768),
            p("Low attack", 104, 1, 1000, 100),
            p("Low release", 108, 100, 10000, 1000),
            p("Low knee", 112, 0, 65536, 52429),
            p("Limiter", 8, 0, 1, 1),
        ],
    },
    Def {
        kind: Kind::OldEq,
        ident: "Fruity Parametric EQ",
        title: "Seven Band EQ (classic)",
        size: 116,
        fixed: &[],
        params: &[
            p("Band 1 level", 0, -1800, 1800, 0),
            p("Band 2 level", 4, -1800, 1800, 0),
            p("Band 3 level", 8, -1800, 1800, 0),
            p("Band 4 level", 12, -1800, 1800, 0),
            p("Band 5 level", 16, -1800, 1800, 0),
            p("Band 6 level", 20, -1800, 1800, 0),
            p("Band 7 level", 24, -1800, 1800, 0),
            p("Band 1 freq", 28, 0, 65536, 5777),
            p("Band 2 freq", 32, 0, 65536, 13024),
            p("Band 3 freq", 36, 0, 65536, 19904),
            p("Band 4 freq", 40, 0, 65536, 33145),
            p("Band 5 freq", 44, 0, 65536, 42327),
            p("Band 6 freq", 48, 0, 65536, 49307),
            p("Band 7 freq", 52, 0, 65536, 55825),
            p("Band 1 width", 56, 0, 65536, 17500),
            p("Band 2 width", 60, 0, 65536, 17500),
            p("Band 3 width", 64, 0, 65536, 17500),
            p("Band 4 width", 68, 0, 65536, 17500),
            p("Band 5 width", 72, 0, 65536, 24000),
            p("Band 6 width", 76, 0, 65536, 24000),
            p("Band 7 width", 80, 0, 65536, 17500),
            p("Band 1 type", 84, 0, 7, 5),
            p("Band 2 type", 88, 0, 7, 6),
            p("Band 3 type", 92, 0, 7, 6),
            p("Band 4 type", 96, 0, 7, 6),
            p("Band 5 type", 100, 0, 7, 6),
            p("Band 6 type", 104, 0, 7, 6),
            p("Band 7 type", 108, 0, 7, 7),
            p("Main level", 112, 0, 256, 128),
        ],
    },
    Def {
        kind: Kind::OldReverb,
        ident: "Fruity Reeverb",
        title: "Room Reverb (classic)",
        size: 44,
        fixed: &[(0, 1)],
        params: &[
            p("Low cut", 4, 0, 65536, 0),
            p("High cut", 8, 0, 65536, 65536),
            p("Predelay", 12, 0, 300, 0),
            p("Room size", 16, 0, 65536, 65536),
            p("Diffusion", 20, 0, 65536, 65536),
            p("Colour", 24, 0, 40, 20),
            p("Decay", 28, 0, 65536, 2048),
            p("High damping", 32, 0, 65536, 65536),
            p("Dry", 36, 0, 65536, 65536),
            p("Wet", 40, 0, 65536, 16384),
        ],
    },
    Def {
        kind: Kind::OldFlanger,
        ident: "Fruity Flanger",
        title: "Flanger (classic)",
        size: 52,
        fixed: &[(0, 1)],
        params: &[
            p("Delay", 4, 0, 1024, 0),
            p("Depth", 8, 0, 5000, 2000),
            p("Rate", 12, 0, 5000, 2000),
            p("Phase", 16, 0, 360, 118),
            p("Damp", 20, 0, 5000, 0),
            p("Shape", 24, 0, 1024, 0),
            p("Feedback", 28, 0, 100, 23),
            p("Invert feedback", 32, 0, 1024, 1024),
            p("Invert wet", 36, 0, 1024, 1024),
            p("Dry", 40, 0, 5000, 3500),
            p("Wet", 44, 0, 5000, 3000),
            p("Cross", 48, 0, 5000, 2500),
        ],
    },
    Def {
        kind: Kind::Maxi,
        ident: "Maximus",
        title: "Band Maximiser",
        size: 1342,
        fixed: &[],
        params: &[
            p("Low split", 4, 0, 65536, 19800),
            p("Low slope", 8, 0, 1, 1),
            p("High split", 12, 0, 65536, 48065),
            p("High slope", 16, 0, 1, 1),
            p("Band delay", 20, 0, 10000, 1115),
            p("Band mix", 24, 0, 1000, 1000),
            p("Low cut", 28, -1, 65536, 32768),
            p("Low mode", 32, 0, 3, 0),
            p("Low pre", 36, 0, 2000, 1000),
            p("Low post", 40, 0, 2000, 1000),
            p("Low separation", 44, -1000, 1000, 0),
            p("Low saturation", 48, -1000, 1000, 0),
            p("Low ceiling", 52, 0, 2000, 1000),
            p("Low attack", 56, 0, 10000, 1115),
            p("Low release 2", 60, 0, 10000, 0),
            p("Low curve", 64, 1, 8, 2),
            p("Low release", 68, 0, 10000, 5000),
            p("Low option", 72, 0, 4, 3),
            p("Low sustain", 76, 0, 10000, 2882),
            p("Low average", 80, 0, 1, 0),
            p("Mid mode", 84, 0, 3, 0),
            p("Mid pre", 88, 0, 2000, 1000),
            p("Mid post", 92, 0, 2000, 1000),
            p("Mid separation", 96, -1000, 1000, 0),
            p("Mid saturation", 100, -1000, 1000, 0),
            p("Mid ceiling", 104, 0, 2000, 1000),
            p("Mid attack", 108, 0, 10000, 1115),
            p("Mid release 2", 112, 0, 10000, 0),
            p("Mid curve", 116, 1, 8, 2),
            p("Mid release", 120, 0, 10000, 5000),
            p("Mid option", 124, 0, 4, 3),
            p("Mid sustain", 128, 0, 10000, 2015),
            p("Mid average", 132, 0, 1, 0),
            p("High mode", 136, 0, 3, 0),
            p("High pre", 140, 0, 2000, 1000),
            p("High post", 144, 0, 2000, 1000),
            p("High separation", 148, -1000, 1000, 0),
            p("High saturation", 152, -1000, 1000, 0),
            p("High ceiling", 156, 0, 2000, 1000),
            p("High attack", 160, 0, 10000, 1115),
            p("High release 2", 164, 0, 10000, 0),
            p("High curve", 168, 1, 8, 2),
            p("High release", 172, 0, 10000, 5000),
            p("High option", 176, 0, 4, 3),
            p("High sustain", 180, 0, 10000, 1115),
            p("High average", 184, 0, 1, 0),
            p("Main mode", 188, 0, 3, 0),
            p("Main pre", 192, 0, 2000, 1000),
            p("Main post", 196, 0, 2000, 1000),
            p("Main separation", 200, -1000, 1000, 0),
            p("Main saturation", 204, -1000, 1000, 0),
            p("Main ceiling", 208, 0, 2000, 1000),
            p("Main attack", 212, 0, 10000, 1115),
            p("Main release 2", 216, 0, 10000, 8096),
            p("Main curve", 220, 1, 8, 2),
            p("Main release", 224, 0, 10000, 5000),
            p("Main option", 228, 0, 4, 3),
            p("Main sustain", 232, 0, 10000, 2882),
            p("Main average", 236, 0, 1, 0),
        ],
    },
    Def {
        kind: Kind::BucketChorus,
        ident: "Vintage Chorus",
        title: "Bucket Chorus",
        size: 72,
        fixed: &[(0, 1)],
        params: &[
            p("Mode", 4, 0, 3, 0),
            p("Speed", 12, 1, 245760, 80),
            p("Wave", 16, 0, 1, 0),
            p("Left right phase", 20, 0, 6000, 3000),
            p("Time 1", 24, 0, 6000, 60),
            p("Time 2", 28, 0, 6000, 3000),
            p("Mix", 32, 0, 6000, 3000),
            p("Mono input", 36, 0, 1, 0),
            p("Invert wet", 40, 0, 1, 0),
            p("Feedback", 44, 0, 6000, 0),
            p("High pass", 48, 0, 6000, 3000),
            p("Gain", 52, 0, 6000, 0),
            p("Noise", 56, 0, 6000, 3000),
            p("Noise gate", 60, 0, 1, 0),
            p("Start phase", 68, 0, 6000, 0),
        ],
    },
    Def {
        kind: Kind::Plush,
        ident: "LuxeVerb",
        title: "Plush Reverb",
        size: 169,
        fixed: &[(0, 8)],
        params: &[
            p("Input gain", 13, -600, 1240, 746),
            p("High cut", 17, 1000, 20000, 20000),
            p("Low cut", 21, 20, 3000, 20),
            p("Reverb on", 25, 0, 1, 1),
            p("Decay", 29, 0, 16384, 6308),
            p("Predelay", 33, 0, 5000, 320),
            p("Predelay beats", 37, 1, 384, 192),
            p("Brightness", 41, 0, 1000, 741),
            p("Diffusion", 45, 0, 100, 50),
            p("Size", 49, 1, 100, 60),
            p("Mod amount", 53, 0, 1000, 103),
            p("Mod speed", 57, 0, 100, 10),
            p("Character", 61, 0, 1000, 196),
            p("Freeze", 65, 0, 2, 0),
            p("Quality", 69, 0, 1, 1),
            p("Loop on", 73, 0, 1, 0),
            p("Loop shift", 77, -120, 120, 120),
            p("Loop delay", 81, 10, 5000, 2988),
            p("Loop delay beats", 85, 48, 768, 192),
            p("Loop gain", 89, 0, 1000, 0),
            p("Loop high cut", 93, 700, 5000, 1719),
            p("Loop low cut", 97, 20, 3000, 720),
            p("Loop mix", 101, 0, 1000, 1000),
            p("Follower scale", 105, -1000, 1000, 0),
            p("Follower threshold", 109, -6000, 0, -4000),
            p("Follower attack", 113, 50, 600, 50),
            p("Follower release", 117, 50, 700, 400),
            p("Follower offset", 121, 0, 100, 50),
            p("Follower mode", 125, 0, 2, 0),
            p("Follower side input", 129, 0, 1, 0),
            p("Follower low cut", 133, 20, 1000, 301),
            p("Follower smooth", 137, 10, 600, 10),
            p("Dry", 141, -600, 1240, 746),
            p("Wet", 145, -600, 1240, 746),
            p("Peak frequency", 149, 40, 8000, 3536),
            p("Peak gain", 153, -1800, 1800, 258),
            p("Peak width", 157, 25, 4000, 487),
            p("Width", 161, 0, 200, 125),
            p("Limiter off", 165, 0, 1, 0),
            p("Switches", 8, 0, 0x0101_0101, 0x1_0000),
        ],
    },
    Def {
        kind: Kind::Pad,
        ident: "Effector",
        title: "Pad Effects",
        size: 316,
        fixed: &[(0, 0x464c_5353), (4, 308), (8, 0x4658_3031)],
        params: &[
            p("Effect", 268, 0, 10000, 1818),
            p("Bypass", 272, 0, 10000, 0),
            p("Mix", 276, 0, 10000, 10000),
            p("X", 280, 0, 10000, 5000),
            p("Y", 284, 0, 10000, 5000),
            p("Rate", 288, 0, 10000, 1111),
            p("Shape", 292, 0, 10000, 0),
            p("X mod", 296, 0, 10000, 5000),
            p("Y mod", 300, 0, 10000, 5000),
            p("Rate multiply", 304, 0, 10000, 0),
            p("Input", 308, 0, 10000, 10000),
            p("Output", 312, 0, 10000, 5000),
        ],
    },
    Def {
        kind: Kind::Conv,
        ident: "Fruity Convolver",
        title: "Impulse Reverb",
        size: 482,
        fixed: &[(0, 14), (20, 1)],
        params: &[p("Dry", 4, 0, 1000, 1000), p("Wet", 8, 0, 1000, 500), p("Input separation", 12, -1000, 1000, 0),
            p("Wet separation", 16, -1000, 1000, 0),
            // kept after the impulse, wherever that ends: see `convset`
            p("Normalize", CONV_AFTER, 0, 1, 1),
            p("Delay", CONV_AFTER + 4, 0, 1000, 0),
            p("Self-convolve", CONV_AFTER + 8, 0, 1000, 0),
            p("Stretch", CONV_AFTER + 12, -1000, 1000, 0),
            p("Equalise", CONV_AFTER + 16, 0, 1000, 1000),
        ],
    },
    Def { kind: Kind::Patch, ident: "Patcher", title: "Patch Bay", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Rack, ident: "Distructor", title: "Drive Rack", size: 958, fixed: &[], params: &[] },
    Def { kind: Kind::Multi, ident: "Multiband Delay", title: "Band Delay", size: 1781, fixed: &[], params: &[] },
    Def { kind: Kind::Hosted, ident: "Hosted Plug-in", title: "Plug-in", size: 0, fixed: &[], params: &[] },
    Def {
        kind: Kind::Xyz,
        ident: "Fruity X-Y-Z Controller",
        title: "Three Axis Pad",
        size: 560,
        fixed: &[],
        params: &[
            p("X", 4, 0, 65536, 32768),
            p("Y", 8, 0, 65536, 32768),
            p("Z", 12, 0, 65536, 32768),
            p("X speed", 16, 0, 4096, 2048),
            p("Y speed", 20, 0, 4096, 2048),
            p("Z speed", 24, 0, 4096, 2048),
            p("X acceleration", 28, 0, 4096, 3072),
            p("Y acceleration", 32, 0, 4096, 3072),
            p("Z acceleration", 36, 0, 4096, 3072),
            p("Global speed", 40, 0, 4096, 2048),
            p("Controller speed", 44, 0, 4096, 1024),
        ],
    },
    Def { kind: Kind::Thru, ident: "Newtime", title: "Time Editor", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Newtone", title: "Pitch Editor", size: 0, fixed: &[], params: &[] },
    // attacks and what follows them, split by a burst detector and treated apart; slot order as stored
    Def { kind: Kind::Split, ident: "Transmitter", title: "Attack Splitter", size: 192, fixed: &[(0, 3)], params: &[p("Sensitivity", 4, 0, 10000, 7475), p("Hold", 8, 0, 10000, 1968), p("Release", 12, 0, 10000, 1968), p("Attack level", 16, 0, 10000, 8743), p("Body level", 20, 0, 10000, 8743), p("Attack tilt", 24, 0, 10000, 5000), p("Body tilt", 28, 0, 10000, 5000), p("Attack diffusion mix", 32, 0, 10000, 10000), p("Attack diffusion", 36, 0, 10000, 9112), p("Attack diffusion length", 40, 0, 10000, 0), p("Body diffusion mix", 44, 0, 10000, 5000), p("Body diffusion", 48, 0, 10000, 5051), p("Body diffusion length", 52, 0, 10000, 5000), p("Attack drive mix", 56, 0, 10000, 10000), p("Attack drive", 60, 0, 10000, 3333), p("Attack warmth", 64, 0, 10000, 0), p("Body drive mix", 68, 0, 10000, 10000), p("Body drive", 72, 0, 10000, 3342), p("Body warmth", 76, 0, 10000, 0), p("Attack bounce mix", 80, 0, 10000, 10000), p("Attack bounce times", 84, 0, 10000, 5320), p("Attack bounce levels", 88, 0, 10000, 5275), p("Body bounce mix", 92, 0, 10000, 10000), p("Body bounce times", 96, 0, 10000, 4443), p("Body bounce levels", 100, 0, 10000, 4642), p("Attack echo mix", 104, 0, 10000, 10000), p("Attack echo time", 108, 0, 10000, 3531), p("Attack echo steps", 112, 12, 768, 144), p("Attack echo feedback", 116, 0, 10000, 0), p("Body echo mix", 120, 0, 10000, 10000), p("Body echo time", 124, 0, 10000, 3156), p("Body echo steps", 128, 12, 768, 240), p("Body echo feedback", 132, 0, 10000, 0), p("Link levels", 136, 0, 1, 1), p("Link sides", 140, 0, 1, 1), p("Mode", 144, 0, 2, 1), p("Attack on", 148, 0, 1, 1), p("Body on", 152, 0, 1, 1), p("Attack effect", 156, 0, 5, 0), p("Body effect", 160, 0, 5, 2), p("Attack echo to tempo", 164, 0, 1, 0), p("Body echo to tempo", 168, 0, 1, 0), p("Look ahead", 172, 0, 1, 0), p("Spare 1", 176, 0, 1, 0), p("Spare 2", 180, 0, 1, 0), p("Display speed", 184, 0, 100, 90), p("Display lines", 188, 0, 65793, 257)] },
    Def { kind: Kind::Thru, ident: "FL Studio Mobile Rack FX", title: "Mobile Modules FX", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Color Mapper", title: "Colour Router", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Envelope", title: "Note Envelope", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Keyboard Splitter", title: "Key Zones", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Key Mapper", title: "Key Remap", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Level Scaler", title: "Level Curve", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Script", title: "Note Script", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "VFX Sequencer", title: "Note Sequencer", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Voc, ident: "Vocodex", title: "Band Vocoder", size: 1182, fixed: &[], params: &[] },
    Def { kind: Kind::OldVoc, ident: "Fruity Vocoder", title: "Classic Vocoder", size: 129, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Wave Candy", title: "Scope Meter", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "ZGameEditor Visualizer", title: "Visual Stage", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Notes, ident: "Fruity NoteBook 2", title: "Note Pages", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Notes, ident: "Fruity NoteBook", title: "Note Pad", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Notes, ident: "Fruity HTML NoteBook", title: "Web Notes", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Surface, ident: "Control Surface", title: "Control Panel", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Edison", title: "Wave Recorder", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Formula, ident: "Fruity Formula Controller", title: "Formula Sender", size: 16, fixed: &[(0, 2)], params: &[p("A", 4, 0, 65536, 0), p("B", 8, 0, 65536, 0), p("C", 12, 0, 65536, 0)] },
    Def {
        kind: Kind::Tuner,
        ident: "Pitcher",
        title: "Voice Tuner",
        size: 160,
        fixed: &[(0, 0x464c_5353), (4, 152), (8, 0x5054_5231), (16, 0x3f80_0000)],
        params: &[
            p("Speed", 12, 0, 10000, 10000),
            p("Fine tune", 20, 0, 10000, 5000),
            p("Gender", 24, 0, 10000, 5000),
            p("Pan 1", 80, 0, 10000, 5000),
            p("Pan 2", 84, 0, 10000, 5000),
            p("Pan 3", 88, 0, 10000, 5000),
            p("Pan 4", 92, 0, 10000, 5000),
            p("Formant", 100, 0, 10000, 0),
            p("Spread", 104, 0, 10000, 5000),
            p("Lowest pitch", 108, 0, 10000, 0),
            p("Follow notes", 112, 0, 10000, 0),
            p("Follow octaves", 116, 0, 10000, 0),
            p("Harmonise", 120, 0, 10000, 0),
            p("Key", 124, 0, 110000, 0),
            p("Scale", 128, 0, 30000, 0),
            p("Reference", 140, 0, 30000, 20000),
            p("Notes C to D#", 144, 0, 0x0101_0101, 0x0101_0101),
            p("Notes E to G", 148, 0, 0x0101_0101, 0x0101_0101),
            p("Notes G# to B", 152, 0, 0x0101_0101, 0x0101_0101),
        ],
    },
    Def {
        kind: Kind::Peak,
        ident: "Fruity Peak Controller",
        title: "Level Follower",
        size: 42,
        fixed: &[],
        params: &[
            p("Peak base", 0, 0, 65536, 0),
            p("Peak amount", 4, -256, 256, 179),
            p("Peak tension", 8, -100, 100, 0),
            p("Peak decay", 12, 0, 128, 66),
            p("Sweep base", 16, 0, 65536, 0),
            p("Sweep amount", 20, -256, 256, 0),
            p("Sweep tension", 24, -100, 100, 0),
            p("Sweep speed", 28, 0, 65536, 32950),
            p("Sweep shape", 32, 0, 4, 0),
            p("Sweep phase", 36, 0, 65535, 0),
            p("Switches", 38, 0, 0x0101_0000, 0x0001_0000),
        ],
    },
    Def {
        kind: Kind::Warp,
        ident: "Gross Beat",
        title: "Beat Juggler",
        size: 13032,
        fixed: &[],
        params: &[
            p("Time slot", 4, 0, 35, 0),
            p("Volume slot", 8, 0, 35, 0),
            p("Time mix", 12, 0, 256, 256),
            p("Volume mix", 16, 0, 256, 256),
            p("Attack", 20, 0, 8192, 1765),
            p("Release", 24, 0, 8192, 5000),
            p("Tension", 28, 0, 256, 0),
        ],
    },
    Def {
        kind: Kind::FreeFilter,
        ident: "Fruity Free Filter",
        title: "Open Filter",
        size: 21,
        fixed: &[(0, 2), (17, 0x0100_0000)],
        params: &[p("Type", 4, 0, 700, 100), p("Frequency", 8, 0, 1024, 1024), p("Q", 12, 0, 1024, 0), p("Gain", 16, 0, 1024, 676)],
    },
    Def {
        kind: Kind::OldDelay,
        ident: "Fruity Delay",
        title: "Plain Echo",
        size: 28,
        fixed: &[(0, 1)],
        params: &[p("Input", 4, 0, 1024, 338), p("Feedback", 8, 0, 1024, 512), p("Cutoff", 12, 0, 1024, 1024), p("Tempo", 16, 0, 1024, 0), p("Steps", 20, 1, 16, 3), p("Mode", 24, 0, 1024, 0)],
    },
    Def { kind: Kind::SendTap, ident: "Fruity Send", title: "Send Tap", size: 16, fixed: &[], params: &[p("Pan", 0, -128, 127, 0), p("Dry", 4, 0, 256, 256), p("Volume", 8, 0, 256, 256), p("Send to", 12, 0, 125, 0)] },
    Def {
        kind: Kind::Squeeze,
        ident: "Fruity Squeeze",
        title: "Crusher",
        size: 52,
        fixed: &[],
        params: &[
            p("Gain", 4, 0, 127, 127),
            p("Mix", 8, 0, 127, 64),
            p("Steps", 12, 0, 65535, 1),
            p("Filter on", 16, 0, 1, 0),
            p("Filter type", 20, 0, 4, 0),
            p("Resonance", 24, 0, 65535, 32768),
            p("Cutoff", 28, 0, 65535, 32768),
            p("Filter post", 32, 0, 1, 0),
            p("Relation", 36, 0, 3, 0),
            p("Hit", 40, 0, 44100, 0),
            p("Keep", 44, 0, 44100, 7500),
            p("Hit level", 48, 0, 65535, 0),
        ],
    },
    Def { kind: Kind::DcBlock, ident: "Fruity Center", title: "Centre", size: 8, fixed: &[(0, 1)], params: &[p("On", 4, 0, 1, 1)] },
    Def {
        kind: Kind::Eq7,
        ident: "Fruity 7 Band EQ",
        title: "Seven Band EQ",
        size: 32,
        fixed: &[(0, 1)],
        params: &[p("63 Hz", 4, -1800, 1800, 0), p("250 Hz", 8, -1800, 1800, 0), p("500 Hz", 12, -1800, 1800, 0), p("1.5 kHz", 16, -1800, 1800, 0), p("3 kHz", 20, -1800, 1800, 0), p("5 kHz", 24, -1800, 1800, 0), p("8 kHz", 28, -1800, 1800, 0)],
    },
    Def { kind: Kind::Mute, ident: "Fruity Mute 2", title: "Mute Switch", size: 8, fixed: &[], params: &[p("On", 0, 0, 1, 1), p("Channel", 4, 0, 2, 0)] },
    Def { kind: Kind::Flip, ident: "Fruity Phase Inverter", title: "Polarity Flip", size: 4, fixed: &[], params: &[p("State", 0, 0, 2, 0)] },
    Def { kind: Kind::Thru, ident: "Fruity dB Meter", title: "Level Meter", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Fruity Big Clock", title: "Big Clock", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Fruity Spectroman", title: "Spectrum View", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Gm, ident: "Fruity LSD", title: "Bank Player", size: 97, fixed: &[], params: &[p("Port", 8, 0, 255, 0), p("Channel 1 patch", 33, 0, 127, 0), p("Channel 2 patch", 37, 0, 127, 0), p("Channel 3 patch", 41, 0, 127, 0), p("Channel 4 patch", 45, 0, 127, 0), p("Channel 5 patch", 49, 0, 127, 0), p("Channel 6 patch", 53, 0, 127, 0), p("Channel 7 patch", 57, 0, 127, 0), p("Channel 8 patch", 61, 0, 127, 0), p("Channel 9 patch", 65, 0, 127, 0), p("Channel 10 patch", 69, 0, 127, 0), p("Channel 11 patch", 73, 0, 127, 0), p("Channel 12 patch", 77, 0, 127, 0), p("Channel 13 patch", 81, 0, 127, 0), p("Channel 14 patch", 85, 0, 127, 0), p("Channel 15 patch", 89, 0, 127, 0), p("Channel 16 patch", 93, 0, 127, 0), p("Channel 1 drums", 33, 0, 1, 0), p("Channel 2 drums", 37, 0, 1, 0), p("Channel 3 drums", 41, 0, 1, 0), p("Channel 4 drums", 45, 0, 1, 0), p("Channel 5 drums", 49, 0, 1, 0), p("Channel 6 drums", 53, 0, 1, 0), p("Channel 7 drums", 57, 0, 1, 0), p("Channel 8 drums", 61, 0, 1, 0), p("Channel 9 drums", 65, 0, 1, 0), p("Channel 10 drums", 69, 0, 1, 1), p("Channel 11 drums", 73, 0, 1, 0), p("Channel 12 drums", 77, 0, 1, 0), p("Channel 13 drums", 81, 0, 1, 0), p("Channel 14 drums", 85, 0, 1, 0), p("Channel 15 drums", 89, 0, 1, 0), p("Channel 16 drums", 93, 0, 1, 0)] },
    Def { kind: Kind::Thru, ident: "Tuner", title: "Pitch Meter", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Razer Chroma", title: "Key Lights", size: 0, fixed: &[], params: &[] },
    Def {
        kind: Kind::Emph,
        ident: "Emphasizer",
        title: "Loudener",
        size: 29,
        fixed: &[(0, 2), (25, 0x0100_0000), (24, 10000)],
        params: &[p("Input gain", 4, 0, 10000, 0), p("Emphasis", 8, 0, 10000, 5013), p("Hard clip", 12, 0, 1, 0), p("Level match", 16, 0, 1, 0), p("Bypass", 20, 0, 1, 0)],
    },
    Def {
        kind: Kind::StonePhaser,
        ident: "Vintage Phaser",
        title: "Stone Phaser",
        size: 149,
        fixed: &[(0, 3), (20, 125), (64, 3000), (68, 1), (80, 3200), (84, 2000), (88, 50), (108, 5000), (116, 2000), (128, 6000), (136, 1), (140, 5000)],
        params: &[
            p("Min", 4, 125, 2048000, 896070),
            p("Max", 8, 125, 2048000, 1588996),
            p("Feedback", 12, 0, 6000, 3000),
            p("Sync", 24, 0, 1, 0),
            p("Speed", 28, 0, 196608, 1),
            p("L/R phase", 48, 0, 6000, 0),
            p("Input HP", 112, 0, 2048000, 2000),
            p("Rate low", 120, 1, 2048000, 25),
            p("Rate high", 124, 1, 2048000, 150000),
            p("Invert feedback", 144, 0, 1, 0),
        ],
    },
    Def {
        kind: Kind::Amp,
        ident: "Hardcore",
        title: "Pedal Board",
        size: 468,
        fixed: &[(0, 0x464c_5353), (4, 460), (8, 0x4843_5231), (60, 0x3f00_0000), (80, 0x3f00_0000), (84, 0x3f00_0000), (88, 0x3f00_0000), (92, 0x3f00_0000), (96, 0x3f00_0000), (100, 0x3f00_0000), (104, 0x3f00_0000), (108, 0x3f00_0000), (112, 0x3f00_0000), (116, 0x3f00_0000), (120, 0x3f00_0000), (124, 0x3f00_0000), (128, 0x3f00_0000), (132, 0x3f00_0000), (136, 0x3f00_0000), (140, 0x3f00_0000), (144, 0x3f00_0000), (148, 0x3f00_0000), (152, 0x3f00_0000), (156, 0x3f00_0000), (160, 0x3f00_0000), (164, 0x3f00_0000), (188, 0x3f00_0000), (208, 0x3f00_0000), (168, 0x3f80_0000), (184, 0x3f80_0000), (192, 0x3f80_0000)],
        params: &[p("Input", 12, 0, 10000, 5000), p("Output", 16, 0, 10000, 5000), p("EQ on", 20, 0, 10000, 0), p("100 Hz", 24, 0, 10000, 5000), p("200 Hz", 28, 0, 10000, 5000), p("400 Hz", 32, 0, 10000, 5000), p("800 Hz", 36, 0, 10000, 5000), p("1.6 kHz", 40, 0, 10000, 5000), p("3.2 kHz", 44, 0, 10000, 5000), p("6.4 kHz", 48, 0, 10000, 5000), p("12.8 kHz", 52, 0, 10000, 5000), p("Cabinet", 56, 0, 200000, 0), p("Drive", 64, 0, 10000, 5000), p("Comb", 68, 0, 10000, 5000), p("Body", 72, 0, 10000, 5000), p("Glass", 76, 0, 10000, 5000), p("Chorus width", 80, 0, 10000, 5000), p("Chorus speed", 84, 0, 10000, 5000), p("Chorus mix", 88, 0, 10000, 5000), p("Pedal 1 on", 172, 0, 10000, 0), p("Pedal 2 on", 176, 0, 10000, 0), p("Pedal 3 on", 180, 0, 10000, 0), p("Pedal 1", 196, 0, 200000, 0), p("Pedal 2", 200, 0, 200000, 0), p("Pedal 3", 204, 0, 200000, 0)],
    },
    Def {
        kind: Kind::Relooper,
        ident: "Transporter",
        title: "Relooper",
        size: 94,
        fixed: &[(0, 4), (84, 131329), (88, 65536)],
        params: &[
            p("Dry", 4, 0, 12800, 12800),
            p("Loop A", 8, 0, 12800, 12800),
            p("Loop B", 12, 0, 12800, 12800),
            p("Reverse", 16, 0, 10000, 0),
            p("Shift", 20, 0, 10000, 0),
            p("Down/up", 24, 0, 10000, 0),
            p("Sensitivity", 28, 0, 1024, 512),
            p("Min length", 32, 1, 10000, 500),
            p("A/B ratio", 36, 0, 64, 8),
            p("Spread", 40, 0, 1024, 512),
            p("Bounce rate", 44, 0, 1024, 512),
            p("Mode", 56, 0, 3, 0),
        ],
    },
    Def { kind: Kind::Deck, ident: "Fruity Scratcher", title: "Turntable", size: 64, fixed: &[(0, 1001000), (8, 0x3f800000), (16, 0x3e800000), (20, 0x3f000000), (32, 0x3f000000), (40, 0x3f000000), (44, 0x3f6872b0), (48, 0x3f7fbe77), (52, 0x3ecccccd), (56, 0x41100000)], params: &[
            // in the order of the block, which is the order events count them in; which of the middle ones is
            // which is our reading of their rest values
            p("Position", 4, 0, 10000, 0),
            p("Direction", 8, 0, 10000, 10000),
            p("Pan", 12, 0, 10000, 5000),
            p("Acceleration", 16, 0, 10000, 2500),
            p("Sensitivity", 20, 0, 10000, 5000),
            p("Volume", 24, 0, 10000, 7800),
            p("Hold", 28, 0, 10000, 0),
            p("Speed", 32, 0, 10000, 5000),
            p("Mute", 36, 0, 10000, 0),
        ],
    },
    Def { kind: Kind::BassBoost, ident: "Fruity Bass Boost", title: "Bass Lift", size: 12, fixed: &[(0, 1)], params: &[p("Frequency", 4, 0, 1024, 512), p("Amount", 8, 0, 1024, 768)] },
    Def {
        kind: Kind::Phaser,
        ident: "Fruity Phaser",
        title: "Phaser",
        size: 40,
        fixed: &[(0, 1)],
        params: &[
            p("Sweep rate", 4, 0, 5000, 1500),
            p("Depth low", 8, 0, 1000, 0),
            p("Depth high", 12, 0, 1000, 900),
            p("Rate range", 16, 0, 1024, 0),
            p("Stereo", 20, 0, 1024, 512),
            p("Stages", 24, 1, 23, 11),
            p("Feedback", 28, 0, 1000, 693),
            p("Dry", 32, 0, 1024, 0),
            p("Gain", 36, 0, 5000, 2500),
        ],
    },
    Def {
        kind: Kind::FreqShift,
        ident: "Frequency Shifter",
        title: "Hertz Shifter",
        size: 52,
        fixed: &[(0, 4)],
        params: &[
            p("Mix", 4, 0, 10000, 10000),
            p("Mode", 8, 0, 3, 0),
            p("Frequency", 12, -100000, 100000, 70000),
            p("Right phase", 16, 0, 10000, 0),
            p("Sideband left", 20, 0, 10000, 10000),
            p("Sideband right", 24, 0, 10000, 10000),
            p("Feedback", 28, 0, 10000, 5000),
            p("Width", 32, 0, 10000, 10000),
            p("Range", 36, 0, 2, 1),
            p("Start phase", 40, 0, 10000, 5000),
        ],
    },
    Def {
        kind: Kind::Finisher,
        ident: "Emphasis",
        title: "Finisher",
        size: 67,
        fixed: &[(0, 10), (12, 1), (60, 1), (63, 0x0101_0000)],
        params: &[
            p("Mode", 24, 0, 3, 0),
            p("Input", 28, 0, 10000, 0),
            p("Linking", 32, 0, 10000, 10000),
            p("Amount", 36, 0, 10000, 10000),
            p("Ceiling", 40, 0, 10000, 9912),
            p("Envelope", 48, 0, 4, 2),
            p("Hardness", 52, 0, 9, 7),
        ],
    },
    Def {
        kind: Kind::LowLift,
        ident: "Low Lifter",
        title: "Low Harmonics",
        size: 82,
        fixed: &[(0, 6), (4, 1), (20, 1), (56, 1), (60, 1), (64, 196609), (68, 196608), (72, 131072), (76, 131072)],
        params: &[
            p("Mix", 8, 0, 10000, 10000),
            p("High gain", 12, 0, 10000, 5000),
            p("Cutoff", 16, 0, 10000, 6993),
            p("Multi band", 24, 0, 1, 0),
            p("Drive", 28, 0, 10000, 4998),
            p("Diffusion depth", 32, 0, 10000, 0),
            p("Low gain", 36, 0, 10000, 5000),
            p("Slope", 44, 0, 10000, 8920),
            p("Diffusion mix", 48, 0, 10000, 0),
        ],
    },
    // 31 third-octave bands, eight curves to morph between and a master curve; the curves follow the settings
    Def {
        kind: Kind::MorphEq,
        ident: "EQUO",
        title: "Morph Equaliser",
        size: 4143,
        fixed: &[],
        params: &[
            p("Mix", 4100, -128, 128, 128),
            p("Morph", 4104, 0, 7000, 0),
            p("Smooth", 4108, 0, 256, 170),
            p("Volume", 4112, 0, 256, 128),
            p("Double rate", 4116, 0, 1, 0),
            p("Band width", 4120, 0, 300, 150),
            // these two are single bytes
            p("Add", 4137, 0, 1, 0),
            p("Send only", 4136, 0, 1, 0),
        ],
    },
    // the offsets here only number the settings: the block is packed and read by its own module
    Def {
        kind: Kind::Bank,
        ident: "Fruity Love Philter",
        title: "Filter Bank",
        size: 0,
        fixed: &[],
        params: &[
            p("Pad X", 0, 0, 1024, 512),
            p("Pad Y", 4, 0, 1024, 512),
            p("Follower attack", 8, 0, 16384, 4096),
            p("Follower release", 12, 0, 16384, 8192),
            p("1 Envelope", 16, 0, 256, 128),
            p("1 Cutoff", 20, 0, 256, 128),
            p("1 Resonance", 24, 0, 256, 128),
            p("1 Low", 28, 0, 128, 128),
            p("1 Band", 32, 0, 128, 0),
            p("1 High", 36, 0, 128, 0),
            p("1 Shaper drive", 40, 0, 256, 128),
            p("1 Shaper mix", 44, 0, 128, 128),
            p("1 Pan", 48, -64, 64, 0),
            p("1 Volume", 52, 0, 160, 128),
            p("1 Input", 56, 0, 160, 128),
            p("1 To next", 60, -160, 160, 0),
            p("1 Slope", 64, 0, 5, 1),
            p("1 Type", 68, 0, 11, 0),
            p("1 On", 72, 0, 1, 1),
            p("1 Shaper on", 76, 0, 1, 0),
            p("1 Shaper two-sided", 80, 0, 1, 0),
            p("2 Envelope", 84, 0, 256, 128),
            p("2 Cutoff", 88, 0, 256, 128),
            p("2 Resonance", 92, 0, 256, 128),
            p("2 Low", 96, 0, 128, 128),
            p("2 Band", 100, 0, 128, 0),
            p("2 High", 104, 0, 128, 0),
            p("2 Shaper drive", 108, 0, 256, 128),
            p("2 Shaper mix", 112, 0, 128, 128),
            p("2 Pan", 116, -64, 64, 0),
            p("2 Volume", 120, 0, 160, 128),
            p("2 Input", 124, 0, 160, 128),
            p("2 To next", 128, -160, 160, 0),
            p("2 Slope", 132, 0, 5, 1),
            p("2 Type", 136, 0, 11, 0),
            p("2 On", 140, 0, 1, 0),
            p("2 Shaper on", 144, 0, 1, 0),
            p("2 Shaper two-sided", 148, 0, 1, 0),
            p("3 Envelope", 152, 0, 256, 128),
            p("3 Cutoff", 156, 0, 256, 128),
            p("3 Resonance", 160, 0, 256, 128),
            p("3 Low", 164, 0, 128, 128),
            p("3 Band", 168, 0, 128, 0),
            p("3 High", 172, 0, 128, 0),
            p("3 Shaper drive", 176, 0, 256, 128),
            p("3 Shaper mix", 180, 0, 128, 128),
            p("3 Pan", 184, -64, 64, 0),
            p("3 Volume", 188, 0, 160, 128),
            p("3 Input", 192, 0, 160, 128),
            p("3 To next", 196, -160, 160, 0),
            p("3 Slope", 200, 0, 5, 1),
            p("3 Type", 204, 0, 11, 0),
            p("3 On", 208, 0, 1, 0),
            p("3 Shaper on", 212, 0, 1, 0),
            p("3 Shaper two-sided", 216, 0, 1, 0),
            p("4 Envelope", 220, 0, 256, 128),
            p("4 Cutoff", 224, 0, 256, 128),
            p("4 Resonance", 228, 0, 256, 128),
            p("4 Low", 232, 0, 128, 128),
            p("4 Band", 236, 0, 128, 0),
            p("4 High", 240, 0, 128, 0),
            p("4 Shaper drive", 244, 0, 256, 128),
            p("4 Shaper mix", 248, 0, 128, 128),
            p("4 Pan", 252, -64, 64, 0),
            p("4 Volume", 256, 0, 160, 128),
            p("4 Input", 260, 0, 160, 128),
            p("4 To next", 264, -160, 160, 0),
            p("4 Slope", 268, 0, 5, 1),
            p("4 Type", 272, 0, 11, 0),
            p("4 On", 276, 0, 1, 0),
            p("4 Shaper on", 280, 0, 1, 0),
            p("4 Shaper two-sided", 284, 0, 1, 0),
            p("5 Envelope", 288, 0, 256, 128),
            p("5 Cutoff", 292, 0, 256, 128),
            p("5 Resonance", 296, 0, 256, 128),
            p("5 Low", 300, 0, 128, 128),
            p("5 Band", 304, 0, 128, 0),
            p("5 High", 308, 0, 128, 0),
            p("5 Shaper drive", 312, 0, 256, 128),
            p("5 Shaper mix", 316, 0, 128, 128),
            p("5 Pan", 320, -64, 64, 0),
            p("5 Volume", 324, 0, 160, 128),
            p("5 Input", 328, 0, 160, 128),
            p("5 To next", 332, -160, 160, 0),
            p("5 Slope", 336, 0, 5, 1),
            p("5 Type", 340, 0, 11, 0),
            p("5 On", 344, 0, 1, 0),
            p("5 Shaper on", 348, 0, 1, 0),
            p("5 Shaper two-sided", 352, 0, 1, 0),
            p("6 Envelope", 356, 0, 256, 128),
            p("6 Cutoff", 360, 0, 256, 128),
            p("6 Resonance", 364, 0, 256, 128),
            p("6 Low", 368, 0, 128, 128),
            p("6 Band", 372, 0, 128, 0),
            p("6 High", 376, 0, 128, 0),
            p("6 Shaper drive", 380, 0, 256, 128),
            p("6 Shaper mix", 384, 0, 128, 128),
            p("6 Pan", 388, -64, 64, 0),
            p("6 Volume", 392, 0, 160, 128),
            p("6 Input", 396, 0, 160, 128),
            p("6 To next", 400, -160, 160, 0),
            p("6 Slope", 404, 0, 5, 1),
            p("6 Type", 408, 0, 11, 0),
            p("6 On", 412, 0, 1, 0),
            p("6 Shaper on", 416, 0, 1, 0),
            p("6 Shaper two-sided", 420, 0, 1, 0),
            p("7 Envelope", 424, 0, 256, 128),
            p("7 Cutoff", 428, 0, 256, 128),
            p("7 Resonance", 432, 0, 256, 128),
            p("7 Low", 436, 0, 128, 128),
            p("7 Band", 440, 0, 128, 0),
            p("7 High", 444, 0, 128, 0),
            p("7 Shaper drive", 448, 0, 256, 128),
            p("7 Shaper mix", 452, 0, 128, 128),
            p("7 Pan", 456, -64, 64, 0),
            p("7 Volume", 460, 0, 160, 128),
            p("7 Input", 464, 0, 160, 128),
            p("7 To next", 468, -160, 160, 0),
            p("7 Slope", 472, 0, 5, 1),
            p("7 Type", 476, 0, 11, 0),
            p("7 On", 480, 0, 1, 0),
            p("7 Shaper on", 484, 0, 1, 0),
            p("7 Shaper two-sided", 488, 0, 1, 0),
            p("8 Envelope", 492, 0, 256, 128),
            p("8 Cutoff", 496, 0, 256, 128),
            p("8 Resonance", 500, 0, 256, 128),
            p("8 Low", 504, 0, 128, 128),
            p("8 Band", 508, 0, 128, 0),
            p("8 High", 512, 0, 128, 0),
            p("8 Shaper drive", 516, 0, 256, 128),
            p("8 Shaper mix", 520, 0, 128, 128),
            p("8 Pan", 524, -64, 64, 0),
            p("8 Volume", 528, 0, 160, 128),
            p("8 Input", 532, 0, 160, 128),
            p("8 To next", 536, -160, 160, 0),
            p("8 Slope", 540, 0, 5, 1),
            p("8 Type", 544, 0, 11, 0),
            p("8 On", 548, 0, 1, 0),
            p("8 Shaper on", 552, 0, 1, 0),
            p("8 Shaper two-sided", 556, 0, 1, 0),
            p("Volume", 560, 0, 160, 128),
            p("Wave level", 564, 0, 160, 128),
        ],
    },
    Def {
        kind: Kind::Xy,
        ident: "Fruity X-Y Controller",
        title: "Pad Controller",
        size: 71,
        fixed: &[],
        params: &[p("X", 4, 0, 65536, 32768), p("Y", 20, 0, 65536, 32768), p("Speed", 36, 0, 4096, 2048), p("Acceleration", 40, 0, 4096, 1024)],
    },
    Def {
        kind: Kind::GrainShift,
        ident: "Pitch Shifter",
        title: "Grain Shifter",
        size: 75,
        fixed: &[(0, 7), (48, 65536), (56, 65536), (64, 256), (71, 0x0101_0000)],
        params: &[
            p("Pitch", 4, -1200, 1200, 0),
            p("Formant", 8, -1200, 1200, 0),
            p("Detector", 12, 0, 2, 0),
            p("Density", 16, 600, 2400, 2400),
            p("Colour", 20, 2000, 15000, 8500),
            p("Duration", 24, 100, 500, 200),
            p("Jitter", 28, 0, 100, 0),
            p("Random", 32, 0, 100, 0),
            p("Delay", 36, 1, 768, 768),
            p("Feedback", 40, 0, 100, 0),
            p("Delay mode", 44, 0, 3, 0),
            p("Delay options", 60, 0, 0x0101_0101, 256),
            p("Mix", 69, 0, 2000, 1000),
        ],
    },
    Def {
        kind: Kind::BandSplit,
        ident: "Frequency Splitter",
        title: "Band Splitter",
        size: 113,
        fixed: &[(0, 4), (28, 2), (32, 45820), (72, 2), (76, 2), (80, 12), (88, 274), (92, 768), (96, 256), (100, 512), (104, 512), (108, 512)],
        params: &[
            p("Low split", 4, 0, 65536, 19800),
            p("High split", 8, 0, 65536, 48065),
            p("Low slope", 12, 0, 7, 3),
            p("High slope", 16, 0, 7, 3),
            p("Link", 20, 0, 1, 0),
            p("Three bands", 24, 0, 1, 1),
            p("Low level", 48, -1800, 1800, 0),
            p("Mid level", 52, -1800, 1800, 0),
            p("High level", 56, -1800, 1800, 0),
            p("Mutes", 84, 0, 63, 0),
            p("Low to", 60, 0, 125, 0),
            p("Mid to", 64, 0, 125, 0),
            p("High to", 68, 0, 125, 0),
        ],
    },
    Def {
        kind: Kind::EchoBank,
        ident: "Fruity Delay Bank",
        title: "Echo Bank",
        size: 792,
        fixed: &[],
        params: &[
            p("Dry", 4, 0, 128, 128),
            p("Wet", 8, 0, 128, 128),
            p("Input", 12, 0, 128, 128),
            p("Feedback", 16, 0, 128, 128),
            p("1 in pan", 20, -128, 128, 0),
            p("1 in volume", 24, 0, 160, 128),
            p("1 cutoff", 28, 0, 65536, 33145),
            p("1 resonance", 32, 0, 65536, 17500),
            p("1 filter gain", 36, 0, 200, 0),
            p("1 separation", 40, -128, 128, 0),
            p("1 echo volume", 44, 0, 128, 64),
            p("1 echo pan", 48, -128, 128, 0),
            p("1 time", 52, 0, 3840, 1440),
            p("1 offset", 56, -480, 480, 0),
            p("1 echo cutoff", 60, 0, 65536, 33145),
            p("1 echo resonance", 64, 0, 65536, 17500),
            p("1 echo filter gain", 68, 0, 200, 0),
            p("1 out pan", 72, -128, 128, 0),
            p("1 out volume", 76, -128, 128, 128),
            p("1 to next", 80, -160, 160, 0),
            p("1 filter", 536, 0, 7, 0),
            p("1 echo filter", 544, 0, 7, 0),
            p("1 modes", 552, 0, 0x0000_0301, 256),
            p("1 switches", 564, 0, 0x0101_0101, 0x0100_0100),
            p("2 in pan", 84, -128, 128, 0),
            p("2 in volume", 88, 0, 160, 128),
            p("2 cutoff", 92, 0, 65536, 33145),
            p("2 resonance", 96, 0, 65536, 17500),
            p("2 filter gain", 100, 0, 200, 0),
            p("2 separation", 104, -128, 128, 0),
            p("2 echo volume", 108, 0, 128, 64),
            p("2 echo pan", 112, -128, 128, 0),
            p("2 time", 116, 0, 3840, 1440),
            p("2 offset", 120, -480, 480, 0),
            p("2 echo cutoff", 124, 0, 65536, 33145),
            p("2 echo resonance", 128, 0, 65536, 17500),
            p("2 echo filter gain", 132, 0, 200, 0),
            p("2 out pan", 136, -128, 128, 0),
            p("2 out volume", 140, -128, 128, 128),
            p("2 to next", 144, -160, 160, 0),
            p("2 filter", 568, 0, 7, 0),
            p("2 echo filter", 576, 0, 7, 0),
            p("2 modes", 584, 0, 0x0000_0301, 256),
            p("2 switches", 596, 0, 0x0101_0101, 0x0100_0000),
            p("3 in pan", 148, -128, 128, 0),
            p("3 in volume", 152, 0, 160, 128),
            p("3 cutoff", 156, 0, 65536, 33145),
            p("3 resonance", 160, 0, 65536, 17500),
            p("3 filter gain", 164, 0, 200, 0),
            p("3 separation", 168, -128, 128, 0),
            p("3 echo volume", 172, 0, 128, 64),
            p("3 echo pan", 176, -128, 128, 0),
            p("3 time", 180, 0, 3840, 1440),
            p("3 offset", 184, -480, 480, 0),
            p("3 echo cutoff", 188, 0, 65536, 33145),
            p("3 echo resonance", 192, 0, 65536, 17500),
            p("3 echo filter gain", 196, 0, 200, 0),
            p("3 out pan", 200, -128, 128, 0),
            p("3 out volume", 204, -128, 128, 128),
            p("3 to next", 208, -160, 160, 0),
            p("3 filter", 600, 0, 7, 0),
            p("3 echo filter", 608, 0, 7, 0),
            p("3 modes", 616, 0, 0x0000_0301, 256),
            p("3 switches", 628, 0, 0x0101_0101, 0x0100_0000),
            p("4 in pan", 212, -128, 128, 0),
            p("4 in volume", 216, 0, 160, 128),
            p("4 cutoff", 220, 0, 65536, 33145),
            p("4 resonance", 224, 0, 65536, 17500),
            p("4 filter gain", 228, 0, 200, 0),
            p("4 separation", 232, -128, 128, 0),
            p("4 echo volume", 236, 0, 128, 64),
            p("4 echo pan", 240, -128, 128, 0),
            p("4 time", 244, 0, 3840, 1440),
            p("4 offset", 248, -480, 480, 0),
            p("4 echo cutoff", 252, 0, 65536, 33145),
            p("4 echo resonance", 256, 0, 65536, 17500),
            p("4 echo filter gain", 260, 0, 200, 0),
            p("4 out pan", 264, -128, 128, 0),
            p("4 out volume", 268, -128, 128, 128),
            p("4 to next", 272, -160, 160, 0),
            p("4 filter", 632, 0, 7, 0),
            p("4 echo filter", 640, 0, 7, 0),
            p("4 modes", 648, 0, 0x0000_0301, 256),
            p("4 switches", 660, 0, 0x0101_0101, 0x0100_0000),
            p("5 in pan", 276, -128, 128, 0),
            p("5 in volume", 280, 0, 160, 128),
            p("5 cutoff", 284, 0, 65536, 33145),
            p("5 resonance", 288, 0, 65536, 17500),
            p("5 filter gain", 292, 0, 200, 0),
            p("5 separation", 296, -128, 128, 0),
            p("5 echo volume", 300, 0, 128, 64),
            p("5 echo pan", 304, -128, 128, 0),
            p("5 time", 308, 0, 3840, 1440),
            p("5 offset", 312, -480, 480, 0),
            p("5 echo cutoff", 316, 0, 65536, 33145),
            p("5 echo resonance", 320, 0, 65536, 17500),
            p("5 echo filter gain", 324, 0, 200, 0),
            p("5 out pan", 328, -128, 128, 0),
            p("5 out volume", 332, -128, 128, 128),
            p("5 to next", 336, -160, 160, 0),
            p("5 filter", 664, 0, 7, 0),
            p("5 echo filter", 672, 0, 7, 0),
            p("5 modes", 680, 0, 0x0000_0301, 256),
            p("5 switches", 692, 0, 0x0101_0101, 0x0100_0000),
            p("6 in pan", 340, -128, 128, 0),
            p("6 in volume", 344, 0, 160, 128),
            p("6 cutoff", 348, 0, 65536, 33145),
            p("6 resonance", 352, 0, 65536, 17500),
            p("6 filter gain", 356, 0, 200, 0),
            p("6 separation", 360, -128, 128, 0),
            p("6 echo volume", 364, 0, 128, 64),
            p("6 echo pan", 368, -128, 128, 0),
            p("6 time", 372, 0, 3840, 1440),
            p("6 offset", 376, -480, 480, 0),
            p("6 echo cutoff", 380, 0, 65536, 33145),
            p("6 echo resonance", 384, 0, 65536, 17500),
            p("6 echo filter gain", 388, 0, 200, 0),
            p("6 out pan", 392, -128, 128, 0),
            p("6 out volume", 396, -128, 128, 128),
            p("6 to next", 400, -160, 160, 0),
            p("6 filter", 696, 0, 7, 0),
            p("6 echo filter", 704, 0, 7, 0),
            p("6 modes", 712, 0, 0x0000_0301, 256),
            p("6 switches", 724, 0, 0x0101_0101, 0x0100_0000),
            p("7 in pan", 404, -128, 128, 0),
            p("7 in volume", 408, 0, 160, 128),
            p("7 cutoff", 412, 0, 65536, 33145),
            p("7 resonance", 416, 0, 65536, 17500),
            p("7 filter gain", 420, 0, 200, 0),
            p("7 separation", 424, -128, 128, 0),
            p("7 echo volume", 428, 0, 128, 64),
            p("7 echo pan", 432, -128, 128, 0),
            p("7 time", 436, 0, 3840, 1440),
            p("7 offset", 440, -480, 480, 0),
            p("7 echo cutoff", 444, 0, 65536, 33145),
            p("7 echo resonance", 448, 0, 65536, 17500),
            p("7 echo filter gain", 452, 0, 200, 0),
            p("7 out pan", 456, -128, 128, 0),
            p("7 out volume", 460, -128, 128, 128),
            p("7 to next", 464, -160, 160, 0),
            p("7 filter", 728, 0, 7, 0),
            p("7 echo filter", 736, 0, 7, 0),
            p("7 modes", 744, 0, 0x0000_0301, 256),
            p("7 switches", 756, 0, 0x0101_0101, 0x0100_0000),
            p("8 in pan", 468, -128, 128, 0),
            p("8 in volume", 472, 0, 160, 128),
            p("8 cutoff", 476, 0, 65536, 33145),
            p("8 resonance", 480, 0, 65536, 17500),
            p("8 filter gain", 484, 0, 200, 0),
            p("8 separation", 488, -128, 128, 0),
            p("8 echo volume", 492, 0, 128, 64),
            p("8 echo pan", 496, -128, 128, 0),
            p("8 time", 500, 0, 3840, 1440),
            p("8 offset", 504, -480, 480, 0),
            p("8 echo cutoff", 508, 0, 65536, 33145),
            p("8 echo resonance", 512, 0, 65536, 17500),
            p("8 echo filter gain", 516, 0, 200, 0),
            p("8 out pan", 520, -128, 128, 0),
            p("8 out volume", 524, -128, 128, 128),
            p("8 to next", 528, -1, -1, -1),
            p("8 filter", 760, 0, 7, 0),
            p("8 echo filter", 768, 0, 7, 0),
            p("8 modes", 776, 0, 0x0000_0301, 256),
            p("8 switches", 788, 0, 0x0101_0101, 0x0100_0000),
            p("Layout", 0, 15, 16, 16),
            p("1 slope", 540, 0, 2, 0),
            p("1 echo slope", 548, 0, 2, 0),
            p("1 grain", 556, 0, 4096, 0),
            p("1 grain shape", 560, -32768, 32768, 0),
            p("2 slope", 572, 0, 2, 0),
            p("2 echo slope", 580, 0, 2, 0),
            p("2 grain", 588, 0, 4096, 0),
            p("2 grain shape", 592, -32768, 32768, 0),
            p("3 slope", 604, 0, 2, 0),
            p("3 echo slope", 612, 0, 2, 0),
            p("3 grain", 620, 0, 4096, 0),
            p("3 grain shape", 624, -32768, 32768, 0),
            p("4 slope", 636, 0, 2, 0),
            p("4 echo slope", 644, 0, 2, 0),
            p("4 grain", 652, 0, 4096, 0),
            p("4 grain shape", 656, -32768, 32768, 0),
            p("5 slope", 668, 0, 2, 0),
            p("5 echo slope", 676, 0, 2, 0),
            p("5 grain", 684, 0, 4096, 0),
            p("5 grain shape", 688, -32768, 32768, 0),
            p("6 slope", 700, 0, 2, 0),
            p("6 echo slope", 708, 0, 2, 0),
            p("6 grain", 716, 0, 4096, 0),
            p("6 grain shape", 720, -32768, 32768, 0),
            p("7 slope", 732, 0, 2, 0),
            p("7 echo slope", 740, 0, 2, 0),
            p("7 grain", 748, 0, 4096, 0),
            p("7 grain shape", 752, -32768, 32768, 0),
            p("8 slope", 764, 0, 2, 0),
            p("8 echo slope", 772, 0, 2, 0),
            p("8 grain", 780, 0, 4096, 0),
            p("8 grain shape", 784, -32768, 32768, 0),
        ],
    },
    Def { kind: Kind::Curve, ident: "Fruity WaveShaper", title: "Curve Shaper", size: 102, fixed: &[], params: &[p("Pre", 4, 0, 256, 128), p("Mix", 8, 0, 128, 128), p("Post", 12, 0, 128, 128)] },
];

/// Points of the curve shaper's table.
const CURVE_N: usize = 257;

/// A fresh curve shaper block: one straight line from corner to corner.
fn curve_fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(102);
    for x in [4i32, 128, 128, 128] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(&[0; 6]);
    for x in [3i32, 3, 2] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    for (x, y) in [(0f64, 0f64), (1.0, 1.0)] {
        v.extend_from_slice(&x.to_le_bytes());
        v.extend_from_slice(&y.to_le_bytes());
        v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 2]);
    }
    for x in [0i32, -1, -1, -1, -1] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v
}

/// Entries in each of the three axis pad's mapping tables.
pub const XYZ_N: usize = 65;

/// A new three axis pad: everything centred, straight mappings in and out.
fn xyz_fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(560);
    for x in [3i32, 32768, 32768, 32768, 2048, 2048, 2048, 3072, 3072, 3072, 2048, 1024, 6, 0x90] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    for _ in 0..3 {
        v.extend_from_slice(&[0; 8]);
        for _ in 0..2 {
            for x in [3i32, 3, 2] {
                v.extend_from_slice(&x.to_le_bytes());
            }
            for (x, y) in [(0f64, 0f64), (1.0, 1.0)] {
                v.extend_from_slice(&x.to_le_bytes());
                v.extend_from_slice(&y.to_le_bytes());
                v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 2]);
            }
            for x in [0i32, -1, -1, -1, -1] {
                v.extend_from_slice(&x.to_le_bytes());
            }
        }
    }
    v
}

/// Where the six mapping lines of a three axis pad start: in and out for X, then Y, then Z.
fn xyz_lines(state: &[u8]) -> Option<Vec<usize>> {
    let int = |o: usize| state.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut heads = Vec::with_capacity(6);
    let mut at = 56;
    for _ in 0..3 {
        at += 8;
        for _ in 0..2 {
            heads.push(at);
            let n = int(at + 8)?;
            if !(1..=4096).contains(&n) {
                return None;
            }
            at += 12 + n as usize * [12, 16, 24, 24][int(at + 4)?.clamp(0, 3) as usize] + 20;
        }
    }
    (at <= state.len()).then_some(heads)
}

/// Read the curve shaper's drawn line into a table over the input range; appends the two-sided switch and the
/// table to `p`.
fn curve_table(state: &[u8], p: &mut Vec<f32>) -> Option<()> {
    p.push(if *state.get(16)? != 0 { 1.0 } else { 0.0 });
    drawn_table(state, 22, p).map(|_| ())
}

/// Read one drawn line whose header (kind, layout, point count) sits at `head` into a table appended to `p`.
/// After the header come the points (three layouts by age) and a 20-byte tail; returns the offset behind it.
fn drawn_table(state: &[u8], head: usize, p: &mut Vec<f32>) -> Option<usize> {
    drawn_table_n(state, head, CURVE_N, p)
}

/// The same with a table of `count` entries.
pub(crate) fn drawn_table_n(state: &[u8], head: usize, count: usize, p: &mut Vec<f32>) -> Option<usize> {
    drawn_table_side(state, head, count, false, p)
}

/// The same, choosing which side of a jump an entry standing right on it takes: the far side when `after`.
fn drawn_table_side(state: &[u8], head: usize, count: usize, after: bool, p: &mut Vec<f32>) -> Option<usize> {
    let int = |o: usize| state.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if !(1..=3).contains(&int(head)?) {
        return None;
    }
    let sub = int(head + 4)?;
    let n = int(head + 8)? as usize;
    let first = head + 12;
    let size = match sub {
        0 => 12,
        1 => 16,
        _ => 24,
    };
    if n == 0 || n > 4096 || state.len() < first + n * size {
        return None;
    }
    // (x, y, tension, segment type); x is stored as the distance from the point before
    let mut pts = Vec::with_capacity(n);
    let mut x = 0f32;
    for k in 0..n {
        let b = &state[first + k * size..first + (k + 1) * size];
        let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let d = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap()) as f32;
        let (dx, y, t, kind) = match size {
            12 => (f(0), f(4), f(8), 0),
            16 => (f(0), f(4), f(8), b[12]),
            _ => (d(0), d(8), f(16), b[20]),
        };
        let ok = |v: f32| if v.is_finite() { v } else { 0.0 };
        x += ok(dx).max(0.0);
        pts.push((x, ok(y).clamp(0.0, 1.0), ok(t), kind));
    }
    let span = x.max(1e-6);
    let mut seg = 1;
    for i in 0..count {
        let at = i as f32 / (count - 1) as f32 * span;
        while seg + 1 < n && (at > pts[seg].0 || (after && at >= pts[seg].0)) {
            seg += 1;
        }
        let v = if n == 1 {
            pts[0].1
        } else {
            let (a, b) = (pts[seg - 1], pts[seg]);
            let w = b.0 - a.0;
            let t = if w > 1e-9 { ((at - a.0) / w).clamp(0.0, 1.0) } else { 1.0 };
            let s = crate::song::line_shape(b.3, t, b.2);
            a.1 + (b.1 - a.1) * s
        };
        p.push(v);
    }
    Some(first + n * size + 20)
}

/// Entries in one of the beat juggler's tables: 128 to the beat over its four beats.
const WARP_N: usize = 513;
/// Slots of each sort (time first, then volume).
const WARP_SLOTS: usize = 36;

/// Where the beat juggler's 72 drawn lines start. The layout around them changed with the versions (newer
/// blocks carry slot names), so the lines are found by their own shape.
fn warp_lines(state: &[u8]) -> Option<Vec<usize>> {
    let int = |o: usize| state.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut heads = Vec::with_capacity(2 * WARP_SLOTS);
    let mut at = 44;
    while at + 12 <= state.len() && heads.len() < 2 * WARP_SLOTS {
        let fits = (|| {
            if int(at)? != 3 || !matches!(int(at + 4)?, 2 | 3) {
                return None;
            }
            let n = int(at + 8)? as usize;
            let end = at + 12 + n.checked_mul(24)? + 20;
            if n == 0 || n > 4096 || end > state.len() {
                return None;
            }
            for k in 0..n {
                let b = &state[at + 12 + k * 24..];
                let x = f64::from_le_bytes(b[0..8].try_into().ok()?);
                let y = f64::from_le_bytes(b[8..16].try_into().ok()?);
                if !(0.0..1e4).contains(&x) || !(-0.01..=1.01).contains(&y) {
                    return None;
                }
            }
            Some(end)
        })();
        match fits {
            Some(end) => {
                heads.push(at);
                at = end;
            }
            None => at += 1,
        }
    }
    (heads.len() == 2 * WARP_SLOTS).then_some(heads)
}

/// Bands and curves of the morphing equaliser, and where its curve values sit behind the listed settings.
const MEQ_BANDS: usize = 31;
const MEQ_HEAD: usize = 8;
/// Filter memory: eight values a band, the two rate changers of each side, and the last input of each side.
const MEQ_STATE: usize = MEQ_BANDS * 8 + 48 + 2;
/// The steep half-band pair used when the whole unit runs at twice the rate (fitted to renders).
const MEQ_HALF: [f32; 6] = [0.039_151_598, 0.147_377_114, 0.302_646_848, 0.482_468_543, 0.674_615_919, 0.883_005_026];

fn meq_fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(4143);
    v.extend_from_slice(&4i32.to_le_bytes());
    for _ in 0..MEQ_BANDS {
        for x in [1.0f32, 0.0, 1.0, 0.0, 1.0, 0.0] {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }
    for _ in 0..9 * MEQ_BANDS * 3 {
        v.extend_from_slice(&1.0f32.to_le_bytes());
    }
    v.extend_from_slice(&0.0f32.to_le_bytes());
    for x in [128i32, 0, 170, 128, 0, 150, -1, 0, 0] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0]);
    v
}

/// Level and pan of every band of the nine curves, appended to the settings.
fn meq_tables(state: &[u8], p: &mut Vec<f32>) -> Option<()> {
    for bank in 0..9 {
        for band in 0..MEQ_BANDS {
            for k in 0..2 {
                let o = 4 + (186 + bank * 93 + band * 3 + k) * 4;
                let b = state.get(o..o + 4)?;
                let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                p.push(if v.is_finite() { v.clamp(0.0, 2.0) } else { 1.0 });
            }
        }
    }
    Some(())
}

/// The ready-made lines a new beat juggler starts with, all our own: (beat, height, segment type) points over
/// the four beats of the bar. For time, the top is now and the bottom two bars back; for volume, the top is full.
/// Type 0 runs straight to its point, type 2 waits and jumps there.
fn warp_shape(slot: usize, volume: bool) -> Vec<(f64, f64, u8)> {
    // how far back, in beats, as a height
    let back = |beats: f64| 1.0 - beats / 8.0;
    let mut v = vec![(0.0, 1.0, 0u8)];
    // the same figure again and again: `len` beats long, from `a` to `b`, straight or as a held step
    let every = |v: &mut Vec<(f64, f64, u8)>, len: f64, a: f64, b: f64| {
        v.clear();
        let mut x = 0.0;
        while x < 4.0 - 1e-9 {
            v.push((x, a, 0));
            v.push((x + len, b, 0));
            x += len;
        }
    };
    // a gate: open for the first `share` of every `len` beats
    let gate = |v: &mut Vec<(f64, f64, u8)>, len: f64, share: f64, first_open: bool| {
        v.clear();
        let (on, off) = if first_open { (1.0, 0.0) } else { (0.0, 1.0) };
        v.push((0.0, on, 0));
        let mut x = 0.0;
        while x < 4.0 - 1e-9 {
            v.push((x + len * share, off, 2));
            v.push((x + len, on, 2));
            x += len;
        }
    };
    if !volume {
        match slot {
            // half speed: over the bar, over half a bar, over a beat
            1 => every(&mut v, 4.0, 1.0, back(2.0)),
            2 => every(&mut v, 2.0, 1.0, back(1.0)),
            3 => every(&mut v, 1.0, 1.0, back(0.5)),
            // the first beat, half beat or quarter beat said again and again
            4 | 5 | 6 => {
                let len = [1.0, 0.5, 0.25][slot - 4];
                let mut k = 1.0;
                while k * len < 4.0 - 1e-9 {
                    v.push((k * len, back(k * len), 2));
                    k += 1.0;
                }
                v.push((4.0, back((k - 1.0) * len), 0));
            }
            // the first half of the bar twice
            7 => v.extend([(2.0, back(2.0), 2), (4.0, back(2.0), 0)]),
            // the third beat's last quarter beats said again to fill the fourth
            8 => v.extend([(3.0, 1.0, 0), (3.25, back(0.25), 2), (3.5, back(0.5), 2), (3.75, back(0.75), 2), (4.0, back(0.75), 0)]),
            // the record slowing to a stop over the bar, and over the last beat
            9 => v.extend((1..=16).map(|k| (k as f64 / 4.0, back((k as f64 / 4.0).powi(2) / 8.0), 0))),
            10 => {
                v.push((3.0, 1.0, 0));
                v.extend((1..=8).map(|k| (3.0 + k as f64 / 8.0, back((k as f64 / 8.0).powi(2) / 2.0), 0)));
            }
            // backwards: the whole bar, then beat by beat, then half beat by half beat
            11 => every(&mut v, 4.0, 1.0, back(8.0)),
            12 => every(&mut v, 1.0, 1.0, back(2.0)),
            13 => every(&mut v, 0.5, 1.0, back(1.0)),
            // forwards for three beats, the last one backwards
            14 => v.extend([(3.0, 1.0, 0), (4.0, back(2.0), 0)]),
            // a wobble: the hand rocking the record once a beat
            15 => {
                v.clear();
                v.extend((0..=32).map(|k| (k as f64 / 8.0, back(0.25 - 0.25 * (k as f64 / 8.0 * std::f64::consts::TAU).cos()), 0)));
            }
            _ => v.push((4.0, 1.0, 0)),
        }
    } else {
        match slot {
            // gates: by the beat, the half beat, the quarter beat, and the off-beat alone
            1 => gate(&mut v, 1.0, 0.5, true),
            2 => gate(&mut v, 0.5, 0.5, true),
            3 => gate(&mut v, 0.25, 0.5, true),
            4 => gate(&mut v, 1.0, 0.5, false),
            5 => gate(&mut v, 0.5, 0.5, false),
            // short stabs
            6 => gate(&mut v, 1.0, 0.25, true),
            7 => gate(&mut v, 0.5, 0.25, true),
            // the pump: down on the beat and back up, by the beat and by the half beat
            8 => every(&mut v, 1.0, 0.0, 1.0),
            9 => every(&mut v, 0.5, 0.0, 1.0),
            // fading away within each beat
            10 => every(&mut v, 1.0, 1.0, 0.0),
            // in and out over the bar
            11 => v = vec![(0.0, 0.0, 0), (4.0, 1.0, 0)],
            12 => v.push((4.0, 0.0, 0)),
            // the last beat left out, the first half left out
            13 => v.extend([(3.0, 0.0, 2), (4.0, 0.0, 0)]),
            14 => v = vec![(0.0, 0.0, 0), (2.0, 1.0, 2), (4.0, 1.0, 0)],
            // a pattern of sixteen steps
            15 => {
                v.clear();
                let steps = [1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 0];
                v.push((0.0, 1.0, 0));
                for (k, on) in steps.iter().enumerate().skip(1) {
                    v.push((k as f64 / 4.0, *on as f64, 2));
                }
                v.push((4.0, 0.0, 0));
            }
            _ => v.push((4.0, 1.0, 0)),
        }
    }
    v
}

/// A fresh beat juggler block: the first slot of each kind is level (no time shift, full volume), the next
/// fifteen hold our ready-made lines, the rest are level.
fn warp_fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(13032);
    for x in [4i32, 0, 0, 256, 256, 1765, 5000, 0, 0, 65536, 0, 0x0101_0000] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    for k in 0..2 * WARP_SLOTS {
        let pts = warp_shape(k % WARP_SLOTS, k >= WARP_SLOTS);
        for x in [1280i32, 1280, 0, 0, 3, 2, pts.len() as i32] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        let mut last = 0.0;
        for (x, y, kind) in pts {
            // a point is kept as its distance from the one before
            v.extend_from_slice(&(x - last).max(0.0).to_le_bytes());
            v.extend_from_slice(&y.clamp(0.0, 1.0).to_le_bytes());
            v.extend_from_slice(&[0, 0, 0, 0, kind, 0, 0, 0]);
            last = x;
        }
        for x in [0i32, -1, -1, -1, -1] {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }
    v
}

/// The band split knob: 30 Hz to 16 kHz over its travel.
fn split_hz(v: f32) -> f32 {
    30.0 * (16000f32 / 30.0).powf((v / 65536.0).clamp(0.0, 1.0))
}

/// Where the maximiser's first drawn line starts: the eight lines (four bands, two states) are the last thing
/// in the block, so the start is the one offset from which eight lines end exactly at the end.
fn maxi_lines(state: &[u8]) -> Option<usize> {
    let int = |o: usize| state.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    'find: for s in 470..600usize.min(state.len()) {
        let mut at = s;
        for _ in 0..8 {
            let (Some(3), Some(sub), Some(n)) = (int(at), int(at + 4), int(at + 8)) else { continue 'find };
            if !(0..=3).contains(&sub) || !(1..=4096).contains(&n) {
                continue 'find;
            }
            at += 12 + n as usize * [12, 16, 24, 24][sub as usize] + 20;
        }
        if at == state.len() {
            return Some(s);
        }
    }
    None
}

/// The maximiser's switches we act on, as places among its switch bytes: twice the rate, linear-phase split.
const MAXI_SWITCHES: [usize; 2] = [12, 21];

/// How far along a drawn line's last point is.
fn line_span(state: &[u8], head: usize) -> Option<f32> {
    let int = |o: usize| state.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let size = match int(head + 4)? {
        0 => 12,
        1 => 16,
        _ => 24,
    };
    let n = int(head + 8)? as usize;
    let mut x = 0f32;
    for k in 0..n.min(4096) {
        let b = state.get(head + 12 + k * size..head + 12 + (k + 1) * size)?;
        let dx = if size == 24 { f64::from_le_bytes(b[0..8].try_into().unwrap()) as f32 } else { f32::from_le_bytes([b[0], b[1], b[2], b[3]]) };
        if dx.is_finite() {
            x += dx.max(0.0);
        }
    }
    Some(x.max(1e-6))
}

/// Where a maximiser setting sits in a block of the given version: the oldest layout has no low cut.
fn maxi_off(version: u32, off: usize) -> Option<usize> {
    match (version, off) {
        (2, 28) => None,
        (2, o) if o > 28 => Some(o - 4),
        (_, o) => Some(o),
    }
}

/// A fresh maximiser block: default settings in both states and eight lines that follow the input up to full
/// level and hold it there.
fn maxi_fresh() -> Vec<u8> {
    let d = def(Kind::Maxi);
    let mut v = Vec::with_capacity(d.size);
    v.extend_from_slice(&0xa0003u32.to_le_bytes());
    for _ in 0..2 {
        for q in d.params {
            v.extend_from_slice(&q.def.to_le_bytes());
        }
    }
    v.extend_from_slice(&50i32.to_le_bytes());
    v.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    for _ in 0..8 {
        for x in [3i32, 3, 3] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        for (k, (x, y)) in [(0f64, 0f64), (0.5, 0.5), (0.5, 0.5)].into_iter().enumerate() {
            v.extend_from_slice(&x.to_le_bytes());
            v.extend_from_slice(&y.to_le_bytes());
            v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, if k == 0 { 0 } else { 2 }]);
        }
        for x in [0i32, -1, -1, -1, -1] {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }
    v
}

pub fn def(kind: Kind) -> &'static Def {
    DEFS.iter().find(|d| d.kind == kind).expect("every kind has a table")
}

/// Our title for an effect identifier from a project file, when we have a unit for it.
pub fn title_for(ident: &str) -> Option<&'static str> {
    DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(ident)).map(|d| d.title)
}

/// An effect's settings, in file units, in the order of its table.
#[derive(Debug, Clone, PartialEq)]
pub struct Fx {
    pub kind: Kind,
    pub p: Vec<f32>,
    /// The impulse of a convolution unit.
    pub ir: Option<Ir>,
    /// The units and cables held by a patch bay.
    pub sub: Option<std::sync::Arc<crate::fxpatch::Patch>>,
    /// The whole block of a unit with drawn lines, as read or as last edited; the lines live here, their tables in `p`.
    pub raw: Option<std::sync::Arc<Vec<u8>>>,
    /// The live plug-in of a hosted unit. Copies of the song share it.
    pub ext: Option<crate::host::Hosted>,
}

/// Stand-in offset for settings that have no fixed place in their block.
const CONV_AFTER: usize = 1 << 30;

/// An impulse and the stored path it came from (empty when the block carried the sound itself).
#[derive(Debug, Clone)]
pub struct Ir {
    pub smp: std::sync::Arc<crate::sample::Sample>,
    pub path: String,
}

impl PartialEq for Ir {
    fn eq(&self, o: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.smp, &o.smp) && self.path == o.path
    }
}

/// The parts of a convolution block after its fixed front: the path, the sound held inside (if any), and where
/// the rest begins.
fn conv_parts(state: &[u8]) -> Option<(String, Option<std::ops::Range<usize>>, std::ops::Range<usize>, usize)> {
    let held = *state.get(20)? == 0;
    let mut len = *state.get(21)? as usize;
    let mut at = 22;
    if len == 0xff {
        len = u32::from_le_bytes(state.get(22..26)?.try_into().ok()?) as usize;
        at = 26;
    }
    let path = String::from_utf8_lossy(state.get(at..at + len)?).to_string();
    let path_range = 21..at + len;
    at += len;
    let mut wave = None;
    if held {
        if let Some(b) = state.get(at..at + 4) {
            let size = u32::from_le_bytes(b.try_into().ok()?) as usize;
            if size > 0 && at + 4 + size <= state.len() {
                wave = Some(at + 4..at + 4 + size);
                at += 4 + size;
            }
        }
    }
    Some((path, wave, path_range, at))
}

/// The sample file a stored effect block names, for units that play one.
pub fn sample_named_in(ident: &str, state: &[u8]) -> Option<String> {
    if ident.eq_ignore_ascii_case("Distructor") {
        return crate::fxrack::cabinet_named_in(state);
    }
    if ident.eq_ignore_ascii_case("Fruity Scratcher") {
        // fourteen settings, then the length of the path and the path
        let n = u32::from_le_bytes(state.get(60..64)?.try_into().ok()?) as usize;
        let raw = state.get(64..64 + n)?;
        return Some(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string()).filter(|s| !s.is_empty());
    }
    if !ident.eq_ignore_ascii_case("Fruity Convolver") {
        return None;
    }
    conv_parts(state).map(|x| x.0).filter(|s| !s.is_empty())
}

impl Fx {
    /// How many frames late this unit's sound comes out; the mixer lines everything else up with it.
    pub fn latency(&self, sr: f32) -> usize {
        match self.kind {
            Kind::Comp => 64,
            Kind::MultiComp if self.p.get(1).copied().unwrap_or(0.0) >= 0.5 => 52,
            Kind::Maxi => crate::fxmaxi::latency(&self.p, sr),
            Kind::Loud => match loud_settings() {
                Some(set) => crate::fxmaxi::latency(&set[(self.p[0] as usize).min(3)], sr),
                None => 0,
            },
            Kind::Limiter => {
                let t = 2.0 * (501f32.powf(self.p.get(3).copied().unwrap_or(0.0) / 10000.0) - 1.0) * 0.001 * sr;
                (t.round().max(0.0) as usize).min(sr as usize)
            }
            _ => 0,
        }
    }
}

impl Fx {
    pub fn new(kind: Kind) -> Self {
        if kind == Kind::Curve {
            return Self::decode(def(kind).ident, &curve_fresh()).expect("our own block reads back");
        }
        if kind == Kind::Maxi {
            return Self::decode(def(kind).ident, &maxi_fresh()).expect("our own block reads back");
        }
        if kind == Kind::Warp {
            return Self::decode(def(kind).ident, &warp_fresh()).expect("our own block reads back");
        }
        if kind == Kind::Xyz {
            return Self::decode(def(kind).ident, &xyz_fresh()).expect("our own block reads back");
        }
        if kind == Kind::MorphEq {
            return Self::decode(def(kind).ident, &meq_fresh()).expect("our own block reads back");
        }
        if kind == Kind::Surface {
            return Self::decode(def(kind).ident, &crate::surface::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Patch {
            return Self::decode(def(kind).ident, &crate::fxpatch::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Rack {
            return Self::decode(def(kind).ident, &crate::fxrack::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Voc {
            return Self::decode(def(kind).ident, &crate::fxvoc::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Multi {
            return Self::decode(def(kind).ident, &crate::fxmulti::fresh()).expect("our own block reads back");
        }
        if kind == Kind::OldVoc {
            return Self::decode(def(kind).ident, &crate::fxvoc::classic::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Bank {
            return Self::decode(def(kind).ident, &crate::fxbank::fresh()).expect("our own block reads back");
        }
        Self { kind, p: def(kind).params.iter().map(|p| p.def as f32).collect(), ir: None, sub: None, raw: None, ext: None }
    }

    /// Put setting `k` at `v` (0..=1) of its travel, as a controller does. Settings past the unit's listed
    /// ones (drawn tables) are left alone.
    pub fn set_travel(&mut self, k: usize, v: f32) {
        if let Some(h) = &self.ext {
            return h.0.set_travel(k, v);
        }
        if self.kind == Kind::Surface {
            // every control's position is a setting, and they all run from nought to one
            if let Some(slot) = self.p.get_mut(k) {
                *slot = v.clamp(0.0, 1.0);
            }
            return;
        }
        if let (Some(q), Some(slot)) = (def(self.kind).params.get(k), self.p.get_mut(k)) {
            *slot = q.min as f32 + (q.max as f32 - q.min as f32) * v.clamp(0.0, 1.0);
        }
    }

    /// Read the stored block of the effect called `ident`. None when we have no unit for it or the block is
    /// too short to hold its settings.
    pub fn decode(ident: &str, state: &[u8]) -> Option<Self> {
        if ident.eq_ignore_ascii_case("Fruity Wrapper") {
            if wrapped(state).is_none() {
                // not one of the old stock units: a plug-in from another maker, when it is installed here
                let inst = crate::host::from_block(state)?;
                return Some(Self { ext: Some(crate::host::Hosted(inst)), ..Self::new(Kind::Hosted) });
            }
            let (d, at) = wrapped(state)?;
            let p = d.params.iter().enumerate().map(|(i, q)| wrapped_value(q, &state[at + i * 4..])).collect();
            return Some(Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        let d = DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(ident))?;
        if old_floats(d, state) {
            let p = d.params.iter().enumerate().map(|(i, q)| wrapped_value(q, &state[i * 4..])).collect();
            return Some(Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::Notes {
            return crate::notes::decode(ident, state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::Surface {
            return crate::surface::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: Some(std::sync::Arc::new(state.to_vec())), ext: None });
        }
        if d.kind == Kind::Formula {
            return formula_decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::Bank {
            return crate::fxbank::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: Some(std::sync::Arc::new(state.to_vec())), ext: None });
        }
        if d.kind == Kind::Patch {
            return crate::fxpatch::decode(state).map(|p| Self { kind: d.kind, p: vec![], ir: None, sub: Some(std::sync::Arc::new(p)), raw: None, ext: None });
        }
        if d.kind == Kind::Gm {
            return crate::gm::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::Rack {
            return crate::fxrack::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::Multi {
            return crate::fxmulti::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::Voc {
            return crate::fxvoc::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        if d.kind == Kind::OldVoc {
            return crate::fxvoc::classic::decode(state).map(|p| Self { kind: d.kind, p, ir: None, sub: None, raw: None, ext: None });
        }
        let need = d.params.iter().map(|p| p.off + 4).max().unwrap_or(0);
        // the oldest reverb blocks (found in presets) have no version number in front
        let lead;
        let state = if d.kind == Kind::Reverb && state.len() == 53 {
            lead = [&10000i32.to_le_bytes()[..], state].concat();
            &lead[..]
        } else {
            state
        };
        // older, shorter blocks are accepted as long as every setting they had is there; the rest read as default
        let least = match d.kind {
            Kind::Comp => 28,
            Kind::Reverb => 56,
            Kind::Plush => 165,
            Kind::Conv => 22,
            Kind::Limiter => 76,
            Kind::MorphEq => 4132,
            _ => need,
        };
        if state.len() < least {
            return None;
        }
        if d.kind == Kind::Maxi {
            let version = u32::from_le_bytes([state[0], state[1], state[2], state[3]]);
            let mut p: Vec<f32> = d
                .params
                .iter()
                .map(|q| match maxi_off(version, q.off).and_then(|o| state.get(o..o + 4)) {
                    Some(b) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                    None => -1.0,
                })
                .collect();
            // the lines come band by band, each band's two states together; the first state is the one in use
            let first = maxi_lines(state)?;
            let mut at = first;
            let mut spans = [1f32; 4];
            let mut spare = vec![];
            for k in 0..8 {
                if k % 2 == 0 {
                    spans[k / 2] = line_span(state, at).unwrap_or(1.0);
                    at = drawn_table(state, at, &mut p)?;
                } else {
                    spare.clear();
                    at = drawn_table(state, at, &mut spare)?;
                }
            }
            p.extend_from_slice(&spans);
            // the switches sit between the settings and the lines; older blocks stop short of some
            let flags = 4 + (if version == 2 { 58 } else { 59 }) * 8 + 4 + 8;
            for i in MAXI_SWITCHES {
                p.push(if flags + i < first && state[flags + i] != 0 { 1.0 } else { 0.0 });
            }
            return Some(Self { kind: d.kind, p, ir: None, sub: None, raw: Some(std::sync::Arc::new(state.to_vec())), ext: None });
        }
        let mut p: Vec<f32> = d
            .params
            .iter()
            .map(|p| match state.get(p.off..p.off + 4) {
                Some(b) if floats(d.kind, p.off) => (f32::from_le_bytes([b[0], b[1], b[2], b[3]]) * 10000.0).round(),
                Some(b) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                None => p.def as f32,
            })
            .collect();
        if d.kind == Kind::Peak {
            // the phase is 16 bits wide; the switch bytes follow straight after it
            p[9] = u16::from_le_bytes([state[36], state[37]]) as f32;
        }
        if d.kind == Kind::Curve {
            curve_table(state, &mut p)?;
        }
        if d.kind == Kind::MorphEq {
            for (i, o) in [(6, 4137), (7, 4136)] {
                p[i] = state.get(o).map_or(0.0, |&b| (b != 0) as u8 as f32);
            }
            meq_tables(state, &mut p)?;
        }
        if d.kind == Kind::Warp {
            for head in warp_lines(state)? {
                drawn_table_side(state, head, WARP_N, true, &mut p)?;
            }
        }
        let mut ir = None;
        let mut conv_tail = None;
        if d.kind == Kind::Conv {
            if let Some((_, Some(w), _, _)) = conv_parts(state) {
                ir = crate::sample::Sample::decode(state[w].to_vec(), "wav").ok().map(|s| Ir { smp: std::sync::Arc::new(s), path: String::new() });
            }
            if let Some((t, tail)) = conv_tail_of(state) {
                p[4..].copy_from_slice(&t.read(tail));
                conv_tail = Some(std::sync::Arc::new(state.to_vec()));
            }
        }
        if d.kind == Kind::Xyz {
            for head in xyz_lines(state)? {
                drawn_table_n(state, head, XYZ_N, &mut p)?;
            }
        }
        let raw = matches!(d.kind, Kind::Curve | Kind::Warp | Kind::Xyz).then(|| std::sync::Arc::new(state.to_vec())).or(conv_tail);
        Some(Self { kind: d.kind, p, ir, sub: None, raw, ext: None })
    }

    /// Write the settings into a stored block: `base` when the effect came from a file (everything we do not
    /// model is kept), a fresh block otherwise.
    pub fn encode(&self, base: Option<&[u8]>) -> Vec<u8> {
        if let Some(h) = &self.ext {
            return crate::host::block(&h.0, base);
        }
        if let Some((d, at)) = base.and_then(wrapped) {
            if d.kind == self.kind {
                // settings go back into the wrapper's own block; untouched ones keep their exact bytes
                let mut out = base.unwrap().to_vec();
                for (i, q) in d.params.iter().enumerate() {
                    let o = at + i * 4;
                    if wrapped_value(q, &out[o..]) != self.p[i] && q.max != q.min {
                        let x = (self.p[i] - q.min as f32) / (q.max - q.min) as f32;
                        out[o..o + 4].copy_from_slice(&x.clamp(0.0, 1.0).to_le_bytes());
                    }
                }
                return out;
            }
        }
        if let Some(old) = base.filter(|b| old_floats(def(self.kind), b)) {
            // a project from the early versions keeps its block of plain travels
            let mut out = old.to_vec();
            for (i, q) in def(self.kind).params.iter().enumerate() {
                let o = i * 4;
                if wrapped_value(q, &out[o..]) != self.p[i] && q.max != q.min {
                    let x = (self.p[i] - q.min as f32) / (q.max - q.min) as f32;
                    out[o..o + 4].copy_from_slice(&x.clamp(0.0, 1.0).to_le_bytes());
                }
            }
            return out;
        }
        // a unit that carries its own block (drawn lines, perhaps edited) builds on that
        let base = self.raw.as_deref().map(|v| &v[..]).or(base);
        if self.kind == Kind::Bank {
            return crate::fxbank::encode(&self.p, base);
        }
        if let (Kind::Patch, Some(sub)) = (self.kind, &self.sub) {
            return crate::fxpatch::encode(sub, base);
        }
        if self.kind == Kind::Gm {
            return crate::gm::encode(&self.p, base);
        }
        if self.kind == Kind::Rack {
            return crate::fxrack::encode(&self.p, base);
        }
        if self.kind == Kind::Surface {
            return crate::surface::encode(&self.p, self.raw.as_ref().map(|r| r.as_slice()).or(base));
        }
        if self.kind == Kind::Formula {
            return formula_encode(&self.p, base);
        }
        if self.kind == Kind::Notes {
            return crate::notes::encode(&self.p, base);
        }
        if self.kind == Kind::Multi {
            return crate::fxmulti::encode(&self.p, base);
        }
        if self.kind == Kind::Voc {
            return crate::fxvoc::encode(&self.p, base);
        }
        if self.kind == Kind::OldVoc {
            return crate::fxvoc::classic::encode(&self.p);
        }
        let d = def(self.kind);
        let mut out = match base {
            Some(b) if self.kind == Kind::Conv => {
                // a newly chosen impulse replaces the stored path and any sound held inside
                match (&self.ir, conv_parts(b)) {
                    (Some(ir), Some((old, _, _, rest))) if !ir.path.is_empty() && ir.path != old => {
                        let mut v = b[..20].to_vec();
                        v.push(1);
                        conv_path(&mut v, &ir.path);
                        v.extend_from_slice(&b[rest..]);
                        v
                    }
                    // a block cut off before the impulse gets the rest
                    _ if b.len() == 20 => [b, &[1, 0][..], &crate::convset::fresh(0.0)].concat(),
                    _ => b.to_vec(),
                }
            }
            Some(b) => b.to_vec(),
            None if self.kind == Kind::Conv => {
                let mut v = vec![0u8; 20];
                v[0] = 14;
                v.push(1);
                let ir = self.ir.as_ref().filter(|i| !i.path.is_empty());
                conv_path(&mut v, ir.map_or("", |i| &i.path));
                v.extend_from_slice(&crate::convset::fresh(ir.map_or(0.0, |i| i.smp.seconds() as f64)));
                v
            }
            None if self.kind == Kind::Curve => curve_fresh(),
            None if self.kind == Kind::Maxi => maxi_fresh(),
            None if self.kind == Kind::Xy => XY_FRESH.to_vec(),
            None if self.kind == Kind::Xyz => xyz_fresh(),
            None if self.kind == Kind::Warp => warp_fresh(),
            None if self.kind == Kind::MorphEq => meq_fresh(),
            None if self.kind == Kind::Eq => eq_fresh(d),
            None => {
                let mut v = vec![0u8; d.size];
                for &(o, x) in d.fixed {
                    if o + 4 <= v.len() {
                        v[o..o + 4].copy_from_slice(&x.to_le_bytes());
                    }
                }
                v
            }
        };
        let version = if self.kind == Kind::Maxi && out.len() >= 4 { u32::from_le_bytes([out[0], out[1], out[2], out[3]]) } else { 0 };
        for (p, v) in d.params.iter().zip(&self.p) {
            let Some(off) = (if self.kind == Kind::Maxi { maxi_off(version, p.off) } else { Some(p.off) }) else { continue };
            if self.kind == Kind::MorphEq && off >= 4136 {
                if off < out.len() {
                    out[off] = (*v >= 0.5) as u8;
                }
                continue;
            }
            if off + 4 <= out.len() {
                if floats(self.kind, p.off) {
                    // leave a stored value alone unless the setting moved off it
                    let old = f32::from_le_bytes([out[off], out[off + 1], out[off + 2], out[off + 3]]);
                    if (old * 10000.0).round() != v.round() {
                        out[off..off + 4].copy_from_slice(&(v / 10000.0).to_le_bytes());
                    }
                } else {
                    out[off..off + 4].copy_from_slice(&(v.round() as i32).to_le_bytes());
                }
            }
        }
        if self.kind == Kind::MorphEq && self.p.len() >= MEQ_HEAD + 9 * MEQ_BANDS * 2 {
            // the curves go back where they came from; a value that did not move keeps its stored bytes
            for bank in 0..9 {
                for band in 0..MEQ_BANDS {
                    for k in 0..2 {
                        let o = 4 + (186 + bank * 93 + band * 3 + k) * 4;
                        let v = self.p[MEQ_HEAD + bank * 62 + band * 2 + k];
                        if o + 4 <= out.len() {
                            let old = f32::from_le_bytes([out[o], out[o + 1], out[o + 2], out[o + 3]]);
                            if !(old.is_finite() && old.clamp(0.0, 2.0) == v) {
                                out[o..o + 4].copy_from_slice(&v.to_le_bytes());
                            }
                        }
                    }
                }
            }
        }
        if self.kind == Kind::Deck && out.len() >= 64 {
            // a newly chosen record replaces the stored path
            if let Some(ir) = self.ir.as_ref().filter(|i| !i.path.is_empty()) {
                let old_n = (u32::from_le_bytes([out[60], out[61], out[62], out[63]]) as usize).min(out.len() - 64);
                if String::from_utf8_lossy(&out[64..64 + old_n]).trim_end_matches('\0') != ir.path {
                    let new = [&(ir.path.len() as u32).to_le_bytes()[..], ir.path.as_bytes()].concat();
                    out.splice(60..64 + old_n, new);
                }
            }
        }
        if self.kind == Kind::Conv && self.p.len() >= 4 + crate::convset::EXTRA {
            if let Some((t, at)) = conv_tail_of(&out).map(|(t, tail)| (t, out.len() - tail.len())) {
                t.write(&mut out[at..], &self.p[4..]);
            }
        }
        out
    }
}

/// The part of an impulse reverb block after the impulse, when it is laid out as we know it.
fn conv_tail_of(state: &[u8]) -> Option<(crate::convset::Tail, &[u8])> {
    let (_, _, _, rest) = conv_parts(state)?;
    let tail = &state[rest..];
    let version = u32::from_le_bytes(state.get(..4)?.try_into().ok()?);
    crate::convset::parse(tail, version).map(|t| (t, tail))
}

fn conv_path(v: &mut Vec<u8>, path: &str) {
    let b = path.as_bytes();
    if b.len() < 0xff {
        v.push(b.len() as u8);
    } else {
        v.push(0xff);
        v.extend_from_slice(&(b.len() as u32).to_le_bytes());
    }
    v.extend_from_slice(b);
}

/// A new pad controller's block: both axes centred, the plain grid.
const XY_FRESH: [u8; 71] = [1, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 4, 0, 0, 0, 4, 70, 108, 97, 116, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 70, 0, 0, 0];

/// A block of the host wrapper that holds one of the old plug-in-format builds of a stock effect: the unit
/// we have for it, and where its settings (floats from 0 to 1, one per setting) begin.
fn wrapped(state: &[u8]) -> Option<(&'static Def, usize)> {
    let mut at = 4;
    let (mut name, mut body) = (None, None);
    while at + 12 <= state.len() {
        let id = u32::from_le_bytes(state[at..at + 4].try_into().ok()?);
        let size = u64::from_le_bytes(state[at + 4..at + 12].try_into().ok()?) as usize;
        at += 12;
        let data = state.get(at..at.checked_add(size)?)?;
        match id {
            54 => name = Some(String::from_utf8_lossy(data).trim_end_matches('\0').to_string()),
            53 => body = Some(at),
            _ => {}
        }
        at += size;
    }
    let (name, body) = (name?, body?);
    let d = DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(&name))?;
    if state.get(body..body + 4)? != [0xf7, 0xff, 0xff, 0xff] {
        return None;
    }
    let long = state.get(body + 5..body + 9)? == [0xfe, 0xff, 0xff, 0xff];
    let at = body + if long { 21 } else { 13 };
    let n = u32::from_le_bytes(state.get(at..at + 4)?.try_into().ok()?) as usize;
    if d.params.is_empty() || n != d.params.len() || state.len() < at + 4 + n * 4 {
        return None;
    }
    Some((d, at + 4))
}

/// The early versions stored the stock effects as nothing but each knob's travel, 0 to 1, in the unit's own
/// order: as many numbers as the unit has settings and no version number in front.
fn old_floats(d: &Def, state: &[u8]) -> bool {
    let n = d.params.len();
    n > 0
        && state.len() == n * 4
        && state.len() != d.size
        && state.chunks_exact(4).all(|b| {
            let x = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            (0.0..=1.0).contains(&x)
        })
        // a current block starts with its version number; a travel that small would be a denormal
        && !d.fixed.iter().any(|&(at, v)| at == 0 && state[..4] == (v as u32).to_le_bytes())
}

/// A new seven band block: version, the settings, a second bank holding the same, then the view options.
fn eq_fresh(d: &Def) -> Vec<u8> {
    let mut v = vec![0u8; 305];
    v[..4].copy_from_slice(&2i32.to_le_bytes());
    for q in d.params {
        let x = (q.def as i32).to_le_bytes();
        v[q.off..q.off + 4].copy_from_slice(&x);
        if q.off < 148 {
            v[q.off + 144..q.off + 148].copy_from_slice(&x);
        }
    }
    v[292] = 1;
    v[300] = 1;
    v[301] = 2;
    v
}

fn wrapped_value(q: &Param, raw: &[u8]) -> f32 {
    let x = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    (q.min as f32 + x * (q.max - q.min) as f32).round()
}

/// The formula sender's block: version, the three knobs (65536 = full), then the formula as counted text and
/// the notes. We carry the formula's letters after the knobs: their count, then one value per letter.
fn formula_decode(state: &[u8]) -> Option<Vec<f32>> {
    let mut p: Vec<f32> = (0..3).map(|k| state.get(4 + k * 4..8 + k * 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32)).collect::<Option<_>>()?;
    let (n, at) = counted(state, 16)?;
    let text = state.get(at..at + n)?;
    p.push(n as f32);
    p.extend(text.iter().map(|&c| c as f32));
    Some(p)
}

/// The formula is short text: one byte of length, then that many letters. Gives the count and where it starts.
fn counted(b: &[u8], at: usize) -> Option<(usize, usize)> {
    b.get(at).map(|&n| (n as usize, at + 1))
}

fn formula_encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let mut out = match base {
        Some(b) if b.len() >= 16 => b[..16].to_vec(),
        _ => {
            let mut v = vec![0u8; 16];
            v[0] = 2;
            v
        }
    };
    for k in 0..3 {
        out[4 + k * 4..8 + k * 4].copy_from_slice(&(p.get(k).copied().unwrap_or(0.0).clamp(0.0, 65536.0) as u32).to_le_bytes());
    }
    let text: Vec<u8> = p.iter().skip(4).take(255).map(|&c| c as u8).collect();
    out.push(text.len() as u8);
    out.extend_from_slice(&text);
    // the notes that follow the formula stay as they were
    match base.and_then(|b| counted(b, 16).and_then(|(n, at)| b.get(at + n..))) {
        Some(rest) => out.extend_from_slice(rest),
        None => out.extend_from_slice(&[0, 0, 0, 0]),
    }
    out
}

impl Fx {
    /// Where each drawn line of this unit starts in `block`, in the order the unit uses them.
    fn line_heads(&self, block: &[u8]) -> Vec<usize> {
        match self.kind {
            Kind::Curve => vec![22],
            Kind::Warp => warp_lines(block).unwrap_or_default(),
            Kind::Xyz => xyz_lines(block).unwrap_or_default(),
            Kind::Maxi => {
                let int = |o: usize| block.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                let mut heads = vec![];
                let mut at = maxi_lines(block);
                let mut k = 0;
                while let (Some(a), true) = (at, k < 8) {
                    if k % 2 == 0 {
                        heads.push(a);
                    }
                    k += 1;
                    at = (|| Some(a + 12 + int(a + 8)? as usize * [12, 16, 24, 24][int(a + 4)?.clamp(0, 3) as usize] + 20))();
                }
                heads
            }
            _ => vec![],
        }
    }
    /// The band maximiser's switches: 0 runs it at twice the rate, 1 makes its split linear-phase.
    pub fn maxi_switch(&self, k: usize) -> bool {
        self.kind == Kind::Maxi && self.p.get(crate::fxmaxi::OVERSAMPLE + k).is_some_and(|v| *v >= 0.5)
    }
    pub fn set_maxi_switch(&mut self, k: usize, on: bool) {
        if self.kind != Kind::Maxi || k >= MAXI_SWITCHES.len() || self.p.len() < crate::fxmaxi::LEN {
            return;
        }
        self.p[crate::fxmaxi::OVERSAMPLE + k] = if on { 1.0 } else { 0.0 };
        // kept in the block as well, where the block is new enough to have the place
        if let Some(raw) = &mut self.raw {
            let version = raw.get(0..4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            let at = 4 + (if version == 2 { 58 } else { 59 }) * 8 + 4 + 8 + MAXI_SWITCHES[k];
            if maxi_lines(raw).is_some_and(|first| at < first) {
                std::sync::Arc::make_mut(raw)[at] = on as u8;
            }
        }
    }
    /// How many drawn lines this unit has that can be edited.
    pub fn line_count(&self) -> usize {
        match self.kind {
            Kind::Curve => 1,
            Kind::Maxi => 4,
            Kind::Warp => 2 * WARP_SLOTS,
            Kind::Xyz => 6,
            Kind::Bank => crate::fxbank::BANKS * crate::fxbank::LINES,
            _ => 0,
        }
    }
    /// Drawn line `k` as points. None for a line stored the old way, which has to be drawn afresh.
    pub fn line(&self, k: usize) -> Option<crate::fm::Curve> {
        let block = self.encode(None);
        if self.kind == Kind::Bank {
            return crate::fxbank::line(&block, k);
        }
        crate::fm::Curve::read_at(&block, *self.line_heads(&block).get(k)?)
    }
    /// The four numbers a filter bank keeps behind line `k` (pattern and slow-wave settings).
    pub fn line_after(&self, k: usize) -> Option<[i32; 4]> {
        (self.kind == Kind::Bank).then(|| crate::fxbank::after(&self.encode(None), k)).flatten()
    }
    pub fn set_bank_head(&mut self, o: usize, v: i32) -> bool {
        if self.kind != Kind::Bank {
            return false;
        }
        match crate::fxbank::set_head(&self.encode(None), o, v).and_then(|b| Self::decode(def(self.kind).ident, &b)) {
            Some(new) => {
                *self = new;
                true
            }
            None => false,
        }
    }
    pub fn set_line_after(&mut self, k: usize, v: [i32; 4]) -> bool {
        if self.kind != Kind::Bank {
            return false;
        }
        match crate::fxbank::set_after(&self.encode(None), k, v).and_then(|b| Self::decode(def(self.kind).ident, &b)) {
            Some(new) => {
                *self = new;
                true
            }
            None => false,
        }
    }
    /// Put an edited (or newly drawn) line in place of line `k`.
    pub fn set_line(&mut self, k: usize, c: &crate::fm::Curve) -> bool {
        let mut block = self.encode(None);
        if self.kind == Kind::Bank {
            return match crate::fxbank::set_line(&block, k, c).and_then(|b| Self::decode(def(self.kind).ident, &b)) {
                Some(new) => {
                    *self = new;
                    true
                }
                None => false,
            };
        }
        let Some(&head) = self.line_heads(&block).get(k) else { return false };
        if !crate::fm::Curve::replace_at(&mut block, head, c) {
            return false;
        }
        match Self::decode(def(self.kind).ident, &block) {
            Some(new) => {
                *self = new;
                true
            }
            None => false,
        }
    }

    /// The formula a formula sender holds.
    pub fn formula(&self) -> String {
        self.p.iter().skip(4).map(|&c| c as u8 as char).collect()
    }
    pub fn set_formula(&mut self, text: &str) {
        self.p.resize(3, 0.0);
        let letters: Vec<f32> = text.chars().filter(|c| (*c as u32) < 256 && !c.is_control()).take(255).map(|c| c as u32 as f32).collect();
        self.p.push(letters.len() as f32);
        self.p.extend(letters);
    }
}

/// Units whose block holds 32-bit floats from 0 to 1; we carry those as 0 to 10000.
fn floats(kind: Kind, off: usize) -> bool {
    kind == Kind::Pad || kind == Kind::Amp || kind == Kind::Deck || (kind == Kind::Tuner && off < 144)
}

/// How long the three axis pad's pointer takes to close most of the way in on a new place, in seconds, from the
/// global setting and the axis's own speed. Nothing at all with the global setting at zero.
pub fn xyz_time(global: f32, speed: f32) -> f32 {
    0.15 * (global / 2048.0).max(0.0) * 2048.0 / speed.max(64.0)
}

/// What a unit needs to know about the moment it is running in.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub sr: f32,
    /// Frames in one step (a sixteenth) at the current tempo.
    pub step: f32,
    /// Where the song is, for hosted plug-ins that follow it.
    pub clock: crate::host::Clock,
}

/// The loudener is the band maximiser at one of four fixed settings. Those settings are files in the user's own
/// install; they are read from there when it is known and never kept in this program. Without them the unit falls
/// back to a plain fitted three-band push.
fn loud_settings() -> Option<std::sync::Arc<[Vec<f32>; 4]>> {
    static S: std::sync::Mutex<Option<std::sync::Arc<[Vec<f32>; 4]>>> = std::sync::Mutex::new(None);
    let mut s = S.lock().unwrap();
    if s.is_none() {
        'roots: for root in crate::keys::roots() {
            for sub in ["Data/Patches", "Patches"] {
                let dir = root.join(sub).join("Plugin presets/Effects/Maximus/Soundgoodizer presets");
                let mut all: [Vec<f32>; 4] = Default::default();
                let mut ok = true;
                for (i, name) in ["a.fst", "b.fst", "c.fst", "d.fst"].iter().enumerate() {
                    match crate::preset::read(&dir.join(name)).ok().and_then(|pre| Fx::decode(&pre.ident, &pre.state)) {
                        Some(fx) if fx.kind == Kind::Maxi && fx.p.len() == crate::fxmaxi::LEN => {
                            all[i] = fx.p;
                            // this unit leaves the low band unturned
                            all[i].push(1.0);
                        }
                        _ => ok = false,
                    }
                }
                if ok {
                    *s = Some(std::sync::Arc::new(all));
                    break 'roots;
                }
            }
        }
    }
    s.clone()
}

/// The classic units run on the processor of their successor. This gives the successor's kind for one of them.
fn successor(kind: Kind) -> Option<Kind> {
    match kind {
        Kind::OldEq => Some(Kind::Eq),
        Kind::OldReverb => Some(Kind::Reverb),
        Kind::Plush => Some(Kind::Reverb),
        _ => None,
    }
}

/// Settings of a classic unit `p` put into the terms of its successor `q`.
fn translate(kind: Kind, p: &[f32], q: &mut [f32], cx: &Ctx) {
    match kind {
        Kind::OldEq => {
            q[..28].copy_from_slice(&p[..28]);
            q[28..35].fill(0.0);
            q[35] = 2000.0 * knob_amp(p[28] / 128.0).max(1e-6).log10();
            q[36] = -1.0;
        }
        Kind::OldReverb => {
            let cut = |v: f32| if v >= 65536.0 { 221.0 } else { 5.0 + v / 65536.0 * 215.0 };
            q[0] = if p[0] <= 0.0 { 19.0 } else { 20.0 + p[0] / 65536.0 * 3000.0 };
            q[1] = cut(p[1]);
            // predelay in milliseconds
            q[2] = p[2] * 44.1 / 114.8453;
            // the room takes whole steps
            q[3] = (p[3] / 655.36).round();
            q[4] = p[4] / 655.36;
            // decay and tail level as measured against its render test
            q[5] = (1.0 + 199.0 * p[6] / 65536.0).clamp(1.0, 200.0);
            q[6] = cut(p[7]);
            q[7] = 100.0 + (p[5] - 20.0) * 4.0;
            q[8] = 500.0;
            q[9] = 0.0;
            // the dry level is a plain gain
            q[10] = 128.0 * (1.0 + 10.0 * (p[8] / 65536.0).max(0.0)).ln() / 11f32.ln();
            // no separate early tap in this one
            q[11] = 0.0;
            q[12] = { let l = p[9] / 65536.0 / 0.4633; if l <= 1.0 { 128.0 * l.max(0.0).powf(2.0 / 3.0) } else { 128.0 + 64.0 * (l - 1.0) } };
            q[13] = 0.0;
            q[14] = 0.0;
        }
        Kind::Plush => {
            // gains: 746 is unity, twenty units to the dB, the bottom of the travel is silence
            let lvl = |v: f32| if v <= -600.0 { 0.0 } else { db((v - 746.0) * 0.05) };
            let wet = if p[3] < 0.5 { 0.0 } else { lvl(p[0]) * lvl(p[33]) };
            let beat_ms = cx.step * 4.0 / cx.sr * 1000.0;
            q[0] = if p[2] <= 20.0 { 19.0 } else { p[2].min(2500.0) };
            q[1] = if p[1] >= 20000.0 { 221.0 } else { (p[1] / 100.0).clamp(5.0, 220.0) };
            q[2] = if p[39] as i32 & 1 != 0 { p[6] / 192.0 * beat_ms } else { p[5] / 10.0 }.clamp(0.0, 1000.0);
            q[3] = p[9].clamp(1.0, 100.0);
            q[4] = p[8].clamp(0.0, 100.0);
            q[5] = (1000f32.powf(p[4] / 16384.0)).clamp(1.0, 200.0);
            q[6] = (5.0 * 40f32.powf(p[7] / 1000.0)).clamp(5.0, 221.0);
            q[7] = 100.0;
            q[8] = 500.0;
            q[9] = 64.0 * (1.0 - p[37] / 125.0);
            q[10] = 128.0 * lvl(p[32]);
            q[11] = 32.0 * wet;
            q[12] = 64.0 * wet;
            q[13] = p[11] / 100.0 * 2048.0;
            q[14] = p[10] / 1000.0 * 2048.0;
        }
        _ => {}
    }
}

/// How far off its own pace each flanger of the stack runs at full spread, read off click trains at every
/// order. The list is walked down the left side from the widest flanger and back up the right side from the
/// narrowest, and comes round again after twenty; two of the twenty all but stand still.
const STACK_OFF: [f32; 20] = [
    -0.064931, 0.079540, 0.042449, -0.98975, -0.045490, 0.067082, -0.006020, -0.057225, 0.125743, -0.103170, 0.062910, -0.98977, 0.129950,
    -0.091820, -0.017930, 0.100390, -0.033770, 0.104625, 0.038393, 0.142767,
];

/// One running effect: the settings plus whatever memory the processor needs.
pub struct Unit {
    pub fx: Fx,
    inner: Option<Box<Unit>>,
    z: [f32; 64],
    buf: Vec<[f32; 2]>,
    pos: usize,
    ph: f32,
    lines: Vec<Vec<f32>>,
    lpos: Vec<usize>,
    /// What a controller unit is putting out, 0..=1 each (level follower: both, peak, sweep).
    pub out: [f32; 3],
    /// Where the song stands, for a formula sender: quarter notes and tempo. Set by whoever runs the unit.
    pub song_at: (f64, f32),
    /// The formula sender's compiled formula and the letters it was made from; its clock; its dice.
    form: Option<(Vec<f32>, Option<crate::formula::Formula>)>,
    clock: f64,
    dice: u64,
    bank: Option<Box<crate::fxbank::State>>,
    conv: Option<Box<crate::conv::State>>,
    rack: Option<Box<crate::fxrack::State>>,
    voc: Option<Box<crate::fxvoc::State>>,
    multi: Option<Box<crate::fxmulti::State>>,
    split: Option<Box<crate::fxsplit::State>>,
    maxi: Option<Box<crate::fxmaxi::State>>,
    oldvoc: Option<Box<crate::fxvoc::classic::State>>,
    /// What a send tap took off the signal in the last block, for the player to add to the target track.
    pub tap: Vec<[f32; 2]>,
    /// A band splitter held in a patch bay: note every band in `tap` (low, mid, high) besides sending them on.
    pub tap_all: bool,
    /// The sound a compressor listens to in place of its own, when it was handed one for this block.
    side: Vec<[f32; 2]>,
    side_on: bool,
    patch: Option<Box<crate::fxpatch::State>>,
    psola: Option<Box<crate::psola::Shifter>>,
}

/// Comb and all-pass times of the pad reverb, in ms (four combs, then two all-passes).
const PAD_MS: [f32; 6] = [29.7, 37.1, 41.1, 43.7, 5.0, 1.7];
/// First two formants of the vowels A E I O U, in Hz.
const VOWELS: [[f32; 2]; 5] = [[800.0, 1150.0], [400.0, 2000.0], [300.0, 2500.0], [450.0, 800.0], [325.0, 700.0]];

/// The onset shaper's drive curve, read off a reference render in steps of 0.05: unit slope at rest, flat at
/// one from an input of three.
const TRANSIENT_BEND: [f32; 61] = [
    0.00000, 0.04996, 0.09971, 0.14904, 0.19772, 0.24557, 0.29223, 0.33778, 0.38198, 0.42468,
    0.46578, 0.50518, 0.54282, 0.57863, 0.61260, 0.64470, 0.67493, 0.70331, 0.72988, 0.75467,
    0.77773, 0.79913, 0.81892, 0.83718, 0.85399, 0.86942, 0.88354, 0.89644, 0.90818, 0.91885,
    0.92852, 0.93725, 0.94512, 0.95219, 0.95852, 0.96417, 0.96920, 0.97366, 0.97760, 0.98107,
    0.98410, 0.98675, 0.98905, 0.99103, 0.99273, 0.99417, 0.99539, 0.99641, 0.99725, 0.99794,
    0.99849, 0.99892, 0.99926, 0.99952, 0.99974, 0.99987, 0.99995, 0.99998, 1.00000, 1.00000,
    1.00000,
];

fn transient_bend(x: f32) -> f32 {
    let u = x.abs() * 20.0;
    if !(u < 60.0) {
        return x.signum();
    }
    let i = u as usize;
    let y = TRANSIENT_BEND[i] + (TRANSIENT_BEND[i + 1] - TRANSIENT_BEND[i]) * (u - i as f32);
    y.copysign(x)
}

pub(crate) fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

/// Knob position 0..=1 to Hz across the audible range.
/// The level knob law shared by many units: nothing at 0, unity at 1, measured as (11^x - 1) / 10.
fn knob_amp(x: f32) -> f32 {
    (11f32.powf(x.max(0.0)) - 1.0) / 10.0
}

fn knob_hz(x: f32) -> f32 {
    20.0 * 1000f32.powf(x.clamp(0.0, 1.0))
}

/// State variable filter step: returns (low, band, high).
#[inline]
pub(crate) fn svf(z: &mut [f32], x: f32, g: f32, k: f32) -> (f32, f32, f32) {
    let hp = (x - (k + g) * z[0] - z[1]) / (1.0 + g * (g + k));
    let v1 = g * hp;
    let bp = v1 + z[0];
    z[0] = bp + v1;
    let v2 = g * bp;
    let lp = v2 + z[1];
    z[1] = lp + v2;
    (lp, bp, hp)
}

pub(crate) fn svf_g(hz: f32, sr: f32) -> f32 {
    (PI * (hz / sr).clamp(0.0002, 0.49)).tan()
}

/// The level knob curve several of the small units share (measured): nothing at 0, unity at 1, and
/// (11^v - 1) / 10 between and beyond.
pub(crate) fn knob_gain(v: f32) -> f32 {
    (11f32.powf(v.max(0.0)) - 1.0) / 10.0
}

/// The balance unit's pan, measured every sixteenth of the way out from the centre: what the side
/// panned toward gets, and what the other side keeps.
const BALANCE_NEAR: [f32; 17] = [
    1.0, 1.08978, 1.16345, 1.22186, 1.26636, 1.29846, 1.32001, 1.33295, 1.33936, 1.34143, 1.34149, 1.34167, 1.34412, 1.35089, 1.36377,
    1.38440, 1.41421,
];
const BALANCE_FAR: [f32; 17] = [
    1.0, 0.90314, 0.80963, 0.72015, 0.63531, 0.55566, 0.48145, 0.41284, 0.34985, 0.29230, 0.23987, 0.19208, 0.14844, 0.10822, 0.07062,
    0.03485, 0.0,
];

/// Levels that travel toward where their knobs sit instead of jumping there (measured on two of the
/// small units): once a tick each level takes a straight line toward its goal, covering no more than a
/// thousandth a frame at 44.1 kHz. `z` holds the levels, then their per-frame moves, then the frames
/// left in the tick; a new unit starts with every level at nothing.
fn glide(z: &mut [f32], want: &[f32], tick: f32, sr: f32) {
    let n = want.len();
    if z[2 * n] < 0.5 {
        let tick = tick.max(1.0);
        z[2 * n] += tick;
        let frames = z[2 * n].round().max(1.0);
        let most = tick * 44.1 / sr;
        for k in 0..n {
            z[n + k] = (want[k] - z[k]).clamp(-most, most) / frames;
        }
    }
    for k in 0..n {
        z[k] += z[n + k];
    }
    z[2 * n] -= 1.0;
}

/// The auto pan wave rate in Hz for its stored speed: measured every 2500 from 27500 (3/64 Hz) to 55000
/// (19.2 Hz), read smoothly on a log scale.
fn auto_pan_hz(v: f32) -> f32 {
    const HZ: [f32; 12] = [0.046_875, 0.075_796, 0.123_536, 0.202_337, 0.334_116, 0.557_092, 0.940_329, 1.612_364, 2.824_501, 5.094_101, 9.581_756, 19.2];
    let mut ln = [0f32; 12];
    for (a, b) in ln.iter_mut().zip(HZ) {
        *a = b.ln();
    }
    curve_at(&ln, (v - 27500.0) / 27500.0).exp()
}

/// A smooth read (Catmull-Rom) of a curve tabled at even steps over 0..=1.
fn curve_at(t: &[f32], x: f32) -> f32 {
    let n = t.len() - 1;
    let f = x.clamp(0.0, 1.0) * n as f32;
    let i = (f as usize).min(n - 1);
    let u = f - i as f32;
    let at = |k: isize| {
        if k < 0 {
            2.0 * t[0] - t[1]
        } else if k as usize > n {
            2.0 * t[n] - t[n - 1]
        } else {
            t[k as usize]
        }
    };
    let (a, b, c, d) = (at(i as isize - 1), t[i], t[i + 1], at(i as isize + 2));
    b + 0.5 * u * (c - a + u * (2.0 * a - 5.0 * b + 4.0 * c - d + u * (3.0 * (b - c) + d - a)))
}

/// Balance-style pan: the far side is turned down, the near side stays at unity.
fn balance(p: f32) -> (f32, f32) {
    let p = p.clamp(-1.0, 1.0);
    ((1.0 - p).min(1.0), (1.0 + p).min(1.0))
}

/// Biquad coefficients b0 b1 b2 a1 a2 for the seven band equaliser's band types.
fn band_coeffs(kind: i32, hz: f32, gain_db: f32, width: f32, sr: f32) -> [f32; 5] {
    let w = TAU * (hz / sr).clamp(0.0002, 0.49);
    let (s, c) = w.sin_cos();
    let a = 10f32.powf(gain_db / 40.0);
    // width 0..=1: narrow to wide for the bell, steep to gentle for the shelves, resonant to flat for the passes
    let bw = 0.2 * (width.clamp(0.0, 1.0) * 6.0).exp2();
    let q_bell = 1.0 / (2.0 * (std::f32::consts::LN_2 / 2.0 * bw * w / s.max(1e-6)).sinh()).max(1e-3);
    let q_res = (0.3 * ((1.0 - width.clamp(0.0, 1.0)) * 4.5).exp2()).clamp(0.3, 12.0);
    let q_shelf = 0.4 + (1.0 - width.clamp(0.0, 1.0)) * 0.8;
    band_raw(kind, s, c, a, q_bell, q_res, q_shelf)
}

/// Band of the seven band unit, laws as measured: the bell spans 0.1 to 5 octaves between its half-gain points,
/// band pass and notch use that width over and times root two, the passes run from sharp to heavily damped.
fn eq_coeffs(kind: i32, hz: f32, gain_db: f32, width: f32, sr: f32, order: i32, part: usize) -> [f32; 5] {
    // sharpness of each section of a steep pass at the rest width, as measured
    const FLAT: [[f32; 4]; 3] = [[0.5297, 1.4025, 0.0, 0.0], [0.4908, 0.7650, 1.8688, 0.0], [0.4629, 0.741, 0.805, 2.529]];
    // steep bells: narrower bells set side by side for a flat top, as (width factor, offset in widths)
    const STAGGER: [[(f32, f32); 4]; 3] = [
        [(0.5, 0.1248), (0.5, -0.1248), (0.0, 0.0), (0.0, 0.0)],
        [(0.33, 0.17), (0.33, -0.17), (0.5, 0.0), (0.0, 0.0)],
        [(0.25, 0.194), (0.25, -0.194), (0.3925, 0.075), (0.3925, -0.075)],
    ];
    const STAGGER_GAIN: [f32; 3] = [0.6425, 0.507, 0.4289];
    let extra = order.unsigned_abs().min(3) as usize;
    let x = width.clamp(0.0, 1.0);
    let mut oct = 0.1 + 4.9 * x;
    let mut hz = hz;
    // bells and shelves of a higher order are equal shares of the gain in a row
    let mut share = if matches!(kind, 5 | 6 | 7) { 1.0 / (extra + 1) as f32 } else { 1.0 };
    if kind == 6 && order < 0 {
        let (narrow, off) = STAGGER[extra - 1][part.min(extra)];
        hz *= (off * oct).exp2();
        oct *= narrow;
        share = STAGGER_GAIN[extra - 1];
    }
    let w = TAU * (hz / sr).clamp(0.0001, 0.49);
    let (s, c) = w.sin_cos();
    let a = 10f32.powf(gain_db * share / 40.0);
    let q_of = |bw: f32| 1.0 / (2.0 * (std::f32::consts::LN_2 / 2.0 * bw).sinh());
    // the band widths stretch towards the top of the range the way the textbook bell does
    let warp = w / s.max(1e-6);
    let q_band = match kind {
        // a band pass or notch of a higher order is that many more equal sections, the band pass ones
        // wider and the notch ones narrower by the root of half their number (either sign of the order)
        2 => q_of(oct * ((extra + 1) as f32 * 0.5).sqrt() * warp),
        4 => q_of(oct / ((extra + 1) as f32 * 0.5).sqrt() * warp),
        _ => q_of(oct * warp),
    };
    // the passes: a cube law from ten times the flat sharpness down to a tenth of it
    let mut q_res = (0.1 + 9.9 * (1.0 - x).powi(3)) * std::f32::consts::FRAC_1_SQRT_2;
    // the shelves are set by steepness, which runs in a straight line over the knob and is one at its rest place
    let steep = 2.0539 - 1.7210 * x;
    let mut q_shelf = 1.0 / ((a + 1.0 / a) * (1.0 / steep - 1.0) + 2.0).max(0.01).sqrt();
    // steeper passes and shelves: more sections, either all alike (gentle) or spread the maximally flat way (steep)
    if extra > 0 {
        let n = (extra + 1) as f32;
        if order > 0 {
            q_res = (q_res * 0.5f32.powf(extra as f32 * 0.5)).powf(1.0 / n);
        } else {
            let flat = FLAT[extra - 1][part.min(extra)];
            q_res = flat * (q_res / 0.7071).powf(1.0 / n);
            // a steep shelf: every section has its own share of the steepness
            const SHARE: [[f32; 4]; 3] = [[1.6244, 0.3805, 0.0, 0.0], [1.9739, 0.8965, 0.1940, 0.0], [1.7330, 1.5235, 0.6506, 0.0971]];
            q_shelf = 1.0 / ((a + 1.0 / a) * (SHARE[extra - 1][part.min(extra)] / steep - 1.0) + 2.0).max(0.01).sqrt();
        }
    }
    band_raw(kind, s, c, a, q_band, q_res, q_shelf)
}

/// Level in dB that one band of the seven band unit gives a tone of `at_hz`: the same sections the sound runs
/// through, so the drawn curve is the heard one. `freq` and `width` are the knobs from 0 to 1.
pub fn eq_band_db(kind: i32, freq: f32, gain_db: f32, width: f32, order: i32, at_hz: f32) -> f32 {
    if kind == 0 || (matches!(kind, 5 | 6 | 7) && gain_db.abs() < 0.005) {
        return 0.0;
    }
    let rate = 88200.0;
    let w = TAU * (at_hz / rate).clamp(0.0, 0.5);
    let (s1, c1) = w.sin_cos();
    let (s2, c2) = (2.0 * w).sin_cos();
    let mut db = 0.0;
    for part in 0..=order.unsigned_abs().min(3) as usize {
        let c = eq_coeffs(kind, knob_hz(freq), gain_db, width, rate, order, part);
        let num = (c[0] + c[1] * c1 + c[2] * c2).powi(2) + (c[1] * s1 + c[2] * s2).powi(2);
        let den = (1.0 + c[3] * c1 + c[4] * c2).powi(2) + (c[3] * s1 + c[4] * s2).powi(2);
        db += 10.0 * (num.max(1e-12) / den.max(1e-12)).log10();
    }
    db
}

/// Three first-order sections of a two-path half-band filter; `st` holds four values.
#[inline]
fn hb_chain(c: &[f32; 3], st: &mut [f32], x: f32) -> f32 {
    let mut v = x;
    for k in 0..3 {
        let y = c[k] * (v - st[k + 1]) + st[k];
        st[k] = v;
        v = y;
    }
    st[3] = v;
    v
}

const HB_EVEN: [f32; 3] = [0.039_151_598, 0.302_646_85, 0.674_615_9];
const HB_ODD: [f32; 3] = [0.147_377_11, 0.482_468_54, 0.883_005_03];

/// One band of the classic equalizer, at the plain rate, by its own measured laws.
fn old_eq_coeffs(kind: i32, x_hz: f32, gain_db: f32, width: f32, sr: f32) -> [f32; 5] {
    let hz = (159.9 * 101f32.powf(x_hz.clamp(0.0, 1.0)) - 149.9).min(sr * 0.49);
    let x = width.clamp(0.0, 1.0);
    let a = 10f32.powf(gain_db / 40.0);
    let w = TAU * hz / sr;
    let q_pass = 0.1 + 18.9 * x;
    let q_bell = if kind == 6 { 0.1 * 1000f32.powf(x) } else { q_pass };
    let slope = 0.1 + 1.9 * x;
    let q_shelf = 1.0 / ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).max(0.01).sqrt();
    band_raw(kind, w.sin(), w.cos(), a, q_bell, q_pass, q_shelf)
}

fn band_raw(kind: i32, s: f32, c: f32, a: f32, q_bell: f32, q_res: f32, q_shelf: f32) -> [f32; 5] {
    let (b0, b1, b2, a0, a1, a2);
    match kind {
        1 => {
            let al = s / (2.0 * q_res);
            b0 = (1.0 - c) / 2.0;
            b1 = 1.0 - c;
            b2 = (1.0 - c) / 2.0;
            a0 = 1.0 + al;
            a1 = -2.0 * c;
            a2 = 1.0 - al;
        }
        2 => {
            let al = s / (2.0 * q_bell);
            b0 = al;
            b1 = 0.0;
            b2 = -al;
            a0 = 1.0 + al;
            a1 = -2.0 * c;
            a2 = 1.0 - al;
        }
        3 => {
            let al = s / (2.0 * q_res);
            b0 = (1.0 + c) / 2.0;
            b1 = -(1.0 + c);
            b2 = (1.0 + c) / 2.0;
            a0 = 1.0 + al;
            a1 = -2.0 * c;
            a2 = 1.0 - al;
        }
        4 => {
            let al = s / (2.0 * q_bell);
            b0 = 1.0;
            b1 = -2.0 * c;
            b2 = 1.0;
            a0 = 1.0 + al;
            a1 = -2.0 * c;
            a2 = 1.0 - al;
        }
        5 | 7 => {
            let al = s / (2.0 * q_shelf);
            let k = 2.0 * a.sqrt() * al;
            if kind == 5 {
                b0 = a * ((a + 1.0) - (a - 1.0) * c + k);
                b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * c);
                b2 = a * ((a + 1.0) - (a - 1.0) * c - k);
                a0 = (a + 1.0) + (a - 1.0) * c + k;
                a1 = -2.0 * ((a - 1.0) + (a + 1.0) * c);
                a2 = (a + 1.0) + (a - 1.0) * c - k;
            } else {
                b0 = a * ((a + 1.0) + (a - 1.0) * c + k);
                b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * c);
                b2 = a * ((a + 1.0) + (a - 1.0) * c - k);
                a0 = (a + 1.0) - (a - 1.0) * c + k;
                a1 = 2.0 * ((a - 1.0) - (a + 1.0) * c);
                a2 = (a + 1.0) - (a - 1.0) * c - k;
            }
        }
        _ => {
            let al = s / (2.0 * q_bell);
            b0 = 1.0 + al * a;
            b1 = -2.0 * c;
            b2 = 1.0 - al * a;
            a0 = 1.0 + al / a;
            a1 = -2.0 * c;
            a2 = 1.0 - al / a;
        }
    }
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

#[inline]
/// Echo time of the echo unit in frames: steps when synced to tempo, otherwise a fixed scale.
fn echo_time(p: &[f32], cx: &Ctx) -> f32 {
    if p[2] >= 0.5 {
        p[4] / 48.0 * cx.step
    } else {
        p[4] / 48.0 * 0.125 * cx.sr
    }
}


/// The tallest point of the three-part chorus sweep before it is scaled to 1.
const CHORUS_PEAK: f32 = 1.276_142_4;
/// One cycle of a chorus sweep, -1 to 1, phase 0 to 1. The round wave is built from parabolas.
fn chorus_wave(ph: f32, shape: f32) -> f32 {
    // four shapes, measured: a wave of two parabolas, that one cubed, a sine with two faster ones
    // riding on it (brought back to full swing), and that one cubed
    let which = (shape * 3.0 / 1024.0) as i32;
    let w = if which >= 2 {
        let a = ph.fract() * TAU;
        (a.sin() + (2.0 * a).sin() / 3.0 + (3.0 * a).sin() / 3.0) / CHORUS_PEAK
    } else {
        let t = ph.fract() * 2.0;
        if t < 1.0 { 4.0 * t * (1.0 - t) } else { -4.0 * (t - 1.0) * (2.0 - t) }
    };
    if which == 1 || which >= 3 { w * w * w } else { w }
}
/// Echo time of the plain echo in frames: the song's steps, or steps of its own tempo.
fn old_delay_time(p: &[f32], cx: &Ctx) -> f32 {
    let step = if p[3] < 1.0 { cx.step } else { 15.0 / (280.0 * p[3] / 1024.0).max(20.0) * cx.sr };
    (p[4].max(1.0) * step).max(1.0)
}

fn biquad(c: &[f32; 5], z: &mut [f32], x: f32) -> f32 {
    let y = c[0] * x + z[0];
    z[0] = c[1] * x - c[3] * y + z[1];
    z[1] = c[2] * x - c[4] * y;
    y
}

/// Echo time of the grain shifter in frames: steps when its tempo switch is on, milliseconds otherwise.
fn shift_delay(p: &[f32], cx: &Ctx) -> f32 {
    if p[11] as i32 & 1 != 0 {
        p[8] / 48.0 * cx.step
    } else {
        p[8] * 0.001 * cx.sr
    }
}

/// Cutoff in Hz of a band edge of the three band compressor for its stored value (20 to 20000), read off
/// renders of single clicks through one band.
fn band_edge_hz(v: f32) -> f32 {
    const T: [(f32, f32); 24] = [
        (20.0, 20.0), (500.0, 23.64), (1000.0, 27.88), (1500.0, 33.77), (2000.0, 41.5), (3000.0, 64.89), (4000.0, 102.11), (5000.0, 159.3),
        (6500.0, 300.5), (7000.0, 367.12), (8000.0, 541.09), (9000.0, 782.9), (10000.0, 1114.2), (11000.0, 1561.36), (12000.0, 2157.76),
        (13000.0, 2945.19), (14000.0, 3975.71), (15000.0, 5319.1), (16000.0, 7035.13), (17000.0, 9236.5), (18000.0, 12032.69),
        (19000.0, 15564.36), (19500.0, 17657.46), (20000.0, 20010.0),
    ];
    let v = v.clamp(20.0, 20000.0);
    let i = T.iter().position(|e| e.0 >= v).unwrap_or(23).max(1);
    let (a, b) = (T[i - 1], T[i]);
    a.1 * (b.1 / a.1).powf((v - a.0) / (b.0 - a.0))
}

/// Echo time of one bank of the echo bank in frames. Tempo mode counts in parts of a step (480 to a step in
/// the newer layout, 48 in the older one), the free mode in milliseconds.
fn bank_time(q: &[f32], layout: f32, cx: &Ctx) -> f32 {
    let fine = if layout >= 16.0 { 10.0 } else { 1.0 };
    if (q[19] as i32 >> 24) & 1 != 0 {
        q[8] / (48.0 * fine) * cx.step
    } else {
        // 3840 is one second in the newer layout
        q[8] / (0.384 * fine) * 0.001 * cx.sr
    }
}

/// Delay lengths in samples at 44.1 kHz for room sizes 1..=100, read off impulse renders of every size:
/// eight diffusers, the two inner diffusers, then eight taps for each side.
const REV2_LEN: [[u16; 26]; 100] = [
    [32, 43, 84, 116, 43, 43, 278, 206, 814, 552, 81, 555, 912, 1395, 1879, 2297, 2872, 3077, 108, 518, 1112, 1422, 1895, 2397, 2890, 3057],
    [34, 46, 90, 123, 46, 46, 295, 219, 865, 586, 86, 590, 969, 1483, 1997, 2441, 3052, 3269, 115, 550, 1182, 1511, 2014, 2547, 3071, 3248],
    [36, 49, 95, 130, 49, 49, 313, 231, 916, 621, 91, 624, 1026, 1570, 2115, 2585, 3232, 3462, 121, 583, 1251, 1600, 2132, 2697, 3252, 3439],
    [38, 51, 100, 138, 51, 51, 330, 244, 967, 655, 96, 659, 1083, 1658, 2232, 2729, 3412, 3655, 128, 615, 1321, 1690, 2251, 2847, 3432, 3631],
    [41, 54, 106, 145, 54, 54, 348, 257, 1018, 690, 101, 694, 1140, 1745, 2350, 2873, 3592, 3847, 135, 648, 1390, 1778, 2369, 2997, 3613, 3822],
    [43, 57, 111, 152, 57, 57, 365, 270, 1069, 724, 107, 729, 1197, 1831, 2467, 3016, 3771, 4039, 142, 680, 1460, 1868, 2488, 3147, 3794, 4013],
    [45, 59, 116, 159, 59, 59, 382, 283, 1120, 759, 112, 763, 1254, 1919, 2584, 3159, 3950, 4232, 148, 712, 1529, 1956, 2606, 3297, 3975, 4205],
    [47, 62, 122, 167, 62, 62, 400, 296, 1171, 793, 117, 798, 1311, 2006, 2702, 3303, 4130, 4424, 155, 745, 1599, 2045, 2725, 3446, 4155, 4395],
    [49, 65, 127, 174, 65, 65, 417, 309, 1222, 828, 122, 833, 1368, 2094, 2820, 3447, 4310, 4617, 162, 777, 1668, 2135, 2844, 3597, 4337, 4587],
    [51, 68, 132, 181, 68, 68, 435, 322, 1273, 862, 127, 868, 1425, 2181, 2937, 3591, 4490, 4810, 169, 810, 1738, 2223, 2962, 3746, 4516, 4777],
    [53, 70, 138, 188, 70, 70, 452, 334, 1324, 897, 132, 902, 1482, 2268, 3055, 3735, 4670, 5002, 175, 842, 1808, 2312, 3080, 3896, 4697, 4969],
    [55, 73, 143, 196, 73, 73, 470, 347, 1374, 931, 137, 937, 1539, 2355, 3172, 3877, 4848, 5193, 182, 874, 1877, 2401, 3199, 4046, 4878, 5160],
    [57, 76, 148, 203, 76, 76, 487, 360, 1425, 966, 142, 972, 1596, 2442, 3289, 4021, 5027, 5386, 189, 907, 1947, 2490, 3317, 4196, 5059, 5352],
    [59, 78, 154, 210, 78, 78, 504, 373, 1476, 1000, 147, 1006, 1653, 2530, 3407, 4165, 5207, 5578, 196, 939, 2016, 2578, 3435, 4345, 5239, 5542],
    [61, 81, 159, 218, 81, 81, 522, 386, 1527, 1035, 153, 1041, 1710, 2617, 3525, 4309, 5387, 5771, 203, 972, 2086, 2668, 3554, 4496, 5421, 5734],
    [63, 84, 164, 225, 84, 84, 539, 399, 1578, 1069, 158, 1076, 1767, 2705, 3642, 4453, 5567, 5963, 209, 1004, 2155, 2757, 3673, 4645, 5600, 5924],
    [65, 87, 169, 232, 87, 87, 557, 412, 1629, 1104, 163, 1111, 1824, 2792, 3760, 4597, 5747, 6156, 216, 1036, 2225, 2846, 3792, 4796, 5782, 6117],
    [67, 89, 175, 239, 89, 89, 574, 425, 1680, 1138, 168, 1145, 1881, 2879, 3878, 4740, 5927, 6349, 223, 1069, 2294, 2935, 3910, 4945, 5962, 6307],
    [69, 92, 180, 247, 92, 92, 591, 438, 1731, 1173, 173, 1180, 1938, 2967, 3996, 4884, 6107, 6541, 230, 1101, 2364, 3023, 4028, 5095, 6143, 6499],
    [71, 95, 185, 254, 95, 95, 609, 450, 1782, 1207, 178, 1215, 1995, 3053, 4112, 5027, 6285, 6733, 236, 1134, 2433, 3113, 4147, 5245, 6324, 6690],
    [73, 98, 191, 261, 98, 98, 626, 463, 1833, 1242, 183, 1250, 2052, 3141, 4230, 5171, 6465, 6926, 243, 1166, 2503, 3202, 4265, 5395, 6505, 6881],
    [75, 100, 196, 268, 100, 100, 644, 476, 1884, 1276, 188, 1284, 2109, 3228, 4348, 5315, 6645, 7118, 250, 1198, 2572, 3290, 4383, 5544, 6684, 7071],
    [77, 103, 201, 276, 103, 103, 661, 489, 1935, 1311, 193, 1319, 2166, 3316, 4465, 5459, 6825, 7311, 257, 1231, 2642, 3380, 4503, 5695, 6866, 7264],
    [80, 106, 207, 283, 106, 106, 678, 502, 1986, 1345, 198, 1354, 2223, 3403, 4583, 5603, 7005, 7504, 263, 1263, 2712, 3468, 4621, 5844, 7046, 7454],
    [82, 108, 212, 290, 108, 108, 696, 515, 2037, 1380, 204, 1388, 2280, 3490, 4701, 5747, 7185, 7696, 270, 1296, 2781, 3558, 4740, 5996, 7228, 7647],
    [84, 111, 217, 297, 111, 111, 713, 528, 2087, 1414, 209, 1423, 2337, 3577, 4817, 5889, 7363, 7887, 277, 1328, 2851, 3647, 4858, 6145, 7408, 7837],
    [86, 114, 223, 305, 114, 114, 731, 541, 2138, 1449, 214, 1458, 2394, 3664, 4935, 6033, 7543, 8080, 284, 1360, 2920, 3736, 4977, 6296, 7590, 8029],
    [88, 117, 228, 312, 117, 117, 748, 554, 2189, 1484, 219, 1493, 2451, 3752, 5053, 6176, 7722, 8272, 291, 1393, 2990, 3825, 5095, 6446, 7770, 8220],
    [90, 119, 233, 319, 119, 119, 766, 566, 2240, 1518, 224, 1527, 2509, 3839, 5170, 6320, 7902, 8465, 297, 1425, 3059, 3913, 5213, 6595, 7950, 8411],
    [92, 122, 238, 327, 122, 122, 783, 579, 2291, 1553, 229, 1562, 2565, 3927, 5288, 6464, 8082, 8658, 304, 1458, 3129, 4003, 5332, 6746, 8132, 8603],
    [94, 125, 244, 334, 125, 125, 800, 592, 2342, 1587, 234, 1597, 2623, 4014, 5406, 6608, 8262, 8850, 311, 1490, 3199, 4091, 5451, 6895, 8312, 8794],
    [96, 127, 249, 341, 127, 127, 818, 605, 2393, 1622, 239, 1631, 2680, 4101, 5523, 6752, 8442, 9043, 318, 1522, 3268, 4180, 5569, 7045, 8493, 8985],
    [98, 130, 254, 348, 130, 130, 835, 618, 2444, 1656, 244, 1666, 2736, 4188, 5640, 6895, 8621, 9234, 324, 1555, 3337, 4270, 5688, 7195, 8674, 9176],
    [100, 133, 260, 356, 133, 133, 853, 631, 2495, 1691, 249, 1701, 2794, 4275, 5758, 7039, 8800, 9427, 331, 1587, 3407, 4358, 5806, 7345, 8855, 9367],
    [102, 136, 265, 363, 136, 136, 870, 644, 2546, 1725, 255, 1736, 2851, 4363, 5875, 7183, 8980, 9620, 338, 1620, 3477, 4447, 5924, 7494, 9034, 9558],
    [104, 138, 270, 370, 138, 138, 887, 657, 2597, 1760, 260, 1770, 2908, 4450, 5993, 7327, 9160, 9812, 345, 1652, 3546, 4536, 6043, 7645, 9216, 9750],
    [106, 141, 276, 377, 141, 141, 905, 669, 2648, 1794, 265, 1805, 2965, 4537, 6111, 7471, 9340, 10005, 351, 1684, 3616, 4625, 6161, 7794, 9396, 9940],
    [108, 144, 281, 385, 144, 144, 922, 682, 2699, 1829, 270, 1840, 3022, 4625, 6228, 7615, 9520, 10198, 358, 1717, 3685, 4715, 6280, 7945, 9578, 10133],
    [110, 147, 286, 392, 147, 147, 940, 695, 2749, 1863, 275, 1875, 3079, 4712, 6346, 7757, 9699, 10389, 365, 1749, 3755, 4803, 6399, 8094, 9758, 10323],
    [112, 149, 292, 399, 149, 149, 957, 708, 2800, 1898, 280, 1909, 3136, 4800, 6464, 7901, 9879, 10582, 372, 1782, 3824, 4892, 6517, 8244, 9939, 10514],
    [114, 152, 297, 406, 152, 152, 974, 721, 2851, 1932, 285, 1944, 3193, 4886, 6580, 8044, 10057, 10774, 379, 1814, 3894, 4981, 6636, 8394, 10119, 10706],
    [116, 155, 302, 414, 155, 155, 992, 734, 2902, 1967, 290, 1979, 3250, 4974, 6698, 8188, 10237, 10966, 385, 1847, 3963, 5070, 6754, 8544, 10300, 10897],
    [118, 157, 308, 421, 157, 157, 1009, 747, 2953, 2001, 295, 2013, 3307, 5061, 6816, 8332, 10417, 11159, 392, 1879, 4033, 5158, 6872, 8693, 10480, 11087],
    [121, 160, 313, 428, 160, 160, 1027, 760, 3004, 2036, 300, 2048, 3364, 5148, 6934, 8476, 10597, 11352, 399, 1911, 4103, 5248, 6991, 8844, 10662, 11280],
    [123, 163, 318, 436, 163, 163, 1044, 773, 3055, 2070, 306, 2083, 3421, 5236, 7051, 8620, 10777, 11544, 406, 1944, 4172, 5337, 7109, 8993, 10842, 11470],
    [125, 166, 323, 443, 166, 166, 1061, 785, 3106, 2105, 311, 2118, 3478, 5323, 7169, 8764, 10957, 11737, 412, 1976, 4242, 5426, 7229, 9144, 11024, 11662],
    [127, 168, 329, 450, 168, 168, 1079, 798, 3157, 2139, 316, 2152, 3535, 5410, 7286, 8907, 11136, 11929, 419, 2008, 4311, 5515, 7347, 9293, 11203, 11853],
    [129, 171, 334, 457, 171, 171, 1096, 811, 3208, 2174, 321, 2187, 3592, 5497, 7403, 9051, 11316, 12121, 426, 2041, 4381, 5604, 7466, 9444, 11385, 12045],
    [131, 174, 339, 465, 174, 174, 1114, 824, 3259, 2208, 326, 2222, 3649, 5585, 7521, 9195, 11495, 12314, 433, 2073, 4450, 5693, 7584, 9593, 11565, 12235],
    [133, 176, 345, 472, 176, 176, 1131, 837, 3310, 2243, 331, 2257, 3706, 5672, 7639, 9338, 11675, 12507, 439, 2106, 4520, 5782, 7702, 9743, 11746, 12427],
    [135, 179, 350, 479, 179, 179, 1149, 850, 3361, 2277, 336, 2291, 3763, 5759, 7756, 9482, 11855, 12699, 446, 2138, 4589, 5871, 7821, 9893, 11927, 12618],
    [137, 182, 355, 486, 182, 182, 1166, 863, 3412, 2312, 341, 2326, 3820, 5847, 7874, 9626, 12035, 12892, 453, 2171, 4659, 5960, 7939, 10043, 12108, 12809],
    [139, 185, 361, 494, 185, 185, 1183, 876, 3462, 2346, 346, 2361, 3877, 5934, 7992, 9769, 12214, 13083, 460, 2203, 4728, 6048, 8057, 10192, 12287, 13000],
    [141, 187, 366, 501, 187, 187, 1201, 889, 3513, 2381, 351, 2395, 3934, 6022, 8109, 9913, 12394, 13276, 467, 2235, 4798, 6138, 8177, 10343, 12469, 13192],
    [143, 190, 371, 508, 190, 190, 1218, 901, 3564, 2415, 357, 2430, 3991, 6108, 8226, 10056, 12573, 13468, 473, 2268, 4867, 6226, 8295, 10492, 12649, 13382],
    [145, 193, 377, 515, 193, 193, 1236, 914, 3615, 2450, 362, 2465, 4048, 6196, 8344, 10200, 12752, 13660, 480, 2300, 4937, 6315, 8413, 10642, 12830, 13574],
    [147, 196, 382, 523, 196, 196, 1253, 927, 3666, 2484, 367, 2500, 4105, 6283, 8461, 10344, 12932, 13853, 487, 2333, 5007, 6405, 8532, 10792, 13011, 13765],
    [149, 198, 387, 530, 198, 198, 1270, 940, 3717, 2519, 372, 2534, 4162, 6370, 8579, 10488, 13112, 14046, 494, 2365, 5076, 6493, 8650, 10942, 13192, 13956],
    [151, 201, 393, 537, 201, 201, 1288, 953, 3768, 2553, 377, 2569, 4219, 6458, 8697, 10632, 13292, 14238, 500, 2397, 5146, 6583, 8769, 11092, 13373, 14148],
    [153, 204, 398, 545, 204, 204, 1305, 966, 3819, 2588, 382, 2604, 4276, 6545, 8814, 10775, 13472, 14431, 507, 2430, 5215, 6671, 8887, 11242, 13553, 14339],
    [155, 206, 403, 552, 206, 206, 1323, 979, 3870, 2622, 387, 2638, 4333, 6633, 8932, 10919, 13652, 14624, 514, 2462, 5285, 6760, 9006, 11391, 13733, 14529],
    [157, 209, 408, 559, 209, 209, 1340, 992, 3921, 2657, 392, 2673, 4390, 6719, 9049, 11062, 13830, 14815, 521, 2495, 5354, 6850, 9125, 11542, 13915, 14722],
    [160, 212, 414, 566, 212, 212, 1357, 1005, 3972, 2691, 397, 2708, 4447, 6807, 9166, 11206, 14010, 15008, 527, 2527, 5424, 6938, 9243, 11692, 14095, 14912],
    [162, 215, 419, 574, 215, 215, 1375, 1017, 4023, 2726, 402, 2743, 4504, 6894, 9284, 11350, 14190, 15200, 534, 2559, 5493, 7027, 9361, 11842, 14276, 15103],
    [164, 217, 424, 581, 217, 217, 1392, 1030, 4074, 2761, 408, 2777, 4561, 6981, 9402, 11494, 14370, 15393, 541, 2592, 5563, 7116, 9480, 11993, 14458, 15296],
    [166, 220, 430, 588, 220, 220, 1410, 1043, 4124, 2795, 413, 2812, 4618, 7069, 9519, 11637, 14549, 15585, 548, 2624, 5632, 7205, 9598, 12142, 14637, 15486],
    [168, 223, 435, 595, 223, 223, 1427, 1056, 4175, 2830, 418, 2847, 4675, 7156, 9637, 11781, 14729, 15778, 555, 2657, 5702, 7295, 9717, 12293, 14819, 15678],
    [170, 225, 440, 603, 225, 225, 1444, 1069, 4226, 2864, 423, 2882, 4732, 7243, 9754, 11924, 14908, 15969, 561, 2689, 5772, 7383, 9835, 12442, 14999, 15869],
    [172, 228, 446, 610, 228, 228, 1462, 1082, 4277, 2899, 428, 2916, 4789, 7330, 9872, 12068, 15088, 16162, 568, 2721, 5841, 7473, 9955, 12593, 15181, 16061],
    [174, 231, 451, 617, 231, 231, 1479, 1095, 4328, 2933, 433, 2951, 4846, 7417, 9989, 12212, 15267, 16354, 575, 2754, 5911, 7561, 10073, 12742, 15361, 16251],
    [176, 234, 456, 624, 234, 234, 1497, 1108, 4379, 2968, 438, 2986, 4903, 7505, 10107, 12355, 15447, 16547, 582, 2786, 5980, 7650, 10191, 12892, 15542, 16443],
    [178, 236, 462, 632, 236, 236, 1514, 1120, 4430, 3002, 443, 3020, 4960, 7592, 10225, 12499, 15627, 16740, 588, 2819, 6050, 7739, 10310, 13042, 15722, 16634],
    [180, 239, 467, 639, 239, 239, 1532, 1133, 4481, 3037, 448, 3055, 5018, 7680, 10342, 12643, 15807, 16932, 595, 2851, 6119, 7828, 10428, 13192, 15903, 16825],
    [182, 242, 472, 646, 242, 242, 1549, 1146, 4532, 3071, 453, 3090, 5074, 7767, 10460, 12787, 15987, 17125, 602, 2883, 6189, 7917, 10546, 13341, 16083, 17015],
    [184, 245, 478, 654, 245, 245, 1566, 1159, 4583, 3106, 459, 3125, 5132, 7855, 10578, 12931, 16167, 17318, 609, 2916, 6258, 8006, 10665, 13492, 16265, 17208],
    [186, 247, 483, 661, 247, 247, 1584, 1172, 4634, 3140, 464, 3159, 5189, 7941, 10694, 13074, 16346, 17509, 615, 2948, 6328, 8095, 10784, 13641, 16445, 17398],
    [188, 250, 488, 668, 250, 250, 1601, 1185, 4685, 3175, 469, 3194, 5246, 8028, 10812, 13218, 16525, 17702, 622, 2981, 6398, 8184, 10903, 13792, 16627, 17591],
    [190, 253, 493, 675, 253, 253, 1619, 1198, 4736, 3209, 474, 3229, 5303, 8116, 10930, 13362, 16705, 17895, 629, 3013, 6467, 8273, 11021, 13941, 16806, 17781],
    [192, 255, 499, 683, 255, 255, 1636, 1211, 4786, 3244, 479, 3263, 5360, 8203, 11047, 13505, 16884, 18086, 636, 3045, 6536, 8362, 11139, 14091, 16987, 17972],
    [194, 258, 504, 690, 258, 258, 1653, 1224, 4837, 3278, 484, 3298, 5417, 8291, 11165, 13649, 17064, 18279, 642, 3078, 6606, 8451, 11258, 14241, 17168, 18163],
    [196, 261, 509, 697, 261, 261, 1671, 1236, 4888, 3313, 489, 3333, 5474, 8378, 11283, 13793, 17244, 18471, 649, 3110, 6676, 8540, 11376, 14391, 17349, 18355],
    [199, 264, 515, 704, 264, 264, 1688, 1249, 4939, 3347, 494, 3368, 5531, 8465, 11399, 13935, 17423, 18663, 656, 3143, 6745, 8628, 11494, 14540, 17529, 18545],
    [201, 266, 520, 712, 266, 266, 1706, 1262, 4990, 3382, 499, 3402, 5588, 8552, 11517, 14079, 17602, 18856, 663, 3175, 6815, 8718, 11613, 14691, 17711, 18737],
    [203, 269, 525, 719, 269, 269, 1723, 1275, 5041, 3416, 504, 3437, 5645, 8639, 11635, 14223, 17782, 19049, 670, 3207, 6884, 8806, 11732, 14840, 17890, 18928],
    [205, 272, 531, 726, 272, 272, 1740, 1288, 5092, 3451, 510, 3472, 5702, 8727, 11752, 14367, 17962, 19241, 676, 3240, 6954, 8895, 11850, 14990, 18071, 19119],
    [207, 274, 536, 733, 274, 274, 1758, 1301, 5143, 3485, 515, 3507, 5759, 8814, 11870, 14511, 18142, 19434, 683, 3272, 7023, 8985, 11969, 15140, 18252, 19310],
    [209, 277, 541, 741, 277, 277, 1775, 1314, 5194, 3520, 520, 3541, 5816, 8902, 11988, 14655, 18322, 19626, 690, 3305, 7093, 9073, 12087, 15290, 18433, 19502],
    [211, 280, 547, 748, 280, 280, 1793, 1327, 5245, 3554, 525, 3576, 5873, 8989, 12105, 14799, 18502, 19819, 697, 3337, 7162, 9163, 12206, 15440, 18614, 19693],
    [213, 283, 552, 755, 283, 283, 1810, 1340, 5296, 3589, 530, 3611, 5930, 9076, 12222, 14942, 18681, 20011, 703, 3369, 7232, 9251, 12324, 15590, 18795, 19884],
    [215, 285, 557, 763, 285, 285, 1828, 1352, 5347, 3623, 535, 3645, 5987, 9163, 12340, 15086, 18861, 20203, 710, 3402, 7302, 9341, 12443, 15740, 18976, 20076],
    [217, 288, 562, 770, 288, 288, 1845, 1365, 5398, 3658, 540, 3680, 6044, 9250, 12457, 15230, 19040, 20396, 717, 3434, 7371, 9430, 12561, 15890, 19156, 20267],
    [219, 291, 568, 777, 291, 291, 1862, 1378, 5449, 3692, 545, 3715, 6101, 9338, 12575, 15374, 19220, 20589, 724, 3467, 7441, 9518, 12680, 16039, 19336, 20457],
    [221, 294, 573, 784, 294, 294, 1880, 1391, 5499, 3727, 550, 3750, 6158, 9425, 12693, 15516, 19399, 20780, 730, 3499, 7510, 9608, 12799, 16190, 19518, 20650],
    [223, 296, 578, 792, 296, 296, 1897, 1404, 5550, 3761, 555, 3784, 6215, 9513, 12811, 15660, 19579, 20973, 737, 3532, 7580, 9696, 12917, 16339, 19698, 20840],
    [225, 299, 584, 799, 299, 299, 1915, 1417, 5601, 3796, 561, 3819, 6272, 9600, 12928, 15804, 19759, 21166, 744, 3564, 7649, 9785, 13035, 16489, 19879, 21031],
    [227, 302, 589, 806, 302, 302, 1932, 1430, 5652, 3830, 566, 3854, 6329, 9687, 13046, 15948, 19939, 21358, 751, 3596, 7719, 9874, 13154, 16639, 20060, 21223],
    [229, 304, 594, 813, 304, 304, 1949, 1443, 5703, 3865, 571, 3889, 6386, 9774, 13163, 16091, 20118, 21550, 758, 3629, 7788, 9963, 13272, 16789, 20240, 21414],
    [231, 307, 600, 821, 307, 307, 1967, 1455, 5754, 3899, 576, 3923, 6443, 9861, 13280, 16235, 20297, 21743, 764, 3661, 7858, 10053, 13391, 16939, 20421, 21605],
    [233, 310, 605, 828, 310, 310, 1984, 1468, 5805, 3934, 581, 3958, 6500, 9949, 13398, 16379, 20477, 21935, 771, 3694, 7927, 10141, 13510, 17089, 20602, 21797],
    [235, 313, 610, 835, 313, 313, 2002, 1481, 5856, 3969, 586, 3993, 6557, 10036, 13516, 16523, 20657, 22128, 778, 3726, 7997, 10230, 13628, 17240, 20783, 21988],
];
const REV2_EARLY: [f32; 2] = [0.1758, 0.1668];
/// The powers the ring diffusers' gains take.
const REV2_G5: f32 = 1.265;
const REV2_G6: f32 = 2.46;

/// Kind B curve of the fast distortion, measured from reference renders: one row per
/// threshold step 1..10, natural log of the output at inputs 10^(-3 + i/16), i = 0..=48.
#[rustfmt::skip]
const FAST_DIST_B: [[f32; 49]; 10] = [
    [-6.37564, -6.24086, -6.09735, -5.97287, -5.85112, -5.72179, -5.58730, -5.45782, -5.32422, -5.18942, -5.06370, -4.93076, -4.80116, -4.67139, -4.53994, -4.40876, -4.27750, -4.14626, -4.01544, -3.88474, -3.75215, -3.62218, -3.49041, -3.35821, -3.22715, -3.09491, -2.96303, -2.83101, -2.69825, -2.56676, -2.43403, -2.30144, -2.16897, -2.03619, -1.90271, -1.76936, -1.63602, -1.50222, -1.36827, -1.23408, -1.09968, -0.96468, -0.82934, -0.69372, -0.55759, -0.42055, -0.28240, -0.14327, -0.00044],
    [-5.87212, -5.75535, -5.63067, -5.51651, -5.39720, -5.27662, -5.15773, -5.04043, -4.92293, -4.80420, -4.68827, -4.56694, -4.44894, -4.33119, -4.21205, -4.09173, -3.97313, -3.85425, -3.73530, -3.61543, -3.49481, -3.37532, -3.25592, -3.13506, -3.01500, -2.89384, -2.77316, -2.65189, -2.53015, -2.40921, -2.28702, -2.16488, -2.04291, -1.92015, -1.79682, -1.67335, -1.54974, -1.42547, -1.30093, -1.17574, -1.05015, -0.92373, -0.79660, -0.66875, -0.53994, -0.40959, -0.27732, -0.14264, -0.00085],
    [-5.40624, -5.29987, -5.19187, -5.08840, -4.98130, -4.87316, -4.76522, -4.66192, -4.55296, -4.44567, -4.33964, -4.22921, -4.12294, -4.01486, -3.90715, -3.79921, -3.69093, -3.58252, -3.47401, -3.36475, -3.25516, -3.14648, -3.03629, -2.92677, -2.81682, -2.70584, -2.59510, -2.48417, -2.37245, -2.26134, -2.14914, -2.03653, -1.92405, -1.81076, -1.69690, -1.58264, -1.46806, -1.35266, -1.23678, -1.12015, -1.00286, -0.88454, -0.76515, -0.64468, -0.52281, -0.39889, -0.27230, -0.14201, -0.00129],
    [-4.98030, -4.88229, -4.78823, -4.69127, -4.59708, -4.50021, -4.40324, -4.30779, -4.20908, -4.11291, -4.01556, -3.91725, -3.82080, -3.72324, -3.62495, -3.52666, -3.42800, -3.32914, -3.23099, -3.13176, -3.03146, -2.93247, -2.83215, -2.73204, -2.63136, -2.53017, -2.42871, -2.32679, -2.22476, -2.12254, -2.01924, -1.91593, -1.81203, -1.70761, -1.60243, -1.49688, -1.39071, -1.28358, -1.17591, -1.06722, -0.95772, -0.84697, -0.73498, -0.62152, -0.50626, -0.38849, -0.26739, -0.14139, -0.00176],
    [-4.58692, -4.50182, -4.41502, -4.32792, -4.24168, -4.15460, -4.06676, -3.98028, -3.89212, -3.80496, -3.71682, -3.62948, -3.54102, -3.45186, -3.36328, -3.27404, -3.18476, -3.09499, -3.00508, -2.91437, -2.82398, -2.73300, -2.64157, -2.55029, -2.45806, -2.36572, -2.27294, -2.17966, -2.08607, -1.99213, -1.89738, -1.80227, -1.70663, -1.61042, -1.51333, -1.41563, -1.31732, -1.21804, -1.11797, -1.01679, -0.91464, -0.81104, -0.70594, -0.59916, -0.49019, -0.37837, -0.26253, -0.14076, -0.00229],
    [-4.22388, -4.14977, -4.07164, -3.99311, -3.91437, -3.83642, -3.75830, -3.67812, -3.60034, -3.52058, -3.44083, -3.36092, -3.28076, -3.20071, -3.12030, -3.03909, -2.95823, -2.87680, -2.79472, -2.71270, -2.63000, -2.54719, -2.46409, -2.38034, -2.29634, -2.21200, -2.12715, -2.04185, -1.95612, -1.86970, -1.78295, -1.69545, -1.60737, -1.51862, -1.42914, -1.33895, -1.24789, -1.15585, -1.06292, -0.96877, -0.87344, -0.77661, -0.67808, -0.57761, -0.47466, -0.36848, -0.25779, -0.14013, -0.00283],
    [-3.89074, -3.82606, -3.75497, -3.68240, -3.61336, -3.54237, -3.47192, -3.39927, -3.32952, -3.25766, -3.18486, -3.11332, -3.04076, -2.96724, -2.89514, -2.82111, -2.74762, -2.67371, -2.59914, -2.52442, -2.44980, -2.37393, -2.29822, -2.22208, -2.14536, -2.06835, -1.99078, -1.91265, -1.83415, -1.75486, -1.67522, -1.59489, -1.51380, -1.43213, -1.34967, -1.26632, -1.18207, -1.09683, -1.01053, -0.92298, -0.83413, -0.74360, -0.65127, -0.55680, -0.45961, -0.35886, -0.25310, -0.13951, -0.00342],
    [-3.58392, -3.52658, -3.46307, -3.39778, -3.33480, -3.27014, -3.20805, -3.14165, -3.07887, -3.01430, -2.94867, -2.88348, -2.81787, -2.75102, -2.68588, -2.61909, -2.55238, -2.48537, -2.41757, -2.34962, -2.28152, -2.21245, -2.14368, -2.07411, -2.00419, -1.93393, -1.86304, -1.79175, -1.71984, -1.64713, -1.57409, -1.50026, -1.42581, -1.35052, -1.27457, -1.19767, -1.11975, -1.04087, -0.96073, -0.87939, -0.79656, -0.71205, -0.62557, -0.53675, -0.44507, -0.34951, -0.24855, -0.13890, -0.00406],
    [-3.30043, -3.25061, -3.19350, -3.13400, -3.07720, -3.01999, -2.96404, -2.90344, -2.84716, -2.78855, -2.72926, -2.67085, -2.61179, -2.55074, -2.49170, -2.43148, -2.37097, -2.31021, -2.24852, -2.18664, -2.12520, -2.06217, -1.99966, -1.93616, -1.87233, -1.80827, -1.74364, -1.67850, -1.61271, -1.54587, -1.47903, -1.41130, -1.34284, -1.27369, -1.20365, -1.13273, -1.06074, -0.98770, -0.91337, -0.83782, -0.76073, -0.68180, -0.60088, -0.51744, -0.43093, -0.34038, -0.24406, -0.13827, -0.00474],
    [-3.04020, -2.99668, -2.94548, -2.89063, -2.84008, -2.78823, -2.73842, -2.68298, -2.63340, -2.58048, -2.52625, -2.47403, -2.42059, -2.36489, -2.31178, -2.25711, -2.20218, -2.14728, -2.09152, -2.03502, -1.97940, -1.92191, -1.86509, -1.80748, -1.74934, -1.69092, -1.63183, -1.57235, -1.51220, -1.45093, -1.38978, -1.32766, -1.26472, -1.20110, -1.13669, -1.07131, -1.00477, -0.93729, -0.86837, -0.79826, -0.72645, -0.65289, -0.57713, -0.49882, -0.41726, -0.33150, -0.23963, -0.13767, -0.00547],
];

fn fast_dist_b(row: &[f32; 49], u: f32) -> f32 {
    if u <= 0.0 {
        return 0.0;
    }
    // Position on the grid in sixteenths of a decade above 0.001.
    let pos = (u.log10() + 3.0) * 16.0;
    if pos <= 0.0 {
        // Below the measured range the curve keeps its starting slope.
        let slope = (row[4] - row[0]) / 4.0;
        return (row[0] + pos * slope).exp();
    }
    let i = (pos as usize).min(47);
    let f = (pos - i as f32).min(1.0);
    (row[i] + (row[i + 1] - row[i]) * f).exp()
}

impl Unit {
    pub fn new(fx: Fx, sr: f32) -> Self {
        let mut inner = successor(fx.kind).map(|k| Box::new(Unit::new(Fx::new(k), sr)));
        if fx.kind == Kind::Loud && loud_settings().is_some() {
            inner = Some(Box::new(Unit::new(Fx::new(Kind::Maxi), sr)));
        }
        let mut u = Self { fx, inner, z: [0.0; 64], buf: vec![], pos: 0, ph: 0.0, lines: vec![], lpos: vec![], out: [0.0; 3], song_at: (0.0, 130.0), form: None, clock: 0.0, dice: 0x2545_f491_4f6c_dd1d, bank: None, conv: None, rack: None, voc: None, multi: None, split: None, maxi: None, oldvoc: None, tap: Vec::new(), tap_all: false, side: Vec::new(), side_on: false, patch: None, psola: None };
        if let Some(sub) = &u.fx.sub {
            u.patch = Some(Box::new(crate::fxpatch::State::new(sub, sr)));
        }
        match u.fx.kind {
            Kind::Plush => {
                // the ring of the loop's shifter, the loop's own delay a side, and a work bench
                u.buf = vec![[0.0; 2]; (sr * 0.1) as usize + 8];
                u.lines = vec![vec![0.0; (sr * 4.0) as usize + 8], vec![0.0; (sr * 4.0) as usize + 8], vec![]];
                u.lpos.push(0);
            }
            Kind::Delay => {
                u.buf = vec![[0.0; 2]; (sr * 6.0) as usize + 8];
                u.lines = vec![vec![0.0; (sr * 0.5) as usize + 8]];
                u.lpos = vec![0];
            }
            Kind::Echo | Kind::OldDelay => u.buf = vec![[0.0; 2]; (sr * 6.0) as usize + 8],
            Kind::Flanger | Kind::Chorus | Kind::OldFlanger => u.buf = vec![[0.0; 2]; (sr * 0.12) as usize + 8],
            // the delay knob alone reaches 1.2 s a quarter of the way up
            Kind::Stack => u.buf = vec![[0.0; 2]; (sr * 3.0) as usize + 8],
            Kind::Swarm => u.buf = vec![[0.0; 2]; (sr * 0.12) as usize + 8],
            Kind::Spread => {
                for ms in [7.3f32, 11.9, 17.1] {
                    u.lines.push(vec![0.0; (ms * sr / 1000.0) as usize + 2]);
                    u.lpos.push(0);
                }
            }
            Kind::Enhancer | Kind::Shaper => u.buf = vec![[0.0; 2]; (sr * 0.6) as usize + 8],
            Kind::BucketChorus | Kind::Amp => u.buf = vec![[0.0; 2]; (sr * 0.05) as usize + 8],
            // two bars at 40 beats a minute and up
            Kind::Warp => u.buf = vec![[0.0; 2]; (sr * 12.0) as usize + 8],
            Kind::Bank => u.bank = Some(Box::default()),
            Kind::Voc => u.voc = Some(Box::default()),
            Kind::Multi => u.multi = Some(Box::default()),
            Kind::Split => u.split = Some(Box::default()),
            Kind::Maxi => u.maxi = Some(Box::default()),
            Kind::OldVoc => u.oldvoc = Some(Box::default()),
            Kind::Rack => {
                let mut st = crate::fxrack::State::default();
                if let Some(ir) = &u.fx.ir {
                    let k = crate::conv::Kernel::new(&ir.smp, sr, 256, true);
                    st.conv = Some(Box::new(crate::conv::State::new(std::sync::Arc::new(k))));
                }
                u.rack = Some(Box::new(st));
            }
            Kind::Conv => {
                if let Some(ir) = &u.fx.ir {
                    let extra = u.fx.p.get(4..4 + crate::convset::EXTRA).unwrap_or(&crate::convset::EXTRA_DEF);
                    let tail = u.fx.raw.as_ref().and_then(|b| conv_tail_of(b)).map(|t| t.0);
                    let shaped = crate::convset::shape(&ir.smp, extra, tail.as_ref());
                    let k = crate::conv::Kernel::new(&shaped, sr, 512, extra[0] >= 0.5);
                    u.conv = Some(Box::new(crate::conv::State::new(std::sync::Arc::new(k))));
                }
            }
            Kind::FreqShift => u.buf = vec![[0.0; 2]; 36],
            Kind::Relooper => {
                u.lines.push(vec![0.0; (sr * 20.0) as usize + 8]);
                u.lpos.push(12345);
            }
            Kind::MorphEq => {
                u.lines.push(vec![0.0; MEQ_STATE]);
                u.lines.push(vec![-1.0; MEQ_BANDS * 2]);
            }
            Kind::Tuner => {
                u.buf = vec![[0.0; 2]; (sr * 0.2) as usize + 8];
                u.lines.push(vec![0.0; 2048]);
                u.lpos.push(0);
            }
            Kind::Pad => {
                u.buf = vec![[0.0; 2]; (sr * 4.0) as usize + 8];
                for ms in PAD_MS {
                    for side in [1.0f32, 1.07] {
                        u.lines.push(vec![0.0; (ms * side * sr / 1000.0) as usize + 2]);
                        u.lpos.push(0);
                    }
                }
            }
            Kind::EchoBank => {
                for _ in 0..8 {
                    u.lines.push(vec![0.0; ((sr * 2.4) as usize + 8) * 2]);
                    u.lpos.push(0);
                }
                // filter memories: per bank two filters, three stages, two sides; then the grain clocks
                u.lines.push(vec![0.0; 8 * 24]);
                for _ in 0..8 {
                    u.lpos.push(0);
                }
            }
            Kind::GrainShift => {
                u.buf = vec![[0.0; 2]; (sr * 0.25) as usize + 8];
                for _ in 0..2 {
                    u.lines.push(vec![0.0; (sr * 8.0) as usize + 8]);
                    u.lpos.push(0);
                }
            }
            Kind::Reverb => {
                let k = sr / 44100.0;
                u.buf = vec![[0.0; 2]; (sr * 2.7) as usize + 8];
                for n in [240.0f32, 320.0, 620.0, 840.0, 320.0, 320.0, 2010.0, 1490.0, 13600.0, 13700.0, 5900.0, 4000.0, 8700.0, 8400.0] {
                    u.lines.push(vec![0.0; (n * k) as usize + 16]);
                    u.lpos.push(0);
                }
            }
            _ => {}
        }
        u
    }

    /// How long the unit can keep sounding after its input stops, in frames.
    pub fn tail(&self, cx: &Ctx) -> usize {
        if let Some(u) = &self.inner {
            let mut q = u.fx.clone();
            translate(self.fx.kind, &self.fx.p, &mut q.p, cx);
            return Unit { fx: q, inner: None, z: [0.0; 64], buf: vec![], pos: 0, ph: 0.0, lines: vec![], lpos: vec![], out: [0.0; 3], song_at: (0.0, 130.0), form: None, clock: 0.0, dice: 0x2545_f491_4f6c_dd1d, bank: None, conv: None, rack: None, voc: None, multi: None, split: None, maxi: None, oldvoc: None, tap: Vec::new(), tap_all: false, side: Vec::new(), side_on: false, patch: None, psola: None }.tail(cx);
        }
        let p = &self.fx.p;
        if let Some(c) = &self.conv {
            return c.kernel.frames;
        }
        if let Some(r) = &self.rack {
            return r.conv.as_ref().map_or(0, |c| c.kernel.frames) + (cx.sr * 0.25) as usize;
        }
        match self.fx.kind {
            Kind::Delay => {
                let fb = (p[3] / 128.0).clamp(0.0, 0.98);
                let time = p[4] / 48.0 * cx.step;
                // until the echoes are 60 dB down
                let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                (time * (n + 1.0)) as usize + 4096 + if p[5] != 0.0 { 24000 } else { 0 }
            }
            Kind::Echo => {
                let fb = (p[11] / 6000.0).clamp(0.0, 0.97);
                let time = echo_time(p, cx);
                let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                (time * (n + 1.0)) as usize + 4096
            }
            Kind::Flanger | Kind::Chorus | Kind::Stack | Kind::OldFlanger | Kind::Swarm | Kind::BucketChorus | Kind::BandSplit => (cx.sr * 0.5) as usize,
            Kind::EchoBank => {
                // the longest ring among the banks that are switched on
                let mut most = 0usize;
                for b in 0..8 {
                    let q = &p[4 + b * 20..24 + b * 20];
                    if (q[19] as i32 >> 8) & 1 == 0 {
                        continue;
                    }
                    let fb = (q[6] / 128.0 * p[3] / 128.0).clamp(0.0, 0.97);
                    let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                    most = most.max((bank_time(q, p[164], cx) * (n + 1.0)) as usize);
                }
                most.min((cx.sr * 60.0) as usize) + (cx.sr * 0.33) as usize + 4096
            }
            Kind::GrainShift => {
                let fb = (p[9] / 100.0).clamp(0.0, 0.97);
                let time = shift_delay(p, cx);
                let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                (time * (n + 1.0)) as usize + (cx.sr * 0.3) as usize
            }
            Kind::Spread | Kind::Transient | Kind::Tuner => (cx.sr * 0.25) as usize,
            Kind::Peak | Kind::Voc | Kind::OldVoc | Kind::Multi => (cx.sr * 3.0) as usize,
            Kind::Split => (cx.sr * 6.0) as usize,
            Kind::Warp => (cx.step * 32.0) as usize + 64,
            Kind::Bank | Kind::FreeFilter | Kind::BassBoost | Kind::FreqShift | Kind::Finisher | Kind::LowLift | Kind::MorphEq => (cx.sr * 0.25) as usize,
            Kind::Phaser => (cx.sr * 1.5) as usize,
            Kind::OldDelay => {
                let fb = (p[1] / 1024.0).clamp(0.0, 0.97);
                let time = old_delay_time(p, cx);
                let echoes = if fb < 0.01 { 1.0 } else { (0.001f32.ln() / fb.ln()).min(60.0) };
                (time * (echoes + 1.0)) as usize
            }
            Kind::DcBlock => (cx.sr * 2.0) as usize,
            Kind::Squeeze | Kind::Eq7 | Kind::Emph | Kind::Amp => (cx.sr * 0.25) as usize,
            Kind::StonePhaser => cx.sr as usize,
            Kind::Relooper => (cx.sr * 4.0) as usize,
            Kind::Pad => (cx.sr * 6.0) as usize,
            Kind::Patch => self.patch.as_ref().map_or(0, |st| st.tail(cx)),
            Kind::Reverb => ((p[5] / 10.0 * 1.5 * (p[7] / 100.0).clamp(1.0, 1.6) + p[2] / 384.0 + 0.5) * cx.sr) as usize,
            Kind::Enhancer | Kind::Shaper => (cx.sr * 0.6) as usize,
            Kind::Comp | Kind::MultiComp | Kind::Maxi | Kind::Limiter | Kind::Eq | Kind::Filter | Kind::FastLp | Kind::Overdrive | Kind::Loud => (cx.sr * 0.25) as usize,
            _ => 0,
        }
    }

    /// Mixer tracks this unit listens to besides its own: (modulator, carrier), counted from 1 among the
    /// tracks routed to its insert.
    pub fn side_wants(&self) -> Option<[i32; 2]> {
        match self.fx.kind {
            Kind::Voc => Some(crate::fxvoc::selectors(&self.fx.p)),
            // a compressor set to listen to another track
            Kind::Limiter => self.fx.p.get(18).filter(|v| **v >= 1.0).map(|v| [*v as i32, 0]),
            _ => None,
        }
    }
    /// Hand over one block of a listened-to track.
    pub fn side_set(&mut self, which: usize, buf: &[[f32; 2]]) {
        if let Some(st) = &mut self.voc {
            st.side[which].clear();
            st.side[which].extend_from_slice(buf);
            st.side_on[which] = true;
        }
        if self.fx.kind == Kind::Limiter && which == 0 {
            self.side.clear();
            self.side.extend_from_slice(buf);
            self.side_on = true;
        }
    }

    /// For a patch bay standing as an instrument: where to add the sound of its layer `k` this block.
    pub fn bay_feed(&mut self, k: usize, n: usize) -> Option<&mut [[f32; 2]]> {
        self.patch.as_mut()?.feed(k, n)
    }

    /// What output `k` of a sender is sending now.
    pub fn sends(&self, k: usize) -> Option<f32> {
        match self.fx.kind {
            // a board has as many outputs as it has controls
            Kind::Surface => self.fx.p.get(k).copied(),
            _ => self.out.get(k).copied(),
        }
    }

    /// The running patch bay, when this unit is one.
    pub fn bay_state(&mut self) -> Option<&mut crate::fxpatch::State> {
        self.patch.as_deref_mut()
    }

    /// The plush reverb: the room reverb's tank with a pitched loop round it, a hold, a follower that turns the
    /// tail down or up with the playing, and a bell on the tail.
    fn plush(&mut self, io: &mut [[f32; 2]], cx: &Ctx) {
        let sr = cx.sr;
        let p = &self.fx.p;
        let lvl = |v: f32| if v <= -600.0 { 0.0 } else { db((v - 746.0) * 0.05) };
        let on = p[3] >= 0.5;
        // the tail's level follows the tank's own level law, as it did when the tank set it
        let (g_dry, g_wet) = (knob_amp(lvl(p[32])), if on { (lvl(p[0]) * lvl(p[33])).powf(1.5) } else { 0.0 });
        let hold = p[13] >= 0.5;
        let beat = cx.step * 4.0;
        let sw = p[39] as i32;
        let loop_on = p[15] >= 0.5 && p[19] > 0.0;
        let loop_gain = (p[19] / 1000.0).clamp(0.0, 1.0);
        let loop_mix = (p[22] / 1000.0).clamp(0.0, 1.0);
        let n_line = self.lines[0].len();
        let loop_len = (if sw & 0x100 != 0 { p[18] / 192.0 * beat } else { p[17] / 10_000.0 * sr } as usize).clamp(16, n_line - 2);
        let ratio = (p[16] / 120.0).exp2();
        let (g_hc, g_lc) = (svf_g(p[20].clamp(700.0, 5000.0) * 4.0, sr), svf_g(p[21].clamp(20.0, 3000.0), sr));
        let f_scale = (p[23] / 1000.0).clamp(-1.0, 1.0);
        let f_thr = p[24] / 100.0;
        let coef = |ms: f32| 1.0 - (-1.0 / (ms.max(0.1) * 0.001 * sr)).exp();
        let (f_att, f_rel, f_smooth) = (coef(p[25] / 10.0), coef(p[26]), coef(p[31] / 10.0));
        let g_fl = svf_g(p[30].clamp(20.0, 1000.0), sr);
        let bell = db(p[35] / 100.0) - 1.0;
        let g_pk = svf_g(p[34].clamp(40.0, 8000.0), sr);
        let k_pk = (p[36] / 1000.0).clamp(0.05, 4.0);
        let Some(inner) = self.inner.as_mut() else { return };
        translate(Kind::Plush, &self.fx.p, &mut inner.fx.p, cx);
        // the tank gives its tail alone, at its own level; the levels are set out here, after the loop
        inner.fx.p[10] = 0.0;
        inner.fx.p[11] = 32.0;
        inner.fx.p[12] = 64.0;
        if hold {
            inner.fx.p[5] = 1.0e7;
            inner.fx.p[6] = 221.0;
            inner.fx.p[7] = 100.0;
        }
        let z = &mut self.z;
        let n = self.buf.len();
        let grain = (0.06 * sr).min(n as f32 - 8.0);
        let (line, rest) = self.lines.split_at_mut(2);
        let work = &mut rest[0];
        let step = 32.min(loop_len);
        for chunk in io.chunks_mut(step) {
            work.resize(chunk.len() * 2, 0.0);
            // what goes into the tank: the playing, unless held, and what the loop brings round
            let mut feed = [[0.0f32; 2]; 32];
            let lp = self.lpos[0];
            for (i, f) in chunk.iter().enumerate() {
                work[i * 2] = f[0];
                work[i * 2 + 1] = f[1];
                let back = (lp + i + n_line - loop_len) % n_line;
                let live = if hold { 0.0 } else { 1.0 };
                feed[i] = [f[0] * live + line[0][back], f[1] * live + line[1][back]];
            }
            let wet = &mut feed[..chunk.len()];
            inner.process(wet, cx);
            for (i, f) in chunk.iter_mut().enumerate() {
                let dry = [work[i * 2], work[i * 2 + 1]];
                let w = wet[i];
                // the loop: the tail moved in pitch, thinned top and bottom, sent round again
                let mut round = [0.0f32; 2];
                if loop_on {
                    self.buf[self.pos] = w;
                    let mut y = [0f32; 2];
                    for k in 0..2 {
                        let ph = (self.ph + k as f32 * 0.5).fract();
                        let win = 0.5 - 0.5 * (ph * TAU).cos();
                        let at = self.pos as f32 + n as f32 - (ph * grain + 1.0);
                        let j = at as usize;
                        let fr = at - j as f32;
                        for c in 0..2 {
                            y[c] += (self.buf[j % n][c] * (1.0 - fr) + self.buf[(j + 1) % n][c] * fr) * win;
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    self.ph = (self.ph + (1.0 - ratio) / grain).rem_euclid(1.0);
                    for c in 0..2 {
                        let v = w[c] * (1.0 - loop_mix) + y[c] * loop_mix;
                        let low = svf(&mut z[c * 4..], v, g_hc, 1.414).0;
                        let high = svf(&mut z[c * 4 + 2..], low, g_lc, 1.414).2;
                        // it can never run away: past full level the loop gives no more
                        round[c] = (high * loop_gain).tanh();
                    }
                }
                let at = (lp + i) % n_line;
                line[0][at] = round[0];
                line[1][at] = round[1];
                // the follower: how loud the playing is over its threshold moves the tail
                let side = svf(&mut z[8..], (dry[0] + dry[1]) * 0.5, g_fl, 1.414).2.abs();
                z[10] += (side - z[10]) * if side > z[10] { f_att } else { f_rel };
                let over = ((20.0 * z[10].max(1e-6).log10() - f_thr) / 24.0).clamp(0.0, 1.0);
                z[11] += ((1.0 + f_scale * over) - z[11]) * f_smooth;
                let follow = if f_scale == 0.0 { 1.0 } else { z[11] };
                for c in 0..2 {
                    let band = svf(&mut z[12 + c * 2..], w[c], g_pk, k_pk).1;
                    let tail = w[c] + bell * k_pk * band;
                    f[c] = dry[c] * g_dry + tail * g_wet * follow;
                }
            }
            self.lpos[0] = (lp + chunk.len()) % n_line;
        }
    }

    pub fn process(&mut self, io: &mut [[f32; 2]], cx: &Ctx) {
        if self.fx.kind == Kind::Loud {
            if let (Some(u), Some(set)) = (&mut self.inner, loud_settings()) {
                let q = &set[(self.fx.p[0] as usize).min(3)];
                if u.fx.p.len() == q.len() {
                    u.fx.p.copy_from_slice(q);
                } else {
                    u.fx.p = q.clone();
                }
                // the one knob is the maximiser's band mix
                u.fx.p[5] = self.fx.p[1].clamp(0.0, 1000.0);
                u.process(io, cx);
                return;
            }
        }
        if self.fx.kind == Kind::Plush && self.inner.is_some() && self.lines.len() == 3 {
            self.plush(io, cx);
            return;
        }
        if let Some(u) = &mut self.inner {
            translate(self.fx.kind, &self.fx.p, &mut u.fx.p, cx);
            u.process(io, cx);
            return;
        }
        if let Some(h) = &self.fx.ext {
            h.0.process(io, &[], &cx.clock, cx.sr);
            return;
        }
        let sr = cx.sr;
        let p = &self.fx.p;
        let z = &mut self.z;
        match self.fx.kind {
            Kind::Balance => {
                // both knobs swept: the level is the usual knob curve, and the pan lifts the near side to
                // +3 dB while it takes the far side down to nothing, along the two measured curves
                let x = (p[0] / 128.0).clamp(-1.0, 1.0);
                let (near, far) = (curve_at(&BALANCE_NEAR, x.abs()), curve_at(&BALANCE_FAR, x.abs()));
                let (l, r) = if x < 0.0 { (near, far) } else { (far, near) };
                let g = knob_gain(p[1] / 256.0);
                for f in io.iter_mut() {
                    f[0] *= l * g;
                    f[1] *= r * g;
                }
            }
            Kind::SoftClip => {
                // Measured: straight up to the threshold, then an exponential approach to
                // full scale; the output level follows (11^v - 1) / 10 and the result is
                // clipped at full scale.
                let t = (p[0] / 128.0).clamp(0.0, 0.9999);
                let post = (11f32.powf(p[1] / 128.0) - 1.0) / 10.0;
                let room = 1.0 - t;
                for f in io.iter_mut() {
                    for x in f.iter_mut() {
                        let a = x.abs();
                        if a > t {
                            *x = (t + room * (1.0 - (-(a - t) / room).exp())).copysign(*x);
                        }
                        *x = (*x * post).clamp(-1.0, 1.0);
                    }
                }
            }
            Kind::FastDist => {
                // Measured against reference renders: every level knob follows
                // (11^v - 1) / 10; kind A clips at full scale, then a power curve
                // with a straight start below -25 dB; kind B is a measured table.
                let lev = |v: f32| (11f32.powf(v / 128.0) - 1.0) / 10.0;
                let pre = lev(p[0]);
                let t = p[1].round().clamp(1.0, 10.0);
                let b = p[2] >= 0.5;
                let mix = lev(p[3]).clamp(0.0, 1.0);
                let post = lev(p[4]);
                let ceil = 10f32.powf(-0.045 * (t - 1.0));
                let expo = 1.1 - t / 10.0;
                let knee = 10f32.powf(-1.25);
                let slope = knee.powf(expo - 1.0);
                let row = &FAST_DIST_B[t as usize - 1];
                for f in io.iter_mut() {
                    for x in f.iter_mut() {
                        let d = (*x * pre).clamp(-1.0, 1.0);
                        let u = d.abs();
                        let w = if b {
                            fast_dist_b(row, u)
                        } else if u <= knee {
                            ceil * slope * u
                        } else {
                            ceil * u.powf(expo)
                        };
                        // The dry part passes the output level twice.
                        *x = (*x * (1.0 - mix) * post + w.copysign(d) * mix) * post;
                    }
                }
            }
            Kind::Enhancer => {
                // All laws measured against reference renders.
                let (pl, pr) = balance(p[0] / 128.0);
                let g = (11f32.powf(p[1] / 256.0) - 1.0) / 10.0;
                // Separation: each side takes c of the other, then both drop by c/2.
                let c = 0.75 * p[2] / 96.0;
                let keep = 1.0 - c * 0.5;
                // Delay of one side: exponential in the knob, 18750 samples at 44.1 kHz at full.
                let r = (p[3].abs() / 500.0).min(1.0);
                let off = ((851.2f64.powf(r as f64) - 1.0) / 850.2 * 18750.0 * sr as f64 / 44100.0).round() as usize;
                let off = off.min(self.buf.len().saturating_sub(2));
                let left_late = p[3] < 0.0;
                let pan_first = p[4] >= 0.5;
                let inv = p[5] as i32;
                let n = self.buf.len();
                for f in io.iter_mut() {
                    let (mut l, mut r) = (f[0], f[1]);
                    if pan_first {
                        l *= pl;
                        r *= pr;
                    }
                    let (a, b) = ((l + c * r) * keep, (r + c * l) * keep);
                    l = a;
                    r = b;
                    if !pan_first {
                        l *= pl;
                        r *= pr;
                    }
                    self.buf[self.pos] = [l, r];
                    if off > 0 {
                        let d = self.buf[(self.pos + n - off) % n];
                        if left_late {
                            l = d[0];
                        } else {
                            r = d[1];
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    l *= g;
                    r *= g;
                    if inv == 1 {
                        l = -l;
                    } else if inv == 2 {
                        r = -r;
                    }
                    *f = [l, r];
                }
            }
            Kind::Loud => {
                // three bands, each pushed into a soft ceiling; the amount sets how hard
                let a = (p[1] / 1000.0).clamp(0.0, 1.0);
                let w: [f32; 3] = match p[0] as i32 {
                    1 => [1.25, 0.85, 1.1],
                    2 => [0.85, 1.0, 1.3],
                    3 => [1.3, 1.1, 1.3],
                    _ => [1.0, 1.0, 1.0],
                };
                let g1 = svf_g(180.0, sr);
                let g2 = svf_g(3200.0, sr);
                let drive = [db(a * 14.0 * w[0]), db(a * 14.0 * w[1]), db(a * 14.0 * w[2])];
                let out = db(-a * 3.0);
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let x = f[ch];
                        let (lo, _, _) = svf(&mut z[ch * 4..], x, g1, 2.0);
                        let (mid, _, hi) = svf(&mut z[ch * 4 + 2..], x - lo, g2, 2.0);
                        let y = (lo * drive[0]).tanh() + (mid * drive[1]).tanh() + (hi * drive[2]).tanh();
                        f[ch] = y * out;
                    }
                }
            }
            Kind::FastLp => {
                // measured: two one-pole stages with feedback around them and no zeros. With the resonance
                // down the pair falls to a single pole; the cutoff sweep gives the square of the stage
                // coefficient as an exponential of the knob, and the resonance sets the pole radius.
                let x = (p[0] / 1024.0).clamp(0.0, 1.0) as f64;
                let fc = (0.001_823_36 * (6.310_77 * x).exp() - 0.001_595_9).clamp(1e-6, 0.9998).sqrt();
                let rad = 1.001_63 * (1.0 - (-4.0 * (p[1] / 1024.0).clamp(0.0, 1.0) as f64).exp());
                let fb = (rad / (1.0 - fc) - 1.0 + fc) / fc;
                let open = p[0] >= 1024.0 && p[1] <= 0.0;
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let (mut b0, mut b1) = (z[ch * 2] as f64, z[ch * 2 + 1] as f64);
                        b0 += fc * (f[ch] as f64 - b0 + fb * (b0 - b1));
                        b1 += fc * (b0 - b1);
                        z[ch * 2] = b0 as f32;
                        z[ch * 2 + 1] = b1 as f32;
                        if !open {
                            f[ch] = b1 as f32;
                        }
                    }
                }
            }
            Kind::Filter => {
                // measured over sweeps of each knob: the plain two-integrator loop (low-pass updated first),
                // damping 1 - 0.8 x resonance, the cutoff on an exponential knob curve and the three levels on
                // the usual level knob curve.
                let fc = (26f32.powf((p[0] / 1024.0).clamp(0.0, 1.0)) - 1.0) / 25.0;
                let q = 1.0 - 0.8 * (p[1] / 1024.0).clamp(0.0, 1.0);
                let lv = |v: f32| knob_gain(v / 1024.0);
                let (l, b, h) = (lv(p[2]), lv(p[3]), lv(p[4]));
                let twice = p[5] >= 0.5;
                // fully open low-pass is a plain wire
                let open = p[0] >= 1024.0 && b == 0.0 && h == 0.0 && l == 1.0;
                let run = |z: &mut [f32], x: f32| {
                    z[0] += fc * z[1];
                    let hp = x - z[0] - q * z[1];
                    z[1] += fc * hp;
                    z[0] * l + z[1] * b + hp * h
                };
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let mut y = run(&mut z[ch * 4..], f[ch]);
                        if twice {
                            y = run(&mut z[ch * 4 + 2..], y);
                        }
                        if !open {
                            f[ch] = y;
                        }
                    }
                }
            }
            Kind::Overdrive => {
                // Measured against reference renders sample by sample. The colour stage comes first and
                // stays clean: a state filter (damping 1, coefficient the cube of the knob) whose input
                // plus scaled high output is blended with its band output by the band knob. The pre amp
                // (1 + 25 r^3, the x100 switch multiplying the added part) feeds a second state filter
                // with coefficient 1/4; its low, band and high outputs each pass a cubic soft clip
                // and are summed. The squared output level and a hard limit at full scale come next,
                // then two equal one-pole low-passes (present even with the knob down).
                let band = (p[0] / 10000.0).clamp(0.0, 1.0);
                let fc = (p[1] / 10000.0).clamp(0.0, 1.0).powi(3);
                let r = p[2] / 10000.0;
                let amp = 1.5609 * (1.0 + 25.0 * r * r * r * if p[3] >= 0.5 { 100.0 } else { 1.0 });
                let a = 0.8 - 0.7 * (p[4] / 10000.0).clamp(0.0, 1.0);
                let post = (p[5] / 10000.0).powi(2);
                let soft = |u: f32| {
                    let u = u.clamp(-1.5, 1.5);
                    u - u * u * u * (4.0 / 27.0)
                };
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let z = &mut z[ch * 6..ch * 6 + 6];
                        let x = f[ch];
                        z[0] += fc * z[1];
                        let hc = x - z[0] - z[1];
                        z[1] += fc * hc;
                        let v = ((1.0 - band) * (x + fc * hc) + band * z[1]) * amp;
                        z[2] += 0.25 * z[3];
                        let h = v - z[2] - z[3];
                        z[3] += 0.25 * h;
                        let y = ((soft(z[2]) + soft(h) + soft(z[3])) * post).clamp(-1.0, 1.0);
                        z[4] += a * (y - z[4]);
                        z[5] += a * (z[4] - z[5]);
                        f[ch] = z[5];
                    }
                }
            }
            Kind::AutoPan => {
                // measured against renders: the panned-from side also feeds the other one, the four mix
                // levels travel in straight lines tick by tick from nothing, and the wave (when it has a
                // target: 1 the pan, 2 the volume) starts at rest
                let amount = (p[4] / 128.0).clamp(0.0, 1.0);
                let step = auto_pan_hz(p[5]) / sr;
                let shape = p[2] as i32;
                let target = p[3] as i32;
                let vol = knob_gain(p[1] / 256.0);
                let pan = (p[0] - 128.0) / 128.0;
                let tick = (cx.step / 24.0).max(1.0);
                let mut want = [0f32; 4];
                for f in io.iter_mut() {
                    if z[8] < 0.5 {
                        let ph = (self.ph + step * tick).fract();
                        let lfo = match shape {
                            1 => 1.0 - 4.0 * ((ph + 0.25).fract() - 0.5).abs(),
                            2 => {
                                if ph < 0.5 {
                                    1.0
                                } else {
                                    -1.0
                                }
                            }
                            _ => (ph * TAU).sin(),
                        } * amount;
                        let (mut pn, mut v) = (pan, vol);
                        match target {
                            1 => pn = (pn + lfo).clamp(-1.0, 1.0),
                            2 => v = (v + 0.5 * lfo).max(0.0),
                            _ => {}
                        }
                        let a = pn.abs();
                        let (near, cross, far) = ((1.0 - 0.5 * a) * v, 0.5 * a * v, (1.0 - a) * v);
                        want = if pn <= 0.0 { [near, cross, 0.0, far] } else { [far, 0.0, cross, near] };
                    }
                    glide(&mut z[..9], &want, tick, sr);
                    self.ph = (self.ph + step).fract();
                    let x = *f;
                    f[0] = x[0] * z[0] + x[1] * z[1];
                    f[1] = x[0] * z[2] + x[1] * z[3];
                }
            }
            Kind::Delay => {
                // measured against renders: whole-sample times, the cut filter on everything entering the line,
                // the stereo offset as one extra wait on one side, the level knobs on the common knob law
                let n = self.buf.len();
                let (il, ir) = balance(p[0] / 128.0);
                let iv = knob_amp(p[1] / 128.0);
                let dry = knob_amp(p[2] / 128.0);
                let fb = (p[3] / 128.0).clamp(0.0, 1.0);
                let m = self.lines[0].len();
                let off = ((0.5 * sr * (1001f32.powf(p[5].abs() / 512.0) - 1.0) / 1000.0).round() as usize).min(m - 1);
                let time = ((p[4] / 48.0 * cx.step).round() as usize).clamp(1, n - 2);
                let side = if p[5] < 0.0 { 0 } else { 1 };
                let mode = p[6] as i32;
                let a = ((21f32.powf((p[7] / 128.0).clamp(0.0, 1.0)) - 1.0) / 20.0).clamp(0.0, 1.0);
                for f in io.iter_mut() {
                    let mut x = [f[0] * il * iv, f[1] * ir * iv];
                    // the stereo offset holds one side's input back before it enters the line
                    let lp = self.lpos[0];
                    self.lines[0][lp] = x[side];
                    x[side] = self.lines[0][(lp + m - off) % m];
                    self.lpos[0] = (lp + 1) % m;
                    let back = self.buf[(self.pos + n - time) % n];
                    let [l, r] = back;
                    // ping pong: each side feeds the other
                    let into = if mode == 2 { [x[1] + back[1] * fb, x[0] + back[0] * fb] } else { [x[0] + back[0] * fb, x[1] + back[1] * fb] };
                    z[0] += a * (into[0] - z[0]);
                    z[1] += a * (into[1] - z[1]);
                    self.buf[self.pos] = [z[0], z[1]];
                    self.pos = (self.pos + 1) % n;
                    *f = if mode == 1 { [f[0] * dry + r, f[1] * dry + l] } else { [f[0] * dry + l, f[1] * dry + r] };
                }
                for v in z[..2].iter_mut() {
                    if v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::Comp => {
                // measured against renders: 64 samples of delay, a 3 Hz high-pass, a peak detector that falls
                // over about a millisecond, the gain smoothed as a plain factor, and a look-ahead limiter last
                let thr = (p[0] / 10.0) as f64;
                let slope = 1.0 / (p[1] as f64 / 10.0).max(0.4) - 1.0;
                let makeup = 10f64.powf(p[2] as f64 / 200.0);
                let srd = sr as f64;
                let coef = |ms: f64| if ms <= 0.0 { 1.0 } else { 1.0 - (-std::f64::consts::LN_10 / (ms * 0.001 * srd)).exp() };
                let ka = coef(p[3] as f64 / 10.0);
                let kr = coef((p[4] as f64).max(1.0));
                let kd = coef(1.0);
                let tcr = (p[5] as i32).clamp(0, 7) >= 4;
                let ty = (p[5] as i32).clamp(0, 7) % 4;
                let half = match ty {
                    1 => 6.0,
                    3 => 15.0,
                    _ => 0.0,
                };
                // the vintage curve: a cubic from 7 dB under the threshold, held at its top
                let vint = {
                    let sv = (-slope).clamp(0.5, 0.9667);
                    let pts = [0.5, 0.75, 0.9, 0.9667];
                    let lag = |v: [f64; 4]| {
                        let mut y = 0.0;
                        for i in 0..4 {
                            let mut w = v[i];
                            for j in 0..4 {
                                if j != i {
                                    w *= (sv - pts[j]) / (pts[i] - pts[j]);
                                }
                            }
                            y += w;
                        }
                        y * -slope
                    };
                    let a = lag([0.0915, 0.0998, 0.1108, 0.1234]);
                    let b = lag([0.04316, 0.04314, 0.04213, 0.03960]);
                    let c = lag([-0.001068, -0.000979, -0.000842, -0.000669]);
                    let top = if c.abs() > 1e-12 { (-2.0 * b - (4.0 * b * b - 12.0 * a * c).max(0.0).sqrt()) / (6.0 * c) } else { 0.0 };
                    (a, b, c, top)
                };
                let hp = (-2.0 * std::f64::consts::PI * 3.0 / srd).exp();
                let hg = (1.0 + hp) / 2.0;
                const LAG: usize = 64;
                const CEIL: f64 = 0.988;
                if self.buf.len() != LAG * 2 {
                    // the second half keeps one minus the limiter's recent targets for its smoothing window
                    self.buf = vec![[0.0; 2]; LAG * 2];
                    self.pos = 0;
                }
                let hold = (160.0 * srd / 44100.0) as f32;
                let rise = 4.6e-6 * (44100.0 / srd).powi(2);
                for f in io.iter_mut() {
                    let mut x = [0f64; 2];
                    for ch in 0..2 {
                        let v = f[ch] as f64;
                        let y = hg * (v - z[ch] as f64) + hp * z[2 + ch] as f64;
                        z[ch] = f[ch];
                        z[2 + ch] = y as f32;
                        x[ch] = y;
                    }
                    let v = x[0].abs().max(x[1].abs());
                    let mut e = z[4] as f64;
                    e = if v > e { v } else { e + (v - e) * kd };
                    z[4] = e as f32;
                    let over = 20.0 * e.max(1e-9).log10() - thr;
                    let red = if ty == 2 {
                        let o = (over + 7.0).clamp(0.0, vint.3.max(0.0));
                        -(vint.0 * o + vint.1 * o * o + vint.2 * o * o * o)
                    } else if half > 0.0 && over.abs() < half {
                        slope * (over + half).powi(2) / (4.0 * half)
                    } else if over > 0.0 {
                        slope * over
                    } else {
                        0.0
                    };
                    let target = 10f64.powf(red / 20.0);
                    // z[5] holds one minus the smoothed gain so silence starts at unity
                    let mut g = 1.0 - z[5] as f64;
                    // the second four types: a slow follower trails the gain downwards at the release pace and
                    // jumps up with it; the release runs up to ten times faster the further the gain is under it
                    // (measured), so short peaks let go quickly and long passages slowly
                    let mut slow = 1.0 - z[10] as f64;
                    let boost = if tcr { 1.0 + 9.0 * (slow - g).max(0.0) } else { 1.0 };
                    g += (target - g) * if target < g { ka } else { (kr * boost).min(1.0) };
                    slow = if g > slow { g } else { slow + (g - slow) * kr };
                    z[10] = (1.0 - slow) as f32;
                    z[5] = (1.0 - g) as f32;
                    let c = [x[0] * g * makeup, x[1] * g * makeup];
                    // limiter: the peak is held, the gain drops ahead of it and comes back slowly at first
                    let pk = c[0].abs().max(c[1].abs()) as f32;
                    if pk >= z[7] {
                        z[7] = pk;
                        z[8] = 0.0;
                    } else {
                        z[8] += 1.0;
                        if z[8] > hold {
                            z[7] = pk;
                            z[8] = 0.0;
                        }
                    }
                    let want = if z[7] as f64 > CEIL { CEIL / z[7] as f64 } else { 1.0 };
                    let mut l = 1.0 - z[6] as f64;
                    if want <= l + 1e-6 {
                        l = want;
                        z[9] = 0.0;
                    } else {
                        z[9] += 1.0;
                        l += (want - l) * (rise * z[9] as f64).min(1.0);
                    }
                    z[6] = (1.0 - l) as f32;
                    self.buf[LAG + self.pos][0] = z[6];
                    let mut acc = 0.0;
                    for k in 0..LAG {
                        let age = (self.pos + LAG - k) % LAG;
                        let w = (age as f64 - 3.5).min(LAG as f64 - 0.5 - age as f64).clamp(0.0, 12.0) / 12.0;
                        acc += w * self.buf[LAG + k][0] as f64;
                    }
                    let l = 1.0 - acc / 48.0;
                    let d = self.buf[self.pos];
                    self.buf[self.pos] = [c[0] as f32, c[1] as f32];
                    self.pos = (self.pos + 1) % LAG;
                    f[0] = (d[0] as f64 * l) as f32;
                    f[1] = (d[1] as f64 * l) as f32;
                }
            }
            Kind::Shaper => {
                // measured: each of the four levels follows (3^x - 1) / 2 with its sign kept, and the delay
                // knob runs to half a second along (1000^x - 1) / 999
                let lv = |v: f32| (3f32.powf((v / 12800.0).abs()) - 1.0) / 2.0 * v.signum();
                let want = [lv(p[0]), lv(p[1]), lv(p[2]), lv(p[3])];
                let tick = cx.step / 24.0;
                let n = self.buf.len();
                let travel = (1000f32.powf((p[4].abs() / 4096.0).min(1.0)) - 1.0) / 999.0;
                let off = ((travel * 0.5 * sr).round()).min(n as f32 - 2.0) as usize;
                let left_late = p[4] < 0.0;
                // the phase knob: two equal first-order all-pass stages on one side (the right for values above
                // nothing), whose corner falls from the top of the band along a straight line in the logarithm
                // towards a floor of a few hertz (measured at ten points)
                let turn = (p[5].abs() / 4096.0).min(1.0);
                let ap = if turn > 0.0 {
                    let fc = 5.5 + 21_836.0 * (-8.488 * turn).exp();
                    let t = (std::f32::consts::PI * fc.min(sr * 0.49) / sr).tan();
                    Some(((1.0 - t) / (1.0 + t), if p[5] > 0.0 { 1 } else { 0 }))
                } else {
                    None
                };
                for f in io.iter_mut() {
                    let x = *f;
                    // the four levels start from nothing and travel in straight lines, tick by tick (measured)
                    glide(&mut z[..9], &want, tick, sr);
                    let (rl, ll, rr, lr) = (z[0], z[1], z[2], z[3]);
                    let mut y = [x[0] * ll + x[1] * rl, x[1] * rr + x[0] * lr];
                    self.buf[self.pos] = y;
                    if off > 0 {
                        let d = self.buf[(self.pos + n - off) % n];
                        if left_late {
                            y[0] = d[0];
                        } else {
                            y[1] = d[1];
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    if let Some((a, side)) = ap {
                        let mut v = y[side];
                        for k in [10, 12] {
                            let o = -a * v + z[k] + a * z[k + 1];
                            z[k] = v;
                            z[k + 1] = o;
                            v = o;
                        }
                        y[side] = v;
                    }
                    *f = y;
                }
            }
            Kind::OldEq | Kind::OldReverb | Kind::Plush | Kind::Thru | Kind::Gm | Kind::Hosted | Kind::Notes => {}
            Kind::Conv => {
                if let Some(st) = &mut self.conv {
                    // a normalized impulse lands this far under unit strength (measured on one render)
                    let level = if p.get(4).is_some_and(|v| *v >= 0.5) { 0.4163 } else { 1.0 };
                    let (dry, wet) = (p[0] / 1000.0, p[1] / 1000.0 * level);
                    let (si, sw) = (1.0 - p[2] / 1000.0, 1.0 - p[3] / 1000.0);
                    for f in io.iter_mut() {
                        let (m, s) = ((f[0] + f[1]) * 0.5, (f[0] - f[1]) * 0.5 * si);
                        let y = st.tick([m + s, m - s]);
                        let (ym, ys) = ((y[0] + y[1]) * 0.5, (y[0] - y[1]) * 0.5 * sw);
                        f[0] = f[0] * dry + (ym + ys) * wet;
                        f[1] = f[1] * dry + (ym - ys) * wet;
                    }
                }
            }
            Kind::Curve => {
                if p.len() < 4 + CURVE_N {
                    return;
                }
                let pre = ((p[0] - 128.0) / 32.0).exp2();
                let mix = (p[1] / 128.0).clamp(0.0, 1.0);
                let post = p[2] / 128.0;
                let two = p[3] >= 0.5;
                let table = &p[4..4 + CURVE_N];
                let top = (CURVE_N - 1) as f32;
                let read = |x: f32| {
                    let at = x.clamp(0.0, 1.0) * top;
                    let i = (at as usize).min(CURVE_N - 2);
                    table[i] + (table[i + 1] - table[i]) * (at - i as f32)
                };
                for f in io.iter_mut() {
                    for x in f.iter_mut() {
                        let d = *x * pre;
                        let w = if two { read(d * 0.5 + 0.5) * 2.0 - 1.0 } else { read(d.abs()) * d.signum() };
                        *x = (*x * (1.0 - mix) + w * mix) * post;
                    }
                }
            }
            Kind::Maxi => {
                if let Some(st) = &mut self.maxi {
                    crate::fxmaxi::process(p, st, io, cx);
                }
            }
            Kind::Pad => {
                if p[1] >= 5000.0 {
                    return;
                }
                let which = (p[0] / 10000.0 * 11.0).round() as i32;
                let mix = (p[2] / 10000.0).clamp(0.0, 1.0);
                let (gin, gout) = (p[10] / 10000.0, p[11] / 5000.0);
                // the sweep: beats in a cycle from the rate switches, sped up by the multiplier
                let beats = 8.0 / ((p[5] / 10000.0 * 6.0).round()).exp2();
                let cycle = (beats * cx.step * 4.0 / (1.0 + p[9] / 10000.0 * 7.0)).max(16.0);
                let shape = (p[6] / 10000.0 * 2.0).round() as i32;
                let (xm, ym) = (p[7] / 5000.0 - 1.0, p[8] / 5000.0 - 1.0);
                let n = self.buf.len();
                for f in io.iter_mut() {
                    let ph = self.ph;
                    let l = match shape {
                        0 => (ph * TAU).sin(),
                        1 => 2.0 * ph - 1.0,
                        _ => if ph < 0.5 { 1.0 } else { -1.0 },
                    };
                    let x = (p[3] / 10000.0 + xm * l * 0.5).clamp(0.0, 1.0);
                    let y = (p[4] / 10000.0 + ym * l * 0.5).clamp(0.0, 1.0);
                    let dry = [f[0] * gin, f[1] * gin];
                    let mut w = dry;
                    // z[40] is the unit's own slow sweep, z[41] a counter
                    z[40] = (z[40] + 0.3 / sr).fract();
                    let slow = 0.5 - 0.5 * (z[40] * TAU).cos();
                    match which {
                        0 => {
                            let drive = 1.0 + x * x * 60.0;
                            let a = 1.0 - (-TAU * 400.0 * 50f32.powf(y) / sr).exp();
                            for c in 0..2 {
                                let v = (dry[c] * drive).tanh() / drive.sqrt();
                                z[c] += (v - z[c]) * a;
                                w[c] = z[c];
                            }
                        }
                        1 => {
                            let steps = (16.0 - x * 14.0).exp2();
                            let hold = 1.0 + y * y * 60.0;
                            z[41] += 1.0;
                            if z[41] >= hold {
                                z[41] -= hold;
                                z[0] = (dry[0] * steps).round() / steps;
                                z[1] = (dry[1] * steps).round() / steps;
                            }
                            w = [z[0], z[1]];
                        }
                        2 => {
                            let d = (0.5 + slow * x * 8.0) * 0.001 * sr;
                            let at = self.pos as f32 + n as f32 - d;
                            let i = at as usize;
                            let fr = at - i as f32;
                            for c in 0..2 {
                                let v = self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                                self.buf[self.pos][c] = dry[c] + v * y * 0.9;
                                w[c] = (dry[c] + v) * 0.7;
                            }
                        }
                        3 => {
                            let g = svf_g(300.0 * 12f32.powf(slow * x), sr);
                            let a = (1.0 - g) / (1.0 + g);
                            for c in 0..2 {
                                let mut v = dry[c] + z[c * 5 + 4] * y * 0.9;
                                for k in 0..4 {
                                    let o = z[c * 5 + k] - a * v;
                                    z[c * 5 + k] = v + a * o;
                                    v = o;
                                }
                                z[c * 5 + 4] = v;
                                w[c] = (dry[c] + v) * 0.7;
                            }
                        }
                        4 => {
                            let low = x < 0.5;
                            let hz = if low { 80.0 * 250f32.powf(x * 2.0) } else { 20.0 * 500f32.powf(x * 2.0 - 1.0) };
                            let g = svf_g(hz, sr);
                            let k = 2.0 - y * 1.9;
                            for c in 0..2 {
                                let (lp, _, hp) = svf(&mut z[c * 2..], dry[c], g, k);
                                w[c] = if low { lp } else { hp };
                            }
                        }
                        5 => {
                            let d = ((cx.step * 3.0) as usize).clamp(1, n - 1);
                            let a = 1.0 - (-TAU * (300.0 + 18000.0 * (1.0 - y) * (1.0 - y)) / sr).exp();
                            let old = self.buf[(self.pos + n - d) % n];
                            for c in 0..2 {
                                z[c] += (old[c] - z[c]) * a;
                                self.buf[self.pos][c] = dry[c] + z[c] * x * 0.95;
                                w[c] = dry[c] + z[c];
                            }
                        }
                        6 => {
                            let fb = 0.7 + x * 0.28;
                            for c in 0..2 {
                                let mut sum = 0.0;
                                for k in 0..4 {
                                    let li = k * 2 + c;
                                    let m = self.lines[li].len();
                                    let lp = self.lpos[li];
                                    let old = self.lines[li][lp];
                                    z[c * 4 + k] += (old - z[c * 4 + k]) * 0.6;
                                    self.lines[li][lp] = dry[c] + z[c * 4 + k] * fb;
                                    self.lpos[li] = (lp + 1) % m;
                                    sum += old;
                                }
                                let mut v = sum * 0.25;
                                for k in 4..6 {
                                    let li = k * 2 + c;
                                    let m = self.lines[li].len();
                                    let lp = self.lpos[li];
                                    let old = self.lines[li][lp];
                                    let into = v + old * 0.5;
                                    self.lines[li][lp] = into;
                                    self.lpos[li] = (lp + 1) % m;
                                    v = old - into * 0.5;
                                }
                                w[c] = dry[c] + v * y * 2.0;
                            }
                        }
                        7 => {
                            let (gl, gr) = balance(x * 2.0 - 1.0);
                            // the far ear hears it a little later
                            let late = ((x - 0.5).abs() * 2.0 * y * 0.0007 * sr) as usize;
                            self.buf[self.pos] = dry;
                            let old = self.buf[(self.pos + n - late) % n];
                            let m = (dry[0] + dry[1]) * 0.5 * y;
                            let s = [dry[0] * (1.0 - y) + m, dry[1] * (1.0 - y) + m];
                            let o = [old[0] * (1.0 - y) + (old[0] + old[1]) * 0.5 * y, old[1] * (1.0 - y) + (old[0] + old[1]) * 0.5 * y];
                            w = if x < 0.5 { [s[0] * gl, o[1] * gr] } else { [o[0] * gl, s[1] * gr] };
                        }
                        8 => {
                            let rel = 0.02 + x * x * 2.0;
                            let env = (-ph / rel).exp();
                            let g = (1.0 - y) + y * env;
                            w = [dry[0] * g, dry[1] * g];
                        }
                        9 => {
                            // the first slice of every cycle is caught and said again until the cycle ends
                            let g = ((cycle / 8.0) as usize).clamp(8, n - 1);
                            let at = (ph * cycle) as usize;
                            if at < g {
                                self.buf[at] = dry;
                            } else {
                                w = self.buf[at % g];
                            }
                        }
                        10 => {
                            let v = (x * 4.0).clamp(0.0, 3.999);
                            let (i, fr) = (v as usize, v.fract());
                            let throat = 1.4 * 0.5f32.powf(y);
                            for c in 0..2 {
                                let mut sum = 0.0;
                                for k in 0..2 {
                                    let hz = (VOWELS[i][k] * (1.0 - fr) + VOWELS[i + 1][k] * fr) * throat;
                                    let (_, bp, _) = svf(&mut z[c * 4 + k * 2..], dry[c], svf_g(hz, sr), 0.12);
                                    sum += bp * if k == 0 { 0.12 } else { 0.08 };
                                }
                                w[c] = sum;
                            }
                        }
                        _ => {
                            z[41] = (z[41] + 20.0 * 500f32.powf(x) / sr).fract();
                            let m = (z[41] * TAU).sin();
                            w = [dry[0] * m, dry[1] * m];
                        }
                    }
                    if !matches!(which, 9) {
                        self.pos = (self.pos + 1) % n;
                    }
                    self.ph = (self.ph + 1.0 / cycle).fract();
                    *f = [(dry[0] + (w[0] - dry[0]) * mix) * gout, (dry[1] + (w[1] - dry[1]) * mix) * gout];
                }
            }
            Kind::Tuner => {
                // listen: the mono sum at half rate goes into a ring, and every 512 frames the ring is searched
                // for the lag at which the wave best repeats
                let n = self.buf.len();
                let low = 50.0 * 10f32.powf((p[9] / 10000.0).clamp(0.0, 1.0));
                let fine = (p[1] / 5000.0 - 1.0) / 12.0;
                let speed = (p[0] / 10000.0).clamp(0.0, 1.0);
                let a = 1.0 - (-1.0 / ((0.003 + (1.0 - speed) * (1.0 - speed) * 0.4) * sr)).exp();
                let mut on = [false; 12];
                for (k, o) in on.iter_mut().enumerate() {
                    *o = (p[16 + k / 4] as i32 >> (8 * (k % 4))) & 0xff != 0;
                }
                let half = sr * 0.5;
                for f in io.iter_mut() {
                    let m = (f[0] + f[1]) * 0.5;
                    z[1] += m;
                    z[2] += 1.0;
                    if z[2] >= 2.0 {
                        let lp = self.lpos[0];
                        self.lines[0][lp] = z[1] * 0.5;
                        self.lpos[0] = (lp + 1) % 2048;
                        z[1] = 0.0;
                        z[2] = 0.0;
                    }
                    z[0] += 1.0;
                    if z[0] >= 512.0 {
                        z[0] = 0.0;
                        const W: usize = 1024;
                        let mut x = [0f32; W];
                        let lp = self.lpos[0];
                        for (i, v) in x.iter_mut().enumerate() {
                            *v = self.lines[0][(lp + 2048 - W + i) % 2048];
                        }
                        let top = ((half / low) as usize).clamp(8, 500);
                        let bottom = ((half / 1200.0) as usize).max(2);
                        let span = W - top;
                        let power: f32 = x[..span].iter().map(|v| v * v).sum::<f32>() / span as f32;
                        let mut hz = 0.0;
                        if power > 1e-7 {
                            let mut d = [0f32; 502];
                            let mut run = 0.0;
                            let mut found = 0usize;
                            for lag in 1..=top {
                                let mut sum = 0.0;
                                for i in 0..span {
                                    let e = x[i] - x[i + lag];
                                    sum += e * e;
                                }
                                run += sum;
                                d[lag] = sum * lag as f32 / run.max(1e-12);
                            }
                            let mut lag = bottom;
                            while lag < top {
                                if d[lag] < 0.15 {
                                    while lag + 1 < top && d[lag + 1] < d[lag] {
                                        lag += 1;
                                    }
                                    found = lag;
                                    break;
                                }
                                lag += 1;
                            }
                            if found > 1 && found + 1 <= top {
                                let (y0, y1, y2) = (d[found - 1], d[found], d[found + 1]);
                                let den = y0 - 2.0 * y1 + y2;
                                let shift = if den.abs() > 1e-9 { 0.5 * (y0 - y2) / den } else { 0.0 };
                                hz = half / (found as f32 + shift.clamp(-1.0, 1.0));
                            }
                        }
                        z[3] = hz;
                        // where it should be: the nearest note that is switched on
                        let mut want = 0.0;
                        if hz > 0.0 {
                            let note = 69.0 + 12.0 * (hz / 440.0).log2();
                            let near = note.round() as i32;
                            let mut best = f32::MAX;
                            for c in near - 6..=near + 6 {
                                let dist = (c as f32 - note).abs();
                                if on[c.rem_euclid(12) as usize] && dist < best {
                                    best = dist;
                                    want = (c as f32 - note) / 12.0;
                                }
                            }
                            let period = sr / hz;
                            z[5] = period * (sr * 0.012 / period).ceil();
                        }
                        z[6] = want + fine;
                    }
                    // z[4] is the shift in octaves, sliding toward the wanted one
                    z[4] += (z[6] - z[4]) * a;
                    let ratio = z[4].exp2();
                    if p[7] >= 5000.0 {
                        // the colour of the voice is kept: whole cycles are laid closer or wider, and the gender
                        // knob plays them faster or slower
                        let period = if z[3] > 0.0 { sr / z[3] } else { 0.0 };
                        let colour = ((p[2] / 5000.0 - 1.0) * 0.5).exp2();
                        let sh = self.psola.get_or_insert_with(|| Box::new(crate::psola::Shifter::new(sr)));
                        *f = sh.step(*f, period, ratio, colour, sr);
                        continue;
                    }
                    let grain = if z[5] > 0.0 { z[5] } else { sr * 0.02 }.min(n as f32 - 8.0);
                    self.buf[self.pos] = *f;
                    let mut out = [0f32; 2];
                    for k in 0..2 {
                        let ph = (self.ph + k as f32 * 0.5).fract();
                        let w = 0.5 - 0.5 * (ph * TAU).cos();
                        let at = self.pos as f32 + n as f32 - (ph * grain + 1.0);
                        let i = at as usize;
                        let fr = at - i as f32;
                        for c in 0..2 {
                            out[c] += (self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr) * w;
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    self.ph = (self.ph + (1.0 - ratio) / grain).rem_euclid(1.0);
                    *f = out;
                }
            }
            Kind::Xy => {
                // the dot chases where it was put: at once with the speed full up, otherwise closing in over a
                // time the speed sets and picking up pace as fast as the acceleration allows
                let dt = io.len() as f32 / cx.sr.max(1.0);
                let (speed, accel) = (p.get(2).copied().unwrap_or(4096.0), p.get(3).copied().unwrap_or(4096.0));
                let fresh = self.z[6] == 0.0;
                self.z[6] = 1.0;
                for a in 0..2 {
                    let want = p[a] / 65536.0;
                    if fresh || speed >= 4096.0 {
                        self.z[a] = want;
                        self.z[3 + a] = 0.0;
                    } else {
                        let time = xyz_time(2048.0, speed);
                        let go = (want - self.z[a]) * (1.0 - (-dt / time).exp()) / dt.max(1e-6);
                        let pick = time * 2.0 * (1.0 - accel / 4096.0).clamp(0.0, 1.0);
                        let k = if pick > 1e-4 { 1.0 - (-dt / pick).exp() } else { 1.0 };
                        self.z[3 + a] += (go - self.z[3 + a]) * k;
                        self.z[a] = (self.z[a] + self.z[3 + a] * dt).clamp(0.0, 1.0);
                    }
                    self.out[a] = self.z[a];
                }
                self.out[2] = 0.0;
            }
            Kind::Surface => {
                for (o, v) in self.out.iter_mut().zip(p.iter()) {
                    *o = *v;
                }
            }
            Kind::Xyz => {
                if p.len() < 11 + 6 * XYZ_N {
                    return;
                }
                let dt = io.len() as f32 / cx.sr.max(1.0);
                let read = |t: &[f32], x: f32| {
                    let at = x.clamp(0.0, 1.0) * (XYZ_N - 1) as f32;
                    let i = (at as usize).min(XYZ_N - 2);
                    t[i] + (t[i + 1] - t[i]) * (at - i as f32)
                };
                let fresh = self.z[6] == 0.0;
                self.z[6] = 1.0;
                for a in 0..3 {
                    let lines = &p[11 + 2 * a * XYZ_N..11 + (2 * a + 2) * XYZ_N];
                    let want = read(&lines[..XYZ_N], p[a] / 65536.0);
                    let time = xyz_time(p[9], p[3 + a]);
                    if fresh || time <= 0.0 {
                        self.z[a] = want;
                        self.z[3 + a] = 0.0;
                    } else {
                        // the pointer chases where it was put, picking up speed at the rate the acceleration allows
                        let go = (want - self.z[a]) * (1.0 - (-dt / time).exp()) / dt.max(1e-6);
                        let pick = time * 2.0 * (1.0 - p[6 + a] / 4096.0).clamp(0.0, 1.0);
                        let k = if pick > 1e-4 { 1.0 - (-dt / pick).exp() } else { 1.0 };
                        self.z[3 + a] += (go - self.z[3 + a]) * k;
                        self.z[a] = (self.z[a] + self.z[3 + a] * dt).clamp(0.0, 1.0);
                    }
                    self.out[a] = read(&lines[XYZ_N..], self.z[a]).clamp(0.0, 1.0);
                }
            }
            Kind::Formula => {
                let letters = p.get(4..).unwrap_or(&[]);
                if self.form.as_ref().is_none_or(|f| f.0 != letters) {
                    let text: String = letters.iter().map(|&c| c as u8 as char).collect();
                    self.form = Some((letters.to_vec(), crate::formula::compile(&text).ok()));
                }
                self.dice ^= self.dice << 13;
                self.dice ^= self.dice >> 7;
                self.dice ^= self.dice << 17;
                let v = crate::formula::Vars { a: (p[0] / 65536.0) as f64, b: (p[1] / 65536.0) as f64, c: (p[2] / 65536.0) as f64, song: self.song_at.0, tempo: self.song_at.1 as f64, ms: self.clock, rand: (self.dice >> 11) as f64 / (1u64 << 53) as f64 };
                self.clock += io.len() as f64 * 1000.0 / sr as f64;
                let o = self.form.as_ref().and_then(|f| f.1.as_ref()).map(|f| f.out(&v)).unwrap_or(0.0);
                self.out = [o; 3];
            }
            Kind::FreeFilter => {
                // seven shapes on one two-pole stage: low, band, high pass, notch, low shelf, bell, high shelf.
                // Laws measured from renders: frequency 10 Hz..16 kHz on a base-101 curve, resonance linear
                // 0.1..19 for the pass shapes and 0.1..100 (exponential) for the bell, shelf slope 0.1..2,
                // gain -18..+18 dB, and a 4 Hz blocker behind the stage.
                let kind = ((p[0] / 100.0).round() as i32).clamp(1, 7);
                let r = (p[1] / 1024.0).clamp(0.0, 1.0) as f64;
                let hz = 10.0 + 159.9 * (101f64.powf(r) - 1.0);
                let qr = (p[2] / 1024.0).clamp(0.0, 1.0) as f64;
                let a = 10f64.powf((p[3] as f64 / 1024.0 * 36.0 - 18.0) / 40.0);
                let w = std::f64::consts::TAU * (hz / sr as f64).clamp(0.00002, 0.49);
                let (sn, c) = w.sin_cos();
                let al = match kind {
                    5 | 7 => {
                        let s = 0.1 + 1.9 * qr;
                        sn / 2.0 * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).max(0.0).sqrt()
                    }
                    6 => sn / (2.0 * 0.1 * 1000f64.powf(qr)),
                    _ => sn / (2.0 * (0.1 + 18.89 * qr)),
                };
                let k = 2.0 * a.sqrt() * al;
                let (b0, b1, b2, a0, a1, a2) = match kind {
                    1 => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al),
                    2 => (al, 0.0, -al, 1.0 + al, -2.0 * c, 1.0 - al),
                    3 => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al),
                    4 => (1.0, -2.0 * c, 1.0, 1.0 + al, -2.0 * c, 1.0 - al),
                    5 => (
                        a * ((a + 1.0) - (a - 1.0) * c + k),
                        2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                        a * ((a + 1.0) - (a - 1.0) * c - k),
                        (a + 1.0) + (a - 1.0) * c + k,
                        -2.0 * ((a - 1.0) + (a + 1.0) * c),
                        (a + 1.0) + (a - 1.0) * c - k,
                    ),
                    6 => (1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a),
                    _ => (
                        a * ((a + 1.0) + (a - 1.0) * c + k),
                        -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                        a * ((a + 1.0) + (a - 1.0) * c - k),
                        (a + 1.0) - (a - 1.0) * c + k,
                        2.0 * ((a - 1.0) - (a + 1.0) * c),
                        (a + 1.0) - (a - 1.0) * c - k,
                    ),
                };
                let co = [(b0 / a0) as f32, (b1 / a0) as f32, (b2 / a0) as f32, (a1 / a0) as f32, (a2 / a0) as f32];
                let hp = 1.0 - 5.78e-4 * 44100.0 / sr;
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let y = biquad(&co, &mut z[ch * 2..ch * 2 + 2], f[ch]);
                        let o = y - z[4 + ch] + hp * z[6 + ch];
                        z[4 + ch] = y;
                        z[6 + ch] = o;
                        f[ch] = o;
                    }
                }
            }
            Kind::OldDelay => {
                let n = self.buf.len();
                let len = (old_delay_time(p, cx) as usize).clamp(1, n - 1);
                let (ing, fb) = (p[0] / 1024.0, p[1] / 1024.0);
                let a = if p[2] >= 1024.0 { 1.0 } else { 1.0 - (-TAU * knob_hz(p[2] / 1024.0) / sr).exp() };
                let (swap_out, cross) = (p[5] >= 341.0 && p[5] < 683.0, p[5] >= 683.0);
                for f in io.iter_mut() {
                    let d = self.buf[(self.pos + n - len) % n];
                    z[0] += a * (d[0] - z[0]);
                    z[1] += a * (d[1] - z[1]);
                    let e = [z[0], z[1]];
                    self.buf[self.pos] = if cross { [f[0] * ing + e[1] * fb, f[1] * ing + e[0] * fb] } else { [f[0] * ing + e[0] * fb, f[1] * ing + e[1] * fb] };
                    self.pos = (self.pos + 1) % n;
                    if swap_out {
                        f[0] += e[1];
                        f[1] += e[0];
                    } else {
                        f[0] += e[0];
                        f[1] += e[1];
                    }
                }
                for v in z[..2].iter_mut() {
                    if v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::SendTap => {
                let (l, r) = balance(p[0] / 128.0);
                let (dry, vol) = (p[1] / 256.0, p[2] / 256.0);
                self.tap.clear();
                if p[3] >= 1.0 {
                    self.tap.extend(io.iter().map(|f| [f[0] * l * vol, f[1] * r * vol]));
                }
                for f in io.iter_mut() {
                    f[0] *= dry;
                    f[1] *= dry;
                }
            }
            Kind::Squeeze => {
                // Every law here was measured against reference renders.
                let (gain, mix) = (p[0] / 127.0, p[1] / 128.0);
                // Step size doubles every 1/30 of the knob, from one 16 bit step.
                let step = (2f32).powf(30.0 * p[2] / 65536.0 - 15.0);
                let (filter_on, kind, post) = (p[3] >= 0.5, p[4] as i32, p[7] >= 0.5);
                let fc = 50.0 * 661.5f32.powf(p[6] / 65535.0);
                let ff = 2.0 * (std::f32::consts::PI * (fc / (2.0 * sr)).min(0.25)).sin();
                let damp = (2.0 * (1.0 - (p[5] / 65535.0).clamp(0.0, 1.0).powf(0.25))).min((2.0 / ff - ff * 0.5).min(2.0));
                // Block lengths in samples: one sample at zero, a second at the top of the range.
                let len = |v: f32| 44101f64.powf(v as f64 / 44100.0) as f32;
                let keep = len(p[10]);
                let hit = match p[8] as i32 {
                    1 => keep,
                    2 => keep * keep,
                    3 => keep.sqrt(),
                    _ => len(p[9]),
                };
                let punch_on = p[8] >= 0.5 || p[9] >= 0.5;
                let scale = sr / 44100.0;
                let (keep, cycle) = (keep * scale, (keep + hit) * scale);
                let level = p[11] / 65535.0;
                let filt = |z: &mut [f32], x: f32| -> f32 {
                    let mut out = 0.0;
                    for _ in 0..2 {
                        let notch = x - damp * z[1];
                        z[0] += ff * z[1];
                        let high = notch - z[0];
                        z[1] += ff * high;
                        out += 0.5
                            * match kind {
                                1 => high,
                                2 => z[1],
                                3 => notch,
                                4 => z[0] - high,
                                _ => z[0],
                            };
                    }
                    out
                };
                if z[6] == 0.0 {
                    // From a cold start it only runs 170 samples of silence before stopping.
                    z[5] = 41.0 * scale;
                    z[6] = 1.0;
                }
                for f in io.iter_mut() {
                    let dry = *f;
                    let mut y = *f;
                    // The unit stops 212 samples after its input goes silent, whatever is still ringing inside.
                    if dry[0].abs() > 1e-7 || dry[1].abs() > 1e-7 {
                        z[5] = 0.0;
                    } else {
                        z[5] += 1.0;
                    }
                    if z[5] >= 212.0 * scale {
                        // Everything inside holds still until sound comes back.
                        *f = [0.0; 2];
                        continue;
                    }
                    if filter_on && !post {
                        y[0] = filt(&mut z[0..2], y[0]);
                        y[1] = filt(&mut z[2..4], y[1]);
                    }
                    if step < 4.0 {
                        y[0] = ((y[0] / step).round() * step).clamp(-1.0, 1.0);
                        y[1] = ((y[1] / step).round() * step).clamp(-1.0, 1.0);
                    } else {
                        y = [0.0; 2];
                    }
                    if punch_on {
                        // Share of this sample that falls inside the kept block; block edges land between samples.
                        let t = z[4];
                        let over = |a: f32, b: f32| ((t + 1.0).min(b) - t.max(a)).max(0.0);
                        let w = (over(0.0, keep) + over(cycle, cycle + keep)).min(1.0);
                        let g = w + (1.0 - w) * level;
                        y[0] *= g;
                        y[1] *= g;
                        z[4] = t + 1.0;
                        if z[4] >= cycle {
                            z[4] -= cycle;
                        }
                    }
                    if filter_on && post {
                        y[0] = filt(&mut z[0..2], y[0]);
                        y[1] = filt(&mut z[2..4], y[1]);
                    }
                    f[0] = (dry[0] + (y[0] - dry[0]) * mix) * gain;
                    f[1] = (dry[1] + (y[1] - dry[1]) * mix) * gain;
                }
                for v in z[..4].iter_mut() {
                    if !v.is_finite() || v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::DcBlock => {
                if p[0] >= 0.5 {
                    let a = 1.0 - (-TAU * 0.806 / sr).exp();
                    for f in io.iter_mut() {
                        for c in 0..2 {
                            z[c] += a * (f[c] - z[c]);
                            f[c] -= z[c];
                        }
                    }
                }
            }
            Kind::Eq7 => {
                const HZ: [f32; 7] = [63.0, 250.0, 500.0, 1500.0, 3000.0, 5000.0, 8000.0];
                for b in 0..7 {
                    if p[b].abs() < 1.0 {
                        continue;
                    }
                    let kind = if b == 0 { 5 } else if b == 6 { 7 } else { 6 };
                    // measured: textbook sections; the bells span two octaves up to 1.5 kHz and one above,
                    // both shelves have half the steepest slope
                    let w = TAU * (HZ[b] / sr).clamp(0.0002, 0.49);
                    let (sn, cs) = w.sin_cos();
                    let amp = 10f32.powf(p[b] / 4000.0);
                    let oct = if b < 4 { 2.0 } else { 1.0 };
                    let q_bell = 1.0 / (2.0 * (std::f32::consts::LN_2 / 2.0 * oct * w / sn.max(1e-6)).sinh());
                    let q_shelf = 1.0 / (amp + 1.0 / amp + 2.0).sqrt();
                    let co = band_raw(kind, sn, cs, amp, q_bell, 1.0, q_shelf);
                    let st = &mut z[b * 4..b * 4 + 4];
                    for f in io.iter_mut() {
                        f[0] = biquad(&co, &mut st[0..2], f[0]);
                        f[1] = biquad(&co, &mut st[2..4], f[1]);
                    }
                    for v in st.iter_mut() {
                        if !v.is_finite() || v.abs() < 1e-20 {
                            *v = 0.0;
                        }
                    }
                }
            }
            Kind::Mute => {
                // the switch reads 0 while it mutes
                if p[0] < 0.5 {
                    let which = p[1] as i32;
                    for f in io.iter_mut() {
                        if which != 2 {
                            f[0] = 0.0;
                        }
                        if which != 1 {
                            f[1] = 0.0;
                        }
                    }
                }
            }
            Kind::Flip => {
                let which = p[0] as i32;
                if which > 0 {
                    for f in io.iter_mut() {
                        f[which as usize - 1] = -f[which as usize - 1];
                    }
                }
            }
            Kind::Emph => {
                if p[4] < 0.5 {
                    let g = db(p[0] / 10000.0 * 24.0);
                    let e = (p[1] / 10000.0).clamp(0.0, 1.0);
                    let ceil = db(e * 12.0);
                    let out = if p[3] >= 0.5 { 1.0 / g } else { 1.0 };
                    let rel = (-1.0 / (0.08 * sr)).exp();
                    let hard = p[2] >= 0.5;
                    for f in io.iter_mut() {
                        let x = [f[0] * g, f[1] * g];
                        let pk = x[0].abs().max(x[1].abs());
                        z[0] = if pk > z[0] { pk } else { pk + (z[0] - pk) * rel };
                        let lim = if z[0] > ceil { ceil / z[0] } else { 1.0 };
                        for c in 0..2 {
                            let y = x[c] * lim;
                            let a = y.abs();
                            let y = if hard || e <= 0.0 {
                                y.clamp(-1.0, 1.0)
                            } else if a > 0.75 {
                                y.signum() * (0.75 + 0.25 * ((a - 0.75) / 0.25).tanh())
                            } else {
                                y
                            };
                            f[c] = y * out;
                        }
                    }
                    if z[0] < 1e-20 {
                        z[0] = 0.0;
                    }
                }
            }
            Kind::StonePhaser => {
                let (lo, hi) = ((p[0] / 100.0).clamp(20.0, 20000.0), (p[1] / 100.0).clamp(20.0, 20000.0));
                let t = (p[4] / 196608.0).clamp(0.0, 1.0);
                let rate = if p[3] >= 0.5 {
                    // tempo locked: from a sixteen bar sweep down to a fraction of a step
                    1.0 / (cx.step * 4.0 * 64.0 * (-8.0 * t).exp2())
                } else {
                    let (a, b) = (p[7].max(1.0) / 100.0, p[8].max(1.0) / 100.0);
                    a * (b / a).powf(t) / sr
                };
                let fb = p[2] / 6000.0 * 0.9 * if p[9] >= 0.5 { -1.0 } else { 1.0 };
                let spread = p[5] / 6000.0;
                let hp = 1.0 - (-TAU * (p[6] / 100.0).clamp(1.0, 2000.0) / sr).exp();
                let mut co = [0.0f32; 2];
                for (i, f) in io.iter_mut().enumerate() {
                    if i % 16 == 0 {
                        self.ph += rate * 16.0;
                        self.ph -= self.ph.floor();
                        for c in 0..2 {
                            let l = 0.5 - 0.5 * ((self.ph + spread * c as f32) * TAU).cos();
                            let w = (PI * lo * (hi / lo).powf(l) / sr).min(1.5).tan();
                            co[c] = (w - 1.0) / (w + 1.0);
                        }
                    }
                    for c in 0..2 {
                        z[10 + c] += hp * (f[c] - z[10 + c]);
                        let dry = f[c] - z[10 + c];
                        let mut x = dry + fb * z[12 + c];
                        for s in 0..4 {
                            let k = c * 4 + s;
                            let y = co[c] * x + z[k];
                            z[k] = x - co[c] * y;
                            x = y;
                        }
                        z[12 + c] = x.clamp(-4.0, 4.0);
                        f[c] = 0.5 * (dry + x);
                    }
                }
                for v in z[..14].iter_mut() {
                    if !v.is_finite() || v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::Amp => {
                // p: 0 input, 1 output, 2 equaliser switch, 3..11 its bands, 11 cabinet, 12..16 drive pedal,
                // 16..19 chorus pedal, 19..22 pedal switches, 22..25 what sits in the three pedal places
                let gin = (p[0] / 5000.0).powi(2);
                let gout = (p[1] / 5000.0).powi(2);
                for f in io.iter_mut() {
                    f[0] *= gin;
                    f[1] *= gin;
                }
                for slot in 0..3 {
                    if p[19 + slot] < 5000.0 {
                        continue;
                    }
                    match (p[22 + slot] / 10000.0).round() as i32 {
                        7 => {
                            let drive = 1.0 + 60.0 * (p[12] / 10000.0).powi(2);
                            let comp = 1.0 / drive.sqrt().max(1.0) * 1.6;
                            let body = band_coeffs(6, 130.0, p[14] / 10000.0 * 12.0, 0.5, sr);
                            let a = 1.0 - (-TAU * (1200.0 + 10000.0 * p[15] / 10000.0) / sr).exp();
                            for f in io.iter_mut() {
                                for c in 0..2 {
                                    let x = biquad(&body, &mut z[c * 2..c * 2 + 2], f[c]);
                                    let y = (x * drive).tanh() * comp;
                                    z[4 + c] += a * (y - z[4 + c]);
                                    f[c] = z[4 + c];
                                }
                            }
                        }
                        2 => {
                            let n = self.buf.len();
                            let depth = p[16] / 10000.0 * 0.004 * sr;
                            let inc = (0.1 + 5.0 * p[17] / 10000.0) / sr;
                            let mix = p[18] / 10000.0;
                            for f in io.iter_mut() {
                                self.buf[self.pos] = *f;
                                self.ph += inc;
                                self.ph -= self.ph.floor();
                                for c in 0..2 {
                                    let d = 0.008 * sr + depth * (1.0 + ((self.ph + 0.25 * c as f32) * TAU).sin());
                                    let at = self.pos as f32 + n as f32 - d;
                                    let i0 = at as usize;
                                    let fr = at - i0 as f32;
                                    let w = self.buf[i0 % n][c] * (1.0 - fr) + self.buf[(i0 + 1) % n][c] * fr;
                                    f[c] += w * mix;
                                }
                                self.pos = (self.pos + 1) % n;
                            }
                        }
                        _ => {}
                    }
                }
                if p[2] >= 5000.0 {
                    for b in 0..8 {
                        let gain = (p[3 + b] / 5000.0 - 1.0) * 12.0;
                        if gain.abs() < 0.05 {
                            continue;
                        }
                        let co = band_coeffs(6, 100.0 * (b as f32).exp2(), gain, 0.5, sr);
                        let st = &mut z[8 + b * 4..12 + b * 4];
                        for f in io.iter_mut() {
                            f[0] = biquad(&co, &mut st[0..2], f[0]);
                            f[1] = biquad(&co, &mut st[2..4], f[1]);
                        }
                    }
                }
                // the speaker box: no deep lows, no fizz, a bump where a guitar speaker is loud
                let model = (p[11] / 10000.0).round().clamp(0.0, 20.0);
                let (gh, gl) = (svf_g(70.0 + 8.0 * model, sr), svf_g((3600.0 + 350.0 * model).min(sr * 0.45), sr));
                let bump = band_coeffs(6, 2200.0 + 120.0 * model, 4.0, 0.5, sr);
                for f in io.iter_mut() {
                    for c in 0..2 {
                        let x = svf(&mut z[40 + c * 2..42 + c * 2], f[c], gh, 1.4).2;
                        let x = svf(&mut z[44 + c * 2..46 + c * 2], x, gl, 1.2).0;
                        f[c] = biquad(&bump, &mut z[48 + c * 2..50 + c * 2], x) * gout;
                    }
                }
                for v in z[..52].iter_mut() {
                    if !v.is_finite() || v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::Relooper => {
                // the mono input runs into a long ring; a sharp rise in level marks a new loop, which is the stretch
                // since the mark before it, and both loop voices keep repeating theirs until the next mark
                // z: 0 fast level, 1 slow level, per voice at 8 + 8 v: start, length, read place, speed, frames since mark, play length
                let n = self.lines[0].len();
                let (dry, la, lb) = ((p[0] / 12800.0).powi(2), (p[1] / 12800.0).powi(2), (p[2] / 12800.0).powi(2));
                let mode = p[11] as i32;
                let base = if mode == 3 { p[7] / 1000.0 * cx.step * 4.0 } else { p[7] / 1000.0 * sr };
                let ratio = 1.0 + p[8] / 8.0;
                let min_len = [base.clamp(64.0, n as f32 * 0.5), (base / ratio).clamp(64.0, n as f32 * 0.5)];
                let thresh = 1.2 + 6.0 * (1.0 - p[6] / 1024.0);
                let (af, asl) = (1.0 - (-1.0 / (0.002 * sr)).exp(), 1.0 - (-1.0 / (0.08 * sr)).exp());
                let spread = ((p[9] - 512.0) / 512.0).max(0.0);
                let sweep = ((512.0 - p[9]) / 512.0).max(0.0);
                let bounce = (-(p[10] - 512.0) / 512.0 * 0.25).exp2();
                let fade = (0.002 * sr).max(8.0);
                for f in io.iter_mut() {
                    let m = (f[0] + f[1]) * 0.5;
                    let w = self.pos;
                    self.lines[0][w] = m;
                    self.pos = (w + 1) % n;
                    z[0] += af * (m.abs() - z[0]);
                    z[1] += asl * (m.abs() - z[1]);
                    let hit = z[0] > 0.004 && z[0] > z[1] * thresh;
                    self.ph += sweep * 10.0 / sr;
                    self.ph -= self.ph.floor();
                    let mut out = [f[0] * dry, f[1] * dry];
                    for v in 0..2 {
                        let s = &mut z[8 + v * 8..16 + v * 8];
                        s[4] += 1.0;
                        let tempo_due = mode == 3 && s[4] >= min_len[v];
                        if (hit || tempo_due) && s[4] >= min_len[v] {
                            // roll for this loop's direction and speed
                            let mut roll = || {
                                self.lpos[0] = self.lpos[0].wrapping_mul(1664525).wrapping_add(1013904223) & 0xffff_ffff;
                                (self.lpos[0] >> 8 & 0xffff) as f32 / 65536.0
                            };
                            let len = s[4].min(n as f32 * 0.5);
                            s[0] = (w as f32 - len + n as f32) % n as f32;
                            s[1] = len;
                            s[5] = if mode == 3 { min_len[v] } else { len };
                            let rev = roll() < p[3] / 10000.0;
                            let oct = if roll() < p[4] / 10000.0 { if roll() < p[5] / 10000.0 { 2.0 } else { 0.5 } } else { 1.0 };
                            s[3] = if rev { -oct } else { oct };
                            s[2] = if rev { s[5] - 1.0 } else { 0.0 };
                            s[4] = 0.0;
                        }
                        if s[1] >= 1.0 && s[5] >= 1.0 {
                            let at = s[0] + s[2].clamp(0.0, s[1] - 1.0);
                            let i0 = at as usize % n;
                            let fr = at - at.floor();
                            let x = self.lines[0][i0] * (1.0 - fr) + self.lines[0][(i0 + 1) % n] * fr;
                            let edge = (s[2].min(s[5] - s[2]) / fade).clamp(0.0, 1.0);
                            s[2] += s[3];
                            if s[2] >= s[5] || s[2] < 0.0 {
                                if mode == 1 {
                                    s[5] = (s[5] * bounce).clamp(64.0, s[1]);
                                }
                                s[2] = if s[3] < 0.0 { s[5] - 1.0 } else { 0.0 };
                            }
                            let side = if v == 0 { -1.0 } else { 1.0 };
                            let pan = side * spread + side * sweep * (self.ph * TAU).sin();
                            let (l, r) = balance(pan.clamp(-1.0, 1.0));
                            let g = x * edge * if v == 0 { la } else { lb };
                            out[0] += g * l;
                            out[1] += g * r;
                        }
                    }
                    *f = out;
                }
                for v in z[..2].iter_mut() {
                    if v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::Deck => {
                // a record heard on top of what comes in: it turns by itself, or is dragged to wherever the
                // position setting points while that is held or being moved
                if let Some(ir) = &self.fx.ir {
                    let n = ir.smp.frames.len();
                    if n > 1 && p.len() >= 9 {
                        let inc = ir.smp.rate as f32 / sr;
                        let (l, r) = balance(p[2] / 5000.0 - 1.0);
                        let vol = if p[8] >= 5000.0 { 0.0 } else { p[5] / 10000.0 };
                        let hold = p[6] >= 5000.0;
                        let dir = if p[1] > 7500.0 { 1.0 } else if p[1] < 2500.0 { -1.0 } else { 0.0 };
                        let own = dir * ((p[7] / 10000.0 - 0.5) * 4.0).exp2() * inc;
                        let want = (p[0] / 10000.0).clamp(0.0, 1.0) as f64 * (n - 1) as f64;
                        // a heavier platter (less acceleration) takes longer to come up to speed
                        let slow = 1.0 - (p[3] / 10000.0).clamp(0.0, 1.0);
                        let a_own = 1.0 - (-1.0 / ((0.004 + 0.8 * slow * slow * slow) * sr)).exp();
                        let a_hand = 1.0 - (-1.0 / (0.003 * sr)).exp();
                        let catch = 0.012 * sr;
                        if z[3] == 0.0 {
                            z[3] = 1.0;
                            z[1] = p[0];
                            z[0] = if hold { 0.0 } else { own };
                            self.clock = want;
                        }
                        if p[0] != z[1] {
                            z[1] = p[0];
                            z[2] = 0.1 * sr;
                        }
                        let len = n as f64;
                        for f in io.iter_mut() {
                            if hold || z[2] > 0.0 {
                                z[2] -= 1.0;
                                z[0] += (((want - self.clock) as f32 / catch) - z[0]) * a_hand;
                            } else {
                                z[0] += (own - z[0]) * a_own;
                            }
                            self.clock = (self.clock + z[0] as f64).rem_euclid(len);
                            let i = self.clock as usize % n;
                            let t = (self.clock - self.clock.floor()) as f32;
                            let (x, y) = (ir.smp.frames[i], ir.smp.frames[(i + 1) % n]);
                            // a record standing still makes no sound
                            let g = vol * (z[0].abs() / inc * 50.0).min(1.0);
                            f[0] += (x[0] + (y[0] - x[0]) * t) * l * g;
                            f[1] += (x[1] + (y[1] - x[1]) * t) * r * g;
                        }
                        if z[0].abs() < 1e-9 {
                            z[0] = 0.0;
                        }
                    }
                }
            }
            Kind::BassBoost => {
                // Measured: the input through two equal one-pole low-passes (20..100 Hz)
                // is added back on top of it, up to five times over.
                let w = TAU * (20.0 + 80.0 * p[0] / 1024.0) / sr;
                let a = w * (1.0 - 0.17 * w);
                let ratio = (11f32.powf(p[1] / 1024.0) - 1.0) / 2.0;
                for f in io.iter_mut() {
                    for c in 0..2 {
                        z[c] += a * (f[c] - z[c]);
                        z[c + 2] += a * (z[c] - z[c + 2]);
                        f[c] += z[c + 2] * ratio;
                    }
                }
            }
            Kind::Phaser => {
                // measured with clicks and a tracked tone: the stages are plain first order all-pass sections
                // whose coefficient is minus the depth, swept between the two depth knobs by a raised cosine
                // that starts half way up. The sweep runs on a 24 bit counter, so its speed is a whole number
                // of counts per frame: 2 Hz at the top of an exponential knob curve, five times that with
                // the range switch. The right side runs up to half a turn ahead. The input is scaled so the
                // fed back loop keeps its loudness, the fed back part is one frame late, and there is always
                // one more stage than the knob says (three at least, 23 at most). The loop itself runs at 0.99 of
                // the feedback knob.
                const TURN: f32 = 16_777_216.0;
                let x = (p[0] / 5000.0).clamp(0.0, 1.0);
                let wide = if p[3] > 512.0 { 5.0 } else { 1.0 };
                let step = (2.0 * TURN as f64 / sr as f64 * ((4.0 * x as f64).exp() - 1.0) / (4f64.exp() - 1.0) * wide).round() as f32;
                if z[63] == 0.0 {
                    z[63] = 1.0;
                    z[62] = TURN / 4.0;
                }
                let (lo, hi) = (p[1] / 1000.0, p[2] / 1000.0);
                let spread = p[4] / 2048.0;
                let stages = (p[5] as usize).clamp(2, 22) + 1;
                let fb = (p[6] / 1000.0).clamp(0.0, 1.0);
                let norm = (1.0 - fb * fb).sqrt();
                let dry = (p[7] / 1024.0).clamp(0.0, 1.0);
                let gain = (p[8] / 2500.0).max(0.0);
                for f in io.iter_mut() {
                    let ph = z[62] / TURN;
                    z[62] += step;
                    if z[62] >= TURN {
                        z[62] -= TURN;
                    }
                    for c in 0..2 {
                        let l = 0.5 - 0.5 * ((ph + spread * c as f32) * TAU).cos();
                        let co = -(lo + (hi - lo) * l).clamp(0.0, 1.0);
                        let input = f[c] * norm;
                        let mut x = input + 0.99 * fb * z[60 + c];
                        for s in 0..stages {
                            let k = c * 24 + s;
                            let y = co * x + z[k];
                            z[k] = x - co * y;
                            x = y;
                        }
                        if !x.is_finite() {
                            x = 0.0;
                        }
                        z[60 + c] = x.clamp(-8.0, 8.0);
                        f[c] = (input * dry + x * (1.0 - dry)) * gain;
                    }
                }
            }
            Kind::FreqShift => {
                // a pair of all-pass chains gives two copies of the input a quarter turn apart; turning them
                // against a sine and cosine moves every frequency by the same number of hertz
                const A: [f32; 4] = [0.6923878, 0.9360654322959, 0.9882295226860, 0.9987488452737];
                const B: [f32; 4] = [0.4021921162426, 0.8561710882420, 0.9722909545651, 0.9952884791278];
                let mix = (p[0] / 10000.0).clamp(0.0, 1.0);
                let mode = p[1] as i32;
                let x = (p[2].abs() / 100000.0).clamp(0.0, 1.0).powf(4.85) * p[2].signum();
                let hz = match p[8] as i32 {
                    0 => 20000.0 * x,
                    1 => 200.0 * x,
                    // tempo: so many turns to four bars
                    _ => p[2] / 10000.0 * sr / (cx.step * 64.0),
                };
                let inc = hz / sr;
                let side = [p[4] / 5000.0 - 1.0, p[5] / 5000.0 - 1.0];
                let fb = (p[6] / 5000.0 - 1.0).clamp(-0.98, 0.98);
                let width = (p[7] / 10000.0).clamp(0.0, 1.0);
                let offs = [p[9] / 10000.0 - 0.5, p[9] / 10000.0 - 0.5 + p[3] / 10000.0];
                let b = &mut self.buf;
                for f in io.iter_mut() {
                    self.ph = (self.ph + inc).rem_euclid(1.0);
                    let mut wet = [0.0f32; 2];
                    for c in 0..2 {
                        let (sn, cs) = ((self.ph + offs[c]) * TAU).sin_cos();
                        let x = f[c] + fb * b[33][c];
                        let y = if mode >= 2 {
                            x * sn
                        } else {
                            let (mut i, mut q) = (x, x);
                            for s in 0..4 {
                                let k = s * 4;
                                let y = A[s] * (i + b[k + 3][c]) - b[k + 1][c];
                                b[k + 1][c] = b[k][c];
                                b[k][c] = i;
                                b[k + 3][c] = b[k + 2][c];
                                b[k + 2][c] = y;
                                i = y;
                                let k = 16 + s * 4;
                                let y = B[s] * (q + b[k + 3][c]) - b[k + 1][c];
                                b[k + 1][c] = b[k][c];
                                b[k][c] = q;
                                b[k + 3][c] = b[k + 2][c];
                                b[k + 2][c] = y;
                                q = y;
                            }
                            let held = b[32][c];
                            b[32][c] = i;
                            held * cs + side[c] * q * sn
                        };
                        let y = if y.is_finite() { y.clamp(-8.0, 8.0) } else { 0.0 };
                        b[33][c] = y;
                        wet[c] = y;
                    }
                    let (m, s) = ((wet[0] + wet[1]) * 0.5, (wet[0] - wet[1]) * 0.5 * width);
                    f[0] = f[0] * (1.0 - mix) + (m + s) * mix;
                    f[1] = f[1] * (1.0 - mix) + (m - s) * mix;
                }
            }
            Kind::Finisher => {
                // a look-ahead gain rider holds the level at the ceiling; the hardness lets it give part of the
                // reduction back, and a two-slope curve then folds those peaks under the ceiling
                let drive = db(p[1] / 10000.0 * 24.0 - 0.75);
                let link = (p[2] / 10000.0).clamp(0.0, 1.0);
                // the limit itself is fixed; the last knob is a plain gain after it
                let out = db((p[4] / 10000.0 - 1.0) * 25.45);
                let ceil = 1.0f32;
                let order = p[5].clamp(0.0, 4.0);
                let lift = 1.0 + (p[6] / 9.0).clamp(0.0, 1.0).powf(9.2);
                const KNEE: f32 = 0.57;
                let look = (139.0 * sr / 44100.0).round() as usize;
                if self.buf.len() != look + 1 || self.lines.len() != 2 || self.lines[0].len() != look + 1 {
                    self.buf = vec![[0.0; 2]; look + 1];
                    self.lines = vec![vec![0.0; look + 1]; 2];
                    self.pos = 0;
                }
                let att = 1.0 - (-6.0 / look as f32).exp();
                let rel = 1.0 - (-1.0 / ((0.05 + 0.05 * order) * sr)).exp();
                for f in io.iter_mut() {
                    let x = [f[0] * drive, f[1] * drive];
                    let both = x[0].abs().max(x[1].abs());
                    let late = self.buf[self.pos];
                    self.buf[self.pos] = x;
                    for c in 0..2 {
                        self.lines[c][self.pos] = x[c].abs() + (both - x[c].abs()) * link;
                        let pk = self.lines[c].iter().copied().fold(0.0, f32::max);
                        let (want, give) = if pk > ceil { (ceil / pk, lift.min(pk / ceil)) } else { (1.0, 1.0) };
                        // z holds the reduction in force: down within the look-ahead, back up slowly
                        let have = 1.0 - z[c];
                        let next = if want < have { have + (want - have) * att } else { have + (want - have) * rel };
                        z[c] = 1.0 - next;
                        let u = (late[c] * next * give / ceil).abs();
                        let shaped = if u <= KNEE || give <= 1.0 { u } else { KNEE + (u - KNEE) * (1.0 - KNEE) / (give - KNEE) };
                        f[c] = late[c].signum() * shaped.min(1.0) * ceil * out;
                    }
                    self.pos = (self.pos + 1) % (look + 1);
                }
            }
            Kind::LowLift => {
                // the band under the cutoff is driven through a curve that adds overtones, then put back
                let mix = (p[0] / 10000.0).clamp(0.0, 1.0);
                let high = db((p[1] / 5000.0 - 1.0) * 12.0);
                let a = 1.0 - (-TAU * (20.0 * 20f32.powf(p[2] / 10000.0)) / sr).exp();
                let d = 1.0 + 3.0 * p[4] / 10000.0;
                let low = db((p[6] / 5000.0 - 1.0) * 12.0);
                // the slope keeps more or less of the upper overtones
                let a2 = 1.0 - (-TAU * (20.0 * 20f32.powf(p[2] / 10000.0) * (2.0 + 6.0 * p[7] / 10000.0)) / sr).exp();
                for f in io.iter_mut() {
                    for c in 0..2 {
                        let k = c * 4;
                        z[k] += a * (f[c] - z[k]);
                        z[k + 1] += a * (z[k] - z[k + 1]);
                        let lo = z[k + 1];
                        let hi = f[c] - lo;
                        let bent = (lo * d).tanh() / d;
                        z[k + 2] += a2 * (bent - z[k + 2]);
                        let wet = z[k + 2] * low + hi * high;
                        f[c] = f[c] * (1.0 - mix) + wet * mix;
                    }
                }
            }
            Kind::MorphEq => {
                if p.len() < MEQ_HEAD + 9 * MEQ_BANDS * 2 || self.lines.len() < 2 || self.lines[0].len() < MEQ_STATE {
                    return;
                }
                let out = knob_gain(p[3] / 128.0);
                if p[7] >= 0.5 {
                    // only the send gets the bands; the sound itself goes through as it is
                    for f in io.iter_mut() {
                        *f = [f[0] * out, f[1] * out];
                    }
                    return;
                }
                // 31 band-passes a third of an octave apart, all summed the same way up (measured against
                // renders): the band width sets how wide each is, and their level is scaled so a flat curve
                // adds up to about the sound that went in
                let bw = (p[5] / 300.0).clamp(0.0, 1.0);
                let pos = (p[1] / 1000.0).clamp(0.0, 7.0);
                let (ba, t) = ((pos as usize).min(7), pos.fract());
                let bb = (ba + 1).min(7);
                let mix = p[0] / 128.0;
                let at = |bank: usize, i: usize, k: usize| -> f32 { p[MEQ_HEAD + bank * 62 + i * 2 + k] };
                let law = |v: f32| (3f32.powf(v) - 1.0) * 0.5;
                let twice = p[4] >= 0.5;
                let add = p[6] >= 0.5;
                let base = if add { 1.0 } else { 0.0 };
                let mut g = [0.0f32; MEQ_BANDS];
                let mut kq = [1.0f32; MEQ_BANDS];
                let mut sc = [0.0f32; MEQ_BANDS];
                let mut fast = [false; MEQ_BANDS];
                let mut want = [[0.0f32; 2]; MEQ_BANDS];
                for i in 0..MEQ_BANDS {
                    let hz = 20.0 * 2f32.powf(i as f32 / 3.0);
                    // the top bands run at twice the rate so they keep their shape near the top
                    fast[i] = !twice && hz > sr * 0.25;
                    let w = TAU * hz / if twice || fast[i] { sr * 2.0 } else { sr };
                    // (at no width at all the original's output sticks at full scale; ours is silent instead)
                    if bw > 0.0 && w < PI * 0.98 {
                        g[i] = (w * 0.5).tan();
                        kq[i] = 2.0 * (std::f32::consts::LN_2 * 0.5 * bw * w / w.sin()).sinh();
                        sc[i] = 0.236 / bw * kq[i];
                    }
                    // the mix knob scales how far the curve sits from flat, and can turn it over
                    let v = 1.0 + (at(ba, i, 0) * (1.0 - t) + at(bb, i, 0) * t - 1.0) * mix;
                    let pan = (1.0 + (at(ba, i, 1) * (1.0 - t) + at(bb, i, 1) * t - 1.0) * mix + at(8, i, 1) - 1.0).clamp(0.0, 2.0);
                    let lv = law(v) * law(at(8, i, 0));
                    want[i] = [lv * (2.0 - pan).min(1.0), lv * pan.min(1.0)];
                }
                let (st, rest) = self.lines.split_at_mut(1);
                let (st, cur) = (&mut st[0], &mut rest[0]);
                let smooth = p[2].clamp(0.0, 256.0);
                let tau = 0.4427 * smooth / (950.0 - smooth);
                if cur[0] == -1.0 && cur[1] == -1.0 {
                    // it starts out flat and has been moving to its settings for a moment already
                    let left = if tau > 0.0 { (-0.06 / tau).exp() } else { 0.0 };
                    for i in 0..MEQ_BANDS {
                        for c in 0..2 {
                            cur[i * 2 + c] = want[i][c] + (1.0 - want[i][c]) * left;
                        }
                    }
                }
                let glide = if tau > 0.0 { 1.0 - (-1.0 / (tau * sr)).exp() } else { 1.0 };
                fn pass(z: &mut [f32], x: f32, a: f32) -> f32 {
                    let y = a * (x - z[1]) + z[0];
                    z[0] = x;
                    z[1] = y;
                    y
                }
                // one of the two all-pass chains of the half-band pair
                fn chain(z: &mut [f32], x: f32, odd: usize) -> f32 {
                    let mut v = x;
                    for j in (odd..6).step_by(2) {
                        v = pass(&mut z[j * 2..j * 2 + 2], v, MEQ_HALF[j]);
                    }
                    v
                }
                const UP: usize = MEQ_BANDS * 8;
                const DOWN: usize = UP + 24;
                const LAST: usize = DOWN + 24;
                for f in io.iter_mut() {
                    for c in 0..2 {
                        let x = f[c];
                        for i in 0..MEQ_BANDS {
                            let lv = &mut cur[i * 2 + c];
                            *lv += glide * (want[i][c] - *lv);
                        }
                        let acc = if twice {
                            let up = &mut st[UP + c * 12..UP + c * 12 + 12];
                            let pair = [chain(up, x, 0), chain(up, x, 1)];
                            let mut y = [0.0f32; 2];
                            for h in 0..2 {
                                let mut a = pair[h] * base;
                                for i in 0..MEQ_BANDS {
                                    let k = i * 8 + c * 4;
                                    let (_, b, _) = svf(&mut st[k..k + 2], pair[h], g[i], kq[i]);
                                    a += b * sc[i] * (cur[i * 2 + c] - base);
                                }
                                y[h] = a;
                            }
                            let down = &mut st[DOWN + c * 12..DOWN + c * 12 + 12];
                            0.5 * (chain(down, y[1], 0) + chain(down, y[0], 1))
                        } else {
                            let mid = 0.5 * (st[LAST + c] + x);
                            st[LAST + c] = x;
                            let mut a = x * base;
                            for i in 0..MEQ_BANDS {
                                let k = i * 8 + c * 4;
                                if fast[i] {
                                    svf(&mut st[k..k + 2], mid, g[i], kq[i]);
                                }
                                let (_, b, _) = svf(&mut st[k..k + 2], x, g[i], kq[i]);
                                a += b * sc[i] * (cur[i * 2 + c] - base);
                            }
                            a
                        };
                        f[c] = acc * out;
                    }
                }
            }
            Kind::Bank => {
                if let Some(st) = &mut self.bank {
                    crate::fxbank::process(p, st, io, cx);
                }
            }
            Kind::Patch => {
                if let (Some(st), Some(sub)) = (&mut self.patch, &self.fx.sub) {
                    st.sync(sub);
                    st.song_at = self.song_at;
                    crate::fxpatch::process(sub, st, io, cx);
                }
            }
            Kind::Rack => {
                if let Some(st) = &mut self.rack {
                    crate::fxrack::process(p, st, io, cx);
                }
            }
            Kind::Multi => {
                if let Some(st) = &mut self.multi {
                    crate::fxmulti::process(p, st, io, cx);
                }
            }
            Kind::Split => {
                if let Some(st) = &mut self.split {
                    crate::fxsplit::process(p, st, io, cx);
                    self.out[0] = st.env;
                }
            }
            Kind::Voc => {
                if let Some(st) = &mut self.voc {
                    crate::fxvoc::process(p, st, io, cx);
                }
            }
            Kind::OldVoc => {
                if let Some(st) = &mut self.oldvoc {
                    crate::fxvoc::classic::process(p, st, io, cx);
                }
            }
            Kind::Warp => {
                // everything heard goes into a two-bar ring; the time line says how far back to read at each
                // moment of the bar (top = now, bottom = two bars ago) and the volume line gates the result
                let n = self.buf.len();
                let k = sr / 44100.0;
                let bar = (cx.step * 16.0).max(1.0);
                let span = (cx.step * 32.0).min(n as f32 - 64.0);
                let table = |slot: f32, second: bool| {
                    let k = (slot.round().clamp(0.0, WARP_SLOTS as f32 - 1.0)) as usize + if second { WARP_SLOTS } else { 0 };
                    let from = 7 + k * WARP_N;
                    p.get(from..from + WARP_N)
                };
                let (Some(time), Some(vol)) = (table(p[0], false), table(p[1], true)) else { return };
                let (tmix, vmix) = ((p[2] / 256.0).clamp(0.0, 1.0), (p[3] / 256.0).clamp(0.0, 1.0));
                // attack and release each run two smoothing stages in a row; a time of nothing follows at once
                let coef = |v: f32, twice: f32| {
                    let tau = twice * 6.7719 * ((5.0926 * (v / 8192.0).clamp(0.0, 1.0)).exp() - 1.0) * k;
                    if tau < 1e-6 { 1.0 } else { 1.0 - (-1.0 / tau).exp() }
                };
                let (up, down) = (coef(p[4], 1.0), coef(p[5], 2.0));
                let tension = (p[6] / 256.0).clamp(0.0, 1.0);
                let bend = |v: f32| {
                    if tension < 1e-4 { v } else { (1.0 - 1000f32.powf(-v * tension)) / (1.0 - 1000f32.powf(-tension)) }
                };
                // a drawn jump stays a jump; a slope is followed smoothly
                let line = |t: &[f32], ph: f32| {
                    let x = ph * (WARP_N - 1) as f32;
                    let i = (x as usize).min(WARP_N - 2);
                    let w = x - i as f32;
                    let (a, b) = (t[i], t[i + 1]);
                    let d = (b - a).abs();
                    let near = (a - t[i.saturating_sub(1)]).abs().max((t[(i + 2).min(WARP_N - 1)] - b).abs());
                    if d > 1.0 / 64.0 || (d > 1e-4 && d > 4.0 * near) {
                        // the jump sits at the far edge; up to it the line carries on as it came
                        let came = if i > 0 { a - t[i - 1] } else { 0.0 };
                        if came.abs() < d / 4.0 { (a + came * w).clamp(0.0, 1.0) } else { a }
                    } else {
                        a + (b - a) * w
                    }
                };
                // the signal always runs a few frames late; unless the line lies flat along the top, a fraction more
                let late = 15.0 * k;
                let more = if time.iter().all(|v| *v >= 1.0) { 0.0 } else { 15.0 / 64.0 * k };
                let fall = (-1.0 / (239.4 * k)).exp();
                let calm = 0.9006f32.powf(1.0 / k);
                let buf = &mut self.buf;
                // a short reader, six neighbours either side; when the place being read moves faster than the
                // music it listens more dully and more quietly, by an amount fitted to renders
                let i0 = |x: f32| {
                    let (mut sum, mut term, q) = (1f32, 1f32, x * x / 4.0);
                    for m in 1..18 {
                        term *= q / (m * m) as f32;
                        sum += term;
                    }
                    sum
                };
                let top = i0(7.0);
                let read = |buf: &Vec<[f32; 2]>, pos: usize, behind: f32, rho: f32| {
                    let bi = behind.ceil();
                    let f = bi - behind;
                    let bi = bi as isize;
                    let mut o = [0f32; 2];
                    let mut sum = 0f32;
                    for t in -5isize..=6 {
                        let x = t as f32 - f;
                        let a = PI * rho * x;
                        let g = if a.abs() < 1e-5 { 1.0 } else { a.sin() / a } * i0(7.0 * (1.0 - (x / 6.0).powi(2)).max(0.0).sqrt()) / top;
                        sum += g;
                        let far = bi - t;
                        if far < 0 || far as usize >= n {
                            continue;
                        }
                        let s = buf[(pos + n - far as usize) % n];
                        o[0] += s[0] * g;
                        o[1] += s[1] * g;
                    }
                    if rho >= 1.0 {
                        return o;
                    }
                    let mut level = 0f32;
                    for t in -15isize..=16 {
                        let x = t as f32 - f;
                        let (a, c) = (PI * rho * x, PI * x / 16.0);
                        level += rho * if a.abs() < 1e-5 { 1.0 } else { a.sin() / a } * (0.42 + 0.5 * c.cos() + 0.08 * (2.0 * c).cos());
                    }
                    let g = level / sum.max(1e-6);
                    [o[0] * g, o[1] * g]
                };
                for f in io.iter_mut() {
                    buf[self.pos] = *f;
                    // the place in the bar is counted in whole frames so it cannot creep
                    self.ph = z[7] / bar;
                    let ph = ((z[7] + 1.0) / bar).fract();
                    let back = ((1.0 - line(time, ph)) * span).clamp(0.0, span);
                    let want = bend(line(vol, ph).clamp(0.0, 1.0));
                    if z[11] == 0.0 {
                        z[11] = 1.0;
                        z[0] = want;
                        z[1] = want;
                        z[2] = back;
                        z[6] = 1.0;
                    }
                    // z[2] is the distance last used, z[6] how fast the place read moves along a slope, z[3] the
                    // rush left over from a jump, z[4..6] what is left of the sound a jump cut off
                    let step = back - z[2];
                    let jump = step.abs() > 48.0 * k;
                    if jump {
                        z[3] += 0.1235 * step.abs();
                    } else {
                        z[6] = (1.0 - step).abs();
                    }
                    z[2] = back;
                    let rho = 1.0 / (z[6] + z[3]).max(1.0);
                    let dry = buf[(self.pos + n - late.round() as usize) % n];
                    let mut wet = if more == 0.0 && rho >= 1.0 && k == 1.0 {
                        dry
                    } else {
                        read(buf, self.pos, back + late + more, rho)
                    };
                    if jump {
                        z[4] += z[8] - wet[0];
                        z[5] += z[9] - wet[1];
                    }
                    wet[0] += z[4];
                    wet[1] += z[5];
                    z[8] = wet[0];
                    z[9] = wet[1];
                    z[3] = if z[3] < 1e-4 { 0.0 } else { z[3] * calm };
                    for c in 4..6 {
                        z[c] = if z[c].abs() < 1e-9 { 0.0 } else { z[c] * fall };
                    }
                    // z[0] and z[1] are the two stages of the gate
                    z[0] += (want - z[0]) * if want > z[0] { up } else { down };
                    z[1] += (z[0] - z[1]) * if z[0] > z[1] { up } else { down };
                    let g = 1.0 - vmix * (1.0 - z[1]);
                    *f = [(dry[0] + (wet[0] - dry[0]) * tmix) * g, (dry[1] + (wet[1] - dry[1]) * tmix) * g];
                    self.pos = (self.pos + 1) % n;
                    z[7] += 1.0;
                    if z[7] >= bar {
                        z[7] -= bar;
                    }
                }
            }
            Kind::Peak => {
                // the loudest moment of the block lifts the follower at once; it then falls back over the decay
                let top = io.iter().fold(0f32, |m, f| m.max(f[0].abs()).max(f[1].abs())).min(1.0);
                let fall = (-(io.len() as f32) / (0.008 * 300f32.powf(p[3] / 128.0) * sr)).exp();
                z[0] = if top > 0.00316 && top > z[0] { top } else { z[0] * fall };
                let shaped = |v: f32, tension: f32| v.clamp(0.0, 1.0).powf((-tension / 50.0).exp2());
                let peak = p[0] / 65536.0 + p[1] / 128.0 * shaped(z[0], p[2]);
                let hz = ((32768.0 - p[7]) / 4096.0).exp2();
                self.ph = (self.ph + hz * io.len() as f32 / sr).fract();
                let ph = (self.ph + p[9] / 65536.0).fract();
                let wave = match p[8] as i32 {
                    0 => 0.5 - 0.5 * (ph * TAU).cos(),
                    1 => 1.0 - (2.0 * ph - 1.0).abs(),
                    2 => if ph < 0.5 { 1.0 } else { 0.0 },
                    3 => ph,
                    _ => {
                        // a new value every cycle
                        if ph < z[2] {
                            z[1] = ((z[1] * 9301.0 + 0.2113).fract() * 7.31 + ph).fract();
                        }
                        z[2] = ph;
                        z[1]
                    }
                };
                let sweep = p[4] / 65536.0 + p[5] / 256.0 * shaped(wave, p[6]);
                self.out = [(peak + sweep).clamp(0.0, 1.0), peak.clamp(0.0, 1.0), sweep.clamp(0.0, 1.0)];
                // the mute switch silences the sound on its way through (a trigger nobody is meant to hear);
                // projects whose master track carries one with the switch byte clear play, which settles its sense
                if (p[10] as i32 >> 16) & 1 == 1 {
                    io.fill([0.0; 2]);
                }
            }
            Kind::BucketChorus => {
                // modes: 0 and 1 are the two fixed settings, 3 both at once, 2 follows the knobs
                let custom = p[0] as i32 == 2;
                let ms = |v: f32| 0.88 + v * 0.00145;
                let (hz, t1, t2, lr, sine) = match p[0] as i32 {
                    2 => (12.0 * (p[1] / 245760.0).powi(2), ms(p[4]), ms(p[5]), p[3] / 6000.0, p[2] >= 0.5),
                    1 => (0.863, 1.66, 5.35, 0.5, false),
                    3 => (9.75, 3.3, 3.7, 0.5, true),
                    _ => (0.513, 1.66, 5.35, 0.5, false),
                };
                let start = if custom { p[14] / 6000.0 } else { 0.0 };
                let mix = (p[6] / 6000.0).clamp(0.0, 1.0);
                let (dry, wet) = ((mix * PI * 0.5).cos(), (mix * PI * 0.5).sin() * if p[8] >= 0.5 { -1.0 } else { 1.0 });
                let fb = (p[9] / 6000.0).clamp(0.0, 1.0) * 0.9;
                let hp = 1.0 - (-TAU * 20.0 * 50f32.powf((p[10] / 6000.0).clamp(0.0, 1.0)) / sr).exp();
                let gain = db(p[11] / 6000.0 * 12.0);
                let mono = p[7] >= 0.5;
                let n = self.buf.len();
                let step = hz / sr;
                // the hiss of the delay chips, heard in the wet part; gated, it is only there while sound comes in
                let hiss = (p[12] / 6000.0).clamp(0.0, 1.0).powi(2) * 0.01;
                let gated = p[13] >= 0.5;
                let (up, down) = (1.0 - (-1.0 / (0.005 * sr)).exp(), 1.0 - (-1.0 / (0.25 * sr)).exp());
                for f in io.iter_mut() {
                    let x = if mono { [(f[0] + f[1]) * 0.5; 2] } else { *f };
                    let mut w = [0f32; 2];
                    let open = if !gated || x[0].abs().max(x[1].abs()) > 0.001 { 1.0 } else { 0.0 };
                    z[4] += (open - z[4]) * if open > z[4] { up } else { down };
                    for c in 0..2 {
                        let ph = (self.ph + start + lr * c as f32).fract();
                        let l = if sine { 0.5 - 0.5 * (ph * TAU).cos() } else { 1.0 - (2.0 * ph - 1.0).abs() };
                        let d = (t1 + (t2 - t1) * l) * 0.001 * sr;
                        let at = self.pos as f32 + n as f32 - d.clamp(1.0, n as f32 - 4.0);
                        let i = at as usize;
                        let fr = at - i as f32;
                        let v = self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        z[c] += (v - z[c]) * hp;
                        self.dice ^= self.dice << 13;
                        self.dice ^= self.dice >> 7;
                        self.dice ^= self.dice << 17;
                        let white = (self.dice >> 40) as f32 / (1u64 << 23) as f32 - 1.0;
                        w[c] = v - z[c] + white * hiss * z[4];
                    }
                    self.buf[self.pos] = [x[0] + w[0] * fb, x[1] + w[1] * fb];
                    self.pos = (self.pos + 1) % n;
                    self.ph = (self.ph + step).fract();
                    *f = [(x[0] * dry + w[0] * wet) * gain, (x[1] * dry + w[1] * wet) * gain];
                }
            }
            Kind::GrainShift => {
                let ratio = (p[0] / 1200.0).exp2();
                let n = self.buf.len();
                let grain = (p[5] / 10.0 * 0.001 * sr).clamp(64.0, n as f32 - 8.0);
                let taps = ((p[3] / 600.0).round() as usize).clamp(2, 4);
                let fb = (p[9] / 100.0).clamp(0.0, 0.97);
                let plain = p[10] as i32 == 3;
                let len = self.lines[0].len();
                let delay = (shift_delay(p, cx) as usize).clamp(1, len - 1);
                let mix = p[12] / 1000.0;
                let (dry, wet) = ((1.0 - mix).max(0.0), mix.max(0.0));
                let step = (1.0 - ratio) / grain;
                for f in io.iter_mut() {
                    let lp = self.lpos[0];
                    let back = (lp + len - delay) % len;
                    let echo = [self.lines[0][back] * fb, self.lines[1][back] * fb];
                    // the echo goes round through the shifter, or beside it in the plain mode
                    self.buf[self.pos] = if plain { *f } else { [f[0] + echo[0], f[1] + echo[1]] };
                    let mut y = [0f32; 2];
                    for k in 0..taps {
                        let ph = (self.ph + k as f32 / taps as f32).rem_euclid(1.0);
                        let win = (0.5 - 0.5 * (ph * TAU).cos()) * 2.0 / taps as f32;
                        let at = self.pos as f32 + n as f32 - ph * grain - 1.0;
                        let i = at as usize;
                        let fr = at - i as f32;
                        for c in 0..2 {
                            y[c] += (self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr) * win;
                        }
                    }
                    if p[0].abs() < 0.5 {
                        // no shift asked for: the newest frame, not a blend of delayed ones
                        y = self.buf[self.pos];
                    }
                    if p[1].abs() >= 0.5 {
                        // a colour of its own: the cycles of the sound are found and laid down afresh
                        let x = self.buf[self.pos];
                        let sh = self.psola.get_or_insert_with(|| Box::new(crate::psola::Shifter::new(sr)));
                        let period = sh.listen(x, sr);
                        y = sh.step(x, period, ratio, (p[1] / 1200.0).exp2(), sr);
                    }
                    if plain {
                        y = [y[0] + echo[0], y[1] + echo[1]];
                    }
                    self.lines[0][lp] = y[0];
                    self.lines[1][lp] = y[1];
                    self.lpos[0] = (lp + 1) % len;
                    self.pos = (self.pos + 1) % n;
                    self.ph = (self.ph + step).rem_euclid(1.0);
                    *f = [f[0] * dry + y[0] * wet, f[1] * dry + y[1] * wet];
                }
            }
            Kind::BandSplit => {
                let (f_lo, f_hi) = (split_hz(p[0]), split_hz(p[1]));
                let (g_lo, g_hi) = (svf_g(f_lo, sr), svf_g(f_hi, sr));
                let (a_lo, a_hi) = (1.0 - (-TAU * f_lo / sr).exp(), 1.0 - (-TAU * f_hi / sr).exp());
                // slopes come in 6 dB steps; two-pole stages carry them, with a one-pole stage for the odd ones
                let order = |v: f32| (v as usize).min(7) + 1;
                let (n_lo, n_hi) = (order(p[2]), order(p[3]));
                let three = p[5] >= 0.5;
                let mutes = p[9] as i32;
                let level = |v: f32, bit: i32| if v <= -1800.0 || mutes & bit != 0 { 0.0 } else { db(v / 100.0) };
                let (l_lo, l_mid, l_hi) = (level(p[6], 8), level(p[7], 16), level(p[8], 32));
                const Q: f32 = std::f32::consts::SQRT_2;
                // a band given its own way out leaves the track here; the mixer carries it on from `tap`, which
                // holds the low, mid and high bands one after the other
                let away = [p.get(10).is_some_and(|v| *v >= 1.0), p.get(11).is_some_and(|v| *v >= 1.0), p.get(12).is_some_and(|v| *v >= 1.0)];
                let n = io.len();
                let noted = self.tap_all;
                self.tap.clear();
                if noted || away.iter().any(|a| *a) {
                    self.tap.resize(n * 3, [0.0; 2]);
                }
                let tap = &mut self.tap;
                for (i, f) in io.iter_mut().enumerate() {
                    for c in 0..2 {
                        let z = &mut z[c * 20..c * 20 + 20];
                        let x = f[c];
                        let mut low = x;
                        for k in 0..(n_lo / 2).min(4) {
                            low = svf(&mut z[k * 2..], low, g_lo, Q).0;
                        }
                        if n_lo % 2 == 1 {
                            z[16] += (low - z[16]) * a_lo;
                            low = z[16];
                        }
                        let mut part = |band: usize, v: f32| {
                            if away[band] || noted {
                                tap[band * n + i][c] = v;
                            }
                            if away[band] {
                                0.0
                            } else {
                                v
                            }
                        };
                        if !three {
                            f[c] = part(0, low * l_lo) + part(2, (x - low) * l_hi);
                            continue;
                        }
                        let mut high = x;
                        for k in 0..(n_hi / 2).min(4) {
                            high = svf(&mut z[8 + k * 2..], high, g_hi, Q).2;
                        }
                        if n_hi % 2 == 1 {
                            z[17] += (high - z[17]) * a_hi;
                            high -= z[17];
                        }
                        f[c] = part(0, low * l_lo) + part(1, (x - low - high) * l_mid) + part(2, high * l_hi);
                    }
                }
            }
            Kind::EchoBank => {
                if p.len() < 197 || self.lines.len() < 9 || self.lpos.len() < 16 {
                    return;
                }
                let (rings, mem) = self.lines.split_at_mut(8);
                let mem = &mut mem[0];
                // every level knob follows the shared curve (measured on single clicks)
                let (dry, wet, input) = (knob_gain(p[0] / 128.0), knob_gain(p[1] / 128.0), knob_gain(p[2] / 128.0));
                let len = rings[0].len() / 2;
                // the furthest a side can be held back before the line
                let ahead = ((sr * 0.33) as usize).min(len / 4);
                struct Bank {
                    on: bool,
                    pan_in: [f32; 4],
                    // kind, the five numbers of one stage, and whether it sits after the line
                    filt: (i32, [f32; 5], bool),
                    echo_filt: (i32, [f32; 5]),
                    // stages of each
                    shape: [usize; 2],
                    // what one trip round the line does to the two sides: [left from left, left from right,
                    // right from left, right from right]
                    turn: [f32; 4],
                    // grain length in frames (0 = whole) and the bend of its envelope
                    grain: (usize, f32),
                    // level lost on every trip, and whether the first echo skips that loss
                    fb: f32,
                    whole_first: bool,
                    mode: i32,
                    time: usize,
                    // how much later each side goes into the line
                    lead: [usize; 2],
                    pan_out: [f32; 4],
                    next: f32,
                }
                // every filter is a plain two-pole section; the knob laws were read off noise run through each
                // kind (1 to 7: low, band, notch, high, low shelf, bell, high shelf). The low pass has its own
                // cutoff scale, bands and notches are set by width in octaves, shelves by steepness
                let section = |kind: i32, cut: f32, res: f32, level: f32, stages: usize| -> [f32; 5] {
                    // more stages share the work: each takes its root of the level and of the sharpness, and a
                    // band is made that many times wider
                    let n = stages as f32;
                    let (x, r) = ((cut / 65536.0).clamp(0.0, 1.0), (res / 65536.0).clamp(0.0, 1.0));
                    let turn = 1501.2f32.powf(x);
                    let hz = if kind == 1 { 29.31 * turn + 103.01 } else { 17.636 * turn - 13.217 };
                    let w = std::f32::consts::TAU * hz.min(20000.0).min(sr * 0.4535) / sr;
                    let (sn, cs) = w.sin_cos();
                    let a = (0.1 + 0.006_663 * level.clamp(0.0, 200.0) + 2.9e-6 * level.clamp(0.0, 200.0).powi(2)).powf(1.0 / n);
                    let (b, d): ([f32; 3], [f32; 3]) = match kind {
                        1 | 4 => {
                            let al = sn / (2.0 * (0.075 + 0.025 * 1997f32.powf(r)).powf(1.0 / n));
                            let b = if kind == 1 { [(1.0 - cs) * 0.5, 1.0 - cs, (1.0 - cs) * 0.5] } else { [(1.0 + cs) * 0.5, -1.0 - cs, (1.0 + cs) * 0.5] };
                            (b, [1.0 + al, -2.0 * cs, 1.0 - al])
                        }
                        2 | 3 => {
                            let wide = (0.02885 + 0.345 * (101.6f32.powf(r) - 1.0)) * n;
                            let al = sn * (std::f32::consts::LN_2 * 0.5 * wide * w / sn).min(30.0).sinh();
                            (if kind == 2 { [al, 0.0, -al] } else { [1.0, -2.0 * cs, 1.0] }, [1.0 + al, -2.0 * cs, 1.0 - al])
                        }
                        6 => {
                            let al = sn / (2.0 * (0.05 + 99.95 * 1999f32.powf(-r)));
                            ([1.0 + al * a, -2.0 * cs, 1.0 - al * a], [1.0 + al / a, -2.0 * cs, 1.0 - al / a])
                        }
                        _ => {
                            let steep = 1.35 * 3f32.powf(-r) - 0.35;
                            let al = sn * 0.5 * ((a + 1.0 / a) * (1.0 / steep - 1.0) + 2.0).sqrt();
                            let t = 2.0 * a.sqrt() * al;
                            // the high shelf is the low one with the cosine turned over
                            let c = if kind == 5 { cs } else { -cs };
                            let f = if kind == 5 { 1.0 } else { -1.0 };
                            (
                                [a * ((a + 1.0) - (a - 1.0) * c + t), f * 2.0 * a * ((a - 1.0) - (a + 1.0) * c), a * ((a + 1.0) - (a - 1.0) * c - t)],
                                [(a + 1.0) + (a - 1.0) * c + t, f * -2.0 * ((a - 1.0) + (a + 1.0) * c), (a + 1.0) + (a - 1.0) * c - t],
                            )
                        }
                    };
                    [b[0] / d[0], b[1] / d[0], b[2] / d[0], d[1] / d[0], d[2] / d[0]]
                };
                // a pan knob (in, out and separation alike): the side turned away from fades out, and what it
                // carried moves over to the other side, so that at the end one side holds both at 0.707.
                // Rows: [left from left, left from right, right from left, right from right]
                let pan = |v: f32, level: f32| {
                    let a = (1.0 + v.abs().min(1.0)) * std::f32::consts::FRAC_PI_4;
                    let (gl, gr) = (std::f32::consts::SQRT_2 * a.cos(), std::f32::consts::SQRT_2 * a.sin());
                    let (keep, moved) = (gr * (1.0 + gl) * 0.5 * level, gr * (1.0 - gl) * 0.5 * level);
                    if v >= 0.0 {
                        [gl * level, 0.0, moved, keep]
                    } else {
                        [keep, moved, 0.0, gl * level]
                    }
                };
                let signed = |v: f32| knob_gain(v.abs()) * v.signum();
                let banks: [Bank; 8] = std::array::from_fn(|b| {
                    let q = &p[4 + b * 20..24 + b * 20];
                    let (modes, sw) = (q[18] as i32, q[19] as i32);
                    let fine = if p[164] >= 16.0 { 10.0 } else { 1.0 };
                    let t = bank_time(q, p[164], cx);
                    // measured: the held-back side lags by half a millisecond times (650^x - 1)
                    let lag = 0.0005 * sr * (650f32.powf((q[9].abs() / (48.0 * fine)).min(1.0)) - 1.0);
                    let lead = |on: bool| if on { ((lag + 0.5) as usize).min(ahead - 1) } else { 0 };
                    // the echo pan knob bleeds each side into the other: towards the right the two sides
                    // merge, towards the left the other side comes in turned over
                    let ep = (q[7] / 128.0).clamp(-1.0, 1.0);
                    let (same, cross) = if ep >= 0.0 {
                        let c = (5f32.powf(ep) - 1.0) / 8.0;
                        (1.0 - c, c)
                    } else {
                        let h = -ep * 0.5;
                        (1.0 - h * h, -h * (1.0 - h))
                    };
                    // one trip: the bleed first, then separation (with both knobs turned the side that receives
                    // comes out a little differently in the original; either knob alone is exact)
                    let sp = pan(q[5] / 128.0, 1.0);
                    let turn = [sp[0] * same + sp[1] * cross, sp[0] * cross + sp[1] * same, sp[2] * same + sp[3] * cross, sp[2] * cross + sp[3] * same];
                    Bank {
                        on: (sw >> 8) & 1 != 0,
                        pan_in: pan(q[0] / 128.0, knob_gain(q[1] / 128.0)),
                        filt: (q[16] as i32, section(q[16] as i32, q[2], q[3], q[4], 1 + (p[165 + b * 4] as usize).min(2)), modes & 1 != 0),
                        echo_filt: (q[17] as i32, section(q[17] as i32, q[10], q[11], q[12], 1 + (p[166 + b * 4] as usize).min(2))),
                        shape: [1 + (p[165 + b * 4] as usize).min(2), 1 + (p[166 + b * 4] as usize).min(2)],
                        turn,
                        grain: {
                            let div = (p[167 + b * 4] / 256.0).round();
                            (if div >= 1.0 { ((t / div) as usize).max(8) } else { 0 }, (p[168 + b * 4] / 32768.0).clamp(-1.0, 1.0))
                        },
                        fb: (q[6] / 128.0 * p[3] / 128.0).clamp(0.0, 1.0),
                        whole_first: (sw >> 16) & 1 != 0,
                        mode: (modes >> 8) & 0xff,
                        time: (t.round_ties_even().max(1.0) as usize).min(len - ahead - 2),
                        lead: [lead(q[9] < 0.0), lead(q[9] > 0.0)],
                        pan_out: pan(q[13] / 128.0, signed(q[14] / 128.0)),
                        next: if b < 7 { signed(q[15] / 128.0) } else { 0.0 },
                    }
                });
                let run = |m: &mut [f32], k: &[f32; 5], stages: usize, x: f32| {
                    let mut x = x;
                    for st in 0..stages {
                        let y = k[0] * x + m[st * 2];
                        m[st * 2] = k[1] * x - k[3] * y + m[st * 2 + 1];
                        m[st * 2 + 1] = k[2] * x - k[4] * y;
                        x = y;
                    }
                    x
                };
                for f in io.iter_mut() {
                    let x = [f[0] * input, f[1] * input];
                    let mut sum = [0f32; 2];
                    let mut carry = [0f32; 2];
                    for (b, k) in banks.iter().enumerate() {
                        if !k.on {
                            carry = [0.0; 2];
                            continue;
                        }
                        let z = &mut mem[b * 24..b * 24 + 24];
                        let line = &mut rings[b];
                        let pos = self.lpos[b];
                        let i = &k.pan_in;
                        let mut s = [x[0] * i[0] + x[1] * i[1] + carry[0], x[0] * i[2] + x[1] * i[3] + carry[1]];
                        if (1..=7).contains(&k.filt.0) && !k.filt.2 {
                            for c in 0..2 {
                                s[c] = run(&mut z[c * 6..], &k.filt.1, k.shape[0], s[c]);
                            }
                        }
                        if k.grain.0 > 0 {
                            // each echo is cut into equal pieces, every piece under its own rise and fall
                            let at = self.lpos[8 + b] % k.grain.0;
                            self.lpos[8 + b] = (at + 1) % k.grain.0;
                            let ph = at as f32 / k.grain.0 as f32;
                            let env = (1.0 - (2.0 * ph - 1.0).abs()).powf(4f32.powf(k.grain.1));
                            s = [s[0] * env, s[1] * env];
                        }
                        let mut o = if k.mode == 0 {
                            // no line at all: the bank passes its input straight on
                            s
                        } else {
                            // measured: one echo time for both sides; the side offset holds one side's input
                            // back before the line, so with crossed echoes that lag changes sides every trip
                            let rd = (pos + len - k.time) % len;
                            let mut e = [line[rd * 2].clamp(-16.0, 16.0), line[rd * 2 + 1].clamp(-16.0, 16.0)];
                            // the echo filter is part of the trip: read here, the first echo has been through it
                            // once and every later one once more; a whole first echo skips it along with the trip
                            if (1..=7).contains(&k.echo_filt.0) && !k.whole_first {
                                for c in 0..2 {
                                    e[c] = run(&mut z[12 + c * 6..], &k.echo_filt.1, k.shape[1], e[c]);
                                }
                            }
                            let clear = (pos + ahead) % len;
                            line[clear * 2] = 0.0;
                            line[clear * 2 + 1] = 0.0;
                            // one trip: separation, bleed and loss, then the sides change places in the bouncing mode
                            let t = &k.turn;
                            let trip = |v: [f32; 2]| {
                                let w = [(t[0] * v[0] + t[1] * v[1]) * k.fb, (t[2] * v[0] + t[3] * v[1]) * k.fb];
                                if k.mode == 3 { [w[1], w[0]] } else { w }
                            };
                            let mut back = trip(e);
                            if (1..=7).contains(&k.echo_filt.0) && k.whole_first {
                                for c in 0..2 {
                                    back[c] = run(&mut z[12 + c * 6..], &k.echo_filt.1, k.shape[1], back[c]);
                                }
                            }
                            // mode 2 swaps the sides once on the way in; a whole first echo skips the trip
                            for c in 0..2 {
                                let mut one = [0f32; 2];
                                one[if k.mode == 2 { 1 - c } else { c }] = s[c];
                                let w = if k.whole_first { one } else { trip(one) };
                                let at = (pos + k.lead[c]) % len;
                                line[at * 2] += w[0];
                                line[at * 2 + 1] += w[1];
                            }
                            line[pos * 2] += back[0];
                            line[pos * 2 + 1] += back[1];
                            self.lpos[b] = (pos + 1) % len;
                            e
                        };
                        if (1..=7).contains(&k.filt.0) && k.filt.2 {
                            for c in 0..2 {
                                o[c] = run(&mut z[c * 6..], &k.filt.1, k.shape[0], o[c]);
                            }
                        }
                        let u = &k.pan_out;
                        sum[0] += o[0] * u[0] + o[1] * u[1];
                        sum[1] += o[0] * u[2] + o[1] * u[3];
                        carry = [o[0] * k.next, o[1] * k.next];
                    }
                    *f = [f[0] * dry + sum[0] * wet, f[1] * dry + sum[1] * wet];
                }
            }
            Kind::MultiComp => {
                // every band has its own edges; the stored cutoff follows a measured curve
                let (g_lo, g_m0, g_m1, g_hi) = (svf_g(band_edge_hz(p[20]), sr), svf_g(band_edge_hz(p[12]), sr), svf_g(band_edge_hz(p[11]), sr), svf_g(band_edge_hz(p[3]), sr));
                // per band: state, gain, threshold, ratio, knee width, attack, release. Measured on level ramps:
                // the threshold spans 60 dB, the part of the level over it is divided down in plain amplitude
                // (ratio 1 holds the output at the threshold), and the corner is a parabola of fixed least width
                // that widens with the threshold as the knee knob comes down.
                let band = |b: usize| {
                    let o = [4usize, 13, 21][b];
                    let u = |i: usize| (p[o + i] / 65536.0).clamp(0.0, 1.0);
                    let thr = 0.001f32.powf(1.0 - u(1));
                    (
                        p[[2usize, 10, 19][b]] as i32,
                        2f32.powf(3.0 * u(0)) - 1.0,
                        thr,
                        u(2),
                        (0.005 + thr * (1.0 - u(5) * u(5))).min(1.0),
                        1.0 - (-1.0 / (p[o + 3].max(0.1) * 0.0001 * sr / std::f32::consts::TAU)).exp(),
                        (-1.0 / (p[o + 4].max(1.0) * 0.0001 * sr)).exp(),
                    )
                };
                let bands = [band(0), band(1), band(2)];
                let main = 2f32.powf(2.0 * (p[0] / 65536.0).clamp(0.0, 1.0)) - 1.0;
                let limit = p[27] >= 0.5;
                // the limiter's level drops straight back to the peak and then falls away over about 15 ms
                let hold = (-1.0 / (0.014_955 * sr)).exp();
                // every edge is a four-stage flat-topped filter (measured from single clicks through one band)
                let low4 = |z: &mut [f32], x: f32, g: f32| -> f32 {
                    let l = svf(z, x, g, 1.847_759).0;
                    svf(&mut z[2..], l, g, 0.765_367).0
                };
                let high4 = |z: &mut [f32], x: f32, g: f32| -> f32 {
                    let h = svf(z, x, g, 1.847_759).2;
                    svf(&mut z[2..], h, g, 0.765_367).2
                };
                // the linear phase choice splits with four short symmetric low passes instead (measured tap by
                // tap): the edge is a straight ramp from 17% under to 17% over the cutoff, 105 taps under a
                // smooth window, 52 frames late; mid and high are differences against the delayed input
                const TAPS: usize = 105;
                let fir = p[1] >= 0.5;
                if fir {
                    let edges = [p[20], p[12], p[11], p[3]];
                    if self.lines.len() != 2 || self.lines[0].len() != 4 * TAPS + 5 {
                        self.lines = vec![vec![-1.0; 4 * TAPS + 5], vec![0.0; 2 * 128]];
                        self.lpos = vec![0];
                    }
                    let t = &mut self.lines[0];
                    if t[4 * TAPS..4 * TAPS + 4] != edges || t[4 * TAPS + 4] != sr {
                        for (e, &v) in edges.iter().enumerate() {
                            let hz = band_edge_hz(v);
                            let (lo, hi) = (hz * 0.8299, (hz * 1.1701).min(sr * 0.5));
                            let (mid, wide) = ((lo + hi) / sr, (hi - lo) / sr);
                            let sinc = |x: f32| if x.abs() < 1e-6 { 1.0 } else { (std::f32::consts::PI * x).sin() / (std::f32::consts::PI * x) };
                            for i in 0..TAPS {
                                let k = i as f32 - 52.0;
                                let q = std::f32::consts::PI * k.abs() / 58.2537;
                                let win = 0.483_98 + 0.493_52 * q.cos() + 0.022_38 * (2.0 * q).cos();
                                t[e * TAPS + i] = win * mid * sinc(mid * k) * sinc(wide * k);
                            }
                            t[4 * TAPS + e] = v;
                        }
                        t[4 * TAPS + 4] = sr;
                    }
                }
                for f in io.iter_mut() {
                    let mut part = [[0f32; 2]; 3];
                    if fir {
                        let (t, past) = self.lines.split_at_mut(1);
                        let (t, past) = (&t[0], &mut past[0]);
                        let at = self.lpos[0];
                        self.lpos[0] = (at + 1) & 127;
                        for c in 0..2 {
                            past[c * 128 + at] = f[c];
                            let mut sum = [0f32; 4];
                            for i in 0..TAPS {
                                let x = past[c * 128 + ((at + 128 - i) & 127)];
                                for (e, v) in sum.iter_mut().enumerate() {
                                    *v += t[e * TAPS + i] * x;
                                }
                            }
                            part[0][c] = past[c * 128 + ((at + 128 - 52) & 127)] - sum[3];
                            // a mid band whose lower edge sits above its upper edge is empty (measured)
                            part[1][c] = if p[12] >= p[11] { 0.0 } else { sum[2] - sum[1] };
                            part[2][c] = sum[0];
                        }
                    } else {
                        for c in 0..2 {
                            let x = f[c];
                            let m = &mut z[c * 18..c * 18 + 16];
                            part[0][c] = high4(m, x, g_hi);
                            let mid = high4(&mut m[4..], x, g_m0);
                            part[1][c] = low4(&mut m[8..], mid, g_m1);
                            part[2][c] = low4(&mut m[12..], x, g_lo);
                        }
                    }
                    let mut out = [0f32; 2];
                    for (b, &(state, gain, thr, ratio, width, att, rel)) in bands.iter().enumerate() {
                        let s = part[b];
                        match state {
                            1 => continue,
                            2 => {
                                // a passed band still takes its gain knob
                                out[0] += s[0] * gain;
                                out[1] += s[1] * gain;
                                continue;
                            }
                            _ => {}
                        }
                        // the peak falls away at the release rate, and the level then trails it at the attack rate
                        // once a band has died away past the smallest whole number the follower starts afresh
                        // (measured: a click after some 680 frames of silence comes through at full level,
                        // however slow the release; after 650 it does not)
                        if s[0].abs() < f32::MIN_POSITIVE && s[1].abs() < f32::MIN_POSITIVE {
                            z[43 + b] = 0.0;
                            z[40 + b] = 0.0;
                        }
                        let peak = s[0].abs().max(s[1].abs()).max(z[43 + b] * rel);
                        z[43 + b] = peak;
                        // the gain reads the level midway between this frame and the last
                        let was = z[40 + b];
                        let now = was + (peak - was) * att;
                        z[40 + b] = now;
                        let d = (was + now) * 0.5 - thr;
                        let over = if d <= -width {
                            0.0
                        } else if d >= width {
                            d
                        } else {
                            (d + width) * (d + width) / (4.0 * width)
                        };
                        let g = gain / (1.0 + ratio * over / thr);
                        out[0] += s[0] * g;
                        out[1] += s[1] * g;
                    }
                    for c in 0..2 {
                        let v = out[c] * main;
                        f[c] = if limit {
                            // each side on its own: the same divided-down law as the bands, fixed at -1.2 dB and
                            // three quarters. The level chases the held peak with an overshooting step and the
                            // gain reads midway between this frame and the last, so a lone sample slips through
                            const T: f32 = 0.870_96;
                            const W: f32 = 0.1698;
                            // a new peak is held for one more frame before it starts to let go, so a steady
                            // level ripples slightly every third frame
                            let was = z[48 + c];
                            if v.abs() > z[46 + c] {
                                z[46 + c] = v.abs();
                                z[50 + c] = 0.0;
                            } else {
                                z[50 + c] += 1.0;
                                if z[50 + c] >= 2.0 {
                                    z[50 + c] = 2.0;
                                    z[46 + c] *= hold;
                                }
                            }
                            let peak = z[46 + c];
                            let now = was + (peak - was) * 1.385;
                            z[48 + c] = now;
                            let d = (was + now) * 0.5 - T;
                            let over = if d <= -W {
                                0.0
                            } else if d >= W {
                                d
                            } else {
                                (d + W) * (d + W) / (4.0 * W)
                            };
                            (v / (1.0 + 0.75 * over / T)).clamp(-1.0, 1.0)
                        } else {
                            v
                        };
                    }
                }
            }
            Kind::Eq => {
                // runs at twice the rate between two half-band filters, so bands near the top keep their shape
                let main = db(p[35] / 100.0);
                const BANDS: usize = 32;
                if self.lines.len() != 1 || self.lines[0].len() != BANDS + 7 * 16 {
                    self.lines = vec![vec![0.0; BANDS + 7 * 16]];
                }
                let high = p[36] >= 0.5;
                let classic = p[36] < -0.5;
                let rate = if high { sr * 2.0 } else { sr };
                let mut bands = [([[0f32; 5]; 4], 0usize, 0usize); 7];
                let mut nb = 0;
                for b in 0..7 {
                    let kind = p[21 + b] as i32;
                    let gain = p[b] / 100.0;
                    if kind == 0 || (matches!(kind, 5 | 6 | 7) && gain.abs() < 0.005) {
                        continue;
                    }
                    let order = if !classic { p[28 + b] as i32 } else { 0 };
                    let parts = 1 + order.unsigned_abs().min(3) as usize;
                    let mut c = [[0f32; 5]; 4];
                    for (k, c) in c.iter_mut().enumerate().take(parts) {
                        *c = if classic {
                            old_eq_coeffs(kind, p[7 + b] / 65536.0, gain, p[14 + b] / 65536.0, rate)
                        } else {
                            eq_coeffs(kind, knob_hz(p[7 + b] / 65536.0), gain, p[14 + b] / 65536.0, rate, order, k)
                        };
                    }
                    bands[nb] = (c, BANDS + b * 16, parts);
                    nb += 1;
                }
                let st = &mut self.lines[0];
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let h = ch * 16;
                        let mut two = if high { [hb_chain(&HB_EVEN, &mut st[h..h + 4], f[ch]), hb_chain(&HB_ODD, &mut st[h + 4..h + 8], f[ch])] } else { [f[ch], 0.0] };
                        for v in two.iter_mut().take(if high { 2 } else { 1 }) {
                            for (c, zo, parts) in &bands[..nb] {
                                for k in 0..*parts {
                                    *v = biquad(&c[k], &mut st[zo + k * 4 + ch * 2..], *v);
                                }
                            }
                        }
                        f[ch] = main * if high { 0.5 * (hb_chain(&HB_EVEN, &mut st[h + 8..h + 12], two[1]) + hb_chain(&HB_ODD, &mut st[h + 12..h + 16], two[0])) } else { two[0] };
                    }
                }
            }
            Kind::Reverb => {
                // Measured structure: the mono sum goes through a low cut and a high cut once, then
                // feeds one early tap a side and, after the predelay, four diffusers in series and a
                // ring of two halves. Each half is a damping filter, one diffuser and a tapped line;
                // its end feeds the other half through the decay gain and the bass shelf.
                let n = self.buf.len();
                let k = sr / 44100.0;
                let pole = |hz: f32| {
                    let c = 2.0 - (TAU * hz.min(0.49 * sr) / sr).cos();
                    c - (c * c - 1.0).sqrt()
                };
                let lc_on = p[0] > 19.5;
                let pl = pole(p[0].max(1.0));
                let ph = if p[1] >= 220.5 { 0.0 } else { pole(p[1] * 100.0) };
                let pd = if p[6] >= 220.5 { 0.0 } else { pole(p[6] * 100.0) };
                let pre = if p[2] < 0.5 { 0 } else { ((((p[2] * 114.8453).floor() - 1.0).max(0.0) * k).round() as usize).min(n - 2) };
                let size = p[3].clamp(1.0, 100.0);
                let scale = (15.0 + size) / 65.0;
                let t60 = (p[5] / 10.0).max(0.01);
                let h = (-0.758_926_5 * scale / t60).exp();
                let h_low = h.powf(100.0 / p[7].max(1.0));
                let g = if p[4] < 0.5 { 0.0 } else { 0.5f32.powf((14.636 + size) / 64.64 * 50.0 / p[4]) };
                let g5 = if p[4] < 0.5 { 0.0 } else { 0.5f32.powf(scale * 50.0 / p[4] * REV2_G5) };
                let g6 = if p[4] < 0.5 { 0.0 } else { 0.5f32.powf(scale * 50.0 / p[4] * REV2_G6) };
                // the bass shelf on the way from one half to the other: a first order shelf around
                // the crossover whose low side takes the bass decay
                let shelf = {
                    let t = (std::f32::consts::PI * p[8].max(1.0).min(0.45 * sr) / sr).tan();
                    let gl = h_low / h;
                    let (b0, b1, a0, a1) = if gl >= 1.0 { (gl * t + 1.0, gl * t - 1.0, t + 1.0, t - 1.0) } else { (t + 1.0, t - 1.0, t / gl + 1.0, t / gl - 1.0) };
                    [b0 / a0, b1 / a0, a1 / a0]
                };
                let lvl = |v: f32| if v <= 128.0 { (v.max(0.0) / 128.0).powf(1.5) } else { 1.0 + (v - 128.0) / 64.0 };
                let dry = knob_amp(p[10] / 128.0);
                let early = lvl(p[11]);
                let wet = lvl(p[12]) * 0.4633;
                let sp = (p[9] / 64.0).clamp(-1.0, 1.0);
                let cross = sp.signum() * (5f32.powf(sp.abs()) - 1.0) / 8.0;
                // lengths: one measured row per whole room size
                let row = &REV2_LEN[(size.round() as usize).clamp(1, 100) - 1];
                let fl = |v: u16, cap: usize| ((v as f32 * k).round() as usize).clamp(1, cap - 2);
                let mut ap = [0usize; 8];
                for i in 0..8 {
                    ap[i] = fl(row[i], self.lines[i].len());
                }
                let mid = [fl(row[8], self.lines[10].len()), fl(row[9], self.lines[11].len())];
                let e_at = [
                    (((16.05 * (size + 3.0)).round() * k).round() as usize).min(n - 2),
                    (((12.19 * (size + 3.0)).round() * k).round() as usize).min(n - 2),
                ];
                let mut at = [[0usize; 8]; 2];
                for hf in 0..2 {
                    let cap = self.lines[8 + hf].len();
                    for i in 0..8 {
                        at[hf][i] = ((row[10 + hf * 8 + i] as f32 * k).round() as usize).max(1);
                    }
                    // the first three taps and the entry of the inner diffuser come off the first
                    // line, taps four and five from inside the inner diffuser, the rest after it
                    let p0 = at[hf][4].saturating_sub(mid[hf]).max(1);
                    at[hf][3] = at[hf][3].saturating_sub(p0).min(mid[hf]);
                    for i in 5..8 {
                        at[hf][i] = (at[hf][i] - at[hf][4]).clamp(1, self.lines[12 + hf].len() - 2);
                    }
                    at[hf][4] = p0.min(cap - 2);
                    for i in 0..3 {
                        at[hf][i] = at[hf][i].clamp(1, cap - 2);
                    }
                }
                let w = [1.0, 0.6 + 0.4 * h, 0.25 + 0.75 * h, h, h * (0.7 + 0.3 * h), h * (0.5 + 0.5 * h), h * (0.2 + 0.8 * h)];
                // signs of the seven taps of each half; even taps of the first half and odd taps of
                // the second go left, the others right
                const SG: [[f32; 7]; 2] = [[1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0], [1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0]];
                fn diffuse(line: &mut [f32], lp: &mut usize, m: usize, g: f32, x: f32) -> f32 {
                    let len = line.len();
                    let d = line[(*lp + len - m) % len];
                    let y = d - g * x;
                    line[*lp] = x + g * y;
                    *lp = (*lp + 1) % len;
                    y
                }
                for f in io.iter_mut() {
                    let x = *f;
                    let mut m = 0.5 * (x[0] + x[1]);
                    if lc_on {
                        let hp = 0.5 * (1.0 + pl) * (m - z[0]) + pl * z[1];
                        z[0] = m;
                        z[1] = hp;
                        m = hp;
                    }
                    z[2] = (1.0 - ph) * m + ph * z[2];
                    m = z[2];
                    self.buf[self.pos][0] = m;
                    let mut er = [0f32; 2];
                    for ch in 0..2 {
                        let v = self.buf[(self.pos + n - e_at[ch]) % n][0];
                        er[ch] = diffuse(&mut self.lines[4 + ch], &mut self.lpos[4 + ch], ap[4 + ch], g, v);
                    }
                    let mut v = self.buf[(self.pos + n - pre) % n][0];
                    self.pos = (self.pos + 1) % n;
                    for i in 0..4 {
                        v = diffuse(&mut self.lines[i], &mut self.lpos[i], ap[i], g, v);
                    }
                    // the ends of both halves, through the decay and the bass shelf
                    let mut fb = [0f32; 2];
                    for hf in 0..2 {
                        let line = &self.lines[12 + hf];
                        let e = line[(self.lpos[12 + hf] + line.len() - at[hf][7]) % line.len()];
                        let y = shelf[0] * e + shelf[1] * z[6 + hf] - shelf[2] * z[8 + hf];
                        z[6 + hf] = e;
                        z[8 + hf] = y;
                        fb[hf] = h * h * y;
                    }
                    let mut tl = 0f32;
                    let mut tr = 0f32;
                    for hf in 0..2 {
                        let inp = v + fb[1 - hf];
                        z[4 + hf] = (1.0 - pd) * inp + pd * z[4 + hf];
                        let a = diffuse(&mut self.lines[6 + hf], &mut self.lpos[6 + hf], ap[6 + hf], g5, z[4 + hf]);
                        let mut tap = [0f32; 7];
                        let lp = self.lpos[8 + hf];
                        let line = &mut self.lines[8 + hf];
                        let len = line.len();
                        line[lp] = a;
                        for i in 0..3 {
                            tap[i] = line[(lp + len - at[hf][i]) % len];
                        }
                        let into = line[(lp + len - at[hf][4]) % len];
                        self.lpos[8 + hf] = (lp + 1) % len;
                        let out = diffuse(&mut self.lines[10 + hf], &mut self.lpos[10 + hf], mid[hf], g6, into);
                        let lp = self.lpos[10 + hf];
                        let line = &self.lines[10 + hf];
                        let len = line.len();
                        // the diffuser has just stepped: its newest entry is one behind
                        tap[3] = line[(lp + len - 1 - at[hf][3]) % len];
                        tap[4] = line[(lp + len - 1 - mid[hf]) % len];
                        let lp = self.lpos[12 + hf];
                        let line = &mut self.lines[12 + hf];
                        let len = line.len();
                        line[lp] = out;
                        for i in 5..7 {
                            tap[i] = line[(lp + len - at[hf][i]) % len];
                        }
                        self.lpos[12 + hf] = (lp + 1) % len;
                        for i in 0..7 {
                            let t = tap[i] * w[i] * SG[hf][i];
                            if (i + hf) % 2 == 0 { tl += t } else { tr += t }
                        }
                    }
                    f[0] = x[0] * dry + er[0] * early * REV2_EARLY[0] + (tl * (1.0 - cross) + tr * cross) * wet;
                    f[1] = x[1] * dry + er[1] * early * REV2_EARLY[1] + (tr * (1.0 - cross) + tl * cross) * wet;
                }
            }
            Kind::Limiter => {
                // gain, then a compressor stage, then the limiter against the ceiling
                let gain = (7f32.powf(p[0] / 1000.0) - 1.0) / 6.0;
                let ceil = ((3f32.powf(p[2] / 1000.0) - 1.0) / 2.0 * 0.9749).max(1e-4);
                // the three time knobs share one law, 0 to 1000 ms
                let time = |v: f32| 2.0 * (501f32.powf(v / 10000.0) - 1.0) * 0.001 * sr;
                let t_att = time(p[3]);
                let t_rel = time(p[4]);
                // the sustain time widens the window the loudest level is taken from
                let win = (time(p[5]).max(t_att).round() as usize).min(sr as usize);
                let n_att = (p[15].round() as usize).clamp(1, 8);
                let n_rel = (p[16].round() as usize).clamp(1, 8);
                // the curve setting is the number of smoothing stages in a row
                let ka = if t_att < 0.5 { 1.0 } else { 1.0 - (-(n_att as f32).sqrt() * 5.756 / t_att).exp() };
                let kr = if t_rel < 0.5 { 1.0 } else { 1.0 - (-(n_rel as f32).sqrt() * std::f32::consts::LN_10 / t_rel).exp() };
                let lag = (t_att.round() as usize).min(sr as usize);
                if self.buf.len() != sr as usize + 8 {
                    self.buf = vec![[0.0; 2]; sr as usize + 8];
                    self.pos = 0;
                }
                let nb = self.buf.len();
                // the loudest level inside the look-ahead window, kept as a queue of falling candidates
                let cap = sr as usize + 8;
                if self.lines.len() != 3 || self.lines[0].len() != cap {
                    self.lines = vec![vec![0.0; cap]; 3];
                    self.lpos = vec![0; 5];
                }
                const WRAP: usize = 1 << 22;
                // the compressor: threshold, ratio 1:1 to 20:1, and a knee that bends the curve as a logarithm
                let c_thr = ((7f32.powf(p[6] / 1000.0) - 1.0) / 6.0).max(0.001);
                let c_ratio = 1.0 + 19.0 * (51f32.powf(p[7] / 1000.0) - 1.0) / 50.0;
                let c_base = 1.0 + (201f32.powf(p[8] / 1000.0) - 1.0) / 200.0;
                let c_ln = c_base.ln();
                let n_cmp = (p[17].round() as usize).clamp(1, 8);
                let c_time = |v: f32, n: usize| {
                    let t = 4.0 * (501f32.powf(v / 10000.0) - 1.0) * 0.001 * sr;
                    if t < 0.5 { 1.0 } else { 1.0 - (-(n as f32).sqrt() * 100f32.ln() / t).exp() }
                };
                let c_att = {
                    let t = time(p[9]);
                    if t < 0.5 { 1.0 } else { 1.0 - (-(n_cmp as f32).sqrt() * 100f32.ln() / t).exp() }
                };
                let c_rel = c_time(p[10], n_cmp);
                let c_win = (time(p[11]).round() as usize).min(sr as usize);
                let comp_on = c_ratio > 1.0 || p[8] > 0.0;
                // the gate
                let g_thr = (7f32.powf(p[13] / 1000.0) - 1.0) / 6.0;
                let g_gain = ((7f32.powf(p[12] / 1000.0) - 1.0) / 6.0).min(1.0);
                let g_rel = c_time(p[14], 1);
                let gate_on = p[12] < 1000.0 && p[13] > 0.0;
                let sat = (3f32.powf(p[1] / 1000.0) - 1.0) / 2.0;
                let soft = |x: f32| {
                    let a = x.abs();
                    if a <= sat { x } else { (sat + (1.0 - sat) * (1.0 - (-(a - sat) / (1.0 - sat)).exp())).copysign(x) }
                };
                // set to listen to another track, the compressor follows that one's level
                let heard = std::mem::take(&mut self.side_on) && p.get(18).is_some_and(|v| *v >= 1.0) && self.side.len() >= io.len();
                for (i, f) in io.iter_mut().enumerate() {
                    let l = f[0] * gain;
                    let r = f[1] * gain;
                    let mut cg = 1.0;
                    if comp_on {
                        // the level is the power of both sides, averaged over the window when one is set
                        let d = if heard { self.side[i] } else { *f };
                        let pw = (d[0] * d[0] + d[1] * d[1]) * 0.5;
                        let mut lv = if c_win == 0 {
                            pw.sqrt()
                        } else {
                            let at = self.lpos[3] % c_win.max(1);
                            if self.lpos[4] != c_win {
                                // the window changed: start it over
                                self.lines[2][..c_win].fill(0.0);
                                self.lpos[4] = c_win;
                                z[30] = 0.0;
                            }
                            z[30] += pw - self.lines[2][at];
                            self.lines[2][at] = pw;
                            self.lpos[3] = (at + 1) % c_win;
                            if at == 0 {
                                // the running sum is taken afresh once a turn so that rounding cannot build up
                                z[30] = self.lines[2][..c_win].iter().sum::<f32>();
                            }
                            (z[30].max(0.0) / c_win as f32).sqrt()
                        };
                        for k in 0..n_cmp {
                            let c = if lv > z[20 + k] { c_att } else { c_rel };
                            z[20 + k] += (lv - z[20 + k]) * c;
                            lv = z[20 + k];
                        }
                        let over = (20.0 * (lv.max(1e-9) / c_thr).log10()).clamp(0.0, 40.0);
                        let bent = if c_ln > 1e-6 { (1.0 + over * (c_base - 1.0) / c_base).ln() / c_ln } else { over };
                        cg = db(-(over - bent / c_ratio));
                    }
                    // that level is followed straight up, and down through the release stages
                    let now = l.abs().max(r.abs());
                    let peak = if win == 0 {
                        now
                    } else {
                        let (head, len, clock) = (self.lpos[0], self.lpos[1], self.lpos[2]);
                        let mut len = len;
                        let mut head = head;
                        while len > 0 && self.lines[0][(head + len - 1) % cap] <= now {
                            len -= 1;
                        }
                        self.lines[0][(head + len) % cap] = now;
                        self.lines[1][(head + len) % cap] = clock as f32;
                        len += 1;
                        while (clock + WRAP - self.lines[1][head] as usize) % WRAP >= win {
                            head = (head + 1) % cap;
                            len -= 1;
                        }
                        self.lpos[0] = head;
                        self.lpos[1] = len;
                        self.lpos[2] = (clock + 1) % WRAP;
                        self.lines[0][head]
                    };
                    if peak >= z[3] {
                        z[3] = peak;
                        for k in 0..n_rel {
                            z[4 + k] = peak;
                        }
                    } else {
                        let mut to = peak;
                        for k in 0..n_rel {
                            z[4 + k] += (to - z[4 + k]) * kr;
                            to = z[4 + k];
                        }
                        z[3] = to.max(peak);
                    }
                    // whichever of the two asks for less wins
                    let mut want = if z[3] * cg > ceil { ceil / z[3] } else { cg };
                    if gate_on {
                        // below its threshold (set against the level before the gain knob) the gate falls to its gain over the release time, and opens at once
                        let to = if z[3] < g_thr * gain { g_gain } else { 1.0 };
                        z[2] = if to >= 1.0 { 0.0 } else { z[2] + (1.0 - to - z[2]) * g_rel };
                        want *= 1.0 - z[2];
                    }
                    // the gain is smoothed over the look-ahead time; the stages hold 1 - gain so that silence is the resting state
                    let mut to = 1.0 - want;
                    for k in 0..n_att {
                        z[12 + k] += (to - z[12 + k]) * ka;
                        to = z[12 + k];
                    }
                    z[0] = to;
                    let g = 1.0 - to;
                    self.buf[self.pos] = [l, r];
                    let at = (self.pos + nb - lag) % nb;
                    self.pos = (self.pos + 1) % nb;
                    let d = self.buf[at];
                    if p[1] < 1000.0 {
                        f[0] = soft(d[0] * g);
                        f[1] = soft(d[1] * g);
                    } else {
                        f[0] = d[0] * g;
                        f[1] = d[1] * g;
                    }
                }
            }
            Kind::Flanger => {
                let n = self.buf.len();
                let base = (p[0] * 0.001 * sr).clamp(1.0, n as f32 * 0.4);
                let depth = (p[1] / 1024.0 * 0.004 * sr).min(n as f32 * 0.4);
                let inc = 0.02 * 250f32.powf(p[2] / 1024.0) / sr;
                let spread = p[3] / 360.0;
                let a = 1.0 - (-TAU * knob_hz(0.45 + 0.55 * p[4] / 128.0) / sr).exp();
                let fb = (p[5] / 128.0).clamp(-0.95, 0.95);
                let (dry, wet) = (p[6] / 128.0, p[7] / 128.0);
                for f in io.iter_mut() {
                    self.ph += inc;
                    if self.ph >= 1.0 {
                        self.ph -= 1.0;
                    }
                    let mut out = [0.0f32; 2];
                    for c in 0..2 {
                        let l = 0.5 - 0.5 * ((self.ph + spread * c as f32) * TAU).cos();
                        let d = base + depth * l;
                        let at = self.pos as f32 + n as f32 - d;
                        let i = at as usize;
                        let fr = at - i as f32;
                        let y = self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        z[c] += a * (y - z[c]);
                        out[c] = y;
                    }
                    self.buf[self.pos] = [f[0] + z[0] * fb, f[1] + z[1] * fb];
                    self.pos = (self.pos + 1) % n;
                    *f = [f[0] * dry + out[0] * wet, f[1] * dry + out[1] * wet];
                }
            }
            Kind::OldFlanger => {
                // measured with clicks and click trains: one swept delay a side, each feeding back into itself,
                // and the cross knob shares the two between the sides. Delay, depth and rate all follow the
                // same bent knob curve x / (k - (k - 1) x): up to 20 ms each for delay and depth (k = 5) and
                // up to 5 Hz for the rate (k = 10). The sweep is a sine that starts in the middle going up
                // (blended toward a triangle by the shape knob), the right side ahead by the phase knob. The
                // damping is a one pole low pass whose pole is the knob itself, the loop runs at 0.98 of the
                // feedback knob, and dry, wet and cross are plain levels.
                let n = self.buf.len();
                let bend = |x: f32, k: f32| {
                    let x = x.clamp(0.0, 1.0);
                    x / (k - (k - 1.0) * x)
                };
                let base = bend(p[0] / 1024.0, 5.0) * 0.020 * sr;
                let span = bend(p[1] / 5000.0, 5.0) * 0.020 * sr;
                // the sweep runs on a 24 bit counter: a whole number of counts a frame
                const TURN: f32 = 16_777_216.0;
                let step = (5.0 * bend(p[2] / 5000.0, 10.0) * TURN / sr).round();
                // the swing is the whole frames strictly inside half the depth, worked out in double precision
                let deep = (p[1] as f64 / 5000.0).clamp(0.0, 1.0);
                let half = deep / (5.0 - 4.0 * deep) * 0.02 * sr as f64 / 2.0;
                let swing = (half.ceil() - 1.0).max(0.0) as f32;
                let lead = p[3] / 360.0;
                let pole = (p[4] / 5000.0).clamp(0.0, 1.0);
                let shape = (p[5] / 1024.0).clamp(0.0, 1.0);
                let g = 0.98 * (p[6] / 100.0).clamp(0.0, 1.0) * if p[7] >= 512.0 { -1.0 } else { 1.0 };
                let turn = if p[8] >= 512.0 { -1.0 } else { 1.0 };
                let (near, far) = (p[10] / 5000.0 * turn, p[11] / 5000.0 * turn);
                let dry = p[9] / 5000.0;
                let top = n as f32 - 4.0;
                for f in io.iter_mut() {
                    let mut v = [0.0f32; 2];
                    let ph = z[2] / TURN;
                    for c in 0..2 {
                        let t = (ph + lead * c as f32).fract();
                        let tri = 4.0 * ((t + 0.75).fract() - 0.5).abs() - 1.0;
                        let w = (t * TAU).sin() * (1.0 - shape) + tri * shape;
                                                let d = (base + 0.5 * span + swing * w).min(top);
                        // the frame being written counts as zero frames back, and what is fed back is a frame late
                        self.buf[self.pos][c] = f[c] + g * z[c];
                        let at = self.pos as f32 + n as f32 - d;
                        let k = at as usize;
                        let fr = at - k as f32;
                        let y = self.buf[k % n][c] * (1.0 - fr) + self.buf[(k + 1) % n][c] * fr;
                        z[c] = y + pole * (z[c] - y);
                        v[c] = z[c];
                    }
                    self.pos = (self.pos + 1) % n;
                    z[2] += step;
                    if z[2] >= TURN {
                        z[2] -= TURN;
                    }
                    *f = [f[0] * dry + v[0] * near + v[1] * far, f[1] * dry + v[1] * near + v[0] * far];
                }
            }
            Kind::Stack => {
                // a stack of plain flangers per side, no feedback, measured with clicks and click trains:
                // every flanger rides the same slow wave (two parabolas, so its pace is a triangle), the right
                // side half a turn ahead. The delay knob sets where the sweep starts (0.1 s times the level
                // law, never under 8 frames), the depth how far it reaches (20 frames up to 50 ms, by the same
                // law), and the speed how fast the delay moves at the middle of the sweep, whatever the depth.
                // Flanger j of n reaches j/n of the way and crosses j/n of the cross amount to the other side.
                let n = self.buf.len();
                let k = sr / 44100.0;
                let lev = |v: f32| ((11f64.powf(v.abs() as f64 / 128.0) - 1.0) / 10.0) as f32 * if v < 0.0 { -1.0 } else { 1.0 };
                let order = (p[0].round() as usize).clamp(1, 16);
                let full = (20.0 + 2185.0 * (11f64.powf(p[1].clamp(0.0, 1024.0) as f64 / 1024.0) - 1.0) / 10.0) * k as f64;
                // frames of time for the delay to move one frame at its quickest: 25 at the top of the knob,
                // 3775 at the bottom
                let pace = sr as f64 / (25.0 + 37.5 * (101f64.powf(1.0 - p[2].clamp(0.0, 1024.0) as f64 / 1024.0) - 1.0));
                let hz = pace / (4.0 * full);
                let base = (441.0 * (11f64.powf(p[3].max(0.0) as f64 / 128.0) - 1.0)).max(8.0) * k as f64;
                // the spread turns each flanger's start and pace away from the others, steeply near the top
                let sp = ((1000f64.powf(p[4].clamp(0.0, 128.0) as f64 / 128.0) - 1.0) / 999.0) as f32;
                let cross = (p[5] / 128.0).clamp(-1.0, 1.0);
                // one gain for every flanger, set by the sum of all the shares they send out
                let m = order as f64 + cross.abs() as f64 * (order as f64 + 1.0) / 2.0;
                let g = (1.0 / ((0.28925 * m + 0.49997).powi(2) + 0.15634 + 0.34579 / (m + 0.21211)).sqrt()) as f32;
                let (dry, wet) = (lev(p[6]), lev(p[7]) * g);
                let top = n as f64 - 4.0;
                for f in io.iter_mut() {
                    self.buf[self.pos] = *f;
                    let mut own = [0.0f32; 2];
                    let mut over = [0.0f32; 2];
                    for c in 0..2 {
                        for v in 0..order {
                            // v counts from the widest flanger down
                            let share = (order - v) as f32 / order as f32;
                            let i = v * 2 + c;
                            // the phase is kept in two parts, the steps being far too small for one
                            // with the spread up each flanger heads for its own turn rate, the one that makes
                            // its delay move as fast as the widest one's, and a little off even that
                            let reach = 20.0 * k + (full as f32 - 20.0 * k) * share;
                            let turn = (1.0 - sp) + full as f32 / reach * sp * (1.0 + sp * STACK_OFF[(if c == 0 { v } else { 2 * order - 1 - v }) % STACK_OFF.len()]);
                            let step = (hz / sr as f64) as f32 * turn - z[32 + i];
                            let t = z[i] + step;
                            z[32 + i] = (t - z[i]) - step;
                            z[i] = if t >= 1.0 { t - 1.0 } else { t };
                            let ph = z[i] + 0.5 * c as f32 + 0.5 * sp * share;
                            let ph = ph - ph.floor();
                            let h = if ph < 0.5 { ph } else { 1.0 - ph } as f64;
                            let w = if h < 0.25 { 8.0 * h * h } else { 1.0 - 8.0 * (0.5 - h) * (0.5 - h) };
                            let d = (base + (20.0 * k as f64 + (full - 20.0 * k as f64) * share as f64) * w).min(top);
                            let at = self.pos as f64 + n as f64 - d;
                            let q = at as usize;
                            let fr = (at - q as f64) as f32;
                            let y = self.buf[q % n][c] * (1.0 - fr) + self.buf[(q + 1) % n][c] * fr;
                            own[c] += y;
                            over[c] += y * share;
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    *f = [f[0] * dry + (own[0] + over[1] * cross) * wet, f[1] * dry + (own[1] + over[0] * cross) * wet];
                }
            }
            Kind::Chorus => {
                // three voices on one delay line, each swept by its own slow wave between the delay and the
                // delay plus the depth; only one side of a gentle split feeds them
                let n = self.buf.len();
                let base = (0.00005 + p[0] / 1024.0 * 0.02995) * sr;
                let depth = p[1] / 1000.0 * 0.001 * sr;
                // the full turn of the stereo knob puts the right side half a cycle away
                let stereo = p[2] / 2048.0;
                // both switches already read as on at 300 of 1024 (and wet only still off at 100)
                let wet_only = p[11] >= 256.0;
                let low_side = p[9] >= 256.0;
                // the split is a one-pole low-pass of the averaging kind, its corner 8.2 Hz to 10.8 kHz
                let t = (std::f32::consts::PI * (8.216 * 1527f32.powf(p[10] / 5000.0)).min(10756.0) / sr).min(1.5).tan();
                let pole = (1.0 - t) / (1.0 + t);
                let dry = if wet_only { 0.0 } else { 0.7 };
                for f in io.iter_mut() {
                    let mut x = *f;
                    for c in 0..2 {
                        z[8 + c] = 0.5 * (1.0 - pole) * (x[c] + z[10 + c]) + pole * z[8 + c];
                        z[10 + c] = x[c];
                        x[c] = if low_side { z[8 + c] } else { x[c] - z[8 + c] };
                    }
                    self.buf[self.pos] = x;
                    let mut out = [0.0f32; 2];
                    for v in 0..3 {
                        let shape = p[6 + v];
                        for c in 0..2 {
                            let ph = (z[v] + 0.0012 + stereo * c as f32).fract();
                            let l = 0.5 + 0.5 * chorus_wave(ph, shape);
                            let d = base + depth * l;
                            let at = self.pos as f32 + n as f32 - d;
                            let i = at as usize;
                            let fr = at - i as f32;
                            out[c] += self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        }
                        let r = p[3 + v] / 5000.0;
                        // the rate is 5 r^2 Hz with the step per frame cut to whole 2^-24ths of a turn
                        z[v] += (5.0 * r * r / sr * 16_777_216.0).floor() / 16_777_216.0;
                        if z[v] >= 1.0 {
                            z[v] -= 1.0;
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    *f = [f[0] * dry + out[0] * 0.42, f[1] * dry + out[1] * 0.42];
                }
            }
            Kind::Spread => {
                // a smeared copy of the middle goes in with opposite signs left and right, so the two still sum
                // to exactly what came in
                let k = p[0] / 10000.0 * 0.9;
                let sep = (p[1] / 10000.0).clamp(-1.0, 1.0);
                let hz = (10.0 * 1000f32.powf(p[2] / 1_000_000.0)).min(sr * 0.45);
                let a = 1.0 - (-TAU * hz / sr).exp();
                let mono = p[3] >= 0.5;
                for f in io.iter_mut() {
                    z[0] += a * (f[0] - z[0]);
                    z[1] += a * (f[1] - z[1]);
                    let (mut ll, mut lr) = (z[0], z[1]);
                    let (hl, hr) = (f[0] - ll, f[1] - lr);
                    if mono {
                        ll = 0.5 * (ll + lr);
                        lr = ll;
                    }
                    let mid = 0.5 * (hl + hr);
                    let side = 0.5 * (hl - hr) * (1.0 + sep);
                    let mut d = mid;
                    for (line, at) in self.lines.iter_mut().zip(self.lpos.iter_mut()) {
                        let old = line[*at];
                        let v = d + 0.55 * old;
                        line[*at] = v;
                        *at = (*at + 1) % line.len();
                        d = old - 0.55 * v;
                    }
                    *f = [ll + mid + side + k * d, lr + mid - side - k * d];
                }
            }
            Kind::Transient => {
                // every figure here was read from reference renders of steps and tone bursts. Only the part
                // above the split is shaped. Both channels share one level, the sum of their sizes. Onsets are
                // found by holding a peak follower against a slower riser, tails by holding a lingering peak
                // against a quicker one; the knobs turn those two distances into one amount, which lifts the
                // gain directly when positive and divides it when negative
                let ta = ((p[0] - 80.0) / 80.0).clamp(-1.0, 1.0);
                let tr = ((p[2] - 80.0) / 80.0).clamp(-1.0, 1.0);
                let (ka, kr) = (ta * ta, tr * tr);
                let gain = db((p[3] - 80.0) / 80.0 * 9.0);
                // per sample figures were measured at 44.1 kHz
                let k = 44100.0 / sr;
                let per = |c: f32| 1.0 - (1.0 - c).powf(k);
                let rise = per([0.0104f32, 0.0052, 0.0026][(p[4] as usize).min(2)]);
                let shape = (p[5] as usize).min(2);
                let fall = per([0.001052f32, 0.000526, 0.000263][shape]);
                let smooth = per(1.0 / [195.0f32, 390.0, 780.0][shape]);
                let (ebb, linger) = (per(2.612e-4), per(1.29e-5));
                let hz = 5.0 + 995.0 * (p[6] / 160.0).clamp(0.0, 1.0).powi(2);
                let t = (std::f32::consts::PI * hz / sr).tan();
                let (pole, feed) = ((1.0 - t) / (1.0 + t), t / (1.0 + t));
                let (low, high) = ((2.0 - p[7] / 80.0).clamp(0.0, 1.0), (p[7] / 80.0).clamp(0.0, 1.0));
                let push = if p[1] <= 0.0 {
                    0.0
                } else {
                    let x = (p[1] / 20.0).min(1.0);
                    0.7387 * (0.01881 * p[1]).exp() * x * (2.0 - x)
                };
                let soft = |u: f32| {
                    let u = u.min(1.5);
                    u - 4.0 / 27.0 * u * u * u
                };
                for f in io.iter_mut() {
                    z[0] = pole * z[0] + feed * (f[0] + z[2]);
                    z[1] = pole * z[1] + feed * (f[1] + z[3]);
                    z[2] = f[0];
                    z[3] = f[1];
                    let (hl, hr) = (f[0] - z[0], f[1] - z[1]);
                    let a = 0.5 * (hl.abs() + hr.abs());
                    // z[4] the slow riser, z[5] its peak, z[6] the lingering peak, z[7] the quick one, z[8] the
                    // quick one smoothed on the way down
                    z[4] = if a > z[4] { z[4] + rise * (a - z[4]) } else { a.max(z[4] * (1.0 - ebb)) };
                    z[5] = a.max(z[5] * (1.0 - ebb));
                    z[6] = a.max(z[6] * (1.0 - linger));
                    z[7] = a.max(z[7] * (1.0 - fall));
                    z[8] = a.max(z[8] + smooth * (z[7] - z[8]));
                    let mut amount = 0.0;
                    if ka > 0.0 && z[4] > 1e-12 {
                        let v = z[5] / z[4] - 1.0;
                        if ta < 0.0 {
                            amount -= ka * 0.6 * v;
                        } else if v > 0.0 {
                            amount += ka * (2.2563 * soft(v / 6.06) - 0.0171);
                        }
                    }
                    if kr > 0.0 && z[7] > 1e-9 {
                        if tr < 0.0 {
                            amount -= kr * (z[6] / z[7] - 1.0).min(1e3);
                        } else {
                            amount += kr * (6.65 * soft((z[6] / z[8] - 1.0).min(1e3) / 13.4) - 0.0957);
                        }
                    }
                    let g = if amount >= 0.0 { 1.0 + amount } else { 1.0 / (1.0 - amount) };
                    let (mut wl, mut wr) = (hl * g, hr * g);
                    if push > 0.0 {
                        wl = transient_bend(wl * push) / push;
                        wr = transient_bend(wr * push) / push;
                    }
                    *f = [(z[0] * low + wl * high) * gain, (z[1] * low + wr * high) * gain];
                }
            }
            Kind::Swarm => {
                let n = self.buf.len();
                let voices = if p[6] >= 0.5 { 8 } else { 4 };
                let base = (0.003 + p[0] / 1000.0 * 0.03) * sr;
                let fb = p[1] / 1000.0 * 0.6;
                let depth = p[2] / 1000.0 * 0.004 * sr;
                let rate = 0.05 * 100f32.powf(p[3] / 1000.0) / sr;
                let pan_amt = p[4] / 1000.0;
                let mov = 0.02 * 100f32.powf(p[5] / 1000.0) / sr;
                let lp = if p[7] >= 21999.0 { 1.0 } else { 1.0 - (-TAU * p[7].min(sr * 0.45) / sr).exp() };
                let hp = 1.0 - (-TAU * p[8].min(sr * 0.45) / sr).exp();
                let spread = p[9] / 1000.0;
                let mix = p[10] / 1000.0;
                let norm = 1.0 / (voices as f32).sqrt();
                for f in io.iter_mut() {
                    self.buf[self.pos] = [f[0] + z[10] * fb, f[1] + z[11] * fb];
                    z[0] = (z[0] + rate).fract();
                    z[1] = (z[1] + mov).fract();
                    let mut out = [0.0f32; 2];
                    for v in 0..voices {
                        let t = v as f32 / voices as f32;
                        let l = (TAU * (z[0] + t * spread)).sin();
                        let d = base * (1.0 + 0.35 * t) + depth * (1.0 + l);
                        let at = self.pos as f32 + n as f32 - d;
                        let i = at as usize;
                        let fr = at - i as f32;
                        let c = v % 2;
                        let s = self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        let pan = (if c == 0 { -0.5 } else { 0.5 } + 0.5 * pan_amt * (TAU * (z[1] + t)).sin()).clamp(-1.0, 1.0);
                        out[0] += s * (0.5 - 0.5 * pan).sqrt();
                        out[1] += s * (0.5 + 0.5 * pan).sqrt();
                    }
                    self.pos = (self.pos + 1) % n;
                    for c in 0..2 {
                        let mut w = out[c] * norm * 1.4;
                        z[2 + c] += lp * (w - z[2 + c]);
                        w = z[2 + c];
                        z[4 + c] += hp * (w - z[4 + c]);
                        w -= z[4 + c];
                        z[10 + c] = w;
                        f[c] = f[c] * (1.0 - mix).min(0.5) * 2.0 + w * mix.min(0.5) * 2.0;
                    }
                }
            }
            Kind::Echo => {
                let n = self.buf.len();
                let iv = p[0] / 6000.0;
                let model = p[1] as i32;
                let time = echo_time(p, cx).clamp(2.0, n as f32 - 4.0);
                let off = (p[5] - 3000.0) / 3000.0 * 0.25 * time;
                let fb = (p[11] / 6000.0).clamp(0.0, 0.97);
                let a = if p[12] >= 5999.0 { 1.0 } else { 1.0 - (-TAU * (20.0 * 1000f32.powf(p[12] / 6000.0)).min(sr * 0.45) / sr).exp() };
                let drive = p[14] / 6000.0 * 8.0;
                let (wet, dry) = (p[17] / 6000.0, p[19] / 6000.0);
                let tone = p[18] / 6000.0;
                let ta = 1.0 - (-TAU * 900.0 / sr).exp();
                let inc = if p[7] > 0.0 { 0.05 * 200f32.powf(p[7] / 6000.0) / sr } else { 0.0 };
                let wob = p[8] / 6000.0 * 0.01 * time;
                for f in io.iter_mut() {
                    self.ph += inc;
                    if self.ph >= 1.0 {
                        self.ph -= 1.0;
                    }
                    let m = wob * (self.ph * TAU).sin();
                    let mut y = [0.0f32; 2];
                    for c in 0..2 {
                        let d = (time + m + if c == 0 { (-off).max(0.0) } else { off.max(0.0) }).clamp(1.0, n as f32 - 3.0);
                        let at = self.pos as f32 + n as f32 - d;
                        let i = at as usize;
                        let fr = at - i as f32;
                        y[c] = self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        let mut w = y[c] * fb;
                        if drive > 0.0 {
                            w = (w * (1.0 + drive)).tanh() / (1.0 + drive * 0.5);
                        }
                        z[c] += a * (w - z[c]);
                    }
                    let x = [f[0] * iv, f[1] * iv];
                    self.buf[self.pos] = match model {
                        0 => {
                            let s = (x[0] + x[1]) * 0.5 + (z[0] + z[1]) * 0.5;
                            [s, s]
                        }
                        2 => [(x[0] + x[1]) * 0.5 + z[1], z[0]],
                        _ => [x[0] + z[0], x[1] + z[1]],
                    };
                    self.pos = (self.pos + 1) % n;
                    for c in 0..2 {
                        // tone tilts the echoes darker (left of centre) or thinner (right)
                        z[4 + c] += ta * (y[c] - z[4 + c]);
                        let shaped = if tone < 0.0 { y[c] + (z[4 + c] - y[c]) * -tone } else { y[c] - z[4 + c] * tone };
                        f[c] = f[c] * dry + shaped * wet;
                    }
                }
            }
        }
        // the three band compressor has to ring all the way down: its followers start afresh only then
        let floor = if self.fx.kind == Kind::MultiComp { f32::MIN_POSITIVE } else { 1e-20 };
        for v in self.z.iter_mut() {
            if !v.is_finite() || v.abs() < floor {
                *v = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_formula_block_keeps_its_notes_and_takes_a_new_formula() {
        let mut block = vec![2u8, 0, 0, 0, 0, 0x80, 0, 0, 0, 0, 1, 0, 0, 0x40, 0, 0];
        block.push(5);
        block.extend_from_slice(b"a+b+c");
        block.extend_from_slice(&[3, 0, 0, 0, b'h', b'e', b'y']);
        let mut f = Fx::decode("Fruity Formula Controller", &block).unwrap();
        assert_eq!(f.kind, Kind::Formula);
        assert_eq!((f.p[0], f.p[1], f.p[2]), (32768.0, 65536.0, 16384.0));
        assert_eq!(f.formula(), "a+b+c");
        assert_eq!(f.encode(Some(&block)), block);
        f.set_formula("Sin(SongTime)*a");
        let again = f.encode(Some(&block));
        assert!(again.ends_with(&[3, 0, 0, 0, b'h', b'e', b'y']));
        assert_eq!(Fx::decode("Fruity Formula Controller", &again).unwrap(), f);
        // a new one writes a block that reads back
        let mut fresh = Fx::new(Kind::Formula);
        fresh.set_formula("a");
        assert_eq!(Fx::decode("Fruity Formula Controller", &fresh.encode(None)).unwrap(), fresh);
    }

    #[test]
    fn wrapped_stock_effect_round_trips() {
        let mut b = 8u32.to_le_bytes().to_vec();
        let mut chunk = |id: u32, data: &[u8]| {
            b.extend(id.to_le_bytes());
            b.extend((data.len() as u64).to_le_bytes());
            b.extend(data);
        };
        chunk(54, b"Fruity Balance");
        let mut body = vec![0xf7, 0xff, 0xff, 0xff, 5, 0, 0, 0, 0, 0, 0, 0, 0];
        body.extend(2u32.to_le_bytes());
        body.extend(0.5f32.to_le_bytes());
        body.extend(0.8f32.to_le_bytes());
        body.extend(1u32.to_le_bytes());
        chunk(53, &body);
        let fx = Fx::decode("Fruity Wrapper", &b).expect("decodes");
        assert_eq!(fx.p.len(), 2);
        assert_eq!(fx.encode(Some(&b)), b);
        let mut g = fx.clone();
        g.p[0] = DEFS.iter().find(|d| d.kind == g.kind).unwrap().params[0].max as f32;
        let out = g.encode(Some(&b));
        assert_eq!(out.len(), b.len());
        assert_eq!(Fx::decode("Fruity Wrapper", &out).unwrap().p, g.p);
    }

    use super::*;

    fn run(fx: Fx, input: impl Fn(usize) -> [f32; 2], n: usize) -> Vec<[f32; 2]> {
        let cx = Ctx { sr: 44100.0, step: 44100.0 * 60.0 / 130.0 / 4.0, clock: Default::default() };
        let mut u = Unit::new(fx, cx.sr);
        let mut io: Vec<[f32; 2]> = (0..n).map(input).collect();
        for c in io.chunks_mut(256) {
            u.process(c, &cx);
        }
        io
    }

    fn sine(i: usize) -> [f32; 2] {
        let v = (i as f32 * 440.0 * TAU / 44100.0).sin() * 0.5;
        [v, v]
    }

    fn rms(x: &[[f32; 2]]) -> f32 {
        (x.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum::<f32>() / (2 * x.len()) as f32).sqrt()
    }

    #[test]
    fn default_settings_are_transparent() {
        // every unit at its defaults leaves a moderate signal alone, apart from the ones that add sound by design
        for d in DEFS {
            if matches!(d.kind, Kind::Comp | Kind::Limiter) {
                // late by the look-ahead by design (64 samples, and the limiter's 2 ms attack), otherwise untouched
                let late = if d.kind == Kind::Comp { 64 } else { 88 };
                let out = run(Fx::new(d.kind), sine, 8192);
                let worst = out.iter().enumerate().skip(4096).map(|(i, f)| (f[0] - sine(i - late)[0]).abs()).fold(0.0, f32::max);
                assert!(worst < 5e-3, "{} changes the signal by {worst}", d.title);
                continue;
            }
            if matches!(d.kind, Kind::Eq | Kind::OldEq | Kind::Finisher | Kind::Split | Kind::Delay | Kind::Reverb | Kind::Loud | Kind::Flanger | Kind::Stack | Kind::OldFlanger | Kind::Chorus | Kind::Echo | Kind::Spread | Kind::Swarm | Kind::EchoBank | Kind::MultiComp | Kind::Maxi | Kind::BucketChorus | Kind::GrainShift | Kind::OldReverb | Kind::Plush | Kind::Pad | Kind::Tuner | Kind::Peak | Kind::Bank | Kind::Rack | Kind::Voc | Kind::Multi | Kind::OldVoc | Kind::OldDelay | Kind::Squeeze | Kind::DcBlock | Kind::StonePhaser | Kind::Amp | Kind::Relooper | Kind::Deck | Kind::BassBoost | Kind::Phaser | Kind::FreqShift | Kind::FreeFilter | Kind::LowLift | Kind::MorphEq | Kind::FastDist | Kind::Overdrive | Kind::Warp) {
                continue;
            }
            let out = run(Fx::new(d.kind), sine, 8192);
            let worst = out.iter().enumerate().skip(4096).map(|(i, f)| (f[0] - sine(i)[0]).abs()).fold(0.0, f32::max);
            assert!(worst < 2e-3, "{} changes the signal by {worst}", d.title);
        }
    }

    #[test]
    fn drawn_lines_can_be_edited_and_saved() {
        for kind in [Kind::Curve, Kind::Maxi, Kind::Warp] {
            let mut f = Fx::new(kind);
            let ident = def(kind).ident;
            assert!(f.line_count() > 0);
            let last = f.line_count() - 1;
            for k in [0, last] {
                let mut c = f.line(k).unwrap_or_else(|| panic!("{ident} line {k}"));
                let before = f.p.clone();
                // a new point in the middle, pulled off the line
                let x = c.pts[1].x;
                c.pts[1].x = x * 0.5;
                c.pts.insert(1, crate::fm::Pt { x: x * 0.5, y: 0.9, ..Default::default() });
                assert!(f.set_line(k, &c), "{ident} line {k}");
                assert_ne!(f.p, before, "{ident} line {k} changes the table");
                assert_eq!(f.line(k).unwrap().pts.len(), c.pts.len());
            }
            // a knob moved afterwards and the lines both reach the saved block, and read back the same
            f.p[0] += 1.0;
            let block = f.encode(None);
            let back = Fx::decode(ident, &block).unwrap();
            assert_eq!(back.p, f.p, "{ident}");
            // saving over the file's old block gives the same thing
            assert_eq!(f.encode(Some(&Fx::new(kind).encode(None))), block, "{ident}");
        }
    }

    #[test]
    fn blocks_round_trip() {
        for d in DEFS {
            // the filter bank's block is packed; its own module tests the trip
            if matches!(d.kind, Kind::Bank | Kind::Rack | Kind::Voc | Kind::Multi | Kind::OldVoc | Kind::Patch | Kind::Formula | Kind::Notes | Kind::Surface) {
                continue;
            }
            let mut fx = Fx::new(d.kind);
            for (v, p) in fx.p.iter_mut().zip(d.params) {
                *v = ((p.min + p.max) / 2) as f32;
            }
            let bytes = fx.encode(None);
            assert_eq!(bytes.len(), d.size);
            // (a unit that carries its block carries the one it was made from, so only the settings are compared)
            assert_eq!(Fx::decode(d.ident, &bytes).map(|f| (f.kind, f.p, f.ir)), Some((fx.kind, fx.p.clone(), fx.ir.clone())), "{}", d.title);
            // unknown bytes of an existing block survive
            fx.raw = None;
            let mut base = vec![0xAAu8; d.size + 9];
            base = fx.encode(Some(&base));
            assert_eq!(base.len(), d.size + 9);
            assert_eq!(base[d.size + 8], 0xAA);
        }
    }

    #[test]
    fn time_units_echo_and_stay_bounded() {
        let click = |i: usize| if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] };
        // echo: the default time is four steps, the default feedback a half
        let t = (44100.0f32 * 60.0 / 130.0) as usize;
        let mut fx = Fx::new(Kind::Echo);
        fx.p[1] = 1.0;
        let out = run(fx, click, t * 2 + 400);
        let near = |at: usize| out[at - 3..at + 4].iter().map(|f| f[0]).sum::<f32>();
        assert!((near(t) - 1.0).abs() < 0.05, "{}", near(t));
        assert!((near(2 * t) - 0.5).abs() < 0.05, "{}", near(2 * t));
        assert!(out[t / 2][0].abs() < 1e-3);
        for kind in [Kind::Flanger, Kind::Chorus, Kind::Echo, Kind::Swarm, Kind::Spread, Kind::Transient] {
            let mut fx = Fx::new(kind);
            for (v, p) in fx.p.iter_mut().zip(def(kind).params) {
                *v = p.max as f32;
            }
            let out = run(fx, sine, 30000);
            assert!(out.iter().all(|f| f[0].is_finite() && f[0].abs() < 40.0), "{kind:?}");
            let out = run(Fx::new(kind), sine, 8000);
            assert!(rms(&out[4000..]) > 0.05, "{kind:?}");
        }
    }

    #[test]
    fn hertz_shifter_moves_a_tone_up() {
        // 440 Hz moved by the stored value that reads 800 Hz lands on 1240 Hz
        let mut fx = Fx::new(Kind::FreqShift);
        fx.p[2] = 51542.0;
        fx.p[8] = 0.0;
        let out = run(fx, sine, 44100 * 2);
        let tail = &out[44100..];
        let crossings = tail.windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        assert!((crossings as i32 - 1240).abs() <= 8, "{crossings} cycles in a second");
        assert!((rms(tail) - 0.3535).abs() < 0.03, "level {}", rms(tail));
    }

    #[test]
    fn open_filter_shapes() {
        // low pass far below the tone removes it; a high pass there leaves it alone
        let mut fx = Fx::new(Kind::FreeFilter);
        fx.p[1] = 80.0;
        fx.p[2] = 300.0;
        assert!(rms(&run(fx.clone(), sine, 16384)[8192..]) < 0.03);
        fx.p[0] = 300.0;
        assert!((rms(&run(fx, sine, 16384)[8192..]) - 0.3535).abs() < 0.02);
        let mut ph = Fx::new(Kind::Phaser);
        ph.p[7] = 512.0;
        let out = run(ph, sine, 44100);
        assert!(out.iter().all(|f| f[0].abs() < 4.0));
    }

    #[test]
    fn the_phaser_matches_the_measured_clicks_and_sweep() {
        let click = |i: usize| if i == 100 { [0.5f32, 0.5] } else { [0.0, 0.0] };
        let set = |v: &[(usize, f32)]| {
            let mut fx = Fx::new(Kind::Phaser);
            for (k, x) in [(0, 0.0), (1, 500.0), (2, 500.0), (3, 0.0), (4, 0.0), (5, 1.0), (6, 0.0), (7, 0.0), (8, 2500.0)] {
                fx.p[k] = x;
            }
            for &(k, x) in v {
                fx.p[k] = x;
            }
            fx
        };
        let near = |got: &[[f32; 2]], want: &[f32]| {
            for (k, w) in want.iter().enumerate() {
                assert!((got[100 + k][0] - w).abs() < 2e-4, "frame {k}: {} for {w}", got[100 + k][0]);
            }
        };
        // three stages at the least, coefficient minus the depth
        near(&run(set(&[]), click, 400), &[-0.0625, 0.2812, -0.2812, -0.1406, 0.0352, 0.1230]);
        near(&run(set(&[(5, 4.0)]), click, 400), &[-0.015625]);
        near(&run(set(&[(8, 4000.0)]), click, 400), &[-0.1, 0.45, -0.45]);
        near(&run(set(&[(7, 512.0)]), click, 400), &[0.2187, 0.1406, -0.1406]);
        // the loop: scaled input, 0.99 of the knob fed back a frame late
        near(&run(set(&[(6, 800.0)]), click, 400), &[-0.0375, 0.1725, -0.2025, 0.0292, -0.1405, 0.1505, 0.0413, 0.2068]);
        assert!(run(set(&[(6, 1000.0)]), click, 400).iter().all(|f| f[0] == 0.0));
        // the sweep: 33 counts a frame at 1500 (0.08674 Hz), five times that on the wide range, right side ahead
        let tone = |i: usize| [(i as f32 * 0.1).sin() * 0.5; 2];
        let sweep = run(set(&[(0, 1500.0), (1, 0.0), (2, 1000.0), (4, 512.0)]), tone, 44100 * 6);
        let still = run(set(&[(0, 0.0), (1, 1000.0), (2, 1000.0)]), tone, 44100 * 6);
        // a quarter of a turn takes 2^22 / 33 frames; there the left side is at full depth, where every stage only flips the sign
        let at = (4_194_304.0f32 / 33.0) as usize;
        for i in at - 20..at + 20 {
            assert!((sweep[i][0] - still[i][0]).abs() < 0.002, "frame {i}: {} for {}", sweep[i][0], still[i][0]);
        }
        let wide = run(set(&[(0, 1500.0), (1, 0.0), (2, 1000.0), (3, 1024.0)]), tone, 44100);
        let at = (4_194_304.0f32 / 165.0) as usize;
        for i in at - 20..at + 20 {
            assert!((wide[i][0] - still[i][0]).abs() < 0.004, "wide frame {i}");
        }
        // the right side with the stereo knob at 512 is a quarter turn ahead: it starts at full depth
        for i in 3..40 {
            assert!((sweep[i][1] - still[i][1]).abs() < 0.002, "right frame {i}: {} for {}", sweep[i][1], still[i][1]);
        }
    }

    #[test]
    fn the_classic_flanger_matches_the_measured_clicks() {
        let click = |i: usize| if i == 100 { [0.5f32, 0.5] } else { [0.0, 0.0] };
        let set = |v: &[(usize, f32)]| {
            let mut fx = Fx::new(Kind::OldFlanger);
            for k in 0..12 {
                fx.p[k] = if k == 10 { 5000.0 } else { 0.0 };
            }
            for &(k, x) in v {
                fx.p[k] = x;
            }
            fx
        };
        let hits = |fx: Fx, want: &[(usize, f32)]| {
            let out = run(fx, click, 1400);
            for &(at, w) in want {
                assert!((out[100 + at][0] - w).abs() < 2e-4, "frame {at}: {} for {w}", out[100 + at][0]);
            }
            let loud: f32 = out.iter().map(|f| f[0].abs()).sum();
            let wanted: f32 = want.iter().map(|w| w.1.abs()).sum();
            assert!((loud - wanted).abs() < 0.02 || want.len() > 4, "sum {loud} for {wanted}");
        };
        // the bent delay and depth curves
        hits(set(&[]), &[(0, 0.5)]);
        hits(set(&[(0, 256.0)]), &[(55, 0.4375), (56, 0.0625)]);
        hits(set(&[(0, 512.0)]), &[(147, 0.5)]);
        hits(set(&[(0, 1024.0)]), &[(882, 0.5)]);
        hits(set(&[(1, 2500.0)]), &[(73, 0.25), (74, 0.25)]);
        hits(set(&[(1, 5000.0)]), &[(441, 0.5)]);
        // levels, the turned wet part, the damping pole
        hits(set(&[(0, 256.0), (10, 2500.0), (9, 2500.0), (8, 1024.0)]), &[(0, 0.25), (55, -0.2188), (56, -0.0312)]);
        hits(set(&[(0, 256.0), (4, 1000.0)]), &[(55, 0.3501), (56, 0.1199), (57, 0.024), (58, 0.0048), (59, 0.0009)]);
        // the loop: 0.98 of the knob, one frame more than the delay a round
        hits(set(&[(0, 256.0), (6, 50.0)]), &[(55, 0.4376), (56, 0.0624), (111, 0.1877), (112, 0.0535), (113, 0.0038), (167, 0.0805), (168, 0.0344)]);
        hits(set(&[(0, 256.0), (6, 50.0), (7, 1024.0)]), &[(55, 0.4376), (111, -0.1877), (112, -0.0535), (167, 0.0805), (168, 0.0344), (169, 0.0049)]);
        // the sweep: 119 counts a frame at rate 2000; a quarter turn on, the delay is the middle plus the whole frames of half the depth
        let at = (4_194_304.0f32 / 119.0) as usize - 140;
        let late = move |i: usize| if i == at { [0.5f32, 0.5] } else { [0.0, 0.0] };
        let out = run(set(&[(1, 2500.0), (2, 2000.0)]), late, at + 400);
        let (peak, _) = (0..400).map(|k| (k, out[at + k][0])).fold((0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
        assert!((146..=147).contains(&peak), "peak {peak} frames late");
    }

    #[test]
    fn the_seven_band_unit_follows_the_measured_laws() {
        // levels read off renders of the original, one band at the middle of the frequency knob:
        // (kind, level dB, width, order, tone Hz, measured dB)
        let rows = [
            (1, 0.0, 0.0, 0, 632.5, 16.99),
            (1, 0.0, 10000.0, -1, 632.5, 13.16),
            (2, 0.0, 30000.0, 2, 1265.1, -4.49),
            (2, 0.0, 30000.0, -2, 1265.1, -4.49),
            (5, -12.0, 0.0, 0, 449.5, -12.09),
            (5, -12.0, 10000.0, -2, 500.6, -13.12),
            (5, -12.0, 60000.0, 2, 500.6, -7.30),
        ];
        for (kind, level, width, order, hz, want) in rows {
            let got = eq_band_db(kind, 0.5, level, width / 65536.0, order, hz);
            assert!((got - want).abs() < 0.1, "kind {kind} order {order}: {got} against {want}");
        }
        // a pass at its rest width is the flat one, three dB down at the corner
        assert!((eq_band_db(3, 0.5, 0.0, 36068.0 / 65536.0, 0, 632.456) + 3.01).abs() < 0.02);
    }

    #[test]
    fn the_flanger_stack_matches_the_measured_clicks() {
        // order, depth, speed, delay, spread, cross, dry, wet; a 0.5 click on the left only
        let shot = |v: [f32; 8], at: usize, len: usize| {
            let mut fx = Fx::new(Kind::Stack);
            fx.p[..8].copy_from_slice(&v);
            let out = run(fx, move |i| if i == at { [0.5f32, 0.0] } else { [0.0, 0.0] }, at + len);
            let sum = |c: usize| out[at..].iter().map(|f| f[c]).sum::<f32>();
            let mid = out[at..].iter().enumerate().map(|(k, f)| k as f32 * f[0]).sum::<f32>() / sum(0).max(1e-9);
            (sum(0), sum(1), mid)
        };
        let near = |a: f32, b: f32, tol: f32| assert!((a - b).abs() < tol, "{a} for {b}");
        // the level laws: dry is exact, one flanger a little under, more flangers share a ceiling
        near(shot([1.0, 0.0, 0.0, 8.0, 0.0, 0.0, 128.0, 0.0], 100, 200).0, 0.5, 1e-4);
        near(shot([1.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 128.0], 100, 200).0, 0.4846, 4e-4);
        near(shot([1.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 64.0], 100, 200).0, 0.1124, 2e-4);
        near(shot([1.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, -32.0], 100, 200).0, -0.0398, 1e-4);
        near(shot([2.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 128.0], 100, 200).0, 0.8231, 6e-4);
        near(shot([4.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 128.0], 100, 200).0, 1.158, 8e-4);
        near(shot([16.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 32.0], 100, 200).0, 0.1277, 1e-4);
        // the cross: flanger j of n sends j/n of it over, and the shares count toward the ceiling
        let (own, over, _) = shot([16.0, 0.0, 0.0, 8.0, 0.0, 128.0, 0.0, 32.0], 100, 200);
        near(own, 0.0864, 1e-4);
        near(over, 0.0460, 1e-4);
        let (own, over, _) = shot([1.0, 0.0, 0.0, 8.0, 0.0, -64.0, 0.0, 128.0], 100, 200);
        near(own, 0.4846 * 0.9293, 5e-4);
        near(over, -0.4846 * 0.4645, 5e-4);
        // the delay knob: 0.1 s times the level law, 8 frames at the least
        near(shot([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 128.0], 100, 200).2, 8.0, 0.05);
        near(shot([1.0, 0.0, 0.0, 16.0, 0.0, 0.0, 0.0, 128.0], 100, 400).2, 154.17, 0.06);
        near(shot([1.0, 0.0, 0.0, 64.0, 0.0, 0.0, 0.0, 128.0], 100, 1400).2, 1021.68, 0.06);
        // the sweep: 199.42 frames deep at depth 256, 0.51969 turns a second at speed 768, half way up a
        // quarter turn in; the smaller flanger of two reaches 109.71
        near(shot([1.0, 256.0, 768.0, 8.0, 0.0, 0.0, 0.0, 128.0], 21044, 400).2, 171.01, 0.1);
        near(shot([1.0, 256.0, 768.0, 8.0, 0.0, 0.0, 0.0, 128.0], 42430 - 271, 400).2, 270.72, 0.1);
        near(shot([2.0, 256.0, 768.0, 8.0, 0.0, 0.0, 0.0, 128.0], 42430 - 226, 400).2, 71.30 + (199.42 + 109.71) / 2.0, 0.1);
        // full spread starts the lone left flanger half a turn in and slows it to 0.93514 of the pace
        near(shot([1.0, 256.0, 768.0, 8.0, 128.0, 0.0, 0.0, 128.0], 30, 400).2, 270.72, 0.1);
        near(shot([1.0, 256.0, 768.0, 8.0, 128.0, 0.0, 0.0, 128.0], (42430.0f32 / 0.93514) as usize - 71, 400).2, 71.30, 0.1);
    }

    #[test]
    fn finisher_holds_the_ceiling() {
        let mut fx = Fx::new(Kind::Finisher);
        fx.p[1] = 7500.0;
        let out = run(fx, sine, 44100);
        let peak = out.iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!(peak <= 0.98 && peak > 0.8, "peak {peak}");
        assert!(rms(&out[22050..]) > 0.5);
        // at rest it takes a little under 1 dB off and is otherwise clean
        let level = rms(&run(Fx::new(Kind::Finisher), sine, 8192)[4096..]) / rms(&(4096..8192).map(sine).collect::<Vec<_>>());
        assert!((level - 0.894).abs() < 0.01, "level {level}");
    }

    #[test]
    fn morph_equaliser_is_about_flat_and_cuts_bands() {
        let fx = Fx::new(Kind::MorphEq);
        for hz in [100.0f32, 440.0, 1000.0, 1122.0, 5000.0] {
            let tone = move |i: usize| {
                let v = (i as f32 * hz * TAU / 44100.0).sin() * 0.5;
                [v, v]
            };
            let level = rms(&run(fx.clone(), tone, 32768)[16384..]);
            assert!((level / 0.3535 - 1.0).abs() < 0.2, "{hz} Hz comes out at {level}");
        }
        // silence the bands from 250 Hz up on the first curve: 440 Hz mostly goes away
        let mut cut = fx.clone();
        for band in 11..MEQ_BANDS {
            cut.p[MEQ_HEAD + band * 2] = 0.0;
        }
        // (the bands are wide, so the ones left below still pass a little of it)
        let (left, whole) = (rms(&run(cut, sine, 32768)[16384..]), rms(&run(fx, sine, 32768)[16384..]));
        assert!(left < whole * 0.4, "{left} of {whole}");
    }

    #[test]
    fn morph_equaliser_adds_to_the_sound_and_keeps_its_switch_bytes() {
        // in add mode a flat curve changes nothing, at either rate
        for twice in [0.0, 1.0] {
            let mut fx = Fx::new(Kind::MorphEq);
            fx.p[6] = 1.0;
            fx.p[4] = twice;
            let tone = |i: usize| {
                let v = (i as f32 * 3000.0 * TAU / 44100.0).sin() * 0.5;
                [v, v]
            };
            let level = rms(&run(fx, tone, 16384)[8192..]);
            assert!((level / 0.3535 - 1.0).abs() < 0.01, "{twice}: {level}");
        }
        // the two switches are single bytes; the byte after them stays as it was
        let mut fx = Fx::new(Kind::MorphEq);
        fx.p[6] = 1.0;
        fx.p[MEQ_HEAD + 5 * 2] = 1.5;
        let block = fx.encode(None);
        assert_eq!(block.len(), 4143);
        assert_eq!(&block[4136..4140], &[0, 1, 0, 1]);
        let back = Fx::decode("EQUO", &block).unwrap();
        assert_eq!(back.p, fx.p);
    }

    #[test]
    fn the_ready_made_juggler_lines_span_the_bar() {
        for volume in [false, true] {
            for slot in 0..WARP_SLOTS {
                let pts = warp_shape(slot, volume);
                assert!(pts.len() >= 2, "{slot}");
                assert_eq!(pts[0].0, 0.0);
                assert!((pts.last().unwrap().0 - 4.0).abs() < 1e-9, "{slot} {volume}");
                assert!(pts.windows(2).all(|w| w[1].0 >= w[0].0), "{slot} {volume}");
                assert!(pts.iter().all(|q| (0.0..=1.0).contains(&q.1)), "{slot} {volume}");
            }
        }
        // the first-beat repeat reads the first beat four times: one beat in, it looks one beat back
        let fx = Fx::new(Kind::Warp);
        let at = |slot: usize, beat: f64| fx.p[7 + slot * WARP_N + (beat / 4.0 * (WARP_N - 1) as f64).round() as usize];
        assert!((at(4, 1.5) - (1.0 - 1.0 / 8.0)).abs() < 0.01, "{}", at(4, 1.5));
        assert!((at(4, 3.5) - (1.0 - 3.0 / 8.0)).abs() < 0.01, "{}", at(4, 3.5));
        // the beat gate is open in the first half of a beat and shut in the second
        let vol = |slot: usize, beat: f64| fx.p[7 + (WARP_SLOTS + slot) * WARP_N + (beat / 4.0 * (WARP_N - 1) as f64).round() as usize];
        assert!(vol(1, 0.25) > 0.9 && vol(1, 0.75) < 0.1 && vol(1, 2.25) > 0.9, "{} {}", vol(1, 0.25), vol(1, 0.75));
    }

    #[test]
    fn beat_juggler_repeats_and_gates() {
        // a fresh one changes nothing and writes a block that reads back
        let fresh = Fx::new(Kind::Warp);
        assert_eq!(fresh.p.len(), 7 + 72 * WARP_N);
        let block = fresh.encode(None);
        assert_eq!(block.len(), 13032);
        assert_eq!(Fx::decode("Gross Beat", &block).unwrap(), fresh);
        // time slot 0: from beat 2 on, read two beats back; volume slot 0: shut for the last beat
        let mut fx = fresh.clone();
        for i in 0..WARP_N {
            let beat = i as f32 / 128.0;
            fx.p[7 + i] = if beat >= 2.0 { 0.75 } else { 1.0 };
            fx.p[7 + 36 * WARP_N + i] = if beat >= 3.0 { 0.0 } else { 1.0 };
        }
        fx.p[4] = 0.0;
        fx.p[5] = 0.0;
        let cx = Ctx { sr: 44100.0, step: 6000.0, clock: Default::default() };
        let src = |i: f32| 0.4 * (i * std::f32::consts::TAU / 400.0).sin();
        // left alone it only runs fifteen frames late
        let mut u = Unit::new(fresh, cx.sr);
        let mut io: Vec<[f32; 2]> = (0..4096).map(|i| [src(i as f32); 2]).collect();
        u.process(&mut io, &cx);
        assert!(io[..15].iter().all(|f| f[0] == 0.0));
        assert!((15..4096).all(|i| io[i][0] == src((i - 15) as f32)));
        // with a line drawn it runs a fraction later still
        let late = 15.0 + 15.0 / 64.0;
        let mut u = Unit::new(fx, cx.sr);
        let mut io: Vec<[f32; 2]> = (0..96000).map(|i| [src(i as f32); 2]).collect();
        for c in io.chunks_mut(512) {
            u.process(c, &cx);
        }
        assert!((io[1000][0] - src(1000.0 - late)).abs() < 2e-3, "{}", io[1000][0] - src(1000.0 - late));
        assert!((io[60000][0] - src(60000.0 - 48000.0 - late)).abs() < 2e-3, "beat 2.5 replays beat 0.5");
        assert!(io[80000][0].abs() < 1e-3, "the gate is shut in the last beat");
        // a jump in time never clicks: the output moves no faster than the sound itself does
        let most = io.windows(2).take(70000).map(|w| (w[1][0] - w[0][0]).abs()).fold(0.0, f32::max);
        assert!(most < 0.02, "{most}");
    }

    #[test]
    fn units_do_what_they_say() {
        // balance: hard left silences the right side
        let mut fx = Fx::new(Kind::Balance);
        fx.p[0] = -128.0;
        let out = run(fx, sine, 2048);
        assert!(out.iter().all(|f| f[1] == 0.0) && rms(&out) > 0.1);

        // delay: an impulse comes back one delay time later
        let mut fx = Fx::new(Kind::Delay);
        fx.p[4] = 48.0;
        fx.p[6] = 0.0;
        let step = (44100.0f32 * 60.0 / 130.0 / 4.0) as usize;
        let out = run(fx, |i| if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] }, step * 3);
        assert!(out[step][0].abs() > 0.9, "{}", out[step][0]);
        assert!(out[step * 2][0].abs() > 0.2 && out[step * 2][0].abs() < 0.9);
        assert!(out[step / 2][0].abs() < 1e-6);

        // low-pass at a low cutoff removes most of a 440 Hz tone
        let mut fx = Fx::new(Kind::FastLp);
        fx.p[0] = 100.0;
        assert!(rms(&run(fx, sine, 8192)[4096..]) < 0.05);

        // compressor: a loud tone comes out quieter, a quiet one untouched
        let mut fx = Fx::new(Kind::Comp);
        fx.p[0] = -300.0;
        fx.p[1] = 100.0;
        let loud = rms(&run(fx.clone(), sine, 16384)[8192..]);
        assert!(loud < 0.1, "{loud}");

        // limiter: nothing above the ceiling
        let mut fx = Fx::new(Kind::Limiter);
        fx.p[0] = 2000.0;
        fx.p[2] = 500.0;
        let out = run(fx, sine, 8192);
        assert!(out.iter().all(|f| f[0].abs() <= 0.5001));

        // band maximiser: a quiet tone passes close to untouched, a loud one is held near full level, and the
        // fresh block writes back unchanged
        let fx = Fx::new(Kind::Maxi);
        assert_eq!(fx.encode(None), maxi_fresh());
        let quiet = rms(&run(fx.clone(), sine, 16384)[8192..]);
        assert!((quiet / rms(&(8192..16384).map(sine).collect::<Vec<_>>()) - 1.0).abs() < 0.1, "{quiet}");
        let mut loud = fx;
        loud.p[7 + 3 * 13 + 1] = 2000.0;
        loud.p[7 + 1] = 2000.0;
        loud.p[20 + 1] = 2000.0;
        let out = run(loud, sine, 32768);
        let peak = out[16384..].iter().fold(0f32, |m, f| m.max(f[0].abs()));
        assert!(peak < 1.7 && peak > 0.7, "{peak}");

        // plush reverb: fresh block reads back, a click leaves a tail, dry off and wet off behave
        let fx = Fx::new(Kind::Plush);
        assert_eq!(Fx::decode("LuxeVerb", &fx.encode(None)).unwrap(), fx);
        let click = |i: usize| if i == 0 { [1.0f32; 2] } else { [0.0; 2] };
        let mut wet_only = fx.clone();
        wet_only.p[32] = -600.0;
        let out = run(wet_only, click, 22050);
        assert_eq!(out[0][0], 0.0);
        assert!(rms(&out[2000..]) > 1e-5);
        let mut dry_only = fx.clone();
        dry_only.p[33] = -600.0;
        let out = run(dry_only, click, 4096);
        assert!((out[0][0] - 1.0).abs() < 1e-3 && rms(&out[64..]) < 1e-6);
        // held, the tail of a burst stays up where it would have died away
        let burst = |i: usize| if i < 4000 { [((i * 7919 % 1000) as f32 / 500.0 - 1.0) * 0.5; 2] } else { [0.0; 2] };
        let mut short = fx.clone();
        short.p[32] = -600.0;
        short.p[4] = 3000.0;
        let free = run(short.clone(), burst, 132_300);
        // the hold comes on with sound already in the tank
        let mut u = Unit::new(short.clone(), 44100.0);
        let cx = Ctx { sr: 44100.0, step: 5512.5, clock: Default::default() };
        let mut a: Vec<[f32; 2]> = (0..22_050).map(burst).collect();
        u.process(&mut a, &cx);
        u.fx.p[13] = 1.0;
        let mut b = vec![[0.0f32; 2]; 110_250];
        u.process(&mut b, &cx);
        assert!(rms(&b[88_200..]) > 20.0 * rms(&free[110_250..]), "{} {}", rms(&b[88_200..]), rms(&free[110_250..]));
        // the loop sends the tail round an octave up: more comes back than without it
        let mut looped = short.clone();
        looped.p[15] = 1.0;
        looped.p[19] = 900.0;
        let with = run(looped, burst, 132_300);
        assert!(rms(&with[66_150..]) > 1.5 * rms(&free[66_150..]) && with.iter().all(|f| f[0].abs() < 4.0));
        // the follower turns the tail down while the playing is loud
        let mut duck = fx.clone();
        duck.p[32] = -600.0;
        duck.p[23] = -1000.0;
        duck.p[24] = -4000.0;
        let noise = |i: usize| [((i * 7919 % 1000) as f32 / 500.0 - 1.0) * 0.5; 2];
        let plain_tail = run({ let mut f = fx.clone(); f.p[32] = -600.0; f }, noise, 44_100);
        let ducked = run(duck, noise, 44_100);
        assert!(rms(&ducked[22_050..]) < 0.3 * rms(&plain_tail[22_050..]));

        // pad effects: fresh block reads back, every effect stays bounded, bypass passes
        let fx = Fx::new(Kind::Pad);
        assert_eq!(Fx::decode("Effector", &fx.encode(None)).unwrap(), fx);
        for k in 0..12 {
            let mut e = fx.clone();
            e.p[0] = (k as f32 / 11.0 * 10000.0).round();
            let out = run(e, sine, 22050);
            assert!(out.iter().all(|f| f[0].is_finite() && f[0].abs() < 8.0), "effect {k}");
        }
        let mut e = fx;
        e.p[1] = 10000.0;
        assert_eq!(run(e, sine, 64)[10], sine(10));

        // voice tuner: a tone a little sharp of A is pulled onto A
        let fx = Fx::new(Kind::Tuner);
        assert_eq!(Fx::decode("Pitcher", &fx.encode(None)).unwrap(), fx);
        let sharp = |i: usize| [(i as f32 * 450.0 * TAU / 44100.0).sin() * 0.5; 2];
        let out = run(fx, sharp, 88200);
        let ups = out[44100..].windows(2).filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0).count() as f32;
        assert!((ups - 440.0).abs() < 3.0, "{ups}");
        // and with the colour kept it lands on A as well
        let mut keep = Fx::new(Kind::Tuner);
        keep.p[7] = 10000.0;
        let out = run(keep, sharp, 88200);
        let ups = out[44100..].windows(2).filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0).count() as f32;
        assert!((ups - 440.0).abs() < 3.0, "{ups}");

        // grain shifter: an octave up doubles the number of zero crossings
        let mut fx = Fx::new(Kind::GrainShift);
        fx.p[0] = 1200.0;
        let cross = |v: &[[f32; 2]]| v.windows(2).filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0).count() as f32;
        let out = run(fx, sine, 44100);
        let plain: Vec<_> = (0..44100).map(sine).collect();
        let r = cross(&out[8192..]) / cross(&plain[8192..]);
        assert!((r - 2.0).abs() < 0.15, "{r}");

        // band splitter: flat when the levels are level, quieter with the band holding the tone turned off
        let fx = Fx::new(Kind::BandSplit);
        assert!((rms(&run(fx.clone(), sine, 16384)[8192..]) / rms(&plain[8192..16384]) - 1.0).abs() < 0.01);
        let mut cut = fx;
        cut.p[7] = -1800.0;
        assert!(rms(&run(cut, sine, 16384)[8192..]) < 0.5 * rms(&plain[8192..16384]));
        // a band sent its own way leaves the track and waits in the tap
        let mut gone = Fx::new(Kind::BandSplit);
        gone.p[10] = 1.0;
        gone.p[11] = 1.0;
        gone.p[12] = 1.0;
        assert_eq!(Fx::decode("Frequency Splitter", &gone.encode(None)).unwrap(), gone);
        let mut u = Unit::new(gone, 44100.0);
        let mut io: Vec<[f32; 2]> = (0..4096).map(sine).collect();
        u.process(&mut io, &Ctx { sr: 44100.0, step: 5512.5, clock: Default::default() });
        assert!(rms(&io) < 1e-6 && u.tap.len() == 4096 * 3 && rms(&u.tap) > 0.05);

        // bucket chorus: sounds, stays bounded
        let out = run(Fx::new(Kind::BucketChorus), sine, 16384);
        assert!(rms(&out[8192..]) > 0.05 && out.iter().all(|f| f[0].abs() < 2.0));

        // echo bank: the first bank repeats a click after its echo time
        let fx = Fx::new(Kind::EchoBank);
        assert_eq!(Fx::decode("Fruity Delay Bank", &fx.encode(None)).unwrap(), fx);
        let out = run(fx, |i| if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] }, 44100);
        let at = out.iter().skip(1).position(|f| f[0].abs() > 0.4).map(|i| i + 1);
        assert!(at.is_some_and(|i| i > 1000), "{at:?}");
        // a low shelf turned right down in front of the echoes thins a low tone, and a steeper low-pass far below
        // the tone removes more of it than a gentle one
        let wet = |set: &dyn Fn(&mut Fx)| {
            let mut fx = Fx::new(Kind::EchoBank);
            fx.p[0] = 0.0;
            set(&mut fx);
            rms(&run(fx, sine, 88200)[44100..])
        };
        let flat = wet(&|_| {});
        let shelf = wet(&|f| {
            f.p[4 + 16] = 5.0;
            f.p[4 + 2] = 65536.0;
        });
        assert!(flat > 0.01 && shelf < 0.3 * flat, "{flat} {shelf}");
        let low = |slope: f32| {
            wet(&|f| {
                f.p[4 + 16] = 1.0;
                f.p[4 + 2] = 12000.0;
                f.p[165] = slope;
            })
        };
        assert!(low(2.0) < 0.5 * low(0.0), "{} {}", low(0.0), low(2.0));
        // grain: the echoes come out chopped, so quieter overall
        let chopped = wet(&|f| f.p[167] = 2048.0);
        assert!(chopped < 0.8 * flat && chopped > 0.2 * flat, "{flat} {chopped}");

        // equaliser: a 18 dB bell at the tone's frequency makes it much louder, a low-pass far below removes it
        let mut fx = Fx::new(Kind::Eq);
        fx.p[3] = 1800.0;
        fx.p[10] = (440f32 / 20.0).ln() / 1000f32.ln() * 65536.0;
        let boosted = rms(&run(fx, sine, 16384)[8192..]);
        assert!(boosted > 0.35 * 6.0, "{boosted}");
        let mut fx = Fx::new(Kind::Eq);
        fx.p[21] = 1.0;
        fx.p[7] = 4000.0;
        assert!(rms(&run(fx, sine, 16384)[8192..]) < 0.02);

        // reverb: rings on after the input stops, and dies away
        let mut fx = Fx::new(Kind::Reverb);
        fx.p[12] = 128.0;
        let out = run(fx, |i| if i < 2000 { sine(i) } else { [0.0, 0.0] }, 44100 * 6);
        assert!(rms(&out[6000..16000]) > 1e-3);
        assert!(rms(&out[44100 * 5..]) < 1e-4);
        assert!(out.iter().all(|f| f[0].is_finite() && f[0].abs() < 4.0));
    }

    #[test]
    fn the_widener_keeps_the_mono_sum_and_the_shaper_lifts_onsets() {
        let out = run(Fx::new(Kind::Spread), sine, 8192);
        let worst = out.iter().enumerate().skip(4096).map(|(i, f)| (0.5 * (f[0] + f[1]) - sine(i)[0]).abs()).fold(0.0, f32::max);
        assert!(worst < 2e-3, "mono sum moved by {worst}");
        assert!(out.iter().skip(4096).any(|f| (f[0] - f[1]).abs() > 0.02), "no width was added");
        // bursts of tone: more attack makes the start of each burst louder than the plain signal
        let burst = |i: usize| {
            let v = if i % 8000 < 2000 { (i as f32 * 0.2).sin() * 0.3 } else { 0.0 };
            [v, v]
        };
        let mut fx = Fx::new(Kind::Transient);
        fx.p[0] = 160.0;
        let out = run(fx, burst, 32000);
        let peak = |a: usize, b: usize| out[a..b].iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!(peak(16000, 16300) > 0.45, "{}", peak(16000, 16300));
        assert!(peak(17850, 18000) < 0.36, "{}", peak(17850, 18000));
    }

    #[test]
    fn the_onset_shaper_matches_the_measured_tone_steps() {
        // a 495 Hz tone stepping 0.05 -> 0.4 -> 0.05, split at its lowest and only the high side heard:
        // peaks per 200 frames read from reference renders
        let tone = |i: usize| {
            let v = (i as f32 * TAU * 495.0 / 44100.0).sin() * if (22050..44100).contains(&i) { 0.4 } else { 0.05 };
            [v, v]
        };
        let shaped = |knob: usize, v: f32| {
            let mut fx = Fx::new(Kind::Transient);
            fx.p[6] = 0.0;
            fx.p[7] = 160.0;
            fx.p[knob] = v;
            run(fx, tone, 50000)
        };
        let peak = |out: &[[f32; 2]], at: usize| out[at..at + 200].iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        let out = shaped(0, 0.0);
        for (at, want) in [(22050, 0.2397), (22250, 0.2950), (22450, 0.3291), (22850, 0.3518), (25250, 0.3665)] {
            assert!((peak(&out, at) - want).abs() < 0.003, "attack down at {at}: {} for {want}", peak(&out, at));
        }
        let out = shaped(0, 160.0);
        for (at, want) in [(22250, 0.5460), (22450, 0.4760), (22850, 0.4341), (25250, 0.4164)] {
            assert!((peak(&out, at) - want).abs() < 0.008, "attack up at {at}: {} for {want}", peak(&out, at));
        }
        let out = shaped(2, 0.0);
        for (at, want) in [(44100, 0.0522), (44300, 0.0470), (44500, 0.0410), (44900, 0.0339), (45700, 0.0223)] {
            assert!((peak(&out, at) - want).abs() < 0.0015, "release down at {at}: {} for {want}", peak(&out, at));
        }
        let out = shaped(2, 160.0);
        for (at, want) in [(22250, 0.3691), (44300, 0.0497), (44900, 0.0565), (45700, 0.0721)] {
            assert!((peak(&out, at) - want).abs() < 0.003, "release up at {at}: {} for {want}", peak(&out, at));
        }
        // drive bends the shaped part towards a ceiling of one fifteenth at full, and is out of the way at rest
        assert!((transient_bend(1.0) - 0.7777).abs() < 1e-3 && transient_bend(-4.0) == -1.0);
        let out = shaped(1, 160.0);
        assert!((peak(&out, 30000) - 1.0 / 15.0).abs() < 1e-3, "{}", peak(&out, 30000));
    }

    #[test]
    fn the_chorus_matches_the_measured_click_and_wave_shapes() {
        // all three waves stopped, no delay or depth, wet only, low side of the split at its
        // default corner: figures read from a reference render of a 0.5 click
        let mut fx = Fx::new(Kind::Chorus);
        for (i, v) in [(0, 0.0), (1, 0.0), (3, 0.0), (4, 0.0), (5, 0.0), (9, 1024.0), (11, 1024.0)] {
            fx.p[i] = v;
        }
        let out = run(fx, |i| if i == 0 { [0.5, 0.5] } else { [0.0, 0.0] }, 16);
        for (i, want) in [(1, 0.0), (2, 0.0112), (3, 0.0247), (4, 0.0265), (5, 0.0253), (9, 0.0211)] {
            assert!((out[i][0] - want).abs() < 0.0004, "frame {i}: {} for {want}", out[i][0]);
        }
        // the wave knob picks one of four shapes in thirds of its travel
        assert!((chorus_wave(0.125, 300.0) - 0.75).abs() < 1e-6);
        assert!((chorus_wave(0.125, 342.0) - 0.421875).abs() < 1e-6);
        let third = ((0.25f32 * TAU).sin() + (0.5f32 * TAU).sin() / 3.0 + (0.75f32 * TAU).sin() / 3.0) / CHORUS_PEAK;
        assert!((chorus_wave(0.25, 700.0) - third).abs() < 1e-6);
        assert!((chorus_wave(0.25, 1024.0) - third.powi(3)).abs() < 1e-6);
        assert!((0..1000).all(|i| chorus_wave(i as f32 / 1000.0, 768.0).abs() <= 1.0001));
    }

    #[test]
    fn the_overdrive_follows_the_measured_step_and_limits_before_its_last_filter() {
        // a step of 0.5 into the fully open pre amp with the output level at 20 %: figures read
        // from a reference render (three clipped paths, then one, then the swing below zero)
        let mut fx = Fx::new(Kind::Overdrive);
        fx.p[2] = 10000.0;
        fx.p[5] = 2000.0;
        let out = run(fx, |_| [0.5, 0.5], 64);
        for (i, want) in [(0, 0.0512), (3, 0.1184), (9, 0.04), (15, -0.04), (27, 0.0752)] {
            assert!((out[i][0] - want).abs() < 0.0015, "frame {i}: {} for {want}", out[i][0]);
        }
        // a small click through the x100 switch at full output level: the limit sits before the
        // two smoothing stages, so the first frame is 0.8 squared
        let mut fx = Fx::new(Kind::Overdrive);
        fx.p[2] = 5000.0;
        fx.p[3] = 1.0;
        let out = run(fx, |i| if i == 0 { [0.02, 0.02] } else { [0.0, 0.0] }, 16);
        for (i, want) in [(0, 0.64), (1, 0.6643), (2, 0.7455), (6, -0.4666)] {
            assert!((out[i][0] - want).abs() < 0.015, "frame {i}: {} for {want}", out[i][0]);
        }
    }

    #[test]
    fn the_impulse_unit_rings_on_and_keeps_what_it_does_not_read() {
        let mut block = vec![0u8; 20];
        block[0] = 3;
        block[4..8].copy_from_slice(&0i32.to_le_bytes());
        block[8..12].copy_from_slice(&1000i32.to_le_bytes());
        block.push(1);
        block.push(5);
        block.extend_from_slice(b"a.wav");
        block.extend_from_slice(&[9, 8, 7, 6]);
        assert_eq!(sample_named_in("Fruity Convolver", &block).as_deref(), Some("a.wav"));
        let mut fx = Fx::decode("Fruity Convolver", &block).unwrap();
        assert_eq!(fx.p, vec![0.0, 1000.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1000.0]);
        assert_eq!(fx.encode(Some(&block)), block);
        let mut h = vec![0f32; 2000];
        h[0] = 1.0;
        h[1500] = 1.0;
        fx.ir = Some(Ir { smp: std::sync::Arc::new(crate::sample::Sample::from_mono("h", 44_100, &h)), path: "longer name.wav".into() });
        let out = fx.encode(Some(&block));
        assert_eq!(sample_named_in("Fruity Convolver", &out).as_deref(), Some("longer name.wav"));
        assert_eq!(&out[out.len() - 4..], &[9, 8, 7, 6]);
        let y = run(fx, |i| if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] }, 2000);
        let g = y[0][0];
        assert!(g > 0.25 && (y[1500][0] - g).abs() < 1e-3 && y[700][0].abs() < 1e-4, "{g} {}", y[1500][0]);
    }
    #[test]
    fn the_impulse_reverb_keeps_its_reshaping_settings() {
        let mut fx = Fx::new(Kind::Conv);
        let mut h = vec![0f32; 2000];
        h[0] = 1.0;
        fx.ir = Some(Ir { smp: std::sync::Arc::new(crate::sample::Sample::from_mono("h", 44_100, &h)), path: "h.wav".into() });
        fx.p[0] = 0.0;
        fx.p[1] = 1000.0;
        fx.p[4..].copy_from_slice(&[0.0, 10.0, 0.0, 0.0, 1000.0]);
        let block = fx.encode(None);
        let back = Fx::decode("Fruity Convolver", &block).unwrap();
        assert_eq!(back.p, fx.p);
        assert!(back.raw.is_some());
        // a stored block takes a moved setting and nothing else changes
        let mut moved = back.clone();
        moved.p[7] = -250.0;
        let again = moved.encode(Some(&block));
        assert_eq!(again.len(), block.len());
        assert_eq!(Fx::decode("Fruity Convolver", &again).unwrap().p[7], -250.0);
        assert_eq!(again.iter().zip(&block).filter(|(a, b)| a != b).count(), 4);
        // ten thousandths of a second of delay: the click comes out 441 frames late
        let y = run(fx, |i| if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] }, 1000);
        assert!(y[441][0] > 0.9 && y[0][0].abs() < 1e-4 && y[440][0].abs() < 1e-4, "{} {}", y[441][0], y[0][0]);
    }
    #[test]
    fn the_turntable_turns_holds_and_is_dragged() {
        let ramp: Vec<f32> = (0..44_100).map(|i| i as f32 / 44_100.0).collect();
        let rec = Ir { smp: std::sync::Arc::new(crate::sample::Sample::from_mono("r", 44_100, &ramp)), path: "r.wav".into() };
        let quiet = |_: usize| [0.0f32, 0.0];
        let mut fx = Fx::new(Kind::Deck);
        fx.ir = Some(rec);
        fx.p[2] = 5000.0;
        fx.p[5] = 10000.0;
        // forwards at its own speed: the ramp comes out as it is
        let y = run(fx.clone(), quiet, 2000);
        let (l, _) = balance(0.0);
        assert!((y[1000][0] - ramp[1001] * l).abs() < 1e-3, "{}", y[1000][0]);
        // backwards
        let mut back = fx.clone();
        back.p[0] = 5000.0;
        back.p[1] = 0.0;
        let y = run(back, quiet, 2000);
        assert!(y[1000][0] < y[10][0] && y[10][0] > 0.4 * l);
        // held, the record stands still and is silent; muted it is silent too
        let mut held = fx.clone();
        held.p[0] = 5000.0;
        held.p[6] = 10000.0;
        assert!(run(held.clone(), quiet, 2000).iter().all(|f| f[0].abs() < 1e-4));
        let mut mute = fx.clone();
        mute.p[8] = 10000.0;
        assert!(run(mute, quiet, 2000).iter().all(|f| f[0] == 0.0));
        // moving the position while held drags the record there
        let cx = Ctx { sr: 44100.0, step: 5000.0, clock: Default::default() };
        let mut u = Unit::new(held, cx.sr);
        let mut io = vec![[0.0f32; 2]; 256];
        u.process(&mut io, &cx);
        u.fx.p[0] = 6000.0;
        let mut heard = 0f32;
        for _ in 0..40 {
            io.fill([0.0; 2]);
            u.process(&mut io, &cx);
            heard = heard.max(io.iter().fold(0f32, |m, f| m.max(f[0])));
        }
        assert!(heard > 0.5 * l && heard < 0.61, "{heard}");
        assert!((u.clock - 0.6 * 44_099.0).abs() < 2.0, "{}", u.clock);
        // a new record's path goes into the block and the settings stay
        let block = fx.encode(None);
        assert_eq!(sample_named_in("Fruity Scratcher", &block).as_deref(), Some("r.wav"));
        assert_eq!(Fx::decode("Fruity Scratcher", &block).unwrap().p, fx.p);
        assert_eq!(fx.encode(Some(&block)), block);
    }
    #[test]
    fn the_three_axis_pad_reads_its_block_and_follows_at_its_own_pace() {
        let fresh = Fx::new(Kind::Xyz);
        assert_eq!(fresh.encode(None), xyz_fresh());
        assert_eq!(fresh.line_count(), 6);
        // an output line turned upside down turns the axis upside down, and the block still reads back
        let mut f = fresh.clone();
        let mut line = f.line(1).expect("X out");
        line.pts[0].y = 1.0;
        line.pts[1].y = 0.0;
        assert!(f.set_line(1, &line));
        f.p[0] = 65536.0;
        f.p[9] = 0.0;
        let again = Fx::decode("Fruity X-Y-Z Controller", &f.encode(None)).unwrap();
        assert_eq!(again.p, f.p);
        let cx = Ctx { sr: 48000.0, step: 6000.0, clock: Default::default() };
        let mut u = Unit::new(again, cx.sr);
        let mut io = vec![[0.25f32; 2]; 480];
        u.process(&mut io, &cx);
        assert!(u.out[0] < 0.01 && (u.out[1] - 0.5).abs() < 0.01 && (u.out[2] - 0.5).abs() < 0.01, "{:?}", u.out);
        assert!(io.iter().all(|x| x[0] == 0.25), "the sound passes untouched");
        // with a pace set, a jump is chased rather than taken at once
        let mut u = Unit::new(fresh, cx.sr);
        u.process(&mut io, &cx);
        u.fx.p[2] = 65536.0;
        u.process(&mut io, &cx);
        let early = u.out[2];
        assert!(early > 0.5 && early < 0.75, "{early}");
        for _ in 0..200 {
            u.process(&mut io, &cx);
        }
        assert!(u.out[2] > 0.99, "{}", u.out[2]);
    }

}

#[cfg(test)]
mod pad_glide_tests {
    use super::*;

    #[test]
    fn blocks_from_the_early_versions_are_plain_travels() {
        // ten knobs of the classic reverb, each 0 to 1
        let vals = [0.0f32, 1.0, 0.28, 1.0, 1.0, 0.7, 0.0297, 1.0, 1.0, 0.2614];
        let block: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let fx = Fx::decode("Fruity Reeverb", &block).expect("reads");
        assert_eq!(fx.p[1], 65536.0);
        assert_eq!(fx.p[2], 84.0);
        assert_eq!(fx.p[5], 28.0);
        // untouched, the block goes back as it came; a turned knob changes only its own number
        assert_eq!(fx.encode(Some(&block)), block);
        let mut turned = fx.clone();
        turned.p[2] = 150.0;
        let out = turned.encode(Some(&block));
        assert_eq!(out.len(), block.len());
        assert_eq!(&out[..8], &block[..8]);
        assert_eq!(&out[12..], &block[12..]);
        assert_eq!(Fx::decode("Fruity Reeverb", &out).unwrap().p[2], 150.0);
        // a current block of the same unit still reads the current way
        let now = Fx::new(Kind::OldReverb).encode(None);
        assert_eq!(Fx::decode("Fruity Reeverb", &now).unwrap().p, Fx::new(Kind::OldReverb).p);
    }

    #[test]
    fn the_pad_dot_glides_to_a_new_place_and_jumps_with_the_speed_full_up() {
        let cx = Ctx { sr: 48_000.0, step: 6000.0, clock: Default::default() };
        let run = |speed: f32| {
            let mut f = Fx::new(Kind::Xy);
            f.p[2] = speed;
            let mut u = Unit::new(f, cx.sr);
            let mut io = vec![[0.0f32; 2]; 256];
            u.process(&mut io, &cx);
            u.fx.p[0] = 65536.0;
            for _ in 0..20 {
                u.process(&mut io, &cx);
            }
            let soon = u.sends(0).unwrap();
            for _ in 0..600 {
                u.process(&mut io, &cx);
            }
            (soon, u.sends(0).unwrap())
        };
        let (soon, late) = run(2048.0);
        assert!(soon > 0.52 && soon < 0.95, "{soon}");
        assert!(late > 0.99, "{late}");
        assert!(run(4096.0).0 > 0.999);
    }
}
