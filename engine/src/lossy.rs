//! MP3 and Ogg Vorbis export.
//!
//! The coding itself is done by two pure-Rust crates (see docs/THIRD-PARTY.md); this file feeds them the mix
//! and, for Vorbis, wraps the packets they return into Ogg pages.

/// Encode interleaved stereo samples as a constant bit rate MP3 file.
pub fn mp3(samples: &[f32], rate: u32, kbps: u32) -> Result<Vec<u8>, String> {
    use rusty_mp3::{Error, Mp3Encoder, Mp3EncoderConfig};
    let mut enc = Mp3Encoder::new(Mp3EncoderConfig { bitrate_kbps: kbps, vbr_quality: None });
    let mut out = Vec::with_capacity(samples.len() / 8);
    let drain = |enc: &mut Mp3Encoder, out: &mut Vec<u8>| -> Result<(), String> {
        loop {
            match enc.next_packet() {
                Ok(p) => out.extend_from_slice(&p),
                Err(Error::Again) | Err(Error::Eof) => return Ok(()),
                Err(e) => return Err(format!("{e:?}")),
            }
        }
    };
    // silence still makes a file
    let silent = [0.0f32; 2];
    for chunk in (if samples.is_empty() { &silent[..] } else { samples }).chunks(2 * 1152 * 64) {
        enc.push_pcm_f32(chunk, 2, rate).map_err(|e| format!("{e:?}"))?;
        drain(&mut enc, &mut out)?;
    }
    enc.finish();
    drain(&mut enc, &mut out)?;
    // the first frame is the tag that tells players the true length; it is complete only now
    if let Some(tag) = enc.lame_tag_frame() {
        if tag.len() <= out.len() {
            out[..tag.len()].copy_from_slice(tag);
        }
    }
    Ok(out)
}

/// The checksum Ogg pages carry: CRC-32 with polynomial 0x04c11db7, no reflection, zero start.
fn ogg_crc(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04c1_1db7 } else { r << 1 };
            }
            *slot = r;
        }
        t
    });
    data.iter().fold(0u32, |c, &b| (c << 8) ^ t[((c >> 24) as u8 ^ b) as usize])
}

/// Builds the pages of one Ogg stream.
struct OggWriter {
    out: Vec<u8>,
    serial: u32,
    seq: u32,
    /// The packets waiting for the next page: lacing values and bytes.
    lacing: Vec<u8>,
    body: Vec<u8>,
    /// Position after the last packet that ends on the waiting page.
    granule: i64,
    /// The waiting page starts in the middle of a packet.
    continued: bool,
    started: bool,
}

impl OggWriter {
    fn flush(&mut self, last: bool) {
        if self.lacing.is_empty() && !last {
            return;
        }
        let mut page = Vec::with_capacity(27 + self.lacing.len() + self.body.len());
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(self.continued as u8 | if self.started { 0 } else { 2 } | if last { 4 } else { 0 });
        page.extend_from_slice(&self.granule.to_le_bytes());
        page.extend_from_slice(&self.serial.to_le_bytes());
        page.extend_from_slice(&self.seq.to_le_bytes());
        page.extend_from_slice(&[0; 4]);
        page.push(self.lacing.len() as u8);
        page.extend_from_slice(&self.lacing);
        page.extend_from_slice(&self.body);
        let crc = ogg_crc(&page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        self.out.extend_from_slice(&page);
        self.seq += 1;
        self.started = true;
        self.continued = false;
        self.lacing.clear();
        self.body.clear();
    }

    /// Add one packet; `granule` is the stream position once it has been decoded.
    fn packet(&mut self, data: &[u8], granule: i64) {
        let mut rest = data;
        loop {
            let n = rest.len().min(255);
            if self.lacing.len() == 255 {
                // the page is full in the middle of this packet: nothing ends on it unless an earlier one did
                let keep = self.granule;
                self.flush(false);
                self.continued = true;
                self.granule = keep;
            }
            self.lacing.push(n as u8);
            self.body.extend_from_slice(&rest[..n]);
            rest = &rest[n..];
            if n < 255 {
                break;
            }
        }
        self.granule = granule;
    }
}

/// Encode interleaved stereo samples as an Ogg Vorbis file. `quality` is the usual -1..=10 scale.
pub fn ogg(samples: &[f32], rate: u32, quality: f32) -> Result<Vec<u8>, String> {
    use rusty_vorbis::{quality01_from_vorbis_q, Error, VorbisEncoder, VorbisEncoderConfig};
    let mut enc = VorbisEncoder::new(VorbisEncoderConfig { quality: quality01_from_vorbis_q(quality as f64), ..Default::default() });
    let silent = [0.0f32; 2];
    enc.push_pcm_f32(if samples.is_empty() { &silent[..] } else { samples }, 2, rate).map_err(|e| format!("{e:?}"))?;
    enc.finish();
    // any number will do for the stream's serial; make it differ from file to file
    let serial = samples.len() as u32 ^ rate.rotate_left(13) ^ 0x4252_4b46;
    let mut w = OggWriter { out: Vec::with_capacity(samples.len() / 6), serial, seq: 0, lacing: vec![], body: vec![], granule: 0, continued: false, started: false };
    let mut headers = 0;
    let mut since = 0i64;
    loop {
        match enc.next_packet() {
            Ok(p) => {
                if headers < 3 {
                    // the first header has a page to itself; the other two share pages and audio starts afresh
                    w.packet(&p.data, 0);
                    headers += 1;
                    if headers != 2 {
                        w.flush(false);
                    }
                    continue;
                }
                w.packet(&p.data, p.pts);
                since += p.duration;
                // about a tenth of a second to a page
                if since >= rate as i64 / 10 || w.body.len() > 4096 {
                    w.flush(false);
                    since = 0;
                }
            }
            Err(Error::Eof) => break,
            Err(e) => return Err(format!("{e:?}")),
        }
    }
    w.flush(true);
    Ok(w.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).flat_map(|i| {
            let t = i as f32 / rate as f32;
            let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5 * (1.0 - t / secs);
            [v, v * 0.5]
        }).collect()
    }

    /// What we wrote reads back as the same sound: same length within a frame or two, and the same energy.
    fn check(bytes: Vec<u8>, ext: &str, src: &[f32], rate: u32) {
        let s = crate::sample::Sample::decode(bytes, ext).expect("our own file reads back");
        assert_eq!(s.rate, rate);
        let want = src.len() / 2;
        assert!((s.frames.len() as i64 - want as i64).abs() < 3000, "{} frames for {want}", s.frames.len());
        let e = |it: &mut dyn Iterator<Item = f32>| it.map(|v| v * v).sum::<f32>();
        let a = e(&mut src.chunks(2).map(|f| f[0]));
        let b = e(&mut s.frames.iter().map(|f| f[0]));
        assert!((a / b).ln().abs() < 0.1, "energy {a} became {b}");
        let ar = e(&mut src.chunks(2).map(|f| f[1]));
        let br = e(&mut s.frames.iter().map(|f| f[1]));
        assert!((ar / br).ln().abs() < 0.1, "right energy {ar} became {br}");
    }

    #[test]
    fn mp3_reads_back() {
        let src = tone(44_100, 1.5);
        check(mp3(&src, 44_100, 192).unwrap(), "mp3", &src, 44_100);
    }

    #[test]
    fn ogg_reads_back() {
        let src = tone(44_100, 1.5);
        let bytes = ogg(&src, 44_100, 6.0).unwrap();
        assert_eq!(&bytes[..4], b"OggS");
        check(bytes, "ogg", &src, 44_100);
    }
}
