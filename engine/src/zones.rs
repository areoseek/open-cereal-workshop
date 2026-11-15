//! Sample maps: instruments that are a set of recorded sounds spread over keys and velocities. A drum pad
//! machine is one (each pad a key, each pad's layers a velocity range); a multi-sample player is another
//! (each zone a key range that follows the keyboard). The blocks differ, the playing does not, so both are
//! read into the same list of zones.

use crate::sample::{Library, Sample};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Zone {
    pub name: String,
    pub key_lo: u8,
    pub key_hi: u8,
    pub vel_lo: u8,
    pub vel_hi: u8,
    /// The key the recording plays at its own pitch.
    pub root: i32,
    /// Whether the pitch follows the keyboard (a drum pad does not).
    pub track: bool,
    pub path: String,
    pub sample: Option<Arc<Sample>>,
    pub gain: f32,
    /// how far velocity and key (around middle C) move the filter knob
    pub cut_links: [f32; 2],
    pub pan: f32,
    pub tune_cents: f32,
    /// Whether the note's velocity scales the level.
    pub vel_scale: bool,
    /// Plays through whatever the note length.
    pub one_shot: bool,
    /// Loop points in the sample's own frames.
    pub looped: Option<(usize, usize)>,
    pub attack: f32,
    pub release: f32,
    /// Cut group this zone is in, and the group that stops it; negative for none.
    pub cut: i32,
    pub cut_by: i32,
    /// Mixer track offset from the channel's own track.
    pub out: usize,
    /// The zone's own level envelope, when it brings one.
    pub env: Option<crate::song::Env>,
    /// A low-pass the zone asks for: cutoff on the voice filter's 0..=1 scale, and resonance.
    pub filter: Option<(f32, f32)>,
    /// How a note chooses among the zones it falls in: 0 sounds them all, 1 takes them in turn,
    /// 2 picks one at random, 3 picks one at random but never the same twice running.
    pub pick: u8,
    /// Envelopes that move the pitch (amount in cents) and the filter (amount on the cutoff's 0..=1 scale).
    pub penv: Option<crate::song::Env>,
    pub cenv: Option<crate::song::Env>,
    /// Two slow wobbles: seconds before it starts, cycles per second, then its reach over pitch (cents),
    /// the cutoff (0..=1 scale) and the level (decibels).
    pub lfos: [[f32; 5]; 2],
    /// Cents each key above the root raises the pitch (100 for the usual scale).
    pub key_cents: f32,
    /// Decibels the level wobble may lift the zone above its resting level, when there is a ceiling.
    pub lift_db: Option<f32>,
    /// How the envelopes' hold and decay shrink up the keyboard: time cents per key below 60, for the level
    /// envelope's hold and decay and then the moving envelope's.
    pub key_times: [f32; 4],
    /// A velocity the zone always plays at, whatever the note's.
    pub fix_vel: Option<f32>,
}

impl Default for Zone {
    fn default() -> Self {
        Self { name: String::new(), key_lo: 0, key_hi: 127, vel_lo: 0, vel_hi: 127, root: 60, track: true, path: String::new(), sample: None, gain: 1.0, cut_links: [0.0; 2], pan: 0.0, tune_cents: 0.0, vel_scale: true, pick: 0, one_shot: false, looped: None, attack: 0.001, release: 0.01, cut: -1, cut_by: -1, out: 0, env: None, filter: None, penv: None, cenv: None, lfos: [[0.0; 5]; 2], key_cents: 100.0, lift_db: None, key_times: [0.0; 4], fix_vel: None }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Map {
    pub zones: Vec<Zone>,
    /// The sound bank file behind the zones, when they come from one.
    pub bank: Option<crate::sf2::Info>,
    /// The pad machine's own block behind the zones, when they come from one.
    pub kit: Option<Vec<u8>>,
    /// Effects the instrument runs on the sum of its voices.
    pub tail: crate::tailfx::Set,
}

impl Map {
    /// Read an instrument's block by its file identifier.
    pub fn parse(ident: &str, state: &[u8]) -> Option<Self> {
        if ident.eq_ignore_ascii_case("FPC") {
            match early_pads(state) {
                Some(now) => pads(&now),
                None => pads(state),
            }
        } else if ident.eq_ignore_ascii_case("DirectWave") {
            match early_programs(state) {
                Some(now) => programs(&now),
                None => programs(state),
            }
        } else {
            None
        }
    }

    /// Load every zone's recording. Zones whose file is not found stay silent.
    pub fn load(&mut self, lib: &mut Library, project_dir: Option<&Path>) {
        for z in &mut self.zones {
            z.sample = lib.load_stored(&z.path, project_dir);
        }
    }

    /// The zones a note sounds.
    pub fn hit(&self, key: u8, vel: u8) -> impl Iterator<Item = &Zone> {
        self.zones.iter().filter(move |z| (z.key_lo..=z.key_hi).contains(&key) && (z.vel_lo..=z.vel_hi).contains(&vel))
    }

    /// Whether a sounding note on `old` is stopped by a new note on `new`.
    pub fn cuts(&self, new: u8, old: u8) -> bool {
        let group = |k: u8, f: fn(&Zone) -> i32| self.zones.iter().find(|z| (z.key_lo..=z.key_hi).contains(&k)).map(f).unwrap_or(-1);
        let g = group(new, |z| z.cut);
        g >= 0 && group(old, |z| z.cut_by) == g
    }
}

/// Split a run of tagged pieces: a 4-byte number, an 8-byte length, then that many bytes.
fn pieces(b: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let mut out = vec![];
    let mut at = 0;
    while at + 12 <= b.len() {
        let id = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let n = u64::from_le_bytes(b[at + 4..at + 12].try_into().unwrap()) as usize;
        let body = b.get(at + 12..(at + 12).checked_add(n)?)?;
        out.push((id, body));
        at += 12 + n;
    }
    (at == b.len()).then_some(out)
}

/// Whether a text piece holds one byte per letter, as the early versions wrote it.
fn narrow_text(b: &[u8]) -> bool {
    !b.is_empty() && b.iter().all(|&c| c >= 0x20)
}

fn wide(b: &[u8]) -> String {
    if narrow_text(b) {
        return b.iter().map(|&c| c as char).collect();
    }
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u).trim_end_matches('\0').to_string()
}

/// The pad machine's block: a 4-byte mark, then one piece per pad (number 2). A pad holds its settings
/// (100: level 0..128, pan, switches, key, output offset, cut group, cut-by group), its name (101) and one
/// piece per layer (105), which holds the file's stored path (300) and the layer's settings (301: level
/// 0..127, pan, lowest and highest velocity, a switch, tune).
fn pads(state: &[u8]) -> Option<Map> {
    let top = pieces(state.get(4..)?)?;
    let mut zones = vec![];
    for (id, pad) in top {
        if id != 2 {
            continue;
        }
        let parts = pieces(pad)?;
        let ints = |b: &[u8]| -> Vec<i32> { b.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect() };
        let set = parts.iter().find(|p| p.0 == 100).map(|p| ints(p.1))?;
        if set.len() < 7 || !(0..128).contains(&set[3]) {
            continue;
        }
        let name = parts.iter().find(|p| p.0 == 101).map(|p| wide(p.1)).unwrap_or_default();
        let opt = parts.iter().find(|p| p.0 == 102).map(|p| ints(p.1)).unwrap_or_default().first().copied().unwrap_or(1);
        // the pad's pitch knob: taken as cents, an estimate
        let pitch = set.get(7).copied().unwrap_or(0) as f32;
        for (pid, layer) in &parts {
            if *pid != 105 {
                continue;
            }
            let Some(inner) = pieces(layer) else { continue };
            let path = inner.iter().find(|p| p.0 == 300).map(|p| wide(p.1)).unwrap_or_default();
            let l = inner.iter().find(|p| p.0 == 301).map(|p| ints(p.1)).unwrap_or_default();
            if path.is_empty() || l.len() < 6 {
                continue;
            }
            zones.push(Zone {
                name: name.clone(),
                key_lo: set[3] as u8,
                key_hi: set[3] as u8,
                vel_lo: l[2].clamp(0, 127) as u8,
                vel_hi: l[3].clamp(0, 127) as u8,
                root: set[3],
                track: false,
                path,
                // measured: a layer's level knob follows the fader curve around its resting 99
                gain: (set[0] as f32 / 128.0).clamp(0.0, 1.0) * 0.74 * (l[0].clamp(0, 127) as f32 / 99.0).powf(2.83),
                // measured: the pan knobs reach half way to a side at their ends
                pan: ((set[1] + l[1]) as f32 / 128.0).clamp(-1.0, 1.0),
                // the tune knob's reach is an estimate: an octave either way at its ends
                tune_cents: l[5] as f32 * 12.0 + pitch,
                vel_scale: opt & 1 != 0,
                pick: if opt & 2 != 0 { 1 } else { 0 },
                one_shot: true,
                // a pad starts at once, with no fade in
                attack: 0.0,
                cut: set[5],
                cut_by: set[6],
                out: set[4].clamp(0, 125) as usize,
                ..Default::default()
            });
        }
    }
    Some(Map { zones, bank: None, kit: Some(state.to_vec()), tail: Default::default() })
}

type Parts = Vec<(u32, Vec<u8>)>;

fn own(b: &[u8]) -> Option<Parts> {
    Some(pieces(b)?.into_iter().map(|(i, p)| (i, p.to_vec())).collect())
}

fn join(parts: &Parts) -> Vec<u8> {
    let mut out = vec![];
    for (id, body) in parts {
        out.extend(id.to_le_bytes());
        out.extend((body.len() as u64).to_le_bytes());
        out.extend(body);
    }
    out
}

fn int_at(parts: &Parts, id: u32, at: usize) -> i32 {
    parts.iter().find(|p| p.0 == id).and_then(|p| p.1.get(at * 4..at * 4 + 4)).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
}

fn put_int(parts: &mut Parts, id: u32, at: usize, v: i32) {
    if let Some(b) = parts.iter_mut().find(|p| p.0 == id).and_then(|p| p.1.get_mut(at * 4..at * 4 + 4)) {
        b.copy_from_slice(&v.to_le_bytes());
    }
}

fn to_wide(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// One layer of a pad, held as its pieces: the file's stored path (300) and its settings (301).
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    parts: Parts,
}

impl Layer {
    pub const LEVEL: usize = 0;
    pub const PAN: usize = 1;
    pub const VEL_LO: usize = 2;
    pub const VEL_HI: usize = 3;
    pub const TUNE: usize = 5;

    /// An empty layer as a fresh pad holds one.
    pub fn fresh() -> Self {
        let set: Vec<u8> = [99i32, 0, 0, 127, 1, 0, -1].iter().flat_map(|v| v.to_le_bytes()).collect();
        Self { parts: vec![(300, vec![]), (301, set), (302, vec![])] }
    }

    pub fn path(&self) -> String {
        self.parts.iter().find(|p| p.0 == 300).map(|p| wide(&p.1)).unwrap_or_default()
    }

    pub fn put_path(&mut self, path: &str) {
        if let Some(p) = self.parts.iter_mut().find(|p| p.0 == 300) {
            p.1 = if narrow_text(&p.1) && path.is_ascii() { path.as_bytes().to_vec() } else { to_wide(path) };
        }
    }

    pub fn val(&self, at: usize) -> i32 {
        int_at(&self.parts, 301, at)
    }

    pub fn put(&mut self, at: usize, v: i32) {
        put_int(&mut self.parts, 301, at, v);
    }
}

/// One pad, held as its pieces so that what is not understood goes back as it came.
#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    head: Parts,
    pub layers: Vec<Layer>,
    tail: Parts,
}

impl Pad {
    pub const LEVEL: usize = 0;
    pub const PAN: usize = 1;
    pub const KEY: usize = 3;
    pub const OUT: usize = 4;
    pub const CUT: usize = 5;
    pub const CUT_BY: usize = 6;
    pub const PITCH: usize = 7;

    fn read(b: &[u8]) -> Option<Self> {
        let parts = own(b)?;
        let first = parts.iter().position(|p| p.0 == 105).unwrap_or(parts.len().min(3));
        let n = parts[first..].iter().take_while(|p| p.0 == 105).count();
        if parts[first + n..].iter().any(|p| p.0 == 105) || !parts.iter().any(|p| p.0 == 100 && p.1.len() >= 32) {
            return None;
        }
        let layers = parts[first..first + n].iter().map(|p| own(&p.1).map(|parts| Layer { parts })).collect::<Option<Vec<_>>>()?;
        Some(Self { head: parts[..first].to_vec(), layers, tail: parts[first + n..].to_vec() })
    }

    fn store(&self) -> Vec<u8> {
        let mut all = self.head.clone();
        all.extend(self.layers.iter().map(|l| (105, join(&l.parts))));
        all.extend(self.tail.iter().cloned());
        join(&all)
    }

    pub fn val(&self, at: usize) -> i32 {
        int_at(&self.head, 100, at)
    }

    pub fn put(&mut self, at: usize, v: i32) {
        put_int(&mut self.head, 100, at, v);
    }

    pub fn name(&self) -> String {
        self.head.iter().find(|p| p.0 == 101).map(|p| wide(&p.1)).unwrap_or_default()
    }

    pub fn put_name(&mut self, name: &str) {
        if let Some(p) = self.head.iter_mut().find(|p| p.0 == 101) {
            p.1 = if narrow_text(&p.1) && name.is_ascii() { name.as_bytes().to_vec() } else { to_wide(name) };
        }
    }

    /// Whether the note's velocity scales the pad's level.
    pub fn scale_volume(&self) -> bool {
        int_at(&self.head, 102, 0) & 1 != 0
    }

    pub fn put_scale_volume(&mut self, on: bool) {
        let v = int_at(&self.head, 102, 0);
        put_int(&mut self.head, 102, 0, if on { v | 1 } else { v & !1 });
    }

    /// Whether layers with overlapping velocity ranges take turns instead of sounding together.
    pub fn random_layers(&self) -> bool {
        int_at(&self.head, 102, 0) & 2 != 0
    }

    /// In the stored blocks seen, the third setting carries 8 whenever this switch is on; both are kept in step.
    pub fn put_random_layers(&mut self, on: bool) {
        let (v, s) = (int_at(&self.head, 102, 0), self.val(2));
        put_int(&mut self.head, 102, 0, if on { v | 2 } else { v & !2 });
        self.put(2, if on { s | 8 } else { s & !8 });
    }

    /// Share the velocity range out among the layers, lowest first, with no overlap.
    pub fn spread_even(&mut self) {
        let n = self.layers.len().max(1) as i32;
        for (i, l) in self.layers.iter_mut().enumerate() {
            let i = i as i32;
            l.put(Layer::VEL_LO, i * 128 / n);
            l.put(Layer::VEL_HI, (i + 1) * 128 / n - 1);
        }
    }

    /// Back to an empty pad on the same key.
    pub fn clear(&mut self) {
        let key = self.val(Self::KEY);
        *self = Self::fresh(key);
    }

    fn fresh(key: i32) -> Self {
        let ints = |v: &[i32]| -> Vec<u8> { v.iter().flat_map(|v| v.to_le_bytes()).collect() };
        // a drawn line: kind, sub-kind, point count, a spare, the points, five closing numbers, then four knobs
        let line = |sub: i32, pts: &[(f64, f64, u8)]| -> Vec<u8> {
            let mut b = ints(&[1, sub, pts.len() as i32, 0]);
            for (x, y, mark) in pts {
                b.extend(x.to_le_bytes());
                b.extend(y.to_le_bytes());
                b.extend(0.2f32.to_le_bytes());
                b.extend([0, 0, 0, *mark]);
            }
            b.extend(ints(&[-1, -1, -1, 128, 128, 0, 128]));
            b
        };
        let mut set = [0i32; 16];
        set[0] = 128;
        set[3] = key;
        set[5] = -1;
        set[6] = -1;
        let mut opt = [0i32; 8];
        opt[0] = 1;
        Self {
            head: vec![(100, ints(&set)), (101, vec![]), (102, ints(&opt))],
            layers: vec![Layer::fresh()],
            tail: vec![(103, ints(&[0, 0])), (104, line(3, &[(0.0, 1.0, 0), (1.25, 0.0, 1)])), (103, ints(&[1, 0])), (104, line(3, &[(0.0, 0.5, 0)])), (106, vec![])],
        }
    }
}

/// The pad machine's whole block, pad by pad, so edits can be written back.
#[derive(Debug, Clone, PartialEq)]
pub struct Kit {
    mark: [u8; 4],
    top: Parts,
    pub pads: Vec<Pad>,
}

impl Kit {
    pub const IDENT: &'static str = "FPC";

    /// The keys of a new kit's 32 pads, bank A then bank B.
    const KEYS: [i32; 32] = [37, 36, 42, 54, 40, 38, 46, 44, 48, 47, 45, 43, 49, 55, 51, 53, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75];

    pub fn fresh() -> Self {
        Self { mark: [3, 0xe4, 0x84, 0x1e], top: vec![(1, vec![]), (3, vec![1]), (4, vec![0; 32]), (u32::MAX, vec![])], pads: Self::KEYS.iter().map(|k| Pad::fresh(*k)).collect() }
    }

    /// Read a block. Pads must sit together, as they do in every block seen.
    pub fn parse(state: &[u8]) -> Option<Self> {
        let all = own(state.get(4..)?)?;
        let first = all.iter().position(|p| p.0 == 2)?;
        let n = all[first..].iter().take_while(|p| p.0 == 2).count();
        if all[first + n..].iter().any(|p| p.0 == 2) {
            return None;
        }
        let pads = all[first..first + n].iter().map(|p| Pad::read(&p.1)).collect::<Option<Vec<_>>>()?;
        let mut top = all[..first].to_vec();
        // what follows the pads is kept behind a marker piece that is never written
        top.push((u32::MAX, vec![]));
        top.extend(all[first + n..].iter().cloned());
        Some(Self { mark: state[..4].try_into().unwrap(), top, pads })
    }

    pub fn write(&self) -> Vec<u8> {
        let split = self.top.iter().position(|p| p.0 == u32::MAX).unwrap_or(self.top.len());
        let mut all: Parts = self.top[..split].to_vec();
        all.extend(self.pads.iter().map(|p| (2, p.store())));
        all.extend(self.top.iter().skip(split + 1).cloned());
        let mut out = self.mark.to_vec();
        out.extend(join(&all));
        out
    }
}

/// The multi-sample player's block: a 4-byte number, then pieces; each program is a piece (number 1) whose
/// zones are pieces (number 3). A zone holds its keys (500: root key, lowest key, highest key, lowest and
/// highest velocity, one byte each), its name (501), the file's stored path (502) and the recording's facts
/// (503: frames, a spare, channels, a number, rate as a float, loop kind, loop start, loop end). The first
/// program that has zones is the one that plays.
///
/// The rest of a zone is knobs stored 0 to 1: the level envelope (509: attack, decay, sustain, release), two
/// filters (507: kind, cutoff, emphasis, shape), two free envelopes (514), two slow wobbles (515) and sixteen
/// links (516: source, target, amount with 0.5 as none). Which numbers the sources and targets carry is
/// placed from the links every stored program starts with, and the knob-to-seconds law is our estimate.
fn programs(state: &[u8]) -> Option<Map> {
    let top = pieces(state.get(4..)?)?;
    let mut zones = vec![];
    let mut tail = crate::tailfx::Set::default();
    for (id, prog) in top {
        if id != 1 {
            continue;
        }
        let Some(parts) = pieces(prog) else { continue };
        for (pid, zone) in &parts {
            if *pid != 3 {
                continue;
            }
            let Some(inner) = pieces(zone) else { continue };
            let find = |n: u32| inner.iter().find(|p| p.0 == n).map(|p| p.1);
            let (Some(keys), Some(path)) = (find(500), find(502)) else { continue };
            if keys.len() < 5 || path.is_empty() {
                continue;
            }
            let facts = find(503).unwrap_or(&[]);
            let int = |at: usize| facts.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).unwrap_or(0);
            let looped = (int(20) != 0 && int(28) > int(24)).then(|| (int(24), int(28)));
            let all = |n: u32| inner.iter().filter(move |p| p.0 == n).map(|p| p.1);
            let knobs = |b: &[u8], from: usize| -> Vec<f32> { b.get(from..).unwrap_or(&[]).chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap()).clamp(0.0, 1.0)).collect() };
            // measured on the factory test: times go with the fourth power of the knob and the lines are squared
            let secs = |k: f32| 10.0 * k * k * k * k;
            let adsr = |b: &[u8], amount: f32| {
                let k = knobs(b, 0);
                (k.len() >= 4).then(|| crate::song::Env { on: true, attack: secs(k[0]), decay: secs(k[1]), sustain: k[2], release: secs(k[3]).max(0.005), amount, tension: [0.0, -1.0 / 3.0, -1.0 / 3.0], ..Default::default() })
            };
            let env = find(509).and_then(|b| adsr(b, 0.0));
            // links: velocity (2) to level (2), the free envelopes (9, 10) to pitch (1) or the first cutoff (7)
            let mut vel_scale = all(516).next().is_none();
            let (mut penv, mut cenv) = (None, None);
            let mut cut_links = [0.0f32; 2];
            let free: Vec<&[u8]> = all(514).collect();
            for l in all(516) {
                if l.len() < 8 {
                    continue;
                }
                let (src, dst) = (u16::from_le_bytes([l[0], l[1]]), u16::from_le_bytes([l[2], l[3]]));
                let amount = (f32::from_le_bytes(l[4..8].try_into().unwrap()).clamp(0.0, 1.0) - 0.5) * 2.0;
                if amount == 0.0 {
                    continue;
                }
                match (src, dst) {
                    (2, 2) => vel_scale = amount > 0.0,
                    (2, 7) => cut_links[0] = amount,
                    (1, 7) => cut_links[1] = amount,
                    (9 | 10, 1) => penv = free.get(src as usize - 9).and_then(|b| adsr(b, amount * 1200.0)),
                    (9 | 10, 7) => cenv = free.get(src as usize - 9).and_then(|b| adsr(b, amount)),
                    _ => {}
                }
            }
            // only the low-pass kind (1) is run; emphasis 0.5 is the knob's rest
            let filter = all(507).next().filter(|b| b.len() >= 12 && b[0] == 1).map(|b| {
                let k = knobs(b, 4);
                (k[0], ((k[1] - 0.5) * 2.0).clamp(0.0, 1.0))
            });
            let pick = find(505).and_then(|b| b.get(1).copied()).filter(|&t| t <= 3).unwrap_or(0);
            // the program's echo, chorus and room, each fed by the zone's send knob (half way is a quarter
            // of the level); they run once for the instrument, so the first zone's sends stand for all
            if zones.is_empty() {
                use crate::tailfx::{Chorus, Delay, Reverb};
                let sends = find(506).map(|b| knobs(b, 0)).filter(|k| k.len() >= 3).unwrap_or(vec![0.0; 3]);
                let unit = |n: u32, lead: usize| parts.iter().find(|p| p.0 == n).map(|p| p.1).filter(|b| b.first() == Some(&1)).map(|b| knobs(b, lead)).filter(|k| k.len() >= 4);
                let wet = |i: usize| sends[i] * sends[i];
                tail = crate::tailfx::Set {
                    delay: unit(105, 2).filter(|_| sends[0] > 0.0).map(|k| Delay { free: 0.02 * 50f32.powf(k[0]), fb: k[1] * 0.95, low_hz: 20.0 * 100f32.powf(k[2]), high_hz: 200.0 * 100f32.powf(k[3]), wet: wet(0), ..Default::default() }),
                    chorus: unit(107, 1).filter(|_| sends[1] > 0.0).map(|k| Chorus { depth: k[1], hz: 0.05 * 100f32.powf(k[2]), wet: wet(1) }),
                    reverb: unit(106, 1).filter(|_| sends[2] > 0.0).map(|k| Reverb { secs: 0.2 + 20.0 * k[3] * k[3], high_hz: 1000.0 * 20f32.powf(k[1]), wet: wet(2) }),
                    ..Default::default()
                };
            }
            zones.push(Zone {
                name: find(501).map(|n| String::from_utf8_lossy(n).into_owned()).unwrap_or_default(),
                key_lo: keys[1].min(127),
                key_hi: keys[2].min(127),
                vel_lo: keys[3].min(127),
                vel_hi: keys[4].min(127),
                root: keys[0] as i32,
                path: String::from_utf8_lossy(path).trim_end_matches('\0').to_string(),
                looped,
                release: env.map_or(0.25, |e| e.release),
                env,
                vel_scale,
                // the zone's level knob, squared: half way measured 11.5 dB down on the factory test
                gain: find(504).map(|b| knobs(b, 0)).and_then(|k| k.first().copied()).map_or(1.0, |k| k * k * 1.0614),
                cut_links,
                penv,
                cenv,
                filter,
                pick,
                ..Default::default()
            });
        }
        if !zones.is_empty() {
            break;
        }
    }
    Some(Map { zones, bank: None, kit: None, tail })
}

/// The pad machine's block as versions before the pieces wrote it: the letters JB and a version, the pad
/// count, then each pad in a row: level (0..127), pan, two switches, key, output offset, a byte, cut group,
/// cut-by group, the name (a length byte, then letters), the layer count, each layer (path the same way,
/// then level, pan, lowest and highest velocity, a switch, two bytes, a closing number) and two drawn lines
/// (kind, sub-kind, point count, the points, eight spare bytes, seven closing numbers). Gives the same
/// content laid out as pieces, so one reader serves both.
fn early_pads(state: &[u8]) -> Option<Vec<u8>> {
    if state.get(..2)? != b"JB" {
        return None;
    }
    struct Cur<'a>(&'a [u8], usize);
    impl<'a> Cur<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let b = self.0.get(self.1..self.1.checked_add(n)?)?;
            self.1 += n;
            Some(b)
        }
        fn int(&mut self) -> Option<i32> {
            Some(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
        }
        fn text(&mut self) -> Option<String> {
            let n = self.take(1)?[0] as usize;
            Some(self.take(n)?.iter().map(|&c| c as char).collect())
        }
    }
    fn piece(id: u32, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend((body.len() as u64).to_le_bytes());
        out.extend(body);
        out
    }
    let ints = |v: &[i32]| -> Vec<u8> { v.iter().flat_map(|v| v.to_le_bytes()).collect() };
    let mut c = Cur(state, 4);
    let n = c.int()?;
    if !(1..=128).contains(&n) {
        return None;
    }
    let mut out = vec![3, 0xe4, 0x84, 0x1e];
    out.extend(piece(1, &[]));
    out.extend(piece(3, &[1]));
    out.extend(piece(4, &[0; 32]));
    for _ in 0..n {
        let (level, pan) = (c.int()?, c.int()?);
        let switches = c.take(2)?;
        let (key, out_at) = (c.int()?, c.int()?);
        c.take(1)?;
        let (cut, cut_by) = (c.int()?, c.int()?);
        let name = c.text()?;
        let layers = c.int()?;
        if !(0..128).contains(&key) || !(0..=64).contains(&layers) {
            return None;
        }
        let mut set = [0i32; 16];
        set[0] = (level.clamp(0, 127) * 128 + 63) / 127;
        set[1] = pan;
        set[2] = switches[0] as i32 | (switches[1] as i32) << 8;
        set[3] = key;
        set[4] = out_at;
        set[5] = cut;
        set[6] = cut_by;
        let mut opt = [0i32; 8];
        opt[0] = 1;
        let mut pad = piece(100, &ints(&set));
        pad.extend(piece(101, &to_wide(&name)));
        pad.extend(piece(102, &ints(&opt)));
        for _ in 0..layers {
            let path = c.text()?;
            let l = [c.int()?, c.int()?, c.int()?, c.int()?, c.int()?];
            c.take(2)?;
            let close = c.int()?;
            let mut inner = piece(300, &to_wide(&path));
            inner.extend(piece(301, &ints(&[l[0], l[1], l[2], l[3], l[4], 0, close])));
            inner.extend(piece(302, &[]));
            pad.extend(piece(105, &inner));
        }
        for which in 0..2 {
            let (kind, sub, points) = (c.int()?, c.int()?, c.int()?);
            if kind != 1 || !(0..=4096).contains(&points) {
                return None;
            }
            let mut line = ints(&[kind, sub, points, 0]);
            line.extend(c.take(points as usize * 24)?);
            c.take(8)?;
            line.extend(c.take(28)?);
            pad.extend(piece(103, &ints(&[which, 0])));
            pad.extend(piece(104, &line));
        }
        pad.extend(piece(106, &[]));
        out.extend(piece(2, &pad));
    }
    Some(out)
}

/// The multi-sample player's block as versions before the pieces wrote it: fixed records. After 9992 bytes
/// of general settings come 128 programs of 452 bytes (name 32, file 260, at 297 the switches of the echo,
/// room and chorus, at 304 the zone count, from 388 the three units' knobs), each followed by its zones of
/// 1698 bytes (name 32, path 260, at 292 the keys, at 300 frames, channels, sample width, a spare, rate, at
/// 324 loop kind, start and end, at 346 the two filters, at 394 the level envelope, at 470 the free
/// envelopes, at 502 the wobbles, at 538 the sends, at 1566 the sixteen links), and four closing bytes.
/// Gives the same content laid out as pieces, so one reader serves both.
fn early_programs(state: &[u8]) -> Option<Vec<u8>> {
    const FIRST: usize = 9992;
    const PROG: usize = 452;
    const ZONE: usize = 1698;
    if state.len() < FIRST + 128 * PROG + 4 {
        return None;
    }
    let mut at = FIRST;
    let mut progs = vec![];
    for _ in 0..128 {
        let head = state.get(at..at + PROG)?;
        let n = u32::from_le_bytes(head[304..308].try_into().unwrap()) as usize;
        let zones = state.get(at + PROG..(at + PROG).checked_add(n.checked_mul(ZONE)?)?)?;
        progs.push((head, zones));
        at += PROG + n * ZONE;
    }
    if at + 4 != state.len() {
        return None;
    }
    fn piece(id: u32, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend((body.len() as u64).to_le_bytes());
        out.extend(body);
        out
    }
    let text = |b: &[u8]| -> Vec<u8> { b.iter().copied().take_while(|&c| c != 0).collect() };
    let mut out = state[..4].to_vec();
    for (head, zones) in progs {
        let mut prog = vec![];
        // each unit: its switch, the lead bytes the pieces carry, then four knobs
        for (id, switch, lead, knobs) in [(105u32, 297usize, 1usize, 388usize), (106, 298, 0, 404), (107, 299, 0, 420)] {
            let mut b = vec![head[switch]];
            b.extend(std::iter::repeat_n(0u8, lead));
            b.extend(&head[knobs..knobs + 16]);
            prog.extend(piece(id, &b));
        }
        for z in zones.chunks_exact(ZONE) {
            let mut zone = piece(500, &z[292..297]);
            zone.extend(piece(501, &text(&z[..32])));
            zone.extend(piece(502, &text(&z[32..292])));
            let mut facts = z[300..304].to_vec();
            facts.extend([0u8; 4]);
            facts.extend(&z[304..312]);
            facts.extend(&z[316..320]);
            facts.extend(&z[324..336]);
            zone.extend(piece(503, &facts));
            zone.extend(piece(506, &z[538..586]));
            zone.extend(piece(507, &z[346..366]));
            zone.extend(piece(507, &z[366..386]));
            zone.extend(piece(509, &z[394..410]));
            zone.extend(piece(514, &z[470..486]));
            zone.extend(piece(514, &z[486..502]));
            zone.extend(piece(515, &z[502..522]));
            zone.extend(piece(515, &z[522..542]));
            for l in z[1566..1694].chunks_exact(8) {
                zone.extend(piece(516, l));
            }
            prog.extend(piece(3, &zone));
        }
        out.extend(piece(1, &prog));
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn piece(id: u32, body: &[u8]) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend((body.len() as u64).to_le_bytes());
        v.extend(body);
        v
    }

    fn wide_bytes(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    pub(crate) fn pad_block(pads: &[(i32, i32, i32, &[(&str, i32, i32)])]) -> Vec<u8> {
        let mut b = vec![3, 0xe4, 0x84, 0x1e];
        for (key, cut, by, layers) in pads {
            let mut set = vec![0u8; 64];
            for (i, v) in [128, 0, 0, *key, 0, *cut, *by].iter().enumerate() {
                set[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            let mut pad = piece(100, &set);
            pad.extend(piece(101, &wide_bytes("Pad")));
            for (path, lo, hi) in *layers {
                let mut l = vec![];
                for v in [127, 0, *lo, *hi, 1, 0, -1] {
                    l.extend(v.to_le_bytes());
                }
                let mut inner = piece(300, &wide_bytes(path));
                inner.extend(piece(301, &l));
                inner.extend(piece(302, &[]));
                pad.extend(piece(105, &inner));
            }
            pad.extend(piece(106, &[]));
            b.extend(piece(2, &pad));
        }
        b
    }

    #[test]
    fn program_zones_are_read() {
        let mut facts = vec![];
        for v in [48_000u32, 0, 1, 16] {
            facts.extend(v.to_le_bytes());
        }
        facts.extend(44100f32.to_le_bytes());
        for v in [2u32, 1000, 48_000, 0, 16] {
            facts.extend(v.to_le_bytes());
        }
        let mut zone = piece(500, &[60, 48, 71, 0, 127, 0, 0, 0, 0]);
        zone.extend(piece(501, b"Tone"));
        zone.extend(piece(502, b"%FLStudioFactoryData%\\Data\\tone.wav"));
        zone.extend(piece(503, &facts));
        let floats = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let mut low = vec![1, 0, 0, 0];
        low.extend(floats(&[0.4, 0.75, 0.0, 0.0]));
        zone.extend(piece(507, &low));
        zone.extend(piece(509, &floats(&[0.0, 0.5, 0.8, 0.5])));
        zone.extend(piece(514, &floats(&[0.2, 1.0, 0.5, 0.25])));
        zone.extend(piece(514, &floats(&[0.0, 0.5, 0.5, 0.25])));
        for (src, dst, amount) in [(2u16, 2u16, 1.0f32), (3, 34, 0.75), (12, 1, 0.5), (9, 7, 0.75)] {
            let mut l = src.to_le_bytes().to_vec();
            l.extend(dst.to_le_bytes());
            l.extend(amount.to_le_bytes());
            zone.extend(piece(516, &l));
        }
        zone.extend(piece(506, &floats(&[0.5, 0.0, 0.5, 1.0])));
        let mut prog = piece(100, &[0; 30]);
        let unit = |on: &[u8], k: &[f32]| on.iter().copied().chain(floats(k)).collect::<Vec<u8>>();
        prog.extend(piece(105, &unit(&[0, 0], &[0.75, 0.75, 0.5, 0.75])));
        prog.extend(piece(106, &unit(&[1], &[0.25, 0.5, 0.75, 0.25])));
        prog.extend(piece(107, &unit(&[1], &[0.25, 0.5, 0.25, 0.0])));
        prog.extend(piece(3, &zone));
        let mut b = vec![37, 0, 0, 0];
        b.extend(piece(6, &[0; 16]));
        b.extend(piece(1, &piece(100, &[0; 30])));
        b.extend(piece(1, &prog));
        let m = Map::parse("DirectWave", &b).unwrap();
        assert_eq!(m.zones.len(), 1);
        let z = &m.zones[0];
        assert_eq!((z.root, z.key_lo, z.key_hi, z.track), (60, 48, 71, true));
        assert_eq!(z.looped, Some((1000, 48_000)));
        assert!(z.path.ends_with("tone.wav"));
        assert!(m.hit(72, 100).next().is_none());
        let e = z.env.unwrap();
        assert!((e.sustain - 0.8).abs() < 1e-6 && (e.release - 0.625).abs() < 1e-4 && e.attack == 0.0);
        assert_eq!(z.filter, Some((0.4, 0.5)));
        assert!(z.vel_scale && z.penv.is_none());
        let c = z.cenv.unwrap();
        assert!((c.amount - 0.5).abs() < 1e-6 && (c.attack - 0.016).abs() < 1e-4);
        // the room is on and fed; the echo is switched off and the chorus gets nothing
        let r = m.tail.reverb.unwrap();
        assert!((r.wet - 0.25).abs() < 1e-6 && (r.secs - 1.45).abs() < 1e-4);
        assert!(m.tail.delay.is_none() && m.tail.chorus.is_none());
    }

    #[test]
    fn pads_and_layers_are_read() {
        let b = pad_block(&[(36, -1, -1, &[("a.wav", 0, 63), ("b.wav", 64, 127)]), (42, 0, 0, &[("hat.wav", 0, 127)]), (46, 0, 0, &[("open.wav", 0, 127)])]);
        let m = Map::parse("FPC", &b).unwrap();
        assert_eq!(m.zones.len(), 4);
        let soft: Vec<_> = m.hit(36, 20).map(|z| z.path.as_str()).collect();
        assert_eq!(soft, ["a.wav"]);
        let hard: Vec<_> = m.hit(36, 100).map(|z| z.path.as_str()).collect();
        assert_eq!(hard, ["b.wav"]);
        assert!(m.hit(37, 100).next().is_none());
        assert!(m.cuts(42, 46), "the closed hat stops the open one");
        assert!(!m.cuts(36, 46));
        assert!(!m.zones[0].track);
        assert!(Map::parse("FPC", &b[..b.len() - 3]).is_none(), "a torn block is refused");
    }

    #[test]
    fn a_kit_is_edited_and_written_back() {
        let mut k = Kit::fresh();
        assert_eq!(Kit::parse(&k.write()).as_ref(), Some(&k));
        let p = &mut k.pads[1];
        p.put_name("Kick");
        p.put(Pad::PITCH, 50);
        p.put_scale_volume(false);
        p.put_random_layers(true);
        p.layers[0].put_path("a.wav");
        p.layers.push(Layer::fresh());
        p.layers[1].put_path("b.wav");
        let block = k.write();
        assert_eq!(Kit::parse(&block).unwrap(), k);
        let m = Map::parse("FPC", &block).unwrap();
        assert_eq!(m.kit.as_deref(), Some(&block[..]));
        let hit: Vec<_> = m.hit(36, 100).collect();
        assert_eq!(hit.len(), 2, "both layers cover the note");
        assert!(hit.iter().all(|z| z.pick == 1 && !z.vel_scale && z.name == "Kick" && z.tune_cents == 50.0));
        k.pads[1].spread_even();
        let m = Map::parse("FPC", &k.write()).unwrap();
        assert_eq!(m.hit(36, 100).map(|z| z.path.as_str()).collect::<Vec<_>>(), ["b.wav"]);
        assert_eq!(m.hit(36, 63).map(|z| z.path.as_str()).collect::<Vec<_>>(), ["a.wav"]);
        k.pads[1].clear();
        assert_eq!(k.pads[1], Kit::fresh().pads[1]);
    }

    #[test]
    fn stored_kits_come_back_byte_for_byte() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut seen = 0;
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) && p.to_string_lossy().contains("FPC") {
                    let Ok(pre) = crate::preset::read(&p) else { continue };
                    if !pre.ident.eq_ignore_ascii_case("FPC") {
                        continue;
                    }
                    let kit = Kit::parse(&pre.state).unwrap_or_else(|| panic!("{} is not read", p.display()));
                    assert_eq!(kit.write(), pre.state, "{}", p.display());
                    seen += 1;
                }
            }
        }
        assert!(seen > 10, "only {seen} kits were found");
    }

    /// With the user's own content at hand: every zone program file gives zones with a level envelope, and
    /// its recordings are found.
    #[test]
    fn stored_zone_programs_are_read() {
        let (Ok(root), Ok(factory)) = (std::env::var("CEREAL_FLP_CORPUS"), std::env::var("CEREAL_FACTORY_DATA")) else { return };
        let mut lib = Library::default();
        lib.factory_roots.push(factory.into());
        let (mut seen, mut found, mut zones) = (0, 0, 0);
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwp")) {
                    let pre = crate::preset::read(&p).unwrap();
                    let mut m = Map::parse(&pre.ident, &pre.state).unwrap_or_else(|| panic!("{} is not read", p.display()));
                    assert!(!m.zones.is_empty(), "{}", p.display());
                    assert!(m.zones.iter().all(|z| z.env.is_some_and(|e| e.release > 0.0 && e.release < 2.0) && z.vel_scale), "{}", p.display());
                    if seen % 8 == 0 {
                        m.load(&mut lib, p.parent());
                        found += m.zones.iter().filter(|z| z.sample.is_some()).count();
                        zones += m.zones.len();
                    }
                    seen += 1;
                }
            }
        }
        assert!(seen > 10, "only {seen} programs were found");
        eprintln!("zone programs: {seen}, recordings found {found} of {zones}");
    }
}
