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
    Plush,
    Pad,
    Tuner,
    Peak,
    Xy,
    Curve,
    /// Units that leave the sound alone: displays, notes, recorders and senders of control values.
    Thru,
    Conv,
    Rack,
    Voc,
    OldVoc,
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
        params: &[p("Threshold", 4, -600, 0, 0), p("Ratio", 8, 10, 300, 10), p("Gain", 12, -300, 300, 0), p("Attack", 16, 0, 4000, 10), p("Release", 20, 1, 4000, 200), p("Type", 24, 0, 7, 0)],
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
        size: 148,
        fixed: &[(0, 8)],
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
        size: 76,
        fixed: &[(0, 7), (20, 3), (28, 3), (56, 1)],
        params: &[
            p("Gain", 4, 0, 2000, 1000),
            p("Saturation", 8, 0, 1000, 1000),
            p("Ceiling", 12, 0, 2000, 1000),
            p("Attack", 16, 0, 10000, 1115),
            p("Release", 24, 0, 10000, 5000),
            p("Sustain", 32, 0, 10000, 2882),
            p("Comp threshold", 36, 0, 1000, 1000),
            p("Comp knee", 40, 0, 1000, 0),
            p("Comp ratio", 44, 0, 1000, 0),
            p("Comp attack", 48, 0, 10000, 0),
            p("Comp release", 52, 0, 10000, 6666),
            p("Comp sustain", 60, 0, 10000, 650),
            p("Gate gain", 64, 0, 1000, 1000),
            p("Gate threshold", 68, 0, 1000, 0),
            p("Gate release", 72, 0, 10000, 4187),
        ],
    },
    Def {
        kind: Kind::Flanger,
        ident: "Fruity Flangus",
        title: "Flanger",
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
            p("Split balance", 28, 0, 160, 80),
            p("Split frequency", 32, 0, 160, 80),
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
        fixed: &[(8, 1), (120, 50)],
        params: &[
            p("Gain", 4, 0, 65536, 32768),
            p("Limiter", 12, 0, 1, 0),
            p("High state", 16, 0, 2, 0),
            p("High from", 20, 20, 20000, 17000),
            p("High threshold", 24, 0, 65536, 21845),
            p("High ratio", 28, 0, 65536, 45875),
            p("High knee", 32, 0, 65536, 32768),
            p("High attack", 36, 1, 1000, 100),
            p("High release", 40, 100, 10000, 1000),
            p("High gain", 44, 0, 65536, 52429),
            p("Mid state", 48, 0, 2, 0),
            p("Mid to", 52, 20, 20000, 17000),
            p("Mid from", 56, 20, 20000, 6500),
            p("Mid threshold", 60, 0, 65536, 21845),
            p("Mid ratio", 64, 0, 65536, 45875),
            p("Mid knee", 68, 0, 65536, 32768),
            p("Mid attack", 72, 1, 1000, 100),
            p("Mid release", 76, 100, 10000, 1000),
            p("Mid gain", 80, 0, 65536, 52429),
            p("Low state", 84, 0, 2, 0),
            p("Low to", 88, 20, 20000, 6500),
            p("Low threshold", 92, 0, 65536, 21845),
            p("Low ratio", 96, 0, 65536, 45875),
            p("Low knee", 100, 0, 65536, 32768),
            p("Low attack", 104, 1, 1000, 100),
            p("Low release", 108, 100, 10000, 1000),
            p("Low gain", 112, 0, 65536, 52429),
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
        size: 24,
        fixed: &[(0, 14), (20, 1)],
        params: &[p("Dry", 4, 0, 1000, 1000), p("Wet", 8, 0, 1000, 500), p("Input separation", 12, -1000, 1000, 0), p("Wet separation", 16, -1000, 1000, 0)],
    },
    Def { kind: Kind::Rack, ident: "Distructor", title: "Drive Rack", size: 958, fixed: &[], params: &[] },
    Def { kind: Kind::Voc, ident: "Vocodex", title: "Band Vocoder", size: 1182, fixed: &[], params: &[] },
    Def { kind: Kind::OldVoc, ident: "Fruity Vocoder", title: "Classic Vocoder", size: 129, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Wave Candy", title: "Scope Meter", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "ZGameEditor Visualizer", title: "Visual Stage", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Fruity NoteBook 2", title: "Note Pages", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Fruity NoteBook", title: "Note Pad", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Fruity HTML NoteBook", title: "Web Notes", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Control Surface", title: "Control Panel", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Edison", title: "Wave Recorder", size: 0, fixed: &[], params: &[] },
    Def { kind: Kind::Thru, ident: "Fruity Formula Controller", title: "Formula Sender", size: 0, fixed: &[], params: &[] },
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
            p("Switches", 38, 0, 0x0101_0000, 0),
        ],
    },
    Def {
        kind: Kind::Warp,
        ident: "Gross Beat",
        title: "Beat Juggler",
        size: 6960,
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
            p("Ceiling", 32, 0, 10000, 10000),
            p("Amount", 36, 0, 10000, 10000),
            p("Linking", 40, 0, 10000, 9912),
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
            p("Shift", 4100, 0, 256, 128),
            p("Morph", 4104, 0, 7000, 0),
            p("Smooth", 4108, 0, 256, 170),
            p("Volume", 4112, 0, 256, 128),
            p("Add", 4116, 0, 1, 0),
            p("Band width", 4120, 0, 300, 150),
            p("Send only", 4128, 0, 1, 0),
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
        ],
    },
    Def {
        kind: Kind::Xy,
        ident: "Fruity X-Y Controller",
        title: "Pad Controller",
        size: 71,
        fixed: &[],
        params: &[p("X", 4, 0, 65536, 32768), p("Y", 20, 0, 65536, 32768)],
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
        while seg + 1 < n && at > pts[seg].0 {
            seg += 1;
        }
        let v = if n == 1 {
            pts[0].1
        } else {
            let (a, b) = (pts[seg - 1], pts[seg]);
            let w = b.0 - a.0;
            let t = if w > 1e-9 { ((at - a.0) / w).clamp(0.0, 1.0) } else { 1.0 };
            let s = match b.3 {
                1 | 8 | 12 => {
                    if t < 0.5 {
                        0.5 * crate::fm::bend(t * 2.0, b.2)
                    } else {
                        1.0 - 0.5 * crate::fm::bend(2.0 - t * 2.0, b.2)
                    }
                }
                2 => {
                    if t >= 1.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
                9 => 0.5 - 0.5 * (t * PI).cos(),
                10 => t * t * (3.0 - 2.0 * t),
                _ => crate::fm::bend(t, b.2),
            };
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
const MEQ_HEAD: usize = 7;

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

/// A fresh beat juggler block: every slot holds a level line (no time shift, full volume).
fn warp_fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(6960);
    for x in [4i32, 0, 0, 256, 256, 1765, 5000, 0, 0, 65536, 0, 0x0101_0000] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    for _ in 0..2 * WARP_SLOTS {
        for x in [1280i32, 1280, 0, 0, 3, 2, 2] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        for x in [0f64, 4.0] {
            v.extend_from_slice(&x.to_le_bytes());
            v.extend_from_slice(&1f64.to_le_bytes());
            v.extend_from_slice(&[0, 0, 0, 0, 2, 0, 0, 0]);
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
    v.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
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
    DEFS.iter().find(|d| d.ident == ident).map(|d| d.title)
}

/// An effect's settings, in file units, in the order of its table.
#[derive(Debug, Clone, PartialEq)]
pub struct Fx {
    pub kind: Kind,
    pub p: Vec<f32>,
    /// The impulse of a convolution unit.
    pub ir: Option<Ir>,
}

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
    if !ident.eq_ignore_ascii_case("Fruity Convolver") {
        return None;
    }
    conv_parts(state).map(|x| x.0).filter(|s| !s.is_empty())
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
        if kind == Kind::MorphEq {
            return Self::decode(def(kind).ident, &meq_fresh()).expect("our own block reads back");
        }
        if kind == Kind::Rack {
            return Self::decode(def(kind).ident, &crate::fxrack::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Voc {
            return Self::decode(def(kind).ident, &crate::fxvoc::fresh()).expect("our own block reads back");
        }
        if kind == Kind::OldVoc {
            return Self::decode(def(kind).ident, &crate::fxvoc::classic::fresh()).expect("our own block reads back");
        }
        if kind == Kind::Bank {
            return Self::decode(def(kind).ident, &crate::fxbank::fresh()).expect("our own block reads back");
        }
        Self { kind, p: def(kind).params.iter().map(|p| p.def as f32).collect(), ir: None }
    }

    /// Put setting `k` at `v` (0..=1) of its travel, as a controller does. Settings past the unit's listed
    /// ones (drawn tables) are left alone.
    pub fn set_travel(&mut self, k: usize, v: f32) {
        if let (Some(q), Some(slot)) = (def(self.kind).params.get(k), self.p.get_mut(k)) {
            *slot = q.min as f32 + (q.max as f32 - q.min as f32) * v.clamp(0.0, 1.0);
        }
    }

    /// Read the stored block of the effect called `ident`. None when we have no unit for it or the block is
    /// too short to hold its settings.
    pub fn decode(ident: &str, state: &[u8]) -> Option<Self> {
        let d = DEFS.iter().find(|d| d.ident == ident)?;
        if d.kind == Kind::Bank {
            return crate::fxbank::decode(state).map(|p| Self { kind: d.kind, p, ir: None });
        }
        if d.kind == Kind::Rack {
            return crate::fxrack::decode(state).map(|p| Self { kind: d.kind, p, ir: None });
        }
        if d.kind == Kind::Voc {
            return crate::fxvoc::decode(state).map(|p| Self { kind: d.kind, p, ir: None });
        }
        if d.kind == Kind::OldVoc {
            return crate::fxvoc::classic::decode(state).map(|p| Self { kind: d.kind, p, ir: None });
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
            let mut at = maxi_lines(state)?;
            for _ in 0..4 {
                at = drawn_table(state, at, &mut p)?;
            }
            return Some(Self { kind: d.kind, p, ir: None });
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
            meq_tables(state, &mut p)?;
        }
        if d.kind == Kind::Warp {
            for head in warp_lines(state)? {
                drawn_table_n(state, head, WARP_N, &mut p)?;
            }
        }
        let mut ir = None;
        if d.kind == Kind::Conv {
            if let Some((_, Some(w), _, _)) = conv_parts(state) {
                ir = crate::sample::Sample::decode(state[w].to_vec(), "wav").ok().map(|s| Ir { smp: std::sync::Arc::new(s), path: String::new() });
            }
        }
        Some(Self { kind: d.kind, p, ir })
    }

    /// Write the settings into a stored block: `base` when the effect came from a file (everything we do not
    /// model is kept), a fresh block otherwise.
    pub fn encode(&self, base: Option<&[u8]>) -> Vec<u8> {
        if self.kind == Kind::Bank {
            return crate::fxbank::encode(&self.p, base);
        }
        if self.kind == Kind::Rack {
            return crate::fxrack::encode(&self.p, base);
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
                    _ => b.to_vec(),
                }
            }
            Some(b) => b.to_vec(),
            None if self.kind == Kind::Conv && self.ir.as_ref().is_some_and(|i| !i.path.is_empty()) => {
                let mut v = vec![0u8; 20];
                v[0] = 14;
                v.push(1);
                conv_path(&mut v, &self.ir.as_ref().unwrap().path);
                v
            }
            None if self.kind == Kind::Curve => curve_fresh(),
            None if self.kind == Kind::Maxi => maxi_fresh(),
            None if self.kind == Kind::Xy => XY_FRESH.to_vec(),
            None if self.kind == Kind::Warp => warp_fresh(),
            None if self.kind == Kind::MorphEq => meq_fresh(),
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
        out
    }
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

/// Units whose block holds 32-bit floats from 0 to 1; we carry those as 0 to 10000.
fn floats(kind: Kind, off: usize) -> bool {
    kind == Kind::Pad || (kind == Kind::Tuner && off < 144)
}

/// What a unit needs to know about the moment it is running in.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub sr: f32,
    /// Frames in one step (a sixteenth) at the current tempo.
    pub step: f32,
}

/// The classic units run on the processor of their successor. This gives the successor's kind for one of them.
fn successor(kind: Kind) -> Option<Kind> {
    match kind {
        Kind::OldEq => Some(Kind::Eq),
        Kind::OldReverb => Some(Kind::Reverb),
        Kind::OldFlanger => Some(Kind::Flanger),
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
            q[35] = 2000.0 * (p[28].max(1.0) / 128.0).log10();
        }
        Kind::OldReverb => {
            let cut = |v: f32| if v >= 65536.0 { 221.0 } else { 5.0 + v / 65536.0 * 215.0 };
            q[0] = if p[0] <= 0.0 { 19.0 } else { 20.0 + p[0] / 65536.0 * 3000.0 };
            q[1] = cut(p[1]);
            q[2] = p[2];
            q[3] = p[3] / 655.36;
            q[4] = p[4] / 655.36;
            q[5] = (p[6] / 100.0).clamp(1.0, 200.0);
            q[6] = cut(p[7]);
            q[7] = 100.0 + (p[5] - 20.0) * 4.0;
            q[8] = 500.0;
            q[9] = 0.0;
            q[10] = p[8] / 512.0;
            q[11] = p[9] / 1024.0;
            q[12] = p[9] / 512.0;
            q[13] = 0.0;
            q[14] = 0.0;
        }
        Kind::OldFlanger => {
            let sign = |v: f32| if v >= 512.0 { -1.0 } else { 1.0 };
            q[0] = 1.0 + p[0] / 1024.0 * 9.0;
            q[1] = p[1] / 5000.0 * 1024.0;
            q[2] = 1024.0 * ((p[2] / 1000.0).max(0.02) / 0.02).ln() / 250f32.ln();
            q[3] = p[3];
            q[4] = 128.0 * (1.0 - p[4] / 5000.0);
            q[5] = p[6] * 1.28 * sign(p[7]);
            q[6] = p[9] / 5000.0 * 128.0;
            q[7] = p[10] / 5000.0 * 128.0 * sign(p[8]);
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
    bank: Option<Box<crate::fxbank::State>>,
    conv: Option<Box<crate::conv::State>>,
    rack: Option<Box<crate::fxrack::State>>,
    voc: Option<Box<crate::fxvoc::State>>,
    oldvoc: Option<Box<crate::fxvoc::classic::State>>,
}

/// Comb and all-pass times of the pad reverb, in ms (four combs, then two all-passes).
const PAD_MS: [f32; 6] = [29.7, 37.1, 41.1, 43.7, 5.0, 1.7];
/// First two formants of the vowels A E I O U, in Hz.
const VOWELS: [[f32; 2]; 5] = [[800.0, 1150.0], [400.0, 2000.0], [300.0, 2500.0], [450.0, 800.0], [325.0, 700.0]];

pub(crate) fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

/// Knob position 0..=1 to Hz across the audible range.
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

/// Echo time of one bank of the echo bank in frames. Tempo mode counts in parts of a step (480 to a step in
/// the newer layout, 48 in the older one), the free mode in milliseconds.
fn bank_time(q: &[f32], layout: f32, cx: &Ctx) -> f32 {
    let fine = if layout >= 16.0 { 10.0 } else { 1.0 };
    if (q[19] as i32 >> 24) & 1 != 0 {
        q[8] / (48.0 * fine) * cx.step
    } else {
        q[8] / fine * 0.001 * cx.sr
    }
}

/// Reverb line lengths in milliseconds at full room size.
const REV_MS: [f32; 8] = [31.7, 37.1, 41.3, 47.9, 53.3, 61.7, 71.3, 83.9];
const REV_AP_MS: [f32; 4] = [4.7, 3.6, 12.7, 9.3];

impl Unit {
    pub fn new(fx: Fx, sr: f32) -> Self {
        let inner = successor(fx.kind).map(|k| Box::new(Unit::new(Fx::new(k), sr)));
        let mut u = Self { fx, inner, z: [0.0; 64], buf: vec![], pos: 0, ph: 0.0, lines: vec![], lpos: vec![], out: [0.0; 3], bank: None, conv: None, rack: None, voc: None, oldvoc: None };
        match u.fx.kind {
            Kind::Delay | Kind::Echo => u.buf = vec![[0.0; 2]; (sr * 6.0) as usize + 8],
            Kind::Flanger | Kind::Chorus => u.buf = vec![[0.0; 2]; (sr * 0.12) as usize + 8],
            Kind::Swarm => u.buf = vec![[0.0; 2]; (sr * 0.12) as usize + 8],
            Kind::Spread => {
                for ms in [7.3f32, 11.9, 17.1] {
                    u.lines.push(vec![0.0; (ms * sr / 1000.0) as usize + 2]);
                    u.lpos.push(0);
                }
            }
            Kind::Enhancer | Kind::Shaper => u.buf = vec![[0.0; 2]; (sr * 0.6) as usize + 8],
            Kind::BucketChorus => u.buf = vec![[0.0; 2]; (sr * 0.05) as usize + 8],
            // two bars at 40 beats a minute and up
            Kind::Warp => u.buf = vec![[0.0; 2]; (sr * 12.0) as usize + 8],
            Kind::Bank => u.bank = Some(Box::default()),
            Kind::Voc => u.voc = Some(Box::default()),
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
                    let k = crate::conv::Kernel::new(&ir.smp, sr, 512, true);
                    u.conv = Some(Box::new(crate::conv::State::new(std::sync::Arc::new(k))));
                }
            }
            Kind::FreqShift => u.buf = vec![[0.0; 2]; 36],
            Kind::MorphEq => {
                u.lines.push(vec![0.0; MEQ_BANDS * 8]);
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
                    u.lines.push(vec![0.0; ((sr * 2.0) as usize + 8) * 2]);
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
                u.buf = vec![[0.0; 2]; (sr * 1.1) as usize + 8];
                for ms in REV_MS.iter().chain(&REV_AP_MS) {
                    u.lines.push(vec![0.0; (ms * 1.6 * sr / 1000.0) as usize + 8]);
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
            return Unit { fx: q, inner: None, z: [0.0; 64], buf: vec![], pos: 0, ph: 0.0, lines: vec![], lpos: vec![], out: [0.0; 3], bank: None, conv: None, rack: None, voc: None, oldvoc: None }.tail(cx);
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
                (time * (n + 1.0)) as usize + 4096
            }
            Kind::Echo => {
                let fb = (p[11] / 6000.0).clamp(0.0, 0.97);
                let time = echo_time(p, cx);
                let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                (time * (n + 1.0)) as usize + 4096
            }
            Kind::Flanger | Kind::Chorus | Kind::Swarm | Kind::BucketChorus | Kind::BandSplit => (cx.sr * 0.5) as usize,
            Kind::EchoBank => {
                // the longest ring among the banks that are switched on
                let mut most = 0usize;
                for b in 0..8 {
                    let q = &p[4 + b * 20..24 + b * 20];
                    if (q[19] as i32 >> 8) & 1 == 0 {
                        continue;
                    }
                    let fb = (q[6] / 128.0 * p[3] / 128.0).clamp(0.0, 0.995);
                    let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                    most = most.max((bank_time(q, p[164], cx) * (n + 1.0)) as usize);
                }
                most.min((cx.sr * 60.0) as usize) + 4096
            }
            Kind::GrainShift => {
                let fb = (p[9] / 100.0).clamp(0.0, 0.97);
                let time = shift_delay(p, cx);
                let n = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 1.0 };
                (time * (n + 1.0)) as usize + (cx.sr * 0.3) as usize
            }
            Kind::Spread | Kind::Transient | Kind::Tuner => (cx.sr * 0.25) as usize,
            Kind::Peak | Kind::Voc | Kind::OldVoc => (cx.sr * 3.0) as usize,
            Kind::Warp => (cx.step * 32.0) as usize + 64,
            Kind::Bank | Kind::FreeFilter | Kind::BassBoost | Kind::FreqShift | Kind::Finisher | Kind::LowLift | Kind::MorphEq => (cx.sr * 0.25) as usize,
            Kind::Phaser => (cx.sr * 1.5) as usize,
            Kind::Pad => (cx.sr * 6.0) as usize,
            Kind::Reverb => ((p[5] / 10.0 + p[2] / 1000.0 + 0.5) * cx.sr) as usize,
            Kind::Enhancer | Kind::Shaper => (cx.sr * 0.6) as usize,
            Kind::Comp | Kind::MultiComp | Kind::Maxi | Kind::Limiter | Kind::Eq | Kind::Filter | Kind::FastLp | Kind::Overdrive | Kind::Loud => (cx.sr * 0.25) as usize,
            _ => 0,
        }
    }

    /// Mixer tracks this unit listens to besides its own: (modulator, carrier), counted from 1 among the
    /// tracks routed to its insert.
    pub fn side_wants(&self) -> Option<[i32; 2]> {
        (self.fx.kind == Kind::Voc).then(|| crate::fxvoc::selectors(&self.fx.p))
    }
    /// Hand over one block of a listened-to track.
    pub fn side_set(&mut self, which: usize, buf: &[[f32; 2]]) {
        if let Some(st) = &mut self.voc {
            st.side[which].clear();
            st.side[which].extend_from_slice(buf);
            st.side_on[which] = true;
        }
    }

    pub fn process(&mut self, io: &mut [[f32; 2]], cx: &Ctx) {
        if let Some(u) = &mut self.inner {
            translate(self.fx.kind, &self.fx.p, &mut u.fx.p, cx);
            u.process(io, cx);
            return;
        }
        let sr = cx.sr;
        let p = &self.fx.p;
        let z = &mut self.z;
        match self.fx.kind {
            Kind::Balance => {
                let (l, r) = balance(p[0] / 128.0);
                let g = p[1] / 256.0;
                for f in io.iter_mut() {
                    f[0] *= l * g;
                    f[1] *= r * g;
                }
            }
            Kind::SoftClip => {
                let t = (p[0] / 127.0).clamp(0.01, 0.999);
                let post = p[1] / 128.0;
                let room = 1.0 - t;
                for f in io.iter_mut() {
                    for x in f.iter_mut() {
                        let a = x.abs();
                        if a > t {
                            *x = x.signum() * (t + room * ((a - t) / room).tanh());
                        }
                        *x *= post;
                    }
                }
            }
            Kind::FastDist => {
                let pre = (p[0] / 128.0).powi(4);
                let thr = (p[1] / 10.0).clamp(0.05, 1.0);
                let b = p[2] >= 0.5;
                let mix = (p[3] / 128.0).clamp(0.0, 1.0);
                let post = p[4] / 128.0;
                for f in io.iter_mut() {
                    for x in f.iter_mut() {
                        let d = *x * pre;
                        let w = if b { thr * (d / thr).tanh() } else { d.clamp(-thr, thr) };
                        *x = (*x * (1.0 - mix) + w * mix) * post;
                    }
                }
            }
            Kind::Enhancer => {
                let (pl, pr) = balance(p[0] / 128.0);
                let g = p[1] / 256.0;
                let side = 1.0 - p[2] / 96.0;
                let off = (p[3].abs() / 1000.0 * sr).min(self.buf.len() as f32 - 2.0) as usize;
                let left_late = p[3] < 0.0;
                let inv = p[5] as i32;
                let n = self.buf.len();
                for f in io.iter_mut() {
                    self.buf[self.pos] = *f;
                    if off > 0 {
                        let d = self.buf[(self.pos + n - off) % n];
                        if left_late {
                            f[0] = d[0];
                        } else {
                            f[1] = d[1];
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    let m = (f[0] + f[1]) * 0.5;
                    let s = (f[0] - f[1]) * 0.5 * side;
                    let mut l = (m + s) * pl * g;
                    let mut r = (m - s) * pr * g;
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
                let g = svf_g(knob_hz(p[0] / 1024.0), sr);
                let k = 2.0 - (p[1] / 1024.0).clamp(0.0, 1.0) * 1.9;
                let open = p[0] >= 1024.0 && p[1] <= 0.0;
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let (lp, _, _) = svf(&mut z[ch * 2..], f[ch], g, k);
                        if !open {
                            f[ch] = lp;
                        }
                    }
                }
            }
            Kind::Filter => {
                let g = svf_g(knob_hz(p[0] / 1024.0), sr);
                let k = 2.0 - (p[1] / 1024.0).clamp(0.0, 1.0) * 1.9;
                let (l, b, h) = (p[2] / 1024.0, p[3] / 1024.0, p[4] / 1024.0);
                let twice = p[5] >= 0.5;
                // fully open low-pass is a plain wire
                let open = p[0] >= 1024.0 && b == 0.0 && h == 0.0 && l == 1.0;
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let (lp, bp, hp) = svf(&mut z[ch * 4..], f[ch], g, k);
                        let mut y = lp * l + bp * b * k + hp * h;
                        if twice {
                            let (lp, bp, hp) = svf(&mut z[ch * 4 + 2..], y, g, k);
                            y = lp * l + bp * b * k + hp * h;
                        }
                        if !open {
                            f[ch] = y;
                        }
                    }
                }
            }
            Kind::Overdrive => {
                let band = p[0] / 10000.0;
                let g_band = svf_g(200.0 * 40f32.powf(p[1] / 10000.0), sr);
                let amp = (1.0 + p[2] / 10000.0 * 9.0) * if p[3] >= 0.5 { 100.0 } else { 1.0 };
                let pf = (p[4] / 10000.0).clamp(0.0, 1.0);
                let g_post = svf_g(20000.0 * 0.02f32.powf(pf), sr);
                let post = p[5] / 10000.0;
                let idle = p[0] == 0.0 && p[2] == 0.0 && p[3] < 0.5 && pf == 0.0;
                for f in io.iter_mut() {
                    for ch in 0..2 {
                        let x = f[ch];
                        let (_, bp, _) = svf(&mut z[ch * 4..], x, g_band, 1.0);
                        let d = (x + bp * band * 4.0) * amp;
                        let y = if idle { x } else { d.tanh() };
                        let (lp, _, _) = svf(&mut z[ch * 4 + 2..], y, g_post, 1.4);
                        f[ch] = if pf > 0.0 { lp } else { y } * post;
                    }
                }
            }
            Kind::AutoPan => {
                let amount = (p[4] - 64.0) / 64.0;
                let hz = 0.05 * ((p[5] - 27500.0) / 27500.0 * 8.0).exp2();
                let step = hz / sr;
                let shape = p[2] as i32;
                let target = p[3] as i32;
                let vol = p[1] / 256.0;
                let pan = (p[0] - 128.0) / 128.0;
                for f in io.iter_mut() {
                    let lfo = match shape {
                        1 => 1.0 - 4.0 * (self.ph - 0.5).abs(),
                        2 => {
                            if self.ph < 0.5 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                        _ => (self.ph * TAU).sin(),
                    } * amount;
                    self.ph = (self.ph + step).fract();
                    let (mut pn, mut v) = (pan, vol);
                    if amount != 0.0 {
                        if target == 1 {
                            v *= (1.0 + lfo).clamp(0.0, 2.0) * 0.5 + 0.5 * (1.0 - amount.abs());
                        } else {
                            pn = (pn + lfo).clamp(-1.0, 1.0);
                        }
                    }
                    let (l, r) = crate::law::pan(pn);
                    f[0] *= l * v;
                    f[1] *= r * v;
                }
            }
            Kind::Delay => {
                let n = self.buf.len();
                let (il, ir) = balance(p[0] / 128.0);
                let iv = p[1] / 128.0;
                let dry = p[2] / 128.0;
                let fb = (p[3] / 128.0).clamp(0.0, 0.98);
                let time = (p[4] / 48.0 * cx.step).clamp(1.0, n as f32 - 2.0);
                let off = p[5] / 512.0 * 0.5 * time;
                let dl = ((time + (-off).max(0.0)) as usize).clamp(1, n - 1);
                let dr = ((time + off.max(0.0)) as usize).clamp(1, n - 1);
                let mode = p[6] as i32;
                let cut = (p[7] / 128.0).clamp(0.0, 1.0);
                let a = if cut >= 1.0 { 1.0 } else { 1.0 - (-TAU * knob_hz(cut) / sr).exp() };
                for f in io.iter_mut() {
                    let x = [f[0] * il * iv, f[1] * ir * iv];
                    let l = self.buf[(self.pos + n - dl) % n][0];
                    let r = self.buf[(self.pos + n - dr) % n][1];
                    z[0] += a * (l * fb - z[0]);
                    z[1] += a * (r * fb - z[1]);
                    self.buf[self.pos] = match mode {
                        1 => [x[0] + z[1], x[1] + z[0]],
                        2 => [(x[0] + x[1]) * 0.5 + z[1], z[0]],
                        _ => [x[0] + z[0], x[1] + z[1]],
                    };
                    self.pos = (self.pos + 1) % n;
                    *f = [f[0] * dry + l, f[1] * dry + r];
                }
                for v in z[..2].iter_mut() {
                    if v.abs() < 1e-20 {
                        *v = 0.0;
                    }
                }
            }
            Kind::Comp => {
                let thr = p[0] / 10.0;
                let ratio = (p[1] / 10.0).max(1.0);
                let makeup = db(p[2] / 10.0);
                let att = (-1.0 / ((p[3] / 10.0).max(0.05) * 0.001 * sr)).exp();
                let rel = (-1.0 / (p[4].max(1.0) * 0.001 * sr)).exp();
                // types: hard, medium, vintage, soft, then the same four with a slower "R" release
                let knee = match p[5] as i32 % 4 {
                    0 => 0.0,
                    1 => 6.0,
                    2 => 10.0,
                    _ => 16.0,
                };
                for f in io.iter_mut() {
                    let peak = f[0].abs().max(f[1].abs()).max(1e-9);
                    let lv = 20.0 * peak.log10();
                    let over = lv - thr;
                    let red = if knee > 0.0 && over.abs() < knee / 2.0 {
                        (1.0 / ratio - 1.0) * (over + knee / 2.0).powi(2) / (2.0 * knee)
                    } else if over > 0.0 {
                        (1.0 / ratio - 1.0) * over
                    } else {
                        0.0
                    };
                    // z[0] holds the smoothed reduction in dB (negative)
                    let c = if red < z[0] { att } else { rel };
                    z[0] = red + (z[0] - red) * c;
                    let g = db(z[0]) * makeup;
                    f[0] *= g;
                    f[1] *= g;
                }
            }
            Kind::Shaper => {
                let (rl, ll, rr, lr) = (p[0] / 12800.0, p[1] / 12800.0, p[2] / 12800.0, p[3] / 12800.0);
                let n = self.buf.len();
                let off = ((p[4].abs() / 4096.0) * 0.05 * sr).min(n as f32 - 2.0) as usize;
                let left_late = p[4] < 0.0;
                for f in io.iter_mut() {
                    let x = *f;
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
                    *f = y;
                }
            }
            Kind::OldEq | Kind::OldReverb | Kind::OldFlanger | Kind::Plush | Kind::Thru => {}
            Kind::Conv => {
                if let Some(st) = &mut self.conv {
                    let (dry, wet) = (p[0] / 1000.0, p[1] / 1000.0);
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
                if p.len() < 59 + 4 * CURVE_N {
                    return;
                }
                let g_lo = svf_g(split_hz(p[0]), sr);
                let g_hi = svf_g(split_hz(p[2]), sr);
                let (steep_lo, steep_hi) = (p[1] >= 0.5, p[3] >= 0.5);
                let mix = (p[5] / 1000.0).clamp(0.0, 1.0);
                // the low cut knob rests at 20 Hz in the middle; -1 is off
                let cut = if p[6] < 0.0 { None } else { Some(svf_g(20.0 * 4f32.powf((p[6] - 32768.0) / 32768.0), sr)) };
                struct Band {
                    mode: i32,
                    pre: f32,
                    post: f32,
                    width: f32,
                    sat: f32,
                    ceil: f32,
                    att: f32,
                    rel: f32,
                    rms: Option<f32>,
                }
                let band = |b: usize| {
                    let q = &p[7 + b * 13..20 + b * 13];
                    let rel = 0.005 + ((q[9] / 10000.0).powi(2) + (q[7] / 10000.0).powi(2)) * 4.0;
                    Band {
                        mode: q[0] as i32,
                        pre: q[1] / 1000.0,
                        post: q[2] / 1000.0,
                        width: 1.0 - (q[3] / 1000.0).clamp(-1.0, 1.0),
                        sat: (q[4] / 1000.0).clamp(-1.0, 1.0),
                        ceil: (q[5] / 1000.0).max(0.01),
                        att: (-1.0 / ((0.0002 + (q[6] / 10000.0).powi(2) * 0.25) * sr)).exp(),
                        rel: (-1.0 / (rel * sr)).exp(),
                        rms: (q[12] >= 0.5).then(|| (-1.0 / ((0.001 + (q[11] / 10000.0).powi(2) * 4.0) * sr)).exp()),
                    }
                };
                let bands = [band(0), band(1), band(2), band(3)];
                let top = (CURVE_N - 1) as f32;
                // one band: gain in, level follower, the drawn line as an in/out map (its middle is full level),
                // saturation, width, gain out
                let run = |b: usize, s: [f32; 2], z: &mut [f32]| -> [f32; 2] {
                    let k = &bands[b];
                    match k.mode {
                        2 => return [0.0; 2],
                        3 => return s,
                        _ => {}
                    }
                    let mut s = [s[0] * k.pre, s[1] * k.pre];
                    if k.mode == 0 {
                        let peak = s[0].abs().max(s[1].abs());
                        let lv = match k.rms {
                            Some(c) => {
                                let sq = (s[0] * s[0] + s[1] * s[1]) * 0.5;
                                z[1] = sq + (z[1] - sq) * c;
                                // a sine at full level reads as full level
                                (z[1] * 2.0).sqrt()
                            }
                            None => peak,
                        };
                        let c = if lv > z[0] { k.att } else { k.rel };
                        z[0] = lv + (z[0] - lv) * c;
                        let at = (z[0] * 0.5).clamp(1.0 / 512.0, 1.0);
                        let table = &p[59 + b * CURVE_N..59 + (b + 1) * CURVE_N];
                        let x = at * top;
                        let i = (x as usize).min(CURVE_N - 2);
                        let out = table[i] + (table[i + 1] - table[i]) * (x - i as f32);
                        let g = (out / at).min(64.0);
                        s = [s[0] * g, s[1] * g];
                    }
                    if k.sat != 0.0 {
                        let thr = k.ceil * (1.0 - k.sat.abs());
                        let room = (k.ceil - thr).max(1e-6);
                        for v in s.iter_mut() {
                            let a = v.abs();
                            if a > thr {
                                let t = (a - thr) / room;
                                let bent = if k.sat < 0.0 { t.tanh() } else { 1.0 - (-t).exp() };
                                *v = v.signum() * (thr + room * bent);
                            }
                        }
                    }
                    if k.width != 1.0 {
                        let (m, d) = ((s[0] + s[1]) * 0.5, (s[0] - s[1]) * 0.5 * k.width);
                        s = [m + d, m - d];
                    }
                    [s[0] * k.post, s[1] * k.post]
                };
                const Q: f32 = std::f32::consts::SQRT_2;
                for f in io.iter_mut() {
                    let mut part = [[0f32; 2]; 3];
                    let mut dry = [0f32; 2];
                    for c in 0..2 {
                        let mut x = f[c];
                        if let Some(g) = cut {
                            x = svf(&mut z[c * 10 + 8..], x, g, Q).2;
                        }
                        dry[c] = x;
                        let (mut low, _, _) = svf(&mut z[c * 10..], x, g_lo, Q);
                        if steep_lo {
                            low = svf(&mut z[c * 10 + 2..], low, g_lo, Q).0;
                        }
                        let (_, _, mut high) = svf(&mut z[c * 10 + 4..], x, g_hi, Q);
                        if steep_hi {
                            high = svf(&mut z[c * 10 + 6..], high, g_hi, Q).2;
                        }
                        part[0][c] = low;
                        part[2][c] = high;
                        part[1][c] = x - low - high;
                    }
                    let mut sum = [0f32; 2];
                    for b in 0..3 {
                        let y = run(b, part[b], &mut z[20 + b * 2..22 + b * 2]);
                        sum[0] += y[0];
                        sum[1] += y[1];
                    }
                    let into = [dry[0] + (sum[0] - dry[0]) * mix, dry[1] + (sum[1] - dry[1]) * mix];
                    *f = run(3, into, &mut z[26..28]);
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
            Kind::Xy => self.out = [p[0] / 65536.0, p[1] / 65536.0, 0.0],
            Kind::FreeFilter => {
                // seven shapes on one two-pole stage: low, band, high pass, notch, low shelf, bell, high shelf
                let kind = ((p[0] / 100.0) as i32).clamp(1, 7);
                let hz = (3.966 + 0.00683 * p[1] - 1.22e-6 * p[1] * p[1]).exp();
                let q = 0.707 * 20f32.powf(p[2] / 1024.0);
                let a = 10f32.powf((p[3] / 676.0 - 1.0) * 36.0 / 40.0);
                let w = TAU * (hz / sr).clamp(0.0002, 0.49);
                let (sn, c) = w.sin_cos();
                let al = sn / (2.0 * q);
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
                let co = [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0];
                for f in io.iter_mut() {
                    f[0] = biquad(&co, &mut z[0..2], f[0]);
                    f[1] = biquad(&co, &mut z[2..4], f[1]);
                }
            }
            Kind::BassBoost => {
                // a slow one-pole follower of the input is added back on top of it
                let sel = 140.0 - 130.0 * p[0] / 1024.0;
                let a = 1.0 - (-TAU * (44100.0 / (TAU * (sel + 1.0))) / sr).exp();
                let ratio = p[1] / 1024.0 * 4.0;
                for f in io.iter_mut() {
                    for c in 0..2 {
                        z[c] += a * (f[c] - z[c]);
                        f[c] += z[c] * ratio;
                    }
                }
            }
            Kind::Phaser => {
                let top = 2.0 + 8.0 * p[3] / 1024.0;
                let inc = p[0] / 5000.0 * top / sr;
                let (lo, hi) = (p[1] / 1000.0, p[2] / 1000.0);
                let spread = p[4] / 1024.0;
                let stages = (p[5] as usize).clamp(1, 23);
                let fb = (p[6] / 1000.0).clamp(0.0, 0.98);
                let dry = (p[7] / 1024.0).clamp(0.0, 1.0);
                let gain = p[8] / 2500.0;
                // odd stage counts turn the loop's sign over, so the fed back part keeps ringing the same way
                let sign = if stages % 2 == 0 { 1.0 } else { -1.0 };
                let mut co = [0.0f32; 2];
                for (i, f) in io.iter_mut().enumerate() {
                    if i % 16 == 0 {
                        self.ph += inc * 16.0;
                        if self.ph >= 1.0 {
                            self.ph -= 1.0;
                        }
                        for c in 0..2 {
                            let l = 0.5 - 0.5 * ((self.ph + spread * c as f32) * TAU).cos();
                            let t = (PI * knob_hz(0.15 + 0.8 * (lo + (hi - lo) * l)) / sr).min(1.5).tan();
                            co[c] = (t - 1.0) / (t + 1.0);
                        }
                    }
                    for c in 0..2 {
                        let mut x = f[c] + sign * fb * z[60 + c];
                        for s in 0..stages {
                            let k = c * 24 + s;
                            let y = co[c] * x + z[k];
                            z[k] = x - co[c] * y;
                            x = y;
                        }
                        if !x.is_finite() {
                            x = 0.0;
                        }
                        z[60 + c] = x.clamp(-8.0, 8.0);
                        f[c] = (f[c] * dry + x * (1.0 - dry)) * gain;
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
                // two stages: a clean gain rider holds the level near the ceiling, and a shaper takes off what
                // the rider lets through; the mode and the amount decide how the work is split between them
                let mode = p[0] as i32;
                let drive = db(p[1] / 10000.0 * 24.0);
                let ceil = db((p[2] / 10000.0 - 1.0) * 24.0);
                let amt = (p[3] / 10000.0).clamp(0.0, 1.0);
                let link = (p[4] / 10000.0).clamp(0.0, 1.0);
                let order = p[5].clamp(0.0, 4.0);
                let hard = (p[6] / 9.0).clamp(0.0, 1.0);
                let over = db(match mode {
                    0 => hard * 3.0 * amt,
                    1 => hard * 6.0 * amt,
                    2 => hard * 6.0 * amt,
                    _ => amt * 24.0,
                });
                let att_ms = if mode == 2 { 0.3 + 4.0 * amt } else { 0.3 };
                let att = 1.0 - (-1.0 / (att_ms * 0.001 * sr)).exp();
                let rel = 1.0 - (-1.0 / ((0.05 + 0.05 * order) * sr)).exp();
                let knee = hard.min(0.999);
                for f in io.iter_mut() {
                    let x = [f[0] * drive, f[1] * drive];
                    let both = x[0].abs().max(x[1].abs());
                    for c in 0..2 {
                        let pk = x[c].abs() + (both - x[c].abs()) * link;
                        let want = if pk > over { over / pk } else { 1.0 };
                        // z holds the reduction in force: down fast, back up slowly
                        let g = &mut z[c];
                        let have = 1.0 - *g;
                        let next = if want < have { have + (want - have) * att } else { have + (want - have) * rel };
                        *g = 1.0 - next;
                        let y = x[c] * next;
                        let a = y.abs();
                        let shaped = if a <= knee { a } else if hard >= 0.999 { 1.0 } else { knee + (1.0 - knee) * ((a - knee) / (1.0 - knee)).tanh() };
                        f[c] = y.signum() * shaped.min(1.0) * ceil;
                    }
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
                if p.len() < MEQ_HEAD + 9 * MEQ_BANDS * 2 || self.lines.len() < 2 {
                    return;
                }
                // every band is a pair of band-pass stages; neighbours are summed with opposite signs so they
                // add up where they cross
                let kq = 0.2316 * (p[5] / 100.0).clamp(0.3, 3.0);
                let pos = (p[1] / 1000.0).clamp(0.0, 7.0);
                let (ba, t) = ((pos as usize).min(7), pos.fract());
                let bb = (ba + 1).min(7);
                let shift = (p[0] - 128.0) / 128.0 * 15.0;
                let at = |bank: usize, x: f32, k: usize| -> f32 {
                    let x = x.clamp(0.0, (MEQ_BANDS - 1) as f32);
                    let (i, fr) = (x as usize, x.fract());
                    let j = (i + 1).min(MEQ_BANDS - 1);
                    p[MEQ_HEAD + bank * 62 + i * 2 + k] * (1.0 - fr) + p[MEQ_HEAD + bank * 62 + j * 2 + k] * fr
                };
                // what the flat sum comes to, measured between and on the bands around 1 kHz
                let mut norm = 0.0f32;
                for probe in [1000.0f32, 1122.0] {
                    let (mut re, mut im) = (0.0f32, 0.0f32);
                    for i in 0..MEQ_BANDS {
                        let r = probe / (20.0 * 2f32.powf(i as f32 / 3.0));
                        // one stage: j k r / (1 - r^2 + j k r)
                        let (dr, di) = (1.0 - r * r, kq * r);
                        let den = dr * dr + di * di;
                        let (hr, hi) = (di * di / den, kq * r * dr / den);
                        let (sr2, si2) = (hr * hr - hi * hi, 2.0 * hr * hi);
                        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                        re += sign * sr2;
                        im += sign * si2;
                    }
                    norm += 0.5 * (re * re + im * im).sqrt();
                }
                let out = p[3] / 128.0 / norm.max(0.05);
                let mut g = [0.0f32; MEQ_BANDS];
                let mut want = [[0.0f32; 2]; MEQ_BANDS];
                for i in 0..MEQ_BANDS {
                    g[i] = svf_g(20.0 * 2f32.powf(i as f32 / 3.0), sr);
                    let x = i as f32 - shift;
                    let vol = (at(ba, x, 0) * (1.0 - t) + at(bb, x, 0) * t) * at(8, i as f32, 0);
                    let pan = (at(ba, x, 1) * (1.0 - t) + at(bb, x, 1) * t + at(8, i as f32, 1) - 1.0).clamp(0.0, 2.0);
                    let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                    let lv = vol * vol * out * sign;
                    want[i] = [lv * (2.0 - pan).min(1.0), lv * pan.min(1.0)];
                }
                let (st, rest) = self.lines.split_at_mut(1);
                let (st, cur) = (&mut st[0], &mut rest[0]);
                if cur[0] == -1.0 && cur[1] == -1.0 {
                    for i in 0..MEQ_BANDS {
                        cur[i * 2] = want[i][0];
                        cur[i * 2 + 1] = want[i][1];
                    }
                }
                let glide = 1.0 - (-1.0 / ((0.002 + p[2] / 170.0 * 0.08) * sr)).exp();
                for f in io.iter_mut() {
                    let mut acc = [0.0f32; 2];
                    for i in 0..MEQ_BANDS {
                        for c in 0..2 {
                            let k = i * 8 + c * 4;
                            let (_, b1, _) = svf(&mut st[k..k + 2], f[c], g[i], kq);
                            let (_, b2, _) = svf(&mut st[k + 2..k + 4], b1 * kq, g[i], kq);
                            let lv = &mut cur[i * 2 + c];
                            *lv += glide * (want[i][c] - *lv);
                            acc[c] += b2 * kq * *lv;
                        }
                    }
                    *f = acc;
                }
            }
            Kind::Bank => {
                if let Some(st) = &mut self.bank {
                    crate::fxbank::process(p, st, io, cx);
                }
            }
            Kind::Rack => {
                if let Some(st) = &mut self.rack {
                    crate::fxrack::process(p, st, io, cx);
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
                let bar = (cx.step * 16.0).max(1.0);
                let span = (cx.step * 32.0).min(n as f32 - 4.0);
                let table = |slot: f32, second: bool| {
                    let k = (slot.round().clamp(0.0, WARP_SLOTS as f32 - 1.0)) as usize + if second { WARP_SLOTS } else { 0 };
                    let from = 7 + k * WARP_N;
                    p.get(from..from + WARP_N)
                };
                let (Some(time), Some(vol)) = (table(p[0], false), table(p[1], true)) else { return };
                let (tmix, vmix) = ((p[2] / 256.0).clamp(0.0, 1.0), (p[3] / 256.0).clamp(0.0, 1.0));
                let coef = |v: f32| {
                    let ms = 500.0 * (v / 8192.0).clamp(0.0, 1.0).powi(2);
                    1.0 - (-1.0 / ((ms / 1000.0 * sr).max(1.0))).exp()
                };
                let (up, down) = (coef(p[4]), coef(p[5]));
                let fade = 1.0 / (0.003 * sr);
                let buf = &mut self.buf;
                let read = |buf: &Vec<[f32; 2]>, pos: usize, back: f32| {
                    let at = pos as f32 - back.clamp(0.0, span);
                    let at = if at < 0.0 { at + n as f32 } else { at };
                    let i = at as usize % n;
                    let w = at - at.floor();
                    let (a, b) = (buf[i], buf[(i + 1) % n]);
                    [a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w]
                };
                for f in io.iter_mut() {
                    buf[self.pos] = *f;
                    let x = self.ph * (WARP_N - 1) as f32;
                    let i = (x as usize).min(WARP_N - 2);
                    let w = x - i as f32;
                    // a drawn jump stays a jump; a slope is followed smoothly
                    let (a, b) = (time[i], time[i + 1]);
                    let y = if (b - a).abs() > 1.0 / 64.0 { a } else { a + (b - a) * w };
                    let back = (1.0 - y) * span;
                    // z[1] is the distance in use, z[2] the one being faded out, z[3] the fade
                    if (back - z[1]).abs() > 48.0 {
                        z[2] = z[1];
                        z[3] = 1.0;
                    }
                    z[1] = back;
                    let mut wet = if back < 0.5 { *f } else { read(buf, self.pos, back) };
                    if z[3] > 0.0 {
                        let old = read(buf, self.pos, z[2]);
                        wet = [wet[0] + (old[0] - wet[0]) * z[3], wet[1] + (old[1] - wet[1]) * z[3]];
                        z[3] = (z[3] - fade).max(0.0);
                    }
                    // z[0] holds how far the gate is closed, so a fresh unit starts open
                    let want = 1.0 - (vol[i] + (vol[i + 1] - vol[i]) * w);
                    z[0] += (want - z[0]) * if want < z[0] { up } else { down };
                    let g = 1.0 - vmix * z[0];
                    *f = [(f[0] + (wet[0] - f[0]) * tmix) * g, (f[1] + (wet[1] - f[1]) * tmix) * g];
                    self.pos = (self.pos + 1) % n;
                    self.ph = (self.ph + 1.0 / bar).fract();
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
                // muted unless the pass switch is on
                if (p[10] as i32 >> 16) & 1 == 0 {
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
                for f in io.iter_mut() {
                    let x = if mono { [(f[0] + f[1]) * 0.5; 2] } else { *f };
                    let mut w = [0f32; 2];
                    for c in 0..2 {
                        let ph = (self.ph + start + lr * c as f32).fract();
                        let l = if sine { 0.5 - 0.5 * (ph * TAU).cos() } else { 1.0 - (2.0 * ph - 1.0).abs() };
                        let d = (t1 + (t2 - t1) * l) * 0.001 * sr;
                        let at = self.pos as f32 + n as f32 - d.clamp(1.0, n as f32 - 4.0);
                        let i = at as usize;
                        let fr = at - i as f32;
                        let v = self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        z[c] += (v - z[c]) * hp;
                        w[c] = v - z[c];
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
                for f in io.iter_mut() {
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
                        if !three {
                            f[c] = low * l_lo + (x - low) * l_hi;
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
                        f[c] = low * l_lo + (x - low - high) * l_mid + high * l_hi;
                    }
                }
            }
            Kind::EchoBank => {
                if p.len() < 165 || self.lines.len() < 8 {
                    return;
                }
                let (dry, wet, input) = (p[0] / 128.0, p[1] / 128.0, p[2] / 128.0);
                let len = self.lines[0].len() / 2;
                struct Bank {
                    on: bool,
                    pan_in: (f32, f32),
                    filt: (i32, f32, f32, bool),
                    echo_filt: (i32, f32, f32),
                    fb: f32,
                    mode: i32,
                    pan_fb: (f32, f32),
                    time: [usize; 2],
                    pan_out: (f32, f32),
                    next: f32,
                }
                let hz = |v: f32| 20.0 * 1000f32.powf((v / 65536.0).clamp(0.0, 1.0));
                let damp = |v: f32| std::f32::consts::SQRT_2 / 20f32.powf(((v - 17500.0) / 48036.0).clamp(-0.3, 1.0));
                let banks: [Bank; 8] = std::array::from_fn(|b| {
                    let q = &p[4 + b * 20..24 + b * 20];
                    let (modes, sw) = (q[18] as i32, q[19] as i32);
                    let t = bank_time(q, p[164], cx);
                    let ofs = q[9] / 48.0 * cx.step;
                    let frames = |d: f32| (d.max(1.0) as usize).min(len - 1);
                    let (il, ir) = balance(q[0] / 128.0);
                    let (ol, or) = balance(q[13] / 128.0);
                    Bank {
                        on: (sw >> 8) & 1 != 0,
                        pan_in: (il * q[1] / 128.0, ir * q[1] / 128.0),
                        filt: (q[16] as i32, svf_g(hz(q[2]), sr), damp(q[3]), modes & 1 != 0),
                        echo_filt: (q[17] as i32, svf_g(hz(q[10]), sr), damp(q[11])),
                        fb: (q[6] / 128.0 * p[3] / 128.0).clamp(0.0, 0.995),
                        mode: (modes >> 8) & 0xff,
                        pan_fb: balance(q[7] / 128.0),
                        time: [frames(t + (-ofs).max(0.0)), frames(t + ofs.max(0.0))],
                        pan_out: (ol * q[14] / 128.0, or * q[14] / 128.0),
                        next: if b < 7 { q[15] / 128.0 } else { 0.0 },
                    }
                });
                // filter kinds 1 to 4: low, band, notch, high; the shelving kinds pass
                let pick = |kind: i32, (lp, bp, hp): (f32, f32, f32), x: f32, k: f32| match kind {
                    1 => lp,
                    2 => bp * k,
                    3 => lp + hp,
                    4 => hp,
                    _ => x,
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
                        let z = &mut z[b * 8..b * 8 + 8];
                        let line = &mut self.lines[b];
                        let pos = self.lpos[b];
                        let mut s = [x[0] * k.pan_in.0 + carry[0], x[1] * k.pan_in.1 + carry[1]];
                        if (1..=4).contains(&k.filt.0) && !k.filt.3 {
                            for c in 0..2 {
                                s[c] = pick(k.filt.0, svf(&mut z[c * 2..], s[c], k.filt.1, k.filt.2), s[c], k.filt.2);
                            }
                        }
                        let e = [line[((pos + len - k.time[0]) % len) * 2], line[((pos + len - k.time[1]) % len) * 2 + 1]];
                        let mut back = match k.mode {
                            0 => [0.0; 2],
                            2 | 3 => [e[1] * k.fb, e[0] * k.fb],
                            _ => [e[0] * k.fb, e[1] * k.fb],
                        };
                        if (1..=4).contains(&k.echo_filt.0) {
                            for c in 0..2 {
                                back[c] = pick(k.echo_filt.0, svf(&mut z[4 + c * 2..], back[c], k.echo_filt.1, k.echo_filt.2), back[c], k.echo_filt.2);
                            }
                        }
                        line[pos * 2] = (s[0] + back[0] * k.pan_fb.0).clamp(-16.0, 16.0);
                        line[pos * 2 + 1] = (s[1] + back[1] * k.pan_fb.1).clamp(-16.0, 16.0);
                        self.lpos[b] = (pos + 1) % len;
                        let mut o = e;
                        if (1..=4).contains(&k.filt.0) && k.filt.3 {
                            for c in 0..2 {
                                o[c] = pick(k.filt.0, svf(&mut z[c * 2..], o[c], k.filt.1, k.filt.2), o[c], k.filt.2);
                            }
                        }
                        sum[0] += o[0] * k.pan_out.0;
                        sum[1] += o[1] * k.pan_out.1;
                        carry = [o[0] * k.next, o[1] * k.next];
                    }
                    *f = [f[0] * dry + sum[0] * wet, f[1] * dry + sum[1] * wet];
                }
            }
            Kind::MultiComp => {
                // stored cutoffs run on a logarithmic knob scale
                let hz = |v: f32| 20.0 * 1000f32.powf((v / 20000.0).clamp(0.0, 1.0));
                let g_lo = svf_g(hz(p[20]), sr);
                let g_hi = svf_g(hz(p[3]), sr);
                // per band: state, threshold dB, slope, knee dB, attack, release, gain
                let band = |b: usize| {
                    let o = [4usize, 13, 21][b];
                    let x = (p[o + 1] / 65536.0).clamp(0.0, 1.0);
                    (
                        p[[2usize, 10, 19][b]] as i32,
                        -60.0 * (p[o] / 65536.0).clamp(0.0, 1.0),
                        x,
                        (p[o + 2] / 65536.0).clamp(0.0, 1.0) * 24.0,
                        (-1.0 / ((p[o + 3] / 10.0).max(0.05) * 0.001 * sr)).exp(),
                        (-1.0 / ((p[o + 4] / 10.0).max(1.0) * 0.001 * sr)).exp(),
                        p[o + 5] / 52429.0,
                    )
                };
                let bands = [band(0), band(1), band(2)];
                let main = p[0] / 32768.0;
                let limit = p[1] >= 0.5;
                const Q: f32 = std::f32::consts::SQRT_2;
                for f in io.iter_mut() {
                    let mut part = [[0f32; 2]; 3];
                    for c in 0..2 {
                        let x = f[c];
                        let (l1, _, _) = svf(&mut z[c * 8..], x, g_lo, Q);
                        let (low, _, _) = svf(&mut z[c * 8 + 2..], l1, g_lo, Q);
                        let (_, _, h1) = svf(&mut z[c * 8 + 4..], x, g_hi, Q);
                        let (_, _, high) = svf(&mut z[c * 8 + 6..], h1, g_hi, Q);
                        part[0][c] = high;
                        part[2][c] = low;
                        part[1][c] = x - low - high;
                    }
                    let mut out = [0f32; 2];
                    for (b, &(state, thr, slope, knee, att, rel, gain)) in bands.iter().enumerate() {
                        let s = part[b];
                        match state {
                            1 => continue,
                            2 => {
                                out[0] += s[0];
                                out[1] += s[1];
                                continue;
                            }
                            _ => {}
                        }
                        let peak = s[0].abs().max(s[1].abs()).max(1e-9);
                        let over = 20.0 * peak.log10() - thr;
                        let want = if knee > 0.0 && over.abs() < knee / 2.0 {
                            -slope * (over + knee / 2.0).powi(2) / (2.0 * knee)
                        } else if over > 0.0 {
                            -slope * over
                        } else {
                            0.0
                        };
                        let e = &mut z[32 + b];
                        let k = if want < *e { att } else { rel };
                        *e = want + (*e - want) * k;
                        let g = db(*e) * gain;
                        out[0] += s[0] * g;
                        out[1] += s[1] * g;
                    }
                    for c in 0..2 {
                        let v = out[c] * main;
                        f[c] = if limit { v.clamp(-1.0, 1.0) } else { v };
                    }
                }
            }
            Kind::Eq => {
                let main = db(p[35] / 100.0);
                for b in 0..7 {
                    let kind = p[21 + b] as i32;
                    let gain = p[b] / 100.0;
                    if kind == 0 || (matches!(kind, 5 | 6 | 7) && gain.abs() < 0.005) {
                        continue;
                    }
                    let c = band_coeffs(kind, knob_hz(p[7 + b] / 65536.0), gain, p[14 + b] / 65536.0, sr);
                    // steeper slopes on the pass types run the section more than once
                    let passes = if matches!(kind, 1 | 3) { (1 + (p[28 + b].abs() as usize)).min(4) } else { 1 };
                    for k in 0..passes.min(2) {
                        let zo = b * 8 + k * 4;
                        for f in io.iter_mut() {
                            f[0] = biquad(&c, &mut z[zo..], f[0]);
                            f[1] = biquad(&c, &mut z[zo + 2..], f[1]);
                        }
                    }
                }
                if main != 1.0 {
                    for f in io.iter_mut() {
                        f[0] *= main;
                        f[1] *= main;
                    }
                }
            }
            Kind::Reverb => {
                let n = self.buf.len();
                let g_low = svf_g(p[0].max(19.0), sr);
                let low_on = p[0] > 20.0;
                let a_high = if p[1] >= 220.0 { 1.0 } else { 1.0 - (-TAU * p[1] * 100.0 / sr).exp() };
                let pre = ((p[2] / 1000.0 * sr) as usize).min(n - 2);
                let size = 0.25 + p[3] / 100.0 * 1.25;
                let diff = p[4] / 100.0 * 0.62;
                let decay = (p[5] / 10.0).max(0.1);
                let a_damp = if p[6] >= 220.0 { 1.0 } else { 1.0 - (-TAU * p[6] * 100.0 / sr).exp() };
                let side = 1.0 - p[9] / 64.0;
                let (dry, early, wet) = (p[10] / 128.0, p[11] / 128.0, p[12] / 128.0);
                let mut len = [0usize; 12];
                let mut gain = [0f32; 8];
                for i in 0..12 {
                    let ms = if i < 8 { REV_MS[i] * size } else { REV_AP_MS[i - 8] };
                    len[i] = ((ms * sr / 1000.0) as usize).clamp(1, self.lines[i].len() - 1);
                    if i < 8 {
                        gain[i] = 10f32.powf(-3.0 * (len[i] as f32 / sr) / decay);
                    }
                }
                let taps = [0.0043f32, 0.0215, 0.0225, 0.0268, 0.0270, 0.0298, 0.0458, 0.0485];
                let tap_g = [0.84f32, 0.50, 0.49, 0.38, 0.38, 0.35, 0.29, 0.27];
                for f in io.iter_mut() {
                    let x = *f;
                    // input: low cut, high cut, predelay
                    let mut s = [x[0], x[1]];
                    for ch in 0..2 {
                        if low_on {
                            let (_, _, hp) = svf(&mut z[ch * 2..], s[ch], g_low, 1.414);
                            s[ch] = hp;
                        }
                        z[4 + ch] += a_high * (s[ch] - z[4 + ch]);
                        s[ch] = z[4 + ch];
                    }
                    self.buf[self.pos] = s;
                    let d = self.buf[(self.pos + n - pre) % n];
                    // early reflections straight off the predelay line
                    let mut er = [0f32; 2];
                    for (k, (&t, &g)) in taps.iter().zip(&tap_g).enumerate() {
                        let o = ((t * size * sr) as usize + pre).min(n - 1);
                        let v = self.buf[(self.pos + n - o) % n];
                        er[k % 2] += v[k % 2] * g;
                        er[(k + 1) % 2] += v[(k + 1) % 2] * g * 0.3;
                    }
                    self.pos = (self.pos + 1) % n;
                    // diffusion: two all-pass sections a side
                    let mut inp = [d[0], d[1]];
                    for ch in 0..2 {
                        for k in 0..2 {
                            let li = 8 + ch * 2 + k;
                            let line = &mut self.lines[li];
                            let lp = &mut self.lpos[li];
                            let m = line.len();
                            let old = line[(*lp + m - len[li]) % m];
                            let v = inp[ch] + old * diff;
                            line[*lp] = v;
                            *lp = (*lp + 1) % m;
                            inp[ch] = old - v * diff;
                        }
                    }
                    // the tank: eight lines, each damped, mixed through a Householder reflection
                    let mut outs = [0f32; 8];
                    let mut sum = 0.0;
                    for i in 0..8 {
                        let m = self.lines[i].len();
                        let o = self.lines[i][(self.lpos[i] + m - len[i]) % m];
                        z[8 + i] += a_damp * (o - z[8 + i]);
                        outs[i] = z[8 + i] * gain[i];
                        sum += outs[i];
                    }
                    let sum = sum * 0.25;
                    let mut w = [0f32; 2];
                    for i in 0..8 {
                        let v = outs[i] - sum + inp[i % 2] * 0.5;
                        let lp = self.lpos[i];
                        self.lines[i][lp] = v;
                        self.lpos[i] = (lp + 1) % self.lines[i].len();
                        w[i % 2] += outs[i];
                    }
                    let (wl, wr) = (w[0] * 0.5, w[1] * 0.5);
                    let m = (wl + wr) * 0.5;
                    let sd = (wl - wr) * 0.5 * side;
                    f[0] = x[0] * dry + er[0] * early * 0.5 + (m + sd) * wet;
                    f[1] = x[1] * dry + er[1] * early * 0.5 + (m - sd) * wet;
                }
            }
            Kind::Limiter => {
                // gain, then a compressor stage, then the limiter against the ceiling
                let gain = (p[0] / 1000.0).powi(2);
                let ceil = (p[2] / 1000.0).max(0.001);
                let att = (-1.0 / ((0.0002 + (p[3] / 10000.0).powi(2) * 0.25) * sr)).exp();
                let rel = (-1.0 / ((0.005 + (p[4] / 10000.0).powi(2) * 4.0) * sr)).exp();
                let c_thr = 20.0 * (p[6] / 1000.0).max(0.001).powi(2).log10();
                let c_ratio = 1.0 + (p[8] / 1000.0).powi(2) * 29.0;
                let c_knee = p[7] / 1000.0 * 24.0;
                let c_att = (-1.0 / ((0.0002 + (p[9] / 10000.0).powi(2) * 0.25) * sr)).exp();
                let c_rel = (-1.0 / ((0.005 + (p[10] / 10000.0).powi(2) * 4.0) * sr)).exp();
                let comp_on = p[8] > 0.0 && p[6] < 1000.0;
                let sat = p[1] / 1000.0;
                for f in io.iter_mut() {
                    let mut l = f[0] * gain;
                    let mut r = f[1] * gain;
                    if comp_on {
                        let lv = 20.0 * l.abs().max(r.abs()).max(1e-9).log10();
                        let over = lv - c_thr;
                        let red = if c_knee > 0.0 && over.abs() < c_knee / 2.0 {
                            (1.0 / c_ratio - 1.0) * (over + c_knee / 2.0).powi(2) / (2.0 * c_knee)
                        } else if over > 0.0 {
                            (1.0 / c_ratio - 1.0) * over
                        } else {
                            0.0
                        };
                        let c = if red < z[1] { c_att } else { c_rel };
                        z[1] = red + (z[1] - red) * c;
                        let g = db(z[1]);
                        l *= g;
                        r *= g;
                    }
                    if sat < 1.0 {
                        let t = sat.max(0.05);
                        l = t * (l / t).tanh();
                        r = t * (r / t).tanh();
                    }
                    let peak = l.abs().max(r.abs());
                    let want = if peak > ceil { ceil / peak } else { 1.0 };
                    // z[0] holds 1 - gain so that silence is the resting state
                    let cur = 1.0 - z[0];
                    let c = if want < cur { att } else { rel };
                    let g = want + (cur - want) * c;
                    z[0] = 1.0 - g;
                    f[0] = (l * g).clamp(-ceil, ceil);
                    f[1] = (r * g).clamp(-ceil, ceil);
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
            Kind::Chorus => {
                let n = self.buf.len();
                let base = (0.004 + p[0] / 1024.0 * 0.026) * sr;
                let depth = p[1] / 5000.0 * 0.006 * sr;
                let stereo = p[2] / 1024.0 * 0.5;
                let wet_only = p[11] >= 512.0;
                // a high-pass on what feeds the voices, so the low end stays where it was
                let cross = p[9] >= 512.0;
                let a = 1.0 - (-TAU * (20.0 * 1000f32.powf(p[10] / 5000.0 * 0.5)) / sr).exp();
                for f in io.iter_mut() {
                    let mut x = *f;
                    if cross {
                        for c in 0..2 {
                            z[8 + c] += a * (x[c] - z[8 + c]);
                            x[c] -= z[8 + c];
                        }
                    }
                    self.buf[self.pos] = x;
                    let mut out = [0.0f32; 2];
                    for v in 0..3 {
                        z[v] += (p[3 + v] / 1000.0).max(0.01) / sr;
                        if z[v] >= 1.0 {
                            z[v] -= 1.0;
                        }
                        let tri = p[6 + v] >= 512.0;
                        for c in 0..2 {
                            let ph = (z[v] + stereo * c as f32 + v as f32 / 3.0).fract();
                            let l = if tri { 1.0 - (2.0 * ph - 1.0).abs() } else { 0.5 - 0.5 * (ph * TAU).cos() };
                            let d = base + depth * l;
                            let at = self.pos as f32 + n as f32 - d;
                            let i = at as usize;
                            let fr = at - i as f32;
                            out[c] += self.buf[i % n][c] * (1.0 - fr) + self.buf[(i + 1) % n][c] * fr;
                        }
                    }
                    self.pos = (self.pos + 1) % n;
                    let dry = if wet_only { 0.0 } else { 0.7 };
                    let wet = if wet_only { 1.0 / 3.0 } else { 0.7 / 3.0 };
                    *f = [f[0] * dry + out[0] * wet + if cross && !wet_only { z[8] * 0.3 } else { 0.0 }, f[1] * dry + out[1] * wet + if cross && !wet_only { z[9] * 0.3 } else { 0.0 }];
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
                let att = (p[0] - 80.0) / 80.0;
                let rel = (p[2] - 80.0) / 80.0;
                let drive = p[1] / 160.0;
                let gain = db((p[3] - 80.0) / 80.0 * 24.0);
                let win = |s: f32| [0.008f32, 0.02, 0.05][(s as usize).min(2)];
                let coef = |t: f32| 1.0 - (-1.0 / (t * sr)).exp();
                let (a_fast, a_slow) = (coef(0.0005), coef(win(p[4])));
                let (r_fast, r_slow) = (coef(win(p[5]) * 2.0), coef(win(p[5]) * 20.0));
                let (dry, wet) = if p[6] <= 80.0 { (p[6] / 80.0, 1.0) } else { (1.0, (160.0 - p[6]) / 80.0) };
                let x = (p[7] - 80.0) / 80.0;
                let hz = if x >= 0.0 { 260.0 * (1000.0f32 / 260.0).powf(x) } else { 260.0 * (5.0f32 / 260.0).powf(-x) };
                let a = 1.0 - (-TAU * hz / sr).exp();
                let smooth = coef(0.002);
                for f in io.iter_mut() {
                    z[0] += a * (f[0] - z[0]);
                    z[1] += a * (f[1] - z[1]);
                    let (hl, hr) = (f[0] - z[0], f[1] - z[1]);
                    let lv = hl.abs().max(hr.abs()) + 1e-6;
                    // one peak follower; a slow riser against it finds onsets, a slow faller finds tails
                    z[2] += if lv > z[2] { a_fast } else { r_fast } * (lv - z[2]);
                    let e = z[2];
                    z[3] = if e > z[3] { z[3] + a_slow * (e - z[3]) } else { e };
                    z[5] = if e > z[5] { e } else { z[5] + r_slow * (e - z[5]) };
                    let up = (20.0 * ((e + 1e-6) / (z[3] + 1e-6)).log10()).clamp(0.0, 24.0);
                    let tail = (20.0 * ((z[5] + 1e-6) / (e + 1e-6)).log10()).clamp(0.0, 24.0);
                    let want = (att * up + rel * tail).clamp(-36.0, 24.0);
                    z[6] += smooth * (want - z[6]);
                    let g = db(z[6]);
                    let (mut wl, mut wr) = (hl * g, hr * g);
                    if drive > 0.0 {
                        let d = 1.0 + drive * 7.0;
                        wl = (wl * d).tanh() / d * (1.0 + drive);
                        wr = (wr * d).tanh() / d * (1.0 + drive);
                    }
                    *f = [(z[0] * dry + wl * wet) * gain, (z[1] * dry + wr * wet) * gain];
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
        for v in self.z.iter_mut() {
            if !v.is_finite() || v.abs() < 1e-20 {
                *v = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(fx: Fx, input: impl Fn(usize) -> [f32; 2], n: usize) -> Vec<[f32; 2]> {
        let cx = Ctx { sr: 44100.0, step: 44100.0 * 60.0 / 130.0 / 4.0 };
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
            if matches!(d.kind, Kind::Delay | Kind::Reverb | Kind::Loud | Kind::Flanger | Kind::Chorus | Kind::Echo | Kind::Spread | Kind::Swarm | Kind::EchoBank | Kind::MultiComp | Kind::Maxi | Kind::BucketChorus | Kind::GrainShift | Kind::OldReverb | Kind::OldFlanger | Kind::Plush | Kind::Pad | Kind::Tuner | Kind::Peak | Kind::Bank | Kind::Rack | Kind::Voc | Kind::OldVoc | Kind::BassBoost | Kind::Phaser | Kind::FreqShift | Kind::FreeFilter | Kind::LowLift | Kind::MorphEq) {
                continue;
            }
            let out = run(Fx::new(d.kind), sine, 8192);
            let worst = out.iter().enumerate().skip(4096).map(|(i, f)| (f[0] - sine(i)[0]).abs()).fold(0.0, f32::max);
            assert!(worst < 2e-3, "{} changes the signal by {worst}", d.title);
        }
    }

    #[test]
    fn blocks_round_trip() {
        for d in DEFS {
            // the filter bank's block is packed; its own module tests the trip
            if matches!(d.kind, Kind::Bank | Kind::Rack | Kind::Voc | Kind::OldVoc) {
                continue;
            }
            let mut fx = Fx::new(d.kind);
            for (v, p) in fx.p.iter_mut().zip(d.params) {
                *v = ((p.min + p.max) / 2) as f32;
            }
            let bytes = fx.encode(None);
            assert_eq!(bytes.len(), d.size);
            assert_eq!(Fx::decode(d.ident, &bytes).as_ref(), Some(&fx), "{}", d.title);
            // unknown bytes of an existing block survive
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
        assert!(rms(&run(fx.clone(), sine, 16384)[8192..]) < 0.03);
        fx.p[0] = 300.0;
        assert!((rms(&run(fx, sine, 16384)[8192..]) - 0.3535).abs() < 0.02);
        let mut ph = Fx::new(Kind::Phaser);
        ph.p[7] = 512.0;
        let out = run(ph, sine, 44100);
        assert!(out.iter().all(|f| f[0].abs() < 4.0));
    }

    #[test]
    fn finisher_holds_the_ceiling() {
        let mut fx = Fx::new(Kind::Finisher);
        fx.p[1] = 7500.0;
        let out = run(fx, sine, 44100);
        let peak = out.iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!(peak <= 1.0001 && peak > 0.8, "peak {peak}");
        assert!(rms(&out[22050..]) > 0.5);
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
        // silence the bands from 250 Hz up on the first curve: 440 Hz goes away
        let mut cut = fx.clone();
        for band in 11..MEQ_BANDS {
            cut.p[MEQ_HEAD + band * 2] = 0.0;
        }
        assert!(rms(&run(cut, sine, 32768)[16384..]) < 0.02);
    }

    #[test]
    fn beat_juggler_repeats_and_gates() {
        // a fresh one changes nothing and writes a block that reads back
        let fresh = Fx::new(Kind::Warp);
        assert_eq!(fresh.p.len(), 7 + 72 * WARP_N);
        let block = fresh.encode(None);
        assert_eq!(block.len(), 6960);
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
        let cx = Ctx { sr: 48000.0, step: 6000.0 };
        let mut u = Unit::new(fx, cx.sr);
        let src = |i: usize| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5;
        let mut io: Vec<[f32; 2]> = (0..96000).map(|i| [src(i); 2]).collect();
        for c in io.chunks_mut(512) {
            u.process(c, &cx);
        }
        assert!((io[1000][0] - src(1000)).abs() < 1e-6);
        assert!((io[60000][0] - src(60000 - 48000)).abs() < 1e-3, "beat 2.5 replays beat 0.5");
        assert!(io[80000][0].abs() < 1e-3, "the gate is shut in the last beat");
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
        assert!(peak < 1.3 && peak > 0.7, "{peak}");

        // plush reverb: fresh block reads back, a click leaves a tail, dry off and wet off behave
        let fx = Fx::new(Kind::Plush);
        assert_eq!(Fx::decode("LuxeVerb", &fx.encode(None)).unwrap(), fx);
        let click = |i: usize| if i == 0 { [1.0f32; 2] } else { [0.0; 2] };
        let mut wet_only = fx.clone();
        wet_only.p[32] = -600.0;
        let out = run(wet_only, click, 22050);
        assert_eq!(out[0][0], 0.0);
        assert!(rms(&out[2000..]) > 1e-5);
        let mut dry_only = fx;
        dry_only.p[33] = -600.0;
        let out = run(dry_only, click, 4096);
        assert!((out[0][0] - 1.0).abs() < 1e-3 && rms(&out[64..]) < 1e-6);

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

        // bucket chorus: sounds, stays bounded
        let out = run(Fx::new(Kind::BucketChorus), sine, 16384);
        assert!(rms(&out[8192..]) > 0.05 && out.iter().all(|f| f[0].abs() < 2.0));

        // echo bank: the first bank repeats a click after its echo time
        let fx = Fx::new(Kind::EchoBank);
        assert_eq!(Fx::decode("Fruity Delay Bank", &fx.encode(None)).unwrap(), fx);
        let out = run(fx, |i| if i == 0 { [1.0, 1.0] } else { [0.0, 0.0] }, 44100);
        let at = out.iter().skip(1).position(|f| f[0].abs() > 0.5).map(|i| i + 1);
        assert!(at.is_some_and(|i| i > 1000), "{at:?}");

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
        assert_eq!(fx.p, vec![0.0, 1000.0, 0.0, 0.0]);
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
        assert!(g > 0.5 && (y[1500][0] - g).abs() < 1e-3 && y[700][0].abs() < 1e-4, "{g} {}", y[1500][0]);
    }
}
