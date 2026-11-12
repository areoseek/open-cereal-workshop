//! The band vocoder (format identifier "Vocodex"): one sound (the modulator) opens and closes up to a hundred
//! bands of another (the carrier). Either can be the insert's own sound or a mixer track routed to the insert,
//! and a small built-in synth can stand in for the carrier.
//!
//! The stored block is a four-byte version, thirteen mapping envelopes, a run of whole-number settings, a run
//! of switch bytes, the name of the built-in carrier sound and the notes it holds. Older blocks have one
//! setting fewer and four switch bytes instead of twenty-one. Everything is written back as it was read.
//! The sound is our own working-out from the public manual.

use crate::fx::Ctx;

pub const INTS: usize = 27;
pub const FLAGS: usize = 27;
pub const NFLAGS: usize = 21;
pub const CARRIER: usize = 48;
/// Count of held notes, then up to `MAXNOTES` note numbers.
pub const NOTES: usize = 49;
pub const MAXNOTES: usize = 16;
pub const ENVS: usize = 66;
pub const NENV: usize = 13;

pub const WET: usize = 0;
pub const FINISH: usize = 1;
pub const MOD_THRU: usize = 2;
pub const MOD_HP: usize = 3;
pub const CAR_THRU: usize = 4;
pub const CAR_LP: usize = 5;
pub const NOISE: usize = 6;
pub const NOISE_HP: usize = 7;
pub const HOLD: usize = 8;
pub const ATT: usize = 9;
pub const REL: usize = 11;
pub const WIDTH: usize = 14;
pub const MOD_WIDTH: usize = 16;
pub const SHIFT: usize = 17;
pub const UNI_SHIFT: usize = 18;
pub const UNI_PAN: usize = 19;
pub const RMS: usize = 20;
pub const ORDER: usize = 21;
pub const UNISON: usize = 22;
pub const BANDS: usize = 23;
pub const MOD_SEL: usize = 25;
pub const CAR_SEL: usize = 26;

/// Switch bytes: left is modulator and right is carrier; then the four contour switches.
pub const F_LR: usize = 0;
pub const F_FROM_CAR: usize = 2;
pub const F_FROM_CONS: usize = 3;
pub const F_TO_MOD: usize = 4;
pub const F_TO_NOISE: usize = 5;

/// The mapping envelopes in stored order.
pub const ENV_NAMES: [&str; NENV] = ["Band gain", "Band pan", "Modulator through", "Gain offset", "Noise floor", "Hold", "Attack", "Release", "Band spread", "Band width", "Pitch shift", "Saturation mix", "Saturation curve"];
const E_GAIN: usize = 0;
const E_PAN: usize = 1;
const E_OFFSET: usize = 3;
const E_FLOOR: usize = 4;
const E_HOLD: usize = 5;
const E_ATT: usize = 6;
const E_REL: usize = 7;
const E_SPREAD: usize = 8;
const E_WIDTH: usize = 9;
const E_SHIFT: usize = 10;
const E_SATMIX: usize = 11;
const E_SATCURVE: usize = 12;

/// Built-in carrier sounds: the name found in files, and ours.
pub const CARRIERS: [(&str, &str); 9] = [("Saw", "Saw"), ("PWM", "Pulse sweep"), ("Supersaw", "Stacked saws"), ("Ensemble", "Ensemble"), ("Robot saw", "Hard saw"), ("Power", "Octaves"), ("Growling", "Growl"), ("Elderly", "Wobbly"), ("Phased 2", "Phased")];

const DEF_INTS: [i32; INTS] = [1000, 150, 12800, 55000, 0, 28000, 0, 55000, 1666, 1115, 0, 3500, 0, 0, 349, 100, 2000, 0, 250, 128, 0, 2, 0, 47, 0, 1, 0];
const DEF_FLAGS: [u8; NFLAGS] = [0, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0];
const DEF_ENV: [[f32; 2]; NENV] = [[0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.0, 0.0], [0.0, 0.0], [0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.143, 0.992], [0.5, 0.5], [0.5, 0.5], [0.0, 0.0], [0.0, 1.0]];
const ENV_TAIL: [u8; 20] = [0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255];

struct RawEnv<'a> {
    kind: u32,
    /// Whole point records, 24 bytes each: step from the point before, level, eight bytes of shape.
    points: &'a [u8],
    tail: &'a [u8],
}

struct Raw<'a> {
    head: &'a [u8],
    envs: Vec<RawEnv<'a>>,
    ints: Vec<i32>,
    flags: &'a [u8],
    name: &'a [u8],
    notes: Vec<i32>,
}

fn u32_at(s: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(s.get(o..o + 4)?.try_into().ok()?))
}

fn parse(s: &[u8]) -> Option<Raw<'_>> {
    let head = s.get(..4)?;
    let mut o = 4;
    let mut envs = Vec::with_capacity(NENV);
    for _ in 0..NENV {
        if u32_at(s, o)? != 3 {
            return None;
        }
        let kind = u32_at(s, o + 4)?;
        let n = u32_at(s, o + 8)? as usize;
        if !(1..=3).contains(&kind) || n > 4096 {
            return None;
        }
        o += 12;
        let points = s.get(o..o + n * 24)?;
        o += n * 24;
        let tail = s.get(o..o + 20)?;
        o += 20;
        envs.push(RawEnv { kind, points, tail });
    }
    for (ni, nf) in [(27usize, 21usize), (27, 4), (26, 4)] {
        let f = o + ni * 4;
        let Some(&len) = s.get(f + nf) else { continue };
        let at = f + nf + 1 + len as usize;
        let Some(count) = u32_at(s, at) else { continue };
        if count > 64 || at + 4 + count as usize * 4 != s.len() {
            continue;
        }
        let ints = (0..ni).map(|i| u32_at(s, o + i * 4).unwrap_or(0) as i32).collect();
        let notes = (0..count as usize).map(|i| u32_at(s, at + 4 + i * 4).unwrap_or(0) as i32).collect();
        return Some(Raw { head, envs, ints, flags: &s[f..f + nf], name: &s[f + nf + 1..at], notes });
    }
    None
}

fn carrier_index(name: &[u8]) -> usize {
    CARRIERS.iter().position(|c| c.0.as_bytes() == name).unwrap_or(0)
}

fn points_of(raw: &[u8]) -> Vec<[f32; 2]> {
    let mut x = 0f64;
    raw.chunks_exact(24)
        .map(|c| {
            x += f64::from_le_bytes(c[..8].try_into().unwrap());
            [x as f32, f64::from_le_bytes(c[8..16].try_into().unwrap()) as f32]
        })
        .collect()
}

pub fn decode(state: &[u8]) -> Option<Vec<f32>> {
    let raw = parse(state)?;
    let mut p = vec![0f32; ENVS];
    let mut ints = raw.ints.clone();
    if ints.len() == 26 {
        ints.insert(NOISE_HP, DEF_INTS[NOISE_HP]);
    }
    for (i, v) in ints.iter().enumerate() {
        p[i] = *v as f32;
    }
    for (i, d) in DEF_FLAGS.iter().enumerate() {
        p[FLAGS + i] = *d as f32;
    }
    if raw.flags.len() == NFLAGS {
        for (i, b) in raw.flags.iter().enumerate() {
            p[FLAGS + i] = *b as f32;
        }
    } else {
        p[FLAGS + F_LR] = raw.flags[0] as f32;
    }
    p[CARRIER] = carrier_index(raw.name) as f32;
    let n = raw.notes.len().min(MAXNOTES);
    p[NOTES] = n as f32;
    for (i, v) in raw.notes.iter().take(n).enumerate() {
        p[NOTES + 1 + i] = *v as f32;
    }
    for e in &raw.envs {
        let pts = points_of(e.points);
        p.push(pts.len() as f32);
        for q in pts {
            p.extend_from_slice(&q);
        }
    }
    Some(p)
}

/// Where envelope `e` sits in the settings: (index of its first point, point count).
pub fn env_at(p: &[f32], e: usize) -> Option<(usize, usize)> {
    let mut o = ENVS;
    for i in 0..NENV {
        let n = *p.get(o)? as usize;
        if p.len() < o + 1 + n * 2 {
            return None;
        }
        if i == e {
            return Some((o + 1, n));
        }
        o += 1 + n * 2;
    }
    None
}

/// The points of envelope `e`, each as (place across the bands, level).
pub fn env_points(p: &[f32], e: usize) -> Option<Vec<[f32; 2]>> {
    let (o, n) = env_at(p, e)?;
    Some((0..n).map(|i| [p[o + i * 2], p[o + i * 2 + 1]]).collect())
}

/// Give envelope `e` new points. False when the settings hold no such envelope or the points are unusable.
pub fn set_env_points(p: &mut Vec<f32>, e: usize, pts: &[[f32; 2]]) -> bool {
    let Some((o, n)) = env_at(p, e) else { return false };
    if pts.is_empty() || pts.len() > 4096 {
        return false;
    }
    p[o - 1] = pts.len() as f32;
    p.splice(o..o + n * 2, pts.iter().flat_map(|q| [q[0], q[1].clamp(0.0, 1.0)]));
    true
}

/// The level of envelope `e` at `t` (0..=1 across the bands).
pub fn env_level(p: &[f32], e: usize, t: f32) -> f32 {
    let Some((o, n)) = env_at(p, e) else { return DEF_ENV[e.min(NENV - 1)][0] };
    if n == 0 {
        return DEF_ENV[e][0];
    }
    let pt = |i: usize| (p[o + i * 2], p[o + i * 2 + 1]);
    if t <= pt(0).0 {
        return pt(0).1;
    }
    for i in 1..n {
        let (x1, y1) = pt(i);
        if t <= x1 {
            let (x0, y0) = pt(i - 1);
            let w = x1 - x0;
            return if w > 1e-9 { y0 + (y1 - y0) * (t - x0) / w } else { y1 };
        }
    }
    pt(n - 1).1
}

pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let raw = base.and_then(parse);
    let mut out = Vec::with_capacity(1400);
    out.extend_from_slice(raw.as_ref().map_or(&[8, 0, 0, 0][..], |r| r.head));
    for e in 0..NENV {
        let was = raw.as_ref().map(|r| &r.envs[e]);
        let now: Vec<[f32; 2]> = match env_at(p, e) {
            Some((o, n)) => (0..n).map(|i| [p[o + i * 2], p[o + i * 2 + 1]]).collect(),
            None => vec![[0.0, DEF_ENV[e][0]], [1.0, DEF_ENV[e][1]]],
        };
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&was.map_or(3, |w| w.kind).to_le_bytes());
        out.extend_from_slice(&(now.len() as u32).to_le_bytes());
        match was {
            Some(w) if points_of(w.points) == now => out.extend_from_slice(w.points),
            _ => {
                let mut last = 0f32;
                for q in &now {
                    out.extend_from_slice(&((q[0] - last).max(0.0) as f64).to_le_bytes());
                    out.extend_from_slice(&(q[1] as f64).to_le_bytes());
                    out.extend_from_slice(&[0; 8]);
                    last = q[0];
                }
            }
        }
        out.extend_from_slice(was.map_or(&ENV_TAIL[..], |w| w.tail));
    }
    let short = raw.as_ref().is_some_and(|r| r.ints.len() == 26);
    for i in 0..INTS {
        if short && i == NOISE_HP {
            continue;
        }
        out.extend_from_slice(&(p[i].round() as i32).to_le_bytes());
    }
    match raw.as_ref().map(|r| r.flags) {
        Some(f) if f.len() != NFLAGS => {
            out.push(p[FLAGS + F_LR] as u8);
            out.extend_from_slice(&f[1..]);
        }
        _ => out.extend((0..NFLAGS).map(|i| p[FLAGS + i] as u8)),
    }
    let idx = (p[CARRIER] as usize).min(CARRIERS.len() - 1);
    let name: &[u8] = match &raw {
        Some(r) if carrier_index(r.name) == idx => r.name,
        _ => CARRIERS[idx].0.as_bytes(),
    };
    out.push(name.len() as u8);
    out.extend_from_slice(name);
    let n = (p[NOTES] as usize).min(MAXNOTES);
    match &raw {
        // more held notes than we keep: the stored list stays as long as ours still starts it
        Some(r) if r.notes.len() > MAXNOTES && n == MAXNOTES && (0..n).all(|i| p[NOTES + 1 + i] as i32 == r.notes[i]) => {
            out.extend_from_slice(&(r.notes.len() as u32).to_le_bytes());
            for v in &r.notes {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        _ => {
            out.extend_from_slice(&(n as u32).to_le_bytes());
            for i in 0..n {
                out.extend_from_slice(&(p[NOTES + 1 + i] as i32).to_le_bytes());
            }
        }
    }
    out
}

pub fn fresh() -> Vec<u8> {
    let mut p = vec![0f32; ENVS];
    for (i, v) in DEF_INTS.iter().enumerate() {
        p[i] = *v as f32;
    }
    for (i, v) in DEF_FLAGS.iter().enumerate() {
        p[FLAGS + i] = *v as f32;
    }
    p[NOTES] = 1.0;
    p[NOTES + 1] = 60.0;
    for d in DEF_ENV {
        p.extend_from_slice(&[2.0, 0.0, d[0], 1.0, d[1]]);
    }
    encode(&p, None)
}

/// Which mixer inputs the unit listens to: (modulator, carrier). 0 is the insert's own sound, 1 and up count
/// the tracks routed to the insert, below 0 is nothing.
pub fn selectors(p: &[f32]) -> [i32; 2] {
    if p[FLAGS + F_LR] >= 0.5 {
        return [0, 0];
    }
    [p[MOD_SEL] as i32, p[CAR_SEL] as i32]
}

#[derive(Default, Clone)]
struct An {
    a: [f32; 3],
    k: f32,
    z: [f32; 8],
    env: f32,
    held: f32,
    hc: u32,
    ms: f32,
    pl: f32,
    pr: f32,
}

#[derive(Default, Clone)]
struct Band {
    a: [f32; 3],
    k: f32,
    z: [[f32; 4]; 2],
    an: Vec<An>,
    gain: f32,
    offset: f32,
    floor: f32,
    satmix: f32,
    att: f32,
    rel: f32,
    hold: u32,
    rms: f32,
}

#[derive(Default)]
pub struct State {
    /// Sound from the tracks picked as modulator and carrier, filled by the mixer before each block.
    pub side: [Vec<[f32; 2]>; 2],
    pub side_on: [bool; 2],
    seen: Vec<f32>,
    sr: f32,
    bands: Vec<Band>,
    stages: usize,
    order: usize,
    curve: Vec<f32>,
    m: Vec<f32>,
    ms: Vec<[f32; 2]>,
    c: Vec<[f32; 2]>,
    out: Vec<[f32; 2]>,
    lv: Vec<[f32; 2]>,
    z: [f32; 24],
    rng: u32,
    ph: Vec<f32>,
    lfo: f32,
}

fn coefs(f: f32, q: f32, sr: f32) -> ([f32; 3], f32) {
    let g = (std::f32::consts::PI * (f / sr).clamp(0.0002, 0.48)).tan();
    let k = 1.0 / q.clamp(0.3, 300.0);
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    ([a1, a2, g * a2], k)
}

/// One state-variable step: (low, band, high).
#[inline(always)]
fn step(a: &[f32; 3], k: f32, z: &mut [f32], x: f32) -> (f32, f32, f32) {
    let v3 = x - z[1];
    let v1 = a[0] * z[0] + a[1] * v3;
    let v2 = z[1] + a[1] * z[0] + a[2] * v3;
    z[0] = 2.0 * v1 - z[0];
    z[1] = 2.0 * v2 - z[1];
    (v2, v1, x - k * v1 - v2)
}

fn spread_hz(y: f32) -> f32 {
    20000.0 * 2f32.powf((y - 1.0) * 9.3)
}

fn knob_hz(v: f32) -> f32 {
    20.0 * 1000f32.powf((v / 65536.0).clamp(0.0, 1.0))
}

fn time_coef(v: f32, mult: f32, sr: f32) -> f32 {
    let t = 2.0 * (v / 10000.0).clamp(0.0, 1.0).powi(3) * mult;
    if t * sr < 1.0 { 1.0 } else { 1.0 - (-1.0 / (t * sr)).exp() }
}

fn times(y: f32) -> f32 {
    8f32.powf((y * 2.0 - 1.0).clamp(-1.0, 1.0))
}

fn build(p: &[f32], st: &mut State, sr: f32) {
    let n = (p[BANDS] as usize).clamp(5, 100);
    let order = (p[ORDER] as usize).clamp(1, 4);
    let uni = (p[UNISON] as usize).min(5);
    let voices = 1 + uni;
    let old = std::mem::take(&mut st.bands);
    let keep = old.len() == n && old.first().is_some_and(|b| b.an.len() == voices);
    let hz: Vec<f32> = (0..n).map(|b| spread_hz(env_level(p, E_SPREAD, (b as f32 + 0.5) / n as f32)).clamp(20.0, sr * 0.45)).collect();
    let width = (p[WIDTH] / 349.0).clamp(0.05, 8.0);
    let mod_width = (p[MOD_WIDTH] / 2000.0).clamp(0.05, 8.0);
    let pan_amt = (p[UNI_PAN] - 128.0) / 128.0;
    st.bands = (0..n)
        .map(|b| {
            let t = (b as f32 + 0.5) / n as f32;
            let lo = hz[b.saturating_sub(1)];
            let hi = hz[(b + 1).min(n - 1)];
            let span = if b == 0 || b == n - 1 { 1.0 } else { 2.0 };
            let oct = ((hi / lo).max(1.0001).log2() / span * width * times(env_level(p, E_WIDTH, t))).clamp(0.01, 4.0);
            let q_of = |o: f32| 1.0 / (2f32.powf(o * 0.5) - 2f32.powf(-o * 0.5));
            let (a, k) = coefs(hz[b], q_of(oct), sr);
            let pan = (env_level(p, E_PAN, t) * 2.0 - 1.0).clamp(-1.0, 1.0);
            let cents = p[SHIFT] + (env_level(p, E_SHIFT, t) * 2.0 - 1.0) * 1200.0;
            let mut band = if keep { old[b].clone() } else { Band { an: vec![An::default(); voices], ..Default::default() } };
            band.a = a;
            band.k = k;
            band.gain = (env_level(p, E_GAIN, t) * 2.0).powi(2);
            band.offset = env_level(p, E_OFFSET, t).max(0.0);
            band.floor = env_level(p, E_FLOOR, t).max(0.0);
            band.satmix = env_level(p, E_SATMIX, t).clamp(0.0, 1.0);
            band.att = time_coef(p[ATT], times(env_level(p, E_ATT, t)), sr);
            band.rel = time_coef(p[REL], times(env_level(p, E_REL, t)), sr);
            let hold = 2.0 * (p[HOLD] / 10000.0).clamp(0.0, 1.0).powi(3) * times(env_level(p, E_HOLD, t));
            band.hold = (hold * sr) as u32;
            band.rms = if hold * sr < 1.0 { 1.0 } else { 1.0 - (-1.0 / (hold * sr)).exp() };
            for (v, an) in band.an.iter_mut().enumerate() {
                let off = if voices > 1 { (v as f32 / (voices - 1) as f32) * 2.0 - 1.0 } else { 0.0 };
                let c = cents + off * p[UNI_SHIFT] / 10.0;
                let f = (hz[b] / 2f32.powf(c / 1200.0)).clamp(20.0, sr * 0.45);
                let (a, k) = coefs(f, q_of((oct * mod_width).clamp(0.01, 4.0)), sr);
                an.a = a;
                an.k = k;
                let vp = (pan + if voices > 1 { off * pan_amt * if v % 2 == 0 { 1.0 } else { -1.0 } } else { 0.0 }).clamp(-1.0, 1.0);
                let g = 1.0 / (voices as f32).sqrt();
                an.pl = g * (1.0 - vp.max(0.0));
                an.pr = g * (1.0 + vp.min(0.0));
            }
            band
        })
        .collect();
    st.stages = if order > 2 { 2 } else { 1 };
    st.order = order;
    st.curve = (0..=64).map(|i| env_level(p, E_SATCURVE, i as f32 / 64.0)).collect();
    st.sr = sr;
}

fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

fn saw(ph: &mut f32, dt: f32) -> f32 {
    *ph += dt;
    if *ph >= 1.0 {
        *ph -= 1.0;
    }
    2.0 * *ph - 1.0 - blep(*ph, dt)
}

/// The built-in carrier: every held note, in the chosen sound.
fn carrier(p: &[f32], st: &mut State, n: usize, sr: f32) {
    let notes = (p[NOTES] as usize).min(MAXNOTES);
    let kind = p[CARRIER] as usize;
    if st.ph.len() != MAXNOTES * 5 {
        st.ph = (0..MAXNOTES * 5).map(|i| (i as f32 * 0.6180339) % 1.0).collect();
    }
    for s in 0..n {
        st.lfo = (st.lfo + 1.0 / sr) % 64.0;
        let lfo = st.lfo * std::f32::consts::TAU;
        let (mut l, mut r) = (0f32, 0f32);
        for i in 0..notes {
            let f = 440.0 * 2f32.powf((p[NOTES + 1 + i] - 69.0) / 12.0);
            let dt = (f / sr).min(0.45);
            let ph = &mut st.ph[i * 5..i * 5 + 5];
            match kind {
                1 => {
                    let w = 0.5 + 0.38 * (lfo * 0.4 + i as f32).sin();
                    let a = saw(&mut ph[0], dt);
                    let q = (ph[0] + w) % 1.0;
                    let b = 2.0 * q - 1.0 - blep(q, dt);
                    let x = (a - b) * 0.7;
                    l += x;
                    r += x;
                }
                2 | 3 => {
                    let amt = if kind == 2 { 0.011 } else { 0.006 };
                    for (v, d) in [0.0f32, -1.0, 1.0, -0.45, 0.45].iter().enumerate() {
                        let x = saw(&mut ph[v], dt * (1.0 + d * amt)) * 0.45;
                        l += x * (1.0 - d.max(0.0) * 0.7);
                        r += x * (1.0 + d.min(0.0) * 0.7);
                    }
                }
                5 => {
                    let x = saw(&mut ph[0], dt) * 0.7 + saw(&mut ph[1], dt * 0.5) * 0.6;
                    l += x;
                    r += x;
                }
                6 => {
                    let x = saw(&mut ph[0], dt) * (0.6 + 0.4 * (lfo * 31.0).sin()) + saw(&mut ph[1], dt * 0.5) * 0.4;
                    l += x;
                    r += x;
                }
                7 => {
                    let x = saw(&mut ph[0], dt * (1.0 + 0.012 * (lfo * 5.5 + i as f32).sin() + 0.004 * (lfo * 13.1).sin()));
                    l += x;
                    r += x;
                }
                8 => {
                    l += saw(&mut ph[0], dt * 1.0015);
                    r += saw(&mut ph[1], dt * 0.9985);
                }
                _ => {
                    let x = saw(&mut ph[0], dt);
                    l += x;
                    r += x;
                }
            }
        }
        st.c[s] = [l * 0.22, r * 0.22];
    }
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if p.len() < ENVS + NENV {
        return;
    }
    let n = io.len();
    let sr = cx.sr;
    if st.seen != p || st.sr != sr {
        build(p, st, sr);
        st.seen.clear();
        st.seen.extend_from_slice(p);
    }
    for v in [&mut st.ms, &mut st.c, &mut st.out, &mut st.lv] {
        v.clear();
        v.resize(n, [0.0; 2]);
    }
    st.m.clear();
    st.m.resize(n, 0.0);
    if st.rng == 0 {
        st.rng = 0x2545_f491;
    }
    let lr = p[FLAGS + F_LR] >= 0.5;
    let [msel, csel] = selectors(p);
    // the modulator, in stereo for passing through and as one channel for the bands
    if lr {
        for (d, f) in st.ms.iter_mut().zip(io.iter()) {
            *d = [f[0], f[0]];
        }
    } else if msel == 0 {
        st.ms.copy_from_slice(io);
    } else if msel > 0 && st.side_on[0] && st.side[0].len() >= n {
        st.ms.copy_from_slice(&st.side[0][..n]);
    }
    for (m, f) in st.m.iter_mut().zip(&st.ms) {
        *m = (f[0] + f[1]) * 0.5;
    }
    // the carrier
    if lr {
        for (d, f) in st.c.iter_mut().zip(io.iter()) {
            *d = [f[1], f[1]];
        }
    } else if csel == 0 {
        st.c.copy_from_slice(io);
    } else if csel > 0 {
        if st.side_on[1] && st.side[1].len() >= n {
            let (c, side) = (&mut st.c, &st.side[1]);
            c.copy_from_slice(&side[..n]);
        }
    } else {
        carrier(p, st, n, sr);
    }
    st.side_on = [false; 2];

    // contour: how loud the carrier is, and how much hiss the modulator has
    let from_car = p[FLAGS + F_FROM_CAR] >= 0.5;
    let from_cons = p[FLAGS + F_FROM_CONS] >= 0.5;
    let to_mod = p[FLAGS + F_TO_MOD] >= 0.5 && (from_car || from_cons);
    let to_noise = p[FLAGS + F_TO_NOISE] >= 0.5 && (from_car || from_cons);
    let noise = p[NOISE] / 65536.0;
    let mod_thru = p[MOD_THRU] / 65536.0;
    let car_thru = p[CAR_THRU] / 65536.0;
    let (hp, hk) = coefs(knob_hz(p[MOD_HP]), 0.707, sr);
    let (lp, lk) = coefs(knob_hz(p[CAR_LP]), 0.707, sr);
    let (nh, nk) = coefs(knob_hz(p[NOISE_HP]), 0.707, sr);
    let (ch, ck) = coefs(5000.0, 0.707, sr);
    let fast = 1.0 - (-1.0 / (0.004 * sr)).exp();
    let slow = 1.0 - (-1.0 / (0.06 * sr)).exp();
    let z = &mut st.z;
    for s in 0..n {
        let c = st.c[s];
        let ca = (c[0].abs() + c[1].abs()) * 0.5;
        z[16] += (ca - z[16]) * if ca > z[16] { fast } else { slow };
        let cons = step(&ch, ck, &mut z[12..14], st.m[s]).2.abs();
        z[17] += (cons - z[17]) * if cons > z[17] { fast } else { slow };
        let mut cont = 0f32;
        if from_car {
            cont = cont.max((z[16] * 4.0).min(1.0));
        }
        if from_cons {
            cont = cont.max((z[17] * 8.0).min(1.0));
        }
        // straight-through parts go to the output now, while the buffers still hold the plain sound
        let mt = mod_thru * if to_mod { cont } else { 1.0 };
        let m = st.ms[s];
        let ml = step(&hp, hk, &mut z[0..2], m[0]).2;
        let mr = step(&hp, hk, &mut z[2..4], m[1]).2;
        let cl = step(&lp, lk, &mut z[4..6], c[0]).0;
        let cr = step(&lp, lk, &mut z[6..8], c[1]).0;
        st.out[s] = [ml * mt + cl * car_thru, mr * mt + cr * car_thru];
        if noise > 0.0 {
            let nt = noise * if to_noise { cont } else { 1.0 };
            let mut w = [0f32; 2];
            for (i, x) in w.iter_mut().enumerate() {
                st.rng ^= st.rng << 13;
                st.rng ^= st.rng >> 17;
                st.rng ^= st.rng << 5;
                let v = (st.rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
                *x = step(&nh, nk, &mut z[8 + i * 2..10 + i * 2], v).2;
            }
            st.c[s] = [c[0] + w[0] * nt, c[1] + w[1] * nt];
        }
    }
    for v in z.iter_mut() {
        if !v.is_finite() || v.abs() < 1e-20 {
            *v = 0.0;
        }
    }

    let wet = p[WET] / 1000.0;
    let rms = p[RMS] >= 0.5;
    let nb = st.bands.len().max(1) as f32;
    // narrow bands carry little of a wide carrier: bring the sum back up
    let makeup = wet * 0.85 * nb.sqrt() * [0.66, 1.0, 4.7, 5.4][st.order.clamp(1, 4) - 1];
    let (order, stages) = (st.order, st.stages);
    let State { bands, m, c, out, lv, curve, .. } = st;
    for band in bands.iter_mut() {
        for f in lv.iter_mut() {
            *f = [0.0; 2];
        }
        let Band { an, att, rel, hold, rms: rc, floor, satmix, .. } = band;
        for a in an.iter_mut() {
            for (s, f) in lv.iter_mut().enumerate() {
                let mut x = m[s];
                for o in 0..order {
                    x = step(&a.a, a.k, &mut a.z[o * 2..o * 2 + 2], x).1 * a.k;
                }
                let mut r = x.abs();
                if rms {
                    a.ms += (x * x * 2.0 - a.ms) * *rc;
                    r = a.ms.max(0.0).sqrt();
                } else if r >= a.held {
                    a.held = r;
                    a.hc = *hold;
                } else if a.hc > 0 {
                    a.hc -= 1;
                    r = a.held;
                } else {
                    a.held = r;
                }
                a.env += (r - a.env) * if r > a.env { *att } else { *rel };
                let mut l = (a.env - *floor).max(0.0);
                if *satmix > 0.0 {
                    let t = l.min(1.0) * 64.0;
                    let i = (t as usize).min(63);
                    let shaped = curve[i] + (curve[i + 1] - curve[i]) * (t - i as f32);
                    l += (shaped - l) * *satmix;
                }
                f[0] += l * a.pl;
                f[1] += l * a.pr;
            }
            for v in a.z.iter_mut().chain([&mut a.env, &mut a.ms]) {
                if !v.is_finite() || v.abs() < 1e-20 {
                    *v = 0.0;
                }
            }
        }
        let g = band.gain * makeup;
        let off = band.offset * makeup;
        for s in 0..n {
            for ch in 0..2 {
                let mut x = c[s][ch];
                for o in 0..stages {
                    x = step(&band.a, band.k, &mut band.z[ch][o * 2..o * 2 + 2], x).1 * band.k;
                }
                out[s][ch] += x * (lv[s][ch] * g + off);
            }
        }
        for v in band.z.iter_mut().flatten() {
            if !v.is_finite() || v.abs() < 1e-20 {
                *v = 0.0;
            }
        }
    }
    // the finishing stage: push the level up into a soft ceiling
    let fin = (p[FINISH] / 1000.0).clamp(0.0, 1.0);
    for (f, o) in io.iter_mut().zip(out.iter()) {
        for ch in 0..2 {
            let x = o[ch];
            f[ch] = if fin > 0.0 { x + ((x * (1.0 + fin * 3.0)).tanh() - x) * fin.sqrt() } else { x };
        }
    }
}

/// The older vocoder (format identifier "Fruity Vocoder"): the left channel opens and closes bands of the right.
/// Settings: band count, filter order, one number we do not read, hold, twelve whole numbers, then a level per
/// band (room for 128).
pub mod classic {
    use super::{coefs, step};
    use crate::fx::Ctx;

    pub const BANDS: usize = 0;
    pub const ORDER: usize = 1;
    pub const HOLD: usize = 3;
    pub const INTS: usize = 4;
    pub const MIN: usize = INTS;
    pub const MAX: usize = INTS + 1;
    pub const SCALE: usize = INTS + 2;
    pub const INVERT: usize = INTS + 3;
    pub const FORMANT: usize = INTS + 4;
    pub const WIDTH: usize = INTS + 5;
    pub const ATTACK: usize = INTS + 6;
    pub const DECAY: usize = INTS + 7;
    pub const MOD: usize = INTS + 10;
    pub const CAR: usize = INTS + 11;
    pub const GAINS: usize = 16;
    pub const MOST: usize = 128;
    const DEF: [i32; 12] = [0, 65536, 64, 0, 0, 50, 20, 100, 0, 0, 0, 128];

    pub fn decode(s: &[u8]) -> Option<Vec<f32>> {
        let int = |o: usize| s.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
        let n = int(4)? as usize;
        if int(0)? != 2 || !(1..=MOST).contains(&n) || s.len() != 65 + n * 4 {
            return None;
        }
        let mut p = vec![1f32; GAINS + MOST];
        p[BANDS] = n as f32;
        p[ORDER] = int(8)? as f32;
        p[2] = int(12)? as f32;
        p[HOLD] = s[16] as f32;
        for b in 0..n {
            p[GAINS + b] = f32::from_le_bytes(s[17 + b * 4..21 + b * 4].try_into().unwrap());
        }
        for i in 0..12 {
            p[INTS + i] = int(17 + n * 4 + i * 4)? as f32;
        }
        Some(p)
    }

    pub fn encode(p: &[f32]) -> Vec<u8> {
        let n = (p[BANDS] as usize).clamp(1, MOST);
        let mut out = Vec::with_capacity(65 + n * 4);
        for v in [2, n as i32, p[ORDER] as i32, p[2] as i32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.push(p[HOLD] as u8);
        for b in 0..n {
            out.extend_from_slice(&p[GAINS + b].to_le_bytes());
        }
        for i in 0..12 {
            out.extend_from_slice(&(p[INTS + i].round() as i32).to_le_bytes());
        }
        out
    }

    pub fn fresh() -> Vec<u8> {
        let mut p = vec![1f32; GAINS + MOST];
        p[BANDS] = 16.0;
        p[ORDER] = 2.0;
        p[2] = 2.0;
        p[HOLD] = 0.0;
        for (i, v) in DEF.iter().enumerate() {
            p[INTS + i] = *v as f32;
        }
        encode(&p)
    }

    #[derive(Default, Clone)]
    struct Band {
        an: [f32; 3],
        ak: f32,
        az: [f32; 8],
        sy: [f32; 3],
        sk: f32,
        sz: [f32; 4],
        env: f32,
    }

    #[derive(Default)]
    pub struct State {
        seen: Vec<f32>,
        sr: f32,
        bands: Vec<Band>,
        out: Vec<f32>,
    }

    fn place(p: &[f32], t: f32) -> f32 {
        let lo = 20.0 * 1000f32.powf((p[MIN] / 65536.0).clamp(0.0, 1.0));
        let hi = (20.0 * 1000f32.powf((p[MAX] / 65536.0).clamp(0.0, 1.0))).max(lo * 1.1);
        let log = lo * (hi / lo).powf(t);
        let lin = lo + (hi - lo) * t;
        let k = (p[SCALE] / 128.0).clamp(0.0, 1.0);
        // the middle of the knob sits near the logarithmic end, where voices spread best
        lin + (log - lin) * (k * 2.0).min(1.0) * (1.0 - (k - 0.5).max(0.0) * 0.5)
    }

    fn build(p: &[f32], st: &mut State, sr: f32) {
        let n = (p[BANDS] as usize).clamp(1, MOST);
        let old = std::mem::take(&mut st.bands);
        let keep = old.len() == n;
        let width = (p[WIDTH] / 50.0).clamp(0.1, 6.0);
        let shift = 2f32.powf(p[FORMANT].clamp(-2400.0, 2400.0) / 1200.0);
        let hz: Vec<f32> = (0..n).map(|b| place(p, (b as f32 + 0.5) / n as f32).clamp(20.0, sr * 0.45)).collect();
        st.bands = (0..n)
            .map(|b| {
                let mut band = if keep { old[b].clone() } else { Band::default() };
                let lo = place(p, b as f32 / n as f32);
                let hi = place(p, (b + 1) as f32 / n as f32);
                let oct = ((hi / lo).max(1.0001).log2() * width).clamp(0.01, 4.0);
                let q = 1.0 / (2f32.powf(oct * 0.5) - 2f32.powf(-oct * 0.5));
                let from = if p[INVERT] >= 0.5 { hz[n - 1 - b] } else { hz[b] };
                (band.an, band.ak) = coefs((from / shift).clamp(20.0, sr * 0.45), q, sr);
                (band.sy, band.sk) = coefs(hz[b], q, sr);
                band
            })
            .collect();
        st.sr = sr;
    }

    pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
        if p.len() < GAINS + MOST {
            return;
        }
        let sr = cx.sr;
        if st.seen != p || st.sr != sr {
            build(p, st, sr);
            st.seen.clear();
            st.seen.extend_from_slice(p);
        }
        let n = io.len();
        st.out.clear();
        st.out.resize(n, 0.0);
        let order = (p[ORDER] as usize).clamp(1, 4);
        let stages = if order > 2 { 2 } else { 1 };
        let att = 1.0 - (-1.0 / ((p[ATTACK].max(0.0) * 0.0001 + 0.0002) * sr)).exp();
        let dec = 1.0 - (-1.0 / ((p[DECAY].max(0.0) * 0.001 + 0.001) * sr)).exp();
        let hold = p[HOLD] >= 0.5;
        let makeup = 0.85 * (st.bands.len() as f32).sqrt() * [0.66, 1.0, 4.7, 5.4][order - 1];
        for (b, band) in st.bands.iter_mut().enumerate() {
            let g = p[GAINS + b].clamp(0.0, 4.0) * makeup;
            for (s, f) in io.iter().enumerate() {
                if !hold {
                    let mut x = f[0];
                    for o in 0..order {
                        x = step(&band.an, band.ak, &mut band.az[o * 2..o * 2 + 2], x).1 * band.ak;
                    }
                    let r = x.abs();
                    band.env += (r - band.env) * if r > band.env { att } else { dec };
                }
                let mut y = f[1];
                for o in 0..stages {
                    y = step(&band.sy, band.sk, &mut band.sz[o * 2..o * 2 + 2], y).1 * band.sk;
                }
                st.out[s] += y * band.env * g;
            }
            for v in band.az.iter_mut().chain(band.sz.iter_mut()).chain([&mut band.env]) {
                if !v.is_finite() || v.abs() < 1e-20 {
                    *v = 0.0;
                }
            }
        }
        let (m, c) = (p[MOD] / 128.0, p[CAR] / 128.0);
        for (f, o) in io.iter_mut().zip(&st.out) {
            let y = f[0] * m + o * c;
            *f = [y, y];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_block_reads_back_and_keeps_what_it_does_not_read() {
        let b = fresh();
        let p = decode(&b).unwrap();
        assert_eq!(encode(&p, Some(&b)), b);
        assert_eq!(p[BANDS], 47.0);
        assert_eq!(selectors(&p), [1, 0]);
        // an older block: one setting fewer, four switch bytes, shape bytes on a point
        let raw = parse(&b).unwrap();
        let mut old = vec![2, 0, 0, 0];
        for (i, e) in raw.envs.iter().enumerate() {
            old.extend_from_slice(&3u32.to_le_bytes());
            old.extend_from_slice(&2u32.to_le_bytes());
            old.extend_from_slice(&2u32.to_le_bytes());
            let mut pts = e.points.to_vec();
            if i == 4 {
                pts[20] = 7;
            }
            old.extend_from_slice(&pts);
            old.extend_from_slice(e.tail);
        }
        for (i, v) in raw.ints.iter().enumerate() {
            if i != NOISE_HP {
                old.extend_from_slice(&v.to_le_bytes());
            }
        }
        old.extend_from_slice(&[1, 1, 0, 1, 3, b'X', b'y', b'z', 2, 0, 0, 0, 60, 0, 0, 0, 67, 0, 0, 0]);
        let mut p = decode(&old).unwrap();
        assert_eq!(p[FLAGS + F_LR], 1.0);
        assert_eq!(p[NOTES], 2.0);
        assert_eq!(encode(&p, Some(&old)), old);
        p[BANDS] = 12.0;
        let again = decode(&encode(&p, Some(&old))).unwrap();
        assert_eq!(again[BANDS], 12.0);
        assert_eq!(again.len(), p.len());
    }

    /// With CEREAL_STATE_DIR set to a folder of stored blocks of this unit: each reads and writes back the same.
    #[test]
    fn stored_blocks_write_back_the_same() {
        let Ok(dir) = std::env::var("CEREAL_STATE_DIR") else { return };
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let b = std::fs::read(e.path()).unwrap();
            let p = decode(&b).unwrap_or_else(|| panic!("{:?} does not read", e.path()));
            assert!(encode(&p, Some(&b)) == b, "{:?} differs", e.path());
            n += 1;
        }
        assert!(n > 0);
    }

    #[test]
    fn the_modulator_shapes_the_carrier() {
        let cx = Ctx { sr: 44_100.0, step: 5512.0, clock: Default::default() };
        let mut p = decode(&fresh()).unwrap();
        p[MOD_THRU] = 0.0;
        p[FINISH] = 0.0;
        let mut st = State::default();
        let n = 512;
        let run = |st: &mut State, p: &[f32], mod_hz: f32, blocks: usize| -> (f32, f32) {
            let (mut lo, mut hi) = (0f32, 0f32);
            let mut zl = [0f32; 4];
            let mut t = 0usize;
            let mut seed = 12345u32;
            for b in 0..blocks {
                let mut io = vec![[0f32; 2]; n];
                let mut side = vec![[0f32; 2]; n];
                for s in 0..n {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let w = (seed >> 8) as f32 / 8388608.0 - 1.0;
                    io[s] = [w * 0.3; 2];
                    let m = (t as f32 * mod_hz * std::f32::consts::TAU / 44_100.0).sin() * 0.5;
                    side[s] = [m; 2];
                    t += 1;
                }
                st.side[0] = side;
                st.side_on[0] = true;
                process(p, st, &mut io, &cx);
                if b >= blocks / 2 {
                    let (a, k) = coefs(1000.0, 0.707, 44_100.0);
                    for f in &io {
                        assert!(f[0].is_finite() && f[0].abs() < 8.0);
                        let (l, _, h) = step(&a, k, &mut zl[0..2], f[0]);
                        lo += l * l;
                        hi += h * h;
                    }
                }
            }
            (lo.sqrt(), hi.sqrt())
        };
        // a low modulator opens the low bands of the noise, a high one the high bands
        let (lo1, hi1) = run(&mut st, &p, 200.0, 40);
        let mut st2 = State::default();
        let (lo2, hi2) = run(&mut st2, &p, 6000.0, 40);
        assert!(lo1 > hi1 * 2.0, "{lo1} {hi1}");
        assert!(hi2 > lo2 * 2.0, "{lo2} {hi2}");
        // no modulator, nothing out
        let mut st3 = State::default();
        let mut io = vec![[0.2f32, -0.2]; n];
        for _ in 0..40 {
            io = (0..n).map(|s| [((s * 37 % 101) as f32 / 50.0 - 1.0) * 0.3; 2]).collect();
            process(&p, &mut st3, &mut io, &cx);
        }
        assert!(io.iter().all(|f| f[0].abs() < 1e-4));
        // the built-in carrier sounds when nothing else is picked
        p[CAR_SEL] = -1.0;
        let mut st4 = State::default();
        let (lo4, hi4) = run(&mut st4, &p, 300.0, 40);
        assert!(lo4 + hi4 > 0.01, "{lo4} {hi4}");
    }
    #[test]
    fn the_classic_unit_reads_back_and_gates_the_right_channel() {
        if let Ok(dir) = std::env::var("CEREAL_STATE_DIR_CLASSIC") {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let b = std::fs::read(e.path()).unwrap();
                assert!(classic::encode(&classic::decode(&b).unwrap()) == b, "{:?} differs", e.path());
            }
        }
        let b = classic::fresh();
        let p = classic::decode(&b).unwrap();
        assert_eq!(classic::encode(&p), b);
        assert_eq!(b.len(), 129);
        let cx = Ctx { sr: 44_100.0, step: 5512.0, clock: Default::default() };
        let mut st = classic::State::default();
        let mut seed = 7u32;
        let mut level = |on: bool, st: &mut classic::State| {
            let mut e = 0f32;
            for _ in 0..40 {
                let mut io: Vec<[f32; 2]> = (0..512)
                    .map(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let a = (seed >> 8) as f32 / 8388608.0 - 1.0;
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let c = (seed >> 8) as f32 / 8388608.0 - 1.0;
                        [if on { a * 0.4 } else { 0.0 }, c * 0.3]
                    })
                    .collect();
                classic::process(&p, st, &mut io, &cx);
                e = io.iter().map(|f| f[0] * f[0]).sum::<f32>().sqrt();
                assert!(io.iter().all(|f| f[0].is_finite() && f[0] == f[1]));
            }
            e
        };
        let open = level(true, &mut st);
        let shut = level(false, &mut st);
        assert!(open > 0.5 && shut < open * 0.05, "{open} {shut}");
    }

    #[test]
    fn a_drawn_line_goes_into_the_block_and_comes_back() {
        let mut p = decode(&fresh()).unwrap();
        let pts = [[0.0, 0.2], [0.25, 0.9], [1.0, 0.4]];
        assert!(set_env_points(&mut p, 3, &pts));
        assert_eq!(env_points(&p, 3).unwrap(), pts);
        assert!((env_level(&p, 3, 0.125) - 0.55).abs() < 1e-5);
        // the lines after it are still where they were
        assert_eq!(env_points(&p, 12).unwrap(), [[0.0, 0.0], [1.0, 1.0]]);
        let block = encode(&p, Some(&fresh()));
        let q = decode(&block).unwrap();
        assert_eq!(env_points(&q, 3).unwrap(), pts);
        assert_eq!(q, p);
    }
}
