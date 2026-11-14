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

const TN: usize = 2048;
/// Frames between two looks at the envelopes.
const TICK: u32 = 16;
/// Phase swing, in cycles, of a full-scale modulator at 100 %.
const DEPTH: f32 = 2.0;
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

/// Points in the kept cycle of a plucked operator.
const STR: usize = 1024;

/// Wears a plucked operator's kept cycle down by `dt` seconds of going round at `hz`, with the damping
/// setting at `d` (1 is the loosest). Measured on long notes over a spread of keys and settings: harmonic h
/// falls by k * h * h dB a second, where k grows with the cube of the pitch (the same loss on every trip
/// round) and steeply as the setting comes down. The setting as made slopes up with the key, which is why
/// high notes do not die at once. The loss is made here by smoothing the cycle forwards and
/// backwards, which leaves the phases alone.
fn wear(tab: &mut [f32], hz: f32, dt: f32, d: f32) {
    if tab.len() != STR || dt <= 0.0 {
        return;
    }
    // the loss on one trip round, as a multiple of the squared angle a harmonic turns through per frame at 44.1 kHz
    let d = d.clamp(0.0, 1.0);
    let a = (3.338 - 10.002 * d + 3.111 * d * d).exp();
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

/// Bend a 0..=1 ramp. Positive tension moves fast first and settles (nearly every decay in the presets at hand
/// is drawn with positive tension, which only makes sense this way round); negative starts slowly.
pub(crate) fn bend(t: f32, tension: f32) -> f32 {
    let k = (tension.clamp(-1.0, 1.0).abs() * 3.0).exp2();
    if tension.abs() < 1e-4 {
        t
    } else if tension > 0.0 {
        1.0 - (1.0 - t).powf(k)
    } else {
        t.powf(k)
    }
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
    pub centre: Option<usize>,
    pub pluck: Option<usize>,
    /// The first layout: filters hold no type and keep the shaper switch in one byte.
    pub first: bool,
}

impl OpLay {
    pub const NEW: OpLay = OpLay { len: 2128, noise: 48, mags: 52, harmonics: 256, tail: 2100, flags: 2120, centre: Some(2123), pluck: Some(2126), first: false };
    pub const MID: OpLay = OpLay { len: 1104, noise: 48, mags: 52, harmonics: 256, tail: 1076, flags: 1096, centre: Some(1099), pluck: Some(1102), first: false };
    /// Which of the first layout's last switches are centre and plucked is not known, so they read as off.
    pub const FIRST: OpLay = OpLay { len: 271, noise: 260, mags: 48, harmonics: 48, tail: 240, flags: 264, centre: None, pluck: None, first: true };

    /// The settings as the current layout holds them.
    fn widen(&self, h: &[u8]) -> Vec<u8> {
        let mut w = vec![0u8; 2128];
        w[..48].copy_from_slice(&h[..48]);
        w[48..52].copy_from_slice(&h[self.noise..self.noise + 4]);
        w[52..52 + self.harmonics * 4].copy_from_slice(&h[self.mags..self.mags + self.harmonics * 4]);
        w[2100..2120].copy_from_slice(&h[self.tail..self.tail + 20]);
        w[2120..2123].copy_from_slice(&h[self.flags..self.flags + 3]);
        if let Some(c) = self.centre {
            w[2123] = h[c];
        }
        if let Some(c) = self.pluck {
            w[2126] = h[c];
        }
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
        if let (true, Some(p)) = (by > 0.0, self.pts.get_mut(at)) {
            p.y = (p.y + by).min(1.0);
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
                return if b.mode == 2 { a.y } else { a.y + (b.y - a.y) * swell(u, b.tension) };
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
                if span <= 1e-6 || dt >= span {
                    self.pts[n - 1].y
                } else {
                    self.seg(ts + dt) + (from - self.pts[s].y) * (1.0 - dt / span)
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
    /// Key, velocity, X, Y, random, unison.
    pub maps: [Curve; 6],
    /// The setting is an offset around the middle (pan, pitch, phase, cutoff, resonance) rather than a level.
    pub bipolar: bool,
}

impl Art {
    fn neutral(bipolar: bool) -> Self {
        let y = if bipolar { 0.5 } else { 1.0 };
        Self { env: Curve::flat(y), lfo: Curve::flat(0.5), lfo_hz: 5.0, maps: std::array::from_fn(|_| Curve::flat(y)), bipolar }
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
    fn value(&self, stat: f32, t: f32, rel: Option<f32>, lfo_gain: f32, beat: f32) -> f32 {
        let read = |c: &Curve| {
            let b = if c.flags & 1 != 0 { beat } else { 2.0 };
            c.env(t * b, rel.map(|r| r * b))
        };
        let l = if self.lfo.on() { (TAU * self.lfo_hz * t).sin() * (read(&self.lfo) - 0.5) * 2.0 * lfo_gain } else { 0.0 };
        if self.bipolar {
            stat + if self.env.on() { (read(&self.env) - 0.5) * 2.0 } else { 0.0 } + l
        } else {
            (stat * if self.env.on() { read(&self.env) } else { 1.0 } * (1.0 + l)).max(0.0)
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
    pub uni_pan: f32,
    pub uni_vol: f32,
    pub uni_pitch: f32,
    pub uni_phase: f32,
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
        env.lift(k[2].max(0) as f32 / 128.0 + g[3]);
        env.prepare([knob(k[0]) * g[0], knob(k[1]) * g[1], knob(k[3]) * g[2]]);
        let mut lfo = self.curve(2)?;
        let speed = self.i()?;
        self.bytes(self.lfo_rest)?;
        lfo.prepare([1.0; 3]);
        let mut maps: [Curve; 6] = Default::default();
        for m in maps.iter_mut() {
            *m = self.curve(3)?;
            m.prepare([1.0; 3]);
        }
        Some(Art { env, lfo, lfo_hz: 0.05 * (speed.clamp(0, 65536) as f32 / 65536.0 * 11.0).exp2(), maps, bipolar })
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
            env.lift(len[2].max(0) as f32 / 128.0);
            env.prepare([knob(len[0]), knob(len[1]), knob(len[3])]);
            let mut lfo = r.curve(2)?;
            let speed = r.i()?;
            r.bytes(16)?;
            lfo.prepare([1.0; 3]);
            let mut art = Art::neutral(bipolar);
            for m in art.maps.iter_mut().take(n_maps) {
                *m = r.curve(3)?;
                m.prepare([1.0; 3]);
            }
            art.env = env;
            art.lfo = lfo;
            art.lfo_hz = 0.05 * (speed.clamp(0, 65536) as f32 / 65536.0 * 11.0).exp2();
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
        let l = (TAU * s.art.lfo_hz * t).sin() * (s.art.lfo.env(t, rel) - 0.5) * 2.0 * s.lfo;
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
        y = (a * y).sin() / if a < PI * 0.5 { a.sin() } else { 1.0 };
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
    let mut base: Vec<f32> = (0..TN).map(|k| shape(sh, tn, sk, sn, k as f32 / TN as f32)).collect();
    // the pre-filter cuts the wave off above a harmonic: four are left at the bottom of the knob, three times as
    // many for every eighth of its travel (measured at 0, 16, 32)
    let pre = i(2116).clamp(0, 128);
    if pre < 128 {
        keep_harmonics(&mut base, (4.33 * 3f32.powf(pre as f32 / 16.0)) as usize);
    }
    let mags: Vec<f32> = (0..256).map(|k| f(52 + k * 4)).map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }).collect();
    let phs: Vec<f32> = (0..256).map(|k| f(1076 + k * 4)).map(|v| if v.is_finite() { v } else { 0.0 }).collect();
    let top = mags.iter().rposition(|&m| m > 0.0).map(|k| k + 1).unwrap_or(0);
    let plain = top <= 1 && (top == 0 || (mags[0] == 1.0 && phs[0] == 0.0));
    let finish = |mut t: Vec<f32>, scale: bool| -> Arc<Vec<f32>> {
        if scale {
            let peak = t.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            if peak > 1e-6 {
                t.iter_mut().for_each(|v| *v /= peak);
            }
        }
        if stretch {
            t = (0..TN).map(|j| (t[j / 2] + t[(j + 1) / 2]) * 0.5).collect();
        }
        if squeeze {
            t = (0..TN).map(|j| if j < TN / 2 { t[j * 2] } else { 0.0 }).collect();
        }
        if absolute {
            t.iter_mut().for_each(|v| *v = v.abs());
        }
        t.push(t[0]);
        Arc::new(t)
    };
    let (mut tables, mut caps) = (vec![], vec![]);
    if plain {
        tables.push(finish(base, false));
        caps.push(1);
    } else {
        for cap in [256usize, 64, 16, 4] {
            let n = top.min(cap);
            if caps.last() == Some(&(n as u32)) {
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
            tables.push(finish(t, true));
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
        let mut p = Self { volume: 1.0, lfo: 1.0, pitch: 0.0, send: [0.0; 9], fm: [[0.0; 6]; 6], rm: [[0.0; 6]; 6], feed: [[0.0; 6]; 3], pan: [0.0; 9], out, ops, filts: Default::default(), arts, uni: 1, uni_pan: 1.0, uni_vol: 0.8, uni_pitch: 0.5, uni_phase: 0.1, modx: 0.5, mody: 0.75, op_at: [0; 6], filt_at: [0; 3], lay: OpLay::NEW, tail: Default::default(), used: [false; 6], fused: [false; 3], any_moving: false };
        p.wire();
        p
    }
}

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
        p.volume = at(d, 0).clamp(0, 256) as f32 / 128.0;
        p.lfo = at(d, 4).clamp(-128, 128) as f32 / 128.0;
        p.pitch = at(d, 8).clamp(-48, 48) as f32;
        // every knob of the grid follows one curve (measured on renders at a dozen settings): half way up
        // gives 0.23 of the full amount
        let cell = |base: usize, r: usize, c: usize| {
            grid(at(d, base + r * 36 + c * 4).clamp(-256, 256) as f32 / 256.0)
        };
        for r in 0..9 {
            p.pan[r] = at(d, 12 + r * 36 + 24).clamp(-256, 256) as f32 / 256.0;
            let (send, out) = (cell(12, r, 7), cell(12, r, 8));
            sent += send.abs();
            plain += out.abs();
            p.out[r] = out;
            p.send[r] = send;
            for c in 0..6 {
                if r < 6 {
                    p.fm[r][c] = cell(12, r, c) * 2.0;
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
        p.uni_phase = at(d, 712).clamp(0, 128) as f32 / 128.0;
        p.modx = at(d, 728).clamp(0, 1024) as f32 / 1024.0;
        p.mody = at(d, 732).clamp(0, 1024) as f32 / 1024.0;
        if ver >= 19 && sent > 0.0 {
            p.tail = effects(d);
            p.tail.split = true;
        } else {
            // older blocks carry no effects here: what is sent is simply heard
            for r in 0..9 {
                p.out[r] += p.send[r].abs();
                p.send[r] = 0.0;
            }
        }
        if ver >= 19 && d.len() >= 1137 {
            // the equaliser stands on the output, on the effects' share, or on both; here it is one filter
            // over the whole sound, so it runs unless it is meant for a share that is not the greater part
            let _ = plain;
            p.tail.tone_at = d[1085];
            if (1..=3).contains(&d[1085]) && (sent > 0.0 || d[1085] != 3) {
                p.tail.tone = equaliser(d);
            }
        }

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

    /// Operators that are heard directly or through a filter.
    fn carriers(&self) -> impl Iterator<Item = usize> + '_ {
        (0..6).filter(|&k| self.used[k] && (self.out[k] != 0.0 || self.send[k] != 0.0 || (0..3).any(|j| self.fused[j] && self.feed[j][k] != 0.0)))
    }

    /// Seconds the sound needs after the key is let go `t` seconds into the note.
    /// How many times over a note at this pitch must be worked out so that what its operators do to each
    /// other stays clean: 1 when nothing reaches past the top of the range, else 2 or 4.
    pub fn oversampling(&self, dphase: f32, sr: f32) -> u32 {
        let used = self.used();
        let (mut fastest, mut bend, mut ring) = (0.0f32, 0.0f32, false);
        for r in 0..6 {
            if !used[r] {
                continue;
            }
            fastest = fastest.max((dphase * self.ops[r].ratio + self.ops[r].offset_hz / sr).abs());
            bend = bend.max(self.fm[r].iter().map(|x| x.abs()).sum::<f32>());
            ring |= self.rm[r].iter().any(|&x| x != 0.0);
        }
        if bend == 0.0 && !ring {
            return 1;
        }
        // a full knob spreads partials to some forty times the bending operator's pitch
        let top = fastest * (2.0 + 20.0 * bend);
        if top < 0.5 {
            1
        } else if top < 1.5 {
            2
        } else {
            4
        }
    }

    pub fn tail_s(&self, t: f32, beat: f32) -> f32 {
        let mut tail = 0.0f32;
        let mut vols: Vec<usize> = self.carriers().map(|k| k * 6 + 1).collect();
        vols.extend((0..3).filter(|&k| self.fused[k]).map(|k| 36 + k * 8 + 1));
        for k in vols {
            let e = &self.arts[k].env;
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
    ph: [f32; 6],
    out: [f32; 6],
    prev: [f32; 6],
    amp: [f32; 6],
    inc: [f32; 6],
    tab: [u8; 6],
    pl: [f32; 9],
    pr: [f32; 9],
    fs: [[[f32; 2]; 6]; 3],
    plan: [[[f32; 6]; 6]; 3],
    stages: [u8; 3],
    ap: [[f32; 10]; 3],
    fb: [f32; 3],
    fg: [f32; 3],
    fk: [f32; 3],
    seed: u32,
    /// Plucked operators: the first cycle each one made, going round and wearing down (empty for the others).
    strings: [Vec<f32>; 6],
    /// How far through that first cycle the operator is, the last place written, and time not yet worn off.
    run: [f32; 6],
    fill: [u32; 6],
    due: [f32; 6],
    hiss: [f32; 6],
}

/// Halves the sample rate: a half-band low-pass over the last frames, giving one frame out for two in.
#[derive(Debug, Clone)]
struct Halver {
    buf: [[f32; 2]; 64],
    at: usize,
}

/// The taps of the two halving steps (the first works at four times the rate and can be short).
fn half_taps() -> &'static [Vec<f32>; 2] {
    static T: std::sync::OnceLock<[Vec<f32>; 2]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let make = |n: usize, beta: f64| -> Vec<f32> {
            let i0 = |x: f64| {
                let (mut sum, mut term) = (1.0, 1.0);
                for k in 1..30 {
                    term *= (x / 2.0 / k as f64).powi(2);
                    sum += term;
                }
                sum
            };
            let mid = (n - 1) as f64 / 2.0;
            let mut t: Vec<f64> = (0..n)
                .map(|k| {
                    let d = k as f64 - mid;
                    let sinc = if d == 0.0 { 0.5 } else { (std::f64::consts::PI * d / 2.0).sin() / (std::f64::consts::PI * d) };
                    sinc * i0(beta * (1.0 - (d / mid).powi(2)).max(0.0).sqrt()) / i0(beta)
                })
                .collect();
            let sum: f64 = t.iter().sum();
            t.iter_mut().for_each(|x| *x /= sum);
            t.into_iter().map(|x| x as f32).collect()
        };
        [make(15, 5.0), make(59, 7.0)]
    })
}

impl Halver {
    fn new() -> Self {
        Self { buf: [[0.0; 2]; 64], at: 0 }
    }

    fn push(&mut self, a: [f32; 2], b: [f32; 2], taps: &[f32]) -> [f32; 2] {
        self.buf[self.at & 63] = a;
        self.buf[(self.at + 1) & 63] = b;
        self.at = (self.at + 2) & 63;
        let mut out = [0.0f32; 2];
        for (k, &t) in taps.iter().enumerate() {
            if t != 0.0 {
                let x = self.buf[(self.at + 64 - 1 - k) & 63];
                out[0] += x[0] * t;
                out[1] += x[1] * t;
            }
        }
        out
    }
}

/// One sounding note.
#[derive(Debug, Clone)]
pub struct Voice {
    /// how many frames are worked out for each one given: operators that bend each other make partials far
    /// above the top of the hearing range, and those must be made and filtered away, not folded back down
    over: u32,
    halvers: [Halver; 2],
    /// the same for what goes to the effects, and that share of the last frame made
    send_halvers: [Halver; 2],
    sent: [f32; 2],
    sr: f32,
    t: u32,
    rel: Option<f32>,
    ctl: u32,
    /// beats in a second, for lines that follow the tempo
    pub beat: f32,
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
        let mut seed = seed | 1;
        let over = p.oversampling(dphase, sr);
        let (sr, dphase) = (sr * over as f32, dphase / over as f32);
        let n = p.uni.clamp(1, 9) as usize;
        let rand = rnd(&mut seed) * 0.5 + 0.5;
        let mut subs = Vec::with_capacity(n);
        for s in 0..n {
            let u = if n == 1 { 0.0 } else { s as f32 / (n - 1) as f32 * 2.0 - 1.0 };
            let xs = [(key as f32 / 120.0).min(1.0), axes[0].clamp(0.0, 1.0), axes[1].clamp(0.0, 1.0), axes[2].clamp(0.0, 1.0), rand, (u + 1.0) * 0.5];
            let mut stat = [0.0; ARTS];
            for (k, a) in p.arts.iter().enumerate().take(ARTS) {
                stat[k] = a.stat(&xs);
            }
            let mut ph = [0.0; 6];
            let mut tab = [0u8; 6];
            for k in 0..6 {
                let o = &p.ops[k];
                ph[k] = o.phase + if n > 1 { (rnd(&mut seed) * 0.5 + 0.5) * p.uni_phase } else { 0.0 };
                if o.free {
                    ph[k] += rnd(&mut seed) * 0.5 + 0.5;
                }
                // the fullest table whose top harmonic still fits under half the sample rate
                let f = (dphase * o.ratio + o.offset_hz / sr).abs().max(1e-6);
                let fit = (0.45 / f) as u32;
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
                detune: (u * p.uni_pitch * 24.0 / 1200.0).exp2(),
                pan: u * p.uni_pan * 0.8,
                gain: (1.0 - u.abs() * p.uni_vol * 0.5) / (n as f32).sqrt(),
                ph,
                out: [0.0; 6],
                prev: [0.0; 6],
                amp: [0.0; 6],
                inc: [0.0; 6],
                tab,
                pl: [1.0; 9],
                pr: [1.0; 9],
                fs: [[[0.0; 2]; 6]; 3],
                plan: [[[0.0; 6]; 6]; 3],
                stages: [0; 3],
                ap: [[0.0; 10]; 3],
                fb: [0.0; 3],
                fg: [0.1; 3],
                fk: [1.0; 3],
                seed: seed.wrapping_add(s as u32 * 7919) | 1,
            });
        }
        let mut v = Self { over, halvers: [Halver::new(), Halver::new()], send_halvers: [Halver::new(), Halver::new()], sent: [0.0; 2], sr, t: 0, rel: None, ctl: 0, beat: 2.0, subs, main_l: 1.0, main_r: 1.0 };
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

    fn control(&mut self, p: &Patch, dphase: f32, first: bool) {
        let t = self.t as f32 / self.sr;
        let rel = self.rel;
        let sr = self.sr;
        let beat = self.beat;
        for s in self.subs.iter_mut() {
            if first || p.any_moving {
                for (k, a) in p.arts.iter().enumerate().take(ARTS) {
                    if first || a.moving() {
                        s.val[k] = a.value(s.stat[k], t, rel, p.lfo, beat);
                    }
                }
            }
            for k in 0..6 {
                let o = &p.ops[k];
                let bend = (p.pitch / 12.0 + s.val[k * 6 + 3] * o.pitch_cents / 1200.0).exp2();
                s.inc[k] = (dphase * s.detune * bend * o.ratio + o.offset_hz / sr).clamp(-0.5, 0.5);
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
                let c = (f.cut + s.val[b + 2] * f.env_amt).clamp(0.0, 1.0);
                let r = (f.res + s.val[b + 3] * 0.5).clamp(0.0, 1.0);
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
                    4 | 5 | 7 => {
                        // band kinds keep their overall width whatever the slope: each stage of a band-pass
                        // is made wider, each stage of a notch narrower
                        let w = table(&WIDTH, r);
                        let steps = f.slope.max(1) as f32;
                        let kk = width(if f.kind == 5 { w / n.max(1) as f32 } else if f.kind == 4 { w * steps } else { w });
                        let m = match f.kind {
                            4 => [0.0, kk, 0.0],
                            5 => [1.0, -kk, 0.0],
                            _ => [1.0, -2.0 * kk, 0.0],
                        };
                        plan[..n].fill(stage(s.fg[k], kk, m));
                        if f.kind == 4 && f.slope >= 4 {
                            // the two widest band-passes move their later stages up, which spreads the band
                            let far = if f.slope == 4 { 0.435 } else { 0.613 };
                            plan[1..n].fill(stage(g_at(band_hz(1.0 - far * (1.0 - c))), kk, m));
                        }
                    }
                    6 => {
                        let kk = table(&DAMP_WIDE, r).powf(1.0 / n.max(1) as f32);
                        plan[..n].fill(stage(s.fg[k], kk, [1.0, -kk, -1.0]));
                    }
                    8..=10 => {
                        // shelves and the peak are one stage whatever the slope says
                        count = n.min(1);
                        let a = 10f32.powf(f.db / 40.0);
                        plan[0] = if f.kind == 9 {
                            let kk = 1.0 / (table(&PEAK_Q, r) * a);
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
                        s.fg[k] = g_at((86.46 * 159.6f32.powf(c) - 82.3).max(1.0));
                        s.fk[k] = 0.98 * r.sqrt();
                    }
                    11 => {
                        // the drive kind is a ladder of four plain stages per slope step, with the resonance
                        // fed back round the first ladder and half made up for at its input
                        count = n;
                        let g = g_at(180.3 * 167.3f32.powf(c) - 70.3);
                        let big = g / (1.0 + g);
                        let back = 3.83 * r;
                        for (st, slot) in plan.iter_mut().enumerate().take(n) {
                            let kb = if st == 0 { back } else { 0.0 };
                            *slot = [big, kb, 1.0 + 0.49 * kb, 1.0 / (1.0 + kb * big * big * big * big), 0.0, 0.0];
                        }
                        if f.drive > 0.0 {
                            // with any drive the ladder works on a scaled copy of the signal: how far it is
                            // pushed into the bend, and what brings it back out
                            let lv = f.drive * 128.0;
                            let push = 1.692 * 17.9f32.powf((lv - 64.0) / 64.0);
                            const UP: [f32; 5] = [0.0, 1.9, 6.1, 11.6, 17.4];
                            let at = ((lv - 64.0) / 16.0).clamp(0.0, 4.0);
                            let i = (at as usize).min(3);
                            let db = UP[i] + (UP[i + 1] - UP[i]) * (at - i as f32);
                            plan[0][4] = push;
                            plan[0][5] = 10f32.powf(db / 20.0) / push;
                        }
                    }
                    kind => {
                        // the low-pass family: one stage rings, the rest stay just short of ringing; the two
                        // widest slopes put their other stages part of the way up to the top and ring there
                        let far: [f32; 3] = match (f.slope, kind) {
                            (4, 3) => [0.0, 0.05, 0.0],
                            (5, 3) => [0.0, 0.538, 0.163],
                            (4, 0) => [0.0, 0.53, 0.0],
                            (5, 0) => [0.0, 0.774, 0.592],
                            (4, _) => [0.0, 0.41, 0.0],
                            _ => [0.0, 0.716, 0.492],
                        };
                        let spread = f.slope >= 4;
                        let ringer = if spread { n - 1 } else { 0 };
                        let lv = [f.levels[0] * s.val[b + 4], f.levels[1] * s.val[b + 5], f.levels[2] * s.val[b + 6]];
                        for (st, slot) in plan.iter_mut().enumerate().take(count) {
                            let cs = if spread && st > 0 { 1.0 - far[st % 3] * (1.0 - c) } else { c };
                            let kk = match kind {
                                0 => if st == ringer { 1.0 - 0.79 * r } else { 1.0 },
                                3 => table(&DAMP_WIDE, r).powf(1.0 / n.max(1) as f32),
                                _ if spread => table(&DAMP, r).powf(1.0 / n as f32),
                                _ => if st == ringer { table(&DAMP, r) } else { 1.0 },
                            };
                            let m = match kind {
                                0 => [lv[2], lv[1] - lv[2] * kk, lv[0] - lv[2]],
                                2 => [1.0, 0.0, -1.0],
                                _ => [0.0, 0.0, 1.0],
                            };
                            *slot = stage(g_at(cutoff_hz(cs)), kk, m);
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
        match self.over {
            1 => self.one(p, dphase),
            2 => {
                let d = dphase * 0.5;
                let (a, sa) = (self.one(p, d), self.sent);
                let (b, sb) = (self.one(p, d), self.sent);
                if any {
                    self.sent = self.send_halvers[1].push(sa, sb, &half_taps()[1]);
                }
                self.halvers[1].push(a, b, &half_taps()[1])
            }
            _ => {
                let d = dphase * 0.25;
                let t = half_taps();
                let (a, sa) = (self.one(p, d), self.sent);
                let (b, sb) = (self.one(p, d), self.sent);
                let x = self.halvers[0].push(a, b, &t[0]);
                let (c, sc) = (self.one(p, d), self.sent);
                let (e, se) = (self.one(p, d), self.sent);
                let y = self.halvers[0].push(c, e, &t[0]);
                if any {
                    let sx = self.send_halvers[0].push(sa, sb, &t[0]);
                    let sy = self.send_halvers[0].push(sc, se, &t[0]);
                    self.sent = self.send_halvers[1].push(sx, sy, &t[1]);
                }
                self.halvers[1].push(x, y, &t[1])
            }
        }
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
        let fade = {
            let x = self.t as f32 / (self.sr * 0.00052);
            if x < 1.0 { 0.5 - 0.5 * (PI * x).cos() } else { 1.0 }
        };
        for s in self.subs.iter_mut() {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            let (mut sl, mut sr) = (0.0f32, 0.0f32);
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
                        pm += a * if j > i { s.out[j] } else if j == i { (last[i] + s.prev[i]) * 0.5 } else { last[j] };
                    }
                }
                let ph = s.ph[i] + s.val[b + 4] * 0.5;
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
                                s.hiss[i] = rnd(&mut s.seed);
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
                        y
                    } else {
                        let f = x - at as f32;
                        tab[at] + (tab[(at + 1) % STR] - tab[at]) * f
                    }
                } else {
                    let mut y = look(&o.tables[s.tab[i] as usize], ph + pm * s.val[b + 2] * DEPTH);
                    if o.nx > 0.0 {
                        if fresh {
                            s.hiss[i] = rnd(&mut s.seed);
                        }
                        y += (s.hiss[i] - y) * o.nx;
                    }
                    y
                };
                for j in 0..6 {
                    let a = p.rm[i][j];
                    if a != 0.0 {
                        // ringing with itself goes by its own output one frame back (measured)
                        y *= 1.0 - a.abs() + a * if j > i { s.out[j] } else { last[j] };
                    }
                }
                s.amp[i] += (s.val[b + 1] * o.vol - s.amp[i]) * 0.02;
                s.prev[i] = last[i];
                s.out[i] = y * s.amp[i] * if o.declick { fade } else { 1.0 };
                s.ph[i] += s.inc[i];
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
            let mut carry = 0.0;
            for k in 0..3 {
                if !p.fused[k] {
                    carry = 0.0;
                    continue;
                }
                let f = &p.filts[k];
                let b = 36 + k * 8;
                let mut x = carry;
                for j in 0..6 {
                    let a = p.feed[k][j];
                    if a != 0.0 {
                        x += a * s.out[j];
                    }
                }
                if f.ws_on {
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
                    x += (shaped - x) * (f.ws_mix * s.val[b + 7]).clamp(0.0, 1.0);
                }
                if f.kind == 12 {
                    // phaser: a row of all-pass stages whose output comes back to its own input, solved
                    // without a sample of delay (the row is a straight line: gain times input plus memory)
                    let g = s.fg[k];
                    let a = (g - 1.0) / (g + 1.0);
                    let n = (f.slope.clamp(1, 5) as usize) * 2;
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
                    x = u;
                } else if f.kind == 11 && s.plan[k][0][4] > 0.0 {
                    // the driven ladder: every stage bends what comes in and what it holds, and the resonance
                    // goes back in under the first bend
                    let bend = |v: f32| v / (1.0 + v * v).sqrt();
                    let lean = |v: f32| {
                        let q = 1.0 / (1.0 + v * v);
                        q * q.sqrt()
                    };
                    let (push, pull) = (s.plan[k][0][4], s.plan[k][0][5]);
                    let mut u_in = x * push;
                    let mut first = true;
                    for st in 0..s.stages[k] as usize {
                        let [big, back, make_up, ..] = s.plan[k][st];
                        let g = big / (1.0 - big).max(1e-6);
                        let (lo, hi) = s.fs[k].split_at_mut(st * 2 + 1);
                        let z = [&mut lo[st * 2], &mut hi[0]];
                        let held = [z[0][0], z[0][1], z[1][0], z[1][1]];
                        // the four stages for one guess of what comes back; gives the last stage and how
                        // steeply it follows the guess
                        let chain = |guess: f32, keep: &mut [f32; 4]| -> (f32, f32) {
                            let arg = if first { u_in - back * guess } else { u_in };
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
                        if first && back > 0.0 {
                            for _ in 0..2 {
                                let (got, slope) = chain(guess, &mut keep);
                                guess -= (got - guess) / (slope - 1.0);
                            }
                        }
                        let (got, _) = chain(guess, &mut keep);
                        for q in 0..4 {
                            z[q / 2][q % 2] = keep[q];
                        }
                        if first {
                            x = got * pull * make_up;
                        } else {
                            x = got * pull * s.plan[k][0][2];
                        }
                        u_in = got;
                        first = false;
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
                if !x.is_finite() {
                    x = 0.0;
                    s.fs[k] = [[0.0; 2]; 6];
                    s.ap[k] = [0.0; 10];
                    s.fb[k] = 0.0;
                }
                // a ringing stage after a hot shaper is kept inside sane bounds
                x = (x * s.val[b + 1]).clamp(-8.0, 8.0);
                carry = x * f.next;
                let g = p.out[6 + k];
                if g != 0.0 {
                    l += x * g * s.pl[6 + k];
                    r += x * g * s.pr[6 + k];
                }
                let g = p.send[6 + k];
                if g != 0.0 {
                    sl += x * g * s.pl[6 + k];
                    sr += x * g * s.pr[6 + k];
                }
            }
            mix[0] += l * s.gain * (1.0 - s.pan).min(1.0);
            mix[1] += r * s.gain * (1.0 + s.pan).min(1.0);
            smix[0] += sl * s.gain * (1.0 - s.pan).min(1.0);
            smix[1] += sr * s.gain * (1.0 + s.pan).min(1.0);
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
        let sr = 48000.0;
        let hz = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0);
        let mut v = Voice::start(p, sr, key, [0.8, 0.5, 0.5], hz / sr, 99);
        (0..total)
            .map(|k| {
                if k == hold {
                    v.release();
                }
                v.frame(p, hz / sr)
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
                let o = run(&p, 48, 24000, 30000);
                let peak = o.iter().fold(0.0f32, |a, f| a.max(f[0].abs()).max(f[1].abs()));
                assert!(peak.is_finite() && peak < 24.0, "{} peaks at {peak}", path.display());
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
