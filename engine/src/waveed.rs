//! The plain operations of the wave editor, on stereo frames. Every one takes the stretch `a..b` it works on
//! and leaves the rest alone.

type Frames = Vec<[f32; 2]>;

fn span(f: &Frames, a: usize, b: usize) -> (usize, usize) {
    let b = b.min(f.len());
    (a.min(b), b)
}

/// The loudest sample in the stretch.
pub fn peak(f: &Frames, a: usize, b: usize) -> f32 {
    let (a, b) = span(f, a, b);
    f[a..b].iter().fold(0.0f32, |m, x| m.max(x[0].abs()).max(x[1].abs()))
}

pub fn gain(f: &mut Frames, a: usize, b: usize, g: f32) {
    let (a, b) = span(f, a, b);
    for x in &mut f[a..b] {
        x[0] *= g;
        x[1] *= g;
    }
}

/// Bring the loudest sample of the stretch to full scale. Silence stays silence.
pub fn normalize(f: &mut Frames, a: usize, b: usize) {
    let p = peak(f, a, b);
    if p > 1e-6 {
        gain(f, a, b, 1.0 / p);
    }
}

/// A straight fade over the stretch: up from nothing, or down to nothing.
pub fn fade(f: &mut Frames, a: usize, b: usize, up: bool) {
    let (a, b) = span(f, a, b);
    let n = (b - a).max(1) as f32;
    for (i, x) in f[a..b].iter_mut().enumerate() {
        let u = i as f32 / n;
        let g = if up { u } else { 1.0 - u };
        x[0] *= g;
        x[1] *= g;
    }
}

pub fn reverse(f: &mut Frames, a: usize, b: usize) {
    let (a, b) = span(f, a, b);
    f[a..b].reverse();
}

pub fn silence(f: &mut Frames, a: usize, b: usize) {
    let (a, b) = span(f, a, b);
    f[a..b].fill([0.0; 2]);
}

/// Take the stretch out; what came after closes up. Gives back what was taken.
pub fn remove(f: &mut Frames, a: usize, b: usize) -> Frames {
    let (a, b) = span(f, a, b);
    f.drain(a..b).collect()
}

/// Keep only the stretch.
pub fn trim(f: &mut Frames, a: usize, b: usize) {
    let (a, b) = span(f, a, b);
    f.truncate(b);
    f.drain(..a);
}

pub fn insert(f: &mut Frames, at: usize, clip: &[[f32; 2]]) {
    let at = at.min(f.len());
    f.splice(at..at, clip.iter().copied());
}

/// Take out a standing offset, each side by itself.
pub fn centre(f: &mut Frames, a: usize, b: usize) {
    let (a, b) = span(f, a, b);
    if b == a {
        return;
    }
    for c in 0..2 {
        let mean = f[a..b].iter().map(|x| x[c] as f64).sum::<f64>() / (b - a) as f64;
        for x in &mut f[a..b] {
            x[c] -= mean as f32;
        }
    }
}

/// Swap left and right.
pub fn swap(f: &mut Frames, a: usize, b: usize) {
    let (a, b) = span(f, a, b);
    for x in &mut f[a..b] {
        x.swap(0, 1);
    }
}

/// Lowest and highest value per picture column over `a..b`, both sides together. Long stretches are sampled, not
/// read in full, so the picture of an hour costs the same as that of a second.
pub fn columns(f: &Frames, a: usize, b: usize, cols: usize) -> Vec<(f32, f32)> {
    let (a, b) = span(f, a, b);
    let n = b - a;
    (0..cols)
        .map(|c| {
            let (s, e) = (a + n * c / cols.max(1), (a + n * (c + 1) / cols.max(1)).max(a + n * c / cols.max(1) + 1).min(b));
            if s >= e {
                return (0.0, 0.0);
            }
            let step = ((e - s) / 96).max(1);
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            let mut i = s;
            while i < e {
                let m = (f[i][0] + f[i][1]) * 0.5;
                lo = lo.min(m.min(f[i][0]).min(f[i][1]));
                hi = hi.max(m.max(f[i][0]).max(f[i][1]));
                i += step;
            }
            (lo, hi)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Frames {
        (0..n).map(|i| [i as f32 / n as f32 * 0.5, -(i as f32) / n as f32 * 0.25]).collect()
    }

    #[test]
    fn the_stretch_is_changed_and_the_rest_is_left() {
        let mut f = ramp(100);
        let before = f.clone();
        normalize(&mut f, 20, 60);
        assert!((peak(&f, 20, 60) - 1.0).abs() < 1e-6);
        assert_eq!(f[..20], before[..20]);
        assert_eq!(f[60..], before[60..]);
        let mut g = before.clone();
        fade(&mut g, 0, 50, true);
        assert_eq!(g[0], [0.0, 0.0]);
        assert_eq!(g[50..], before[50..]);
        fade(&mut g, 50, 100, false);
        assert!(g[99][0].abs() < before[99][0].abs() * 0.05);
        let mut r = before.clone();
        reverse(&mut r, 10, 20);
        assert_eq!(r[10], before[19]);
        assert_eq!(r[20], before[20]);
    }

    #[test]
    fn cutting_and_pasting_keeps_every_frame() {
        let mut f = ramp(100);
        let before = f.clone();
        let cut = remove(&mut f, 30, 40);
        assert_eq!((f.len(), cut.len()), (90, 10));
        assert_eq!(f[30], before[40]);
        insert(&mut f, 30, &cut);
        assert_eq!(f, before);
        trim(&mut f, 10, 25);
        assert_eq!(f, before[10..25]);
        // ranges past the end are cut to size and never panic
        silence(&mut f, 10, 5000);
        assert_eq!(f[14], [0.0, 0.0]);
        assert!(remove(&mut f, 400, 500).is_empty());
        let mut d: Frames = vec![[0.5, -0.25]; 64];
        centre(&mut d, 0, 64);
        assert!(peak(&d, 0, 64) < 1e-6);
        assert_eq!(columns(&before, 0, 100, 10).len(), 10);
        assert!(columns(&before, 0, 100, 10)[9].1 > 0.4);
    }
}
