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
pub const LISTED: usize = HEAD + BANKS * PER;
/// Drawn lines in one bank: the shaper, then for each of eight targets ten patterns, a slow oscillator and
/// three mappings (pad X, pad Y, input follower).
pub const LINES: usize = 113;
const TARGETS: usize = 8;
const WS_N: usize = 257;
const PAT_N: usize = 257;
const MAP_N: usize = 65;
/// Table space of one target: pattern length in beats (0 = unused), the slow wave's depth, speed and
/// tempo switch, the pattern, three mappings.
const ART: usize = 4 + PAT_N + 3 * MAP_N;
const TABLES: usize = WS_N + TARGETS * ART;
/// Frames between two readings of the drawn lines.
const CHUNK: usize = 32;

/// Where the bank knobs sit in front of the bank's first line: 14 numbers, then three switch bytes.
const KNOBS_BACK: usize = 59;
/// Header offsets of the four settings in front.
const HEAD_OFF: [usize; HEAD] = [24, 28, 12, 16];
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
    for b in 0..BANKS {
        let first = b * LINES;
        drawn_table_n(&d, l[first].0, WS_N, &mut p)?;
        for a in 0..TARGETS {
            let k = first + 1 + a * 14;
            let (head, n, span, _) = l[k];
            p.push(if n > 1 && span > 1e-4 { span } else { 0.0 });
            // the slow wave: its first point says how deep it goes (the top is none), scaled by the unit's
            // slow-wave slider; behind the line sits its speed
            let (lh, ln, _, lend) = l[k + 10];
            let size = (lend - 20 - lh - 12) / ln;
            let y = if size == 24 { f64::from_le_bytes(d[lh + 20..lh + 28].try_into().ok()?) as f32 } else { f32::from_le_bytes(d[lh + 16..lh + 20].try_into().ok()?) };
            let level = int(4)? as f32 / 128.0;
            let flags = int(lend - 20)?;
            p.push(if flags & 1 != 0 { (1.0 - y).clamp(0.0, 1.0) * level * level.abs() } else { 0.0 });
            p.push(int(lend)? as f32);
            p.push((flags >> 1 & 1) as f32);
            drawn_table_n(&d, head, PAT_N, &mut p)?;
            for m in 11..14 {
                drawn_table_n(&d, l[k + m].0, MAP_N, &mut p)?;
            }
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
    if !crate::fm::Curve::replace_at(&mut d, head, c) {
        return None;
    }
    // the block still has to read as a whole
    lines(&d)?;
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
    /// Filter memory: bank, stage, channel, two integrators.
    svf: [[[[f32; 2]; 2]; 3]; BANKS],
    /// Output gains in use, left and right.
    gain: [[f32; 2]; BANKS],
    follow: f32,
}

impl Default for State {
    fn default() -> Self {
        Self { pos: [[0.0; TARGETS]; BANKS], wave: [[0.0; TARGETS]; BANKS], svf: [[[[0.0; 2]; 2]; 3]; BANKS], gain: [[-1.0; 2]; BANKS], follow: 0.0 }
    }
}

fn look(t: &[f32], x: f32) -> f32 {
    let f = x.clamp(0.0, 1.0) * (t.len() - 1) as f32;
    let i = (f as usize).min(t.len() - 2);
    t[i] + (t[i + 1] - t[i]) * (f - i as f32)
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if p.len() < LISTED + BANKS * TABLES {
        return;
    }
    let sr = cx.sr;
    let (pad_x, pad_y) = (p[0] / 1024.0, p[1] / 1024.0);
    let rise = 1.0 - (-(CHUNK as f32) / ((0.010 * p[2] / 4096.0).max(0.0005) * sr)).exp();
    let fall = 1.0 - (-(CHUNK as f32) / ((0.100 * p[3] / 8192.0).max(0.0005) * sr)).exp();
    let smooth = 1.0 - (-1.0 / (0.002 * sr)).exp();
    for chunk in io.chunks_mut(CHUNK) {
        let n = chunk.len();
        let top = chunk.iter().fold(0f32, |m, f| m.max(f[0].abs()).max(f[1].abs())).min(1.0);
        st.follow += (top - st.follow) * if top > st.follow { rise } else { fall };
        let mut dry = [[0f32; 2]; CHUNK];
        dry[..n].copy_from_slice(chunk);
        let mut sent = [[0f32; 2]; CHUNK];
        let mut send_level = 0f32;
        for f in chunk.iter_mut() {
            *f = [0.0; 2];
        }
        for b in 0..BANKS {
            let q = &p[HEAD + b * PER..HEAD + (b + 1) * PER];
            let tables = &p[LISTED + b * TABLES..LISTED + (b + 1) * TABLES];
            if q[14] < 0.5 {
                send_level = 0.0;
                continue;
            }
            // the eight targets: pan, volume, cutoff, resonance, low, band, high, shaper mix
            let mut drawn = [0f32; TARGETS];
            let mut steady = [0f32; TARGETS];
            for a in 0..TARGETS {
                let art = &tables[WS_N + a * ART..WS_N + (a + 1) * ART];
                let len = art[0];
                let maps = &art[4 + PAT_N..];
                let (mx, my, mf) = (look(&maps[..MAP_N], pad_x), look(&maps[MAP_N..2 * MAP_N], pad_y), look(&maps[2 * MAP_N..], st.follow));
                let pat = if len > 0.0 {
                    let v = look(&art[4..4 + PAT_N], st.pos[b][a] / len);
                    st.pos[b][a] = (st.pos[b][a] + n as f32 / (cx.step * 4.0).max(1.0)) % len;
                    Some(v)
                } else {
                    None
                };
                // the slow wave: free speeds run on an estimated scale, tempo speeds double every 4850 units
                // around one turn a beat
                let wave = if art[1].abs() > 1e-4 {
                    let per_frame = if art[3] >= 0.5 { ((art[2] - 31102.0) / 4850.0).exp2() / (cx.step * 4.0).max(1.0) } else { 0.05 * (art[2].clamp(0.0, 65536.0) / 65536.0 * 11.0).exp2() / sr };
                    let ph = &mut st.wave[b][a];
                    *ph = (*ph + per_frame * n as f32).fract();
                    Some((*ph * 2.0 * PI).sin())
                } else {
                    None
                };
                if matches!(a, 0 | 2 | 3) {
                    drawn[a] = pat.map_or(0.0, |v| v - 0.5) + mf - 0.5 + wave.map_or(0.0, |w| 0.5 * art[1] * w);
                    steady[a] = mx - 0.5 + my - 0.5;
                } else {
                    drawn[a] = pat.unwrap_or(1.0) * mf * wave.map_or(1.0, |w| 1.0 - art[1] * (0.5 - 0.5 * w));
                    steady[a] = mx * my;
                }
            }
            let depth = (q[0] - 128.0) / 128.0;
            let cut = q[1] / 256.0 + steady[2] + 2.0 * depth * drawn[2];
            let hz = (20.0 * 1000f32.powf(cut.clamp(0.0, 1.0))).min(sr * 0.45);
            let kind = q[13].round() as i32;
            let res = (q[2] / 256.0 + steady[3] + drawn[3]).clamp(0.0, 1.0);
            let quality = 0.5 * 20f32.powf(res) * if kind == 11 { 2.0 } else { 1.0 };
            let stages = [0usize, 1, 2, 3, 2, 3][(q[12].round() as usize).min(5)];
            let g = (PI * hz / sr).tan();
            let k = 1.0 / quality;
            let a1 = 1.0 / (1.0 + g * (g + k));
            let (a2, a3) = (g * a1, g * g * a1);
            let (low, band, high) = (q[3] / 128.0 * drawn[4] * steady[4], q[4] / 128.0 * drawn[5] * steady[5], q[5] / 128.0 * drawn[6] * steady[6]);
            let shaper = q[15] >= 0.5;
            let pre = ((q[6] - 128.0) / 32.0).exp2();
            let wmix = (q[7] / 128.0 * drawn[7] * steady[7]).clamp(0.0, 1.0);
            let two_sided = q[16] >= 0.5;
            let ws = &tables[..WS_N];
            let pan = (q[8] / 64.0 + 2.0 * (steady[0] + drawn[0])).clamp(-1.0, 1.0);
            let vol = (q[9] / 128.0 * drawn[1] * steady[1]).max(0.0);
            let want = [vol * (1.0 - pan).min(1.0), vol * (1.0 + pan).min(1.0)];
            if st.gain[b][0] < 0.0 {
                st.gain[b] = want;
            }
            let input = q[10] / 128.0;
            for i in 0..n {
                let mut x = [dry[i][0] * input + sent[i][0] * send_level, dry[i][1] * input + sent[i][1] * send_level];
                for (s, mem) in st.svf[b].iter_mut().enumerate().take(stages) {
                    let _ = s;
                    for c in 0..2 {
                        let z = &mut mem[c];
                        let v3 = x[c] - z[1];
                        let v1 = a1 * z[0] + a2 * v3;
                        let v2 = z[1] + a2 * z[0] + a3 * v3;
                        z[0] = 2.0 * v1 - z[0];
                        z[1] = 2.0 * v2 - z[1];
                        let (lo, bp, hi) = (v2, v1, x[c] - k * v1 - v2);
                        x[c] = match kind {
                            0 => low * lo + band * bp + high * hi,
                            2 | 6 => hi,
                            4 => k * bp,
                            5 => lo + hi,
                            7 => lo - k * bp + hi,
                            8 => x[c] + lo,
                            9 => x[c] + 3.0 * k * bp,
                            10 => x[c] + hi,
                            _ => lo,
                        };
                    }
                }
                if shaper {
                    for v in x.iter_mut() {
                        let d = *v * pre;
                        let w = if two_sided { look(ws, d * 0.5 + 0.5) * 2.0 - 1.0 } else { look(ws, d.abs()) * d.signum() };
                        *v += (w - *v) * wmix;
                    }
                }
                sent[i] = x;
                for c in 0..2 {
                    st.gain[b][c] += (want[c] - st.gain[b][c]) * smooth;
                    chunk[i][c] += x[c] * st.gain[b][c];
                }
            }
            send_level = q[11] / 128.0;
            // keep the memory finite whatever the settings were
            for mem in st.svf[b].iter_mut() {
                for z in mem.iter_mut() {
                    if !(z[0].is_finite() && z[1].is_finite()) {
                        *z = [0.0; 2];
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
        assert!(level(100.0) > 0.4, "{}", level(100.0));
        assert!(level(8000.0) < 0.05, "{}", level(8000.0));
    }

    #[test]
    fn a_slow_wave_drawn_down_moves_the_level() {
        let block = fresh();
        let k = 1 + 14 + 10;
        let mut c = line(&block, k).unwrap();
        c.pts[0].y = 0.0;
        let wobbly = decode(&set_line(&block, k, &c).unwrap()).unwrap();
        let steady = decode(&block).unwrap();
        let cx = Ctx { sr: 48000.0, step: 6000.0, clock: Default::default() };
        let spread = |p: &[f32]| {
            let mut st = State::default();
            let mut io: Vec<[f32; 2]> = (0..48000).map(|i| [(i as f32 * 200.0 / 48000.0 * 2.0 * PI).sin() * 0.5; 2]).collect();
            process(p, &mut st, &mut io, &cx);
            let tops: Vec<f32> = io[4800..].chunks(480).map(|c| c.iter().fold(0f32, |m, f| m.max(f[0].abs()))).collect();
            let (lo, hi) = tops.iter().fold((f32::MAX, 0f32), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            hi / lo.max(1e-6)
        };
        assert!(spread(&steady) < 1.1);
        assert!(spread(&wobbly) > 4.0);
    }
}

