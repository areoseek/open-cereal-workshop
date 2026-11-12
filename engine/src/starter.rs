//! Starter sounds made by code: a small drum kit and a few tones, so a fresh install has something to play
//! without anybody's sample pack. Everything here is synthesized; nothing is copied from anywhere.

const RATE: f32 = 44_100.0;
const TAU: f32 = std::f32::consts::TAU;

/// The sample rate the starter sounds are made at.
pub const SAMPLE_RATE: u32 = 44_100;

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// A two-pole filter giving low, band and high outputs at once.
struct Svf {
    g: f32,
    k: f32,
    a: f32,
    b: f32,
}

impl Svf {
    fn new(hz: f32, q: f32) -> Self {
        Self { g: (std::f32::consts::PI * (hz / RATE).min(0.49)).tan(), k: 1.0 / q.max(0.1), a: 0.0, b: 0.0 }
    }
    /// (low, band, high)
    fn run(&mut self, x: f32) -> (f32, f32, f32) {
        let h = (x - (self.k + self.g) * self.a - self.b) / (1.0 + self.g * (self.g + self.k));
        let band = self.g * h + self.a;
        self.a = self.g * h + band;
        let low = self.g * band + self.b;
        self.b = self.g * band + low;
        (low, band, h)
    }
}

fn frames(secs: f32) -> usize {
    (secs * RATE) as usize
}

/// Bring the loudest point to a set level and fade the last few milliseconds to nothing.
fn finish(mut v: Vec<f32>, peak: f32) -> Vec<f32> {
    let top = v.iter().fold(0f32, |a, x| a.max(x.abs())).max(1e-6);
    let n = v.len();
    let fade = frames(0.005).min(n);
    for (i, x) in v.iter_mut().enumerate() {
        *x *= peak / top;
        if i + fade >= n {
            *x *= (n - 1 - i) as f32 / fade as f32;
        }
    }
    v
}

/// A sine whose pitch falls from one note to another: the body of kicks and toms.
fn thump(from: f32, to: f32, fall: f32, secs: f32, click: f32) -> Vec<f32> {
    let n = frames(secs);
    let mut ph = 0.0f32;
    let mut noise = Noise(0x1234_5678);
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let hz = to + (from - to) * (-t / fall).exp();
            ph += hz / RATE;
            let amp = (-t / (secs * 0.28)).exp();
            let tick = if t < 0.004 { noise.next() * click * (1.0 - t / 0.004) } else { 0.0 };
            ((ph * TAU).sin() * amp + tick).tanh()
        })
        .collect();
    finish(v, 0.95)
}

fn snare() -> Vec<f32> {
    let n = frames(0.28);
    let mut noise = Noise(0x0bad_cafe);
    let mut f = Svf::new(3200.0, 0.8);
    let mut ph = 0.0f32;
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            ph += (175.0 + 90.0 * (-t / 0.02).exp()) / RATE;
            let body = (ph * TAU).sin() * (-t / 0.06).exp() * 0.7;
            let (_, band, high) = f.run(noise.next());
            body + (band * 0.5 + high * 0.5) * (-t / 0.075).exp()
        })
        .collect();
    finish(v, 0.9)
}

fn clap() -> Vec<f32> {
    let n = frames(0.32);
    let mut noise = Noise(0x00c0_ffee);
    let mut f = Svf::new(1300.0, 1.6);
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            // three quick slaps and then the room
            let slaps: f32 = [0.0f32, 0.011, 0.022].iter().map(|s| if t >= *s { (-(t - s) / 0.004).exp() } else { 0.0 }).sum();
            let tail = if t >= 0.03 { (-(t - 0.03) / 0.07).exp() } else { 0.0 };
            f.run(noise.next()).1 * (slaps * 0.8 + tail)
        })
        .collect();
    finish(v, 0.9)
}

/// Six square waves at uneven pitches through a high filter: the ring of hats and cymbals.
fn metal(secs: f32, fall: f32, cut: f32, noise_mix: f32) -> Vec<f32> {
    let n = frames(secs);
    let mut noise = Noise(0x5eed_1234);
    let mut f = Svf::new(cut, 0.9);
    let tones = [205.3f32, 304.4, 369.6, 522.7, 540.0, 800.0];
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let ring: f32 = tones.iter().map(|hz| if (t * hz * 8.0).fract() < 0.5 { 1.0 } else { -1.0 }).sum::<f32>() / 6.0;
            let x = ring * (1.0 - noise_mix) + noise.next() * noise_mix;
            f.run(x).2 * (-t / fall).exp()
        })
        .collect();
    finish(v, 0.8)
}

fn rim() -> Vec<f32> {
    let n = frames(0.06);
    let mut noise = Noise(0x0f0f_0f0f);
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            ((t * 1720.0 * TAU).sin() * 0.7 + (t * 830.0 * TAU).sin() * 0.4 + noise.next() * 0.2 * (-t / 0.002).exp()) * (-t / 0.012).exp()
        })
        .collect();
    finish(v, 0.85)
}

fn cowbell() -> Vec<f32> {
    let n = frames(0.35);
    let mut f = Svf::new(2640.0, 1.2);
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let sq = |hz: f32| if (t * hz).fract() < 0.5 { 1.0 } else { -1.0 };
            let amp = (-t / 0.012).exp() * 0.6 + (-t / 0.09).exp() * 0.4;
            f.run(sq(540.0) + sq(800.0)).1 * amp
        })
        .collect();
    finish(v, 0.8)
}

fn shaker() -> Vec<f32> {
    let n = frames(0.16);
    let mut noise = Noise(0x7777_1111);
    let mut f = Svf::new(6500.0, 1.0);
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let amp = (t / 0.03).min(1.0) * (-(t - 0.03).max(0.0) / 0.035).exp();
            f.run(noise.next()).1 * amp
        })
        .collect();
    finish(v, 0.7)
}

fn sub_bass() -> Vec<f32> {
    let n = frames(1.2);
    let hz = 65.406;
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let amp = (t / 0.004).min(1.0) * (-t / 0.6).exp();
            ((t * hz * TAU).sin() + (t * hz * 2.0 * TAU).sin() * 0.25 * (-t / 0.15).exp()) * amp
        })
        .collect();
    finish(v, 0.9)
}

/// A plucked string: a burst of noise going round a delay line that dulls it a little every turn.
fn pluck(hz: f32, secs: f32) -> Vec<f32> {
    let n = frames(secs);
    let len = (RATE / hz).round().max(2.0) as usize;
    let mut noise = Noise(0x2468_ace1);
    let mut line: Vec<f32> = (0..len).map(|_| noise.next()).collect();
    let mut at = 0;
    let v = (0..n)
        .map(|_| {
            let out = line[at];
            let next = line[(at + 1) % len];
            line[at] = (out + next) * 0.498;
            at = (at + 1) % len;
            out
        })
        .collect();
    finish(v, 0.85)
}

/// Three saws a little apart, with a filter that closes as the note dies.
fn saw_stab(hz: f32) -> Vec<f32> {
    let n = frames(0.9);
    let mut ph = [0.0f32, 0.33, 0.67];
    let tune = [0.994f32, 1.0, 1.006];
    let mut low = 0.0f32;
    let v = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let mut x = 0.0;
            for k in 0..3 {
                ph[k] = (ph[k] + hz * tune[k] / RATE).fract();
                x += ph[k] * 2.0 - 1.0;
            }
            let cut = 0.02 + 0.5 * (-t / 0.18).exp();
            low += (x / 3.0 - low) * cut;
            low * (t / 0.003).min(1.0) * (-t / 0.4).exp()
        })
        .collect();
    finish(v, 0.85)
}

/// A soft held tone that loops cleanly: whole cycles of a few partials.
fn pad(hz: f32) -> Vec<f32> {
    let cycles = (hz * 2.0).round();
    let n = (cycles / hz * RATE).round() as usize;
    let v = (0..n)
        .map(|i| {
            let ph = i as f32 / n as f32 * cycles;
            (1..=6).map(|k| (ph * k as f32 * TAU + k as f32).sin() / (k * k) as f32).sum()
        })
        .collect::<Vec<f32>>();
    let top = v.iter().fold(0f32, |a, x| a.max(x.abs())).max(1e-6);
    v.into_iter().map(|x| x / top * 0.8).collect()
}

/// Every starter sound: its name and its mono samples at `SAMPLE_RATE`.
pub fn sounds() -> Vec<(&'static str, Vec<f32>)> {
    vec![
        ("Kick", thump(170.0, 46.0, 0.035, 0.45, 0.5)),
        ("Kick Deep", thump(130.0, 36.0, 0.06, 0.8, 0.2)),
        ("Snare", snare()),
        ("Clap", clap()),
        ("Hat", metal(0.09, 0.022, 7000.0, 0.5)),
        ("Open Hat", metal(0.5, 0.16, 6500.0, 0.5)),
        ("Crash", metal(1.8, 0.5, 4500.0, 0.65)),
        ("Rim", rim()),
        ("Cowbell", cowbell()),
        ("Shaker", shaker()),
        ("Tom Low", thump(160.0, 82.0, 0.05, 0.5, 0.15)),
        ("Tom Mid", thump(220.0, 120.0, 0.05, 0.42, 0.15)),
        ("Tom High", thump(300.0, 170.0, 0.045, 0.35, 0.15)),
        ("Sub Bass C2", sub_bass()),
        ("Pluck C4", pluck(261.626, 1.5)),
        ("Saw Stab C3", saw_stab(130.813)),
        ("Pad Wave C3", pad(130.813)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_starter_sound_is_sound() {
        let all = sounds();
        assert!(all.len() >= 16);
        for (name, v) in &all {
            assert!(v.len() > 500, "{name} is too short");
            assert!(v.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{name} goes out of range");
            let peak = v.iter().fold(0f32, |a, x| a.max(x.abs()));
            assert!(peak > 0.5, "{name} is too quiet: {peak}");
        }
        // the same every time, so projects that name these files find the same sound
        assert_eq!(sounds()[0].1, all[0].1);
        // the hat is bright and the kick is not: count how often each crosses zero
        let crossings = |v: &[f32]| v.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count() as f32 / v.len() as f32;
        let get = |n: &str| all.iter().find(|s| s.0 == n).map(|s| s.1.clone()).unwrap();
        assert!(crossings(&get("Hat")) > 0.2);
        assert!(crossings(&get("Kick")) < 0.02);
    }
}
