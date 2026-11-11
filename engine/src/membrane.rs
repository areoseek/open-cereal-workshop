//! Membrane Drum: one struck drum head worked out as a bank of ringing modes, with a mallet of pulse and noise,
//! a low cut, a falling sine sweep, a moving mid band, a high shelf, a bit crusher and four links from the
//! strength of the hit to any of the settings.
//!
//! The stored block is a name (128 bytes) and 40 numbers. The places of the numbers were worked out from many
//! stored sounds; the model and the curves behind the settings are our own.

use std::sync::OnceLock;

use crate::duo::{Ctl, Info};

pub const COUNT: usize = 40;
/// Where the numbers start in a one-drum block.
pub const FIRST: usize = 140;
const SIZE: usize = FIRST + COUNT * 4;
const MODES: usize = 24;
const TRIM: f32 = 0.9;

pub const GROUPS: [&str; 9] = ["Mallet", "Membrane", "Low cut", "Sweep", "Mid band", "High shelf", "Output", "Hit strength", "Kept"];

const DEFS: [f32; COUNT] = [
    0.0, 0.0, 0.4, 0.5, 0.05, 0.0, 0.6, 0.33, 0.38, 0.6, 0.45, 0.55, 1.0, 0.2, 0.1, 0.5, 0.5, 0.0, 0.5, 0.5, 0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0, 0.0, 1.0, 0.5, 1.0, 0.0, 0.0,
    0.0, 0.8, 0.0, 0.0, 0.0, 1.0, 0.0,
];

/// The settings a link from the hit strength can move, by their stored number (0 is none).
pub const TARGETS: &[&str] = &[
    "None",
    "Mallet strength",
    "Mallet decay",
    "Noise level",
    "Noise phase",
    "Membrane decay",
    "Tension",
    "Material",
    "Cutoff",
    "Phase",
    "Size",
    "Shape",
    "Low cut frequency",
    "Low cut resonance",
    "High shelf frequency",
    "High shelf level",
    "High shelf resonance",
    "Sweep frequency",
    "Sweep decay",
    "Sweep level",
    "Mid frequency",
    "Mid frequency move",
    "Mid level",
    "Mid level move",
    "Mid decay",
    "Mid resonance",
    "Crush",
    "Hold",
    "Pitch",
];

fn placed() -> Vec<(usize, &'static str, Ctl, u8)> {
    vec![
        (2, "Strength", Ctl::Knob, 0),
        (3, "Decay", Ctl::Knob, 0),
        (4, "Noise level", Ctl::Knob, 0),
        (5, "Noise phase", Ctl::Knob, 0),
        (0, "Same noise every hit", Ctl::Switch, 0),
        (6, "Decay", Ctl::Knob, 1),
        (7, "Tension", Ctl::Knob, 1),
        (8, "Material", Ctl::Knob, 1),
        (9, "Cutoff", Ctl::Knob, 1),
        (10, "Phase", Ctl::Knob, 1),
        (11, "Size", Ctl::Knob, 1),
        (12, "Shape", Ctl::Knob, 1),
        (13, "Frequency", Ctl::Knob, 2),
        (14, "Resonance", Ctl::Knob, 2),
        (18, "Frequency", Ctl::Knob, 3),
        (19, "Decay", Ctl::Knob, 3),
        (20, "Level", Ctl::Knob, 3),
        (21, "Frequency", Ctl::Knob, 4),
        (22, "Frequency move", Ctl::Knob, 4),
        (23, "Level", Ctl::Knob, 4),
        (24, "Level move", Ctl::Knob, 4),
        (26, "Resonance", Ctl::Knob, 4),
        (25, "Decay", Ctl::Knob, 4),
        (1, "Repeat the hit", Ctl::Choice(&["Off", "Once", "Twice", "Three times"]), 4),
        (15, "Frequency", Ctl::Knob, 5),
        (16, "Level", Ctl::Knob, 5),
        (17, "Resonance", Ctl::Knob, 5),
        (28, "Hold", Ctl::Knob, 6),
        (27, "Crush", Ctl::Knob, 6),
        (29, "Pitch", Ctl::Knob, 6),
        (38, "Level", Ctl::Knob, 6),
        (30, "Link 1 moves", Ctl::Choice(TARGETS), 7),
        (34, "Link 1 value", Ctl::Knob, 7),
        (31, "Link 2 moves", Ctl::Choice(TARGETS), 7),
        (35, "Link 2 value", Ctl::Knob, 7),
        (32, "Link 3 moves", Ctl::Choice(TARGETS), 7),
        (36, "Link 3 value", Ctl::Knob, 7),
        (33, "Link 4 moves", Ctl::Choice(TARGETS), 7),
        (37, "Link 4 value", Ctl::Knob, 7),
    ]
}

pub fn params() -> &'static [Info] {
    static P: OnceLock<Vec<Info>> = OnceLock::new();
    P.get_or_init(|| {
        let mut v: Vec<Info> = DEFS.iter().map(|&def| Info { name: "", ctl: Ctl::Kept, def, group: 8 }).collect();
        for (i, name, ctl, group) in placed() {
            v[i] = Info { name, ctl, def: DEFS[i], group };
        }
        v
    })
}

pub fn defaults() -> Vec<f32> {
    DEFS.to_vec()
}

/// True when the block is a one-drum block we can read.
pub fn fits(raw: &[u8]) -> bool {
    raw.len() >= SIZE && &raw[..4] == b"SSLF" && &raw[9..12] == b"PMD"
}

/// Forty numbers from this place in a block.
pub fn values_at(raw: &[u8], at: usize) -> Vec<f32> {
    (0..COUNT)
        .map(|i| {
            let o = at + i * 4;
            let v = raw.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).unwrap_or(f32::NAN);
            if v.is_finite() {
                v
            } else {
                DEFS[i]
            }
        })
        .collect()
}

pub fn values(raw: &[u8]) -> Vec<f32> {
    if fits(raw) {
        values_at(raw, FIRST)
    } else {
        Vec::new()
    }
}

/// The name and the forty numbers of one drum saved by itself: the pad files of the kit instrument in their
/// three ages (35, 36 and 39 numbers after a tag and a name), or a whole one-drum block.
pub fn read_patch(b: &[u8]) -> Option<(String, Vec<f32>)> {
    let text = |b: &[u8]| String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned();
    if fits(b) {
        return Some((text(&b[12..FIRST]), values_at(b, FIRST)));
    }
    if b.len() < 132 + 35 * 4 || &b[1..4] != b"PMD" {
        return None;
    }
    let mut v: Vec<f32> = b[132..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    if v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    match v.len() {
        35 => {
            // the first age had no shape and no high shelf
            v.insert(12, 1.0);
            v.splice(15..15, [0.5, 0.5, 0.0]);
        }
        36 => {
            v.splice(15..15, [0.5, 0.5, 0.0]);
        }
        39 | 40 => {}
        _ => return None,
    }
    v.resize(COUNT, 0.0);
    Some((text(&b[4..132]), v))
}

/// A block of our own for a new instrument.
pub fn fresh() -> Vec<u8> {
    let mut out = Vec::with_capacity(SIZE);
    out.extend_from_slice(b"SSLF");
    out.extend_from_slice(&((SIZE - 8) as u32).to_le_bytes());
    out.extend_from_slice(b"3PMD");
    out.resize(FIRST, 0);
    out[12..19].copy_from_slice(b"Default");
    for v in DEFS {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

// frequencies of the modes of a round and of a square skin, against the lowest
const ROUND: [f32; MODES] = [
    1.0, 1.594, 2.136, 2.296, 2.653, 2.918, 3.156, 3.501, 3.600, 3.652, 4.060, 4.154, 4.230, 4.601, 4.610, 4.832, 4.903, 5.131, 5.412, 5.540, 5.651, 5.977, 6.153, 6.209,
];
const SQUARE: [f32; MODES] = [
    1.0, 1.581, 2.0, 2.236, 2.550, 2.915, 3.0, 3.162, 3.536, 3.606, 3.808, 4.0, 4.123, 4.301, 4.472, 4.528, 4.743, 5.0, 5.0, 5.099, 5.148, 5.385, 5.523, 5.701,
];

/// One drum, ready to be struck. The settings are kept as they are stored: a hit works them out for its strength.
#[derive(Debug, Clone)]
pub struct Set {
    p: [f32; COUNT],
}

pub fn set(p: &[f32]) -> Set {
    let mut out = [0.0; COUNT];
    for (i, o) in out.iter_mut().enumerate() {
        let v = p.get(i).copied().filter(|v| v.is_finite()).unwrap_or(DEFS[i]);
        *o = if (30..34).contains(&i) {
            v.clamp(0.0, 64.0)
        } else if i == 1 {
            // the count of repeats, 0 to 3; one later version of the kit block keeps it as a share of three
            if v > 0.0 && v < 1.0 { (v * 3.0).round() } else { v.clamp(0.0, 3.0).round() }
        } else {
            v.clamp(0.0, 1.0)
        };
    }
    Set { p: out }
}

impl Set {
    /// The settings as one hit of this strength hears them.
    fn struck(&self, vel: f32) -> [f32; COUNT] {
        let mut p = self.p;
        // where a link target sits among the stored numbers
        const PLACE: [usize; 29] = [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29];
        for k in 0..4 {
            let t = self.p[30 + k].round() as usize;
            if t >= 1 && t < PLACE.len() {
                let at = PLACE[t];
                p[at] = self.p[at] + (self.p[34 + k] - self.p[at]) * vel.clamp(0.0, 1.0);
            }
        }
        p
    }

    /// Seconds a hit may ring at most.
    pub fn tail(&self) -> f32 {
        let d = self.p[6].max(self.p[19] * self.p[20].min(1.0));
        let ring = (0.02 * 2f32.powf(d * 7.0) * 7.0).min(8.0);
        let hold = if self.p[28] >= 0.999 { 8.0 } else { 0.01 * 2f32.powf(self.p[28] * 9.0) * 7.0 };
        ring.min(hold).max(0.05) + 0.08
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    #[inline]
    pub(crate) fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }

    fn norm(&mut self, b: [f32; 3], a: [f32; 3]) {
        self.b = [b[0] / a[0], b[1] / a[0], b[2] / a[0]];
        self.a = [a[1] / a[0], a[2] / a[0]];
    }

    fn high_pass(&mut self, sr: f32, hz: f32, q: f32) {
        let w = std::f32::consts::TAU * (hz / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        self.norm([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], [1.0 + al, -2.0 * c, 1.0 - al]);
    }

    pub(crate) fn peak(&mut self, sr: f32, hz: f32, q: f32, db: f32) {
        let w = std::f32::consts::TAU * (hz / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let a = 10f32.powf(db / 40.0);
        let al = s / (2.0 * q);
        self.norm([1.0 + al * a, -2.0 * c, 1.0 - al * a], [1.0 + al / a, -2.0 * c, 1.0 - al / a]);
    }

    fn high_shelf(&mut self, sr: f32, hz: f32, q: f32, db: f32) {
        let w = std::f32::consts::TAU * (hz / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let a = 10f32.powf(db / 40.0);
        let al = s / (2.0 * q);
        let k = 2.0 * a.sqrt() * al;
        self.norm(
            [a * ((a + 1.0) + (a - 1.0) * c + k), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - k)],
            [(a + 1.0) - (a - 1.0) * c + k, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - k],
        );
    }
}

/// One hit, ringing.
#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    n: u32,
    seed: u32,
    // mallet
    amp: f32,
    fall: f32,
    env: f32,
    noise: f32,
    last: f32,
    thin: f32,
    again: u8,
    // modes: cosine term, radius squared, input gain, two states
    modes: [[f32; 5]; MODES],
    live: usize,
    cut: f32,
    lp: f32,
    low: Option<Biquad>,
    // sweep: phase, frequency, the factor it falls by, level
    sweep: [f32; 4],
    sweep_env: f32,
    sweep_fall: f32,
    mid: Biquad,
    // mid band: frequency, move in octaves, level dB, move dB, q, envelope, its fall
    midp: [f32; 7],
    high: Option<Biquad>,
    steps: f32,
    hold: f32,
    hold_fall: f32,
    gain: f32,
}

#[inline]
fn rnd(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    (*seed >> 8) as f32 / 8388608.0 - 1.0
}

impl Voice {
    /// `tune` is the frequency of the played key against the middle one.
    pub fn start(set: &Set, sr: f32, vel: f32, tune: f32, seed: u32) -> Self {
        let p = set.struck(vel);
        let tune = if tune.is_finite() && tune > 0.0 { tune } else { 1.0 };
        let f0 = 30.0 * 2f32.powf((1.0 - p[11]) * 3.5 + p[7] * 3.5 + (p[29] - 0.5) * 2.0) * tune;
        let ring = 0.02 * 2f32.powf(p[6] * 7.0);
        let mat = p[8];
        let shape = p[12];
        let hit = 0.08 + 0.84 * p[10];
        let mut modes = [[0.0f32; 5]; MODES];
        let mut live = 0;
        for k in 0..MODES {
            let ratio = SQUARE[k] + (ROUND[k] - SQUARE[k]) * shape;
            // lighter, stiffer stuff spreads the upper modes and lets them ring on
            let ratio = ratio * (1.0 + mat * mat * 0.12 * (ratio - 1.0));
            let hz = f0 * ratio;
            if hz >= sr * 0.47 {
                continue;
            }
            let secs = ring * ratio.powf(-(1.0 - mat) * 2.2);
            let r = (-1.0 / (secs * sr)).exp();
            let w = std::f32::consts::TAU * hz / sr;
            // where the mallet lands sets how much of each mode it wakes
            let wake = 0.25 + 0.75 * (std::f32::consts::PI * (k as f32 + 1.0) * hit * 0.5).sin().abs();
            modes[live] = [2.0 * r * w.cos(), r * r, w.sin() * wake / (1.0 + k as f32 * (1.0 - mat) * 0.35), 0.0, 0.0];
            live += 1;
        }
        let even = 0.6 / (live.max(1) as f32).sqrt();
        for m in &mut modes[..live] {
            m[2] *= even;
        }
        let strike = 0.0003 * 2f32.powf(p[3] * 8.0);
        let low = (p[13] > 0.01 || p[14] > 0.01).then(|| {
            let mut b = Biquad::default();
            b.high_pass(sr, 20.0 * 2f32.powf(p[13] * 8.0), 0.707 + p[14] * p[14] * 9.0);
            b
        });
        let high = ((p[16] - 0.5).abs() > 0.004).then(|| {
            let mut b = Biquad::default();
            b.high_shelf(sr, 1000.0 * 2f32.powf(p[15] * 4.0), 0.707 + p[17] * p[17] * 5.0, (p[16] - 0.5) * 36.0);
            b
        });
        let sweep_secs = 0.004 * 2f32.powf(p[19] * 6.5);
        let mid_secs = 0.004 * 2f32.powf(p[25] * 7.0);
        let seed = if p[0] >= 0.5 { 0x9e3779b9u32 ^ ((p[5] * 65535.0) as u32).wrapping_mul(2654435761) } else { seed.wrapping_mul(747796405).wrapping_add(2891336453) ^ ((p[5] * 65535.0) as u32) };
        let mut v = Self {
            sr,
            n: 0,
            seed,
            amp: p[2] * 2.0,
            fall: (-1.0 / (strike * sr)).exp(),
            env: 1.0,
            noise: p[4],
            last: 0.0,
            thin: 2.0 / (1.0 + strike * sr * 0.5).sqrt(),
            again: p[1] as u8,
            modes,
            live,
            cut: 1.0 - (-std::f32::consts::TAU * (200.0 * 2f32.powf(p[9] * 6.6) * tune.sqrt()).min(sr * 0.45) / sr).exp(),
            lp: 0.0,
            low,
            sweep: [0.0, 50.0 * 2f32.powf(p[18] * 7.0) * tune, (-1.0 / (sweep_secs * sr)).exp(), p[20] * p[20] * p[2] * 1.5],
            sweep_env: 1.0,
            sweep_fall: (-1.0 / (sweep_secs * 4.0 * sr)).exp(),
            mid: Biquad::default(),
            midp: [150.0 * 2f32.powf(p[21] * 6.0), (p[22] - 0.5) * 4.0, (p[23] - 0.5) * 30.0, (p[24] - 0.5) * 24.0, 0.6 + p[26] * p[26] * 11.0, 1.0, (-16.0 / (mid_secs * sr)).exp()],
            high,
            steps: if p[27] > 0.004 { 2f32.powf(15.0 - p[27] * 13.0) } else { 0.0 },
            hold: 1.0,
            hold_fall: if p[28] >= 0.999 { 1.0 } else { (-1.0 / (0.01 * 2f32.powf(p[28] * 9.0) * sr)).exp() },
            // the last number is taken as a level; its curve is kept gentle so that low settings stay heard
            gain: p[38].sqrt() * TRIM,
        };
        v.tune_mid();
        v
    }

    fn tune_mid(&mut self) {
        let [hz, oct, db, move_db, q, env, _] = self.midp;
        self.mid.peak(self.sr, (hz * 2f32.powf(oct * env)).clamp(30.0, self.sr * 0.45), q, db + move_db * env);
    }

    /// The drum rings on by itself: seconds until it may be dropped.
    pub fn release(&mut self, set: &Set) -> f32 {
        (set.tail() - self.n as f32 / self.sr).max(0.0)
    }

    pub fn frame(&mut self, _set: &Set) -> [f32; 2] {
        // the repeated hit of a clap: up to three more strikes close behind the first
        if self.again > 0 && self.n > 0 && self.n % ((self.sr * 0.011) as u32).max(1) == 0 {
            self.again -= 1;
            self.env = 1.0;
            self.last = 0.0;
            self.midp[5] = 1.0;
        }
        if self.n % 16 == 0 {
            self.tune_mid();
            self.midp[5] *= self.midp[6];
        }
        self.n += 1;
        // the push of the mallet reaches the skin as its change; its noise as it is, evened out over its length
        let (hit, hiss) = if self.env > 1e-5 {
            let push = self.env * self.amp * (1.0 - self.noise);
            let hiss = self.env * self.amp * self.noise * rnd(&mut self.seed);
            self.env *= self.fall;
            let d = push - self.last;
            self.last = push;
            (d + hiss * self.thin, hiss)
        } else {
            (0.0, 0.0)
        };
        let mut y = 0.0;
        for m in &mut self.modes[..self.live] {
            let o = m[0] * m[3] - m[1] * m[4] + m[2] * hit;
            m[4] = m[3];
            m[3] = o;
            y += o;
        }
        // a share of the strike itself is heard through the skin
        y += hiss * 0.5;
        self.lp += (y - self.lp) * self.cut;
        let mut x = self.lp;
        if self.sweep[3] > 0.0 && self.sweep_env > 1e-4 {
            self.sweep[0] += (20.0 + self.sweep[1]).min(self.sr * 0.45) / self.sr;
            if self.sweep[0] >= 1.0 {
                self.sweep[0] -= 1.0;
            }
            self.sweep[1] *= self.sweep[2];
            x += (self.sweep[0] * std::f32::consts::TAU).sin() * self.sweep_env * self.sweep[3];
            self.sweep_env *= self.sweep_fall;
        }
        if let Some(b) = &mut self.low {
            x = b.run(x);
        }
        x = self.mid.run(x);
        if let Some(b) = &mut self.high {
            x = b.run(x);
        }
        if self.steps > 0.0 {
            x = (x * self.steps).round() / self.steps;
        }
        // loud sounds are rounded off before they clip
        if x.abs() > 0.5 {
            x = x.signum() * (0.5 + ((x.abs() - 0.5) / 1.5).tanh() * 1.5);
        }
        x *= self.hold * self.gain;
        self.hold *= self.hold_fall;
        if !x.is_finite() {
            x = 0.0;
        }
        [x, x]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(p: &[f32], vel: f32, secs: f32) -> Vec<f32> {
        let s = set(p);
        let mut v = Voice::start(&s, 48000.0, vel, 1.0, 7);
        (0..(secs * 48000.0) as usize).map(|_| v.frame(&s)[0]).collect()
    }

    fn peak(b: &[f32]) -> f32 {
        b.iter().fold(0.0f32, |m, f| m.max(f.abs()))
    }

    fn crossings(b: &[f32]) -> usize {
        b.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn a_new_block_reads_back_and_sounds() {
        let raw = fresh();
        assert_eq!(raw.len(), 300);
        assert!(fits(&raw));
        assert_eq!(values(&raw), defaults());
        let out = play(&defaults(), 0.8, 0.5);
        let top = peak(&out);
        assert!(top > 0.05 && top < 1.5, "{top}");
        assert!(peak(&out[20000..]) < top * 0.5);
    }

    #[test]
    fn size_tension_and_pitch_move_the_tone() {
        let mut p = defaults();
        p[4] = 0.0;
        let base = crossings(&play(&p, 0.8, 0.2));
        for (i, v) in [(11usize, 0.2f32), (7, 0.7), (29, 0.9)] {
            let mut q = p.clone();
            q[i] = v;
            assert!(crossings(&play(&q, 0.8, 0.2)) > base + base / 4, "setting {i}");
        }
    }

    #[test]
    fn a_link_from_the_hit_strength_moves_its_setting() {
        let mut p = defaults();
        p[2] = 0.1;
        let soft = peak(&play(&p, 0.2, 0.2));
        let hard = peak(&play(&p, 1.0, 0.2));
        assert!(hard > soft * 2.5, "{soft} {hard}");
        p[30] = 0.0;
        let a = peak(&play(&p, 0.2, 0.2));
        let b = peak(&play(&p, 1.0, 0.2));
        assert!((a - b).abs() < 1e-6);
    }

    #[test]
    fn the_same_noise_comes_back_only_when_asked() {
        let mut p = defaults();
        p[4] = 1.0;
        p[0] = 1.0;
        let s = set(&p);
        let run = |seed: u32| {
            let mut v = Voice::start(&s, 48000.0, 0.8, 1.0, seed);
            (0..2000).map(|_| v.frame(&s)[0]).collect::<Vec<f32>>()
        };
        assert_eq!(run(1), run(2));
        p[0] = 0.0;
        let s = set(&p);
        let run = |seed: u32| {
            let mut v = Voice::start(&s, 48000.0, 0.8, 1.0, seed);
            (0..2000).map(|_| v.frame(&s)[0]).collect::<Vec<f32>>()
        };
        assert_ne!(run(1), run(2));
    }

    #[test]
    fn hold_cuts_the_ring_short_and_crush_steps_it() {
        let mut p = defaults();
        let long = play(&p, 0.8, 0.4);
        p[28] = 0.2;
        let short = play(&p, 0.8, 0.4);
        assert!(peak(&short[9600..]) < peak(&long[9600..]) * 0.1);
        p[28] = 1.0;
        p[27] = 1.0;
        let rough = play(&p, 0.8, 0.1);
        let mut seen: Vec<i32> = rough.iter().map(|v| (v * 1e5) as i32).collect();
        seen.sort();
        seen.dedup();
        assert!(seen.len() < 24, "{}", seen.len());
    }

    #[test]
    fn a_broken_block_is_left_alone() {
        assert!(values(b"SSLF").is_empty());
        let s = set(&[]);
        assert!(s.tail() > 0.05);
    }
}
