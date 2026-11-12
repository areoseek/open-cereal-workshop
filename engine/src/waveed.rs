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

/// A flag in the take: a bare marker, or with a length a region.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mark {
    pub at: usize,
    /// 0 for a marker.
    pub len: usize,
    pub name: String,
}

/// Keep the flags right after `a..b` was taken out: those inside go, those after move up.
pub fn marks_removed(marks: &mut Vec<Mark>, a: usize, b: usize) {
    marks.retain(|m| m.at < a || m.at >= b);
    for m in marks.iter_mut() {
        if m.at >= b {
            m.at -= b - a;
        } else if m.at + m.len > a {
            // a region reaching into the hole ends at it, or closes over it
            m.len = if m.at + m.len >= b { m.len - (b - a) } else { a - m.at };
        }
    }
}

/// Keep the flags right after `n` frames were put in at `at`.
pub fn marks_inserted(marks: &mut [Mark], at: usize, n: usize) {
    for m in marks.iter_mut() {
        if m.at >= at {
            m.at += n;
        } else if m.at + m.len > at {
            m.len += n;
        }
    }
}

/// Add `clip` on top of what is there from `at` on.
pub fn mix(f: &mut Frames, at: usize, clip: &[[f32; 2]]) {
    if f.len() < at + clip.len() {
        f.resize(at + clip.len(), [0.0; 2]);
    }
    for (x, c) in f[at..].iter_mut().zip(clip) {
        x[0] += c[0];
        x[1] += c[1];
    }
}

/// A short fade at one end of the stretch, to take a click off it.
pub fn declick(f: &mut Frames, a: usize, b: usize, rate: u32, at_start: bool) {
    let (a, b) = span(f, a, b);
    let n = ((rate as usize) / 200).min((b - a) / 2);
    if at_start {
        fade(f, a, a + n, true);
    } else {
        fade(f, b - n, b, false);
    }
}

/// How loud the take is around every frame: the peak over about ten milliseconds either side.
fn loudness(f: &Frames, a: usize, b: usize, rate: u32) -> (Vec<f32>, usize) {
    let hop = (rate as usize / 200).max(1);
    let blocks: Vec<f32> = f[a..b].chunks(hop).map(|c| c.iter().fold(0.0f32, |m, x| m.max(x[0].abs()).max(x[1].abs()))).collect();
    // each block also hears its neighbours, so a tail is not chopped at its first quiet instant
    let wide = (0..blocks.len()).map(|i| blocks[i.saturating_sub(2)..(i + 3).min(blocks.len())].iter().fold(0.0f32, |m, x| m.max(*x))).collect();
    (wide, hop)
}

/// The stretches inside `a..b` that are louder than `thresh`.
pub fn loud_parts(f: &Frames, a: usize, b: usize, thresh: f32, rate: u32) -> Vec<(usize, usize)> {
    let (a, b) = span(f, a, b);
    let (env, hop) = loudness(f, a, b, rate);
    let mut out: Vec<(usize, usize)> = vec![];
    let mut start = None;
    for (i, e) in env.iter().enumerate() {
        match (start, *e > thresh) {
            (None, true) => start = Some(a + i * hop),
            (Some(s), false) => {
                out.push((s, a + i * hop));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, b));
    }
    out
}

/// Silence everything in the stretch that is quieter than `thresh`, with short fades at the joins.
pub fn gate(f: &mut Frames, a: usize, b: usize, thresh: f32, rate: u32) {
    let (a, b) = span(f, a, b);
    let loud = loud_parts(f, a, b, thresh, rate);
    let mut at = a;
    for (s, e) in loud.iter().copied().chain([(b, b)]) {
        if s > at {
            let n = (rate as usize / 400).min((s - at) / 2);
            fade(f, at, at + n, false);
            silence(f, at + n, s - n);
            fade(f, s - n, s, true);
        }
        at = e;
    }
}

/// Where new sounds start inside the stretch: places where the level jumps well above what went just before.
/// `sense` runs from 0 (only the hardest hits) to 1 (the soft ones as well).
pub fn onsets(f: &Frames, a: usize, b: usize, rate: u32, sense: f32) -> Vec<usize> {
    let (a, b) = span(f, a, b);
    let hop = (rate as usize / 400).max(1);
    let env: Vec<f32> = f[a..b].chunks(hop).map(|c| (c.iter().map(|x| x[0] * x[0] + x[1] * x[1]).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let top = env.iter().fold(0.0f32, |m, x| m.max(*x)).max(1e-9);
    let ratio = 4.0 - 2.7 * sense.clamp(0.0, 1.0);
    let floor = top * (0.12 - 0.1 * sense.clamp(0.0, 1.0));
    let gap = (rate as usize / 20 / hop).max(1);
    let mut out = vec![];
    let mut last = None::<usize>;
    for i in 1..env.len() {
        let before = env[i.saturating_sub(8)..i].iter().sum::<f32>() / (i.min(8)) as f32;
        if env[i] > floor && env[i] > before * ratio && last.is_none_or(|l| i - l >= gap) {
            out.push(a + i * hop);
            last = Some(i);
        }
    }
    out
}

fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = std::f32::consts::TAU / len as f32 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (x, y) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[y] * cr - im[y] * ci, re[y] * ci + im[y] * cr);
                re[y] = re[x] - tr;
                im[y] = im[x] - ti;
                re[x] += tr;
                im[x] += ti;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
    if inverse {
        for v in re.iter_mut().chain(im.iter_mut()) {
            *v /= n as f32;
        }
    }
}

const NOISE_N: usize = 2048;

fn window(i: usize) -> f32 {
    0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / NOISE_N as f32).cos()
}

/// The average level of each frequency over a stretch of noise alone, for `denoise` to take away.
pub fn noise_profile(f: &Frames, a: usize, b: usize) -> Vec<f32> {
    let (a, b) = span(f, a, b);
    let mut sum = vec![0.0f32; NOISE_N / 2 + 1];
    let mut count = 0;
    let mut at = a;
    while at < b {
        for side in 0..2 {
            let mut re: Vec<f32> = (0..NOISE_N).map(|i| f.get(at + i).filter(|_| at + i < b).map_or(0.0, |x| x[side]) * window(i)).collect();
            let mut im = vec![0.0; NOISE_N];
            fft(&mut re, &mut im, false);
            for (k, s) in sum.iter_mut().enumerate() {
                *s += (re[k] * re[k] + im[k] * im[k]).sqrt();
            }
            count += 1;
        }
        at += NOISE_N / 2;
    }
    sum.iter().map(|s| s / count.max(1) as f32).collect()
}

/// Take the noise of `profile` out of the stretch: each frequency is turned down by as much as the noise
/// makes up of it. `amount` 1 takes the measured noise, more takes more, at the cost of a duller sound.
pub fn denoise(f: &mut Frames, a: usize, b: usize, profile: &[f32], amount: f32) {
    let (a, b) = span(f, a, b);
    if profile.len() != NOISE_N / 2 + 1 || b <= a {
        return;
    }
    let hop = NOISE_N / 4;
    let mut out = vec![[0.0f32; 2]; b - a + NOISE_N];
    let mut at = 0isize - (NOISE_N - hop) as isize;
    while at < (b - a) as isize {
        for side in 0..2 {
            let mut re: Vec<f32> = (0..NOISE_N)
                .map(|i| {
                    let k = at + i as isize;
                    if k >= 0 && (k as usize) < b - a { f[a + k as usize][side] * window(i) } else { 0.0 }
                })
                .collect();
            let mut im = vec![0.0; NOISE_N];
            fft(&mut re, &mut im, false);
            for k in 0..=NOISE_N / 2 {
                let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
                let keep = if mag > 1e-12 { ((mag - profile[k] * amount) / mag).max(0.03) } else { 0.0 };
                re[k] *= keep;
                im[k] *= keep;
                if k > 0 && k < NOISE_N / 2 {
                    re[NOISE_N - k] *= keep;
                    im[NOISE_N - k] *= keep;
                }
            }
            fft(&mut re, &mut im, true);
            for i in 0..NOISE_N {
                let k = at + i as isize;
                if k >= 0 && (k as usize) < b - a {
                    // four overlapping windows, each used twice, add up to 1.5
                    out[k as usize][side] += re[i] * window(i) / 1.5;
                }
            }
        }
        at += hop as isize;
    }
    f[a..b].copy_from_slice(&out[..b - a]);
}

/// The flags of a wave file: its cue points, with the names and lengths kept beside them.
pub fn read_marks(wav: &[u8]) -> Vec<Mark> {
    let mut out: Vec<(u32, Mark)> = vec![];
    if wav.len() < 12 || &wav[..4] != b"RIFF" {
        return vec![];
    }
    let int = |at: usize| wav.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let text = |a: usize, b: usize| wav.get(a..b.min(wav.len())).map(|t| String::from_utf8_lossy(t).trim_end_matches('\0').to_string()).unwrap_or_default();
    let mut at = 12;
    let mut notes: Vec<(u32, Option<String>, Option<usize>)> = vec![];
    while let (Some(id), Some(n)) = (wav.get(at..at + 4), int(at + 4)) {
        let body = at + 8;
        if id == b"cue " {
            for k in 0..int(body).unwrap_or(0).min(4096) {
                if let (Some(num), Some(frame)) = (int(body + 4 + k * 24), int(body + 4 + k * 24 + 20)) {
                    out.push((num as u32, Mark { at: frame, len: 0, name: String::new() }));
                }
            }
        } else if id == b"LIST" && wav.get(body..body + 4) == Some(b"adtl") {
            let mut p = body + 4;
            while let (Some(sub), Some(m)) = (wav.get(p..p + 4), int(p + 4)) {
                if p + 8 + m > body + n {
                    break;
                }
                let num = int(p + 8).unwrap_or(0) as u32;
                if sub == b"labl" {
                    notes.push((num, Some(text(p + 12, p + 8 + m)), None));
                } else if sub == b"ltxt" {
                    notes.push((num, None, int(p + 12)));
                }
                p += 8 + m + (m & 1);
            }
        }
        at = body + n + (n & 1);
    }
    for (num, name, len) in notes {
        if let Some((_, m)) = out.iter_mut().find(|m| m.0 == num) {
            if let Some(name) = name {
                m.name = name;
            }
            if let Some(len) = len {
                m.len = len;
            }
        }
    }
    let mut marks: Vec<Mark> = out.into_iter().map(|m| m.1).collect();
    marks.sort_by_key(|m| m.at);
    marks
}

/// Add the flags to a finished wave file, the way other programs read them: cue points, then their names and
/// region lengths in a list beside them.
pub fn write_marks(wav: &mut Vec<u8>, marks: &[Mark]) {
    if marks.is_empty() || wav.len() < 12 {
        return;
    }
    let mut cue = (marks.len() as u32).to_le_bytes().to_vec();
    let mut list = b"adtl".to_vec();
    for (k, m) in marks.iter().enumerate() {
        let num = k as u32 + 1;
        for v in [num, m.at as u32, u32::from_le_bytes(*b"data"), 0, 0, m.at as u32] {
            cue.extend(v.to_le_bytes());
        }
        if !m.name.is_empty() {
            let mut t = m.name.as_bytes().to_vec();
            t.push(0);
            list.extend(b"labl");
            list.extend((t.len() as u32 + 4).to_le_bytes());
            list.extend(num.to_le_bytes());
            list.extend(&t);
            if t.len() % 2 == 1 {
                list.push(0);
            }
        }
        if m.len > 0 {
            list.extend(b"ltxt");
            list.extend(20u32.to_le_bytes());
            list.extend(num.to_le_bytes());
            list.extend((m.len as u32).to_le_bytes());
            list.extend(b"rgn ");
            list.extend([0u8; 8]);
        }
    }
    wav.extend(b"cue ");
    wav.extend((cue.len() as u32).to_le_bytes());
    wav.extend(&cue);
    if list.len() > 4 {
        wav.extend(b"LIST");
        wav.extend((list.len() as u32).to_le_bytes());
        wav.extend(&list);
    }
    let total = (wav.len() - 8) as u32;
    wav[4..8].copy_from_slice(&total.to_le_bytes());
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

    #[test]
    fn flags_follow_the_cuts_and_survive_a_wave_file() {
        let mut marks = vec![Mark { at: 10, len: 0, name: "a".into() }, Mark { at: 40, len: 30, name: "verse".into() }, Mark { at: 90, len: 0, name: String::new() }];
        marks_removed(&mut marks, 50, 60);
        assert_eq!(marks.iter().map(|m| (m.at, m.len)).collect::<Vec<_>>(), [(10, 0), (40, 20), (80, 0)]);
        marks_inserted(&mut marks, 45, 5);
        assert_eq!(marks.iter().map(|m| (m.at, m.len)).collect::<Vec<_>>(), [(10, 0), (40, 25), (85, 0)]);
        let mut cur = std::io::Cursor::new(Vec::new());
        crate::wav::write(&mut cur, &[0.0; 400], 48_000, 2, 24).unwrap();
        let mut wav = cur.into_inner();
        write_marks(&mut wav, &marks);
        assert_eq!(read_marks(&wav), marks);
        assert_eq!(crate::slicer::markers(&wav).0, [10, 40, 85]);
    }

    #[test]
    fn noise_is_gated_found_and_taken_out() {
        // a quiet hiss with two loud tone bursts in it
        let mut seed = 7u32;
        let mut hiss = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 8) as f32 / 8388608.0 - 1.0) * 0.02
        };
        let rate = 48_000;
        let mut f: Frames = (0..48_000)
            .map(|i| {
                let tone = if (12_000..18_000).contains(&i) || (30_000..36_000).contains(&i) { (i as f32 * 0.06).sin() * 0.6 } else { 0.0 };
                let h = hiss();
                [tone + h, tone + h]
            })
            .collect();
        let thresh = peak(&f, 0, 8000) * 1.2;
        let loud = loud_parts(&f, 0, 48_000, thresh, rate);
        assert_eq!(loud.len(), 2);
        assert!(loud[0].0 > 11_000 && loud[0].0 <= 12_000 && loud[1].1 >= 36_000 && loud[1].1 < 37_500);
        let hits = onsets(&f, 0, 48_000, rate, 0.5);
        assert_eq!(hits.len(), 2);
        assert!((hits[0] as i64 - 12_000).abs() < 400 && (hits[1] as i64 - 30_000).abs() < 400);
        // the hiss between the bursts drops a lot, the bursts stay
        let profile = noise_profile(&f, 0, 8000);
        let mut clean = f.clone();
        denoise(&mut clean, 0, 48_000, &profile, 1.5);
        let rms = |f: &Frames, a: usize, b: usize| (f[a..b].iter().map(|x| x[0] * x[0]).sum::<f32>() / (b - a) as f32).sqrt();
        assert!(rms(&clean, 20_000, 28_000) < rms(&f, 20_000, 28_000) * 0.35);
        assert!(rms(&clean, 13_000, 17_000) > rms(&f, 13_000, 17_000) * 0.9);
        gate(&mut f, 0, 48_000, thresh, rate);
        assert_eq!(peak(&f, 20_000, 28_000), 0.0);
        assert!(peak(&f, 13_000, 17_000) > 0.5);
    }
}
