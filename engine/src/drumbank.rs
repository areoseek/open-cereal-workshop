//! The drum bank: 120 drum patches, one per key, each made from a swept tone or band-passed noise plus a second
//! band of noise with its own attack. The stored block gives a name and a record of whole numbers per key.
//! Which number is which control was worked out from the named patches of real projects; the scales are estimates
//! (docs/PARITY.md, "Instruments"). The recorded drum waves the original can mix in are not ours to ship, so the
//! noise source stands in for them.

use std::f32::consts::{PI, TAU};

/// Numbers kept per key.
pub const REC: usize = 33;
pub const KEYS: usize = 120;

mod ix {
    pub const LEVEL: usize = 0;
    pub const PAN: usize = 1;
    pub const HARM: usize = 2;
    pub const FREQ1: usize = 4;
    /// Sweep start in tone mode, bandwidth in noise mode.
    pub const BEND1: usize = 5;
    pub const SWEEP_TIME: usize = 6;
    pub const DECAY1: usize = 7;
    pub const MIX1: usize = 8;
    pub const FREQ2: usize = 10;
    pub const WIDTH2: usize = 11;
    pub const ATTACK2: usize = 12;
    pub const DECAY2: usize = 13;
    pub const MIX2: usize = 14;
    pub const COUNT: usize = 18;
    pub const RATE: usize = 19;
    pub const TRIG_DECAY: usize = 20;
    pub const RING: usize = 23;
    pub const NOISE1: usize = 32;
}

/// A small kit of our own for blocks we cannot read: kick, snare, closed hat, tom.
const OWN: [[i16; REC]; 4] = [
    [100, 0, -20, 0, 170, 84, 30, 80, 90, 0, 490, 127, 0, 13, 0, -1, 0, 100, 1, 32, 25, 127, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [100, 0, 10, 0, 330, 76, 28, 46, 88, 0, 800, 85, 0, 62, 50, -1, 0, 100, 1, 32, 25, 127, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [85, 0, 0, 0, 900, 64, 127, 50, 0, 0, 980, 120, 0, 60, 60, -1, 0, 100, 1, 32, 25, 127, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [98, 0, -14, 0, 300, 82, 10, 66, 84, 0, 310, 64, 40, 68, 40, -1, 0, 100, 1, 32, 25, 127, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
];

/// The per-key numbers of a stored block, `KEYS * REC` long. Old layouts get our own small kit.
pub fn parse(state: &[u8]) -> Vec<f32> {
    read(state).unwrap_or_else(|| (0..KEYS).flat_map(|k| OWN[k % 4].iter().map(|&v| v as f32)).collect())
}

fn read(state: &[u8]) -> Option<Vec<f32>> {
    let ver = u32::from_le_bytes(state.get(..4)?.try_into().ok()?);
    if !(3..=4).contains(&ver) {
        return None;
    }
    let mut o = 8;
    for _ in 0..KEYS {
        o += 1 + *state.get(o)? as usize;
    }
    let body = state.get(o..o + KEYS * 152)?;
    let mut out = Vec::with_capacity(KEYS * REC);
    for rec in body.chunks_exact(152) {
        out.extend(rec.chunks_exact(4).take(REC).map(|c| i32::from_le_bytes(c.try_into().unwrap()) as f32));
    }
    Some(out)
}

/// The names of the patches, one per key.
pub fn names(state: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut o = 8;
    if state.len() < 4 || !(3..=4).contains(&u32::from_le_bytes(state[..4].try_into().unwrap())) {
        return out;
    }
    for _ in 0..KEYS {
        let Some(&n) = state.get(o) else { break };
        let Some(s) = state.get(o + 1..o + 1 + n as usize) else { break };
        out.push(String::from_utf8_lossy(s).into_owned());
        o += 1 + n as usize;
    }
    out
}

/// A band-pass over noise, level-matched so narrow and wide bands sound equally loud.
#[derive(Debug, Clone, Default)]
struct Band {
    g: f32,
    k: f32,
    gain: f32,
    white: f32,
    s: [f32; 2],
}

impl Band {
    fn new(hz: f32, width: f32, sr: f32) -> Self {
        let w = (width / 127.0).clamp(0.0, 1.0);
        let q = 0.5 * 400f32.powf(1.0 - w);
        let hz = hz.min(sr * 0.45);
        let gain = ((sr * 0.5) / (1.57 * hz / q)).sqrt().clamp(1.0, 30.0);
        Self { g: (PI * hz / sr).tan(), k: 1.0 / q, gain, white: w.powi(4), s: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let hp = (x - (self.k + self.g) * self.s[0] - self.s[1]) / (1.0 + self.k * self.g + self.g * self.g);
        let bp = self.g * hp + self.s[0];
        self.s[0] = self.g * hp + bp;
        let lp = self.g * bp + self.s[1];
        self.s[1] = self.g * bp + lp;
        let y = bp * self.k * self.gain;
        y + (x - y) * self.white
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    t: u32,
    seed: u32,
    ph: f32,
    hz: f32,
    from: f32,
    sweep: f32,
    noise1: bool,
    b1: Band,
    b2: Band,
    harm: f32,
    dec1: f32,
    dec2: f32,
    att2: f32,
    mix1: f32,
    mix2: f32,
    ring: f32,
    /// Early hits before the main one: how many, seconds apart, and their decay.
    hits: u32,
    gap: f32,
    hit_dec: f32,
    gain: [f32; 2],
}

fn hz_of(v: f32) -> f32 {
    20.0 * 1000f32.powf((v / 1000.0).clamp(0.0, 1.0))
}

fn decay_s(v: f32) -> f32 {
    0.006 + (v / 127.0).clamp(0.0, 1.0).powi(3) * 2.5
}

impl Voice {
    pub fn start(bank: &[f32], sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let own: Vec<f32>;
        let r = match bank.get(key as usize * REC..key as usize * REC + REC) {
            Some(r) => r,
            None => {
                own = OWN[key as usize % 4].iter().map(|&v| v as f32).collect();
                &own
            }
        };
        let hz = hz_of(r[ix::FREQ1]);
        let pan = (r[ix::PAN] / 64.0).clamp(-1.0, 1.0);
        let level = r[ix::LEVEL] / 100.0 * vel * 0.8;
        let hits = (r[ix::COUNT] as i32 - 1).clamp(0, 16) as u32;
        Self {
            sr,
            t: 0,
            seed: seed | 1,
            ph: 0.0,
            hz,
            from: hz * 2f32.powf((r[ix::BEND1] - 64.0) / 64.0 * 3.0),
            sweep: 0.004 + (r[ix::SWEEP_TIME] / 127.0).powi(2) * 0.4,
            noise1: r[ix::NOISE1] != 0.0,
            b1: Band::new(hz, r[ix::BEND1], sr),
            b2: Band::new(hz_of(r[ix::FREQ2]), r[ix::WIDTH2], sr),
            harm: (r[ix::HARM] / 64.0).clamp(-1.0, 1.0),
            dec1: decay_s(r[ix::DECAY1]),
            dec2: decay_s(r[ix::DECAY2]),
            att2: (r[ix::ATTACK2] / 127.0).powi(2) * 0.25,
            mix1: r[ix::MIX1] / 127.0 * 1.4,
            mix2: r[ix::MIX2] / 127.0 * 1.4,
            ring: (r[ix::RING] / 127.0).clamp(0.0, 1.0),
            hits,
            gap: 0.004 + r[ix::RATE] / 127.0 * 0.03,
            hit_dec: decay_s(r[ix::TRIG_DECAY]),
            gain: [level * (1.0 - pan).min(1.0), level * (1.0 + pan).min(1.0)],
        }
    }

    /// Seconds of sound left.
    pub fn left(&self) -> f32 {
        (self.hits as f32 * self.gap + self.att2 + self.dec1.max(self.dec2) * 7.0 - self.t as f32 / self.sr).max(0.0)
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub fn frame(&mut self) -> [f32; 2] {
        let sec = self.t as f32 / self.sr;
        self.t += 1;
        // the early hits each start the envelopes again; the last one rings out at the patch's own decay
        let hit = ((sec / self.gap) as u32).min(self.hits);
        let since = sec - hit as f32 * self.gap;
        let last = hit == self.hits;
        let (d1, d2) = if last { (self.dec1, self.dec2) } else { (self.hit_dec.min(self.dec1), self.hit_dec.min(self.dec2)) };
        let e1 = (-since / d1).exp() * (1.0 - (-since * 3000.0).exp());
        let rise = if self.att2 > 0.0005 { (since / self.att2).min(1.0) } else { 1.0 };
        let e2 = if rise < 1.0 { rise } else { (-(since - self.att2).max(0.0) / d2).exp() } * (1.0 - (-since * 3000.0).exp());

        let mut a = if self.noise1 {
            let n = self.noise();
            self.b1.run(n)
        } else {
            // the sweep runs once, from the first hit
            let hz = self.hz + (self.from - self.hz) * (-sec / self.sweep).exp();
            let x = (self.ph * TAU).sin();
            self.ph += (hz / self.sr).clamp(0.0, 0.45);
            self.ph -= self.ph.floor();
            x
        };
        // left of centre thickens with odd harmonics, right of centre adds even ones
        if self.harm < 0.0 {
            let d = 1.0 - self.harm * 5.0;
            a = (a * d).tanh() / d.tanh();
        } else if self.harm > 0.0 {
            a = a * (1.0 - self.harm * 0.5) + (a.abs() * 2.0 - 1.0) * self.harm * 0.5;
        }
        let n = self.noise();
        let b = self.b2.run(n);
        let a = a * (1.0 + (b - 1.0) * self.ring);
        let o = (a * e1 * self.mix1).clamp(-1.0, 1.0) + (b * e2 * self.mix2).clamp(-1.0, 1.0);
        [o * self.gain[0], o * self.gain[1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Vec<u8> {
        let mut b = vec![4, 0, 0, 0, 0, 0, 0, 0];
        for k in 0..KEYS {
            let name = format!("Drum {k}");
            b.push(name.len() as u8);
            b.extend_from_slice(name.as_bytes());
        }
        for k in 0..KEYS {
            let mut rec = [0i32; 38];
            for (i, v) in OWN[k % 4].iter().enumerate() {
                rec[i] = *v as i32;
            }
            rec[34] = k as i32;
            b.extend(rec.iter().flat_map(|v| v.to_le_bytes()));
        }
        b.extend_from_slice(&[0; 4]);
        b
    }

    #[test]
    fn each_key_reads_its_own_patch_and_sounds() {
        let b = block();
        let bank = parse(&b);
        assert_eq!(bank.len(), KEYS * REC);
        assert_eq!(names(&b)[5], "Drum 5");
        assert_eq!(bank[REC + ix::FREQ1], 330.0);
        let energy = |key: u8| {
            let mut v = Voice::start(&bank, 44100.0, key, 1.0, 7);
            let mut lo = 0.0f32;
            let mut all = 0.0f32;
            let mut z = 0.0f32;
            for _ in 0..8000 {
                let x = v.frame()[0];
                assert!(x.is_finite());
                z += 0.02 * (x - z);
                lo += z * z;
                all += x * x;
            }
            (lo, all)
        };
        let (kick_lo, kick) = energy(0);
        let (hat_lo, hat) = energy(2);
        assert!(kick > 1.0 && hat > 0.01);
        // the kick is mostly low, the hat mostly high
        assert!(kick_lo / kick > 0.5 && hat_lo / hat < 0.1);
    }

    #[test]
    fn unreadable_blocks_fall_back_to_our_own_kit() {
        assert_eq!(parse(&[1, 0, 0, 0, 9, 9]).len(), KEYS * REC);
        let v = Voice::start(&[], 44100.0, 200, 1.0, 1);
        assert!(v.left() > 0.0);
    }
}
