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

/// The compressor. Measured against the band maximiser: it is that unit at one of four fixed settings, with
/// the mix knob as the unit's own band mix. The settings are files in the user's own install; they are read
/// from there when it is known and never kept in this program. Without them the sound passes as it is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Squash {
    /// 1 to 4.
    pub kind: u8,
    /// 0 to 1.
    pub mix: f32,
}

type Sets = [Option<std::sync::Arc<Vec<f32>>>; 4];

fn squash_settings(kind: u8) -> Option<std::sync::Arc<Vec<f32>>> {
    static S: std::sync::Mutex<Option<Sets>> = std::sync::Mutex::new(None);
    let mut s = S.lock().unwrap();
    if s.is_none() {
        'roots: for root in crate::keys::roots() {
            for up in ["", "..", "../.."] {
                for sub in ["Plugins/Fruity/Generators/Harmless/Data/Compression", "Plugins/Fruity/Generators/Harmor/System/Compression"] {
                    let dir = root.join(up).join(sub);
                    if !dir.is_dir() {
                        continue;
                    }
                    let mut all: Sets = Default::default();
                    for (i, slot) in all.iter_mut().enumerate() {
                        *slot = crate::preset::read(&dir.join(format!("{}.fst", i + 1)))
                            .ok()
                            .and_then(|pre| crate::fx::Fx::decode(&pre.ident, &pre.state))
                            .filter(|fx| fx.kind == crate::fx::Kind::Maxi && fx.p.len() == crate::fxmaxi::LEN)
                            .map(|fx| std::sync::Arc::new(fx.p));
                    }
                    if all.iter().any(|a| a.is_some()) {
                        *s = Some(all);
                        break 'roots;
                    }
                }
            }
        }
    }
    s.as_ref().and_then(|all| all.get((kind as usize).wrapping_sub(1)).cloned().flatten())
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Set {
    pub echo: Option<Echo>,
    pub squash: Option<Squash>,
}

impl Set {
    pub fn any(&self) -> bool {
        self.echo.is_some() || self.squash.is_some()
    }

    /// How many frames late the sound comes out, for the player to make up.
    pub fn late(&self, sr: f32) -> usize {
        self.squash.and_then(|q| squash_settings(q.kind)).map_or(0, |p| crate::fxmaxi::latency(&p, sr))
    }

    /// Frames the effects still sound after their input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0;
        if let Some(e) = &self.echo {
            let repeats = if e.fb >= 0.999 { 200.0 } else if e.fb > 0.001 { (0.0005f32.ln() / e.fb.ln()).ceil().min(200.0) } else { 0.0 };
            n += (e.steps * step).max(1.0) * (repeats + 1.0) + sr * 0.021;
        }
        if self.squash.is_some() {
            n += sr * 0.5;
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
    /// The compressor: which kind its settings are for, the settings and the unit.
    squash: Option<(u8, Vec<f32>, Box<crate::fxmaxi::State>)>,
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

    /// The part that runs after everything else the synth carries.
    pub fn last(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32) {
        let Some(q) = &set.squash else { return };
        if self.squash.as_ref().is_none_or(|k| k.0 != q.kind) {
            let p = squash_settings(q.kind).map(|p| p.to_vec()).unwrap_or_default();
            self.squash = Some((q.kind, p, Box::default()));
        }
        let (_, p, st) = self.squash.as_mut().unwrap();
        if p.len() < crate::fxmaxi::LEN {
            return;
        }
        p[5] = q.mix.clamp(0.0, 1.0) * 1000.0;
        crate::fxmaxi::process(p, st, buf, &crate::fx::Ctx { sr: self.sr, step, clock: Default::default() });
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
