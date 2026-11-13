//! The metallic additive instrument: a seeded spread of harmonics that rings and decays. The stored block is a
//! version number and a compressed list of settings; only the leading ones are read, at estimated scales, and the
//! drawn harmonic maps, envelopes, filter and built-in effects are not played yet. The seeds give our own spread of
//! harmonics, in the same spirit as the original's but not the same numbers.

use crate::additive::{ix, Patch};

const HARMONICS: usize = 512;

fn hash(seed: u32, k: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Which harmonics a seed lets through: all of them for no seed, otherwise about one in six at full level.
fn weight(seed: i32, k: usize) -> f32 {
    if seed <= 0 {
        1.0
    } else if hash(seed as u32, k as u32) < 0.17 {
        1.0
    } else {
        0.03
    }
}

/// The settings as stored, unpacked.
pub fn unpack(state: &[u8]) -> Option<Vec<u8>> {
    let d = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(state.get(4..)?, 1 << 24).ok()?;
    (d.len() >= 48).then_some(d)
}

pub fn patch(state: &[u8]) -> Option<Patch> {
    let d = unpack(state)?;
    let at = |off: usize| i32::from_le_bytes(d[off..off + 4].try_into().unwrap()) as f32;
    let (volume, pitch, decay, release, full, morph) = (at(0), at(4), at(24), at(28), at(32), at(36));
    let (seed1, seed2) = (at(40) as i32, at(44) as i32);
    let mut p: Vec<f32> = crate::synth::def(crate::synth::Kind::Additive).params.iter().map(|p| p.def as f32).collect();
    p[ix::FILTER_ON] = 0.0;
    p[ix::F_AMOUNT] = 0.0;
    p[ix::CUTOFF] = 8192.0;
    p[ix::VOLUME] = (volume / 128.0 * 1000.0).clamp(0.0, 2000.0);
    p[ix::PITCH] = (pitch * 100.0).clamp(-2400.0, 2400.0);
    // times are taken as milliseconds and turned into the engine's knob positions
    let dec = 1000.0 * (decay.clamp(0.0, 10_000.0) / 10_000.0).cbrt();
    p[ix::DECAY] = dec;
    p[ix::PLUCK] = dec;
    p[ix::RELEASE] = (1000.0 * (release.max(20.0) / 4000.0).sqrt()).min(1000.0);
    p[ix::TIMBRE] = 11.0;
    let full = ((full + 1000.0) / 2000.0).clamp(0.0, 1.0);
    let morph = (morph / 1000.0).clamp(0.0, 1.0);
    let other = if seed2 > 0 { seed2 } else { seed1.wrapping_add(7919).max(1) };
    let amp = (1..=HARMONICS)
        .map(|k| {
            let w = weight(seed1, k) * (1.0 - morph) + weight(other, k) * morph;
            let w = w + (1.0 - w) * full;
            // the note itself always sounds, so the pitch stays readable
            (if k == 1 { w.max(0.6) } else { w }) / (k as f32).powf(0.6)
        })
        .collect();
    let phase = (1..=HARMONICS).map(|k| hash(seed1 as u32 ^ 0x5bd1, k as u32)).collect();
    Some(Patch { p, custom: Some((amp, phase)), rise: 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(vals: &[(usize, i32)]) -> Vec<u8> {
        let mut d = vec![0u8; 400];
        for &(off, v) in vals {
            d[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        let mut s = vec![14, 0, 0, 0];
        s.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&d, 6));
        s
    }

    #[test]
    fn seeds_thin_the_harmonics_and_fullness_brings_them_back() {
        let thin = patch(&block(&[(0, 128), (24, 4000), (28, 2000), (32, -1000), (40, 25)])).unwrap();
        let (a, _) = thin.custom.as_ref().unwrap();
        let quiet = a.iter().enumerate().filter(|(k, v)| **v * ((k + 1) as f32).powf(0.6) < 0.1).count();
        assert!(quiet > 350 && quiet < 480, "{quiet}");
        let full = patch(&block(&[(0, 128), (24, 4000), (32, 1000), (40, 25)])).unwrap();
        let (a, _) = full.custom.as_ref().unwrap();
        assert!(a.iter().enumerate().all(|(k, v)| (*v * ((k + 1) as f32).powf(0.6) - 1.0).abs() < 1e-4));
        assert!(thin.p[ix::DECAY] < 1000.0 && thin.p[ix::VOLUME] == 1000.0);
        assert!(patch(&[1, 2, 3]).is_none());
    }
}
