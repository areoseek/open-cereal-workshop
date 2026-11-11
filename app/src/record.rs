//! Recording sound from the default input device while the song plays.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use cereal_engine::Transport;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Open the default input device and hand everything it hears to `sink` as interleaved floats.
fn open_input(mut sink: impl FnMut(&[f32], &cpal::InputCallbackInfo) + Send + 'static) -> Result<(cpal::Stream, u32, usize), String> {
    let device = input_device().ok_or("no audio input device")?;
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let channels = config.channels() as usize;
    let rate = config.sample_rate().0;
    let err = |e| eprintln!("audio input error: {e}");
    let mut conv: Vec<f32> = Vec::new();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(&config.into(), move |d: &[f32], i: &cpal::InputCallbackInfo| sink(d, i), err, None),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |d: &[i16], i: &cpal::InputCallbackInfo| {
                conv.clear();
                conv.extend(d.iter().map(|&s| s as f32 / 32768.0));
                sink(&conv, i)
            },
            err,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            &config.into(),
            move |d: &[i32], i: &cpal::InputCallbackInfo| {
                conv.clear();
                conv.extend(d.iter().map(|&s| s as f32 / 2_147_483_648.0));
                sink(&conv, i)
            },
            err,
            None,
        ),
        other => return Err(format!("unsupported input format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate, channels))
}

/// The input device chosen in the settings, by name; empty = the system's default.
static IN_NAME: Mutex<String> = Mutex::new(String::new());
static IN_CHANNELS: Mutex<Option<usize>> = Mutex::new(None);

pub fn set_input(name: &str) {
    if let Ok(mut g) = IN_NAME.lock() {
        *g = name.to_string();
    }
    if let Ok(mut g) = IN_CHANNELS.lock() {
        *g = None;
    }
}

fn input_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    let want = IN_NAME.lock().map(|g| g.clone()).unwrap_or_default();
    if !want.is_empty() {
        if let Some(d) = host.input_devices().ok().and_then(|mut l| l.find(|d| d.name().map(|n| n == want).unwrap_or(false))) {
            return Some(d);
        }
    }
    host.default_input_device()
}

/// How many inputs the input device has; asked once per choice of device.
pub fn input_channels() -> usize {
    let Ok(mut g) = IN_CHANNELS.lock() else { return 0 };
    *g.get_or_insert_with(|| input_device().and_then(|d| d.default_input_config().ok()).map(|c| c.channels() as usize).unwrap_or(0))
}

/// The names in the mixer's input list: stereo pairs first, then every input on its own.
pub fn input_names(channels: usize) -> Vec<String> {
    let pairs = (0..channels.div_ceil(2)).map(|p| if p * 2 + 1 < channels { format!("In {} - In {}", p * 2 + 1, p * 2 + 2) } else { format!("In {}", p * 2 + 1) });
    pairs.chain((0..channels).map(|c| format!("In {} (mono)", c + 1))).collect()
}

/// Bring interleaved sound from one rate to another by drawing straight lines between frames.
pub struct Resample {
    step: f64,
    at: f64,
    last: Vec<f32>,
}

impl Resample {
    pub fn new(from: u32, to: u32, channels: usize) -> Self {
        Self { step: from as f64 / to.max(1) as f64, at: 0.0, last: vec![0.0; channels] }
    }
    pub fn run(&mut self, data: &[f32], out: &mut Vec<f32>) {
        let ch = self.last.len();
        out.clear();
        if ch == 0 {
            return;
        }
        let n = data.len() / ch;
        // position 0 is the frame kept from last time, position 1 the first new one
        while self.at < n as f64 {
            let i = self.at.floor() as usize;
            let f = (self.at - i as f64) as f32;
            for c in 0..ch {
                let a = if i == 0 { self.last[c] } else { data[(i - 1) * ch + c] };
                let b = data[i * ch + c];
                out.push(a + (b - a) * f);
            }
            self.at += self.step;
        }
        self.at -= n as f64;
        if n > 0 {
            self.last.copy_from_slice(&data[(n - 1) * ch..n * ch]);
        }
    }
}

/// The sound card inputs kept open and fed to the mixer while any track listens to one.
pub struct LiveIn {
    _stream: cpal::Stream,
}

impl LiveIn {
    pub fn start(tr: Arc<Transport>, engine_rate: u32) -> Result<Self, String> {
        let channels = input_channels();
        let mut rs: Option<Resample> = None;
        let mut out: Vec<f32> = Vec::new();
        let mut rate = 0u32;
        let shared = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let seen = shared.clone();
        let (stream, r, ch) = open_input(move |d, _| {
            if rate == 0 {
                rate = seen.load(Relaxed);
                if rate != 0 && rate != engine_rate {
                    rs = Some(Resample::new(rate, engine_rate, channels));
                }
            }
            match &mut rs {
                Some(r) => {
                    r.run(d, &mut out);
                    tr.feed_input(&out, channels);
                }
                None => tr.feed_input(d, channels),
            }
        })?;
        if ch != channels {
            return Err("the input device changed".into());
        }
        shared.store(r, Relaxed);
        Ok(Self { _stream: stream })
    }
}

pub struct AudioRec {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    channels: usize,
    /// Play position when the recording was asked for.
    pub at: u32,
    /// Play position when the first sound arrived (bits of an f64, all ones before then), and how long that
    /// sound had been on its way in, in millionths of a second.
    first: Arc<AtomicU64>,
    lag_in: Arc<AtomicU64>,
}

impl AudioRec {
    pub fn start(at: u32, tr: Arc<Transport>) -> Result<Self, String> {
        let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
        let first = Arc::new(AtomicU64::new(u64::MAX));
        let lag_in = Arc::new(AtomicU64::new(0));
        let (b, f, l) = (buf.clone(), first.clone(), lag_in.clone());
        let (stream, rate, channels) = open_input(move |d, info| {
            if f.load(Relaxed) == u64::MAX {
                let t = info.timestamp();
                l.store(t.callback.duration_since(&t.capture).map(|d| d.as_micros() as u64).unwrap_or(0), Relaxed);
                f.store(tr.tick().to_bits(), Relaxed);
            }
            if let Ok(mut v) = b.lock() {
                v.extend_from_slice(d);
            }
        })?;
        Ok(Self { _stream: stream, buf, rate, channels, at, first, lag_in })
    }

    /// Where the take belongs and how much of its head to drop, once the time sound spends on its way out to the
    /// speakers and back in is taken off. `out_lag` is the way out in seconds, `tps` ticks a second.
    pub fn placed(&self, song_mode: bool, out_lag: f64, tps: f64) -> (u32, f64) {
        let first = self.first.load(Relaxed);
        if !song_mode || first == u64::MAX {
            return (self.at, 0.0);
        }
        place(f64::from_bits(first), self.lag_in.load(Relaxed) as f64 / 1e6 + out_lag, tps)
    }

    /// Stop and write what was heard to a WAV file in `dir`. Nothing is written for silence shorter than a blink.
    pub fn finish(self, dir: &Path, trim_secs: f64) -> Result<PathBuf, String> {
        let Self { _stream, buf, rate, channels, .. } = self;
        drop(_stream);
        let raw = buf.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
        let mut stereo = to_stereo(&raw, channels);
        let cut = ((trim_secs.max(0.0) * rate as f64) as usize * 2).min(stereo.len());
        stereo.drain(..cut);
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

/// A take whose first sound arrived at tick `first` after `lag` seconds on the way: the tick it starts at, and the
/// seconds to drop from its head when that would fall before the start of the song.
fn place(first: f64, lag: f64, tps: f64) -> (u32, f64) {
    let at = first - lag * tps;
    if at >= 0.0 {
        (at.round() as u32, 0.0)
    } else {
        (0, -at / tps.max(1e-9))
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

    #[test]
    fn a_take_is_moved_back_by_the_time_sound_was_on_its_way() {
        // 192 ticks a second: 50 ms is 9.6 ticks
        assert_eq!(place(960.0, 0.05, 192.0), (950, 0.0));
        let (at, cut) = place(4.8, 0.05, 192.0);
        assert_eq!(at, 0);
        assert!((cut - 0.025).abs() < 1e-9);
    }

    #[test]
    fn rates_are_matched_and_inputs_named() {
        let mut r = Resample::new(24_000, 48_000, 1);
        let mut out = vec![];
        r.run(&[1.0, 1.0, 1.0, 1.0], &mut out);
        assert_eq!(out.len(), 8);
        r.run(&[1.0; 100], &mut out);
        assert_eq!(out.len(), 200);
        assert!(out.iter().all(|v| (v - 1.0).abs() < 1e-6));
        let mut r = Resample::new(48_000, 44_100, 2);
        let mut total = 0;
        for _ in 0..100 {
            r.run(&[0.5; 960], &mut out);
            total += out.len() / 2;
        }
        assert!((total as i64 - 44_100).abs() <= 1, "{total}");
        assert_eq!(input_names(3), vec!["In 1 - In 2", "In 3", "In 1 (mono)", "In 2 (mono)", "In 3 (mono)"]);
        assert!(input_names(0).is_empty());
    }
}
