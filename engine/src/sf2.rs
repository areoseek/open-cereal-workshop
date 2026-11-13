//! Sound bank files (the public SoundFont 2 layout): a pool of 16-bit recordings, instruments that spread
//! them over keys and velocities, and presets that stack instruments. A preset is read into the same list
//! of zones the other sample-map instruments play from.

use crate::sample::{Library, Sample};
use crate::song::Env;
use crate::zones::{Map, Zone};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// What a sound bank channel's block says: the file and the preset picked from it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Setting {
    pub path: String,
    pub bank: u16,
    pub patch: u16,
}

/// What the channel window needs to know about the bank behind a map.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub setting: Setting,
    pub found: bool,
    /// Bank, patch and name of every preset in the file.
    pub presets: Vec<(u16, u16, String)>,
    /// The channel's block with the setting above written in.
    pub block: Vec<u8>,
}

pub fn is_ident(ident: &str) -> bool {
    let l = ident.to_ascii_lowercase();
    l == "fruity soundfont player" || l == "soundfont player"
}

const PATH_AT: usize = 56;

fn varint(b: &[u8], at: usize) -> Option<(usize, usize)> {
    let (mut n, mut shift, mut i) = (0usize, 0, at);
    loop {
        let c = *b.get(i)?;
        n |= ((c & 0x7f) as usize) << shift;
        i += 1;
        if c & 0x80 == 0 {
            return Some((n, i));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

/// The block is a run of 4-byte numbers (version, a switch, patch, two send levels, bank, then eight
/// overrides that are -1 when unused), the file's stored path with its length in front, and a tail of
/// send and option settings. Which of the leading numbers is the patch and which the bank is taken from
/// the few stored examples, all of which pick the first preset.
pub fn setting(state: &[u8]) -> Option<Setting> {
    let n = |k: usize| state.get(k * 4..k * 4 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    n(0)?;
    let (len, at) = varint(state, PATH_AT)?;
    let path = String::from_utf8_lossy(state.get(at..at + len)?).to_string();
    Some(Setting { path, bank: n(5)?.min(16383) as u16, patch: n(2)?.min(127) as u16 })
}

/// The block with another file or preset written in.
pub fn with(state: &[u8], s: &Setting) -> Vec<u8> {
    let mut head = state.get(..PATH_AT).map(<[u8]>::to_vec).unwrap_or_else(|| {
        let mut h = vec![0xffu8; PATH_AT];
        for (k, v) in [5u32, 1, 0, 128, 128, 0].into_iter().enumerate() {
            h[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        h[44..48].copy_from_slice(&1u32.to_le_bytes());
        h
    });
    head[8..12].copy_from_slice(&(s.patch as u32).to_le_bytes());
    head[20..24].copy_from_slice(&(s.bank as u32).to_le_bytes());
    let tail: Vec<u8> = match varint(state, PATH_AT) {
        Some((len, at)) if at + len <= state.len() => state[at + len..].to_vec(),
        _ => vec![0xff, 0xff, 0xff, 0xff, 0, 0xff, 0xff, 0xff, 0xff, 0, 0, 1, 1, 0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1],
    };
    let mut n = s.path.len();
    loop {
        let c = (n & 0x7f) as u8;
        n >>= 7;
        head.push(if n > 0 { c | 0x80 } else { c });
        if n == 0 {
            break;
        }
    }
    head.extend_from_slice(s.path.as_bytes());
    head.extend(tail);
    head
}

struct Shdr {
    name: String,
    start: usize,
    end: usize,
    loop_start: usize,
    loop_end: usize,
    rate: u32,
    pitch: u8,
    correction: i8,
}

pub struct Bank {
    data: Vec<u8>,
    smpl: (usize, usize),
    /// Name, patch, bank, first bag.
    phdr: Vec<(String, u16, u16, usize)>,
    pbag: Vec<usize>,
    pgen: Vec<(u16, u16)>,
    inst: Vec<usize>,
    ibag: Vec<usize>,
    igen: Vec<(u16, u16)>,
    shdr: Vec<Shdr>,
}

fn name20(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

const G_INSTRUMENT: usize = 41;
const G_KEYS: usize = 43;
const G_VELS: usize = 44;
const G_SAMPLE: usize = 53;
const GENS: usize = 61;

#[derive(Clone, Copy)]
struct Gens {
    v: [i32; GENS],
    key: (u8, u8),
    vel: (u8, u8),
}

impl Gens {
    fn zero() -> Self {
        Self { v: [0; GENS], key: (0, 127), vel: (0, 127) }
    }

    fn defaults() -> Self {
        let mut g = Self::zero();
        g.v[8] = 13500;
        for k in [21, 23, 25, 26, 27, 28, 30, 33, 34, 35, 36, 38] {
            g.v[k] = -12000;
        }
        g.v[46] = -1;
        g.v[47] = -1;
        g.v[56] = 100;
        g.v[58] = -1;
        g
    }

    fn set(&mut self, list: &[(u16, u16)]) {
        for &(op, amount) in list {
            let op = op as usize;
            match op {
                G_KEYS => self.key = ((amount & 0xff) as u8, (amount >> 8) as u8),
                G_VELS => self.vel = ((amount & 0xff) as u8, (amount >> 8) as u8),
                G_INSTRUMENT | G_SAMPLE => self.v[op] = amount as i32,
                _ if op < GENS => self.v[op] = amount as i16 as i32,
                _ => {}
            }
        }
    }
}

fn seconds(timecents: i32) -> f32 {
    if timecents <= -11950 {
        0.0
    } else {
        2f32.powf(timecents.min(8000) as f32 / 1200.0)
    }
}

impl Bank {
    pub fn parse(data: Vec<u8>) -> Option<Self> {
        if data.get(..4)? != b"RIFF" || data.get(8..12)? != b"sfbk" {
            return None;
        }
        let u16at = |b: &[u8], at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
        let u32at = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
        let mut smpl = (0, 0);
        let mut parts: HashMap<[u8; 4], (usize, usize)> = HashMap::new();
        let mut at = 12;
        while at + 8 <= data.len() {
            let n = u32at(&data, at + 4);
            let end = (at + 8).checked_add(n)?.min(data.len());
            if &data[at..at + 4] == b"LIST" && at + 12 <= end {
                let mut sub = at + 12;
                while sub + 8 <= end {
                    let m = u32at(&data, sub + 4);
                    let stop = (sub + 8).checked_add(m)?.min(end);
                    let id: [u8; 4] = data[sub..sub + 4].try_into().unwrap();
                    if &id == b"smpl" {
                        smpl = (sub + 8, stop);
                    } else {
                        parts.insert(id, (sub + 8, stop));
                    }
                    sub = stop + (m & 1);
                }
            }
            at = end + (n & 1);
        }
        let rows = |id: &[u8; 4], size: usize| -> Vec<&[u8]> { parts.get(id).map(|&(a, b)| data[a..b].chunks_exact(size).collect()).unwrap_or_default() };
        let pairs = |id: &[u8; 4]| -> Vec<(u16, u16)> { rows(id, 4).iter().map(|r| (u16at(r, 0), u16at(r, 2))).collect() };
        let phdr = rows(b"phdr", 38).iter().map(|r| (name20(&r[..20]), u16at(r, 20), u16at(r, 22), u16at(r, 24) as usize)).collect::<Vec<_>>();
        let pbag = rows(b"pbag", 4).iter().map(|r| u16at(r, 0) as usize).collect::<Vec<_>>();
        let inst = rows(b"inst", 22).iter().map(|r| u16at(r, 20) as usize).collect::<Vec<_>>();
        let ibag = rows(b"ibag", 4).iter().map(|r| u16at(r, 0) as usize).collect::<Vec<_>>();
        let shdr = rows(b"shdr", 46)
            .iter()
            .map(|r| Shdr { name: name20(&r[..20]), start: u32at(r, 20), end: u32at(r, 24), loop_start: u32at(r, 28), loop_end: u32at(r, 32), rate: u32at(r, 36) as u32, pitch: r[40], correction: r[41] as i8 })
            .collect::<Vec<_>>();
        let (pgen, igen) = (pairs(b"pgen"), pairs(b"igen"));
        if phdr.len() < 2 || shdr.is_empty() {
            return None;
        }
        Some(Self { data, smpl, phdr, pbag, pgen, inst, ibag, igen, shdr })
    }

    /// Bank, patch and name of every preset, in bank then patch order.
    pub fn presets(&self) -> Vec<(u16, u16, String)> {
        let mut v: Vec<_> = self.phdr[..self.phdr.len() - 1].iter().map(|p| (p.2, p.1, p.0.clone())).collect();
        v.sort();
        v
    }

    /// The generator lists of the bags `from..to`.
    fn bags<'a>(bag: &[usize], gens: &'a [(u16, u16)], from: usize, to: usize) -> Vec<&'a [(u16, u16)]> {
        (from..to.min(bag.len().saturating_sub(1))).filter_map(|i| gens.get(bag[i]..bag[i + 1])).collect()
    }

    /// The zones of one preset, their recordings cut out of the pool.
    pub fn zones(&self, bank: u16, patch: u16) -> Vec<Zone> {
        let live = &self.phdr[..self.phdr.len() - 1];
        let Some(pi) = live.iter().position(|p| p.2 == bank && p.1 == patch).or_else(|| (0..live.len()).min_by_key(|&i| (live[i].2, live[i].1))) else { return vec![] };
        let mut cuts: HashMap<(usize, usize, usize), Arc<Sample>> = HashMap::new();
        let mut out = vec![];
        let mut pglobal: &[(u16, u16)] = &[];
        for (zi, pz) in Self::bags(&self.pbag, &self.pgen, self.phdr[pi].3, self.phdr[pi + 1].3).into_iter().enumerate() {
            let Some(&(_, ins)) = pz.iter().find(|g| g.0 as usize == G_INSTRUMENT) else {
                if zi == 0 {
                    pglobal = pz;
                }
                continue;
            };
            let mut p = Gens::zero();
            p.set(pglobal);
            p.set(pz);
            let ins = ins as usize;
            if ins + 1 >= self.inst.len() {
                continue;
            }
            let mut iglobal: &[(u16, u16)] = &[];
            for (k, iz) in Self::bags(&self.ibag, &self.igen, self.inst[ins], self.inst[ins + 1]).into_iter().enumerate() {
                let Some(&(_, sid)) = iz.iter().find(|g| g.0 as usize == G_SAMPLE) else {
                    if k == 0 {
                        iglobal = iz;
                    }
                    continue;
                };
                let mut g = Gens::defaults();
                g.set(iglobal);
                g.set(iz);
                let key = (g.key.0.max(p.key.0), g.key.1.min(p.key.1));
                let vel = (g.vel.0.max(p.vel.0), g.vel.1.min(p.vel.1));
                let Some(h) = self.shdr.get(sid as usize) else { continue };
                if key.0 > key.1 || vel.0 > vel.1 {
                    continue;
                }
                // the preset level shifts everything but the settings that belong to the recording itself
                for op in 0..GENS {
                    if !matches!(op, 0..=4 | 12 | 41 | 43..=47 | 50 | 53 | 54 | 57 | 58) {
                        g.v[op] += p.v[op];
                    }
                }
                let v = &g.v;
                let pool = (self.smpl.1 - self.smpl.0) / 2;
                let at = |base: usize, fine: i32, coarse: i32| (base as i64 + fine as i64 + coarse as i64 * 32768).clamp(0, pool as i64) as usize;
                let (start, end) = (at(h.start, v[0], v[4]), at(h.end, v[1], v[12]));
                let (ls, le) = (at(h.loop_start, v[2], v[45]), at(h.loop_end, v[3], v[50]));
                let looped = matches!(v[54] & 3, 1 | 3) && le > ls && ls >= start;
                let stop = if looped { end.max(le) } else { end };
                if stop <= start || h.rate == 0 {
                    continue;
                }
                let smp = cuts
                    .entry((sid as usize, start, stop))
                    .or_insert_with(|| {
                        let raw = &self.data[self.smpl.0 + start * 2..self.smpl.0 + stop * 2];
                        let mono: Vec<f32> = raw.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
                        Arc::new(Sample::from_mono(&h.name, h.rate, &mono))
                    })
                    .clone();
                let root = if (0..=127).contains(&v[58]) { v[58] } else if h.pitch <= 127 { h.pitch as i32 } else { 60 };
                let mut tune = v[51] as f32 * 100.0 + v[52] as f32 + h.correction as f32;
                let mut track = v[56] != 0;
                if (0..=127).contains(&v[46]) {
                    // a fixed key: the zone always plays at that pitch
                    tune += (v[46] - root) as f32 * v[56] as f32;
                    track = false;
                }
                let sustain = if v[37] >= 1000 { 0.0 } else { 10f32.powf(-(v[37].max(0) as f32) / 200.0) };
                let env = Env { on: true, delay: seconds(v[33]), attack: seconds(v[34]), hold: seconds(v[35]), decay: seconds(v[36]), sustain, release: seconds(v[38]).max(0.005), amount: 0.0, tension: [0.0, -1.0, -1.0] };
                let hz = 8.176 * 2f32.powf(v[8].clamp(1500, 13500) as f32 / 1200.0);
                let q = 10f32.powf(v[9].clamp(0, 960) as f32 / 200.0) * 0.707;
                let menv = |amount: f32| Env { on: true, delay: seconds(v[25]), attack: seconds(v[26]), hold: seconds(v[27]), decay: seconds(v[28]), sustain: 1.0 - (v[29].clamp(0, 1000) as f32) / 1000.0, release: seconds(v[30]).max(0.005), amount, tension: [0.0; 3] };
                const SPAN: f32 = 11958.9; // cents across the cutoff scale
                let rate = |c: i32| 8.176 * 2f32.powf(c.clamp(-16000, 4500) as f32 / 1200.0);
                let lfos = [[seconds(v[21]), rate(v[22]), v[5] as f32, v[10] as f32 / SPAN, v[13] as f32 / 10.0], [seconds(v[23]), rate(v[24]), v[6] as f32, 0.0, 0.0]];
                let lfos = lfos.map(|l| if l[2] == 0.0 && l[3] == 0.0 && l[4] == 0.0 { [0.0; 5] } else { l });
                let moved = v[11] != 0 || v[10] != 0;
                let filter = (hz < 19_000.0 || moved).then(|| (((hz / 20.0).ln() / 1000f32.ln()).clamp(0.0, 1.0), ((q - 0.707) / 10.0).clamp(0.0, 1.0)));
                out.push(Zone {
                    name: h.name.clone(),
                    cut_links: [0.0; 2],
                    key_lo: key.0,
                    key_hi: key.1.min(127),
                    vel_lo: vel.0,
                    vel_hi: vel.1.min(127),
                    root,
                    track,
                    path: String::new(),
                    sample: Some(smp),
                    // the usual reading of the stored level: four tenths of the written figure
                    gain: 10f32.powf(-(v[48].clamp(0, 1440) as f32) * 0.4 / 200.0),
                    pan: (v[17] as f32 / 500.0).clamp(-1.0, 1.0),
                    tune_cents: tune,
                    vel_scale: true,
                    one_shot: false,
                    looped: looped.then_some((ls - start, le - start)),
                    attack: 0.001,
                    release: 0.01,
                    cut: if v[57] > 0 { v[57] } else { -1 },
                    cut_by: if v[57] > 0 { v[57] } else { -1 },
                    out: 0,
                    env: Some(env),
                    filter,
                    pick: 0,
                    penv: (v[7] != 0).then(|| menv(v[7] as f32)),
                    cenv: (v[11] != 0).then(|| menv(v[11] as f32 / SPAN)),
                    lfos,
                });
            }
        }
        out
    }
}

/// Open the bank a sound bank channel names and read the preset it picks.
pub fn open(ident: &str, state: &[u8], lib: Option<&mut Library>, project_dir: Option<&Path>) -> Option<Map> {
    if !is_ident(ident) {
        return None;
    }
    let setting = setting(state).unwrap_or_default();
    let bank = lib.filter(|_| !setting.path.is_empty()).and_then(|l| l.resolve(&setting.path, project_dir)).and_then(|p| std::fs::read(p).ok()).and_then(Bank::parse);
    let block = if state.len() > PATH_AT { state.to_vec() } else { with(state, &setting) };
    Some(match bank {
        Some(b) => Map { zones: b.zones(setting.bank, setting.patch), bank: Some(Info { setting, found: true, presets: b.presets(), block }), kit: None, tail: Default::default() },
        None => Map { zones: vec![], bank: Some(Info { setting, found: false, presets: vec![], block }), kit: None, tail: Default::default() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        if body.len() & 1 == 1 {
            v.push(0);
        }
        v
    }

    fn list(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        for p in parts {
            body.extend(p);
        }
        chunk(b"LIST", &body)
    }

    fn named(name: &str, rest: &[u8]) -> Vec<u8> {
        let mut v = [0u8; 20].to_vec();
        v[..name.len()].copy_from_slice(name.as_bytes());
        v.extend(rest);
        v
    }

    fn gens(list: &[(u16, i32)]) -> Vec<u8> {
        list.iter().flat_map(|&(op, a)| [op.to_le_bytes(), (a as u16).to_le_bytes()].concat()).collect()
    }

    /// A one-preset bank: a 400-sample tone, looped, on keys 40 to 80, with the preset tuning it up an octave.
    fn tiny() -> Vec<u8> {
        let pcm: Vec<u8> = (0..446).flat_map(|i| (((i as f32 * 0.2).sin() * 12000.0) as i16).to_le_bytes()).collect();
        let w16 = |v: &[u16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let phdr = [named("Tone", &[w16(&[7, 0, 0]), vec![0; 12]].concat()), named("EOP", &[w16(&[0, 0, 1]), vec![0; 12]].concat())].concat();
        let inst = [named("Tone", &w16(&[0])), named("EOI", &w16(&[2]))].concat();
        let mut sh = named("tone", &[]);
        for v in [0u32, 400, 100, 300, 22050] {
            sh.extend(v.to_le_bytes());
        }
        sh.extend([60u8, 0, 0, 0, 1, 0]);
        let shdr = [sh, named("EOS", &[0; 26])].concat();
        let body = [
            b"sfbk".to_vec(),
            list(b"sdta", &[chunk(b"smpl", &pcm)]),
            list(
                b"pdta",
                &[
                    chunk(b"phdr", &phdr),
                    chunk(b"pbag", &w16(&[0, 0, 2, 0])),
                    chunk(b"pgen", &gens(&[(51, 12), (41, 0), (0, 0)])),
                    chunk(b"inst", &inst),
                    chunk(b"ibag", &w16(&[0, 0, 1, 0, 8, 0])),
                    chunk(b"igen", &gens(&[(17, -250), (43, 40 | 80 << 8), (54, 1), (37, 200), (6, 50), (24, 0), (11, 2400), (53, 0), (0, 0)])),
                    chunk(b"shdr", &shdr),
                ],
            ),
        ]
        .concat();
        chunk(b"RIFF", &body)
    }

    #[test]
    fn a_small_bank_reads() {
        let b = Bank::parse(tiny()).unwrap();
        assert_eq!(b.presets(), vec![(0, 7, "Tone".to_string())]);
        let z = b.zones(0, 7);
        assert_eq!(z.len(), 1);
        let z = &z[0];
        assert_eq!((z.key_lo, z.key_hi, z.root), (40, 80, 60));
        assert_eq!(z.looped, Some((100, 300)));
        assert_eq!(z.tune_cents, 1200.0);
        // the pan set for the whole instrument reaches the zone
        assert_eq!(z.pan, -0.5);
        assert!((z.env.unwrap().sustain - 0.1).abs() < 1e-4);
        assert_eq!(z.sample.as_ref().unwrap().frames.len(), 400);
        // the vibrato and the filter sweep come through
        assert!((z.lfos[1][1] - 8.176).abs() < 1e-3 && z.lfos[1][2] == 50.0 && z.lfos[0] == [0.0; 5]);
        assert!(z.penv.is_none() && (z.cenv.unwrap().amount - 0.2007).abs() < 1e-3 && z.filter.is_some());
        // an unknown preset falls back to the first one
        assert_eq!(b.zones(3, 3).len(), 1);
    }

    #[test]
    fn the_block_keeps_its_shape() {
        let s = Setting { path: "%FLStudioFactoryData%\\Data\\Patches\\Soundfonts\\A.sf2".into(), bank: 0, patch: 5 };
        let made = with(&[], &s);
        assert_eq!(setting(&made).unwrap(), s);
        let again = with(&made, &Setting { patch: 9, ..s.clone() });
        assert_eq!(again.len(), made.len());
        assert_eq!(setting(&again).unwrap().patch, 9);
    }
}

#[cfg(test)]
mod corpus {
    use super::*;

    /// Reads whatever sound banks the user's own content holds, when it is there.
    #[test]
    fn real_banks_read() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let dir = Path::new(&root).join("Patches/Soundfonts");
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for f in rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sf2"))) {
            let b = Bank::parse(std::fs::read(&f).unwrap()).unwrap();
            let ps = b.presets();
            assert!(!ps.is_empty());
            for (bank, patch, name) in &ps {
                let z = b.zones(*bank, *patch);
                assert!(!z.is_empty(), "{name}");
            }
        }
    }
}
