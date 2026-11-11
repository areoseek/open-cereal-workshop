//! Band Delay: the signal is split into sixteen bands and each band gets its own delay time, level and
//! position. Eight banks of band settings are kept and one knob sweeps through them.
//!
//! Stored block: two words (version 4, 393 values), then 8 banks x (16 delay times, 16 levels, 16 positions)
//! as fixed point numbers with 2^30 for full travel, then plain whole numbers for the knobs and switches.
//! The band tables are certain. Of the knobs, the scale is certain; where the rest sit was worked out from
//! which presets move them and is marked as a guess below.

use crate::fx::Ctx;

pub const BANDS: usize = 16;
pub const BANKS: usize = 8;
/// Number of band values (three tables of sixteen per bank).
pub const TABLE: usize = BANKS * 3 * BANDS;
/// Knobs and switches kept after the tables.
pub const KNOBS: usize = 16;
const SIZE: usize = 1781;
const FULL: f32 = 1_073_741_824.0;

/// Positions in `p` after the tables.
pub const SCALE: usize = TABLE; // -6000..=6000, the middle is no delay
pub const RANGE: usize = TABLE + 1; // short range (a tenth)
pub const MORPH: usize = TABLE + 2; // 0..=6000 across the banks (guess)
pub const FEEDBACK: usize = TABLE + 3; // 0..=6000 (guess)
pub const FILTER: usize = TABLE + 4; // 0 gentle, 1 steep, 2 steepest (guess)
pub const WEIGHT: usize = TABLE + 5; // longer ring for the high bands (guess)
pub const HARMONIC: usize = TABLE + 8; // delay times read as loop pitches (guess)

pub fn decode(state: &[u8]) -> Option<Vec<f32>> {
    if state.len() < 8 + (TABLE + KNOBS) * 4 {
        return None;
    }
    let word = |i: usize| i32::from_le_bytes([state[8 + i * 4], state[9 + i * 4], state[10 + i * 4], state[11 + i * 4]]);
    let mut p: Vec<f32> = (0..TABLE).map(|i| word(i) as f32 / FULL).collect();
    p.extend((TABLE..TABLE + KNOBS).map(|i| word(i) as f32));
    Some(p)
}

/// Write the settings back. Values that were not touched keep their stored bytes.
pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let mut out = match base {
        Some(b) if b.len() >= 8 + (TABLE + KNOBS) * 4 => b.to_vec(),
        _ => {
            let mut b = vec![0u8; SIZE];
            b[0] = 4;
            b[4..8].copy_from_slice(&393i32.to_le_bytes());
            b
        }
    };
    let old = decode(&out).unwrap_or_default();
    for (i, v) in p.iter().enumerate().take(TABLE + KNOBS) {
        if old.get(i).is_some_and(|o| o == v) && base.is_some() {
            continue;
        }
        let w = if i < TABLE { (v.clamp(-1.0, 1.0) * FULL) as i32 } else { *v as i32 };
        out[8 + i * 4..12 + i * 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

pub fn fresh() -> Vec<u8> {
    let mut p = Vec::with_capacity(TABLE + KNOBS);
    for _ in 0..BANKS {
        p.extend((0..BANDS).map(|k| k as f32 / (BANDS - 1) as f32));
        p.extend([1.0; BANDS]);
        p.extend([0.5; BANDS]);
    }
    let mut knobs = [0.0; KNOBS];
    knobs[0] = 1800.0;
    knobs[3] = 1500.0;
    knobs[7] = 128.0;
    knobs[11] = 1.0;
    p.extend(knobs);
    encode(&p, None)
}

pub struct State {
    /// Low-pass states at the fifteen band edges: per channel, per edge, two two-pole stages.
    lp: [[[f32; 4]; BANDS - 1]; 2],
    lines: Vec<Vec<f32>>,
    pos: usize,
    /// Smoothed delay per band, in frames.
    time: [f32; BANDS],
    sr: f32,
}

impl Default for State {
    fn default() -> Self {
        Self { lp: [[[0.0; 4]; BANDS - 1]; 2], lines: vec![], pos: 0, time: [-1.0; BANDS], sr: 0.0 }
    }
}

/// One value of a table, morphed between the two banks around `m` (0..=1).
fn morphed(p: &[f32], table: usize, band: usize, m: f32) -> f32 {
    let x = m.clamp(0.0, 1.0) * (BANKS - 1) as f32;
    let i = (x as usize).min(BANKS - 2);
    let f = x - i as f32;
    let at = |bank: usize| p[bank * 3 * BANDS + table * BANDS + band];
    at(i) + (at(i + 1) - at(i)) * f
}

pub fn process(p: &[f32], st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if p.len() < TABLE + KNOBS {
        return;
    }
    let len = (cx.sr * 1.1) as usize + 8;
    if st.sr != cx.sr || st.lines.len() != BANDS * 2 {
        st.lines = vec![vec![0.0; len]; BANDS * 2];
        st.pos = 0;
        st.sr = cx.sr;
        st.time = [-1.0; BANDS];
    }
    let scale = (p[SCALE] / 6000.0).clamp(-1.0, 1.0);
    let range = if p[RANGE] >= 0.5 { 0.1 } else { 1.0 };
    let m = p[MORPH] / 6000.0;
    let fb = (p[FEEDBACK] / 6000.0).clamp(0.0, 1.0) * 0.98;
    let steep = p[FILTER] >= 0.5;
    let weight = p[WEIGHT] >= 0.5;
    let harmonic = p[HARMONIC] >= 0.5;
    let mut want = [0.0f32; BANDS];
    let mut gain = [[0.0f32; 2]; BANDS];
    let mut back = [0.0f32; BANDS];
    for k in 0..BANDS {
        let d = morphed(p, 0, k, m).clamp(0.0, 1.0);
        let d = if scale < 0.0 { 1.0 - d } else { d };
        let secs = if harmonic {
            // the band's loop sits up to two octaves either side of the length the scale sets
            scale.abs() * range * 0.25 * 2f32.powf(-(d - 0.5) * 4.0)
        } else {
            scale.abs() * range * d
        };
        want[k] = (secs * cx.sr).min(len as f32 - 4.0);
        let v = morphed(p, 1, k, m).clamp(0.0, 1.0);
        let pan = morphed(p, 2, k, m).clamp(0.0, 1.0);
        gain[k] = [v * (2.0 * (1.0 - pan)).min(1.0), v * (2.0 * pan).min(1.0)];
        back[k] = if weight { fb.powf(1.0 - 0.6 * k as f32 / (BANDS - 1) as f32) } else { fb };
        if st.time[k] < 0.0 {
            st.time[k] = want[k];
        }
    }
    // band edges, evenly spread over the audible range
    let mut g = [0.0f32; BANDS - 1];
    for (e, g) in g.iter_mut().enumerate() {
        let hz = 20.0 * 1000f32.powf((e + 1) as f32 / BANDS as f32);
        *g = (std::f32::consts::PI * (hz / cx.sr).min(0.49)).tan();
    }
    let k = std::f32::consts::SQRT_2;
    let glide = 1.0 - (-1.0 / (0.05 * cx.sr)).exp();
    for f in io.iter_mut() {
        for b in 0..BANDS {
            st.time[b] += (want[b] - st.time[b]) * glide;
        }
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let x = f[c];
            let mut below = 0.0;
            for b in 0..BANDS {
                // everything under the band's upper edge, less what the lower bands took
                let under = if b == BANDS - 1 {
                    x
                } else {
                    let z = &mut st.lp[c][b];
                    let lo = crate::fx::svf(&mut z[0..2], x, g[b], k).0;
                    if steep {
                        crate::fx::svf(&mut z[2..4], lo, g[b], k).0
                    } else {
                        lo
                    }
                };
                let band = under - below;
                below = under;
                let t = st.time[b];
                let line = &mut st.lines[b * 2 + c];
                let y = if t < 1.0 {
                    line[st.pos] = band;
                    band
                } else {
                    let whole = t as usize;
                    let frac = t - whole as f32;
                    let a = line[(st.pos + len - whole) % len];
                    let z = line[(st.pos + len - whole - 1) % len];
                    let y = a + (z - a) * frac;
                    line[st.pos] = band + y * back[b];
                    y
                };
                out[0] += y * gain[b][c] * if c == 0 { 1.0 } else { 0.0 };
                out[1] += y * gain[b][c] * if c == 1 { 1.0 } else { 0.0 };
            }
        }
        st.pos = (st.pos + 1) % len;
        *f = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_block_reads_back_and_keeps_its_bytes() {
        let b = fresh();
        assert_eq!(b.len(), SIZE);
        let p = decode(&b).unwrap();
        assert_eq!(p[SCALE], 1800.0);
        assert!((p[15] - 1.0).abs() < 1e-6 && (p[BANDS] - 1.0).abs() < 1e-6 && (p[2 * BANDS] - 0.5).abs() < 1e-6);
        assert_eq!(encode(&p, Some(&b)), b);
    }

    #[test]
    fn no_scale_passes_the_sound_and_scale_delays_the_top() {
        let cx = Ctx { sr: 44100.0, step: 5000.0 };
        let mut p = decode(&fresh()).unwrap();
        p[SCALE] = 0.0;
        p[FEEDBACK] = 0.0;
        let mut st = State::default();
        let mut io: Vec<[f32; 2]> = (0..4096).map(|i| [(i as f32 * 0.05).sin() * 0.5; 2]).collect();
        let dry = io.clone();
        process(&p, &mut st, &mut io, &cx);
        let worst = io.iter().zip(&dry).map(|(a, b)| (a[0] - b[0]).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-4, "{worst}");
        // full scale: a click's top end arrives a second later, its bottom at once
        p[SCALE] = 6000.0;
        let mut st = State::default();
        let mut io = vec![[0.0f32; 2]; 48000];
        io[0] = [1.0; 2];
        process(&p, &mut st, &mut io, &cx);
        let early: f32 = io[..2000].iter().map(|f| f[0].abs()).sum();
        let late: f32 = io[44000..44200].iter().map(|f| f[0].abs()).sum();
        assert!(early > 0.01 && late > 0.3, "{early} {late}");
    }
}
