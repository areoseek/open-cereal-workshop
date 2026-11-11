//! Morph Additive: four generators of 128 sine overtones each, blended along a drawn path.
//!
//! A generator splits the keyboard into zones. A zone holds a row of snapshots in time; each snapshot gives
//! every overtone a strength, a place between left and right and a shift in pitch, and a note plays through
//! the row, sliding from one snapshot to the next. The path says how much of each generator is heard as the
//! note goes on. After the voices come a drive, a chorus, an echo, a room and a tone control.
//!
//! The stored block is a nest of sized pieces. Only the list of 42 numbers near its end is ever rewritten;
//! the snapshots, the path, the four extra envelopes and the table of links are kept byte for byte.

use crate::duo::{Ctl, Info};
use crate::tailfx;
use std::f32::consts::PI;
use std::sync::{Arc, Mutex, OnceLock};

pub const COUNT: usize = 42;
/// Overtones in a generator.
const H: usize = 128;
/// One step of stored time, in seconds (or in hundredths of a beat when a row keeps step with the song).
const STEP: f32 = 0.01;

pub const GROUPS: [&str; 7] = ["Master", "Level envelope", "Chorus", "Echo", "Room", "Tone control", "Voices"];

const DEFS: [f32; COUNT] = [
    0.5, 0.0, 0.0, 31.0, 0.25, 0.5, 0.5, 0.0, 0.17, 0.0, 1.0, 1.0, 0.2, 0.5, 0.5, 0.5, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.5, 0.5, 0.5, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5,
    0.5, 0.5, 0.0, 0.5, 0.5, 0.0, 1.0, 0.5, 0.0,
];

const SLIDES: &[&str] = &["Every note, by distance", "Held notes, by distance", "Every note, set time", "Held notes, set time"];
const BANDS: [&str; 8] = ["Band 1", "Band 2", "Band 3", "Band 4", "Band 5", "Band 6", "Band 7", "Band 8"];

fn placed() -> Vec<(usize, &'static str, Ctl, u8)> {
    use Ctl::*;
    let mut v = vec![
        (0, "Level", Knob, 0),
        (1, "Drive", Knob, 0),
        (8, "Touch", Knob, 0),
        (5, "Transpose", Knob, 0),
        (6, "Fine", Knob, 0),
        (9, "Attack", Knob, 1),
        (10, "Decay", Knob, 1),
        (11, "Sustain", Knob, 1),
        (12, "Release", Knob, 1),
        (16, "On", Switch, 2),
        (13, "Depth", Knob, 2),
        (14, "Rate", Knob, 2),
        (15, "Mix", Knob, 2),
        (22, "On", Switch, 3),
        (17, "Time", Knob, 3),
        (19, "Feedback", Knob, 3),
        (20, "Wobble", Knob, 3),
        (21, "Mix", Knob, 3),
        (26, "On", Switch, 4),
        (23, "Decay", Knob, 4),
        (24, "High cut", Knob, 4),
        (25, "Mix", Knob, 4),
        (35, "On", Switch, 5),
        (3, "Voices", Whole(31), 6),
        (7, "Slide", Knob, 6),
        (2, "Slide kind", Choice(SLIDES), 6),
    ];
    for (k, name) in BANDS.iter().enumerate() {
        v.push((27 + k, name, Knob, 5));
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

// ---------------------------------------------------------------------------------------------------------
// Reading the block

struct Rd<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Rd<'a> {
    fn u(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.b.get(self.o..self.o + 4)?.try_into().ok()?);
        self.o += 4;
        Some(v)
    }
    fn f(&mut self) -> Option<f32> {
        let v = f32::from_bits(self.u()?);
        Some(if v.is_finite() { v } else { 0.0 })
    }
    fn tag(&mut self, t: &[u8; 4]) -> Option<()> {
        (self.b.get(self.o..self.o + 4)? == t).then(|| self.o += 4)
    }
    fn byte(&mut self) -> Option<u8> {
        let v = *self.b.get(self.o)?;
        self.o += 1;
        Some(v)
    }
    /// A plain piece: its size (counting the size itself), then that much.
    fn piece(&mut self) -> Option<&'a [u8]> {
        let n = self.u()? as usize;
        let v = self.b.get(self.o..(self.o + n.checked_sub(4)?))?;
        self.o += n - 4;
        Some(v)
    }
    /// Step over a named piece: its size, then a name and the rest.
    fn skip(&mut self, t: &[u8; 4]) -> Option<()> {
        let n = self.u()? as usize;
        if self.b.get(self.o..self.o + 4)? != t || self.o + n > self.b.len() {
            return None;
        }
        self.o += n;
        Some(())
    }
}

fn num(b: &[u8], i: usize) -> f32 {
    let v = b.get(i * 4..i * 4 + 4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).unwrap_or(0.0);
    if v.is_finite() { v } else { 0.0 }
}

fn whole(b: &[u8], i: usize) -> u32 {
    b.get(i * 4..i * 4 + 4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).unwrap_or(0)
}

/// Where the list of numbers starts in a stored block, when the block is one of ours to read.
pub fn base(raw: &[u8]) -> Option<usize> {
    if raw.len() < 16 || &raw[..4] != b"SSLF" {
        return None;
    }
    let mut r = Rd { b: raw, o: 8 };
    r.tag(b"0roM")?;
    r.tag(b"0RPM")?;
    for _ in 0..4 {
        r.skip(b"0CVM")?;
    }
    r.skip(b"0LMM")?;
    r.skip(b"0DMM")?;
    let n = r.u()? as usize;
    (n >= 4 + COUNT * 4 && r.o + n - 4 <= raw.len()).then_some(r.o)
}

pub fn values(raw: &[u8]) -> Vec<f32> {
    let Some(at) = base(raw) else { return Vec::new() };
    (0..COUNT).map(|i| num(&raw[at..], i)).collect()
}

/// A stored sound file becomes a project block by a short head.
pub fn wrap(file: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(file.len() + 8);
    v.extend_from_slice(b"SSLF");
    v.extend_from_slice(&(file.len() as u32).to_le_bytes());
    v.extend_from_slice(file);
    v
}

/// A block of our own for a new instrument: one zone with a bright, even row of overtones.
pub fn fresh() -> Vec<u8> {
    fn u(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn f(v: &mut Vec<u8>, x: f32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    let generator = |full: bool| {
        let mut shot = b"0PBM".to_vec();
        if full {
            for g in 0..16 {
                shot.push(0xff);
                for k in 0..8 {
                    // the strength heard is the square of the stored one
                    f(&mut shot, (1.0 / (g * 8 + k + 1) as f32).sqrt());
                }
            }
            shot.extend_from_slice(&[0; 32]);
        } else {
            shot.extend_from_slice(&[0; 48]);
        }
        let mut row = b"0PSM".to_vec();
        u(&mut row, 1);
        u(&mut row, 100);
        f(&mut row, 1.0);
        u(&mut row, shot.len() as u32);
        row.extend_from_slice(&shot);
        u(&mut row, 1060);
        for x in [100, 110, 0, 3] {
            u(&mut row, x);
        }
        for x in [0.5, 0.5, 0.5] {
            f(&mut row, x);
        }
        u(&mut row, 0);
        for _ in 0..256 {
            f(&mut row, 1.0);
        }
        let mut zones = b"0HZM".to_vec();
        u(&mut zones, 1);
        u(&mut zones, 128);
        f(&mut zones, 1.0);
        u(&mut zones, row.len() as u32);
        zones.extend_from_slice(&row);
        let mut g = b"0CVM".to_vec();
        u(&mut g, zones.len() as u32);
        g.extend_from_slice(&zones);
        u(&mut g, 28);
        for x in [0.5, 0.0, 0.0, 0.0, 1.0, 1.0] {
            f(&mut g, x);
        }
        g
    };
    let mut v = b"0roM0RPM".to_vec();
    for k in 0..4 {
        let g = generator(k == 0);
        u(&mut v, g.len() as u32);
        v.extend_from_slice(&g);
    }
    // the path: one point, all of the first generator
    let mut path = b"0LMM".to_vec();
    u(&mut path, 1);
    u(&mut path, 12);
    f(&mut path, 0.0);
    f(&mut path, 1.0);
    u(&mut path, 100);
    u(&mut path, 84);
    for x in [100, 110, 3, 0] {
        u(&mut path, x);
    }
    for x in [1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.5] {
        f(&mut path, x);
    }
    for _ in 0..4 {
        u(&mut path, 0);
    }
    for x in [0.5, 0.5, 0.5] {
        f(&mut path, x);
    }
    u(&mut path, 0);
    u(&mut v, path.len() as u32);
    v.extend_from_slice(&path);
    // four flat envelopes and an empty table of links
    let mut extra = b"0DMM".to_vec();
    u(&mut extra, 4);
    for _ in 0..4 {
        u(&mut extra, 64);
        extra.extend_from_slice(b"0GEM");
        u(&mut extra, 1);
        u(&mut extra, 8);
        f(&mut extra, 1.0);
        u(&mut extra, 100);
        u(&mut extra, 44);
        u(&mut extra, 100);
        u(&mut extra, 110);
        u(&mut extra, 0);
        f(&mut extra, 0.0);
        u(&mut extra, 3);
        for x in [0.5, 0.0, 0.5, 0.5, 0.0] {
            f(&mut extra, x);
        }
    }
    u(&mut extra, 148);
    for _ in 0..24 {
        u(&mut extra, 0);
    }
    for _ in 0..12 {
        f(&mut extra, 0.5);
    }
    u(&mut v, extra.len() as u32);
    v.extend_from_slice(&extra);
    u(&mut v, 1476);
    for x in DEFS {
        f(&mut v, x);
    }
    v.resize(v.len() + 1472 - COUNT * 4, 0);
    wrap(&v)
}

// ---------------------------------------------------------------------------------------------------------
// The snapshots and the path

/// How a row is played through: its length, the loop inside it and how fast it goes.
#[derive(Debug, Clone, Copy, Default)]
struct Run {
    len: f32,
    from: f32,
    to: f32,
    /// 0 back and forth, 1 forward, 2 backward, 3 none.
    mode: u8,
    /// Keeps step with the song.
    sync: bool,
    /// Letting the note go leaves the loop.
    out: bool,
    rate: f32,
    key: f32,
    vel: f32,
}

impl Run {
    /// Stored steps gone by in a second, for this note.
    fn speed(&self, key: u8, vel: f32) -> f32 {
        let per = if self.sync { crate::minis::tempo() / 60.0 * 100.0 } else { 1.0 / STEP };
        per * 4f32.powf(self.rate) * 2f32.powf(self.key * (key as f32 - 60.0) / 12.0 + self.vel * (vel - 0.5) * 2.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Clock {
    pos: f32,
    back: bool,
    free: bool,
}

impl Clock {
    fn step(&mut self, d: f32, r: &Run) {
        if r.mode >= 3 || self.free || r.to <= r.from {
            self.pos = (self.pos + d).min(r.len);
            return;
        }
        if self.back {
            self.pos -= d;
            if self.pos <= r.from {
                if r.mode == 0 {
                    self.pos = r.from;
                    self.back = false;
                } else {
                    self.pos = r.to;
                }
            }
        } else {
            self.pos += d;
            if self.pos >= r.to {
                if r.mode == 1 {
                    self.pos = r.from + (self.pos - r.to) % (r.to - r.from);
                } else {
                    self.pos = r.to;
                    self.back = true;
                }
            }
        }
    }
}

struct Shot {
    at: f32,
    level: f32,
    /// Strength, place (-1 left to 1 right) and shift (1 is a whole overtone up) of each overtone.
    amp: [f32; H],
    pan: [f32; H],
    det: [f32; H],
}

struct Zone {
    /// The first key above the zone.
    top: u32,
    level: f32,
    shots: Vec<Shot>,
    run: Run,
    /// How far each overtone answers to how hard the key is struck, and a fixed curve over them.
    touch: [f32; H],
    window: [f32; H],
    /// Overtones up to the highest one that sounds.
    used: usize,
    bent: bool,
}

struct Path {
    /// When, then across (0 is A and D, 1 is B and C) and up (1 is A and B, 0 is D and C).
    points: Vec<[f32; 3]>,
    run: Run,
    /// Level of each generator (none when it is switched out).
    mix: [f32; 4],
    pan: [f32; 4],
}

pub struct Body {
    gens: [Vec<Zone>; 4],
    path: Path,
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Body({:?} zones)", self.gens.iter().map(|g| g.len()).collect::<Vec<_>>())
    }
}

fn section(r: &mut Rd, out: &mut [f32; H], lo: f32) -> Option<()> {
    for g in 0..16 {
        let mask = r.byte()?;
        for bit in 0..8 {
            if mask >> bit & 1 == 1 {
                out[g * 8 + bit] = r.f()?.clamp(lo, 1.0);
            }
        }
    }
    Some(())
}

fn zone(r: &mut Rd, top: u32) -> Option<Zone> {
    let level = r.f()?.clamp(0.0, 1.0);
    let size = r.u()? as usize;
    let end = r.o.checked_add(size)?;
    r.tag(b"0PSM")?;
    let n = r.u()? as usize;
    if n > r.b.len() / 60 {
        return None;
    }
    let mut shots = Vec::with_capacity(n);
    let mut at = 0.0;
    let (mut used, mut bent) = (0, false);
    for _ in 0..n {
        let len = r.u()? as f32;
        let lvl = r.f()?.clamp(0.0, 1.0);
        let size = r.u()? as usize;
        let stop = r.o.checked_add(size)?;
        r.tag(b"0PBM")?;
        let mut s = Shot { at, level: lvl * lvl, amp: [0.0; H], pan: [0.0; H], det: [0.0; H] };
        section(r, &mut s.amp, 0.0)?;
        section(r, &mut s.pan, -1.0)?;
        section(r, &mut s.det, -1.0)?;
        if r.o != stop {
            return None;
        }
        for h in 0..H {
            s.amp[h] *= s.amp[h];
            if s.amp[h] > 1e-6 {
                used = used.max(h + 1);
                bent |= s.det[h] != 0.0;
            }
        }
        shots.push(s);
        at += len;
    }
    let tail = r.piece()?;
    if r.o != end || tail.len() < 32 + H * 8 {
        return None;
    }
    let knob = |i: usize| ((num(tail, i) - 0.5) * 2.0).clamp(-1.0, 1.0);
    let run = Run { len: at, from: whole(tail, 0) as f32, to: whole(tail, 1) as f32, sync: whole(tail, 2) != 0, mode: whole(tail, 3).min(3) as u8, rate: knob(4), key: knob(5), vel: knob(6), out: whole(tail, 7) != 0 };
    Some(Zone { top, level: level * level, shots, run, touch: std::array::from_fn(|h| num(tail, 8 + h).clamp(0.0, 1.0)), window: std::array::from_fn(|h| num(tail, 8 + H + h).clamp(0.0, 1.0)), used, bent })
}

fn parse(raw: &[u8]) -> Option<Body> {
    let mut r = Rd { b: raw, o: 8 };
    r.tag(b"0roM")?;
    r.tag(b"0RPM")?;
    let mut gens: [Vec<Zone>; 4] = Default::default();
    for g in &mut gens {
        let size = r.u()? as usize;
        let end = r.o.checked_add(size)?;
        r.tag(b"0CVM")?;
        let inner = r.u()? as usize;
        let stop = r.o.checked_add(inner)?;
        r.tag(b"0HZM")?;
        let n = r.u()?;
        if n > 128 {
            return None;
        }
        let mut top = 0;
        for _ in 0..n {
            top += r.u()?.min(128);
            g.push(zone(&mut r, top)?);
        }
        if r.o != stop {
            return None;
        }
        r.piece()?;
        if r.o != end {
            return None;
        }
    }
    let size = r.u()? as usize;
    let end = r.o.checked_add(size)?;
    r.tag(b"0LMM")?;
    let n = r.u()? as usize;
    let each = r.u()? as usize;
    if each != 12 || n > raw.len() / 12 {
        return None;
    }
    let mut points = Vec::with_capacity(n);
    let mut at = 0.0;
    for _ in 0..n {
        let (x, y) = (r.f()?.clamp(0.0, 1.0), r.f()?.clamp(0.0, 1.0));
        points.push([at, x, y]);
        at += r.u()? as f32;
    }
    let tail = r.piece()?;
    if r.o != end || tail.len() < 76 {
        return None;
    }
    // an older form of the piece carries three switches, not four
    let full = tail.len() >= 80;
    let rate = if full { 16 } else { 15 };
    let knob = |i: usize| ((num(tail, i) - 0.5) * 2.0).clamp(-1.0, 1.0);
    let run = Run { len: at, from: whole(tail, 0) as f32, to: whole(tail, 1) as f32, mode: whole(tail, 2).min(3) as u8, sync: whole(tail, 3) != 0, out: false, rate: knob(rate), key: knob(rate + 1), vel: knob(rate + 2) };
    let path = Path { points, run, mix: std::array::from_fn(|k| if full && whole(tail, 12 + k) != 0 { 0.0 } else { num(tail, 4 + k).clamp(0.0, 1.0) }), pan: std::array::from_fn(|k| ((num(tail, 8 + k) - 0.5) * 2.0).clamp(-1.0, 1.0)) };
    Some(Body { gens, path })
}

fn sum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

const KEPT: usize = 48;

struct Store {
    held: Vec<(u64, u32, Option<Arc<Body>>)>,
    next: u32,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store { held: Vec::new(), next: 1 }))
}

/// The number a voice finds the snapshots and the path of this block under (0 when it cannot be read).
pub fn body_id(raw: &[u8]) -> f32 {
    let Some(at) = base(raw) else { return 0.0 };
    let bytes = &raw[8..at - 4];
    let key = sum(bytes);
    let mut st = store().lock().unwrap();
    if let Some(found) = st.held.iter().find(|o| o.0 == key) {
        return found.1 as f32;
    }
    let body = parse(raw).map(Arc::new);
    let id = st.next;
    st.next += 1;
    if st.held.len() >= KEPT {
        st.held.remove(0);
    }
    st.held.push((key, id, body));
    id as f32
}

fn body(id: f32) -> Option<Arc<Body>> {
    let id = if id.is_finite() && id >= 1.0 { id.round() as u32 } else { return None };
    store().lock().unwrap().held.iter().find(|o| o.1 == id).and_then(|o| o.2.clone())
}

// ---------------------------------------------------------------------------------------------------------
// Settings

#[derive(Debug, Clone)]
pub struct Set {
    body: Option<Arc<Body>>,
    level: f32,
    /// How much a soft touch takes away, 0 to 1.
    touch: f32,
    /// Frequency against the note.
    tune: f32,
    amp: [f32; 4],
    /// One voice at a time, with this slide.
    pub mono: Option<f32>,
}

fn secs(v: f32) -> f32 {
    0.002 + v * v * v * 8.0
}

fn unit(p: &[f32], i: usize) -> f32 {
    p.get(i).copied().filter(|v| v.is_finite()).unwrap_or(DEFS[i]).clamp(0.0, 1.0)
}

/// `p` is the list of numbers, then the number of the snapshots (see `body_id`).
pub fn set(p: &[f32]) -> Set {
    let u = |i: usize| unit(p, i);
    let semis = ((u(5) - 0.5) * 48.0).round() + (u(6) - 0.5) * 2.0;
    let voices = p.get(3).copied().filter(|v| v.is_finite()).unwrap_or(DEFS[3]).round();
    Set {
        body: p.get(COUNT).and_then(|&id| body(id)),
        level: (u(0) * 2.0).powi(2),
        touch: (u(8) * 3.0).min(1.0),
        tune: 2f32.powf(semis / 12.0),
        amp: [secs(u(9)), secs(u(10)), u(11) * u(11), secs(u(12))],
        mono: (voices < 0.5).then(|| u(7).powi(2) * 2.0),
    }
}

/// The effects after the voices.
pub fn tail(p: &[f32]) -> tailfx::Set {
    if p.len() < COUNT {
        return Default::default();
    }
    let u = |i: usize| unit(p, i);
    let on = |i: usize| p[i] >= 0.5;
    let b = |k: usize| (u(27 + k) - 0.5) * 24.0;
    let db = [b(0), (b(1) + b(2)) / 2.0, (b(3) + b(4) + b(5)) / 3.0, b(6), b(7)];
    let rack = tailfx::Rack { drive: (u(1) > 0.02).then(|| [u(1) * 0.8, 16000.0]), eq: (on(35) && db.iter().any(|d| d.abs() > 0.05)).then_some(db), ..Default::default() };
    tailfx::Set {
        rack: (rack != tailfx::Rack::default()).then_some(rack),
        chorus: (on(16) && u(15) > 0.0).then(|| tailfx::Chorus { depth: u(13), hz: 0.05 * 100f32.powf(u(14)), wet: u(15) * 0.5 }),
        delay: (on(22) && u(21) > 0.0).then(|| tailfx::Delay { left: 0.0, right: 0.0, fb: u(19) * 0.9, wobble: u(20), low_hz: 0.0, high_hz: 9000.0, wet: u(21) * 0.6, free: 0.002 + u(17) * 0.997 }),
        reverb: (on(26) && u(25) > 0.0).then(|| tailfx::Reverb { secs: 0.3 + u(23) * u(23) * 8.0, high_hz: 800.0 * 20f32.powf(u(24)), wet: u(25) * 0.6 }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------------------
// The voice

const BLOCK: usize = 32;
/// Level of one full-strength overtone.
const TRIM: f32 = 0.22;

/// One generator sounding: a turning pointer for every overtone.
#[derive(Debug, Clone)]
struct Bank {
    gen: usize,
    zone: usize,
    clock: Clock,
    re: [f32; H],
    im: [f32; H],
    cos: [f32; H],
    sin: [f32; H],
    l: [f32; H],
    r: [f32; H],
    dl: [f32; H],
    dr: [f32; H],
    /// The pitch the turns were worked out for.
    turned: f32,
    n: usize,
    new: bool,
}

#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    key: u8,
    vel: f32,
    banks: Vec<Bank>,
    path: Clock,
    out: [[f32; 2]; BLOCK],
    at: usize,
    env: f32,
    /// 0 rising, 1 falling to the hold level, 2 let go.
    stage: u8,
    fall: f32,
}

impl Voice {
    pub fn start(set: &Set, sr: f32, key: u8, vel: f32, _seed: u32) -> Self {
        let mut banks = Vec::new();
        if let Some(body) = &set.body {
            for (gen, zones) in body.gens.iter().enumerate() {
                let Some(zone) = zones.iter().position(|z| (key as u32) < z.top).or(zones.len().checked_sub(1)) else { continue };
                if zones[zone].used == 0 || zones[zone].shots.is_empty() {
                    continue;
                }
                banks.push(Bank {
                    gen,
                    zone,
                    clock: Clock::default(),
                    re: [1.0; H],
                    im: [0.0; H],
                    cos: [1.0; H],
                    sin: [0.0; H],
                    l: [0.0; H],
                    r: [0.0; H],
                    dl: [0.0; H],
                    dr: [0.0; H],
                    turned: -1.0,
                    n: 0,
                    new: true,
                });
            }
        }
        Self { sr, key, vel: vel.clamp(0.0, 1.0), banks, path: Clock::default(), out: [[0.0; 2]; BLOCK], at: BLOCK, env: 0.0, stage: 0, fall: 0.0 }
    }

    /// Let the note go; gives the seconds it still sounds.
    pub fn release(&mut self, set: &Set) -> f32 {
        if self.stage != 2 {
            self.stage = 2;
            self.fall = self.env.max(0.001) / (set.amp[3] * self.sr);
            if let Some(body) = &set.body {
                for b in &mut self.banks {
                    if body.gens[b.gen][b.zone].run.out {
                        b.clock.free = true;
                        b.clock.back = false;
                    }
                }
            }
        }
        set.amp[3] + 0.02
    }

    fn render(&mut self, set: &Set, dphase: f32) {
        self.out = [[0.0; 2]; BLOCK];
        let Some(body) = &set.body else { return };
        let secs = BLOCK as f32 / self.sr;
        // where the path stands: how much of each generator
        let p = &body.path;
        let (x, y) = match p.points.partition_point(|q| q[0] <= self.path.pos) {
            0 => p.points.first().map(|q| (q[1], q[2])).unwrap_or((0.0, 1.0)),
            i if i == p.points.len() => (p.points[i - 1][1], p.points[i - 1][2]),
            i => {
                let (a, b) = (p.points[i - 1], p.points[i]);
                let t = (self.path.pos - a[0]) / (b[0] - a[0]).max(1e-6);
                (a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)
            }
        };
        self.path.step(secs * p.run.speed(self.key, self.vel), &p.run);
        let share = [(1.0 - x) * y, x * y, x * (1.0 - y), (1.0 - x) * (1.0 - y)];
        let f = (dphase * set.tune).max(1e-6);
        let soft = set.touch * (1.0 - self.vel);
        for b in &mut self.banks {
            let z = &body.gens[b.gen][b.zone];
            let gain = share[b.gen] * p.mix[b.gen] * z.level * TRIM;
            let pos = b.clock.pos;
            b.clock.step(secs * z.run.speed(self.key, self.vel), &z.run);
            let i = z.shots.partition_point(|s| s.at <= pos).max(1) - 1;
            let a = &z.shots[i];
            let (c, t) = match z.shots.get(i + 1) {
                Some(c) => (c, ((pos - a.at) / (c.at - a.at).max(1e-6)).clamp(0.0, 1.0)),
                None => (a, 0.0),
            };
            let gain = gain * (a.level + (c.level - a.level) * t);
            // overtones above half the sample rate are left out
            let n = z.used.min((0.49 / f) as usize);
            if gain <= 1e-6 && b.l[..b.n].iter().chain(&b.r[..b.n]).all(|v| *v == 0.0) {
                b.n = n;
                b.new = false;
                continue;
            }
            if z.bent {
                for h in 0..n {
                    let d = a.det[h] + (c.det[h] - a.det[h]) * t;
                    let w = 2.0 * PI * (f * (h as f32 + 1.0 + d)).clamp(0.0, 0.49);
                    (b.sin[h], b.cos[h]) = w.sin_cos();
                }
            } else if b.turned != f || b.n < n {
                let (s1, c1) = (2.0 * PI * f).sin_cos();
                let (mut s, mut c) = (s1, c1);
                for h in 0..n {
                    // every eighth one is worked out afresh so the small errors do not pile up
                    if h % 8 == 0 {
                        (s, c) = (2.0 * PI * f * (h as f32 + 1.0)).sin_cos();
                    }
                    b.sin[h] = s;
                    b.cos[h] = c;
                    (s, c) = (s * c1 + c * s1, c * c1 - s * s1);
                }
                b.turned = f;
            }
            let gpan = p.pan[b.gen];
            for h in 0..H {
                let (mut l, mut r) = (0.0, 0.0);
                if h < n {
                    let amp = (a.amp[h] + (c.amp[h] - a.amp[h]) * t) * z.window[h] * (1.0 - z.touch[h] * soft) * gain;
                    let pan = (a.pan[h] + (c.pan[h] - a.pan[h]) * t + gpan).clamp(-1.0, 1.0);
                    l = amp * (1.0 - pan).min(1.0);
                    r = amp * (1.0 + pan).min(1.0);
                }
                if b.new {
                    b.l[h] = l;
                    b.r[h] = r;
                }
                b.dl[h] = (l - b.l[h]) / BLOCK as f32;
                b.dr[h] = (r - b.r[h]) / BLOCK as f32;
            }
            b.new = false;
            // the pointers that sounded last time fade out even when they are now too high
            let live = n.max(b.n).div_ceil(8) * 8;
            b.n = n;
            for frame in &mut self.out {
                let (mut sl, mut sr) = ([0.0f32; 8], [0.0f32; 8]);
                for h in (0..live).step_by(8) {
                    for k in 0..8 {
                        let j = h + k;
                        let re = b.re[j] * b.cos[j] - b.im[j] * b.sin[j];
                        let im = b.re[j] * b.sin[j] + b.im[j] * b.cos[j];
                        b.re[j] = re;
                        b.im[j] = im;
                        b.l[j] += b.dl[j];
                        b.r[j] += b.dr[j];
                        sl[k] += im * b.l[j];
                        sr[k] += im * b.r[j];
                    }
                }
                frame[0] += sl.iter().sum::<f32>();
                frame[1] += sr.iter().sum::<f32>();
            }
            // keep the pointers at length one
            for h in 0..live {
                let m = 1.5 - 0.5 * (b.re[h] * b.re[h] + b.im[h] * b.im[h]);
                b.re[h] *= m;
                b.im[h] *= m;
            }
        }
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        if self.at >= BLOCK {
            self.render(set, dphase);
            self.at = 0;
        }
        let e = &set.amp;
        match self.stage {
            0 => {
                self.env += 1.0 / (e[0] * self.sr);
                if self.env >= 1.0 {
                    self.env = 1.0;
                    self.stage = 1;
                }
            }
            1 => self.env += (e[2] - self.env) * (5.0 / (e[1] * self.sr)).min(1.0),
            _ => self.env = (self.env - self.fall).max(0.0),
        }
        let o = self.out[self.at];
        self.at += 1;
        let g = self.env * set.level;
        [o[0] * g, o[1] * g]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(raw: &[u8], key: u8, secs: f32) -> Vec<[f32; 2]> {
        let mut p = values(raw);
        p.push(body_id(raw));
        let s = set(&p);
        let mut v = Voice::start(&s, 44100.0, key, 0.8, 1);
        let dp = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0) / 44100.0;
        (0..(secs * 44100.0) as usize).map(|_| v.frame(&s, dp)).collect()
    }

    #[test]
    fn a_new_block_reads_back_and_sounds() {
        let raw = fresh();
        assert_eq!(raw.len(), 8 + 6796 + 512);
        assert_eq!(values(&raw), DEFS.to_vec());
        let out = play(&raw, 57, 0.3);
        let peak = out.iter().fold(0.0f32, |m, f| m.max(f[0].abs()));
        assert!(peak > 0.1 && peak < 1.5, "{peak}");
        assert!(out.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    }

    #[test]
    fn the_pitch_is_the_note() {
        let out = play(&fresh(), 57, 0.5);
        // 220 Hz: count the rising crossings of the second quarter second
        let half = &out[11025..];
        let ups = half.windows(2).filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0).count();
        assert!((54..=56).contains(&ups), "{ups}");
    }

    #[test]
    fn high_notes_drop_the_overtones_that_would_fold() {
        let out = play(&fresh(), 120, 0.1);
        assert!(out.iter().all(|f| f[0].abs() < 2.0));
    }

    #[test]
    fn a_loop_goes_round_and_a_plain_row_stops_at_its_end() {
        let r = Run { len: 100.0, from: 20.0, to: 40.0, mode: 1, ..Default::default() };
        let mut c = Clock::default();
        for _ in 0..1000 {
            c.step(1.5, &r);
            assert!(c.pos <= 40.0);
        }
        assert!(c.pos >= 20.0);
        c.free = true;
        for _ in 0..100 {
            c.step(1.5, &r);
        }
        assert_eq!(c.pos, 100.0);
        let mut c = Clock::default();
        let r = Run { mode: 0, ..r };
        let mut low = 100.0f32;
        for i in 0..1000 {
            c.step(1.5, &r);
            if i > 40 {
                low = low.min(c.pos);
            }
        }
        assert_eq!(low, 20.0);
    }

    #[test]
    fn a_broken_block_is_left_alone() {
        let mut raw = fresh();
        assert!(base(&raw[..200]).is_none());
        raw[30] = 0xff;
        raw[31] = 0xff;
        assert_eq!(body_id(&raw[..400]), 0.0);
        let p = defaults();
        let s = set(&p);
        assert!(s.body.is_none());
    }
}
