//! Triple Synth: three wavetable oscillators (the first can be synced, or bent or multiplied by the second),
//! noise, up to sixteen stacked copies, one filter, two attack-decay envelopes, two slow oscillators and a
//! level envelope. After the voices comes a row of ten simple effects and a five-band tone control
//! (`tailfx.rs`).
//!
//! The stored settings are three wave file names and a list of numbers from 0 to 1. Which number is which
//! was worked out from how they move across many stored sounds and from the public manual's description of
//! the controls; the scales are our estimates. The single-cycle wave files are the user's own content: they
//! are looked up by name in the folders the user pointed us at, and a plain built shape stands in when one is
//! not there.

use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::duo::{pick, Ctl, Info};
use crate::tailfx;

/// Numbers we read from the stored list.
pub const COUNT: usize = 212;
/// Wave name slots at the head of the block, each this long.
const SLOTS: usize = 7;
const SLOT: usize = 512;

pub const GROUPS: [&str; 15] = [
    "Master", "Level envelope", "Oscillators", "Mix", "Stack", "Filter", "Envelope 1", "Envelope 2", "Slow oscillator 1", "Slow oscillator 2", "Effects on", "Effects, first control",
    "Effects, second control", "Tone control", "Echo",
];

const DEFS: [f32; COUNT] = [
    0.75, 0.5, 0.5, 0.444444, 0.0, 1.0, 1.0, 0.1, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.5, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5,
    0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.556667, 0.556667, 0.556667, 0.556667, 0.556667, 0.556667,
    0.556667, 0.556667, 0.556667, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 1.0, 0.5, 0.0, 0.5, 0.0, 0.166667, 0.0, 0.5,
    0.5, 0.5, 0.5, 0.5,
];

const FX: [&str; 10] = ["Overdrive", "Crush", "Flanger", "Phaser", "Low pass", "High pass", "Echo", "Room", "Pan", "Pump"];
const EG_TO: &[&str] = &["Cutoff", "Bend amount", "Resonance", "Pitch"];
const LFO_TO: &[&str] = &["Pitch", "Cutoff", "Level"];
const SHAPES: &[&str] = &["Sine", "Triangle", "Saw", "Square"];
const SYNCS: &[&str] = &["Free", "Beats", "Bars", "Four bars"];

fn placed() -> Vec<(usize, &'static str, Ctl, u8)> {
    use Ctl::{Knob, Pick, Switch};
    let mut v = vec![
        (0, "Output", Knob, 0),
        (1, "Pan", Knob, 0),
        (2, "Pitch", Knob, 0),
        (8, "Level", Knob, 0),
        (44, "Octave", Pick(&["-2", "-1", "0", "+1", "+2"]), 0),
        (40, "One voice", Switch, 0),
        (11, "Slide", Switch, 0),
        (10, "Slide time", Knob, 0),
        (4, "Attack", Knob, 1),
        (5, "Decay", Knob, 1),
        (6, "Sustain", Knob, 1),
        (7, "Release", Knob, 1),
        (55, "Pitch 1", Knob, 2),
        (56, "Fine 1", Knob, 2),
        (57, "Pitch 2", Knob, 2),
        (58, "Fine 2", Knob, 2),
        (59, "Pitch 3", Knob, 2),
        (60, "Fine 3", Knob, 2),
        (51, "Bend", Pick(&["Sync", "2 bends 1", "2 times 1"]), 2),
        (50, "Bend amount", Knob, 2),
        (48, "Oscillator 2", Knob, 3),
        (53, "Flip 2", Switch, 3),
        (49, "Oscillator 3", Knob, 3),
        (45, "Noise", Knob, 3),
        (46, "Start of 2", Knob, 3),
        (47, "Restart 2", Switch, 3),
        (41, "Copies", Knob, 4),
        (43, "Spread", Knob, 4),
        (42, "Detune", Knob, 4),
        (52, "Start", Knob, 4),
        (12, "Cutoff", Knob, 5),
        (13, "Resonance", Knob, 5),
        (14, "Follow keys", Switch, 5),
        (15, "Kind", Pick(&["Low pass", "Band pass", "High pass"]), 5),
    ];
    for e in 0..2 {
        let g = 6 + e as u8;
        v.extend([(20 + e, "Attack", Knob, g), (22 + e, "Decay", Knob, g), (16 + e, "Amount", Knob, g), (18 + e, "Flip", Switch, g), (24 + e, "To", Pick(EG_TO), g)]);
        let g = 8 + e as u8;
        v.extend([
            (34 + e, "Rate", Knob, g),
            (38 + e, "Sync", Pick(SYNCS), g),
            (28 + e, "Amount", Knob, g),
            (26 + e, "Restart", Switch, g),
            (36 + e, "To", Pick(LFO_TO), g),
            (30 + e, "Flip", Switch, g),
            (32 + e, "Shape", Pick(SHAPES), g),
        ]);
    }
    for (k, name) in FX.iter().enumerate() {
        v.push((147 + k, name, Switch, 10));
        v.push((117 + k, name, Knob, 11));
        v.push((127 + k, name, Knob, 12));
    }
    for (k, name) in ["60 Hz", "220 Hz", "1.5 kHz", "8 kHz", "12 kHz"].iter().enumerate() {
        v.push((177 + k, name, Knob, 13));
    }
    v.push((143, "Time", Knob, 14));
    v
}

pub fn params() -> &'static [Info] {
    static P: OnceLock<Vec<Info>> = OnceLock::new();
    P.get_or_init(|| {
        let mut v: Vec<Info> = DEFS.iter().map(|&def| Info { name: "", ctl: Ctl::Kept, def, group: 0 }).collect();
        for (i, name, ctl, group) in placed() {
            v[i] = Info { name, ctl, def: DEFS[i], group };
        }
        v
    })
}

pub fn defaults() -> Vec<f32> {
    DEFS.to_vec()
}

/// Where the list of numbers starts in a stored block, when the block is one of ours to read.
pub fn base(raw: &[u8]) -> Option<usize> {
    if raw.len() < 12 || &raw[..4] != b"SSLF" || &raw[9..12] != b"nyS" {
        return None;
    }
    // the later layout carries three more numbers ahead of the list
    let at = 12 + SLOTS * SLOT + if raw[8] >= b'4' { 12 } else { 0 };
    (raw.len() >= at + COUNT * 4).then_some(at)
}

pub fn values(raw: &[u8]) -> Vec<f32> {
    let Some(at) = base(raw) else { return Vec::new() };
    (0..COUNT).map(|i| f32::from_le_bytes(raw[at + i * 4..at + i * 4 + 4].try_into().unwrap())).map(|v| if v.is_finite() { v } else { 0.0 }).collect()
}

/// A block as a stored sound file carries it becomes a project block by a short head.
pub fn wrap(file: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(file.len() + 8);
    v.extend_from_slice(b"SSLF");
    v.extend_from_slice(&(file.len() as u32).to_le_bytes());
    v.extend_from_slice(file);
    v
}

pub fn fresh() -> Vec<u8> {
    let mut v = vec![0u8; 8 + 4948];
    v[..4].copy_from_slice(b"SSLF");
    v[4..8].copy_from_slice(&4948u32.to_le_bytes());
    v[8..12].copy_from_slice(b"3nyS");
    for k in 0..3 {
        v[12 + k * SLOT..12 + k * SLOT + 7].copy_from_slice(b"Saw.wav");
    }
    let at = 12 + SLOTS * SLOT;
    for (i, x) in DEFS.iter().enumerate() {
        v[at + i * 4..at + i * 4 + 4].copy_from_slice(&x.to_le_bytes());
    }
    v[at + COUNT * 4..at + COUNT * 4 + 7].copy_from_slice(b"Default");
    v
}

/// The wave name in slot `k` (0 to 2): the file name without its folders and ending.
pub fn wave_name(raw: &[u8], k: usize) -> String {
    let Some(s) = raw.get(12 + k * SLOT..12 + (k + 1) * SLOT) else { return String::new() };
    let s = &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())];
    let s = String::from_utf8_lossy(s);
    let file = s.rsplit(['\\', '/']).next().unwrap_or("");
    file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_string()
}

/// Put another wave name in slot `k`, keeping the folder the block names.
pub fn set_wave_name(raw: &mut [u8], k: usize, name: &str) {
    let Some(s) = raw.get_mut(12 + k * SLOT..12 + (k + 1) * SLOT) else { return };
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    let cut = s[..end].iter().rposition(|&b| b == b'\\' || b == b'/').map_or(0, |i| i + 1);
    let new = format!("{name}.wav");
    if cut + new.len() >= SLOT {
        return;
    }
    s[cut..].fill(0);
    s[cut..cut + new.len()].copy_from_slice(new.as_bytes());
}

// ---------------------------------------------------------------------------------------------------------
// Waves

const TABLE: usize = 2048;
const LEVELS: usize = 10;

/// One cycle, kept at ten levels of detail so high notes do not fold over.
#[derive(Debug)]
pub struct Wave {
    levels: Vec<Vec<f32>>,
}

impl Wave {
    /// From the strength of each overtone as (cosine, sine) parts, the first being the note itself.
    pub(crate) fn from_parts(parts: &[(f32, f32)]) -> Self {
        let mut levels = vec![vec![0.0f32; TABLE + 1]; LEVELS];
        let mut acc = vec![0.0f32; TABLE + 1];
        // the level with the fewest overtones first, adding the next ones for each richer level
        let mut from = 0usize;
        for lv in (0..LEVELS).rev() {
            let upto = (512usize >> lv).min(parts.len());
            for h in from..upto {
                let (c, s) = parts[h];
                if c == 0.0 && s == 0.0 {
                    continue;
                }
                let w = TAU * (h + 1) as f32 / TABLE as f32;
                for (i, a) in acc.iter_mut().enumerate() {
                    let x = w * i as f32;
                    *a += c * x.cos() + s * x.sin();
                }
            }
            from = from.max(upto);
            levels[lv].copy_from_slice(&acc);
        }
        Self { levels }
    }

    fn from_cycle(cycle: &[f32]) -> Self {
        let n = cycle.len();
        let mean = cycle.iter().sum::<f32>() / n.max(1) as f32;
        let parts: Vec<(f32, f32)> = (1..=(n / 2).min(512))
            .map(|h| {
                let w = TAU * h as f32 / n as f32;
                let (mut c, mut s) = (0.0f32, 0.0f32);
                for (i, &x) in cycle.iter().enumerate() {
                    let a = w * i as f32;
                    c += (x - mean) * a.cos();
                    s += (x - mean) * a.sin();
                }
                (c * 2.0 / n as f32, s * 2.0 / n as f32)
            })
            .collect();
        Self::from_parts(&parts)
    }

    /// A plain shape for a name we have no file for.
    fn built(name: &str) -> Self {
        let n = name.to_lowercase();
        let parts: Vec<(f32, f32)> = (1..=512)
            .map(|h| {
                let h = h as f32;
                let odd = h as u32 % 2 == 1;
                if n.contains("sine") || n.contains("sub") {
                    (0.0, if h == 1.0 { 1.0 } else { 0.0 })
                } else if n.contains("square") {
                    (0.0, if odd { 4.0 / PI / h } else { 0.0 })
                } else if n.contains("triangle") {
                    (0.0, if odd { 8.0 / PI / PI / h / h * if h as u32 % 4 == 1 { 1.0 } else { -1.0 } } else { 0.0 })
                } else {
                    (0.0, 2.0 / PI / h * if odd { 1.0 } else { -1.0 })
                }
            })
            .collect();
        Self::from_parts(&parts)
    }

    /// The wave at `ph` (0 to 1) for a note that moves `dp` of a cycle per frame.
    #[inline]
    pub(crate) fn at(&self, ph: f32, dp: f32) -> f32 {
        let room = 0.45 / dp.abs().max(1e-6);
        let mut lv = 0;
        while lv < LEVELS - 1 && (512 >> lv) as f32 > room {
            lv += 1;
        }
        let x = (ph - ph.floor()) * TABLE as f32;
        let i = (x as usize).min(TABLE - 1);
        let t = &self.levels[lv];
        t[i] + (t[i + 1] - t[i]) * (x - i as f32)
    }
}

#[derive(Default)]
struct Store {
    names: Vec<String>,
    waves: Vec<Arc<Wave>>,
    roots: usize,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn wave_dirs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = vec![];
    for r in roots {
        let mut cur = Some(r.clone());
        for part in ["Plugins", "Fruity", "Generators", "GMS", "Synth Waves"] {
            cur = cur.and_then(|c| crate::keys::child_ci(&c, part));
        }
        out.extend(cur);
    }
    out
}

fn load(dirs: &[PathBuf], name: &str) -> Wave {
    for d in dirs {
        let Some(path) = crate::keys::child_ci(d, &format!("{name}.wav")) else { continue };
        if let Ok(s) = crate::sample::Sample::load(&path) {
            if s.frames.len() >= 16 && s.frames.len() <= 1 << 16 {
                let cycle: Vec<f32> = s.frames.iter().map(|f| f[0]).collect();
                return Wave::from_cycle(&cycle);
            }
        }
    }
    Wave::built(name)
}

/// The number a voice finds this wave under.
pub fn wave_id(name: &str) -> usize {
    let roots = crate::keys::roots();
    let mut s = store().lock().unwrap();
    let dirs = wave_dirs(&roots);
    if s.roots != roots.len() {
        // more of the user's folders are known now: look again for the files
        s.roots = roots.len();
        let names = s.names.clone();
        s.waves = names.iter().map(|n| Arc::new(load(&dirs, n))).collect();
    }
    let key = name.to_lowercase();
    if let Some(i) = s.names.iter().position(|n| *n == key) {
        return i;
    }
    let w = Arc::new(load(&dirs, &key));
    s.names.push(key);
    s.waves.push(w);
    s.names.len() - 1
}

fn wave(id: usize) -> Arc<Wave> {
    let have = store().lock().unwrap().waves.get(id).cloned();
    have.unwrap_or_else(|| {
        let id = wave_id("saw");
        store().lock().unwrap().waves[id].clone()
    })
}

/// The names of the waves there are to pick from: the user's files when we know of them, and the built ones.
pub fn wave_names() -> Vec<String> {
    let mut out: Vec<String> = vec!["Saw".into(), "Sine".into(), "Square".into(), "Triangle".into()];
    for d in wave_dirs(&crate::keys::roots()) {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p: &Path = &e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) {
                let stem = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                if !out.iter().any(|n| n.eq_ignore_ascii_case(&stem)) {
                    out.push(stem);
                }
            }
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// The three wave numbers of a block, as the extra numbers a voice reads after the list.
pub fn wave_ids(raw: &[u8]) -> [f32; 3] {
    [0, 1, 2].map(|k| wave_id(&wave_name(raw, k)) as f32)
}

// ---------------------------------------------------------------------------------------------------------
// Settings

#[derive(Debug, Clone, Copy, Default)]
struct Eg {
    a: f32,
    d: f32,
    amount: f32,
    flip: bool,
    to: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct Lfo {
    hz: f32,
    amount: f32,
    restart: bool,
    to: usize,
    flip: bool,
    shape: usize,
}

#[derive(Debug, Clone)]
pub struct Set {
    waves: [Arc<Wave>; 3],
    /// Frequency of each oscillator against the note.
    ratio: [f32; 3],
    mode: usize,
    amount: f32,
    mix2: f32,
    flip2: bool,
    mix3: f32,
    noise: f32,
    start2: f32,
    restart2: bool,
    copies: usize,
    spread: f32,
    /// Cents from the middle copy to the outer ones.
    detune: f32,
    start: f32,
    cutoff: f32,
    res: f32,
    follow: bool,
    kind: usize,
    eg: [Eg; 2],
    lfo: [Lfo; 2],
    /// Attack, decay and release in seconds, sustain as a level.
    amp: [f32; 4],
    gain: f32,
    pan: f32,
    /// One voice at a time, and the seconds a held note takes to slide to the next.
    pub mono: Option<f32>,
}

pub fn set(p: &[f32]) -> Set {
    let own;
    let (p, ids) = if p.len() >= COUNT {
        (p, p.get(COUNT..COUNT + 3))
    } else {
        own = defaults();
        (&own[..], None)
    };
    let u = |i: usize| if p[i].is_finite() { p[i].clamp(0.0, 1.0) } else { DEFS[i] };
    let on = |i: usize| p[i] >= 0.5;
    let waves = [0, 1, 2].map(|k| wave(ids.map_or(usize::MAX, |w| w[k].max(0.0) as usize)));
    let octave = pick(u(44), 5) as f32 - 2.0;
    let whole = (u(2) - 0.5) * 24.0 + octave * 12.0;
    let ratio = [0, 1, 2].map(|k| {
        let semis = ((u(55 + 2 * k) - 0.5) * 72.0).trunc() + (u(56 + 2 * k) - 0.5) * 2.0;
        2f32.powf((whole + semis) / 12.0)
    });
    let beat = 60.0 / crate::minis::tempo().clamp(20.0, 999.0);
    let eg = [0, 1].map(|e| Eg { a: 0.001 + u(20 + e).powi(2) * 4.0, d: 0.005 + u(22 + e).powi(2) * 6.0, amount: u(16 + e), flip: on(18 + e), to: pick(u(24 + e), 4) });
    let lfo = [0, 1].map(|e| {
        let v = u(34 + e);
        let beats = [8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125, 0.0625][pick(v, 8)];
        let hz = match pick(u(38 + e), 4) {
            0 => 0.05 * 400f32.powf(v),
            1 => 1.0 / (beats * beat),
            2 => 1.0 / (beats * beat * 4.0),
            _ => 1.0 / (beats * beat * 16.0),
        };
        Lfo { hz, amount: u(28 + e), restart: on(26 + e), to: pick(u(36 + e), 3), flip: on(30 + e), shape: pick(u(32 + e), 4) }
    });
    Set {
        waves,
        ratio,
        mode: pick(u(51), 3),
        amount: u(50),
        mix2: u(48),
        flip2: on(53),
        mix3: u(49),
        noise: u(45),
        start2: u(46),
        restart2: on(47),
        copies: 1 + (u(41) * 15.0).round() as usize,
        spread: u(43),
        detune: u(42) * 50.0,
        start: u(52),
        cutoff: u(12),
        res: u(13),
        follow: on(14),
        kind: pick(u(15), 3),
        eg,
        lfo,
        amp: [0.001 + u(4).powi(2) * 5.0, 0.005 + u(5).powi(2) * 8.0, u(6), 0.005 + u(7).powi(2) * 5.0],
        gain: u(0) / 0.75 * u(8) * 2.0 * 0.4,
        pan: (u(1) - 0.5) * 2.0,
        mono: on(40).then(|| if on(11) { 0.005 + u(10).powi(2) * 2.0 } else { 0.0 }),
    }
}

/// The effects after the voices: only the ones switched to stay on sound.
pub fn tail(p: &[f32]) -> tailfx::Set {
    if p.len() < COUNT {
        return Default::default();
    }
    let u = |i: usize| if p[i].is_finite() { p[i].clamp(0.0, 1.0) } else { DEFS[i] };
    let fx = |k: usize| (p[147 + k] >= 0.5).then(|| [u(117 + k), u(127 + k)]);
    let db = [0, 1, 2, 3, 4].map(|k| (u(177 + k) - 0.5) * 24.0);
    let rack = tailfx::Rack {
        drive: fx(0).map(|[x, y]| [x, 300.0 * 60f32.powf(y)]),
        crush: fx(1).map(|[x, y]| [1.0 + x * x * 40.0, 300.0 * 60f32.powf(y)]),
        flange: fx(2),
        phase: fx(3),
        low: fx(4).map(|[x, y]| [40.0 * 500f32.powf(x), y]),
        high: fx(5).map(|[x, y]| [20.0 * 500f32.powf(x), y]),
        pan: fx(8).map(|[x, _]| (x - 0.5) * 2.0),
        pump: fx(9).map(|[x, y]| [0.5 + x * 3.5, y]),
        eq: db.iter().any(|d| d.abs() > 0.05).then_some(db),
    };
    let steps = [0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0][pick(u(143), 9)];
    tailfx::Set {
        rack: (rack != tailfx::Rack::default()).then_some(rack),
        delay: fx(6).map(|[x, y]| tailfx::Delay { left: steps, right: steps, fb: x * 0.9, wobble: 0.0, low_hz: 0.0, high_hz: 600.0 * 30f32.powf(y), wet: 0.45, free: 0.0 }),
        reverb: fx(7).filter(|f| f[1] > 0.0).map(|[x, y]| tailfx::Reverb { secs: 0.3 + x * x * 8.0, high_hz: 6000.0, wet: y }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------------------
// The voice

const EVERY: u32 = 16;
const MOST: usize = 16;

#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    key: u8,
    vel: f32,
    n: usize,
    ph: [[f32; 3]; MOST],
    lead: [f32; MOST],
    tune: [f32; MOST],
    pans: [[f32; 2]; MOST],
    seed: u32,
    filt: [[f32; 2]; 2],
    /// Seconds since the note began.
    t: f32,
    lfo: [f32; 2],
    env: f32,
    stage: u8,
    fall: f32,
    tick: u32,
    g: f32,
    k: f32,
    trim: f32,
    bend: f32,
    amount: f32,
    fast: f32,
    level: f32,
    level_step: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

impl Voice {
    pub fn start(set: &Set, sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let mut seed = seed | 1;
        let n = set.copies.clamp(1, MOST);
        let mut v = Self {
            sr,
            key,
            vel,
            n,
            ph: [[0.0; 3]; MOST],
            lead: [0.0; MOST],
            tune: [1.0; MOST],
            pans: [[1.0; 2]; MOST],
            seed: 0,
            filt: [[0.0; 2]; 2],
            t: 0.0,
            lfo: [0.0; 2],
            env: 0.0,
            stage: 0,
            fall: 0.0,
            tick: 0,
            g: 0.1,
            k: 1.4,
            trim: 1.0,
            bend: 1.0,
            amount: 0.0,
            fast: 1.0,
            level: 0.0,
            level_step: 0.0,
        };
        let free2 = if set.restart2 { set.start2 } else { rnd(&mut seed) * 0.5 + 0.5 };
        for c in 0..n {
            let pos = if n > 1 { c as f32 / (n - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
            v.tune[c] = 2f32.powf(set.detune * pos / 1200.0);
            v.pans[c] = [(1.0 - pos * set.spread).min(1.0), (1.0 + pos * set.spread).min(1.0)];
            let at = (c as f32 * set.start * 0.618).fract();
            v.ph[c] = [at, (at + free2).fract(), at];
        }
        for (e, l) in set.lfo.iter().enumerate() {
            v.lfo[e] = if l.restart { 0.0 } else { rnd(&mut seed) * 0.5 + 0.5 };
        }
        v.seed = seed;
        v.control(set);
        v.level = v.level_step * EVERY as f32;
        v.level_step = 0.0;
        v
    }

    /// Let the note go; gives the seconds it still sounds.
    pub fn release(&mut self, set: &Set) -> f32 {
        if self.stage != 2 {
            self.stage = 2;
            self.fall = self.env.max(0.001) / (set.amp[3] * self.sr);
        }
        set.amp[3] + 0.02
    }

    fn control(&mut self, set: &Set) {
        let dt = EVERY as f32 / self.sr;
        // the level envelope
        match self.stage {
            0 => {
                self.env += dt / set.amp[0];
                if self.env >= 1.0 {
                    self.env = 1.0;
                    self.stage = 1;
                }
            }
            1 => self.env += (set.amp[2] - self.env) * (1.0 - (-dt * 4.0 / set.amp[1]).exp()),
            _ => self.env = (self.env - self.fall * EVERY as f32).max(0.0),
        }
        let (mut cutoff, mut res, mut amount, mut semis, mut level) = (set.cutoff, set.res, set.amount, 0.0f32, 1.0f32);
        for e in &set.eg {
            if e.amount <= 0.0 {
                continue;
            }
            let x = if self.t < e.a { self.t / e.a } else { (-(self.t - e.a) * 4.0 / e.d).exp() };
            // the envelope lifts its target from where the control stands, or presses it down when flipped
            let f = e.amount * if e.flip { -x } else { x };
            match e.to {
                0 => cutoff += f,
                1 => amount += f,
                2 => res += f,
                _ => semis += f * 24.0,
            }
        }
        for (e, l) in set.lfo.iter().enumerate() {
            self.lfo[e] = (self.lfo[e] + l.hz * dt).fract();
            if l.amount <= 0.0 {
                continue;
            }
            let ph = self.lfo[e];
            // the sine swings both ways, the other shapes run from nothing to full
            let (x, both) = match l.shape {
                0 => ((ph * TAU).sin(), true),
                1 => (1.0 - (2.0 * ph - 1.0).abs(), false),
                2 => (ph, false),
                _ => (if ph < 0.5 { 1.0 } else { 0.0 }, false),
            };
            let x = match (l.flip, both) {
                (false, _) => x,
                (true, true) => -x,
                (true, false) => 1.0 - x,
            };
            match l.to {
                0 => semis += l.amount * 4.0 * x,
                1 => cutoff += l.amount * 0.5 * x,
                _ => level *= 1.0 - l.amount * if both { 0.5 - 0.5 * x } else { 1.0 - x },
            }
        }
        // following the keys, the filter starts from the note itself
        let low = if set.follow { 440.0 * 2f32.powf((self.key as f32 - 69.0) / 12.0) } else { 60.0 };
        let hz = low * 2f32.powf(8.4 * cutoff.clamp(0.0, 1.0));
        self.g = (PI * hz.clamp(20.0, self.sr * 0.45) / self.sr).tan();
        let res = res.clamp(0.0, 1.0);
        self.k = 1.0 / (0.7 + res * res * 7.0);
        self.trim = 1.0 / (1.0 + res * res * 1.5);
        self.bend = 2f32.powf(semis / 12.0);
        self.amount = amount.clamp(0.0, 1.0);
        self.fast = 2f32.powf(self.amount * 3.0);
        let want = self.env * level * set.gain * self.vel;
        self.level_step = (want - self.level) / EVERY as f32;
        self.t += dt;
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        if self.tick == 0 {
            self.control(set);
        }
        self.tick = (self.tick + 1) % EVERY;
        let [w0, w1, w2] = [&*set.waves[0], &*set.waves[1], &*set.waves[2]];
        let mut out = [0.0f32; 2];
        for c in 0..self.n {
            let f = dphase * self.tune[c] * self.bend;
            let d = [f * set.ratio[0], f * set.ratio[1], f * set.ratio[2]];
            let ph = &mut self.ph[c];
            ph[1] = (ph[1] + d[1]).fract();
            let mut o2 = w1.at(ph[1], d[1]);
            if set.flip2 {
                o2 = -o2;
            }
            let o1 = match set.mode {
                0 if self.amount > 0.001 => {
                    // a faster copy of the first oscillator, started over at the note's own pace
                    let step = d[0] * self.fast;
                    self.lead[c] += d[0];
                    if self.lead[c] >= 1.0 {
                        self.lead[c] -= 1.0;
                        ph[0] = self.lead[c] * self.fast;
                    } else {
                        ph[0] += step;
                    }
                    w0.at(ph[0], step)
                }
                0 => {
                    ph[0] = (ph[0] + d[0]).fract();
                    w0.at(ph[0], d[0])
                }
                1 => {
                    ph[0] = (ph[0] + d[0]).fract();
                    w0.at(ph[0] + o2 * self.amount * self.amount * 1.5, d[0] * (1.0 + self.amount * 3.0))
                }
                _ => {
                    ph[0] = (ph[0] + d[0]).fract();
                    w0.at(ph[0], d[0]) * (1.0 - self.amount + self.amount * o2)
                }
            };
            ph[2] = (ph[2] + d[2]).fract();
            let o3 = w2.at(ph[2], d[2]);
            let s = o1 + (o2 - o1) * set.mix2;
            let s = s + (o3 - s) * set.mix3;
            out[0] += s * self.pans[c][0];
            out[1] += s * self.pans[c][1];
        }
        let norm = 1.0 / (self.n as f32).sqrt();
        let hiss = if set.noise > 0.0 { rnd(&mut self.seed) } else { 0.0 };
        self.level += self.level_step;
        let (l, r) = ((1.0 - set.pan).min(1.0), (1.0 + set.pan).min(1.0));
        for side in 0..2 {
            let x = out[side] * norm;
            let x = (x + (hiss - x) * set.noise) * self.trim;
            let s = &mut self.filt[side];
            let a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
            let a2 = self.g * a1;
            let v3 = x - s[1];
            let v1 = a1 * s[0] + a2 * v3;
            let v2 = s[1] + a2 * s[0] + self.g * a2 * v3;
            s[0] = 2.0 * v1 - s[0];
            s[1] = 2.0 * v2 - s[1];
            let y = match set.kind {
                0 => v2,
                1 => v1,
                _ => x - self.k * v1 - v2,
            };
            out[side] = y * self.level * if side == 0 { l } else { r };
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(p: &[f32], key: u8, hz: f32, held: usize, after: usize) -> Vec<[f32; 2]> {
        let set = set(p);
        let d = hz / 44100.0;
        let mut v = Voice::start(&set, 44100.0, key, 0.8, 7);
        let mut out: Vec<[f32; 2]> = (0..held).map(|_| v.frame(&set, d)).collect();
        v.release(&set);
        out.extend((0..after).map(|_| v.frame(&set, d)));
        out
    }

    #[test]
    fn a_fresh_block_reads_back_and_plays_in_tune() {
        let raw = fresh();
        assert_eq!(base(&raw), Some(12 + 7 * 512));
        let p = values(&raw);
        assert_eq!(p, defaults());
        assert_eq!(wave_name(&raw, 0), "Saw");
        let x = play(&p, 69, 440.0, 44100, 0);
        let ups = x.windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        assert!((436..=444).contains(&ups), "{ups}");
        let peak = x.iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.1 && peak < 1.0, "{peak}");
    }

    #[test]
    fn the_wave_name_is_changed_in_its_folder() {
        let mut raw = fresh();
        let path = b"C:\\Some Folder\\Waves\\Saw.wav";
        raw[12 + 512..12 + 512 + path.len()].copy_from_slice(path);
        set_wave_name(&mut raw, 1, "Square Retro");
        assert_eq!(wave_name(&raw, 1), "Square Retro");
        assert_eq!(&raw[12 + 512..12 + 512 + 21], b"C:\\Some Folder\\Waves\\");
        assert_eq!(wave_name(&raw, 0), "Saw");
        // the later layout starts three numbers further on
        let mut later = raw.clone();
        later[8] = b'4';
        later.extend_from_slice(&[0; 12]);
        assert_eq!(base(&later), Some(12 + 7 * 512 + 12));
    }

    #[test]
    fn the_filter_envelope_closes_the_sound_and_release_ends_it() {
        let mut p = defaults();
        p[12] = 0.0;
        p[16] = 1.0;
        p[22] = 0.2;
        let x = play(&p, 60, 261.63, 44100, 22050);
        let rms = |a: &[[f32; 2]]| (a.iter().map(|f| f[0] * f[0]).sum::<f32>() / a.len() as f32).sqrt();
        // bright at first, dull once the envelope has run out
        let bright = |a: &[[f32; 2]]| rms(&a.windows(2).map(|w| [w[1][0] - w[0][0], 0.0]).collect::<Vec<_>>()) / rms(a).max(1e-9);
        assert!(bright(&x[100..1200]) > bright(&x[30000..40000]) * 2.0);
        assert!(rms(&x[64000..]) < 0.001, "{}", rms(&x[64000..]));
    }

    #[test]
    fn stacked_copies_sync_and_bends_stay_sound() {
        for mode in [0.0, 0.5, 1.0] {
            let mut p = defaults();
            p[41] = 1.0;
            p[42] = 0.6;
            p[43] = 1.0;
            p[51] = mode;
            p[50] = 0.7;
            p[48] = 0.5;
            p[49] = 0.3;
            p[45] = 0.1;
            let x = play(&p, 72, 523.25, 22050, 4000);
            let peak = x.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
            assert!(peak.is_finite() && peak > 0.05 && peak < 3.0, "{mode}: {peak}");
            // the copies sit apart, so the two sides differ
            assert!(x.iter().any(|f| (f[0] - f[1]).abs() > 0.01));
        }
    }

    #[test]
    fn switched_on_effects_reach_the_tail() {
        let mut p = defaults();
        assert!(!tail(&p).any());
        p[147] = 1.0;
        p[153] = 1.0;
        p[154] = 1.0;
        p[134] = 0.5;
        p[179] = 0.8;
        let t = tail(&p);
        assert!(t.rack.is_some_and(|r| r.drive.is_some() && r.eq.is_some() && r.crush.is_none()));
        assert!(t.delay.is_some() && t.reverb.is_some());
        let mut buf: Vec<[f32; 2]> = (0..22050).map(|i| [(i as f32 * 0.05).sin() * 0.5; 2]).collect();
        tailfx::State::new(44100.0).process(&t, &mut buf, 5512.0, None);
        assert!(buf.iter().all(|f| f[0].is_finite() && f[0].abs() < 4.0));
    }
}
