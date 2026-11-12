//! An electric guitar voice: plucked strings read through a pickup, then an amplifier, a tone stack and a cabinet.
//!
//! Settings are in the order of `synth::GUITAR`. The layout was placed from value ranges and the public manual's
//! list of controls; every scale here is an estimate. The amplifier works per note (three strings share one in the
//! power chord mode), so chords played as separate notes do not grind against each other the way one amp would
//! make them. The effects that sit before the amplifier run per note as well; the ones after it run once for the
//! channel (`tail`). Feedback sends the amplifier's sound back into the strings. The chord mode plays root, fifth
//! and octaves for every key (no thirds, as one note cannot tell major from minor). The second pitch wheel, the
//! strum-what-is-held mode and the two gliding solo modes are not built.

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
    // the effect ahead of the amplifier
    fx: u8,
    f1: f32,
    f2: f32,
    lfo: f32,
    lfo_by: f32,
    ap: [f32; 4],
    ap_fb: f32,
    wah: [f32; 2],
    wah2: [f32; 2],
    env: f32,
    ring: f32,
    ring_by: f32,
    held: f32,
    count: f32,
    flip: f32,
    was_up: bool,
    // amplifier back into the strings
    feed: f32,
    out: f32,
}

/// Names for the settings that are a choice and not an amount: (setting, names).
pub const CHOICES: [(usize, &[&str]); 2] = [
    (17, &["Dry", "Before amp: Phaser", "Before amp: Tremolo", "Before amp: Wah", "Before amp: Wah, swept", "Before amp: Ring", "Before amp: Fuzz", "Before amp: Octaves", "Before amp: Talker", "After amp: Chorus", "After amp: Flanger", "After amp: Phaser", "After amp: Rotary", "After amp: Tremolo", "After amp: Dub echo", "After amp: Multi-tap echo"]),
    (20, &["Plain", "Chords", "Power chords", "Strummed", "Solo, even glide", "Solo, glide by distance"]),
];

/// The effects that come after the amplifier, run once for the whole channel.
pub fn tail(p: &[f32]) -> crate::tailfx::Set {
    use crate::tailfx::{Chorus, Delay, Rack, Set};
    let u = |i: usize| (p.get(i).copied().unwrap_or(0.0) / 65535.0).clamp(0.0, 1.0);
    let (a, b) = (u(18), u(19));
    match p.get(17).copied().unwrap_or(0.0) as u8 {
        9 => Set { chorus: Some(Chorus { depth: b, hz: 0.1 * 50f32.powf(a), wet: 0.5 }), ..Default::default() },
        10 => Set { rack: Some(Rack { flange: Some([b, 0.3 + a * 0.5]), ..Default::default() }), ..Default::default() },
        11 => Set { rack: Some(Rack { phase: Some([0.4 + a * 0.6, b * 0.9]), ..Default::default() }), ..Default::default() },
        12 => Set { chorus: Some(Chorus { depth: b * 0.6, hz: 0.6 + a * 6.0, wet: 0.5 }), ..Default::default() },
        14 => Set { delay: Some(Delay { free: 0.05 + a * 0.95, fb: 0.2 + b * 0.7, low_hz: 200.0, high_hz: 3000.0, wet: 0.4, ..Default::default() }), ..Default::default() },
        15 => {
            let steps = (1.0 + a * 7.0).round();
            let tap = |k: f32| Delay { left: steps * k, right: steps * k, fb: b * 0.85, low_hz: 120.0, high_hz: 6000.0, wet: 0.3, ..Default::default() };
            Set { delay: Some(tap(1.0)), more: [Some(Delay { left: steps * 0.5, right: steps * 1.5, ..tap(1.0) }), None], ..Default::default() }
        }
        _ => Set::default(),
    }
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
        } else if mode == 1 {
            // a full chord shape without its thirds, strummed low to high
            notes = vec![0.0, 7.0, 12.0, 19.0, 24.0];
        }
        let fx = v(17) as u8;
        let (f1, f2) = (u(18), u(19));
        let lfo_hz = match fx {
            1 | 4 | 8 => 0.1 * 80f32.powf(f1),
            2 | 13 => 1.0 + f1 * 14.0,
            _ => 0.0,
        };
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
            fx,
            f1,
            f2,
            lfo: 0.0,
            lfo_by: lfo_hz / sr,
            ap: [0.0; 4],
            ap_fb: 0.0,
            wah: [0.0; 2],
            wah2: [0.0; 2],
            env: 0.0,
            ring: 0.0,
            ring_by: (base * (1.0 + (f1 * 7.0).round()) / sr).min(0.45),
            held: 0.0,
            count: 0.0,
            flip: 1.0,
            was_up: false,
            feed: u(12),
            out: 0.0,
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

    /// One band-pass step at `hz`; gives the band.
    fn band(z: &mut [f32; 2], x: f32, hz: f32, sr: f32, k: f32) -> f32 {
        let g = (PI * hz.min(sr * 0.4) / sr).tan();
        let hp = (x - (k + g) * z[0] - z[1]) / (1.0 + g * (g + k));
        let band = g * hp + z[0];
        z[0] = g * hp + band;
        z[1] = g * band + z[1] + g * band;
        band
    }

    /// The effect that sits between the guitar and the amplifier.
    fn before_amp(&mut self, x: f32) -> f32 {
        if self.fx == 0 || self.fx > 8 {
            return x;
        }
        self.lfo = (self.lfo + self.lfo_by).fract();
        let wave = 0.5 - 0.5 * (self.lfo * 2.0 * PI).cos();
        let sr = self.sr;
        match self.fx {
            1 => {
                // four all-pass stages swept together, with some of the result sent round again
                let hz = 300.0 * 8f32.powf(wave);
                let t = (PI * hz / sr).tan();
                let c = (t - 1.0) / (t + 1.0);
                let mut y = x + self.ap_fb * self.f2 * 0.8;
                for z in &mut self.ap {
                    let o = c * y + *z;
                    *z = y - c * o;
                    y = o;
                }
                self.ap_fb = y;
                (x + y) * 0.5
            }
            2 => x * (1.0 - self.f2 * wave),
            3 | 4 => {
                // a peak that climbs with how hard the string is played, or with the slow wave
                let open = if self.fx == 3 {
                    let rate = if x.abs() > self.env { 0.0005 + self.f1 * 0.02 } else { 0.0002 };
                    self.env += (x.abs() - self.env) * rate;
                    (self.env * 6.0).min(1.0)
                } else {
                    wave
                };
                let hz = 350.0 * 7f32.powf(open * self.f2);
                let band = Self::band(&mut self.wah, x, hz, sr, 0.25);
                band * 1.5 + x * (1.0 - self.f2) * 0.5
            }
            5 => {
                self.ring = (self.ring + self.ring_by).fract();
                let m = (self.ring * 2.0 * PI).sin();
                // the second knob is not described anywhere public: here it is how much of the ring is heard
                let mix = 0.5 + 0.5 * self.f2;
                x * (1.0 - mix) + x * m * mix
            }
            6 => {
                let mut y = (x * (1.0 + self.f1 * 40.0)).tanh();
                if self.f2 > 0.0 {
                    // the digital kind: held samples and coarse steps
                    self.count += 1.0;
                    if self.count >= 1.0 + self.f2 * 24.0 {
                        self.count = 0.0;
                        let steps = 2.0 + (1.0 - self.f2) * 62.0;
                        self.held = (y * steps).round() / steps;
                    }
                    y += (self.held - y) * self.f2;
                }
                y * (1.0 - self.f1 * 0.6)
            }
            7 => {
                // an octave up from folding the wave over, an octave down from every other swing
                let up = x > 0.0;
                if up && !self.was_up {
                    self.flip = -self.flip;
                }
                self.was_up = up;
                self.env += (x.abs() - self.env) * 0.002;
                let above = x.abs() - self.env;
                let below = self.flip * self.env;
                self.wah[0] += (below - self.wah[0]) * 0.1;
                x + above * self.f1 * 1.5 + self.wah[0] * self.f2 * 1.5
            }
            _ => {
                // two mouth resonances gliding from one vowel to the next
                let shift = 0.7 * 2f32.powf(self.f2);
                let (a, b) = (300.0 + 500.0 * wave, 2300.0 - 1300.0 * wave);
                let lo = Self::band(&mut self.wah, x, a * shift, sr, 0.15);
                let hi = Self::band(&mut self.wah2, x, b * shift, sr, 0.15);
                (lo + hi * 0.7) * 2.0
            }
        }
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
            if self.feed > 0.0 {
                // the loudspeaker shakes the string: notes held near the amp sing on
                y = (y + self.out * self.feed * 0.03).clamp(-1.5, 1.5);
            }
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
        y = self.before_amp(y);
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
        let mut y = y - self.dc;
        self.out = y;
        if self.fx == 13 {
            // tremolo after the amplifier
            self.lfo = (self.lfo + self.lfo_by).fract();
            y *= 1.0 - self.f2 * (0.5 - 0.5 * (self.lfo * 2.0 * PI).cos());
        }
        [y, y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Vec<f32> {
        crate::synth::Gen::decode("FL Slayer", &[]).unwrap().p.clone()
    }

    fn play(p: &[f32], frames: usize) -> Vec<f32> {
        let mut v = Voice::start(p, 44_100.0, 52, 0.9, 164.8 / 44_100.0, 7);
        (0..frames).map(|_| v.frame()[0]).collect()
    }

    #[test]
    fn every_effect_before_the_amp_changes_the_sound_and_stays_sane() {
        let mut p = set();
        let dry = play(&p, 20_000);
        for fx in 1..=8 {
            p[17] = fx as f32;
            p[18] = 40_000.0;
            p[19] = 50_000.0;
            let wet = play(&p, 20_000);
            assert!(wet.iter().all(|x| x.is_finite() && x.abs() < 4.0), "effect {fx} runs away");
            let diff: f32 = wet.iter().zip(&dry).map(|(a, b)| (a - b).abs()).sum::<f32>() / wet.len() as f32;
            assert!(diff > 1e-4, "effect {fx} does nothing");
            assert!(wet.iter().any(|x| x.abs() > 0.01), "effect {fx} is silent");
        }
        // the ones after the amp are handed to the channel
        for fx in [9, 10, 11, 12, 14, 15] {
            p[17] = fx as f32;
            assert!(tail(&p).any(), "effect {fx} has no channel part");
        }
        p[17] = 0.0;
        assert!(!tail(&p).any());
    }

    #[test]
    fn feedback_keeps_a_note_ringing() {
        let mut p = set();
        p[10] = 50_000.0;
        let level = |v: &[f32]| v[v.len() - 4000..].iter().map(|x| x * x).sum::<f32>();
        p[12] = 0.0;
        let without = level(&play(&p, 120_000));
        p[12] = 65_535.0;
        let with = play(&p, 120_000);
        assert!(with.iter().all(|x| x.is_finite()));
        assert!(level(&with) > without * 1.5, "{} against {without}", level(&with));
    }

    #[test]
    fn the_chord_mode_plays_more_strings() {
        let mut p = set();
        p[20] = 1.0;
        assert_eq!(Voice::start(&p, 44_100.0, 52, 0.9, 164.8 / 44_100.0, 7).strings.len(), 5);
    }
}
