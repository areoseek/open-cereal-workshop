//! The small built-in instruments: each is a short voice of its own that reads the settings of `synth.rs`.
//! Scales are estimates unless a comment says how they were found (docs/PARITY.md, "Instruments").

use crate::synth::Kind;
use std::f32::consts::TAU;

/// The settings a voice reads, in the order of the instrument's table.
#[derive(Debug, Clone)]
pub struct Patch {
    pub kind: Kind,
    pub p: Vec<f32>,
    /// The sample set of the sampled keys.
    pub set: Option<std::sync::Arc<crate::keys::KeySet>>,
    /// The picture of the picture instrument.
    pub img: Option<std::sync::Arc<crate::beep::Image>>,
    /// The sample and settings of the grain player.
    pub smp: Option<(std::sync::Arc<crate::sample::Sample>, crate::grain::Set)>,
    /// The sample and phrases of the path scratcher.
    pub path: Option<(std::sync::Arc<crate::sample::Sample>, std::sync::Arc<crate::travel::Set>)>,
}

static TEMPO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The player notes the tempo here for voices whose slow parts keep step with the song.
pub fn set_tempo(bpm: f32) {
    TEMPO.store(bpm.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub fn tempo() -> f32 {
    let t = f32::from_bits(TEMPO.load(std::sync::atomic::Ordering::Relaxed));
    if t > 1.0 { t } else { 120.0 }
}

/// Cycles per second of key 60.
const C5: f32 = 261.6256;

#[derive(Debug, Clone)]
pub enum Voice {
    Kick { sr: f32, ph: f64, hz: f64, fall: f64, end: f64, level: f64, fade: f64, vel: f32 },
    Fm2(Box<Fm2>),
    Pluck(Box<Pluck>),
    Keys(Box<crate::keys::Voice>),
    Bank(Box<crate::drumbank::Voice>),
    Kit(Box<crate::drumkit::Voice>),
    Image(Box<crate::beep::Voice>),
    Grain(Box<crate::grain::Voice>),
    Travel(Box<crate::travel::Voice>),
    String(Box<Ks>),
    Guitar(Box<crate::guitar::Voice>),
    Analog(Box<(crate::analog::Voice, crate::analog::Set)>),
    Duo(Box<(crate::duo::Voice, crate::duo::Set)>),
    Str(Box<(crate::strmodel::Voice, crate::strmodel::Set)>),
    Triple(Box<(crate::triple::Voice, crate::triple::Set)>),
    Hybrid(Box<(crate::hybrid::Voice, crate::hybrid::Set)>),
    KickD(Box<(crate::kickd::Voice, crate::kickd::Set)>),
    Skin(Box<(crate::membrane::Voice, crate::membrane::Set)>),
    Morph(Box<(crate::morph::Voice, crate::morph::Set)>),
    Quiet,
}

/// A carrier bent by two sine modulators, after the public two-operator design this instrument descends from.
#[derive(Debug, Clone, Default)]
pub struct Fm2 {
    car: f32,
    dcar: f32,
    env: f32,
    cenv: f32,
    catt: f32,
    cdec: f32,
    crel: f32,
    /// Per modulator: the two sine states, the turn factor, level now, level aimed at, glide rate, release rate.
    m: [[f32; 7]; 2],
    lfo: [f32; 2],
    dlfo: f32,
    mw: f32,
    k: u32,
    rich: f32,
    thru: f32,
    vib: f32,
    done: bool,
}

/// A plucked bass string as a handful of partials, shaped on what the factory test of the original gave: it
/// sounds two octaves under the key, the partials follow a pluck near the end of the string (one strong lobe, a
/// gap, a weak second lobe), the upper ones die sooner, and a let go note is gone in a few thousandths.
#[derive(Debug, Clone, Default)]
pub struct Pluck {
    /// Phase, phase step, level and what the level is multiplied by each frame, per partial.
    parts: Vec<[f32; 4]>,
    gain: f32,
    fall: f32,
    held: bool,
}

/// The simple string: one loop of noise per side, averaged as it goes round.
#[derive(Debug, Clone, Default)]
pub struct Ks {
    line: [Vec<f32>; 2],
    pos: usize,
    /// How far between two samples the loop's true length falls.
    frac: f32,
    last: [f32; 2],
    keep: f32,
    fade: f32,
    fade_by: f32,
    wide: bool,
    vel: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn s_gate(p: &Patch) -> bool {
    p.kind == Kind::String && p.p.get(3).is_some_and(|&g| g != 0.0)
}

impl Voice {
    pub fn start(p: &Patch, sr: f32, key: u8, vel: f32, dphase: f32, seed: u32) -> Self {
        let s = &p.p;
        if let (Kind::Keys, Some(set)) = (p.kind, &p.set) {
            return Voice::Keys(Box::new(crate::keys::Voice::start(set, s, sr, key, vel, dphase)));
        }
        if let (Kind::Image, Some(img)) = (p.kind, &p.img) {
            return Voice::Image(Box::new(crate::beep::Voice::start(img, sr, vel, dphase)));
        }
        if let (Kind::Travel, Some((smp, set))) = (p.kind, &p.path) {
            // a key nothing was drawn for plays the sample straight through
            let plain;
            let ph = match set.phrase(key) {
                Some(ph) => ph,
                None => {
                    plain = crate::travel::Phrase::default();
                    &plain
                }
            };
            return Voice::Travel(Box::new(crate::travel::Voice::start(smp, ph, sr, vel, tempo())));
        }
        if let (Kind::Grain, Some((smp, set))) = (p.kind, &p.smp) {
            return Voice::Grain(Box::new(crate::grain::Voice::start(smp, *set, sr, vel, dphase, key, tempo())));
        }
        match p.kind {
            Kind::Image | Kind::Quiet | Kind::Grain | Kind::Travel => Voice::Quiet,
            Kind::String => {
                let hz = (dphase * sr).clamp(20.0, sr * 0.4);
                // the averaging step adds half a sample to the loop
                let len = sr / hz - 0.5;
                let n = (len.floor() as usize).max(2);
                let mut seed = seed | 1;
                let colour = (s[1] / 128.0).clamp(0.0, 1.0);
                let c = 0.03 + colour * colour * 0.97;
                let mut fill = || {
                    let mut z = 0.0;
                    let mut v: Vec<f32> = (0..n + 1).map(|_| {
                        z += (rnd(&mut seed) - z) * c;
                        z
                    }).collect();
                    let top = v.iter().fold(1e-6f32, |m, x| m.max(x.abs()));
                    let mean = v.iter().sum::<f32>() / v.len() as f32;
                    v.iter_mut().for_each(|x| *x = (*x - mean) / top);
                    v
                };
                let wide = s[4] != 0.0;
                let a = fill();
                let b = if wide { fill() } else { Vec::new() };
                // seconds to fall sixty decibels
                let t60 = 0.15 + (s[0] / 128.0).powi(2) * 6.0;
                // evened out, every key takes the same time; otherwise the loss is per trip, so high keys die sooner
                let trips = if s[2] != 0.0 { t60 * hz } else { t60 * C5 };
                let keep = 0.001f32.powf(1.0 / trips.max(1.0));
                Voice::String(Box::new(Ks { line: [a, b], pos: 0, frac: len - len.floor(), last: [0.0; 2], keep, fade: 1.0, fade_by: 1.0, wide, vel }))
            }
            Kind::Guitar => Voice::Guitar(Box::new(crate::guitar::Voice::start(s, sr, key, vel, dphase, seed))),
            Kind::Buzz | Kind::BuzzXt | Kind::Pocket | Kind::Trio | Kind::SawSyn | Kind::Poly | Kind::Poly2 | Kind::Acid => {
                let set = match p.kind {
                    Kind::Pocket => crate::analog::pocket(s),
                    Kind::Trio => crate::analog::trio(s),
                    Kind::SawSyn => crate::analog::saw(s),
                    Kind::Poly => crate::analog::poly(s),
                    Kind::Poly2 => crate::analog::poly2(s),
                    Kind::Acid => crate::analog::acid(),
                    _ => crate::analog::buzz(s),
                };
                Voice::Analog(Box::new((crate::analog::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::Duo => {
                let set = crate::duo::set(s);
                Voice::Duo(Box::new((crate::duo::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::Hybrid => {
                let set = crate::hybrid::set(s);
                Voice::Hybrid(Box::new((crate::hybrid::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::Skin => {
                let set = crate::membrane::set(s);
                Voice::Skin(Box::new((crate::membrane::Voice::start(&set, sr, vel, dphase * sr / C5, seed), set)))
            }
            Kind::KickD => {
                let set = crate::kickd::set(s);
                Voice::KickD(Box::new((crate::kickd::Voice::start(&set, sr, vel, dphase * sr / C5), set)))
            }
            Kind::Morph => {
                let set = crate::morph::set(s);
                Voice::Morph(Box::new((crate::morph::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::Triple => {
                let set = crate::triple::set(s);
                Voice::Triple(Box::new((crate::triple::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::StrModel => {
                let set = crate::strmodel::set(s);
                Voice::Str(Box::new((crate::strmodel::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::Kit => Voice::Kit(Box::new(crate::drumkit::Voice::start(s, sr, key, vel, seed))),
            Kind::DrumBank => Voice::Bank(Box::new(crate::drumbank::Voice::start(s, sr, key, vel, seed))),
            Kind::Kick => {
                // (measured) both falls lose a fixed share every frame at 44.1 kHz; the share follows its knob
                // along a curve that divides by 1001 over the knob's travel
                let share = |knob: f32, most: f64, least: f64| {
                    let per = least + (most - least) * (1001f64.powf(1.0 - (knob as f64 / 256.0).clamp(0.0, 1.0)) - 1.0) / 1000.0;
                    (1.0 - per).powf(44100.0 / sr as f64)
                };
                // (measured) the start pitch is two octaves above the middle key and moves half as far as the
                // key does; the end pitch does not follow the key
                let top = 1046.502 * 2f64.powf(s[0] as f64 / 1200.0) * ((dphase * sr / C5) as f64).sqrt();
                Voice::Kick { sr, ph: s[4] as f64 / 256.0, hz: top, fall: share(s[2], 0.01, 0.0003), end: 65.4064 * 2f64.powf(s[1] as f64 / 1200.0), level: 1.0, fade: share(s[3], 0.005, 0.00002), vel }
            }
            Kind::Fm2 => {
                let ifs = 1.0 / sr;
                let u = |k: usize| s[k] / 65535.0;
                let mut v = Fm2::default();
                let note = key as f32;
                let l = (0.057_762_265 * note).exp();
                v.dcar = 8.175_799 * ifs * 2f32.powf(s[21] + 1.0) * l;
                // follow the played pitch (slides, channel tuning) rather than the key number alone
                let want = dphase * 2.0 * 2f32.powf(s[21]) * 2.0;
                if want.is_finite() && want > 0.0 {
                    v.dcar = want * 0.5;
                }
                // (measured: a default note reaches it as 78 of 127)
                let touch = vel * 127.0 * 0.78;
                let lk = l.min(50.0) * (64.0 + u(9) * (touch - 64.0));
                let ratio = |c: f32, f: f32| {
                    let rati = (40.1 * c * c).floor();
                    let ratf = if f < 0.5 {
                        0.2 * f * f
                    } else {
                        match (8.9 * f) as i32 {
                            4 => 0.25,
                            5 => 0.333_333_34,
                            6 => 0.5,
                            7 => 0.707_106_77,
                            _ => 0.75,
                        }
                    };
                    std::f32::consts::FRAC_PI_2 * (rati + ratf)
                };
                for (k, at) in [(0usize, [3usize, 4, 5, 6, 7, 8]), (1, [15, 16, 17, 18, 19, 20])] {
                    let dmod = ratio(u(at[0]), u(at[1])) * v.dcar;
                    v.m[k] = [
                        -dmod.sin(),
                        -(2.0 * dmod).sin(),
                        2.0 * dmod.cos(),
                        0.0002 * u(at[2]) * u(at[2]) * lk,
                        0.0002 * u(at[4]) * u(at[4]) * lk,
                        1.0 - (-ifs * (6.0 - 7.0 * u(at[3])).exp()).exp(),
                        1.0 - (-ifs * (5.0 - 8.0 * u(at[5])).exp()).exp(),
                    ];
                }
                v.catt = 1.0 - (-ifs * (8.0 - 8.0 * u(0)).exp()).exp();
                v.car = 0.0;
                v.cdec = if u(1) > 0.98 { 1.0 } else { (-ifs * (5.0 - 8.0 * u(1)).exp()).exp() };
                v.crel = (-ifs * (5.0 - 5.0 * u(2)).exp()).exp();
                v.rich = 0.5 - 3.0 * u(11) * u(11);
                v.thru = 0.25 * u(12) * u(12);
                v.vib = 0.001 * u(10) * u(10);
                v.dlfo = 628.3 * ifs * 25.0 * u(13) * u(13);
                v.lfo = [0.0, 1.0];
                v.env = (1.5 - u(11)) * 0.0035 * (touch + 10.0);
                Voice::Fm2(Box::new(v))
            }
            _ => {
                // measured a touch sharp of two octaves down
                let hz = (dphase * sr * 0.25 * 1.0046).clamp(4.0, 4000.0);
                let u = |k: usize| s[k] / 32767.0 - 1.0;
                let g = [crate::fx::db(u(0) * 12.0), crate::fx::db(u(1) * 12.0), crate::fx::db(u(2) * 12.0)];
                // higher strings are shorter of breath: the same shape over fewer partials, a little louder
                let squeeze = (1.0 - 0.165 * (hz / 66.0).log2()).clamp(0.67, 1.0);
                let wide = (1.0 - 0.1 * (hz / 66.0).log2()).clamp(0.8, 1.0);
                let mut parts = Vec::new();
                for k in 1..=24 {
                    let f = hz * k as f32;
                    if f > sr * 0.45 {
                        break;
                    }
                    let x = k as f32 / squeeze;
                    let xm = k as f32 / wide;
                    let main = xm.powf(1.2) * (-(xm / 3.0).powi(2)).exp() / (wide * wide);
                    let second = -0.24 / (1.0 + (f / 1500.0).powi(2)).powf(0.9) * (-((x - 11.0) / 1.2).powi(2) / 2.0).exp();
                    // the three tone knobs lean on the low, middle and high partials
                    let (l1, l2) = (1.0 / (1.0 + (f / 220.0).powi(2)).sqrt(), 1.0 / (1.0 + (f / 1400.0).powi(2)).sqrt());
                    let tone = g[0] * l1 + g[1] * (l2 - l1) + g[2] * (1.0 - l2);
                    let a = (main + second) * tone;
                    if a.abs() < 1e-5 {
                        continue;
                    }
                    let db_s = 9.0 * (f / 200.0).max(1.0).powf(1.1);
                    parts.push([0.0, f / sr, a, 10f32.powf(-db_s / 20.0 / sr)]);
                }
                let _ = seed;
                Voice::Pluck(Box::new(Pluck { parts, gain: vel * 0.0678, fall: (-1.0 / (0.0039 * sr)).exp(), held: true }))
            }
        }
    }

    /// The key was let go. Returns the seconds the sound still needs.
    pub fn release(&mut self, p: &Patch) -> f32 {
        match self {
            // (measured) letting go leaves the drum as it is; the channel closes it over 10 ms
            Voice::Kick { .. } => {
                let _ = p;
                0.0
            }
            Voice::Fm2(v) => {
                if !v.done {
                    v.done = true;
                    v.cdec = v.crel;
                    v.env = v.cenv;
                    v.catt = 1.0;
                    for m in &mut v.m {
                        m[4] = 0.0;
                        m[5] = m[6];
                    }
                }
                // until the level is under a thousandth
                (0.001f32.ln() / v.crel.max(1e-9).ln().min(-1e-9)).min(20.0 * 48000.0) / 44100.0
            }
            Voice::Pluck(v) => {
                v.held = false;
                0.03
            }
            Voice::Keys(v) => v.release(),
            Voice::Bank(v) => v.left(),
            Voice::Kit(v) => v.release(),
            Voice::Image(v) => v.release(),
            Voice::Grain(v) => v.release(),
            Voice::Travel(v) => v.release(),
            Voice::Guitar(v) => v.release(),
            Voice::String(v) => {
                if s_gate(p) {
                    v.fade_by = 0.99;
                    0.03
                } else {
                    // the string rings on: the time left is the loop's own decay
                    let per_sec = v.keep.ln() * (44100.0 / v.line[0].len() as f32);
                    (0.001f32.ln() / per_sec.min(-1e-6)).min(8.0)
                }
            }
            Voice::Analog(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Duo(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Triple(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Hybrid(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::KickD(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Skin(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Morph(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Str(v) => {
                let (v, set) = &mut **v;
                v.release(set)
            }
            Voice::Quiet => 0.0,
        }
    }

    pub fn frame(&mut self, p: &Patch, dphase: f32) -> [f32; 2] {
        let s = &p.p;
        match self {
            Voice::Keys(v) => v.frame(dphase),
            Voice::Bank(v) => v.frame(),
            Voice::Kit(v) => v.frame(&p.p),
            Voice::Image(v) => v.frame(),
            Voice::Grain(v) => v.frame(),
            Voice::Travel(v) => v.frame(),
            Voice::Guitar(v) => v.frame(),
            Voice::String(v) => {
                let n = v.line[0].len();
                let (at, next) = (v.pos, (v.pos + 1) % n);
                let mut out = [0.0f32; 2];
                for side in 0..if v.wide { 2 } else { 1 } {
                    let line = &mut v.line[side];
                    // read between two samples so the pitch lands between whole loop lengths
                    let x = line[at] + (line[next] - line[at]) * v.frac;
                    let y = (x + v.last[side]) * 0.5 * v.keep;
                    v.last[side] = x;
                    line[at] = y;
                    out[side] = x;
                }
                if !v.wide {
                    out[1] = out[0];
                } else {
                    // keep the middle solid: the sides differ only in part
                    let mid = (out[0] + out[1]) * 0.5;
                    out = [mid + (out[0] - mid) * 0.7, mid + (out[1] - mid) * 0.7];
                }
                v.pos = next;
                v.fade *= v.fade_by;
                let g = v.fade * v.vel * 0.7;
                [out[0] * g, out[1] * g]
            }
            Voice::Analog(v) => {
                let (v, set) = &mut **v;
                v.frame(set, dphase)
            }
            Voice::Duo(v) => {
                let (v, set) = &mut **v;
                v.frame(set, dphase)
            }
            Voice::Triple(v) => {
                let (v, set) = &mut **v;
                v.frame(set, dphase)
            }
            Voice::Hybrid(v) => {
                let (v, set) = &mut **v;
                v.frame(set, dphase)
            }
            Voice::KickD(v) => {
                let (v, set) = &mut **v;
                v.frame(set)
            }
            Voice::Skin(v) => {
                let (v, set) = &mut **v;
                v.frame(set)
            }
            Voice::Morph(v) => {
                let (v, set) = &mut **v;
                v.frame(set, dphase)
            }
            Voice::Str(v) => {
                let (v, set) = &mut **v;
                v.frame(set, dphase)
            }
            Voice::Quiet => [0.0; 2],
            Voice::Kick { sr, ph, hz, fall, end, level, fade, vel } => {
                // (measured) the first frame is already at full level, at the phase the click knob sets
                let mut x = ((*ph * std::f64::consts::TAU).sin() * *level) as f32;
                *ph += (hz.max(*end) / *sr as f64).min(0.45);
                *ph -= ph.floor();
                *hz *= *fall;
                *level *= *fade;
                // (measured) the drive knob mixes in a bent copy: a straight rise up to a knee, a tenth power
                // above it
                let d = (s[5] / 128.0).clamp(0.0, 1.0);
                if d > 0.0 {
                    let a = x.abs();
                    x = (1.0 - d) * x + d * (1.731_82 * a).min(0.716_18 * a.powf(0.1)).copysign(x);
                }
                // the channel weighs the velocity; nothing of it is used here
                let _ = vel;
                [x * 1.060_66; 2]
            }
            Voice::Fm2(v) => {
                if v.k == 0 {
                    v.k = 100;
                    v.lfo[0] += v.dlfo * v.lfo[1];
                    v.lfo[1] -= v.dlfo * v.lfo[0];
                    v.mw = v.lfo[1] * v.vib;
                }
                v.k -= 1;
                let e = v.env;
                v.env = e * v.cdec;
                v.cenv += v.catt * (e - v.cenv);
                let mut bend = 0.0;
                let mut thru = 0.0;
                for (k, m) in v.m.iter_mut().enumerate() {
                    let x = m[2] * m[0] - m[1];
                    m[1] = m[0];
                    m[0] = x;
                    m[3] += m[5] * (m[4] - m[3]);
                    bend += x * m[3];
                    if k == 0 {
                        thru = m[1];
                    }
                }
                let mut x = v.car + v.dcar + bend + v.mw;
                while x > 1.0 {
                    x -= 2.0;
                }
                while x < -1.0 {
                    x += 2.0;
                }
                v.car = x;
                let r = v.rich;
                let o = v.cenv * (v.thru * thru + (x + x * x * x * (r * x * x - 1.0 - r)));
                [o * 2.111; 2]
            }
            Voice::Pluck(v) => {
                let _ = dphase;
                let mut o = 0.0;
                for p in v.parts.iter_mut() {
                    o += (p[0] * TAU).sin() * p[2];
                    p[0] += p[1];
                    if p[0] >= 1.0 {
                        p[0] -= 1.0;
                    }
                    p[2] *= p[3];
                }
                if !v.held {
                    v.gain *= v.fall;
                }
                [o * v.gain; 2]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{Gen, ToneState};

    fn play(g: &Gen, key: u8, frames: usize) -> Vec<f32> {
        let tone = g.tone();
        let sr = 44100.0;
        let d = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0) / sr;
        let mut st = ToneState::start(&tone, 5);
        st.note(&tone, sr, key, 0.8, d);
        (0..frames).map(|_| st.frame(&tone, d)[0]).collect()
    }

    fn ups(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn kick_falls_in_pitch_and_level() {
        let g = Gen::new(Kind::Kick);
        let x = play(&g, 60, 44100);
        let early = ups(&x[..2205]);
        let late = ups(&x[8820..11025]);
        assert!(early > late, "{early} {late}");
        let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        // (measured) the voice itself peaks a little over one; the channel level brings it down
        assert!(peak > 0.2 && peak <= 1.07, "{peak}");
        assert!(x[40000..].iter().all(|v| v.abs() < 0.03));
        // the first frame is at the click knob's phase, an eighth of a cycle at its middle
        assert!((x[0] / peak - 0.707).abs() < 0.01, "{}", x[0]);
    }

    #[test]
    fn two_operator_voice_sounds_at_pitch_and_fades() {
        let mut g = Gen::new(Kind::Fm2);
        // no modulation: a plain tone at the key's pitch
        g.p[5] = 0.0;
        g.p[7] = 0.0;
        g.p[13] = 0.0;
        let x = play(&g, 69, 44100);
        let n = ups(&x[..22050]);
        assert!((215..=225).contains(&n), "{n}");
        let g = Gen::new(Kind::Fm2);
        let tone = g.tone();
        let mut st = ToneState::start(&tone, 5);
        st.note(&tone, 44100.0, 60, 0.8, 0.006);
        let a: f32 = (0..4000).map(|_| st.frame(&tone, 0.006)[0].abs()).fold(0.0, f32::max);
        assert!(a > 0.05 && a < 1.2, "{a}");
        let tail = st.release(&tone);
        assert!(tail > 0.01 && tail < 30.0, "{tail}");
        let n = (tail * 44100.0) as usize;
        let mut last = 0.0f32;
        for _ in 0..n {
            last = st.frame(&tone, 0.006)[0].abs();
        }
        assert!(last < 0.01, "{last}");
    }

    #[test]
    fn plucked_bass_holds_its_pitch() {
        let g = Gen::new(Kind::Bass);
        let x = play(&g, 45, 44100);
        // two octaves under the key and a touch sharp: the wave repeats every 1596 frames
        let y = &x[4410..22050];
        let score = |lag: usize| (0..y.len() - 1800).map(|i| y[i] * y[i + lag]).sum::<f32>();
        let best = (1400..1800).max_by(|&a, &b| score(a).total_cmp(&score(b))).unwrap();
        assert!((1593..=1599).contains(&best), "{best}");
        let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.02 && peak < 1.2, "{peak}");
    }
}
