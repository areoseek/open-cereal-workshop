//! Duo Synth: two oscillators (saw or pulse) with sync and ring, a noise source, one resonant filter, two
//! envelopes, a slow oscillator and up to four stacked copies of the voice. After the voices come an echo, a
//! chorus and a sixteen-step gate (`tailfx.rs`).
//!
//! The stored settings are a list of numbers, most from 0 to 1. Which number is which was worked out from how
//! they move together across many stored sounds and from the public manual's description of the controls;
//! the scales are our estimates.

use std::f32::consts::{PI, TAU};

use crate::tailfx;

/// Numbers we read from the stored list.
pub const COUNT: usize = 81;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ctl {
    Knob,
    Switch,
    Pick(&'static [&'static str]),
    /// A whole number from 0 to this.
    Whole(u32),
    /// One of these by name, stored as its place in the list.
    Choice(&'static [&'static str]),
    /// One of these by name, stored as its place in the list over ten.
    Tenths(&'static [&'static str]),
    /// Not placed yet: kept, not shown.
    Kept,
}

pub struct Info {
    pub name: &'static str,
    pub ctl: Ctl,
    pub def: f32,
    /// The group it is shown in.
    pub group: u8,
}

pub const GROUPS: [&str; 10] = ["Oscillator A", "Oscillator B", "Mix", "Filter", "Envelope", "Level", "Slow oscillator", "Voice", "Echo and chorus", "Gate"];

const fn k(name: &'static str, def: f32, group: u8) -> Info {
    Info { name, ctl: Ctl::Knob, def, group }
}
const fn s(name: &'static str, def: f32, group: u8) -> Info {
    Info { name, ctl: Ctl::Switch, def, group }
}
const fn kept(def: f32) -> Info {
    Info { name: "", ctl: Ctl::Kept, def, group: 0 }
}

const TARGETS: &[&str] = &["Cutoff", "Pitch A", "Pitch A and B", "Width A", "Width A and B"];
const LFO_TARGETS: &[&str] = &["Cutoff", "Pitch A", "Pitch A and B", "Width A", "Width A and B", "Pan"];

pub const PARAMS: [Info; COUNT] = [
    s("Pulse", 0.0, 0),
    k("Width", 1.0, 0),
    s("Ring", 0.0, 0),
    s("Sync", 0.0, 0),
    s("Pulse", 0.0, 1),
    k("Width", 1.0, 1),
    k("Pitch", 0.5, 1),
    k("Detune", 0.0, 1),
    k("A / B", 0.0, 2),
    k("Noise", 0.0, 2),
    k("Amount", 0.5, 4),
    k("Attack", 0.0, 4),
    k("Decay", 1.0, 4),
    k("Sustain", 1.0, 4),
    k("Release", 1.0, 4),
    k("Gain", 0.5, 7),
    k("Fine tune", 0.5, 7),
    k("Transpose", 0.5, 7),
    k("Cutoff", 1.0, 3),
    k("Resonance", 0.0, 3),
    k("Key follow", 0.0, 3),
    k("Velocity", 0.0, 3),
    k("Attack", 0.0, 5),
    k("Decay", 1.0, 5),
    k("Sustain", 1.0, 5),
    k("Release", 0.0, 5),
    k("Amount", 0.5, 6),
    s("Restart", 0.0, 6),
    k("Rate", 0.2, 6),
    k("Stack detune", 0.1, 7),
    k("Stack pan", 0.0, 7),
    s("Echo", 0.0, 8),
    k("Left", 0.42857143, 8),
    k("Right", 0.42857143, 8),
    k("Feedback", 0.5, 8),
    k("Wobble", 0.3, 8),
    k("Low cut", 0.0, 8),
    k("High cut", 1.0, 8),
    k("Echo wet", 0.5, 8),
    s("Chorus", 0.0, 8),
    k("Depth", 0.5, 8),
    k("Rate", 0.5, 8),
    k("Chorus wet", 0.4, 8),
    kept(0.5),
    s("One voice", 0.0, 7),
    s("Octaves", 0.0, 7),
    k("Glide", 0.0, 7),
    k("Gate wet", 0.0, 9),
    k("Smooth", 0.0, 9),
    s("1", 1.0, 9),
    s("2", 0.0, 9),
    s("3", 1.0, 9),
    s("4", 0.0, 9),
    s("5", 1.0, 9),
    s("6", 1.0, 9),
    s("7", 1.0, 9),
    s("8", 0.0, 9),
    s("9", 1.0, 9),
    s("10", 0.0, 9),
    s("11", 1.0, 9),
    s("12", 0.0, 9),
    s("13", 1.0, 9),
    s("14", 1.0, 9),
    s("15", 1.0, 9),
    s("16", 0.0, 9),
    Info { name: "Target", ctl: Ctl::Pick(TARGETS), def: 0.0, group: 4 },
    Info { name: "Mode", ctl: Ctl::Pick(&["Low pass", "Band pass", "High pass"]), def: 0.0, group: 3 },
    Info { name: "Target", ctl: Ctl::Pick(LFO_TARGETS), def: 0.0, group: 6 },
    Info { name: "Shape", ctl: Ctl::Pick(&["Saw", "Pulse", "Triangle", "Sine", "Random"]), def: 0.5, group: 6 },
    kept(0.0),
    kept(0.0),
    kept(0.0),
    kept(0.0),
    kept(0.0),
    kept(0.0),
    Info { name: "Stack", ctl: Ctl::Whole(3), def: 0.0, group: 7 },
    kept(15.0),
    kept(1.0),
    kept(0.0),
    kept(0.0),
    kept(0.0),
];

/// The values of a new instrument.
pub fn defaults() -> Vec<f32> {
    PARAMS.iter().map(|i| i.def).collect()
}

/// A fresh stored block: the mark, the length of what follows, the version mark, the numbers, the controller
/// links (none) and an empty name.
pub fn fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(700);
    v.extend_from_slice(b"SSLF");
    v.extend_from_slice(&692u32.to_le_bytes());
    v.extend_from_slice(b"4reV");
    for x in defaults() {
        v.extend_from_slice(&x.to_le_bytes());
    }
    for _ in 81..155 {
        v.extend_from_slice(&(-1.0f32).to_le_bytes());
    }
    v.resize(700, 0);
    v
}

/// Which option of `n` a stored 0 to 1 value picks.
pub fn pick(v: f32, n: usize) -> usize {
    ((v.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(n - 1)
}

#[derive(Debug, Clone, Copy, Default)]
struct Adsr {
    a: f32,
    d: f32,
    s: f32,
    r: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Set {
    pulse: [bool; 2],
    width: [f32; 2],
    ring: bool,
    sync: bool,
    /// Semitones of the second oscillator against the first.
    b_semis: f32,
    /// Semitones of the whole voice.
    tune: f32,
    bal: f32,
    noise: f32,
    mode: usize,
    cutoff: f32,
    res: f32,
    track: f32,
    vel_track: f32,
    eg: Adsr,
    eg_to: usize,
    eg_amt: f32,
    amp: Adsr,
    lfo_to: usize,
    lfo_amt: f32,
    lfo_hz: f32,
    lfo_shape: usize,
    lfo_restart: bool,
    stack: usize,
    stack_detune: f32,
    stack_pan: f32,
    octaves: bool,
    gain: f32,
    /// One voice at a time, and the seconds a held note takes to slide to the next.
    pub mono: Option<f32>,
}

fn time(v: f32) -> f32 {
    0.002 + v.clamp(0.0, 1.0).powi(3) * 8.0
}

/// Whole semitones of a setting that runs two octaves each way.
fn semis(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 48.0 + 0.001).floor() - 24.0
}

pub fn set(p: &[f32]) -> Set {
    let d;
    let p = if p.len() >= COUNT {
        p
    } else {
        d = defaults();
        &d[..]
    };
    let on = |i: usize| p[i] >= 0.5;
    let u = |i: usize| p[i].clamp(0.0, 1.0);
    let env = |i: usize| Adsr { a: time(p[i]), d: time(p[i + 1]), s: u(i + 2), r: time(p[i + 3]) };
    Set {
        pulse: [on(0), on(4)],
        width: [u(1), u(5)],
        ring: on(2),
        sync: on(3),
        b_semis: semis(p[6]) + u(7) * 0.5,
        tune: semis(p[17]) + (u(16) - 0.5) * 2.0,
        bal: u(8),
        noise: u(9),
        mode: pick(p[66], 3),
        cutoff: u(18),
        res: u(19),
        track: u(20),
        vel_track: u(21),
        eg: env(11),
        eg_to: pick(p[65], 5),
        eg_amt: (u(10) - 0.5) * 2.0,
        amp: env(22),
        lfo_to: pick(p[67], 6),
        lfo_amt: (u(26) - 0.5) * 2.0,
        lfo_hz: 0.05 * 600f32.powf(u(28)),
        lfo_shape: pick(p[68], 5),
        lfo_restart: on(27),
        stack: 1 + (p[75].round().clamp(0.0, 3.0) as usize),
        stack_detune: u(29),
        stack_pan: u(30),
        octaves: on(45),
        gain: 0.1 + u(15) * 0.5,
        mono: on(44).then(|| if u(46) < 0.005 { 0.0 } else { 0.005 + u(46).powi(2) * 2.0 }),
    }
}

/// The effects after the voices.
pub fn tail(p: &[f32]) -> tailfx::Set {
    if p.len() < COUNT {
        return tailfx::Set::default();
    }
    let u = |i: usize| p[i].clamp(0.0, 1.0);
    let steps = |i: usize| 1.0 + (u(i) * 7.0).round();
    let mut pattern = 0u16;
    for k in 0..16 {
        if p[49 + k] >= 0.5 {
            pattern |= 1 << k;
        }
    }
    tailfx::Set {
        delay: (p[31] >= 0.5 && u(38) > 0.0).then(|| tailfx::Delay {
            left: steps(32),
            right: steps(33),
            fb: u(34) * 0.95,
            wobble: u(35),
            low_hz: if u(36) < 0.005 { 0.0 } else { 20.0 * 2f32.powf(u(36) * 8.0) },
            high_hz: 200.0 * 100f32.powf(u(37)),
            wet: u(38),
            free: 0.0,
        }),
        chorus: (p[39] >= 0.5 && u(42) > 0.0).then(|| tailfx::Chorus { depth: u(40), hz: 0.05 * 100f32.powf(u(41)), wet: u(42) }),
        gate: (u(47) > 0.0).then(|| tailfx::Gate { wet: u(47), smooth: u(48), steps: pattern, len: 1.0 }),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Env {
    v: f32,
    stage: u8,
}

impl Env {
    fn step(&mut self, e: &Adsr, dt: f32) -> f32 {
        match self.stage {
            0 => {
                self.v += dt / e.a.max(0.0005);
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.stage = 1;
                }
            }
            1 => self.v = e.s + (self.v - e.s) * (-dt * 4.6 / e.d.max(0.001)).exp(),
            _ => self.v *= (-dt * 4.6 / e.r.max(0.001)).exp(),
        }
        self.v
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Copy1 {
    pa: f32,
    pb: f32,
    /// What the sync correction still owes the next frame.
    owe: f32,
    ratio: f32,
    gl: f32,
    gr: f32,
}

const EVERY: u32 = 16;

#[derive(Debug, Clone, Default)]
pub struct Voice {
    sr: f32,
    key: f32,
    vel: f32,
    c: [Copy1; 4],
    amp: Env,
    eg: Env,
    lfo: f32,
    hold: f32,
    seed: u32,
    t: u32,
    z: [[f32; 4]; 2],
    g: f32,
    k: f32,
    a: f32,
    a_to: f32,
    ra: f32,
    rb: f32,
    wa: f32,
    wb: f32,
    pan: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / 8_388_608.0 - 1.0
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

/// The share of the cycle a pulse stays up: a spike at 0, a square at 1.
fn duty(width: f32) -> f32 {
    0.03 + 0.47 * width.clamp(0.0, 1.0)
}

fn plain(pulse: bool, ph: f32, width: f32) -> f32 {
    if pulse {
        if ph < duty(width) {
            1.0
        } else {
            -1.0
        }
    } else {
        2.0 * ph - 1.0
    }
}

fn osc(pulse: bool, ph: f32, dt: f32, width: f32) -> f32 {
    if pulse {
        let q = ph - duty(width);
        plain(true, ph, width) + blep(ph, dt) - blep(if q < 0.0 { q + 1.0 } else { q }, dt)
    } else {
        2.0 * ph - 1.0 - blep(ph, dt)
    }
}

impl Voice {
    pub fn start(set: &Set, sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let mut v = Voice { sr, key: key as f32, vel, seed: seed | 1, ra: 1.0, rb: 1.0, ..Default::default() };
        let n = set.stack;
        for (i, c) in v.c.iter_mut().enumerate().take(n) {
            // spread from -1 to 1 across the stack; a single copy sits in the middle
            let x = if n > 1 { i as f32 / (n - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
            c.pa = rnd(&mut v.seed) * 0.5 + 0.5;
            c.pb = rnd(&mut v.seed) * 0.5 + 0.5;
            let up = if set.octaves && i % 2 == 1 { 2.0 } else { 1.0 };
            c.ratio = 2f32.powf(x * set.stack_detune * 0.5 / 12.0) * up;
            let pan = x * set.stack_pan;
            let norm = 1.0 / (n as f32).sqrt();
            c.gl = (1.0 - pan).min(1.0) * norm;
            c.gr = (1.0 + pan).min(1.0) * norm;
        }
        v.lfo = if set.lfo_restart { 0.0 } else { rnd(&mut v.seed) * 0.5 + 0.5 };
        v.hold = rnd(&mut v.seed);
        v
    }

    /// Let the note go; gives the seconds until it has died away.
    pub fn release(&mut self, set: &Set) -> f32 {
        self.amp.stage = 2;
        self.eg.stage = 2;
        set.amp.r.max(0.005) * 1.5
    }

    fn control(&mut self, set: &Set) {
        let dt = EVERY as f32 / self.sr;
        self.a_to = self.amp.step(&set.amp, dt);
        let e = self.eg.step(&set.eg, dt) * set.eg_amt;
        let before = self.lfo;
        self.lfo = (before + set.lfo_hz * dt).fract();
        let ph = self.lfo;
        if ph < before {
            self.hold = rnd(&mut self.seed);
        }
        let l = match set.lfo_shape {
            0 => 1.0 - 2.0 * ph,
            1 => {
                if ph < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            2 => 1.0 - 4.0 * (ph - 0.5).abs(),
            3 => (ph * TAU).sin(),
            _ => self.hold,
        } * set.lfo_amt;
        let mut cut = (self.key - 60.0) / 12.0 * set.track + self.vel * set.vel_track * 4.0;
        let (mut sa, mut sb) = (0.0, 0.0);
        let (mut wa, mut wb) = (set.width[0], set.width[1]);
        match set.eg_to {
            0 => cut += e * 10.0,
            1 => sa += e * 36.0,
            2 => {
                sa += e * 36.0;
                sb += e * 36.0;
            }
            3 => wa += e,
            _ => {
                wa += e;
                wb += e;
            }
        }
        self.pan = 0.0;
        // small amounts give a gentle vibrato, the top of the knob a full octave
        let bend = l * l.abs() * 12.0;
        match set.lfo_to {
            0 => cut += l * 4.0,
            1 => sa += bend,
            2 => {
                sa += bend;
                sb += bend;
            }
            3 => wa += l * 0.5,
            4 => {
                wa += l * 0.5;
                wb += l * 0.5;
            }
            _ => self.pan = l,
        }
        self.ra = 2f32.powf((set.tune + sa) / 12.0);
        self.rb = 2f32.powf((set.tune + set.b_semis + sb) / 12.0);
        self.wa = wa.clamp(0.0, 1.0);
        self.wb = wb.clamp(0.0, 1.0);
        let hz = (20.0 * 1000f32.powf(set.cutoff) * 2f32.powf(cut)).clamp(16.0, self.sr * 0.45);
        self.g = (PI * hz / self.sr).tan();
        self.k = 2.0 - 1.96 * set.res;
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        if self.t % EVERY == 0 {
            self.control(set);
        }
        self.t = self.t.wrapping_add(1);
        self.a += (self.a_to - self.a) * 0.02;
        let (wa, wb) = (self.wa, self.wb);
        let mut out = [0.0f32; 2];
        for c in self.c.iter_mut().take(set.stack) {
            let da = (dphase * c.ratio * self.ra).min(0.45);
            let db = (dphase * c.ratio * self.rb).min(0.45);
            let b = osc(set.pulse[1], c.pb, db, wb);
            let mut a = osc(set.pulse[0], c.pa, da, wa) + c.owe;
            c.owe = 0.0;
            c.pa += da;
            if c.pa >= 1.0 {
                c.pa -= 1.0;
            }
            c.pb += db;
            if c.pb >= 1.0 {
                c.pb -= 1.0;
                if set.sync {
                    // the first oscillator starts over where the second did; the jump is rounded off over this
                    // frame and the next
                    let since = c.pb / db;
                    let fresh = (since * da).min(0.999);
                    let h = plain(set.pulse[0], fresh, wa) - plain(set.pulse[0], c.pa, wa);
                    a += h * 0.5 * since * since;
                    c.owe = -h * 0.5 * (1.0 - since) * (1.0 - since);
                    c.pa = fresh;
                }
            }
            if set.ring {
                a *= b;
            }
            let mut x = a * (1.0 - set.bal) + b * set.bal;
            if set.noise > 0.0 {
                x = x * (1.0 - set.noise) + rnd(&mut self.seed) * set.noise;
            }
            out[0] += x * c.gl;
            out[1] += x * c.gr;
        }
        let (g, k) = (self.g, self.k);
        let wide = set.stack > 1 && set.stack_pan > 0.0;
        let mode = set.mode;
        let run = |z: &mut [f32; 4], x: f32| {
            let stage = |z: &mut [f32], x: f32, k: f32| {
                let hp = (x - (k + g) * z[0] - z[1]) / (1.0 + g * (g + k));
                let bp = g * hp + z[0];
                z[0] = g * hp + bp;
                let lp = g * bp + z[1];
                z[1] = g * bp + lp;
                // keep a ringing filter inside its rails
                z[0] = z[0].clamp(-8.0, 8.0);
                (lp, bp, hp)
            };
            let (lp, bp, hp) = stage(&mut z[0..2], x, k);
            match mode {
                0 => stage(&mut z[2..4], lp, 1.5).0,
                1 => bp * k.max(0.25),
                _ => stage(&mut z[2..4], hp, 1.5).2,
            }
        };
        let l = run(&mut self.z[0], out[0] * 0.5);
        let r = if wide { run(&mut self.z[1], out[1] * 0.5) } else { l };
        let lvl = self.a * self.vel * set.gain;
        [l * lvl * (1.0 - self.pan).min(1.0), r * lvl * (1.0 + self.pan).min(1.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &[f32], key: u8, secs: f32) -> Vec<[f32; 2]> {
        let set = set(p);
        let sr = 44100.0;
        let mut v = Voice::start(&set, sr, key, 0.8, 7);
        let d = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0) / sr;
        (0..(sr * secs) as usize).map(|_| v.frame(&set, d)).collect()
    }

    fn rms(x: &[[f32; 2]]) -> f32 {
        (x.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn a_fresh_block_reads_back_as_the_defaults() {
        let b = fresh();
        assert_eq!(b.len(), 700);
        let back: Vec<f32> = b[12..12 + COUNT * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(back, defaults());
    }

    #[test]
    fn the_default_sound_plays_and_stays_in_range() {
        let out = run(&defaults(), 60, 0.5);
        assert!(rms(&out) > 0.02, "{}", rms(&out));
        assert!(out.iter().all(|f| f[0].abs() < 1.5 && f[0].is_finite()));
    }

    #[test]
    fn closing_the_filter_darkens_the_sound() {
        let mut p = defaults();
        let open = rms(&run(&p, 72, 0.3));
        p[18] = 0.2;
        p[10] = 0.5;
        let shut = rms(&run(&p, 72, 0.3));
        assert!(shut < open * 0.3, "{shut} {open}");
    }

    #[test]
    fn sync_ring_stack_and_modes_all_stay_finite() {
        let mut p = defaults();
        p[3] = 1.0;
        p[2] = 1.0;
        p[6] = 0.3;
        p[10] = 1.0;
        p[65] = 0.25;
        p[75] = 3.0;
        p[29] = 0.6;
        p[30] = 1.0;
        p[45] = 1.0;
        p[19] = 1.0;
        for mode in [0.0, 0.5, 1.0] {
            p[66] = mode;
            let out = run(&p, 48, 0.4);
            assert!(out.iter().all(|f| f[0].is_finite() && f[1].is_finite() && f[0].abs() < 8.0), "mode {mode}");
            assert!(rms(&out) > 0.001);
        }
        // a panned stack is not the same on both sides
        let out = run(&p, 48, 0.2);
        assert!(out.iter().any(|f| (f[0] - f[1]).abs() > 0.01));
    }

    #[test]
    fn the_tail_follows_its_switches() {
        let mut p = defaults();
        assert_eq!(tail(&p), tailfx::Set::default());
        p[31] = 1.0;
        p[39] = 1.0;
        p[47] = 1.0;
        let t = tail(&p);
        assert_eq!(t.delay.unwrap().left, 4.0);
        assert!(t.chorus.is_some());
        assert_eq!(t.gate.unwrap().steps, 0b0111_0101_0111_0101);
    }
}
