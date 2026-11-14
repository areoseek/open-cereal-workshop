//! Audio samples: decoding files into memory and finding the files a project refers to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

static QUALITY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

/// The names of the ways a sample is read between its frames, cheapest first.
pub const QUALITIES: [&str; 9] = [
    "2-point linear",
    "6-point Hermite",
    "16-point sinc",
    "24-point sinc",
    "32-point sinc",
    "64-point sinc",
    "128-point sinc",
    "256-point sinc",
    "512-point sinc",
];

/// Where the list of `QUALITIES` is split under headings, and the headings.
pub const QUALITY_GROUPS: [(usize, &str); 3] = [(0, "Low quality / fastest"), (2, "Average quality"), (5, "High quality / slowest")];

/// The way of reading a rendering starts with.
pub const RENDER_QUALITY: u8 = 4;

/// Choose how samples are read between their frames (an index into `QUALITIES`), for everything that plays.
pub fn set_quality(q: u8) {
    QUALITY.store(q.min(QUALITIES.len() as u8 - 1), std::sync::atomic::Ordering::Relaxed);
}

pub fn quality() -> u8 {
    QUALITY.load(std::sync::atomic::Ordering::Relaxed)
}

/// A decoded sample, always held as stereo frames (mono files are doubled).
#[derive(Debug, Default)]
pub struct Sample {
    pub name: String,
    pub rate: u32,
    pub mono: bool,
    pub frames: Vec<[f32; 2]>,
    /// Loop stored in the file: first frame and one past the last.
    pub loop_pts: Option<(u32, u32)>,
}

impl Sample {
    pub fn from_mono(name: &str, rate: u32, data: &[f32]) -> Self {
        Self { name: name.into(), rate, mono: true, frames: data.iter().map(|&s| [s, s]).collect(), loop_pts: None }
    }

    pub fn seconds(&self) -> f32 {
        self.frames.len() as f32 / self.rate.max(1) as f32
    }

    /// The sound at a fractional frame position, read between the frames the way `set_quality` chose
    /// (six-point Hermite unless told otherwise).
    #[inline]
    pub fn at(&self, pos: f64) -> [f32; 2] {
        self.at_with(pos, QUALITY.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// The same, with the way of reading (an index into `QUALITIES`) given.
    #[inline]
    pub fn at_with(&self, pos: f64, quality: u8) -> [f32; 2] {
        self.at_step_with(pos, 1.0, quality)
    }

    /// The sound at a fractional place while the read point moves `step` frames each output frame: the sinc ways
    /// of reading lower their cutoff when the sample is read faster than it was recorded, so nothing folds back.
    #[inline]
    pub fn at_step(&self, pos: f64, step: f64) -> [f32; 2] {
        self.at_step_with(pos, step, QUALITY.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// A windowed sinc read over `half` frames each side. The window is 0.34 + 0.5 cos + 0.16 cos 2, the cutoff
    /// drops to 1 / step when the read moves faster than one frame at a time.
    fn sinc_read(&self, pos: f64, step: f64, half: usize) -> [f32; 2] {
        use std::f64::consts::PI;
        let f = &self.frames;
        let n = f.len() as isize;
        let i = pos.floor();
        let t = pos - i;
        let i = i as isize;
        let c = (1.0 / step.abs().max(1e-9)).min(1.0);
        if t < 1e-9 && c >= 1.0 {
            return if i >= 0 && i < n { f[i as usize] } else { [0.0, 0.0] };
        }
        let h = half as isize;
        // x runs from the first frame used to the last, measured from the read point
        let x0 = -(h - 1) as f64 - t;
        let (mut ss, mut sc) = (PI * c * x0).sin_cos();
        let (rs, rc) = (PI * c).sin_cos();
        let (mut ws, mut wc) = (PI * x0 / half as f64).sin_cos();
        let (qs, qc) = (PI / half as f64).sin_cos();
        let mut out = [0.0f64; 2];
        for j in 0..2 * h {
            let x = x0 + j as f64;
            let at = i - (h - 1) + j;
            if at >= 0 && at < n {
                let w = 0.34 + 0.5 * wc + 0.16 * (2.0 * wc * wc - 1.0);
                let k = if x.abs() < 1e-7 { c } else { ss / (PI * x) };
                let s = f[at as usize];
                out[0] += s[0] as f64 * k * w;
                out[1] += s[1] as f64 * k * w;
            }
            let (a, b) = (ss * rc + sc * rs, sc * rc - ss * rs);
            ss = a;
            sc = b;
            let (a, b) = (ws * qc + wc * qs, wc * qc - ws * qs);
            ws = a;
            wc = b;
        }
        [out[0] as f32, out[1] as f32]
    }

    /// This sample brought to another sample rate, the way a channel with Resample on holds it: a 128-point
    /// windowed sinc read. Results are kept while the source is alive.
    pub fn to_rate(self: &Arc<Sample>, rate: u32) -> Arc<Sample> {
        if self.rate == rate || self.rate == 0 || rate == 0 || self.frames.is_empty() {
            return self.clone();
        }
        type Kept = Vec<(std::sync::Weak<Sample>, u32, Arc<Sample>)>;
        static KEPT: std::sync::Mutex<Kept> = std::sync::Mutex::new(Vec::new());
        {
            let mut kept = KEPT.lock().unwrap();
            kept.retain(|(w, _, _)| w.strong_count() > 0);
            if let Some((_, _, s)) = kept.iter().find(|(w, r, _)| *r == rate && std::ptr::eq(w.as_ptr(), Arc::as_ptr(self))) {
                return s.clone();
            }
        }
        const HALF: isize = 64;
        fn gcd(a: u64, b: u64) -> u64 {
            if b == 0 { a } else { gcd(b, a % b) }
        }
        let g = gcd(self.rate as u64, rate as u64);
        let (up, down) = (self.rate as u64 / g, rate as u64 / g);
        let n_in = self.frames.len();
        let n_out = ((n_in as u64 * down + up / 2) / up) as usize;
        // going down in rate, the cutoff drops to the new rate's half
        let cut = (down as f64 / up as f64).min(1.0);
        let weights = |t: f64| -> Vec<f32> {
            use std::f64::consts::PI;
            (0..2 * HALF)
                .map(|j| {
                    let x = (j - (HALF - 1)) as f64 - t;
                    let s = if x.abs() < 1e-9 { cut } else { (PI * cut * x).sin() / (PI * x) };
                    let a = PI * x / HALF as f64;
                    (s * (0.34 + 0.5 * a.cos() + 0.16 * (2.0 * a).cos())) as f32
                })
                .collect()
        };
        let table: Option<Vec<Vec<f32>>> = (down <= 2048).then(|| (0..down).map(|p| weights(p as f64 / down as f64)).collect());
        let f = &self.frames;
        let mut frames = Vec::with_capacity(n_out);
        for i in 0..n_out as u64 {
            let (whole, part) = ((i * up / down) as isize, i * up % down);
            if part == 0 && cut >= 1.0 {
                frames.push(f.get(whole as usize).copied().unwrap_or([0.0, 0.0]));
                continue;
            }
            let own;
            let w: &[f32] = match &table {
                Some(t) => &t[part as usize],
                None => {
                    own = weights(part as f64 / down as f64);
                    &own
                }
            };
            let first = whole - (HALF - 1);
            // (measured: past either end the edge frame carries on, so a sample that starts or stops away
            // from zero does not ring)
            let mut out = [0.0f32; 2];
            for at in first..first + 2 * HALF {
                let (s, k) = (f[at.clamp(0, n_in as isize - 1) as usize], w[(at - first) as usize]);
                out[0] += s[0] * k;
                out[1] += s[1] * k;
            }
            frames.push(out);
        }
        let scale = |v: u32| ((v as u64 * down + up / 2) / up) as u32;
        let made = Arc::new(Sample { name: self.name.clone(), rate, mono: self.mono, frames, loop_pts: self.loop_pts.map(|(a, b)| (scale(a), scale(b))) });
        KEPT.lock().unwrap().push((Arc::downgrade(self), rate, made.clone()));
        made
    }

    /// The same, with the way of reading (an index into `QUALITIES`) given.
    #[inline]
    pub fn at_step_with(&self, pos: f64, step: f64, quality: u8) -> [f32; 2] {
        let i = pos as usize;
        let f = &self.frames;
        let n = f.len();
        if i >= n {
            return [0.0, 0.0];
        }
        let t = (pos - i as f64) as f32;
        let p1 = f[i];
        match quality {
            1 => {}
            0 => {
                let p2 = if i + 1 < n { f[i + 1] } else { [0.0, 0.0] };
                return [p1[0] + (p2[0] - p1[0]) * t, p1[1] + (p2[1] - p1[1]) * t];
            }
            q => return self.sinc_read(pos, step, [8, 12, 16, 32, 64, 128, 256][(q as usize - 2).min(6)]),
        }
        // six-point, fifth-order Hermite
        let g = |k: isize| -> [f32; 2] {
            let at = i as isize + k;
            if at < 0 { f[0] } else if (at as usize) < n { f[at as usize] } else { [0.0, 0.0] }
        };
        let (m2, m1, p2, p3, p4) = (g(-2), g(-1), g(1), g(2), g(3));
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let (ym2, ym1, y0, y1, y2, y3) = (m2[c], m1[c], p1[c], p2[c], p3[c], p4[c]);
            let c1 = (ym2 - y2) / 12.0 + (y1 - ym1) * (2.0 / 3.0);
            let c2 = ym1 * (13.0 / 12.0) - y0 * (25.0 / 12.0) + y1 * 1.5 - y2 * (11.0 / 24.0) + y3 / 12.0 - ym2 / 8.0;
            let c3 = y0 * (5.0 / 12.0) - y1 * (7.0 / 12.0) + y2 * (7.0 / 24.0) - (ym2 + ym1 + y3) / 24.0;
            let c4 = ym2 / 8.0 - ym1 * (7.0 / 12.0) + y0 * (13.0 / 12.0) - y1 + y2 * (11.0 / 24.0) - y3 / 12.0;
            let c5 = (y3 - ym2) / 24.0 + (ym1 - y2) * (5.0 / 24.0) + (y1 - y0) * (5.0 / 12.0);
            out[c] = ((((c5 * t + c4) * t + c3) * t + c2) * t + c1) * t + y0;
        }
        out
    }

    /// Decode an audio file (WAV, FLAC, Ogg Vorbis, MP3, AIFF, WavPack). WAV files whose data chunk holds an Ogg Vorbis
    /// stream (format tags 0x674F to 0x6751 and 0x676F to 0x6771) are handled too.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        // a drum patch kept as text is turned into its wave here
        if let Some(wave) = crate::dsfile::render(&bytes) {
            return Ok(Self { name, ..Self::from_mono("", 44_100, &wave) });
        }
        // a speech preset is spoken here
        if let Some(v) = crate::speech::parse(&bytes) {
            return Ok(Self { name, ..Self::from_mono("", 44_100, &crate::speech::render(&v, 44_100)) });
        }
        // a packed wave says how many frames it really holds; the packed stream rounds that up
        let mut true_len = None;
        let (bytes, ext) = match ogg_inside_wav(&bytes) {
            Some(range) => {
                true_len = wav_fact(&bytes);
                (first_ogg_stream(&bytes[range]), "ogg".to_string())
            }
            None => (bytes, ext),
        };
        let odd = odd_wav(&bytes);
        let (bytes, ext) = match odd {
            Some(Odd::Plain(s)) => {
                return Ok(Self { name, loop_pts: wave_loop(&bytes).map(|(a, b)| (a as u32, b as u32 + 1)), ..s });
            }
            Some(Odd::Mp3(range)) => (bytes[range].to_vec(), "mp3".to_string()),
            None => (bytes, ext),
        };
        let lp = if bytes.starts_with(b"RIFF") { wave_loop(&bytes) } else { None };
        let mut s = Self::decode(bytes, &ext).map_err(|e| format!("{}: {e}", path.display()))?;
        s.name = name;
        if let Some(n) = true_len.filter(|&n| n > 0 && n < s.frames.len()) {
            s.frames.truncate(n);
        }
        s.loop_pts = lp.map(|(a, b)| (a as u32, b as u32 + 1)).filter(|l| l.1 > l.0 && l.1 as usize <= s.frames.len());
        Ok(s)
    }

    /// Decode audio held in memory; `ext` hints at the container.
    pub fn decode(bytes: Vec<u8>, ext: &str) -> Result<Self, String> {
        if crate::wavpack::is_wavpack(&bytes) {
            let d = crate::wavpack::decode(&bytes)?;
            let k = 1.0 / (1u64 << (8 * d.bytes - 1)) as f32;
            let val = |s: i32| if d.float { f32::from_bits(s as u32) } else { s as f32 * k };
            let frames: Vec<[f32; 2]> = if d.channels == 1 {
                d.samples.iter().map(|&s| [val(s); 2]).collect()
            } else {
                d.samples.chunks_exact(2).map(|f| [val(f[0]), val(f[1])]).collect()
            };
            return Ok(Self { name: String::new(), rate: d.rate, mono: d.channels == 1, frames, loop_pts: None });
        }
        let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        if !ext.is_empty() {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| e.to_string())?;
        let mut format = probed.format;
        let track = format.default_track().ok_or("no audio track")?;
        let track_id = track.id;
        let mut rate = track.codec_params.sample_rate.unwrap_or(44_100);
        let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| e.to_string())?;
        let mut frames: Vec<[f32; 2]> = Vec::new();
        let mut mono = true;
        let mut buf: Option<SampleBuffer<f32>> = None;
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(_) => break,
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    rate = spec.rate;
                    let ch = spec.channels.count().max(1);
                    let need = decoded.capacity() * ch;
                    if buf.as_ref().map(|b| b.capacity() < need).unwrap_or(true) {
                        buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                    }
                    let sb = buf.as_mut().unwrap();
                    sb.copy_interleaved_ref(decoded);
                    if ch == 1 {
                        frames.extend(sb.samples().iter().map(|&s| [s, s]));
                    } else {
                        mono = false;
                        frames.extend(sb.samples().chunks_exact(ch).map(|f| [f[0], f[1]]));
                    }
                }
                Err(SymError::DecodeError(_)) => continue,
                Err(_) => break,
            }
        }
        if frames.is_empty() {
            return Err("no audio decoded".into());
        }
        Ok(Self { name: String::new(), rate, mono, frames, loop_pts: None })
    }
}

/// The frame count a WAV states in its `fact` chunk.
fn wav_fact(bytes: &[u8]) -> Option<usize> {
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        if &bytes[i..i + 4] == b"fact" && size >= 4 {
            return Some(u32::from_le_bytes(bytes.get(i + 8..i + 12)?.try_into().ok()?) as usize);
        }
        i = i + 8 + size + (size & 1);
    }
    None
}

/// If `bytes` is a WAV whose data chunk is really an Ogg stream, give the range of that stream.
fn ogg_inside_wav(bytes: &[u8]) -> Option<std::ops::Range<usize>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    let mut tag = 0u16;
    while i + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        let body = i + 8;
        let end = (body + size).min(bytes.len());
        match &bytes[i..i + 4] {
            b"fmt " if size >= 2 && body + 2 <= bytes.len() => tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]),
            b"data" => {
                let vorbis_tag = matches!(tag, 0x674F..=0x6751 | 0x676F..=0x6771);
                return (vorbis_tag && bytes[body..end].starts_with(b"OggS")).then_some(body..end);
            }
            _ => {}
        }
        i = body + size + (size & 1);
    }
    None
}

/// The pages of the first stream in a run of Ogg pages, up to its last page (some writers mix in empty
/// pages of another stream, which a reader takes for a broken chain).
fn first_ogg_stream(d: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let mut p = 0;
    let serial = d.get(14..18).map(|s| s.to_vec()).unwrap_or_default();
    while p + 27 <= d.len() && &d[p..p + 4] == b"OggS" {
        let segs = d[p + 26] as usize;
        let Some(lace) = d.get(p + 27..p + 27 + segs) else { break };
        let next = p + 27 + segs + lace.iter().map(|&l| l as usize).sum::<usize>();
        if next > d.len() {
            break;
        }
        if d[p + 14..p + 18] == serial[..] {
            out.extend_from_slice(&d[p..next]);
            if d[p + 5] & 4 != 0 {
                break;
            }
        }
        p = next;
    }
    if out.is_empty() { d.to_vec() } else { out }
}

enum Odd {
    Plain(Sample),
    Mp3(std::ops::Range<usize>),
}

/// WAV files the general reader turns away: plain or float samples behind a format chunk of an unusual
/// length, and MP3 data in a WAV wrapper.
fn odd_wav(bytes: &[u8]) -> Option<Odd> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    let mut fmt: Option<&[u8]> = None;
    while i + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        let body = i + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match &bytes[i..i + 4] {
            b"fmt " => fmt = Some(&bytes[body..end]),
            b"data" => {
                let f = fmt?;
                if f.len() < 16 {
                    return None;
                }
                let tag = u16::from_le_bytes([f[0], f[1]]);
                if tag == 0x55 {
                    return Some(Odd::Mp3(body..end));
                }
                if !matches!(tag, 1 | 3) || matches!(f.len(), 16 | 18 | 40) {
                    return None;
                }
                let ch = u16::from_le_bytes([f[2], f[3]]) as usize;
                let rate = u32::from_le_bytes(f[4..8].try_into().ok()?);
                let bits = u16::from_le_bytes([f[14], f[15]]) as usize;
                let wide = bits / 8;
                if ch == 0 || rate == 0 || !matches!((tag, bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64)) {
                    return None;
                }
                let one = |b: &[u8]| -> f32 {
                    match (tag, bits) {
                        (1, 8) => (b[0] as f32 - 128.0) / 128.0,
                        (1, 16) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
                        (1, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
                        (1, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
                        (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        _ => f64::from_le_bytes(b[..8].try_into().unwrap()) as f32,
                    }
                };
                let frames: Vec<[f32; 2]> = bytes[body..end]
                    .chunks_exact(wide * ch)
                    .map(|f| {
                        let l = one(&f[..wide]);
                        let r = if ch > 1 { one(&f[wide..2 * wide]) } else { l };
                        [l, r]
                    })
                    .collect();
                if frames.is_empty() {
                    return None;
                }
                return Some(Odd::Plain(Sample { name: String::new(), rate, mono: ch == 1, frames, loop_pts: None }));
            }
            _ => {}
        }
        i = body + size + (size & 1);
    }
    None
}

/// Finds and caches the sample files a project names. Project files store Windows paths, often starting with a
/// placeholder for the original program's data folder; the library maps those onto real folders.
#[derive(Default)]
pub struct Library {
    /// Folders that stand in for the factory data placeholder (each should contain `Data/Patches`).
    pub factory_roots: Vec<PathBuf>,
    /// Folders that stand in for the user data placeholder.
    pub user_roots: Vec<PathBuf>,
    /// Extra folders searched by file name when the stored path leads nowhere.
    pub search_dirs: Vec<PathBuf>,
    cache: HashMap<PathBuf, Arc<Sample>>,
    by_name: Option<HashMap<String, PathBuf>>,
    pub missing: Vec<String>,
    /// Every file a stored path has led to, in the order first asked for.
    pub used: Vec<PathBuf>,
}

fn split_parts(stored: &str) -> Vec<&str> {
    stored.split(['\\', '/']).filter(|p| !p.is_empty()).collect()
}

/// Walk `parts` under `base`, matching each part without regard to letter case (the paths were written on a
/// case-insensitive file system).
fn walk_ci(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut cur = base.to_path_buf();
    for part in parts {
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let want = part.to_lowercase();
        let found = std::fs::read_dir(&cur).ok()?.flatten().find(|e| e.file_name().to_string_lossy().to_lowercase() == want)?;
        cur = found.path();
    }
    Some(cur)
}

impl Library {
    pub fn new() -> Self {
        let mut lib = Self::default();
        // CEREAL_FACTORY_DATA / CEREAL_SAMPLE_DIRS let a user point at folders they already own.
        if let Some(v) = std::env::var_os("CEREAL_FACTORY_DATA") {
            lib.factory_roots.extend(std::env::split_paths(&v));
        }
        if let Some(v) = std::env::var_os("CEREAL_SAMPLE_DIRS") {
            lib.search_dirs.extend(std::env::split_paths(&v));
        }
        lib
    }

    /// Turn a stored path into a file on this machine, if one can be found.
    pub fn resolve(&mut self, stored: &str, project_dir: Option<&Path>) -> Option<PathBuf> {
        let found = self.resolve_quiet(stored, project_dir)?;
        if !self.used.contains(&found) {
            self.used.push(found.clone());
        }
        Some(found)
    }

    fn resolve_quiet(&mut self, stored: &str, project_dir: Option<&Path>) -> Option<PathBuf> {
        if stored.is_empty() {
            return None;
        }
        let parts = split_parts(stored);
        let first = parts.first()?.to_lowercase();
        let mut tries: Vec<(PathBuf, &[&str])> = vec![];
        if first == "%flstudiofactorydata%" {
            for r in &self.factory_roots {
                tries.push((r.clone(), &parts[1..]));
            }
        } else if first == "%flstudiodata%" {
            // the early versions' name for the Data folder of the installation
            for r in &self.factory_roots {
                tries.push((r.join("Data"), &parts[1..]));
                tries.push((r.clone(), &parts[1..]));
            }
        } else if first == "%flstudiouserdata%" {
            for r in &self.user_roots {
                tries.push((r.clone(), &parts[1..]));
            }
        } else if first.len() == 2 && first.ends_with(':') {
            // a Windows drive path: try it as an absolute path, and under each known root
            #[cfg(windows)]
            tries.push((PathBuf::from(format!("{}\\", parts[0])), &parts[1..]));
            tries.push((PathBuf::from("/"), &parts[1..]));
        } else if stored.starts_with('/') {
            tries.push((PathBuf::from("/"), &parts[..]));
        }
        if let Some(d) = project_dir {
            tries.push((d.to_path_buf(), &parts[parts.len() - 1..]));
            tries.push((d.to_path_buf(), &parts[..]));
        }
        for (base, rest) in tries {
            if let Some(p) = walk_ci(&base, rest) {
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        let name = parts.last()?.to_lowercase();
        // a file of the same name anywhere under the project's folder (a zipped project keeps its files
        // in folders of its own)
        if let Some(d) = project_dir {
            let mut stack = vec![(d.to_path_buf(), 0)];
            while let Some((dir, depth)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        if depth < 4 {
                            stack.push((p, depth + 1));
                        }
                    } else if e.file_name().to_string_lossy().to_lowercase() == name {
                        return Some(p);
                    }
                }
            }
        }
        // last resort: any file of the same name in the search folders and factory roots
        if self.by_name.is_none() {
            let mut map = HashMap::new();
            let mut stack: Vec<PathBuf> = self.search_dirs.iter().chain(&self.factory_roots).chain(&self.user_roots).cloned().collect();
            while let Some(dir) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else {
                        map.entry(e.file_name().to_string_lossy().to_lowercase()).or_insert(p);
                    }
                }
            }
            self.by_name = Some(map);
        }
        self.by_name.as_ref().and_then(|m| m.get(&name).cloned())
    }

    /// Forget the file-name index (after the folders change).
    pub fn rescan(&mut self) {
        self.by_name = None;
    }

    pub fn load_path(&mut self, path: &Path) -> Result<Arc<Sample>, String> {
        if let Some(s) = self.cache.get(path) {
            return Ok(s.clone());
        }
        let s = Arc::new(Sample::load(path)?);
        self.cache.insert(path.to_path_buf(), s.clone());
        Ok(s)
    }

    /// Resolve and load; a miss is recorded in `missing` and gives None.
    pub fn load_stored(&mut self, stored: &str, project_dir: Option<&Path>) -> Option<Arc<Sample>> {
        if stored.is_empty() {
            return None;
        }
        match self.resolve(stored, project_dir) {
            Some(p) => match self.load_path(&p) {
                Ok(s) => Some(s),
                Err(e) => {
                    self.missing.push(format!("{stored} ({e})"));
                    None
                }
            },
            None => {
                self.missing.push(stored.to_string());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(serial: u32, flags: u8, body: &[u8]) -> Vec<u8> {
        let mut p = b"OggS".to_vec();
        p.push(0);
        p.push(flags);
        p.extend_from_slice(&[0u8; 8]);
        p.extend_from_slice(&serial.to_le_bytes());
        p.extend_from_slice(&[0u8; 8]);
        p.push(1);
        p.push(body.len() as u8);
        p.extend_from_slice(body);
        p
    }

    #[test]
    fn stray_pages_of_another_stream_are_dropped() {
        let (a, b, c) = (page(7, 2, b"one"), page(u32::MAX, 0, b""), page(7, 4, b"two"));
        let all = [a.clone(), b.clone(), c.clone(), b].concat();
        assert_eq!(first_ogg_stream(&all), [a, c].concat());
    }

    #[test]
    fn a_wav_with_a_long_format_chunk_still_reads() {
        let mut w = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        w.extend_from_slice(&20u32.to_le_bytes());
        for v in [1u16, 1] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        w.extend_from_slice(&22050u32.to_le_bytes());
        w.extend_from_slice(&44100u32.to_le_bytes());
        for v in [2u16, 16, 0, 0] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        w.extend_from_slice(b"data");
        w.extend_from_slice(&4u32.to_le_bytes());
        w.extend_from_slice(&16384i16.to_le_bytes());
        w.extend_from_slice(&(-16384i16).to_le_bytes());
        let Some(Odd::Plain(s)) = odd_wav(&w) else { panic!("not read") };
        assert_eq!((s.rate, s.mono, s.frames.len()), (22050, true, 2));
        assert!((s.frames[0][0] - 0.5).abs() < 1e-3 && (s.frames[1][0] + 0.5).abs() < 1e-3);
    }


    #[test]
    fn interpolation_hits_the_frames_and_blends_between() {
        let s = Sample::from_mono("t", 44_100, &[0.0, 1.0, 0.0, -1.0]);
        assert_eq!(s.at(1.0), [1.0, 1.0]);
        let mid = s.at(0.5)[0];
        assert!(mid > 0.3 && mid < 0.8, "{mid}");
        assert_eq!(s.at(10.0), [0.0, 0.0]);
    }

    #[test]
    fn wav_written_by_us_decodes_back() {
        let dir = std::env::temp_dir().join(format!("cereal-sample-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Data/Patches/Packs")).unwrap();
        let path = dir.join("Data/Patches/Packs/Tone.wav");
        let data: Vec<f32> = (0..4410).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        crate::wav::write_pcm16(std::fs::File::create(&path).unwrap(), &data, 44_100, 1).unwrap();
        let s = Sample::load(&path).unwrap();
        assert_eq!(s.rate, 44_100);
        assert_eq!(s.frames.len(), 4410);
        assert!((s.frames[100][0] - data[100]).abs() < 1e-3);

        // and the library finds it through a stored Windows path, whatever the letter case
        let mut lib = Library::default();
        lib.factory_roots.push(dir.clone());
        let got = lib.resolve("%FLStudioFactoryData%\\data\\PATCHES\\Packs\\tone.WAV", None).unwrap();
        // (a file system that ignores letter case hands the path back as it was asked for)
        let flat = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
        assert!(got.is_file());
        assert_eq!(flat(&got), flat(&path));
        #[cfg(not(windows))]
        assert_eq!(got, path);
        assert!(lib.load_stored("%FLStudioFactoryData%\\Data\\Patches\\Packs\\Tone.wav", None).is_some());
        assert!(lib.load_stored("%FLStudioFactoryData%\\Data\\Nope.wav", None).is_none());
        assert_eq!(lib.missing.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod quality_tests {
    use super::*;

    #[test]
    fn sinc_reads_a_sine_between_frames_better_than_a_straight_line() {
        // a tone a fifth of the way to the top of the band, read at odd places between the frames
        let hz = 0.1f64;
        let data: Vec<f32> = (0..2000).map(|i| (i as f64 * hz * std::f64::consts::TAU).sin() as f32).collect();
        let s = Sample::from_mono("t", 44_100, &data);
        let mut err = [0.0f64; 4];
        for (q, e) in err.iter_mut().enumerate() {
            for k in 0..500 {
                let pos = 200.0 + k as f64 * 1.37;
                let want = (pos * hz * std::f64::consts::TAU).sin();
                *e = e.max((s.at_with(pos, q as u8)[0] as f64 - want).abs());
            }
        }
        assert!(err[0] > err[1] && err[1] > err[2] && err[2] > err[3], "{err:?}");
        assert!(err[3] < 1e-4, "{err:?}");
    }
}

/// The first loop of a wave file's sampler chunk: start frame and last frame (inclusive).
pub fn wave_loop(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        if &bytes[at..at + 4] == b"smpl" && len >= 60 && at + 8 + 60 <= bytes.len() {
            let b = at + 8;
            let int = |o: usize| u32::from_le_bytes(bytes[b + o..b + o + 4].try_into().unwrap()) as usize;
            if int(28) >= 1 {
                return Some((int(44), int(48)));
            }
        }
        at += 8 + len + (len & 1);
    }
    None
}
