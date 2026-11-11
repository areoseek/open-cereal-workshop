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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Set {
    pub delay: Option<Delay>,
    pub chorus: Option<Chorus>,
    pub gate: Option<Gate>,
    pub body: Option<Body>,
    pub reverb: Option<Reverb>,
}

impl Set {
    pub fn any(&self) -> bool {
        self.delay.is_some() || self.chorus.is_some() || self.gate.is_some() || self.body.is_some() || self.reverb.is_some()
    }

    /// Frames the effects still sound after their input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0;
        if let Some(d) = &self.delay {
            let time = if d.free > 0.0 { d.free * sr } else { d.left.max(d.right) * step };
            let repeats = if d.fb > 0.001 { (0.001f32.ln() / d.fb.ln()).ceil().min(60.0) } else { 1.0 };
            n += time * (repeats + 1.0);
        }
        if self.chorus.is_some() {
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
}

/// Comb and all-pass lengths of the room at 44.1 kHz; the right side is a little longer.
const COMBS: [usize; 4] = [1557, 1617, 1491, 1422];
const PASSES: [usize; 2] = [225, 556];

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, line: Vec::new(), at: 0, sweep: 0.0, lp: [0.0; 2], hp: [0.0; 2], time: [0.0; 2], cho: Vec::new(), cat: 0, cph: 0.0, open: 1.0, clock: 0.0, body: [[[0.0; 2]; 2]; 8], combs: Vec::new(), passes: Vec::new() }
    }

    /// Run the effects over `buf`. `step` is frames per sixteenth note; `song` is where the song stands in
    /// sixteenth notes while it plays, so the gate keeps its place in the bar.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        let sr = self.sr;
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
