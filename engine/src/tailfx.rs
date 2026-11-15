//! The effects some instruments carry after their voices: a body of eight resonances, a stereo echo, a
//! chorus, a sixteen-step gate and a room. They run once per channel, on the sum of its voices.

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Delay {
    /// Echo times of the two sides, in sixteenth notes.
    pub left: f32,
    pub right: f32,
    /// 0 to under 1.
    pub fb: f32,
    /// How far a slow sweep moves the echo time, 0 to 1.
    pub wobble: f32,
    /// Filters inside the loop: each repeat loses more below and above these (0 turns the low cut off).
    pub low_hz: f32,
    pub high_hz: f32,
    pub wet: f32,
    /// Above 0: the echo time of both sides in seconds, whatever the tempo.
    pub free: f32,
}

/// Eight resonances the sound sets ringing, as the body of an instrument does.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Body {
    pub hz: [f32; 8],
    /// How much each one adds and how long it rings, 0 to 1 (0 is off).
    pub fb: [f32; 8],
    /// Bit k set: stage k is added upside down.
    pub flip: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reverb {
    /// Seconds the room takes to fade.
    pub secs: f32,
    /// Highs above this fade faster.
    pub high_hz: f32,
    pub wet: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Chorus {
    pub depth: f32,
    pub hz: f32,
    pub wet: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Gate {
    pub wet: f32,
    /// 0 snaps open and shut, 1 fades over most of a step.
    pub smooth: f32,
    /// Bit k set: step k lets the sound through.
    pub steps: u16,
    /// Sixteenth notes per step.
    pub len: f32,
}

/// A row of simple effects an instrument carries: the first six run before the echo, the rest after the room.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rack {
    /// Overdrive: amount 0 to 1, then a high cut in Hz.
    pub drive: Option<[f32; 2]>,
    /// Held samples: how many frames (at 44.1 kHz) each one is held, then a high cut in Hz.
    pub crush: Option<[f32; 2]>,
    /// A short swept echo: depth and feedback, each 0 to 1.
    pub flange: Option<[f32; 2]>,
    /// Swept all-pass stages: depth and feedback, each 0 to 1.
    pub phase: Option<[f32; 2]>,
    /// Low pass: Hz and resonance 0 to 1.
    pub low: Option<[f32; 2]>,
    /// High pass: Hz and resonance 0 to 1.
    pub high: Option<[f32; 2]>,
    /// Left (-1) to right (1).
    pub pan: Option<f32>,
    /// A dip on every beat: sixteenth notes to recover, and how deep (0 to 1).
    pub pump: Option<[f32; 2]>,
    /// Five bands (60, 220, 1500, 8000, 12000 Hz), each in dB.
    pub eq: Option<[f32; 5]>,
}

#[derive(Debug, Clone, Default)]
struct RackState {
    drive_lp: [f32; 2],
    crush_lp: [f32; 2],
    held: [f32; 2],
    count: f32,
    fl: Vec<[f32; 2]>,
    fl_at: usize,
    fl_ph: f32,
    ph: [[f32; 2]; 4],
    ph_last: [f32; 2],
    ph_ph: f32,
    low: [[f32; 2]; 2],
    high: [[f32; 2]; 2],
    pump: f32,
    clock: f64,
    eq: [[[f32; 2]; 2]; 5],
}

/// One step of a state-variable filter; gives (low, band, high).
#[inline]
fn svf(s: &mut [f32; 2], x: f32, g: f32, k: f32) -> (f32, f32, f32) {
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    let v3 = x - s[1];
    let v1 = a1 * s[0] + a2 * v3;
    let v2 = s[1] + a2 * s[0] + g * a2 * v3;
    s[0] = 2.0 * v1 - s[0];
    s[1] = 2.0 * v2 - s[1];
    (v2, v1, x - k * v1 - v2)
}

/// One band of a three-band equaliser.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Band {
    /// 0 low shelf, 1 peak, 2 high shelf.
    pub kind: u8,
    pub hz: f32,
    pub db: f32,
    /// Sharpness: a peak's centre over its width, a shelf's steepness (0.707 is the steepest without a bump).
    pub q: f32,
}

impl Band {
    /// The filter as (b0, b1, b2, a1, a2), after the well-known cookbook formulas.
    fn coefs(&self, sr: f32) -> [f32; 5] {
        let a = 10f32.powf(self.db / 40.0);
        let w = TAU * self.hz.clamp(5.0, sr * 0.45) / sr;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * self.q.clamp(0.05, 200.0));
        let (b, a0, a1, a2);
        match self.kind {
            1 => {
                b = [1.0 + alpha * a, -2.0 * cs, 1.0 - alpha * a];
                (a0, a1, a2) = (1.0 + alpha / a, -2.0 * cs, 1.0 - alpha / a);
            }
            k => {
                let r = 2.0 * a.sqrt() * alpha;
                // the high shelf is the low one mirrored
                let s = if k == 0 { 1.0 } else { -1.0 };
                b = [a * ((a + 1.0) - s * (a - 1.0) * cs + r), s * 2.0 * a * ((a - 1.0) - s * (a + 1.0) * cs), a * ((a + 1.0) - s * (a - 1.0) * cs - r)];
                (a0, a1, a2) = ((a + 1.0) + s * (a - 1.0) * cs + r, s * -2.0 * ((a - 1.0) + s * (a + 1.0) * cs), (a + 1.0) + s * (a - 1.0) * cs - r);
            }
        }
        [b[0] / a0, b[1] / a0, b[2] / a0, a1 / a0, a2 / a0]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Set {
    /// A three-band equaliser ahead of everything else.
    pub tone: Option<[Band; 3]>,
    pub rack: Option<Rack>,
    pub delay: Option<Delay>,
    /// Further echoes beside the first, each fed the same sound.
    pub more: [Option<Delay>; 2],
    pub chorus: Option<Chorus>,
    pub gate: Option<Gate>,
    pub body: Option<Body>,
    pub reverb: Option<Reverb>,
    /// The instrument hands over what it sends to the effects apart from the rest of its sound.
    pub split: bool,
    /// With `split`: the equaliser stands on everything (0 or 1), on the plain part (2) or on the sent part (3).
    pub tone_at: u8,
    /// The effects section of the operator synth, which stands in for everything above but the equaliser.
    pub op: Option<crate::opfx::Set>,
    /// Anything standing still in the sound is let drain away (the operator synth's own switch).
    pub level: bool,
}

impl Set {
    pub fn any(&self) -> bool {
        self.split || self.level || self.tone.is_some() || self.rack.is_some() || self.delay.is_some() || self.chorus.is_some() || self.gate.is_some() || self.body.is_some() || self.reverb.is_some()
    }

    /// Frames the effects still sound after their input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0;
        if let Some(o) = &self.op {
            return o.tail(sr, step) + if self.level { (sr * 0.5) as usize } else { 0 };
        }
        if self.level {
            n = sr * 0.5;
        }
        for d in self.delay.iter().chain(self.more.iter().flatten()) {
            let time = if d.free > 0.0 { d.free * sr } else { d.left.max(d.right) * step };
            let repeats = if d.fb > 0.001 { (0.001f32.ln() / d.fb.ln()).ceil().min(60.0) } else { 1.0 };
            n += time * (repeats + 1.0);
        }
        if self.chorus.is_some() || self.rack.is_some() {
            n += sr * 0.05;
        }
        if self.body.is_some() {
            n += sr * 0.6;
        }
        if let Some(r) = &self.reverb {
            n += sr * r.secs;
        }
        n as usize + 2048
    }
}

pub struct State {
    sr: f32,
    still: [[f32; 2]; 2],
    line: Vec<[f32; 2]>,
    at: usize,
    sweep: f32,
    lp: [f32; 2],
    hp: [f32; 2],
    time: [f32; 2],
    cho: Vec<[f32; 2]>,
    cat: usize,
    cph: f32,
    open: f32,
    /// Where the gate stands, in sixteenth notes.
    clock: f64,
    body: [[[f32; 2]; 2]; 8],
    combs: Vec<(Vec<f32>, usize, f32)>,
    passes: Vec<(Vec<f32>, usize)>,
    rack: Box<RackState>,
    tone: [[[f32; 2]; 2]; 3],
    more: [Side; 2],
    dry: Vec<[f32; 2]>,
    op: Option<Box<crate::opfx::State>>,
}

/// What one of the further echoes remembers.
#[derive(Default)]
struct Side {
    line: Vec<[f32; 2]>,
    at: usize,
    lp: [f32; 2],
}

/// Comb and all-pass lengths of the room at 44.1 kHz; the right side is a little longer.
const COMBS: [usize; 4] = [1557, 1617, 1491, 1422];
const PASSES: [usize; 2] = [225, 556];

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, still: [[0.0; 2]; 2], line: Vec::new(), at: 0, sweep: 0.0, lp: [0.0; 2], hp: [0.0; 2], time: [0.0; 2], cho: Vec::new(), cat: 0, cph: 0.0, open: 1.0, clock: 0.0, body: [[[0.0; 2]; 2]; 8], combs: Vec::new(), passes: Vec::new(), rack: Box::default(), tone: [[[0.0; 2]; 2]; 3], more: Default::default(), dry: Vec::new(), op: None }
    }

    /// Run the effects over `buf`. `step` is frames per sixteenth note; `song` is where the song stands in
    /// sixteenth notes while it plays, so the gate keeps its place in the bar.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        self.tone(set, buf);
        self.rest(set, buf, step, song);
        self.level(set, buf);
    }

    /// Measured on a note with a standing part: with the switch on, the standing part fades the way it does
    /// through a single-stage high-pass at 3.8 Hz, and comes back the other way round when the note ends.
    fn level(&mut self, set: &Set, buf: &mut [[f32; 2]]) {
        if !set.level {
            return;
        }
        let a = (-2.0 * std::f32::consts::PI * 3.8 / self.sr).exp();
        for f in buf.iter_mut() {
            for side in 0..2 {
                let y = a * (self.still[side][1] + (f[side] - self.still[side][0]));
                self.still[side] = [f[side], if y.abs() > 1e-20 { y } else { 0.0 }];
                f[side] = y;
            }
        }
    }

    /// For an instrument that sends only a share of its sound to the effects: `sent` goes through them and
    /// joins `plain`, and the equaliser works on whichever part it is set to (see `Set::tone_at`).
    pub fn split(&mut self, set: &Set, plain: &mut [[f32; 2]], sent: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        match set.tone_at {
            2 => self.tone(set, plain),
            3 => self.tone(set, sent),
            _ => {}
        }
        match &set.op {
            Some(o) => self.op.get_or_insert_with(|| Box::new(crate::opfx::State::new(self.sr))).process(o, sent, step),
            None => self.rest(set, sent, step, song),
        }
        for (o, f) in plain.iter_mut().zip(sent.iter_mut()) {
            o[0] += f[0];
            o[1] += f[1];
            *f = [0.0; 2];
        }
        if set.tone_at < 2 {
            self.tone(set, plain);
        }
        self.level(set, plain);
    }

    fn tone(&mut self, set: &Set, buf: &mut [[f32; 2]]) {
        let sr = self.sr;
        if let Some(bands) = &set.tone {
            for (band, z) in bands.iter().zip(self.tone.iter_mut()) {
                if band.db == 0.0 {
                    continue;
                }
                let c = band.coefs(sr);
                for f in buf.iter_mut() {
                    for side in 0..2 {
                        let x = f[side];
                        let y = c[0] * x + z[side][0];
                        z[side][0] = c[1] * x - c[3] * y + z[side][1];
                        z[side][1] = c[2] * x - c[4] * y;
                        f[side] = y;
                    }
                }
            }
        }
    }

    fn rest(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        let sr = self.sr;
        if let Some(r) = &set.rack {
            self.rack_first(r, buf);
        }
        if let Some(b) = &set.body {
            for k in 0..8 {
                if b.fb[k] <= 0.0 {
                    continue;
                }
                let g = (std::f32::consts::PI * b.hz[k].clamp(20.0, sr * 0.45) / sr).tan();
                // a higher setting rings longer
                let r = 1.0 / (2.0 + 60.0 * b.fb[k] * b.fb[k]);
                let h = 1.0 / (1.0 + g * (g + r));
                let amount = b.fb[k] * if b.flip >> k & 1 == 1 { -1.0 } else { 1.0 };
                let z = &mut self.body[k];
                // each stage lifts its own band of what the ones before it passed on
                for f in buf.iter_mut() {
                    for side in 0..2 {
                        let x = f[side];
                        let hp = (x - (g + r) * z[side][0] - z[side][1]) * h;
                        let bp = g * hp + z[side][0];
                        z[side][0] = g * hp + bp;
                        let lp = g * bp + z[side][1];
                        z[side][1] = g * bp + lp;
                        f[side] = x + bp * r * amount;
                    }
                }
            }
        }
        let many = set.more.iter().any(|m| m.is_some());
        if many {
            self.dry.clear();
            self.dry.extend_from_slice(buf);
        }
        if let Some(d) = &set.delay {
            let n = (sr * 4.0) as usize;
            if self.line.len() != n {
                self.line = vec![[0.0; 2]; n];
                self.at = 0;
                self.time = [0.0; 2];
            }
            let (left, right) = if d.free > 0.0 { (d.free * sr, d.free * sr) } else { (d.left * step, d.right * step) };
            let want = [left.clamp(8.0, n as f32 - 64.0), right.clamp(8.0, n as f32 - 64.0)];
            if self.time[0] == 0.0 {
                self.time = want;
            }
            let lpc = 1.0 - (-TAU * d.high_hz.min(sr * 0.45) / sr).exp();
            let hpc = if d.low_hz > 0.0 { 1.0 - (-TAU * d.low_hz / sr).exp() } else { 0.0 };
            let depth = d.wobble * 0.0015 * sr;
            for f in buf.iter_mut() {
                self.sweep = (self.sweep + 0.35 / sr).fract();
                let w = (self.sweep * TAU).sin() * depth;
                let mut wet = [0.0f32; 2];
                for side in 0..2 {
                    // a changed time is reached by sliding, which keeps it free of clicks
                    self.time[side] += (want[side] - self.time[side]) * 0.0005;
                    let back = (self.time[side] + if side == 0 { w } else { -w }).clamp(2.0, n as f32 - 4.0);
                    let whole = back.floor();
                    let i = (self.at + n - whole as usize) % n;
                    let j = (i + n - 1) % n;
                    let mut y = self.line[i][side] + (self.line[j][side] - self.line[i][side]) * (back - whole);
                    self.lp[side] += (y - self.lp[side]) * lpc;
                    y = self.lp[side];
                    if hpc > 0.0 {
                        self.hp[side] += (y - self.hp[side]) * hpc;
                        y -= self.hp[side];
                    }
                    wet[side] = y;
                }
                self.line[self.at] = [f[0] + wet[0] * d.fb, f[1] + wet[1] * d.fb];
                self.at = (self.at + 1) % n;
                f[0] += wet[0] * d.wet;
                f[1] += wet[1] * d.wet;
            }
        }
        for (d, st) in set.more.iter().zip(self.more.iter_mut()) {
            let Some(d) = d else { continue };
            let n = (sr * 4.0) as usize;
            if st.line.len() != n {
                st.line = vec![[0.0; 2]; n];
                st.at = 0;
            }
            let (left, right) = if d.free > 0.0 { (d.free * sr, d.free * sr) } else { (d.left * step, d.right * step) };
            let back = [left.clamp(8.0, n as f32 - 64.0) as usize, right.clamp(8.0, n as f32 - 64.0) as usize];
            let lpc = 1.0 - (-TAU * d.high_hz.min(sr * 0.45) / sr).exp();
            for (f, x) in buf.iter_mut().zip(self.dry.iter()) {
                let mut wet = [0.0f32; 2];
                for side in 0..2 {
                    let y = st.line[(st.at + n - back[side]) % n][side];
                    st.lp[side] += (y - st.lp[side]) * lpc;
                    wet[side] = st.lp[side];
                }
                st.line[st.at] = [x[0] + wet[0] * d.fb, x[1] + wet[1] * d.fb];
                st.at = (st.at + 1) % n;
                f[0] += wet[0] * d.wet;
                f[1] += wet[1] * d.wet;
            }
        }
        if let Some(c) = &set.chorus {
            let n = (sr * 0.05) as usize + 8;
            if self.cho.len() != n {
                self.cho = vec![[0.0; 2]; n];
                self.cat = 0;
            }
            let (base, depth) = (0.002 * sr, c.depth * 0.012 * sr);
            for f in buf.iter_mut() {
                self.cho[self.cat] = *f;
                self.cph = (self.cph + c.hz / sr).fract();
                for side in 0..2 {
                    // the two sides sweep a quarter turn apart
                    let ph = (self.cph + side as f32 * 0.25).fract();
                    let back = base + (0.5 - 0.5 * (ph * TAU).cos()) * depth;
                    let whole = back.floor();
                    let i = (self.cat + n - whole as usize) % n;
                    let j = (i + n - 1) % n;
                    let y = self.cho[i][side] + (self.cho[j][side] - self.cho[i][side]) * (back - whole);
                    f[side] += y * c.wet;
                }
                self.cat = (self.cat + 1) % n;
            }
        }
        if let Some(g) = &set.gate {
            if let Some(at) = song {
                self.clock = at;
            }
            let per = 1.0 / step.max(1.0) as f64;
            let len = g.len.max(0.03125) as f64;
            // from a millisecond to most of a step
            let secs = 0.001 + g.smooth * g.smooth * 0.8 * len as f32 * step / sr;
            let c = 1.0 - (-1.0 / (secs * sr)).exp();
            for f in buf.iter_mut() {
                let k = ((self.clock / len) as u64 % 16) as u16;
                let want = if g.steps >> k & 1 == 1 { 1.0 } else { 0.0 };
                self.open += (want - self.open) * c;
                let gain = 1.0 - g.wet * (1.0 - self.open);
                f[0] *= gain;
                f[1] *= gain;
                self.clock += per;
            }
        } else if let Some(at) = song {
            self.clock = at;
        }
        if let Some(r) = &set.reverb {
            if self.combs.is_empty() {
                let scale = sr / 44100.0;
                for side in 0..2 {
                    for len in COMBS {
                        self.combs.push((vec![0.0; ((len + side * 23) as f32 * scale) as usize + 1], 0, 0.0));
                    }
                }
                for side in 0..2 {
                    for len in PASSES {
                        self.passes.push((vec![0.0; ((len + side * 23) as f32 * scale) as usize + 1], 0));
                    }
                }
            }
            let damp = 1.0 - (-TAU * r.high_hz.clamp(200.0, sr * 0.45) / sr).exp();
            let mut gains = [0.0f32; 8];
            for (g, c) in gains.iter_mut().zip(&self.combs) {
                *g = 10f32.powf(-3.0 * c.0.len() as f32 / (r.secs.max(0.05) * sr));
            }
            for f in buf.iter_mut() {
                for side in 0..2 {
                    let x = f[side] * 0.25;
                    let mut sum = 0.0;
                    for k in 0..4 {
                        let (line, at, low) = &mut self.combs[side * 4 + k];
                        let y = line[*at];
                        *low += (y - *low) * damp;
                        line[*at] = x + *low * gains[side * 4 + k];
                        *at = (*at + 1) % line.len();
                        sum += y;
                    }
                    for k in 0..2 {
                        let (line, at) = &mut self.passes[side * 2 + k];
                        let held = line[*at];
                        let y = held - sum * 0.5;
                        line[*at] = sum + held * 0.5;
                        *at = (*at + 1) % line.len();
                        sum = y;
                    }
                    f[side] = f[side] * (1.0 - r.wet * 0.5) + sum * r.wet;
                }
            }
        }
        if let Some(r) = &set.rack {
            self.rack_last(r, buf, step, song);
        }
    }

    fn rack_first(&mut self, r: &Rack, buf: &mut [[f32; 2]]) {
        let sr = self.sr;
        let st = &mut *self.rack;
        let pole = |hz: f32| 1.0 - (-TAU * hz.clamp(20.0, sr * 0.45) / sr).exp();
        if let Some([amount, hz]) = r.drive {
            let push = 1.0 + amount * amount * 40.0;
            // louder in, about as loud out
            let back = 1.0 / (1.0 + amount * 2.0);
            let c = pole(hz);
            for f in buf.iter_mut() {
                for side in 0..2 {
                    let y = (f[side] * push).tanh() * back * (1.0 + amount);
                    st.drive_lp[side] += (y - st.drive_lp[side]) * c;
                    f[side] = st.drive_lp[side];
                }
            }
        }
        if let Some([hold, hz]) = r.crush {
            let hold = (hold * sr / 44100.0).max(1.0);
            let c = pole(hz);
            for f in buf.iter_mut() {
                st.count += 1.0;
                if st.count >= hold {
                    st.count -= hold;
                    st.held = *f;
                }
                for side in 0..2 {
                    st.crush_lp[side] += (st.held[side] - st.crush_lp[side]) * c;
                    f[side] = st.crush_lp[side];
                }
            }
        }
        if let Some([depth, fb]) = r.flange {
            let n = (sr * 0.012) as usize + 8;
            if st.fl.len() != n {
                st.fl = vec![[0.0; 2]; n];
                st.fl_at = 0;
            }
            let fb = fb.clamp(0.0, 1.0) * 0.9;
            for f in buf.iter_mut() {
                st.fl_ph = (st.fl_ph + 0.25 / sr).fract();
                let mut wet = [0.0f32; 2];
                for side in 0..2 {
                    let ph = (st.fl_ph + side as f32 * 0.25).fract();
                    let back = (0.0005 + (0.5 - 0.5 * (ph * TAU).cos()) * depth * 0.008) * sr + 1.0;
                    let whole = back.floor();
                    let i = (st.fl_at + n - whole as usize) % n;
                    let j = (i + n - 1) % n;
                    wet[side] = st.fl[i][side] + (st.fl[j][side] - st.fl[i][side]) * (back - whole);
                }
                st.fl[st.fl_at] = [f[0] + wet[0] * fb, f[1] + wet[1] * fb];
                st.fl_at = (st.fl_at + 1) % n;
                f[0] = (f[0] + wet[0]) * 0.7;
                f[1] = (f[1] + wet[1]) * 0.7;
            }
        }
        if let Some([depth, fb]) = r.phase {
            let fb = fb.clamp(0.0, 1.0) * 0.85;
            for f in buf.iter_mut() {
                st.ph_ph = (st.ph_ph + 0.3 / sr).fract();
                for side in 0..2 {
                    let ph = (st.ph_ph + side as f32 * 0.25).fract();
                    let hz = 300.0 * (1.0 + depth * 12.0 * (0.5 - 0.5 * (ph * TAU).cos()));
                    let t = (std::f32::consts::PI * hz.min(sr * 0.4) / sr).tan();
                    let a = (t - 1.0) / (t + 1.0);
                    let mut y = f[side] + st.ph_last[side] * fb;
                    for stage in st.ph.iter_mut() {
                        let out = a * y + stage[side];
                        stage[side] = y - a * out;
                        y = out;
                    }
                    st.ph_last[side] = y;
                    f[side] = (f[side] + y) * 0.6;
                }
            }
        }
        for (which, low) in [(r.low, true), (r.high, false)] {
            let Some([hz, res]) = which else { continue };
            let g = (std::f32::consts::PI * hz.clamp(20.0, sr * 0.45) / sr).tan();
            let k = 1.0 / (0.7 + res.clamp(0.0, 1.0) * res * 9.0);
            let state = if low { &mut st.low } else { &mut st.high };
            for f in buf.iter_mut() {
                for side in 0..2 {
                    let (lo, _, hi) = svf(&mut state[side], f[side], g, k);
                    f[side] = if low { lo } else { hi };
                }
            }
        }
    }

    fn rack_last(&mut self, r: &Rack, buf: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        let sr = self.sr;
        let st = &mut *self.rack;
        if let Some(pan) = r.pan {
            let (l, rt) = ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0));
            for f in buf.iter_mut() {
                f[0] *= l;
                f[1] *= rt;
            }
        }
        if let Some(at) = song {
            st.clock = at;
        }
        let per = 1.0 / step.max(1.0) as f64;
        if let Some([recover, drop]) = r.pump {
            let c = 1.0 - (-1.0 / (recover.max(0.05) * step * 0.35)).exp();
            for f in buf.iter_mut() {
                let before = st.clock;
                st.clock += per;
                if (before / 4.0).floor() != (st.clock / 4.0).floor() || before == 0.0 {
                    st.pump = drop.clamp(0.0, 1.0);
                }
                st.pump -= st.pump * c;
                let gain = 1.0 - st.pump;
                f[0] *= gain;
                f[1] *= gain;
            }
        } else {
            st.clock += per * buf.len() as f64;
        }
        if let Some(db) = r.eq {
            for (band, (&gain, hz)) in db.iter().zip([60.0f32, 220.0, 1500.0, 8000.0, 12000.0]).enumerate() {
                if gain.abs() < 0.05 {
                    continue;
                }
                // a band lift or cut: the band-pass part of the filter, added back scaled
                let g = (std::f32::consts::PI * hz.min(sr * 0.45) / sr).tan();
                let k = 1.2;
                let lift = 10f32.powf(gain / 20.0) - 1.0;
                for f in buf.iter_mut() {
                    for side in 0..2 {
                        let (lo, bp, hi) = svf(&mut st.eq[band][side], f[side], g, k);
                        f[side] += lift * match band {
                            0 => lo,
                            4 => hi,
                            _ => bp * k,
                        };
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_echo_comes_back_on_time_and_fades() {
        let set = Set { delay: Some(Delay { left: 2.0, right: 4.0, fb: 0.5, wobble: 0.0, low_hz: 0.0, high_hz: 20000.0, wet: 1.0, free: 0.0 }), ..Default::default() };
        let mut st = State::new(48000.0);
        let mut buf = vec![[0.0f32; 2]; 9000];
        buf[0] = [1.0, 1.0];
        st.process(&set, &mut buf, 1000.0, None);
        let peak = |side: usize, from: usize, to: usize| (from..to).map(|i| buf[i][side].abs()).fold(0.0f32, f32::max);
        assert!(peak(0, 1990, 2010) > 0.7);
        assert!(peak(0, 3990, 4010) > 0.3 && peak(0, 3990, 4010) < 0.6);
        assert!(peak(1, 1990, 2010) < 0.01);
        assert!(peak(1, 3990, 4010) > 0.7);
        assert!(set.tail(48000.0, 1000.0) > 8000);
    }

    #[test]
    fn the_gate_follows_its_pattern() {
        let set = Set { gate: Some(Gate { wet: 1.0, smooth: 0.0, steps: 0b01, len: 1.0 }), ..Default::default() };
        let mut st = State::new(48000.0);
        let mut buf = vec![[1.0f32; 2]; 4000];
        st.process(&set, &mut buf, 1000.0, Some(0.0));
        assert!(buf[500][0] > 0.95);
        assert!(buf[1800][0] < 0.01);
        // half wet lets half through on a shut step
        let half = Set { gate: Some(Gate { wet: 0.5, smooth: 0.0, steps: 0b01, len: 1.0 }), ..Default::default() };
        let mut buf = vec![[1.0f32; 2]; 2000];
        st.process(&half, &mut buf, 1000.0, Some(1.0));
        assert!((buf[900][0] - 0.5).abs() < 0.02);
    }

    #[test]
    fn the_chorus_adds_a_moving_copy() {
        let set = Set { chorus: Some(Chorus { depth: 0.5, hz: 1.0, wet: 1.0 }), ..Default::default() };
        let mut st = State::new(48000.0);
        let mut buf: Vec<[f32; 2]> = (0..4800).map(|i| [(i as f32 * 0.05).sin(); 2]).collect();
        let dry = buf.clone();
        st.process(&set, &mut buf, 1000.0, None);
        assert!(buf.iter().zip(&dry).skip(1000).any(|(a, b)| (a[0] - b[0]).abs() > 0.2));
        assert!(buf.iter().skip(1000).any(|f| (f[0] - f[1]).abs() > 0.05));
    }

    #[test]
    fn the_body_rings_at_its_own_pitch_and_the_room_fades() {
        let mut hz = [0.0; 8];
        let mut fb = [0.0; 8];
        hz[2] = 480.0;
        fb[2] = 0.8;
        let set = Set { body: Some(Body { hz, fb, flip: 0 }), ..Default::default() };
        let mut st = State::new(48000.0);
        let mut buf = vec![[0.0f32; 2]; 4800];
        buf[0] = [1.0, 1.0];
        st.process(&set, &mut buf, 1000.0, None);
        // a tap leaves a ring: one hundred frames is a full turn at 480 Hz
        let crossings = (1001..3000).filter(|&i| buf[i - 1][0] < 0.0 && buf[i][0] >= 0.0).count();
        assert!((19..=21).contains(&crossings), "{crossings}");
        assert!(buf[2000][0].abs() + buf[2025][0].abs() > 1e-4);
        let set = Set { reverb: Some(Reverb { secs: 1.0, high_hz: 8000.0, wet: 1.0 }), ..Default::default() };
        let mut st = State::new(48000.0);
        let mut buf = vec![[0.0f32; 2]; 96000];
        buf[0] = [1.0, 1.0];
        st.process(&set, &mut buf, 1000.0, None);
        let rms = |from: usize, to: usize| (buf[from..to].iter().map(|f| f[0] * f[0]).sum::<f32>() / (to - from) as f32).sqrt();
        let (early, late) = (rms(4000, 12000), rms(52000, 60000));
        assert!(early > 1e-3 && late < early * 0.1 && late > early * 0.0001, "{early} {late}");
        assert!(buf.iter().skip(3000).any(|f| (f[0] - f[1]).abs() > 1e-3));
        assert!(set.tail(48000.0, 1000.0) > 48000);
    }
}

#[cfg(test)]
mod tone_tests {
    use super::*;

    fn level(set: &Set, hz: f32) -> f32 {
        let sr = 48000.0;
        let mut st = State::new(sr);
        let mut buf: Vec<[f32; 2]> = (0..24000).map(|i| [(TAU * hz * i as f32 / sr).sin(); 2]).collect();
        st.process(set, &mut buf, 6000.0, None);
        let rms = (buf[12000..].iter().map(|f| f[0] * f[0]).sum::<f32>() / 12000.0).sqrt();
        20.0 * (rms * std::f32::consts::SQRT_2).log10()
    }

    #[test]
    fn the_three_bands_lift_and_cut_where_they_stand() {
        let band = |kind: u8, hz, db| Band { kind, hz, db, q: if kind == 1 { std::f32::consts::SQRT_2 } else { std::f32::consts::FRAC_1_SQRT_2 } };
        let low = Set { tone: Some([band(0, 200.0, -12.0), band(1, 1000.0, 0.0), band(2, 5000.0, 0.0)]), ..Default::default() };
        assert!((level(&low, 40.0) + 12.0).abs() < 1.0 && level(&low, 4000.0).abs() < 0.5);
        let mid = Set { tone: Some([band(0, 200.0, 0.0), band(1, 1000.0, 9.0), band(2, 5000.0, 0.0)]), ..Default::default() };
        assert!((level(&mid, 1000.0) - 9.0).abs() < 0.5 && level(&mid, 100.0).abs() < 0.5 && level(&mid, 9000.0).abs() < 0.5);
        let high = Set { tone: Some([band(0, 200.0, 0.0), band(1, 1000.0, 0.0), band(2, 5000.0, 6.0)]), ..Default::default() };
        assert!((level(&high, 16000.0) - 6.0).abs() < 1.0 && level(&high, 300.0).abs() < 0.5);
    }
}
