//! The sampled keyboard instrument. It plays sample sets described by `.wti` text files (an ini layout that is
//! documented beside the sets themselves): zones of notes, each with a root key, cut from one raw file or from
//! separate wave files. The sets are the user's own files; when none is found a built tone set stands in.

use crate::sample::Sample;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Setting numbers, as the set files number them.
pub mod ix {
    pub const DECAY: usize = 0;
    pub const RELEASE: usize = 1;
    pub const HARDNESS: usize = 2;
    pub const VEL_HARDNESS: usize = 3;
    pub const MUFFLE: usize = 4;
    pub const VEL_MUFFLE: usize = 5;
    pub const VEL_SENS: usize = 6;
    pub const STEREO: usize = 7;
    pub const TUNE: usize = 8;
    pub const DETUNE: usize = 9;
    pub const STRETCH: usize = 10;
    pub const TREBLE: usize = 11;
    pub const PAN: usize = 12;
    pub const LFO: usize = 13;
    pub const DRIVE: usize = 14;
}

/// The stored range of each setting: (lowest, highest). A setting runs 0..1 across it.
pub const RANGES: [(i32, i32); 15] = [
    (-1000, 1000),
    (-1000, 1000),
    (-1000, 1000),
    (-1000, 1000),
    (0, 1000),
    (0, 2000),
    (-1000, 1000),
    (0, 2000),
    (-1000, 1000),
    (0, 500),
    (-500, 500),
    (-1000, 1000),
    (-1000, 1000),
    (0, 65535),
    (0, 1000),
];

#[derive(Debug)]
pub struct Zone {
    pub root: i32,
    /// Cycles per second of the recording.
    pub hz: f64,
    pub high: i32,
    pub sample: Arc<Sample>,
    pub start: usize,
    pub end: usize,
    /// Frames from the end that repeat while the key is held; 0 plays once.
    pub loop_len: usize,
}

#[derive(Debug)]
pub struct KeySet {
    pub name: String,
    pub rate: f32,
    pub vel_factor: f32,
    pub release_high: bool,
    /// The lowest velocity of each layer; zones come in runs of this many.
    pub layers: Vec<i32>,
    pub zones: Vec<Zone>,
}

struct Store {
    roots: Vec<PathBuf>,
    sets: HashMap<String, Arc<KeySet>>,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| {
        let mut roots = vec![];
        if let Some(v) = std::env::var_os("CEREAL_FACTORY_DATA") {
            roots.extend(std::env::split_paths(&v));
        }
        Mutex::new(Store { roots, sets: HashMap::new() })
    })
}

/// Tell the instrument about a folder of the user's own content (an install root).
/// The user's content folders known so far.
pub fn roots() -> Vec<PathBuf> {
    store().lock().unwrap().roots.clone()
}

pub fn add_root(root: &Path) {
    let mut s = store().lock().unwrap();
    if !s.roots.iter().any(|r| r == root) {
        s.roots.push(root.to_path_buf());
        // sets that fell back to the built tones get another look
        s.sets.clear();
    }
}

pub(crate) fn child_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    let want = name.to_lowercase();
    std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().to_lowercase() == want).map(|e| e.path())
}

fn set_dirs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = vec![];
    for r in roots {
        let mut cur = Some(r.clone());
        for part in ["Plugins", "Fruity", "Generators", "FL Keys", "Wavtables"] {
            cur = cur.and_then(|c| child_ci(&c, part));
        }
        out.extend(cur);
    }
    out
}

fn key_hz(key: i32) -> f64 {
    440.0 * 2f64.powf((key as f64 - 69.0) / 12.0)
}

/// The sections of an ini text: lower-case section name to its key/value lines.
fn ini(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut sec = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sec = name.trim().to_lowercase();
        } else if let Some((k, v)) = line.split_once('=') {
            out.entry(sec.clone()).or_default().insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    out
}

/// The repeat points a wave file carries, as (start, end) frames.
fn read_set(wti: &Path) -> Option<KeySet> {
    let text = String::from_utf8_lossy(&std::fs::read(wti).ok()?).into_owned();
    let secs = ini(&text);
    let info = secs.get("info")?;
    let get = |k: &str| info.get(k).map(|s| s.as_str()).unwrap_or("");
    let flag = |k: &str, d: bool| match get(k).to_lowercase().as_str() {
        "true" | "1" => true,
        "false" | "0" => false,
        _ => d,
    };
    let count: usize = get("samples").parse().ok()?;
    let rate: f32 = get("samplerate").parse().unwrap_or(44100.0);
    let dir = wti.parent()?;
    let base = if get("path").is_empty() { dir.to_path_buf() } else { child_ci(dir, get("path"))? };
    let raw = get("filetype").eq_ignore_ascii_case("raw");
    let can_loop = flag("canloop", true);
    let whole = if raw {
        // one file of 16-bit frames, cut by the zones
        let bytes = std::fs::read(child_ci(&base, get("samplefile"))?).ok()?;
        let data: Vec<f32> = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        Some(Arc::new(Sample::from_mono(get("name"), rate as u32, &data)))
    } else {
        None
    };
    let mut layers = vec![];
    if let Some(v) = secs.get("velocitylayers") {
        let n: usize = v.get("count").and_then(|c| c.parse().ok()).unwrap_or(1);
        for k in 0..n {
            layers.push(v.get(&k.to_string()).and_then(|c| c.parse().ok()).unwrap_or(0));
        }
    }
    if layers.is_empty() {
        layers.push(0);
    }
    let mut zones = vec![];
    let mut last_name = String::new();
    let mut last: Option<(Arc<Sample>, Option<(usize, usize)>)> = None;
    for k in 0..count {
        let z = secs.get(&format!("sample{k}"))?;
        let num = |key: &str| z.get(key).and_then(|v| v.parse::<i64>().ok());
        // the further layers of a key group leave the keys out and take the group's
        let prev = zones.last().map(|z: &Zone| (z.root, z.high));
        let root = match (num("root"), prev) {
            (Some(r), _) => r as i32,
            (None, Some((r, _))) => r,
            _ => return None,
        };
        let high = match (num("high"), num("root"), prev) {
            (Some(h), _, _) => h as i32,
            (None, None, Some((_, h))) => h,
            _ => 999,
        };
        let loops = can_loop && z.get("canloop").map(|v| !v.eq_ignore_ascii_case("false")).unwrap_or(true);
        if let Some(w) = &whole {
            let start = num("pos").unwrap_or(0).max(0) as usize;
            let end = (num("end").unwrap_or(0).max(0) as usize).min(w.frames.len().saturating_sub(1));
            if end <= start {
                return None;
            }
            let loop_len = if loops { (num("loop").unwrap_or(0).max(0) as usize).min(end - start) } else { 0 };
            zones.push(Zone { root, hz: key_hz(root), high, sample: w.clone(), start, end, loop_len });
        } else {
            let name = z.get("name").cloned().filter(|n| !n.is_empty()).unwrap_or_else(|| last_name.clone());
            if name != last_name || last.is_none() {
                let path = child_ci(&base, &name)?;
                let bytes = std::fs::read(&path).ok()?;
                let lp = crate::sample::wave_loop(&bytes);
                let s = Sample::load(&path).ok()?;
                last = Some((Arc::new(s), lp));
                last_name = name;
            }
            let (s, lp) = last.clone()?;
            let mut end = s.frames.len().saturating_sub(1);
            let mut loop_len = 0;
            if let (true, Some((a, b))) = (loops, lp) {
                if b > a && b < s.frames.len() {
                    // the last frame of the repeat belongs to it (measured)
                    end = b + 1;
                    loop_len = b - a + 1;
                }
            }
            zones.push(Zone { root, hz: key_hz(root), high, sample: s, start: 0, end, loop_len });
        }
    }
    if zones.is_empty() {
        return None;
    }
    let name = if get("name").is_empty() { wti.file_stem()?.to_string_lossy().into_owned() } else { get("name").to_string() };
    Some(KeySet { name, rate, vel_factor: get("velocityfactor").parse().unwrap_or(1.0), release_high: flag("releasehighnotes", false), layers, zones })
}

/// Find the set a stored name means among the user's folders.
fn find(roots: &[PathBuf], want: &str) -> Option<KeySet> {
    let want = want.trim().to_lowercase();
    let mut fallback = None;
    for dir in set_dirs(roots) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("wti")).unwrap_or(false)).collect();
        files.sort();
        for f in files {
            let stem = f.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if stem == "default" {
                continue;
            }
            let text = String::from_utf8_lossy(&std::fs::read(&f).unwrap_or_default()).into_owned();
            let secs = ini(&text);
            let names: Vec<String> = secs.get("info").map(|i| ["name", "shortname"].iter().filter_map(|k| i.get(*k)).map(|s| s.to_lowercase()).collect()).unwrap_or_default();
            // stored names are the set's name, its short name, or its name with a note in brackets
            let hit = stem == want || names.iter().any(|n| *n == want) || want.split('(').next().map(|w| w.trim() == stem).unwrap_or(false);
            if hit {
                if let Some(s) = read_set(&f) {
                    return Some(s);
                }
            }
            if names.iter().any(|n| n == "piano") && fallback.is_none() {
                fallback = Some(f.clone());
            }
        }
    }
    fallback.and_then(|f| read_set(&f))
}

/// A set of plain tones, for when the user has no set files: a struck-string colour, or a softer bar colour for
/// names that ask for an electric piano, or steady pipes for an organ.
fn built(name: &str) -> KeySet {
    let lower = name.to_lowercase();
    let kind = if lower.contains("organ") { 2 } else if lower.contains("rhodes") || lower.contains("epiano") { 1 } else { 0 };
    let rate = 22050.0f32;
    let mut data: Vec<f32> = vec![];
    let mut cuts = vec![];
    for root in (24..=96).step_by(6) {
        let hz = 440.0 * 2f32.powf((root as f32 - 69.0) / 12.0);
        // a whole number of frames per cycle, so the end repeats cleanly; the voice retunes for the difference
        let period = (rate / hz).round().max(2.0);
        let cycles = (rate * 0.9 / period).ceil() as usize;
        let n = cycles * period as usize;
        let start = data.len();
        let top = ((rate * 0.45 / (rate / period)) as usize).clamp(1, 24);
        for i in 0..n {
            let t = i as f32 / rate;
            let ph = i as f32 / period * std::f32::consts::TAU;
            let mut x = 0.0;
            for h in 1..=top {
                let hf = h as f32;
                let a = match kind {
                    2 => if [1, 2, 3, 4, 6, 8].contains(&h) { 0.5 / hf.sqrt() } else { 0.0 },
                    1 => (1.0 / (hf * hf)) * (1.0 + 3.0 * (-t * 9.0 * hf).exp()) * if h == 1 { 1.0 } else { 0.6 },
                    _ => (1.0 / hf.powf(1.4)) * (0.35 + (-t * 2.5 * hf).exp()),
                };
                x += a * (ph * hf).sin();
            }
            data.push(x * 0.45);
        }
        let loop_len = (period as usize * 8).min(n / 2);
        cuts.push((root, start, data.len() - 1, loop_len, hz, rate / period));
    }
    let whole = Arc::new(Sample::from_mono("built", rate as u32, &data));
    let zones = cuts.iter().map(|&(root, start, end, loop_len, _, hz)| Zone { root, hz: hz as f64, high: root + 5, sample: whole.clone(), start, end, loop_len }).collect::<Vec<_>>();
    let mut set = KeySet { name: name.to_string(), rate, vel_factor: 1.0, release_high: kind != 0, layers: vec![0], zones };
    if let Some(z) = set.zones.last_mut() {
        z.high = 999;
    }
    set
}

/// The set a stored name means. Reads files the first time; later calls share the loaded set.
pub fn set(name: &str) -> Arc<KeySet> {
    let key = name.to_lowercase();
    let mut s = store().lock().unwrap();
    if let Some(k) = s.sets.get(&key) {
        return k.clone();
    }
    let k = Arc::new(find(&s.roots, name).unwrap_or_else(|| built(name)));
    s.sets.insert(key, k.clone());
    k
}

/// The set name a stored block asks for.
/// The sets on hand, by the name a project stores for them.
pub fn set_list() -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for dir in set_dirs(&roots()) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for f in rd.flatten().map(|e| e.path()) {
            let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if !f.extension().map(|e| e.eq_ignore_ascii_case("wti")).unwrap_or(false) || stem.eq_ignore_ascii_case("default") {
                continue;
            }
            let text = String::from_utf8_lossy(&std::fs::read(&f).unwrap_or_default()).into_owned();
            let name = ini(&text).get("info").and_then(|i| i.get("name")).filter(|n| !n.is_empty()).cloned().unwrap_or(stem);
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
                out.push(name);
            }
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// The settings a set asks for when it is chosen: (setting, value from 0 to 1).
pub fn first_values(name: &str) -> Vec<(usize, f32)> {
    let want = name.trim().to_lowercase();
    for dir in set_dirs(&roots()) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for f in rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("wti")).unwrap_or(false)) {
            let text = String::from_utf8_lossy(&std::fs::read(&f).unwrap_or_default()).into_owned();
            let secs = ini(&text);
            if secs.get("info").and_then(|i| i.get("name")).map(|n| n.to_lowercase()) != Some(want.clone()) {
                continue;
            }
            let mut out: Vec<(usize, f32)> = secs
                .get("parameters")
                .map(|p| p.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.parse::<f32>().ok()?.clamp(0.0, 1.0)))).filter(|e: &(usize, f32)| e.0 < RANGES.len()).collect())
                .unwrap_or_default();
            out.sort_by_key(|e| e.0);
            return out;
        }
    }
    vec![]
}

/// Write another set's name into a stored block; the settings behind the name move with it.
pub fn rename(raw: &mut Vec<u8>, name: &str) {
    let name = &name.as_bytes()[..name.len().min(255)];
    let old = 5 + raw.get(4).copied().unwrap_or(0) as usize;
    let mut out = raw[..4.min(raw.len())].to_vec();
    out.resize(4, 0);
    out.push(name.len() as u8);
    out.extend_from_slice(name);
    out.extend_from_slice(raw.get(old..).unwrap_or(&[]));
    *raw = out;
}

pub fn name_in(raw: &[u8]) -> String {
    let n = raw.get(4).copied().unwrap_or(0) as usize;
    raw.get(5..5 + n).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default()
}

/// Frames the widening echo waits.
const WIDE: usize = 127;

#[derive(Debug, Clone)]
pub struct Voice {
    set: Arc<KeySet>,
    zone: usize,
    pos: f64,
    /// Frames of the sample per cycle-per-frame of the note.
    per: f64,
    env: f32,
    dec: f32,
    rel: f32,
    f0: f32,
    f1: f32,
    /// The same pair for the right side of a set recorded in stereo.
    g: [f32; 2],
    ff: f32,
    out: [f32; 2],
    /// How fast the level falls each second, held and let go (in powers of e), and how many such
    /// powers the note has before it counts as over.
    fade: [f64; 3],
    held: bool,
    over: bool,
    /// The slow swing: two numbers turning about each other, and the turn per frame.
    lfo: [f32; 2],
    dlfo: f32,
    /// Turns a second of the slow swing.
    swing: f32,
    /// How far the swing moves the left and the right level.
    sway: [f32; 2],
    drive: f32,
    /// How far under the middle the bent wave may reach, as a share of the note's level.
    low: f32,
    treble: f32,
    tz: [f32; 2],
    tf: f32,
    /// The widening echo: the last frames of both sides summed, and how much of them comes back.
    wide: [f32; WIDE + 1],
    at: usize,
    depth: f32,
}

impl Voice {
    /// `u` holds the fifteen settings, each 0..1; `vel` is the note's velocity with 1.0 the usual 100.
    /// (Measured against renders of the original: level, touch, stereo and swing laws.)
    pub fn start(set: &Arc<KeySet>, u: &[f32], sr: f32, key: u8, vel: f32, _dphase: f32) -> Self {
        let note = key as i32;
        // the level follows the host's velocity curve; hardness, layers and muffle take the plain velocity
        let level = 0.5515 * crate::law::velocity_gain(vel);
        let velocity = (vel * 100.0).clamp(0.0, 127.0);
        let k = (note - 60) * (note - 60);
        // tuning in semitones: fine, a fixed scatter per key, and the stretch of high notes
        let mut l = (u[ix::TUNE] - 0.5) * 2.0 + 0.077 * u[ix::DETUNE] * u[ix::DETUNE] * ((k % 13) as f32 - 6.5);
        if note > 60 {
            l += 0.000434 * (u[ix::STRETCH] - 0.5) * k as f32;
        }
        // hardness moves which zone a key takes: harder picks a lower zone and plays it faster
        // (measured: each part rounds to a whole key on its own)
        let mut shift = (12.0 * u[ix::HARDNESS] - 6.0).round() as i32;
        if velocity > 40.0 {
            shift += (0.12 * u[ix::VEL_HARDNESS] * (velocity - 40.0)).round() as i32;
        }
        let per_group = set.layers.len().max(1);
        let groups = set.zones.len() / per_group;
        let mut g = 0;
        while g + 1 < groups && note > set.zones[g * per_group].high + shift {
            g += 1;
        }
        let layer = set.layers.iter().rposition(|&low| velocity as i32 > low).unwrap_or(0); // a layer starts just above its mark (measured)
        let zone = (g * per_group + layer).min(set.zones.len() - 1);
        let z = &set.zones[zone];
        let per = z.sample.rate as f64 / z.hz * 2f64.powf(l as f64 / 12.0);
        let velsens = {
            let s = u[ix::VEL_SENS];
            let mut v = 1.0 + s + s;
            if s < 0.25 {
                v -= 0.75 - 3.0 * s;
            }
            v
        };
        // the wider the stereo, the lower the level
        let depth = u[ix::STEREO] * u[ix::STEREO];
        let trim = 1.5 - 0.79 * depth;
        let env = LEVEL * trim * level.powf(velsens) * set.vel_factor.max(0.1);
        let muffvel = u[ix::VEL_MUFFLE] * u[ix::VEL_MUFFLE] * 5.0;
        let mut m = 50.0 + u[ix::MUFFLE] * u[ix::MUFFLE] * 160.0 + muffvel * (velocity - 64.0);
        // the muffle never closes further than a floor that rises with the key (measured)
        m = m.max((2495.0 + 63.7 * note as f32).max(0.0).sqrt()).min(210.0);
        let ff = (m * m / sr).min(1.0);
        let n = note.clamp(12, 108) as f32;
        let width = (0.04 * u[ix::STEREO]).min(0.03);
        let right = 1.0 + width * (n - 60.0);
        let d = u[ix::DECAY];
        let dec = if d <= 0.0 {
            1.0
        } else {
            let mut l = 2.0 * d;
            if l < 1.0 {
                l += 0.25 - 0.5 * d;
            }
            (-(1.0 / sr) * (-0.6 + 0.033 * n.max(44.0) - l).exp()).exp()
        };
        // let go, the keys under 94 of a set that leaves its high notes alone fade slowly; the high
        // keys, and every key of a set that lets them go too, fade fast (measured)
        let rel = if note < 94 && !set.release_high {
            (-(1.0 / sr) * (2.0 + 0.017 * n - 2.0 * u[ix::RELEASE]).exp()).exp()
        } else {
            (-(1.0 / sr) * (6.0 + 0.01 * note as f32 - 5.0 * u[ix::RELEASE]).exp()).exp()
        };
        // left of centre the two sides swing against each other, right of it together
        let lean = u[ix::PAN] * 2.0 - 1.0;
        let treble = u[ix::TREBLE];
        Self {
            set: set.clone(),
            zone,
            pos: z.start as f64,
            per,
            env,
            dec,
            rel,
            f0: 0.0,
            f1: 0.0,
            g: [0.0; 2],
            ff,
            out: [2.0 - right, right],
            fade: [-(dec as f64).ln() * sr as f64, -(rel as f64).ln() * sr as f64, (set.vel_factor.max(0.1) as f64 / 1e-4).ln()],
            held: true,
            over: false,
            lfo: [0.0, 1.0],
            dlfo: std::f32::consts::TAU * swing_hz(u[ix::LFO]) / sr,
            swing: swing_hz(u[ix::LFO]),
            sway: [lean, lean.abs()],
            // measured: the bend is set against the note's own starting level, so every touch bends alike
            drive: 1.8 * set.vel_factor.max(0.1) * u[ix::DRIVE] / env.max(1e-6),
            low: 0.48 / set.vel_factor.max(0.1),
            treble: 4.0 * treble * treble - 1.0,
            tz: [0.0; 2],
            tf: 1.0 - (-(if treble > 0.5 { 14000.0 } else { 5000.0 }) / sr).exp(),
            wide: [0.0; WIDE + 1],
            at: 0,
            depth,
        }
    }

    /// The key was let go. Returns the seconds the sound still needs.
    pub fn release(&mut self) -> f32 {
        self.held = false;
        self.dec = self.rel;
        // until the level is under a thousandth of where it is
        ((0.001f32 / self.env.max(0.001)).min(0.999).ln() / self.dec.ln().min(-1e-9) / 44100.0).min(20.0)
    }

    /// The slow swing belongs to the instrument, not the note: a note that starts `secs` after the
    /// instrument's first joins the swing where it stands by then.
    /// How long a note held for `hold` seconds counts as sounding, from its start. The original
    /// drops a voice once its level has fallen to a set fraction of a full touch, whatever the
    /// note's own touch, and looks once every `step` seconds (measured: once a tick).
    pub fn sounding(&self, hold: f64, step: f64) -> f64 {
        let [held, let_go, room] = self.fade;
        let whole = |t: f64| if step > 0.0 { (t / step - 1e-6).ceil().max(0.0) * step } else { t };
        if held * hold >= room {
            return whole(room / held.max(1e-9));
        }
        hold + whole((room - held * hold) / let_go.max(1e-9))
    }

    pub fn joined(&mut self, secs: f64) {
        let turn = (self.swing as f64 * secs).fract() * std::f64::consts::TAU;
        self.lfo = [turn.sin() as f32, turn.cos() as f32];
    }

    pub fn frame(&mut self, dphase: f32) -> [f32; 2] {
        let mut y = 0.0;
        let mut y2 = None;
        if !self.over {
            let z = &self.set.zones[self.zone];
            self.pos += self.per * dphase as f64;
            if self.pos >= z.end as f64 {
                if z.loop_len > 0 {
                    self.pos -= z.loop_len as f64;
                } else {
                    self.over = true;
                }
            }
            if !self.over {
                let s = z.sample.at(self.pos);
                let stereo = !z.sample.mono;
                let mut x = if stereo { s[0] } else { (s[0] + s[1]) * 0.5 } * self.env;
                if stereo {
                    // a set recorded in stereo keeps its two sides apart all the way through (measured)
                    let mut x = s[1] * self.env;
                    if self.drive > 0.0 && x > 0.0 {
                        x -= self.drive * x * x;
                        x = x.max(-self.low * self.env);
                    }
                    self.g[0] += self.ff * (x + self.g[1] - self.g[0]);
                    self.g[1] = x;
                    y2 = Some(self.g[0]);
                }
                // the overdrive bends the upper half of the wave down, as far as a floor that fades with the note
                if self.drive > 0.0 && x > 0.0 {
                    x -= self.drive * x * x;
                    x = x.max(-self.low * self.env);
                }
                self.env *= self.dec;
                // the muffle: a gentle low-pass that opens with the setting and the touch
                self.f0 += self.ff * (x + self.f1 - self.f0);
                self.f1 = x;
                y = self.f0;
            }
        }
        let (mut l, mut r) = (y * self.out[0], y2.unwrap_or(y) * self.out[1]);
        if self.treble != 0.0 {
            self.tz[0] += self.tf * (l - self.tz[0]);
            self.tz[1] += self.tf * (r - self.tz[1]);
            l += self.treble * (l - self.tz[0]);
            r += self.treble * (r - self.tz[1]);
        }
        if self.sway[1] != 0.0 {
            self.lfo[0] += self.dlfo * self.lfo[1];
            self.lfo[1] -= self.dlfo * self.lfo[0];
            l += l * self.sway[0] * self.lfo[1];
            r += r * self.sway[1] * self.lfo[1];
        }
        // the sum of both sides comes back a moment later, added on the left and taken off the right
        self.wide[self.at] = l + r;
        self.at = (self.at + 1) % (WIDE + 1);
        let back = self.depth * self.wide[self.at];
        [l + back, r - back]
    }
}

/// The level of a full-touch note before the stereo trim.
const LEVEL: f32 = 0.171;

/// Turns a second of the slow swing for its rate setting.
fn swing_hz(u: f32) -> f32 {
    // one turn a second at the middle, a hundred times slower and faster at the ends
    (9.2198 * (u - 0.5)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Vec<f32> {
        crate::synth::Gen::new(crate::synth::Kind::Keys).p.iter().zip(RANGES).map(|(&v, (lo, hi))| (v - lo as f32) / (hi - lo) as f32).collect()
    }

    #[test]
    fn built_tones_play_in_tune_and_fade() {
        let set = Arc::new(built("Piano"));
        let u = settings();
        let sr = 44100.0;
        for (key, hz) in [(69u8, 440.0f32), (40, 82.41), (84, 1046.5)] {
            let d = hz / sr;
            let mut v = Voice::start(&set, &u, sr, key, 0.8, d);
            let x: Vec<f32> = (0..44100).map(|_| v.frame(d)[0]).collect();
            let ups = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
            assert!((ups - hz).abs() < hz * 0.006 + 2.0, "key {key}: {ups}");
            let peak = x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.02 && peak < 1.0, "{peak}");
            let tail = v.release();
            assert!(tail > 0.0 && tail <= 20.0);
            let mut last = 0.0f32;
            for _ in 0..(tail * sr) as usize {
                last = v.frame(d)[0].abs().max(last * 0.99);
            }
            assert!(last < 0.01, "{last}");
        }
    }

    #[test]
    fn a_set_file_is_read() {
        let dir = std::env::temp_dir().join(format!("cereal-keys-{}", std::process::id()));
        let w = dir.join("Plugins/Fruity/Generators/FL Keys/Wavtables");
        std::fs::create_dir_all(&w).unwrap();
        let mut raw = vec![];
        for i in 0..4000 {
            let v = ((i as f32 / 50.0 * std::f32::consts::TAU).sin() * 12000.0) as i16;
            raw.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(w.join("tones.dat"), &raw).unwrap();
        std::fs::write(w.join("Tones.wti"), "[info]\nname=Test Tones\nshortname=Tones\nsamples=2\nsamplefile=tones.dat\nfiletype=raw\nsamplerate=22050\n\n[sample0]\nroot=69\nhigh=75\npos=0\nend=1999\nloop=500\n\n[sample1]\nroot=81\nhigh=999\npos=2000\nend=3999\nloop=500\n").unwrap();
        let set = find(&[dir.clone()], "test tones").unwrap();
        assert_eq!(set.zones.len(), 2);
        assert_eq!((set.zones[1].start, set.zones[1].end, set.zones[1].loop_len), (2000, 3999, 500));
        assert_eq!(find(&[dir.clone()], "Tones (something)").unwrap().name, "Test Tones");
        // the tone is 441 cycles a second at its root; a key an octave up plays it twice as fast
        let u = settings();
        let set = Arc::new(set);
        let d = 880.0 / 44100.0;
        let mut v = Voice::start(&set, &u, 44100.0, 69, 0.8, d);
        let x: Vec<f32> = (0..44100).map(|_| v.frame(d)[0]).collect();
        let ups = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((870..=895).contains(&ups), "{ups}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With the user's own install at hand (the corpus variable), its sets load and sound.
    #[test]
    fn installed_sets_load() {
        let Some(data) = std::env::var_os("CEREAL_FLP_CORPUS") else { return };
        let Some(root) = Path::new(&data).parent().map(|p| p.to_path_buf()) else { return };
        if set_dirs(&[root.clone()]).is_empty() {
            return;
        }
        let u = settings();
        for (name, zones) in [("Grand Piano", 30), ("mda Piano", 15), ("Piano", 15), ("Rhodes (mda ePiano)", 0), ("Roto Organ", 7)] {
            let set = Arc::new(find(&[root.clone()], name).unwrap_or_else(|| panic!("{name} not read")));
            assert!(zones == 0 || set.zones.len() == zones, "{name}: {}", set.zones.len());
            let d = 440.0 / 44100.0;
            let mut v = Voice::start(&set, &u, 44100.0, 69, 0.8, d);
            let x: Vec<f32> = (0..44100).map(|_| v.frame(d)[0]).collect();
            let peak = x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.02 && peak < 1.5, "{name}: {peak}");
        }
    }
}
