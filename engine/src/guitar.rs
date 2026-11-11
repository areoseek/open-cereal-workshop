//! An electric guitar voice: plucked strings read through a pickup, then an amplifier, a tone stack and a cabinet.
//!
//! Settings are in the order of `synth::GUITAR`. The layout was placed from value ranges and the public manual's
//! list of controls; every scale here is an estimate. The amplifier works per note (three strings share one in the
//! power chord mode), so chords played as separate notes do not grind against each other the way one amp would
//! make them. The effects unit, the feedback loop, the second pitch wheel and the strummed and gliding play modes
//! are not built.

use std::f32::consts::PI;

/// Cycles per second of key 60.
const C5: f32 = 261.6256;

#[derive(Debug, Clone, Default)]
struct StringLine {
    line: Vec<f32>,
    pos: usize,
    frac: f32,
    last: f32,
    keep: f32,
    /// Frames to wait before this string is struck.
    wait: u32,
    /// How far along the loop the pickup sits, in samples.
    tap: [usize; 2],
}

#[derive(Debug, Clone, Default)]
pub struct Voice {
    strings: Vec<StringLine>,
    sr: f32,
    seed: u32,
    gain: f32,
    /// One-sided limit for the string's swing (the slap), 0 for none.
    slap: f32,
    coils: u8,
    click: f32,
    click_by: f32,
    click_z: f32,
    fade: f32,
    fade_by: f32,
    hold: bool,
    // pickup tone
    bright: f32,
    bz: f32,
    // body resonance
    fg: f32,
    fz: [f32; 2],
    // amplifier
    amp: u8,
    pre: f32,
    pres: f32,
    pz: f32,
    bp: [[f32; 2]; 3],
    bpg: f32,
    eq: [f32; 3],
    ez: [f32; 2],
    ec: [f32; 2],
    // cabinet
    cab: u8,
    cz: [f32; 4],
    cc: [f32; 3],
    dc: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / 8_388_608.0 - 1.0
}

fn pole(hz: f32, sr: f32) -> f32 {
    1.0 - (-2.0 * PI * hz / sr).exp()
}

impl Voice {
    pub fn start(p: &[f32], sr: f32, key: u8, vel: f32, dphase: f32, seed: u32) -> Self {
        let v = |i: usize| p.get(i).copied().unwrap_or(0.0);
        let u = |i: usize| (v(i) / 65535.0).clamp(0.0, 1.0);
        let mut seed = seed | 1;
        let kind = v(0) as usize;
        // per string type: how much of the strike is noise, how bright it is, where along the string it lands
        let (noise, bright, at) = [(1.0, 1.0, 0.5), (0.25, 0.6, 0.13), (0.4, 0.8, 0.2), (0.15, 0.45, 0.3), (0.6, 1.0, 0.08), (0.2, 0.35, 0.18), (0.05, 0.2, 0.4)][kind.min(6)];
        let damp = (u(7) * (1.0 - u(8) * vel)).clamp(0.0, 1.0);
        let t60 = 0.12 + (1.0 - damp).powi(2) * 7.0;
        let speed = u(21);
        let mode = v(20) as u8;
        let base = (dphase * sr).clamp(20.0, sr * 0.4);
        let mut notes = vec![0.0f32];
        if mode == 2 {
            // power chords: root, fifth (fourth in the low half) and octave
            notes = vec![0.0, if key >= 60 { 7.0 } else { 5.0 }, 12.0];
        }
        let strings = notes
            .iter()
            .enumerate()
            .map(|(k, semis)| {
                let hz = (base * 2f32.powf(semis / 12.0)).min(sr * 0.4);
                let len = sr / hz - 0.5;
                let n = (len.floor() as usize).max(4);
                let peak = ((n as f32 * at) as usize).clamp(1, n - 1);
                let c = 0.05 + bright * 0.95;
                let mut z = 0.0;
                let mut line: Vec<f32> = (0..n)
                    .map(|i| {
                        // a string pulled to a point: two straight runs
                        let tri = if i < peak { i as f32 / peak as f32 } else { (n - i) as f32 / (n - peak) as f32 };
                        let x = (tri * 2.0 - 1.0) * (1.0 - noise) + rnd(&mut seed) * noise;
                        z += (x - z) * c;
                        z
                    })
                    .collect();
                let mean = line.iter().sum::<f32>() / n as f32;
                let top = line.iter().fold(1e-6f32, |m, x| m.max((x - mean).abs()));
                line.iter_mut().for_each(|x| *x = (*x - mean) / top);
                // lower strings ring longer, as on the instrument
                let trips = t60 * hz.sqrt() * C5.sqrt();
                StringLine {
                    line,
                    pos: 0,
                    frac: len - len.floor(),
                    last: 0.0,
                    keep: 0.001f32.powf(1.0 / trips.max(1.0)),
                    wait: (k as f32 * (0.004 + speed * 0.05) * sr) as u32,
                    tap: [((n as f32 * (0.08 + 0.1)) as usize).max(1), ((n as f32 * (0.16 + 0.1)) as usize).max(2)],
                }
            })
            .collect();
        let amp = v(9) as u8;
        let drive = u(10);
        let db = |x: f32| 10f32.powf((x - 0.5) * 24.0 / 20.0);
        let cab = v(16) as u8;
        // cabinet: low cut, then two low-pass poles at the speaker's top end
        let (low, top) = [(20.0, 18000.0), (95.0, 4200.0), (70.0, 3200.0), (110.0, 6500.0)][(cab as usize).min(3)];
        Voice {
            strings,
            sr,
            seed,
            gain: (1.0 - u(6) * (1.0 - vel)) * 0.5,
            slap: u(3) * vel,
            coils: v(1) as u8,
            click: u(4) * vel * 0.6,
            click_by: (-1.0 / (0.008 * sr)).exp(),
            click_z: 0.0,
            fade: 1.0,
            fade_by: 1.0,
            hold: v(22) != 0.0,
            bright: pole(700.0 * 2f32.powf(u(5) * 4.0), sr),
            bz: 0.0,
            fg: (PI * (250.0 * 2f32.powf(u(2) * 3.5)).min(sr * 0.4) / sr).tan(),
            fz: [0.0; 2],
            amp,
            pre: if amp == 0 { drive * 2.0 } else { 1.0 + drive * drive * 60.0 },
            pres: u(11),
            pz: 0.0,
            bp: [[0.0; 2]; 3],
            bpg: (PI * (300.0 * 2f32.powf(u(11) * 3.5)).min(sr * 0.4) / sr).tan(),
            eq: [db(u(13)), db(u(14)), db(u(15))],
            ez: [0.0; 2],
            ec: [pole(200.0, sr), pole(2500.0, sr)],
            cab,
            cz: [0.0; 4],
            cc: [pole(low, sr), pole(top, sr), pole(top * 1.4, sr)],
            dc: 0.0,
        }
    }

    /// Let the note go; gives the seconds of sound still to come.
    pub fn release(&mut self) -> f32 {
        if self.hold {
            let s = &self.strings[0];
            let per_sec = s.keep.ln() * self.sr / s.line.len() as f32;
            (0.001f32.ln() / per_sec.min(-1e-6)).min(8.0)
        } else {
            // a hand on the strings
            self.fade_by = (-1.0 / (0.02 * self.sr)).exp();
            0.15
        }
    }

    fn eq(&mut self, x: f32) -> f32 {
        self.ez[0] += (x - self.ez[0]) * self.ec[0];
        self.ez[1] += (x - self.ez[1]) * self.ec[1];
        let (low, high) = (self.ez[0], x - self.ez[1]);
        low * self.eq[0] + (x - low - high) * self.eq[1] + high * self.eq[2]
    }

    pub fn frame(&mut self) -> [f32; 2] {
        let mut x = 0.0;
        for s in &mut self.strings {
            if s.wait > 0 {
                s.wait -= 1;
                continue;
            }
            let n = s.line.len();
            let next = (s.pos + 1) % n;
            let a = s.line[s.pos] + (s.line[next] - s.line[s.pos]) * s.frac;
            let mut y = (a + s.last) * 0.5 * s.keep;
            if self.slap > 0.0 {
                // the string meets the frets on one side of its swing
                let lim = 1.0 - self.slap * 0.75;
                if y > lim {
                    y = lim + (y - lim) * 0.2;
                }
            }
            s.last = a;
            s.line[s.pos] = y * self.fade_by.min(1.0).powi(if self.fade_by < 1.0 { 4 } else { 0 });
            // the pickup hears the string at one or two spots along it
            x += match self.coils {
                0 => a,
                1 => a - s.line[(s.pos + s.tap[0]) % n],
                _ => a - 0.5 * (s.line[(s.pos + s.tap[0]) % n] + s.line[(s.pos + s.tap[1]) % n]),
            };
            s.pos = next;
        }
        if self.strings.len() > 1 {
            x *= 0.6;
        }
        if self.click > 1e-4 {
            // fret noise: a short bright scrape
            let n = rnd(&mut self.seed);
            self.click_z += (n - self.click_z) * 0.3;
            x += (n - self.click_z) * self.click;
            self.click *= self.click_by;
        }
        // pickup brightness, then the body's own voice mixed in
        self.bz += (x - self.bz) * self.bright;
        let x = self.bz;
        let g = self.fg;
        let hp = (x - (0.7 + g) * self.fz[0] - self.fz[1]) / (1.0 + g * (g + 0.7));
        let band = g * hp + self.fz[0];
        self.fz[0] = g * hp + band;
        self.fz[1] = g * band + self.fz[1] + g * band;
        let mut y = x + band * 0.6;
        self.fade *= self.fade_by;
        y *= self.fade * self.gain;
        // amplifier
        match self.amp {
            0 => y = self.eq(y * self.pre),
            2 => {
                y = self.eq(y);
                for _ in 0..3 {
                    y = (y * self.pre.powf(1.0 / 3.0) * 1.6).tanh();
                }
                y *= 0.5;
            }
            3 => {
                let g = self.bpg;
                let pre = self.pre.powf(1.0 / 3.0) * 1.6;
                for z in &mut self.bp {
                    let hp = (y - (1.0 + g) * z[0] - z[1]) / (1.0 + g * (g + 1.0));
                    let band = g * hp + z[0];
                    z[0] = g * hp + band;
                    z[1] = g * band + z[1] + g * band;
                    y = (band * pre * 1.5).tanh();
                }
                y = self.eq(y) * 0.5;
            }
            _ => {
                // presence: lift or cut the top before the valves
                self.pz += (y - self.pz) * 0.25;
                y += (y - self.pz) * (self.pres - 0.5) * 3.0;
                let pre = self.pre.powf(1.0 / 3.0) * 1.6;
                for k in 0..3 {
                    // valves clip one side sooner than the other
                    let bias = if k == 1 { 0.15 } else { 0.0 };
                    y = ((y + bias) * pre).tanh() - (bias * pre).tanh();
                }
                y = self.eq(y) * 0.5;
            }
        }
        // cabinet
        if self.cab != 0 {
            self.cz[0] += (y - self.cz[0]) * self.cc[0];
            y -= self.cz[0];
            self.cz[1] += (y - self.cz[1]) * self.cc[1];
            self.cz[2] += (self.cz[1] - self.cz[2]) * self.cc[2];
            let top = self.cz[1] - self.cz[2];
            y = self.cz[2] + top * if self.cab == 3 { 2.5 } else { 0.8 };
        }
        self.dc += (y - self.dc) * 0.0005;
        let y = y - self.dc;
        [y, y]
    }
}
