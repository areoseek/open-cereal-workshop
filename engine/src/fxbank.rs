//! The filter bank unit: eight filter units side by side or in a row, each with a curve shaper and drawn
//! lines that move its settings. The stored block is a version number followed by a zlib stream; inside are a
//! short header, a note text and eight equal records of knobs and drawn lines.

use crate::fx::{drawn_table_n, Ctx};
use std::f32::consts::PI;

pub const BANKS: usize = 8;
/// Settings in front of the banks: pad X, pad Y, follower attack, follower release.
pub const HEAD: usize = 4;
/// Settings of one bank, in the order of `NAMES`.
pub const PER: usize = 17;
/// The two settings listed behind the banks: the unit's volume and how far the slow waves swing.
pub const TAIL: usize = 2;
pub const LISTED: usize = HEAD + BANKS * PER + TAIL;
/// Drawn lines in one bank: the shaper, then for each of eight targets ten patterns, a slow oscillator and
/// three mappings (pad X, pad Y, input follower).
pub const LINES: usize = 113;
const TARGETS: usize = 8;
const WS_N: usize = 257;
const PAT_N: usize = 257;
const MAP_N: usize = 65;
/// Table space of one target: pattern length in beats (0 = unused), the slow wave's swing, turns per beat
/// and tension, the pattern, three mappings, where the pattern's loop starts and ends in beats (-1: none),
/// and where the loop's first stretch ends.
pub const ART: usize = 4 + PAT_N + 3 * MAP_N + 3;
const TABLES: usize = WS_N + TARGETS * ART;
/// How hard a slow wave's tension bends its triangle.
const WAVE_BEND: f32 = 4.88;
/// Frames between two readings of the drawn lines.
const CHUNK: usize = 8;
/// The half-band all-pass pair both doubling steps use.
const HALF: [f32; 6] = [0.039_151_598, 0.147_377_114, 0.302_646_848, 0.482_468_543, 0.674_615_919, 0.883_005_026];

/// Where the bank knobs sit in front of the bank's first line: 14 numbers, then three switch bytes.
const KNOBS_BACK: usize = 59;
/// Header offsets of the four settings in front.
const HEAD_OFF: [usize; HEAD] = [24, 28, 12, 16];
/// Header offsets of the two settings listed last.
const TAIL_OFF: [usize; TAIL] = [0, 4];
/// Switch bytes behind the 14 numbers, in our order: on, shaper on, shaper two-sided.
const SWITCH_OFF: [usize; 3] = [57, 58, 56];

fn inflate(z: &[u8]) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(z, 1 << 24).ok()
}

/// Every drawn line of the block, in order: (start, points, length, end). The text in front has no fixed
/// length and changed its coding with the versions, so the lines are found by their own shape.
fn lines(d: &[u8]) -> Option<Vec<(usize, usize, f32, usize)>> {
    let int = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut out = Vec::with_capacity(BANKS * LINES);
    let mut at = 64;
    while at + 12 <= d.len() && out.len() < BANKS * LINES {
        let hit = (|| {
            let (kind, layout, n) = (int(at)?, int(at + 4)?, int(at + 8)?);
            if !(1..=3).contains(&kind) || !(0..=3).contains(&layout) || !(1..=4096).contains(&n) {
                return None;
            }
            let size = match layout {
                0 => 12,
                1 => 16,
                _ => 24,
            };
            let n = n as usize;
            let end = at + 12 + n * size + 20;
            if end > d.len() {
                return None;
            }
            let mut span = 0f64;
            for k in 0..n {
                let b = &d[at + 12 + k * size..];
                let (x, y) = if size == 24 {
                    (f64::from_le_bytes(b[0..8].try_into().ok()?), f64::from_le_bytes(b[8..16].try_into().ok()?))
                } else {
                    (f32::from_le_bytes(b[0..4].try_into().ok()?) as f64, f32::from_le_bytes(b[4..8].try_into().ok()?) as f64)
                };
                if !(0.0..1e4).contains(&x) || !(-0.01..=1.01).contains(&y) {
                    return None;
                }
                span += x;
            }
            Some((at, n, span as f32, end))
        })();
        match hit {
            Some(h) => {
                at = h.3;
                out.push(h);
            }
            None => at += 1,
        }
    }
    (out.len() == BANKS * LINES).then_some(out)
}

/// Read a stored block: the listed settings, then every bank's tables.
pub fn decode(state: &[u8]) -> Option<Vec<f32>> {
    let d = inflate(state.get(4..)?)?;
    let l = lines(&d)?;
    let int = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut p = Vec::with_capacity(LISTED + BANKS * TABLES);
    for o in HEAD_OFF {
        p.push(int(o)? as f32);
    }
    for b in 0..BANKS {
        let at = l[b * LINES].0.checked_sub(KNOBS_BACK)?;
        for i in 0..14 {
            p.push(int(at + i * 4)? as f32);
        }
        for o in SWITCH_OFF {
            p.push(d[at + o] as f32);
        }
    }
    for o in TAIL_OFF {
        p.push(int(o)? as f32);
    }
    for b in 0..BANKS {
        let first = b * LINES;
        drawn_table_n(&d, l[first].0, WS_N, &mut p)?;
        for a in 0..TARGETS {
            let k = first + 1 + a * 14;
            let (head, n, span, _) = l[k];
            // a pattern runs when its live switch is on; without the tempo switch its beats are half seconds
            // (kept here as a negative length)
            let timed = int(l[k].3 - 20)? & 1 != 0;
            // (a live pattern of one point just holds that point's height)
            p.push(if int(l[k].3 - 16)? == 0 {
                0.0
            } else if n > 1 && span > 1e-4 {
                if timed { span } else { -span }
            } else {
                1e-3
            });
            // the slow wave: its first point says how far it swings and which way (the middle is none); behind
            // the line sit its speed and how far its triangle is bent. It runs while its live switch is on.
            let (lh, ln, _, lend) = l[k + 10];
            let size = (lend - 20 - lh - 12) / ln;
            let y = if size == 24 { f64::from_le_bytes(d[lh + 20..lh + 28].try_into().ok()?) as f32 } else { f32::from_le_bytes(d[lh + 16..lh + 20].try_into().ok()?) };
            let live = int(lend - 16)? != 0;
            p.push(if live { (2.0 * y - 1.0).clamp(-1.0, 1.0) } else { 0.0 });
            // turns per beat (a free wave counts half seconds as beats and is kept negative)
            let turns = 16.0 * (1e4f32.powf(int(lend)?.clamp(0, 65535) as f32 / 65535.0) - 1.0) / 9999.0;
            p.push(if int(lend - 20)? & 1 != 0 { turns } else { -turns });
            p.push((int(lend + 4)? as f32 / 100.0).clamp(-1.0, 1.0));
            drawn_table_n(&d, head, PAT_N, &mut p)?;
            for m in 11..14 {
                drawn_table_n(&d, l[k + m].0, MAP_N, &mut p)?;
            }
            // the pattern's loop: point numbers behind the line, turned into beats
            let (from, to) = (int(l[k].3 - 8)?, int(l[k].3 - 4)?);
            let size = (l[k].3 - 20 - head - 12) / n;
            let at = |i: i32| -> f32 {
                (1..=(i.max(0) as usize).min(n - 1))
                    .map(|j| {
                        let o = head + 12 + j * size;
                        if size == 24 {
                            f64::from_le_bytes(d[o..o + 8].try_into().unwrap()) as f32
                        } else {
                            f32::from_le_bytes(d[o..o + 4].try_into().unwrap())
                        }
                    })
                    .sum()
            };
            let looped = from >= 0 && to > from;
            p.push(if looped { at(from) } else { -1.0 });
            p.push(if looped { at(to) } else { -1.0 });
            p.push(if looped { at(from + 1) } else { -1.0 });
        }
    }
    Some(p)
}

/// Write the listed settings into a stored block. An untouched block comes back byte for byte.
pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let made;
    let base = match base {
        Some(b) => b,
        None => {
            made = fresh();
            &made[..]
        }
    };
    let Some(old) = decode(base) else { return base.to_vec() };
    if p.len() < LISTED || old[..LISTED] == p[..LISTED] {
        return base.to_vec();
    }
    let (Some(mut d), true) = (inflate(&base[4..]), base.len() >= 4) else { return base.to_vec() };
    let Some(l) = lines(&d) else { return base.to_vec() };
    let mut put = |o: usize, v: f32| d[o..o + 4].copy_from_slice(&(v.round() as i32).to_le_bytes());
    for (k, o) in HEAD_OFF.into_iter().enumerate() {
        put(o, p[k]);
    }
    for (k, o) in TAIL_OFF.into_iter().enumerate() {
        put(o, p[HEAD + BANKS * PER + k]);
    }
    for b in 0..BANKS {
        let at = l[b * LINES].0 - KNOBS_BACK;
        let q = &p[HEAD + b * PER..HEAD + (b + 1) * PER];
        for i in 0..14 {
            put(at + i * 4, q[i]);
        }
    }
    for b in 0..BANKS {
        let at = l[b * LINES].0 - KNOBS_BACK;
        for (k, o) in SWITCH_OFF.into_iter().enumerate() {
            d[at + o] = (p[HEAD + b * PER + 14 + k] >= 0.5) as u8;
        }
    }
    let mut out = base[..4].to_vec();
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
    out
}

/// Drawn line `k` of the block (banks follow each other, `LINES` each) as points. Lines stored in an older
/// point layout come back with straight stretches.
pub fn line(state: &[u8], k: usize) -> Option<crate::fm::Curve> {
    use crate::fm::{Curve, Pt};
    let d = inflate(state.get(4..)?)?;
    let l = lines(&d)?;
    let (head, n, _, end) = *l.get(k)?;
    if let Some(c) = Curve::read_at(&d, head) {
        return Some(c);
    }
    let int = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let size = (end - 20 - head - 12) / n;
    let mut c = Curve::default();
    for i in 0..n {
        let o = head + 12 + i * size;
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        c.pts.push(Pt { x: if i == 0 { 0.0 } else { f(o) }, y: f(o + 4).clamp(0.0, 1.0), ..Default::default() });
    }
    c.flags = int(end - 20)?;
    c.decay = int(end - 12)?;
    c.loop_start = int(end - 8)?;
    c.sustain = int(end - 4)?;
    Some(c)
}

/// The block with line `k` replaced.
pub fn set_line(state: &[u8], k: usize, c: &crate::fm::Curve) -> Option<Vec<u8>> {
    let mut d = inflate(state.get(4..)?)?;
    let head = lines(&d)?.get(k)?.0;
    let int = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (kind, layout) = (int(&d, head)?, int(&d, head + 4)?);
    if !crate::fm::Curve::replace_at(&mut d, head, c) {
        return None;
    }
    // a slow wave's record carries one more number behind it once its line is in the current layout
    if kind == 2 && layout < 3 && int(&d, head + 4)? == 3 {
        let end = lines(&d)?.get(k)?.3;
        if end + 16 > d.len() {
            return None;
        }
        d.splice(end + 16..end + 16, [0u8; 4]);
    }
    // the block still has to read as a whole
    lines(&d)?;
    let mut out = state[..4].to_vec();
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
    Some(out)
}

/// The four numbers stored behind line `k` (a pattern's or slow wave's own settings).
pub fn after(state: &[u8], k: usize) -> Option<[i32; 4]> {
    let d = inflate(state.get(4..)?)?;
    let end = lines(&d)?.get(k)?.3;
    let mut out = [0; 4];
    for (i, o) in out.iter_mut().enumerate() {
        *o = i32::from_le_bytes(d.get(end + i * 4..end + i * 4 + 4)?.try_into().ok()?);
    }
    Some(out)
}

/// The stored bytes of line `k` with the numbers behind it, for looking at the layout.
pub fn raw_line(state: &[u8], k: usize) -> Option<Vec<u8>> {
    let d = inflate(state.get(4..)?)?;
    let (head, _, _, end) = *lines(&d)?.get(k)?;
    d.get(head..end + 16).map(|b| b.to_vec())
}

/// The block with the four numbers behind line `k` replaced.
pub fn set_after(state: &[u8], k: usize, v: [i32; 4]) -> Option<Vec<u8>> {
    let mut d = inflate(state.get(4..)?)?;
    let end = lines(&d)?.get(k)?.3;
    for (i, x) in v.iter().enumerate() {
        d.get_mut(end + i * 4..end + i * 4 + 4)?.copy_from_slice(&x.to_le_bytes());
    }
    let mut out = state[..4].to_vec();
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
    Some(out)
}

/// The block with the whole number at header offset `o` replaced (for trying out the settings in front).
pub fn set_head(state: &[u8], o: usize, v: i32) -> Option<Vec<u8>> {
    let mut d = inflate(state.get(4..)?)?;
    if o + 4 > 64 {
        return None;
    }
    d.get_mut(o..o + 4)?.copy_from_slice(&v.to_le_bytes());
    let mut out = state[..4].to_vec();
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
    Some(out)
}

/// A fresh block: bank 1 on as a plain low-pass, every line level.
pub fn fresh() -> Vec<u8> {
    let mut d: Vec<u8> = Vec::with_capacity(64 + BANKS * 7420);
    let int = |d: &mut Vec<u8>, xs: &[i32]| {
        for x in xs {
            d.extend_from_slice(&x.to_le_bytes());
        }
    };
    // one drawn line in the 16-byte point layout
    let line = |d: &mut Vec<u8>, kind: i32, pts: &[(f32, f32)], tail: [i32; 5]| {
        for x in [kind, 1, pts.len() as i32] {
            d.extend_from_slice(&x.to_le_bytes());
        }
        for &(x, y) in pts {
            d.extend_from_slice(&x.to_le_bytes());
            d.extend_from_slice(&y.to_le_bytes());
            d.extend_from_slice(&[0; 8]);
        }
        for x in tail {
            d.extend_from_slice(&x.to_le_bytes());
        }
    };
    int(&mut d, &[128, 128, 128, 4096, 8192, 1, 512, 512, 0, 0x3f77_f14b, -1, 0, 1, 0x0001_0001, 0, 0]);
    for b in 0..BANKS {
        int(&mut d, &[128, 128, 128, 128, 0, 0, 128, 128, 0, 128, 128, 0, 1, 0]);
        d.extend_from_slice(&[0, (b == 0) as u8, 0]);
        line(&mut d, 3, &[(0.0, 0.0), (1.0, 1.0)], [0, -1, -1, -1, -1]);
        d.push(0);
        for a in 0..TARGETS {
            let centred = matches!(a, 0 | 2 | 3);
            int(&mut d, &[1]);
            for v in [if centred { 0f32 } else { 1.0 }, 1.0, 1.0] {
                d.extend_from_slice(&v.to_le_bytes());
            }
            for _ in 0..10 {
                line(&mut d, 1, &[(0.0, 1.0)], [1, 0, -1, -1, -1]);
                int(&mut d, &[128, 128, 0, 128]);
            }
            line(&mut d, 2, &[(0.0, 1.0)], [1, 0, -1, -1, -1]);
            int(&mut d, &[40000, 0, 0, 0]);
            let y = if centred { 0.5 } else { 1.0 };
            for _ in 0..3 {
                line(&mut d, 3, &[(0.0, y), (1.0, y)], [0, -1, -1, -1, -1]);
            }
        }
    }
    let mut out = 2i32.to_le_bytes().to_vec();
    out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
    out
}

/// What the unit remembers between blocks.
#[derive(Clone)]
pub struct State {
    /// Place in each target's pattern, in beats.
    pos: [[f32; TARGETS]; BANKS],
    /// Where each target's slow wave is in its turn (0..1).
    wave: [[f32; TARGETS]; BANKS],
    /// How far off the drawn line each pattern was when it last went round.
    off: [[f32; TARGETS]; BANKS],
    /// Filter memory: bank, stage, channel.
    mem: [[[[f64; 12]; 2]; 3]; BANKS],
    /// Output gains in use, left and right.
    gain: [[f32; 2]; BANKS],
    /// Shaper share in use.
    wmix: [f32; BANKS],
    /// Shaper drive in use, where it is heading, and the steps left to get there.
    drive: [(f32, f32, u32); BANKS],
    /// The same for the shaper's mix knob and the cutoff knob.
    mixk: [(f32, f32, u32); BANKS],
    cutk: [(f32, f32, u32); BANKS],
    follow: f32,
    late: f32,
    /// The two doubling steps up and the two halving steps down, per channel.
    up: [[[f32; 12]; 2]; 2],
    down: [[[f32; 12]; 2]; 2],
}

impl Default for State {
    fn default() -> Self {
        Self { pos: [[0.0; TARGETS]; BANKS], wave: [[0.0; TARGETS]; BANKS], off: [[0.0; TARGETS]; BANKS], mem: [[[[0.0; 12]; 2]; 3]; BANKS], gain: [[-1.0; 2]; BANKS], wmix: [0.0; BANKS], drive: [(1.0, 1.0, 0); BANKS], mixk: [(0.0, 0.0, 0); BANKS], cutk: [(CUT0, CUT0, 0); BANKS], follow: 0.0, late: 0.0, up: [[[0.0; 12]; 2]; 2], down: [[[0.0; 12]; 2]; 2] }
    }
}

fn look(t: &[f32], x: f32) -> f32 {
    let f = x.clamp(0.0, 1.0) * (t.len() - 1) as f32;
    let i = (f as usize).min(t.len() - 2);
    t[i] + (t[i + 1] - t[i]) * (f - i as f32)
}

/// The knob curve the unit uses everywhere: 0 at 0, 1 at 1, bent by `k`.
fn cur(x: f32, k: f32) -> f32 {
    ((1.0 + k).powf(x) - 1.0) / k
}

/// One of the two all-pass chains of a half-band pair (the same pair the morphing equaliser uses).
fn chain(z: &mut [f32; 12], x: f32, odd: usize) -> f32 {
    let mut v = x;
    for j in (odd..6).step_by(2) {
        let y = HALF[j] * (v - z[j * 2 + 1]) + z[j * 2];
        z[j * 2] = v;
        z[j * 2 + 1] = y;
        v = y;
    }
    v
}

/// One filter stage as it runs at four times the rate.
#[derive(Clone, Copy)]
enum Stage {
    /// State-variable loop with its three outputs mixed.
    Mix { f: f64, d: f64, lo: f64, bd: f64, hi: f64 },
    /// Two-integrator low-pass, or what it leaves over.
    Pole { f: f64, d: f64, rest: bool },
    Biquad { b: [f64; 3], a: [f64; 2] },
    /// Four one-pole steps with feedback around them.
    Ladder { g: f64, k: f64, gain: f64, post: f64 },
}

impl Stage {
    #[inline]
    fn run(&self, m: &mut [f64; 12], x: f64) -> f64 {
        match *self {
            Stage::Mix { f, d, lo, bd, hi } => {
                m[0] += f * m[1];
                let high = x - m[0] - d * m[1];
                m[1] += f * high;
                lo * m[0] + bd * m[1] + hi * high
            }
            Stage::Pole { f, d, rest } => {
                m[1] += f * (x - m[0] - d * m[1]);
                m[0] += f * m[1];
                if rest {
                    x - m[0]
                } else {
                    m[0]
                }
            }
            Stage::Biquad { b, a } => {
                let y = b[0] * x + m[0];
                m[0] = b[1] * x - a[0] * y + m[1];
                m[1] = b[2] * x - a[1] * y;
                y
            }
            Stage::Ladder { g, k, gain, post } => {
                // four one-pole stages in a loop that hears its own output one frame late. Each stage leans a
                // little on its previous input, more so towards the top of the cutoff. `gain` is the drive over
                // the knee: the stages bend at the input's own level, so nothing has to be scaled back after
                let sat = |d: f64| if gain <= 0.0 { d } else { d / (1.0 + (d * gain).abs().powf(LADDER_BEND)).powf(1.0 / LADDER_BEND) };
                let a = 0.145 + 0.117 * g * g;
                let mut v = sat(x - k * m[3]);
                for i in 0..4 {
                    let vin = (v + a * m[4 + i]) / (1.0 + a);
                    m[4 + i] = v;
                    m[i] += g * (vin - sat(m[i]));
                    v = if i == 3 { m[i] } else { sat(m[i]) };
                }
                post * m[3]
            }
        }
    }
}

/// Peak width against the resonance knob, at eighth steps (measured).
const PEAK_Q: [f32; 9] = [100.0, 27.08, 15.1, 5.61, 2.275, 0.909, 0.388, 0.179, 0.100];
/// How long the shaper's drive takes to reach a new setting, in frames.
const DRIVE_RAMP: u32 = 104;
const CUT0: f32 = 1.0;

/// Move a gliding knob one step on towards `to`.
fn glide(k: &mut (f32, f32, u32), to: f32, steps: u32, by: u32) -> f32 {
    if k.1 != to {
        *k = (k.0, to, steps);
    }
    if k.2 > by {
        k.0 += (k.1 - k.0) * by as f32 / k.2 as f32;
        k.2 -= by;
    } else {
        k.0 = k.1;
        k.2 = 0;
    }
    k.0
}

fn steps(t: &[f32; 9], r: f32) -> f32 {
    let f = r.clamp(0.0, 1.0) * 8.0;
    let i = (f as usize).min(7);
    (t[i].ln() + (t[i + 1].ln() - t[i].ln()) * (f - i as f32)).exp()
}

/// How sharply the ladder's stages bend when driven (fitted).
const LADDER_BEND: f64 = 2.152;
/// The ladder's per-stage turn (natural log) at these cutoff knob positions (measured).
const LADDER_CUT: [f64; 10] = [0.0, 32.0, 64.0, 96.0, 128.0, 160.0, 192.0, 224.0, 240.0, 256.0];
const LADDER_TURN: [f64; 10] = [-5.543741, -4.669523, -3.919280, -3.225103, -2.567073, -1.943882, -1.367171, -0.851061, -0.614876, -0.378475];
/// The ladder's loop strength with the resonance knob at its top, at the same positions (measured).
const LADDER_LOOP: [f64; 10] = [3.7953, 3.8058, 3.8317, 3.8760, 3.9605, 4.1063, 4.3400, 4.6002, 4.6536, 4.3380];
/// The ladder's make-up level (natural log) at these low knob positions (measured).
const LADDER_LOW: [f64; 14] = [0.0, 32.0, 48.0, 64.0, 72.0, 80.0, 88.0, 96.0, 104.0, 112.0, 120.0, 136.0, 160.0, 192.0];
const LADDER_MADE: [f64; 14] = [0.0, 0.0015, 0.00322, 0.0212, 0.08732, 0.22638, 0.44008, 0.71089, 1.01819, 1.34121, 1.67698, 2.35973, 3.39982, 4.77785];

/// A smooth line through measured points (`xs` rising), never overshooting between them; straight on past
/// either end.
fn curve(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let n = xs.len();
    let rise = |i: usize| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]);
    let lean = |i: usize| {
        if i == 0 {
            rise(0)
        } else if i == n - 1 {
            rise(n - 2)
        } else {
            let (a, b) = (rise(i - 1), rise(i));
            if a * b <= 0.0 {
                0.0
            } else {
                // weighted harmonic mean of the two sides
                let (ha, hb) = (xs[i] - xs[i - 1], xs[i + 1] - xs[i]);
                let (wa, wb) = (2.0 * hb + ha, hb + 2.0 * ha);
                (wa + wb) / (wa / a + wb / b)
            }
        }
    };
    if x <= xs[0] {
        return ys[0] + (x - xs[0]) * lean(0);
    }
    if x >= xs[n - 1] {
        return ys[n - 1] + (x - xs[n - 1]) * lean(n - 1);
    }
    let i = (0..n - 1).rfind(|&i| xs[i] <= x).unwrap_or(0);
    let h = xs[i + 1] - xs[i];
    let t = (x - xs[i]) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * ys[i] + (t3 - 2.0 * t2 + t) * h * lean(i) + (-2.0 * t3 + 3.0 * t2) * ys[i + 1] + (t3 - t2) * h * lean(i + 1)
}

/// The stages of one bank for a cutoff and resonance position (both 0..1 on the knob's own scale; the cutoff may
/// be pushed past the top). `n` stages, `alt` for the spread-out slopes. `mix` is the low, band and high share
/// of the first type; `lift` is how far the shelves and the peak raise or lower their band (the low knob sets it:
/// flat in the middle, eight times at the top).
fn plan(kind: i32, n: usize, alt: bool, cut: f32, res: f32, mix: [f32; 3], lift: f32) -> [Option<Stage>; 3] {
    let mut out = [None; 3];
    let res = res.clamp(0.0, 1.0);
    let cut = cut.max(0.0);
    let root = 1.0 / n.max(1) as f32;
    for (s, slot) in out.iter_mut().enumerate().take(n) {
        // the spread-out slopes move every further stage up towards the top (fitted roughly, three cutoffs each)
        let x = if alt && s > 0 { cut + (1.0 - cut).max(0.0) * if n == 2 { 0.5 } else { 0.2 * s as f32 } } else { cut };
        // cutoff as a share of the song's rate
        let share = |lo: f32, top: f32, k: f32, cap: f32| (lo + (top - lo) * cur(x, k)).min(cap) as f64;
        let turn = |share: f64| 2.0 * (std::f64::consts::PI * share / 4.0).sin();
        *slot = Some(match kind {
            0 => {
                let last = s + 1 == n;
                // measured oddity: with the knob at its very top the loop is damped more again
                let d = if !last { 1.0 } else if res > 0.998 { 0.2 } else { 1.0 - 0.9 * res };
                let (lo, bd, hi) = if last { (mix[0], mix[1], mix[2]) } else { (1.0, 0.0, 0.0) };
                Stage::Mix { f: turn(share(0.001_587_3, 0.254_63, 50.0, 0.42)), d: d as f64, lo: lo as f64, bd: bd as f64, hi: hi as f64 }
            }
            1 | 2 => {
                let q = 1.0 + 49.0 * cur(res, 1000.0);
                Stage::Pole { f: turn(share(0.001_748_3, 0.25, 75.0, 0.401_75)), d: (1.0 / q).powf(root) as f64, rest: kind == 2 }
            }
            11 => {
                let at = x as f64 * 256.0;
                let g = curve(&LADDER_CUT, &LADDER_TURN, at).exp().min(0.95);
                // the low knob drives the stages: none at the bottom (clean, level in = level out), doubling
                // every sixteenth of the way up
                let low = ((lift as f64).log2() / 3.0 + 1.0) * 64.0;
                let drive = 7.683 * ((low / 16.0).exp2() - 1.0).max(0.0) / 255.0;
                let gain = drive / 0.259;
                // drive up to the knee strengthens the resonance
                let r = res as f64 * (1.0 + 0.316_23 * gain.min(1.0));
                // more stages each loop less, and less again towards the top of the knob (measured at three settings)
                let each = match n {
                    2 => 0.7977 - 0.1284 * r + 0.0389 * r * r,
                    3 => 0.6611 - 0.1672 * r + 0.0512 * r * r,
                    _ => 1.0,
                };
                let k = r * each * curve(&LADDER_CUT, &LADDER_LOOP, at).max(3.0);
                // the level is made up after the stages: nothing while clean, as much as the drive far up
                let top = LADDER_LOW[LADDER_LOW.len() - 1];
                let made = if low > top { LADDER_MADE[LADDER_MADE.len() - 1] + (low - top) / 16.0 * std::f64::consts::LN_2 } else { curve(&LADDER_LOW, &LADDER_MADE, low) };
                Stage::Ladder { g, k, gain, post: (1.0 + 0.5 * k) * made.exp() }
            }
            _ => {
                let sh = if kind == 3 { share(0.003, 1.0, 1500.0, 1.0) } else { share(0.0001, 0.6, 1500.0, 1.0) };
                let w = 2.0 * std::f64::consts::PI * sh / 4.0;
                let (sn, cs) = w.sin_cos();
                let q = (0.1 + 49.9 * cur(res, 2000.0)).powf(root) as f64;
                // the band-pass widths add up over the stages, the notch's width is shared out over them
                let bw = (0.01 + 11.99 * cur(res, 100.0)) as f64 * if kind == 5 { root as f64 } else { n as f64 };
                let wide = sn * (std::f64::consts::LN_2 / 2.0 * bw * w / sn).sinh();
                // the lift of the shelves and the peak is shared out over the stages
                let big = (lift.max(1e-3) as f64).powf(root as f64);
                let (b, a0, a) = match kind {
                    3 => {
                        let al = sn / (2.0 * q);
                        ([(1.0 - cs) / 2.0, 1.0 - cs, (1.0 - cs) / 2.0], 1.0 + al, [-2.0 * cs, 1.0 - al])
                    }
                    6 => {
                        let al = sn / (2.0 * q);
                        ([(1.0 + cs) / 2.0, -(1.0 + cs), (1.0 + cs) / 2.0], 1.0 + al, [-2.0 * cs, 1.0 - al])
                    }
                    7 => {
                        let al = sn / (2.0 * q);
                        ([1.0 - al, -2.0 * cs, 1.0 + al], 1.0 + al, [-2.0 * cs, 1.0 - al])
                    }
                    4 => ([wide, 0.0, -wide], 1.0 + wide, [-2.0 * cs, 1.0 - wide]),
                    5 => ([1.0, -2.0 * cs, 1.0], 1.0 + wide, [-2.0 * cs, 1.0 - wide]),
                    9 => {
                        let al = sn / (2.0 * steps(&PEAK_Q, res) as f64);
                        ([1.0 + al * big, -2.0 * cs, 1.0 - al * big], 1.0 + al / big, [-2.0 * cs, 1.0 - al / big])
                    }
                    _ => {
                        // the resonance knob sets the shelf's slope, from the gentlest overshoot-free one down to a
                        // tenth of it (measured, single and stacked stages)
                        let slope = 1.35 * 3f64.powf(-(res as f64)) - 0.35;
                        let al = sn / 2.0 * ((big + 1.0 / big) * (1.0 / slope - 1.0) + 2.0).sqrt();
                        let r = 2.0 * big.sqrt() * al;
                        // low shelf; the high shelf is its mirror
                        let c = if kind == 8 { cs } else { -cs };
                        let b = [big * ((big + 1.0) - (big - 1.0) * c + r), 2.0 * big * ((big - 1.0) - (big + 1.0) * c), big * ((big + 1.0) - (big - 1.0) * c - r)];
                        let a = [(big + 1.0) + (big - 1.0) * c + r, -2.0 * ((big - 1.0) + (big + 1.0) * c), (big + 1.0) + (big - 1.0) * c - r];
                        let flip = if kind == 8 { 1.0 } else { -1.0 };
                        ([b[0], b[1] * flip, b[2]], a[0], [a[1] * flip, a[2]])
                    }
                };
                Stage::Biquad { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [a[0] / a0, a[1] / a0] }
            }
        });
    }
    out
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if p.len() < LISTED + BANKS * TABLES {
        return;
    }
    let sr = cx.sr;
    // the pad reads its maps a little short of their far end (measured: full travel lands at 0.96)
    let (pad_x, pad_y) = (p[0] * 0.96 / 1024.0, p[1] * 0.96 / 1024.0);
    // follower times: both knobs share one curve, a quarter of a second at the top
    let rate = |v: f32| {
        let t = 0.2192 * cur((v / 16384.0).clamp(0.0, 1.0), 80.0);
        if t * sr < 0.05 {
            1.0
        } else {
            1.0 - (-1.0 / (t * sr)).exp()
        }
    };
    let (rise, fall) = (rate(p[2]), rate(p[3]));
    let master = cur(p[HEAD + BANKS * PER].max(0.0) / 128.0, 10.0);
    let swing = cur(p[HEAD + BANKS * PER + 1].max(0.0) / 128.0, 10.0);
    #[derive(Clone, Copy)]
    struct Bank {
        on: bool,
        stages: [Option<Stage>; 3],
        input: f32,
        send: f32,
        shaper: bool,
        pre: f32,
        wmix: f32,
        wknob: f32,
        want: [f32; 2],
    }
    for chunk in io.chunks_mut(CHUNK) {
        let n = chunk.len();
        // the level reaches the settings one step late (measured against the reference)
        let follow = st.late;
        st.late = st.follow;
        for f in chunk.iter() {
            let top = f[0].abs().max(f[1].abs());
            st.follow += (top - st.follow) * if top > st.follow { rise } else { fall };
        }
        let mut banks = [Bank { on: false, stages: [None; 3], input: 0.0, send: 0.0, shaper: false, pre: 1.0, wmix: 0.0, wknob: 0.0, want: [0.0; 2] }; BANKS];
        for b in 0..BANKS {
            let q = &p[HEAD + b * PER..HEAD + (b + 1) * PER];
            let tables = &p[LISTED + b * TABLES..LISTED + (b + 1) * TABLES];
            if q[14] < 0.5 {
                continue;
            }
            // the eight targets: pan, volume, cutoff, resonance, low, band, high, shaper mix. Each gets an offset
            // (pan, cutoff, resonance: lines rest at the middle) or a factor (the others: lines rest at the top).
            let mut steady = [0f32; TARGETS];
            let mut led = [0f32; TARGETS];
            for a in 0..TARGETS {
                let art = &tables[WS_N + a * ART..WS_N + (a + 1) * ART];
                let len = art[0];
                let maps = &art[4 + PAT_N..];
                let (mx, my, mf) = (look(&maps[..MAP_N], pad_x), look(&maps[MAP_N..2 * MAP_N], pad_y), look(&maps[2 * MAP_N..3 * MAP_N], follow));
                let pat = if len != 0.0 {
                    let (free, len) = (len < 0.0, len.abs());
                    let (from, to, first) = (art[ART - 3], art[ART - 2], art[ART - 1]);
                    let at = st.pos[b][a];
                    let mut v = look(&art[4..4 + PAT_N], at / len);
                    // going round does not jump back to the drawn height: the first stretch of the loop
                    // starts from wherever the line had got to
                    if st.off[b][a] != 0.0 && at < first && first > from {
                        v += st.off[b][a] * (first - at) / (first - from);
                    }
                    let mut next = at + if free { n as f32 / (sr * 0.5) } else { n as f32 / (cx.step * 4.0).max(1.0) };
                    if from >= 0.0 && to > from + 1e-4 {
                        if next >= to {
                            let end = look(&art[4..4 + PAT_N], to / len) + if at < first && first > to { st.off[b][a] * (first - to) / (first - from) } else { 0.0 };
                            next = from + (next - to) % (to - from);
                            st.off[b][a] = end - look(&art[4..4 + PAT_N], from / len);
                            if st.off[b][a] == 0.0 {
                                st.off[b][a] = 1e-9;
                            }
                        }
                    } else {
                        next = next.min(len);
                    }
                    st.pos[b][a] = next;
                    Some(v)
                } else {
                    None
                };
                let wave = if art[1].abs() > 1e-4 {
                    let per_frame = if art[2] >= 0.0 { art[2] / (cx.step * 4.0).max(1.0) } else { -art[2] / (sr * 0.5) };
                    let ph = &mut st.wave[b][a];
                    let at = *ph;
                    *ph = (*ph + per_frame * n as f32).fract();
                    // a triangle from the middle upwards, each quarter bent by the tension
                    let q = at * 4.0;
                    let (u, sign) = if q < 1.0 { (q, 1.0) } else if q < 3.0 { ((2.0 - q).abs(), if q < 2.0 { 1.0 } else { -1.0 }) } else { (4.0 - q, -1.0) };
                    let t = art[3];
                    let v = if t >= 0.999 {
                        if u > 0.0 { 1.0 } else { 0.0 }
                    } else if t <= -0.999 {
                        if u >= 1.0 { 1.0 } else { 0.0 }
                    } else if t.abs() < 1e-3 {
                        u
                    } else {
                        let k = WAVE_BEND * t / (1.0 - t.abs());
                        (1.0 - (-k * u).exp()) / (1.0 - (-k).exp())
                    };
                    Some(art[1] * swing * sign * v)
                } else {
                    None
                };
                if matches!(a, 0 | 2 | 3) {
                    steady[a] = mx - 0.5 + my - 0.5;
                    led[a] = mf - 0.5 + pat.map_or(0.0, |v| v - 0.5) + wave.map_or(0.0, |w| 0.5 * w);
                } else {
                    steady[a] = mx * my * mf * pat.unwrap_or(1.0) * wave.map_or(1.0, |w| 1.0 + w);
                }
            }
            let kind = q[13].round() as i32;
            let slope = (q[12].round() as usize).min(5);
            let cut = glide(&mut st.cutk[b], q[1] / 256.0, DRIVE_RAMP, n as u32) + steady[2] + led[2] * q[0] / 128.0;
            let res = q[2] / 256.0 + steady[3] + led[3];
            let mix = [cur(q[3] / 128.0, 10.0) * steady[4], cur(q[4] / 128.0, 10.0) * steady[5], cur(q[5] / 128.0, 10.0) * steady[6]];
            let pan = q[8] / 128.0 + 2.0 * (steady[0] + led[0]);
            let vol = (cur(q[9] / 128.0, 10.0) * steady[1]).max(0.0) * master;
            let turn = (1.0 + pan.clamp(-3.0, 3.0)) * PI / 4.0;
            let send = q[11] / 128.0;
            banks[b] = Bank {
                on: true,
                stages: plan(kind, [0, 1, 2, 3, 2, 3][slope], slope > 3, cut, res, mix, 8f32.powf(2.0 * q[3] / 128.0 * steady[4] - 1.0)),
                input: cur(q[10] / 128.0, 10.0),
                send: send.signum() * cur(send.abs(), 10.0),
                shaper: q[15] >= 0.5,
                pre: q[6] / 128.0,
                wmix: steady[7],
                wknob: q[7] / 128.0,
                want: [vol * 2f32.sqrt() * turn.cos(), vol * 2f32.sqrt() * turn.sin()],
            };
            if st.gain[b][0] < -0.5 {
                st.gain[b] = banks[b].want;
                st.wmix[b] = banks[b].wmix;
            }
        }
        let ramp = 1.0 / (4 * n) as f32;
        let from = st.gain;
        let from_mix = st.wmix;
        let mut t = 0f32;
        for f in chunk.iter_mut() {
            // up to four times the rate
            let mut sub = [[0f32; 4]; 2];
            for c in 0..2 {
                let two = [chain(&mut st.up[0][c], f[c], 0), chain(&mut st.up[0][c], f[c], 1)];
                for (h, v) in two.into_iter().enumerate() {
                    sub[c][h * 2] = chain(&mut st.up[1][c], v, 0);
                    sub[c][h * 2 + 1] = chain(&mut st.up[1][c], v, 1);
                }
            }
            let mut out = [[0f32; 4]; 2];
            for k in 0..4 {
                t += ramp;
                let mut sent = [0f32; 2];
                let mut level = 0f32;
                for b in 0..BANKS {
                    let bank = &banks[b];
                    if !bank.on {
                        level = 0.0;
                        continue;
                    }
                    let wmix = from_mix[b] + (bank.wmix - from_mix[b]) * t;
                    let pre = glide(&mut st.drive[b], bank.pre, DRIVE_RAMP * 4, 1);
                    let wmix = (wmix * glide(&mut st.mixk[b], bank.wknob, DRIVE_RAMP * 4, 1)).clamp(0.0, 1.0);
                    for c in 0..2 {
                        let mut x = (sub[c][k] * bank.input + sent[c] * level) as f64;
                        for (s, stage) in bank.stages.iter().enumerate() {
                            if let Some(stage) = stage {
                                x = stage.run(&mut st.mem[b][s][c], x);
                            }
                        }
                        let mut x = x as f32;
                        if bank.shaper {
                            let d = (x * pre).clamp(-1.0, 1.0);
                            let w = look(&p[LISTED + b * TABLES..LISTED + b * TABLES + WS_N], d.abs()) * d.signum();
                            x += (w - x) * wmix;
                        }
                        sent[c] = x;
                        out[c][k] += x * (from[b][c] + (bank.want[c] - from[b][c]) * t);
                    }
                    level = bank.send;
                }
            }
            // and back down
            for c in 0..2 {
                let a = 0.5 * (chain(&mut st.down[1][c], out[c][1], 0) + chain(&mut st.down[1][c], out[c][0], 1));
                let b = 0.5 * (chain(&mut st.down[1][c], out[c][3], 0) + chain(&mut st.down[1][c], out[c][2], 1));
                f[c] = 0.5 * (chain(&mut st.down[0][c], b, 0) + chain(&mut st.down[0][c], a, 1));
            }
        }
        for b in 0..BANKS {
            if banks[b].on {
                st.gain[b] = banks[b].want;
                st.wmix[b] = banks[b].wmix;
            }
            // keep the memory finite whatever the settings were
            for mem in st.mem[b].iter_mut() {
                for z in mem.iter_mut() {
                    if !z.iter().all(|v| v.is_finite() && v.abs() < 1e6) {
                        *z = [0.0; 12];
                    }
                }
            }
        }
    }
}

/// Setting names of one bank; the unit's table lists them once per bank.
pub const NAMES: [&str; PER] = [
    "Envelope",
    "Cutoff",
    "Resonance",
    "Low",
    "Band",
    "High",
    "Shaper drive",
    "Shaper mix",
    "Pan",
    "Volume",
    "Input",
    "To next",
    "Slope",
    "Type",
    "On",
    "Shaper on",
    "Shaper two-sided",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_block_reads_back_and_filters() {
        let block = fresh();
        let p = decode(&block).expect("our own block reads back");
        assert_eq!(p.len(), LISTED + BANKS * TABLES);
        assert_eq!(encode(&p, Some(&block)), block);
        // a changed knob lands in the block and survives the trip
        let mut q = p.clone();
        q[HEAD + 1] = 200.0;
        q[HEAD + PER + 14] = 1.0;
        let again = decode(&encode(&q, Some(&block))).unwrap();
        assert_eq!(again[..LISTED], q[..LISTED]);
        // every line can be read, and a redrawn one lands where it belongs without moving the others
        for k in 0..BANKS * LINES {
            assert!(line(&block, k).is_some(), "line {k}");
        }
        let k = LINES + 1 + 2 * 14 + 12;
        let mut c = line(&block, k).unwrap();
        assert_eq!(c.pts.len(), 2);
        c.pts[1].y = 0.25;
        let edited = set_line(&block, k, &c).unwrap();
        assert_eq!(line(&edited, k).unwrap().pts[1].y, 0.25);
        assert_eq!(line(&edited, k + 1), line(&block, k + 1).map(|mut n| { n.span = line(&edited, k + 1).unwrap().span; n }));
        let r = decode(&edited).unwrap();
        assert_eq!(r[..LISTED], p[..LISTED]);
        assert_ne!(r, p);
        // the default low-pass lets a low tone through and holds a high one back
        let cx = Ctx { sr: 48000.0, step: 6000.0, clock: Default::default() };
        let level = |hz: f32| {
            let mut st = State::default();
            let mut io: Vec<[f32; 2]> = (0..9600).map(|i| [(i as f32 * hz / 48000.0 * 2.0 * PI).sin() * 0.5; 2]).collect();
            process(&p, &mut st, &mut io, &cx);
            io[4800..].iter().fold(0f32, |m, f| m.max(f[0].abs()))
        };
        assert!(level(100.0) > 0.25, "{}", level(100.0));
        assert!(level(8000.0) < 0.05, "{}", level(8000.0));
    }

    #[test]
    fn a_slow_wave_drawn_down_moves_the_level() {
        let block = fresh();
        let k = 1 + 14 + 10;
        let mut c = line(&block, k).unwrap();
        c.pts[0].y = 0.0;
        c.live = true;
        let wobbly = decode(&set_line(&block, k, &c).unwrap()).unwrap();
        let steady = decode(&block).unwrap();
        let cx = Ctx { sr: 48000.0, step: 6000.0, clock: Default::default() };
        let spread = |p: &[f32]| {
            let mut st = State::default();
            let mut io: Vec<[f32; 2]> = (0..288000).map(|i| [(i as f32 * 200.0 / 48000.0 * 2.0 * PI).sin() * 0.5; 2]).collect();
            process(p, &mut st, &mut io, &cx);
            let tops: Vec<f32> = io[4800..].chunks(480).map(|c| c.iter().fold(0f32, |m, f| m.max(f[0].abs()))).collect();
            let (lo, hi) = tops.iter().fold((f32::MAX, 0f32), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            hi / lo.max(1e-6)
        };
        assert!(spread(&steady) < 1.1);
        assert!(spread(&wobbly) > 4.0);
    }
}

