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
}

impl Set {
    pub fn parse(state: &[u8]) -> Self {
        let int = |i: usize| state.get(i * 4..i * 4 + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f32);
        Self {
            hold: (int(1).unwrap_or(200.0) / 10000.0).clamp(0.002, 1.0),
            attack: (int(2).unwrap_or(50.0) / 10000.0).clamp(0.0005, 0.5),
            grain_spacing: (int(3).unwrap_or(611.0) / 611.0).clamp(0.05, 8.0),
            wave_spacing: (int(4).unwrap_or(611.0) / 611.0).clamp(-8.0, 8.0),
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
}

impl Voice {
    pub fn start(smp: &Arc<Sample>, set: Set, sr: f32, vel: f32, dphase: f32) -> Self {
        let step = (dphase * sr / 261.6256) as f64 * smp.rate as f64 / sr as f64;
        let next_at = if set.wave_spacing < 0.0 { (smp.frames.len() as f64 - set.hold as f64 * smp.rate as f64).max(0.0) } else { 0.0 };
        Self { smp: smp.clone(), set, step, sr, grains: Default::default(), next_at, wait: 0.0, vel, done: smp.frames.is_empty(), fade: 1.0, fading: false }
    }

    pub fn release(&mut self) -> f32 {
        self.fading = true;
        0.05
    }

    pub fn frame(&mut self) -> [f32; 2] {
        let s = self.set;
        let n = self.smp.frames.len() as f64;
        if !self.done && self.wait <= 0.0 {
            if self.next_at < 0.0 || self.next_at >= n {
                self.done = true;
            } else if let Some(g) = self.grains.iter_mut().find(|g| !g.on) {
                *g = Grain { at: self.next_at, t: 0.0, on: true };
                self.wait += s.hold * s.grain_spacing * self.sr;
                // one grain's worth of wave at the played pitch
                self.next_at += s.wave_spacing as f64 * s.hold as f64 * self.sr as f64 * self.step;
            }
        }
        self.wait -= 1.0;
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
            o[0] += (a[0] + (b[0] - a[0]) * f) * e;
            o[1] += (a[1] + (b[1] - a[1]) * f) * e;
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
        let mut v = Voice::start(&ramp(), set, 44100.0, 1.0, 261.6256 / 44100.0);
        let out: Vec<f32> = (0..30000).map(|_| v.frame()[0]).collect();
        // the ramp comes out as a ramp, grain joins included
        for t in [5000usize, 12345, 29000] {
            assert!((out[t] - t as f32 / 44100.0).abs() < 0.02, "{t}: {}", out[t]);
        }
        let mut v = Voice::start(&ramp(), Set::parse(&block(-611)), 44100.0, 1.0, 261.6256 / 44100.0);
        let out: Vec<f32> = (0..30000).map(|_| v.frame()[0]).collect();
        assert!(out[2000] > 0.9 && out[25000] < 0.5);
        // twice the wave spacing runs out in half the time
        let mut v = Voice::start(&ramp(), Set::parse(&block(1222)), 44100.0, 1.0, 261.6256 / 44100.0);
        for _ in 0..26000 {
            v.frame();
        }
        assert_eq!(v.left(), 0.0);
    }
}
