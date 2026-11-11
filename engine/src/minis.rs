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
}

/// Cycles per second of key 60.
const C5: f32 = 261.6256;

#[derive(Debug, Clone)]
pub enum Voice {
    Kick { sr: f32, t: u32, ph: f32, scale: f32, vel: f32 },
    Fm2(Box<Fm2>),
    Pluck(Box<Pluck>),
    Keys(Box<crate::keys::Voice>),
    Bank(Box<crate::drumbank::Voice>),
    Image(Box<crate::beep::Voice>),
    Grain(Box<crate::grain::Voice>),
    String(Box<Ks>),
    Analog(Box<(crate::analog::Voice, crate::analog::Set)>),
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

/// A plucked string: a loop of noise that wears down, then three tone bands.
#[derive(Debug, Clone, Default)]
pub struct Pluck {
    line: Vec<f32>,
    pos: usize,
    frac: f32,
    last: f32,
    damp: f32,
    keep: f32,
    z: [f32; 4],
    g: [f32; 3],
    f: [f32; 2],
    vel: f32,
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
        if let (Kind::Grain, Some((smp, set))) = (p.kind, &p.smp) {
            // an empty settings list marks the unit that plays its sample at its own speed whatever the key
            let dphase = if p.p.is_empty() && set.hold == 0.05 { C5 / sr } else { dphase };
            return Voice::Grain(Box::new(crate::grain::Voice::start(smp, *set, sr, vel, dphase)));
        }
        match p.kind {
            Kind::Image | Kind::Quiet | Kind::Grain => Voice::Quiet,
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
            Kind::Buzz | Kind::BuzzXt => {
                let set = crate::analog::buzz(s);
                Voice::Analog(Box::new((crate::analog::Voice::start(&set, sr, key, vel, seed), set)))
            }
            Kind::DrumBank => Voice::Bank(Box::new(crate::drumbank::Voice::start(s, sr, key, vel, seed))),
            Kind::Kick => Voice::Kick { sr, t: 0, ph: s[4] / 64.0 * 0.25, scale: dphase * sr / C5, vel },
            Kind::Fm2 => {
                let ifs = 1.0 / sr;
                let u = |k: usize| s[k] / 65535.0;
                let mut v = Fm2::default();
                let note = key as f32;
                let l = (0.057_762_265 * (note + 2.0 * u(11) - 1.0)).exp();
                v.dcar = 8.175_799 * ifs * 2f32.powf(s[21] + 1.0) * l;
                // follow the played pitch (slides, channel tuning) rather than the key number alone
                let want = dphase * 2.0 * 2f32.powf(s[21]) * 2f32.powf((2.0 * u(11) - 1.0) / 12.0) * 2.0;
                if want.is_finite() && want > 0.0 {
                    v.dcar = want * 0.5;
                }
                let lk = l.min(50.0) * (64.0 + u(9) * (vel * 127.0 - 64.0));
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
                        0.0,
                        dmod.sin(),
                        2.0 * dmod.cos(),
                        0.0002 * u(at[2]) * u(at[2]) * lk,
                        0.0002 * u(at[4]) * u(at[4]) * lk,
                        1.0 - (-ifs * (6.0 - 7.0 * u(at[3])).exp()).exp(),
                        1.0 - (-ifs * (5.0 - 8.0 * u(at[5])).exp()).exp(),
                    ];
                }
                v.catt = 1.0 - (-ifs * (8.0 - 8.0 * u(0)).exp()).exp();
                v.cdec = if u(1) > 0.98 { 1.0 } else { (-ifs * (5.0 - 8.0 * u(1)).exp()).exp() };
                v.crel = (-ifs * (5.0 - 5.0 * u(2)).exp()).exp();
                v.rich = 0.5 - 3.0 * u(12) * u(12);
                v.thru = 0.25 * u(13) * u(13);
                v.vib = 0.001 * u(10) * u(10);
                v.dlfo = 628.3 * ifs * 25.0 * u(14) * u(14);
                v.lfo = [0.0, 1.0];
                v.env = (1.5 - u(12)) * 0.0035 * (vel * 127.0 + 10.0);
                Voice::Fm2(Box::new(v))
            }
            _ => {
                let hz = (dphase * sr).clamp(16.0, 4000.0);
                let n = sr / hz;
                let len = (n.floor() as usize).max(2);
                let mut seed = seed | 1;
                // the pluck: noise with the top taken off, so low notes do not buzz
                let mut line = vec![0.0f32; len];
                let mut lp = 0.0;
                for x in line.iter_mut() {
                    lp += 0.45 * (rnd(&mut seed) - lp);
                    *x = lp;
                }
                let u = |k: usize| s[k] / 32767.0 - 1.0;
                let band = |hz: f32| 1.0 - (-TAU * hz / sr).exp();
                Voice::Pluck(Box::new(Pluck {
                    line,
                    pos: 0,
                    frac: n - len as f32,
                    last: 0.0,
                    damp: 0.5,
                    // a low string rings for a couple of seconds, a high one less
                    keep: 0.001f32.powf(1.0 / (hz * (2.6 - (hz / 400.0).min(1.6)))),
                    z: [0.0; 4],
                    g: [crate::fx::db(u(0) * 12.0), crate::fx::db(u(1) * 12.0), crate::fx::db(u(2) * 12.0)],
                    f: [band(220.0), band(1400.0)],
                    vel,
                    held: true,
                }))
            }
        }
    }

    /// The key was let go. Returns the seconds the sound still needs.
    pub fn release(&mut self, p: &Patch) -> f32 {
        match self {
            Voice::Kick { sr, t, .. } => (kick_amp_s(&p.p) * 7.0 - *t as f32 / *sr).max(0.0),
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
                0.12
            }
            Voice::Keys(v) => v.release(),
            Voice::Bank(v) => v.left(),
            Voice::Image(v) => v.release(),
            Voice::Grain(v) => v.release(),
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
            Voice::Quiet => 0.0,
        }
    }

    pub fn frame(&mut self, p: &Patch, dphase: f32) -> [f32; 2] {
        let s = &p.p;
        match self {
            Voice::Keys(v) => v.frame(dphase),
            Voice::Bank(v) => v.frame(),
            Voice::Image(v) => v.frame(),
            Voice::Grain(v) => v.frame(),
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
            Voice::Quiet => [0.0; 2],
            Voice::Kick { sr, t, ph, scale, vel } => {
                let sec = *t as f32 / *sr;
                *t += 1;
                // the end pitch: key 36's pitch at zero, found from the step the block stores beside it
                let end = 65.406 * 2f32.powf(s[1] / 1200.0);
                let top = 523.25 * 2f32.powf(s[0] / 1200.0);
                let sweep = 0.002 + (s[2] / 256.0).powi(2) * 0.25;
                let hz = (end + (top - end) * (-sec / sweep).exp()) * *scale;
                let x = (*ph * TAU).sin();
                *ph += (hz / *sr).min(0.45);
                *ph -= ph.floor();
                let drive = 1.0 + s[5] / 128.0 * 7.0;
                let x = (x * drive).clamp(-1.0, 1.0);
                let a = (-sec / kick_amp_s(s)).exp() * (1.0 - (-sec * 4000.0).exp()) * 0.7 * *vel;
                [x * a; 2]
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
                [o * 1.6; 2]
            }
            Voice::Pluck(v) => {
                let n = v.line.len();
                let a = v.line[v.pos];
                let b = v.line[(v.pos + 1) % n];
                // the fraction of a frame the loop is short by, and the wear of each trip
                let y = a + (b - a) * v.frac;
                let worn = (y * (1.0 - v.damp * 0.5) + v.last * v.damp * 0.5) * if v.held { v.keep } else { v.keep * 0.995 };
                v.last = y;
                v.line[v.pos] = worn;
                v.pos = (v.pos + 1) % n;
                let _ = dphase;
                v.z[0] += v.f[0] * (y - v.z[0]);
                v.z[1] += v.f[1] * (y - v.z[1]);
                let lo = v.z[0];
                let mid = v.z[1] - v.z[0];
                let hi = y - v.z[1];
                let o = (lo * v.g[0] * 1.6 + mid * v.g[1] + hi * v.g[2] * 0.6) * v.vel * 1.4;
                [o; 2]
            }
        }
    }
}

/// Seconds for the drum's level to fall to about a third.
fn kick_amp_s(s: &[f32]) -> f32 {
    0.015 + (s[3] / 256.0).powi(2) * 0.9
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
        assert!(peak > 0.2 && peak <= 1.0, "{peak}");
        assert!(x[40000..].iter().all(|v| v.abs() < 0.03));
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
        // the wave repeats every 401 frames at 110 cycles a second
        let y = &x[4410..22050];
        let score = |lag: usize| (0..y.len() - 500).map(|i| y[i] * y[i + lag]).sum::<f32>();
        let best = (300..500).max_by(|&a, &b| score(a).total_cmp(&score(b))).unwrap();
        assert!((399..=403).contains(&best), "{best}");
        let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.1 && peak < 1.2, "{peak}");
    }
}
