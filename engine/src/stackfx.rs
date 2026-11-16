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

/// The chorus. Measured on long held notes by fitting the render as a sum of copies of the dry sound: every
/// voice is one copy read from a line with straight-line interpolation, its lateness swinging between a
/// nearest and a farthest point along two parabolas. The swings run from the moment the song starts, not
/// from the note. At full mix nothing of the dry sound is left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Chorus {
    /// 1 to 9.
    pub kind: u8,
    /// 0 to 1.
    pub mix: f32,
}

/// One voice: nearest point and swing in frames at 44.1 kHz, the time of one swing in seconds, where in the
/// swing it starts, and how much of it each side gets.
#[derive(Debug, Clone, Copy)]
struct Voice {
    near: f32,
    swing: f32,
    secs: f64,
    from: f64,
    gain: [f32; 2],
}

/// Rungs of voices whose swings grow by `step`, one set for each side half a swing apart. `own` is what a
/// side hears of its set and `other` what it hears of rung one of the other side's, growing with the rung.
fn rungs(n: usize, step: f32, secs: f64, own: f32, other: f32) -> Vec<Voice> {
    let mut v = Vec::with_capacity(n * 2);
    for m in 1..=n {
        let o = other * m as f32;
        v.push(Voice { near: 8.0, swing: 20.0 + step * m as f32, secs, from: 0.25, gain: [own, o] });
        v.push(Voice { near: 8.0, swing: 20.0 + step * m as f32, secs, from: 0.75, gain: [o, own] });
    }
    v
}

fn voices(kind: u8) -> Vec<Voice> {
    let one = |near: f32, swing: f32, secs: f64, from: f64, l: f32, r: f32| Voice { near, swing, secs, from, gain: [l, r] };
    match kind {
        1 => CLASSIC.iter().map(|v| one(v.0, v.1, v.2, v.3, v.4, v.5)).collect(),
        2 => THICK.iter().map(|v| one(v.0, v.1, v.2, v.3, v.4, v.5)).collect(),
        3 => DEEP.iter().map(|v| one(v.0, v.1, v.2, v.3, v.4, v.5)).collect(),
        4 => rungs(6, 99.0489, 3.799015, 0.388, 0.0192),
        5 => vec![one(8.0, 101.9425, 8.467358, 0.25, 0.823, 0.823), one(8.0, 101.9425, 8.467358, 0.75, 0.823, 0.823)],
        6 => rungs(2, 24.3733, 1.290364, 0.823, 0.0),
        7 => vec![one(8.0, 42.582, 0.159435, 0.25, 0.9485, -0.163), one(8.0, 42.582, 0.159435, 0.75, -0.163, 0.9485)],
        8 => rungs(9, 5.0706, 10.406192, 0.258, -0.01478),
        _ => rungs(7, 1.4201, 0.087464, 0.39, 0.0),
    }
}

type Row = (f32, f32, f64, f64, f32, f32);
/// The first kind: four rungs for each side, every voice with its own nearest point, time and start.
const CLASSIC: [Row; 8] = [
    (8.00004, 146.54527, 137.75328, 0.375, 0.5267, -0.0329),
    (37.16405, 273.09105, 2.52781, 0.5, 0.5267, -0.0658),
    (57.72878, 399.63684, 3.57204, 0.625, 0.5267, -0.0988),
    (66.67339, 526.18232, 5.42965, 0.75, 0.5267, -0.1317),
    (17.01486, 146.54559, 1.48138, 0.875, -0.0329, 0.5267),
    (38.04252, 273.09126, 2.46942, 0.0, -0.0658, 0.5267),
    (53.15574, 399.63689, 3.87937, 0.125, -0.0988, 0.5267),
    (67.22599, 526.18247, 5.38491, 0.25, -0.1317, 0.5267),
];
/// The second kind: five rungs for each side, the starts a tenth of a swing apart, every voice with its own time.
const THICK: [Row; 10] = [
    (8.0, 457.0, 2.929422, 0.35, 0.4082, 0.0434),
    (8.0, 894.0, 532.196349, 0.45, 0.4082, 0.0867),
    (8.0, 1331.0, 7.812574, 0.55, 0.4082, 0.1301),
    (8.0, 1768.0, 10.020738, 0.65, 0.4082, 0.1735),
    (8.0, 2205.0, 14.428378, 0.75, 0.4082, 0.2168),
    (8.0, 457.0, 2.620447, 0.85, 0.0434, 0.4082),
    (8.0, 894.0, 5.502969, 0.95, 0.0867, 0.4082),
    (8.0, 1331.0, 8.637831, 0.05, 0.1301, 0.4082),
    (8.0, 1768.0, 9.609428, 0.15, 0.1735, 0.4082),
    (8.0, 2205.0, 15.043489, 0.25, 0.2168, 0.4082),
];
/// The third kind: seven rungs for each side, the starts a fourteenth of a swing apart, every voice with its own
/// near point and its own time.
const DEEP: [Row; 14] = [
    (626.229, 92.311, 3.06929, 0.32145, 0.3284, 0.0198),
    (1344.53, 164.623, 5.09876, 0.39286, 0.3284, 0.0396),
    (1804.07, 236.917, 8.20347, 0.4643, 0.3284, 0.0594),
    (25.853, 309.258, 993.8575, 0.53571, 0.3284, 0.0792),
    (3283.701, 381.55, 12.09684, 0.60716, 0.3284, 0.099),
    (4080.589, 453.861, 13.89524, 0.67858, 0.3284, 0.1188),
    (4123.743, 526.201, 18.59685, 0.75001, 0.3284, 0.1386),
    (594.0, 92.305, 3.23582, 0.82143, 0.0198, 0.3284),
    (1418.416, 164.603, 4.83309, 0.89286, 0.0396, 0.3284),
    (1694.977, 236.917, 8.73153, 0.96428, 0.0594, 0.3284),
    (2678.665, 309.245, 9.6151, 0.0357, 0.0792, 0.3284),
    (32.067, 381.572, 1232.8783, 0.10714, 0.099, 0.3284),
    (4271.27, 453.861, 13.274, 0.17855, 0.1188, 0.3284),
    (4005.209, 526.167, 19.14895, 0.25001, 0.1386, 0.3284),
];

/// The room. Measured by its answer to a click: it is the room unit of the effects rack at fixed settings,
/// one set for each kind, added to the untouched sound. The volume knob climbs as (11^v - 1) / 10.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Room {
    /// 1 to 6.
    pub kind: u8,
    /// 0 to 1.
    pub vol: f32,
}

/// For each kind: low cut, high cut, wait, size, spread, decay, damping, bass, crossover, separation, early
/// share, and what the unit's output is multiplied by.
const ROOMS: [[f32; 12]; 6] = [
    [75.0, 40.0, 0.0, 40.0, 100.0, 15.0, 40.0, 100.0, 500.0, 0.0, 0.0, 3.126],
    [19.0, 56.0, 18.0, 40.0, 100.0, 18.0, 85.0, 184.0, 1590.0, 0.0, 0.0, 3.244],
    [36.0, 20.0, 27.0, 90.0, 100.0, 57.0, 21.0, 65.0, 885.0, 1.0, 0.0, 3.288],
    [375.0, 46.0, 31.0, 60.0, 65.0, 21.0, 49.0, 27.0, 25.0, 0.0, 2.0, 3.106],
    [99.0, 40.0, 0.0, 35.0, 100.0, 6.0, 33.0, 94.0, 110.0, 0.0, 4.0, 3.12],
    [96.0, 41.0, 0.0, 17.0, 100.0, 5.0, 41.0, 110.0, 647.0, 0.0, 2.0, 3.065],
];

fn room_unit(kind: u8, sr: f32) -> crate::fx::Unit {
    let row = &ROOMS[(kind as usize).clamp(1, 6) - 1];
    let mut fx = crate::fx::Fx::new(crate::fx::Kind::Reverb);
    fx.p[..10].copy_from_slice(&row[..10]);
    fx.p[10] = 0.0;
    fx.p[11] = row[10];
    fx.p[12] = 100.0;
    crate::fx::Unit::new(fx, sr)
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
    pub chorus: Option<Chorus>,
    pub echo: Option<Echo>,
    pub room: Option<Room>,
    pub squash: Option<Squash>,
}

impl Set {
    pub fn any(&self) -> bool {
        self.chorus.is_some() || self.echo.is_some() || self.room.is_some() || self.squash.is_some()
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
        if self.chorus.is_some() {
            n += sr * 0.1;
        }
        if let Some(r) = &self.room {
            n += sr * (0.5 + 0.2 * ROOMS[(r.kind as usize).clamp(1, 6) - 1][5]);
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
    /// The chorus: which kind the voices are for, the voices, the two lines and where they stand, and how many
    /// frames have gone by.
    voices: (u8, Vec<Voice>),
    cho: [Vec<f32>; 2],
    cho_at: usize,
    frame: u64,
    /// The room: its kind, the unit and a block to run it on.
    room: Option<(u8, Box<crate::fx::Unit>)>,
    wet: Vec<[f32; 2]>,
}

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, ..Default::default() }
    }

    /// `step` is frames per sixteenth note.
    /// `frame` is how far the song has run, when the caller knows; the chorus swings by it.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32, frame: Option<u64>) {
        if let Some(f) = frame {
            self.frame = f;
        }
        if let Some(c) = &set.chorus {
            self.chorus(c, buf);
        }
        self.frame += buf.len() as u64;
        if let Some(e) = &set.echo {
            self.echo(e, buf, step);
        }
    }

    /// The part that runs after everything else the synth carries.
    pub fn last(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32) {
        if let Some(r) = &set.room {
            if self.room.as_ref().is_none_or(|k| k.0 != r.kind) {
                self.room = Some((r.kind, Box::new(room_unit(r.kind, self.sr))));
            }
            let unit = &mut self.room.as_mut().unwrap().1;
            self.wet.clear();
            self.wet.extend_from_slice(buf);
            unit.process(&mut self.wet, &crate::fx::Ctx { sr: self.sr, step, clock: Default::default() });
            let g = ROOMS[(r.kind as usize).clamp(1, 6) - 1][11] * (11f32.powf(r.vol.clamp(0.0, 1.0)) - 1.0) / 10.0;
            for (f, w) in buf.iter_mut().zip(&self.wet) {
                f[0] += w[0] * g;
                f[1] += w[1] * g;
            }
        }
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

    /// The mix knob bends: the voices get (1 - 5^-m) / 0.8 and the dry sound the same curve read backwards.
    fn chorus(&mut self, c: &Chorus, buf: &mut [[f32; 2]]) {
        if self.voices.0 != c.kind || self.cho[0].is_empty() {
            self.voices = (c.kind, voices(c.kind));
            let far = self.voices.1.iter().map(|v| v.near + v.swing).fold(0.0f32, f32::max);
            let len = ((far * self.sr / 44100.0) as usize + 8).next_power_of_two();
            self.cho = [vec![0.0; len], vec![0.0; len]];
            self.cho_at = 0;
        }
        let bend = |m: f32| (1.0 - 5f32.powf(-m.clamp(0.0, 1.0))) / 0.8;
        let (wet, dry) = (bend(c.mix), bend(1.0 - c.mix));
        let mask = self.cho[0].len() - 1;
        let scale = self.sr / 44100.0;
        for (i, f) in buf.iter_mut().enumerate() {
            let t = (self.frame + i as u64) as f64 / self.sr as f64;
            self.cho[0][self.cho_at] = f[0];
            self.cho[1][self.cho_at] = f[1];
            let mut out = [f[0] * dry, f[1] * dry];
            for v in &self.voices.1 {
                let x = ((t / v.secs + v.from + 0.5).fract() * 2.0 - 1.0) as f32;
                let late = (v.near + v.swing * (1.0 - 4.0 * x * (1.0 - x.abs())) * 0.5) * scale;
                let whole = late as usize;
                let part = late - whole as f32;
                let a = (self.cho_at + mask + 1 - whole) & mask;
                let b = (a + mask) & mask;
                for side in 0..2 {
                    let line = &self.cho[side];
                    out[side] += (line[a] + (line[b] - line[a]) * part) * v.gain[side] * wet;
                }
            }
            *f = out;
            self.cho_at = (self.cho_at + 1) & mask;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chorus_kind_has_voices_on_both_sides() {
        for kind in 1..=9u8 {
            let v = voices(kind);
            assert!(!v.is_empty(), "kind {kind}");
            for side in 0..2 {
                let own: f32 = v.iter().map(|x| x.gain[side] * x.gain[side]).sum();
                assert!(own > 0.3 && own < 1.5, "kind {kind} side {side}: {own}");
            }
            assert!(v.iter().all(|x| x.near >= 1.0 && x.swing > 0.0 && x.secs > 0.0));
        }
    }
}
