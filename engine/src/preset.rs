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
