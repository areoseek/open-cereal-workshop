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
}

impl Default for Zone {
    fn default() -> Self {
        Self { name: String::new(), key_lo: 0, key_hi: 127, vel_lo: 0, vel_hi: 127, root: 60, track: true, path: String::new(), sample: None, gain: 1.0, pan: 0.0, tune_cents: 0.0, vel_scale: true, one_shot: false, looped: None, attack: 0.001, release: 0.01, cut: -1, cut_by: -1, out: 0, env: None, filter: None }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Map {
    pub zones: Vec<Zone>,
    /// The sound bank file behind the zones, when they come from one.
    pub bank: Option<crate::sf2::Info>,
}

impl Map {
    /// Read an instrument's block by its file identifier.
    pub fn parse(ident: &str, state: &[u8]) -> Option<Self> {
        if ident.eq_ignore_ascii_case("FPC") {
            pads(state)
        } else if ident.eq_ignore_ascii_case("DirectWave") {
            programs(state)
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

fn wide(b: &[u8]) -> String {
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
                gain: (set[0] as f32 / 128.0).clamp(0.0, 1.0) * (l[0] as f32 / 127.0).clamp(0.0, 1.0),
                pan: ((set[1] + l[1]) as f32 / 64.0).clamp(-1.0, 1.0),
                // the tune knob's reach is an estimate: an octave either way at its ends
                tune_cents: l[5] as f32 * 12.0,
                one_shot: true,
                cut: set[5],
                cut_by: set[6],
                out: set[4].clamp(0, 125) as usize,
                ..Default::default()
            });
        }
    }
    Some(Map { zones, bank: None })
}

/// The multi-sample player's block: a 4-byte number, then pieces; each program is a piece (number 1) whose
/// zones are pieces (number 3). A zone holds its keys (500: root key, lowest key, highest key, lowest and
/// highest velocity, one byte each), its name (501), the file's stored path (502) and the recording's facts
/// (503: frames, a spare, channels, a number, rate as a float, loop kind, loop start, loop end). The first
/// program that has zones is the one that plays.
fn programs(state: &[u8]) -> Option<Map> {
    let top = pieces(state.get(4..)?)?;
    let mut zones = vec![];
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
            zones.push(Zone {
                name: find(501).map(|n| String::from_utf8_lossy(n).into_owned()).unwrap_or_default(),
                key_lo: keys[1].min(127),
                key_hi: keys[2].min(127),
                vel_lo: keys[3].min(127),
                vel_hi: keys[4].min(127),
                root: keys[0] as i32,
                path: String::from_utf8_lossy(path).trim_end_matches('\0').to_string(),
                looped,
                // the zone envelopes are not read yet: a quick start and a short fade stand in
                release: 0.25,
                ..Default::default()
            });
        }
        if !zones.is_empty() {
            break;
        }
    }
    Some(Map { zones, bank: None })
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
        let mut prog = piece(100, &[0; 30]);
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
}
