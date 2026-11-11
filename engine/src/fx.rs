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
];

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
}

impl Fx {
    pub fn new(kind: Kind) -> Self {
        Self { kind, p: def(kind).params.iter().map(|p| p.def as f32).collect() }
    }

    /// Read the stored block of the effect called `ident`. None when we have no unit for it or the block is
    /// too short to hold its settings.
    pub fn decode(ident: &str, state: &[u8]) -> Option<Self> {
        let d = DEFS.iter().find(|d| d.ident == ident)?;
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
            _ => need,
        };
        if state.len() < least {
            return None;
        }
        let p = d
            .params
            .iter()
            .map(|p| match state.get(p.off..p.off + 4) {
                Some(b) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                None => p.def as f32,
            })
            .collect();
        Some(Self { kind: d.kind, p })
    }

    /// Write the settings into a stored block: `base` when the effect came from a file (everything we do not
    /// model is kept), a fresh block otherwise.
    pub fn encode(&self, base: Option<&[u8]>) -> Vec<u8> {
        let d = def(self.kind);
        let mut out = match base {
            Some(b) => b.to_vec(),
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
        for (p, v) in d.params.iter().zip(&self.p) {
            if p.off + 4 <= out.len() {
                out[p.off..p.off + 4].copy_from_slice(&(v.round() as i32).to_le_bytes());
            }
        }
        out
    }
}

/// What a unit needs to know about the moment it is running in.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub sr: f32,
    /// Frames in one step (a sixteenth) at the current tempo.
    pub step: f32,
}

/// One running effect: the settings plus whatever memory the processor needs.
pub struct Unit {
    pub fx: Fx,
    z: [f32; 64],
    buf: Vec<[f32; 2]>,
    pos: usize,
    ph: f32,
    lines: Vec<Vec<f32>>,
    lpos: Vec<usize>,
}

fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

/// Knob position 0..=1 to Hz across the audible range.
fn knob_hz(x: f32) -> f32 {
    20.0 * 1000f32.powf(x.clamp(0.0, 1.0))
}

/// State variable filter step: returns (low, band, high).
#[inline]
fn svf(z: &mut [f32], x: f32, g: f32, k: f32) -> (f32, f32, f32) {
    let hp = (x - (k + g) * z[0] - z[1]) / (1.0 + g * (g + k));
    let v1 = g * hp;
    let bp = v1 + z[0];
    z[0] = bp + v1;
    let v2 = g * bp;
    let lp = v2 + z[1];
    z[1] = lp + v2;
    (lp, bp, hp)
}

fn svf_g(hz: f32, sr: f32) -> f32 {
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

/// Reverb line lengths in milliseconds at full room size.
const REV_MS: [f32; 8] = [31.7, 37.1, 41.3, 47.9, 53.3, 61.7, 71.3, 83.9];
const REV_AP_MS: [f32; 4] = [4.7, 3.6, 12.7, 9.3];

impl Unit {
    pub fn new(fx: Fx, sr: f32) -> Self {
        let mut u = Self { fx, z: [0.0; 64], buf: vec![], pos: 0, ph: 0.0, lines: vec![], lpos: vec![] };
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
        let p = &self.fx.p;
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
            Kind::Flanger | Kind::Chorus | Kind::Swarm => (cx.sr * 0.5) as usize,
            Kind::Spread | Kind::Transient => (cx.sr * 0.25) as usize,
            Kind::Reverb => ((p[5] / 10.0 + p[2] / 1000.0 + 0.5) * cx.sr) as usize,
            Kind::Enhancer | Kind::Shaper => (cx.sr * 0.6) as usize,
            Kind::Comp | Kind::Limiter | Kind::Eq | Kind::Filter | Kind::FastLp | Kind::Overdrive | Kind::Loud => (cx.sr * 0.25) as usize,
            _ => 0,
        }
    }

    pub fn process(&mut self, io: &mut [[f32; 2]], cx: &Ctx) {
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
            if matches!(d.kind, Kind::Delay | Kind::Reverb | Kind::Loud | Kind::Flanger | Kind::Chorus | Kind::Echo | Kind::Spread | Kind::Swarm) {
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
}
