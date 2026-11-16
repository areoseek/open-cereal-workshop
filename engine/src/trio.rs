//! Analog Trio: three oscillators, a slow oscillator, a two-stage filter and two envelopes.
//!
//! Every law here was measured from renders of the reference at 44.1 kHz, one control at a time. The numbers in
//! the stored block are 0..=1 knob places followed by on/off words (see `synth::trio_values`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Full level of one oscillator at level 1, velocity 100 and volume 1, against full scale.
/// (The measured figure is behind a channel at its usual 78 % volume.)
const FULL: f32 = 4862.0 / 0.507_812_5 / 32768.0 / 0.549_066 * 0.9967;
/// Where a fresh oscillator starts in its cycle.
const START: f64 = 0.39965;
/// The level follows its envelope through a one-pole of this much per frame.
const EASE: f32 = 0.0303;
/// Frames in a block: the slow oscillator and the pitch bends move once a block.
const BLOCK: u32 = 32;

/// A knob's place as the instrument holds it: one of 256 steps.
fn step(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 255.0 + 1e-4).floor()
}

/// A knob read through its steps, 0..=1.
fn held(v: f32) -> f32 {
    step(v) / 255.0
}

/// Where a percent knob stands on its own, finer grid. (Measured in steps of 1/256: a quarter reads 24, a half
/// 49, five eighths of the way +24 hundredths, full 100; the grid's size is the simplest that gives those.)
fn fine(v: f32) -> f64 {
    (v.clamp(0.0, 1.0) as f64 * 65535.0 + 1e-6).floor()
}

/// The panel shows whole percents and the sound follows what is shown.
fn cents(v: f32) -> f32 {
    let c = v as f64 * 100.0;
    (c.trunc() / 100.0) as f32
}

fn shown(v: f32) -> f32 {
    ((fine(v) * 100.0 / 65535.0).floor() / 100.0) as f32
}

/// The same for a knob with its zero in the middle, -1..=1.
fn shown_mid(v: f32) -> f32 {
    ((fine(v) * 200.0 / 65535.0 - 100.0).trunc() / 100.0) as f32
}

#[derive(Debug, Clone, Copy, Default)]
struct OscSet {
    on: bool,
    up1: bool,
    up2: bool,
    warm: bool,
    width: f32,
    /// How far the sine leans to its peaked form.
    shape: f32,
    /// Knob place of the coarse tuning, which also tunes the noise.
    coarse: f32,
    /// Semitones from tuning, without the bends.
    /// Whole semitones off the key.
    whole: f32,
    semis: f32,
    level: f32,
    slow: f32,
    bend: f32,
    wave: u8,
}

#[derive(Debug, Clone, Copy, Default)]
struct Env {
    a: f32,
    d: f32,
    s: f32,
    r: f32,
}

#[derive(Debug, Clone)]
pub struct Set {
    osc: [OscSet; 3],
    slow_on: bool,
    slow_fresh: bool,
    slow_delay: f32,
    slow_rate: f32,
    slow_wave: u8,
    filter: bool,
    cut: f32,
    damp: f32,
    cut_env: f32,
    cut_slow: f32,
    follow: f32,
    high: f32,
    band: f32,
    track: bool,
    amp: Env,
    fenv: Env,
    gain: f32,
    ring: bool,
    /// Tells one channel's settings from another's, for what carries from note to note.
    mark: u64,
}

pub fn set(p: &[f32]) -> Set {
    let u = |i: usize| p.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
    let on = |i: usize| u(i) >= 0.5;
    let wave = |i: usize| ((u(i) * 10.0).round() as i32).clamp(0, 5) as u8;
    let mut osc = [OscSet::default(); 3];
    for (o, s) in osc.iter_mut().enumerate() {
        let (b, f) = (o * 7, 71 + o * 4);
        let coarse = ((u(b + 1).clamp(0.0, 1.0) - 0.5) * 24.0 + if u(b + 1) >= 0.5 { 1e-3 } else { -1e-3 }).trunc();
        let fine = shown_mid(u(b + 2));
        *s = OscSet {
            on: on(f),
            up1: on(f + 1),
            up2: on(f + 2),
            warm: on(f + 3),
            width: u(b).clamp(0.0, 1.0),
            shape: (4.0 + (step(u(b)) * 96.0 / 255.0 + 1e-4).floor()) / 96.0,
            coarse: u(b + 1).clamp(0.0, 1.0),
            whole: coarse,
            semis: fine + (1.0 / 3.0 + o as f32) / 100.0,
            level: shown(u(b + 3)).powi(2),
            slow: shown_mid(u(b + 4)) * 12.0,
            bend: shown_mid(u(b + 5)) * 12.0,
            wave: wave(b + 6),
        };
    }
    let env = |i: usize| Env { a: u(i).clamp(0.0, 1.0), d: u(i + 1).clamp(0.0, 1.0), s: held(u(i + 2)), r: u(i + 3).clamp(0.0, 1.0) };
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for i in 0..90 {
        h = (h ^ u(i).to_bits() as u64).wrapping_mul(0x100_0000_01b3);
    }
    Set {
        osc,
        slow_on: on(83),
        slow_fresh: on(84),
        slow_delay: shown(u(21)),
        slow_rate: u(22).clamp(0.0, 1.0),
        slow_wave: wave(24),
        filter: on(85),
        cut: held(u(26)),
        damp: 0.1 + (1.0 - held(u(27))).powi(2),
        cut_env: (u(28).clamp(0.0, 1.0) * 255.0 - 127.5).trunc() / 128.0,
        cut_slow: (u(29).clamp(0.0, 1.0) * 255.0 - 127.5).trunc() / 128.0,
        follow: if step(u(30)) > 0.0 { (step(u(30)) + 1.0) / 256.0 } else { 0.0 },
        high: step(u(32)) / 255.0,
        band: step(u(33)) / 255.0,
        track: on(87),
        amp: env(35),
        fenv: env(41),
        gain: FULL * u(39).max(0.0) * if u(70) != 0.0 { 4.0 / 3.0 } else { 1.0 },
        ring: on(88),
        mark: h,
    }
}

/// What one channel's notes hand on to the next: where the oscillators and the slow oscillator stand.
#[derive(Debug, Default)]
struct Carry {
    ph: [AtomicU64; 3],
    slow: AtomicU64,
    used: AtomicU64,
}

fn carry(mark: u64) -> Arc<Carry> {
    static C: OnceLock<Mutex<HashMap<u64, Arc<Carry>>>> = OnceLock::new();
    let mut m = C.get_or_init(Default::default).lock().unwrap();
    if m.len() > 4096 {
        m.clear();
    }
    m.entry(mark).or_default().clone()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    Decay,
    Hold,
    Release,
    Done,
}

#[derive(Debug, Clone, Copy)]
struct EnvRun {
    stage: Stage,
    x: f64,
    lin: bool,
    /// Where the release set off from: it takes its whole time from any height.
    from: f64,
}

impl EnvRun {
    /// The level envelope bends its fall (a square); the filter's runs straight.
    fn shape(&self) -> f32 {
        (if self.lin { self.x } else { self.x * self.x }) as f32
    }

    /// One frame on; gives the envelope's value.
    fn step(&mut self, e: &Env, sr: f32) -> f32 {
        // (measured: a stage lasts a whole number of milliseconds, the knob read on its fine grid)
        let ms = |v: f32, p: i32| (8000.0 * (fine(v) / 65535.0).powi(p)).floor() * sr as f64 / 1000.0;
        let s = e.s as f64;
        match self.stage {
            Stage::Attack => {
                let t = ms(e.a, 3);
                if t < 1.0 && self.lin {
                    // (measured: with no attack time the filter's envelope never shows its top)
                    self.x = 1.0;
                    self.stage = Stage::Decay;
                    return self.step(e, sr);
                }
                self.x = if t < 1.0 { 1.0 } else { self.x + 1.0 / t };
                if self.x >= 1.0 {
                    self.x = 1.0;
                    self.stage = Stage::Decay;
                }
                self.x as f32
            }
            Stage::Decay => {
                let t = ms(e.d, 2);
                self.x = if t < 1.0 { s } else { self.x - (1.0 - s) / t };
                if self.x <= s {
                    self.x = s;
                    self.stage = Stage::Hold;
                }
                self.shape()
            }
            Stage::Hold => {
                self.x = s;
                self.shape()
            }
            Stage::Release => {
                let t = ms(e.r, 2).max(94.0);
                self.x -= self.from / t;
                if self.x <= 0.0 {
                    self.x = 0.0;
                    self.stage = Stage::Done;
                }
                self.shape()
            }
            Stage::Done => 0.0,
        }
    }

    fn release(&mut self) {
        // (measured: let go while still rising, the value is kept and read as the later stages read it)
        if self.stage != Stage::Done {
            self.from = self.x;
            self.stage = Stage::Release;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    key: f32,
    vel: f32,
    ph: [f64; 3],
    warm: [f64; 3],
    /// Noise filter state per oscillator.
    noise: [[f32; 2]; 3],
    rng: u32,
    smooth: f32,
    amp: EnvRun,
    /// Frames until a let-go key takes hold.
    fenv: EnvRun,
    level: f32,
    /// The slow oscillator carries on from the note before.
    slow_on_from: bool,
    /// How far past the voice's first frame its note's exact place lies.
    frac: f64,
    slow_half: i64,
    acc: f32,
    /// How far the slow oscillator has faded in.
    fade: f32,
    /// How many samples the slow oscillator has been stepped, and the length of its half turn.
    slow_at: u32,
    slow_len: u32,
    acc_n: i64,
    acc_k: u32,
    slow_held: f32,
    slow_v: f32,
    bend_v: f32,
    cut_v: f32,
    slow_c: f32,
    n: u32,
    /// A first note: the oscillators still have their head start to take.
    lead: bool,
    flt: [[f32; 2]; 2],
    after: u32,
    done_at: Option<u32>,
    carry: Arc<Carry>,
}

fn tri(ph: f64) -> f32 {
    let p = ph - ph.floor();
    (if p < 0.25 { 4.0 * p } else if p < 0.75 { 2.0 - 4.0 * p } else { 4.0 * p - 4.0 }) as f32
}

/// The pulse, one cycle in 4096 points for each width from 2 % to 98 % (measured point by point): a square
/// that rests at nothing for the last nine points of the cycle, rounded by a one-pole low-pass that starts the
/// cycle already one step into the rise.
fn pulses() -> &'static Vec<Vec<f32>> {
    static T: std::sync::OnceLock<Vec<Vec<f32>>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        (0..97)
            .map(|w| {
                // (a width that lands on a whole point falls one point early: 25 % measured at 1023)
                let fall = ((2 + w) * 4096 - 1) / 100;
                let (mut y, mut was) = (0.175f32, 1.0f32);
                let mut t: Vec<f32> = (0..4096)
                    .map(|i| {
                        let x = if i < fall { 1.0 } else if i < 4087 { -1.0 } else { 0.0 };
                        y = 0.65 * y + 0.175 * (x + was);
                        was = x;
                        y
                    })
                    .collect();
                // the very last point sits a little short of where the decay would put it
                t[4095] = t[4094] * 0.779;
                t
            })
            .collect()
    })
}

/// One of the pitched waves at `ph` (0..1), as it stands at one instant.
fn raw(kind: u8, width: f32, shape: f32, ph: f64) -> f32 {
    let p = ph - ph.floor();
    match kind {
        0 => tri(p),
        2 => {
            // the width knob moves in 256 steps over 2 % to 98 %, whole per cent only
            let duty = (step(width) as f64 * 96.0 / 255.0).trunc() as usize;
            let t = &pulses()[duty.min(96)];
            0.6503 * t[(p * 4096.0) as usize % 4096]
        }
        3 => {
            let u = ((p * 2.0 + 0.35037) * std::f64::consts::TAU).cos() as f32;
            (1.0 - shape) * u + shape * u.powi(4)
        }
        _ => (std::f64::consts::PI * p).cos() as f32,
    }
}

/// The wave over one frame: the oscillators run at twice the rate and each frame is the mean of its two steps.
fn wave(s: &OscSet, ph: f64, inc: f64) -> f32 {
    0.5 * (raw(s.wave, s.width, s.shape, ph - inc * 0.25) + raw(s.wave, s.width, s.shape, ph + inc * 0.25))
}

/// Where each oscillator stands when a channel's first note begins. It goes by which parts are switched on,
/// not by pitch, level or time (measured case by case; the cases not yet measured take the nearest known one).
fn starts(set: &Set) -> [f64; 3] {
    let on: Vec<usize> = (0..3).filter(|&o| set.osc[o].on).collect();
    let hiss = on.iter().any(|&o| set.osc[o].wave == 4);
    let row: &[f64] = match (on.len(), set.slow_on) {
        (2, false) if hiss => &[0.038, 0.038],
        (2, false) => &[0.1055, 0.2405],
        (2, true) => &[0.456, 0.446],
        (3, false) => &[0.1755, 0.456, 0.446],
        (3, true) => &[0.461, 0.456, 0.446],
        (_, true) => &[0.4225],
        _ => &[0.401],
    };
    let mut out = [START; 3];
    for (i, &o) in on.iter().enumerate() {
        out[o] = row[i.min(row.len() - 1)] + START - 0.401;
    }
    out
}

impl Voice {
    pub fn start(set: &Set, sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let carry = carry(set.mark);
        let had = carry.used.swap(1, Ordering::Relaxed) != 0;
        let first = starts(set);
        let take = |o: usize| if had { f64::from_bits(carry.ph[o].load(Ordering::Relaxed)) } else { first[o] };
        let ph = [take(0), take(1), take(2)];
        let slow_on_from = had && !set.slow_fresh;
        let was = carry.slow.load(Ordering::Relaxed);
        let mut rng = seed | 1;
        rng ^= rng << 13;
        Self {
            sr,
            key: key as f32,
            // the host has already scaled the note by its own velocity curve, (11^u - 1) / 10 with u 1 at velocity
            // 100; this instrument's is (21^m - 1) / 20 with m 1 at velocity 127 (measured), so the difference is
            // made up here, with velocity 100 as the reference
            vel: {
                let g = vel.max(1e-6);
                let m = (g * 10.0 + 1.0).ln() / 11f32.ln() * 100.0 / 127.0;
                let own = |m: f32| (21f32.powf(m) - 1.0) / 20.0;
                own(m) / own(100.0 / 127.0) / g
            },
            ph,
            warm: ph,
            noise: [[0.0; 2]; 3],
            rng,
            smooth: 0.0,
            amp: EnvRun { stage: Stage::Attack, x: 0.0, lin: false, from: 1.0 },
            fenv: EnvRun { stage: Stage::Attack, x: 0.0, lin: true, from: 1.0 },
            level: 0.0,
            slow_on_from,
            frac: crate::minis::note_frac() as f64,
            slow_half: 0,
            acc: 1.0,
            fade: 0.0,
            slow_at: 0,
            slow_len: 0,
            acc_n: if slow_on_from { (was & 1) as i64 } else { 0 },
            acc_k: if slow_on_from { (was >> 1) as u32 } else { 0 },
            slow_held: 0.0,
            slow_v: 0.0,
            bend_v: 0.0,
            cut_v: 0.0,
            slow_c: 0.0,
            n: 0,
            lead: !had,
            flt: [[0.0; 2]; 2],
            after: 0,
            done_at: None,
            carry,
        }
    }

    /// Seconds of sound left once the key is let go.
    pub fn release(&mut self, set: &Set) -> f32 {
        self.amp.release();
        self.fenv.release();
        8.0 * set.amp.r.powi(2) + (crate::minis::tick() + 400.0) / self.sr
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / 8_388_608.0 - 1.0
    }

    /// The slow oscillator's value over one block, already faded in. It moves once a block (measured: half a turn
    /// lasts about 176930 samples times the cube of what is left of the knob, never over four seconds or under two blocks).
    fn slow(&mut self, set: &Set, b: u32) -> f32 {
        if !set.slow_on {
            self.slow_c = 0.0;
            return 0.0;
        }
        // half a turn is a whole number of samples, never under two blocks
        let half = ((176930.3 * (1.0 - set.slow_rate as f64).powi(3)).floor().clamp(64.0, 176400.0) * self.sr as f64 / 44100.0).round().max(1.0);
        // (measured: each half turn is counted down from one in single precision, a sample at a time, and set back
        // to one when its samples are up, so the small rounding of every step shows as a slightly bent rate)
        let d1 = (1.0 / half) as f32;
        let steps = if b > 0 { BLOCK } else { 1 };
        if b == 0 && self.slow_on_from {
            // (measured: a note that carries on takes over the count of samples, and starts its counter afresh
            // from where that many whole steps would have left it)
            let mut k = self.acc_k as i64;
            while k >= half as i64 {
                k -= half as i64;
                self.acc_n += 1;
            }
            self.acc_k = k.max(0) as u32;
            self.acc = (1.0 - self.acc_k as f64 / half) as f32;
            self.acc_k += 1;
        }
        self.slow_at = b * BLOCK + 1;
        self.slow_len = half as u32;
        for _ in 0..steps {
            self.acc = (self.acc - d1).max(0.0);
            self.acc_k += 1;
            if self.acc_k >= half as u32 {
                self.acc = 1.0;
                self.acc_k = 0;
                self.acc_n += 1;
            }
        }
        let at = (self.acc_n as f64 + (1.0 - self.acc as f64).min(1.0 - 1e-9)) / 2.0;
        let p = at.fract();
        let v = match set.slow_wave {
            1 | 5 => 1.0 - (2.0 * p).fract() as f32,
            2 => if p < 0.5 { 0.0 } else { 1.0 },
            4 => {
                let half = (at * 2.0).floor() as i64;
                if b == 0 || half != self.slow_half {
                    self.slow_held = self.rand() * 0.5 + 0.5;
                }
                self.slow_half = half;
                self.slow_held
            }
            // (measured: a triangle value landing on a whole cent reads as the cent below)
            _ => -tri(p) * (1.0 - 1e-6),
        };
        let fade = 8.0 * self.sr * set.slow_delay;
        // (measured: the fade is counted up in single precision a sample at a time, and shows that rounding)
        if b > 0 && fade >= 1.0 {
            let d = 1.0 / fade;
            for _ in 0..BLOCK {
                self.fade = (self.fade + d).min(1.0);
            }
        }
        let depth = if fade >= 1.0 { self.fade } else { 1.0 };
        self.carry.slow.store((self.acc_k as u64) << 1 | (self.acc_n as u64 & 1), Ordering::Relaxed);
        // the filter hears the triangle from its foot, up and back once a turn
        self.slow_c = depth * if matches!(set.slow_wave, 1 | 2 | 4 | 5) { v } else { 1.0 - (1.0 - 2.0 * p as f32).abs() };
        v * depth
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        if self.after > 0 {
            return [0.0; 2];
        }
        let v = self.amp.step(&set.amp, self.sr);
        let fv = self.fenv.step(&set.fenv, self.sr);
        self.level += (v * v - self.level) * EASE;
        if self.amp.stage == Stage::Done && self.done_at.is_none() {
            // (measured: the voice is cleared away on the first song tick after its release has run out)
            let t = crate::minis::tick() as f64;
            self.done_at = Some(((((self.n as f64 - self.frac) / t).ceil() * t + self.frac + 0.5).floor()).max(0.0) as u32);
        }
        if self.done_at.is_some_and(|d| self.n >= d) {
            self.after = 1;
            if self.slow_len > 0 {
                // the slow oscillator's count runs to the voice's last sample
                let mut k = self.acc_k as u64 + self.n.saturating_sub(self.slow_at) as u64;
                let mut n = self.acc_n as u64;
                while k >= self.slow_len as u64 {
                    k -= self.slow_len as u64;
                    n += 1;
                }
                self.carry.slow.store(k << 1 | (n & 1), Ordering::Relaxed);
            }
            return [0.0; 2];
        }
        let turn = self.n % BLOCK == 0;
        if turn {
            self.slow_v = self.slow(set, self.n / BLOCK);
            self.bend_v = v;
        }
        self.n = self.n.wrapping_add(1);
        let slow = self.slow_v;
        let mut tone = [0.0f32; 3];
        let mut hiss = 0.0;
        for o in 0..3 {
            let s = set.osc[o];
            if !s.on {
                continue;
            }
            if s.wave == 4 {
                let f = (0.325 + 0.45 * s.coarse).powi(2);
                let x = self.rand() * 8.0;
                let [low, band] = &mut self.noise[o];
                *low += f * *band;
                let high = x - *low - *band;
                *band += f * high;
                hiss += *band * s.level;
                continue;
            }
            // The slow oscillator's and the envelope's pitch offsets are each held in whole cents,
            // cut towards zero.
            // (measured: the lowest key plays no lower than the one above the very bottom)
            let semis = s.whole.max(1.0 - self.key) + s.semis + cents(s.bend * self.bend_v) + cents(s.slow * slow);
            let inc = (dphase as f64 * (1.0 - 1.17e-6) * 2f64.powf(semis as f64 / 12.0)).min(0.49);
            if self.lead {
                self.ph[o] += inc * 0.25;
            }
            let p = self.ph[o];
            let mut y = wave(&s, p, inc);
            if s.up1 {
                y += wave(&s, p * 2.0, inc * 2.0);
            }
            if s.up2 {
                y += wave(&s, p * 4.0, inc * 4.0);
            }
            if s.warm {
                let winc = inc * 1.011_629;
                y += wave(&s, self.warm[o], winc);
                self.warm[o] = (self.warm[o] + winc).fract();
            }
            // kept over four cycles so the octave copies stay whole
            self.ph[o] = (p + inc) % 4.0;
            self.carry.ph[o].store(self.ph[o].to_bits(), Ordering::Relaxed);
            tone[o] = y * s.level;
        }
        self.lead = false;
        let mixed = if set.ring { 2.0 * tone[0] * tone[1] + tone[2] } else { tone[0] + tone[1] + tone[2] };
        self.smooth = self.smooth / 3.0 + mixed * 2.0 / 3.0;
        let mut x = self.smooth + hiss;
        if set.filter {
            let env = if set.track { v } else { fv };
            let now = (set.cut + set.cut_env * env + set.cut_slow * self.slow_c + set.follow * (self.key - 12.0) / 127.0).clamp(0.0, 1.0);
            if turn {
                self.cut_v = now;
            }
            let c = self.cut_v;
            let stage = |st: &mut [f32; 2], f: f32, x: f32| {
                st[0] += f * st[1];
                let high = x - st[0] - set.damp * st[1];
                st[1] += f * high;
                [st[0], high, st[1]]
            };
            // two stages in a row; each hands on its own blend of low, high and band, the knobs turning
            // a quarter circle so the blend keeps its power
            let (hs, hc) = (set.high * std::f32::consts::FRAC_PI_2).sin_cos();
            let (bs, bc) = (set.band * std::f32::consts::FRAC_PI_2).sin_cos();
            let blend = |o: [f32; 3]| bc * (hc * o[0] + hs * o[1]) + bs * o[2];
            let first = blend(stage(&mut self.flt[0], c * c, x));
            x = blend(stage(&mut self.flt[1], c, first));
        }
        let out = x * self.level * self.vel * set.gain;
        [out, out]
    }
}
