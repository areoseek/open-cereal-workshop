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

/// Work on the stretch one overlapping window of `n` frames at a time, in frequencies: `work` gets the real
/// and imaginary halves of each window (bins 0 to n/2 are the ones to change; the mirror half is kept in step
/// for it) and which side it is.
fn by_windows(f: &mut Frames, a: usize, b: usize, n: usize, mut work: impl FnMut(&mut [f32], &mut [f32], usize)) {
    let hop = n / 4;
    let win = |i: usize| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos();
    let mut out = vec![[0.0f32; 2]; b - a];
    let mut at = 0isize - (n - hop) as isize;
    while at < (b - a) as isize {
        for side in 0..2 {
            let mut re: Vec<f32> = (0..n)
                .map(|i| {
                    let k = at + i as isize;
                    if k >= 0 && (k as usize) < b - a { f[a + k as usize][side] * win(i) } else { 0.0 }
                })
                .collect();
            let mut im = vec![0.0; n];
            fft(&mut re, &mut im, false);
            work(&mut re, &mut im, side);
            for k in 1..n / 2 {
                re[n - k] = re[k];
                im[n - k] = -im[k];
            }
            fft(&mut re, &mut im, true);
            for i in 0..n {
                let k = at + i as isize;
                if k >= 0 && (k as usize) < b - a {
                    // four overlapping windows, each used twice, add up to 1.5
                    out[k as usize][side] += re[i] * win(i) / 1.5;
                }
            }
        }
        at += hop as isize;
    }
    f[a..b].copy_from_slice(&out);
}

/// Take the noise of `profile` out of the stretch: each frequency is turned down by as much as the noise
/// makes up of it. `amount` 1 takes the measured noise, more takes more, at the cost of a duller sound.
pub fn denoise(f: &mut Frames, a: usize, b: usize, profile: &[f32], amount: f32) {
    let (a, b) = span(f, a, b);
    if profile.len() != NOISE_N / 2 + 1 || b <= a {
        return;
    }
    by_windows(f, a, b, NOISE_N, |re, im, _| {
        for k in 0..=NOISE_N / 2 {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            let keep = if mag > 1e-12 { ((mag - profile[k] * amount) / mag).max(0.03) } else { 0.0 };
            re[k] *= keep;
            im[k] *= keep;
        }
    });
}

/// Smear the stretch in time: every frequency keeps its level but loses its place inside a long window.
/// `amount` 0 to 1 sets how long the smear is, from a twentieth of a second to about a third.
pub fn blur(f: &mut Frames, a: usize, b: usize, amount: f32) {
    let (a, b) = span(f, a, b);
    if b <= a {
        return;
    }
    let n = [2048, 4096, 8192, 16384][((amount.clamp(0.0, 1.0) * 3.99) as usize).min(3)];
    let mut seed = 12345u32;
    by_windows(f, a, b, n, |re, im, _| {
        for k in 1..n / 2 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let ph = (seed >> 8) as f32 / 16777216.0 * std::f32::consts::TAU;
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            re[k] = mag * ph.cos();
            im[k] = mag * ph.sin();
        }
    });
}

/// The centre frequencies of the equaliser's bands.
pub const EQ_BANDS: [f32; 10] = [31.0, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

/// Turn each part of the frequency range up or down: `db` holds the change at each of `EQ_BANDS`, and the
/// curve runs smoothly between them.
pub fn equalize(f: &mut Frames, a: usize, b: usize, rate: u32, db: &[f32; 10]) {
    let (a, b) = span(f, a, b);
    if b <= a {
        return;
    }
    let n = 4096;
    let gains: Vec<f32> = (0..=n / 2)
        .map(|k| {
            let hz = (k as f32 * rate as f32 / n as f32).max(1.0);
            let at = (hz / EQ_BANDS[0]).log2().clamp(0.0, 9.0);
            let (i, t) = ((at as usize).min(8), at - (at as usize).min(8) as f32);
            10f32.powf((db[i] + (db[i + 1] - db[i]) * t) / 20.0)
        })
        .collect();
    by_windows(f, a, b, n, |re, im, _| {
        for k in 0..=n / 2 {
            re[k] *= gains[k];
            im[k] *= gains[k];
        }
    });
}

/// Pass the stretch through the room whose echo is `impulse`: the echo is mixed in at `wet` (0 to 1) beside
/// the sound as it was, and its tail runs on past the end of the stretch, lengthening the take if it must.
pub fn convolve(f: &mut Frames, a: usize, b: usize, impulse: &[[f32; 2]], wet: f32) {
    let (a, b) = span(f, a, b);
    if b <= a || impulse.is_empty() {
        return;
    }
    let total = b - a + impulse.len() - 1;
    let n = total.next_power_of_two();
    let dry_peak = peak(f, a, b);
    let mut echo = vec![[0.0f32; 2]; total];
    for side in 0..2 {
        let mut xr: Vec<f32> = (0..n).map(|i| if i < b - a { f[a + i][side] } else { 0.0 }).collect();
        let mut xi = vec![0.0; n];
        let mut hr: Vec<f32> = (0..n).map(|i| impulse.get(i).map_or(0.0, |x| x[side])).collect();
        let mut hi = vec![0.0; n];
        fft(&mut xr, &mut xi, false);
        fft(&mut hr, &mut hi, false);
        for k in 0..n {
            (xr[k], xi[k]) = (xr[k] * hr[k] - xi[k] * hi[k], xr[k] * hi[k] + xi[k] * hr[k]);
        }
        fft(&mut xr, &mut xi, true);
        for (e, x) in echo.iter_mut().zip(&xr) {
            e[side] = *x;
        }
    }
    // the echo comes out as loud as the sound went in
    let top = echo.iter().fold(0.0f32, |m, x| m.max(x[0].abs()).max(x[1].abs())).max(1e-9);
    let scale = dry_peak / top * wet;
    gain(f, a, b, 1.0 - wet * 0.5);
    let wet: Frames = echo.iter().map(|x| [x[0] * scale, x[1] * scale]).collect();
    mix(f, a, &wet);
}

/// A room echo made of fading noise, `secs` long, for when there is no recorded one at hand.
pub fn noise_impulse(rate: u32, secs: f32) -> Frames {
    let n = (rate as f32 * secs.clamp(0.05, 10.0)) as usize;
    let mut seed = 99u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 8388608.0 - 1.0
    };
    (0..n)
        .map(|i| {
            // sixty decibels down by the end
            let fall = (-6.9 * i as f32 / n as f32).exp();
            [next() * fall, next() * fall]
        })
        .collect()
}

/// Make the loop `a..b` join up with itself: its last stretch fades over into the sound that leads up to its
/// start, so the end runs into the beginning without a step. `fade` is the length of the join in frames.
pub fn tune_loop(f: &mut Frames, a: usize, b: usize, fade: usize) {
    let (a, b) = span(f, a, b);
    let fade = fade.min((b - a) / 2);
    if fade == 0 {
        return;
    }
    for i in 0..fade {
        let t = (i + 1) as f32 / fade as f32;
        // with no lead-up to borrow, the start of the loop itself is what the end fades into
        let from = if a >= fade { f[a - fade + i] } else { f[a + i] };
        let x = &mut f[b - fade + i];
        let (keep, take) = ((1.0 - t).sqrt(), t.sqrt());
        *x = [x[0] * keep + from[0] * take, x[1] * keep + from[1] * take];
    }
}

/// Change the length of a sound without changing its pitch, and its pitch without changing its length.
/// `time` is how many times longer it comes out, `semis` how many semitones higher.
pub fn stretch(clip: &[[f32; 2]], time: f32, semis: f32, rate: u32) -> Frames {
    let shift = 2f64.powf(semis as f64 / 12.0);
    let longer = time.clamp(0.05, 20.0) as f64 * shift;
    let long = if (longer - 1.0).abs() < 1e-4 { clip.to_vec() } else { overlap_stretch(clip, longer, rate) };
    if (shift - 1.0).abs() < 1e-6 {
        return long;
    }
    // played faster by the pitch change, which also brings the length back
    let n = (long.len() as f64 / shift) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * shift;
            let (k, t) = (p as usize, (p - p.floor()) as f32);
            let (x, y) = (long[k.min(long.len() - 1)], long[(k + 1).min(long.len() - 1)]);
            [x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t]
        })
        .collect()
}

/// Length change by laying short overlapping pieces down closer together or further apart; each piece is
/// taken from where it lines up best with the one before, so the wave carries on unbroken.
fn overlap_stretch(clip: &[[f32; 2]], factor: f64, rate: u32) -> Frames {
    let n = ((rate as usize / 25).max(64)) & !1;
    if clip.len() < n * 2 {
        return clip.to_vec();
    }
    let hop = n / 2;
    let reach = n / 4;
    let mono: Vec<f32> = clip.iter().map(|x| x[0] + x[1]).collect();
    let out_len = (clip.len() as f64 * factor) as usize;
    let mut out = vec![[0.0f32; 2]; out_len + n];
    let win = |i: usize| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos();
    // where the piece before would have carried on from, in the source
    let mut carry = 0usize;
    let mut k = 0usize;
    while k * hop < out_len {
        let want = (k as f64 * hop as f64 / factor) as usize;
        let mut best = want.min(clip.len() - n);
        if k > 0 && carry + n <= clip.len() {
            let lo = want.saturating_sub(reach);
            let hi = (want + reach).min(clip.len() - n);
            let mut top = f32::MIN;
            let mut s = lo;
            while s <= hi {
                let score: f32 = (0..hop).step_by(4).map(|i| mono[s + i] * mono[carry + i]).sum();
                if score > top {
                    top = score;
                    best = s;
                }
                s += 2;
            }
        }
        for i in 0..n {
            let x = clip[best + i];
            let w = win(i);
            let o = &mut out[k * hop + i];
            o[0] += x[0] * w;
            o[1] += x[1] * w;
        }
        carry = best + hop;
        k += 1;
    }
    out.truncate(out_len);
    out
}

/// Move the pieces of a beat apart or together without stretching the pieces themselves: each piece starts
/// `time` times later than it did; a piece left too long for its new place is faded short, one too short is
/// followed by silence. `cuts` are where the pieces start, counted from the start of `clip`.
/// Throw away every `every`-th stretch of `period` frames and close the gaps, with a short blend at each join.
pub fn claw(clip: &[[f32; 2]], period: usize, every: usize, rate: u32) -> Frames {
    let period = period.max(1);
    let every = every.max(2);
    let blend = ((rate as usize) / 200).min(period / 2);
    let mut out: Frames = Vec::with_capacity(clip.len());
    let mut joined = false;
    for (k, part) in clip.chunks(period).enumerate() {
        if (k + 1) % every == 0 {
            joined = true;
            continue;
        }
        let from = out.len();
        out.extend_from_slice(part);
        if joined && from >= blend {
            // a quick fade down before the cut and up after it, so the join does not click
            for i in 0..blend.min(part.len()) {
                let t = (i + 1) as f32 / (blend + 1) as f32;
                for c in 0..2 {
                    out[from + i][c] *= t;
                    out[from - 1 - i][c] *= t;
                }
            }
        }
        joined = false;
    }
    out
}

pub fn drum_stretch(clip: &[[f32; 2]], cuts: &[usize], time: f32, rate: u32) -> Frames {
    let time = time.clamp(0.05, 20.0) as f64;
    let mut starts: Vec<usize> = cuts.iter().copied().filter(|&c| c > 0 && c < clip.len()).collect();
    starts.insert(0, 0);
    starts.dedup();
    let mut out = vec![[0.0f32; 2]; (clip.len() as f64 * time) as usize];
    for (k, &s) in starts.iter().enumerate() {
        let e = starts.get(k + 1).copied().unwrap_or(clip.len());
        let to = (s as f64 * time) as usize;
        let room = ((e as f64 * time) as usize).min(out.len()).saturating_sub(to);
        let len = (e - s).min(room);
        let fade = (rate as usize / 200).min(len);
        for i in 0..len {
            let g = if len < e - s && i >= len - fade { (len - i) as f32 / fade as f32 } else { 1.0 };
            out[to + i] = [clip[s + i][0] * g, clip[s + i][1] * g];
        }
    }
    out
}

/// A picture of the frequencies over time: `cols` columns across `v0..v1`, `rows` rows from high (first) to
/// low on an even pitch scale, each cell 0 (silent) to 1 (full level).
pub fn spectrum(f: &Frames, v0: usize, v1: usize, cols: usize, rows: usize, rate: u32) -> Vec<f32> {
    let n = 2048;
    let mut out = vec![0.0f32; cols * rows];
    let (v0, v1) = span(f, v0, v1);
    if v1 <= v0 || cols == 0 || rows == 0 {
        return out;
    }
    let (low, high) = (30.0f32, (rate as f32 / 2.0).max(60.0));
    for c in 0..cols {
        let mid = v0 + ((c as f64 + 0.5) / cols as f64 * (v1 - v0) as f64) as usize;
        let mut re: Vec<f32> = (0..n)
            .map(|i| {
                let k = mid as isize + i as isize - (n / 2) as isize;
                if k >= 0 && (k as usize) < f.len() { (f[k as usize][0] + f[k as usize][1]) * 0.5 * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()) } else { 0.0 }
            })
            .collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im, false);
        for r in 0..rows {
            let hz = low * (high / low).powf(1.0 - (r as f32 + 0.5) / rows as f32);
            let k = ((hz * n as f32 / rate as f32) as usize).clamp(1, n / 2 - 1);
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() / (n as f32 / 4.0);
            out[r * cols + c] = ((20.0 * mag.max(1e-6).log10() + 84.0) / 84.0).clamp(0.0, 1.0);
        }
    }
    out
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

/// Add tempo and loop notes to a finished wave file, the way loop libraries read them: how many beats it holds
/// at what tempo, and (when `looped`) one loop over the first `frames` frames.
pub fn write_loop_info(wav: &mut Vec<u8>, frames: usize, rate: u32, tempo: f32, beats: u32, meter: (u16, u16), looped: bool) {
    if wav.len() < 12 || frames == 0 {
        return;
    }
    let mut acid = Vec::new();
    // bit 0 says "play once"; a loop leaves it clear
    acid.extend((if looped { 0u32 } else { 1 }).to_le_bytes());
    acid.extend(60u16.to_le_bytes());
    acid.extend(0x8000u16.to_le_bytes());
    acid.extend(0f32.to_le_bytes());
    acid.extend(beats.to_le_bytes());
    acid.extend(meter.1.to_le_bytes());
    acid.extend(meter.0.to_le_bytes());
    acid.extend(tempo.to_le_bytes());
    wav.extend(b"acid");
    wav.extend((acid.len() as u32).to_le_bytes());
    wav.extend(&acid);
    if looped {
        let mut smpl = Vec::new();
        let period = (1e9 / rate.max(1) as f64) as u32;
        for v in [0u32, 0, period, 60, 0, 0, 0, 1, 0] {
            smpl.extend(v.to_le_bytes());
        }
        for v in [0u32, 0, 0, frames as u32 - 1, 0, 0] {
            smpl.extend(v.to_le_bytes());
        }
        wav.extend(b"smpl");
        wav.extend((smpl.len() as u32).to_le_bytes());
        wav.extend(&smpl);
    }
    let total = (wav.len() - 8) as u32;
    wav[4..8].copy_from_slice(&total.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_notes_are_added_as_whole_chunks() {
        let mut wav = Vec::new();
        crate::wav::write(&mut wav, &[0.0; 200], 44_100, 2, 16).unwrap();
        let before = wav.len();
        write_loop_info(&mut wav, 100, 44_100, 140.0, 8, (4, 4), true);
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize, wav.len() - 8);
        assert_eq!(&wav[before..before + 4], b"acid");
        assert_eq!(u32::from_le_bytes(wav[before + 4..before + 8].try_into().unwrap()), 24);
        assert_eq!(f32::from_le_bytes(wav[before + 28..before + 32].try_into().unwrap()), 140.0);
        let s = before + 32;
        assert_eq!(&wav[s..s + 4], b"smpl");
        assert_eq!(u32::from_le_bytes(wav[s + 4..s + 8].try_into().unwrap()), 60);
        // the loop ends on the last frame
        assert_eq!(u32::from_le_bytes(wav[s + 8 + 48..s + 8 + 52].try_into().unwrap()), 99);
        // and the file still opens
        assert_eq!(crate::sample::Sample::decode(wav, "wav").unwrap().frames.len(), 100);
    }

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

    #[test]
    fn the_claw_takes_every_nth_stretch() {
        let clip: Frames = (0..1000).map(|i| [(i / 100) as f32, 1.0]).collect();
        let out = claw(&clip, 100, 4, 48_000);
        assert_eq!(out.len(), 800);
        // stretches 3 and 7 are gone: the fourth hundred now holds what was the fifth
        assert_eq!(out[350][0], 4.0);
        assert_eq!(out[750][0], 9.0);
        assert_eq!(out[240][0], 2.0);
    }

    #[test]
    fn the_frequency_tools_do_what_they_say() {
        let rate = 48_000u32;
        let tone = |hz: f32, n: usize| -> Frames { (0..n).map(|i| [(i as f32 * hz * std::f32::consts::TAU / rate as f32).sin() * 0.5; 2]).collect() };
        let rms = |f: &[[f32; 2]]| (f.iter().map(|x| x[0] * x[0]).sum::<f32>() / f.len() as f32).sqrt();
        // how often the wave crosses zero tells its pitch
        let crossings = |f: &[[f32; 2]]| f.windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count() as f32 / f.len() as f32 * rate as f32;
        // twice as long at the same pitch; the same length an octave up
        let src = tone(440.0, 48_000);
        let long = stretch(&src, 2.0, 0.0, rate);
        assert!((long.len() as i64 - 96_000).abs() < 10);
        assert!((crossings(&long[4000..90_000]) - 440.0).abs() < 12.0, "{}", crossings(&long[4000..90_000]));
        assert!(rms(&long[4000..90_000]) > 0.25);
        let high = stretch(&src, 1.0, 12.0, rate);
        assert!((high.len() as i64 - 48_000).abs() < 10);
        assert!((crossings(&high[4000..44_000]) - 880.0).abs() < 25.0, "{}", crossings(&high[4000..44_000]));
        // the equaliser takes the high tone down and leaves the low one
        let mut both: Frames = tone(125.0, 48_000).iter().zip(tone(8000.0, 48_000)).map(|(a, b)| [a[0] + b[0]; 2]).collect();
        let before = rms(&both[8000..40_000]);
        let mut db = [0.0; 10];
        db[7] = -40.0;
        db[8] = -40.0;
        db[9] = -40.0;
        equalize(&mut both, 0, 48_000, rate, &db);
        let after = rms(&both[8000..40_000]);
        assert!((after / before - 0.707).abs() < 0.05, "{}", after / before);
        // a click through a room is followed by the room, and the take grows by the tail
        let mut click: Frames = vec![[0.0; 2]; 1000];
        click[10] = [1.0; 2];
        convolve(&mut click, 0, 1000, &noise_impulse(rate, 0.5), 1.0);
        assert_eq!(click.len(), 1000 + 24_000 - 1);
        assert!(rms(&click[2000..6000]) > 0.01 && rms(&click[2000..6000]) > rms(&click[20_000..24_000]) * 4.0);
        // blur keeps the level about where it was and spreads a burst out
        let mut burst: Frames = vec![[0.0; 2]; 48_000];
        burst[20_000..22_000].copy_from_slice(&tone(1000.0, 2000));
        blur(&mut burst, 0, 48_000, 1.0);
        assert!(rms(&burst[14_000..18_000]) > 0.005);
        // the loop end meets the loop start
        let mut ramp: Frames = (0..10_000).map(|i| [i as f32 / 10_000.0; 2]).collect();
        tune_loop(&mut ramp, 2000, 8000, 1000);
        assert!((ramp[7999][0] - ramp[1999][0]).abs() < 1e-4);
        // pieces of a beat move apart untouched
        let beat: Frames = (0..4000).map(|i| [if i % 1000 < 100 { 1.0 } else { 0.0 }; 2]).collect();
        let wide = drum_stretch(&beat, &[1000, 2000, 3000], 2.0, rate);
        assert_eq!(wide.len(), 8000);
        assert!(wide[2000][0] == 1.0 && wide[2099][0] == 1.0 && wide[1500][0] == 0.0 && wide[6050][0] == 1.0);
        // the picture shows the tone as one bright row
        let pic = spectrum(&tone(1000.0, 48_000), 0, 48_000, 8, 64, rate);
        let col: Vec<f32> = (0..64).map(|r| pic[r * 8 + 4]).collect();
        let brightest = col.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let hz = 30.0 * (24_000.0f32 / 30.0).powf(1.0 - (brightest as f32 + 0.5) / 64.0);
        assert!((hz / 1000.0).log2().abs() < 0.2, "{hz}");
    }
}
