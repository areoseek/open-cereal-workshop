//! A minimal RIFF/WAVE writer: 16-bit PCM, no dependencies. Enough for project export; more formats later.

use std::io::{self, Write};

/// Write interleaved f32 samples in [-1, 1] as a 16-bit PCM WAV. `channels` is 1 or 2.
pub fn write_pcm16<W: Write>(mut w: W, samples: &[f32], sample_rate: u32, channels: u16) -> io::Result<()> {
    let bits = 16u16;
    let block_align = channels * bits / 8;
    let byte_rate = sample_rate * block_align as u32;
    let data_len = (samples.len() * 2) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?; // PCM fmt chunk size
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    w.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_well_formed() {
        let mut out = vec![];
        write_pcm16(&mut out, &[0.0, 1.0, -1.0, 0.5], 48_000, 1).unwrap();
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WAVE");
        assert_eq!(&out[36..40], b"data");
        let data_len = u32::from_le_bytes(out[40..44].try_into().unwrap());
        assert_eq!(data_len, 8); // 4 samples * 2 bytes
        assert_eq!(out.len(), 44 + 8);
        // 1.0 -> 32767, -1.0 clamps to -32767
        assert_eq!(i16::from_le_bytes(out[46..48].try_into().unwrap()), 32767);
        assert_eq!(i16::from_le_bytes(out[48..50].try_into().unwrap()), -32767);
    }
}
