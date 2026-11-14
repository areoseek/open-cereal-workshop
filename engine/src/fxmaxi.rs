//! The band maximiser: a low cut, a three-way split, a look-ahead level stage with a drawn in/out line and a
//! shaper on each band, a mix against the untouched bands, and one more such stage on the sum. Optionally the
//! whole thing runs at twice the rate between a half-band pair.
//!
//! Every law here was fitted to renders of test signals; see docs/PARITY.md.

use crate::fx::Ctx;
use std::collections::VecDeque;
use std::f64::consts::PI;

/// Settings ahead of the tables.
pub const N: usize = 59;
/// Entries in each band's line table.
pub const TABLE: usize = 257;
/// Everything a unit carries: settings, four tables, their four widths, the two switches.
pub const LEN: usize = N + 4 * TABLE + 6;
pub const SPANS: usize = N + 4 * TABLE;
pub const OVERSAMPLE: usize = SPANS + 4;
pub const LINEAR: usize = SPANS + 5;
/// One more place a caller may add behind everything: set, the low band is not turned behind a steep high split
/// (the one-knob unit built on this one runs that way).
pub const UNTURNED: usize = SPANS + 6;

/// What each stage that works on levels loses.
const STAGE_GAIN: f64 = 0.9749;
/// The half-band pair: two chains of first-order all-pass sections.
const HALF_A: [f64; 3] = [0.03584, 0.29496, 0.67399];
const HALF_B: [f64; 3] = [0.15569, 0.48577, 0.88301];
/// How late the linear-phase split is, in seconds.
const LINEAR_LATE: f64 = 0.002;

/// A time setting in milliseconds.
pub fn time_ms(v: f32) -> f64 {
    2.0 * (501f64.powf((v as f64 / 10000.0).clamp(0.0, 1.0)) - 1.0)
}
/// A gain or ceiling setting as a factor.
pub fn level(v: f32) -> f64 {
    (7f64.powf(v as f64 / 1000.0) - 1.0) / 6.0
}
pub fn split_hz(v: f32) -> f64 {
    30.0 * (1600f64 / 3.0).powf((v as f64 / 65536.0).clamp(0.0, 1.0))
}
/// The low cut's corner; None when it is off.
pub fn low_cut_hz(v: f32) -> Option<f64> {
    (v >= 0.0).then(|| 4.0 * 2f64.powf(v as f64 / 16384.0))
}
/// How much of the side signal a separation setting leaves: none at +1000, double at -1000.
pub fn side_gain(v: f32) -> f64 {
    let s = (v as f64 / 1000.0).clamp(-1.0, 1.0);
    if s <= 0.0 {
        1.0 - s
    } else {
        let t = 1.0 - s;
        1.0 - s * ((t * t + 1.0).sqrt() - t)
    }
}

fn frames(ms: f64, rate: f64) -> f64 {
    ms * 0.001 * rate
}

/// How many frames late the unit's sound comes out.
pub fn latency(p: &[f32], sr: f32) -> usize {
    if p.len() < LEN {
        return 0;
    }
    let sr = sr as f64;
    let mode = |b: usize| p[7 + 13 * b] as i32;
    let mut late = 0;
    let split = mode(0) != 3 || mode(2) != 3;
    if p[LINEAR] >= 0.5 && split {
        late += (LINEAR_LATE * sr) as usize;
    }
    if (0..3).any(|b| mode(b) == 0) {
        late += frames(time_ms(p[4]), sr).round() as usize;
    }
    if mode(3) == 0 {
        late += frames(time_ms(p[7 + 39 + 6]), sr).round() as usize;
    }
    late
}

#[derive(Clone, Copy, Default)]
struct Bq {
    b: [f64; 3],
    a: [f64; 2],
}

impl Bq {
    #[inline]
    fn run(&self, z: &mut [f64; 2], x: f64) -> f64 {
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// One half of a split: the low and high sections, each run twice in a row.
fn halves(f: f64, steep: bool, rate: f64) -> (Bq, Bq) {
    let k = (PI * f.min(rate * 0.49) / rate).tan();
    if steep {
        let n = 1.0 / (1.0 + std::f64::consts::SQRT_2 * k + k * k);
        let a = [2.0 * (k * k - 1.0) * n, (1.0 - std::f64::consts::SQRT_2 * k + k * k) * n];
        (Bq { b: [k * k * n, 2.0 * k * k * n, k * k * n], a }, Bq { b: [n, -2.0 * n, n], a })
    } else {
        let n = 1.0 / (1.0 + k);
        let a = [(k - 1.0) * n, 0.0];
        (Bq { b: [k * n, k * n, 0.0], a }, Bq { b: [n, -n, 0.0], a })
    }
}

/// A two-section filter's memory for both sides.
type Mem = [[[f64; 2]; 2]; 2];

#[inline]
fn twice(q: &Bq, m: &mut Mem, ch: usize, x: f64) -> f64 {
    let y = q.run(&mut m[ch][0], x);
    q.run(&mut m[ch][1], y)
}

#[derive(Clone, Copy, Default)]
struct Pass {
    x1: f64,
    y1: f64,
}

impl Pass {
    #[inline]
    fn run(&mut self, a: f64, x: f64) -> f64 {
        let y = a * x + self.x1 - a * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

#[inline]
fn chain(set: &[f64; 3], m: &mut [Pass; 3], mut x: f64) -> f64 {
    for (a, z) in set.iter().zip(m.iter_mut()) {
        x = z.run(*a, x);
    }
    x
}

/// What one stage is set to, worked out once per block.
#[derive(Clone, Copy, Default)]
struct Set {
    mode: i32,
    pre: f64,
    post: f64,
    side: f64,
    /// Shaper: 0 none, 1 the rounded knee, 2 the soft curve.
    shape: u8,
    ceil: f64,
    knee: f64,
    power: f64,
    norm: f64,
    lag: usize,
    window: usize,
    average: usize,
    n: usize,
    m: usize,
    ka: f64,
    k2: f64,
    kr: f64,
    table: usize,
    span: f64,
}

impl Set {
    /// `look` is the look-ahead in milliseconds; `lag` how many frames a band is held back whatever its mode.
    fn new(p: &[f32], b: usize, rate: f64, look: Option<f64>, lag: usize) -> Self {
        let q = &p[7 + 13 * b..7 + 13 * b + 13];
        let mode = q[0] as i32;
        let t_att = frames(time_ms(q[6]), rate);
        let look = frames(look.unwrap_or_else(|| time_ms(q[6])), rate);
        let t_att = t_att.max(look);
        let t_r2 = frames(time_ms(q[7]), rate);
        let t_rel = frames(time_ms(q[9]), rate);
        let hold = frames(time_ms(q[11]), rate);
        let n = (q[8] as usize).clamp(1, 8);
        let m = (q[10] as usize).min(4);
        let rn = (n as f64).sqrt();
        let s = (q[4] as f64 / 1000.0).clamp(-1.0, 1.0);
        let ceil = level(q[5]);
        let mut set = Set {
            mode,
            pre: level(q[1]),
            post: level(q[2]),
            side: side_gain(q[3]),
            shape: 0,
            ceil,
            knee: 0.0,
            power: 1.0,
            norm: 1.0,
            lag,
            window: hold.max(look).round() as usize,
            // the mean runs over the same stretch the peak would be held for
            average: if q[12] >= 0.5 { hold.max(look).round() as usize } else { 0 },
            n,
            m,
            ka: if t_att < 0.5 { 1.0 } else { 1.0 - (-rn * 5.756 / t_att).exp() },
            k2: if t_r2 < 0.5 { 1.0 } else { 1.0 - (-rn * std::f64::consts::LN_10 / t_r2).exp() },
            kr: if t_rel < 0.5 || m == 0 { 1.0 } else { 1.0 - (-(m as f64).sqrt() * std::f64::consts::LN_10 / t_rel).exp() },
            table: N + b * TABLE,
            span: (p[SPANS + b] as f64).max(1e-6),
        };
        if s > 0.0 {
            set.shape = 1;
            set.knee = ceil * (1.0 - s);
        } else if s < 0.0 {
            set.shape = 2;
            set.power = 1.0 - 4.0614 * (-s).ln();
            // scaled so the loudest thing that can come in lands on the ceiling: the most the drawn line asks
            // for behind the level stage, four times full level without it, both times the gain after
            let far = p[set.table..set.table + TABLE].iter().fold(0f32, |a, &b| a.max(b)) as f64;
            let top = (if mode == 0 { (9f64.powf(far) - 1.0) * 0.5 } else { 4.0 } * set.post).max(ceil);
            set.norm = if ceil > 1e-9 { soft(top / ceil, set.power) } else { 1.0 };
        }
        set
    }

    #[inline]
    fn shape(&self, x: f64) -> f64 {
        match self.shape {
            1 => {
                let a = x.abs();
                if a <= self.knee {
                    x
                } else {
                    let r = self.ceil - self.knee;
                    if r <= 1e-12 {
                        self.ceil.copysign(x)
                    } else {
                        (self.knee + r * (1.0 - (-(a - self.knee) / r).exp())).copysign(x)
                    }
                }
            }
            2 => {
                if self.ceil <= 1e-9 {
                    0.0
                } else {
                    (self.ceil * soft(x.abs() / self.ceil, self.power) / self.norm).copysign(x)
                }
            }
            _ => x,
        }
    }

    /// The gain the drawn line asks for at a level. The line's axes are both bent: half way is full level,
    /// the far end four times that.
    #[inline]
    fn want(&self, p: &[f32], lv: f64) -> f64 {
        let lv = lv.max(1e-5);
        let v = (2.0 * lv + 1.0).ln() / 9f64.ln();
        let at = (v / self.span).min(1.0) * (TABLE - 1) as f64;
        let i = (at as usize).min(TABLE - 2);
        let f = at - i as f64;
        let y = p[self.table + i] as f64 * (1.0 - f) + p[self.table + i + 1] as f64 * f;
        (9f64.powf(y) - 1.0) * 0.5 / lv
    }
}

/// The soft curve before it is scaled: straight at first, flat in the end; the power sets how sudden.
#[inline]
fn soft(u: f64, power: f64) -> f64 {
    let z = u.powf(power);
    if z < 1e-4 {
        u
    } else {
        z.atan().powf(1.0 / power)
    }
}

#[derive(Default)]
struct Stage {
    ring: Vec<[f32; 2]>,
    pos: usize,
    peaks: VecDeque<(f64, u64)>,
    squares: Vec<f64>,
    sq_pos: usize,
    sq_sum: f64,
    sq_len: usize,
    zr: [f64; 4],
    zg: [f64; 8],
    t: u64,
}

impl Stage {
    fn fit(&mut self, s: &Set) {
        let need = (s.lag + 1).next_power_of_two();
        if self.ring.len() < need {
            self.ring = vec![[0.0; 2]; need];
            self.pos = 0;
        }
        if s.average != self.sq_len {
            self.sq_len = s.average;
            self.squares = vec![0.0; s.average];
            self.sq_pos = 0;
            self.sq_sum = 0.0;
        }
    }

    /// One frame in; the held-back untouched frame and the worked one out.
    #[inline]
    fn run(&mut self, s: &Set, p: &[f32], x: [f64; 2]) -> ([f64; 2], [f64; 2]) {
        let mask = self.ring.len() - 1;
        self.ring[self.pos] = [x[0] as f32, x[1] as f32];
        let d = self.ring[(self.pos + self.ring.len() - s.lag) & mask];
        self.pos = (self.pos + 1) & mask;
        let dry = [d[0] as f64, d[1] as f64];
        let mut y = match s.mode {
            0 => {
                let now = x[0].abs().max(x[1].abs()) * s.pre;
                let peak = if s.average > 0 {
                    let q = now * now;
                    self.sq_sum += q - self.squares[self.sq_pos];
                    self.squares[self.sq_pos] = q;
                    self.sq_pos += 1;
                    if self.sq_pos >= self.sq_len {
                        self.sq_pos = 0;
                        // keep the running sum honest
                        self.sq_sum = self.squares.iter().sum();
                    }
                    (self.sq_sum.max(0.0) / self.sq_len as f64).sqrt()
                } else if s.window > 0 {
                    while self.peaks.back().is_some_and(|b| b.0 <= now) {
                        self.peaks.pop_back();
                    }
                    self.peaks.push_back((now, self.t));
                    while self.peaks.front().is_some_and(|f| self.t - f.1 >= s.window as u64) {
                        self.peaks.pop_front();
                    }
                    self.peaks.front().map_or(now, |f| f.0)
                } else {
                    now
                };
                self.t += 1;
                // the level: a row of followers, each up at once and down at its own pace
                let mut level = peak;
                for z in &mut self.zr[..s.m] {
                    if level >= *z {
                        *z = level;
                    } else {
                        *z += (level - *z) * s.kr;
                    }
                    level = *z;
                }
                let mut to = 1.0 - s.want(p, level);
                for z in &mut self.zg[..s.n] {
                    let c = if to > *z { s.ka } else { s.ka.min(s.k2) };
                    *z += (to - *z) * c;
                    if z.abs() < 1e-30 {
                        *z = 0.0;
                    }
                    to = *z;
                }
                // the gain after comes before the shaper, so the ceiling is the last word
                let g = (1.0 - to) * s.pre * STAGE_GAIN * s.post;
                [s.shape(dry[0] * g), s.shape(dry[1] * g)]
            }
            // no level stage: the gain before does nothing, the gain after still feeds the shaper
            1 => [s.shape(dry[0] * s.post), s.shape(dry[1] * s.post)],
            _ => [0.0; 2],
        };
        if s.side != 1.0 && s.mode < 2 {
            let (mid, side) = ((y[0] + y[1]) * 0.5, (y[0] - y[1]) * 0.5 * s.side);
            y = [mid + side, mid - side];
        }
        (dry, y)
    }
}

/// The linear-phase split: two low-pass kernels over one line of past input.
#[derive(Default)]
struct Linear {
    key: (u64, u64, bool, bool, u64),
    low: Vec<f32>,
    high: Vec<f32>,
    late: usize,
    ring: Vec<[f32; 2]>,
    pos: usize,
}

fn kernel(f: f64, steep: bool, rate: f64, late: usize) -> Vec<f32> {
    // the reach each side, in frames: measured between 0.6403 and 0.6469 periods of the corner for the steep
    // split (half that for the gentle one), never under three frames
    let half = ((if steep { 0.6436 } else { 0.3218 }) * rate / f).floor().max(3.0).min(late as f64);
    (0..=2 * late)
        .map(|j| {
            let n = j as f64 - late as f64;
            if n.abs() > half {
                return 0.0;
            }
            let x = 2.0 * f * n / rate;
            let sinc = if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) };
            let w = PI * n / half;
            let win = 0.35875 + 0.48829 * w.cos() + 0.14128 * (2.0 * w).cos() + 0.01168 * (3.0 * w).cos();
            (2.0 * f / rate * sinc * win) as f32
        })
        .collect()
}

pub struct State {
    sr: f32,
    cut: [[[f64; 2]; 4]; 2],
    up: [[[Pass; 3]; 2]; 2],
    down: [[[Pass; 3]; 2]; 2],
    lp_lo: Mem,
    hp_lo: Mem,
    lp_hi: Mem,
    hp_hi: Mem,
    fix_lp: Mem,
    fix_hp: Mem,
    linear: Linear,
    stages: [Stage; 4],
}

impl Default for State {
    fn default() -> Self {
        Self {
            sr: 0.0,
            cut: Default::default(),
            up: Default::default(),
            down: Default::default(),
            lp_lo: Default::default(),
            hp_lo: Default::default(),
            lp_hi: Default::default(),
            hp_hi: Default::default(),
            fix_lp: Default::default(),
            fix_hp: Default::default(),
            linear: Default::default(),
            stages: Default::default(),
        }
    }
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if p.len() < LEN {
        return;
    }
    if st.sr != cx.sr {
        *st = State { sr: cx.sr, ..Default::default() };
    }
    let sr = cx.sr as f64;
    let over = p[OVERSAMPLE] >= 0.5;
    let rate = if over { sr * 2.0 } else { sr };
    let times = if over { 2 } else { 1 };
    let mode = |b: usize| p[7 + 13 * b] as i32;
    let (low_on, mid_on, high_on) = (mode(0) != 3, mode(1) != 3, mode(2) != 3);
    // how the range is cut up: 0 not at all, 1 at the low split only, 2 at the high split only, 3 at both
    let cuts = match (low_on, mid_on, high_on) {
        // a silent middle band leaves a hole: the outer two keep their own splits
        (true, true, true) | (true, false, true) => 3,
        (true, true, false) => 1,
        (false, true, true) => 2,
        _ => 0,
    };
    let (steep_lo, steep_hi) = (p[1] >= 0.5, p[3] >= 0.5);
    let turned = !p.get(UNTURNED).is_some_and(|v| *v >= 0.5);
    // with both splits in use the lower one is always the first, whichever knob it was set with; how steep
    // each is goes by its place
    let (f_lo, f_hi) = (split_hz(p[0]), split_hz(p[2]));
    let (f_lo, f_hi) = if cuts == 3 { (f_lo.min(f_hi), f_lo.max(f_hi)) } else { (f_lo, f_hi) };
    let linear = p[LINEAR] >= 0.5 && cuts != 0;
    let (lo_l, lo_h) = halves(f_lo, steep_lo, rate);
    let (hi_l, hi_h) = halves(f_hi, steep_hi, rate);
    let turn = halves(f_hi, false, rate);
    if linear {
        let late = (LINEAR_LATE * sr) as usize * times;
        let key = (f_lo.to_bits(), f_hi.to_bits(), steep_lo, steep_hi, rate.to_bits());
        let l = &mut st.linear;
        if l.key != key || l.late != late || l.ring.is_empty() {
            l.key = key;
            l.low = kernel(f_lo, steep_lo, rate, late);
            l.high = kernel(f_hi, steep_hi, rate, late);
            if l.late != late || l.ring.is_empty() {
                l.late = late;
                l.ring = vec![[0.0; 2]; (2 * late + 1).next_power_of_two()];
                l.pos = 0;
            }
        }
    }

    // the low cut: four second-order high-pass sections
    let cut: Option<[Bq; 4]> = low_cut_hz(p[6]).map(|f| {
        let k = (PI * f / sr).tan();
        std::array::from_fn(|i| {
            let q = 1.0 / (2.0 * (PI * (2 * i + 1) as f64 / 16.0).cos());
            let n = 1.0 / (1.0 + k / q + k * k);
            Bq { b: [n, -2.0 * n, n], a: [2.0 * (k * k - 1.0) * n, (1.0 - k / q + k * k) * n] }
        })
    });

    let band_look = time_ms(p[4]);
    let band_lag = if (0..3).any(|b| mode(b) == 0) { frames(band_look, sr).round() as usize * times } else { 0 };
    // the bands may run at twice the rate; the stage on their sum never does
    let mut sets: [Set; 4] = std::array::from_fn(|b| if b < 3 { Set::new(p, b, rate, Some(band_look), band_lag) } else { Set::new(p, 3, sr, None, 0) });
    sets[3].lag = if sets[3].mode == 0 { frames(time_ms(p[7 + 39 + 6]), sr).round() as usize } else { 0 };
    for (stage, set) in st.stages.iter_mut().zip(&sets) {
        stage.fit(set);
    }
    let mix = (p[5] as f64 / 1000.0).clamp(0.0, 1.0);

    let State { cut: cut_z, up, down, lp_lo, hp_lo, lp_hi, hp_hi, fix_lp, fix_hp, linear: lin, stages, .. } = st;
    let (stages, main) = stages.split_at_mut(3);
    let main = &mut main[0];
    let mut one = |x: [f64; 2]| -> [f64; 2] {
        // the three bands
        let mut bands = [[0f64; 2]; 3];
        if linear {
            let mask = lin.ring.len() - 1;
            lin.ring[lin.pos] = [x[0] as f32, x[1] as f32];
            let (mut a, mut b) = ([0f64; 2], [0f64; 2]);
            let start = lin.pos + lin.ring.len() - 2 * lin.late;
            for j in 0..=2 * lin.late {
                let (kl, kh) = (lin.low[j], lin.high[j]);
                if kl == 0.0 && kh == 0.0 {
                    continue;
                }
                let v = lin.ring[(start + j) & mask];
                a[0] += (kl * v[0]) as f64;
                a[1] += (kl * v[1]) as f64;
                b[0] += (kh * v[0]) as f64;
                b[1] += (kh * v[1]) as f64;
            }
            let d = lin.ring[(lin.pos + lin.ring.len() - lin.late) & mask];
            lin.pos = (lin.pos + 1) & mask;
            for ch in 0..2 {
                let d = d[ch] as f64;
                match cuts {
                    3 => {
                        bands[0][ch] = a[ch];
                        bands[1][ch] = b[ch] - a[ch];
                        bands[2][ch] = d - b[ch];
                    }
                    1 => {
                        bands[0][ch] = a[ch];
                        bands[1][ch] = d - a[ch];
                    }
                    _ => {
                        bands[1][ch] = b[ch];
                        bands[2][ch] = d - b[ch];
                    }
                }
            }
        } else {
            for ch in 0..2 {
                let x = x[ch];
                match cuts {
                    3 => {
                        let mut lo = twice(&lo_l, lp_lo, ch, x);
                        let rest = twice(&lo_h, hp_lo, ch, x);
                        bands[1][ch] = twice(&hi_l, lp_hi, ch, rest);
                        let hi = twice(&hi_h, hp_hi, ch, rest);
                        // behind a steep high split the low band is turned to stay in step with the others:
                        // twice through a first-order turn at that split
                        if steep_hi && turned {
                            for k in 0..2 {
                                lo = turn.0.run(&mut fix_lp[ch][k], lo) - turn.1.run(&mut fix_hp[ch][k], lo);
                            }
                        }
                        bands[0][ch] = if steep_lo { lo } else { -lo };
                        bands[2][ch] = if steep_hi { hi } else { -hi };
                    }
                    1 => {
                        let lo = twice(&lo_l, lp_lo, ch, x);
                        bands[0][ch] = if steep_lo { lo } else { -lo };
                        bands[1][ch] = twice(&lo_h, hp_lo, ch, x);
                    }
                    2 => {
                        bands[1][ch] = twice(&hi_l, lp_hi, ch, x);
                        let hi = twice(&hi_h, hp_hi, ch, x);
                        bands[2][ch] = if steep_hi { hi } else { -hi };
                    }
                    _ => {
                        let only = if mid_on { 1 } else if low_on { 0 } else { 2 };
                        bands[only][ch] = x;
                    }
                }
            }
        }
        let mut sum = [0f64; 2];
        for b in 0..3 {
            if sets[b].mode == 3 {
                continue;
            }
            let (dry, wet) = stages[b].run(&sets[b], p, bands[b]);
            sum[0] += dry[0] + (wet[0] - dry[0]) * mix;
            sum[1] += dry[1] + (wet[1] - dry[1]) * mix;
        }
        sum
    };

    for f in io.iter_mut() {
        let mut x = [f[0] as f64, f[1] as f64];
        if let Some(cut) = &cut {
            for ch in 0..2 {
                for (q, z) in cut.iter().zip(cut_z[ch].iter_mut()) {
                    x[ch] = q.run(z, x[ch]);
                }
            }
        }
        let y = if over {
            let even = [chain(&HALF_A, &mut up[0][0], x[0]), chain(&HALF_A, &mut up[1][0], x[1])];
            let odd = [chain(&HALF_B, &mut up[0][1], x[0]), chain(&HALF_B, &mut up[1][1], x[1])];
            let a = one(even);
            let b = one(odd);
            [
                0.5 * (chain(&HALF_B, &mut down[0][0], a[0]) + chain(&HALF_A, &mut down[0][1], b[0])),
                0.5 * (chain(&HALF_B, &mut down[1][0], a[1]) + chain(&HALF_A, &mut down[1][1], b[1])),
            ]
        } else {
            one(x)
        };
        let y = if sets[3].mode == 3 { y } else { main.run(&sets[3], p, y).1 };
        *f = [y[0] as f32, y[1] as f32];
    }
}
