//! Built-in instruments. Like the effects (`fx.rs`), each one is our own sound source that reads and writes the
//! stored settings of the matching generator in a project file. Settings stay in file units; the voice code
//! converts them. Field meanings come from comparing many project files and are approximations until measured
//! against the reference (docs/PARITY.md, "Instruments").

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Three mixed oscillators.
    Osc3,
}

pub struct Param {
    pub name: &'static str,
    /// Byte offset in the stored block.
    pub off: usize,
    /// Bytes the value takes (1 or 4).
    pub width: u8,
    pub min: i32,
    pub max: i32,
    pub def: i32,
}

pub struct Def {
    pub kind: Kind,
    /// Identifier used in project files.
    pub ident: &'static str,
    /// Our own name for the instrument.
    pub title: &'static str,
    pub size: usize,
    pub fixed: &'static [(usize, i32)],
    pub params: &'static [Param],
}

const fn p(name: &'static str, off: usize, min: i32, max: i32, def: i32) -> Param {
    Param { name, off, width: 4, min, max, def }
}

const fn b(name: &'static str, off: usize) -> Param {
    Param { name, off, width: 1, min: 0, max: 1, def: 0 }
}

/// Settings per oscillator of the three-oscillator instrument, in table order.
pub const OSC_FIELDS: usize = 7;
pub const SHAPES: [&str; 7] = ["Sine", "Triangle", "Square", "Saw", "Round saw", "Noise", "Custom"];

pub static DEFS: &[Def] = &[Def {
    kind: Kind::Osc3,
    ident: "3x Osc",
    title: "Triple Osc",
    size: 92,
    fixed: &[(0, 12)],
    params: &[
        p("Osc 1 pan", 4, -64, 64, 0),
        p("Osc 1 shape", 8, 0, 6, 3),
        p("Osc 1 coarse", 12, -24, 24, 0),
        p("Osc 1 fine", 16, -100, 100, 0),
        p("Osc 1 phase", 20, 0, 64, 0),
        p("Osc 1 detune", 24, 0, 64, 0),
        p("Osc 2 level", 28, 0, 128, 0),
        p("Osc 2 pan", 32, -64, 64, 0),
        p("Osc 2 shape", 36, 0, 6, 3),
        p("Osc 2 coarse", 40, -24, 24, -12),
        p("Osc 2 fine", 44, -100, 100, 0),
        p("Osc 2 phase", 48, 0, 64, 0),
        p("Osc 2 detune", 52, 0, 64, 0),
        p("Osc 3 level", 56, 0, 128, 0),
        p("Osc 3 pan", 60, -64, 64, 0),
        p("Osc 3 shape", 64, 0, 6, 3),
        p("Osc 3 coarse", 68, -24, 24, -24),
        p("Osc 3 fine", 72, -100, 100, 0),
        p("Osc 3 phase", 76, 0, 64, 0),
        p("Osc 3 detune", 80, 0, 64, 0),
        p("Phase random", 84, 0, 64, 0),
        b("Osc 1 invert", 88),
        b("Osc 2 invert", 89),
        b("Osc 3 invert", 90),
        b("Osc 3 AM", 91),
    ],
}];

pub fn def(kind: Kind) -> &'static Def {
    DEFS.iter().find(|d| d.kind == kind).expect("every kind has a table entry")
}

pub fn title_for(ident: &str) -> Option<&'static str> {
    DEFS.iter().find(|d| d.ident == ident).map(|d| d.title)
}

/// One instrument's settings, in file units, in the order of its table.
#[derive(Debug, Clone, PartialEq)]
pub struct Gen {
    pub kind: Kind,
    pub p: Vec<f32>,
}

impl Gen {
    pub fn new(kind: Kind) -> Self {
        Self { kind, p: def(kind).params.iter().map(|p| p.def as f32).collect() }
    }

    pub fn decode(ident: &str, state: &[u8]) -> Option<Self> {
        let d = DEFS.iter().find(|d| d.ident == ident)?;
        if state.is_empty() {
            return Some(Self::new(d.kind));
        }
        if d.kind == Kind::Osc3 && state.len() == 44 {
            return Some(Self::old_osc3(state));
        }
        if state.len() < d.size {
            return None;
        }
        let p = d
            .params
            .iter()
            .map(|p| {
                let v = if p.width == 1 { state[p.off] as i32 } else { i32::from_le_bytes(state[p.off..p.off + 4].try_into().unwrap()) };
                v.clamp(p.min, p.max) as f32
            })
            .collect();
        Some(Self { kind: d.kind, p })
    }

    /// The first layout of the three-oscillator block: shape, coarse and fine of oscillator 1, then mix level,
    /// shape, coarse and fine of the other two.
    fn old_osc3(state: &[u8]) -> Self {
        let mut g = Self::new(Kind::Osc3);
        let at = |i: usize| i32::from_le_bytes(state[i * 4..i * 4 + 4].try_into().unwrap());
        let d = def(Kind::Osc3);
        let mut set = |name: &str, v: i32| {
            if let Some(k) = d.params.iter().position(|p| p.name == name) {
                g.p[k] = v.clamp(d.params[k].min, d.params[k].max) as f32;
            }
        };
        set("Osc 1 shape", at(0));
        set("Osc 2 level", at(3));
        set("Osc 2 shape", at(4));
        set("Osc 3 level", at(7));
        set("Osc 3 shape", at(8));
        set("Osc 1 coarse", at(1));
        set("Osc 1 fine", at(2));
        set("Osc 2 coarse", at(5));
        set("Osc 2 fine", at(6));
        set("Osc 3 coarse", at(9));
        set("Osc 3 fine", at(10));
        g
    }

    /// The stored block. Bytes we do not understand are kept from `base`, and a value that still reads the same
    /// as `base` is left exactly as it was.
    pub fn encode(&self, base: Option<&[u8]>) -> Vec<u8> {
        let d = def(self.kind);
        let mut out = match base {
            Some(b) if b.len() >= d.size => b.to_vec(),
            _ => {
                let mut v = vec![0u8; d.size];
                for &(off, val) in d.fixed {
                    v[off..off + 4].copy_from_slice(&val.to_le_bytes());
                }
                v
            }
        };
        for (p, &v) in d.params.iter().zip(&self.p) {
            let v = (v.round() as i32).clamp(p.min, p.max);
            if p.width == 1 {
                if (out[p.off] as i32).clamp(p.min, p.max) != v {
                    out[p.off] = v as u8;
                }
            } else {
                let was = i32::from_le_bytes(out[p.off..p.off + 4].try_into().unwrap());
                if was.clamp(p.min, p.max) != v {
                    out[p.off..p.off + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        out
    }

    /// What a voice needs to sound this instrument.
    pub fn tone(&self) -> Tone {
        match self.kind {
            Kind::Osc3 => {
                let mut t = Tone::default();
                for k in 0..3 {
                    let f = &self.p[k * OSC_FIELDS..];
                    t.osc[k] = Osc {
                        pan: (f[0] / 64.0).clamp(-1.0, 1.0),
                        shape: f[1] as u8,
                        ratio: 2f32.powf((f[2] + f[3] / 100.0) / 12.0),
                        phase: f[4] / 64.0,
                        // the knob spreads the two sides by up to half a semitone
                        spread: 2f32.powf(f[5] / 64.0 * 0.5 / 12.0),
                        invert: self.p[21 + k] != 0.0,
                    };
                }
                t.mix2 = self.p[6] / 128.0;
                t.mix3 = self.p[13] / 128.0;
                t.random = self.p[20] / 64.0;
                t.am = self.p[24] != 0.0;
                t
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Osc {
    pub pan: f32,
    pub shape: u8,
    /// Frequency against the note.
    pub ratio: f32,
    /// Start phase, 0..1.
    pub phase: f32,
    /// Frequency ratio between the right and left copies.
    pub spread: f32,
    pub invert: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Tone {
    pub osc: [Osc; 3],
    pub mix2: f32,
    pub mix3: f32,
    pub random: f32,
    pub am: bool,
}

/// Running state of one sounding note.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToneState {
    ph: [[f32; 2]; 3],
    seed: u32,
    hold: [f32; 3],
}

#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
fn wave(shape: u8, t: f32, dt: f32) -> f32 {
    match shape {
        1 => {
            let x = t + 0.25;
            let x = x - x.floor();
            1.0 - 4.0 * (x - 0.5).abs()
        }
        2 => {
            let mut v = if t < 0.5 { 1.0 } else { -1.0 };
            v += blep(t, dt);
            let t2 = t + 0.5;
            v -= blep(t2 - t2.floor(), dt);
            v
        }
        3 => 2.0 * t - 1.0 - blep(t, dt),
        4 => {
            // a saw with the edge rounded off: the rising half of a parabola
            let x = 2.0 * t - 1.0;
            x * (2.0 - x.abs()) - blep(t, dt)
        }
        _ => (t * TAU).sin(),
    }
}

impl ToneState {
    pub fn start(tone: &Tone, seed: u32) -> Self {
        let mut s = Self { seed: seed | 1, ..Default::default() };
        for k in 0..3 {
            let r = if tone.random > 0.0 { s.rand() * 0.5 + 0.5 } else { 0.0 };
            let ph = (tone.osc[k].phase + r * tone.random).fract();
            s.ph[k] = [ph, ph];
        }
        s
    }

    #[inline]
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// One frame. `dphase` is the note's cycles per frame.
    #[inline]
    pub fn frame(&mut self, tone: &Tone, dphase: f32) -> [f32; 2] {
        let mut o = [[0.0f32; 2]; 3];
        for k in 0..3 {
            let osc = &tone.osc[k];
            let base = dphase * osc.ratio;
            if osc.shape == 5 {
                // noise, stepped at the oscillator's pitch so the note still has a colour
                let p = &mut self.ph[k][0];
                *p += (base * 64.0).min(1.0);
                if *p >= 1.0 {
                    *p -= 1.0;
                    self.hold[k] = self.rand();
                }
                o[k] = [self.hold[k]; 2];
            } else {
                let half = osc.spread.sqrt();
                for (c, d) in [(0usize, base / half), (1usize, base * half)] {
                    let d = d.min(0.49);
                    let p = &mut self.ph[k][c];
                    o[k][c] = wave(osc.shape, *p, d);
                    *p += d;
                    if *p >= 1.0 {
                        *p -= 1.0;
                    }
                }
            }
            if osc.invert {
                o[k] = [-o[k][0], -o[k][1]];
            }
            let (l, r) = (1.0 - osc.pan.max(0.0), 1.0 + osc.pan.min(0.0));
            o[k][0] *= l;
            o[k][1] *= r;
        }
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let a = o[0][c] + (o[1][c] - o[0][c]) * tone.mix2;
            out[c] = if tone.am {
                // the third oscillator shapes the level of the other two
                a * (1.0 + (o[2][c] - 1.0) * tone.mix3)
            } else {
                a + (o[2][c] - a) * tone.mix3
            } * 0.4;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_round_trips() {
        let mut g = Gen::new(Kind::Osc3);
        let fresh = g.encode(None);
        assert_eq!(fresh.len(), 92);
        assert_eq!(Gen::decode("3x Osc", &fresh).unwrap(), g);
        g.p[1] = 2.0;
        g.p[6] = 64.0;
        g.p[24] = 1.0;
        let mut odd = fresh.clone();
        odd.extend_from_slice(&[9; 368]);
        let out = g.encode(Some(&odd));
        assert_eq!(out.len(), 460);
        assert_eq!(&out[92..], &odd[92..]);
        assert_eq!(Gen::decode("3x Osc", &out).unwrap(), g);
    }

    #[test]
    fn oscillators_sound_at_pitch() {
        let mut g = Gen::new(Kind::Osc3);
        g.p[1] = 0.0; // sine
        let tone = g.tone();
        let mut st = ToneState::start(&tone, 7);
        let dphase = 441.0 / 44100.0;
        let x: Vec<f32> = (0..44100).map(|_| st.frame(&tone, dphase)[0]).collect();
        let crossings = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((440..=442).contains(&crossings), "{crossings}");
        // every shape stays in range
        for shape in 0..7 {
            g.p[1] = shape as f32;
            g.p[6] = 64.0;
            g.p[13] = 40.0;
            g.p[5] = 30.0;
            let tone = g.tone();
            let mut st = ToneState::start(&tone, 3);
            let peak = (0..20000).map(|_| st.frame(&tone, 0.01)).fold(0.0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()));
            assert!(peak > 0.05 && peak < 1.0, "shape {shape} peak {peak}");
        }
    }
}
