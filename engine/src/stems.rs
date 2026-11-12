//! Pulling a finished mix apart into vocals, drums, bass and the rest.
//!
//! The work is done by the Open-Unmix networks (UMX-HQ, MIT licence, see docs/THIRD-PARTY.md),
//! one per part. Each looks at the loudness of every frequency over time and answers with how
//! much of that loudness belongs to its part. The four answers are then weighed against each
//! other so the parts always add back up to the sample they came from. The weight files are
//! not shipped with the program; the app fetches them the first time they are wanted.

use crate::sample::Sample;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The parts, in the order `split` hands them back.
pub const PARTS: [&str; 4] = ["Vocals", "Drums", "Bass", "Instruments"];
/// The weight file for each part, as published.
pub const FILES: [&str; 4] = ["vocals-b62c91ce.pth", "drums-9619578f.pth", "bass-8d85a5bd.pth", "other-b52fbbf7.pth"];
/// Where the weight files are published.
pub const SOURCE: &str = "https://zenodo.org/api/records/3370489/files";
/// Every weight file is this long.
pub const FILE_LEN: u64 = 35_637_796;

const RATE: u32 = 44100;
const N: usize = 4096;
const HOP: usize = 1024;
const BINS: usize = N / 2 + 1;
const SEEN: usize = 1487;
const WIDE: usize = 512;
const CELL: usize = 256;

// ---------------------------------------------------------------------------------------------
// Reading the weight files

#[derive(Clone, Debug)]
enum V {
    None,
    Mark,
    Int(i64),
    Str(String),
    Global(String),
    Tuple(Vec<V>),
    List(Vec<V>),
    Dict(Vec<(V, V)>),
    Store { key: String, long: bool },
    Tensor { key: String, long: bool, off: usize, shape: Vec<usize> },
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl Rd<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or("weight file ends early")?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn line(&mut self) -> Result<String, String> {
        let rest = &self.b[self.p.min(self.b.len())..];
        let n = rest.iter().position(|&c| c == b'\n').ok_or("weight file ends early")?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.p += n + 1;
        Ok(s)
    }
}

fn pop_mark(stack: &mut Vec<V>) -> Result<Vec<V>, String> {
    let at = stack.iter().rposition(|v| matches!(v, V::Mark)).ok_or("bad weight file")?;
    let items = stack.split_off(at + 1);
    stack.pop();
    Ok(items)
}

/// One stored object. Only the handful of instructions these files use are understood.
fn object(r: &mut Rd) -> Result<V, String> {
    let mut stack: Vec<V> = Vec::new();
    let mut memo: HashMap<u32, V> = HashMap::new();
    let bad = || "bad weight file".to_string();
    loop {
        let op = r.u8()?;
        match op {
            0x80 => {
                r.u8()?;
            }
            b'.' => return stack.pop().ok_or_else(bad),
            b'(' => stack.push(V::Mark),
            b'N' => stack.push(V::None),
            0x88 | 0x89 => stack.push(V::Int((op == 0x88) as i64)),
            b'K' => {
                let v = r.u8()? as i64;
                stack.push(V::Int(v));
            }
            b'M' => {
                let v = u16::from_le_bytes(r.take(2)?.try_into().unwrap()) as i64;
                stack.push(V::Int(v));
            }
            b'J' => {
                let v = r.u32()? as i32 as i64;
                stack.push(V::Int(v));
            }
            0x8a => {
                let n = r.u8()? as usize;
                let mut v = 0i64;
                for (i, &c) in r.take(n)?.iter().enumerate().take(8) {
                    v |= (c as i64) << (8 * i);
                }
                stack.push(V::Int(v));
            }
            b'X' => {
                let n = r.u32()? as usize;
                let s = String::from_utf8_lossy(r.take(n)?).into_owned();
                stack.push(V::Str(s));
            }
            b'c' => {
                let a = r.line()?;
                let b = r.line()?;
                stack.push(V::Global(format!("{a} {b}")));
            }
            b'q' => {
                let k = r.u8()? as u32;
                memo.insert(k, stack.last().ok_or_else(bad)?.clone());
            }
            b'r' => {
                let k = r.u32()?;
                memo.insert(k, stack.last().ok_or_else(bad)?.clone());
            }
            b'h' => {
                let k = r.u8()? as u32;
                stack.push(memo.get(&k).ok_or_else(bad)?.clone());
            }
            b'j' => {
                let k = r.u32()?;
                stack.push(memo.get(&k).ok_or_else(bad)?.clone());
            }
            b')' => stack.push(V::Tuple(Vec::new())),
            b'}' => stack.push(V::Dict(Vec::new())),
            b']' => stack.push(V::List(Vec::new())),
            b't' => {
                let items = pop_mark(&mut stack)?;
                stack.push(V::Tuple(items));
            }
            0x85..=0x87 => {
                let n = (op - 0x84) as usize;
                if stack.len() < n {
                    return Err(bad());
                }
                let items = stack.split_off(stack.len() - n);
                stack.push(V::Tuple(items));
            }
            b'a' => {
                let v = stack.pop().ok_or_else(bad)?;
                if let Some(V::List(l)) = stack.last_mut() {
                    l.push(v);
                }
            }
            b'e' => {
                let items = pop_mark(&mut stack)?;
                if let Some(V::List(l)) = stack.last_mut() {
                    l.extend(items);
                }
            }
            b's' => {
                let v = stack.pop().ok_or_else(bad)?;
                let k = stack.pop().ok_or_else(bad)?;
                if let Some(V::Dict(d)) = stack.last_mut() {
                    d.push((k, v));
                }
            }
            b'u' => {
                let items = pop_mark(&mut stack)?;
                if let Some(V::Dict(d)) = stack.last_mut() {
                    let mut it = items.into_iter();
                    while let (Some(k), Some(v)) = (it.next(), it.next()) {
                        d.push((k, v));
                    }
                }
            }
            b'b' => {
                stack.pop();
            }
            b'Q' => {
                let Some(V::Tuple(t)) = stack.pop() else { return Err(bad()) };
                let (Some(V::Global(kind)), Some(V::Str(key))) = (t.get(1), t.get(2)) else { return Err(bad()) };
                stack.push(V::Store { key: key.clone(), long: kind.contains("Long") });
            }
            b'R' => {
                let Some(V::Tuple(args)) = stack.pop() else { return Err(bad()) };
                let Some(V::Global(what)) = stack.pop() else { return Err(bad()) };
                if what.ends_with("OrderedDict") {
                    stack.push(V::Dict(Vec::new()));
                } else if what.ends_with("_rebuild_tensor_v2") {
                    let (Some(V::Store { key, long }), Some(V::Int(off)), Some(V::Tuple(size))) = (args.first(), args.get(1), args.get(2))
                    else {
                        return Err(bad());
                    };
                    let shape = size.iter().map(|v| if let V::Int(n) = v { *n as usize } else { 0 }).collect();
                    stack.push(V::Tensor { key: key.clone(), long: *long, off: *off as usize, shape });
                } else {
                    return Err(format!("weight file holds something unexpected ({what})"));
                }
            }
            _ => return Err(format!("weight file holds an unknown instruction ({op:#x})")),
        }
    }
}

/// Every named table of numbers in a weight file, with its shape.
fn tables(bytes: &[u8]) -> Result<HashMap<String, (Vec<usize>, Vec<f32>)>, String> {
    let mut r = Rd { b: bytes, p: 0 };
    for _ in 0..3 {
        object(&mut r)?;
    }
    let V::Dict(state) = object(&mut r)? else { return Err("bad weight file".into()) };
    let V::List(order) = object(&mut r)? else { return Err("bad weight file".into()) };
    let mut wide: HashMap<String, bool> = HashMap::new();
    for (_, v) in &state {
        if let V::Tensor { key, long, .. } = v {
            wide.insert(key.clone(), *long);
        }
    }
    let mut at: HashMap<String, (usize, usize)> = HashMap::new();
    for k in order {
        let V::Str(k) = k else { return Err("bad weight file".into()) };
        let n = u64::from_le_bytes(r.take(8)?.try_into().unwrap()) as usize;
        let size = if wide.get(&k).copied().unwrap_or(false) { 8 } else { 4 };
        at.insert(k, (r.p, n));
        r.take(n.checked_mul(size).ok_or("bad weight file")?)?;
    }
    let mut out = HashMap::new();
    for (k, v) in state {
        let (V::Str(name), V::Tensor { key, off, shape, .. }) = (k, v) else { continue };
        if wide.get(&key).copied().unwrap_or(false) {
            continue;
        }
        let &(p, n) = at.get(&key).ok_or("bad weight file")?;
        let count: usize = shape.iter().product();
        if off + count > n {
            return Err("bad weight file".into());
        }
        let data = bytes[p + off * 4..p + (off + count) * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        out.insert(name, (shape, data));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The network

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 16];
    let (ca, cb) = (a.chunks_exact(16), b.chunks_exact(16));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for i in 0..16 {
            acc[i] += x[i] * y[i];
        }
    }
    let mut s: f32 = acc.iter().sum();
    for (x, y) in ra.iter().zip(rb) {
        s += x * y;
    }
    s
}

/// A table of weights, one row per output.
struct Mat {
    cols: usize,
    w: Vec<f32>,
}

impl Mat {
    fn add_to(&self, x: &[f32], out: &mut [f32]) {
        for (o, row) in out.iter_mut().zip(self.w.chunks_exact(self.cols)) {
            *o += dot(row, x);
        }
    }
}

/// The evening-out step after each layer, folded down to one multiply and one add.
struct Norm {
    mul: Vec<f32>,
    add: Vec<f32>,
}

impl Norm {
    fn run(&self, x: &mut [f32]) {
        for ((v, m), a) in x.iter_mut().zip(&self.mul).zip(&self.add) {
            *v = *v * m + a;
        }
    }
}

/// One direction of one remembering layer.
struct Cell {
    from_in: Mat,
    from_self: Mat,
    bias: Vec<f32>,
}

fn squash(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Cell {
    /// Runs over the frames (each `WIDE` long), forwards or backwards, writing `CELL` numbers
    /// per frame into `out` at `shift`.
    fn run(&self, input: &[f32], out: &mut [f32], shift: usize, backwards: bool) {
        let frames = input.len() / WIDE;
        let mut h = vec![0f32; CELL];
        let mut c = vec![0f32; CELL];
        let mut g = vec![0f32; CELL * 4];
        for step in 0..frames {
            let t = if backwards { frames - 1 - step } else { step };
            g.copy_from_slice(&self.bias);
            self.from_in.add_to(&input[t * WIDE..(t + 1) * WIDE], &mut g);
            self.from_self.add_to(&h, &mut g);
            for k in 0..CELL {
                let (i, f, n, o) = (squash(g[k]), squash(g[CELL + k]), g[2 * CELL + k].tanh(), squash(g[3 * CELL + k]));
                c[k] = f * c[k] + i * n;
                h[k] = o * c[k].tanh();
            }
            out[t * WIDE + shift..t * WIDE + shift + CELL].copy_from_slice(&h);
        }
    }
}

/// The network for one part.
pub struct Net {
    in_mean: Vec<f32>,
    in_scale: Vec<f32>,
    out_scale: Vec<f32>,
    out_mean: Vec<f32>,
    first: Mat,
    first_norm: Norm,
    cells: Vec<[Cell; 2]>,
    second: Mat,
    second_norm: Norm,
    third: Mat,
    third_norm: Norm,
}

impl Net {
    /// Reads a published weight file.
    pub fn read(bytes: &[u8]) -> Result<Net, String> {
        let mut t = tables(bytes)?;
        let mut get = |name: &str, len: usize| -> Result<Vec<f32>, String> {
            let (_, data) = t.remove(name).ok_or_else(|| format!("weight file has no {name}"))?;
            if data.len() != len {
                return Err(format!("weight file has the wrong size of {name}"));
            }
            Ok(data)
        };
        let mut norm = |name: &str, len: usize| -> Result<Norm, String> {
            let (w, b) = (get(&format!("{name}.weight"), len)?, get(&format!("{name}.bias"), len)?);
            let (mean, var) = (get(&format!("{name}.running_mean"), len)?, get(&format!("{name}.running_var"), len)?);
            let mul: Vec<f32> = w.iter().zip(&var).map(|(w, v)| w / (v + 1e-5).sqrt()).collect();
            let add = b.iter().zip(&mean).zip(&mul).map(|((b, m), k)| b - m * k).collect();
            Ok(Norm { mul, add })
        };
        let first_norm = norm("bn1", WIDE)?;
        let second_norm = norm("bn2", WIDE)?;
        let third_norm = norm("bn3", BINS * 2)?;
        let mut cells = Vec::new();
        for layer in 0..3 {
            let mut one = |tail: &str| -> Result<Cell, String> {
                let bi = get(&format!("lstm.bias_ih_l{layer}{tail}"), CELL * 4)?;
                let bh = get(&format!("lstm.bias_hh_l{layer}{tail}"), CELL * 4)?;
                Ok(Cell {
                    from_in: Mat { cols: WIDE, w: get(&format!("lstm.weight_ih_l{layer}{tail}"), CELL * 4 * WIDE)? },
                    from_self: Mat { cols: CELL, w: get(&format!("lstm.weight_hh_l{layer}{tail}"), CELL * 4 * CELL)? },
                    bias: bi.iter().zip(&bh).map(|(a, b)| a + b).collect(),
                })
            };
            cells.push([one("")?, one("_reverse")?]);
        }
        Ok(Net {
            in_mean: get("input_mean", SEEN)?,
            in_scale: get("input_scale", SEEN)?,
            out_scale: get("output_scale", BINS)?,
            out_mean: get("output_mean", BINS)?,
            first: Mat { cols: SEEN * 2, w: get("fc1.weight", WIDE * SEEN * 2)? },
            first_norm,
            cells,
            second: Mat { cols: WIDE * 2, w: get("fc2.weight", WIDE * WIDE * 2)? },
            second_norm,
            third: Mat { cols: WIDE, w: get("fc3.weight", BINS * 2 * WIDE)? },
            third_norm,
        })
    }

    /// Takes the loudness of every frequency of both sides for a run of frames (`2 * BINS`
    /// numbers a frame, left side first) and gives back this part's share, in the same layout.
    pub fn run(&self, mag: &[f32], stop: &AtomicBool) -> Vec<f32> {
        let frames = mag.len() / (BINS * 2);
        let mut x = vec![0f32; frames * WIDE];
        let mut seen = vec![0f32; SEEN * 2];
        for t in 0..frames {
            for side in 0..2 {
                let src = &mag[t * BINS * 2 + side * BINS..][..SEEN];
                for (k, v) in src.iter().enumerate() {
                    seen[side * SEEN + k] = (v + self.in_mean[k]) * self.in_scale[k];
                }
            }
            let row = &mut x[t * WIDE..(t + 1) * WIDE];
            self.first.add_to(&seen, row);
            self.first_norm.run(row);
            for v in row.iter_mut() {
                *v = v.tanh();
            }
        }
        let mut cur = x.clone();
        for pair in &self.cells {
            if stop.load(Ordering::Relaxed) {
                return Vec::new();
            }
            let mut fwd = vec![0f32; frames * WIDE];
            let mut back = vec![0f32; frames * WIDE];
            std::thread::scope(|s| {
                s.spawn(|| pair[0].run(&cur, &mut fwd, 0, false));
                pair[1].run(&cur, &mut back, CELL, true);
            });
            for (f, b) in fwd.iter_mut().zip(&back) {
                *f += b;
            }
            cur = fwd;
        }
        let mut out = vec![0f32; frames * BINS * 2];
        let mut both = vec![0f32; WIDE * 2];
        let mut mid = vec![0f32; WIDE];
        for t in 0..frames {
            both[..WIDE].copy_from_slice(&x[t * WIDE..(t + 1) * WIDE]);
            both[WIDE..].copy_from_slice(&cur[t * WIDE..(t + 1) * WIDE]);
            mid.fill(0.0);
            self.second.add_to(&both, &mut mid);
            self.second_norm.run(&mut mid);
            for v in mid.iter_mut() {
                *v = v.max(0.0);
            }
            let row = &mut out[t * BINS * 2..(t + 1) * BINS * 2];
            self.third.add_to(&mid, row);
            self.third_norm.run(row);
            for (k, v) in row.iter_mut().enumerate() {
                let b = k % BINS;
                *v = (*v * self.out_scale[b] + self.out_mean[b]).max(0.0) * mag[t * BINS * 2 + k];
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Frequencies and back

struct Fft {
    cos: Vec<f32>,
    sin: Vec<f32>,
    swap: Vec<(u32, u32)>,
    window: Vec<f32>,
}

impl Fft {
    fn new() -> Self {
        let (cos, sin) = (0..N / 2).map(|k| (std::f64::consts::TAU * k as f64 / N as f64).sin_cos()).map(|(s, c)| (c as f32, s as f32)).unzip();
        let bits = N.trailing_zeros();
        let swap = (0..N as u32).map(|i| (i, i.reverse_bits() >> (32 - bits))).filter(|(i, j)| i < j).collect();
        let window = (0..N).map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / N as f64).cos()) as f32).collect();
        Self { cos, sin, swap, window }
    }

    fn run(&self, re: &mut [f32], im: &mut [f32], inverse: bool) {
        for &(i, j) in &self.swap {
            re.swap(i as usize, j as usize);
            im.swap(i as usize, j as usize);
        }
        let sign = if inverse { 1.0 } else { -1.0 };
        let mut len = 2;
        while len <= N {
            let (half, stride) = (len / 2, N / len);
            for start in (0..N).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * stride], sign * self.sin[k * stride]);
                    let (x, y) = (start + k, start + k + half);
                    let (tr, ti) = (re[y] * wr - im[y] * wi, re[y] * wi + im[y] * wr);
                    re[y] = re[x] - tr;
                    im[y] = im[x] - ti;
                    re[x] += tr;
                    im[x] += ti;
                }
            }
            len <<= 1;
        }
    }
}

/// The sample one way round at the ends, as if a mirror stood at each end.
fn mirrored(frames: &[[f32; 2]], i: isize, side: usize) -> f32 {
    let n = frames.len() as isize;
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return frames[0][side];
    }
    let period = 2 * (n - 1);
    let mut k = i.rem_euclid(period);
    if k >= n {
        k = period - k;
    }
    frames[k as usize][side]
}

/// Frames worked on in one go, and how many extra on each side the networks get to look at.
const BLOCK: usize = 1536;
const MARGIN: usize = 192;

/// Splits a sample into its four parts (see `PARTS`). `done` counts up to `total` as the work
/// goes; raising `stop` gives up early and hands back nothing.
pub fn split(sample: &Sample, nets: &[Net; 4], done: &AtomicUsize, total: &AtomicUsize, stop: &AtomicBool) -> Option<[Sample; 4]> {
    let src: Vec<[f32; 2]> = if sample.rate == RATE {
        sample.frames.clone()
    } else {
        let step = sample.rate as f64 / RATE as f64;
        let n = (sample.frames.len() as f64 / step) as usize;
        (0..n).map(|i| sample.at_with(i as f64 * step, 2)).collect()
    };
    let len = src.len();
    let count = len / HOP + 1;
    total.store(count, Ordering::Relaxed);
    done.store(0, Ordering::Relaxed);
    let fft = Fft::new();
    let mut outs: Vec<Vec<[f32; 2]>> = (0..4).map(|_| vec![[0f32; 2]; len + N]).collect();
    let mut re = vec![0f32; N];
    let mut im = vec![0f32; N];
    let mut sums = vec![0f32; BINS];
    let mut a = 0;
    while a < count {
        let b = (a + BLOCK).min(count);
        let (lo, hi) = (a.saturating_sub(MARGIN), (b + MARGIN).min(count));
        let frames = hi - lo;
        // The frequency picture of this stretch: real and imaginary parts and loudness.
        let mut spec_re = vec![0f32; frames * BINS * 2];
        let mut spec_im = vec![0f32; frames * BINS * 2];
        let mut mag = vec![0f32; frames * BINS * 2];
        for t in 0..frames {
            let first = ((lo + t) * HOP) as isize - (N / 2) as isize;
            for side in 0..2 {
                for i in 0..N {
                    re[i] = mirrored(&src, first + i as isize, side) * fft.window[i];
                }
                im.fill(0.0);
                fft.run(&mut re, &mut im, false);
                let at = (t * 2 + side) * BINS;
                spec_re[at..at + BINS].copy_from_slice(&re[..BINS]);
                spec_im[at..at + BINS].copy_from_slice(&im[..BINS]);
                for k in 0..BINS {
                    mag[at + k] = (re[k] * re[k] + im[k] * im[k]).sqrt();
                }
            }
        }
        let mut parts: Vec<Vec<f32>> = Vec::new();
        std::thread::scope(|s| {
            let mag = &mag;
            let handles: Vec<_> = nets.iter().map(|net| s.spawn(move || net.run(mag, stop))).collect();
            parts = handles.into_iter().map(|h| h.join().unwrap_or_default()).collect();
        });
        if stop.load(Ordering::Relaxed) || parts.iter().any(|p| p.len() != mag.len()) {
            return None;
        }
        // Each part gets the share of the original its loudness earns, so the four add up.
        for t in (a - lo)..(b - lo) {
            let start = (lo + t) * HOP;
            for side in 0..2 {
                let at = (t * 2 + side) * BINS;
                // Where no part lays claim to a frequency, it is handed out the way the rest
                // of the moment was, so nothing of the original goes missing.
                let mut whole = [0f32; 4];
                for (j, part) in parts.iter().enumerate() {
                    whole[j] = part[at..at + BINS].iter().map(|v| v * v).sum();
                }
                let all: f32 = whole.iter().sum();
                for w in &mut whole {
                    *w = if all > 1e-20 { *w / all } else { 0.25 };
                }
                for k in 0..BINS {
                    sums[k] = parts.iter().map(|p| p[at + k] * p[at + k]).sum();
                }
                for (j, part) in parts.iter().enumerate() {
                    for k in 0..BINS {
                        let share = if sums[k] > 1e-20 { part[at + k] * part[at + k] / sums[k] } else { whole[j] };
                        re[k] = spec_re[at + k] * share;
                        im[k] = spec_im[at + k] * share;
                    }
                    for k in 1..N / 2 {
                        re[N - k] = re[k];
                        im[N - k] = -im[k];
                    }
                    fft.run(&mut re, &mut im, true);
                    let out = &mut outs[j];
                    for i in 0..N {
                        out[start + i][side] += re[i] * fft.window[i] / N as f32;
                    }
                }
            }
            done.fetch_add(1, Ordering::Relaxed);
        }
        a = b;
    }
    // Every spot was windowed twice by each frame that covers it; near the ends fewer do.
    let mut cover = vec![0f32; len + N];
    for t in 0..count {
        for i in 0..N {
            cover[t * HOP + i] += fft.window[i] * fft.window[i];
        }
    }
    for out in &mut outs {
        for (v, c) in out.iter_mut().zip(&cover) {
            if *c > 1e-6 {
                v[0] /= c;
                v[1] /= c;
            }
        }
    }
    let mut it = outs.into_iter().enumerate().map(|(j, out)| {
        let body = &out[N / 2..N / 2 + len];
        let frames: Vec<[f32; 2]> = if sample.rate == RATE {
            body.to_vec()
        } else {
            let at44 = Sample { name: String::new(), rate: RATE, mono: false, frames: body.to_vec(), loop_pts: None };
            let step = RATE as f64 / sample.rate as f64;
            (0..sample.frames.len()).map(|i| at44.at_with(i as f64 * step, 2)).collect()
        };
        let stem = sample.name.rsplit_once('.').map_or(sample.name.as_str(), |(s, _)| s);
        Sample { name: format!("{stem} - {}.wav", PARTS[j]), rate: sample.rate, mono: false, frames, loop_pts: None }
    });
    Some([it.next()?, it.next()?, it.next()?, it.next()?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_and_back_give_the_same_sound() {
        let fft = Fft::new();
        let orig: Vec<f32> = (0..N).map(|i| ((i * 37 % 101) as f32 / 50.0 - 1.0) * 0.5).collect();
        let (mut re, mut im) = (orig.clone(), vec![0f32; N]);
        fft.run(&mut re, &mut im, false);
        fft.run(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&orig) {
            assert!((a / N as f32 - b).abs() < 1e-4);
        }
    }

    #[test]
    fn a_tone_lands_in_its_own_slot() {
        let fft = Fft::new();
        let mut re: Vec<f32> = (0..N).map(|i| (std::f32::consts::TAU * 100.0 * i as f32 / N as f32).cos()).collect();
        let mut im = vec![0f32; N];
        fft.run(&mut re, &mut im, false);
        assert!((re[100] - N as f32 / 2.0).abs() < 1.0);
        assert!(re[101].abs() < 1.0 && im[100].abs() < 1.0);
    }

    #[test]
    fn mirrors_stand_at_both_ends() {
        let f: Vec<[f32; 2]> = (0..5).map(|i| [i as f32, 0.0]).collect();
        let got: Vec<f32> = (-3..9).map(|i| mirrored(&f, i, 0)).collect();
        assert_eq!(got, [3.0, 2.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0, 0.0]);
    }

    /// Needs the real weight files: point CEREAL_STEM_MODELS at the folder holding them.
    #[test]
    fn the_parts_add_back_up_to_the_whole() {
        let Ok(dir) = std::env::var("CEREAL_STEM_MODELS") else { return };
        let nets: Vec<Net> = FILES.iter().map(|f| Net::read(&std::fs::read(std::path::Path::new(&dir).join(f)).unwrap()).unwrap()).collect();
        let nets: [Net; 4] = nets.try_into().ok().unwrap();
        // A low steady tone and sharp clicks: the bass part should get the tone.
        let frames: Vec<[f32; 2]> = (0..RATE as usize * 3)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let tone = (std::f32::consts::TAU * 55.0 * t).sin() * 0.3 + (std::f32::consts::TAU * 110.0 * t).sin() * 0.1;
                [tone, tone]
            })
            .collect();
        let s = Sample { name: "x.wav".into(), rate: RATE, mono: false, frames, loop_pts: None };
        let (done, total, stop) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
        let parts = split(&s, &nets, &done, &total, &stop).unwrap();
        assert_eq!(done.load(Ordering::Relaxed), total.load(Ordering::Relaxed));
        let power = |f: &[[f32; 2]]| f.iter().map(|v| v[0] * v[0]).sum::<f32>();
        for i in (0..s.frames.len()).step_by(97) {
            let sum: f32 = parts.iter().map(|p| p.frames[i][0]).sum();
            assert!((sum - s.frames[i][0]).abs() < 2e-3, "at {i}: {sum} vs {}", s.frames[i][0]);
        }
        let bass = power(&parts[2].frames);
        assert!(bass > power(&s.frames) * 0.5, "bass got {bass} of {}", power(&s.frames));
        assert_eq!(parts[2].name, "x - Bass.wav");
    }
}
