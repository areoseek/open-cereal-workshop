//! The additive instrument: every note is a stack of harmonics whose levels are shaped by the patch (timbre, harmonic
//! equaliser, mask, pluck, filter, phaser) and turned into one wave cycle a few hundred times a second.
//!
//! The stored settings are read from the matching generator block. Which field is which control was worked out from
//! value ranges across many project and preset files plus the public manual; every scale below (times, octaves,
//! depths) is our own estimate until measured against the reference (docs/PARITY.md, "Instruments").

use std::f32::consts::TAU;
use std::sync::OnceLock;

/// Frames between spectrum updates; the two neighbouring waves are cross-faded in between.
const TICK: u32 = 128;
/// The larger instrument's way out: seconds a let go note holds on, and seconds of its half cosine fall.
const HOLD: f32 = 245.0 / 44100.0;
const LEAVE: f32 = 420.0 / 44100.0;
const MAX_M: usize = 8192;
const MAX_UNISON: usize = 9;

/// Index of each setting in the instrument's table (`synth::DEFS`).
pub mod ix {
    pub const PHASE: usize = 0;
    pub const LHP: usize = 1;
    pub const SUB: usize = 2;
    pub const EQ: usize = 5;
    pub const MASK: usize = 11;
    pub const MASK_MIX: usize = 23;
    pub const MASK_LFO: usize = 24;
    pub const TREMOLO: usize = 25;
    pub const ATTACK: usize = 26;
    pub const DECAY: usize = 27;
    pub const RELEASE: usize = 28;
    pub const PLUCK: usize = 31;
    pub const PLUCK_MASKED: usize = 34;
    pub const VOLUME: usize = 36;
    pub const VEL_VOLUME: usize = 37;
    pub const PITCH: usize = 38;
    pub const DETUNE: usize = 39;
    pub const GRIT: usize = 40;
    pub const VIBRATO: usize = 41;
    pub const PITCH_LFO: usize = 42;
    pub const PORTAMENTO: usize = 43;
    pub const LEGATO: usize = 44;
    pub const SLIDE_TIME: usize = 45;
    pub const F_ADAPTIVE: usize = 47;
    pub const F_ATTACK: usize = 48;
    pub const F_DECAY: usize = 49;
    pub const F_AMOUNT: usize = 52;
    pub const CUTOFF: usize = 54;
    pub const KEY_TRACK: usize = 57;
    pub const CUTOFF_LFO: usize = 58;
    pub const RESONANCE: usize = 59;
    pub const RES_OFFSET: usize = 61;
    pub const UNI_ORDER: usize = 65;
    pub const UNI_PAN: usize = 66;
    pub const UNI_PITCH: usize = 67;
    pub const UNI_PHASE: usize = 68;
    pub const PH_AMOUNT: usize = 71;
    pub const PH_SCALE: usize = 72;
    pub const PH_WIDTH: usize = 73;
    pub const PH_WIDTH_LFO: usize = 74;
    pub const PH_OFFSET: usize = 75;
    pub const PH_OFFSET_LFO: usize = 76;
    pub const PH_SPEED: usize = 77;
    pub const HZ_AMOUNT: usize = 79;
    pub const HZ_WIDTH: usize = 80;
    pub const HZ_LFO: usize = 81;
    pub const HZ_STRENGTH: usize = 83;
    pub const HZ_TYPE: usize = 84;
    pub const LFO_SHAPE: usize = 86;
    pub const LFO_ATTACK: usize = 88;
    pub const LFO_SPEED: usize = 89;
    pub const CHORUS_TYPE: usize = 90;
    pub const CHORUS_MIX: usize = 91;
    pub const DELAY_TYPE: usize = 92;
    pub const DELAY_VOL: usize = 93;
    pub const DELAY_FB: usize = 94;
    pub const DELAY_TIME: usize = 95;
    pub const REVERB_TYPE: usize = 96;
    pub const REVERB_VOL: usize = 97;
    pub const FILTER_TYPE: usize = 100;
    pub const TIMBRE: usize = 104;
    pub const VEL_ATTACK: usize = 29;
    pub const VEL_RELEASE: usize = 30;
    pub const VEL_PLUCK: usize = 32;
    pub const PLUCK_ALT: usize = 33;
    pub const PLUCK_RELEASE: usize = 35;
    pub const VEL_SLIDE: usize = 46;
    pub const F_ATT_SLOPE: usize = 50;
    pub const F_DEC_SLOPE: usize = 51;
    pub const VEL_ENV: usize = 53;
    pub const CUT_HZ: usize = 55;
    pub const F_WIDTH: usize = 56;
    pub const RES_HZ: usize = 60;
    pub const SELF_OSC: usize = 62;
    pub const RES_ADAPTIVE: usize = 63;
    pub const MOTION: usize = 64;
    pub const UNI_VAR: usize = 69;
    pub const UNI_TYPE: usize = 70;
    pub const PH_KEY: usize = 78;
    pub const VEL_HZ: usize = 82;
    pub const HZ_POS: usize = 85;
    pub const LFO_GLOBAL: usize = 87;
    pub const COMP_MIX: usize = 98;
    pub const VEL_UNI: usize = 99;
    pub const PH_TYPE: usize = 101;
    pub const RES_TYPE: usize = 102;
    pub const COMP_TYPE: usize = 103;
    pub const COUNT: usize = 106;
}

/// The lists behind the settings that pick one of several kinds, as (stored value, our title) in the order
/// a chooser shows them. `None` for a setting that is a plain amount or a switch.
pub fn choices(i: usize) -> Option<&'static [(u8, &'static str)]> {
    Some(match i {
        ix::TIMBRE => &[(0, "Saw"), (1, "Bright saw"), (2, "Brighter saw"), (3, "Shifted saw"), (4, "Shifted saw 2"), (5, "Square saw"), (6, "Square saw 2"), (7, "Burrow"), (8, "Double saw"), (9, "Triple saw"), (10, "Triple saw 2"), (11, "Custom")],
        ix::FILTER_TYPE => &[
            (0, "Classic low pass"), (1, "Classic band pass"), (2, "Classic band stop"), (3, "Classic high pass"), (4, "Coarse low pass"), (5, "Coarse band pass"), (6, "Coarse band stop"), (7, "Coarse high pass"),
            (8, "Mound low pass"), (9, "Stepped low pass"), (10, "Mixed low pass"), (11, "Winding low pass"), (12, "Ridge low pass"), (13, "Seeping low pass"), (14, "Phase sweep"), (15, "Band phase sweep"), (16, "Level band pass"),
        ],
        ix::RES_TYPE => &[(0, "Classic"), (1, "Coil"), (2, "Plinth"), (4, "Reed cap"), (3, "Broad bump"), (5, "Twin cone"), (6, "Pit"), (7, "Tunnel"), (8, "Noise")],
        ix::PH_TYPE => &[(0, "Classic"), (1, "Triangle"), (2, "Ovals"), (4, "Deep"), (3, "Deeper"), (5, "Sheath"), (6, "Pair"), (7, "Falls"), (8, "Box"), (9, "Frequency")],
        ix::UNI_TYPE => &[(0, "Classic"), (1, "Even"), (2, "Smeared")],
        ix::HZ_TYPE => &[(0, "Octave up"), (1, "Octave up +1"), (2, "Octave up +2"), (3, "Harmonic up"), (4, "Harmonic up +1")],
        ix::HZ_POS => &[(0, "Before filter"), (1, "Before phaser"), (2, "Before pluck"), (3, "Last")],
        ix::PH_SCALE => &[(0, "Octaves"), (1, "Hz"), (2, "Harmonics")],
        ix::LFO_SHAPE => &[
            (0, "Sine"), (1, "Smooth saw"), (2, "Smooth pulse"), (3, "Saw"), (4, "Pulse"), (5, "Stairs"), (6, "Triangle"), (7, "Crescent"), (8, "Many sines"),
            (9, "Filter envelope"), (10, "Filter frequency"), (11, "Voice random"), (12, "Velocity"), (13, "Pitch"),
        ],
        ix::CHORUS_TYPE => &[(0, "Off"), (1, "Classic"), (2, "Thick"), (3, "Deep"), (4, "Flanger"), (5, "Flanger 2"), (6, "Stereo"), (7, "Modulation"), (8, "Surround"), (9, "Vibration")],
        ix::DELAY_TYPE => &[(0, "Off"), (1, "Classic"), (2, "Ping-pong"), (3, "Stereo")],
        ix::REVERB_TYPE => &[(0, "Off"), (1, "Classic"), (2, "Large hall"), (3, "Cathedral"), (4, "Venue"), (5, "Drum room"), (6, "Small studio")],
        ix::COMP_TYPE => &[(0, "Off"), (1, "Limiting"), (2, "Warming"), (3, "Heating"), (4, "Distortion")],
        _ => return None,
    })
}

/// The chorus, echo and room after the voices. Each one's type at 0 is off (a fresh one plays dry against
/// the reference, and the first type of each is the only one that does), as is a knob at nothing. The
/// chorus styles and room sizes are our own (the reference names none); the echo is measured
/// and lives in `stackfx`.
pub fn tail(p: &[f32]) -> crate::tailfx::Set {
    use crate::tailfx::{Chorus, Reverb, Set};
    if p.len() < ix::COUNT {
        return Set::default();
    }
    let u = |i: usize| (p[i] / 1000.0).clamp(0.0, 1.0);
    const STYLES: [(f32, f32); 10] = [(0.5, 0.6), (0.3, 0.3), (0.7, 0.4), (0.4, 1.2), (0.8, 0.8), (0.2, 2.5), (0.6, 0.15), (0.9, 0.25), (0.35, 4.0), (1.0, 0.5)];
    const ROOMS: [f32; 7] = [0.5, 0.9, 1.4, 2.0, 2.8, 4.0, 6.0];
    let kind = |i: usize| p[i].max(0.0) as usize;
    let style = STYLES[kind(ix::CHORUS_TYPE).clamp(1, 10) - 1];
    Set {
        chorus: (kind(ix::CHORUS_TYPE) > 0 && u(ix::CHORUS_MIX) > 0.0).then(|| Chorus { depth: style.0, hz: style.1, wet: u(ix::CHORUS_MIX) }),
        // the echo is measured: the time counts 192 to a beat, the feedback knob is the share each repeat keeps
        // and the volume knob follows the curve of the level knobs
        stack: Some(crate::stackfx::Set {
            echo: (kind(ix::DELAY_TYPE) > 0 && u(ix::DELAY_VOL) > 0.0).then(|| crate::stackfx::Echo { kind: kind(ix::DELAY_TYPE) as u8, wet: taper(u(ix::DELAY_VOL)), fb: u(ix::DELAY_FB), steps: p[ix::DELAY_TIME].max(1.0) / 48.0 }),
            squash: (kind(ix::COMP_TYPE) > 0 && u(ix::COMP_MIX) > 0.0).then(|| crate::stackfx::Squash { kind: kind(ix::COMP_TYPE).min(4) as u8, mix: u(ix::COMP_MIX) }),
        })
        .filter(|k| k.any()),
        reverb: (kind(ix::REVERB_TYPE) > 0 && u(ix::REVERB_VOL) > 0.0).then(|| Reverb { secs: ROOMS[kind(ix::REVERB_TYPE).clamp(1, 7) - 1], high_hz: 6000.0, wet: u(ix::REVERB_VOL) * u(ix::REVERB_VOL) }),
        ..Default::default()
    }
}

/// A reading of a measured curve (decibels on an even grid from `from` in steps of 1/80) as a gain.
fn curve(tab: &[f32], from: f32, at: f32) -> f32 {
    let x = ((at - from) * 80.0).clamp(0.0, (tab.len() - 1) as f32);
    let i = (x as usize).min(tab.len() - 2);
    let db = tab[i] + (tab[i + 1] - tab[i]) * (x - i as f32);
    if db <= -119.0 { 0.0 } else { 10f32.powf(db / 20.0) }
}

/// What the filter of the given kind leaves of a partial. `x` is the partial's distance from the filter's
/// nominal frequency and `shift` how far a slope's start sits below it, both in slope widths (octaves over
/// the width in octaves, or the same in plain frequency when the filter is set to work that way).
///
/// All measured on renders of single low notes, one setting at a time: the first kinds fall in a straight
/// line of 36 dB a width, the coarse ones along the fourth power of a quarter cosine wave that reaches
/// nothing after one width; the band kinds sit on the nominal frequency itself.
pub fn filter_gain(kind: u8, x: f32, shift: f32) -> f32 {
    use crate::additive_shapes as t;
    use std::f32::consts::FRAC_PI_2;
    let line = |p: f32| 10f32.powf(-1.8 * p.max(0.0));
    let cos4 = |p: f32| (p.clamp(0.0, 1.0) * FRAC_PI_2).cos().powi(4);
    let (lp, hp, q) = (x + shift, shift - x, x.abs());
    match kind {
        0 => line(lp),
        1 => line(q),
        2 => 1.0 - line(q),
        3 => line(hp),
        4 => cos4(lp),
        5 => cos4(q),
        6 => 1.0 - 2.0 * cos4(q).sqrt() + cos4(q),
        7 => cos4(hp),
        8 => if lp < 2.0 / 3.0 { (lp.max(0.0) * 1.5 * FRAC_PI_2).cos() } else { 0.0 },
        9 => if lp <= -0.1 { 1.0 } else { curve(&t::LEG, -0.1, lp) },
        10 => if lp <= -0.1 { 1.0 } else { curve(&t::MIXED, -0.1, lp) },
        11 => if lp <= -0.1 { 1.0 } else { curve(&t::WINDING, -0.1, lp) },
        12 => if lp >= 0.1 { line(lp) } else { curve(&t::RIDGE, -1.7, lp) },
        13 => 10f32.powf(-0.9 * (lp.clamp(0.0, 1.0) * FRAC_PI_2).sin().powi(2)),
        14 => if x < -1.0 { 1.0 } else { (x * std::f32::consts::PI).cos().powi(4) },
        15 => curve(&t::BAND_SWEEP, 0.0, q),
        _ => cos4((q - 1.0 / 3.0) * 1.5),
    }
}

/// The gain of a knob that cuts and lifts (equaliser bands, volume), `v` -1..1. Measured: silent at the bottom,
/// 0.37 half way down, 2.14 half way up, 4 at the top.
fn boost(v: f32) -> f32 {
    (1.0 + 1.691 * v + 1.015 * v * v + 0.324 * v * v * v).max(0.0)
}

/// The curve the level-like knobs follow (mask steps, harmonic protection): half way is 0.23, a quarter 0.082.
fn taper(x: f32) -> f32 {
    ((2.4 * x.clamp(0.0, 1.0)).exp() - 1.0) / (2.4f32.exp() - 1.0)
}

/// A fall from 1 to 0 over `u` 0..1, bent by `slope` (-1..1): a straight line at 0, an exponential fall that
/// is quick at first when negative, one that holds on and drops at the end when positive.
fn bend(u: f32, slope: f32) -> f32 {
    let k = 13.7 * slope.abs();
    if k < 1e-3 {
        return 1.0 - u;
    }
    let fall = |u: f32| ((-k * u).exp() - (-k).exp()) / (1.0 - (-k).exp());
    if slope < 0.0 { fall(u) } else { 1.0 - fall(1.0 - u) }
}

/// The width of a filter slope in octaves for the width knob (0..1000): half an octave to eight.
pub fn filter_width(knob: f32) -> f32 {
    2.0 * 2f32.powf((knob - 500.0) / 250.0)
}

/// Level of harmonic `k` (1 is the note itself) for the built-in timbres.
fn timbre(kind: u8, k: usize) -> f32 {
    let kf = k as f32;
    let odd = k % 2 == 1;
    // levels measured harmonic by harmonic on each shape; 3 and 4 differ from the plain one in phase only
    match kind {
        1 => 1.414 / (kf + 1.0),
        2 => 2.0 / (kf + 3.0),
        5 => if odd || k == 2 { 1.1 / kf } else { 1.1 / (kf * (kf - 1.0)) },
        6 => if odd || k == 2 { 1.1 / kf } else { 1.1 / (kf * (kf - 1.0).powf(0.2)) },
        7 => 0.566 * (1.0 / kf + 0.79 / (kf * kf) + 1.3 / (kf * kf * kf)),
        8 => if odd { 0.5 / kf } else { 1.49 / (kf + 0.55) },
        9 => if odd { 0.339 / kf } else if k % 4 == 0 { 2.31 / (kf + 1.7) } else { 0.955 / (kf + 0.43) },
        10 => if odd { 0.5 / kf } else if k % 4 == 0 { 1.51 / (kf + 1.1) } else { 0.977 / (kf + 0.3) },
        _ => 1.0 / kf,
    }
}

/// Where the voices of a stack sit in pitch, lowest first, as shares of the detune amount, for the
/// first and the third kind of stack (measured on rendered notes; the second kind is an even spread).
/// The places are in voice order, which is not always pitch order at the top.
const UNI_CLASSIC: [[f32; 9]; 8] = [
    [-1.0690, 0.8770, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [-0.9290, 0.0, 0.8770, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [-1.1807, -0.3563, 0.3097, 0.8770, 0.0, 0.0, 0.0, 0.0, 0.0],
    [-1.0310, -0.5345, 0.0, 0.5905, 0.8768, 0.0, 0.0, 0.0, 0.0],
    [-1.1290, -0.6413, -0.2362, 0.1858, 0.6186, 0.8770, 0.0, 0.0, 0.0],
    [-0.8870, -0.7127, -0.3437, 0.0, 0.3097, 0.7527, 0.8770, 0.0, 0.0],
    [-1.1170, -0.7636, -0.4839, -0.1687, 0.1473, 0.3981, 0.6338, 0.8770, 0.0],
    [-0.9770, -0.8018, -0.4435, -0.2952, 0.0, 0.2822, 0.4645, 0.8377, 0.8770],
];
const UNI_SMEARED: [[f32; 9]; 8] = [
    [-1.0764, 0.8905, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [-0.9116, 0.0, 0.8905, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [-1.1025, -0.3588, 0.3036, 0.8905, 0.0, 0.0, 0.0, 0.0, 0.0],
    [-1.0341, -0.5383, 0.0, 0.5513, 0.8904, 0.0, 0.0, 0.0, 0.0],
    [-1.1669, -0.6459, -0.2205, 0.1823, 0.6205, 0.8905, 0.0, 0.0, 0.0],
    [-0.8540, -0.7175, -0.3447, 0.0, 0.3039, 0.7779, 0.8905, 0.0, 0.0],
    [-1.2063, -0.7689, -0.5001, -0.1575, 0.1477, 0.3907, 0.6100, 0.8906, 0.0],
    [-1.0000, -0.8073, -0.4270, -0.2756, 0.0, 0.2917, 0.4558, 0.9047, 0.8905],
];

/// A patch, ready for voices: the settings in table order plus an analysed wave if the block carries one.
#[derive(Debug, Clone, Default)]
pub struct Patch {
    pub p: Vec<f32>,
    /// Harmonic levels and phases (in cycles) of a custom timbre.
    pub custom: Option<(Vec<f32>, Vec<f32>)>,
    /// The larger instrument's way in and out (measured on its factory test): every harmonic rises on its own,
    /// closing on its level at this many time constants a cycle of its own pitch, and a let go note holds on
    /// for a moment and then leaves on a half cosine. Zero for the plain rise and fall.
    pub rise: f32,
}

impl Patch {
    /// `raw` is the stored block; a custom timbre sits after the fixed part as a count, the levels, then the phases.
    pub fn new(p: &[f32], raw: &[u8]) -> Self {
        let mut custom = None;
        if raw.len() >= 436 {
            let n = i32::from_le_bytes(raw[432..436].try_into().unwrap());
            if n > 0 && n <= 4096 && raw.len() >= 436 + n as usize * 8 {
                let n = n as usize;
                let f = |i: usize| f32::from_le_bytes(raw[436 + i * 4..440 + i * 4].try_into().unwrap());
                let amp: Vec<f32> = (0..n).map(f).map(|v| if v.is_finite() { v.clamp(0.0, 16.0) } else { 0.0 }).collect();
                let ph: Vec<f32> = (n..2 * n).map(f).map(|v| if v.is_finite() { v } else { 0.0 }).collect();
                custom = Some((amp, ph));
            }
        }
        Self { p: p.to_vec(), custom, rise: 0.0 }
    }

    fn v(&self, i: usize) -> f32 {
        self.p.get(i).copied().unwrap_or(0.0)
    }

    /// With legato on, one note sounds at a time and a held note bends to the next.
    pub fn mono(&self) -> Option<f32> {
        (self.v(ix::LEGATO) != 0.0).then_some(0.0)
    }

    /// How notes slide (measured on rendered notes): the pitch closes in on its goal with this time
    /// constant in seconds, for a note of this velocity over 128; the second value says whether every
    /// fresh note starts from the pitch of the one before (a bent held note always slides), the third
    /// whether a bent note also moves to the loudness of the new one, at the same pace.
    pub fn slide(&self, touch: f32) -> (f32, bool, bool) {
        let mut tau = 0.008144 * (41f32.powf(self.v(ix::SLIDE_TIME) / 1000.0) - 1.0);
        if self.v(ix::VEL_SLIDE) != 0.0 {
            // harder notes slide faster; this curve is a fit through seven velocities
            let u = touch.clamp(0.0, 1.0);
            tau *= (1.0 - u) * (1.0 - 0.68 * u);
        }
        (tau, self.v(ix::PORTAMENTO) != 0.0, self.v(ix::VEL_VOLUME) != 0.0)
    }

    /// Seconds for the level to rise at the start of a note.
    pub fn attack_s(&self) -> f32 {
        // measured at four places on the knob: a straight rise, 0.45 s long at the middle
        let k = self.v(ix::ATTACK);
        let x = (k - 500.0) / 250.0;
        if k <= 0.0 { 0.0 } else { (-0.799 + 1.1793 * x + 0.103 * x * x - 0.0453 * x * x * x).exp() }
    }

    /// Time constant, in seconds, of the fall once the note is let go. Measured at four places on the knob:
    /// 613, 170, 51.5 and 16 dB a second from a quarter of its travel to the end.
    pub fn release_s(&self) -> f32 {
        let x = (self.v(ix::RELEASE) - 500.0) / 250.0;
        (8.686 / (5.136 - 1.238 * x + 0.03 * x * x).exp()).max(0.001)
    }
}

fn twiddles() -> &'static [(f32, f32)] {
    static T: OnceLock<Vec<(f32, f32)>> = OnceLock::new();
    T.get_or_init(|| (0..MAX_M / 2).map(|i| { let a = TAU * i as f32 / MAX_M as f32; (a.cos(), a.sin()) }).collect())
}

/// In-place transform from harmonics to one wave (positive exponent, no scaling). `re.len()` is a power of two up to
/// `MAX_M`.
fn synth_fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let tw = twiddles();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = MAX_M / len;
        let half = len / 2;
        let mut i = 0;
        while i < n {
            for k in 0..half {
                let (c, s) = tw[k * step];
                let (a, b) = (i + k, i + k + half);
                let (xr, xi) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// One sounding note.
#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    key: f32,
    vel: f32,
    /// The note's velocity as written, 0..1 (a default note is 0.78).
    pub touch: f32,
    free: f32,
    /// seconds of the song gone by at the note's start
    pub at: f64,
    /// beats a second
    pub beat: f32,
    seed: u32,
    /// Wave length in frames of the table; the table holds two cycles of the note so the sub harmonics fit.
    m: usize,
    tab: [Vec<f32>; 2],
    /// The same waves a quarter turn on, filled only while harmonic detune is in use.
    tabq: [Vec<f32>; 2],
    /// The gap between harmonics against the note's pitch, and how far each unison voice has been moved along.
    spread: f32,
    sh: [f64; MAX_UNISON],
    cur: usize,
    blend: bool,
    fade: f32,
    count: u32,
    frames: u64,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Random start phase per harmonic, in cycles.
    hphase: Vec<f32>,
    last: [f32; 9],
    primed: bool,
    ph: [f64; MAX_UNISON],
    ratio: [f32; MAX_UNISON],
    gl: [f32; MAX_UNISON],
    gr: [f32; MAX_UNISON],
    order: usize,
    pitch_mul: f32,
    level: f32,
    trem: f32,
    /// The note's own rise and fall: the target, what it is multiplied by each frame once let go, and the
    /// smoothed level that is heard (measured: a lag follows every move, longer on the way down).
    env: f32,
    env_mul: f32,
    env_out: f32,
    held: bool,
    /// Frames between two looks at the settings, and frames since the note was let go.
    tick_len: u32,
    gone: u32,
    /// The pluck's clock when last looked at, and how much of it the mask has held back so far.
    pluck_seen: f32,
    pluck_held: f32,
    /// When the key was let go, in seconds from the note's start.
    left: Option<f32>,
    so_ph: f32,
    so_inc: f32,
    so_amp: f32,
    vrand: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as f32 / u32::MAX as f32
}

impl Voice {
    /// `dphase` is the note's cycles per frame, `vel` its velocity 0..1.
    pub fn start(patch: &Patch, sr: f32, key: u8, vel: f32, dphase: f32, seed: u32) -> Self {
        let mut seed = seed | 1;
        let n0 = (0.45 / dphase.max(1e-6)) as usize;
        // a dozen table points or more for the highest harmonic's cycle
        let m = (12 * n0).next_power_of_two().clamp(1024, MAX_M);
        let order = (patch.v(ix::UNI_ORDER) as usize).clamp(1, MAX_UNISON);
        let mut v = Self {
            sr,
            key: key as f32,
            vel,
            touch: 0.78 * vel.max(0.0).sqrt(),
            free: 0.0,
            seed,
            m,
            tab: [vec![0.0; m + 1], vec![0.0; m + 1]],
            tabq: [Vec::new(), Vec::new()],
            spread: 1.0,
            sh: [0.0; MAX_UNISON],
            cur: 0,
            blend: false,
            fade: 0.0,
            count: 0,
            frames: 0,
            at: 0.0,
            beat: 140.0 / 60.0,
            re: vec![0.0; m],
            im: vec![0.0; m],
            hphase: Vec::new(),
            last: [f32::NAN; 9],
            primed: false,
            ph: [0.0; MAX_UNISON],
            ratio: [1.0; MAX_UNISON],
            gl: [0.0; MAX_UNISON],
            gr: [0.0; MAX_UNISON],
            order,
            pitch_mul: 1.0,
            level: 1.0,
            trem: 0.0,
            env: 0.0,
            env_mul: 1.0,
            env_out: 0.0,
            held: true,
            tick_len: TICK,
            gone: 0,
            pluck_seen: 0.0,
            pluck_held: 0.0,
            left: None,
            so_ph: 0.0,
            so_inc: 0.0,
            so_amp: 0.0,
            vrand: 0.0,
        };
        v.vrand = rnd(&mut seed) * 2.0 - 1.0;
        let _ = rnd(&mut seed);
        let phase = patch.v(ix::PHASE);
        if phase < 0.0 {
            let amt = -phase / 1000.0;
            v.hphase = (0..m / 4).map(|_| rnd(&mut seed) * amt).collect();
        }
        let free = if phase > 0.0 { rnd(&mut seed) * phase / 1000.0 } else { 0.0 };
        v.free = free;
        v.stack(patch);
        v.seed = seed;
        v
    }

    /// Lay the voices of the stack out: pitch, side and starting point of each.
    fn stack(&mut self, patch: &Patch) {
        let order = self.order;
        // Measured on rendered notes: voice `u` sits at an even step across the field from left to right,
        // the pairs take the outer pitches inward with the left one high, then low, in turn; each voice
        // starts a share of a cycle ahead of the one before.
        let pan = patch.v(ix::UNI_PAN) / 128.0;
        let mut det = 1.875 * (41f32.powf(patch.v(ix::UNI_PITCH) / 256.0) - 1.0);
        if patch.v(ix::VEL_UNI) != 0.0 {
            det *= self.touch;
        }
        let uph = patch.v(ix::UNI_PHASE) / 128.0;
        let kind = patch.v(ix::UNI_TYPE) as usize;
        let g = if order > 1 { std::f32::consts::SQRT_2 / (0.370 + 0.6036 * order as f32) } else { 1.0 };
        for u in 0..order {
            let (mut s, mut x) = (0.0, 0.0);
            if order > 1 {
                x = u as f32 / (order - 1) as f32 * 2.0 - 1.0;
                let j = u.min(order - 1 - u);
                let high = (u == j) == (j % 2 == 0);
                let pos = if high { order - 1 - j } else { j };
                s = match kind {
                    0 => UNI_CLASSIC[order - 2][pos],
                    1 => pos as f32 / (order - 1) as f32 * 2.0 - 1.0,
                    _ => UNI_SMEARED[order - 2][pos],
                };
            }
            let ratio = 2f64.powf((s * det) as f64 / 1200.0);
            self.ratio[u] = ratio as f32;
            let (sn, cs) = (std::f32::consts::FRAC_PI_4 * (1.0 + x * pan)).sin_cos();
            self.gl[u] = if order > 1 { g * cs } else { 1.0 };
            self.gr[u] = if order > 1 { g * sn } else { 1.0 };
            // measured: a note starts half a cycle in, at the middle of the saw's ramp (the table holds two cycles)
            // the third kind instead starts every voice as if it had already run a while (measured: the
            // knob's eighth power, a hundred thousand cycles at the top), which smears the voices apart
            let ahead = if order < 2 {
                0.0
            } else if kind >= 2 { 1e5 * (uph as f64).powi(8) * ratio } else { (uph * u as f32 / order as f32) as f64 };
            self.ph[u] = (0.25 + self.free as f64 + 0.5 * ahead.fract()).fract();
        }
    }

    /// The note's velocity over 128, once known; the stack's detune may follow it.
    pub fn set_touch(&mut self, patch: &Patch, touch: f32) {
        self.touch = touch;
        if patch.v(ix::VEL_UNI) != 0.0 && self.order > 1 && self.frames == 0 {
            self.stack(patch);
        }
    }

    /// The slow oscillator, -1..1 (measured through its pull on the pitch).
    fn lfo(&mut self, patch: &Patch, t: f32, fenv: f32) -> f32 {
        let shape = patch.v(ix::LFO_SHAPE) as u8;
        let speed = patch.v(ix::LFO_SPEED);
        // it keeps time with the song: one cycle every eight beats at three tenths of the knob, twice as fast
        // for every tenth more; shared by the notes it follows the song, on its own it starts with the note
        let hz = if speed == 0.0 { 0.0 } else { self.beat as f64 * 2f64.powf(speed.abs() as f64 / 1000.0 - 6.0) * speed.signum() as f64 };
        let clock = if patch.v(ix::LFO_GLOBAL) != 0.0 { self.at + t as f64 } else { t as f64 };
        let x = (hz * clock).rem_euclid(1.0) as f32;
        let raw = match shape {
            // a saw that turns round in a soft half-swing
            1 => {
                let d = x - 0.5;
                if d.abs() <= 0.43 { d / 0.43 } else { d.signum() * (std::f32::consts::PI * (0.5 - d.abs()) / 0.14).sin() }
            }
            2 => {
                let s = -(x * TAU).sin();
                s.signum() * (1.0 - (1.0 - s.abs()).powi(6))
            }
            3 => 2.0 * x - 1.0,
            4 => if x < 0.5 { -1.0 } else { 1.0 },
            5 => if x < 1.0 / 3.0 { -1.0 } else if x < 2.0 / 3.0 { 0.0 } else { 1.0 },
            6 => 4.0 * ((x + 0.25).fract() - 0.5).abs() - 1.0,
            7 => 2.0 * x * x * x - 1.0,
            // measured: two sines of equal size, the second five times slower
            8 => {
                let c = hz * clock;
                (-0.5 * (c * TAU as f64).sin() + 0.5 * ((c * 0.2 + 0.323) * TAU as f64).sin()) as f32
            }
            9 => fenv,
            10 => patch.v(ix::CUTOFF) / 8192.0,
            11 => self.vrand,
            12 => self.touch,
            // measured: nothing at the F sharp below middle C, all the way three octaves either side
            13 => ((self.key - 42.0) / 36.0).clamp(-1.0, 1.0),
            _ => -(x * TAU).sin(),
        };
        // the fade-in: a squared rise, 0.11 s at a quarter of the knob, 0.455 s at half, 5 s at the top
        let knob = patch.v(ix::LFO_ATTACK);
        if knob > 0.0 && shape < 9 {
            let k = (knob - 500.0) / 250.0;
            let att = (-0.787 + 1.348 * k - 0.072 * k * k).exp();
            raw * (t / att).min(1.0).powi(2)
        } else {
            raw
        }
    }

    /// Work out the harmonic levels for this moment and, if they moved, build the next wave.
    fn tick(&mut self, patch: &Patch, dphase: f32) {
        let t = self.frames as f32 / self.sr;
        let m = self.m;
        // harmonic detune stretches the gaps between the harmonics: the second one sits `spread` times the
        // note's pitch above the first, and so on up (measured: 1200 cents doubles the gap, -1200 halves it).
        // That is the plain wave played `spread` times as fast and then moved along by a fixed number of Hz,
        // which is how it is made here: the wave's quarter-turned twin comes out of the same transform.
        let spread = 2f32.powf(patch.v(ix::DETUNE) / 1200.0);
        if !self.primed && spread != 1.0 {
            // the harmonics line up where the wave's own cycle begins
            for u in 0..MAX_UNISON {
                let turn = 2.0 * self.ph[u];
                self.ph[u] = (spread as f64 * turn * 0.5).rem_euclid(1.0);
                self.sh[u] = ((1.0 - spread as f64) * turn).rem_euclid(1.0);
            }
        }
        self.spread = spread;
        let n = ((((0.45 / dphase.max(1e-6)) - 1.0) / spread + 1.0) as usize).clamp(1, m / 4 - 1);
        let f0 = dphase * self.sr;

        // filter envelope
        // measured: both stages last 3.66 s at the middle of their knobs and about six times as long for
        // each quarter turn (a little less towards the top); the slope knobs bend the line into an exponential one way or the other
        let stage = |knob: f32| {
            let x = (knob - 500.0) / 250.0;
            if knob <= 0.0 { 0.0 } else { 3.655 * (1.82 * x - 0.091 * x * x).exp() }
        };
        let (fa, fd) = (stage(patch.v(ix::F_ATTACK)), stage(patch.v(ix::F_DECAY)));
        let fenv = if t < fa {
            1.0 - bend(t / fa, -patch.v(ix::F_ATT_SLOPE) / 1000.0)
        } else if fd > 0.0 {
            bend(((t - fa) / fd).min(1.0), patch.v(ix::F_DEC_SLOPE) / 1000.0)
        } else {
            0.0
        };
        let lfo = self.lfo(patch, t, fenv);

        // level and pitch movement
        let dec = patch.v(ix::DECAY);
        self.level = if dec < 1000.0 {
            // measured: 495, 66, 10.7 and 1.87 dB a second from the start of the knob to three quarters, after a
            // short wait
            let x = (dec - 500.0) / 250.0;
            10f32.powf(-(t - 0.012).max(0.0) * (2.373 - 1.7655 * x + 0.035 * x * x - 0.0165 * x * x * x).exp() / 20.0)
        } else {
            1.0
        } * boost(patch.v(ix::VOLUME) / 1000.0 - 1.0);
        if patch.v(ix::VEL_VOLUME) == 0.0 && self.vel > 0.0 {
            // the link from velocity is off: every note is as loud as a full one
            self.level *= (1.0 / self.vel).min(16.0);
        }
        let trem = patch.v(ix::TREMOLO) / 1000.0;
        self.trem = trem;
        // measured: six swings a second, 75 cents either way at the top of the knob, growing in over the
        // first third of a second of each note; the swing itself runs on with the song, so notes share it
        let vib_ph = (((self.at + t as f64) * 6.0 + 0.1475).fract() as f32) * TAU;
        let vib = patch.v(ix::VIBRATO) / 1000.0 * 75.0 * (t / 0.332).min(1.0).powi(2) * vib_ph.sin();
        // measured: the slow oscillator bends the pitch evenly both ways, by (100^knob - 1) / 99 of an octave
        let pl = patch.v(ix::PITCH_LFO) / 1000.0;
        let cents = patch.v(ix::PITCH) + vib + 1200.0 * (100f32.powf(pl.abs()) - 1.0) / 99.0 * pl.signum() * lfo;
        self.pitch_mul = 2f32.powf(cents / 1200.0);

        // filter position, in octaves above the C two octaves under middle C (measured: the knob counts
        // thousandths of an octave from there); key tracking turns about 128 Hz
        let cut = patch.v(ix::CUTOFF) / 1000.0
            + {
                // the envelope either swings the cutoff 4.1 octaves both ways around the knob, or (adaptive)
                // starts as far towards the end of the knob's travel as the amount says and comes back to it
                let amt = patch.v(ix::F_AMOUNT) / 1000.0 * if patch.v(ix::VEL_ENV) != 0.0 { self.touch } else { 1.0 };
                let knob = patch.v(ix::CUTOFF) / 1000.0;
                if patch.v(ix::F_ADAPTIVE) == 0.0 {
                    amt * 4.096 * (2.0 * fenv - 1.0)
                } else if amt > 0.0 {
                    amt * (8.192 - knob) * fenv
                } else {
                    amt * knob * fenv
                }
            }
            // measured: one-sided, nothing at the bottom of the swing and six octaves a full knob at the top
            + patch.v(ix::CUTOFF_LFO) / 1000.0 * 3.0 * (lfo + 1.0)
            + patch.v(ix::KEY_TRACK) / 1000.0 * (f0 / self.pitch_mul / 128.0).max(1e-3).log2();
        let res = patch.v(ix::RESONANCE) / 1000.0;
        // the bell voice shares this engine and has no filter
        let open = patch.rise > 0.0;

        let pluck = patch.v(ix::PLUCK);
        let pluck_on = pluck < 1000.0;
        if !self.held && self.left.is_none() {
            self.left = Some(t);
        }
        // the pluck takes the same number of dB a second off each harmonic for every step up the series
        // (measured: 160, 49, 10.1, 1.86 a second from the start of the knob to three quarters); a softer note
        // plucks faster when velocity is linked, and it can wait for the key to be let go
        let pluck_t = if pluck_on {
            let knob = pluck * if patch.v(ix::VEL_PLUCK) != 0.0 { (self.touch / 0.78).min(1.0) } else { 1.0 };
            let x = (knob - 500.0) / 250.0;
            let since = if patch.v(ix::PLUCK_RELEASE) != 0.0 { self.left.map_or(0.0, |l| t - l) } else { t };
            // the rate's logarithm at the knob's quarters and at nine tenths, lines drawn between
            const KNOTS: [(f32, f32); 5] = [(-2.0, 5.075), (-1.0, 3.896), (0.0, 2.3125), (1.0, 0.6206), (1.6, -0.409)];
            let i = KNOTS.iter().rposition(|k| k.0 <= x).unwrap_or(0).min(KNOTS.len() - 2);
            let ln = KNOTS[i].1 + (KNOTS[i + 1].1 - KNOTS[i].1) * (x - KNOTS[i].0) / (KNOTS[i + 1].0 - KNOTS[i].0);
            (since.max(0.0) * ln.exp()).min(400.0)
        } else {
            0.0
        };

        let mask_mix = (patch.v(ix::MASK_MIX) / 1000.0 + patch.v(ix::MASK_LFO) / 1000.0 * (lfo * 0.5 + 0.5)).clamp(0.0, 1.0);
        // a moving mask holds the pluck back by what it was at each moment (measured: the fall adds up)
        if pluck_t < self.pluck_seen {
            self.pluck_held = 0.0;
            self.pluck_seen = 0.0;
        }
        self.pluck_held += mask_mix * (pluck_t - self.pluck_seen);
        self.pluck_seen = pluck_t;
        let pluck_held = self.pluck_held;
        let ph_amt = patch.v(ix::PH_AMOUNT) / 1000.0;
        let ph_speed = patch.v(ix::PH_SPEED);
        // measured: the comb sweeps in time with the song, four teeth a beat at eight tenths of the knob and
        // twice as fast for every tenth more; it runs on with the song, so notes share its place
        let ph_hz = if ph_speed == 0.0 { 0.0 } else { self.beat * 2f32.powf(ph_speed.abs() / 1000.0 - 6.0) * ph_speed.signum() };
        let ph_run = (ph_hz as f64 * (self.at + t as f64)).fract() as f32;
        let ph_pos = if ph_amt > 0.0 { patch.v(ix::PH_OFFSET) / 1000.0 + ph_run + patch.v(ix::PH_OFFSET_LFO) / 1000.0 * 0.5 * lfo } else { 0.0 };
        // the width knob as tenths: every quarter of the knob makes the comb four times as dense (twice, when
        // it counts in harmonics)
        let ph_width = if ph_amt > 0.0 { patch.v(ix::PH_WIDTH) / 1000.0 } else { 0.0 };
        // measured: the slow oscillator makes the comb twice as dense, or half, at a full knob (the offset
        // moves it half a tooth either way)
        let ph_swing = if ph_amt > 0.0 { 2f32.powf(patch.v(ix::PH_WIDTH_LFO) / 1000.0 * lfo) } else { 1.0 };
        // the comb follows the key by this share, around middle C
        let ph_track = patch.v(ix::PH_KEY) / 1000.0 * (f0 / 256.0).log2();
        let hz_amt = (patch.v(ix::HZ_AMOUNT) / 1000.0).clamp(0.0, 1.0);
        // measured: the slow oscillator widens the harmonizer, one-sided like the filter's link: nothing at the
        // bottom of the swing, the whole knob's worth of width at the top
        let hz_wide = patch.v(ix::HZ_LFO) / 1000.0 * (lfo * 0.5 + 0.5);

        // grit shakes each harmonic by letting its phase wander, so the wave is redrawn on every tick while
        // it is on.
        let grit = patch.v(ix::GRIT) / 1000.0;
        let moving = grit != 0.0;
        // harmonics that rise on their own: looked at often while the fast ones come in, and each wave is the
        // one for the end of its fade in
        let first = !self.primed;
        let rise = patch.rise;
        let mut rise_t = 0.0;
        if rise > 0.0 {
            self.tick_len = if self.frames < 512 { 16 } else if self.frames < 2048 { 32 } else { TICK };
            rise_t = if first { 0.0 } else { t + self.tick_len as f32 / self.sr };
            if rise * f0 * rise_t >= 10.0 {
                rise_t = -1.0;
            }
        }
        // a wave is fully in one tick after it is drawn
        // the count starts half a cycle in, like the note itself
        // the harmonics run steadily off their places in the original; drawing the wave often enough keeps
        // the blend from one drawing to the next from thinning them
        let grit_reach = (65f32.powf(grit.abs()) - 1.0) / 80.0;
        if moving && rise <= 0.0 {
            let want = 0.03 * self.sr / (grit_reach * f0).max(1e-3);
            self.tick_len = if want >= TICK as f32 { TICK } else { (want.max(8.0) as u32).next_power_of_two() / 2 }.max(8);
        }
        let grit_t = 0.5 / f0 + if first { 0.0 } else { t + self.tick_len as f32 / self.sr };
        let key = [n as f32, cut, pluck_t, mask_mix, ph_pos, ph_width * ph_swing, hz_amt + hz_wide, if moving { t } else { 0.0 }, rise_t];
        if self.primed && key.iter().zip(&self.last).all(|(a, b)| (a - b).abs() < 1e-4) {
            self.blend = false;
            return;
        }
        self.last = key;

        // harmonic levels
        let kind = patch.v(ix::TIMBRE) as u8;
        let custom = patch.custom.as_ref().filter(|_| kind == 11);
        let lhp = taper(patch.v(ix::LHP) / 1024.0);
        let ftype = patch.v(ix::FILTER_TYPE) as u8;
        let fc = 65.406 * 2f32.powf(cut.clamp(-4.0, 10.0));
        let span = filter_width(patch.v(ix::F_WIDTH));
        // wide slopes start a little under the nominal frequency
        let lead = (span.max(1.5) - 1.5) / 3.0;
        let in_hz = patch.v(ix::CUT_HZ) != 0.0;
        // set to plain frequency, a slope is about as many nominal frequencies wide as it was octaves
        // (measured at two widths, a power law drawn through them)
        let (hz_span, hz_lead) = (1.0465 * span.powf(0.918) * fc, 0.0572 * span.powf(0.93) * fc);
        // the resonance: a bump (sixth power of a cosine) by which the filtered level is raised, sitting where
        // the slope starts, or on the nominal frequency for the band kinds. It is an octave wide; set to plain
        // frequency 1325 Hz to either side; the adaptive kind is a little taller and seven or so of the note's
        // harmonics wide. Its height grows as a power of the knob and with the octaves a harmonic lies above the
        // note. All measured.
        let res_off = patch.v(ix::RES_OFFSET) / 1200.0;
        let res_at = res_off + match ftype { 3 | 7 => lead, 1 | 2 | 5 | 6 | 14 | 15 | 16 => 0.0, _ => -lead };
        let res_hz = fc * 2f32.powf(res_at);
        let res_adaptive = patch.v(ix::RES_ADAPTIVE) != 0.0;
        let res_in_hz = patch.v(ix::RES_HZ) != 0.0;
        let res_power = if res > 0.0 { 1.781 * ((2.389 * res).exp() - 1.0) } else { 0.0 };
        let res_g = res_power / 4.786 * match (res_adaptive, res_in_hz) { (true, false) => 1.215, (true, true) => 1.097, _ => 1.0 };
        let res_floor = if res_in_hz && !res_adaptive { 1.5 } else { 0.143 };
        // self oscillation: a sine on the resonance, as loud as the knob and the resonance make it
        self.so_inc = (res_hz / self.sr).min(0.45);
        self.so_amp = 0.0597 * patch.v(ix::SELF_OSC) / 1024.0 * res_power;
        let eq: [f32; 6] = std::array::from_fn(|b| {
            let v = patch.v(ix::EQ + b) / 1024.0;
            boost(v)
        });
        let scale = patch.v(ix::PH_SCALE) as u8;
        let masked_pluck = patch.v(ix::PLUCK_MASKED) != 0.0;
        let alt_pluck = patch.v(ix::PLUCK_ALT) != 0.0;
        self.re[..m].fill(0.0);
        self.im[..m].fill(0.0);
        // the harmonizer: copies of lower harmonics laid on higher ones (measured). Step j of the octave kinds
        // takes the harmonic 2^j times lower (the +1 and +2 kinds land that many places higher); the harmonic
        // kinds take the one 2, 3, 4... times lower, or 2, 4, 6... times lower landing one place higher. Each
        // step's share falls off as a cube along the width, doubles with every quarter of the strength knob,
        // never passes the full source level, and the whole is scaled back by the sum of the shares
        let hz_type = (patch.v(ix::HZ_TYPE) as usize).min(4);
        let hz_pos = patch.v(ix::HZ_POS) as usize;
        // with the velocity link a soft note reaches less far (measured at one velocity)
        let hz_vel = if patch.v(ix::VEL_HZ) != 0.0 { self.touch } else { 1.0 };
        let hz_len = 1.0 + (patch.v(ix::HZ_WIDTH) / 1000.0 * hz_vel + hz_wide).max(0.0) * if hz_type >= 3 { 10.0 } else { 5.0 };
        let hz_lift = 2f32.powf(patch.v(ix::HZ_STRENGTH) / 256.0);
        let mut hz_share = [0f32; 12];
        let mut hz_steps = 0;
        if hz_amt > 0.0 {
            for j in 1..=11usize {
                let c = hz_amt * hz_amt * (hz_lift * (1.1 * (1.0 - j as f32 / hz_len)).max(0.0).powi(3)).min(1.0);
                if c <= 0.0 {
                    break;
                }
                hz_share[j] = c;
                hz_steps = j;
            }
        }
        let hz_norm = 1.0 / (1.0 + hz_share.iter().sum::<f32>());
        // where step j finds its source for harmonic k
        let hz_src = |k: usize, j: usize| -> Option<usize> {
            let (mul, up) = match hz_type {
                0 => (1usize << j, 0),
                1 => (1 << j, 1),
                2 => (1 << j, 2),
                3 => (j + 1, 0),
                _ => (2 * j, 1),
            };
            (k > up && (k - up) % mul == 0).then(|| (k - up) / mul)
        };
        let plain = |k: usize| -> f32 {
            match custom {
                Some((amp, _)) => amp.get(k - 1).copied().unwrap_or(0.0),
                None => timbre(kind, k),
            }
        };
        // what the equaliser, mask and filter (1), the phaser (2) and the pluck (4) make of harmonic k
        let stage = |k: usize, parts: u8| -> (f32, f32) {
            let kf = k as f32;
            let mut a = 1.0f32;
                // floating equaliser: bands sit on harmonics 1, 2, 4, 8, 16, 32
                let x = kf.log2().min(5.0);
                let b = (x as usize).min(4);
                let w = x - b as f32;
                let eq_gain = eq[b] + (eq[b + 1] - eq[b]) * w * w * (3.0 - 2.0 * w);
                if parts & 1 != 0 {
                    a *= eq_gain;
                }
                // what the mask, the pluck and the filter take away, the protection gives back to the first three
                // harmonics: all of it to the first with the knob full, a twelfth of that a step up, then far less
                let keep = match k {
                    1 => lhp,
                    2 => lhp * 0.0827,
                    3 => lhp * 0.000566,
                    _ => 0.0,
                };
                let mval = taper(patch.v(ix::MASK + (k - 1) % 12) / 1000.0);
                let bare = a;
                // with the pluck set to go by the mask, the mask leaves the levels alone (even with the pluck off)
                if mask_mix > 0.0 && parts & 1 != 0 && !masked_pluck {
                    a *= 1.0 - mask_mix * (1.0 - mval);
                }
                if pluck_on && parts & 4 != 0 {
                    // the alternative pluck spares what the equaliser lifts: the fall slows by very nearly the lift
                    // itself (measured at five lifts). Going by the mask, the fall is slowed by what the mask
                    // would have taken from the level: a harmonic the mask shuts out does not fall at all (measured)
                    let steer = if alt_pluck { eq_gain.max(1e-3).powf(-0.97) } else { 1.0 };
                    let clock = if masked_pluck { pluck_t - pluck_held * (1.0 - mval) } else { pluck_t };
                    a *= 10f32.powf(-clock * kf * steer / 20.0);
                }
                if !open && parts & 1 != 0 {
                    // the filter goes by each harmonic's place in the series, wherever detune has put it
                    let f = kf * f0;
                    let oct = (f / fc).log2();
                    a *= if in_hz { filter_gain(ftype, (f - fc) / hz_span, hz_lead / hz_span) } else { filter_gain(ftype, oct / span, lead / span) };
                    let d = if res_adaptive { (f - res_hz) / (if res_in_hz { 12.8 } else { 7.4 } * f0) } else if res_in_hz { (f - res_hz) / 2650.0 } else { oct - res_at };
                    if keep > 0.0 {
                        a += (bare - a) * keep;
                    }
                    if res_g > 0.0 && d.abs() < 0.5 {
                        a *= 1.0 + res_g * (kf.log2() - res_floor).max(0.0) * (d * std::f32::consts::PI).cos().powi(6);
                    }
                } else if keep > 0.0 {
                    a += (bare - a) * keep;
                }
                if rise_t >= 0.0 && rise > 0.0 && parts & 1 != 0 {
                    a *= 1.0 - (-rise * kf * f0 * rise_t).exp();
                }
                if ph_amt > 0.0 && parts & 2 != 0 {
                    // measured: a comb of raised-cosine teeth, silent at the bottom and 1.633 times up at the top, mixed
                    // with the plain sound by the amount. Counted in octaves it has three teeth every four
                    // octaves at the middle of the width knob, in Hz one every 3214 Hz, in harmonics one every
                    // two; the offset slides it by whole teeth
                    let pos = match scale {
                        1 => kf * f0 / 2f32.powf(ph_track) / 3214.0 * 4f32.powf((ph_width - 5.0) / 2.5),
                        2 => kf / 2.0 * 2f32.powf((ph_width - 5.0) / 2.5),
                        _ => ((kf * f0).log2() - ph_track) * 0.75 * 4f32.powf((ph_width - 5.0) / 2.5),
                    };
                    let pos = pos * ph_swing;
                    // the lowest harmonics are spared by the protection, as from the filter
                    let g = 1.0 - ph_amt + ph_amt * 1.633 * (0.5 - 0.5 * ((pos - ph_pos) * TAU).cos());
                    a *= g + (1.0 - g) * keep;
                }
            (a, keep)
        };
        // the parts that work before the copies are made and those that work after
        let (hz_before, hz_after) = match hz_pos {
            0 => (0u8, 7u8),
            1 => (1, 6),
            2 => (3, 4),
            _ => (7, 0),
        };
        let hz_pre: Vec<f32> = if hz_steps > 0 && hz_before != 0 { (0..=n).map(|k| if k == 0 { 0.0 } else { stage(k, hz_before).0 }).collect() } else { Vec::new() };
        for k in 1..=n {
            let kf = k as f32;
            let (whole, keep) = stage(k, 7);
            let mut a = plain(k) * whole;
            if hz_steps > 0 {
                let pre = |k: usize| if hz_pre.is_empty() { 1.0 } else { hz_pre[k] };
                let mut sum = plain(k) * pre(k);
                for j in 1..=hz_steps {
                    if let Some(src) = hz_src(k, j) {
                        sum += hz_share[j] * plain(src) * pre(src);
                    }
                }
                let made = hz_norm * sum * if hz_after == 0 { 1.0 } else { stage(k, hz_after).0 };
                // the protection spares the lowest harmonics here too
                a = made + (a - made) * keep;
            }
            if a == 0.0 {
                continue;
            }
            let phi = match custom {
                Some((_, ph)) => ph.get(k - 1).copied().unwrap_or(0.0),
                None => match (kind, k % 4) {
                    (3 | 5 | 6, 0 | 2) => 0.25,
                    (7, 0 | 2) => -0.25,
                    (4, 2) => 0.5,
                    _ => 0.0,
                },
            } + self.hphase.get(k - 1).copied().unwrap_or(0.0);
            let phi = if moving {
                // measured: each harmonic sits a little off its place, by an amount that swings once every
                // fifteen harmonics, so it runs ahead or behind steadily; the reach grows with the knob as
                // (65^knob - 1) / 80 of the note's pitch
                let off = -grit.signum() * grit_reach * ((kf - 1.0) * TAU / 15.0).sin();
                phi + (off as f64 * f0 as f64 * grit_t as f64).fract() as f32
            } else {
                phi
            };
            // make up what reading the table along straight lines takes from the high harmonics
            let u = std::f32::consts::PI * 2.0 * kf / m as f32;
            let a = a * (u / u.sin()).powi(2).min(2.0);
            let (s, c) = (phi * TAU).sin_cos();
            self.re[2 * k] = a * s;
            self.im[2 * k] = -a * c;
        }
        // the three sub sliders: an octave under the note, then that tone's third and fifth harmonics
        for (i, (bin, level)) in [(1usize, -1.0), (3, 0.667), (5, -0.4)].into_iter().enumerate() {
            let a = patch.v(ix::SUB + i) / 1024.0 * level;
            if a != 0.0 && bin < m / 2 {
                self.re[bin] += a;
            }
        }
        synth_fft(&mut self.re[..m], &mut self.im[..m]);
        let next = if self.primed { 1 - self.cur } else { self.cur };
        self.tab[next][..m].copy_from_slice(&self.re[..m]);
        self.tab[next][m] = self.re[0];
        if spread != 1.0 {
            for q in &mut self.tabq {
                q.resize(m + 1, 0.0);
            }
            self.tabq[next][..m].copy_from_slice(&self.im[..m]);
            self.tabq[next][m] = self.im[0];
        }
        self.blend = self.primed;
        if self.primed {
            // `cur` stays the wave being left until the fade is done
            self.fade = 0.0;
        }
        self.primed = true;
        if first && rise > 0.0 {
            // the silent start fades straight into the first sounding wave
            self.tick(patch, dphase);
        }
    }

    /// Let the note go; returns the seconds it still sounds.
    pub fn release(&mut self, patch: &Patch) -> f32 {
        self.held = false;
        if patch.rise > 0.0 {
            return (HOLD + LEAVE) * 1.05 + 0.002;
        }
        let tau = patch.release_s();
        self.env_mul = (-1.0 / (tau * self.sr)).exp();
        tau * 9.2 + 0.05
    }

    /// One frame. `dphase` is the note's cycles per frame.
    #[inline]
    pub fn frame(&mut self, patch: &Patch, dphase: f32) -> [f32; 2] {
        if self.count == 0 {
            if self.blend {
                self.cur = 1 - self.cur;
            }
            self.tick(patch, dphase * self.pitch_mul);
        }
        self.count += 1;
        if self.count >= self.tick_len {
            self.count = 0;
        }
        self.frames += 1;
        let m = self.m as f64;
        let (a, b) = (&self.tab[self.cur], &self.tab[1 - self.cur]);
        let inc = dphase as f64 * self.pitch_mul as f64 * 0.5;
        let mut out = [0.0f32; 2];
        for u in 0..self.order {
            let x = self.ph[u] * m;
            let i = (x as usize).min(self.m - 1);
            let fr = (x - i as f64) as f32;
            let mut s = a[i] + (a[i + 1] - a[i]) * fr;
            if self.blend {
                let s2 = b[i] + (b[i + 1] - b[i]) * fr;
                s += (s2 - s) * self.fade;
            }
            let step = (inc * self.ratio[u] as f64 * self.spread as f64).min(0.24);
            if self.spread != 1.0 && self.tabq[0].len() > i + 1 {
                let (qa, qb) = (&self.tabq[self.cur], &self.tabq[1 - self.cur]);
                let mut q = qa[i] + (qa[i + 1] - qa[i]) * fr;
                if self.blend {
                    let q2 = qb[i] + (qb[i + 1] - qb[i]) * fr;
                    q += (q2 - q) * self.fade;
                }
                let (sn, cs) = ((self.sh[u] as f32) * TAU).sin_cos();
                s = s * cs - q * sn;
                self.sh[u] = (self.sh[u] + inc * 2.0 * self.ratio[u] as f64 * (1.0 - self.spread as f64)).rem_euclid(1.0);
            }
            out[0] += s * self.gl[u];
            out[1] += s * self.gr[u];
            let p = self.ph[u] + step;
            self.ph[u] = if p >= 1.0 { p - 1.0 } else { p };
        }
        if self.blend {
            self.fade = (self.fade + 1.0 / self.tick_len as f32).min(1.0);
        }
        if self.so_amp > 0.0 {
            let s = self.so_amp * (self.so_ph * TAU).sin();
            out[0] += s;
            out[1] += s;
            self.so_ph = (self.so_ph + self.so_inc).fract();
        }
        if self.held {
            let a = patch.attack_s() * self.sr;
            self.env = if a < 1.0 { 1.0 } else { (self.env + 1.0 / a).min(1.0) };
        } else {
            self.env *= self.env_mul;
        }
        if patch.rise > 0.0 {
            let x = if self.held { 0.0 } else { ((self.gone as f32 / self.sr - HOLD) / LEAVE).clamp(0.0, 1.0) };
            if !self.held {
                self.gone += 1;
            }
            let g = self.level * 0.5304 * (0.5 + 0.5 * (x * std::f32::consts::PI).cos());
            let (tl, tr) = self.sides();
            return [out[0] * g * tl, out[1] * g * tr];
        }
        let lag = if self.held { 0.00215 } else { 0.006 };
        self.env_out += (self.env - self.env_out) * (1.0 - (-1.0 / (lag * self.sr)).exp());
        let g = self.level * 0.5304 * self.env_out;
        let (tl, tr) = self.sides();
        [out[0] * g * tl, out[1] * g * tr]
    }

    /// The tremolo's hold on the two sides. Measured: the sound swings from side to side five times a second,
    /// each side following the arch of a half sine down to silence at full depth, the knob mixing that with
    /// the steady sound, and the depth coming in over the first tenth of a second.
    fn sides(&self) -> (f32, f32) {
        if self.trem <= 0.0 {
            return (1.0, 1.0);
        }
        let t = self.frames as f32 / self.sr;
        let d = self.trem * (t * 10.0).min(1.0);
        // the swing runs on with the song, so notes share it
        let (s, c) = ((((self.at + t as f64) * 5.0 + 0.7302).fract() as f32) * std::f32::consts::PI).sin_cos();
        (1.0 - d + d * 1.255 * s.abs(), 1.0 - d + d * 1.255 * c.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> Patch {
        let g = crate::synth::Gen::new(crate::synth::Kind::Additive);
        Patch::new(&g.p, &g.raw)
    }

    #[test]
    fn the_effects_follow_their_knobs() {
        let mut g = crate::synth::Gen::new(crate::synth::Kind::Additive);
        // a fresh one has all three switched off
        assert!(!g.tail().any());
        for i in [ix::CHORUS_TYPE, ix::DELAY_TYPE, ix::REVERB_TYPE] {
            g.p[i] = 1.0;
        }
        let t = g.tail();
        // the echo of a fresh one is three quarters of a beat long
        assert_eq!(t.stack.unwrap().echo.unwrap().steps, 3.0);
        assert!(t.chorus.is_some() && t.reverb.is_some());
        for i in [ix::CHORUS_MIX, ix::DELAY_VOL, ix::REVERB_VOL] {
            g.p[i] = 0.0;
        }
        assert!(!g.tail().any());
        // the compressor is the fourth: its kind and how much of it
        g.p[ix::COMP_TYPE] = 3.0;
        g.p[ix::COMP_MIX] = 500.0;
        assert_eq!(g.tail().stack.unwrap().squash, Some(crate::stackfx::Squash { kind: 3, mix: 0.5 }));
    }

    fn render(p: &Patch, hz: f32, frames: usize) -> Vec<f32> {
        let sr = 48_000.0;
        let mut v = Voice::start(p, sr, 57, 0.8, hz / sr, 7);
        (0..frames).map(|_| v.frame(p, hz / sr)[0]).collect()
    }

    /// Level of the component at `hz` in `x`.
    fn level_at(x: &[f32], hz: f32) -> f32 {
        let (mut s, mut c) = (0.0f32, 0.0f32);
        for (i, v) in x.iter().enumerate() {
            let a = TAU * hz * i as f32 / 48_000.0;
            s += v * a.sin();
            c += v * a.cos();
        }
        (s * s + c * c).sqrt() * 2.0 / x.len() as f32
    }

    #[test]
    fn harmonic_detune_moves_the_upper_harmonics_and_grit_roughens_them() {
        let mut p = patch();
        p.p[ix::CUTOFF] = 8192.0;
        p.p[ix::F_AMOUNT] = 0.0;
        p.p[ix::PHASE] = 0.0;
        let x = render(&p, 200.0, 48_000);
        p.p[ix::DETUNE] = 240.0;
        let y = render(&p, 200.0, 48_000);
        // the gaps between the harmonics all widen by 240 cents, the first stays
        let moved = 200.0 * (1.0 + 3.0 * 2f32.powf(240.0 / 1200.0));
        assert!(level_at(&y, moved) > 4.0 * level_at(&y, 800.0), "{} {}", level_at(&y, moved), level_at(&y, 800.0));
        assert!(level_at(&y, moved) > 0.5 * level_at(&x, 800.0));
        assert!((level_at(&y, 200.0) / level_at(&x, 200.0) - 1.0).abs() < 0.05);
        p.p[ix::DETUNE] = 0.0;
        p.p[ix::GRIT] = 600.0;
        let z = render(&p, 200.0, 48_000);
        assert!(level_at(&z, 800.0) < 0.8 * level_at(&x, 800.0));
        assert!(level_at(&z, 779.8) > 3.0 * level_at(&x, 779.8));
    }

    #[test]
    fn an_open_saw_has_harmonics_falling_as_one_over_k() {
        let mut p = patch();
        p.p[ix::CUTOFF] = 8192.0;
        p.p[ix::F_AMOUNT] = 0.0;
        p.p[ix::PHASE] = 0.0;
        let x = render(&p, 220.0, 48_000);
        let (h1, h2, h3) = (level_at(&x, 220.0), level_at(&x, 440.0), level_at(&x, 660.0));
        assert!((h1 / h2 - 2.0).abs() < 0.1, "h1/h2 {}", h1 / h2);
        assert!((h1 / h3 - 3.0).abs() < 0.15, "h1/h3 {}", h1 / h3);
        // nothing under the note without the sub sliders
        assert!(level_at(&x, 110.0) < h1 * 0.01);
    }

    #[test]
    fn the_filter_and_the_mask_cut_what_they_should() {
        let mut p = patch();
        p.p[ix::PHASE] = 0.0;
        p.p[ix::F_AMOUNT] = 0.0;
        p.p[ix::CUTOFF] = 3500.0; // a little above the note
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 2200.0) < level_at(&x, 220.0) * 0.02, "the tenth harmonic is filtered out");
        // the default mask keeps odd harmonics; fully mixed in, the even ones go
        let mut p = patch();
        p.p[ix::PHASE] = 0.0;
        p.p[ix::CUTOFF] = 8192.0;
        p.p[ix::F_AMOUNT] = 0.0;
        p.p[ix::MASK_MIX] = 1000.0;
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 440.0) < level_at(&x, 660.0) * 0.05);
        // a sub slider adds the octave below
        p.p[ix::SUB] = 1024.0;
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 110.0) > level_at(&x, 220.0) * 0.5);
    }

    #[test]
    fn a_pluck_darkens_the_note_over_time_and_unison_widens_it() {
        let mut p = patch();
        p.p[ix::PHASE] = 0.0;
        p.p[ix::CUTOFF] = 8192.0;
        p.p[ix::F_AMOUNT] = 0.0;
        p.p[ix::PLUCK] = 400.0;
        let x = render(&p, 220.0, 96_000);
        let early = level_at(&x[..9_600], 1100.0);
        let late = level_at(&x[86_400..], 1100.0);
        assert!(late < early * 0.3, "early {early} late {late}");
        let (first, last) = (level_at(&x[..9_600], 220.0), level_at(&x[86_400..], 220.0));
        assert!(last / first > 3.0 * late / early, "the note itself fades slower");

        let mut p = patch();
        p.p[ix::UNI_ORDER] = 5.0;
        let sr = 48_000.0;
        let mut v = Voice::start(&p, sr, 57, 0.8, 220.0 / sr, 3);
        let mut diff = 0.0f32;
        for _ in 0..48_000 {
            let f = v.frame(&p, 220.0 / sr);
            assert!(f[0].is_finite() && f[0].abs() < 4.0);
            diff += (f[0] - f[1]).abs();
        }
        assert!(diff > 100.0, "left and right differ with unison on: {diff}");
    }

    #[test]
    fn a_custom_wave_in_the_block_is_played() {
        let g = crate::synth::Gen::new(crate::synth::Kind::Additive);
        let mut raw = g.raw[..432].to_vec();
        raw.extend_from_slice(&3i32.to_le_bytes());
        for a in [0.0f32, 0.0, 1.0] {
            raw.extend_from_slice(&a.to_le_bytes());
        }
        for _ in 0..3 {
            raw.extend_from_slice(&0f32.to_le_bytes());
        }
        let mut p = Patch::new(&g.p, &raw);
        p.p[ix::TIMBRE] = 11.0;
        p.p[ix::CUTOFF] = 8192.0;
        p.p[ix::F_AMOUNT] = 0.0;
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 660.0) > 0.2);
        assert!(level_at(&x, 220.0) < 0.01);
    }
}
