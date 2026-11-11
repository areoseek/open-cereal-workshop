//! Convolution: every input sample sets off a copy of an impulse, as a convolution reverb does. The first stretch
//! of the impulse is applied directly, so there is no added delay; the rest is applied a block at a time through
//! a transform.

use crate::sample::Sample;
use std::sync::Arc;

/// Longest impulse kept, in seconds; longer ones are cut.
const MAX_SECONDS: f32 = 30.0;

/// A complex transform of one fixed power-of-two size.
pub struct Plan {
    n: usize,
    rev: Vec<u32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
}

impl Plan {
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 2);
        let bits = n.trailing_zeros();
        let rev = (0..n as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        let (mut cos, mut sin) = (Vec::with_capacity(n / 2), Vec::with_capacity(n / 2));
        for i in 0..n / 2 {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            cos.push(a.cos() as f32);
            sin.push(a.sin() as f32);
        }
        Self { n, rev, cos, sin }
    }

    /// In place, unscaled; `inverse` turns the other way.
    pub fn run(&self, re: &mut [f32], im: &mut [f32], inverse: bool) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i] as usize;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let sign = if inverse { 1.0 } else { -1.0 };
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            let mut start = 0;
            while start < n {
                for k in 0..half {
                    let (c, s) = (self.cos[k * step], sign * self.sin[k * step]);
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * c - im[b] * s;
                    let ti = re[b] * s + im[b] * c;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
                start += len;
            }
            len <<= 1;
        }
    }
}

/// An impulse made ready for one sample rate and block size.
pub struct Kernel {
    b: usize,
    plan: Plan,
    /// The first `b` values of each side, back to front.
    head: [Vec<f32>; 2],
    /// The later blocks of each side as spectra: `parts` times `b + 1` real values, then as many imaginary ones.
    spec: [Vec<f32>; 2],
    parts: usize,
    /// Length of the impulse in frames at the prepared rate.
    pub frames: usize,
}

impl Kernel {
    /// Prepare `ir` (its own rate) for running at `sr` in blocks of `b`. With `normalize` the impulse is brought to
    /// one overall strength, so quiet and loud impulses come out alike.
    pub fn new(ir: &Sample, sr: f32, b: usize, normalize: bool) -> Self {
        let ratio = ir.rate.max(1) as f64 / sr.max(1.0) as f64;
        let want = ((ir.frames.len() as f64 / ratio) as usize).min((MAX_SECONDS * sr) as usize);
        let mut h: [Vec<f32>; 2] = [Vec::with_capacity(want), Vec::with_capacity(want)];
        for i in 0..want {
            let f = if (ratio - 1.0).abs() < 1e-9 { ir.frames[i] } else { ir.at(i as f64 * ratio) };
            h[0].push(f[0]);
            h[1].push(f[1]);
        }
        // drop the inaudible end
        let peak = h.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
        let floor = peak * 1e-5;
        let mut len = want;
        while len > 0 && h[0][len - 1].abs() <= floor && h[1][len - 1].abs() <= floor {
            len -= 1;
        }
        for c in &mut h {
            c.truncate(len);
        }
        if normalize {
            let e = h.iter().map(|c| c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>()).fold(0f64, f64::max);
            if e > 0.0 {
                let g = (1.0 / e.sqrt()) as f32;
                for v in h.iter_mut().flatten() {
                    *v *= g;
                }
            }
        }
        let n = 2 * b;
        let plan = Plan::new(n);
        let parts = if len > b { (len - b).div_ceil(b) } else { 0 };
        let bins = b + 1;
        let mut head = [vec![0f32; b], vec![0f32; b]];
        let mut spec = [vec![0f32; parts * bins * 2], vec![0f32; parts * bins * 2]];
        let (mut re, mut im) = (vec![0f32; n], vec![0f32; n]);
        for c in 0..2 {
            for j in 0..b.min(len) {
                head[c][b - 1 - j] = h[c][j];
            }
            for k in 0..parts {
                re.fill(0.0);
                im.fill(0.0);
                let from = b + k * b;
                let to = (from + b).min(len);
                re[..to - from].copy_from_slice(&h[c][from..to]);
                plan.run(&mut re, &mut im, false);
                let at = k * bins * 2;
                spec[c][at..at + bins].copy_from_slice(&re[..bins]);
                spec[c][at + bins..at + 2 * bins].copy_from_slice(&im[..bins]);
            }
        }
        Self { b, plan, head, spec, parts, frames: len }
    }
}

/// The running memory of one convolution.
pub struct State {
    pub kernel: Arc<Kernel>,
    hist: [Vec<f32>; 2],
    hpos: usize,
    inbuf: [Vec<f32>; 2],
    pos: usize,
    /// Spectra of the latest input blocks, newest at `slot`.
    fdl: [Vec<f32>; 2],
    slot: usize,
    out: [Vec<f32>; 2],
    re: Vec<f32>,
    im: Vec<f32>,
}

impl State {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let b = kernel.b;
        let size = kernel.parts * (b + 1) * 2;
        Self {
            hist: [vec![0.0; 2 * b], vec![0.0; 2 * b]],
            hpos: 0,
            inbuf: [vec![0.0; 2 * b], vec![0.0; 2 * b]],
            pos: 0,
            fdl: [vec![0.0; size], vec![0.0; size]],
            slot: 0,
            out: [vec![0.0; b], vec![0.0; b]],
            re: vec![0.0; 2 * b],
            im: vec![0.0; 2 * b],
            kernel,
        }
    }

    /// One frame in, the convolved frame out.
    pub fn tick(&mut self, x: [f32; 2]) -> [f32; 2] {
        let k = &*self.kernel;
        let b = k.b;
        let mut y = [0f32; 2];
        for c in 0..2 {
            self.hist[c][self.hpos] = x[c];
            self.hist[c][self.hpos + b] = x[c];
            let win = &self.hist[c][self.hpos + 1..self.hpos + 1 + b];
            let mut acc = 0f32;
            for (a, h) in win.iter().zip(&k.head[c]) {
                acc += a * h;
            }
            y[c] = acc + self.out[c][self.pos];
            self.inbuf[c][b + self.pos] = x[c];
        }
        self.hpos = (self.hpos + 1) % b;
        self.pos += 1;
        if self.pos == b {
            self.pos = 0;
            self.block();
        }
        y
    }

    fn block(&mut self) {
        let k = &*self.kernel;
        let (b, n, bins, parts) = (k.b, 2 * k.b, k.b + 1, k.parts);
        if parts == 0 {
            return;
        }
        self.slot = (self.slot + parts - 1) % parts;
        for c in 0..2 {
            self.re.copy_from_slice(&self.inbuf[c]);
            self.im.fill(0.0);
            k.plan.run(&mut self.re, &mut self.im, false);
            let at = self.slot * bins * 2;
            self.fdl[c][at..at + bins].copy_from_slice(&self.re[..bins]);
            self.fdl[c][at + bins..at + 2 * bins].copy_from_slice(&self.im[..bins]);
            self.re.fill(0.0);
            self.im.fill(0.0);
            for p in 0..parts {
                let xa = ((self.slot + p) % parts) * bins * 2;
                let ha = p * bins * 2;
                let (xr, xi) = self.fdl[c][xa..xa + 2 * bins].split_at(bins);
                let (hr, hi) = k.spec[c][ha..ha + 2 * bins].split_at(bins);
                let (ar, ai) = (&mut self.re[..bins], &mut self.im[..bins]);
                for i in 0..bins {
                    ar[i] += xr[i] * hr[i] - xi[i] * hi[i];
                    ai[i] += xr[i] * hi[i] + xi[i] * hr[i];
                }
            }
            for i in 1..b {
                self.re[n - i] = self.re[i];
                self.im[n - i] = -self.im[i];
            }
            k.plan.run(&mut self.re, &mut self.im, true);
            let g = 1.0 / n as f32;
            for i in 0..b {
                self.out[c][i] = self.re[b + i] * g;
            }
            self.inbuf[c].copy_within(b.., 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, mut s: u32) -> Vec<f32> {
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn matches_the_sum_done_the_long_way_with_no_delay() {
        let h = noise(700, 3);
        let ir = Sample::from_mono("h", 44_100, &h);
        let x = noise(1500, 9);
        for b in [64usize, 256, 1024] {
            let mut st = State::new(Arc::new(Kernel::new(&ir, 44_100.0, b, false)));
            for n in 0..x.len() {
                let y = st.tick([x[n], x[n]]);
                let mut want = 0f64;
                for j in 0..h.len().min(n + 1) {
                    want += h[j] as f64 * x[n - j] as f64;
                }
                assert!((y[0] as f64 - want).abs() < 2e-3, "block {b} frame {n}: {} vs {want}", y[0]);
                assert_eq!(y[0], y[1]);
            }
        }
    }

    #[test]
    fn normalizing_brings_impulses_to_one_strength() {
        let quiet: Vec<f32> = noise(3000, 5).iter().map(|v| v * 0.01).collect();
        let k = Kernel::new(&Sample::from_mono("q", 44_100, &quiet), 44_100.0, 256, true);
        let mut st = State::new(Arc::new(k));
        let mut e = 0f64;
        for n in 0..4000 {
            let y = st.tick(if n == 0 { [1.0, 1.0] } else { [0.0, 0.0] });
            e += (y[0] as f64).powi(2);
        }
        assert!((e - 1.0).abs() < 0.01, "{e}");
    }
}
