//! The effects the additive synth carries after its voices. They run once per channel on the sum of its
//! voices, and everything here was measured on renders of the original.

/// The echo. Measured on a short note: clean copies with nothing lost in the loop.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Echo {
    /// 1 both sides alike, 2 and 3 cross over from side to side.
    pub kind: u8,
    /// How loud the first echo is against the sound itself.
    pub wet: f32,
    /// What each further echo keeps of the one before.
    pub fb: f32,
    /// The echo time in sixteenth notes.
    pub steps: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Set {
    pub echo: Option<Echo>,
}

impl Set {
    pub fn any(&self) -> bool {
        self.echo.is_some()
    }

    /// Frames the effects still sound after their input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0;
        if let Some(e) = &self.echo {
            let repeats = if e.fb >= 0.999 { 200.0 } else if e.fb > 0.001 { (0.0005f32.ln() / e.fb.ln()).ceil().min(200.0) } else { 0.0 };
            n += (e.steps * step).max(1.0) * (repeats + 1.0) + sr * 0.021;
        }
        n as usize + 64
    }
}

#[derive(Default)]
pub struct State {
    sr: f32,
    /// The echo's two lines, the short lines that hold one side back before it, and where they stand.
    line: [Vec<f32>; 2],
    at: usize,
    lead: Vec<f32>,
    lead_at: usize,
}

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, ..Default::default() }
    }

    /// `step` is frames per sixteenth note.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32) {
        if let Some(e) = &set.echo {
            self.echo(e, buf, step);
        }
    }

    /// Measured: the echo comes a whole number of frames late and every repeat is the one before times the
    /// feedback, to the sample. The second kind holds the left side back a thousandth of a second before the
    /// lines and the third the right side a fiftieth; in both, each line feeds the other, so the late side
    /// changes with every repeat.
    fn echo(&mut self, e: &Echo, buf: &mut [[f32; 2]], step: f32) {
        let len = ((e.steps * step).round() as usize).max(1);
        if self.line[0].len() != len {
            self.line = [vec![0.0; len], vec![0.0; len]];
            self.at = 0;
        }
        let (held, by) = match e.kind {
            2 => (0, (self.sr * 0.001) as usize),
            3 => (1, (self.sr * 0.02) as usize),
            _ => (0, 0),
        };
        if self.lead.len() != by {
            self.lead = vec![0.0; by];
            self.lead_at = 0;
        }
        let cross = e.kind >= 2;
        for f in buf.iter_mut() {
            let mut into = *f;
            if by > 0 {
                into[held] = std::mem::replace(&mut self.lead[self.lead_at], f[held]);
                self.lead_at = (self.lead_at + 1) % by;
            }
            let out = [self.line[0][self.at], self.line[1][self.at]];
            let back = if cross { [out[1], out[0]] } else { out };
            for side in 0..2 {
                let v = into[side] + back[side] * e.fb;
                self.line[side][self.at] = if v.abs() > 1e-20 { v } else { 0.0 };
                f[side] += out[side] * e.wet;
            }
            self.at = (self.at + 1) % len;
        }
    }
}
