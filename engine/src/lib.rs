//! Breakfast Cereal DAW engine, first slice: a 16-step pattern driving four synthesized drum voices.
//!
//! The UI thread and the audio thread share one `Shared` made only of atomics, so the audio callback never
//! locks, allocates or waits. Every sound here is generated from maths; no sample from any other product is used.

use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

pub mod project;
pub mod wav;

pub use project::Project;

pub const STEPS: usize = 16;

/// Render `bars` bars of a project offline into a mono f32 buffer at `sample_rate`. No audio device is
/// touched, so this is what WAV export and the tests use.
pub fn render_project(project: &Project, sample_rate: f32, bars: u32) -> Vec<f32> {
    let shared = project.to_shared();
    shared.set_playing(true);
    let mut engine = Engine::new(shared.clone(), sample_rate);
    let step_len = sample_rate as f64 * 60.0 / project.bpm.max(1.0) as f64 / 4.0;
    let total = (step_len * STEPS as f64 * bars as f64).ceil() as usize;
    let mut buf = vec![0.0f32; total];
    for block in buf.chunks_mut(2048) {
        engine.render(block, 1);
    }
    buf
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drum {
    Kick,
    Clap,
    Hat,
    Snare,
}

/// One row of the channel rack.
pub struct Channel {
    pub name: &'static str,
    pub drum: Drum,
    steps: AtomicU32,
    volume: AtomicU32,
    muted: AtomicBool,
}

impl Channel {
    fn new(name: &'static str, drum: Drum, on: &[usize], volume: f32) -> Self {
        let bits = on.iter().fold(0u32, |b, &i| b | (1 << i));
        Self { name, drum, steps: AtomicU32::new(bits), volume: AtomicU32::new(volume.to_bits()), muted: AtomicBool::new(false) }
    }
    pub fn step(&self, i: usize) -> bool {
        self.steps.load(Relaxed) & (1 << (i % STEPS)) != 0
    }
    pub fn toggle(&self, i: usize) {
        self.steps.fetch_xor(1 << (i % STEPS), Relaxed);
    }
    pub fn set_step(&self, i: usize, on: bool) {
        if on {
            self.steps.fetch_or(1 << (i % STEPS), Relaxed);
        } else {
            self.steps.fetch_and(!(1 << (i % STEPS)), Relaxed);
        }
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Relaxed))
    }
    pub fn set_volume(&self, v: f32) {
        self.volume.store(v.clamp(0.0, 1.5).to_bits(), Relaxed);
    }
    pub fn muted(&self) -> bool {
        self.muted.load(Relaxed)
    }
    pub fn set_muted(&self, m: bool) {
        self.muted.store(m, Relaxed);
    }
}

/// Everything both threads can see.
pub struct Shared {
    pub channels: Vec<Channel>,
    playing: AtomicBool,
    bpm: AtomicU32,
    master: AtomicU32,
    step: AtomicUsize,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Self::from_parts(
            vec![
                Channel::new("Kick", Drum::Kick, &[0, 4, 8, 12], 0.9),
                Channel::new("Clap", Drum::Clap, &[4, 12], 0.7),
                Channel::new("Hat", Drum::Hat, &[2, 6, 10, 14], 0.5),
                Channel::new("Snare", Drum::Snare, &[], 0.7),
            ],
            130.0,
            0.8,
        )
    }

    pub fn from_parts(channels: Vec<Channel>, bpm: f32, master: f32) -> Arc<Self> {
        Arc::new(Self {
            channels,
            playing: AtomicBool::new(false),
            bpm: AtomicU32::new(bpm.to_bits()),
            master: AtomicU32::new(master.to_bits()),
            step: AtomicUsize::new(0),
        })
    }
    pub fn playing(&self) -> bool {
        self.playing.load(Relaxed)
    }
    pub fn set_playing(&self, p: bool) {
        self.playing.store(p, Relaxed);
    }
    pub fn bpm(&self) -> f32 {
        f32::from_bits(self.bpm.load(Relaxed))
    }
    pub fn set_bpm(&self, b: f32) {
        self.bpm.store(b.clamp(10.0, 522.0).to_bits(), Relaxed);
    }
    pub fn master(&self) -> f32 {
        f32::from_bits(self.master.load(Relaxed))
    }
    pub fn set_master(&self, v: f32) {
        self.master.store(v.clamp(0.0, 1.25).to_bits(), Relaxed);
    }
    /// The step that sounded last (for the moving light in the UI).
    pub fn current_step(&self) -> usize {
        self.step.load(Relaxed)
    }
}

struct Voice {
    drum: Drum,
    active: bool,
    t: u32,
    phase: f32,
    rng: u32,
    hp1: f32,
    hp2: f32,
}

impl Voice {
    fn new(drum: Drum, seed: u32) -> Self {
        Self { drum, active: false, t: 0, phase: 0.0, rng: seed, hp1: 0.0, hp2: 0.0 }
    }
    fn trigger(&mut self) {
        self.active = true;
        self.t = 0;
        self.phase = 0.0;
    }
    fn noise(&mut self) -> f32 {
        // xorshift32: cheap and good enough for a noise source
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    fn next(&mut self, sr: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let s = self.t as f32 / sr;
        self.t += 1;
        let (out, length) = match self.drum {
            Drum::Kick => {
                let freq = 45.0 + 130.0 * (-s * 28.0).exp();
                self.phase = (self.phase + TAU * freq / sr) % TAU;
                (self.phase.sin() * (-s * 7.0).exp() + self.noise() * 0.15 * (-s * 400.0).exp(), 0.6)
            }
            Drum::Snare => {
                let tone = (TAU * 185.0 * s).sin() * (-s * 22.0).exp() * 0.5;
                let n = self.noise();
                let hp = n - self.hp1;
                self.hp1 = n;
                (tone + hp * 0.45 * (-s * 16.0).exp(), 0.35)
            }
            Drum::Hat => {
                let n = self.noise();
                let a = n - self.hp1;
                self.hp1 = n;
                let b = a - self.hp2;
                self.hp2 = a;
                (b * 0.35 * (-s * 60.0).exp(), 0.12)
            }
            Drum::Clap => {
                let n = self.noise();
                let hp = n - self.hp1;
                self.hp1 = n;
                let burst = |d: f32| if s >= d { (-(s - d) * 220.0).exp() } else { 0.0 };
                let env = burst(0.0) + burst(0.011) + burst(0.022) + if s >= 0.03 { 0.5 * (-(s - 0.03) * 18.0).exp() } else { 0.0 };
                (hp * 0.4 * env.min(1.0), 0.4)
            }
        };
        if s > length {
            self.active = false;
        }
        out
    }
}

/// Lives on the audio thread.
pub struct Engine {
    shared: Arc<Shared>,
    sr: f32,
    voices: Vec<Voice>,
    step: usize,
    until_next: f64,
    was_playing: bool,
}

impl Engine {
    pub fn new(shared: Arc<Shared>, sample_rate: f32) -> Self {
        let voices = shared.channels.iter().enumerate().map(|(i, c)| Voice::new(c.drum, 0x9E37_79B9 ^ (i as u32 + 1) * 7919)).collect();
        Self { shared, sr: sample_rate, voices, step: 0, until_next: 0.0, was_playing: false }
    }

    /// Fill an interleaved buffer with `channels` output channels.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let sh = self.shared.clone();
        let playing = sh.playing();
        if playing && !self.was_playing {
            self.step = 0;
            self.until_next = 0.0;
        }
        self.was_playing = playing;
        let step_len = self.sr as f64 * 60.0 / sh.bpm() as f64 / 4.0;
        let master = sh.master();
        for frame in out.chunks_mut(channels.max(1)) {
            if playing {
                if self.until_next <= 0.0 {
                    for (v, c) in self.voices.iter_mut().zip(&sh.channels) {
                        if c.step(self.step) && !c.muted() {
                            v.trigger();
                        }
                    }
                    sh.step.store(self.step, Relaxed);
                    self.step = (self.step + 1) % STEPS;
                    self.until_next += step_len;
                }
                self.until_next -= 1.0;
            }
            let mut mix = 0.0;
            for (v, c) in self.voices.iter_mut().zip(&sh.channels) {
                mix += v.next(self.sr) * c.volume();
            }
            let y = (mix * master).tanh();
            for o in frame.iter_mut() {
                *o = y;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onsets(buf: &[f32], window: usize) -> Vec<usize> {
        // start of every stretch of sound that follows `window` samples of silence
        let mut out = vec![];
        let mut quiet = window;
        for (i, x) in buf.iter().enumerate() {
            if x.abs() > 1e-4 {
                if quiet >= window {
                    out.push(i);
                }
                quiet = 0;
            } else {
                quiet += 1;
            }
        }
        out
    }

    #[test]
    fn silent_when_stopped() {
        let sh = Shared::new();
        let mut e = Engine::new(sh, 48_000.0);
        let mut buf = vec![1.0f32; 4800];
        e.render(&mut buf, 2);
        assert!(buf.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn hat_lands_on_its_steps() {
        let sh = Shared::new();
        for c in &sh.channels {
            c.set_muted(c.drum != Drum::Hat);
        }
        sh.set_bpm(120.0);
        sh.set_playing(true);
        let sr = 48_000.0;
        let mut e = Engine::new(sh, sr);
        let mut buf = vec![0.0f32; 96_000]; // one bar at 120 bpm, mono
        for block in buf.chunks_mut(512) {
            e.render(block, 1);
        }
        let step = 6000; // samples per 16th at 120 bpm
        let got = onsets(&buf, 100);
        let want: Vec<usize> = [2, 6, 10, 14].iter().map(|s| s * step).collect();
        assert_eq!(got.len(), want.len(), "onsets {got:?}");
        for (g, w) in got.iter().zip(&want) {
            assert!(g.abs_diff(*w) <= 3, "onset at {g}, wanted {w}");
        }
    }

    #[test]
    fn output_stays_in_range_and_toggles_work() {
        let sh = Shared::new();
        sh.channels[3].toggle(0);
        assert!(sh.channels[3].step(0));
        sh.channels[3].set_step(0, false);
        assert!(!sh.channels[3].step(0));
        for c in &sh.channels {
            for i in 0..STEPS {
                c.set_step(i, true);
            }
            c.set_volume(1.5);
        }
        sh.set_playing(true);
        let mut e = Engine::new(sh, 44_100.0);
        let mut buf = vec![0.0f32; 88_200];
        e.render(&mut buf, 2);
        assert!(buf.iter().any(|x| x.abs() > 0.1));
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        assert!(buf.chunks(2).all(|f| f[0] == f[1]));
    }
}
