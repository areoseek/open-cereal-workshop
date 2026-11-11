//! What the meters and pictures of the pass-through units are worked out from: the size of each
//! frequency in a stretch of sound, and the pitch of a single note in it.

use std::f32::consts::PI;

/// The level of each frequency in `x`, whose length is cut down to a power of two. Entry `k` of the
/// result belongs to the frequency `k * rate / length`; a full-scale sine reads close to 1.
pub fn spectrum(x: &[f32]) -> Vec<f32> {
    let n = if x.is_empty() { 0 } else { 1usize << (usize::BITS - 1 - x.len().leading_zeros()) };
    if n < 2 {
        return Vec::new();
    }
    let x = &x[x.len() - n..];
    // a raised-cosine window keeps a steady tone from smearing over its neighbours
    let mut re: Vec<f32> = x.iter().enumerate().map(|(i, v)| v * (0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos())).collect();
    let mut im = vec![0.0f32; n];
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
    // the window halves a sine's reading and only half the result is kept, hence the 4
    (0..n / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt() * 4.0 / n as f32).collect()
}

/// The pitch of the note in `x` in cycles a second, with how sure the reading is (0..=1), or nothing
/// for silence and noise. Looks for the shortest shift at which the sound lines up with itself.
pub fn pitch(x: &[f32], rate: f32) -> Option<(f32, f32)> {
    let n = x.len();
    if n < 512 || rate <= 0.0 {
        return None;
    }
    let power: f32 = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
    if power < 1e-7 {
        return None;
    }
    let lo = (rate / 2000.0).max(2.0) as usize;
    let hi = ((rate / 25.0) as usize).min(n / 2);
    if hi <= lo + 2 {
        return None;
    }
    // how well the sound matches itself at each shift, 1 = exactly
    let w = n - hi;
    let score = |lag: usize| -> f32 {
        let (mut ac, mut m) = (0.0f32, 0.0f32);
        for i in 0..w {
            ac += x[i] * x[i + lag];
            m += x[i] * x[i] + x[i + lag] * x[i + lag];
        }
        if m > 0.0 {
            2.0 * ac / m
        } else {
            0.0
        }
    };
    let s: Vec<f32> = (lo - 1..=hi).map(score).collect();
    let best = s.iter().cloned().fold(0.0f32, f32::max);
    if best < 0.6 {
        return None;
    }
    // the first peak nearly as good as the best one: later ones are the same note counted in twos and threes
    let mut k = 1;
    while k + 1 < s.len() {
        if s[k] > best * 0.93 && s[k] >= s[k - 1] && s[k] >= s[k + 1] {
            break;
        }
        k += 1;
    }
    if k + 1 >= s.len() {
        return None;
    }
    // the peak lies between samples: fit a curve through its neighbours
    let (a, b, c) = (s[k - 1], s[k], s[k + 1]);
    let d = a - 2.0 * b + c;
    let shift = if d.abs() > 1e-9 { 0.5 * (a - c) / d } else { 0.0 };
    let lag = (lo - 1 + k) as f32 + shift.clamp(-1.0, 1.0);
    Some((rate / lag, b.clamp(0.0, 1.0)))
}

/// A frequency as a key number with the cents it is off by (key 69 is the A at 440).
pub fn nearest_key(hz: f32) -> (i32, f32) {
    let k = 69.0 + 12.0 * (hz / 440.0).log2();
    let key = k.round();
    (key as i32, (k - key) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, rate: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * hz * i as f32 / rate).sin()).collect()
    }

    #[test]
    fn a_sine_shows_up_at_its_own_frequency() {
        let rate = 48_000.0;
        let s = spectrum(&sine(3000.0, rate, 4096));
        assert_eq!(s.len(), 2048);
        let top = s.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        assert_eq!(top.0, 256);
        assert!((top.1 - 1.0).abs() < 0.05, "{}", top.1);
        assert!(s[20] < 1e-3 && s[1000] < 1e-3);
        assert!(spectrum(&[]).is_empty());
    }

    #[test]
    fn notes_are_named_right() {
        let rate = 48_000.0;
        for hz in [55.0f32, 110.0, 261.63, 440.0, 987.77, 1500.0] {
            let (got, sure) = pitch(&sine(hz, rate, 4096), rate).unwrap();
            assert!((got / hz - 1.0).abs() < 0.004, "{hz} read as {got}");
            assert!(sure > 0.9);
        }
        // a note with strong overtones still reads at its lowest one
        let x: Vec<f32> = (0..4096).map(|i| { let t = i as f32 / rate; (2.0 * PI * 220.0 * t).sin() * 0.4 + (2.0 * PI * 440.0 * t).sin() * 0.6 + (2.0 * PI * 660.0 * t).sin() * 0.5 }).collect();
        let (got, _) = pitch(&x, rate).unwrap();
        assert!((got / 220.0 - 1.0).abs() < 0.01, "{got}");
        assert!(pitch(&vec![0.0; 4096], rate).is_none());
        assert_eq!(nearest_key(440.0).0, 69);
        let (k, c) = nearest_key(446.0);
        assert!(k == 69 && c > 20.0 && c < 26.0);
    }
}
