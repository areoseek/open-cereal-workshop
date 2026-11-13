//! Finding the notes in a sung or played line and moving them: the pitch is followed through the sound,
//! stretches of steady pitch become notes, and each note can be moved up or down and its wobble flattened.
//! The sound is rebuilt one cycle at a time, so the colour of the voice stays where it was.

use crate::waveed::Frames;

/// The pitch through a sound: one reading every `hop` frames, in cycles a second, 0 where there is no clear note.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub hop: usize,
    pub hz: Vec<f32>,
}

/// One found note: frames `a..b`, and its middle pitch as a key number with fraction (69 = the A at 440).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PNote {
    pub a: usize,
    pub b: usize,
    pub key: f32,
}

/// What is done to a note: moved by so many semitones, its own wobble kept by this share (1 = as sung, 0 = dead level).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
    pub semis: f32,
    pub wobble: f32,
}

impl Default for Move {
    fn default() -> Self {
        Self { semis: 0.0, wobble: 1.0 }
    }
}

pub fn key_of(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// Follow the pitch of a sound, a reading every hundredth of a second.
pub fn track(f: &Frames, rate: u32) -> Track {
    let hop = (rate as usize / 100).max(1);
    // a quarter of the frames is plenty for the pitch of a voice, and four times quicker
    let by = (rate as usize / 11_000).max(1);
    let small: Vec<f32> = f.chunks(by).map(|c| c.iter().map(|s| s[0] + s[1]).sum::<f32>() / (2 * c.len()) as f32).collect();
    let srate = rate as f32 / by as f32;
    let win = 1024usize;
    let n = f.len() / hop;
    let mut hz = vec![0.0f32; n];
    for (i, h) in hz.iter_mut().enumerate() {
        let mid = i * hop / by;
        let a = mid.saturating_sub(win / 2);
        let b = (a + win).min(small.len());
        if b - a < 512 {
            continue;
        }
        if let Some((p, sure)) = crate::scope::pitch(&small[a..b], srate) {
            if sure > 0.8 && (40.0..=1600.0).contains(&p) {
                *h = p;
            }
        }
    }
    // a lone reading an octave off its neighbours is a slip of the reader
    for i in 1..n.saturating_sub(1) {
        let (l, m, r) = (hz[i - 1], hz[i], hz[i + 1]);
        if l > 0.0 && r > 0.0 && m > 0.0 && (l / r).log2().abs() < 0.1 && (m / l).log2().abs() > 0.8 {
            hz[i] = (l + r) * 0.5;
        }
    }
    Track { hop, hz }
}

/// The notes of a followed sound: runs of readings that stay within about a semitone, at least `min_ms` long.
pub fn notes(t: &Track, rate: u32, min_ms: f32) -> Vec<PNote> {
    let mut out = vec![];
    let min = ((min_ms / 1000.0 * rate as f32) as usize / t.hop.max(1)).max(2);
    let mut i = 0;
    let n = t.hz.len();
    while i < n {
        if t.hz[i] <= 0.0 {
            i += 1;
            continue;
        }
        let start = i;
        let mut sum = key_of(t.hz[i]);
        let mut count = 1.0f32;
        i += 1;
        while i < n && t.hz[i] > 0.0 {
            let k = key_of(t.hz[i]);
            // the first readings may still be sliding in: the middle settles as it goes
            if (k - sum / count).abs() > 0.75 && count >= 3.0 {
                break;
            }
            sum += k;
            count += 1.0;
            i += 1;
        }
        if i - start >= min {
            // the middle pitch is the middle reading, which a slide in or out does not pull about
            let mut keys: Vec<f32> = t.hz[start..i].iter().map(|h| key_of(*h)).collect();
            keys.sort_by(|a, b| a.total_cmp(b));
            out.push(PNote { a: start * t.hop, b: i * t.hop, key: keys[keys.len() / 2] });
        }
    }
    out
}

/// Rebuild the sound with every note moved as asked. `glide_ms` is how long a change of the move takes at the
/// edges of a note. The length stays the same.
pub fn retune(f: &Frames, rate: u32, t: &Track, notes: &[PNote], moves: &[Move], glide_ms: f32) -> Frames {
    let n = f.len();
    if n == 0 || t.hz.is_empty() {
        return f.clone();
    }
    let hop = t.hop.max(1);
    // by how many semitones each reading moves
    let mut by = vec![0.0f32; t.hz.len()];
    for (note, m) in notes.iter().zip(moves.iter()) {
        for i in note.a / hop..(note.b / hop).min(by.len()) {
            if t.hz[i] > 0.0 {
                by[i] = m.semis + (1.0 - m.wobble.clamp(0.0, 2.0)) * (note.key - key_of(t.hz[i]));
            }
        }
    }
    // the edges of a move are eased
    let ease = ((glide_ms / 1000.0 * rate as f32) as usize / hop).min(50);
    if ease > 0 {
        let src = by.clone();
        for i in 0..by.len() {
            let (a, b) = (i.saturating_sub(ease), (i + ease + 1).min(src.len()));
            // only between sounding readings: silence in between must not drag a note's start down
            let (mut s, mut c) = (0.0, 0.0);
            for k in a..b {
                if t.hz[k] > 0.0 {
                    s += src[k];
                    c += 1.0;
                }
            }
            if c > 0.0 && t.hz[i] > 0.0 {
                by[i] = s / c;
            }
        }
    }
    let mono: Vec<f32> = f.iter().map(|s| (s[0] + s[1]) * 0.5).collect();
    let at_hop = |pos: f64| ((pos / hop as f64) as usize).min(t.hz.len() - 1);
    let rest = rate as f64 / 200.0;
    let period = |pos: f64| {
        let h = t.hz[at_hop(pos)];
        if h > 0.0 { rate as f64 / h as f64 } else { rest }
    };
    // where the cycles of the sound start: a step of one cycle, pulled onto the nearest peak
    let mut marks: Vec<(f64, f64)> = Vec::with_capacity(n / 100);
    let mut pos = 0.0f64;
    while pos < n as f64 {
        let p = period(pos);
        let mut at = pos;
        if t.hz[at_hop(pos)] > 0.0 {
            let reach = (p * 0.25) as isize;
            let c = pos as isize;
            let (lo, hi) = ((c - reach).max(0) as usize, ((c + reach).max(0) as usize).min(n - 1));
            if let Some(best) = (lo..=hi).max_by(|a, b| mono[*a].total_cmp(&mono[*b])) {
                at = best as f64;
            }
        }
        marks.push((at, p));
        pos = (at + p).max(pos + p * 0.5);
    }
    let mut out = vec![[0.0f32; 2]; n];
    let mut weight = vec![0.0f32; n];
    let mut s = marks[0].0;
    let mut mi = 0usize;
    while s < n as f64 {
        // the cycle of the old sound nearest to this moment
        while mi + 1 < marks.len() && (marks[mi + 1].0 - s).abs() <= (marks[mi].0 - s).abs() {
            mi += 1;
        }
        let (centre, p) = marks[mi];
        let half = p.round().max(2.0) as isize;
        let (c, d) = (centre.round() as isize, s.round() as isize);
        for k in -half..half {
            let (from, to) = (c + k, d + k);
            if from < 0 || to < 0 || from as usize >= n || to as usize >= n {
                continue;
            }
            let w = 0.5 * (1.0 + (std::f32::consts::PI * k as f32 / half as f32).cos());
            let x = f[from as usize];
            let o = &mut out[to as usize];
            o[0] += x[0] * w;
            o[1] += x[1] * w;
            weight[to as usize] += w;
        }
        let voiced = t.hz[at_hop(s)] > 0.0;
        let ratio = if voiced { 2f64.powf(by[at_hop(s)] as f64 / 12.0) } else { 1.0 };
        // the step is the old sound's own cycle here, so an unmoved sound falls back onto itself
        let step = if mi + 1 < marks.len() { marks[mi + 1].0 - marks[mi].0 } else { p };
        s += (step / ratio).max(2.0);
    }
    for (o, w) in out.iter_mut().zip(weight.iter()) {
        if *w > 1e-3 {
            o[0] /= w;
            o[1] /= w;
        }
    }
    out
}

/// Move pieces of a sound in time: `pins` are (where a moment is, where it should be), in frames. The stretches
/// between the pins are made longer or shorter at their own pitch; the result ends where the last pin says.
pub fn warp(f: &Frames, rate: u32, pins: &[(usize, usize)]) -> Frames {
    let n = f.len();
    let mut p: Vec<(usize, usize)> = pins.iter().copied().filter(|p| p.0 > 0 && p.0 < n).collect();
    p.sort_unstable();
    p.dedup_by_key(|p| p.0);
    // pins may not overtake each other
    let mut last = 0;
    for x in &mut p {
        x.1 = x.1.max(last + 64);
        last = x.1;
    }
    // the end moves with the last pin
    let end = p.last().map_or(n, |l| l.1 + (n - l.0));
    p.insert(0, (0, 0));
    p.push((n, end));
    let mut out: Frames = Vec::with_capacity(end);
    for w in p.windows(2) {
        let (a, b) = (w[0], w[1]);
        let want = b.1 - a.1;
        let src = &f[a.0..b.0];
        if src.is_empty() || want == 0 {
            continue;
        }
        let mut piece = if src.len() == want { src.to_vec() } else { crate::waveed::stretch(src, want as f32 / src.len() as f32, 0.0, rate) };
        piece.resize(want, [0.0; 2]);
        // a short blend over the join
        let fade = 64.min(piece.len()).min(out.len());
        let at = out.len();
        for k in 0..fade {
            let g = k as f32 / fade as f32;
            let tail = out[at - fade + k];
            let head = piece[k];
            out[at - fade + k] = [tail[0] * (1.0 - g) + head[0] * g, tail[1] * (1.0 - g) + head[1] * g];
        }
        out.extend_from_slice(&piece[fade..]);
        out.resize(b.1, [0.0; 2]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: &[(f32, f32)], rate: u32) -> Frames {
        let mut out = vec![];
        let mut ph = 0.0f32;
        for &(h, secs) in hz {
            for _ in 0..(secs * rate as f32) as usize {
                ph += h / rate as f32;
                // a few overtones, like a voice
                let p = ph * std::f32::consts::TAU;
                let s = if h > 0.0 { 0.5 * p.sin() + 0.25 * (2.0 * p).sin() + 0.12 * (3.0 * p).sin() } else { 0.0 };
                out.push([s, s]);
            }
        }
        out
    }

    #[test]
    fn notes_are_found_where_they_are_sung() {
        let f = tone(&[(220.0, 0.5), (0.0, 0.2), (330.0, 0.5)], 44_100);
        let t = track(&f, 44_100);
        let got = notes(&t, 44_100, 60.0);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!((got[0].key - 57.0).abs() < 0.1 && (got[1].key - key_of(330.0)).abs() < 0.1, "{got:?}");
        assert!(got[0].a < 4410 && got[0].b.abs_diff(22_050) < 3000 && got[1].a.abs_diff(30_870) < 3000, "{got:?}");
    }

    #[test]
    fn a_moved_note_sounds_at_its_new_pitch_and_the_rest_stays() {
        let f = tone(&[(220.0, 0.6), (330.0, 0.6)], 44_100);
        let t = track(&f, 44_100);
        let ns = notes(&t, 44_100, 60.0);
        assert_eq!(ns.len(), 2);
        let out = retune(&f, 44_100, &t, &ns, &[Move { semis: 2.0, wobble: 1.0 }, Move::default()], 10.0);
        assert_eq!(out.len(), f.len());
        let t2 = track(&out, 44_100);
        let got = notes(&t2, 44_100, 60.0);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!((got[0].key - 59.0).abs() < 0.15, "{got:?}");
        assert!((got[1].key - key_of(330.0)).abs() < 0.1, "{got:?}");
        // an untouched sound comes back close to itself
        let same = retune(&f, 44_100, &t, &ns, &[Move::default(), Move::default()], 10.0);
        let err: f32 = same.iter().zip(f.iter()).skip(2000).take(20_000).map(|(a, b)| (a[0] - b[0]).abs()).sum::<f32>() / 20_000.0;
        assert!(err < 0.05, "{err}");
    }

    #[test]
    fn a_pin_moves_its_moment_and_keeps_the_pitch() {
        let f = tone(&[(220.0, 0.5), (330.0, 0.5)], 44_100);
        // the change of note, at half a second, is put at three quarters
        let out = warp(&f, 44_100, &[(22_050, 33_075)]);
        assert_eq!(out.len(), 44_100 + 11_025);
        let t = track(&out, 44_100);
        let got = notes(&t, 44_100, 60.0);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!((got[0].key - 57.0).abs() < 0.2 && got[0].b.abs_diff(33_075) < 3000, "{got:?}");
        assert!((got[1].key - key_of(330.0)).abs() < 0.2, "{got:?}");
    }
}
