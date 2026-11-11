//! Hybrid Synth: six oscillators that can bend each other's phase, a filter and two slow oscillators.
//!
//! It reads and writes the block stored under the identifier "Toxic Biohazard". The layout was worked out
//! from stored sound files and the public manual alone: a short head, eight lists of 128 overtone
//! strengths and 128 starting points (the own waves of the six oscillators and the two slow oscillators), then a list of numbers.
//! Scales are estimates. The step runner, the two effect units and the links to outside controllers are
//! kept in the block untouched and are not worked yet.

use std::f32::consts::PI;
use std::sync::{Arc, Mutex, OnceLock};

use crate::duo::{Ctl, Info};
use crate::tailfx;
use crate::triple::Wave;

/// Numbers in the list.
pub const COUNT: usize = 340;
/// Length of the block inside its wrapper, and where its parts sit.
const FILE: usize = 10042;
const SHORT: usize = 9914;
const LIST: usize = 8260;
const OWN: usize = 68;
const OSCS: usize = 6;

pub const GROUPS: [&str; 18] = [
    "Master", "Level envelope", "Filter", "Filter envelope", "Oscillator 1", "Oscillator 2", "Oscillator 3", "Oscillator 4", "Oscillator 5", "Oscillator 6", "Into oscillator 1",
    "Into oscillator 2", "Into oscillator 3", "Into oscillator 4", "Into oscillator 5", "Into oscillator 6", "Slow oscillators", "Tone control",
];

const DEFS: [f32; COUNT] = [
    0.5, 0.0, 0.5, 0.0, 1.0, 1.0, 0.0, 0.0, 0.2, 0.0,
    0.0, 0.5, 0.0, 0.4, 0.4, 0.5, 0.0, 0.0, 0.0, 0.0,
    1.0, 1.0, 1.0, 0.5, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5,
    0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 1.0,
    1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5,
    0.5, 0.5, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5,
    0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5,
    0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.22, 0.22,
    0.8, 0.8, 1.0, 1.0, 0.15, 0.15, 0.8, 0.8, 1.0, 1.0,
    1.0, 1.0, 0.55, 0.55, 0.2, 0.2, 0.4, 0.4, 0.5, 0.5,
    0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.6, 0.6,
    0.24, 0.24, 0.2, 0.2, 0.5, 0.5, 0.35, 0.35, 1.0, 1.0,
    0.4, 0.4, 1.0, 1.0, 0.4, 0.4, 0.4, 0.4, 0.5, 0.5,
    0.5, 0.5, 0.7, 0.7, 0.0, 0.175, 0.0, 1.0, 0.0, 1.0,
    0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
    1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5,
];

const WAVES: &[&str] = &[
    "Own wave", "Sine", "Shape 2", "Shape 3", "Shape 4", "Shape 5", "Shape 6", "Shape 7", "Square", "Soft square", "Shape 10", "Shape 11", "Saw", "Shape 13", "Shape 14", "Shape 15", "Shape 16",
    "Shape 17", "Shape 18", "Shape 19", "Shape 20", "Shape 21", "Shape 22", "Shape 23", "Shape 24", "Shape 25", "Shape 26", "Shape 27", "Shape 28", "Shape 29", "Shape 30", "Shape 31", "Shape 32",
];
const MODES: &[&str] = &["Off", "Low pass", "Band pass", "High pass"];
const FROM: [&str; OSCS] = ["From 1", "From 2", "From 3", "From 4", "From 5", "From 6"];
const TO: [&str; OSCS] = ["Level 1", "Level 2", "Level 3", "Level 4", "Level 5", "Level 6"];
const BANDS: [&str; 8] = ["Band 1", "Band 2", "Band 3", "Band 4", "Band 5", "Band 6", "Band 7", "Band 8"];

fn placed() -> Vec<(usize, &'static str, Ctl, u8)> {
    use Ctl::{Choice, Knob, Switch, Whole};
    let mut v = vec![
        (0, "Level", Knob, 0),
        (8, "Drive", Knob, 0),
        (302, "Copies", Whole(7), 0),
        (9, "Copies detune", Knob, 0),
        (2, "Copies spread", Knob, 0),
        (10, "Slide time", Knob, 0),
        (3, "Attack", Knob, 1),
        (4, "Decay", Knob, 1),
        (5, "Sustain", Knob, 1),
        (6, "Release", Knob, 1),
        (300, "Kind", Choice(MODES), 2),
        (15, "Cutoff", Knob, 2),
        (16, "Resonance", Knob, 2),
        (17, "Key follow", Knob, 2),
        (18, "Strength follow", Knob, 2),
        (23, "Envelope", Knob, 2),
        (24, "Slow 1", Knob, 2),
        (19, "Attack", Knob, 3),
        (20, "Decay", Knob, 3),
        (21, "Sustain", Knob, 3),
        (22, "Release", Knob, 3),
        (11, "To pitch", Knob, 3),
        (13, "Speed 1", Knob, 16),
        (312, "Shape 1", Choice(WAVES), 16),
        (14, "Speed 2", Knob, 16),
        (313, "Shape 2", Choice(WAVES), 16),
        (12, "Slow 2 to pitch", Knob, 16),
        (227, "On", Switch, 17),
    ];
    for k in 0..OSCS {
        let g = 4 + k as u8;
        v.extend([
            (25 + k, "On", Switch, g),
            (306 + k, "Wave", Choice(WAVES), g),
            (31 + k, "Pitch", Knob, g),
            (37 + k, "Fine", Knob, g),
            (43 + k, "Offset", Knob, g),
            (49 + k, "Restart", Switch, g),
            (55 + k, "Start", Knob, g),
            (61 + k, "Strength", Knob, g),
            (67 + k, "First level", Knob, g),
            (73 + k, "Attack", Knob, g),
            (79 + k, "Decay", Knob, g),
            (85 + k, "Sustain", Knob, g),
            (91 + k, "Release", Knob, g),
            (97 + k, "Pan", Knob, g),
            (103 + k, "Level", Knob, g),
        ]);
        for s in 0..OSCS {
            v.push((109 + k * OSCS + s, FROM[s], Knob, 10 + k as u8));
        }
        v.push((145 + k, TO[k], Knob, 16));
    }
    for k in 0..OSCS {
        v.push((151 + k, TO[k], Knob, 16));
    }
    for (k, name) in BANDS.iter().enumerate() {
        v.push((228 + k, name, Knob, 17));
    }
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
    if raw.len() < 8 + SHORT || &raw[..4] != b"SSLF" || &raw[9..12] != b"iBT" {
        return None;
    }
    Some(8 + LIST)
}

pub fn values(raw: &[u8]) -> Vec<f32> {
    let Some(at) = base(raw) else { return Vec::new() };
    (0..COUNT).map(|i| f32::from_le_bytes(raw[at + i * 4..at + i * 4 + 4].try_into().unwrap())).map(|v| if v.is_finite() { v } else { 0.0 }).collect()
}

/// A block as a stored sound file carries it becomes a project block by a short head and room for a name.
pub fn wrap(file: &[u8]) -> Vec<u8> {
    let len = file.len().max(FILE);
    let mut v = Vec::with_capacity(len + 8);
    v.extend_from_slice(b"SSLF");
    v.extend_from_slice(&(len as u32).to_le_bytes());
    v.extend_from_slice(file);
    v.resize(len + 8, 0);
    v
}

pub fn fresh() -> Vec<u8> {
    let mut v = vec![0u8; 8 + FILE];
    v[..4].copy_from_slice(b"SSLF");
    v[4..8].copy_from_slice(&(FILE as u32).to_le_bytes());
    v[8..12].copy_from_slice(b"2iBT");
    v[8 + 20..8 + 36].fill(0xff);
    for (i, x) in DEFS.iter().enumerate() {
        v[8 + LIST + i * 4..8 + LIST + i * 4 + 4].copy_from_slice(&x.to_le_bytes());
    }
    v[8 + LIST + COUNT * 4..8 + SHORT].fill(0xff);
    v[8 + SHORT..8 + SHORT + 7].copy_from_slice(b"Default");
    v
}

// ---------------------------------------------------------------------------------------------------------
// Waves

/// The overtone strengths of a built shape. Only the sine, the saw and the squares are known by their
/// place in the list; the others are stand-ins of our own, the same one each time for a given place.
fn shape(i: usize) -> Vec<(f32, f32)> {
    let saw = |h: f32| 2.0 / PI / h * if h as u32 % 2 == 1 { 1.0 } else { -1.0 };
    (1..=256)
        .map(|n: usize| {
            let h = n as f32;
            let odd = n % 2 == 1;
            let s = match i {
                1 => (n == 1) as u32 as f32,
                12 => saw(h),
                8 => if odd { 4.0 / PI / h } else { 0.0 },
                9 => if odd { 4.0 / PI / h / (1.0 + h / 24.0) } else { 0.0 },
                _ => match i % 5 {
                    0 => saw(h) / (1.0 + h / (4.0 + i as f32)),
                    1 => if odd { 8.0 / PI / PI / h / h * if n % 4 == 1 { 1.0 } else { -1.0 } } else { 0.0 },
                    2 => {
                        // a narrow pulse, its width set by the place in the list
                        let w = 0.08 + 0.04 * (i % 7) as f32;
                        2.0 / PI / h * (PI * h * w).sin()
                    }
                    3 => {
                        // a saw with a raised band of overtones
                        let mid = 2.0 + (i % 11) as f32 * 1.5;
                        saw(h) * (0.35 + 1.6 * (-((h - mid) / 2.5).powi(2)).exp())
                    }
                    _ => if n <= 8 && (n.is_power_of_two() || n == 3 + i % 4) { 0.6 / (1.0 + h * 0.25) } else { 0.0 },
                },
            };
            (0.0, s)
        })
        .collect()
}

/// A list of overtone strengths scaled so the wave it gives just reaches full level.
fn levelled(mut parts: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut peak = 0.0f32;
    for i in 0..1024 {
        let x = i as f32 / 1024.0 * 2.0 * PI;
        let y: f32 = parts.iter().enumerate().map(|(h, p)| p.0 * (x * (h + 1) as f32).cos() + p.1 * (x * (h + 1) as f32).sin()).sum();
        peak = peak.max(y.abs());
    }
    if peak > 1e-6 {
        for p in &mut parts {
            p.0 /= peak;
            p.1 /= peak;
        }
    }
    parts
}

struct Store {
    built: Vec<Option<Arc<Wave>>>,
    /// Own waves by a sum of their bytes, the newest last.
    own: Vec<(u64, usize, Arc<Wave>)>,
    next: usize,
}

const OWN_FIRST: usize = 100;
const OWN_MOST: usize = 128;

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store { built: vec![None; WAVES.len()], own: Vec::new(), next: OWN_FIRST }))
}

fn sum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// The number a voice finds the wave of oscillator `k` under (6 and 7 are the slow oscillators).
fn wave_id(raw: &[u8], vals: &[f32], k: usize) -> usize {
    let place = vals[306 + k].round().clamp(0.0, (WAVES.len() - 1) as f32) as usize;
    let at = 8 + OWN + k * 1024;
    if place != 0 || raw.len() < at + 1024 {
        return place.max(1);
    }
    let bytes = &raw[at..at + 1024];
    let key = sum(bytes);
    let mut st = store().lock().unwrap();
    if let Some(found) = st.own.iter().find(|o| o.0 == key) {
        return found.1;
    }
    // 128 strengths, then where in its cycle each overtone starts
    let num: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).map(|a| if a.is_finite() { a.clamp(0.0, 4.0) } else { 0.0 }).collect();
    let parts: Vec<(f32, f32)> = (0..128).map(|h| (num[h] * (num[128 + h] * 2.0 * PI).sin(), num[h] * (num[128 + h] * 2.0 * PI).cos())).collect();
    let id = st.next;
    st.next += 1;
    if st.own.len() >= OWN_MOST {
        st.own.remove(0);
    }
    st.own.push((key, id, Arc::new(Wave::from_parts(&levelled(parts)))));
    id
}

/// The eight waves of a block as numbers to ride along after the list.
pub fn wave_ids(raw: &[u8], vals: &[f32]) -> [f32; 8] {
    if vals.len() < COUNT || base(raw).is_none() {
        return [1.0; 8];
    }
    std::array::from_fn(|k| wave_id(raw, vals, k) as f32)
}

fn wave(id: f32) -> Arc<Wave> {
    let id = if id.is_finite() { id.round().max(0.0) as usize } else { 1 };
    let mut st = store().lock().unwrap();
    if id >= OWN_FIRST {
        if let Some(found) = st.own.iter().find(|o| o.1 == id) {
            return found.2.clone();
        }
    }
    let i = if id >= WAVES.len() || id == 0 { 1 } else { id };
    st.built[i].get_or_insert_with(|| Arc::new(Wave::from_parts(&levelled(shape(i))))).clone()
}

// ---------------------------------------------------------------------------------------------------------
// Settings

#[derive(Debug, Clone)]
struct Osc {
    wave: Arc<Wave>,
    /// Frequency against the note, and a fixed shift in Hz.
    ratio: f32,
    shift: f32,
    restart: bool,
    start: f32,
    strength: f32,
    first: f32,
    env: [f32; 4],
    pan: [f32; 2],
    level: f32,
    /// How far each oscillator bends this one's phase.
    from: [f32; OSCS],
    slow: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct Set {
    osc: Vec<(usize, Osc)>,
    level: f32,
    copies: usize,
    detune: f32,
    spread: f32,
    amp: [f32; 4],
    kind: u8,
    cutoff: f32,
    res: f32,
    key_follow: f32,
    vel_follow: f32,
    feg: [f32; 4],
    feg_amount: f32,
    feg_pitch: f32,
    slow_cut: f32,
    slow_pitch: f32,
    slow: [(Arc<Wave>, f32); 2],
    /// One voice at a time, with this slide.
    pub mono: Option<f32>,
}

fn secs(v: f32) -> f32 {
    0.002 + v * v * v * 8.0
}

pub fn set(p: &[f32]) -> Set {
    let u = |i: usize| p.get(i).copied().filter(|v| v.is_finite()).unwrap_or(DEFS[i]).clamp(0.0, 1.0);
    let whole = |i: usize| p.get(i).copied().filter(|v| v.is_finite()).unwrap_or(DEFS[i]).round().max(0.0) as usize;
    let bi = |i: usize| (u(i) - 0.5) * 2.0;
    let id = |k: usize| p.get(COUNT + k).copied().unwrap_or(1.0);
    let osc = (0..OSCS)
        .filter(|&k| u(25 + k) >= 0.5)
        .map(|k| {
            let pan = bi(97 + k);
            let o = Osc {
                wave: wave(id(k)),
                ratio: 2f32.powf(((bi(31 + k) * 99.0).trunc() + (bi(37 + k) * 99.0).trunc() / 100.0) / 12.0),
                shift: bi(43 + k) * 20.0,
                restart: u(49 + k) >= 0.5,
                start: u(55 + k),
                strength: u(61 + k),
                first: u(67 + k),
                env: [secs(u(73 + k)), secs(u(79 + k)), u(85 + k), secs(u(91 + k))],
                pan: [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)],
                level: u(103 + k),
                from: std::array::from_fn(|s| u(109 + k * OSCS + s).powi(2) * 1.5),
                slow: [bi(145 + k), bi(151 + k)],
            };
            (k, o)
        })
        .collect();
    let hz = |i: usize| 0.05 * 400f32.powf(u(i));
    let one = whole(301) == 1;
    Set {
        osc,
        level: u(0) * u(0) * 4.0,
        copies: whole(302).min(7) + 1,
        detune: u(9) * 50.0,
        spread: u(2),
        amp: [secs(u(3)), secs(u(4)), u(5), secs(u(6))],
        kind: whole(300).min(3) as u8,
        cutoff: u(15),
        res: u(16),
        key_follow: u(17),
        vel_follow: u(18),
        feg: [secs(u(19)), secs(u(20)), u(21), secs(u(22))],
        feg_amount: bi(23),
        feg_pitch: bi(11) * 12.0,
        slow_cut: u(24),
        slow_pitch: u(12).powi(2) * 2.0,
        slow: [(wave(id(6)), hz(13)), (wave(id(7)), hz(14))],
        mono: one.then(|| u(10).powi(2) * 2.0),
    }
}

/// The effects after the voices: the drive and the tone control.
pub fn tail(p: &[f32]) -> tailfx::Set {
    if p.len() < COUNT {
        return Default::default();
    }
    let u = |i: usize| if p[i].is_finite() { p[i].clamp(0.0, 1.0) } else { DEFS[i] };
    let b = |k: usize| (u(228 + k) - 0.5) * 36.0;
    let db = [b(0), (b(1) + b(2)) / 2.0, (b(3) + b(4) + b(5)) / 3.0, b(6), b(7)];
    let rack = tailfx::Rack {
        drive: (u(8) > 0.25).then(|| [(u(8) - 0.2) * 0.75, 16000.0]),
        eq: (u(227) >= 0.5 && db.iter().any(|d| d.abs() > 0.05)).then_some(db),
        ..Default::default()
    };
    tailfx::Set { rack: (rack != tailfx::Rack::default()).then_some(rack), ..Default::default() }
}

// ---------------------------------------------------------------------------------------------------------
// The voice

const EVERY: u32 = 16;
const MOST: usize = 8;

#[derive(Debug, Clone, Copy, Default)]
struct Env {
    v: f32,
    /// 0 rising, 1 falling to the hold level, 2 let go.
    stage: u8,
}

impl Env {
    #[inline]
    fn step(&mut self, e: &[f32; 4], sr: f32) -> f32 {
        match self.stage {
            0 => {
                self.v += 1.0 / (e[0] * sr);
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.stage = 1;
                }
            }
            1 => self.v += (e[2] - self.v) * (5.0 / (e[1] * sr)).min(1.0),
            _ => self.v -= self.v * (5.0 / (e[3] * sr)).min(1.0),
        }
        self.v
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    key: u8,
    vel: f32,
    n: usize,
    ph: [[f32; OSCS]; MOST],
    last: [[f32; OSCS]; MOST],
    tune: [f32; MOST],
    pans: [[f32; 2]; MOST],
    env: [Env; OSCS],
    feg: Env,
    amp: Env,
    fall: f32,
    slow: [f32; 2],
    filt: [[f32; 4]; 2],
    tick: u32,
    g: f32,
    k: f32,
    bend: f32,
    gain: [f32; OSCS],
}

impl Voice {
    pub fn start(set: &Set, sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let mut seed = seed | 1;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32
        };
        let n = set.copies.clamp(1, MOST);
        let mut v = Self {
            sr,
            key,
            vel,
            n,
            ph: [[0.0; OSCS]; MOST],
            last: [[0.0; OSCS]; MOST],
            tune: [1.0; MOST],
            pans: [[1.0; 2]; MOST],
            env: [Env::default(); OSCS],
            feg: Env::default(),
            amp: Env::default(),
            fall: 0.0,
            slow: [0.0; 2],
            filt: [[0.0; 4]; 2],
            tick: 0,
            g: 0.1,
            k: 1.4,
            bend: 1.0,
            gain: [0.0; OSCS],
        };
        for c in 0..n {
            let pos = if n > 1 { c as f32 / (n - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
            v.tune[c] = 2f32.powf(set.detune * pos / 1200.0);
            v.pans[c] = [(1.0 - pos * set.spread).min(1.0), (1.0 + pos * set.spread).min(1.0)];
            for (k, o) in &set.osc {
                v.ph[c][*k] = if o.restart { o.start + c as f32 * 0.618 * (n > 1) as u32 as f32 } else { rnd() }.fract();
            }
        }
        for (k, o) in &set.osc {
            v.env[*k].v = o.first;
        }
        v
    }

    /// Let the note go; gives the seconds it still sounds.
    pub fn release(&mut self, set: &Set) -> f32 {
        if self.amp.stage != 2 {
            self.amp.stage = 2;
            self.feg.stage = 2;
            for e in &mut self.env {
                e.stage = 2;
            }
            self.fall = self.amp.v.max(0.001) / (set.amp[3] * self.sr);
        }
        set.amp[3] + 0.02
    }

    fn control(&mut self, set: &Set, feg: f32) {
        let step = EVERY as f32 / self.sr;
        let mut slow = [0.0f32; 2];
        for (e, (w, hz)) in set.slow.iter().enumerate() {
            self.slow[e] = (self.slow[e] + hz * step).fract();
            slow[e] = w.at(self.slow[e], 0.0).clamp(-1.0, 1.0);
        }
        self.bend = 2f32.powf((set.feg_pitch * feg + set.slow_pitch * slow[1]) / 12.0);
        for (k, o) in &set.osc {
            self.gain[*k] = (1.0 + o.slow[0] * slow[0]).max(0.0) * (1.0 + o.slow[1] * slow[1]).max(0.0) * (1.0 - o.strength + o.strength * self.vel);
        }
        if set.kind != 0 {
            let oct = set.cutoff * 10.0
                + set.key_follow * (self.key as f32 - 60.0) / 12.0
                + set.vel_follow * (self.vel - 0.5) * 4.0
                + set.feg_amount * feg * 10.0
                + set.slow_cut * slow[0] * 3.0;
            let hz = (20.0 * 2f32.powf(oct)).clamp(20.0, self.sr * 0.45);
            self.g = (PI * hz / self.sr).tan();
            self.k = 1.0 / (0.6 + set.res * set.res * 8.0);
        }
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        let feg = self.feg.step(&set.feg, self.sr);
        if self.tick == 0 {
            self.control(set, feg);
        }
        self.tick = (self.tick + 1) % EVERY;
        let amp = if self.amp.stage == 2 {
            self.amp.v = (self.amp.v - self.fall).max(0.0);
            self.amp.v
        } else {
            self.amp.step(&set.amp, self.sr)
        };
        let mut envs = [0.0f32; OSCS];
        for (k, o) in &set.osc {
            envs[*k] = self.env[*k].step(&o.env, self.sr) * self.gain[*k];
        }
        let mut out = [0.0f32; 2];
        for c in 0..self.n {
            let before = self.last[c];
            let d = dphase * self.bend * self.tune[c];
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for (k, o) in &set.osc {
                let k = *k;
                let dp = d * o.ratio + o.shift / self.sr;
                let mut bent = 0.0f32;
                for (s, amt) in o.from.iter().enumerate() {
                    if *amt != 0.0 {
                        bent += amt * before[s];
                    }
                }
                let ph = self.ph[c][k];
                let x = o.wave.at(ph + bent + 8.0, dp) * envs[k];
                self.last[c][k] = if s_self(o, k) { (x + before[k]) * 0.5 } else { x };
                self.ph[c][k] = (ph + dp).rem_euclid(1.0);
                l += x * o.level * o.pan[0];
                r += x * o.level * o.pan[1];
            }
            out[0] += l * self.pans[c][0];
            out[1] += r * self.pans[c][1];
        }
        let trim = set.level * amp * 0.3 / (self.n as f32).sqrt();
        for (side, x) in out.iter_mut().enumerate() {
            let mut y = *x;
            if set.kind != 0 {
                // two stages in a row for a steep slope
                let trim = 1.0 / (1.0 + set.res * set.res * 1.5);
                for stage in 0..2 {
                    let s = &mut self.filt[side][stage * 2..stage * 2 + 2];
                    let k = if stage == 0 { self.k } else { 1.4 };
                    let a1 = 1.0 / (1.0 + self.g * (self.g + k));
                    let a2 = self.g * a1;
                    let v3 = y - s[1];
                    let v1 = a1 * s[0] + a2 * v3;
                    let v2 = s[1] + a2 * s[0] + self.g * a2 * v3;
                    s[0] = 2.0 * v1 - s[0];
                    s[1] = 2.0 * v2 - s[1];
                    y = match set.kind {
                        1 => v2,
                        2 => v1,
                        _ => y - k * v1 - v2,
                    } * if stage == 0 { trim } else { 1.0 };
                    if set.kind == 2 {
                        break;
                    }
                }
            }
            *x = y * trim;
        }
        out
    }
}

/// Whether an oscillator bends its own phase, which is smoothed to keep it from breaking up.
#[inline]
fn s_self(o: &Osc, k: usize) -> bool {
    o.from[k] != 0.0
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

    fn crossings(x: &[[f32; 2]]) -> usize {
        x.windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count()
    }

    #[test]
    fn a_fresh_block_reads_back_and_plays_in_tune() {
        let raw = fresh();
        assert_eq!(raw.len(), 8 + FILE);
        let p = values(&raw);
        assert_eq!(p, defaults());
        let mut full = p.clone();
        full.extend(wave_ids(&raw, &p));
        let out = play(&full, 69, 440.0, 44100, 2000);
        let n = crossings(&out[..44100]);
        assert!((438..=442).contains(&n), "{n}");
        let peak = out[2000..40000].iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.15 && peak < 1.0, "{peak}");
        assert!(out[44100 + 1900][0].abs() < 0.01);
    }

    #[test]
    fn a_stored_sound_file_becomes_a_block() {
        let mut file = fresh()[8..8 + SHORT].to_vec();
        file[LIST..LIST + 4].copy_from_slice(&0.25f32.to_le_bytes());
        let raw = wrap(&file);
        assert_eq!(raw.len(), 8 + FILE);
        assert_eq!(base(&raw), Some(8 + LIST));
        assert_eq!(values(&raw)[0], 0.25);
        assert!(base(&raw[..200]).is_none());
    }

    #[test]
    fn one_oscillator_bends_another() {
        let mut p = defaults();
        p.extend([1.0; 8]);
        let plain = play(&p, 60, 261.6, 8000, 0);
        // the second oscillator, silent itself, an octave up, into the first
        p[26] = 1.0;
        p[32] = 0.5 + 12.0 / 198.0;
        let silent = play(&p, 60, 261.6, 8000, 0);
        p[109 + 1] = 0.8;
        let bent = play(&p, 60, 261.6, 8000, 0);
        let diff = |a: &[[f32; 2]], b: &[[f32; 2]]| a.iter().zip(b).map(|(x, y)| (x[0] - y[0]).abs()).sum::<f32>() / a.len() as f32;
        assert!(diff(&plain, &silent) < 1e-4);
        assert!(diff(&plain, &bent) > 0.02);
        assert!(bent.iter().all(|f| f[0].is_finite() && f[0].abs() < 2.0));
    }

    #[test]
    fn the_filter_and_the_copies_shape_the_sound() {
        let mut p = defaults();
        p.extend([12.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        let rough = |x: &[[f32; 2]]| x.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).sum::<f32>();
        let open = play(&p, 48, 130.8, 20000, 0);
        p[300] = 1.0;
        p[15] = 0.35;
        let shut = play(&p, 48, 130.8, 20000, 0);
        assert!(rough(&shut) < rough(&open) * 0.4, "{} {}", rough(&shut), rough(&open));
        p[302] = 4.0;
        p[9] = 0.5;
        p[2] = 1.0;
        let wide = play(&p, 48, 130.8, 20000, 0);
        let apart = wide.iter().map(|f| (f[0] - f[1]).abs()).sum::<f32>();
        assert!(apart > 10.0, "{apart}");
        // an own wave is found again under the same number
        let mut raw = fresh();
        raw[8 + LIST + 306 * 4..8 + LIST + 307 * 4].copy_from_slice(&0f32.to_le_bytes());
        raw[8 + OWN..8 + OWN + 4].copy_from_slice(&1f32.to_le_bytes());
        raw[8 + OWN + 8..8 + OWN + 12].copy_from_slice(&0.5f32.to_le_bytes());
        let ids = wave_ids(&raw, &values(&raw));
        assert!(ids[0] >= OWN_FIRST as f32 && ids[1] == 1.0);
        assert_eq!(wave_ids(&raw, &values(&raw)), ids);
    }

    #[test]
    fn the_drive_and_tone_control_reach_the_tail() {
        let mut p = defaults();
        assert!(tail(&p).rack.is_none());
        p[8] = 0.7;
        p[227] = 1.0;
        p[228] = 1.0;
        let rack = tail(&p).rack.unwrap();
        assert!(rack.drive.is_some() && rack.eq.unwrap()[0] > 10.0);
    }
}
