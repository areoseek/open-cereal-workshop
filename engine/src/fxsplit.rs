//! Attack Splitter: the sound is divided into its attacks and what rings on after them by a gate that a
//! burst detector opens; the two parts are complementary, so with nothing else done they add up to the
//! input again. Each part has its own level, tone tilt and one of four effects with a dry/wet mix.
//!
//! The stored block is a version number and 47 whole numbers; which one is which was worked out from the
//! presets' names and values, the laws (times, gains) are our own estimates.

use crate::fx::Ctx;

// places in the settings list (the block's slot number less one)
pub const SENS: usize = 0;
pub const HOLD: usize = 1;
pub const RELEASE: usize = 2;
pub const LEVEL: usize = 3; // attack part, then the rest
pub const TILT: usize = 5;
pub const DIFF: usize = 7; // mix, amount, length; twice
pub const SAT: usize = 13; // mix, drive, warmth; twice
pub const BOUNCE: usize = 19; // mix, time curve, level curve; twice
pub const ECHO: usize = 25; // mix, time, tempo time, feedback; twice
pub const LINK_LEVELS: usize = 33;
pub const LINK_SIDES: usize = 34;
pub const MODE: usize = 35;
pub const ON: usize = 36; // attack part, then the rest
pub const EFFECT: usize = 38; // attack part, then the rest
pub const SYNC: usize = 40; // attack part, then the rest
pub const AHEAD: usize = 42;

pub const EFFECTS: [&str; 6] = ["Saturation", "Bounce", "Diffusion", "Off", "Echo", "Off"];
pub const MODES: [&str; 3] = ["Smooth", "Balanced", "Dense"];

const TAPS: usize = 10;
const STAGES: [f32; 6] = [1.3, 2.9, 4.7, 7.1, 11.3, 17.9];

/// What one part carries between calls.
struct Part {
    tilt: [f32; 2],
    warm: [f32; 2],
    ring: Vec<[f32; 2]>,
    at: usize,
    echo: Vec<[f32; 2]>,
    echo_at: usize,
    pass: Vec<Vec<f32>>,
    pass_at: [usize; 12],
}

impl Part {
    fn new() -> Self {
        Self { tilt: [0.0; 2], warm: [0.0; 2], ring: Vec::new(), at: 0, echo: Vec::new(), echo_at: 0, pass: Vec::new(), pass_at: [0; 12] }
    }
}

pub struct State {
    fast: [f32; 2],
    slow: [f32; 2],
    gate: [f32; 2],
    hold: [u32; 2],
    ahead: Vec<[f32; 2]>,
    ahead_at: usize,
    parts: [Part; 2],
    sr: f32,
    /// The gate as it stands, for controls linked to it.
    pub env: f32,
}

impl Default for State {
    fn default() -> Self {
        Self { fast: [0.0; 2], slow: [0.0; 2], gate: [0.0; 2], hold: [0; 2], ahead: Vec::new(), ahead_at: 0, parts: [Part::new(), Part::new()], sr: 0.0, env: 0.0 }
    }
}

fn unit(p: &[f32], k: usize) -> f32 {
    (p.get(k).copied().unwrap_or(0.0) / 10000.0).clamp(0.0, 1.0)
}

/// The time the gate stays open after a burst, in seconds.
pub fn hold_time(v: f32) -> f32 {
    v * v
}

/// The time the gate takes to close, in seconds.
pub fn release_time(v: f32) -> f32 {
    0.002 + 2.0 * v * v
}

/// A part's level as a gain; the stored default stands for no change.
pub fn level(v: f32) -> f32 {
    (v / 0.8743).powi(4)
}

/// An echo's free time in seconds.
pub fn echo_time(v: f32) -> f32 {
    0.001 + 2.0 * v * v
}

fn bent(u: f32, c: f32) -> f32 {
    let a = 5.0 * c;
    if a.abs() < 1e-3 {
        u
    } else {
        (1.0 - (-a * u).exp()) / (1.0 - (-a).exp())
    }
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    let sr = cx.sr;
    if st.sr != sr {
        *st = State { sr, ..Default::default() };
        st.ahead = vec![[0.0; 2]; (sr * 0.005) as usize + 1];
        for part in &mut st.parts {
            part.ring = vec![[0.0; 2]; (sr * 0.65) as usize + 8];
            part.echo = vec![[0.0; 2]; (sr * 4.0) as usize + 8];
            part.pass = (0..12).map(|i| vec![0.0; (STAGES[i % 6] * 3.5 * 0.001 * sr * if i < 6 { 1.0 } else { 1.07 }) as usize + 8]).collect();
        }
    }
    let ms = |t: f32| 1.0 - (-1.0 / (t * 0.001 * sr).max(1.0)).exp();
    let mode = p.get(MODE).copied().unwrap_or(1.0) as usize;
    // how long the slow follower takes to catch up decides what counts as a burst
    let catch_up = ms([40.0, 20.0, 8.0][mode.min(2)]);
    let (fast_up, fast_down, slow_down) = (ms(0.1), ms(5.0), ms(60.0));
    let s = unit(p, SENS);
    let over = 1.1 + 5.0 * (1.0 - s) * (1.0 - s);
    let hold = (hold_time(unit(p, HOLD)) * sr) as u32;
    let (open, close) = (ms(0.5), ms(release_time(unit(p, RELEASE)) * 1000.0 / 3.0));
    let linked = p.get(LINK_SIDES).copied().unwrap_or(1.0) >= 0.5;
    let ahead = p.get(AHEAD).copied().unwrap_or(0.0) >= 0.5;
    let tilt_a = 1.0 - (-std::f32::consts::TAU * 700.0 / sr).exp();

    struct Set {
        on: bool,
        gain: f32,
        lo: f32,
        hi: f32,
        effect: usize,
        mix: f32,
        a: f32,
        b: f32,
        taps: [(usize, f32); TAPS],
        lens: [usize; 12],
        time: usize,
    }
    let set = |k: usize| -> Set {
        let on = p.get(ON + k).copied().unwrap_or(1.0) >= 0.5;
        let t = (unit(p, TILT + k) - 0.5) * 2.0;
        let db = |x: f32| 10f32.powf(x / 20.0);
        let effect = p.get(EFFECT + k).copied().unwrap_or(5.0) as usize;
        let mut q = Set { on, gain: level(unit(p, LEVEL + k)), lo: db(-6.0 * t), hi: db(6.0 * t), effect, mix: 0.0, a: 0.0, b: 0.0, taps: [(1, 0.0); TAPS], lens: [1; 12], time: 1 };
        match effect {
            0 => {
                q.mix = unit(p, SAT + 3 * k);
                // a third of the way up is no gain
                q.a = db(-12.0 + 36.0 * unit(p, SAT + 3 * k + 1));
                q.b = 1.0 - (-std::f32::consts::TAU * (16000.0 * 0.15f32.powf(unit(p, SAT + 3 * k + 2))) / sr).exp();
            }
            1 => {
                q.mix = unit(p, BOUNCE + 3 * k);
                let (tc, ac) = ((unit(p, BOUNCE + 3 * k + 1) - 0.5) * 2.0, (unit(p, BOUNCE + 3 * k + 2) - 0.5) * 2.0);
                for (i, tap) in q.taps.iter_mut().enumerate() {
                    let u = (i + 1) as f32 / TAPS as f32;
                    let amp = if ac >= 0.0 { (-5.0 * ac * (1.0 - u)).exp() } else { (5.0 * ac * u).exp() };
                    *tap = (((bent(u, tc) * 0.6 * sr) as usize).max(1), amp * 0.7);
                }
            }
            2 => {
                q.mix = unit(p, DIFF + 3 * k);
                q.a = 0.75 * unit(p, DIFF + 3 * k + 1);
                let scale = 0.2 + 3.0 * unit(p, DIFF + 3 * k + 2);
                for (i, l) in q.lens.iter_mut().enumerate() {
                    *l = ((STAGES[i % 6] * scale * 0.001 * sr * if i < 6 { 1.0 } else { 1.07 }) as usize).max(1);
                }
            }
            4 => {
                q.mix = unit(p, ECHO + 4 * k);
                let synced = p.get(SYNC + k).copied().unwrap_or(0.0) >= 0.5;
                let frames = if synced { p.get(ECHO + 4 * k + 2).copied().unwrap_or(48.0) / 48.0 * cx.step } else { echo_time(unit(p, ECHO + 4 * k + 1)) * sr };
                q.time = (frames as usize).max(1);
                q.b = unit(p, ECHO + 4 * k + 3) * 0.95;
            }
            _ => {}
        }
        q
    };
    let sets = [set(0), set(1)];

    for f in io.iter_mut() {
        // the detector hears the sound as it arrives
        let heard = if linked {
            let m = f[0].abs().max(f[1].abs());
            [m, m]
        } else {
            [f[0].abs(), f[1].abs()]
        };
        for c in 0..2 {
            let d = heard[c];
            st.fast[c] += (d - st.fast[c]) * if d > st.fast[c] { fast_up } else { fast_down };
            st.slow[c] += (d - st.slow[c]) * if d > st.slow[c] { catch_up } else { slow_down };
            if st.fast[c] > st.slow[c] * over + 1e-4 {
                st.hold[c] = hold.max(1);
            }
            if st.hold[c] > 0 {
                st.hold[c] -= 1;
                st.gate[c] += (1.0 - st.gate[c]) * open;
            } else {
                st.gate[c] -= st.gate[c] * close;
            }
        }
        st.env = st.gate[0].max(st.gate[1]);
        // looking ahead, the sound itself runs 5 ms late so the gate is open before the burst
        let x = if ahead && !st.ahead.is_empty() {
            let n = st.ahead.len();
            let old = st.ahead[st.ahead_at];
            st.ahead[st.ahead_at] = *f;
            st.ahead_at = (st.ahead_at + 1) % n;
            old
        } else {
            *f
        };
        let mut sum = [0.0f32; 2];
        for (k, (part, q)) in st.parts.iter_mut().zip(sets.iter()).enumerate() {
            let dry = if k == 0 { [x[0] * st.gate[0], x[1] * st.gate[1]] } else { [x[0] * (1.0 - st.gate[0]), x[1] * (1.0 - st.gate[1])] };
            let mut y = dry;
            match q.effect {
                0 => {
                    // quiet sound passes at its own level below no gain, and at part of the gain above it
                    let norm = if q.a > 1.0 { q.a.sqrt() } else { q.a.max(0.25) };
                    for c in 0..2 {
                        let added = (dry[c] * q.a).tanh() / norm - dry[c];
                        // warmth takes the edge off what the curve added, not off the sound itself
                        part.warm[c] += q.b * (added - part.warm[c]);
                        y[c] = dry[c] + part.warm[c] * q.mix;
                    }
                }
                1 => {
                    let n = part.ring.len();
                    part.ring[part.at] = dry;
                    let mut wet = [0.0f32; 2];
                    for &(d, amp) in &q.taps {
                        let v = part.ring[(part.at + n - d.min(n - 1)) % n];
                        wet[0] += v[0] * amp;
                        wet[1] += v[1] * amp;
                    }
                    part.at = (part.at + 1) % n;
                    for c in 0..2 {
                        y[c] = dry[c] * (1.0 - q.mix) + wet[c] * q.mix;
                    }
                }
                2 => {
                    let mut wet = dry;
                    for c in 0..2 {
                        for i in 0..6 {
                            let j = c * 6 + i;
                            let line = &mut part.pass[j];
                            let n = q.lens[j].min(line.len());
                            let at = part.pass_at[j] % n;
                            let held = line[at];
                            let out = held - q.a * wet[c];
                            line[at] = wet[c] + q.a * out;
                            part.pass_at[j] = (at + 1) % n;
                            wet[c] = out;
                        }
                        y[c] = dry[c] + (wet[c] - dry[c]) * q.mix;
                    }
                }
                4 => {
                    let n = part.echo.len();
                    let back = part.echo[(part.echo_at + n - q.time.min(n - 1)) % n];
                    part.echo[part.echo_at] = [dry[0] + back[0] * q.b, dry[1] + back[1] * q.b];
                    part.echo_at = (part.echo_at + 1) % n;
                    for c in 0..2 {
                        y[c] = dry[c] * (1.0 - q.mix) + back[c] * q.mix;
                    }
                }
                _ => {}
            }
            for c in 0..2 {
                part.tilt[c] += tilt_a * (y[c] - part.tilt[c]);
                let (lo, hi) = (part.tilt[c], y[c] - part.tilt[c]);
                if q.on {
                    sum[c] += (lo * q.lo + hi * q.hi) * q.gain;
                }
            }
        }
        *f = sum;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{Fx, Kind, Unit};

    /// A tone that is struck again every quarter second and dies away in between.
    fn hits(i: usize) -> [f32; 2] {
        let t = (i % 12_000) as f32 / 48_000.0;
        let v = (t * 220.0 * std::f32::consts::TAU).sin() * (-t * 14.0).exp() * 0.8;
        [v, v]
    }

    fn run(fx: Fx, n: usize) -> Vec<[f32; 2]> {
        let cx = Ctx { sr: 48_000.0, step: 6000.0, clock: Default::default() };
        let mut u = Unit::new(fx, 48_000.0);
        let mut io: Vec<[f32; 2]> = (0..n).map(hits).collect();
        for block in io.chunks_mut(512) {
            u.process(block, &cx);
        }
        io
    }

    fn plain() -> Fx {
        let mut f = Fx::new(Kind::Split);
        f.p[EFFECT] = 5.0;
        f.p[EFFECT + 1] = 5.0;
        f
    }

    #[test]
    fn the_two_parts_add_up_to_the_input() {
        let out = run(plain(), 48_000);
        let worst = out.iter().enumerate().map(|(i, f)| (f[0] - hits(i)[0]).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn the_attack_part_has_the_hits_and_the_body_the_rest() {
        let energy = |v: &[[f32; 2]], from: usize, to: usize| -> f32 { (24_000..48_000).filter(|i| (from..to).contains(&(i % 12_000))).map(|i| v[i][0] * v[i][0]).sum() };
        let mut attack = plain();
        attack.p[ON + 1] = 0.0;
        let mut body = plain();
        body.p[ON] = 0.0;
        let (a, b) = (run(attack, 48_000), run(body, 48_000));
        // the first 20 ms after a hit against the stretch from 150 ms on
        assert!(energy(&a, 0, 960) > 20.0 * energy(&a, 7200, 12_000), "{} {}", energy(&a, 0, 960), energy(&a, 7200, 12_000));
        assert!(energy(&b, 0, 960) < 0.2 * energy(&a, 0, 960), "{} {}", energy(&b, 0, 960), energy(&a, 0, 960));
        assert!(energy(&b, 7200, 12_000) > 10.0 * energy(&a, 7200, 12_000));
    }

    #[test]
    fn each_effect_changes_its_part_and_stays_bounded() {
        let base = run(plain(), 48_000);
        for effect in [0.0, 1.0, 2.0, 4.0] {
            for part in 0..2 {
                let mut f = plain();
                f.p[EFFECT + part] = effect;
                f.p[SAT + 3 * part + 1] = 9000.0;
                f.p[DIFF + 3 * part] = 10000.0;
                f.p[DIFF + 3 * part + 2] = 5000.0;
                f.p[ECHO + 4 * part + 3] = 9000.0;
                let out = run(f, 48_000);
                let diff: f32 = out.iter().zip(&base).map(|(a, b)| (a[0] - b[0]).abs()).sum();
                assert!(diff > 1.0, "effect {effect} on part {part} did nothing");
                assert!(out.iter().all(|f| f[0].is_finite() && f[0].abs() < 4.0), "effect {effect} on part {part} ran away");
            }
        }
    }

    #[test]
    fn a_tempo_echo_lands_on_its_step() {
        // a single click through the body part's echo, three steps long, no feedback
        let mut f = plain();
        f.p[ON] = 0.0;
        f.p[EFFECT + 1] = 4.0;
        f.p[SYNC + 1] = 1.0;
        f.p[ECHO + 4 + 2] = 144.0;
        f.p[ECHO + 4 + 3] = 0.0;
        f.p[SENS] = 0.0;
        let cx = Ctx { sr: 48_000.0, step: 6000.0, clock: Default::default() };
        let mut u = Unit::new(f, 48_000.0);
        let mut io = vec![[0.0f32; 2]; 24_000];
        // quiet enough not to count as a burst
        io[100] = [1e-5, 1e-5];
        u.process(&mut io, &cx);
        let peak = io.iter().enumerate().max_by(|a, b| a.1[0].abs().total_cmp(&b.1[0].abs())).map(|x| x.0);
        assert_eq!(peak, Some(100 + 18_000));
    }
}
