//! A FLAC writer: fixed predictors, partitioned Rice coding, stereo decorrelation.
//! Lossless, so what matters is that every decoder reads back the exact samples.

use std::io::{self, Write};

const BLOCK: usize = 4096;

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn new() -> Self {
        Self { out: Vec::new(), acc: 0, n: 0 }
    }
    fn put(&mut self, v: u64, bits: u32) {
        if bits == 0 {
            return;
        }
        if bits > 32 {
            self.put(v >> 32, bits - 32);
            self.put(v & 0xffff_ffff, 32);
            return;
        }
        self.acc = (self.acc << bits) | (v & ((1u64 << bits) - 1));
        self.n += bits;
        while self.n >= 8 {
            self.n -= 8;
            self.out.push((self.acc >> self.n) as u8);
        }
    }
    fn signed(&mut self, v: i64, bits: u32) {
        self.put(v as u64, bits);
    }
    fn unary(&mut self, mut q: u32) {
        while q >= 32 {
            self.put(0, 32);
            q -= 32;
        }
        self.put(1, q + 1);
    }
    fn align(&mut self) {
        if self.n > 0 {
            self.put(0, 8 - self.n);
        }
    }
}

fn crc8(d: &[u8]) -> u8 {
    let mut c = 0u8;
    for &b in d {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 { (c << 1) ^ 0x07 } else { c << 1 };
        }
    }
    c
}

fn crc16(d: &[u8]) -> u16 {
    let mut c = 0u16;
    for &b in d {
        c ^= (b as u16) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x8005 } else { c << 1 };
        }
    }
    c
}

fn zig(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

/// Bits needed for a run of residuals at one Rice parameter.
fn rice_bits(u: &[u64], k: u32) -> u64 {
    u.iter().map(|&x| (x >> k) + 1 + k as u64).sum()
}

fn best_k(u: &[u64], max_k: u32) -> (u32, u64) {
    let sum: u64 = u.iter().sum();
    let mean = sum / u.len().max(1) as u64;
    let guess = (64 - mean.leading_zeros()).saturating_sub(1).min(max_k);
    let mut best = (guess, rice_bits(u, guess));
    for k in [guess.saturating_sub(1), (guess + 1).min(max_k)] {
        let b = rice_bits(u, k);
        if b < best.1 {
            best = (k, b);
        }
    }
    best
}

/// One channel of one block, as the smallest of constant, fixed-predicted or verbatim.
fn subframe(w: &mut Bits, x: &[i64], bps: u32) {
    let n = x.len();
    if x.iter().all(|&v| v == x[0]) {
        w.put(0, 8);
        w.signed(x[0], bps);
        return;
    }
    // Residuals of the five fixed predictors.
    let mut res: Vec<Vec<i64>> = vec![x.to_vec()];
    for o in 1..=4usize {
        let prev = &res[o - 1];
        let mut r = vec![0i64; n];
        for i in o..n {
            r[i] = prev[i] - prev[i - 1];
        }
        res.push(r);
    }
    let order = (0..=4usize.min(n.saturating_sub(1))).min_by_key(|&o| res[o][o..].iter().map(|v| v.unsigned_abs()).sum::<u64>()).unwrap_or(0);
    let u: Vec<u64> = res[order][order..].iter().map(|&v| zig(v)).collect();
    let wide = bps > 16;
    let (pbits, max_k) = if wide { (5, 30) } else { (4, 14) };
    // Partition orders that split the block evenly and leave the first part some residuals.
    let mut best: Option<(u32, Vec<u32>, u64)> = None;
    for po in 0..=8u32 {
        let parts = 1usize << po;
        if n % parts != 0 || n / parts <= order {
            break;
        }
        let size = n / parts;
        let mut ks = Vec::with_capacity(parts);
        let mut total = 0u64;
        let mut at = 0;
        for p in 0..parts {
            let len = if p == 0 { size - order } else { size };
            let (k, b) = best_k(&u[at..at + len], max_k);
            ks.push(k);
            total += b + pbits as u64;
            at += len;
        }
        if best.as_ref().is_none_or(|b| total < b.2) {
            best = Some((po, ks, total));
        }
    }
    let Some((po, ks, total)) = best else {
        w.put(0b0000_0010, 8);
        for &v in x {
            w.signed(v, bps);
        }
        return;
    };
    if total + 6 + order as u64 * bps as u64 >= n as u64 * bps as u64 {
        w.put(0b0000_0010, 8);
        for &v in x {
            w.signed(v, bps);
        }
        return;
    }
    w.put(((0b001000 | order as u64) << 1) & 0xff, 8);
    for &v in &x[..order] {
        w.signed(v, bps);
    }
    w.put(wide as u64, 2);
    w.put(po as u64, 4);
    let size = n >> po;
    let mut at = 0;
    for (p, &k) in ks.iter().enumerate() {
        let len = if p == 0 { size - order } else { size };
        w.put(k as u64, pbits);
        for &v in &u[at..at + len] {
            w.unary((v >> k) as u32);
            w.put(v, k);
        }
        at += len;
    }
}

fn frame(index: u32, chans: &[Vec<i64>], bps: u32, rate: u32) -> Vec<u8> {
    let n = chans[0].len();
    let mut w = Bits::new();
    // Stereo: try the four pairings and keep the smallest.
    let mut layouts: Vec<(u64, Vec<(Vec<i64>, u32)>)> = Vec::new();
    if chans.len() == 2 {
        let (l, r) = (&chans[0], &chans[1]);
        let side: Vec<i64> = l.iter().zip(r).map(|(a, b)| a - b).collect();
        let mid: Vec<i64> = l.iter().zip(r).map(|(a, b)| (a + b) >> 1).collect();
        layouts.push((0b0001, vec![(l.clone(), bps), (r.clone(), bps)]));
        layouts.push((0b1000, vec![(l.clone(), bps), (side.clone(), bps + 1)]));
        layouts.push((0b1001, vec![(side.clone(), bps + 1), (r.clone(), bps)]));
        layouts.push((0b1010, vec![(mid, bps), (side, bps + 1)]));
    } else {
        layouts.push((chans.len() as u64 - 1, chans.iter().map(|c| (c.clone(), bps)).collect()));
    }
    let mut best: Option<(u64, Vec<u8>, u32, u64)> = None;
    for (code, subs) in layouts {
        let mut b = Bits::new();
        for (x, bits) in &subs {
            subframe(&mut b, x, *bits);
        }
        let len = b.out.len() as u64 * 8 + b.n as u64;
        if best.as_ref().is_none_or(|x| len < x.3) {
            let (acc, nb) = (b.acc, b.n);
            best = Some((code, b.out, nb, len));
            // The unfinished bits ride along in the top of the last byte.
            if nb > 0 {
                let last = ((acc & ((1 << nb) - 1)) << (8 - nb)) as u8;
                best.as_mut().unwrap().1.push(last);
            }
        }
    }
    let (code, body, _, body_bits) = best.unwrap();
    w.put(0xfff8, 16);
    let bs_code = if n == BLOCK { 0b1100 } else { 0b0111 };
    let rate_code = match rate {
        44_100 => 0b1001,
        48_000 => 0b1010,
        96_000 => 0b1011,
        88_200 => 0b0001,
        192_000 => 0b0011,
        _ => 0,
    };
    w.put(bs_code, 4);
    w.put(rate_code, 4);
    w.put(code, 4);
    w.put(match bps {
        16 => 0b100,
        24 => 0b110,
        _ => 0,
    }, 3);
    w.put(0, 1);
    // The frame number, in the stretched UTF-8 the format uses.
    if index < 0x80 {
        w.put(index as u64, 8);
    } else {
        let extra = match index {
            0..=0x7ff => 1,
            0..=0xffff => 2,
            0..=0x1f_ffff => 3,
            0..=0x3ff_ffff => 4,
            _ => 5,
        };
        let lead = (0xffu64 << (7 - extra)) & 0xff;
        w.put(lead | (index as u64 >> (6 * extra)), 8);
        for i in (0..extra).rev() {
            w.put(0x80 | ((index as u64 >> (6 * i)) & 0x3f), 8);
        }
    }
    if bs_code == 0b0111 {
        w.put(n as u64 - 1, 16);
    }
    let c = crc8(&w.out);
    w.put(c as u64, 8);
    let whole = (body_bits / 8) as usize;
    for &b in &body[..whole] {
        w.put(b as u64, 8);
    }
    let rest = (body_bits % 8) as u32;
    if rest > 0 {
        w.put((body[whole] >> (8 - rest)) as u64, rest);
    }
    w.align();
    let c = crc16(&w.out);
    w.put(c as u64, 16);
    w.out
}

/// Write interleaved f32 samples in [-1, 1] as FLAC at 16 or 24 bits.
pub fn write<W: Write>(mut w: W, samples: &[f32], sample_rate: u32, channels: u16, bits: u32) -> io::Result<()> {
    let bits = if bits > 16 { 24 } else { 16 };
    let ch = channels.max(1) as usize;
    let full = ((1i64 << (bits - 1)) - 1) as f32;
    let ints: Vec<i64> = samples.iter().map(|&s| (s.clamp(-1.0, 1.0) as f64 * full as f64).round() as i64).collect();
    let total = ints.len() / ch;
    let mut pcm = Vec::with_capacity(ints.len() * (bits as usize / 8));
    for &v in &ints[..total * ch] {
        pcm.extend_from_slice(&(v as i32).to_le_bytes()[..bits as usize / 8]);
    }
    let digest = md5(&pcm);
    drop(pcm);
    let mut frames = Vec::new();
    let (mut min_f, mut max_f) = (u32::MAX, 0u32);
    let mut index = 0u32;
    let mut at = 0;
    while at < total {
        let n = (total - at).min(BLOCK);
        let chans: Vec<Vec<i64>> = (0..ch).map(|c| (0..n).map(|i| ints[(at + i) * ch + c]).collect()).collect();
        let f = frame(index, &chans, bits, sample_rate);
        min_f = min_f.min(f.len() as u32);
        max_f = max_f.max(f.len() as u32);
        frames.extend_from_slice(&f);
        index += 1;
        at += n;
    }
    if index == 0 {
        min_f = 0;
    }
    let mut h = Bits::new();
    h.put(BLOCK as u64, 16);
    h.put(BLOCK as u64, 16);
    h.put(min_f as u64, 24);
    h.put(max_f as u64, 24);
    h.put(sample_rate as u64, 20);
    h.put(ch as u64 - 1, 3);
    h.put(bits as u64 - 1, 5);
    h.put(total as u64, 36);
    w.write_all(b"fLaC")?;
    w.write_all(&[0x80, 0, 0, 34])?;
    w.write_all(&h.out)?;
    w.write_all(&digest)?;
    w.write_all(&frames)
}

/// The checksum the stream header carries so decoders can prove the audio came back whole.
fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32).collect();
    let mut h = [0x6745_2301u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut block = |chunk: &[u8]| {
        let m: Vec<u32> = chunk.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let t = a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(t.rotate_left(S[i]));
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(c), h[3].wrapping_add(d)];
    };
    let whole = data.len() / 64 * 64;
    for c in data[..whole].chunks(64) {
        block(c);
    }
    let mut tail = data[whole..].to_vec();
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&(data.len() as u64 * 8).to_le_bytes());
    for c in tail.chunks(64) {
        block(c);
    }
    let mut out = [0u8; 16];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_matches_known_answers() {
        let hex = |d: [u8; 16]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"The quick brown fox jumps over the lazy dog")), "9e107d9d372bb6826bd81d3542a419d6");
    }

    #[test]
    fn what_goes_in_comes_back_exactly() {
        // A tune, noise, silence and a ragged end, so every subframe kind is used.
        let mut seed = 12345u32;
        let mut s = Vec::new();
        for i in 0..23_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
            let t = i as f32 / 44_100.0;
            let (l, r) = match i / 5000 {
                0 => ((t * 440.0 * 6.283).sin() * 0.6, (t * 441.0 * 6.283).sin() * 0.5),
                1 => (noise * 0.9, -noise * 0.3),
                2 => (0.0, 0.0),
                3 => (0.25, (t * 90.0 * 6.283).sin()),
                _ => (noise * 0.001, (t * 3000.0 * 6.283).sin() * 0.99),
            };
            s.push(l);
            s.push(r);
        }
        for bits in [16u32, 24] {
            let mut out = Vec::new();
            write(&mut out, &s, 44_100, 2, bits).unwrap();
            assert!(out.len() < s.len() * (bits as usize / 8), "it should come out smaller than plain audio");
            let dir = std::env::temp_dir().join(format!("cereal-flac-{}-{bits}.flac", std::process::id()));
            std::fs::write(&dir, &out).unwrap();
            let back = crate::sample::Sample::load(&dir);
            std::fs::remove_file(&dir).ok();
            let back = back.expect("our own reader takes it");
            let full = ((1i64 << (bits - 1)) - 1) as f32;
            assert_eq!(back.frames.len(), s.len() / 2);
            for (i, f) in s.chunks(2).enumerate() {
                for c in 0..2 {
                    let want = (f[c].clamp(-1.0, 1.0) as f64 * full as f64).round() / (full as f64 + 1.0);
                    let got = back.frames[i][c] as f64;
                    assert!((want - got).abs() < 0.6 / full as f64, "sample {i} at {bits} bits: {want} against {got}");
                }
            }
        }
    }
}
