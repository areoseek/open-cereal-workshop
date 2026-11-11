//! Recording sound from the default input device while the song plays.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct AudioRec {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    channels: usize,
    /// Play position when the recording began.
    pub at: u32,
}

impl AudioRec {
    pub fn start(at: u32) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("no audio input device")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let channels = config.channels() as usize;
        let rate = config.sample_rate().0;
        let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
        let err = |e| eprintln!("audio input error: {e}");
        let b = buf.clone();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |d: &[f32], _| {
                    if let Ok(mut v) = b.lock() {
                        v.extend_from_slice(d);
                    }
                },
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |d: &[i16], _| {
                    if let Ok(mut v) = b.lock() {
                        v.extend(d.iter().map(|&s| s as f32 / 32768.0));
                    }
                },
                err,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_input_stream(
                &config.into(),
                move |d: &[i32], _| {
                    if let Ok(mut v) = b.lock() {
                        v.extend(d.iter().map(|&s| s as f32 / 2_147_483_648.0));
                    }
                },
                err,
                None,
            ),
            other => return Err(format!("unsupported input format {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, buf, rate, channels, at })
    }

    /// Stop and write what was heard to a WAV file in `dir`. Nothing is written for silence shorter than a blink.
    pub fn finish(self, dir: &Path) -> Result<PathBuf, String> {
        let Self { _stream, buf, rate, channels, .. } = self;
        drop(_stream);
        let raw = buf.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
        let stereo = to_stereo(&raw, channels);
        if stereo.len() < rate as usize / 10 {
            return Err("nothing was recorded".into());
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = (0..1000).map(|i| dir.join(if i == 0 { format!("Rec_{stamp}.wav") } else { format!("Rec_{stamp}_{i}.wav") })).find(|p| !p.exists()).ok_or("no free file name")?;
        let f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        cereal_engine::wav::write(std::io::BufWriter::new(f), &stereo, rate, 2, 24).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

/// Any channel count down to interleaved stereo: one channel is doubled, more than two keep the first pair.
fn to_stereo(raw: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        0 => Vec::new(),
        1 => raw.iter().flat_map(|&s| [s, s]).collect(),
        2 => raw.to_vec(),
        n => raw.chunks_exact(n).flat_map(|f| [f[0], f[1]]).collect(),
    }
}

/// Where recordings go: beside the project when it has a home, else a folder of the user's own.
pub fn folder(project: Option<&Path>) -> PathBuf {
    if let Some(dir) = project.and_then(|p| p.parent()) {
        return dir.join("Recorded");
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Documents").join("Breakfast Cereal").join("Recorded")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_of_any_width_come_out_stereo() {
        assert_eq!(to_stereo(&[0.1, 0.2], 1), vec![0.1, 0.1, 0.2, 0.2]);
        assert_eq!(to_stereo(&[0.1, 0.2, 0.3, 0.4], 2), vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(to_stereo(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8], 4), vec![0.1, 0.2, 0.5, 0.6]);
        assert_eq!(folder(Some(Path::new("/a/b/song.flp"))), PathBuf::from("/a/b/Recorded"));
    }
}
