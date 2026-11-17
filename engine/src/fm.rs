//! Six-operator instrument: operators that modulate each other's phase or level through a matrix, three filter
//! modules with a shaping curve, and a set of envelopes, slow oscillators and mappings on every module.
//!
//! The stored block is a version number followed by a packed stream. The layout below is the one written by
//! recent versions (23 to 25) and was worked out by comparing many project files; every scale in this file is an
//! estimate until measured against the reference (docs/PARITY.md, "Instruments"). The block is never rewritten:
//! what we do not run (the built-in chorus, delays, reverb and equaliser, unison sub level, several switches)
//! stays in the block untouched.

use std::f32::consts::{PI, TAU};
use std::sync::{Arc, OnceLock};

const TN: usize = 16384;
/// Frames between two looks at the envelopes.
const TICK: u32 = 16;
/// Phase swing, in cycles, of a full-scale modulator at 100 %.
const DEPTH: f32 = 2.0;
/// How much weaker an operator's feed into itself is than a feed from another.
const SELF_FEED: f32 = 0.0687;
/// How steeply the knobs of the grid rise: the amount is (GRID^x - 1) / (GRID - 1) for a knob at x.
const GRID: f32 = 11.16;

fn grid(x: f32) -> f32 {
    (GRID.powf(x.abs()) - 1.0) / (GRID - 1.0) * x.signum()
}

/// Left and right gains of a pan knob at -1..1: equal power, 1 and 1 in the middle (measured at five settings).
fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    (a.cos() * std::f32::consts::SQRT_2, a.sin() * std::f32::consts::SQRT_2)
}

/// Where a filter's cutoff knob at 0..1 puts the corner of the low-pass kinds (fitted to renders at nine
/// settings, within 2 %).
fn cutoff_hz(c: f32) -> f32 {
    71.0 + 7449.0 * (29.6f32.powf(c) - 1.0) / 28.6
}

/// The same knob for the second low-pass and its high-pass twin (fitted to hiss through one stage at nine
/// settings, within 1 %).
fn second_hz(c: f32) -> f32 {
    141.28 * 82.8f32.powf(c) - 63.83
}

/// The same knob for the wide low-pass, which opens far past the top of hearing.
fn wide_hz(c: f32) -> f32 {
    28.34 * 1679f32.powf(c) + 104.3
}

/// The same knob for the band, high-pass, shelf and peak kinds, which reach much lower and higher.
fn band_hz(c: f32) -> f32 {
    17.66 * 1496f32.powf(c) - 13.2
}

/// What the resonance knob does, read off renders at every eighth of its travel and joined smoothly.
/// Damping of the ringing low-pass (1 = just short of a peak):
const DAMP: [f32; 9] = [1.0, 0.943, 0.82, 0.626, 0.412, 0.2164, 0.1035, 0.04615, 0.0202];
/// damping of the low-pass and high-pass that start out far from ringing:
const DAMP_WIDE: [f32; 9] = [9.94, 7.12, 4.11, 1.97, 0.837, 0.337, 0.1325, 0.0515, 0.0209];
/// width of the band-pass and notch in octaves:
const WIDTH: [f32; 9] = [0.0104, 0.1036, 0.27, 0.567, 1.094, 2.02, 3.72, 6.67, 12.0];
/// sharpness of the peak:
const PEAK_Q: [f32; 9] = [137.9, 20.0, 9.7, 5.8, 2.267, 0.91, 0.382, 0.178, 0.083];
/// steepness of the shelves:
const SHELF_SLOPE: [f32; 9] = [1.0, 0.829, 0.678, 0.546, 0.43, 0.331, 0.244, 0.167, 0.101];

/// Width in Hz of the first low-pass with its second switch on, over the resonance knob.
fn flat_width(r: f32) -> f32 {
    const W: [f32; 5] = [8370.0, 5000.0, 2753.0, 1170.0, 270.0];
    let x = r.clamp(0.0, 1.0) * 4.0;
    let i = (x as usize).min(3);
    let f = x - i as f32;
    (W[i].ln() * (1.0 - f) + W[i + 1].ln() * f).exp()
}

fn table(t: &[f32; 9], r: f32) -> f32 {
    let x = r.clamp(0.0, 1.0) * 8.0;
    let i = (x as usize).min(7);
    let f = x - i as f32;
    (t[i].ln() * (1.0 - f) + t[i + 1].ln() * f).exp()
}
/// Level of one full-scale operator sent straight out.
const LEVEL: f32 = std::f32::consts::FRAC_1_SQRT_2;

pub const OP_TARGETS: [&str; 6] = ["Pan", "Volume", "Modulation", "Pitch", "Phase", "Damping"];
pub const FILTER_TARGETS: [&str; 8] = ["Pan", "Volume", "Cutoff", "Resonance", "Low", "Band", "High", "Shaper mix"];
pub const MODULES: [&str; 9] = ["Op 1", "Op 2", "Op 3", "Op 4", "Op 5", "Op 6", "Filter 1", "Filter 2", "Filter 3"];

fn sine() -> &'static [f32] {
    static T: OnceLock<Vec<f32>> = OnceLock::new();
    T.get_or_init(|| (0..=TN).map(|i| (i as f32 / TN as f32 * TAU).sin()).collect())
}

/// One value of hiss. Measured: bell-shaped, even across the band, as strong as an even spread over -1..1,
/// and the same run of values on every note (so a plucked operator fed by hiss rings the same each time).
fn hiss(seed: &mut u32) -> f32 {
    let mut sum = 0.0;
    for _ in 0..12 {
        sum += rnd(seed);
    }
    sum * 0.288_675
}

/// Points in the kept cycle of a plucked operator.
const STR: usize = 1024;

/// Wears a plucked operator's kept cycle down by `dt` seconds of going round at `hz`, with the damping
/// setting at `d` (1 is the loosest). Measured on long notes over a spread of keys and settings: harmonic h
/// falls by k * h * h dB a second, where k grows with the cube of the pitch (the same loss on every trip
/// round) and as (1 - d) / d^2 with the setting. The setting as made slopes up with the key, which is why
/// high notes do not die at once. The loss is made here by smoothing the cycle forwards and
/// backwards, which leaves the phases alone.
fn wear(tab: &mut [f32], hz: f32, dt: f32, d: f32) {
    if tab.len() != STR || dt <= 0.0 {
        return;
    }
    // the loss on one trip round, as a multiple of the squared angle a harmonic turns through per frame at 44.1 kHz
    // (the measured loss is that of a one-pole smoother with the setting as its mix, met once a trip at twice 44.1 kHz)
    let d = d.clamp(0.02, 1.0);
    let a = 0.25 * (1.0 - d) / (d * d) * (0.91 + 0.07 * d);
    let stretch = STR as f32 * hz / 44100.0;
    let aq = 0.5 * a * hz * dt * stretch * stretch;
    if aq < 1e-7 {
        return;
    }
    let q = ((2.0 * aq + 1.0) - (4.0 * aq + 1.0).sqrt()) / (2.0 * aq);
    let wrap = 1.0 / (1.0 - q.powi(STR as i32)).max(1e-6);
    // going round a ring: one dry pass finds where the smoothed value ends up, which is where it must start
    let mut y = 0.0f32;
    for &x in tab.iter() {
        y = x + (y - x) * q;
    }
    y *= wrap;
    for x in tab.iter_mut() {
        y = *x + (y - *x) * q;
        *x = y;
    }
    let mut y = 0.0f32;
    for &x in tab.iter().rev() {
        y = x + (y - x) * q;
    }
    y *= wrap;
    for x in tab.iter_mut().rev() {
        y = *x + (y - *x) * q;
        *x = y;
    }
}

#[inline]
fn look(t: &[f32], ph: f32) -> f32 {
    let x = (ph - ph.floor()) * TN as f32;
    let i = (x as usize).min(TN - 1);
    let f = x - i as f32;
    t[i] + (t[i + 1] - t[i]) * f
}

/// Bend a 0..=1 ramp the way a drawn line's piece is bent. Positive tension moves fast first and settles,
/// negative starts slowly. Measured through a level stage whose line had one bent piece: the piece is a stretch
/// of an exponential covering twenty doublings at full tension.
pub(crate) fn bend(t: f32, tension: f32) -> f32 {
    swell(t, tension)
}

/// The bend of a drawn line's piece in this instrument, measured on rendered notes: the piece is a stretch of
/// an exponential that covers twenty doublings at full tension, so a tense fall is a straight line in dB for
/// most of its way. Positive tension moves fast first.
fn swell(u: f32, tension: f32) -> f32 {
    swell_far(u, tension.clamp(-1.0, 1.0))
}

/// [`swell`] without the limit a drawn curve has: the wave knobs bend three times as far.
fn swell_far(u: f32, tension: f32) -> f32 {
    let s = -20.0 * std::f32::consts::LN_2 * tension;
    if s.abs() < 1e-3 {
        u
    } else {
        ((s * u).exp() - 1.0) / (s.exp() - 1.0)
    }
}

/// How many steps the stepped kinds of bend make at this tension (measured: one at full tension, three at
/// three quarters, seventeen at half, 128 or so at a quarter: three doublings for every quarter).
fn steps(tension: f32) -> f32 {
    let r = 1.0 - tension.abs().min(1.0);
    (1.0 + (12.0 * r - 2.0).exp2()).round()
}

/// How far along its way a piece of a drawn line is at `u` of its length, for each kind of bend a point can
/// ask for (all read off rendered notes). Kind 2 holds and is handled by the callers' start level, so it
/// answers nothing here.
fn bent(u: f32, tension: f32, mode: u8) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let t = tension.clamp(-1.0, 1.0);
    // the same bend run to the middle and then point-mirrored
    let twice = |f: &dyn Fn(f32) -> f32| if u < 0.5 { 0.5 * f(2.0 * u) } else { 1.0 - 0.5 * f(2.0 * (1.0 - u)) };
    // a bend defined for positive tension; negative tension is its point mirror
    let flip = |f: &dyn Fn(f32, f32) -> f32, u: f32| if t >= 0.0 { f(u, t) } else { 1.0 - f(1.0 - u, -t) };
    let sixth = |u: f32, t: f32| u + t * (1.0 - (1.0 - u).powi(6) - u);
    let ratio = |u: f32, t: f32| {
        let c = (1.0 - t) * (1.0 - t) / (1.0 + t);
        if u <= 0.0 { 0.0 } else { u / (u + c * (1.0 - u)) }
    };
    let smooth = |u: f32| u * u * (3.0 - 2.0 * u);
    match mode {
        1 => twice(&|u| swell(u, t)),
        2 => 0.0,
        3 => {
            let k = steps(t);
            // (measured) the other way round the steps start one up and the last one is the jump at the end
            if t >= 0.0 { (u * (k + 1.0)).floor().min(k) / k } else { ((u * k).floor().min(k - 1.0) + 1.0) / (k + 1.0) }
        }
        4 => {
            let k = steps(t);
            let p = u * k;
            let j = p.floor().min(k - 1.0);
            (j + smooth(smooth(p - j))) / k
        }
        5 => {
            let k = steps(t);
            let high = ((u * 2.0 * k).floor() as i32).min(2 * k as i32 - 1) & 1 == 1;
            if high == (t >= 0.0) { 1.0 } else { 0.0 }
        }
        6 => {
            let p = u * (steps(t) - 0.5);
            let f = p - p.floor();
            if t >= 0.0 { 1.0 - (2.0 * f - 1.0).abs() } else { 0.5 - 0.5 * (std::f32::consts::TAU * f).cos() }
        }
        7 => flip(&sixth, u),
        8 => twice(&|u| flip(&sixth, u)),
        9 => flip(&|u, t| u + t * ((std::f32::consts::FRAC_PI_2 * u).sin() - u), u),
        10 => smooth(u) + t.abs() * (u - smooth(u)),
        11 => flip(&ratio, u),
        12 => twice(&|u| flip(&ratio, u)),
        _ => swell(u, t),
    }
}

/// Where things sit inside an operator's settings. The layout changed twice: the first blocks hold 48
/// overtones and no phases, the next ones 256 overtones, the current ones phases as well.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpLay {
    pub len: usize,
    pub noise: usize,
    pub mags: usize,
    pub harmonics: usize,
    /// Shape, tension, skew, sine shaper and brightness, four bytes each.
    pub tail: usize,
    /// The half wave switch; fold up sits two bytes on.
    pub flags: usize,
    /// How many switch bytes there are from `flags` on: the two halves, fold up, free running, centre, soft
    /// start and plucked string (measured: the first layout keeps the same seven in the same order).
    pub switches: usize,
    /// The first layout: filters hold no type and keep the shaper switch in one byte.
    pub first: bool,
}

impl OpLay {
    /// The first two layouts, which share the older unison laws.
    pub fn early(&self) -> bool {
        self.harmonics < 256
    }

    pub const NEW: OpLay = OpLay { len: 2128, noise: 48, mags: 52, harmonics: 256, tail: 2100, flags: 2120, switches: 8, first: false };
    pub const MID: OpLay = OpLay { len: 1104, noise: 48, mags: 52, harmonics: 128, tail: 1076, flags: 1096, switches: 8, first: false };
    pub const FIRST: OpLay = OpLay { len: 271, noise: 260, mags: 48, harmonics: 24, tail: 240, flags: 264, switches: 7, first: true };

    /// The settings as the current layout holds them.
    fn widen(&self, h: &[u8]) -> Vec<u8> {
        let mut w = vec![0u8; 2128];
        w[..48].copy_from_slice(&h[..48]);
        w[48..52].copy_from_slice(&h[self.noise..self.noise + 4]);
        w[52..52 + self.harmonics * 4].copy_from_slice(&h[self.mags..self.mags + self.harmonics * 4]);
        if self.harmonics < 256 {
            // (the older layouts hold fewer harmonics and then their phases: two dozen in the first, 128 in the
            // middle one, seen as a drawn wave with its phases scattered)
            let n = self.harmonics * 4;
            w[1076..1076 + n].copy_from_slice(&h[self.mags + n..self.mags + n * 2]);
        }
        w[2100..2120].copy_from_slice(&h[self.tail..self.tail + 20]);
        w[2120..2120 + self.switches].copy_from_slice(&h[self.flags..self.flags + self.switches]);
        w
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pt {
    /// Distance from the point before, in seconds for envelopes.
    pub x: f32,
    pub y: f32,
    pub tension: f32,
    pub mode: u8,
}

/// A drawn line: an envelope with its section markers, or a mapping.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Curve {
    pub pts: Vec<Pt>,
    /// Bit 0: times follow the tempo. Bit 1: one line shared by every note.
    pub flags: i32,
    /// Switched on (envelopes and slow oscillators only; mappings always apply).
    pub live: bool,
    pub decay: i32,
    pub loop_start: i32,
    pub sustain: i32,
    /// Where the line sits in the block it was read from (start and end), so an edited line can go back in.
    pub span: (usize, usize),
    /// Running position of every point, after the length knobs.
    t: Vec<f32>,
}

impl Curve {
    /// How far along a slide over the attack part (start to the attack marker) a point `u` of the way
    /// through it in time is: the bend drawn on the piece that ends at the marker, taken over the whole part
    /// (measured: a sequence whose last piece is fully tense does not slide though its first is straight).
    pub fn attack_way(&self, u: f32) -> f32 {
        match self.pts.get(self.decay.max(0) as usize) {
            Some(p) => bent(u, p.tension, p.mode),
            None => 1.0,
        }
    }

    pub fn flat(y: f32) -> Self {
        let mut c = Self { pts: vec![Pt { x: 0.0, y, ..Default::default() }, Pt { x: 1.0, y, ..Default::default() }], flags: 0, live: false, decay: -1, loop_start: -1, sustain: -1, span: (0, 0), t: vec![] };
        c.prepare([1.0; 3]);
        c
    }

    /// Writes the line back over the place it was read from. The block grows or shrinks with the number of
    /// points, so everything read from it before must be read again. False when the place no longer fits.
    pub fn store(&self, raw: &mut Vec<u8>) -> bool {
        let (a, b) = self.span;
        if b > raw.len() || b < a + 32 {
            return false;
        }
        if at(raw, a + 4) < 2 {
            // a record with the old point layout is rewritten as a current one
            return Self::replace_at(raw, a, self);
        }
        let n = at(raw, a + 8);
        if n < 0 || a + 12 + 24 * n as usize + 20 != b {
            return false;
        }
        let mut out = Vec::with_capacity(32 + 24 * self.pts.len());
        out.extend_from_slice(&raw[a..a + 8]);
        out.extend_from_slice(&(self.pts.len() as i32).to_le_bytes());
        for (i, p) in self.pts.iter().enumerate() {
            // a value that was not moved keeps its stored form, which is finer than what we hold
            let old = (i < n as usize).then(|| &raw[a + 12 + 24 * i..a + 36 + 24 * i]);
            let same = |o: usize, v: f32, hi: f32| old.filter(|b| {
                let w = f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
                w.is_finite() && (w as f32).clamp(0.0, hi) == v
            });
            match same(0, p.x, 1e5) {
                Some(b) => out.extend_from_slice(&b[0..8]),
                None => out.extend_from_slice(&(p.x as f64).to_le_bytes()),
            }
            match same(8, p.y, 1.0) {
                Some(b) => out.extend_from_slice(&b[8..16]),
                None => out.extend_from_slice(&(p.y as f64).to_le_bytes()),
            }
            out.extend_from_slice(&p.tension.to_le_bytes());
            out.push(p.mode);
            out.extend_from_slice(old.map(|b| &b[21..24]).unwrap_or(&[0, 0, 0]));
        }
        // the switch keeps the number it was stored with unless it has been thrown
        let was = if b >= a + 16 && b <= raw.len() { at(raw, b - 16) } else { 0 };
        let switch = if (was != 0) == self.live { was } else { -(self.live as i32) };
        for v in [self.flags, switch, self.decay, self.loop_start, self.sustain] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        raw.splice(a..b, out);
        true
    }

    /// Read the line whose record starts at `head` of a block, whatever it is used for there. Only the current
    /// point layout is read.
    pub fn read_at(raw: &[u8], head: usize) -> Option<Curve> {
        if raw.len() < head + 12 {
            return None;
        }
        let mut r = R { d: raw, p: head, lfo_rest: 0 };
        r.curve(at(raw, head))
    }

    /// Put this line where the record at `head` is, whatever point layout that record had.
    pub fn replace_at(raw: &mut Vec<u8>, head: usize, c: &Curve) -> bool {
        if raw.len() < head + 12 {
            return false;
        }
        let (sub, n) = (at(raw, head + 4), at(raw, head + 8));
        if !(0..=3).contains(&sub) || !(0..=8192).contains(&n) {
            return false;
        }
        let end = head + 12 + n as usize * [12, 16, 24, 24][sub as usize] + 20;
        if end > raw.len() {
            return false;
        }
        if sub < 2 {
            // an old record becomes a current one with the same tail, then takes the points
            let mut fresh = Vec::with_capacity(32);
            fresh.extend_from_slice(&raw[head..head + 4]);
            fresh.extend_from_slice(&3i32.to_le_bytes());
            fresh.extend_from_slice(&0i32.to_le_bytes());
            fresh.extend_from_slice(&raw[end - 20..end]);
            raw.splice(head..end, fresh);
            let mut c = c.clone();
            c.span = (head, head + 32);
            return c.store(raw);
        }
        let mut c = c.clone();
        c.span = (head, end);
        c.store(raw)
    }

    pub fn on(&self) -> bool {
        self.live
    }

    /// Lay the points out in time. `sc` stretches the parts before the decay marker, up to the sustain marker and
    /// after it.
    pub fn prepare(&mut self, sc: [f32; 3]) {
        let mut acc = 0.0;
        self.t.clear();
        for (i, p) in self.pts.iter().enumerate() {
            let i = i as i32;
            // measured: with no decay marker the attack knob has nothing to work on and the decay knob stretches
            // everything up to the sustain marker; with no sustain marker the release knob does nothing
            let s = if self.sustain >= 0 && i > self.sustain {
                sc[2]
            } else if self.decay >= 0 && i <= self.decay {
                sc[0]
            } else {
                sc[1]
            };
            if i > 0 {
                acc += p.x.max(0.0) * s;
            }
            self.t.push(acc);
        }
    }

    /// The sustain knob: the level held (the sustain point, or the last one without a marker) is raised by this
    /// (measured: with the knob at 64 a point at nothing held at half and a point at half at full).
    pub fn lift(&mut self, by: f32) {
        let at = if self.sustain >= 0 { self.sustain as usize } else { self.pts.len().wrapping_sub(1) };
        if let (true, Some(p)) = (by != 0.0, self.pts.get_mut(at)) {
            p.y = (p.y + by).clamp(0.0, 1.0);
        }
    }

    pub fn length(&self) -> f32 {
        self.t.last().copied().unwrap_or(0.0)
    }

    fn seg(&self, pos: f32) -> f32 {
        let n = self.pts.len();
        if n == 0 {
            return 0.0;
        }
        if pos <= self.t[0] {
            return self.pts[0].y;
        }
        for i in 1..n {
            if pos < self.t[i] {
                let (a, b) = (&self.pts[i - 1], &self.pts[i]);
                let d = self.t[i] - self.t[i - 1];
                let u = if d > 1e-9 { (pos - self.t[i - 1]) / d } else { 1.0 };
                return a.y + (b.y - a.y) * bent(u, b.tension, b.mode);
            }
        }
        self.pts[n - 1].y
    }

    /// Read as a mapping: `x` runs 0..=1 over the whole line.
    pub fn map(&self, x: f32) -> f32 {
        self.seg(x.clamp(0.0, 1.0) * self.length())
    }

    /// Time left after the key is let go at `t`.
    pub fn tail(&self, t: f32) -> f32 {
        let n = self.pts.len();
        let s = self.sustain;
        if s >= 0 && (s as usize) < n {
            self.length() - self.t[s as usize]
        } else {
            (self.length() - t).max(0.0)
        }
    }

    /// Read as an envelope `t` seconds into the note; `rel` is when the key was let go.
    pub fn env(&self, t: f32, rel: Option<f32>) -> f32 {
        let n = self.pts.len();
        if n == 0 {
            return 0.0;
        }
        let s = self.sustain;
        if s < 0 || s as usize >= n {
            return self.seg(t);
        }
        let s = s as usize;
        let ts = self.t[s];
        let held = |t: f32| -> f32 {
            if t < ts {
                return self.seg(t);
            }
            let ls = self.loop_start;
            if ls >= 0 && (ls as usize) < s && ts - self.t[ls as usize] > 1e-4 {
                let tl = self.t[ls as usize];
                self.seg(tl + (t - tl) % (ts - tl))
            } else {
                self.pts[s].y
            }
        };
        match rel {
            None => held(t),
            Some(tr) => {
                let from = held(tr);
                let dt = (t - tr).max(0.0);
                let span = self.length() - ts;
                if s + 1 >= n {
                    // (measured: with the marker on the last point there is nothing left to play, and the line
                    // stays where the key found it)
                    from
                } else if dt >= span {
                    self.pts[n - 1].y
                } else if s + 1 < n && dt < self.t[s + 1] - ts {
                    // (measured) the first stretch after the marker starts from wherever the line was when
                    // the key went up, and keeps its own bend
                    let b = &self.pts[s + 1];
                    let u = dt / (self.t[s + 1] - ts);
                    from + (b.y - from) * bent(u, b.tension, b.mode)
                } else {
                    self.seg(ts + dt)
                }
            }
        }
    }
}

/// Everything that can move one setting of a module.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Art {
    pub env: Curve,
    pub lfo: Curve,
    pub lfo_hz: f32,
    /// The slow wave's tension, skew and pulse width.
    pub lfo_shape: [f32; 3],
    /// Key, velocity, X, Y, random, unison.
    pub maps: [Curve; 6],
    /// The setting is an offset around the middle (pan, pitch, phase, cutoff, resonance) rather than a level.
    pub bipolar: bool,
}

impl Art {
    fn neutral(bipolar: bool) -> Self {
        let y = if bipolar { 0.5 } else { 1.0 };
        Self { env: Curve::flat(y), lfo: Curve::flat(0.5), lfo_hz: 5.0, lfo_shape: [0.0; 3], maps: std::array::from_fn(|_| Curve::flat(y)), bipolar }
    }

    fn moving(&self) -> bool {
        self.env.on() || self.lfo.on()
    }

    fn stat(&self, xs: &[f32; 6]) -> f32 {
        if self.bipolar {
            self.maps.iter().zip(xs).map(|(m, &x)| (m.map(x) - 0.5) * 2.0).sum()
        } else {
            self.maps.iter().zip(xs).map(|(m, &x)| m.map(x)).product()
        }
    }

    /// `beat` is how many beats pass in a second. Lines count their time in beats: the ones marked as following
    /// the tempo in beats of the song, the others in beats at 120 to the minute (measured: a piece drawn one
    /// unit long lasts half a second).
    fn value(&self, stat: f32, t: f32, rel: Option<f32>, lfo_gain: f32, beat: f32, song: f64, slide: (f32, f32)) -> f32 {
        let read = |c: &Curve| {
            let b = if c.flags & 1 != 0 { beat } else { 2.0 };
            c.env(t * b, rel.map(|r| r * b))
        };
        // the envelope alone runs on the slide's clock: its first `slide.0` seconds pass `slide.1` times slower
        let own = || {
            let (len, by) = slide;
            let warp = |x: f32| if len <= 0.0 { x } else if x < len { x / by } else { x - len + len / by };
            let b = if self.env.flags & 1 != 0 { beat } else { 2.0 };
            self.env.env(warp(t) * b, rel.map(|r| warp(r) * b))
        };
        // (measured: a wave marked as following the tempo runs at its speed when the song is at 120)
        let rate = if self.lfo.flags & 1 != 0 { beat * 0.5 } else { 1.0 };
        // (measured: a wave with the second switch set turns with the song, not from the start of the note)
        let turn = if self.lfo.flags & 2 != 0 { ((self.lfo_hz * rate) as f64 * (song + t as f64)).fract() as f32 } else { self.lfo_hz * rate * t };
        let l = if self.lfo.on() { lfo_wave(turn, self.lfo_shape) * (read(&self.lfo) - 0.5) * 2.0 * lfo_gain } else { 0.0 };
        if self.bipolar {
            stat + if self.env.on() { (own() - 0.5) * 2.0 } else { 0.0 } + l
        } else {
            (stat * if self.env.on() { own() } else { 1.0 } * (1.0 + l)).max(0.0)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Op {
    /// Frequency against the note. Measured against a render: the stored 20000, shown as 1, sounds an octave
    /// above the note the other instruments play for the same key.
    pub ratio: f32,
    pub offset_hz: f32,
    pub phase: f32,
    pub vol: f32,
    pub pitch_cents: f32,
    pub noise: f32,
    /// How much of the output is noise for that knob setting (measured: little until the knob is well up).
    nx: f32,
    pub pluck: bool,
    /// Each note starts the wave at a place of its own (measured: the start moves from render to render).
    pub free: bool,
    /// The first millisecond of a note is faded in (measured).
    pub declick: bool,
    /// Its standing level is drained away as it plays.
    pub centre: bool,
    /// One cycle, for all harmonics and for fewer of them (used on high notes).
    tables: Vec<Arc<Vec<f32>>>,
    caps: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Filt {
    /// 0 off, 1 to 3 stages (4 and 5 are the alternative two and three).
    pub slope: i32,
    pub env_amt: f32,
    pub cut: f32,
    pub res: f32,
    pub levels: [f32; 3],
    pub ws_on: bool,
    pub ws_gain: f32,
    pub ws_mix: f32,
    pub ws_bipolar: bool,
    pub next: f32,
    /// 0 low/band/high mix, 1 to 3 low-pass, 4 band-pass, 5 notch, 6 high-pass, 7 all-pass, 8 low shelf, 9 peak,
    /// 10 high shelf, 11 driven low-pass, 12 phaser.
    pub kind: i32,
    /// Shelf and peak types: gain minus one. They and the driven low-pass keep their amount where the mix type
    /// keeps its low level.
    /// what the shelf and peak kinds add or take away, in dB
    pub db: f32,
    pub drive: f32,
    pub ws: Vec<f32>,
    /// the quality switch
    pub hq: bool,
    /// the switch for the other way of ringing (a phaser gets five more steps from it)
    pub flat: bool,
}

#[derive(Debug, Clone)]
pub struct Patch {
    pub volume: f32,
    pub lfo: f32,
    pub pitch: f32,
    /// `[target][source]`, 1.0 = 100 %.
    pub fm: [[f32; 6]; 6],
    pub rm: [[f32; 6]; 6],
    /// Operators into each filter.
    pub feed: [[f32; 6]; 3],
    pub pan: [f32; 9],
    pub out: [f32; 9],
    /// What each module passes to the effects (measured: this share is heard through them, the rest plain).
    pub send: [f32; 9],
    pub ops: [Op; 6],
    pub filts: [Filt; 3],
    /// 36 operator settings, 24 filter settings, then pan and volume of the whole instrument.
    pub arts: Vec<Art>,
    pub uni: u32,
    /// Left and right level of what goes to the effects (the effects pan, turned before they hear it).
    pub send_side: [f32; 2],
    pub uni_pan: f32,
    pub uni_vol: f32,
    pub uni_pitch: f32,
    pub uni_phase: f32,
    /// How much of each voice is a copy an octave down (measured: the copy takes this share, the voice keeps
    /// the rest; it works with a single voice too).
    pub uni_sub: f32,
    /// The voices take the older set of shares of the spread (measured: the first two layouts and, again,
    /// states written from version 24 on; version 19 to 22 states take the other set).
    pub uni_old_share: bool,
    /// The voices of a unison set all sit in the middle (measured: one switch byte takes the width away).
    pub uni_mono: bool,
    /// Sliding in from the note before, when its switch is on: whether a far note takes longer, and how the
    /// distance is held in (0 not at all, 1 folded into an octave, 2 to 5 at most four down to one semitone).
    pub slide: Option<(bool, u8)>,
    /// The first operator's pitch movement moves every operator.
    pub pitch_shared: bool,
    /// how many times over the voice is worked out, as a power of two: for a render and for live play
    /// (measured on renders: none means the plain samples with nothing filtered at all)
    pub over: [u8; 2],
    /// Notes play louder the harder they are hit, by the usual law (measured: with the switch off every
    /// velocity is as loud as the default one, and only the instrument's own maps can tell them apart).
    pub vel_vol: bool,
    /// Where the pad on the front sits (0 to 1 each way); a note's own X and Y move away from there.
    pub modx: f32,
    pub mody: f32,
    /// Where each operator's and filter's own settings start in the unpacked block.
    pub op_at: [usize; 6],
    pub filt_at: [usize; 3],
    /// How an operator's own settings are laid out in this block.
    pub lay: OpLay,
    /// Chorus, echo and room behind the effects send (current layouts only).
    pub tail: crate::tailfx::Set,
    used: [bool; 6],
    fused: [bool; 3],
    any_moving: bool,
}

pub const ARTS: usize = 62;

/// The effects section of a current block: a chorus, three echoes and a room. `share` is how much of the sound
/// is sent to them. One echo runs here, the first that is switched on; the chorus knob order is an estimate.
/// The three bands of the equaliser: 16 bytes each (level in hundredths of a dB, place, width, kind).
/// Measured on noise: the place runs from about 10 Hz to 16 kHz on a curve that is a hundred times steeper at
/// the top than at the bottom; a peak's sharpness runs from 0.1 to 100 on an even scale, a shelf's from 0.2
/// to 1.2 in a straight line.
fn equaliser(d: &[u8]) -> Option<[crate::tailfx::Band; 3]> {
    let mut bands = [crate::tailfx::Band::default(); 3];
    for (b, base) in bands.iter_mut().zip([1089usize, 1105, 1121]) {
        let unit = |o: usize| at(d, o).clamp(0, 65536) as f32 / 65536.0;
        let kind = at(d, base + 12).clamp(0, 2) as u8;
        *b = crate::tailfx::Band { kind, hz: tone_hz(unit(base + 4)), db: at(d, base).clamp(-1800, 1800) as f32 / 100.0, q: tone_q(kind, unit(base + 8)) };
    }
    bands.iter().any(|b| b.db != 0.0).then_some(bands)
}

/// Where a band of the three-band equaliser stands for its knob at 0..=1.
pub fn tone_hz(place: f32) -> f32 {
    10.0 + 15990.0 * (100f32.powf(place.clamp(0.0, 1.0)) - 1.0) / 99.0
}

/// How sharp a band of the three-band equaliser is for its width knob at 0..=1 (kind 1 is a peak).
pub fn tone_q(kind: u8, width: f32) -> f32 {
    let w = width.clamp(0.0, 1.0);
    if kind == 1 {
        0.1 * 1000f32.powf(w)
    } else {
        0.2 + w
    }
}

/// The fixed head of an older block laid out the current way, as far as the effects go. The layouts differ in
/// how many switch bytes stand before the echoes (they start 109, 147 or 159 bytes before the notes), in
/// whether there is a room (from layout 12 on) and in the equaliser's bands (three numbers each before layout
/// 14, with the kinds fixed as low shelf, peak, high shelf; four after).
fn current_head(d: &[u8], ver: u32, at_text: usize) -> Vec<u8> {
    let (room, wide_bands) = (ver >= 12, ver >= 14);
    let span = 69 + if room { 36 + 6 } else { 4 } + if wide_bands { 48 } else { 36 };
    let Some(e) = at_text.checked_sub(span).filter(|&e| e >= 964 && at_text <= d.len()) else { return Vec::new() };
    let mut h = d[..964].to_vec();
    h.resize(978, 0);
    h.extend_from_slice(&d[e..e + 69]);
    let mut at = e + 69;
    if room {
        h.extend_from_slice(&d[at..at + 42]);
        at += 42;
    } else {
        h.resize(h.len() + 38, 0);
        h.extend_from_slice(&d[at..at + 4]);
        at += 4;
    }
    for band in 0..3u32 {
        if wide_bands {
            h.extend_from_slice(&d[at..at + 16]);
            at += 16;
        } else {
            h.extend_from_slice(&d[at..at + 12]);
            h.extend_from_slice(&band.to_le_bytes());
            at += 12;
        }
    }
    h
}

/// The effects section: a chorus (724 on), three echoes (978, 1001, 1024) and a room (1047).
fn effects(d: &[u8]) -> crate::tailfx::Set {
    use crate::opfx::{grid, Chorus, Echo, Room, Set};
    let k = |o: usize, top: i32| at(d, o).clamp(-top, top) as f32 / top as f32;
    let mut set = Set::default();
    let order = at(d, 724).clamp(0, crate::opfx::VOICES as i32) as u8;
    if order > 0 && at(d, 748) != 0 {
        set.chorus = Some(Chorus { order, depth: k(728, 1024).max(0.0), speed: k(732, 1024).max(0.0), delay: k(736, 128).max(0.0), spread: k(740, 128), cross: k(744, 128), vol: k(748, 128) });
    }
    for (n, base) in [978usize, 1001, 1024].into_iter().enumerate() {
        if d[base + 22] == 0 {
            continue;
        }
        set.echoes[n] = Some(Echo {
            fb: k(base, 128),
            steps: at(d, base + 4).clamp(1, 48 * 64) as f32 / 48.0,
            late_ms: at(d, base + 8).clamp(-4096, 4096) as f32 * (20.0 / 48.0 / 44.1),
            vol: grid(k(base + 12, 128)),
            mode: at(d, base + 16).clamp(0, 2) as u8,
            tempo: d[base + 20] != 0,
            serial: d[base + 21] != 0,
        });
    }
    if d[1083] != 0 && at(d, 1079) != 0 {
        let v = |o: usize| at(d, o) as f32;
        set.room = Some(Room { low_hz: v(1047), high: v(1051), predelay: v(1055), size: v(1059), diffusion: v(1063), decay: v(1067), damping: v(1071), colour: at(d, 1075).clamp(0, 5) as u8, wet: k(1079, 128) });
    }
    crate::tailfx::Set { op: Some(set), ..Default::default() }
}

struct R<'a> {
    d: &'a [u8],
    p: usize,
    /// Bytes after a slow oscillator's speed (older blocks carry one setting fewer).
    lfo_rest: usize,
}

impl<'a> R<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn i(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn curve(&mut self, kind: i32) -> Option<Curve> {
        let from = self.p;
        if self.i()? != kind {
            return None;
        }
        let sub = self.i()?;
        let n = self.i()?;
        if !(0..=8192).contains(&n) || !(0..=3).contains(&sub) {
            return None;
        }
        let mut pts = Vec::with_capacity(n as usize);
        let fin = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
        for _ in 0..n {
            // older records hold single-precision points, the oldest without the bend's kind
            let (x, y, tension, mode) = if sub >= 2 {
                let b = self.bytes(24)?;
                (f64::from_le_bytes(b[0..8].try_into().ok()?), f64::from_le_bytes(b[8..16].try_into().ok()?), f32::from_le_bytes(b[16..20].try_into().ok()?), b[20])
            } else {
                let b = self.bytes(if sub == 1 { 16 } else { 12 })?;
                let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                (f(0) as f64, f(4) as f64, f(8), if sub == 1 { b[12] } else { 0 })
            };
            pts.push(Pt { x: fin(x).clamp(0.0, 1e5), y: fin(y).clamp(0.0, 1.0), tension: if tension.is_finite() { tension } else { 0.0 }, mode });
        }
        let flags = self.i()?;
        let live = self.i()? != 0;
        let decay = self.i()?;
        let loop_start = self.i()?;
        let sustain = self.i()?;
        Some(Curve { pts, flags, live, decay, loop_start, sustain, span: (from, self.p), t: vec![] })
    }
    /// One setting's envelope, slow oscillator and six mappings. `g` holds the instrument-wide length knobs.
    fn art(&mut self, bipolar: bool, g: [f32; 4]) -> Option<Art> {
        self.bytes(16)?;
        let mut env = self.curve(1)?;
        let k = [self.i()?, self.i()?, self.i()?, self.i()?];
        // (measured since on an older patch: the knob goes below the middle too, and then lowers the level held)
        env.lift(k[2].clamp(-128, 128) as f32 / 128.0 + g[3]);
        env.prepare([knob(k[0]) * g[0], knob(k[1]) * g[1], knob(k[3]) * g[2]]);
        let mut lfo = self.curve(2)?;
        let speed = self.i()?;
        let rest = self.bytes(self.lfo_rest)?;
        let rest: Vec<i32> = rest.chunks_exact(4).take(3).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
        lfo.prepare([1.0; 3]);
        let mut maps: [Curve; 6] = Default::default();
        for m in maps.iter_mut() {
            // (measured with a map on a filter's level in one of the oldest blocks: their maps bend about
            // half as far, ten and a half doublings at full tension; their envelopes and the shaper's line
            // measured as they are)
            let oldest = self.d.get(self.p + 4..self.p + 8) == Some(&[0, 0, 0, 0]);
            *m = self.curve(3)?;
            if oldest {
                for pt in m.pts.iter_mut() {
                    pt.tension *= 0.52;
                }
            }
            m.prepare([1.0; 3]);
        }
        Some(Art { env, lfo, lfo_hz: lfo_hz(speed), lfo_shape: lfo_shape(&rest), maps, bipolar })
    }
}

/// The file name of the envelope sender, the silent channel whose notes run envelopes into linked controls.
pub const ENV_SENDER: &str = "Fruity Envelope Controller";

/// One output of the envelope sender: its drawn lines and its three knobs.
#[derive(Debug, Clone)]
pub struct SendArt {
    pub art: Art,
    /// Where the output rests, 0..=1.
    pub base: f32,
    /// How much of the envelope and of the slow oscillator goes in, -1..=1.
    pub env: f32,
    pub lfo: f32,
    /// The keys that set this output off.
    pub lo: i32,
    pub hi: i32,
    /// Where in the block the added-or-multiplied switch and the key range sit.
    pub mode_at: usize,
    pub range_at: usize,
}

/// The envelope sender's settings, read from its stored block.
#[derive(Debug, Clone)]
pub struct Sender {
    pub arts: Vec<SendArt>,
    pub x: f32,
    pub y: f32,
}

impl Sender {
    /// Reads the block: a table of knobs first, then per output its lines. Older blocks hold four outputs with
    /// two mappings each, newer ones eight with five.
    pub fn parse(raw: &[u8]) -> Option<Self> {
        [(8usize, 5usize, 6usize), (4, 2, 4)].into_iter().find_map(|(n, maps, globals)| Self::parse_as(raw, n, maps, globals))
    }

    fn parse_as(raw: &[u8], n: usize, n_maps: usize, globals: usize) -> Option<Self> {
        let head = 4 + n * 44;
        if raw.len() < head + globals * 4 {
            return None;
        }
        let (x, y) = if globals >= 6 { (at(raw, head) as f32 / 65536.0, at(raw, head + 4) as f32 / 65536.0) } else { (0.5, 0.5) };
        let mut r = R { d: raw, p: head + globals * 4, lfo_rest: 16 };
        let mut arts = Vec::with_capacity(n);
        for k in 0..n {
            let mode_at = r.p;
            let bipolar = r.i()? != 0;
            r.bytes(16)?;
            let mut env = r.curve(1)?;
            let len = [r.i()?, r.i()?, r.i()?, r.i()?];
            env.lift(len[2].clamp(-128, 128) as f32 / 128.0);
            env.prepare([knob(len[0]), knob(len[1]), knob(len[3])]);
            let mut lfo = r.curve(2)?;
            let speed = r.i()?;
            let rest = [r.i()?, r.i()?, r.i()?];
            r.bytes(4)?;
            lfo.prepare([1.0; 3]);
            let mut art = Art::neutral(bipolar);
            for m in art.maps.iter_mut().take(n_maps) {
                *m = r.curve(3)?;
                m.prepare([1.0; 3]);
            }
            art.env = env;
            art.lfo = lfo;
            art.lfo_hz = lfo_hz(speed);
            art.lfo_shape = lfo_shape(&rest);
            let range_at = r.p;
            let (lo, hi) = (r.i()?, r.i()?);
            let h = 4 + k * 44;
            let two = |o: usize| ((at(raw, o) - 128) as f32 / 128.0).clamp(-1.0, 1.0);
            arts.push(SendArt { art, base: (at(raw, h) as f32 / 256.0).clamp(0.0, 1.0), env: two(h + 4), lfo: two(h + 8), lo, hi, mode_at, range_at });
        }
        (r.p == raw.len()).then_some(Self { arts, x: x.clamp(0.0, 1.0), y: y.clamp(0.0, 1.0) })
    }

    /// A block for a new envelope sender: eight multiplied outputs resting at nothing, each with a short
    /// attack, a fall to a held level and a release, over the whole keyboard.
    pub fn fresh() -> Vec<u8> {
        fn line(kind: i32, pts: &[(f64, f64)], tail: [i32; 5], out: &mut Vec<u8>) {
            for v in [kind, 3, pts.len() as i32] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for &(x, y) in pts {
                out.extend_from_slice(&x.to_le_bytes());
                out.extend_from_slice(&y.to_le_bytes());
                out.extend_from_slice(&[0; 8]);
            }
            for v in tail {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        let ints = |out: &mut Vec<u8>, vs: &[i32]| vs.iter().for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
        let mut b = Vec::with_capacity(5200);
        ints(&mut b, &[6]);
        for _ in 0..8 {
            ints(&mut b, &[0, 256, 128, 128, 128, 0, 128, 40000, 0, 0, 0]);
        }
        ints(&mut b, &[32768, 32768, 0, 0, 2, 0]);
        for _ in 0..8 {
            ints(&mut b, &[0, 3]);
            for f in [0.0f32, 1.0, 0.5] {
                b.extend_from_slice(&f.to_le_bytes());
            }
            line(1, &[(0.0, 0.0), (0.05, 1.0), (0.3, 0.6), (0.4, 0.0)], [0, -1, 1, -1, 2], &mut b);
            ints(&mut b, &[128, 128, 0, 128]);
            line(2, &[(0.0, 0.5), (1.0, 0.5)], [0, 0, -1, -1, -1], &mut b);
            ints(&mut b, &[40000, 0, 0, 0, 0]);
            for _ in 0..5 {
                line(3, &[(0.0, 1.0), (1.0, 1.0)], [0, -1, -1, -1, -1], &mut b);
            }
            ints(&mut b, &[0, 256]);
        }
        b
    }

    /// What output `a` sends `t` seconds into a note (`rel` is when the key was let go), or nothing when the key
    /// is outside the output's range. Added-up outputs swing around the resting value; multiplied ones rise from it.
    pub fn value(&self, a: usize, key: u8, vel: f32, rand: f32, t: f32, rel: Option<f32>) -> Option<f32> {
        let s = self.arts.get(a)?;
        if (key as i32) < s.lo || (key as i32) > s.hi {
            return None;
        }
        let xs = [key as f32 / 131.0, vel.clamp(0.0, 1.0), self.x, self.y, rand, 0.5];
        let stat = s.art.stat(&xs);
        let e = s.art.env.env(t, rel);
        let l = lfo_wave(s.art.lfo_hz * t, s.art.lfo_shape) * (s.art.lfo.env(t, rel) - 0.5) * 2.0 * s.lfo;
        let v = if s.art.bipolar {
            (stat + (e - 0.5) * 2.0 * s.env + l) * 0.5
        } else {
            let e = if s.env < 0.0 { 1.0 - e } else { e };
            let k = s.env.abs();
            (stat * (1.0 - k + k * e) * (1.0 + l)).max(0.0)
        };
        Some((s.base + v).clamp(0.0, 1.0))
    }

    /// Seconds output `a` keeps moving after the key is let go at `t`.
    pub fn tail(&self, a: usize, t: f32) -> f32 {
        self.arts.get(a).map(|s| s.art.env.tail(t).max(s.art.lfo.tail(t))).unwrap_or(0.0)
    }
}

/// A length knob: 128 leaves the drawn length alone, and the length goes with the knob in a straight line
/// (measured at a quarter, a half, one and a half and twice).
pub(crate) fn knob(v: i32) -> f32 {
    (v.clamp(0, 256) as f32 / 128.0).max(0.003)
}

fn at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

/// How hard the tension and skew knobs bend, as the tension of [`swell`]: fitted to renders at every quarter of
/// the knob (0.17, 0.53, 1.3 and 3 for tension; skew bends half as much).
fn bend_of(k: f32) -> f32 {
    k.signum() * 0.1447 * ((3.08 * k.abs()).exp() - 1.0)
}

/// How much of the unison spread each voice takes, in the order the voices are made (measured on sets of two
/// to nine voices at the top of the knob, each voice split off the others by its own pitch).
const UNI_SHARE: [f32; 9] = [0.7886, 0.9533, 0.8072, 0.9764, 0.9158, 1.0334, 0.7562, 1.0682, 0.8856];
/// The same for the first two layouts, which carry another set (measured on sets of two, four and six in the
/// first and on a set of four in the second).
const UNI_SHARE_OLD: [f32; 9] = [0.7767, 0.9467, 0.8225, 1.0459, 0.9130, 1.0, 0.7855, 0.9892, 0.8652];

/// The unison spread in cents for the pitch knob at 0..=1 (measured at every quarter: 3.2, 11.4, 32.2 and 84.7
/// cents; the first two layouts open sooner and end lower: 4.7, 13.1, 28.4 and about 56).
/// How far ahead of the song a free-running operator's clock stands, in seconds (the best fit is 128 frames).
/// Renders of one project come out at 128 or at 256 frames from one run to the next, so the checking tools
/// can set the count themselves.
const FREE_AHEAD: f64 = 128.0 / 44100.0;

fn uni_spread(knob: f32, early: bool) -> f32 {
    if early {
        5.73 * (10.8f32.powf(knob) - 1.0)
    } else {
        2.117 * (41.0f32.powf(knob) - 1.0)
    }
}

/// How fast a slow wave runs for its stored speed (measured at six settings: 32 Hz at the top of the knob and
/// ten times slower for every quarter of it below; timed again over long notes at eleven slower settings: the
/// whole line sits 0.0032 Hz lower, so the bottom of the knob stands still).
pub fn lfo_hz(speed: i32) -> f32 {
    0.0032 * (10f32.powf(4.0 * speed.clamp(0, 65536) as f32 / 65536.0) - 1.0)
}

/// The three settings stored after a slow wave's speed, each brought to -1..=1: tension, skew and pulse width.
pub fn lfo_shape(raw: &[i32]) -> [f32; 3] {
    std::array::from_fn(|k| raw.get(k).map_or(0.0, |&v| (v.clamp(-128, 128) as f32 / 128.0).clamp(-1.0, 1.0)))
}

/// A slow wave `cycles` into its run (all read off rendered notes): a triangle that starts at rest going up.
/// Skew slides the peak later and the trough earlier until the wave is a rising saw, pulse width holds the
/// peak (or the trough when negative) for that share of the cycle and squeezes the rest, and tension bends
/// the height the way an operator's wave knob does.
pub fn lfo_wave(cycles: f32, shape: [f32; 3]) -> f32 {
    let [tn, sk, pw] = shape;
    let mut p = cycles.rem_euclid(1.0);
    let a = (0.25 * (1.0 + sk)).clamp(1e-4, 0.5);
    let w = pw.abs().min(0.999);
    if w > 0.0 {
        let at = if pw > 0.0 { a } else { 1.0 - a } * (1.0 - w);
        p = if p < at {
            p / (1.0 - w)
        } else if p < at + w {
            at / (1.0 - w)
        } else {
            (p - w) / (1.0 - w)
        };
    }
    let y = if p < a {
        p / a
    } else if p <= 1.0 - a {
        if a >= 0.5 { 1.0 } else { 1.0 - 2.0 * (p - a) / (1.0 - 2.0 * a) }
    } else {
        (p - 1.0) / a
    };
    if tn != 0.0 {
        y.signum() * swell_far(y.abs(), bend_of(tn))
    } else {
        y
    }
}

/// One point of an operator's drawn wave. All of it is measured on rendered notes: skew squeezes the wave toward
/// the middle of its period, the sine shaper then folds it through a sine of up to three quarter turns (kept at
/// full height while it is less than a quarter turn), and tension bends the height last.
fn shape(sh: f32, tn: f32, sk: f32, sn: f32, ph: f32) -> f32 {
    let p = if sk > 0.0 {
        let t = -bend_of(sk) * 0.5;
        if ph < 0.5 {
            0.5 * swell_far(2.0 * ph, t)
        } else {
            1.0 - 0.5 * swell_far(2.0 * (1.0 - ph), t)
        }
    } else {
        ph
    };
    // a sine, a triangle, a falling saw, a square and a narrow pulse sit a quarter of the knob apart; the sine
    // fades into the triangle, whose peak then slides to the front until it is a saw
    let u = sh * 2.0;
    let wedge = |m: f32| {
        if p < m {
            p / m
        } else if p <= 1.0 - m {
            1.0 - 2.0 * (p - m) / (1.0 - 2.0 * m).max(1e-6)
        } else {
            (p - 1.0) / m
        }
    };
    let pulse = |w: f32| if p < w { 1.0 } else { -1.0 };
    let mut y = if u <= 1.0 {
        let s = (TAU * p).sin();
        s + (wedge(0.25) - s) * u
    } else if u <= 2.0 {
        wedge(0.25 * (2.0 - u))
    } else if u <= 3.0 {
        let saw = 1.0 - 2.0 * p;
        saw + (pulse(0.5) - saw) * (u - 2.0)
    } else {
        pulse(0.5 - 0.375 * (u - 3.0).min(1.0))
    };
    if sn > 0.0 {
        let a = sn * 1.5 * PI;
        // the folded wave is brought back to full height: what it can reach depends on which heights the
        // wave takes at all (a square only ever sits at its two ends)
        let lo = (u - 2.0).clamp(0.0, 1.0) * a;
        let top = if lo <= PI * 0.5 && a >= PI * 0.5 { 1.0 } else { lo.sin().abs().max(a.sin().abs()) };
        y = (a * y).sin() / top.max(0.05);
    }
    if tn != 0.0 {
        y = y.signum() * swell_far(y.abs(), bend_of(tn));
    }
    y
}

/// Keep only the first `n` harmonics of one period.
fn keep_harmonics(t: &mut [f32], n: usize) {
    let len = t.len();
    if n * 2 >= len {
        return;
    }
    let mean = t.iter().sum::<f32>() / len as f32;
    let mut out = vec![mean; len];
    for h in 1..=n {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (j, v) in t.iter().enumerate() {
            let a = std::f64::consts::TAU * ((j * h) % len) as f64 / len as f64;
            re += *v as f64 * a.cos();
            im += *v as f64 * a.sin();
        }
        let (re, im) = ((re * 2.0 / len as f64) as f32, (im * 2.0 / len as f64) as f32);
        if re.abs() + im.abs() < 1e-7 {
            continue;
        }
        for (j, o) in out.iter_mut().enumerate() {
            let a = TAU * ((j * h) % len) as f32 / len as f32;
            *o += re * a.cos() + im * a.sin();
        }
    }
    t.copy_from_slice(&out);
}

fn op_from(h: &[u8], lay: &OpLay) -> Op {
    let wide;
    let h = if lay.len != 2128 {
        wide = lay.widen(h);
        &wide[..]
    } else {
        h
    };
    let i = |o: usize| at(h, o);
    let f = |o: usize| f32::from_le_bytes(h[o..o + 4].try_into().unwrap());
    let (sh, tn, sk, sn) = (i(2100).clamp(0, 512) as f32 / 256.0, i(2104).clamp(-128, 128) as f32 / 128.0, i(2108).clamp(0, 128) as f32 / 128.0, i(2112).clamp(0, 128) as f32 / 128.0);
    // measured: the two "half" switches play the first half of the wave over the whole period, and the whole wave
    // in the first half of the period with silence after; the third folds the wave up
    let (stretch, squeeze, absolute) = (h[2120] != 0, h[2121] != 0, h[2122] != 0);
    let centre = h[2124] != 0;
    let mags: Vec<f32> = (0..256).map(|k| f(52 + k * 4)).map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }).collect();
    let phs: Vec<f32> = (0..256).map(|k| f(1076 + k * 4)).map(|v| if v.is_finite() { v } else { 0.0 }).collect();
    let top = mags.iter().rposition(|&m| m > 0.0).map(|k| k + 1).unwrap_or(0);
    let plain = top <= 1 && (top == 0 || (mags[0] == 1.0 && phs[0] == 0.0));
    // (measured with two drawn harmonics: skew and the sine shaper bend every harmonic's own wave, tension bends
    // the finished sum)
    let mut base: Vec<f32> = (0..TN).map(|k| shape(sh, if plain { tn } else { 0.0 }, sk, sn, k as f32 / TN as f32)).collect();
    // the pre-filter cuts the wave off above a harmonic: four are left at the bottom of the knob, close to three
    // times as many for every eighth of its travel (measured at nine settings up to 32: 4, 6, 8, 9, 10, 13, 17,
    // 22 and 38 left at 0, 4, 8, 10, 12, 16, 20, 24 and 32)
    // (it works on the finished wave: drawn harmonics above the cut are gone too, measured on an older organ)
    let pre = i(2116).clamp(0, 128);
    let kept = (4.4 * 2.95f32.powf(pre as f32 / 16.0)).round() as usize;
    if pre < 128 {
        keep_harmonics(&mut base, kept);
    }
    let halves = |t: &mut Vec<f32>| {
        if stretch {
            *t = (0..TN).map(|j| (t[j / 2] + t[(j + 1) / 2]) * 0.5).collect();
        }
        if squeeze {
            *t = (0..TN).map(|j| if j < TN / 2 { t[j * 2] } else { 0.0 }).collect();
        }
    };
    // (measured with three drawn harmonics: the two half switches work on every harmonic's own wave, so a
    // squeezed set is silent only where all of its harmonics rest; the fold works on the finished sum)
    if !plain {
        halves(&mut base);
    }
    let finish = |mut t: Vec<f32>, scale: f32| -> Arc<Vec<f32>> {
        t.iter_mut().for_each(|v| *v *= scale);
        if plain {
            halves(&mut t);
        }
        if absolute {
            t.iter_mut().for_each(|v| *v = v.abs());
        }
        t.push(t[0]);
        Arc::new(t)
    };
    let (mut tables, mut caps) = (vec![], vec![]);
    // drawn harmonics are scaled so that the whole wave peaks at one, before the pre-filter takes its cut
    // (measured: two drawn sets with the pre-filter shut came out at their full waves' scale)
    let mut scale = 0.0f32;
    if plain {
        tables.push(finish(base, 1.0));
        caps.push(1);
    } else {
        // (a bent sum is bent once, whole: its lighter tables are that wave with the top taken off)
        let mut whole: Option<Vec<f32>> = None;
        // (measured: a low saw keeps every harmonic that fits under half the sample rate, so the steps are fine)
        for cap in [256usize, 181, 128, 90, 64, 45, 32, 22, 16, 11, 8, 5, 4] {
            let n = top.min(cap);
            if caps.last() == Some(&(n as u32)) {
                continue;
            }
            if let Some(w) = &whole {
                let mut t = w.clone();
                keep_harmonics(&mut t, n.min(if pre < 128 { kept } else { n }));
                tables.push(finish(t, scale));
                caps.push(n as u32);
                continue;
            }
            let mut t = vec![0.0f32; TN];
            for k in 0..n {
                let m = mags[k];
                if m > 0.0 {
                    let shift = ((phs[k] - phs[k].floor()) * TN as f32) as usize;
                    for (j, v) in t.iter_mut().enumerate() {
                        *v += m * base[(j * (k + 1) + shift) % TN];
                    }
                }
            }
            if scale == 0.0 {
                let peak = t.iter().fold(0.0f32, |a, v| a.max(v.abs()));
                scale = if peak > 1e-6 { 1.0 / peak } else { 1.0 };
            }
            if tn != 0.0 {
                let bend = bend_of(tn);
                for v in t.iter_mut() {
                    let y = *v * scale;
                    *v = y.signum() * swell_far(y.abs().min(1.0), bend) / scale;
                }
                whole = Some(t.clone());
            }
            if pre < 128 {
                keep_harmonics(&mut t, kept);
            }
            tables.push(finish(t, scale));
            caps.push(n as u32);
        }
    }
    Op {
        ratio: i(40).clamp(0, 640000) as f32 / 10000.0,
        offset_hz: i(44) as f32 / 100.0,
        phase: i(0).clamp(0, 256) as f32 / 256.0,
        vol: i(4).clamp(-256, 512) as f32 / 256.0,
        pitch_cents: i(8).clamp(0, 9600) as f32,
        noise: i(48).clamp(0, 128) as f32 / 128.0,
        nx: (((i(48).clamp(0, 128) as f32 / 128.0 * 2.45).exp() - 1.0) / (2.45f32.exp() - 1.0)).clamp(0.0, 1.0),
        pluck: h[2126] != 0,
        free: h[2123] != 0,
        declick: h[2125] != 0,
        centre,
        tables,
        caps,
    }
}

impl Default for Patch {
    /// One plain operator sent straight out.
    fn default() -> Self {
        let mut ops: [Op; 6] = Default::default();
        for o in ops.iter_mut() {
            *o = Op { ratio: 2.0, vol: 1.0, pitch_cents: 1200.0, tables: vec![Arc::new(sine().to_vec())], caps: vec![1], ..Default::default() };
        }
        let mut arts = Vec::with_capacity(ARTS);
        for k in 0..ARTS {
            arts.push(Art::neutral(bipolar_at(k)));
        }
        let mut out = [0.0; 9];
        out[0] = 1.0;
        let mut p = Self { uni_old_share: false, volume: 1.0, lfo: 1.0, pitch: 0.0, send: [0.0; 9], fm: [[0.0; 6]; 6], rm: [[0.0; 6]; 6], feed: [[0.0; 6]; 3], pan: [0.0; 9], out, ops, filts: Default::default(), arts, uni: 1, send_side: [1.0; 2], uni_pan: 1.0, uni_vol: 0.8, uni_pitch: 0.5, uni_phase: 0.1, uni_sub: 0.0, uni_mono: false, slide: None, pitch_shared: false, over: [0; 2], vel_vol: true, modx: 0.5, mody: 0.75, op_at: [0; 6], filt_at: [0; 3], lay: OpLay::NEW, tail: Default::default(), used: [false; 6], fused: [false; 3], any_moving: false };
        p.wire();
        p
    }
}

/// Seconds the centre switch takes to drain a standing level to 1/e.
const CENTRE_S: f32 = 0.0398;

fn bipolar_at(k: usize) -> bool {
    match k {
        0..=35 => matches!(k % 6, 0 | 3 | 4),
        36..=59 => matches!((k - 36) % 8, 0 | 2 | 3),
        _ => k == 60,
    }
}

impl Patch {
    /// Read a stored block. `None` when it is not one of the layouts we know.
    pub fn parse(state: &[u8]) -> Option<Self> {
        if state.len() < 8 {
            return None;
        }
        // newer blocks keep the layout number in the upper half
        let ver = u32::from_le_bytes(state[0..4].try_into().ok()?);
        let ver = if ver > 0xffff { ver >> 16 } else { ver };
        if !matches!(ver, 5..=7 | 12..=25) {
            return None;
        }
        let d = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&state[4..], 1 << 24).ok()?;
        Self::from_plain(&d, ver)
    }

    /// The stored block unpacked, for editing in place.
    pub fn unpack(state: &[u8]) -> Option<Vec<u8>> {
        Self::parse(state)?;
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&state[4..], 1 << 24).ok()
    }

    /// Changes a stored block: `f` works on the unpacked bytes. Gives the new block and what it now reads as, or
    /// nothing when the result would no longer read (the old block then stays).
    pub fn edited(state: &[u8], f: impl FnOnce(&mut Vec<u8>)) -> Option<(Vec<u8>, Self)> {
        let mut d = Self::unpack(state)?;
        f(&mut d);
        let mut out = state[..4].to_vec();
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
        let p = Self::parse(&out)?;
        Some((out, p))
    }

    /// A stored block for a new instrument: operator 1 as a plain sine straight out, everything else at rest.
    pub fn blank_state() -> Vec<u8> {
        fn line(kind: i32, pts: &[(f64, f64)], tail: [i32; 5], out: &mut Vec<u8>) {
            for v in [kind, 3, pts.len() as i32] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for &(x, y) in pts {
                out.extend_from_slice(&x.to_le_bytes());
                out.extend_from_slice(&y.to_le_bytes());
                out.extend_from_slice(&[0; 8]);
            }
            for v in tail {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        fn art(bipolar: bool, out: &mut Vec<u8>) {
            let y = if bipolar { 0.5 } else { 1.0 };
            out.extend_from_slice(&[0; 16]);
            line(1, &[(0.0, 0.0), (0.01, 1.0), (0.3, 0.6), (0.5, 0.0)], [0, 0, 1, -1, 2], out);
            for v in [128i32, 128, 0, 128] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            line(2, &[(0.0, 0.5), (1.0, 0.5)], [0, 0, -1, -1, -1], out);
            for v in [40000i32, 0, 0, 0, 0] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for _ in 0..6 {
                line(3, &[(0.0, y), (1.0, y)], [0, -1, -1, -1, -1], out);
            }
        }
        let mut d = vec![0u8; 1137];
        let put = |d: &mut Vec<u8>, o: usize, v: i32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut d, 0, 128);
        put(&mut d, 4, 128);
        put(&mut d, 12 + 8 * 4, 256);
        for o in [660, 664, 672, 676, 680, 688] {
            put(&mut d, o, 128);
        }
        put(&mut d, 692, 1);
        put(&mut d, 728, 512);
        put(&mut d, 732, 512);
        d.extend_from_slice(&0i32.to_le_bytes());
        for _ in 0..6 {
            let mut h = vec![0u8; 2128];
            h[4..8].copy_from_slice(&256i32.to_le_bytes());
            h[8..12].copy_from_slice(&1200i32.to_le_bytes());
            h[40..44].copy_from_slice(&20000i32.to_le_bytes());
            h[52..56].copy_from_slice(&1.0f32.to_le_bytes());
            h[2116..2120].copy_from_slice(&128i32.to_le_bytes());
            d.extend_from_slice(&h);
            for a in 0..6 {
                art(bipolar_at(a), &mut d);
            }
        }
        for _ in 0..3 {
            let mut h = vec![0u8; 88];
            for (o, v) in [(0, 0i32), (4, 128), (8, 256), (16, 128), (28, 128), (32, 128)] {
                h[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }
            d.extend_from_slice(&h);
            line(3, &[(0.0, 0.0), (1.0, 1.0)], [0, -1, -1, -1, -1], &mut d);
            d.push(0);
            for a in 0..8 {
                art(bipolar_at(36 + a), &mut d);
            }
        }
        d.extend_from_slice(&[0; 40]);
        art(true, &mut d);
        art(false, &mut d);
        d.push(0);
        let mut out = 22u32.to_le_bytes().to_vec();
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
        out
    }

    pub fn from_plain(d: &[u8], ver: u32) -> Option<Self> {
        if d.len() < 1200 {
            return None;
        }
        let mut p = Self::default();
        let (mut sent, mut plain) = (0.0f32, 0.0f32);
        // the main volume knob is not a straight line: measured at six settings on a plain tone, it follows
        // (e^(2.4 x) - 1) / (e^2.4 - 1) with x the knob over its middle, to within a part in a thousand
        p.volume = ((2.4 * at(d, 0).clamp(0, 256) as f32 / 128.0).exp() - 1.0) / (2.4f32.exp() - 1.0);
        p.lfo = at(d, 4).clamp(-128, 128) as f32 / 128.0;
        p.pitch = at(d, 8).clamp(-48, 48) as f32;
        // every knob of the grid follows one curve (measured on renders at a dozen settings): half way up
        // gives 0.23 of the full amount
        // each knob of the main grid has a mute switch, one byte per knob in the same nine by nine order
        // (measured: a muted knob plays as if it stood at zero, whatever it shows)
        let muted = |base: usize, r: usize, c: usize| base == 12 && d[800 + r * 9 + c] != 0;
        let cell = |base: usize, r: usize, c: usize| {
            if muted(base, r, c) {
                return 0.0;
            }
            grid(at(d, base + r * 36 + c * 4).clamp(-256, 256) as f32 / 256.0)
        };
        for r in 0..9 {
            p.pan[r] = if muted(12, r, 6) { 0.0 } else { at(d, 12 + r * 36 + 24).clamp(-256, 256) as f32 / 256.0 };
            let (send, out) = (cell(12, r, 7), cell(12, r, 8));
            sent += send.abs();
            plain += out.abs();
            p.out[r] = out;
            p.send[r] = send;
            for c in 0..6 {
                if r < 6 {
                    p.fm[r][c] = cell(12, r, c) * 2.0;
                    if r == c {
                        // an operator fed back into itself bends far less than one fed by another (measured
                        // on a rich wave at three settings and two pitches: one plain factor)
                        p.fm[r][c] *= SELF_FEED;
                    }
                    p.rm[r][c] = cell(336, r, c);
                } else {
                    p.feed[r - 6][c] = cell(12, r, c);
                }
            }
        }
        let gv = [knob(at(d, 660)), knob(at(d, 664)), knob(at(d, 672)), at(d, 668).max(0) as f32 / 128.0];
        let gf = [knob(at(d, 676)), knob(at(d, 680)), knob(at(d, 688)), at(d, 684).max(0) as f32 / 128.0];
        p.uni = at(d, 692).clamp(1, 9) as u32;
        p.uni_pan = at(d, 696).clamp(0, 128) as f32 / 128.0;
        p.uni_vol = at(d, 700).clamp(0, 128) as f32 / 128.0;
        p.uni_pitch = at(d, 704).clamp(0, 256) as f32 / 256.0;
        p.uni_phase = at(d, 712).clamp(0, 256) as f32 / 128.0;
        p.uni_sub = at(d, 708).clamp(0, 128) as f32 / 128.0;
        // measured: centre leaves both sides alone, either end gives one side 3 dB more and silences the other
        let turn = (1.0 + at(d, 720).clamp(-128, 128) as f32 / 128.0) * std::f32::consts::FRAC_PI_4;
        p.send_side = [std::f32::consts::SQRT_2 * turn.cos(), std::f32::consts::SQRT_2 * turn.sin()];
        p.modx = at(d, 760).clamp(0, 512) as f32 / 512.0;
        p.mody = at(d, 764).clamp(0, 512) as f32 / 512.0;
        // the patch notes: where they start moved between layouts. Newer blocks hold a one-byte length and two
        // bytes per letter, older ones a four-byte length and one byte per letter
        let at_text = match ver {
            5 => 1075,
            6 => 1076,
            7 => 1077,
            12 => 1115,
            13 => 1116,
            14 => 1129,
            15 | 17 => 1133,
            18 => 1136,
            _ => 1137,
        };
        // the effects of an older block are read through the current layout
        let widened;
        let e = if ver < 19 {
            widened = current_head(d, ver, at_text);
            &widened[..]
        } else {
            d
        };
        // the switches after the head were added at its end one layout at a time, so each sits at the same place in
        // every layout that has it (measured: the set in the middle from layout 13 on at least, the velocity
        // switch from 18)
        p.uni_mono = ver >= 7 && d.len() > 966 && d[966] != 0;
        // (measured by flipping each byte: the slide's own switch, the one that lets distance set its time, and
        // the limit on its distance, all there from layout 15 on)
        p.slide = (ver >= 15 && d.len() > 973 && d[970] != 0).then(|| (d[971] != 0, d[973]));
        if d.len() > 790 {
            p.over = [d[784].min(6), d[780].min(6)];
            p.pitch_shared = d[790] != 0;
        }
        p.vel_vol = ver < 18 || d.len() <= 974 || d[974] != 0;
        // (measured: the byte after the unison one lets a standing level drain away)
        let level = ver >= 7 && d.len() > 967 && d[967] != 0;
        if sent > 0.0 && e.len() >= 1137 {
            p.tail = effects(e);
            p.tail.split = true;
        } else {
            p.tail = Default::default();
            for r in 0..9 {
                p.out[r] += p.send[r].abs();
                p.send[r] = 0.0;
            }
        }
        p.tail.level = level;
        if e.len() >= 1137 {
            // the equaliser stands on the output, on the effects' share, or on both; here it is one filter
            // over the whole sound, so it runs unless it is meant for a share that is not the greater part
            let _ = plain;
            p.tail.tone_at = e[1085];
            if (1..=3).contains(&e[1085]) && (sent > 0.0 || e[1085] != 3) {
                p.tail.tone = equaliser(e);
            }
        }
        let mut r = R { d, p: at_text, lfo_rest: 16 };
        if ver >= 23 {
            let mut n = *r.bytes(1)?.first()? as usize;
            if n == 255 {
                n = usize::try_from(r.i()?).ok()?;
            }
            r.bytes(n.checked_mul(2)?)?;
        } else {
            let n = r.i()?;
            r.bytes(usize::try_from(n).ok()?)?;
        }
        let start = r.p;
        p.uni_old_share = ver < 17 || ver >= 24;
        p.lay = if ver < 12 {
            OpLay::FIRST
        } else if ver < 17 {
            OpLay::MID
        } else {
            OpLay::NEW
        };
        for lfo_rest in [16, 12] {
            let mut q = p.clone();
            if q.modules(R { d, p: start, lfo_rest }, gv, gf).is_some() {
                q.wire();
                return Some(q);
            }
        }
        None
    }

    fn modules(&mut self, mut r: R, gv: [f32; 4], gf: [f32; 4]) -> Option<()> {
        let p = self;
        let one = [1.0, 1.0, 1.0, 0.0];
        p.arts.clear();
        for k in 0..6 {
            p.op_at[k] = r.p;
            p.ops[k] = op_from(r.bytes(p.lay.len)?, &p.lay);
            for a in 0..6 {
                let g = if a == 1 { gv } else { one };
                p.arts.push(r.art(bipolar_at(a), g)?);
            }
        }
        for k in 0..3 {
            p.filt_at[k] = r.p;
            let first = p.lay.first;
            let h = r.bytes(if first { 81 } else { 88 })?;
            let i = |o: usize| at(h, o);
            let mut ws = r.curve(3)?;
            ws.prepare([1.0; 3]);
            let bip = if first { false } else { *r.bytes(1)?.first()? != 0 };
            let f = &mut p.filts[k];
            *f = Filt {
                slope: i(0).clamp(0, 5),
                env_amt: i(4).clamp(0, 256) as f32 / 256.0,
                cut: i(8).clamp(0, 256) as f32 / 256.0,
                res: i(12).clamp(0, 256) as f32 / 256.0,
                levels: [grid(i(16).clamp(0, 128) as f32 / 128.0), grid(i(20).clamp(0, 128) as f32 / 128.0), grid(i(24).clamp(0, 128) as f32 / 128.0)],
                // measured with a straight curve: the knob is a plain factor, one at its middle
                ws_gain: i(28).clamp(0, 256) as f32 / 128.0,
                ws_mix: i(32).clamp(0, 128) as f32 / 128.0,
                next: i(36).clamp(-256, 256) as f32 / 256.0,
                ws_on: if first { h[80] != 0 } else { i(80) != 0 },
                kind: if first { 0 } else { i(84).clamp(0, 12) },
                db: (i(16).clamp(0, 128) - 64) as f32 / 64.0 * 36.0,
                drive: i(16).clamp(0, 128) as f32 / 128.0,
                ws_bipolar: bip,
                ws: (0..=256).map(|j| ws.map(j as f32 / 256.0)).collect(),
                hq: !first && h[40] != 0,
                flat: h[41] != 0,
            };
            for a in 0..8 {
                let g = match a {
                    1 => gv,
                    2 | 3 => gf,
                    _ => one,
                };
                p.arts.push(r.art(bipolar_at(36 + a), g)?);
            }
        }
        r.bytes(40)?;
        p.arts.push(r.art(true, one)?);
        p.arts.push(r.art(false, gv)?);
        // nothing but a few closing bytes may follow, or the layout was not what we took it for
        (r.d.len() - r.p <= 8).then_some(())
    }

    /// Work out which modules can be heard.
    fn wire(&mut self) {
        let mut f = [false; 3];
        for k in (0..3).rev() {
            f[k] = self.out[6 + k] != 0.0 || self.send[6 + k] != 0.0 || (k < 2 && self.filts[k].next != 0.0 && f[k + 1]);
        }
        let mut u = [false; 6];
        for k in 0..6 {
            u[k] = self.out[k] != 0.0 || self.send[k] != 0.0 || (0..3).any(|j| f[j] && self.feed[j][k] != 0.0);
        }
        loop {
            let mut grew = false;
            for src in 0..6 {
                if !u[src] && (0..6).any(|dst| u[dst] && (self.fm[dst][src] != 0.0 || self.rm[dst][src] != 0.0)) {
                    u[src] = true;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        self.used = u;
        self.fused = f;
        self.any_moving = self.arts.iter().any(|a| a.moving());
    }

    /// Seconds the sound needs after the key is let go `t` seconds into the note: until every volume
    /// envelope that is on has ended (measured: one on a filter nothing feeds still holds the note open).
    pub fn tail_s(&self, t: f32, beat: f32) -> f32 {
        let mut tail = 0.0f32;
        // every volume envelope that is switched on counts, even one on a part nothing goes through (measured:
        // a short note rang on for the release of a filter that was fed nothing, and parts with no envelope of
        // their own sounded right to the end of it)
        let vols = (0..6).map(|k| k * 6 + 1).chain((0..3).map(|k| 36 + k * 8 + 1));
        for k in vols {
            let e = &self.arts[k].env;
            if e.on() {
                let b = if e.flags & 1 != 0 { beat.max(1e-3) } else { 2.0 };
                tail = tail.max(e.tail(t * b) / b);
            }
        }
        // the drawn rise and fall of a slow wave that is switched on holds the note open the same way, whatever
        // it moves (measured with one on an operator's volume and one on its pitch, each alone: a note with no
        // volume envelope left rang for the nine tenths of a second after the wave's hold point)
        for a in self.arts.iter().take(ARTS) {
            let e = &a.lfo;
            if e.on() {
                let b = if e.flags & 1 != 0 { beat.max(1e-3) } else { 2.0 };
                tail = tail.max(e.tail(t * b) / b);
            }
        }
        tail.min(12.0)
    }

    pub fn used(&self) -> [bool; 6] {
        self.used
    }
}

#[derive(Debug, Clone)]
struct Sub {
    stat: [f32; ARTS],
    val: [f32; ARTS],
    detune: f32,
    pan: f32,
    gain: f32,
    /// (kept finer than the sound itself: measured, a long note stays in step to the sample)
    ph: [f64; 6],
    out: [f32; 6],
    prev: [f32; 6],
    /// Each operator's last value before its level was applied.
    raw: [f32; 6],
    amp: [f32; 6],
    /// the softened start of each operator
    soft: [f32; 6],
    /// the standing level each centred operator has gathered
    dc: [f32; 6],
    inc: [f32; 6],
    tab: [u8; 6],
    pl: [f32; 9],
    pr: [f32; 9],
    fs: [[[f32; 2]; 6]; 3],
    plan: [[[f32; 6]; 6]; 3],
    stages: [u8; 3],
    ap: [[f32; 20]; 3],
    /// what takes each filter up to twice the rate and back
    twice: [Twice; 3],
    /// what brings the filters' four outputs (out and send, left and right) back down together
    mixd: [Twice; 4],
    fb: [f32; 3],
    fg: [f32; 3],
    fk: [f32; 3],
    /// Where the hiss stands in its run, which starts from the same place on every note.
    hseed: u32,
    /// Plucked operators: the first cycle each one made, going round and wearing down (empty for the others).
    strings: [Vec<f32>; 6],
    /// How far through that first cycle the operator is, the last place written, and time not yet worn off.
    run: [f32; 6],
    fill: [u32; 6],
    due: [f32; 6],
    hiss: [f32; 6],
}

/// Halves the sample rate: two rows of all-pass stages, one a frame behind the other, whose sum keeps what is
/// below a quarter of the rate and drops what is above. It is flat right up to the top of what is kept and
/// only a couple of frames late (measured against hiss: the original's top octave is not rolled off).
#[derive(Debug, Clone)]
struct Halver {
    /// per stage and side: the last two frames in and out
    z: [[[f32; 4]; 2]; 12],
    late: [f32; 2],
}

/// The stage factors of one halving of a voice worked out above the rate asked for, first row then second
/// (fitted to renders of a narrow pulse at every setting: all the steps are alike, each passes everything up
/// to its new top and is three decibels down right at it).
const HALF: [f32; 6] = [0.039_185, 0.302_594, 0.674_638, 0.147_348, 0.482_520, 0.882_997];

/// The stage factors of the two halving steps, first row then second row (the first step works at four times
/// the rate, where what matters is far below the edge, and can be short).
fn half_taps() -> &'static [Vec<f32>; 2] {
    static T: std::sync::OnceLock<[Vec<f32>; 2]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        [
            vec![0.079_866_426, 0.545_353_65, 0.283_829_35, 0.834_411_9],
            vec![
                0.036_681_502, 0.274_631_76, 0.561_098_97, 0.769_741_83, 0.892_260_8, 0.962_094_55, 0.136_547_62, 0.423_138_62, 0.677_540_05, 0.839_889_6,
                0.931_541_96, 0.987_816_4,
            ],
        ]
    })
}

impl Halver {
    fn new() -> Self {
        Self { z: [[[0.0; 4]; 2]; 12], late: [0.0; 2] }
    }

    fn push(&mut self, a: [f32; 2], b: [f32; 2], taps: &[f32]) -> [f32; 2] {
        let half = taps.len() / 2;
        let mut out = [0.0f32; 2];
        for x in [a, b] {
            for side in 0..2 {
                let mut rows = [x[side]; 2];
                for (k, &t) in taps.iter().enumerate() {
                    let z = &mut self.z[k][side];
                    let v = &mut rows[k / half];
                    let y = z[1] + (*v - z[3]) * t;
                    *z = [*v, z[0], y, z[2]];
                    *v = y;
                }
                out[side] = 0.5 * (rows[0] + self.late[side]);
                self.late[side] = rows[1];
            }
        }
        out
    }
}

/// Takes one line of sound up to twice the rate and back down with the same two rows of all-pass stages, so
/// that a filter between the two steps works as it would at the higher rate with nothing lost at the top.
#[derive(Debug, Clone, Copy)]
struct Twice {
    up: [[f32; 2]; 12],
    down: [[f32; 4]; 12],
    late: f32,
}

impl Twice {
    fn new() -> Self {
        Self { up: [[0.0; 2]; 12], down: [[0.0; 4]; 12], late: 0.0 }
    }

    fn up(&mut self, x: f32) -> [f32; 2] {
        let taps = &half_taps()[1];
        let half = taps.len() / 2;
        let mut rows = [x; 2];
        for (k, &t) in taps.iter().enumerate() {
            let z = &mut self.up[k];
            let v = &mut rows[k / half];
            let y = z[0] + (*v - z[1]) * t;
            *z = [*v, y];
            *v = y;
        }
        rows
    }

    fn down(&mut self, pair: [f32; 2]) -> f32 {
        let taps = &half_taps()[1];
        let half = taps.len() / 2;
        let mut out = 0.0;
        for x in pair {
            let mut rows = [x; 2];
            for (k, &t) in taps.iter().enumerate() {
                let z = &mut self.down[k];
                let v = &mut rows[k / half];
                let y = z[1] + (*v - z[3]) * t;
                *z = [*v, z[0], y, z[2]];
                *v = y;
            }
            out = 0.5 * (rows[0] + self.late);
            self.late = rows[1];
        }
        out
    }
}

/// One sounding note.
#[derive(Debug, Clone)]
pub struct Voice {
    /// the chance start each free-running operator was given
    loose: [f32; 6],
    /// how many frames are worked out for each one given: operators that bend each other make partials far
    /// above the top of the hearing range, and those must be made and filtered away, not folded back down
    over: u32,
    /// steps each filter takes per frame
    fsteps: u8,
    /// the slide in from the note before, for each operator: how far (in the pitch output's own measure) and
    /// for how many seconds (beats, when below nothing)
    glide: [(f32, f32, f32); 6],
    halvers: Vec<Halver>,
    /// the same for what goes to the effects, and that share of the last frame made
    send_halvers: Vec<Halver>,
    sent: [f32; 2],
    sr: f32,
    t: u32,
    rel: Option<f32>,
    ctl: u32,
    /// beats in a second, for lines that follow the tempo
    pub beat: f32,
    /// where in the song the note began, in seconds
    song: f64,
    subs: Vec<Sub>,
    main_l: f32,
    main_r: f32,
}

#[inline]
fn rnd(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

impl Voice {
    /// `axes` places the note on the velocity, X and Y axes of the maps (each 0 to 1).
    pub fn start(p: &Patch, sr: f32, key: u8, axes: [f32; 3], dphase: f32, seed: u32) -> Self {
        Self::start_as(p, sr, key, axes, dphase, seed, false, None)
    }

    /// The same; `live` picks the patch's setting for live play over the one for renders.
    /// `from` is the key of the note the channel played before this one.
    pub fn start_as(p: &Patch, sr: f32, key: u8, axes: [f32; 3], dphase: f32, seed: u32, live: bool, from: Option<u8>) -> Self {
        let mut seed = seed | 1;
        let over = 1u32 << p.over[if live { 1 } else { 0 }];
        let (sr, dphase) = (sr * over as f32, dphase / over as f32);
        let n = p.uni.clamp(1, 9) as usize;
        let copies = if p.uni_sub > 0.0 { 2 } else { 1 };
        let mut subs = Vec::with_capacity(n * copies);
        // (measured: a free-running operator starts anywhere, but all the voices of a unison set start together)
        let loose: [f32; 6] = std::array::from_fn(|_| rnd(&mut seed) * 0.5 + 0.5);
        for s2 in 0..n * copies {
            let (s, low) = (s2 % n, s2 >= n);
            // (measured: every voice of a unison set draws its own chance value, so a phase mapped to chance
            // leaves the voices out of step and the set as loud as that many strangers)
            let rand = rnd(&mut seed) * 0.5 + 0.5;
            let (down, share) = if low { (0.5, p.uni_sub) } else { (1.0, 1.0 - p.uni_sub) };
            let u = if n == 1 { 0.0 } else { 1.0 - s as f32 / (n - 1) as f32 * 2.0 };
            // (measured: the key axis counts the key after the patch's own transposing)
            // (measured on a six voice patch with pitch and phase mapped over the set: the first voice sits at the
            // left end of the unison axis)
            let xs = [((key as f32 + p.pitch) / 120.0).clamp(0.0, 1.0), axes[0].clamp(0.0, 1.0), (axes[1] - 0.5 + p.modx).clamp(0.0, 1.0), (axes[2] - 0.5 + p.mody).clamp(0.0, 1.0), rand, (1.0 - u) * 0.5];
            let mut stat = [0.0; ARTS];
            for (k, a) in p.arts.iter().enumerate().take(ARTS) {
                stat[k] = a.stat(&xs);
            }
            // (measured on three layouts: the octave-down copy of a voice takes the next voice's share)
            let spread = (u.abs() * if (if n % 2 == 1 && s > n / 2 { s - 1 } else { s }) % 2 == 0 { 1.0 } else { -1.0 } * uni_spread(p.uni_pitch, p.lay.early()) * if p.uni_old_share { UNI_SHARE_OLD } else { UNI_SHARE }[(s + low as usize) % 9] / 1200.0).exp2();
            let mut ph = [0.0f64; 6];
            let mut tab = [0u8; 6];
            for k in 0..6 {
                let o = &p.ops[k];
                // measured on the first two layouts, and again on states of version 24: the voices' starts are spread evenly over the set share of one
                // cycle (two cycles at the top of the knob). Measured on the rest, at eight places on the knob,
                // two spreads and two keys: each voice starts as far round as its own detuning would carry it in
                // a set count of cycles, a hundred thousand at the top and falling with the eighth power of the
                // knob, so the low third of the knob leaves the voices together
                ph[k] = o.phase as f64 + if p.uni_old_share { (s as f32 / n as f32 * p.uni_phase) as f64 } else {
                    ((spread - 1.0) as f64 * 1.0e5 * (p.uni_phase as f64).min(1.0).powi(8)).rem_euclid(1.0)
                };
                // (measured: an operator that stands still, at no pitch of its own, starts at its set place
                // whatever this switch says)
                if o.free && (o.ratio != 0.0 || o.offset_hz != 0.0) {
                    ph[k] += loose[k] as f64;
                }
                // the fullest table whose top harmonic still fits under half the sample rate
                // (the patch's own transposing counts: measured, a saw set two octaves down keeps its top)
                let f = (dphase * (p.pitch / 12.0).exp2() * down * o.ratio + o.offset_hz / sr).abs().max(1e-6);
                let fit = (0.499 / f) as u32;
                tab[k] = o.caps.iter().position(|&c| c <= fit.max(1)).unwrap_or(o.caps.len() - 1) as u8;
            }
            subs.push(Sub {
                strings: std::array::from_fn(|k| if p.ops[k].pluck && p.used[k] { vec![0.0; STR] } else { Vec::new() }),
                run: [0.0; 6],
                fill: [u32::MAX; 6],
                due: [0.0; 6],
                hiss: [0.0; 6],
                stat,
                val: [0.0; ARTS],
                // measured against renders: the voices run from the left to the right, each as far off pitch as
                // it is off the middle, sharp and flat by turns (a voice in the very middle takes no turn); the
                // spread grows as a power of the knob (3.2 cents at a quarter, 11.5 at the middle, 32 at three
                // quarters, 85 at the top) and every voice has a fixed share of it, the same on every note; the level knob fades in the outer voices of three or more and the whole set is scaled
                // by 1 / (0.621 n + 0.34)
                detune: spread * down,
                pan: if p.uni_mono { 0.0 } else { -u * p.uni_pan },
                gain: share * match n {
                    1 => 1.0,
                    2 => 1.0 / (0.621 * 2.0 + 0.34),
                    _ => (p.uni_vol + 2.0 * (1.0 - p.uni_vol) * (1.0 - u.abs())) / (0.621 * n as f32 + 0.34),
                },
                ph,
                out: [0.0; 6],
                prev: [0.0; 6],
                raw: [0.0; 6],
                amp: [0.0; 6],
                soft: [0.0; 6],
                dc: [0.0; 6],
                inc: [0.0; 6],
                tab,
                pl: [1.0; 9],
                pr: [1.0; 9],
                fs: [[[0.0; 2]; 6]; 3],
                plan: [[[0.0; 6]; 6]; 3],
                stages: [0; 3],
                ap: [[0.0; 20]; 3],
                twice: [Twice::new(); 3],
                mixd: [Twice::new(); 4],
                fb: [0.0; 3],
                fg: [0.1; 3],
                fk: [1.0; 3],
                // the voices of a unison set share one run of hiss (measured: it stays in the middle and adds in step)
                hseed: 3_099_528_473u32,
            });
        }
        // with the slide switch on, an operator whose pitch envelope has an attack part slides in from the note
        // before over that part (measured from six distances: straight in pitch at an octave per length of that
        // part, so a far note takes longer; on top of whatever the envelope itself does; no attack marker or no
        // envelope, no slide), and it follows the bend drawn on that part: a fully tense piece arrives almost at
        // once. The distance can be folded into an octave or held to a few semitones, and with the time switch
        // off every slide takes the length of the part whatever its distance.
        let mut glide = [(0.0f32, 0.0f32, 1.0f32); 6];
        if let (Some(from), Some((by_distance, limit))) = (from.filter(|f| *f != key), p.slide) {
            let st = from as f32 - key as f32;
            let st = match limit {
                0 => st,
                1 => st.signum() * ((st.abs() - 1.0).rem_euclid(12.0) + 1.0),
                2..=5 => st.clamp(limit as f32 - 6.0, 6.0 - limit as f32),
                _ => 0.0,
            };
            let far = st / 12.0;
            for (k, g) in glide.iter_mut().enumerate() {
                let (Some(a), reach) = (p.arts.get(k * 6 + 3), p.ops[k].pitch_cents) else { continue };
                let at = a.env.decay;
                if a.env.on() && at > 0 && reach > 0.0 && far != 0.0 {
                    if let Some(len) = a.env.t.get(at as usize).copied().filter(|l| *l > 0.0) {
                        let by = if by_distance { far.abs() } else { 1.0 };
                        *g = (far * 1200.0 / reach, by * if a.env.flags & 1 != 0 { -len } else { len * 0.5 }, by);
                    }
                }
            }
        }
        let mut v = Self { glide, over, fsteps: if over == 1 && !(p.lay.first && p.filts.iter().any(|f| f.flat)) { 2 } else { 1 }, halvers: vec![Halver::new(); p.over[if live { 1 } else { 0 }] as usize], send_halvers: vec![Halver::new(); p.over[if live { 1 } else { 0 }] as usize], sent: [0.0; 2], sr, t: 0, rel: None, ctl: 0, beat: 2.0, song: 0.0, loose, subs, main_l: 1.0, main_r: 1.0 };
        v.control(p, dphase, true);
        for s in v.subs.iter_mut() {
            for k in 0..6 {
                s.amp[k] = s.val[k * 6 + 1] * p.ops[k].vol;
            }
        }
        v
    }

    /// The key was let go.
    pub fn release(&mut self) {
        if self.rel.is_none() {
            self.rel = Some(self.t as f32 / self.sr);
        }
    }

    pub fn seconds(&self) -> f32 {
        self.t as f32 / self.sr
    }

    /// Place the note in the song: free-running operators stand where the song's clock has carried them.
    pub fn started_at(&mut self, p: &Patch, hz: f32, secs: f64) {
        self.song = secs;
        // a free-running operator turns with the song, not with the note: where it stands when a note begins is
        // its own pitch times the song's time (measured on two notes an octave apart: both starts fit one clock
        // running a little under three thousandths of a second ahead); the chance start stays for notes played
        // by hand, which have no place in the song
        for s in self.subs.iter_mut() {
            for k in 0..6 {
                let o = &p.ops[k];
                if o.free && (o.ratio != 0.0 || o.offset_hz != 0.0) {
                    // (each voice of a unison set by its own detuned pitch, on every layout)
                    let own = s.detune;
                    // (the pitch its maps set does not count: measured on one voice with a map bending it)
                    let f = (hz * (p.pitch / 12.0).exp2() * own * o.ratio + o.offset_hz) as f64;
                    // (measured on the later layouts at six places on the unison phase knob, no detuning: the whole
                    // set stands the knob's count of cycles further round, where an operator that is not free
                    // keeps only each voice's share of it)
                    if !p.uni_old_share {
                        s.ph[k] += (1.0e5 * (p.uni_phase as f64).min(1.0).powi(8)).rem_euclid(1.0);
                    }
                    s.ph[k] += (f * (secs + std::env::var("CEREAL_FREE_AHEAD").ok().and_then(|v| v.parse::<f64>().ok()).map_or(FREE_AHEAD, |v| v / 44100.0))).rem_euclid(1.0) - self.loose[k] as f64;
                }
            }
        }
        // (measured on every layout with the phase knob at nothing: operators that are not free start each
        // note with their voices together)
    }

    fn control(&mut self, p: &Patch, dphase: f32, first: bool) {
        let t = self.t as f32 / self.sr;
        let rel = self.rel;
        let sr = self.sr;
        let beat = self.beat;
        let song = self.song;
        for s in self.subs.iter_mut() {
            if first || p.any_moving {
                for (k, a) in p.arts.iter().enumerate().take(ARTS) {
                    if first || a.moving() {
                        // (measured: while a note slides in from the last key, the attack part of its pitch
                        // envelope takes as long as the slide does)
                        let mut slide = (0.0, 1.0);
                        if k < 36 && k % 6 == 3 && self.glide[k / 6].1 != 0.0 {
                            let (_, len, by) = self.glide[k / 6];
                            slide = (if len < 0.0 { -len / beat.max(1e-3) } else { len }, by);
                        }
                        s.val[k] = a.value(s.stat[k], t, rel, p.lfo, beat, song, slide);
                    }
                }
            }
            let mut shared = 0.0;
            for k in 0..6 {
                let o = &p.ops[k];
                // (measured: the pitch output bends the fixed part of the frequency by the same interval)
                let (far, len, _) = self.glide[k];
                let len = if len < 0.0 { -len / beat.max(1e-3) } else { len };
                let slide = if t < len { far * (1.0 - p.arts[k * 6 + 3].env.attack_way(t / len)) } else { 0.0 };
                let mut cents = (s.val[k * 6 + 3] + slide) * o.pitch_cents;
                // (measured: with the shared pitch switch on, the first operator's pitch movement, slide and
                // all, moves every operator; off, an operator with no movement of its own stays put)
                if k == 0 {
                    shared = if p.pitch_shared { cents } else { 0.0 };
                } else {
                    cents += shared;
                }
                let art = (cents / 1200.0).exp2();
                let bend = (p.pitch / 12.0).exp2() * art;
                s.inc[k] = (dphase * s.detune * bend * o.ratio + o.offset_hz * art / sr).clamp(-0.5, 0.5);
                let pan = (p.pan[k] + s.val[k * 6]).clamp(-1.0, 1.0);
                (s.pl[k], s.pr[k]) = pan_gains(pan);
                if o.pluck && s.run[k] >= 1.0 {
                    s.due[k] += (TICK * self.over) as f32 / sr;
                    if s.due[k] >= 0.0029 {
                        let hz = (s.inc[k] * sr).abs().max(1.0);
                        wear(&mut s.strings[k], hz, s.due[k], s.val[k * 6 + 5]);
                        s.due[k] = 0.0;
                    }
                }
            }
            for k in 0..3 {
                let f = &p.filts[k];
                let b = 36 + k * 8;
                let pan = (p.pan[6 + k] + s.val[b]).clamp(-1.0, 1.0);
                (s.pl[6 + k], s.pr[6 + k]) = pan_gains(pan);
                // (measured: the maps move the corner by half the range whatever the envelope knob says; the
                // knob scales only what moves in time)
                let c = (f.cut + s.stat[b + 2] * 0.5 + (s.val[b + 2] - s.stat[b + 2]) * f.env_amt).clamp(0.0, 1.0);
                let r = (f.res + s.val[b + 3] * 0.5).clamp(0.0, 1.0);
                let sr = sr * self.fsteps as f32;
                let g_at = |hz: f32| (PI * hz.min(sr * 0.45) / sr).tan();
                s.fg[k] = g_at(band_hz(c));
                s.fk[k] = 2.0 - 1.9 * r;
                let n = match f.slope {
                    0 => 0,
                    4 => 2,
                    5 => 3,
                    n => n as usize,
                };
                // one stage: its three step factors, then how much of the input, band and low outputs is kept
                let stage = |g: f32, k: f32, m: [f32; 3]| {
                    let a1 = 1.0 / (1.0 + g * (g + k));
                    [a1, g * a1, g * g * a1, m[0], m[1], m[2]]
                };
                let width = |octaves: f32| 2.0 * (std::f32::consts::LN_2 * 0.5 * octaves).sinh();
                let mut count = n;
                let plan = &mut s.plan[k];
                match f.kind {
                    7 => {
                        // (measured by the turn of a sine at four corners and two slopes: the all-pass is damped
                        // like the high-pass, the knob's wide law shared out over its stages)
                        let wide = if f.flat { 9.94 * (-4.33 * (r * (0.70 + 0.59 * c)).max(0.0).powf(1.45)).exp() } else { table(&DAMP_WIDE, r) };
                        let kk = wide.powf(1.0 / n.max(1) as f32);
                        plan[..n].fill(stage(s.fg[k], kk, [1.0, -2.0 * kk, 0.0]));
                        let far: [f32; 2] = if f.slope == 4 { [0.43, 0.43] } else { [0.71, 0.49] };
                        if f.slope >= 4 {
                            for st in 1..n {
                                plan[st] = stage(g_at(band_hz(1.0 - far[st - 1] * (1.0 - c))), kk, [1.0, -2.0 * kk, 0.0]);
                            }
                        }
                    }
                    4 | 5 => {
                        // band kinds keep their overall width whatever the slope: each stage of a band-pass
                        // is made wider, each stage of a notch narrower
                        // (the second switch gives the band-pass and the first notch a narrower law that
                        // widens with the corner)
                        let w = if f.flat {
                            // (below a quarter of the knob it runs down to the same needle as without the switch)
                            let law = |r: f32| 0.041 * (4.43 * r + 0.96 * (c - 0.5)).exp();
                            let kf = if r >= 0.25 { law(r) } else { (0.0072f32.ln() * (1.0 - r * 4.0) + law(0.25).ln() * r * 4.0).exp() };
                            2.0 * (kf * 0.5).asinh() / std::f32::consts::LN_2
                        } else {
                            table(&WIDTH, r)
                        };
                        let steps = f.slope.max(1) as f32;
                        let kk = width(if f.kind == 5 { w / n.max(1) as f32 } else { w * steps });
                        let m = match f.kind {
                            4 => [0.0, kk, 0.0],
                            _ => [1.0, -kk, 0.0],
                        };
                        plan[..n].fill(stage(s.fg[k], kk, m));
                        if f.kind == 4 && f.slope >= 4 {
                            // the two widest band-passes move their later stages up, which spreads the band
                            let far = if f.slope == 4 { 0.435 } else { 0.613 };
                            plan[1..n].fill(stage(g_at(band_hz(1.0 - far * (1.0 - c))), kk, m));
                        }
                    }
                    6 => {
                        let wide = if f.flat {
                            9.94 * (-4.33 * (r * (0.70 + 0.59 * c)).max(0.0).powf(1.45)).exp()
                        } else {
                            table(&DAMP_WIDE, r)
                        };
                        let kk = wide.powf(1.0 / n.max(1) as f32);
                        plan[..n].fill(stage(s.fg[k], kk, [1.0, -kk, -1.0]));
                        // the two widest slopes move their later stages up, like the low-pass family
                        let far: [f32; 2] = if f.slope == 4 { [0.43, 0.43] } else { [0.71, 0.49] };
                        if f.slope >= 4 {
                            for st in 1..n {
                                plan[st] = stage(g_at(band_hz(1.0 - far[st - 1] * (1.0 - c))), kk, [1.0, -kk, -1.0]);
                            }
                        }
                    }
                    8..=10 => {
                        // shelves and the peak are one stage whatever the slope says
                        count = n.min(1);
                        let a = 10f32.powf(f.db / 40.0);
                        plan[0] = if f.kind == 9 {
                            // (sharper with the second switch on)
                            let q = if f.flat { (99.5 * (-5.64 * r).exp()).min(137.9) } else { table(&PEAK_Q, r) };
                            let kk = 1.0 / (q * a);
                            stage(s.fg[k], kk, [1.0, kk * (a * a - 1.0), 0.0])
                        } else {
                            let kk = ((a + 1.0 / a) * (1.0 / table(&SHELF_SLOPE, r) - 1.0) + 2.0).max(0.01).sqrt();
                            if f.kind == 8 {
                                stage(s.fg[k] / a.sqrt(), kk, [1.0, kk * (a - 1.0), a * a - 1.0])
                            } else {
                                stage((s.fg[k] * a.sqrt()).min(20.0), kk, [a * a, kk * (1.0 - a) * a, 1.0 - a * a])
                            }
                        };
                    }
                    12 => {
                        // the phaser: its own corner law, and the resonance knob is how much comes back round
                        count = 0;
                        // (corner fitted to where the peaks of a ten-stage row sit at four settings of the knob;
                        // the row's coefficient comes straight from the corner, with no bending for the sample rate)
                        s.fg[k] = PI * (93.9 * 150f32.powf(c) - 86.4).clamp(1.0, sr * 0.45) / sr;
                        s.fk[k] = 0.916 * r.powf(0.40);
                    }
                    11 => {
                        // the drive kind is a ladder of four plain stages per slope step, each with the
                        // resonance fed back round it and half of the loss made up
                        count = n;
                        // (measured near and at wide open: past 15.85 kHz the corner climbs more slowly)
                        let over = (180.3 * 167.3f32.powf(c) - 70.3 - 15850.0).max(0.0);
                        let g = g_at(180.3 * 167.3f32.powf(c) - 70.3 - over + over / (1.0 + over / 26000.0));
                        let big = g / (1.0 + g);
                        // (fitted to the response at four settings of the knob and three slopes, from a
                        // low note rich in harmonics: every ladder has the resonance round it, less of it
                        // the more ladders there are, and a third more once there is drive)
                        let rows = n.max(1) as f32 - 1.0;
                        let lean = if f.drive > 0.0 { 0.75 } else { 1.0 };
                        let kb = r / ((0.265 + 0.073 * rows) * lean + 0.036 * rows * r);
                        for slot in plan.iter_mut().take(n) {
                            *slot = [big, kb, 1.0 + 0.51 * kb, 1.0 / (1.0 + kb * big * big * big * big), 0.0, 0.0];
                        }
                        if f.drive > 0.0 {
                            // with any drive the ladder works on a scaled copy of the signal: how far it is
                            // pushed into the bend, and what brings it back out
                            // (measured with a map on the drive output: it scales the knob)
                            let lv = f.drive * 128.0 * s.val[b + 4].clamp(0.0, 1.0);
                            // (measured at seven settings with a quiet and a loud sine: what comes out of
                            // the bend flat out grows by `UP`, quiet sound by `MORE` on top of that)
                            let base = 1.692 * 17.9f32.powf((lv - 64.0) / 64.0);
                            const UP: [f32; 5] = [0.0, 1.9, 6.1, 10.6, 16.4];
                            const MORE: [f32; 5] = [0.0, 2.0, 6.0, 12.3, 19.0];
                            let at = ((lv - 64.0) / 16.0).clamp(0.0, 4.0);
                            let i = (at as usize).min(3);
                            let db = UP[i] + (UP[i + 1] - UP[i]) * (at - i as f32);
                            let more = MORE[i] + (MORE[i + 1] - MORE[i]) * (at - i as f32);
                            // (three slopes measured since: each ladder is pushed and pulled on its own, by
                            // half of what a pair showed, and comes out 3 dB lower flat out. Tried since: starting
                            // the bend later suits the overtones of a quiet sine but leaves real patches too loud)
                            plan[0][5] = 10f32.powf((db - 3.0) / 20.0) / base;
                            plan[0][4] = 10f32.powf((db + more) / 40.0) / plan[0][5];
                        }
                    }
                    0 if p.lay.first && f.flat => {
                        // The first kind with its second switch on, in the oldest blocks (measured with hiss, one knob at a time): every
                        // stage is the plain two-store filter stepped once a sample, whose damping times its
                        // step is set by the resonance knob alone, so it keeps a set width in Hz wherever
                        // the corner sits. One stage follows the knob; the others stay at the knob's bottom,
                        // where the filter is no more than one gentle pole. Each stage mixes its own low,
                        // band and high outputs by the three level knobs and hands that on to the next.
                        // (a map can push the corner past the top of the knob)
                        let c = (f.cut + s.stat[b + 2] * 0.5 + (s.val[b + 2] - s.stat[b + 2]) * f.env_amt).clamp(0.0, 1.1);
                        let hz = (257.4 * 28.5f32.powf(c) - 170.0).clamp(20.0, sr * 0.3);
                        let step = 2.0 * (PI * hz / sr).sin();
                        let lv = [f.levels[0] * s.val[b + 4], f.levels[1] * s.val[b + 5], f.levels[2] * s.val[b + 6]];
                        for (st, slot) in plan.iter_mut().enumerate().take(count) {
                            let damp = if st == 0 { (1.0 - 0.8 * r) * (1.0 - 0.8 * r) } else { 1.0 };
                            *slot = [step, damp / step, 0.0, lv[0], lv[1], lv[2]];
                        }
                    }
                    kind => {
                        // the low-pass family: one stage rings, the rest stay just short of ringing; the two
                        // widest slopes put their other stages part of the way up to the top and ring there
                        let far: [f32; 3] = match (f.slope, kind) {
                            (4, 0) => [0.0, 0.53, 0.0],
                            (5, 0) => [0.0, 0.774, 0.592],
                            // (these lean a little on where the corner is)
                            (4, _) => [0.0, 0.521 - 0.1 * (c - 0.375), 0.0],
                            _ => [0.0, 0.769 - 0.106 * (c - 0.375), 0.582 - 0.18 * (c - 0.375)],
                        };
                        // (the wide kind does not: its two widest slopes measured the same as two and three plain steps)
                        let spread = f.slope >= 4 && kind != 3;
                        let ringer = if spread { n - 1 } else { 0 };
                        let lv = [f.levels[0] * s.val[b + 4], f.levels[1] * s.val[b + 5], f.levels[2] * s.val[b + 6]];
                        for (st, slot) in plan.iter_mut().enumerate().take(count) {
                            let cs = if spread && st > 0 { 1.0 - far[st % 3] * (1.0 - c) } else { c };
                            // with the second switch on, the resonance knob follows another law: the first
                            // kind keeps a set width in Hz, the others ring harder the higher the corner sits
                            let turned = (r * (0.70 + 0.59 * cs)).max(0.0);
                            let own = if st == ringer { r } else { 0.0 };
                            let kk = match kind {
                                0 if f.flat => flat_width(own) / (cutoff_hz(cs) * (1.07 - 0.07 * own)),
                                0 => if st == ringer { 1.0 - 0.79 * r } else { 1.0 },
                                3 if f.flat => (9.94 * (-4.33 * turned.powf(1.45)).exp()).powf(1.0 / n.max(1) as f32),
                                _ if f.flat => (-2.43 * turned.powf(2.44)).exp().powf(1.0 / n.max(1) as f32),
                                3 => table(&DAMP_WIDE, r).powf(1.0 / n.max(1) as f32),
                                _ if spread => table(&DAMP, r).powf(1.0 / n as f32),
                                _ => table(&DAMP, r).powf(1.0 / n.max(1) as f32),
                            };
                            let m = match kind {
                                0 => [lv[2], lv[1] - lv[2] * kk, lv[0] - lv[2]],
                                2 => [1.0, 0.0, -1.0],
                                _ => [0.0, 0.0, 1.0],
                            };
                            let g = match kind {
                                // (its corner sits a little higher the less it rings)
                                0 if f.flat => g_at(cutoff_hz(cs) * (1.07 - 0.07 * own)),
                                0 => g_at(cutoff_hz(cs)),
                                // the wide one's step is not bent for the sample rate, and stops at four tenths of it
                                3 => PI * wide_hz(cs).min(sr * 0.6) / sr,
                                _ => g_at(second_hz(cs)),
                            };
                            *slot = stage(g, kk, m);
                        }
                    }
                }
                s.stages[k] = count as u8;
            }
        }
        if let Some(s) = self.subs.first() {
            // the last two settings change nothing that can be heard: measured with envelopes and with maps
            let _ = s;
            self.main_l = p.volume * LEVEL;
            self.main_r = self.main_l;
        }
    }

    pub fn frame(&mut self, p: &Patch, dphase: f32) -> [f32; 2] {
        let any = p.send.iter().any(|&g| g != 0.0);
        let n = self.halvers.len();
        let out = self.halved(p, dphase / self.over as f32, n, any);
        self.sent = [out.1[0] * p.send_side[0], out.1[1] * p.send_side[1]];
        out.0
    }

    /// One frame at the rate left after `level` halvings, with its share for the effects.
    fn halved(&mut self, p: &Patch, d: f32, level: usize, any: bool) -> ([f32; 2], [f32; 2]) {
        if level == 0 {
            return (self.one(p, d), self.sent);
        }
        let a = self.halved(p, d, level - 1, any);
        let b = self.halved(p, d, level - 1, any);
        let sent = if any { self.send_halvers[level - 1].push(a.1, b.1, &HALF) } else { [0.0; 2] };
        (self.halvers[level - 1].push(a.0, b.0, &HALF), sent)
    }

    /// The share of the last frame that goes to the effects.
    pub fn sent(&self) -> [f32; 2] {
        self.sent
    }

    fn one(&mut self, p: &Patch, dphase: f32) -> [f32; 2] {
        if self.ctl == 0 {
            self.control(p, dphase, false);
            self.ctl = TICK * self.over;
        }
        self.ctl -= 1;
        self.t += 1;
        let mut mix = [0.0f32; 2];
        let mut smix = [0.0f32; 2];
        // noise is drawn once per frame of the rate asked for, so that it is as bright however far up we work
        let fresh = self.t % self.over == 0;
        // the soft start is a one-pole smoother that opens in a straight line (measured: 96 frames at 44.1 kHz,
        // and the same corner at any moment however far up we work)
        let fade = {
            let o = self.over as f32;
            // (our rate here is already the raised one)
            let k = self.sr / 44_100.0;
            ((self.t as f32 - 1.0) / (96.0 * k * o)).min(1.0)
        };
        // with the voices merged, the whole unison set goes through one run of the filters (measured: no width,
        // and a shaper in the filter is driven by the sum)
        let drain = 1.0 - (-1.0 / (CENTRE_S * self.sr)).exp();
        let ease = if self.over == 1 { 0.02 } else { 1.0 - 0.98f32.powf(1.0 / self.over as f32) };
        let merge = p.uni_mono && self.subs.len() > 1;
        let end = self.subs.len() - 1;
        let mut sum = [0.0f32; 6];
        for (nth, s) in self.subs.iter_mut().enumerate() {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            let (mut sl, mut sr) = (0.0f32, 0.0f32);
            let (mut fl, mut fr) = (0.0f32, 0.0f32);
            let (mut fsl, mut fsr) = (0.0f32, 0.0f32);
            let last = s.out;
            for i in (0..6).rev() {
                if !p.used[i] {
                    continue;
                }
                let o = &p.ops[i];
                let b = i * 6;
                let mut pm = 0.0;
                for j in 0..6 {
                    let a = p.fm[i][j];
                    if a != 0.0 {
                        pm += a * if j > i { s.out[j] } else { last[j] };
                    }
                }
                let ph = s.ph[i] as f32 + s.val[b + 4] * 0.5;
                let mut y = if o.pluck {
                    // a plucked string: the first cycle made (wave and noise) is kept and goes round and round,
                    // losing its top as it goes (see `control`); it cannot be phase modulated
                    let x = (ph - ph.floor()) * STR as f32;
                    let at = (x as usize).min(STR - 1);
                    let tab = &mut s.strings[i];
                    if s.run[i] < 1.0 {
                        let mut y = look(&o.tables[s.tab[i] as usize], ph);
                        if o.nx > 0.0 {
                            if fresh {
                                s.hiss[i] = hiss(&mut s.hseed);
                            }
                            y += (s.hiss[i] - y) * o.nx;
                        }
                        let from = s.fill[i] as usize;
                        if from < STR && (at + STR - from) % STR <= STR / 2 {
                            let mut k = from;
                            while k != at {
                                k = (k + 1) % STR;
                                tab[k] = y;
                            }
                        }
                        tab[at] = y;
                        s.fill[i] = at as u32;
                        s.run[i] += s.inc[i].abs();
                        if s.run[i] >= 1.0 {
                            // the kept cycle holds no standing level (measured)
                            let mean = tab.iter().sum::<f32>() / STR as f32;
                            tab.iter_mut().for_each(|v| *v -= mean);
                        }
                        y
                    } else {
                        let f = x - at as f32;
                        tab[at] + (tab[(at + 1) % STR] - tab[at]) * f
                    }
                } else {
                    let mut y = look(&o.tables[s.tab[i] as usize], ph + pm * s.val[b + 2] * DEPTH);
                    if o.nx > 0.0 {
                        if fresh {
                            s.hiss[i] = hiss(&mut s.hseed);
                        }
                        y += (s.hiss[i] - y) * o.nx;
                    }
                    y
                };
                for j in 0..6 {
                    let a = p.rm[i][j];
                    if a != 0.0 {
                        // ringing with itself goes by its own value one frame back, taken before its level
                        // and envelope (measured: a fading note keeps its self-ringing)
                        y *= 1.0 - a.abs() + a * if j > i { s.out[j] } else if j == i { s.raw[i] } else { last[j] };
                    }
                }
                s.amp[i] += (s.val[b + 1] * o.vol - s.amp[i]) * ease;
                s.prev[i] = last[i];
                s.raw[i] = y;
                s.out[i] = if o.declick && fade < 1.0 {
                    s.soft[i] += (y * s.amp[i] - s.soft[i]) * fade;
                    s.soft[i]
                } else {
                    y * s.amp[i]
                };
                if o.centre {
                    // (measured: the standing level goes after the operator's own level, at the pace of a
                    // gentle high-pass near four hertz, so a wave that leans one way starts off-centre and
                    // settles, and a fast fade leaves a short swing the other way)
                    s.dc[i] += (s.out[i] - s.dc[i]) * drain;
                    s.out[i] -= s.dc[i];
                }
                s.ph[i] += s.inc[i] as f64;
                s.ph[i] -= s.ph[i].floor();
                let g = p.out[i];
                if g != 0.0 {
                    l += s.out[i] * g * s.pl[i];
                    r += s.out[i] * g * s.pr[i];
                }
                let g = p.send[i];
                if g != 0.0 {
                    sl += s.out[i] * g * s.pl[i];
                    sr += s.out[i] * g * s.pr[i];
                }
            }
            if merge {
                for j in 0..6 {
                    sum[j] += s.out[j] * s.gain;
                }
            }
            // (measured: a filter mixed with the one it feeds meets it in step at every pitch, so the three
            // filters sit together between one doubling and one halving, not each between its own)
            let mut carry = [0.0f32; 2];
            let mut bus = [[0.0f32; 4]; 2];
            let mut ran = false;
            for k in 0..3 {
                if !p.fused[k] || (merge && nth != end) {
                    carry = [0.0; 2];
                    continue;
                }
                ran = true;
                let f = &p.filts[k];
                let b = 36 + k * 8;
                let mut x = 0.0;
                for j in 0..6 {
                    let a = p.feed[k][j];
                    if a != 0.0 {
                        x += a * if merge { sum[j] } else { s.out[j] };
                    }
                }
                let bend = |x: f32| -> f32 {
                    let xin = x * f.ws_gain;
                    let shaped = if f.ws_bipolar {
                        let q = (xin.clamp(-1.0, 1.0) + 1.0) * 128.0;
                        let i = (q as usize).min(255);
                        (f.ws[i] + (f.ws[i + 1] - f.ws[i]) * (q - i as f32) - 0.5) * 2.0
                    } else {
                        let q = xin.abs().min(1.0) * 256.0;
                        let i = (q as usize).min(255);
                        (f.ws[i] + (f.ws[i + 1] - f.ws[i]) * (q - i as f32)) * xin.signum()
                    };
                    x + (shaped - x) * (f.ws_mix * s.val[b + 7]).clamp(0.0, 1.0)
                };
                // (measured with hiss: the curves are those of filters stepped at twice the rate, and nothing is
                // lost at the very top, so at the plain rate every filter works between a doubling and a halving)
                let fed = if self.fsteps == 2 { s.twice[k].up(x) } else { [x; 2] };
                let mut next = [0.0f32; 2];
                for step in 0..self.fsteps as usize {
                x = fed[step] + carry[step];
                // (measured with a falling level in front of a driven filter and of a shaper: the level is
                // set on the way in, so both keep the sound up while it falls)
                x *= s.val[b + 1];
                if f.kind == 12 {
                    // phaser: a row of all-pass stages whose output comes back to its own input, solved
                    // without a sample of delay (the row is a straight line: gain times input plus memory)
                    let g = s.fg[k];
                    let a = (g - 1.0) / (g + 1.0);
                    // (the second switch adds five to the order)
                    let n = (f.slope.clamp(1, 5) as usize + if f.flat { 5 } else { 0 }) * 2;
                    let (mut gain, mut held) = (1.0f32, 0.0f32);
                    for z in s.ap[k].iter().take(n) {
                        held = a * held + *z;
                        gain *= a;
                    }
                    let back = s.fk[k];
                    let u = (x + back * held) / (1.0 - back * gain);
                    let mut y = u;
                    for z in s.ap[k].iter_mut().take(n) {
                        let o = a * y + *z;
                        *z = y - a * o;
                        y = o;
                    }
                    // (what leaves is the end of the row: as loud as its start at every pitch, but turned, which
                    // shows once it is mixed with another path. Measured against the filter before it.)
                    x = y;
                } else if f.kind == 11 && s.plan[k][0][4] > 0.0 {
                    // the driven ladders: every stage bends what comes in and what it holds, and each ladder
                    // has its own push, pull and resonance, which goes back in under its first bend
                    let bend = |v: f32| v / (1.0 + v * v).sqrt();
                    let lean = |v: f32| {
                        let q = 1.0 / (1.0 + v * v);
                        q * q.sqrt()
                    };
                    let (push, pull) = (s.plan[k][0][4], s.plan[k][0][5]);
                    // (with resonance the sound stays cleaner than the make-up alone would leave it: part of
                    // the make-up is taken off going in and given back coming out)
                    let pre = s.plan[k][0][2].sqrt();
                    for st in 0..s.stages[k] as usize {
                        let [big, back, make_up, ..] = s.plan[k][st];
                        let u_in = x * push / pre;
                        let g = big / (1.0 - big).max(1e-6);
                        let (lo, hi) = s.fs[k].split_at_mut(st * 2 + 1);
                        let z = [&mut lo[st * 2], &mut hi[0]];
                        let held = [z[0][0], z[0][1], z[1][0], z[1][1]];
                        // the four stages for one guess of what comes back; gives the last stage and how
                        // steeply it follows the guess
                        let chain = |guess: f32, keep: &mut [f32; 4]| -> (f32, f32) {
                            let arg = u_in - back * guess;
                            let mut a = bend(arg);
                            let mut slope = -back * lean(arg);
                            let mut u = 0.0;
                            for q in 0..4 {
                                let m = held[q];
                                let want = m + g * a;
                                u = m;
                                for _ in 0..2 {
                                    u -= (u + g * bend(u) - want) / (1.0 + g * lean(u));
                                }
                                keep[q] = 2.0 * u - m;
                                slope *= g / (1.0 + g * lean(u));
                                if q < 3 {
                                    slope *= lean(u);
                                    a = bend(u);
                                }
                            }
                            (u, slope)
                        };
                        let mut keep = [0.0f32; 4];
                        let mut guess = held[3];
                        if back > 0.0 {
                            for _ in 0..2 {
                                let (got, slope) = chain(guess, &mut keep);
                                guess -= (got - guess) / (slope - 1.0);
                            }
                        }
                        let (got, _) = chain(guess, &mut keep);
                        for q in 0..4 {
                            z[q / 2][q % 2] = keep[q];
                        }
                        x = got * pull * make_up * pre;
                    }
                } else if f.kind == 11 {
                    for st in 0..s.stages[k] as usize {
                        let [big, back, make_up, solve, ..] = s.plan[k][st];
                        let keep = 1.0 - big;
                        let (lo, hi) = s.fs[k].split_at_mut(st * 2 + 1);
                        let z = [&mut lo[st * 2], &mut hi[0]];
                        // what the four stages would give with nothing coming in
                        let mut held = 0.0f32;
                        for q in 0..4 {
                            held = big * held + keep * z[q / 2][q % 2];
                        }
                        let mut y = (x * make_up - back * held) * solve;
                        for q in 0..4 {
                            let m = &mut z[q / 2][q % 2];
                            let v = (y - *m) * big;
                            y = v + *m;
                            *m = y + v;
                        }
                        x = y;
                    }
                } else if f.kind == 0 && p.lay.first && f.flat {
                    for st in 0..s.stages[k] as usize {
                        let [step, damp, _, m0, m1, m2] = s.plan[k][st];
                        let z = &mut s.fs[k][st];
                        let low = z[0] + step * z[1];
                        let high = x - low - damp * z[1];
                        let band = z[1] + step * high;
                        *z = [low, band];
                        x = m0 * low + m1 * band + m2 * high;
                    }
                } else {
                    for st in 0..s.stages[k] as usize {
                        let [a1, a2, a3, m0, m1, m2] = s.plan[k][st];
                        let z = &mut s.fs[k][st];
                        let v3 = x - z[1];
                        let v1 = a1 * z[0] + a2 * v3;
                        let v2 = z[1] + a2 * z[0] + a3 * v3;
                        *z = [2.0 * v1 - z[0], 2.0 * v2 - z[1]];
                        x = m0 * x + m1 * v1 + m2 * v2;
                    }
                }
                // (measured with a sine and a closed filter: the shaper bends what comes out of the filter)
                if f.ws_on {
                    x = bend(x);
                }
                if !x.is_finite() {
                    x = 0.0;
                    s.fs[k] = [[0.0; 2]; 6];
                    s.ap[k] = [0.0; 20];
                    s.twice[k] = Twice::new();
                    s.fb[k] = 0.0;
                }
                // a ringing stage after a hot shaper is kept inside sane bounds
                x = x.clamp(-8.0, 8.0);
                next[step] = x * f.next;
                let g = p.out[6 + k];
                if g != 0.0 {
                    bus[step][0] += x * g * s.pl[6 + k];
                    bus[step][1] += x * g * s.pr[6 + k];
                }
                let g = p.send[6 + k];
                if g != 0.0 {
                    bus[step][2] += x * g * s.pl[6 + k];
                    bus[step][3] += x * g * s.pr[6 + k];
                }
                }
                carry = next;
            }
            if ran {
                let mut down = [0.0f32; 4];
                for (c, d) in down.iter_mut().enumerate() {
                    *d = if self.fsteps == 2 { s.mixd[c].down([bus[0][c], bus[1][c]]) } else { bus[0][c] };
                }
                fl += down[0];
                fr += down[1];
                fsl += down[2];
                fsr += down[3];
            }
            let th = (s.pan + 1.0) * std::f32::consts::FRAC_PI_4;
            let (ul, ur) = (th.cos() * std::f32::consts::SQRT_2 * s.gain, th.sin() * std::f32::consts::SQRT_2 * s.gain);
            // (the merged filters were fed with the voices' levels already in)
            let (vl, vr) = if merge { (1.0, 1.0) } else { (ul, ur) };
            mix[0] += l * ul + fl * vl;
            mix[1] += r * ur + fr * vr;
            // what goes to the effects has its own pan and level settings (measured: the level one scales the
            // effects path alone)
            let (el, er) = pan_gains(s.val[60].clamp(-1.0, 1.0));
            smix[0] += (sl * ul + fsl * vl) * s.val[61] * el;
            smix[1] += (sr * ur + fsr * vr) * s.val[61] * er;
        }
        self.sent = [smix[0] * self.main_l, smix[1] * self.main_r];
        [mix[0] * self.main_l, mix[1] * self.main_r]
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An envelope sender block of the newer layout: every output rises from nothing to full over a second and
    /// holds there; the outputs are multiplied ones resting at nothing.
    pub(crate) fn sender_block() -> Vec<u8> {
        let mut b = 6i32.to_le_bytes().to_vec();
        for _ in 0..8 {
            for v in [0i32, 256, 128, 128, 128, 0, 128, 40000, 0, 0, 0] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        for v in [32768i32, 32768, 0, 0, 2, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..8 {
            b.extend_from_slice(&0i32.to_le_bytes());
            b.extend_from_slice(&3i32.to_le_bytes());
            b.extend_from_slice(&[0; 12]);
            curve(1, &[(0.0, 0.0), (1.0, 1.0)], [0, -1, -1, -1, 1], &mut b);
            for v in [128i32, 128, 0, 128] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            curve(2, &[(0.0, 0.5), (1.0, 0.5)], [0, 0, -1, -1, -1], &mut b);
            b.extend_from_slice(&40000i32.to_le_bytes());
            b.extend_from_slice(&[0; 16]);
            for _ in 0..5 {
                curve(3, &[(0.0, 1.0), (1.0, 1.0)], [0, -1, -1, -1, -1], &mut b);
            }
            b.extend_from_slice(&48i32.to_le_bytes());
            b.extend_from_slice(&256i32.to_le_bytes());
        }
        b
    }

    #[test]
    fn an_envelope_sender_block_reads_and_runs() {
        let s = Sender::parse(&sender_block()).unwrap();
        assert_eq!(s.arts.len(), 8);
        assert!(!s.arts[0].art.bipolar);
        // nothing at the start, half way after half a second, full and held after that
        assert!(s.value(0, 60, 1.0, 0.0, 0.0, None).unwrap() < 0.01);
        assert!((s.value(0, 60, 1.0, 0.0, 0.5, None).unwrap() - 0.5).abs() < 0.02);
        assert!(s.value(3, 60, 1.0, 0.0, 5.0, None).unwrap() > 0.99);
        // keys under the output's range do not set it off
        assert_eq!(s.value(0, 40, 1.0, 0.0, 0.5, None), None);
        // a new sender's own block reads, and falls back to nothing once the key is let go
        let f = Sender::parse(&Sender::fresh()).unwrap();
        assert!(f.value(7, 0, 1.0, 0.0, 0.05, None).unwrap() > 0.95);
        assert!((f.value(7, 0, 1.0, 0.0, 3.0, None).unwrap() - 0.6).abs() < 0.01);
        assert!(f.value(7, 0, 1.0, 0.0, 4.0, Some(3.0)).unwrap() < 0.01);
        // a cut-off block is refused rather than half read
        assert!(Sender::parse(&sender_block()[..900]).is_none());
    }

    /// With CEREAL_FLP_CORPUS set: every envelope sender preset and project block there reads.
    #[test]
    fn real_envelope_sender_blocks_read() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut seen = 0;
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                if ext != "fst" && ext != "flp" {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                if !bytes.windows(ENV_SENDER.len()).any(|w| w == ENV_SENDER.as_bytes()) {
                    continue;
                }
                let Ok(f) = crate::flp::parse(&bytes) else { continue };
                for c in f.channels.iter().filter(|c| c.plugin.eq_ignore_ascii_case(ENV_SENDER) && !c.state.is_empty()) {
                    assert!(Sender::parse(&c.state).is_some(), "{}", p.display());
                    seen += 1;
                }
            }
        }
        assert!(seen > 0);
    }

    fn curve(kind: i32, pts: &[(f64, f64)], tail: [i32; 5], out: &mut Vec<u8>) {
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&3i32.to_le_bytes());
        out.extend_from_slice(&(pts.len() as i32).to_le_bytes());
        for &(x, y) in pts {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&[0; 8]);
        }
        for v in tail {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }

    fn art(bipolar: bool, env_on: bool, out: &mut Vec<u8>) {
        let y = if bipolar { 0.5 } else { 1.0 };
        out.extend_from_slice(&[0; 16]);
        curve(1, &[(0.0, 0.0), (0.01, 1.0), (0.05, 0.5), (0.1, 0.0)], [0, -(env_on as i32), 1, -1, 2], out);
        for v in [128i32, 128, 0, 128] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        curve(2, &[(0.0, 0.5), (1.0, 0.5)], [0, 0, -1, -1, -1], out);
        for v in [40000i32, 0, 0, 0, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..6 {
            curve(3, &[(0.0, y), (1.0, y)], [0, -1, -1, -1, -1], out);
        }
    }

    /// A block in the layout of the reader, built from nothing: operator 2 drives operator 1, which goes out both
    /// directly and through filter 1. Operator 1's level follows an envelope.
    pub(crate) fn plain_block() -> Vec<u8> {
        let mut d = vec![0u8; 1137];
        let put = |d: &mut Vec<u8>, o: usize, v: i32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut d, 0, 128);
        put(&mut d, 4, 128);
        put(&mut d, 12 + 4, 64); // operator 1 from operator 2
        put(&mut d, 12 + 8 * 4, 128); // operator 1 out
        put(&mut d, 12 + 6 * 36, 256); // filter 1 from operator 1
        put(&mut d, 12 + 6 * 36 + 8 * 4, 128); // filter 1 out
        for o in [660, 664, 672, 676, 680, 688] {
            put(&mut d, o, 128);
        }
        put(&mut d, 692, 1);
        d.push(2);
        d.extend_from_slice(&[b'h', 0, b'i', 0]);
        for k in 0..6 {
            let mut h = vec![0u8; 2128];
            h[4..8].copy_from_slice(&256i32.to_le_bytes());
            h[8..12].copy_from_slice(&1200i32.to_le_bytes());
            h[40..44].copy_from_slice(&(if k == 1 { 40000i32 } else { 20000 }).to_le_bytes());
            h[52..56].copy_from_slice(&1.0f32.to_le_bytes());
            h[2116..2120].copy_from_slice(&128i32.to_le_bytes());
            d.extend_from_slice(&h);
            for a in 0..6 {
                art(bipolar_at(a), k == 0 && a == 1, &mut d);
            }
        }
        for _ in 0..3 {
            let mut h = vec![0u8; 88];
            for (o, v) in [(0, 1i32), (4, 128), (8, 128), (16, 128), (28, 128), (32, 128)] {
                h[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }
            d.extend_from_slice(&h);
            curve(3, &[(0.0, 0.0), (1.0, 1.0)], [0, -1, -1, -1, -1], &mut d);
            d.push(0);
            for a in 0..8 {
                art(bipolar_at(36 + a), false, &mut d);
            }
        }
        d.extend_from_slice(&[0; 40]);
        art(true, false, &mut d);
        art(false, false, &mut d);
        d.push(0);
        d
    }

    pub(crate) fn packed_block() -> Vec<u8> {
        let mut s = 0x0017_0003u32.to_le_bytes().to_vec();
        s.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&plain_block(), 6));
        s
    }

    #[test]
    fn block_is_read() {
        let p = Patch::parse(&packed_block()).expect("reads");
        assert!((p.fm[0][1] - 0.163).abs() < 0.001, "a quarter turn of a modulation knob");
        assert_eq!(p.feed[0][0], 1.0);
        assert_eq!(p.ops[1].ratio, 4.0);
        assert_eq!(p.ops[0].ratio, 2.0);
        assert_eq!(p.used(), [true, true, false, false, false, false]);
        assert!(p.arts[1].env.on() && !p.arts[7].env.on());
        assert_eq!(p.arts.len(), ARTS);
        // an older layout is not guessed at
        let mut old = packed_block();
        old[2] = 9;
        assert!(Patch::parse(&old).is_none());
        // a cut-off block is refused, not half read
        let plain = plain_block();
        assert!(Patch::from_plain(&plain[..plain.len() - 200], 23).is_none());
    }

    fn run(p: &Patch, key: u8, hold: usize, total: usize) -> Vec<[f32; 2]> {
        run_with_send(p, key, hold, total, false)
    }

    /// `wet` also listens to what the voice passes to the effects, for instruments that send everything there.
    fn run_with_send(p: &Patch, key: u8, hold: usize, total: usize, wet: bool) -> Vec<[f32; 2]> {
        let sr = 48000.0;
        let hz = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0);
        let mut v = Voice::start(p, sr, key, [0.8, 0.5, 0.5], hz / sr, 99);
        (0..total)
            .map(|k| {
                if k == hold {
                    v.release();
                }
                let f = v.frame(p, hz / sr);
                let w = if wet { v.sent() } else { [0.0; 2] };
                // whichever is louder: a send may be the mirror image of the dry sound
                if w[0].abs().max(w[1].abs()) > f[0].abs().max(f[1].abs()) { w } else { f }
            })
            .collect()
    }

    #[test]
    fn envelope_shapes_the_note_and_modulation_adds_overtones() {
        let p = Patch::parse(&packed_block()).unwrap();
        let o = run(&p, 57, 24000, 36000);
        assert!(o.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
        let peak = |r: std::ops::Range<usize>| o[r].iter().fold(0.0f32, |a, f| a.max(f[0].abs()));
        let (top, held, gone) = (peak(300..900), peak(12000..20000), peak(30000..36000));
        assert!(top > held * 1.5, "attack peak {top} above the held level {held}");
        assert!(held > 0.05);
        assert!(gone < 1e-3, "silent after the release, got {gone}");
        // the release is drawn a tenth of a unit long, and a unit is half a second
        assert!((p.tail_s(0.5, 2.0) - 0.05).abs() < 1e-4);

        // with the modulator taken away the held part is a plain tone an octave over the key: next to nothing above
        let mut plain = p.clone();
        plain.fm[0][1] = 0.0;
        plain.feed[0][0] = 0.0;
        plain.wire();
        let q = run(&plain, 57, 24000, 24000);
        let part = |o: &[[f32; 2]], hz: f32| {
            let (mut a, mut b) = (0.0f32, 0.0f32);
            for (k, f) in o[9600..19200].iter().enumerate() {
                let w = TAU * hz * k as f32 / 48000.0;
                a += f[0] * w.cos();
                b += f[0] * w.sin();
            }
            (a * a + b * b).sqrt() / 4800.0
        };
        assert!(part(&q, 440.0) > 0.05 && part(&q, 880.0) < part(&q, 440.0) * 0.01);
        let mut direct = p.clone();
        direct.feed[0][0] = 0.0;
        direct.wire();
        let m = run(&direct, 57, 24000, 24000);
        assert!(part(&m, 1320.0) > part(&m, 440.0) * 0.05, "the modulator adds a partial at three times the note");
    }

    #[test]
    fn unison_spreads_and_filters_stay_stable() {
        let mut p = Patch::parse(&packed_block()).unwrap();
        p.uni = 5;
        p.uni_pan = 1.0;
        p.uni_pitch = 0.5;
        p.filts[0].res = 1.0;
        p.filts[0].slope = 3;
        let o = run(&p, 96, 20000, 30000);
        assert!(o.iter().all(|f| f[0].is_finite() && f[0].abs() < 20.0));
        assert!(o[2000..9000].iter().any(|f| (f[0] - f[1]).abs() > 1e-4), "unison voices sit apart");
    }

    /// One operator straight to the output, nothing else in the way.
    fn lone() -> Patch {
        let mut p = Patch::parse(&packed_block()).unwrap();
        p.fm = [[0.0; 6]; 6];
        p.feed = [[0.0; 6]; 3];
        p.out = [0.0; 9];
        p.out[0] = 1.0;
        p.wire();
        p
    }

    /// How strong the partial at `hz` is over the given frames (left side).
    fn level(o: &[[f32; 2]], hz: f32) -> f32 {
        let w = 2.0 * PI * hz / 48000.0;
        let (mut c, mut s) = (0.0f32, 0.0f32);
        for (k, f) in o.iter().enumerate() {
            c += f[0] * (w * k as f32).cos();
            s += f[0] * (w * k as f32).sin();
        }
        (c * c + s * s).sqrt() * 2.0 / o.len() as f32
    }

    /// One operator through the first filter as a driven low-pass, and nothing else.
    fn driven(slope: i32, drive: f32, res: f32, feed: f32) -> Patch {
        let mut p = lone();
        p.out[0] = 0.0;
        p.out[6] = 1.0;
        p.feed[0][0] = feed;
        let f = &mut p.filts[0];
        (f.kind, f.slope, f.drive, f.res, f.cut, f.env_amt, f.ws_on) = (11, slope, drive, res, 0.5, 0.5, false);
        for j in 0..8 {
            p.arts[36 + j] = Art::neutral(matches!(j, 0 | 2 | 3));
        }
        p.wire();
        p
    }

    /// Every ladder of the driven low-pass has its own push and pull: quiet sound gains the same again with
    /// each slope step, and the resonance sits round every ladder, weaker the more of them there are.
    #[test]
    fn each_driven_ladder_pushes_and_rings_on_its_own() {
        // (the test block's operator sounds an octave over the key)
        let at = |p: &Patch| level(&run(p, 45, 30000, 30000)[8000..28000], 220.0);
        let db = |a: f32, b: f32| 20.0 * (a / b).log10();
        let quiet = [1, 2, 3].map(|n| at(&driven(n, 1.0, 0.0, 0.0002)));
        let (one, two) = (db(quiet[1], quiet[0]), db(quiet[2], quiet[1]));
        assert!((one - 17.7).abs() < 1.5 && (two - one).abs() < 1.5, "{one} {two}");
        // flat out the last ladder sets the level, so another one adds next to nothing
        let loud = [1, 2].map(|n| at(&driven(n, 1.0, 0.0, 1.0)));
        assert!(db(loud[1], loud[0]).abs() < 3.0, "{}", db(loud[1], loud[0]));
        // without drive: well under the corner one ladder at half resonance leaves 1.97 / 2.89 of the sound,
        // each of two leaves 1.72 / 2.40
        let plain = |n, r| at(&driven(n, 0.0, r, 0.05));
        let (a, b) = (db(plain(1, 0.5), plain(1, 0.0)), db(plain(2, 0.5), plain(2, 0.0)));
        assert!((a + 3.3).abs() < 0.6 && (b + 5.8).abs() < 0.8, "{a} {b}");
        // with drive the loop is a third stronger, so quiet sound loses more
        let d = db(at(&driven(1, 1.0, 0.5, 0.004)), at(&driven(1, 1.0, 0.0, 0.004)));
        assert!(d < a - 0.3, "{d} {a}");
    }

    /// The sub level knob hands its share of every voice to a copy an octave down, with one voice too.
    #[test]
    fn sub_level_adds_the_octave_below() {
        let mut p = lone();
        let plain = run(&p, 57, 24000, 24000);
        let whole = level(&plain[4800..24000], 440.0);
        assert!(whole > 0.05 && level(&plain[4800..24000], 220.0) < whole * 0.01);
        p.uni_sub = 0.25;
        let o = run(&p, 57, 24000, 24000);
        let (top, low) = (level(&o[4800..24000], 440.0), level(&o[4800..24000], 220.0));
        assert!((top / whole - 0.75).abs() < 0.03 && (low / whole - 0.25).abs() < 0.03, "{top} {low} {whole}");
    }

    #[test]
    fn slow_wave_speed_and_shape_follow_the_measured_laws() {
        // 32 Hz at the top, ten times slower for each quarter of the knob
        assert!((lfo_hz(65536) - 32.0).abs() < 0.01);
        assert!((lfo_hz(32768) - 0.3168).abs() < 0.001);
        assert!((2.0 / lfo_hz(21502) - 32.045).abs() < 0.1);
        assert_eq!(lfo_hz(0), 0.0);
        assert!((lfo_hz(52544) - 5.15).abs() < 0.03);
        // a plain triangle that starts at rest going up
        let plain = [0.0; 3];
        for (p, y) in [(0.0, 0.0), (0.125, 0.5), (0.25, 1.0), (0.5, 0.0), (0.75, -1.0), (0.875, -0.5), (1.25, 1.0)] {
            assert!((lfo_wave(p, plain) - y).abs() < 1e-4, "{p}");
        }
        // half skew: the peak at three eighths, the trough at five; full skew: a rising saw
        assert!((lfo_wave(0.375, [0.0, 0.5, 0.0]) - 1.0).abs() < 1e-4);
        assert!((lfo_wave(0.625, [0.0, 0.5, 0.0]) + 1.0).abs() < 1e-4);
        assert!((lfo_wave(0.125, [0.0, -0.5, 0.0]) - 1.0).abs() < 1e-4);
        assert!((lfo_wave(0.25, [0.0, 1.0, 0.0]) - 0.5).abs() < 1e-3);
        assert!((lfo_wave(0.75, [0.0, 1.0, 0.0]) + 0.5).abs() < 1e-3);
        // half pulse width: up in an eighth, held for half the cycle, down in a quarter, back in an eighth
        let held = [0.0, 0.0, 0.5];
        for (p, y) in [(0.0625, 0.5), (0.125, 1.0), (0.4, 1.0), (0.62, 1.0), (0.75, 0.0), (0.875, -1.0), (0.9375, -0.5)] {
            assert!((lfo_wave(p, held) - y).abs() < 1e-3, "{p}");
        }
        // negative: the trough is held instead
        let low = [0.0, 0.0, -0.5];
        for (p, y) in [(0.125, 1.0), (0.25, 0.0), (0.375, -1.0), (0.6, -1.0), (0.87, -1.0), (0.9375, -0.5)] {
            assert!((lfo_wave(p, low) - y).abs() < 1e-3, "{p}");
        }
        // skew then width: the held peak starts where the squeezed skewed peak lands
        let both = [0.0, 0.5, 0.5];
        for (p, y) in [(0.1875, 1.0), (0.68, 1.0), (0.8125, -1.0), (0.90625, -0.5)] {
            assert!((lfo_wave(p, both) - y).abs() < 1e-3, "{p}");
        }
        // tension fattens the wave when positive and thins it when negative, mirrored
        let fat = lfo_wave(0.0625, [0.5, 0.0, 0.0]);
        let thin = lfo_wave(0.0625, [-0.5, 0.0, 0.0]);
        assert!((fat - 0.84).abs() < 0.03, "{fat}");
        assert!((thin - (1.0 - lfo_wave(0.1875, [0.5, 0.0, 0.0]))).abs() < 1e-3);
        assert!((lfo_wave(0.25, [0.5, 0.0, 0.0]) - 1.0).abs() < 1e-4);
        assert_eq!(lfo_shape(&[64, -128, 300]), [0.5, -1.0, 1.0]);
        assert_eq!(lfo_shape(&[]), [0.0; 3]);
    }

    #[test]
    fn every_kind_of_bend_follows_its_measured_shape() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        for mode in [0u8, 1, 3, 4, 7, 8, 9, 10, 11, 12] {
            for t in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
                if mode == 3 && t < 0.0 {
                    // (stairs the other way round start one tread up and jump at the very end)
                    continue;
                }
                assert!(bent(0.0, t, mode) < 0.06, "kind {mode} tension {t} starts at the bottom");
                assert!(bent(1.0, t, mode) > 0.99, "kind {mode} tension {t} ends at the top");
            }
        }
        // the doubled kinds pass through the middle and mirror around it
        for mode in [1u8, 8, 12] {
            assert!(near(bent(0.5, 0.5, mode), 0.5));
            assert!(near(bent(0.2, 0.5, mode) + bent(0.8, 0.5, mode), 1.0));
        }
        assert!(near(bent(0.25, 0.5, 1), 0.5 * swell(0.5, 0.5)));
        // the second single curve leans toward one minus the sixth power of what is left
        assert!(near(bent(0.3, 1.0, 7), 1.0 - 0.7f32.powi(6)));
        assert!(near(bent(0.3, 0.5, 7), 0.3 + 0.5 * (1.0 - 0.7f32.powi(6) - 0.3)));
        assert!(near(bent(0.3, -0.5, 7), 1.0 - bent(0.7, 0.5, 7)));
        assert!(near(bent(0.4, 0.0, 7), 0.4) && near(bent(0.4, 0.0, 11), 0.4) && near(bent(0.4, 0.0, 9), 0.4));
        assert!(near(bent(0.5, 1.0, 9), std::f32::consts::FRAC_1_SQRT_2));
        // the smooth kind is a smooth step that tension straightens either way
        assert!(near(bent(0.25, 0.0, 10), 0.15625) && near(bent(0.25, 1.0, 10), 0.25) && near(bent(0.25, -1.0, 10), 0.25));
        // the third single curve: a ratio whose weight was read at three tensions
        assert!(near(bent(0.5, 0.5, 11), 1.0 / (1.0 + 0.25 / 1.5)));
        // stairs: two treads at full tension, four at three quarters, eighteen at half
        assert_eq!((steps(1.0), steps(0.75), steps(0.5), steps(0.625), steps(0.875)), (1.0, 3.0, 17.0, 7.0, 2.0));
        assert!(bent(0.49, 1.0, 3) == 0.0 && bent(0.51, 1.0, 3) == 1.0);
        assert!(near(bent(0.3, 0.75, 3), 1.0 / 3.0) && near(bent(0.6, 0.75, 3), 2.0 / 3.0) && bent(0.8, 0.75, 3) == 1.0);
        assert!(near(bent(0.01, -0.5, 3), 1.0 / 18.0) && near(bent(0.99, -0.5, 3), 17.0 / 18.0));
        assert!(near(bent(0.5, 1.0, 4), 0.5) && bent(0.2, 1.0, 4) < 0.05);
        // the pulse starts low and ends high, seventeen times over at half tension; the other way up when negative
        assert!(bent(0.25, 1.0, 5) == 0.0 && bent(0.75, 1.0, 5) == 1.0 && bent(0.25, -1.0, 5) == 1.0);
        assert!(bent(0.5 / 34.0, 0.5, 5) == 0.0 && bent(1.5 / 34.0, 0.5, 5) == 1.0 && bent(0.999, 0.5, 5) == 1.0);
        // the wave is a triangle that ends on a peak (a plain ramp at full tension), a sine when negative
        assert!(near(bent(0.3, 1.0, 6), 0.3) && near(bent(1.0 / 33.0, 0.5, 6), 1.0) && near(bent(2.0 / 33.0, 0.5, 6), 0.0));
        assert!(near(bent(0.5, -1.0, 6), 0.5) && near(bent(1.0 / 3.0, -1.0, 6), 0.25));
        // a held piece stays where it started
        assert_eq!(bent(0.7, 0.3, 2), 0.0);
        let mut c = Curve::flat(0.0);
        c.pts = vec![Pt { x: 0.0, y: 0.2, tension: 0.0, mode: 0 }, Pt { x: 1.0, y: 1.0, tension: 1.0, mode: 3 }];
        c.prepare([1.0; 3]);
        assert!(near(c.map(0.4), 0.2) && near(c.map(0.6), 1.0));
    }

    /// Two voices half a cycle apart with no detune cancel; the level knob does nothing to a pair; a set of
    /// voices follows 1 / (0.621 n + 0.34) each.
    #[test]
    fn unison_phase_and_level_follow_the_measured_law() {
        let mut p = lone();
        let one = level(&run(&p, 57, 24000, 24000)[4800..], 440.0);
        p.uni = 2;
        p.uni_pitch = 0.0;
        p.uni_pan = 0.0;
        p.uni_phase = 1.0;
        // (the later blocks turn each voice by its own detuning, so with none the knob does nothing)
        let o = run(&p, 57, 24000, 24000);
        assert!((level(&o[4800..], 440.0) / one - 2.0 / (0.621 * 2.0 + 0.34)).abs() < 0.03, "no detuning, no turn");
        // (the first two layouts and the states of version 24 on spread the starts evenly instead)
        p.uni_old_share = true;
        let o = run(&p, 57, 24000, 24000);
        assert!(level(&o[4800..], 440.0) < one * 0.01, "half a cycle apart cancels");
        p.uni_old_share = false;
        p.uni_phase = 0.0;
        for vol in [0.0, 1.0] {
            p.uni_vol = vol;
            let two = level(&run(&p, 57, 24000, 24000)[4800..], 440.0);
            assert!((two / one - 2.0 / (0.621 * 2.0 + 0.34)).abs() < 0.03, "{two} {one}");
        }
        p.uni = 5;
        p.uni_vol = 1.0;
        let five = level(&run(&p, 57, 24000, 24000)[4800..], 440.0);
        assert!((five / one - 5.0 / (0.621 * 5.0 + 0.34)).abs() < 0.05, "{five} {one}");
        // with the level knob down the outer voices fade out and the middle one doubles
        p.uni_vol = 0.0;
        let faded = level(&run(&p, 57, 24000, 24000)[4800..], 440.0);
        assert!((faded / five - 0.8).abs() < 0.03, "{faded} {five}");
    }

    /// The voices of a set run left to right, sharp and flat by turns, each a fixed share of the spread off pitch;
    /// a note placed later in the song still starts with them together.
    #[test]
    fn unison_spread_and_drift_follow_the_measured_law() {
        let mut p = lone();
        p.uni = 3;
        p.uni_pitch = 0.5;
        p.uni_pan = 1.0;
        p.uni_phase = 0.0;
        let v = Voice::start(&p, 48000.0, 57, [0.8, 0.5, 0.5], 440.0 / 48000.0, 1);
        let cents: Vec<f32> = v.subs.iter().map(|s| s.detune.log2() * 1200.0).collect();
        // 11.44 cents of spread at the middle of the knob, the voices either side of the middle one going opposite ways
        assert!((uni_spread(0.5, false) - 11.44).abs() < 0.01 && (uni_spread(1.0, false) - 84.68).abs() < 0.02 && (uni_spread(0.5, true) - 13.1).abs() < 0.05);
        assert!((cents[0].abs() - 11.44 * UNI_SHARE[0].abs()).abs() < 0.02 && cents[1] == 0.0 && (cents[2].abs() - 11.44 * UNI_SHARE[2].abs()).abs() < 0.02, "{cents:?}");
        assert!(cents[0] * cents[2] < 0.0 || UNI_SHARE[0] * UNI_SHARE[2] < 0.0, "{cents:?}");
        assert!(v.subs[0].pan == -1.0 && v.subs[1].pan == 0.0 && v.subs[2].pan == 1.0);
        p.uni = 2;
        // the later blocks: the phase knob turns each voice by its detuning times 1e5 p^8 cycles
        p.uni_phase = 0.5;
        let v = Voice::start(&p, 48000.0, 57, [0.8, 0.5, 0.5], 440.0 / 48000.0, 1);
        let want = ((v.subs[0].detune as f64 - 1.0) * 1.0e5 / 256.0).rem_euclid(1.0);
        assert!((v.subs[0].ph[0] - p.ops[0].phase as f64 - want).abs() < 1e-6 && want > 0.01, "{want}");
        p.uni_phase = 0.0;
        p.lay.first = true;
        let mut v = Voice::start(&p, 48000.0, 57, [0.8, 0.5, 0.5], 440.0 / 48000.0, 1);
        assert!(v.subs[0].detune > 1.0 && v.subs[1].detune < 1.0);
        let before = [v.subs[0].ph[0], v.subs[1].ph[0]];
        v.started_at(&p, 440.0, 2.0);
        // (an operator that is not free starts every note with its voices together, on the first layout too)
        assert!(v.subs[0].ph[0] == before[0] && v.subs[1].ph[0] == before[1]);
    }

    /// A free-running operator stands where the song's clock has carried it: its pitch times the song's time
    /// (128 frames ahead), each unison voice by its own detuned pitch, whatever chance start it was dealt.
    #[test]
    fn free_running_operators_turn_with_the_song() {
        let mut p = lone();
        p.ops[0].free = true;
        p.uni = 2;
        p.uni_pitch = 0.5;
        p.uni_phase = 0.0;
        for seed in [1, 77] {
            let mut v = Voice::start(&p, 44100.0, 57, [0.8, 0.5, 0.5], 440.0 / 44100.0, seed);
            v.started_at(&p, 440.0, 3.25);
            for s in &v.subs {
                let f = (440.0 * (p.pitch / 12.0).exp2() * s.detune * p.ops[0].ratio) as f64;
                let want = (f * (3.25 + 128.0 / 44100.0)).rem_euclid(1.0);
                let got = (s.ph[0] - p.ops[0].phase as f64).rem_euclid(1.0);
                assert!((got - want).abs() < 1e-4 || (got - want).abs() > 1.0 - 1e-4, "{got} {want}");
            }
        }
        // one that is not free keeps its set start
        p.ops[0].free = false;
        let mut v = Voice::start(&p, 44100.0, 57, [0.8, 0.5, 0.5], 440.0 / 44100.0, 1);
        let before = v.subs[0].ph[0];
        v.started_at(&p, 440.0, 3.25);
        assert_eq!(v.subs[0].ph[0], before);
    }

    /// An operator fed back into itself is read far weaker than a feed from another, stays a steady tone at
    /// the top of the knob and gains overtones on the way.
    #[test]
    fn self_feed_is_gentle_and_steady() {
        let mut d = plain_block();
        d[12..16].copy_from_slice(&64i32.to_le_bytes());
        let p = Patch::from_plain(&d, 25).unwrap();
        assert!((p.fm[0][0] / p.fm[0][1] - SELF_FEED).abs() < 0.001);
        let mut q = lone();
        q.fm[0][0] = 2.0 * 0.07 * 1.1;
        q.wire();
        let o = run(&q, 57, 24000, 24000);
        let (a, b, between) = (level(&o[4800..], 440.0), level(&o[4800..], 880.0), level(&o[4800..], 660.0));
        assert!(b > a * 0.3 && b < a && between < a * 0.01, "{a} {b} {between}");
    }

    /// The pre-filter cuts drawn harmonics too: at the bottom of its knob four are left.
    #[test]
    fn pre_filter_cuts_drawn_harmonics() {
        let mut h = vec![0u8; 2128];
        h[40..44].copy_from_slice(&10000i32.to_le_bytes());
        h[4..8].copy_from_slice(&256i32.to_le_bytes());
        for k in 0..12 {
            h[52 + k * 4..56 + k * 4].copy_from_slice(&0.5f32.to_le_bytes());
        }
        let part = |t: &[f32], n: usize| {
            let m = t.len() - 1;
            let (mut c, mut s) = (0.0f32, 0.0f32);
            for (k, v) in t[..m].iter().enumerate() {
                let w = 2.0 * PI * (n * k) as f32 / m as f32;
                c += v * w.cos();
                s += v * w.sin();
            }
            (c * c + s * s).sqrt() / m as f32
        };
        h[2116..2120].copy_from_slice(&128i32.to_le_bytes());
        let open = op_from(&h, &OpLay::NEW);
        assert!(part(&open.tables[0], 9) > part(&open.tables[0], 1) * 0.9);
        h[2116..2120].copy_from_slice(&0i32.to_le_bytes());
        let shut = op_from(&h, &OpLay::NEW);
        assert!(part(&shut.tables[0], 4) > part(&shut.tables[0], 1) * 0.9);
        assert!(part(&shut.tables[0], 5) < part(&shut.tables[0], 1) * 0.01);
    }

    /// With drawn harmonics the squeeze switch squeezes every harmonic's own wave, and the fold works on the sum.
    #[test]
    fn half_switches_work_per_drawn_harmonic() {
        let mut h = vec![0u8; 2128];
        h[40..44].copy_from_slice(&10000i32.to_le_bytes());
        h[4..8].copy_from_slice(&256i32.to_le_bytes());
        h[2116..2120].copy_from_slice(&128i32.to_le_bytes());
        h[56..60].copy_from_slice(&1.0f32.to_le_bytes());
        h[2121] = 1;
        let t = op_from(&h, &OpLay::NEW).tables[0].clone();
        let m = t.len() - 1;
        let loud = |from: usize, to: usize| t[m * from / 16..m * to / 16].iter().fold(0.0f32, |a, v| a.max(v.abs()));
        // the second harmonic alone: each of its two cycles plays in half of its own time
        assert!(loud(1, 3) > 0.5 && loud(9, 11) > 0.5, "{} {}", loud(1, 3), loud(9, 11));
        assert!(loud(5, 7) < 1e-3 && loud(13, 15) < 1e-3, "{} {}", loud(5, 7), loud(13, 15));
        h[2121] = 0;
        h[2122] = 1;
        h[52..56].copy_from_slice(&1.0f32.to_le_bytes());
        let t = op_from(&h, &OpLay::NEW).tables[0].clone();
        assert!(t.iter().all(|&v| v >= 0.0));
        // folded as a sum the wave still repeats once a period (folded one by one it would repeat twice)
        let (mut c, mut q) = (0.0f32, 0.0f32);
        for (k, v) in t[..m].iter().enumerate() {
            let w = 2.0 * PI * k as f32 / m as f32;
            c += v * w.cos();
            q += v * w.sin();
        }
        assert!((c * c + q * q).sqrt() / m as f32 > 0.05);
    }

    /// The sustain knob lowers the level held when it is below the middle, and the first layout keeps its
    /// switches in the same order as the later ones.
    #[test]
    fn sustain_knob_goes_both_ways_and_old_switches_keep_their_order() {
        let pt = |x: f32, y: f32| Pt { x, y, ..Default::default() };
        let mut c = Curve { pts: vec![pt(0.0, 0.0), pt(0.1, 1.0), pt(0.4, 1.0), pt(0.1, 0.0)], flags: 0, live: true, decay: 1, loop_start: -1, sustain: 2, span: (0, 0), t: vec![] };
        c.lift(-0.5);
        c.prepare([1.0; 3]);
        assert!((c.env(5.0, None) - 0.5).abs() < 1e-6 && (c.env(0.1, None) - 1.0).abs() < 1e-6);
        let mut h = vec![0u8; 271];
        h[264 + 4] = 1;
        h[264 + 6] = 1;
        let w = OpLay::FIRST.widen(&h);
        assert_eq!(&w[2120..2128], &[0, 0, 0, 0, 1, 0, 1, 0]);
    }

    /// A muted knob of the grid plays as zero, and only that knob.
    #[test]
    fn a_muted_grid_knob_is_silent() {
        let mut d = plain_block();
        let p = Patch::from_plain(&d, 25).unwrap();
        assert!(p.out[0] > 0.0 && p.out[6] > 0.0 && p.feed[0][0] > 0.0);
        d[800 + 8] = 1;
        d[800 + 6 * 9] = 1;
        let p = Patch::from_plain(&d, 25).unwrap();
        assert_eq!((p.out[0], p.feed[0][0]), (0.0, 0.0));
        assert!(p.out[6] > 0.0 && p.fm[0][1] > 0.0);
    }

    /// One switch byte lets a standing level drain away, and an operator at no pitch starts where it is set.
    #[test]
    fn a_standing_level_can_drain_away() {
        let mut d = plain_block();
        assert!(!Patch::from_plain(&d, 25).unwrap().tail.level);
        d[967] = 1;
        let t = Patch::from_plain(&d, 25).unwrap().tail;
        assert!(t.level && t.any() && !t.split);
        let mut st = crate::tailfx::State::new(44100.0);
        let mut buf = vec![[1.0f32; 2]; 44100];
        st.process(&t, &mut buf, 5512.0, None);
        // a step falls to 1/e in a 3.8 Hz stage's time, 42 ms
        let at = buf.iter().position(|f| f[0] < (-1.0f32).exp()).unwrap() as f32 / 44100.0;
        assert!((at - 0.0419).abs() < 0.002, "{at}");
        assert!(buf[44099][0].abs() < 1e-6 && buf[0][0] > 0.99, "{} {}", buf[44099][0], buf[0][0]);
    }

    /// Nothing sent to the effects leaves them out; a send brings in the echo, chorus and room that are switched on.
    #[test]
    fn the_effects_follow_the_send() {
        let put = |d: &mut Vec<u8>, o: usize, v: i32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let mut d = plain_block();
        for (o, v) in [(724, 4), (728, 512), (732, 768), (736, 8), (740, 128), (744, -32), (748, 128), (978, 64), (982, 144), (990, 128), (1047, 75), (1051, 40), (1059, 40), (1063, 100), (1067, 15), (1071, 40), (1075, 2), (1079, 64)] {
            put(&mut d, o, v);
        }
        d[998] = 1;
        d[1000] = 1;
        d[1083] = 1;
        assert!(!Patch::from_plain(&d, 25).unwrap().tail.any(), "no send, no effects");
        put(&mut d, 12 + 7 * 4, 128);
        // the effects pan: centre leaves the send alone, half left gives 1.3066 and 0.5412 (measured)
        assert_eq!(Patch::from_plain(&d, 25).unwrap().send_side.map(|g| (g * 1000.0).round()), [1000.0, 1000.0]);
        put(&mut d, 720, -64);
        let side = Patch::from_plain(&d, 25).unwrap().send_side;
        assert!((side[0] - 1.3066).abs() < 1e-3 && (side[1] - 0.5412).abs() < 1e-3, "{side:?}");
        put(&mut d, 720, 0);
        let t = Patch::from_plain(&d, 25).unwrap().tail;
        assert!(t.split && t.delay.is_none() && t.reverb.is_none());
        let o = t.op.expect("the effects section");
        let c = o.chorus.expect("the chorus is on");
        assert_eq!((c.order, c.depth, c.speed, c.delay, c.spread, c.cross, c.vol), (4, 0.5, 0.75, 0.0625, 1.0, -0.25, 1.0));
        let e = o.echoes[0].expect("the first echo is on");
        assert_eq!((e.fb, e.steps, e.vol, e.mode, e.tempo, e.serial), (0.5, 3.0, 1.0, 0, true, false));
        assert!(o.echoes[1].is_none() && o.echoes[2].is_none());
        let r = o.room.expect("the room is on");
        assert_eq!((r.decay, r.damping, r.size, r.wet), (15.0, 40.0, 40.0, 0.5));
        // a second echo after the first, which now feeds it
        for (o, v) in [(1001, -40), (1005, 96), (1013, 64), (1017, 2)] {
            put(&mut d, o, v);
        }
        d[1023] = 1;
        d[999] = 1;
        let o = Patch::from_plain(&d, 25).unwrap().tail.op.unwrap();
        let e = o.echoes[1].unwrap();
        assert!(o.echoes[0].unwrap().serial && e.steps == 2.0 && e.mode == 2 && e.fb < 0.0 && (e.vol - 0.2317).abs() < 1e-3);
        d[1023] = 0;
        d[1083] = 0;
        d[1000] = 0;
        put(&mut d, 724, 0);
        let t = Patch::from_plain(&d, 25).unwrap().tail;
        assert!(t.split && !t.op.unwrap().any() && t.tone.is_none());
        // the equaliser: switched to output and effects, its low band pulled down
        d[1085] = 1;
        for (o, v) in [(1089, -900), (1093, 5777), (1097, 17500), (1101, 0), (1109, 33145), (1117, 1), (1125, 55825), (1133, 2)] {
            put(&mut d, o, v);
        }
        let bands = Patch::from_plain(&d, 25).unwrap().tail.tone.expect("the equaliser runs");
        assert_eq!((bands[0].kind, bands[0].db, bands[1].kind, bands[2].kind), (0, -9.0, 1, 2));
        assert!((bands[0].hz - 91.0).abs() < 1.0 && (bands[1].hz - 1507.0).abs() < 10.0 && (bands[2].hz - 8010.0).abs() < 60.0, "{bands:?}");
        d[1085] = 0;
        assert!(Patch::from_plain(&d, 25).unwrap().tail.tone.is_none());
    }

    /// The older point layouts read, and a line changed in one is written back in the current layout.
    #[test]
    fn old_lines_read_and_come_back_current() {
        for (sub, size) in [(0i32, 12usize), (1, 16)] {
            let mut raw = vec![9u8; 5];
            for v in [3i32, sub, 2] {
                raw.extend_from_slice(&v.to_le_bytes());
            }
            for (x, y, t) in [(0.0f32, 0.25f32, 0.0f32), (1.5, 1.0, 0.5)] {
                let mut pt = vec![];
                for f in [x, y, t] {
                    pt.extend_from_slice(&f.to_le_bytes());
                }
                pt.resize(size, 0);
                raw.extend_from_slice(&pt);
            }
            for v in [0i32, 7, -1, -1, -1] {
                raw.extend_from_slice(&v.to_le_bytes());
            }
            raw.extend_from_slice(&[8, 8]);
            let mut c = Curve::read_at(&raw, 5).unwrap();
            assert_eq!(c.pts.len(), 2);
            assert_eq!((c.pts[1].x, c.pts[1].y, c.pts[1].tension), (1.5, 1.0, 0.5));
            c.pts[0].y = 0.5;
            assert!(c.store(&mut raw));
            assert_eq!(at(&raw, 9), 3);
            let again = Curve::read_at(&raw, 5).unwrap();
            assert_eq!(again.pts, c.pts);
            // the number we do not read and what follows the record stay
            assert_eq!(at(&raw, again.span.1 - 16), 7);
            assert_eq!(&raw[again.span.1..], &[8, 8]);
        }
        // the first layout's operator settings land where the current reader looks
        let mut h = vec![0u8; 271];
        h[40..44].copy_from_slice(&20000i32.to_le_bytes());
        h[48..52].copy_from_slice(&1.0f32.to_le_bytes());
        h[240..244].copy_from_slice(&256i32.to_le_bytes());
        h[260..264].copy_from_slice(&64i32.to_le_bytes());
        h[264] = 1;
        let w = OpLay::FIRST.widen(&h);
        assert_eq!((at(&w, 40), at(&w, 48), at(&w, 2100), w[2120]), (20000, 64, 256, 1));
        assert_eq!(f32::from_le_bytes(w[52..56].try_into().unwrap()), 1.0);
    }

    /// With the user's own content at hand: every preset of a layout we read must sound, stay bounded and end.
    #[test]
    fn own_presets_sound_and_stay_bounded() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Sytrus")];
        let (mut n, mut silent) = (0, 0);
        let mut peaks = vec![];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(pre) = crate::preset::read(&path) else { continue };
                let Some(p) = Patch::parse(&pre.state) else { continue };
                let o = run_with_send(&p, 48, 24000, 30000, true);
                let peak = o.iter().fold(0.0f32, |a, f| a.max(f[0].abs()).max(f[1].abs()));
                assert!(peak.is_finite() && peak < 48.0, "{} peaks at {peak}", path.display());
                n += 1;
                if peak < 1e-3 {
                    silent += 1;
                    continue;
                }
                peaks.push(peak);
            }
        }
        peaks.sort_by(|a, b| a.total_cmp(b));
        if n > 0 {
            eprintln!("presets sounded: {n}, silent {silent}, peak median {:.3}, 90% {:.3}, top {:.3}", peaks[peaks.len() / 2], peaks[peaks.len() * 9 / 10], peaks[peaks.len() - 1]);
            assert!(silent * 20 < n, "too many silent presets");
        }
    }

    #[test]
    fn a_blank_instrument_reads_and_takes_edits() {
        let state = Patch::blank_state();
        let p = Patch::parse(&state).unwrap();
        assert_eq!(p.out[0], 1.0);
        assert_eq!(p.ops[0].ratio, 2.0);
        assert_eq!(p.used(), [true, false, false, false, false, false]);
        // operator 2 into operator 1 at 25 %, and a point more in operator 1's volume envelope
        let mut env = p.arts[1].env.clone();
        env.pts.insert(1, Pt { x: 0.005, y: 0.5, ..Default::default() });
        let (state2, q) = Patch::edited(&state, |d| {
            d[12 + 4..12 + 8].copy_from_slice(&64i32.to_le_bytes());
            assert!(env.store(d));
        })
        .unwrap();
        assert!((q.fm[0][1] - 0.163).abs() < 0.001);
        assert_eq!(q.arts[1].env.pts.len(), 5);
        assert_eq!(q.arts[61].env.pts.len(), 4);
        assert_eq!(Patch::unpack(&state2).unwrap().len(), Patch::unpack(&state).unwrap().len() + 24);
        // an edit that breaks the layout is refused
        assert!(Patch::edited(&state, |d| d.truncate(5000)).is_none());
    }

    #[test]
    fn an_edited_line_goes_back_into_its_block() {
        let mut raw = Sender::fresh();
        let before = raw.clone();
        let s = Sender::parse(&raw).unwrap();
        // storing an untouched line changes nothing
        assert!(s.arts[2].art.env.store(&mut raw));
        assert_eq!(raw, before);
        // a point more in one output's envelope: the block grows, still reads, and the others are as they were
        let mut c = s.arts[2].art.env.clone();
        c.pts.insert(2, Pt { x: 0.1, y: 0.25, tension: 0.5, mode: 2 });
        c.sustain = 3;
        assert!(c.store(&mut raw));
        assert_eq!(raw.len(), before.len() + 24);
        let t = Sender::parse(&raw).unwrap();
        assert_eq!(t.arts[2].art.env.pts.len(), 5);
        assert_eq!(t.arts[2].art.env.pts[2], Pt { x: 0.1, y: 0.25, tension: 0.5, mode: 2 });
        assert_eq!(t.arts[2].art.env.sustain, 3);
        assert_eq!(t.arts[1].art.env.pts, s.arts[1].art.env.pts);
        assert_eq!(t.arts[7].art.maps[4].pts, s.arts[7].art.maps[4].pts);
        assert_eq!((t.arts[7].lo, t.arts[7].hi), (s.arts[7].lo, s.arts[7].hi));
        // a line whose place has moved is refused rather than written over something else
        assert!(!s.arts[5].art.env.store(&mut raw[..100].to_vec()));
    }

    #[test]
    fn curves_hold_loop_and_let_go() {
        let mut c = Curve { pts: vec![Pt { x: 0.0, y: 0.0, ..Default::default() }, Pt { x: 1.0, y: 1.0, ..Default::default() }, Pt { x: 1.0, y: 0.5, ..Default::default() }, Pt { x: 2.0, y: 0.0, ..Default::default() }], flags: 0, live: true, decay: 1, loop_start: -1, sustain: 2, span: (0, 0), t: vec![] };
        c.prepare([0.5, 1.0, 2.0]);
        assert!((c.env(0.25, None) - 0.5).abs() < 1e-5);
        assert!((c.env(9.0, None) - 0.5).abs() < 1e-5);
        assert!((c.env(11.0, Some(9.0)) - 0.25).abs() < 1e-5);
        assert_eq!(c.env(14.0, Some(9.0)), 0.0);
        // let go during the attack: falls from where it was
        let v = c.env(0.25, Some(0.25));
        assert!((v - 0.5).abs() < 1e-5);
        assert!((c.tail(0.0) - 4.0).abs() < 1e-5);
        c.loop_start = 1;
        c.prepare([1.0; 3]);
        assert!((c.env(2.5, None) - c.env(1.5, None)).abs() < 1e-5);
        assert!((Curve::flat(0.7).map(0.3) - 0.7).abs() < 1e-6);
    }
}

