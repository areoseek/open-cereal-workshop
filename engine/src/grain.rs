//! The grain player: the channel's sample is cut into short grains that are played one after another. The gap
//! between grains and the distance each new grain moves along the wave are set apart, so a sound can be stretched,
//! squeezed, frozen or walked backwards. Which stored number is which control was placed from presets (the
//! backwards preset has a negative wave spacing); scales are estimates (docs/PARITY.md, "Instruments").

use crate::sample::Sample;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Set {
    /// Seconds a grain holds, and seconds it takes to fade in and out.
    pub hold: f32,
    pub attack: f32,
    /// 1 = grains follow each other without a gap.
    pub grain_spacing: f32,
    /// 1 = each grain starts where the last one ended; negative walks backwards.
    pub wave_spacing: f32,
    /// 0..1: how far the grains are thrown to the left and right in turn.
    pub pan: f32,
    /// A slow wave that pushes the wave spacing about: its depth (0..1) and its speed in Hz.
    pub depth: f32,
    pub speed: f32,
    /// 0..1: how far each grain's start is scattered.
    pub rand: f32,
    /// Starts over from the top once the wave runs out.
    pub looped: bool,
    /// The place in the wave stands still.
    pub frozen: bool,
    /// 0..1: where in the wave a note starts.
    pub start: f32,
    /// What the key played does: 0 pitch, 1 start place from C5 (start) to C7 (end), 2 start later by a step
    /// for every key above C5, 3 start at a marked place (played as steps here).
    pub key_to: u8,
}

/// The stored numbers, four bytes each: (name, place, lowest, highest, new). Places 9 to 14 are taken from the
/// order of the manual's sections and the few presets that set them, so they are less sure than the first eight.
pub const FIELDS: [(&str, usize, i32, i32, i32); 14] = [
    ("Hold", 1, 10, 2000, 200),
    ("Attack", 2, 5, 1000, 50),
    ("Grain spacing", 3, 60, 1145, 611),
    ("Wave spacing", 4, -1145, 1145, 611),
    ("Pan", 5, 0, 256, 0),
    ("Speed", 6, 0, 256, 180),
    ("Depth", 7, 0, 256, 0),
    ("Random", 8, 0, 256, 0),
    ("Loop", 9, 0, 1, 0),
    ("Hold place", 10, 0, 1, 0),
    ("Start", 11, 0, 256, 0),
    ("Transient hold", 12, 0, 256, 0),
    ("Transients", 13, 0, 2, 0),
    ("Key to", 14, 0, 3, 0),
];

pub const KEY_TO: [&str; 4] = ["Key to pitch", "Key to percent", "Key to step", "Key to transient"];
pub const TRANSIENTS: [&str; 3] = ["Off", "Use regions", "Detect"];

/// A new unit's block.
pub fn fresh() -> Vec<u8> {
    [2i32, 200, 50, 611, 611, 0, 180, 0, 0, 0, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Stored number `i`; nothing stored there reads as zero.
pub fn get(state: &[u8], i: usize) -> i32 {
    state.get(i * 4..i * 4 + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
}

/// Sets stored number `i`, growing an older, shorter block to the newest layout first.
pub fn put(state: &mut Vec<u8>, i: usize, v: i32) {
    if state.len() < 60 {
        if state.len() < 44 {
            *state = fresh();
        }
        state.resize(60, 0);
        state[..4].copy_from_slice(&5i32.to_le_bytes());
    }
    state[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
}

/// What a stored number says, for the hint bar.
pub fn says(i: usize, v: i32) -> String {
    let set = |place: usize| {
        let mut b = fresh();
        put(&mut b, place, v);
        Set::parse(&b)
    };
    match i {
        1 => format!("Hold: {:.1} ms", set(1).hold * 1000.0),
        2 => format!("Attack: {:.1} ms", set(2).attack * 1000.0),
        3 => format!("Grain spacing: {:.0} %", set(3).grain_spacing * 100.0),
        4 => format!("Wave spacing: {:.0} %", set(4).wave_spacing * 100.0),
        6 => format!("Speed: {:.2} Hz", set(6).speed),
        _ => format!("{}: {:.0} %", FIELDS.iter().find(|f| f.1 == i).map_or("", |f| f.0), v as f32 / 2.56),
    }
}

/// The two spacing knobs: 611 is even, and the stored presets named "twice as fast" and "twice as slow" sit
/// near 411, so the knob follows a power curve; the sign walks backwards.
fn spacing(v: f32) -> f32 {
    v.signum() * (v.abs() / 611.0).powf(1.75)
}

impl Set {
    pub fn parse(state: &[u8]) -> Self {
        let int = |i: usize| state.get(i * 4..i * 4 + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f32);
        Self {
            hold: (int(1).unwrap_or(200.0) / 10000.0).clamp(0.002, 1.0),
            attack: (int(2).unwrap_or(50.0) / 10000.0).clamp(0.0005, 0.5),
            grain_spacing: spacing(int(3).unwrap_or(611.0)).clamp(0.05, 8.0),
            wave_spacing: spacing(int(4).unwrap_or(611.0)).clamp(-8.0, 8.0),
            pan: (int(5).unwrap_or(0.0) / 256.0).clamp(0.0, 1.0),
            speed: 0.05 * 400f32.powf((int(6).unwrap_or(180.0) / 256.0).clamp(0.0, 1.0)),
            depth: (int(7).unwrap_or(0.0) / 256.0).clamp(0.0, 1.0),
            rand: (int(8).unwrap_or(0.0) / 256.0).clamp(0.0, 1.0),
            looped: int(9).unwrap_or(0.0) != 0.0,
            frozen: int(10).unwrap_or(0.0) != 0.0,
            start: (int(11).unwrap_or(0.0) / 256.0).clamp(0.0, 1.0),
            key_to: int(14).unwrap_or(0.0).clamp(0.0, 3.0) as u8,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Grain {
    /// Place in the sample, in its frames.
    at: f64,
    /// Frames of output played so far, and the frames it lasts.
    t: f32,
    on: bool,
    /// Left and right gains.
    lr: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct Voice {
    smp: Arc<Sample>,
    set: Set,
    /// Sample frames per output frame.
    step: f64,
    sr: f32,
    grains: [Grain; 6],
    /// Where the next grain starts, and output frames until it does.
    next_at: f64,
    wait: f32,
    vel: f32,
    done: bool,
    fade: f32,
    fading: bool,
    /// Grains started, the slow wave's place, and the dice for the scatter.
    count: u32,
    lfo: f32,
    dice: u32,
}

impl Voice {
    pub fn start(smp: &Arc<Sample>, set: Set, sr: f32, vel: f32, dphase: f32, key: u8, tempo: f32) -> Self {
        // a key that picks the starting place leaves the pitch alone
        let dphase = if set.key_to == 0 { dphase } else { 261.6256 / sr };
        let step = (dphase * sr / 261.6256) as f64 * smp.rate as f64 / sr as f64;
        let n = smp.frames.len() as f64;
        let up = (key as f64 - 60.0).max(0.0);
        let from = set.start as f64 * n
            + match set.key_to {
                1 => (up / 24.0).min(1.0) * n,
                2 | 3 => up * 15.0 / tempo.max(1.0) as f64 * smp.rate as f64,
                _ => 0.0,
            };
        let next_at = if from > 0.0 {
            if set.looped { from.rem_euclid(n.max(1.0)) } else { from }
        } else if set.wave_spacing < 0.0 {
            (n - set.hold as f64 * smp.rate as f64).max(0.0)
        } else {
            0.0
        };
        Self { smp: smp.clone(), set, step, sr, grains: Default::default(), next_at, wait: 0.0, vel, done: smp.frames.is_empty(), fade: 1.0, fading: false, count: 0, lfo: 0.0, dice: 0x9e37_79b9 }
    }

    pub fn release(&mut self) -> f32 {
        self.fading = true;
        0.05
    }

    pub fn frame(&mut self) -> [f32; 2] {
        let s = self.set;
        let n = self.smp.frames.len() as f64;
        if !self.done && self.wait <= 0.0 {
            if s.looped && n >= 1.0 && (self.next_at < 0.0 || self.next_at >= n) {
                self.next_at = self.next_at.rem_euclid(n);
            }
            if self.next_at < 0.0 || self.next_at >= n {
                self.done = true;
            } else if let Some(g) = self.grains.iter_mut().find(|g| !g.on) {
                self.dice ^= self.dice << 13;
                self.dice ^= self.dice >> 17;
                self.dice ^= self.dice << 5;
                let throw = (self.dice as f32 / u32::MAX as f32 * 2.0 - 1.0) * s.rand * 0.25 * self.smp.rate as f32;
                let side = if self.count % 2 == 0 { -s.pan } else { s.pan };
                let at = (self.next_at + throw as f64).clamp(0.0, (n - 1.0).max(0.0));
                *g = Grain { at, t: 0.0, on: true, lr: [(1.0 - side).min(1.0), (1.0 + side).min(1.0)] };
                self.count += 1;
                self.wait += s.hold * s.grain_spacing * self.sr;
                // one grain's worth of wave at the played pitch, pushed about by the slow wave
                let push = 1.0 + s.depth * (self.lfo * std::f32::consts::TAU).sin();
                if !s.frozen {
                    self.next_at += (s.wave_spacing * push) as f64 * s.hold as f64 * self.sr as f64 * self.step;
                }
            }
        }
        self.wait -= 1.0;
        self.lfo = (self.lfo + s.speed / self.sr).fract();
        let (hold, att) = (s.hold * self.sr, (s.attack * self.sr).max(1.0));
        let mut o = [0.0f32; 2];
        for g in self.grains.iter_mut().filter(|g| g.on) {
            let i = g.at as usize;
            if g.t >= hold + att || g.at >= n - 1.0 {
                g.on = false;
                continue;
            }
            let f = (g.at - i as f64) as f32;
            let (a, b) = (self.smp.frames[i], self.smp.frames[i + 1]);
            // fade in over the attack, hold, then fade out while the next grain fades in
            let e = (g.t / att).min(1.0).min(((hold + att - g.t) / att).max(0.0));
            o[0] += (a[0] + (b[0] - a[0]) * f) * e * g.lr[0];
            o[1] += (a[1] + (b[1] - a[1]) * f) * e * g.lr[1];
            g.at += self.step;
            g.t += 1.0;
        }
        if self.fading {
            self.fade *= 1.0 - 1.0 / (0.01 * self.sr);
        }
        let g = self.vel * self.fade;
        [o[0] * g, o[1] * g]
    }

    /// Seconds of sound left once no more grains will start.
    pub fn left(&self) -> f32 {
        if self.done && self.grains.iter().all(|g| !g.on) { 0.0 } else { f32::MAX }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Arc<Sample> {
        Arc::new(Sample::from_mono("ramp", 44100, &(0..44100).map(|i| i as f32 / 44100.0).collect::<Vec<_>>()))
    }

    fn block(wave: i32) -> Vec<u8> {
        [5, 200, 50, 611, wave].iter().flat_map(|v: &i32| v.to_le_bytes()).collect()
    }

    #[test]
    fn even_spacing_plays_the_wave_through_and_a_negative_one_walks_back() {
        let set = Set::parse(&block(611));
        assert!((set.wave_spacing - 1.0).abs() < 1e-6 && (set.hold - 0.02).abs() < 1e-6);
        let mut v = Voice::start(&ramp(), set, 44100.0, 1.0, 261.6256 / 44100.0, 60, 120.0);
        let out: Vec<f32> = (0..30000).map(|_| v.frame()[0]).collect();
        // the ramp comes out as a ramp, grain joins included
        for t in [5000usize, 12345, 29000] {
            assert!((out[t] - t as f32 / 44100.0).abs() < 0.02, "{t}: {}", out[t]);
        }
        let mut v = Voice::start(&ramp(), Set::parse(&block(-611)), 44100.0, 1.0, 261.6256 / 44100.0, 60, 120.0);
        let out: Vec<f32> = (0..30000).map(|_| v.frame()[0]).collect();
        assert!(out[2000] > 0.9 && out[25000] < 0.5);
        // twice the wave spacing runs out in half the time
        let mut v = Voice::start(&ramp(), Set::parse(&block(1222)), 44100.0, 1.0, 261.6256 / 44100.0, 60, 120.0);
        for _ in 0..26000 {
            v.frame();
        }
        assert_eq!(v.left(), 0.0);
    }

    #[test]
    fn the_key_can_pick_the_starting_place_and_a_loop_goes_round() {
        let mut b = fresh();
        put(&mut b, 14, 1);
        assert_eq!((b.len(), get(&b, 0), get(&b, 1), get(&b, 14)), (60, 5, 200, 1));
        let set = Set::parse(&b);
        assert_eq!(set.key_to, 1);
        // C6 with the key picking the place starts half way along, at the wave's own pitch
        let mut v = Voice::start(&ramp(), set, 44100.0, 1.0, 2.0 * 261.6256 / 44100.0, 72, 120.0);
        let first = (0..400).map(|_| v.frame()[0]).fold(0.0f32, f32::max);
        assert!((first - 0.5).abs() < 0.05, "{first}");
        // looped, the grains never run out
        put(&mut b, 14, 0);
        put(&mut b, 9, 1);
        let mut v = Voice::start(&ramp(), Set::parse(&b), 44100.0, 1.0, 261.6256 / 44100.0, 60, 120.0);
        for _ in 0..44100 * 3 {
            v.frame();
        }
        assert_eq!(v.left(), f32::MAX);
        assert!(says(4, 611).contains("100 %"), "{}", says(4, 611));
    }
}
