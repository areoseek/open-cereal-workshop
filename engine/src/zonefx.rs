//! The effects section of the zone player: an echo, a chorus and a room, each fed by its own send and
//! laid beside the plain sound. Every law here is measured on renders of our own click recordings.

use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Echo {
    /// Time between repeats in beats.
    pub beats: f32,
    /// What each repeat keeps of the one before.
    pub keep: f32,
    /// Corner knobs of the low cut and the high cut inside the loop.
    pub low: f32,
    pub high: f32,
    /// Repeats take turns between the sides, left first.
    pub turns: bool,
    pub wet: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Chorus {
    pub delay: f32,
    pub depth: f32,
    pub rate: f32,
    pub keep: f32,
    pub wet: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Room {
    pub size: f32,
    pub bright: f32,
    pub spread: f32,
    pub decay: f32,
    pub wet: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Set {
    pub echo: Option<Echo>,
    pub chorus: Option<Chorus>,
    pub room: Option<Room>,
}

/// The corner a cut knob stands for.
pub fn corner(knob: f32) -> f32 {
    13.75 * 2f32.powf(127.0 * knob / 12.0)
}

impl Set {
    pub fn any(&self) -> bool {
        self.echo.is_some() || self.chorus.is_some() || self.room.is_some()
    }

    /// Frames the effects still sound after their input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0f32;
        if let Some(e) = &self.echo {
            let repeats = if e.keep > 0.001 { (0.001f32.ln() / e.keep.min(0.98).ln()).ceil().min(120.0) } else { 1.0 };
            n = n.max(e.beats * 4.0 * step * (repeats + 1.0));
        }
        if let Some(c) = &self.chorus {
            let one = (6.0 + 2113.0 * c.delay * c.delay + 1060.0 * c.depth * c.depth) * sr / 44100.0;
            let repeats = if c.keep > 0.001 { (0.001f32.ln() / c.keep.min(0.98).ln()).ceil().min(200.0) } else { 1.0 };
            n = n.max(one * (repeats + 1.0));
        }
        if let Some(r) = &self.room {
            n = n.max(sr * r.secs());
        }
        n as usize + 2048
    }
}

impl Room {
    /// Seconds the room takes to fall by 60 dB.
    pub fn secs(&self) -> f32 {
        0.14 + 27.7 * self.decay * self.decay
    }
}

/// A two-pole filter stage (direct form, transposed).
#[derive(Default, Clone, Copy)]
struct Two {
    z: [f32; 2],
}

impl Two {
    fn run(&mut self, c: &[f32; 5], x: f32) -> f32 {
        let y = c[0] * x + self.z[0];
        self.z[0] = c[1] * x - c[3] * y + self.z[1];
        self.z[1] = c[2] * x - c[4] * y;
        if y.abs() > 1e-20 { y } else { 0.0 }
    }
}

/// Flat-topped two-pole low pass (`high` false) or high pass.
fn flat(hz: f32, sr: f32, high: bool) -> [f32; 5] {
    let w = 2.0 * PI * hz.clamp(1.0, sr * 0.49) / sr;
    let (s, c) = w.sin_cos();
    let alpha = s / std::f32::consts::SQRT_2;
    let a0 = 1.0 + alpha;
    let (b0, b1) = if high { ((1.0 + c) / 2.0, -(1.0 + c)) } else { ((1.0 - c) / 2.0, 1.0 - c) };
    [b0 / a0, b1 / a0, b0 / a0, -2.0 * c / a0, (1.0 - alpha) / a0]
}

pub struct State {
    sr: f32,
    /// Frames since the song began (the chorus keeps its place by it).
    t: u64,
    begun: bool,
    echo: [Vec<f32>; 2],
    eat: usize,
    ecut: [[Two; 2]; 2],
    /// The part-frame stage after the line: last input and last output a side.
    epart: [[f32; 2]; 2],
    cho: [Vec<f32>; 2],
    cat: usize,
    csm: [f32; 2],
    room: Option<Box<RoomState>>,
}

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, t: 0, begun: false, echo: Default::default(), eat: 0, ecut: Default::default(), epart: [[0.0; 2]; 2], cho: Default::default(), cat: 0, csm: [0.0; 2], room: None }
    }

    /// Run the section over `buf`; `step` is frames per sixteenth note and `song` where the song stands in
    /// sixteenth notes.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        let sr = self.sr;
        let fresh = !self.begun;
        if !self.begun {
            self.begun = true;
            self.t = song.map_or(0, |s| (s.max(0.0) * step as f64) as u64);
        }
        let t0 = self.t;
        self.t += buf.len() as u64;
        let mut out: Vec<[f32; 2]> = buf.to_vec();
        if let Some(e) = &set.echo {
            // (measured) the time is the knob in beats, each repeat keeps the knob's share and passes the
            // two cuts once more; with turns on the repeats go left, right, left at the same times. The
            // line runs a touch long (5/65536 of the next power of two above its length) and the part of
            // a frame is made up by a stage that passes every pitch alike
            let exact = (e.beats * 4.0 * step).max(1.0);
            let total = exact + (exact.ceil() as usize).next_power_of_two() as f32 * 5.0 / 65536.0;
            let len = total as usize;
            let part = total - len as f32;
            let pa = (1.0 - part) / (1.0 + part);
            for l in self.echo.iter_mut() {
                if l.len() != len {
                    *l = vec![0.0; len];
                    self.eat = 0;
                }
            }
            let low = (e.low > 0.0).then(|| flat(corner(e.low), sr, true));
            let high = Some(flat(corner(e.high), sr, false));
            let mut at = self.eat;
            for (f, o) in buf.iter().zip(out.iter_mut()) {
                let mut y = [0.0f32; 2];
                for side in 0..2 {
                    let x = self.echo[side][at];
                    let p = &mut self.epart[side];
                    y[side] = pa * x + p[0] - pa * p[1];
                    *p = [x, if y[side].abs() > 1e-20 { y[side] } else { 0.0 }];
                    if let Some(c) = &low {
                        y[side] = self.ecut[side][0].run(c, y[side]);
                    }
                    if let Some(c) = &high {
                        y[side] = self.ecut[side][1].run(c, y[side]);
                    }
                }
                let back = y;
                if e.turns {
                    self.echo[0][at] = (f[0] + f[1]) * 0.5 + e.keep * back[1];
                    self.echo[1][at] = e.keep * back[0];
                } else {
                    self.echo[0][at] = f[0] + e.keep * back[0];
                    self.echo[1][at] = f[1] + e.keep * back[1];
                }
                o[0] += e.wet * y[0];
                o[1] += e.wet * y[1];
                at += 1;
                if at == len {
                    at = 0;
                }
            }
            self.eat = at;
        }
        if let Some(c) = &set.chorus {
            // (measured) one copy a side whose delay is 5.5 frames + 47.9 ms x delay^2 + 12 ms x depth^2 x
            // (1 + sine), the sine at 20 Hz x rate^2 running 2048 frames ahead of the song and a quarter
            // turn further on the right, slowed by a single-stage low pass at 2.58 Hz
            let k = sr / 44100.0;
            let base = (5.5 + 2112.6 * c.delay * c.delay) * k;
            let swing = 529.2 * c.depth * c.depth * k;
            let hz = 20.0 * c.rate * c.rate;
            let len = ((base + 2.0 * swing) as usize + 8).next_power_of_two();
            for l in self.cho.iter_mut() {
                if l.len() != len {
                    *l = vec![0.0; len];
                    self.cat = 0;
                }
            }
            let mask = len - 1;
            let a = 1.0 - (-2.0 * PI * 2.58 / sr).exp();
            if fresh {
                // the sweep has been running since the song began
                let turn = hz * (t0 as f32 / sr + 2048.0 / 44100.0);
                let (gain, lag) = (1.0 / (1.0 + (hz / 2.58).powi(2)).sqrt(), (hz / 2.58).atan());
                for side in 0..2 {
                    self.csm[side] = gain * (2.0 * PI * (turn + 0.25 * side as f32) - lag).sin();
                }
            }
            let mut at = self.cat;
            for (n, (f, o)) in buf.iter().zip(out.iter_mut()).enumerate() {
                let turn = (hz as f64 * ((t0 + n as u64) as f64 / sr as f64 + 2048.0 / 44100.0)).fract() as f32;
                for side in 0..2 {
                    let s = (2.0 * PI * (turn + 0.25 * side as f32)).sin();
                    self.csm[side] += a * (s - self.csm[side]);
                    let d = (base + swing * (1.0 + self.csm[side])).max(1.0);
                    let whole = d as usize;
                    let part = d - whole as f32;
                    let l = &mut self.cho[side];
                    let y = l[(at + len - whole) & mask] * (1.0 - part) + l[(at + len - whole - 1) & mask] * part;
                    l[at] = f[side] + c.keep * y;
                    o[side] += c.wet * y;
                }
                at = (at + 1) & mask;
            }
            self.cat = at;
        }
        if let Some(r) = &set.room {
            self.room.get_or_insert_with(|| Box::new(RoomState::new(sr))).run(r, buf, &mut out);
        }
        buf.copy_from_slice(&out);
    }
}

/// A delay line read at any distance behind its write point.
struct Line {
    buf: Vec<f32>,
    at: usize,
}

impl Line {
    fn new(most: usize) -> Self {
        Self { buf: vec![0.0; (most + 2).next_power_of_two()], at: 0 }
    }
    fn read(&self, back: usize) -> f32 {
        self.buf[(self.at + self.buf.len() - back) & (self.buf.len() - 1)]
    }
    fn write(&mut self, x: f32) {
        self.buf[self.at] = x;
    }
    fn step(&mut self) {
        self.at = (self.at + 1) & (self.buf.len() - 1);
    }
    /// A smearing stage `len` frames long with share `g`.
    fn smear(&mut self, len: usize, g: f32, x: f32) -> f32 {
        let z = self.read(len);
        let v = x + g * z;
        self.write(v);
        self.step();
        z - g * v
    }
}

/// (measured) the lengths of one half of the room's ring, in frames at 44.1 kHz for a size factor of one.
struct HalfPlan {
    lead: f64,
    first: f64,
    mid: f64,
    last: f64,
    /// Where the seven outputs are read: three in the first line, two inside the middle stage, two in the
    /// last line.
    taps: [f64; 7],
}

const HALVES: [HalfPlan; 2] = [
    HalfPlan { lead: 1741.027, first: 8085.741, mid: 5092.692, last: 6064.768, taps: [510.027, 3472.405, 5702.294, 642.303, 3667.889, 1190.648, 4785.858] },
    HalfPlan { lead: 1288.514, first: 8538.224, mid: 3451.410, last: 7132.5, taps: [676.767, 3240.568, 6954.402, 358.524, 3313.311, 3002.684, 6084.002] },
];
/// (measured) the second half's last line is not a plain multiple of the size: it is this span less one
/// frame, less the first half's fourth output distance, plus its own.
const LAST_SPAN: f64 = 7416.352;
/// The four smearing stages ahead of the ring.
const FRONT: [f64; 4] = [205.117, 272.278, 531.025, 726.827];
/// How far along its stretch each output is weighed, and the side signs.
const WEIGH: [f32; 7] = [0.0, 0.4, 0.75, 0.0, 0.3, 0.5, 0.8];
const SIGN_L: [f32; 7] = [1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const SIGN_R: [f32; 7] = [1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0];
/// (measured) the damping corner at fourteen settings of the brightness knob.
const BRIGHT: [(f32, f32); 14] = [
    (0.0, 501.0),
    (0.05, 1571.0),
    (0.1, 2626.0),
    (0.15, 3649.0),
    (0.25, 5587.0),
    (0.35, 7286.0),
    (0.4, 8067.0),
    (0.5, 9414.0),
    (0.6, 10484.0),
    (0.65, 10927.0),
    (0.75, 11650.0),
    (0.85, 12119.0),
    (0.9, 12255.0),
    (1.0, 12383.0),
];

/// The damping corner for a brightness knob: a smooth curve through the measured settings that never
/// overshoots them.
fn bright_corner(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    let n = BRIGHT.len();
    let h = |i: usize| BRIGHT[i + 1].0 - BRIGHT[i].0;
    let d = |i: usize| (BRIGHT[i + 1].1 - BRIGHT[i].1) / h(i);
    let slope = |i: usize| -> f32 {
        if i == 0 {
            (((2.0 * h(0) + h(1)) * d(0) - h(0) * d(1)) / (h(0) + h(1))).max(0.0).min(3.0 * d(0))
        } else if i == n - 1 {
            (((2.0 * h(n - 2) + h(n - 3)) * d(n - 2) - h(n - 2) * d(n - 3)) / (h(n - 2) + h(n - 3))).max(0.0).min(3.0 * d(n - 2))
        } else {
            let (w1, w2) = (2.0 * h(i) + h(i - 1), h(i) + 2.0 * h(i - 1));
            (w1 + w2) / (w1 / d(i - 1) + w2 / d(i))
        }
    };
    let i = (0..n - 1).rev().find(|&i| k >= BRIGHT[i].0).unwrap_or(0);
    let t = (k - BRIGHT[i].0) / h(i);
    let (m0, m1) = (slope(i), slope(i + 1));
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * BRIGHT[i].1 + (t3 - 2.0 * t2 + t) * h(i) * m0 + (-2.0 * t3 + 3.0 * t2) * BRIGHT[i + 1].1 + (t3 - t2) * h(i) * m1
}

impl Room {
    /// The size factor every length in the room is scaled by.
    fn scale(&self) -> f32 {
        self.size.clamp(0.00998, 1.0) + 0.149986
    }
    /// What a quarter of the ring keeps.
    fn quarter(&self) -> f32 {
        (-self.scale() / (0.0855 + 17.09 * self.decay * self.decay)).exp()
    }
}

struct RoomHalf {
    lead: Line,
    first: Line,
    mid: Line,
    last: Line,
    damp: f32,
}

struct RoomState {
    sr: f32,
    front: [Line; 4],
    half: [RoomHalf; 2],
    low: f32,
    high: f32,
}

impl RoomState {
    fn new(sr: f32) -> Self {
        let most = |n: f64| (n * sr as f64 / 44100.0 * 1.15) as usize + 2;
        let half = |p: &HalfPlan| RoomHalf { lead: Line::new(most(p.lead)), first: Line::new(most(p.first)), mid: Line::new(most(p.mid)), last: Line::new(most(p.last)), damp: 0.0 };
        Self { sr, front: FRONT.map(|n| Line::new(most(n))), half: [half(&HALVES[0]), half(&HALVES[1])], low: 0.0, high: 0.0 }
    }

    /// (measured) a ring of two halves fed alike by the two sides summed. Each half is a smearing stage, a
    /// line, a second smearing stage and a second line; what leaves a half is damped and enters the other.
    /// Seven points of each half are heard, shared out between the sides. Every length follows the size,
    /// and the three smearing shares follow size over spread.
    fn run(&mut self, r: &Room, buf: &[[f32; 2]], out: &mut [[f32; 2]]) {
        let sr = self.sr;
        let f = r.scale();
        let q = |n: f64| ((n * (sr as f64 / 44100.0) * f as f64) as usize).max(1);
        let share = |c: f32| if r.spread > 0.001 { (-c * f / r.spread).exp() } else { 0.0 };
        let (g_front, g_lead, g_mid) = (share(0.5314), share(0.6775), share(1.312));
        let keep = r.quarter();
        let pole = |hz: f32| (-2.0 * PI * hz / sr).exp();
        let a = pole(bright_corner(r.bright).min(0.45 * sr));
        let b = pole(12354.0f32.min(0.45 * sr));
        let p = pole(94.8);
        let front = FRONT.map(q);
        struct Len {
            lead: usize,
            first: usize,
            mid: usize,
            last: usize,
            taps: [usize; 7],
        }
        let len = |h: &HalfPlan| Len { lead: q(h.lead), first: q(h.first), mid: q(h.mid), last: q(h.last), taps: h.taps.map(q) };
        let mut lens = [len(&HALVES[0]), len(&HALVES[1])];
        lens[1].last = (q(LAST_SPAN) + lens[1].taps[3]).saturating_sub(1 + lens[0].taps[3]).max(1);
        let mut weigh = [0.0f32; 7];
        for i in 0..7 {
            weigh[i] = (1.0 - WEIGH[i] * (1.0 - keep)) * if i >= 3 { keep } else { 1.0 };
        }
        let wet = r.wet * 0.99304;
        for (i, o) in out.iter_mut().enumerate() {
            let x = 0.5 * (buf[i][0] + buf[i][1]);
            let y = x - self.high;
            self.high += (1.0 - p) * (x - self.high);
            self.low = (1.0 - b) * y + b * self.low;
            let mut u = self.low;
            for (l, n) in self.front.iter_mut().zip(front) {
                u = l.smear(n, g_front, u);
            }
            let ends = [self.half[0].last.read(lens[0].last), self.half[1].last.read(lens[1].last)];
            let mut taps = [[0.0f32; 7]; 2];
            for h in 0..2 {
                let (hf, ln) = (&mut self.half[h], &lens[h]);
                hf.damp = (1.0 - a) * (u + keep * keep * ends[1 - h]) + a * hf.damp;
                if hf.damp.abs() < 1e-24 {
                    hf.damp = 0.0;
                }
                let v = hf.lead.smear(ln.lead, g_lead, hf.damp);
                hf.first.write(v);
                let z = hf.mid.read(ln.mid);
                let xi = hf.first.read(ln.first);
                let y = z - g_mid * xi;
                hf.mid.write(xi + g_mid * y);
                hf.last.write(y);
                for k in 0..7 {
                    taps[h][k] = weigh[k] * match k {
                        0..=2 => hf.first.read(ln.taps[k]),
                        3 | 4 => hf.mid.read(ln.taps[k]),
                        _ => hf.last.read(ln.taps[k]),
                    };
                }
                hf.first.step();
                hf.mid.step();
                hf.last.step();
            }
            let (mut l, mut rr) = (0.0f32, 0.0f32);
            for k in 0..7 {
                // the odd points go to the left from the first half, the even ones from the second
                let (a, b) = if k % 2 == 0 { (taps[0][k], taps[1][k]) } else { (taps[1][k], taps[0][k]) };
                l += SIGN_L[k] * a;
                rr += SIGN_R[k] * b;
            }
            o[0] += wet * l;
            o[1] += wet * rr;
        }
    }
}
