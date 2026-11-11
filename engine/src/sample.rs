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

/// A decoded sample, always held as stereo frames (mono files are doubled).
#[derive(Debug, Default)]
pub struct Sample {
    pub name: String,
    pub rate: u32,
    pub mono: bool,
    pub frames: Vec<[f32; 2]>,
}

impl Sample {
    pub fn from_mono(name: &str, rate: u32, data: &[f32]) -> Self {
        Self { name: name.into(), rate, mono: true, frames: data.iter().map(|&s| [s, s]).collect() }
    }

    pub fn seconds(&self) -> f32 {
        self.frames.len() as f32 / self.rate.max(1) as f32
    }

    /// Four-point Hermite interpolation at a fractional frame position.
    #[inline]
    pub fn at(&self, pos: f64) -> [f32; 2] {
        let i = pos as usize;
        let f = &self.frames;
        let n = f.len();
        if i >= n {
            return [0.0, 0.0];
        }
        let t = (pos - i as f64) as f32;
        let p1 = f[i];
        let p0 = if i > 0 { f[i - 1] } else { p1 };
        let p2 = if i + 1 < n { f[i + 1] } else { [0.0, 0.0] };
        let p3 = if i + 2 < n { f[i + 2] } else { [0.0, 0.0] };
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let a = -0.5 * p0[c] + 1.5 * p1[c] - 1.5 * p2[c] + 0.5 * p3[c];
            let b = p0[c] - 2.5 * p1[c] + 2.0 * p2[c] - 0.5 * p3[c];
            let cc = -0.5 * p0[c] + 0.5 * p2[c];
            out[c] = ((a * t + b) * t + cc) * t + p1[c];
        }
        out
    }

    /// Decode an audio file (WAV, FLAC, Ogg Vorbis, MP3, AIFF). WAV files whose data chunk holds an Ogg Vorbis
    /// stream (format tags 0x674F to 0x6751 and 0x676F to 0x6771) are handled too.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let (bytes, ext) = match ogg_inside_wav(&bytes) {
            Some(range) => (bytes[range].to_vec(), "ogg".to_string()),
            None => (bytes, ext),
        };
        let mut s = Self::decode(bytes, &ext).map_err(|e| format!("{}: {e}", path.display()))?;
        s.name = name;
        Ok(s)
    }

    /// Decode audio held in memory; `ext` hints at the container.
    pub fn decode(bytes: Vec<u8>, ext: &str) -> Result<Self, String> {
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
        Ok(Self { name: String::new(), rate, mono, frames })
    }
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
        } else if first == "%flstudiouserdata%" {
            for r in &self.user_roots {
                tries.push((r.clone(), &parts[1..]));
            }
        } else if first.len() == 2 && first.ends_with(':') {
            // a Windows drive path: try it as an absolute path, and under each known root
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
        // last resort: any file of the same name in the search folders and factory roots
        let name = parts.last()?.to_lowercase();
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
        assert_eq!(got, path);
        assert!(lib.load_stored("%FLStudioFactoryData%\\Data\\Patches\\Packs\\Tone.wav", None).is_some());
        assert!(lib.load_stored("%FLStudioFactoryData%\\Data\\Nope.wav", None).is_none());
        assert_eq!(lib.missing.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
