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
/// Level of one full-scale operator sent straight out.
const LEVEL: f32 = 0.4;

pub const OP_TARGETS: [&str; 6] = ["Pan", "Volume", "Modulation", "Pitch", "Phase", "Damping"];
pub const FILTER_TARGETS: [&str; 8] = ["Pan", "Volume", "Cutoff", "Resonance", "Low", "Band", "High", "Shaper mix"];
pub const MODULES: [&str; 9] = ["Op 1", "Op 2", "Op 3", "Op 4", "Op 5", "Op 6", "Filter 1", "Filter 2", "Filter 3"];

fn sine() -> &'static [f32] {
    static T: OnceLock<Vec<f32>> = OnceLock::new();
    T.get_or_init(|| (0..=TN).map(|i| (i as f32 / TN as f32 * TAU).sin()).collect())
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
    /// Bit 0: switched on (envelopes and slow oscillators only; mappings always apply).
    pub flags: i32,
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
        let mut c = Self { pts: vec![Pt { x: 0.0, y, ..Default::default() }, Pt { x: 1.0, y, ..Default::default() }], flags: 0, decay: -1, loop_start: -1, sustain: -1, span: (0, 0), t: vec![] };
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
        let n = at(raw, a + 8);
        if n < 0 || a + 12 + 24 * n as usize + 20 != b {
            return false;
        }
        let keep = at(raw, b - 16);
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
        for v in [self.flags, keep, self.decay, self.loop_start, self.sustain] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        raw.splice(a..b, out);
        true
    }

    pub fn on(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Lay the points out in time. `sc` stretches the parts before the decay marker, up to the sustain marker and
    /// after it.
    pub fn prepare(&mut self, sc: [f32; 3]) {
        let mut acc = 0.0;
        self.t.clear();
        for (i, p) in self.pts.iter().enumerate() {
            let i = i as i32;
            let s = if self.sustain >= 0 && i > self.sustain {
                sc[2]
            } else if self.decay >= 0 && i > self.decay {
                sc[1]
            } else {
                sc[0]
            };
            if i > 0 {
                acc += p.x.max(0.0) * s;
            }
            self.t.push(acc);
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
                return if b.mode == 2 { a.y } else { a.y + (b.y - a.y) * bend(u, b.tension) };
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

    fn value(&self, stat: f32, t: f32, rel: Option<f32>, lfo_gain: f32) -> f32 {
        let l = if self.lfo.on() { (TAU * self.lfo_hz * t).sin() * (self.lfo.env(t, rel) - 0.5) * 2.0 * lfo_gain } else { 0.0 };
        if self.bipolar {
            stat + if self.env.on() { (self.env.env(t, rel) - 0.5) * 2.0 } else { 0.0 } + l
        } else {
            (stat * if self.env.on() { self.env.env(t, rel) } else { 1.0 } * (1.0 + l)).max(0.0)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Op {
    /// Frequency against the note (2 in the file is the note itself).
    pub ratio: f32,
    pub offset_hz: f32,
    pub phase: f32,
    pub vol: f32,
    pub pitch_cents: f32,
    pub noise: f32,
    pub pluck: bool,
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
    pub lift: f32,
    pub drive: f32,
    ws: Vec<f32>,
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
    used: [bool; 6],
    fused: [bool; 3],
    any_moving: bool,
}

pub const ARTS: usize = 62;

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
        self.i()?;
        let n = self.i()?;
        if !(0..=8192).contains(&n) {
            return None;
        }
        let mut pts = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let b = self.bytes(24)?;
            let x = f64::from_le_bytes(b[0..8].try_into().ok()?);
            let y = f64::from_le_bytes(b[8..16].try_into().ok()?);
            let tension = f32::from_le_bytes(b[16..20].try_into().ok()?);
            let fin = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
            pts.push(Pt { x: fin(x).clamp(0.0, 1e5), y: fin(y).clamp(0.0, 1.0), tension: if tension.is_finite() { tension } else { 0.0 }, mode: b[20] });
        }
        let flags = self.i()?;
        self.i()?;
        let decay = self.i()?;
        let loop_start = self.i()?;
        let sustain = self.i()?;
        Some(Curve { pts, flags, decay, loop_start, sustain, span: (from, self.p), t: vec![] })
    }
    /// One setting's envelope, slow oscillator and six mappings. `g` holds the instrument-wide length knobs.
    fn art(&mut self, bipolar: bool, g: [f32; 3]) -> Option<Art> {
        self.bytes(16)?;
        let mut env = self.curve(1)?;
        let k = [self.i()?, self.i()?, self.i()?, self.i()?];
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

/// A length knob: 128 leaves the drawn length alone.
fn knob(v: i32) -> f32 {
    let x = v.clamp(0, 256) as f32 / 128.0;
    (x * x).max(0.003)
}

fn at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

fn shape(sh: f32, tn: f32, sk: f32, sn: f32, ph: f32) -> f32 {
    let m = (0.5 - sk * 0.45).clamp(0.05, 0.95);
    let p = if ph < m { ph * 0.5 / m } else { 0.5 + (ph - m) * 0.5 / (1.0 - m) };
    let s = (TAU * p).sin();
    let tri = {
        let x = p + 0.25;
        let x = x - x.floor();
        1.0 - 4.0 * (x - 0.5).abs()
    };
    let sq = if p < 0.5 { 1.0 } else { -1.0 };
    let mut y = if sh <= 1.0 { s + (tri - s) * sh } else { tri + (sq - tri) * (sh - 1.0).min(1.0) };
    if tn != 0.0 {
        y = y.signum() * y.abs().powf((-tn * 2.0).exp2());
    }
    if sn > 0.0 {
        y += ((y * PI * (0.5 + sn * 1.5)).sin() - y) * sn;
    }
    y
}

fn op_from(h: &[u8]) -> Op {
    let i = |o: usize| at(h, o);
    let f = |o: usize| f32::from_le_bytes(h[o..o + 4].try_into().unwrap());
    let (sh, tn, sk, sn) = (i(2100).clamp(0, 512) as f32 / 256.0, i(2104).clamp(-128, 128) as f32 / 128.0, i(2108).clamp(0, 128) as f32 / 128.0, i(2112).clamp(0, 128) as f32 / 128.0);
    let soft = 1.0 - i(2116).clamp(0, 128) as f32 / 128.0;
    let (half, absolute, center) = (h[2120] != 0, h[2122] != 0, h[2123] != 0);
    let mut base: Vec<f32> = (0..TN).map(|k| shape(sh, tn, sk, sn, k as f32 / TN as f32)).collect();
    if soft > 0.0 {
        let c = 1.0 - soft * 0.98;
        let mut z = 0.0;
        for _ in 0..2 {
            for v in base.iter_mut() {
                z += (*v - z) * c;
                *v = z;
            }
        }
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
        if half {
            t[TN / 2..].iter_mut().for_each(|v| *v = 0.0);
        }
        if absolute {
            t.iter_mut().for_each(|v| *v = v.abs());
        }
        if center {
            let mean = t.iter().sum::<f32>() / TN as f32;
            t.iter_mut().for_each(|v| *v -= mean);
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
        ratio: i(40).clamp(0, 640000) as f32 / 20000.0,
        offset_hz: i(44) as f32 / 100.0,
        phase: i(0).clamp(0, 256) as f32 / 256.0,
        vol: i(4).clamp(-256, 512) as f32 / 256.0,
        pitch_cents: i(8).clamp(0, 9600) as f32,
        noise: i(48).clamp(0, 128) as f32 / 128.0,
        pluck: h[2126] != 0,
        tables,
        caps,
    }
}

impl Default for Patch {
    /// One plain operator sent straight out.
    fn default() -> Self {
        let mut ops: [Op; 6] = Default::default();
        for o in ops.iter_mut() {
            *o = Op { ratio: 1.0, vol: 1.0, pitch_cents: 1200.0, tables: vec![Arc::new(sine().to_vec())], caps: vec![1], ..Default::default() };
        }
        let mut arts = Vec::with_capacity(ARTS);
        for k in 0..ARTS {
            arts.push(Art::neutral(bipolar_at(k)));
        }
        let mut out = [0.0; 9];
        out[0] = 1.0;
        let mut p = Self { volume: 1.0, lfo: 1.0, pitch: 0.0, fm: [[0.0; 6]; 6], rm: [[0.0; 6]; 6], feed: [[0.0; 6]; 3], pan: [0.0; 9], out, ops, filts: Default::default(), arts, uni: 1, uni_pan: 1.0, uni_vol: 0.8, uni_pitch: 0.5, uni_phase: 0.1, modx: 0.5, mody: 0.75, op_at: [0; 6], filt_at: [0; 3], used: [false; 6], fused: [false; 3], any_moving: false };
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
        if !(17..=25).contains(&ver) {
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
        p.volume = at(d, 0).clamp(0, 256) as f32 / 128.0;
        p.lfo = at(d, 4).clamp(-128, 128) as f32 / 128.0;
        p.pitch = at(d, 8).clamp(-48, 48) as f32;
        let cell = |base: usize, r: usize, c: usize| at(d, base + r * 36 + c * 4).clamp(-256, 256) as f32 / 256.0;
        for r in 0..9 {
            p.pan[r] = cell(12, r, 6);
            // nothing of ours sits behind the effects send yet, so it joins the plain output
            let (send, out) = (cell(12, r, 7), cell(12, r, 8));
            p.out[r] = out + send.abs() * if out < 0.0 { -1.0 } else { 1.0 };
            for c in 0..6 {
                if r < 6 {
                    p.fm[r][c] = cell(12, r, c);
                    p.rm[r][c] = cell(336, r, c);
                } else {
                    p.feed[r - 6][c] = cell(12, r, c);
                }
            }
        }
        let gv = [knob(at(d, 660)), knob(at(d, 664)), knob(at(d, 672))];
        let gf = [knob(at(d, 676)), knob(at(d, 680)), knob(at(d, 688))];
        p.uni = at(d, 692).clamp(1, 9) as u32;
        p.uni_pan = at(d, 696).clamp(0, 128) as f32 / 128.0;
        p.uni_vol = at(d, 700).clamp(0, 128) as f32 / 128.0;
        p.uni_pitch = at(d, 704).clamp(0, 256) as f32 / 256.0;
        p.uni_phase = at(d, 712).clamp(0, 128) as f32 / 128.0;
        p.modx = at(d, 728).clamp(0, 1024) as f32 / 1024.0;
        p.mody = at(d, 732).clamp(0, 1024) as f32 / 1024.0;

        // the patch notes: where they start moved between layouts. Newer blocks hold a one-byte length and two
        // bytes per letter, older ones a four-byte length and one byte per letter
        let at_text = match ver {
            17 => 1133,
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
        for lfo_rest in [16, 12] {
            let mut q = p.clone();
            if q.modules(R { d, p: start, lfo_rest }, gv, gf).is_some() {
                q.wire();
                return Some(q);
            }
        }
        None
    }

    fn modules(&mut self, mut r: R, gv: [f32; 3], gf: [f32; 3]) -> Option<()> {
        let p = self;
        let one = [1.0; 3];
        p.arts.clear();
        for k in 0..6 {
            p.op_at[k] = r.p;
            p.ops[k] = op_from(r.bytes(2128)?);
            for a in 0..6 {
                let g = if a == 1 { gv } else { one };
                p.arts.push(r.art(bipolar_at(a), g)?);
            }
        }
        for k in 0..3 {
            p.filt_at[k] = r.p;
            let h = r.bytes(88)?;
            let i = |o: usize| at(h, o);
            let mut ws = r.curve(3)?;
            ws.prepare(one);
            let bip = *r.bytes(1)?.first()? != 0;
            let f = &mut p.filts[k];
            *f = Filt {
                slope: i(0).clamp(0, 5),
                env_amt: i(4).clamp(0, 256) as f32 / 256.0,
                cut: i(8).clamp(0, 256) as f32 / 256.0,
                res: i(12).clamp(0, 256) as f32 / 256.0,
                levels: [i(16).clamp(0, 128) as f32 / 128.0, i(20).clamp(0, 128) as f32 / 128.0, i(24).clamp(0, 128) as f32 / 128.0],
                ws_gain: ((i(28).clamp(0, 256) - 128) as f32 / 32.0).exp2(),
                ws_mix: i(32).clamp(0, 128) as f32 / 128.0,
                next: i(36).clamp(-256, 256) as f32 / 256.0,
                ws_on: i(80) != 0,
                kind: i(84).clamp(0, 12),
                lift: 10f32.powf((i(16).clamp(0, 128) - 64) as f32 / 64.0 * 18.0 / 20.0) - 1.0,
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
            f[k] = self.out[6 + k] != 0.0 || (k < 2 && self.filts[k].next != 0.0 && f[k + 1]);
        }
        let mut u = [false; 6];
        for k in 0..6 {
            u[k] = self.out[k] != 0.0 || (0..3).any(|j| f[j] && self.feed[j][k] != 0.0);
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
        (0..6).filter(|&k| self.used[k] && (self.out[k] != 0.0 || (0..3).any(|j| self.fused[j] && self.feed[j][k] != 0.0)))
    }

    /// Seconds the sound needs after the key is let go `t` seconds into the note.
    pub fn tail_s(&self, t: f32) -> f32 {
        let mut tail = 0.0f32;
        let mut vols: Vec<usize> = self.carriers().map(|k| k * 6 + 1).collect();
        vols.extend((0..3).filter(|&k| self.fused[k]).map(|k| 36 + k * 8 + 1));
        vols.push(61);
        for k in vols {
            let e = &self.arts[k].env;
            if e.on() {
                tail = tail.max(e.tail(t));
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
    fs: [[[f32; 2]; 3]; 3],
    ap: [[f32; 10]; 3],
    fb: [f32; 3],
    fg: [f32; 3],
    fk: [f32; 3],
    seed: u32,
}

/// One sounding note.
#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    t: u32,
    rel: Option<f32>,
    ctl: u32,
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
    pub fn start(p: &Patch, sr: f32, key: u8, vel: f32, dphase: f32, seed: u32) -> Self {
        let mut seed = seed | 1;
        let n = p.uni.clamp(1, 9) as usize;
        let rand = rnd(&mut seed) * 0.5 + 0.5;
        let mut subs = Vec::with_capacity(n);
        for s in 0..n {
            let u = if n == 1 { 0.0 } else { s as f32 / (n - 1) as f32 * 2.0 - 1.0 };
            let xs = [key as f32 / 131.0, vel.clamp(0.0, 1.0), p.modx, p.mody, rand, (u + 1.0) * 0.5];
            let mut stat = [0.0; ARTS];
            for (k, a) in p.arts.iter().enumerate().take(ARTS) {
                stat[k] = a.stat(&xs);
            }
            let mut ph = [0.0; 6];
            let mut tab = [0u8; 6];
            for k in 0..6 {
                let o = &p.ops[k];
                ph[k] = o.phase + if n > 1 { (rnd(&mut seed) * 0.5 + 0.5) * p.uni_phase } else { 0.0 };
                // the fullest table whose top harmonic still fits under half the sample rate
                let f = (dphase * o.ratio + o.offset_hz / sr).abs().max(1e-6);
                let fit = (0.45 / f) as u32;
                tab[k] = o.caps.iter().position(|&c| c <= fit.max(1)).unwrap_or(o.caps.len() - 1) as u8;
            }
            subs.push(Sub {
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
                fs: [[[0.0; 2]; 3]; 3],
                ap: [[0.0; 10]; 3],
                fb: [0.0; 3],
                fg: [0.1; 3],
                fk: [1.0; 3],
                seed: seed.wrapping_add(s as u32 * 7919) | 1,
            });
        }
        let mut v = Self { sr, t: 0, rel: None, ctl: 0, subs, main_l: 1.0, main_r: 1.0 };
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
        for s in self.subs.iter_mut() {
            if first || p.any_moving {
                for (k, a) in p.arts.iter().enumerate().take(ARTS) {
                    if first || a.moving() {
                        s.val[k] = a.value(s.stat[k], t, rel, p.lfo);
                    }
                }
            }
            for k in 0..6 {
                let o = &p.ops[k];
                let bend = (p.pitch / 12.0 + s.val[k * 6 + 3] * o.pitch_cents / 1200.0).exp2();
                s.inc[k] = (dphase * s.detune * bend * o.ratio + o.offset_hz / sr).clamp(-0.5, 0.5);
                let pan = (p.pan[k] + s.val[k * 6]).clamp(-1.0, 1.0);
                s.pl[k] = (1.0 - pan).min(1.0);
                s.pr[k] = (1.0 + pan).min(1.0);
            }
            for k in 0..3 {
                let f = &p.filts[k];
                let b = 36 + k * 8;
                let pan = (p.pan[6 + k] + s.val[b]).clamp(-1.0, 1.0);
                s.pl[6 + k] = (1.0 - pan).min(1.0);
                s.pr[6 + k] = (1.0 + pan).min(1.0);
                let c = (f.cut + s.val[b + 2] * f.env_amt).clamp(0.0, 1.0);
                let hz = (20.0 * (c * 10.0).exp2()).min(sr * 0.45);
                s.fg[k] = (PI * hz / sr).tan();
                let r = (f.res + s.val[b + 3] * 0.5).clamp(0.0, 1.0);
                s.fk[k] = 2.0 - 1.9 * r;
            }
        }
        if let Some(s) = self.subs.first() {
            let pan = s.val[60].clamp(-1.0, 1.0);
            let g = s.val[61] * p.volume * LEVEL;
            self.main_l = (1.0 - pan).min(1.0) * g;
            self.main_r = (1.0 + pan).min(1.0) * g;
        }
    }

    pub fn frame(&mut self, p: &Patch, dphase: f32) -> [f32; 2] {
        if self.ctl == 0 {
            self.control(p, dphase, false);
            self.ctl = TICK;
        }
        self.ctl -= 1;
        self.t += 1;
        let sin = sine();
        let mut mix = [0.0f32; 2];
        for s in self.subs.iter_mut() {
            let (mut l, mut r) = (0.0f32, 0.0f32);
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
                    // a plucked string: the full spectrum settles towards a plain tone; cannot be phase modulated
                    let d = s.val[b + 5].clamp(0.0, 1.0);
                    look(&o.tables[s.tab[i] as usize], ph) * d + look(sin, ph) * (1.0 - d)
                } else {
                    look(&o.tables[s.tab[i] as usize], ph + pm * s.val[b + 2] * DEPTH)
                };
                if o.noise > 0.0 {
                    y += (rnd(&mut s.seed) - y) * o.noise;
                }
                for j in 0..6 {
                    let a = p.rm[i][j];
                    if a != 0.0 {
                        // ringing with itself has nothing earlier to go by: it uses the value just made
                        y *= 1.0 - a.abs() + a * if j > i { s.out[j] } else if j == i { y } else { last[j] };
                    }
                }
                s.amp[i] += (s.val[b + 1] * o.vol - s.amp[i]) * 0.02;
                s.prev[i] = last[i];
                s.out[i] = y * s.amp[i];
                s.ph[i] += s.inc[i];
                s.ph[i] -= s.ph[i].floor();
                let g = p.out[i];
                if g != 0.0 {
                    l += s.out[i] * g * s.pl[i];
                    r += s.out[i] * g * s.pr[i];
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
                let stages = match f.slope {
                    0 => 0,
                    4 => 2,
                    5 => 3,
                    n => n as usize,
                };
                if f.kind == 12 {
                    // phaser: a row of all-pass stages around a feedback path, mixed with the dry signal
                    let g = s.fg[k];
                    let a = (g - 1.0) / (g + 1.0);
                    let n = (f.slope.clamp(1, 5) as usize) * 2;
                    let dry = x;
                    let mut y = x + s.fb[k] * (2.0 - s.fk[k]) / 1.95 * 0.9;
                    for z in s.ap[k].iter_mut().take(n) {
                        let o = a * y + *z;
                        *z = y - a * o;
                        y = o;
                    }
                    s.fb[k] = y.clamp(-4.0, 4.0);
                    x = (dry + y) * 0.5;
                } else {
                    if f.kind == 11 && f.drive > 0.0 {
                        x = (x * (1.0 + f.drive * 3.0)).tanh();
                    }
                    let g = s.fg[k];
                    for st in 0..stages {
                        // only the first stage rings; the others stay flat so stacked stages do not multiply the peak
                        let kk = if st == 0 { s.fk[k] } else { std::f32::consts::SQRT_2 };
                        let a1 = 1.0 / (1.0 + g * (g + kk));
                        let a2 = g * a1;
                        let a3 = g * a2;
                        let z = &mut s.fs[k][st];
                        let v3 = x - z[1];
                        let v1 = a1 * z[0] + a2 * v3;
                        let v2 = z[1] + a2 * z[0] + a3 * v3;
                        *z = [2.0 * v1 - z[0], 2.0 * v2 - z[1]];
                        let hi = x - kk * v1 - v2;
                        x = match f.kind {
                            0 => v2 * f.levels[0] * s.val[b + 4] + v1 * f.levels[1] * s.val[b + 5] + hi * f.levels[2] * s.val[b + 6],
                            4 => v1 * kk,
                            5 => v2 + hi,
                            6 => hi,
                            7 => x - 2.0 * kk * v1,
                            8 => x + f.lift * v2,
                            9 => x + f.lift * kk * v1,
                            10 => x + f.lift * hi,
                            _ => v2,
                        };
                    }
                }
                if !x.is_finite() {
                    x = 0.0;
                    s.fs[k] = [[0.0; 2]; 3];
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
            }
            mix[0] += l * s.gain * (1.0 - s.pan).min(1.0);
            mix[1] += r * s.gain * (1.0 + s.pan).min(1.0);
        }
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
        curve(1, &[(0.0, 0.0), (0.01, 1.0), (0.05, 0.5), (0.1, 0.0)], [env_on as i32, 0, 1, -1, 2], out);
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
        assert_eq!(p.fm[0][1], 0.25);
        assert_eq!(p.feed[0][0], 1.0);
        assert_eq!(p.ops[1].ratio, 2.0);
        assert_eq!(p.ops[0].ratio, 1.0);
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
        let mut v = Voice::start(p, sr, key, 0.8, hz / sr, 99);
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
        assert!((p.tail_s(0.5) - 0.1).abs() < 1e-4);

        // with the modulator taken away the held part is a plain tone at 220 Hz: next to nothing at 440 Hz
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
        assert!(part(&q, 220.0) > 0.05 && part(&q, 440.0) < part(&q, 220.0) * 0.01);
        let mut direct = p.clone();
        direct.feed[0][0] = 0.0;
        direct.wire();
        let m = run(&direct, 57, 24000, 24000);
        assert!(part(&m, 660.0) > part(&m, 220.0) * 0.05, "the modulator adds a partial at three times the note");
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
        assert_eq!(p.ops[0].ratio, 1.0);
        assert_eq!(p.used(), [true, false, false, false, false, false]);
        // operator 2 into operator 1 at 25 %, and a point more in operator 1's volume envelope
        let mut env = p.arts[1].env.clone();
        env.pts.insert(1, Pt { x: 0.005, y: 0.5, ..Default::default() });
        let (state2, q) = Patch::edited(&state, |d| {
            d[12 + 4..12 + 8].copy_from_slice(&64i32.to_le_bytes());
            assert!(env.store(d));
        })
        .unwrap();
        assert_eq!(q.fm[0][1], 0.25);
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
        let mut c = Curve { pts: vec![Pt { x: 0.0, y: 0.0, ..Default::default() }, Pt { x: 1.0, y: 1.0, ..Default::default() }, Pt { x: 1.0, y: 0.5, ..Default::default() }, Pt { x: 2.0, y: 0.0, ..Default::default() }], flags: 1, decay: 1, loop_start: -1, sustain: 2, span: (0, 0), t: vec![] };
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
