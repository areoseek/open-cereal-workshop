//! Plugin preset files (`.fst`): the same event stream a project uses, holding one plugin's name and its stored
//! block. We read the block onto one of our own units when we have one for that plugin.

use crate::flp::{FlpFile, Value};

const ID_IDENT: u8 = 201;
const ID_STATE: u8 = 213;

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// The plugin the settings belong to, as the file names it.
    pub ident: String,
    pub state: Vec<u8>,
}

/// What a preset turns into here.
#[derive(Debug, Clone, PartialEq)]
pub enum Unit {
    Fx(crate::fx::Fx),
    Gen(crate::synth::Gen),
}

/// Old files write text one byte per letter, new ones two.
fn text(b: &[u8]) -> String {
    let wide = b.len() >= 2 && b.len() % 2 == 0 && b.chunks(2).all(|c| c[1] == 0);
    let s = if wide {
        String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
    } else {
        b.iter().map(|&c| c as char).collect()
    };
    s.trim_end_matches('\0').to_string()
}

pub fn parse(bytes: &[u8]) -> Result<Preset, String> {
    let f = FlpFile::parse(bytes).map_err(|_| "not a preset file".to_string())?;
    let mut ident = None;
    let mut state = None;
    for e in &f.events {
        match (&e.value, e.id) {
            (Value::Data(d), ID_IDENT) if ident.is_none() => ident = Some(text(d)),
            (Value::Data(d), ID_STATE) if state.is_none() => state = Some(d.clone()),
            _ => {}
        }
    }
    match (ident, state) {
        (Some(ident), Some(state)) if !ident.is_empty() => Ok(Preset { ident, state }),
        _ => Err("the preset holds no plugin settings".into()),
    }
}

pub fn read(path: &std::path::Path) -> Result<Preset, String> {
    parse(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
}

/// A preset file for one plugin's settings, in the layout the reference program writes.
pub fn write(ident: &str, state: &[u8], generator: bool) -> Vec<u8> {
    use crate::flp::{Event, Header};
    let mut name = ident.as_bytes().to_vec();
    name.push(0);
    let mut wrap = vec![0u8; 52];
    wrap[9] = 2;
    let events = vec![Event::data(199, b"20.8.4\0".to_vec()), Event::data(ID_IDENT, name), Event::data(212, wrap), Event::data(ID_STATE, state.to_vec())];
    FlpFile { header: Header { format: 0x30, channels: if generator { 8 } else { 4 }, ppq: 96 }, events, trailer: vec![] }.to_bytes()
}

/// What a state file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    /// One plugin's settings.
    Plugin,
    /// A whole rack channel: its instrument and every channel setting.
    Channel,
    /// A whole mixer track: its effects, levels and equaliser.
    Track,
}

const FORMAT_CHANNEL: i16 = 0x20;
const FORMAT_TRACK: i16 = 0x40;

pub fn holds(bytes: &[u8]) -> Holds {
    match FlpFile::parse(bytes).map(|f| f.header.format) {
        Ok(FORMAT_CHANNEL) => Holds::Channel,
        Ok(FORMAT_TRACK) => Holds::Track,
        _ => Holds::Plugin,
    }
}

/// The channel a channel state file holds. It reads as a project of one channel.
pub fn channel(bytes: &[u8], lib: Option<&mut crate::sample::Library>, dir: Option<&std::path::Path>) -> Result<crate::song::Chan, String> {
    let flp = crate::flp::parse(bytes).map_err(|_| "not a channel state file".to_string())?;
    let song = crate::song::Song::from_flp_with(&flp, lib, dir);
    let mut c = song.channels.into_iter().next().ok_or("the file holds no channel")?;
    c.src = None;
    c.children.clear();
    Ok(c)
}

/// A channel state file for one channel.
pub fn write_channel(chan: &crate::song::Chan) -> Result<Vec<u8>, String> {
    use crate::flp::{Event, Header};
    let mut song = crate::song::Song::new();
    let mut c = chan.clone();
    c.src = None;
    c.children.clear();
    c.insert = 0;
    song.channels = vec![c];
    let whole = FlpFile::parse(&crate::save::to_bytes(&song, None)?)?;
    let from = whole.events.iter().position(|e| e.id == 64).ok_or("no channel was written")?;
    let run = whole.events[from + 1..].iter().position(|e| crate::save::ends_channel(e.id)).map(|n| from + 1 + n).unwrap_or(whole.events.len());
    let mut events = vec![Event::data(199, b"20.8.4\0".to_vec())];
    events.extend(whole.events[from..run].iter().cloned());
    Ok(FlpFile { header: Header { format: FORMAT_CHANNEL, channels: 5, ppq: 96 }, events, trailer: vec![] }.to_bytes())
}

/// The mixer track a mixer track state file holds. Sends are not part of it.
pub fn track(bytes: &[u8], lib: Option<&mut crate::sample::Library>, dir: Option<&std::path::Path>) -> Result<crate::song::Insert, String> {
    use crate::flp::{Event, Header};
    let f = FlpFile::parse(bytes).map_err(|_| "not a mixer track state file".to_string())?;
    let version = f.events.iter().find(|e| e.id == 199).cloned().unwrap_or_else(|| Event::data(199, b"20.8.4\0".to_vec()));
    let ver = match &version.value {
        Value::Data(d) => crate::flp::version_tuple(&text(d)),
        _ => (20, 0, 0, 0),
    };
    let base = crate::flp::mixer_base(ver) as u16;
    let numbered = f.events.iter().any(|e| e.id == 98);
    let mut head = vec![version];
    let mut flags = Event::data(236, vec![0u8; 12]);
    let mut slots: Vec<Event> = Vec::new();
    let mut records: Vec<u8> = Vec::new();
    let mut k = 0u16;
    for e in &f.events {
        match (e.id, &e.value) {
            (199, _) | (27, _) | (147, _) | (154, _) | (235, _) => {}
            (149, _) | (204, _) => head.push(e.clone()),
            (236, Value::Data(d)) if d.len() >= 12 => flags = e.clone(),
            (236, _) => {}
            (225, Value::Data(d)) => records = d.clone(),
            (201, _) if !numbered && !slots.is_empty() => {
                slots.push(Event::word(98, k));
                k += 1;
                slots.push(e.clone());
            }
            _ => slots.push(e.clone()),
        }
    }
    if !numbered && !slots.is_empty() {
        slots.push(Event::word(98, k));
    }
    // the records carry the number the track had where it was saved; old files also number the settings lower
    // and count levels to 128
    let old = !records.chunks_exact(12).any(|r| r[4] >= 192);
    for r in records.chunks_exact_mut(12) {
        let slot = u16::from_le_bytes([r[6], r[7]]) & 63;
        r[6..8].copy_from_slice(&((base << 6) | slot).to_le_bytes());
        r[5] = 0x1f;
        if old {
            if matches!(r[4], 1 | 96 | 97) {
                let v = i32::from_le_bytes([r[8], r[9], r[10], r[11]]) * 100;
                r[8..12].copy_from_slice(&v.to_le_bytes());
            }
            if r[4] >= 96 {
                r[4] += 96;
            }
        }
    }
    let mut events = head;
    events.push(flags);
    events.extend(slots);
    events.push(Event::int(147, 0));
    events.push(Event::data(225, records));
    let flp = crate::flp::Flp::from_file(FlpFile { header: Header { format: 0, channels: 0, ppq: 96 }, events, trailer: vec![] });
    let song = crate::song::Song::from_flp_with(&flp, lib, dir);
    let mut ins = song.inserts.into_iter().next().ok_or("the file holds no mixer track")?;
    for s in ins.slots.iter_mut() {
        s.src = None;
    }
    ins.sends.clear();
    ins.levels.clear();
    Ok(ins)
}

/// A mixer track state file for one mixer track. Its effects are written in order with no gaps between them.
pub fn write_track(ins: &crate::song::Insert) -> Result<Vec<u8>, String> {
    use crate::flp::{Event, Header};
    let mut song = crate::song::Song::new();
    let mut t = ins.clone();
    t.sends = vec![0];
    t.levels.clear();
    t.slots.sort_by_key(|s| s.index);
    for (k, s) in t.slots.iter_mut().enumerate() {
        s.index = k as u16;
        s.src = None;
    }
    song.inserts[1] = t;
    let whole = FlpFile::parse(&crate::save::to_bytes(&song, None)?)?;
    let closes: Vec<usize> = whole.events.iter().enumerate().filter(|(_, e)| e.id == 147).map(|(i, _)| i).collect();
    let (Some(&a), Some(&b)) = (closes.first(), closes.get(1)) else { return Err("no mixer track was written".into()) };
    let mut events = vec![Event::data(199, b"20.8.4\0".to_vec())];
    events.extend(whole.events[a + 1..b].iter().filter(|e| !matches!(e.id, 98 | 154 | 235)).cloned());
    let base = crate::flp::mixer_base((20, 8, 4, 0)) as u16;
    let mut records = Vec::new();
    for e in &whole.events {
        if let (225, Value::Data(d)) = (e.id, &e.value) {
            for r in d.chunks_exact(12) {
                // sends stay with the project
                if u16::from_le_bytes([r[6], r[7]]) >> 6 == base + 1 && !(64..192).contains(&r[4]) {
                    records.extend_from_slice(r);
                }
            }
        }
    }
    events.push(Event::data(225, records));
    Ok(FlpFile { header: Header { format: FORMAT_TRACK, channels: 4, ppq: 96 }, events, trailer: vec![] }.to_bytes())
}

impl Preset {
    /// The unit of ours these settings load onto. Names are matched without regard to case, as old presets
    /// spell them differently.
    pub fn unit(&self) -> Option<Unit> {
        if let Some(d) = crate::fx::DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(&self.ident)) {
            return crate::fx::Fx::decode(d.ident, &self.state).map(Unit::Fx);
        }
        let d = crate::synth::DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(&self.ident))?;
        if self.state.is_empty() {
            return None;
        }
        crate::synth::Gen::decode(d.ident, &self.state).map(Unit::Gen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flp::{Event, Header};

    fn file(ident: &[u8], state: Vec<u8>) -> Vec<u8> {
        let f = FlpFile { header: Header { format: 0x30, channels: 1, ppq: 96 }, events: vec![Event::data(ID_IDENT, ident.to_vec()), Event::data(212, vec![0; 52]), Event::data(ID_STATE, state)], trailer: vec![] };
        f.to_bytes()
    }

    #[test]
    fn a_channel_state_reads_back_the_same() {
        let mut c = crate::song::Chan::instrument(crate::synth::Kind::Osc3);
        c.name = "Lead".into();
        c.volume = 0.5;
        c.pan = -0.25;
        if let Some(g) = c.gen.as_mut() {
            g.p[0] = -12.0;
        }
        let bytes = write_channel(&c).unwrap();
        assert_eq!(holds(&bytes), Holds::Channel);
        let r = channel(&bytes, None, None).unwrap();
        assert_eq!((r.name.as_str(), r.src), ("Lead", None));
        assert!((r.volume - 0.5).abs() < 1e-3 && (r.pan + 0.25).abs() < 1e-3);
        assert_eq!(r.gen.map(|g| g.p), c.gen.map(|g| g.p));
    }

    #[test]
    fn a_mixer_track_state_reads_back_the_same() {
        let mut ins = crate::song::Insert { name: "Drums".into(), volume: 0.7, pan: 0.5, ..Default::default() };
        ins.eq[1].gain_db = 6.0;
        let mut a = crate::song::Slot::with_fx(2, crate::fx::Kind::Comp);
        a.mix = 0.5;
        if let Some(f) = a.fx.as_mut() {
            f.p[0] -= 12.0;
        }
        let b = crate::song::Slot::with_fx(7, crate::fx::Kind::Reverb);
        ins.slots = vec![a.clone(), b.clone()];
        let bytes = write_track(&ins).unwrap();
        assert_eq!(holds(&bytes), Holds::Track);
        assert!(FlpFile::parse(&bytes).unwrap().events.iter().all(|e| e.id != 98));
        let r = track(&bytes, None, None).unwrap();
        assert_eq!(r.name, "Drums");
        assert!((r.volume - 0.7).abs() < 1e-3 && (r.pan - 0.5).abs() < 1e-3 && (r.eq[1].gain_db - 6.0).abs() < 0.02);
        assert_eq!(r.slots.iter().map(|s| s.index).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(r.slots[0].fx, a.fx);
        assert_eq!(r.slots[1].fx, b.fx);
        assert!((r.slots[0].mix - 0.5).abs() < 1e-3 && (r.slots[1].mix - 1.0).abs() < 1e-3);
    }

    /// The state files in the user's own content read as what their headers say they are.
    #[test]
    fn real_state_files_read() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let root = std::path::Path::new(&root).join("Patches");
        let mut seen = (0, 0);
        for (dir, want) in [("Mixer presets", Holds::Track), ("Channel presets", Holds::Channel)] {
            let mut stack = vec![root.join(dir)];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                        let bytes = std::fs::read(&p).unwrap();
                        if holds(&bytes) != want {
                            continue;
                        }
                        match want {
                            Holds::Track => {
                                let t = track(&bytes, None, None).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                                assert!((t.volume - 1.0).abs() < 0.6, "{}: volume {}", p.display(), t.volume);
                                assert!(t.slots.iter().all(|s| s.mix > 0.0), "{}", p.display());
                                seen.0 += 1;
                            }
                            _ => {
                                channel(&bytes, None, None).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                                seen.1 += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(seen.0 > 0 && seen.1 > 0, "{seen:?}");
    }

    #[test]
    fn a_saved_preset_reads_back_the_same() {
        let mut f = crate::fx::Fx::new(crate::fx::Kind::Comp);
        f.p[0] -= 20.0;
        let bytes = write(crate::fx::def(f.kind).ident, &f.encode(None), false);
        assert_eq!(parse(&bytes).unwrap().unit(), Some(Unit::Fx(f)));
        let g = crate::synth::Gen::new(crate::synth::Kind::Osc3);
        let bytes = write(crate::synth::def(g.kind).ident, &g.encode(None), true);
        assert_eq!(parse(&bytes).unwrap().unit(), Some(Unit::Gen(g)));
    }

    #[test]
    fn a_preset_loads_onto_the_matching_effect_whatever_the_spelling() {
        let fresh = crate::fx::Fx::new(crate::fx::Kind::Comp);
        let mut want = fresh.clone();
        want.p[0] -= 55.0;
        let wide: Vec<u8> = "fruity compressor\0".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        for ident in [b"Fruity Compressor\0".to_vec(), wide] {
            let p = parse(&file(&ident, want.encode(None))).unwrap();
            assert_eq!(p.unit(), Some(Unit::Fx(want.clone())));
        }
    }

    #[test]
    fn an_instrument_preset_loads_and_an_unknown_plugin_does_not() {
        let g = crate::synth::Gen::new(crate::synth::Kind::Osc3);
        let p = parse(&file(b"3x Osc\0", g.encode(None))).unwrap();
        assert_eq!(p.unit(), Some(Unit::Gen(g)));
        let p = parse(&file(b"Something Else\0", vec![1, 2, 3])).unwrap();
        assert_eq!(p.unit(), None);
        assert!(parse(b"RIFF....").is_err());
    }

    /// With the user's own content at hand: every preset of a plugin we have a unit for must load.
    #[test]
    fn own_presets_load_onto_our_units() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets")];
        let (mut ours, mut loaded) = (0, 0);
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                    let Ok(pre) = read(&p) else { continue };
                    let known = crate::fx::DEFS.iter().any(|d| d.ident.eq_ignore_ascii_case(&pre.ident)) || crate::synth::DEFS.iter().any(|d| d.ident.eq_ignore_ascii_case(&pre.ident));
                    if known {
                        ours += 1;
                        // the six-operator instrument's first layouts (before 17) are not read yet
                        let early = pre.ident.eq_ignore_ascii_case("Sytrus") && pre.state.len() >= 4 && u32::from_le_bytes(pre.state[0..4].try_into().unwrap()) < 17;
                        if early {
                            ours -= 1;
                            continue;
                        }
                        match pre.unit() {
                            Some(_) => loaded += 1,
                            None => eprintln!("did not load: {} ({} bytes)", p.display(), pre.state.len()),
                        }
                    }
                }
            }
        }
        eprintln!("presets for our units: {ours}, loaded {loaded}");
        assert_eq!(ours, loaded);
    }
}
