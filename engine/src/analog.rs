//! A subtractive voice for the small analog-style instruments: up to three oscillators, a ring modulator, a
//! four-pole filter with its own envelope, two slow oscillators for movement and a drive stage.
//!
//! Each instrument that uses it turns its stored settings into a [`Set`]. Scales are estimates from the public
//! manual's ranges unless a comment says otherwise.

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Shape {
    #[default]
    Saw,
    Pulse,
    /// A pulse that ignores the width setting.
    Square,
    Sine,
    Triangle,
    Noise,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Osc {
    pub shape: Shape,
    /// Semitones from the played key.
    pub semis: f32,
    pub level: f32,
}

/// What a slow oscillator moves.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Dest {
    #[default]
    None,
    /// Pitch of every oscillator, in semitones at full amount.
    Pitch,
    /// Pitch of the first oscillator only.
    Pitch1,
    /// Filter cutoff, in octaves at full amount.
    Cutoff,
    Width,
    /// The balance between the first two oscillators.
    Mix,
    Level,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Lfo {
    pub shape: Shape,
    pub dest: Dest,
    /// 0 to 1.
    pub amount: f32,
    pub hz: f32,
    /// Start from the top of the wave on each note.
    pub reset: bool,
    /// Seconds over which it fades in after the key goes down.
    pub delay: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Filter {
    #[default]
    Low12,
    Low24,
    LowNotch,
    TwoNotch,
    Band,
    High,
}

/// Attack, decay and release in seconds, sustain from 0 to 1.
#[derive(Debug, Clone, Copy)]
pub struct Adsr {
    pub a: f32,
    pub d: f32,
    pub s: f32,
    pub r: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self { a: 0.002, d: 0.3, s: 1.0, r: 0.05 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Set {
    pub osc: [Osc; 3],
    /// How far the pulse is from square, 0 to 1.
    pub width: f32,
    /// Multiply the first two oscillators instead of adding them.
    pub ring: bool,
    /// The first oscillator bends the second's pitch by this much (0 to 1).
    pub fm: f32,
    /// A second, slightly detuned copy of the first two oscillators.
    pub dual: bool,
    pub amp: Adsr,
    pub fenv: Adsr,
    pub filter: Filter,
    pub cutoff_hz: f32,
    /// 0 to 1.
    pub res: f32,
    /// Octaves the filter envelope opens the cutoff at its top.
    pub env_oct: f32,
    /// How much the cutoff follows the key, 0 to 1.
    pub track: f32,
    pub lfo: [Lfo; 3],
    /// White noise mixed in before the filter.
    pub noise: f32,
    /// A gentle low cut before the filter (0 for none).
    pub hpf_hz: f32,
    /// The filter envelope also narrows the pulse by this much.
    pub width_env: f32,
    /// Ensemble after the voice: 0 off, 1 slow, 2 faster, 3 both.
    pub chorus: u8,
    /// Drive (0 to 1) and tone (0 dark to 1 bright), when the drive stage is on.
    pub drive: Option<(f32, f32)>,
    pub gain: f32,
    /// A rise-and-fall envelope: rise and fall in seconds, depth -1 to 1, and what it moves (pulse width, the
    /// first slow wave's depth, the first oscillator's level, the second's pitch).
    pub modenv: Option<(f32, f32, f32, [bool; 4])>,
    /// How much the hit strength sets the level (None: wholly) and opens the filter.
    pub vel_amp: Option<f32>,
    pub vel_cut: f32,
    /// Some(true): oscillators drift a few cents and start anywhere; Some(false): they start together.
    pub drift: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Env {
    v: f32,
    stage: u8,
}

impl Env {
    /// One control step of `dt` seconds.
    fn step(&mut self, e: &Adsr, dt: f32) -> f32 {
        match self.stage {
            0 => {
                self.v += dt / e.a.max(0.0005);
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.stage = 1;
                }
            }
            1 => {
                // falls most of the way to the sustain level in the decay time
                let k = (-dt * 4.6 / e.d.max(0.001)).exp();
                self.v = e.s + (self.v - e.s) * k;
            }
            _ => {
                let k = (-dt * 4.6 / e.r.max(0.001)).exp();
                self.v *= k;
            }
        }
        self.v
    }
}

/// Control values are worked out once every this many frames.
const EVERY: u32 = 16;

#[derive(Debug, Clone, Default)]
pub struct Voice {
    sr: f32,
    vel: f32,
    key: f32,
    ph: [f32; 5],
    amp: Env,
    fenv: Env,
    lfo: [f32; 3],
    lfo_hold: [f32; 3],
    hp: f32,
    hpc: f32,
    ens: Option<Box<Ensemble>>,
    seed: u32,
    t: u32,
    // control values held between updates
    a: f32,
    a_to: f32,
    g: f32,
    k: f32,
    bend: [f32; 2],
    width: f32,
    mix: f32,
    z: [f32; 4],
    tone: f32,
    last1: f32,
    off: [f32; 2],
    one: f32,
    lvl: f32,
}

/// The ensemble of the old string machines and polys: one short delay swept slowly, heard a little early on one
/// side and a little late on the other.
#[derive(Debug, Clone)]
struct Ensemble {
    line: Vec<f32>,
    at: usize,
    ph: f32,
    step: f32,
    depth: f32,
    mid: f32,
}

impl Ensemble {
    fn new(mode: u8, sr: f32) -> Self {
        // sweep rate in cycles per second, and how far the delay moves, in seconds
        let (hz, depth) = match mode {
            1 => (0.5, 0.0018),
            2 => (0.83, 0.0018),
            _ => (8.0, 0.0003),
        };
        Self { line: vec![0.0; (sr * 0.012) as usize + 4], at: 0, ph: 0.0, step: hz / sr, depth: depth * sr, mid: 0.0035 * sr }
    }

    fn frame(&mut self, x: f32) -> [f32; 2] {
        let n = self.line.len();
        self.line[self.at] = x;
        self.ph = (self.ph + self.step).fract();
        let tri = 1.0 - 4.0 * (self.ph - 0.5).abs();
        let read = |d: f32| {
            let back = d.clamp(1.0, n as f32 - 3.0);
            let whole = back.floor();
            let i = (self.at + n - whole as usize) % n;
            let j = (i + n - 1) % n;
            self.line[i] + (self.line[j] - self.line[i]) * (back - whole)
        };
        let out = [(x + read(self.mid + tri * self.depth)) * 0.7, (x + read(self.mid - tri * self.depth)) * 0.7];
        self.at = (self.at + 1) % n;
        out
    }
}

fn rnd(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / 8_388_608.0 - 1.0
}

/// The step-smoothing correction for a wave with a jump at phase 0.
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

fn wave(shape: Shape, ph: f32, dt: f32, width: f32, seed: &mut u32) -> f32 {
    match shape {
        Shape::Saw => 2.0 * ph - 1.0 - blep(ph, dt),
        Shape::Pulse => {
            let duty = 0.5 - 0.45 * width.clamp(0.0, 1.0);
            let mut y = if ph < duty { 1.0 } else { -1.0 };
            y += blep(ph, dt);
            let q = ph - duty;
            y -= blep(if q < 0.0 { q + 1.0 } else { q }, dt);
            y
        }
        Shape::Square => wave(Shape::Pulse, ph, dt, 0.0, seed),
        Shape::Sine => (ph * TAU).sin(),
        Shape::Triangle => 1.0 - 4.0 * (ph - 0.5).abs(),
        Shape::Noise => rnd(seed),
    }
}

impl Voice {
    pub fn start(set: &Set, sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let mut v = Voice { sr, vel, key: key as f32, seed: seed | 1, mix: 0.5, ..Default::default() };
        for (k, l) in set.lfo.iter().enumerate() {
            v.lfo[k] = if l.reset { 0.0 } else { rnd(&mut v.seed) * 0.5 + 0.5 };
        }
        // free-running oscillators: each note starts somewhere else in the wave
        for p in &mut v.ph {
            *p = rnd(&mut v.seed) * 0.5 + 0.5;
        }
        match set.drift {
            Some(true) => v.off = [rnd(&mut v.seed) * 0.05, rnd(&mut v.seed) * 0.05],
            Some(false) => v.ph = [0.0; 5],
            None => {}
        }
        v.one = 1.0;
        v.lvl = match set.vel_amp {
            Some(a) => 1.0 - a + a * vel,
            None => vel,
        };
        v.t = 0;
        if set.hpf_hz > 0.0 {
            v.hpc = 1.0 - (-TAU * set.hpf_hz / sr).exp();
        }
        if set.chorus != 0 {
            v.ens = Some(Box::new(Ensemble::new(set.chorus, sr)));
        }
        v
    }

    /// Let the note go; gives the seconds until it has died away.
    pub fn release(&mut self, set: &Set) -> f32 {
        self.amp.stage = 2;
        self.fenv.stage = 2;
        set.amp.r.max(0.005) * 1.5
    }

    fn control(&mut self, set: &Set) {
        let dt = EVERY as f32 / self.sr;
        let a = self.amp.step(&set.amp, dt);
        let f = self.fenv.step(&set.fenv, dt);
        let mut cut_oct = f * set.env_oct + (self.key - 60.0) / 12.0 * set.track + set.vel_cut * (self.vel - 0.6) * 4.0;
        let mut level = 1.0;
        self.bend = self.off;
        self.width = (set.width + f * set.width_env).clamp(0.0, 1.0);
        self.mix = 0.5;
        let since = self.t as f32 / self.sr;
        let mut lfo1 = 1.0;
        self.one = 1.0;
        if let Some((rise, fall, depth, to)) = set.modenv {
            let m = if since < rise { since / rise.max(1e-4) } else { (-(since - rise) * 4.6 / fall.max(0.001)).exp() } * depth;
            if to[0] {
                self.width = (self.width + m * 0.5).clamp(0.0, 1.0);
            }
            if to[1] {
                lfo1 = (1.0 + m).max(0.0);
            }
            if to[2] {
                self.one = (1.0 + m).clamp(0.0, 2.0);
            }
            if to[3] {
                self.bend[1] += m * 12.0;
            }
        }
        for (k, l) in set.lfo.iter().enumerate() {
            let before = self.lfo[k];
            self.lfo[k] = (before + l.hz * dt).fract();
            let ph = self.lfo[k];
            let x = match l.shape {
                Shape::Saw => 1.0 - 2.0 * ph,
                Shape::Pulse | Shape::Square => {
                    if ph < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                Shape::Sine => (ph * TAU).sin(),
                Shape::Triangle => 1.0 - 4.0 * (ph - 0.5).abs(),
                Shape::Noise => {
                    // a new random level each cycle
                    if ph < before || self.t == 0 {
                        self.lfo_hold[k] = rnd(&mut self.seed);
                    }
                    self.lfo_hold[k]
                }
            } * l.amount
                * if k == 0 { lfo1 } else { 1.0 }
                * if l.delay > 0.0 { (since / l.delay).min(1.0) } else { 1.0 };
            match l.dest {
                Dest::None => {}
                Dest::Pitch => {
                    self.bend[0] += x * 12.0 * l.amount;
                    self.bend[1] += x * 12.0 * l.amount;
                }
                Dest::Pitch1 => self.bend[0] += x * 12.0 * l.amount,
                Dest::Cutoff => cut_oct += x * 4.0,
                Dest::Width => self.width = (self.width + x * 0.5).clamp(0.0, 1.0),
                Dest::Mix => self.mix = (0.5 + x * 0.5).clamp(0.0, 1.0),
                Dest::Level => level *= 1.0 - (0.5 - 0.5 * x).clamp(0.0, 1.0) * l.amount.min(1.0),
            }
        }
        self.a_to = a * level;
        let hz = (set.cutoff_hz * 2f32.powf(cut_oct)).clamp(16.0, self.sr * 0.45);
        self.g = (std::f32::consts::PI * hz / self.sr).tan();
        self.k = 2.0 - 1.92 * set.res.clamp(0.0, 1.0);
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        if self.t % EVERY == 0 {
            self.control(set);
        }
        self.t = self.t.wrapping_add(1);
        self.a += (self.a_to - self.a) * 0.02;
        let width = self.width;
        let step = |semis: f32| (dphase * 2f32.powf(semis / 12.0)).min(0.45);
        // the pitch factors change only with the control values, but the power is cheap next to the filter
        let d1 = step(set.osc[0].semis + self.bend[0]);
        let mut d2 = step(set.osc[1].semis + self.bend[1]);
        let d3 = step(set.osc[2].semis + self.bend[1]);
        if set.fm > 0.0 {
            d2 = (d2 * (1.0 + self.last1 * set.fm * 4.0)).clamp(0.0, 0.45);
        }
        let pair = |ph: &mut [f32], d1: f32, d2: f32, seed: &mut u32| {
            let a = wave(set.osc[0].shape, ph[0], d1, width, seed);
            let b = wave(set.osc[1].shape, ph[1], d2, width, seed);
            ph[0] = (ph[0] + d1).fract();
            ph[1] = (ph[1] + d2).fract();
            (a, b)
        };
        let (mut a, mut b) = pair(&mut self.ph[0..2], d1, d2, &mut self.seed);
        self.last1 = a;
        if set.dual {
            // the copy sits a few cents up
            let (a2, b2) = pair(&mut self.ph[3..5], d1 * 1.0045, d2 * 1.0045, &mut self.seed);
            a = (a + a2) * 0.6;
            b = (b + b2) * 0.6;
        }
        let (la, lb) = (set.osc[0].level * self.one * (1.0 - self.mix) * 2.0, set.osc[1].level * self.mix * 2.0);
        let mut x = if set.ring { a * b * (la + lb) * 0.5 } else { a * la + b * lb };
        if set.osc[2].level > 0.0 {
            x += wave(set.osc[2].shape, self.ph[2], d3, width, &mut self.seed) * set.osc[2].level;
            self.ph[2] = (self.ph[2] + d3).fract();
        }
        if set.noise > 0.0 {
            x += rnd(&mut self.seed) * set.noise;
        }
        x *= 0.5;
        if self.hpc > 0.0 {
            self.hp += (x - self.hp) * self.hpc;
            x -= self.hp;
        }
        // two two-pole stages
        let (g, k) = (self.g, self.k);
        let stage = |z: &mut [f32], x: f32, k: f32| {
            let hp = (x - (k + g) * z[0] - z[1]) / (1.0 + g * (g + k));
            let bp = g * hp + z[0];
            z[0] = g * hp + bp;
            let lp = g * bp + z[1];
            z[1] = g * bp + lp;
            (lp, bp, hp)
        };
        let (lp, bp, hp) = stage(&mut self.z[0..2], x, k);
        let y = match set.filter {
            Filter::Low12 => lp,
            Filter::Low24 => stage(&mut self.z[2..4], lp, 1.6).0,
            Filter::LowNotch => {
                let (l2, _, h2) = stage(&mut self.z[2..4], lp, 1.0);
                (l2 + h2) * 0.9
            }
            Filter::TwoNotch => {
                let n = lp + hp;
                let (l2, _, h2) = stage(&mut self.z[2..4], n, k);
                l2 + h2
            }
            Filter::Band => stage(&mut self.z[2..4], bp * k.max(0.3), k).1 * k.max(0.3),
            Filter::High => stage(&mut self.z[2..4], hp, 1.6).2,
        };
        let mut y = y * self.a;
        if let Some((drive, tone)) = set.drive {
            let pre = 1.0 + drive * 24.0;
            y = (y * pre).tanh() / (1.0 + drive * 2.0);
            // tone: a gentle low-pass that opens as the knob turns right
            let c = 0.05 + tone * tone * 0.95;
            self.tone += (y - self.tone) * c;
            y = self.tone;
        }
        let y = y * set.gain * self.lvl;
        match &mut self.ens {
            Some(e) => e.frame(y),
            None => [y, y],
        }
    }
}

/// The settings of the small three-oscillator synth (`synth::BUZZ` order) as a voice set.
pub fn buzz(p: &[f32]) -> Set {
    let v = |i: usize| p.get(i).copied().unwrap_or(0.0);
    let u = |i: usize| v(i) / 128.0;
    let shape = |i: usize| [Shape::Saw, Shape::Pulse, Shape::Sine, Shape::Noise][(v(i) as usize).min(3)];
    // the coarse knob spans three octaves each way; stored presets sit within a hair of whole semitones
    let pitch = |c: usize, f: usize| ((v(c) - 64.0) / 64.0 * 36.0).round() + (v(f) - 64.0) / 64.0;
    let time = |i: usize| 0.002 + u(i) * u(i) * 5.0;
    let env = |i: usize| Adsr { a: time(i), d: time(i + 1), s: u(i + 2), r: time(i + 3) };
    let mix = u(8);
    let xt = p.len() >= 52;
    let lfo = |i: usize, dests: [Dest; 3]| Lfo {
        shape: shape(i),
        dest: if v(i + 2) > 0.0 { dests[(v(i + 1) as usize).min(2)] } else { Dest::None },
        amount: u(i + 2),
        hz: 0.05 * 2f32.powf(u(i + 3) * 9.0),
        reset: v(i + 5) != 0.0,
        delay: 0.0,
    };
    Set {
        osc: [
            Osc { shape: shape(0), semis: pitch(1, 2), level: ((1.0 - mix) * 2.0).min(1.0) },
            Osc { shape: shape(3), semis: pitch(4, 5), level: (mix * 2.0).min(1.0) },
            // the third sits an octave under the first
            Osc { shape: if v(6) != 0.0 { Shape::Pulse } else { Shape::Saw }, semis: pitch(1, 2) - 12.0, level: u(7) },
        ],
        width: u(9),
        ring: v(11) != 0.0,
        fm: u(10),
        dual: v(40) != 0.0,
        amp: env(12),
        fenv: env(16),
        filter: [Filter::Low24, Filter::Low12, Filter::LowNotch, Filter::TwoNotch, Filter::Band, Filter::High][(v(21) as usize).min(5)],
        cutoff_hz: 30.0 * 2f32.powf(v(22) / 512.0 * 9.5),
        res: u(23),
        env_oct: u(24) * 7.0,
        track: if v(20) != 0.0 { 1.0 } else { 0.0 },
        lfo: [lfo(25, [Dest::Pitch, Dest::Cutoff, Dest::Width]), lfo(31, [Dest::Pitch1, Dest::Mix, Dest::Level]), Lfo::default()],
        drive: (v(37) != 0.0).then(|| (u(38), u(39))),
        gain: 0.5,
        // the later version's eleven
        modenv: (xt && (46..=49).any(|i| v(i) != 0.0) && v(45) != 64.0).then(|| (time(43), time(44), (v(45) - 64.0) / 64.0, [v(46) != 0.0, v(47) != 0.0, v(48) != 0.0, v(49) != 0.0])),
        vel_amp: xt.then(|| u(50)),
        vel_cut: if xt { u(51) } else { 0.0 },
        drift: xt.then(|| v(42) != 0.0),
        ..Default::default()
    }
}

/// The small touch-screen synth's stored values as a voice set. The places of the values were worked out from
/// how they move between presets: wave choice, modifier and noise at 4 to 6, the filter and its envelope at 7 to
/// 14, fine tune at 19, the level envelope at 20 to 23. The rest (play mode, slow oscillator, effects) is not read.
/// Settings of the three-oscillator analogue from its stored values (seventy numbers 0 to 1, then the switch
/// word). Per oscillator, seven numbers: pulse width, coarse, fine, level, slow-wave bend, envelope bend, wave
/// (tenths: sine, saw, pulse, triangle, noise). Then: 21 slow-wave delay, 22 its rate, 24 its wave, 26 cutoff,
/// 27 emphasis, 28 envelope to cutoff, 29 slow wave to cutoff, 30 key follow, 32 high-pass share, 33 band-pass
/// share, 35-38 level envelope, 39 volume, 41-44 filter envelope. The wave order and the envelopes were read off
/// the preset names; 23, 31, 34 and 40 are not placed, and of the switches only the ensemble (guess: bit 2).
pub fn trio(p: &[f32]) -> Set {
    const PLAIN: [f32; 7] = [0.5, 0.5, 0.5, 0.6, 0.5, 0.5, 0.1];
    let full = p.len() >= 71;
    let v = |i: usize| -> f32 {
        if full {
            return p[i];
        }
        match i {
            0..=20 => PLAIN[i % 7],
            26 | 28 | 29 | 39 => 0.5,
            37 => 1.0,
            _ => 0.0,
        }
    };
    let mut width = 0.0;
    let mut osc = [Osc::default(); 3];
    let mut bend = 0.0f32;
    for (i, o) in osc.iter_mut().enumerate() {
        let b = i * 7;
        let shape = match (v(b + 6) * 10.0).round() as u8 {
            1 => Shape::Saw,
            2 => Shape::Pulse,
            3 => Shape::Triangle,
            4 => Shape::Noise,
            _ => Shape::Sine,
        };
        if shape == Shape::Pulse && width == 0.0 {
            width = (2.0 * v(b) - 1.0).abs().min(0.95);
        }
        *o = Osc { shape, semis: ((v(b + 1) - 0.5) * 24.0).round() + (v(b + 2) - 0.5) * 2.0, level: v(b + 3).min(2.5) };
        let x = (v(b + 4) - 0.5) * 2.0;
        if x.abs() > bend.abs() {
            bend = x;
        }
    }
    let time = |i: usize| 0.002 + v(i) * v(i) * 6.0;
    let env = |i: usize| Adsr { a: time(i), d: time(i + 1), s: v(i + 2).min(1.0), r: time(i + 3) };
    let hz = 0.1 * 2f32.powf(v(22) * 8.0);
    let delay = v(21) * v(21) * 4.0;
    let shape = match (v(24) * 10.0).round() as u8 {
        0 => Shape::Sine,
        1 => Shape::Triangle,
        2 => Shape::Saw,
        3 => Shape::Square,
        _ => Shape::Noise,
    };
    let slow = |dest: Dest, amount: f32| Lfo { shape, dest: if amount > 0.004 { dest } else { Dest::None }, amount, hz, reset: false, delay };
    let to_cut = (v(29) - 0.5).abs() * 2.0;
    let filter = if v(33) > 0.5 {
        Filter::Band
    } else if v(32) > 0.75 {
        Filter::High
    } else if v(32) > 0.25 {
        Filter::LowNotch
    } else {
        Filter::Low12
    };
    Set {
        osc,
        width,
        amp: env(35),
        fenv: env(41),
        filter,
        cutoff_hz: 25.0 * 2f32.powf(v(26) * 9.7),
        res: v(27).min(1.0) * 0.95,
        env_oct: (v(28) - 0.5) * 2.0 * 9.0,
        track: v(30).min(1.0),
        lfo: [slow(Dest::Pitch, bend.abs()), slow(Dest::Cutoff, to_cut), Lfo::default()],
        chorus: if full && (p[70] as u32) & 4 != 0 { 1 } else { 0 },
        gain: 0.1 + v(39).min(1.0) * 0.5,
        ..Default::default()
    }
}

/// Settings of the saw synth from its stored values (0 to 1 each, in file order). Placed with fair confidence
/// from how the presets use them and what they are called: 0 level, 2-5 level envelope, 6 transpose (four
/// octaves), 7 fine, 14 unison voices, 15 unison detune, 17 main level, 18 sub phase, 19 sub level, 21 sub
/// restart, 22 sub upside down, 23 square sub, 24 and 25 sync, 27 cutoff, 28 resonance, 29 key follow, then the
/// effects from 47 (see `saw_tail`). Guesses: 32-35 the second envelope, 36 how far it opens the filter. The sub
/// is taken to sit at the main pitch: its octave and detune, the noise, sync itself, the slow wave and the
/// arpeggiator are not placed yet.
///
/// A saw with the same saw upside down and a little later under it is a pulse wave, as wide as the wait; that is
/// how its square sounds are made, and how they are played here.
pub fn saw(p: &[f32]) -> Set {
    let full = p.len() >= 96;
    let v = |i: usize| -> f32 {
        if full {
            return p[i].clamp(0.0, 1.0);
        }
        match i {
            0 | 6 | 7 => 0.5,
            4 | 27 => 1.0,
            _ => 0.0,
        }
    };
    let time = |i: usize| 0.002 + v(i).powi(3) * 8.0;
    let env = |i: usize| Adsr { a: time(i), d: time(i + 1), s: v(i + 2), r: time(i + 3) };
    let tune = ((v(6) - 0.5) * 48.0).round() + (v(7) - 0.5) * 2.0;
    let voices = 1 + (v(14) * 7.0).round() as u32;
    let apart = if voices > 1 { v(15) * 0.3 } else { 0.0 };
    let (main, sub) = if full { (v(17), v(19)) } else { (1.0, 0.0) };
    let mut width = 0.0;
    let osc = if v(23) >= 0.5 {
        [Osc { shape: Shape::Saw, semis: tune + apart, level: main }, Osc { shape: Shape::Square, semis: tune - apart, level: sub }, Osc::default()]
    } else if v(22) >= 0.5 {
        // the part the two saws share turns into a pulse; what is left of the louder one stays a saw
        let both = main.min(sub);
        width = (2.0 * v(18) - 1.0).abs().min(0.95);
        [Osc { shape: Shape::Pulse, semis: tune + apart, level: both }, Osc { shape: Shape::Saw, semis: tune - apart, level: (main - sub).abs() }, Osc::default()]
    } else {
        [Osc { shape: Shape::Saw, semis: tune + apart, level: main }, Osc { shape: Shape::Saw, semis: tune - apart, level: sub }, Osc::default()]
    };
    Set {
        osc,
        width,
        dual: voices > 1,
        amp: env(2),
        fenv: env(32),
        filter: Filter::Low24,
        cutoff_hz: 30.0 * 2f32.powf(v(27) * 9.4),
        res: v(28) * 0.9,
        env_oct: v(36) * 6.0,
        track: v(29),
        gain: 0.1 + v(0) * 0.4,
        ..Default::default()
    }
}

/// The saw synth's effects after its voices: chorus (47 on, depth, rate, mix), phaser (51 on, centre, rate, depth,
/// feedback, mix), echo (57 on, left, right, feedback, sweep, low cut, high cut, mix) and room (65 on, length,
/// high cut, damping, mix). The scales are estimates.
pub fn saw_tail(p: &[f32]) -> crate::tailfx::Set {
    use crate::tailfx;
    if p.len() < 96 {
        return tailfx::Set::default();
    }
    let u = |i: usize| p[i].clamp(0.0, 1.0);
    let on = |i: usize| p[i] >= 0.5;
    let steps = |i: usize| 1.0 + (u(i) * 7.0).round();
    tailfx::Set {
        rack: (on(51) && u(56) > 0.0).then(|| tailfx::Rack { phase: Some([u(54) * u(56), u(55)]), ..Default::default() }),
        delay: (on(57) && u(64) > 0.0).then(|| tailfx::Delay {
            left: steps(58),
            right: steps(59),
            fb: u(60) * 0.95,
            wobble: u(61),
            low_hz: if u(62) < 0.005 { 0.0 } else { 20.0 * 2f32.powf(u(62) * 8.0) },
            high_hz: 200.0 * 100f32.powf(u(63)),
            wet: u(64),
            free: 0.0,
        }),
        chorus: (on(47) && u(50) > 0.0).then(|| tailfx::Chorus { depth: u(48), hz: 0.05 * 100f32.powf(u(49)), wet: u(50) }),
        reverb: (on(65) && u(69) > 0.0).then(|| tailfx::Reverb { secs: 0.3 + u(66) * u(66) * 8.0, high_hz: 800.0 * 20f32.powf(u(67)) * (1.0 - 0.5 * u(68)), wet: u(69) * 0.7 }),
        ..Default::default()
    }
}

pub fn pocket(p: &[f32]) -> Set {
    const PLAIN: [f32; 24] = [0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.68, 0.0, 0.0, 0.37, 0.5, 0.0, 1.0, 1.0, 0.0];
    let v = |i: usize| if p.len() >= 24 { p[i] } else { PLAIN[i] };
    let m = v(5);
    let apart = (m - 0.5) * 48.0;
    let fine = (v(19) - 0.5) * 2.0;
    let o = |shape: Shape, semis: f32, level: f32| Osc { shape, semis: semis + fine, level };
    let none = Osc::default();
    let mut width = 0.0;
    let mut dual = false;
    let mut osc = match (v(4) * 9.0).round() as u8 {
        0 => {
            // a saw with a pulse under it; at either end of the modifier the pulse closes and the saw is alone
            width = (2.0 * m - 1.0).abs();
            [o(Shape::Saw, 0.0, 1.0), o(Shape::Pulse, 0.0, (PI_F * m).sin()), none]
        }
        1 => [o(Shape::Saw, 0.0, 1.0), o(Shape::Saw, apart, 1.0), none],
        2 => {
            width = (2.0 * m - 1.0).abs();
            [o(Shape::Pulse, 0.0, 1.0), o(Shape::Pulse, 0.0, 1.0), none]
        }
        3 => [o(Shape::Pulse, 0.0, 1.0), o(Shape::Saw, apart, 1.0), none],
        4 => [o(Shape::Pulse, 0.0, 1.0), o(Shape::Pulse, apart, 1.0), none],
        5 => {
            // a stack of saws: four here, pulled apart by the modifier
            dual = true;
            [o(Shape::Saw, m * 0.35, 1.0), o(Shape::Saw, -m * 0.35, 1.0), o(Shape::Saw, m * 0.12, 0.7)]
        }
        6 => [o(Shape::Saw, 0.0, 1.0), o(Shape::Sine, 19.02, m), o(Shape::Sine, 28.0, m * 0.5)],
        7 => [o(Shape::Saw, 0.0, 1.0), o(Shape::Saw, 0.0, 1.0), none],
        8 => [o(Shape::Saw, 0.0, 1.0), o(Shape::Saw, 12.0 + m * 0.3, 1.0), o(Shape::Saw, -12.0 - m * 0.3, 1.0)],
        _ => [o(Shape::Saw, 0.0, 1.0), o(Shape::Saw, 7.0 + m * 0.3, 1.0), o(Shape::Saw, 12.0 - m * 0.3, 1.0)],
    };
    if osc[2].level == 0.0 && v(6) > 0.0 {
        osc[2] = Osc { shape: Shape::Noise, semis: 0.0, level: v(6) };
    }
    let time = |i: usize| 0.002 + v(i) * v(i) * 5.0;
    let env = |i: usize| Adsr { a: time(i), d: time(i + 1), s: v(i + 2), r: time(i + 3) };
    Set {
        osc,
        width,
        dual,
        amp: env(20),
        fenv: env(11),
        filter: Filter::Low24,
        cutoff_hz: 30.0 * 2f32.powf(v(7) * 9.5),
        res: v(8) * 0.9,
        env_oct: v(10) * 8.0,
        track: v(9),
        gain: 0.4,
        ..Default::default()
    }
}

/// The six-voice vintage poly's stored values (0 to 1, in file order) as a voice set. The arpeggiator, hold and
/// tempo sync of the slow oscillator are not played.
const ADSR: [&str; 4] = ["Attack", "Decay", "Sustain", "Release"];

fn fill(n: usize, placed: &[(usize, crate::duo::Info)], defs: &[f32]) -> Vec<crate::duo::Info> {
    let mut v: Vec<crate::duo::Info> = (0..n).map(|i| crate::duo::Info { name: "", ctl: crate::duo::Ctl::Kept, def: defs.get(i).copied().unwrap_or(0.0), group: 0 }).collect();
    for (i, info) in placed {
        v[*i] = crate::duo::Info { name: info.name, ctl: info.ctl, def: v[*i].def, group: info.group };
    }
    v
}

const TRIO_GROUPS: [&str; 7] = ["Oscillator 1", "Oscillator 2", "Oscillator 3", "Slow oscillator", "Filter", "Filter envelope", "Level"];
const POCKET_GROUPS: [&str; 4] = ["Oscillators", "Filter", "Filter envelope", "Level envelope"];
const SAW_GROUPS: [&str; 10] = ["Oscillator", "Sub", "Filter", "Filter envelope", "Level envelope", "Chorus", "Phaser", "Echo", "Room", "Master"];

/// What a fresh one of these holds, in file order.
pub fn plain(kind: crate::synth::Kind) -> Vec<f32> {
    use crate::synth::Kind;
    match kind {
        Kind::Trio => (0..71)
            .map(|i| match i {
                0..=20 => [0.5, 0.5, 0.5, 0.6, 0.5, 0.5, 0.1][i % 7],
                26 | 28 | 29 | 39 => 0.5,
                37 => 1.0,
                _ => 0.0,
            })
            .collect(),
        Kind::Pocket => vec![
            0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.68, 0.0, 0.0, 0.374, 0.5, 0.0, 1.0, 1.0, 0.0, 0.0, 0.156, 0.0, 0.0, 0.667, 0.0, 0.0, 1.0, 0.0, 0.498, 0.527, 1.0, 0.0, 0.374, 0.0,
            0.0, 1.0, 0.0,
        ],
        Kind::SawSyn => {
            let mut v = vec![0.0f32; 96];
            for (i, x) in [(0, 0.5), (1, 0.5), (3, 1.0), (4, 1.0), (6, 0.5), (7, 0.5), (15, 0.25), (17, 1.0), (18, 0.5), (27, 1.0), (33, 1.0), (34, 1.0), (35, 1.0), (39, 0.5), (42, 1.0), (48, 0.5), (49, 0.25), (50, 0.3)] {
                v[i] = x;
            }
            for (i, x) in [(52, 0.3), (53, 0.2), (54, 0.7), (56, 0.6), (58, 0.4), (59, 0.4), (60, 0.3), (61, 0.25), (63, 1.0), (64, 0.5), (66, 0.5), (67, 0.5), (68, 0.5), (69, 0.5), (72, 0.5), (74, 1.0), (75, 1.0), (76, 2.0)] {
                v[i] = x;
            }
            for x in &mut v[78..84] {
                *x = 0.5;
            }
            v
        }
        _ => Vec::new(),
    }
}

/// The editing page of the three small analogue synths: the group titles and one entry per stored value.
pub fn page(kind: crate::synth::Kind) -> (&'static [&'static str], Vec<crate::duo::Info>) {
    use crate::duo::Ctl;
    use crate::synth::Kind;
    let e = |name: &'static str, ctl: Ctl, group: u8| crate::duo::Info { name, ctl, def: 0.0, group };
    let k = |name: &'static str, group: u8| e(name, Ctl::Knob, group);
    let mut placed: Vec<(usize, crate::duo::Info)> = Vec::new();
    let adsr = |placed: &mut Vec<(usize, crate::duo::Info)>, at: usize, group: u8| {
        for (j, name) in ADSR.iter().enumerate() {
            placed.push((at + j, k(name, group)));
        }
    };
    let defs = plain(kind);
    match kind {
        Kind::Trio => {
            for o in 0..3 {
                let (b, g) = (o * 7, o as u8);
                placed.push((b + 6, e("Wave", Ctl::Tenths(&["Sine", "Saw", "Pulse", "Triangle", "Noise"]), g)));
                for (j, name) in ["Width", "Coarse", "Fine", "Level", "Slow bend", "Envelope bend"].into_iter().enumerate() {
                    placed.push((b + j, k(name, g)));
                }
            }
            placed.push((24, e("Wave", Ctl::Tenths(&["Sine", "Triangle", "Saw", "Square", "Noise"]), 3)));
            placed.push((22, k("Rate", 3)));
            placed.push((21, k("Delay", 3)));
            for (i, name) in [(26, "Cutoff"), (27, "Emphasis"), (28, "Envelope"), (29, "Slow osc"), (30, "Key follow"), (32, "High pass"), (33, "Band pass")] {
                placed.push((i, k(name, 4)));
            }
            adsr(&mut placed, 41, 5);
            adsr(&mut placed, 35, 6);
            placed.push((39, k("Volume", 6)));
            (&TRIO_GROUPS, fill(71, &placed, &defs))
        }
        Kind::Pocket => {
            const MODES: &[&str] = &["Saw and pulse", "Two saws", "Pulse width", "Pulse and saw", "Two pulses", "Saw stack", "Saw and bells", "Doubled saw", "Octaves", "Fifths"];
            placed.push((4, e("Mode", Ctl::Pick(MODES), 0)));
            placed.push((5, k("Modifier", 0)));
            placed.push((6, k("Noise", 0)));
            placed.push((19, k("Fine", 0)));
            for (i, name) in [(7, "Cutoff"), (8, "Resonance"), (9, "Key follow"), (10, "Envelope")] {
                placed.push((i, k(name, 1)));
            }
            adsr(&mut placed, 11, 2);
            adsr(&mut placed, 20, 3);
            (&POCKET_GROUPS, fill(42, &placed, &defs))
        }
        _ => {
            placed.push((6, k("Transpose", 0)));
            placed.push((7, k("Fine", 0)));
            placed.push((14, e("Voices", Ctl::Pick(&["1", "2", "3", "4", "5", "6", "7", "8"]), 0)));
            placed.push((15, k("Detune", 0)));
            placed.push((17, k("Level", 0)));
            placed.push((19, k("Level", 1)));
            placed.push((18, k("Phase", 1)));
            placed.push((22, e("Upside down", Ctl::Switch, 1)));
            placed.push((23, e("Square", Ctl::Switch, 1)));
            for (i, name) in [(27, "Cutoff"), (28, "Resonance"), (29, "Key follow"), (36, "Envelope")] {
                placed.push((i, k(name, 2)));
            }
            adsr(&mut placed, 32, 3);
            adsr(&mut placed, 2, 4);
            for (at, group, names) in [
                (47usize, 5u8, &["Depth", "Rate", "Mix"][..]),
                (51, 6, &["Centre", "Rate", "Depth", "Feedback", "Mix"][..]),
                (57, 7, &["Left", "Right", "Feedback", "Sweep", "Low cut", "High cut", "Mix"][..]),
                (65, 8, &["Length", "High cut", "Damping", "Mix"][..]),
            ] {
                placed.push((at, e("On", Ctl::Switch, group)));
                for (j, name) in names.iter().enumerate() {
                    placed.push((at + 1 + j, k(name, group)));
                }
            }
            placed.push((0, k("Level", 9)));
            (&SAW_GROUPS, fill(96, &placed, &defs))
        }
    }
}

/// A new block for one of the three, laid out as the stored ones are.
pub fn fresh(kind: crate::synth::Kind) -> Vec<u8> {
    use crate::synth::Kind;
    let p = plain(kind);
    let mut v = Vec::new();
    match kind {
        Kind::Trio => {
            for x in &p[..70] {
                v.extend_from_slice(&(*x as f64).to_le_bytes());
            }
            v.extend_from_slice(&0u32.to_le_bytes());
        }
        Kind::Pocket => {
            let name: Vec<u16> = "New sound".encode_utf16().collect();
            v.extend_from_slice(b"SSLF\0\0\0\0\x99FLS\r\n\x1a\nHEAD");
            v.extend_from_slice(&(6 + name.len() as u32 * 2).to_le_bytes());
            for w in [1u16, 1, name.len() as u16].into_iter().chain(name) {
                v.extend_from_slice(&w.to_le_bytes());
            }
            v.extend_from_slice(b"DATA");
            v.extend_from_slice(&(8 + p.len() as u32 * 4).to_le_bytes());
            v.extend_from_slice(b"0rev\0\0\0\0");
            for x in &p {
                v.extend_from_slice(&x.to_le_bytes());
            }
            let len = v.len() as u32 - 8;
            v[4..8].copy_from_slice(&len.to_le_bytes());
        }
        Kind::SawSyn => {
            v.extend_from_slice(b"SSLF\0\0\0\x001rwS");
            for x in &p {
                v.extend_from_slice(&x.to_le_bytes());
            }
            v.extend_from_slice(&[0xff; 96]);
            v.extend_from_slice(&[0; 128]);
            let len = v.len() as u32 - 8;
            v[4..8].copy_from_slice(&len.to_le_bytes());
        }
        _ => {}
    }
    v
}

/// The length of a beat in seconds, as the player last knew it: tempo-locked rates are worked out from it.
static BEAT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f00_0000);

pub fn set_beat(secs: f32) {
    BEAT.store(secs.clamp(0.05, 6.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn beat() -> f32 {
    f32::from_bits(BEAT.load(std::sync::atomic::Ordering::Relaxed))
}

/// The lengths in beats a tempo-locked rate steps through, slowest first (our estimate of the steps).
const LOCKED: [f32; 9] = [4.0, 2.0, 1.0, 2.0 / 3.0, 0.5, 1.0 / 3.0, 0.25, 1.0 / 6.0, 0.125];

fn locked(v: f32) -> f32 {
    LOCKED[((v.clamp(0.0, 1.0) * 8.0).round() as usize).min(8)]
}

pub const POLY_COUNT: usize = 34;
pub const POLY_GROUPS: [&str; 6] = ["Arpeggio", "Slow oscillator", "Oscillator", "Filters", "Level and envelope", "Chorus and master"];

const fn pk(name: &'static str, def: f32, group: u8) -> crate::duo::Info {
    crate::duo::Info { name, ctl: crate::duo::Ctl::Knob, def, group }
}
const fn ps(name: &'static str, def: f32, group: u8) -> crate::duo::Info {
    crate::duo::Info { name, ctl: crate::duo::Ctl::Switch, def, group }
}
const fn pp(name: &'static str, names: &'static [&'static str], def: f32, group: u8) -> crate::duo::Info {
    crate::duo::Info { name, ctl: crate::duo::Ctl::Pick(names), def, group }
}

pub const POLY_PARAMS: [crate::duo::Info; POLY_COUNT] = [
    ps("On", 0.0, 0),
    pp("Way", &["Up", "Up and down", "Down"], 0.0, 0),
    pp("Range", &["1 octave", "2 octaves", "3 octaves"], 0.0, 0),
    pk("Rate", 0.0, 0),
    ps("Tempo", 0.0, 0),
    pk("Rate", 0.0, 1),
    ps("Tempo", 0.0, 1),
    pk("Delay", 0.0, 1),
    ps("Phase", 0.0, 1),
    pp("Trigger", &["Auto", "Manual"], 0.0, 1),
    pk("Vibrato", 0.0, 2),
    pk("Pulse width", 0.0, 2),
    pp("Width from", &["Slow osc", "Manual", "Envelope"], 0.0, 2),
    ps("Pulse", 0.0, 2),
    ps("Saw", 1.0, 2),
    ps("Sub", 0.0, 2),
    pk("Sub level", 0.0, 2),
    pk("Noise", 0.0, 2),
    pk("Low cut", 0.0, 3),
    pk("Cutoff", 1.0, 3),
    pk("Resonance", 0.0, 3),
    pp("Envelope way", &["Up", "Down"], 0.0, 3),
    pk("Envelope", 0.0, 3),
    pk("Slow osc", 0.0, 3),
    pk("Key follow", 0.0, 3),
    pp("Level by", &["Envelope", "Gate"], 0.0, 4),
    pk("Attack", 0.0, 4),
    pk("Decay", 0.5, 4),
    pk("Sustain", 0.5, 4),
    pk("Release", 0.0, 4),
    pp("Chorus", &["Off", "I", "II", "I + II"], 0.0, 5),
    ps("Hold", 0.0, 5),
    pp("Octave", &["Low", "Middle", "High"], 0.5, 5),
    pk("Volume", 0.5, 5),
];

pub fn poly_defaults() -> Vec<f32> {
    POLY_PARAMS.iter().map(|i| i.def).collect()
}

/// A new block for the vintage poly, laid out as the stored ones are.
pub fn poly_fresh() -> Vec<u8> {
    let name = b"Kepler-1\0";
    let mut v = Vec::with_capacity(178);
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&(name.len() as u32).to_le_bytes());
    v.extend_from_slice(name);
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&(POLY_COUNT as u32).to_le_bytes());
    for i in &POLY_PARAMS {
        v.extend_from_slice(&i.def.to_le_bytes());
    }
    v.extend_from_slice(&[0, 0, 0, 0x80, 0x3f, 0, 0, 0, 0x3f]);
    let len = (v.len() - 12) as u32;
    v[4..8].copy_from_slice(&len.to_le_bytes());
    v
}

/// The vintage poly's own arpeggio, when it is on: the way (1 up, 2 down, 3 up and down), the octaves, and the
/// length of a step in sixteenths.
pub fn poly_arp(p: &[f32]) -> Option<(u8, u8, f32)> {
    if p.len() < POLY_COUNT || p[0] < 0.5 {
        return None;
    }
    let way = [1, 3, 2][crate::duo::pick(p[1], 3)];
    let range = crate::duo::pick(p[2], 3) as u8 + 1;
    let beats = if p[4] >= 0.5 { locked(p[3]) } else { 1.0 / (1.5 * 2f32.powf(p[3] * 4.5)) / beat() };
    Some((way, range, (beats * 4.0).max(0.05)))
}

pub fn poly(p: &[f32]) -> Set {
    let plain = poly_defaults();
    let v = |i: usize| if p.len() >= 34 { p[i] } else { plain[i] };
    let on = |i: usize| v(i) >= 0.5;
    let octave = (v(32) * 2.0).round() * 12.0 - 12.0;
    let hz = if on(6) { 1.0 / (locked(v(5)) * 4.0 * beat()) } else { 0.1 * 2f32.powf(v(5) * 8.2) };
    let delay = v(7) * 3.0;
    // set to manual, the slow oscillator waits for a hand on its button, which a stored song never has
    let auto = !on(9);
    let slow = |dest: Dest, amount: f32| Lfo { shape: Shape::Triangle, dest: if amount > 0.0 && auto { dest } else { Dest::None }, amount, hz, reset: true, delay };
    // where the pulse width comes from: the slow oscillator, the slider itself, or the envelope
    let source = (v(12) * 2.0).round() as u8;
    let env = Adsr { a: 0.0015 * 2f32.powf(v(26) * 11.0), d: 0.002 * 2f32.powf(v(27) * 12.5), s: v(28), r: 0.002 * 2f32.powf(v(29) * 12.5) };
    Set {
        osc: [
            Osc { shape: Shape::Pulse, semis: octave, level: if on(13) { 1.0 } else { 0.0 } },
            Osc { shape: Shape::Saw, semis: octave, level: if on(14) { 1.0 } else { 0.0 } },
            Osc { shape: Shape::Square, semis: octave - 12.0, level: if on(15) { v(16) } else { 0.0 } },
        ],
        width: if source == 1 { v(11) } else { 0.0 },
        width_env: if source == 2 { v(11) } else { 0.0 },
        noise: v(17) * 0.7,
        hpf_hz: if v(18) > 0.01 { 10.0 * 2f32.powf(v(18) * 7.0) } else { 0.0 },
        // gated, the level simply follows the key
        amp: if on(25) { Adsr { a: 0.002, d: 0.1, s: 1.0, r: 0.012 } } else { env },
        fenv: env,
        filter: Filter::Low24,
        cutoff_hz: 12.0 * 2f32.powf(v(19) * 10.5),
        res: v(20),
        env_oct: v(22) * 10.0 * if on(21) { -1.0 } else { 1.0 },
        track: v(24),
        lfo: [slow(Dest::Pitch, v(10) * 0.6), slow(Dest::Cutoff, v(23)), slow(Dest::Width, if source == 0 { v(11) } else { 0.0 })],
        chorus: (v(30) * 3.0).round() as u8,
        gain: 0.25 + v(33) * 0.5,
        ..Default::default()
    }
}

/// The larger vintage poly. Only the first oscillator's section could be placed with any confidence (values 5 to
/// 11: pulse width, its source, the three wave switches, sub level and phase); the second oscillator, cross
/// modulation, filters, envelopes, slow oscillators, modulation table and effects are kept in the block but not
/// read, so the sound has this instrument's wave mix and a plain open shape.
pub fn poly2(p: &[f32]) -> Set {
    let v = |i: usize| p.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 1.0, 1.0, 0.0, 1.0, 0.5][i.min(11)]);
    let on = |i: usize| v(i) >= 0.5;
    let source = (v(6) * 2.0).round() as u8;
    let mut osc = [
        Osc { shape: Shape::Pulse, semis: 0.0, level: if on(7) { 1.0 } else { 0.0 } },
        Osc { shape: Shape::Saw, semis: 0.0, level: if on(8) { 1.0 } else { 0.0 } },
        Osc { shape: Shape::Square, semis: -12.0, level: if on(9) { v(10) } else { 0.0 } },
    ];
    if osc.iter().all(|o| o.level == 0.0) {
        // the second oscillator must be carrying the sound
        osc[1].level = 1.0;
    }
    Set {
        osc,
        width: if source == 1 { v(5) } else { 0.0 },
        dual: true,
        amp: Adsr { a: 0.004, d: 1.2, s: 0.7, r: 0.25 },
        fenv: Adsr { a: 0.004, d: 0.9, s: 0.4, r: 0.25 },
        filter: Filter::Low24,
        cutoff_hz: 900.0,
        res: 0.15,
        env_oct: 3.0,
        track: 0.5,
        lfo: [Lfo { shape: Shape::Triangle, dest: if source == 0 && v(5) > 0.0 { Dest::Width } else { Dest::None }, amount: v(5), hz: 0.8, reset: false, delay: 0.0 }, Lfo::default(), Lfo::default()],
        chorus: 1,
        gain: 0.4,
        ..Default::default()
    }
}

/// The one-oscillator acid bass at a middle setting. Its block is a large tagged file (knobs and a pattern
/// memory) whose places we cannot tell apart from the few stored examples, so every note plays this one sound and
/// the built-in pattern player does not run.
pub fn acid() -> Set {
    Set {
        osc: [Osc { shape: Shape::Saw, semis: -12.0, level: 1.0 }, Osc::default(), Osc::default()],
        amp: Adsr { a: 0.002, d: 1.5, s: 0.0, r: 0.02 },
        fenv: Adsr { a: 0.002, d: 0.25, s: 0.0, r: 0.05 },
        filter: Filter::Low24,
        cutoff_hz: 320.0,
        res: 0.72,
        env_oct: 3.6,
        drive: Some((0.08, 0.9)),
        gain: 0.9,
        ..Default::default()
    }
}

const PI_F: f32 = std::f32::consts::PI;

#[cfg(test)]
mod tests {
    use super::*;

    fn basic() -> Set {
        Set {
            osc: [Osc { shape: Shape::Saw, semis: 0.0, level: 1.0 }, Osc { shape: Shape::Pulse, semis: 0.0, level: 1.0 }, Osc::default()],
            cutoff_hz: 2000.0,
            res: 0.2,
            env_oct: 2.0,
            filter: Filter::Low24,
            gain: 0.5,
            ..Default::default()
        }
    }

    #[test]
    fn sounds_then_dies_after_release() {
        let set = basic();
        let mut v = Voice::start(&set, 44100.0, 60, 1.0, 7);
        let dp = 261.63 / 44100.0;
        let held: f32 = (0..8000).map(|_| v.frame(&set, dp)[0].abs()).fold(0.0, f32::max);
        assert!(held > 0.05 && held < 1.5, "{held}");
        let tail = v.release(&set);
        let n = (tail * 44100.0 * 4.0) as usize;
        let mut last = 0.0f32;
        for i in 0..n {
            let y = v.frame(&set, dp)[0].abs();
            if i > n - 500 {
                last = last.max(y);
            }
        }
        assert!(last < 0.002, "{last}");
    }

    #[test]
    fn the_later_versions_added_settings_are_heard() {
        // 52 settings: everything plain, one saw, filter open
        let mut p = vec![0.0f32; 52];
        (p[1], p[2], p[4], p[5], p[8], p[14], p[22], p[45]) = (64.0, 64.0, 64.0, 64.0, 0.0, 128.0, 512.0, 64.0);
        let dp = 261.63 / 44100.0;
        let peak = |p: &[f32], vel: f32| {
            let set = buzz(p);
            let mut v = Voice::start(&set, 44100.0, 60, vel, 7);
            (0..6000).map(|_| v.frame(&set, dp)[0].abs()).fold(0.0, f32::max)
        };
        // with no hit-strength share, soft and hard are as loud; with all of it, they are not
        assert!((peak(&p, 0.3) - peak(&p, 1.0)).abs() < 1e-3);
        p[50] = 128.0;
        assert!(peak(&p, 0.3) < peak(&p, 1.0) * 0.5);
        // the rise-and-fall envelope can take the first oscillator's level down to nothing at its top
        let loud = peak(&p, 1.0);
        (p[43], p[44], p[45], p[48]) = (0.0, 128.0, 0.0, 1.0);
        assert!(peak(&p, 1.0) < loud * 0.5, "{} {}", peak(&p, 1.0), loud);
        // started together, two notes are the same sample for sample; drifting, they are not
        let take = |p: &[f32], seed: u32| {
            let set = buzz(p);
            let mut v = Voice::start(&set, 44100.0, 60, 1.0, seed);
            (0..500).map(|_| v.frame(&set, dp)[0]).collect::<Vec<f32>>()
        };
        p[48] = 0.0;
        assert_eq!(take(&p, 3), take(&p, 9));
        p[42] = 1.0;
        assert_ne!(take(&p, 3), take(&p, 9));
    }

    #[test]
    fn every_filter_and_shape_stays_finite() {
        for filter in [Filter::Low12, Filter::Low24, Filter::LowNotch, Filter::TwoNotch, Filter::Band, Filter::High] {
            for shape in [Shape::Saw, Shape::Pulse, Shape::Sine, Shape::Triangle, Shape::Noise] {
                let mut set = basic();
                set.filter = filter;
                set.res = 1.0;
                set.osc[0].shape = shape;
                set.ring = shape == Shape::Sine;
                set.fm = 0.7;
                set.dual = true;
                set.drive = Some((0.8, 0.5));
                set.lfo = [Lfo { shape: Shape::Noise, dest: Dest::Cutoff, amount: 1.0, hz: 9.0, reset: true, delay: 0.2 }, Lfo { shape: Shape::Sine, dest: Dest::Pitch1, amount: 1.0, hz: 5.0, reset: false, delay: 0.0 }, Lfo::default()];
                set.noise = 0.3;
                set.hpf_hz = 200.0;
                set.chorus = 1 + (shape as u8) % 3;
                set.width_env = 0.5;
                let mut v = Voice::start(&set, 44100.0, 72, 1.0, 3);
                for _ in 0..20000 {
                    let y = v.frame(&set, 0.02)[0];
                    assert!(y.is_finite() && y.abs() < 8.0, "{filter:?} {shape:?} {y}");
                }
            }
        }
    }
}
