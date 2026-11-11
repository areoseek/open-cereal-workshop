//! The drive rack: up to four modules in a row (distortion, filter, chorus, speaker cabinet), each with its own
//! input level, mix and output level.
//!
//! Stored block (file identifier "Distructor"): u32 version, u32 module count, the module records, a tail.
//! A module record starts with its own u32 version (3: 227 bytes, 4: 231 bytes) and holds every setting of every
//! module type, whatever the module currently is:
//!   4 type (0 distortion, 1 filter, 2 chorus, 3 cabinet), 8 distortion or chorus model, 18 filter model,
//!   22 on (byte), 23 cabinet impulse number, 27.. fifty 32-bit settings.
//! The tail starts with the soft-clip-output switch (byte); newer tails carry the module order after it.
//! Which of the fifty settings a model reads was worked out from stored states; the sound of each model is ours.

use crate::fx::{svf, svf_g, Ctx};
use std::f32::consts::{PI, TAU};

pub const SLOTS: usize = 4;
/// Settings in front of the modules: soft clip output, module count.
pub const HEAD: usize = 2;
/// Per module: type, on, model, filter model, impulse number, then the fifty stored settings.
pub const LEAD: usize = 5;
pub const PER: usize = LEAD + 50;
pub const LEN: usize = HEAD + SLOTS * PER;

pub const TYPES: [&str; 4] = ["Distortion", "Filter", "Chorus", "Cabinet"];
pub const DRIVES: [&str; 18] = [
    "Overdrive", "Quick", "Soft clip", "Classic", "Flat top", "Cubic", "Log", "Soft sat", "Rubber", "Ribbon", "Sine crush", "Bit crush", "Bit jam", "Warm", "Folder", "Aperture", "Abrasive", "Crusher",
];
pub const FILTERS: [&str; 22] = [
    "Low pass 6", "Low pass 12", "Low pass 12 B", "Low pass 24", "Low pass 24 B", "High pass 6", "High pass 12", "High pass 12 B", "High pass 24", "Band pass 12", "Band pass 12 B", "Low shelf", "High shelf",
    "Notch", "Peak", "Phaser 5", "Phaser 4", "Phaser 6", "Vowel", "Comb +", "Comb -", "All pass",
];
pub const CHORUSES: [&str; 3] = ["Mono", "Stereo", "Wide"];

/// Index of the fifty settings: in, mix, out, then the ones each kind of module reads.
pub const IN: usize = 36;
pub const MIX: usize = 37;
pub const OUT: usize = 38;
pub const CUT: usize = 39;
pub const RES: usize = 40;
pub const SEP_IN: usize = 41;
pub const SEP_OUT: usize = 42;

const DEFAULTS: [i32; 50] = [
    0, 5000, 0, 10000, 0, 0, 500, 5000, 5000, 10000, 500, 5000, 5000, 10000, 2000, 0, 128, 10, 128, 128, 0, 100, 128, 500, 0, 1000, 0, 950, 2000, 5000, 2000, 5000, 8400, 0, 0, 0, 5000, 10000, 5000, 10000, 0, 0, 0,
    108, 2880, 1500, 1995, 0, 0, 0,
];

/// The settings a module shows: (name, index into the fifty, min, max).
pub fn shown(kind: i32, model: i32) -> &'static [(&'static str, usize, i32, i32)] {
    match (kind, model) {
        (0, 0) => &[("Pre band", 0, 0, 10000), ("Colour", 1, 0, 10000), ("Pre amp", 2, 0, 10000), ("Post gain", 3, 0, 10000), ("Post filter", 4, 0, 10000), ("x100", 5, 0, 1)],
        (0, 1) => &[("Pre amp", 16, 0, 256), ("Threshold", 17, 1, 10), ("Mix", 18, 0, 128), ("Post gain", 19, 0, 128), ("Soft", 20, 0, 1)],
        (0, 2) => &[("Threshold", 21, 1, 127), ("Post gain", 22, 0, 160)],
        (0, 3..=13) => &[("Amount", 23, 0, 1000), ("Bias", 24, -1000, 1000), ("Level", 25, 0, 1000)],
        (0, 14) => &[("Drive", 6, 0, 10000), ("Symmetry", 7, 0, 10000), ("Shape", 8, 0, 10000), ("Level", 9, 0, 10000)],
        (0, 15) => &[("Amount", 10, 0, 10000), ("Centre", 11, 0, 10000), ("Shape", 12, 0, 10000), ("Level", 13, 0, 10000), ("Tone", 14, 0, 10000), ("Wide", 15, 0, 1)],
        (0, 16) => &[("Drive", 27, 0, 2000), ("Tone", 28, 0, 10000), ("Noise", 29, 0, 10000), ("Rate", 30, 0, 10000), ("Width", 31, 0, 10000), ("Level", 32, 0, 10000), ("Gate", 33, 0, 1)],
        (0, _) => &[("Rate crush", 34, 0, 10000), ("Bit crush", 35, 0, 10000)],
        (1, _) => &[("Cutoff", CUT, 0, 10000), ("Resonance", RES, 0, 10000)],
        (2, _) => &[("Delay", 43, 0, 216), ("Feedback", 44, 0, 2880), ("Speed", 45, 0, 6000), ("Depth", 46, 0, 6000), ("Blur", 47, 0, 6000), ("Colour", 48, -6000, 6000)],
        _ => &[("Separation in", SEP_IN, -1000, 1000), ("Separation out", SEP_OUT, -1000, 1000)],
    }
}

/// File names of the cabinet impulses, in menu order. They live in the user's own content.
pub fn cabinet_file(n: i32) -> Option<String> {
    const PAIRS: [&str; 10] = ["A", "C", "E", "G", "I", "K", "M", "O", "Q", "S"];
    let mut names: Vec<String> = vec![];
    for a in PAIRS {
        let b = (a.as_bytes()[0] + 1) as char;
        names.push(format!("Model {a}"));
        names.push(format!("Model {a}{b}"));
        names.push(format!("Model {b}"));
    }
    for m in ["Close", "Closer", "Closest", "Far", "Farther", "Medium"] {
        names.push(format!("Mono {m}"));
    }
    names.push("Stereo Blues".into());
    for i in 1..=14 {
        names.push(format!("Stereo Combo {i:02}"));
    }
    for m in ["Guitaro", "Metal", "Muffled", "Wide"] {
        names.push(format!("Stereo {m}"));
    }
    names.get(usize::try_from(n).ok()?).map(|s| format!("IMP Cabinet {s}.wv"))
}

pub const CABINETS: i32 = 55;

/// The impulse file the first switched-in cabinet of a stored block wants.
pub fn cabinet_named_in(state: &[u8]) -> Option<String> {
    let p = decode(state)?;
    cabinet_of(&p).and_then(cabinet_file)
}

/// Impulse number of the first cabinet module.
pub fn cabinet_of(p: &[f32]) -> Option<i32> {
    (0..count(p)).map(|m| &p[HEAD + m * PER..HEAD + (m + 1) * PER]).find(|m| m[0] as i32 == 3).map(|m| m[4] as i32)
}

pub fn count(p: &[f32]) -> usize {
    (p.get(1).copied().unwrap_or(0.0) as usize).min(SLOTS).min(p.len().saturating_sub(HEAD) / PER)
}

fn size_of(version: u32) -> Option<usize> {
    match version {
        3 => Some(227),
        4 => Some(231),
        _ => None,
    }
}

/// The module records of a block (as byte ranges) and where the tail starts.
fn records(state: &[u8]) -> Option<(Vec<std::ops::Range<usize>>, usize)> {
    let int = |o: usize| state.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let n = int(4)? as usize;
    if n > SLOTS {
        return None;
    }
    let mut at = 8;
    let mut out = vec![];
    for _ in 0..n {
        let size = size_of(int(at)?)?;
        if at + size > state.len() {
            return None;
        }
        out.push(at..at + size);
        at += size;
    }
    Some((out, at))
}

/// The order the records are heard in.
fn order(state: &[u8], tail: usize, n: usize) -> Vec<usize> {
    let mut o: Vec<usize> = (0..n).collect();
    if state.len() >= tail + 21 {
        let read: Vec<usize> = (0..SLOTS).map(|i| u32::from_le_bytes(state[tail + 5 + i * 4..tail + 9 + i * 4].try_into().unwrap()) as usize).filter(|&x| x < n).collect();
        let mut seen = read.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() == n && read.len() == n {
            o = read;
        }
    }
    o
}

pub fn decode(state: &[u8]) -> Option<Vec<f32>> {
    let (recs, tail) = records(state)?;
    let mut p = vec![0.0; LEN];
    p[0] = state.get(tail).copied().unwrap_or(0) as f32;
    p[1] = recs.len() as f32;
    // modules are kept in the order they are heard
    for (slot, &k) in order(state, tail, recs.len()).iter().enumerate() {
        let r = &state[recs[k].clone()];
        let int = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
        let m = &mut p[HEAD + slot * PER..HEAD + (slot + 1) * PER];
        m[0] = int(4) as f32;
        m[1] = r[22] as f32;
        m[2] = int(8) as f32;
        m[3] = int(18) as f32;
        m[4] = int(23) as f32;
        for i in 0..50 {
            m[LEAD + i] = int(27 + i * 4) as f32;
        }
    }
    for slot in recs.len()..SLOTS {
        blank(&mut p[HEAD + slot * PER..HEAD + (slot + 1) * PER], slot as i32);
    }
    Some(p)
}

/// A module of the given type at its defaults.
pub fn blank(m: &mut [f32], kind: i32) {
    m[0] = kind as f32;
    m[1] = 1.0;
    m[2] = if kind == 0 { 3.0 } else { 0.0 };
    m[3] = 2.0;
    m[4] = if kind == 3 { 10.0 } else { 1.0 };
    for (a, b) in m[LEAD..].iter_mut().zip(DEFAULTS) {
        *a = b as f32;
    }
}

fn fresh_record() -> Vec<u8> {
    let mut r = vec![0u8; 231];
    r[0] = 4;
    r[12] = 1;
    r[16] = 1;
    r
}

/// Write the settings into a block. An untouched block comes back byte for byte.
pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let parsed = base.and_then(|b| records(b).map(|(r, t)| (b, r, t)));
    let n = count(p);
    let mut out = vec![];
    let version = parsed.as_ref().map(|(b, _, _)| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(5);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    let ord = match &parsed {
        Some((b, r, t)) if r.len() == n => order(b, *t, n),
        _ => (0..n).collect(),
    };
    let mut recs: Vec<Vec<u8>> = (0..n)
        .map(|k| match &parsed {
            Some((b, r, _)) if k < r.len() => b[r[k].clone()].to_vec(),
            _ => fresh_record(),
        })
        .collect();
    for (slot, &k) in ord.iter().enumerate() {
        let m = &p[HEAD + slot * PER..HEAD + (slot + 1) * PER];
        let r = &mut recs[k];
        let mut put = |o: usize, v: f32| r[o..o + 4].copy_from_slice(&(v.round() as i32).to_le_bytes());
        put(4, m[0]);
        put(8, m[2]);
        put(18, m[3]);
        put(23, m[4]);
        for i in 0..50 {
            put(27 + i * 4, m[LEAD + i]);
        }
        r[22] = (m[1] >= 0.5) as u8;
    }
    for r in recs {
        out.extend_from_slice(&r);
    }
    let soft = (p[0] >= 0.5) as u8;
    match &parsed {
        Some((b, r, t)) if r.len() == n && b.len() > *t => {
            out.push(soft);
            out.extend_from_slice(&b[t + 1..]);
        }
        _ => {
            out.push(soft);
            out.extend_from_slice(&4u32.to_le_bytes());
            for i in 0..SLOTS as u32 {
                out.extend_from_slice(&i.to_le_bytes());
            }
            out.extend_from_slice(&[0, 0, 0, 0, 1]);
        }
    }
    out
}

/// A new rack: one distortion, a filter, a chorus and a cabinet, as the original's default has it.
pub fn fresh() -> Vec<u8> {
    let mut p = vec![0.0; LEN];
    p[0] = 1.0;
    p[1] = SLOTS as f32;
    for slot in 0..SLOTS {
        blank(&mut p[HEAD + slot * PER..HEAD + (slot + 1) * PER], slot as i32);
    }
    encode(&p, None)
}

#[derive(Clone, Default)]
struct Slot {
    z: [f32; 32],
    line: Vec<[f32; 2]>,
    pos: usize,
    ph: f32,
    hold: [f32; 2],
    count: f32,
    seed: u32,
}

#[derive(Default)]
pub struct State {
    slots: [Slot; SLOTS],
    pub conv: Option<Box<crate::conv::State>>,
}

fn level(v: f32) -> f32 {
    let x = v / 5000.0;
    x * x
}

fn fold(v: f32) -> f32 {
    4.0 * ((v * 0.25 + 0.25).rem_euclid(1.0) - 0.5).abs() - 1.0
}

/// One of the curve models: `a` is the amount (0..=1).
fn curve(model: i32, x: f32, a: f32) -> f32 {
    let d = 1.0 + a * a * 40.0;
    let v = x * d;
    match model {
        4 => v.clamp(-1.0, 1.0),
        5 => {
            let c = v.clamp(-1.0, 1.0);
            1.5 * c - 0.5 * c * c * c
        }
        6 => v.signum() * (1.0 + v.abs() * 4.0).ln() / 5f32.ln().max(1.0) * 0.8,
        7 => v / (1.0 + v.abs()),
        8 => (v.clamp(-1.0, 1.0) * PI * 0.5).sin(),
        9 => fold(v),
        10 => (v * PI * 0.5).sin(),
        11 => {
            let steps = (16.0 - 14.0 * a).exp2();
            (x * steps).round() / steps
        }
        12 => {
            let bits = (a * 14.0) as u32;
            let i = (x.clamp(-1.0, 1.0) * 32767.0) as i32;
            ((i >> bits) << bits) as f32 / 32767.0
        }
        13 => v.tanh() * 0.9 + (v * 0.5).tanh() * 0.1,
        _ => v.tanh(),
    }
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if p.len() < LEN {
        return;
    }
    let sr = cx.sr;
    let mut cab_done = false;
    for slot in 0..count(p) {
        let m = &p[HEAD + slot * PER..HEAD + (slot + 1) * PER];
        if m[1] < 0.5 {
            continue;
        }
        let s = &mut st.slots[slot];
        let v = |i: usize| m[LEAD + i];
        let (gi, mix, go) = (level(v(IN)), (v(MIX) / 10000.0).clamp(0.0, 1.0), level(v(OUT)));
        let kind = m[0] as i32;
        let model = m[2] as i32;
        match kind {
            0 => {
                let g_band = svf_g(200.0 * 40f32.powf(v(1) / 10000.0), sr);
                let amp = (1.0 + v(2) / 10000.0 * 9.0) * if v(5) >= 0.5 { 100.0 } else { 1.0 };
                let pf = (v(4) / 10000.0).clamp(0.0, 1.0);
                let g_post = svf_g(20000.0 * 0.02f32.powf(pf), sr);
                let a = (v(23) / 1000.0).clamp(0.0, 1.0);
                let bias = v(24) / 1000.0 * 0.5;
                let lvl = v(25) / 1000.0;
                let hold = 1.0 + (v(34) / 10000.0).powi(2) * 60.0 * sr / 44100.0;
                let steps = (16.0 - 14.0 * (v(35) / 10000.0).clamp(0.0, 1.0)).exp2();
                for f in io.iter_mut() {
                    let dry = *f;
                    if model == 17 {
                        s.count += 1.0;
                        if s.count >= hold {
                            s.count -= hold;
                            s.hold = [f[0] * gi, f[1] * gi];
                        }
                    }
                    for ch in 0..2 {
                        let x = f[ch] * gi;
                        let y = match model {
                            0 => {
                                let (_, bp, _) = svf(&mut s.z[ch * 4..], x, g_band, 1.0);
                                let y = ((x + bp * v(0) / 10000.0 * 4.0) * amp).tanh();
                                let (lp, _, _) = svf(&mut s.z[ch * 4 + 2..], y, g_post, 1.4);
                                (if pf > 0.0 { lp } else { y }) * v(3) / 10000.0
                            }
                            1 => {
                                let d = x * (v(16) / 128.0).powi(4);
                                let thr = (v(17) / 10.0).clamp(0.05, 1.0);
                                let w = if v(20) >= 0.5 { thr * (d / thr).tanh() } else { d.clamp(-thr, thr) };
                                let k = (v(18) / 128.0).clamp(0.0, 1.0);
                                (x * (1.0 - k) + w * k) * v(19) / 128.0
                            }
                            2 => {
                                let t = (v(21) / 127.0).clamp(0.01, 0.999);
                                let room = 1.0 - t;
                                let a = x.abs();
                                (if a > t { x.signum() * (t + room * ((a - t) / room).tanh()) } else { x }) * v(22) / 128.0
                            }
                            3..=13 => (curve(model, x + bias * a, a) - curve(model, bias * a, a)) * lvl,
                            14 => {
                                let g = 1.0 + v(6) / 10000.0 * 30.0;
                                let shape = v(8) / 10000.0;
                                let w = x * g + (v(7) - 5000.0) / 5000.0 * 0.5;
                                (fold(w) * shape * 2.0 + (w * PI * 0.5).sin() * (1.0 - shape * 2.0).max(0.0)).clamp(-1.0, 1.0) * v(9) / 10000.0
                            }
                            15 => {
                                let amt = v(10) / 10000.0;
                                let w = (x * (1.0 + amt * 8.0)).clamp(-1.0, 1.0);
                                let e = 1.0 + amt * 6.0 * (v(12) / 5000.0);
                                w.signum() * (1.0 - (1.0 - w.abs()).powf(e)) * v(13) / 10000.0
                            }
                            16 => {
                                s.seed = s.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                let n = (s.seed >> 8) as f32 / 8_388_608.0 - 1.0;
                                let w = (x * (1.0 + v(27) / 250.0)).tanh();
                                w * (1.0 - v(29) / 10000.0 * (0.5 + 0.5 * n)) * v(32) / 8400.0
                            }
                            _ => (s.hold[ch] * steps).round() / steps,
                        };
                        f[ch] = (dry[ch] * (1.0 - mix) + y * mix) * go;
                    }
                }
            }
            1 => {
                let fm = m[3] as i32;
                let c = (v(CUT) / 10000.0).clamp(0.0, 1.0);
                let r = (v(RES) / 10000.0).clamp(0.0, 1.0);
                let hz = 20.0 * 1000f32.powf(c);
                let g = svf_g(hz, sr);
                let k = 2.0 - 1.9 * r;
                let pole = 1.0 - (-TAU * hz / sr).exp();
                let shelf = 10f32.powf((r - 0.5) * 36.0 / 20.0);
                let n = (sr / hz.max(25.0)) as usize + 2;
                if matches!(fm, 19 | 20) && s.line.len() < (sr / 20.0) as usize + 4 {
                    s.line = vec![[0.0; 2]; (sr / 20.0) as usize + 4];
                    s.pos = 0;
                }
                let stages = match fm {
                    15 => 5,
                    16 => 4,
                    _ => 6,
                };
                let vw = c * 4.0;
                let (vi, vf) = ((vw as usize).min(3), vw - (vw as usize).min(3) as f32);
                const VOWELS: [[f32; 2]; 5] = [[800.0, 1150.0], [400.0, 2000.0], [300.0, 2500.0], [450.0, 800.0], [325.0, 700.0]];
                let shift = (r * 2.0 - 1.0).exp2();
                let f1 = svf_g((VOWELS[vi][0] + (VOWELS[vi + 1][0] - VOWELS[vi][0]) * vf) * if fm == 18 { shift } else { 1.0 }, sr);
                let f2 = svf_g((VOWELS[vi][1] + (VOWELS[vi + 1][1] - VOWELS[vi][1]) * vf) * if fm == 18 { shift } else { 1.0 }, sr);
                for f in io.iter_mut() {
                    let dry = *f;
                    for ch in 0..2 {
                        let x = f[ch] * gi;
                        let z = &mut s.z[ch * 16..ch * 16 + 16];
                        let y = match fm {
                            0 | 5 | 11 | 12 => {
                                z[0] += pole * (x - z[0]);
                                match fm {
                                    0 => z[0],
                                    5 => x - z[0],
                                    11 => z[0] * shelf + (x - z[0]),
                                    _ => z[0] + (x - z[0]) * shelf,
                                }
                            }
                            1 | 2 => svf(z, x, g, k).0,
                            3 | 4 => {
                                let a = svf(z, x, g, 1.4).0;
                                svf(&mut z[2..], a, g, k).0
                            }
                            6 | 7 => svf(z, x, g, k).2,
                            8 => {
                                let a = svf(z, x, g, 1.4).2;
                                svf(&mut z[2..], a, g, k).2
                            }
                            9 | 10 => svf(z, x, g, k).1 * k.max(0.3),
                            13 => {
                                let q = 0.1 + 1.9 * r;
                                let (lp, _, hp) = svf(z, x, g, q);
                                lp + hp
                            }
                            14 => {
                                let (_, bp, _) = svf(z, x, g, 0.5);
                                x + bp * 0.5 * (10f32.powf(r * 24.0 / 20.0) - 1.0)
                            }
                            15..=17 => {
                                let mut a = x;
                                for i in 0..stages {
                                    let fr = hz * (1.0 + r * 0.5 * i as f32);
                                    let t = (PI * (fr / sr).min(0.49)).tan();
                                    let co = (t - 1.0) / (t + 1.0);
                                    let o = co * a + z[i];
                                    z[i] = a - co * o;
                                    a = o;
                                }
                                (x + a) * 0.5
                            }
                            18 => {
                                let a = svf(z, x, f1, 0.15).1 * 0.15;
                                let b = svf(&mut z[2..], x, f2, 0.15).1 * 0.15;
                                (a + b * 0.7) * 2.0
                            }
                            19 | 20 => {
                                let len = s.line.len();
                                let back = s.line[(s.pos + len - n.min(len - 1)) % len][ch];
                                let sign = if fm == 19 { 1.0 } else { -1.0 };
                                let o = x + back * r * 0.95 * sign;
                                s.line[s.pos][ch] = o;
                                o * (1.0 - r * 0.5)
                            }
                            _ => {
                                let q = 0.1 + 1.9 * r;
                                let (_, bp, _) = svf(z, x, g, q);
                                x - 2.0 * q * bp
                            }
                        };
                        f[ch] = (dry[ch] * (1.0 - mix) + y * mix) * go;
                    }
                    if matches!(fm, 19 | 20) {
                        s.pos = (s.pos + 1) % s.line.len();
                    }
                }
            }
            2 => {
                let want = (sr * 0.08) as usize + 8;
                if s.line.len() != want {
                    s.line = vec![[0.0; 2]; want];
                    s.pos = 0;
                }
                let base = v(43).max(0.0) * 0.0001 * sr;
                let strength = (v(44) / 2880.0).clamp(0.0, 1.0);
                let step = 0.1 * 100f32.powf((v(45) / 6000.0).clamp(0.0, 1.0)) / sr;
                let depth = (v(46) / 6000.0).clamp(0.0, 1.0) * 0.008 * sr;
                let voices = 1 + ((v(47) / 1500.0).round() as usize).min(4);
                let col = (v(48) / 6000.0).clamp(-1.0, 1.0);
                let lp = 1.0 - (-TAU * 18000.0 * 0.03f32.powf((-col).max(0.0)) / sr).exp();
                let hp = 1.0 - (-TAU * 20.0 * 150f32.powf(col.max(0.0)) / sr).exp();
                let len = s.line.len();
                for f in io.iter_mut() {
                    let dry = *f;
                    let x = [f[0] * gi, f[1] * gi];
                    s.line[s.pos] = if model == 0 { [(x[0] + x[1]) * 0.5; 2] } else { x };
                    let mut wet = [0.0f32; 2];
                    for vo in 0..voices {
                        for ch in 0..2 {
                            let off = vo as f32 / voices as f32 + if model == 2 && ch == 1 { 0.25 } else { 0.0 };
                            let d = base + depth * (0.5 + 0.5 * ((s.ph + off) * TAU).sin()) + 1.0;
                            let at = s.pos as f32 + len as f32 - d.min(len as f32 - 2.0);
                            let i = at as usize;
                            let fr = at - i as f32;
                            let src = if model == 2 && vo % 2 == 1 { 1 - ch } else { ch };
                            let a = s.line[i % len][src];
                            let b = s.line[(i + 1) % len][src];
                            wet[ch] += (a + (b - a) * fr) / voices as f32;
                        }
                    }
                    s.ph = (s.ph + step).fract();
                    s.pos = (s.pos + 1) % len;
                    for ch in 0..2 {
                        let z = &mut s.z[ch * 2..ch * 2 + 2];
                        z[0] += lp * (wet[ch] - z[0]);
                        z[1] += hp * (z[0] - z[1]);
                        let w = z[0] - z[1];
                        let y = x[ch] + w * strength;
                        f[ch] = (dry[ch] * (1.0 - mix) + y * mix) * go;
                    }
                }
            }
            _ => {
                // one cabinet is heard; a second one in the same rack passes the sound on
                let Some(cv) = st.conv.as_mut().filter(|_| !cab_done) else { continue };
                cab_done = true;
                let (si, so) = (1.0 - v(SEP_IN) / 1000.0, 1.0 - v(SEP_OUT) / 1000.0);
                for f in io.iter_mut() {
                    let dry = *f;
                    let (md, sd) = ((f[0] + f[1]) * 0.5 * gi, (f[0] - f[1]) * 0.5 * gi * si);
                    let y = cv.tick([md + sd, md - sd]);
                    let (ym, ys) = ((y[0] + y[1]) * 0.5, (y[0] - y[1]) * 0.5 * so);
                    f[0] = (dry[0] * (1.0 - mix) + (ym + ys) * mix) * go;
                    f[1] = (dry[1] * (1.0 - mix) + (ym - ys) * mix) * go;
                }
            }
        }
    }
    if p[0] >= 0.5 {
        for f in io.iter_mut() {
            for x in f.iter_mut() {
                let a = x.abs();
                if a > 0.6 {
                    *x = x.signum() * (0.6 + 0.4 * ((a - 0.6) / 0.4).tanh());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_rack_reads_back_and_keeps_what_it_does_not_know() {
        let b = fresh();
        assert_eq!(b.len(), 8 + 4 * 231 + 26);
        let p = decode(&b).unwrap();
        assert_eq!(count(&p), 4);
        assert_eq!(encode(&p, Some(&b)), b);
        assert_eq!(cabinet_of(&p), Some(10));
        assert_eq!(cabinet_file(0).as_deref(), Some("IMP Cabinet Model A.wv"));
        assert_eq!(cabinet_file(CABINETS - 1).as_deref(), Some("IMP Cabinet Stereo Wide.wv"));
        assert_eq!(cabinet_file(CABINETS), None);
        // unknown bytes of a record and the tail survive a change
        let mut odd = b.clone();
        odd[8 + 13] = 0x55;
        let n = odd.len();
        odd[n - 1] = 0x77;
        let mut p = decode(&odd).unwrap();
        p[HEAD + LEAD + MIX] = 1234.0;
        let out = encode(&p, Some(&odd));
        assert_eq!(out[8 + 13], 0x55);
        assert_eq!(out[n - 1], 0x77);
        assert_eq!(decode(&out).unwrap()[HEAD + LEAD + MIX], 1234.0);
        // fewer modules: a shorter block that still reads
        p[1] = 2.0;
        let short = encode(&p, Some(&odd));
        assert_eq!(count(&decode(&short).unwrap()), 2);
    }

    #[test]
    fn every_model_keeps_the_sound_finite_and_bounded() {
        let cx = Ctx { sr: 44_100.0, step: 5512.0 };
        for kind in 0..3 {
            let models = [DRIVES.len(), FILTERS.len(), CHORUSES.len()][kind];
            for model in 0..models {
                let mut p = vec![0.0; LEN];
                p[1] = 1.0;
                blank(&mut p[HEAD..HEAD + PER], kind as i32);
                p[HEAD + 2] = model as f32;
                p[HEAD + 3] = model as f32;
                p[HEAD + LEAD + 34] = 5000.0;
                p[HEAD + LEAD + 35] = 5000.0;
                p[HEAD + LEAD + RES] = 8000.0;
                p[HEAD + LEAD + CUT] = 5000.0;
                let mut st = State::default();
                let mut io: Vec<[f32; 2]> = (0..8192).map(|i| [(i as f32 * 0.05).sin() * 0.5; 2]).collect();
                process(&p, &mut st, &mut io, &cx);
                let peak = io.iter().map(|f| f[0].abs().max(f[1].abs())).fold(0.0, f32::max);
                assert!(peak.is_finite() && peak < 8.0, "{kind} {model}: {peak}");
                if kind != 1 {
                    assert!(peak > 0.01, "{kind} {model} went silent");
                }
            }
        }
    }
}
