//! The effects some instruments carry after their voices: a tempo-locked stereo echo, a chorus and a
//! sixteen-step gate. They run once per channel, on the sum of its voices.

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
}

impl Set {
    pub fn any(&self) -> bool {
        self.delay.is_some() || self.chorus.is_some() || self.gate.is_some()
    }

    /// Frames the effects still sound after their input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0;
        if let Some(d) = &self.delay {
            let time = d.left.max(d.right) * step;
            let repeats = if d.fb > 0.001 { (0.001f32.ln() / d.fb.ln()).ceil().min(60.0) } else { 1.0 };
            n += time * (repeats + 1.0);
        }
        if self.chorus.is_some() {
            n += sr * 0.05;
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
}

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, line: Vec::new(), at: 0, sweep: 0.0, lp: [0.0; 2], hp: [0.0; 2], time: [0.0; 2], cho: Vec::new(), cat: 0, cph: 0.0, open: 1.0, clock: 0.0 }
    }

    /// Run the effects over `buf`. `step` is frames per sixteenth note; `song` is where the song stands in
    /// sixteenth notes while it plays, so the gate keeps its place in the bar.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32, song: Option<f64>) {
        let sr = self.sr;
        if let Some(d) = &set.delay {
            let n = (sr * 4.0) as usize;
            if self.line.len() != n {
                self.line = vec![[0.0; 2]; n];
                self.at = 0;
                self.time = [0.0; 2];
            }
            let want = [(d.left * step).clamp(8.0, n as f32 - 64.0), (d.right * step).clamp(8.0, n as f32 - 64.0)];
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_echo_comes_back_on_time_and_fades() {
        let set = Set { delay: Some(Delay { left: 2.0, right: 4.0, fb: 0.5, wobble: 0.0, low_hz: 0.0, high_hz: 20000.0, wet: 1.0 }), ..Default::default() };
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
}
