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
    pub lfo: [Lfo; 2],
    /// Drive (0 to 1) and tone (0 dark to 1 bright), when the drive stage is on.
    pub drive: Option<(f32, f32)>,
    pub gain: f32,
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
    lfo: [f32; 2],
    lfo_hold: [f32; 2],
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
        v.t = 0;
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
        let mut cut_oct = f * set.env_oct + (self.key - 60.0) / 12.0 * set.track;
        let mut level = 1.0;
        self.bend = [0.0; 2];
        self.width = set.width;
        self.mix = 0.5;
        for (k, l) in set.lfo.iter().enumerate() {
            let before = self.lfo[k];
            self.lfo[k] = (before + l.hz * dt).fract();
            let ph = self.lfo[k];
            let x = match l.shape {
                Shape::Saw => 1.0 - 2.0 * ph,
                Shape::Pulse => {
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
            } * l.amount;
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
        let (la, lb) = (set.osc[0].level * (1.0 - self.mix) * 2.0, set.osc[1].level * self.mix * 2.0);
        let mut x = if set.ring { a * b * (la + lb) * 0.5 } else { a * la + b * lb };
        if set.osc[2].level > 0.0 {
            x += wave(set.osc[2].shape, self.ph[2], d3, width, &mut self.seed) * set.osc[2].level;
            self.ph[2] = (self.ph[2] + d3).fract();
        }
        x *= 0.5;
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
        let y = y * set.gain * self.vel;
        [y, y]
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
    let lfo = |i: usize, dests: [Dest; 3]| Lfo {
        shape: shape(i),
        dest: if v(i + 2) > 0.0 { dests[(v(i + 1) as usize).min(2)] } else { Dest::None },
        amount: u(i + 2),
        hz: 0.05 * 2f32.powf(u(i + 3) * 9.0),
        reset: v(i + 5) != 0.0,
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
        lfo: [lfo(25, [Dest::Pitch, Dest::Cutoff, Dest::Width]), lfo(31, [Dest::Pitch1, Dest::Mix, Dest::Level])],
        drive: (v(37) != 0.0).then(|| (u(38), u(39))),
        gain: 0.5,
    }
}

/// The small touch-screen synth's stored values as a voice set. The places of the values were worked out from
/// how they move between presets: wave choice, modifier and noise at 4 to 6, the filter and its envelope at 7 to
/// 14, fine tune at 19, the level envelope at 20 to 23. The rest (play mode, slow oscillator, effects) is not read.
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
                set.lfo = [Lfo { shape: Shape::Noise, dest: Dest::Cutoff, amount: 1.0, hz: 9.0, reset: true }, Lfo { shape: Shape::Sine, dest: Dest::Pitch1, amount: 1.0, hz: 5.0, reset: false }];
                let mut v = Voice::start(&set, 44100.0, 72, 1.0, 3);
                for _ in 0..20000 {
                    let y = v.frame(&set, 0.02)[0];
                    assert!(y.is_finite() && y.abs() < 8.0, "{filter:?} {shape:?} {y}");
                }
            }
        }
    }
}
