//! What follows the impulse in a stored impulse reverb block: the envelopes drawn over the impulse, its
//! equaliser curve and the settings that reshape it before it is used. Laid out as: four doubles and a float;
//! four envelopes (pan, volume, stereo separation, free) and the equaliser curve, each a 12 byte head, 24 bytes a
//! point (time since the point before, level, tension, kind) and 20 bytes after; four switch bytes; in the newest
//! layout the reported delay; then five whole numbers.
//!
//! No shipped setting moves the last numbers off their rest values, so which is which follows the order of the
//! controls, and their scales are our estimate.

use crate::sample::Sample;

/// Settings added after the four level controls: normalize, delay, self-convolution, stretch, equaliser amount.
pub const EXTRA: usize = 5;
pub const EXTRA_DEF: [f32; EXTRA] = [1.0, 0.0, 0.0, 0.0, 1000.0];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Env {
    /// Time from the start (seconds; the equaliser's runs 0 to 1 across the spectrum), level 0 to 1, tension of
    /// the stretch that ends at the point.
    pub pts: Vec<(f64, f32, f32)>,
}

impl Env {
    pub fn at(&self, x: f64) -> f32 {
        let p = &self.pts;
        let Some(first) = p.first() else { return 0.5 };
        if x <= first.0 {
            return first.1;
        }
        for w in p.windows(2) {
            if x < w[1].0 {
                let t = ((x - w[0].0) / (w[1].0 - w[0].0).max(1e-12)) as f32;
                return w[0].1 + (w[1].1 - w[0].1) * crate::fm::bend(t, w[1].2);
            }
        }
        p[p.len() - 1].1
    }

    /// True when the line sits on `level` throughout.
    pub fn flat(&self, level: f32) -> bool {
        self.pts.iter().all(|q| (q.1 - level).abs() < 1e-6)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tail {
    pub envs: [Env; 5],
    /// Where the switch bytes sit, and whether the reported delay follows them.
    knobs: usize,
    wide: bool,
}

fn layout(version: u32) -> u32 {
    if version > 0xffff { version >> 16 } else { version }
}

pub fn parse(tail: &[u8], version: u32) -> Option<Tail> {
    let mut at = 36;
    let mut envs: [Env; 5] = Default::default();
    for e in &mut envs {
        let n = u32::from_le_bytes(tail.get(at + 8..at + 12)?.try_into().ok()?) as usize;
        if n > 100_000 {
            return None;
        }
        at += 12;
        let mut x = 0f64;
        for _ in 0..n {
            let q = tail.get(at..at + 24)?;
            x += f64::from_le_bytes(q[..8].try_into().ok()?);
            let y = f64::from_le_bytes(q[8..16].try_into().ok()?) as f32;
            let tension = f32::from_le_bytes(q[16..20].try_into().ok()?);
            if !x.is_finite() || !y.is_finite() || !tension.is_finite() {
                return None;
            }
            e.pts.push((x, y, tension));
            at += 24;
        }
        at += 20;
    }
    let wide = layout(version) >= 15;
    (tail.len() == at + if wide { 28 } else { 24 }).then_some(Tail { envs, knobs: at, wide })
}

impl Tail {
    fn numbers(&self) -> usize {
        self.knobs + if self.wide { 12 } else { 8 }
    }

    pub fn read(&self, tail: &[u8]) -> [f32; EXTRA] {
        let at = self.numbers();
        let n = |i: usize| i32::from_le_bytes(tail[at + i * 4..at + i * 4 + 4].try_into().unwrap()) as f32;
        [tail[self.knobs + 3] as f32, n(0), n(1), n(2), n(3)]
    }

    pub fn write(&self, tail: &mut [u8], v: &[f32]) {
        let at = self.numbers();
        tail[self.knobs + 3] = (v[0] >= 0.5) as u8;
        for i in 0..4 {
            tail[at + i * 4..at + i * 4 + 4].copy_from_slice(&(v[1 + i].round() as i32).to_le_bytes());
        }
    }
}

/// The tail of a new block for an impulse `seconds` long: every line level, nothing reshaped.
pub fn fresh(seconds: f64) -> Vec<u8> {
    let mut v = Vec::with_capacity(460);
    for d in [-1.0, -1.0, 0.0, seconds] {
        v.extend_from_slice(&f64::to_le_bytes(d));
    }
    v.extend_from_slice(&0.02f32.to_le_bytes());
    for k in 0..5 {
        for n in [if k == 4 { 3u32 } else { 6 }, 3, 2] {
            v.extend_from_slice(&n.to_le_bytes());
        }
        for (dx, kind) in [(0.0, 0u32), (if k == 4 { 1.0 } else { seconds }, 0x0200_0000)] {
            v.extend_from_slice(&f64::to_le_bytes(dx));
            v.extend_from_slice(&0.5f64.to_le_bytes());
            v.extend_from_slice(&0f32.to_le_bytes());
            v.extend_from_slice(&kind.to_le_bytes());
        }
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&[0xff; 16]);
    }
    v.extend_from_slice(&[0, 0, 1, 1]);
    for n in [0i32, 0, 0, 0, 1000] {
        v.extend_from_slice(&n.to_le_bytes());
    }
    v
}

/// Level of the equaliser line as a gain: the top is four times (+12 dB), the middle leaves alone, the bottom
/// silences.
fn eq_gain(y: f32) -> f32 {
    let a = (y * 2.0).clamp(0.0, 2.0);
    a * a
}

fn pow2(n: usize) -> usize {
    n.max(2).next_power_of_two()
}

/// The impulse as it is used: drawn envelopes applied, equalised, convolved with itself, stretched and delayed.
/// `extra` holds the settings of `EXTRA` after the normalize switch is taken out by the caller.
pub fn shape(ir: &Sample, extra: &[f32], tail: Option<&Tail>) -> Sample {
    let (delay, selfc, stretch, eq) = (extra[1], (extra[2] / 1000.0).clamp(0.0, 1.0), extra[3], (extra[4] / 1000.0).clamp(0.0, 1.0));
    let rate = ir.rate.max(1) as f64;
    let mut h = ir.frames.clone();
    if let Some(t) = tail {
        let [pan, vol, sep, _, _] = &t.envs;
        if !(pan.flat(0.5) && vol.flat(0.5) && sep.flat(0.5)) {
            for (i, f) in h.iter_mut().enumerate() {
                let x = i as f64 / rate;
                let g = vol.at(x) * 2.0;
                let side = (2.0 - sep.at(x) * 2.0).max(0.0);
                let (m, s) = ((f[0] + f[1]) * 0.5, (f[0] - f[1]) * 0.5 * side);
                let b = pan.at(x) * 2.0 - 1.0;
                *f = [(m + s) * g * (1.0 - b).min(1.0), (m - s) * g * (1.0 + b).min(1.0)];
            }
        }
        let line = &t.envs[4];
        if eq > 0.0 && !line.flat(0.5) && !h.is_empty() {
            // the same gain on every side of the spectrum and no turn of phase
            let n = pow2(h.len() * 2);
            let plan = crate::conv::Plan::new(n);
            let gains: Vec<f32> = (0..=n / 2)
                .map(|k| {
                    let hz = k as f64 * rate / n as f64;
                    let x = ((hz.max(1.0) / 20.0).ln() / 1000f64.ln()).clamp(0.0, 1.0);
                    1.0 + (eq_gain(line.at(x)) - 1.0) * eq
                })
                .collect();
            let (mut re, mut im) = (vec![0f32; n], vec![0f32; n]);
            for c in 0..2 {
                re.fill(0.0);
                im.fill(0.0);
                for (r, f) in re.iter_mut().zip(&h) {
                    *r = f[c];
                }
                plan.run(&mut re, &mut im, false);
                for k in 0..n {
                    let g = gains[k.min(n - k)] / n as f32;
                    re[k] *= g;
                    im[k] *= g;
                }
                plan.run(&mut re, &mut im, true);
                for (r, f) in re.iter().zip(h.iter_mut()) {
                    f[c] = *r;
                }
            }
        }
    }
    if selfc > 0.0 && !h.is_empty() {
        let len = (h.len() * 2 - 1).min((rate * 30.0) as usize).max(1);
        let n = pow2(h.len() * 2);
        let plan = crate::conv::Plan::new(n);
        let (mut re, mut im) = (vec![0f32; n], vec![0f32; n]);
        let mut twice = vec![[0f32; 2]; len];
        for c in 0..2 {
            re.fill(0.0);
            im.fill(0.0);
            for (r, f) in re.iter_mut().zip(&h) {
                *r = f[c];
            }
            plan.run(&mut re, &mut im, false);
            for k in 0..n {
                let (a, b) = (re[k], im[k]);
                re[k] = (a * a - b * b) / n as f32;
                im[k] = 2.0 * a * b / n as f32;
            }
            plan.run(&mut re, &mut im, true);
            for (r, f) in re.iter().zip(twice.iter_mut()) {
                f[c] = *r;
            }
        }
        // as strong as the impulse it came from, then the two are mixed
        let energy = |v: &[[f32; 2]]| v.iter().map(|f| (f[0] as f64).powi(2) + (f[1] as f64).powi(2)).sum::<f64>();
        let (e0, e1) = (energy(&h), energy(&twice));
        let g = if e1 > 0.0 { (e0 / e1).sqrt() as f32 } else { 0.0 };
        h.resize(len, [0.0; 2]);
        for (f, t) in h.iter_mut().zip(&twice) {
            for c in 0..2 {
                f[c] = f[c] * (1.0 - selfc) + t[c] * g * selfc;
            }
        }
    }
    // stretching plays the impulse slower or faster, pitch and all: an octave either way at the ends
    let factor = (stretch as f64 / 1000.0).clamp(-1.0, 1.0).exp2();
    let rate = rate / factor;
    let lead = (delay.clamp(0.0, 1000.0) as f64 / 1000.0 * rate) as usize;
    if lead > 0 {
        h.splice(0..0, std::iter::repeat_n([0.0; 2], lead));
    }
    Sample { name: ir.name.clone(), rate: rate.round().max(1.0) as u32, mono: ir.mono, frames: h }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_tail_reads_back_and_takes_new_settings() {
        let mut t = fresh(2.5);
        assert_eq!(t.len(), 460);
        let tail = parse(&t, 14).unwrap();
        assert_eq!(tail.read(&t), EXTRA_DEF);
        assert!(tail.envs.iter().all(|e| e.flat(0.5) && e.pts.len() == 2));
        assert_eq!(tail.envs[1].pts[1].0, 2.5);
        tail.write(&mut t, &[0.0, 120.0, 300.0, -500.0, 250.0]);
        assert_eq!(parse(&t, 14).unwrap().read(&t), [0.0, 120.0, 300.0, -500.0, 250.0]);
        assert!(parse(&t, 15 << 16 | 3).is_none());
        assert!(parse(&t[..t.len() - 1], 14).is_none());
    }

    #[test]
    fn the_impulse_is_reshaped() {
        let mut d = vec![0f32; 1000];
        d[0] = 1.0;
        d[500] = 0.5;
        let ir = Sample::from_mono("h", 1000, &d);
        // untouched
        let same = shape(&ir, &EXTRA_DEF, None);
        assert_eq!((same.rate, &same.frames), (1000, &ir.frames));
        // delay puts silence in front, stretch slows the clock
        let late = shape(&ir, &[1.0, 100.0, 0.0, 1000.0, 1000.0], None);
        assert_eq!(late.rate, 500);
        assert_eq!(late.frames.len(), 1050);
        assert_eq!(late.frames[50][0], 1.0);
        // convolved with itself the echo at 500 turns up again at 1000
        let twice = shape(&ir, &[1.0, 0.0, 1000.0, 0.0, 1000.0], None);
        assert!(twice.frames.len() > 1000);
        let (a, b, c) = (twice.frames[0][0], twice.frames[500][0], twice.frames[1000][0]);
        assert!((b / a - 1.0).abs() < 1e-3 && (c / a - 0.25).abs() < 1e-3, "{a} {b} {c}");
        // a volume line falling to nothing fades the tail; an equaliser line at the bottom silences
        let mut t = parse(&fresh(1.0), 14).unwrap();
        t.envs[1].pts[1].1 = 0.0;
        let faded = shape(&ir, &EXTRA_DEF, Some(&t));
        assert!((faded.frames[0][0] - 1.0).abs() < 1e-6 && (faded.frames[500][0] - 0.25).abs() < 1e-3);
        let mut t = parse(&fresh(1.0), 14).unwrap();
        t.envs[4].pts.iter_mut().for_each(|q| q.1 = 0.0);
        let gone = shape(&ir, &EXTRA_DEF, Some(&t));
        assert!(gone.frames.iter().all(|f| f[0].abs() < 1e-4));
        let half = shape(&ir, &[1.0, 0.0, 0.0, 0.0, 500.0], Some(&t));
        assert!((half.frames[0][0] - 0.5).abs() < 1e-3);
    }
}
