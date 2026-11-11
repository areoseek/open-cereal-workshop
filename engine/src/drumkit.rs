//! Drum Modeler: sixteen modelled drums on pads, each with its own key, with a built-in step runner.
//!
//! The block is read where it sits in song and preset files (identifier "Drumaxx"). Its layout was worked out
//! from the stored kits alone and is not confirmed against the original:
//!
//! - a tag and a name, then from byte 268 sixteen pad records of 44 numbers: volume, pan, mute, solo, key, and
//!   the first 39 numbers of a one-drum block (`membrane.rs`)
//! - sixteen kit numbers: one unused, main level, swing, tone bands on, eight bands, ceiling on, its drive, its
//!   let-go time, one unused
//! - sixteen step patterns of 1026 bytes: length less one, the key that runs it, then 64 steps for each of the
//!   16 pads (top bit: the step hits, the rest: how hard)
//! - sixteen pad names (128 bytes each, the name in the last 80)
//! - one byte per pad for its cut group and one for its mixer track offset
//!
//! Which kit number is the swing and which the drive of the ceiling, the band frequencies and every curve are
//! our own guesses. Pads are not sent to their own mixer tracks yet.

use crate::membrane;
use std::sync::atomic::{AtomicU32, Ordering};

pub const PADS: usize = 16;
pub const SIZE: usize = 21644;
/// Where the numbers start in the block.
pub const FIRST: usize = 268;
/// Numbers per pad record.
pub const REC: usize = 44;
/// Where the drum's own numbers start in a pad record, and how many are kept there.
pub const DRUM: usize = 5;
pub const DRUM_COUNT: usize = 39;
/// Where the kit numbers start in the list.
pub const KIT: usize = PADS * REC;
/// Numbers in the list: the pad records, then the kit numbers.
pub const COUNT: usize = KIT + 16;
pub const PATTERNS: usize = 16;
pub const STEPS: usize = 64;
const PAT: usize = 3148;
const PAT_SIZE: usize = 2 + PADS * STEPS;
const NAMES: usize = 19564;
const CUTS: usize = 21612;
const OUTS: usize = 21628;

// what a voice gets after the list: the age of the block, the cut groups, then the patterns
const V_AGE: usize = COUNT;
const V_CUT: usize = COUNT + 1;
const V_PAT: usize = V_CUT + PADS;

/// Names of the kit numbers we show, by place.
pub const KIT_NAMES: [(usize, &str); 3] = [(1, "Main level"), (2, "Swing"), (14, "Let go")];
pub const BANDS: [f32; 8] = [80.0, 160.0, 320.0, 640.0, 1250.0, 2500.0, 5000.0, 10000.0];

/// True when the block is a kit block we can read.
pub fn fits(raw: &[u8]) -> bool {
    raw.len() >= SIZE && &raw[..4] == b"SSLF" && &raw[9..12] == b"SMD"
}

fn empty_pad(key: f32) -> [f32; REC] {
    let mut r = [0.0; REC];
    r[0] = 0.73;
    r[1] = 0.5;
    r[4] = key;
    let d = membrane::defaults();
    r[DRUM..DRUM + DRUM_COUNT].copy_from_slice(&d[..DRUM_COUNT]);
    r[DRUM + 38] = 0.0;
    r
}

const KIT_DEFS: [f32; 16] = [0.0, 0.36, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.5, 0.0];

pub fn defaults() -> Vec<f32> {
    let mut v = Vec::with_capacity(COUNT);
    for pad in 0..PADS {
        let mut r = empty_pad(36.0 + pad as f32);
        // a spread of sizes so a new kit does not sound one drum sixteen times
        r[DRUM + 29] = 0.3 + pad as f32 * 0.03;
        v.extend_from_slice(&r);
    }
    v.extend_from_slice(&KIT_DEFS);
    v
}

pub fn values(raw: &[u8]) -> Vec<f32> {
    if !fits(raw) {
        return Vec::new();
    }
    let d = defaults();
    (0..COUNT)
        .map(|i| {
            let o = FIRST + i * 4;
            let v = f32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
            if v.is_finite() { v } else { d[i] }
        })
        .collect()
}

/// A block of our own for a new instrument.
pub fn fresh() -> Vec<u8> {
    let mut out = Vec::with_capacity(SIZE);
    out.extend_from_slice(b"SSLF");
    out.extend_from_slice(&((SIZE - 8) as u32).to_le_bytes());
    out.extend_from_slice(b"3SMD");
    out.resize(FIRST, 0);
    out[12..19].copy_from_slice(b"Default");
    for v in defaults() {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for k in 0..PATTERNS {
        out.push(15);
        out.push(60 + k as u8);
        out.resize(out.len() + PADS * STEPS, 100);
    }
    out.resize(SIZE, 0);
    for pad in 0..PADS {
        set_name(&mut out, pad, &format!("Drum {}", pad + 1));
    }
    out
}

pub fn name(raw: &[u8], pad: usize) -> String {
    let at = NAMES + pad * 128;
    match raw.get(at..at + 80) {
        Some(b) => String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned(),
        None => String::new(),
    }
}

pub fn set_name(raw: &mut [u8], pad: usize, name: &str) {
    let at = NAMES + pad * 128;
    if let Some(b) = raw.get_mut(at..at + 80) {
        b.fill(0);
        let n = name.as_bytes();
        let n = &n[..n.len().min(79)];
        b[..n.len()].copy_from_slice(n);
    }
}

/// The cut group of a pad: 0 for none. Pads of one group cut each other short.
pub fn cut(raw: &[u8], pad: usize) -> u8 {
    raw.get(CUTS + pad).copied().unwrap_or(0)
}

pub fn set_cut(raw: &mut [u8], pad: usize, group: u8) {
    if let Some(b) = raw.get_mut(CUTS + pad) {
        *b = group;
    }
}

/// The mixer track offset of a pad (kept; not used for routing yet).
pub fn out_offset(raw: &[u8], pad: usize) -> i8 {
    raw.get(OUTS + pad).copied().unwrap_or(0) as i8
}

pub fn set_out_offset(raw: &mut [u8], pad: usize, by: i8) {
    if let Some(b) = raw.get_mut(OUTS + pad) {
        *b = by as u8;
    }
}

/// Steps in a pattern, 1 to 64, or none when the pattern is not in use.
pub fn pat_len(raw: &[u8], pat: usize) -> Option<usize> {
    let b = *raw.get(PAT + pat * PAT_SIZE)? as usize;
    (b < STEPS).then_some(b + 1)
}

pub fn set_pat_len(raw: &mut [u8], pat: usize, len: usize) {
    if let Some(b) = raw.get_mut(PAT + pat * PAT_SIZE) {
        *b = (len.clamp(1, STEPS) - 1) as u8;
    }
}

/// The key that runs a pattern, or none.
pub fn pat_key(raw: &[u8], pat: usize) -> Option<u8> {
    let b = *raw.get(PAT + pat * PAT_SIZE + 1)?;
    (b < 127).then_some(b)
}

pub fn set_pat_key(raw: &mut [u8], pat: usize, key: Option<u8>) {
    if let Some(b) = raw.get_mut(PAT + pat * PAT_SIZE + 1) {
        *b = key.unwrap_or(127).min(127);
    }
}

/// One step: whether it hits, and how hard (0 to 127).
pub fn step(raw: &[u8], pat: usize, pad: usize, at: usize) -> (bool, u8) {
    let b = raw.get(PAT + pat * PAT_SIZE + 2 + pad * STEPS + at).copied().unwrap_or(0);
    (b & 0x80 != 0, b & 0x7f)
}

pub fn set_step(raw: &mut [u8], pat: usize, pad: usize, at: usize, on: bool, hard: u8) {
    if pad < PADS && at < STEPS {
        if let Some(b) = raw.get_mut(PAT + pat * PAT_SIZE + 2 + pad * STEPS + at) {
            *b = (hard & 0x7f) | if on { 0x80 } else { 0 };
        }
    }
}

/// Empties one pattern: sixteen steps, nothing hitting.
pub fn reset_pattern(raw: &mut [u8], pat: usize) {
    let at = PAT + pat * PAT_SIZE;
    if let Some(b) = raw.get_mut(at + 2..at + PAT_SIZE) {
        b.fill(100);
        raw[at] = 15;
    }
}

/// Reads a pattern saved by itself into one pattern of the kit. The files we have seen hold a length and ten
/// rows of 64 steps.
pub fn import_pattern(raw: &mut [u8], pat: usize, file: &[u8]) -> bool {
    if !fits(raw) || pat >= PATTERNS || file.len() < 5 + STEPS || &file[..4] != b"TPMD" {
        return false;
    }
    let len = file[4] as usize;
    if len >= STEPS {
        return false;
    }
    reset_pattern(raw, pat);
    raw[PAT + pat * PAT_SIZE] = len as u8;
    for (pad, row) in file[5..].chunks_exact(STEPS).take(PADS).enumerate() {
        let at = PAT + pat * PAT_SIZE + 2 + pad * STEPS;
        raw[at..at + STEPS].copy_from_slice(row);
    }
    true
}

/// Puts a drum saved by itself on a pad. `p` is the instrument's number list.
pub fn load_pad(p: &mut [f32], raw: &mut [u8], pad: usize, file: &[u8]) -> bool {
    let Some((name, v)) = membrane::read_patch(file) else {
        return false;
    };
    if p.len() < COUNT || pad >= PADS {
        return false;
    }
    let at = pad * REC + DRUM;
    p[at..at + DRUM_COUNT].copy_from_slice(&v[..DRUM_COUNT]);
    p[at + 38] = 0.0;
    if !name.is_empty() {
        set_name(raw, pad, &name);
    }
    true
}

/// What a voice needs: the number list, then the age of the block, the cut groups and the patterns.
pub fn voice_list(p: &[f32], raw: &[u8]) -> Vec<f32> {
    let mut v = Vec::with_capacity(V_PAT + PATTERNS * PAT_SIZE);
    v.extend_from_slice(&p[..COUNT.min(p.len())]);
    v.resize(COUNT, 0.0);
    let ok = fits(raw);
    v.push(if ok && raw[8] == b'4' { 4.0 } else { 3.0 });
    for pad in 0..PADS {
        v.push(if ok { cut(raw, pad) as f32 } else { 0.0 });
    }
    if ok {
        v.extend(raw[PAT..PAT + PATTERNS * PAT_SIZE].iter().map(|&b| b as f32));
    } else {
        v.resize(V_PAT + PATTERNS * PAT_SIZE, 127.0);
    }
    v
}

// Pads of one cut group stop each other across notes: each hit of a group moves its counter on, and a ringing
// hit that sees the counter moved fades out. The slots are shared by every kit; two kits rarely meet in one.
static CHOKE: [AtomicU32; 64] = [const { AtomicU32::new(0) }; 64];

#[derive(Debug, Clone)]
struct Hit {
    v: membrane::Voice,
    set: membrane::Set,
    gain: [f32; 2],
    pad: u8,
    slot: u8,
    stamp: u32,
    fade: f32,
    left: i64,
}

#[derive(Debug, Clone)]
struct Runner {
    pat: usize,
    len: usize,
    /// Steps fired so far.
    n: u64,
    /// Frames since the key went down, and the frame the next step is due.
    t: f64,
    due: f64,
}

#[derive(Debug, Clone)]
struct Ceiling {
    drive: f32,
    keep: f32,
    env: f32,
}

/// One held key: a pad struck once, or a pattern running for as long as the key is down.
#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    vel: f32,
    seed: u32,
    id: usize,
    held: bool,
    main: f32,
    hits: Vec<Hit>,
    run: Option<Runner>,
    bands: Option<Box<[[membrane::Biquad; 2]; 8]>>,
    ceiling: Option<Ceiling>,
    n: u32,
}

fn pad_set(s: &[f32], pad: usize) -> membrane::Set {
    let at = pad * REC + DRUM;
    let mut v = [0.0f32; membrane::COUNT];
    v[..DRUM_COUNT].copy_from_slice(&s[at..at + DRUM_COUNT]);
    // the level of a drum on a pad is the pad's own volume
    v[38] = 1.0;
    membrane::set(&v)
}

impl Voice {
    pub fn start(s: &[f32], sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let ok = s.len() >= V_PAT + PATTERNS * PAT_SIZE;
        let age4 = ok && s[V_AGE] == 4.0;
        let kit = |i: usize| if ok { s[KIT + i].clamp(0.0, 1.0) } else { KIT_DEFS[i] };
        let main = (kit(1) / if age4 { 0.72 } else { 0.36 }).powi(2).min(8.0);
        let bands = (kit(3) >= 0.5).then(|| {
            let mut b = [[membrane::Biquad::default(); 2]; 8];
            for (i, pair) in b.iter_mut().enumerate() {
                for f in pair.iter_mut() {
                    f.peak(sr, BANDS[i], 1.4, (kit(4 + i) - 0.5) * 12.0);
                }
            }
            Box::new(b)
        });
        let ceiling = (kit(12) >= 0.5).then(|| Ceiling { drive: 1.0 + kit(13) * 7.0, keep: (-1.0 / (sr * (0.02 + kit(14).powi(2) * 1.5))).exp(), env: 0.0 });
        let mut v = Self { sr, vel, seed: seed | 1, id: s.as_ptr() as usize >> 4, held: true, main, hits: Vec::new(), run: None, bands, ceiling, n: 0 };
        if !ok {
            return v;
        }
        for pad in 0..PADS {
            if s[pad * REC + 4].round() == key as f32 && key > 0 {
                v.strike(s, pad, vel);
            }
        }
        for pat in 0..PATTERNS {
            let at = V_PAT + pat * PAT_SIZE;
            let (len, k) = (s[at] as usize, s[at + 1] as u8);
            if len < STEPS && k == key && k < 127 {
                v.run = Some(Runner { pat, len: len + 1, n: 0, t: 0.0, due: 0.0 });
                break;
            }
        }
        v
    }

    fn strike(&mut self, s: &[f32], pad: usize, vel: f32) {
        let r = &s[pad * REC..(pad + 1) * REC];
        let solo = (0..PADS).any(|k| s[k * REC + 3] >= 0.5);
        if r[2] >= 0.5 || (solo && r[3] < 0.5) {
            return;
        }
        let age4 = s[V_AGE] == 4.0;
        let vol = (r[0].clamp(0.0, 1.0) / if age4 { 0.98 } else { 0.73 }).powi(2);
        let pan = r[1].clamp(0.0, 1.0);
        let gain = [vol * (2.0 - 2.0 * pan).min(1.0), vol * (2.0 * pan).min(1.0)];
        let set = pad_set(s, pad);
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let group = s[V_CUT + pad] as usize;
        let slot = ((self.id ^ group.wrapping_mul(7)) % CHOKE.len()) as u8;
        let stamp = if group > 0 { CHOKE[slot as usize].fetch_add(1, Ordering::Relaxed).wrapping_add(1) } else { 0 };
        // a pad struck again takes over from its own ringing hit
        for h in self.hits.iter_mut().filter(|h| h.pad == pad as u8) {
            h.fade = h.fade.min(0.999);
        }
        let left = (set.tail() * self.sr) as i64;
        let v = membrane::Voice::start(&set, self.sr, vel, 1.0, self.seed);
        self.hits.push(Hit { v, set, gain, pad: pad as u8, slot: if group > 0 { slot } else { u8::MAX }, stamp, fade: 1.0, left });
    }

    /// The key was let go: a running pattern stops, what rings rings out. Returns the seconds still needed.
    pub fn release(&mut self) -> f32 {
        self.held = false;
        self.hits.iter().map(|h| h.left).max().unwrap_or(0).max(0) as f32 / self.sr + 0.02
    }

    pub fn frame(&mut self, s: &[f32]) -> [f32; 2] {
        if self.held && self.run.is_some() && s.len() >= V_PAT + PATTERNS * PAT_SIZE {
            let step = self.sr as f64 * 15.0 / crate::minis::tempo() as f64;
            let swing = s[KIT + 2].clamp(0.0, 1.0) as f64;
            loop {
                let Some(r) = self.run.as_mut() else { break };
                let at = (r.n % r.len as u64) as usize;
                // swing holds every second step back
                let when = r.due + if at % 2 == 1 { swing * 0.5 * step } else { 0.0 };
                if r.t < when {
                    r.t += 1.0;
                    break;
                }
                r.n += 1;
                r.due += step;
                let pat = r.pat;
                for pad in 0..PADS {
                    let b = s[V_PAT + pat * PAT_SIZE + 2 + pad * STEPS + at] as u8;
                    if b & 0x80 != 0 {
                        let vel = self.vel * (b & 0x7f) as f32 / 127.0;
                        self.strike(s, pad, vel);
                    }
                }
            }
        }
        self.n = self.n.wrapping_add(1);
        let look = self.n % 32 == 0;
        let mut out = [0.0f32; 2];
        for h in self.hits.iter_mut() {
            if look && h.slot != u8::MAX && h.fade >= 1.0 && CHOKE[h.slot as usize].load(Ordering::Relaxed) != h.stamp {
                h.fade = 0.999;
            }
            let x = h.v.frame(&h.set);
            if h.fade < 1.0 {
                // about five thousandths of a second
                h.fade *= 1.0 - 900.0 / self.sr;
            }
            h.left -= 1;
            out[0] += x[0] * h.gain[0] * h.fade;
            out[1] += x[1] * h.gain[1] * h.fade;
        }
        if look {
            self.hits.retain(|h| h.left > 0 && h.fade > 1e-4);
        }
        out[0] *= self.main;
        out[1] *= self.main;
        if let Some(b) = self.bands.as_mut() {
            for pair in b.iter_mut() {
                out[0] = pair[0].run(out[0]);
                out[1] = pair[1].run(out[1]);
            }
        }
        if let Some(c) = self.ceiling.as_mut() {
            out[0] *= c.drive;
            out[1] *= c.drive;
            let top = out[0].abs().max(out[1].abs());
            c.env = if top > c.env { top } else { c.env * c.keep };
            if c.env > 1.0 {
                out[0] /= c.env;
                out[1] /= c.env;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(raw: &[u8]) -> Vec<f32> {
        voice_list(&values(raw), raw)
    }

    fn play(s: &[f32], key: u8, hold: usize, more: usize) -> Vec<[f32; 2]> {
        let mut v = Voice::start(s, 44100.0, key, 0.8, 7);
        let mut out: Vec<[f32; 2]> = (0..hold).map(|_| v.frame(s)).collect();
        v.release();
        out.extend((0..more).map(|_| v.frame(s)));
        out
    }

    fn peak(x: &[[f32; 2]]) -> f32 {
        x.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    #[test]
    fn a_new_block_reads_back_and_each_pad_answers_its_key() {
        let raw = fresh();
        assert_eq!(raw.len(), SIZE);
        assert!(fits(&raw));
        assert_eq!(values(&raw), defaults());
        let s = list(&raw);
        for pad in 0..PADS {
            let top = peak(&play(&s, 36 + pad as u8, 4000, 0));
            assert!(top > 0.05 && top < 2.0, "pad {pad}: {top}");
        }
        assert_eq!(peak(&play(&s, 20, 4000, 0)), 0.0);
        assert_eq!(name(&raw, 3), "Drum 4");
    }

    #[test]
    fn mute_solo_pan_and_volume_work() {
        let raw = fresh();
        let mut p = values(&raw);
        p[2] = 1.0;
        assert_eq!(peak(&play(&voice_list(&p, &raw), 36, 4000, 0)), 0.0);
        p[2] = 0.0;
        p[REC + 3] = 1.0;
        assert_eq!(peak(&play(&voice_list(&p, &raw), 36, 4000, 0)), 0.0);
        assert!(peak(&play(&voice_list(&p, &raw), 37, 4000, 0)) > 0.05);
        p[REC + 3] = 0.0;
        p[1] = 0.0;
        let out = play(&voice_list(&p, &raw), 36, 4000, 0);
        assert!(out.iter().all(|f| f[1] == 0.0) && peak(&out) > 0.05);
        p[1] = 0.5;
        let full = peak(&play(&voice_list(&p, &raw), 36, 4000, 0));
        p[0] = 0.3;
        assert!(peak(&play(&voice_list(&p, &raw), 36, 4000, 0)) < full * 0.5);
    }

    #[test]
    fn a_pattern_key_runs_its_steps_in_time_while_held() {
        let mut raw = fresh();
        set_pat_len(&mut raw, 0, 4);
        set_step(&mut raw, 0, 0, 0, true, 127);
        set_step(&mut raw, 0, 0, 2, true, 127);
        assert_eq!(pat_key(&raw, 0), Some(60));
        assert_eq!(step(&raw, 0, 0, 2), (true, 127));
        let mut p = values(&raw);
        // a short drum so the hits stand apart
        p[DRUM + 6] = 0.1;
        p[DRUM + 28] = 0.2;
        let s = voice_list(&p, &raw);
        crate::minis::set_tempo(120.0);
        // a step is 5512.5 frames: hits at 0, 11025, 22050 ...
        let out = play(&s, 60, 30000, 8000);
        let loud = |a: usize| peak(&out[a..a + 2000]);
        assert!(loud(0) > 0.05 && loud(11025) > 0.05 && loud(22050) > 0.05);
        assert!(loud(8000) < loud(0) * 0.2, "{} {}", loud(8000), loud(0));
        // let go at 30000: the hit due at 33075 never comes
        assert!(peak(&out[33075..]) < loud(0) * 0.1);
    }

    #[test]
    fn pads_of_one_cut_group_stop_each_other() {
        let mut raw = fresh();
        set_cut(&mut raw, 0, 1);
        set_cut(&mut raw, 1, 1);
        let mut p = values(&raw);
        p[DRUM + 6] = 0.9;
        p[DRUM + 28] = 1.0;
        let s = voice_list(&p, &raw);
        let mut a = Voice::start(&s, 44100.0, 36, 0.8, 3);
        let before: Vec<[f32; 2]> = (0..4000).map(|_| a.frame(&s)).collect();
        let mut b = Voice::start(&s, 44100.0, 37, 0.8, 5);
        let after: Vec<[f32; 2]> = (0..4000).map(|_| { b.frame(&s); a.frame(&s) }).collect();
        assert!(peak(&before[3000..]) > 0.01);
        assert_eq!(peak(&after[2000..]), 0.0);
        assert!(a.hits.is_empty());
    }

    #[test]
    fn a_saved_drum_and_a_saved_pattern_load() {
        let mut raw = fresh();
        let mut p = values(&raw);
        let mut file = b"1PMDSmack".to_vec();
        file.resize(132, 0);
        for i in 0..35 {
            file.extend_from_slice(&(i as f32 / 100.0).to_le_bytes());
        }
        assert!(load_pad(&mut p, &mut raw, 2, &file));
        assert_eq!(name(&raw, 2), "Smack");
        let at = 2 * REC + DRUM;
        assert_eq!(p[at + 11], 0.11);
        assert_eq!(p[at + 12], 1.0);
        assert_eq!(p[at + 13], 0.12);
        assert_eq!(&p[at + 15..at + 18], &[0.5, 0.5, 0.0]);
        assert_eq!(p[at + 18], 0.14);
        let mut pat = b"TPMD".to_vec();
        pat.push(7);
        pat.resize(5 + 640, 0x7f);
        pat[5 + 64 + 4] = 0xe0;
        assert!(import_pattern(&mut raw, 5, &pat));
        assert_eq!(pat_len(&raw, 5), Some(8));
        assert_eq!(step(&raw, 5, 1, 4), (true, 0x60));
        assert_eq!(step(&raw, 5, 12, 4), (false, 100));
    }
}
