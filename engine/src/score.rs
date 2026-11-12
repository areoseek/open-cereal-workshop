//! Score files (`.fsc`): the notes of one piano roll in the same event stream a project uses.

use crate::flp::{Event, FlpFile, Header, Value, NOTE_SIZE};
use crate::song::Note;

const ID_NOTES: u8 = 224;

/// The notes of a score, with positions scaled to `ppq`. Every note is put on channel `chan`.
pub fn parse(bytes: &[u8], ppq: u32, chan: u16) -> Result<Vec<Note>, String> {
    let f = FlpFile::parse(bytes).map_err(|_| "not a score file".to_string())?;
    let from = (f.header.ppq as u64).max(1);
    let scale = |t: u32| (t as u64 * ppq as u64 / from) as u32;
    let mut out = Vec::new();
    for e in &f.events {
        let (Value::Data(d), ID_NOTES) = (&e.value, e.id) else { continue };
        for n in d.chunks_exact(NOTE_SIZE) {
            let u32at = |o: usize| u32::from_le_bytes([n[o], n[o + 1], n[o + 2], n[o + 3]]);
            let flags = u16::from_le_bytes([n[4], n[5]]);
            out.push(Note {
                pos: scale(u32at(0)),
                len: scale(u32at(8)),
                chan,
                key: u16::from_le_bytes([n[12], n[13]]).min(131) as u8,
                vel: n[21] as f32 / 100.0,
                pan: (n[20] as f32 - 64.0) / 64.0,
                fine_cents: (n[16] as f32 - 120.0) * 10.0,
                muted: flags & 0x2000 != 0,
                release: n[18] as f32 / 128.0,
                mod_x: (n[22] as f32 - 128.0) / 128.0,
                mod_y: (n[23] as f32 - 128.0) / 128.0,
                slide: flags & 0x0008 != 0,
                porta: flags & 0x0010 != 0,
                color: n[19] & 15,
                group: u16::from_le_bytes([n[14], n[15]]),
            });
        }
    }
    if out.is_empty() {
        return Err("the score holds no notes".into());
    }
    out.sort_by_key(|n| (n.pos, n.key));
    Ok(out)
}

pub fn read(path: &std::path::Path, ppq: u32, chan: u16) -> Result<Vec<Note>, String> {
    parse(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?, ppq, chan)
}

/// A score file holding these notes (whatever channel they are on).
pub fn write(notes: &[&Note], ppq: u32) -> Vec<u8> {
    let mut sorted: Vec<&Note> = notes.to_vec();
    sorted.sort_by_key(|n| (n.pos, n.key));
    let mut data = Vec::with_capacity(sorted.len() * NOTE_SIZE);
    for n in sorted {
        let f = crate::flp::Note {
            position: n.pos,
            flags: 0x4000 | if n.muted { 0x2000 } else { 0 } | if n.slide { 8 } else { 0 } | if n.porta { 16 } else { 0 },
            rack_channel: 0,
            length: n.len,
            key: n.key as u16,
            group: 0,
            fine_pitch: (n.fine_cents / 10.0 + 120.0).round().clamp(0.0, 240.0) as u8,
            release: (n.release * 128.0).round().clamp(0.0, 128.0) as u8,
            midi_channel: n.color & 15,
            pan: (n.pan * 64.0 + 64.0).round().clamp(0.0, 128.0) as u8,
            velocity: (n.vel * 100.0).round().clamp(0.0, 128.0) as u8,
            mod_x: (n.mod_x * 128.0 + 128.0).round().clamp(0.0, 255.0) as u8,
            mod_y: (n.mod_y * 128.0 + 128.0).round().clamp(0.0, 255.0) as u8,
        };
        f.write(&mut data);
    }
    let events = vec![Event::data(199, b"20.8.4\0".to_vec()), Event::word(65, 0), Event::data(ID_NOTES, data)];
    FlpFile { header: Header { format: 0x10, channels: 1, ppq: ppq.min(0x7fff) as u16 }, events, trailer: vec![] }.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_comes_back_the_same_and_rescales() {
        let mk = |pos, len, key, vel, pan, fine, muted| Note { pos, len, chan: 3, key, vel, pan, fine_cents: fine, muted, ..Default::default() };
        let notes = vec![mk(0, 24, 60, 1.0, 0.0, 0.0, false), mk(24, 48, 64, 0.5, -0.5, 100.0, true), mk(96, 0, 67, 1.28, 1.0, -200.0, false)];
        let bytes = write(&notes.iter().collect::<Vec<_>>(), 96);
        assert_eq!(parse(&bytes, 96, 3).unwrap(), notes);
        let twice = parse(&bytes, 192, 0).unwrap();
        assert_eq!((twice[1].pos, twice[1].len, twice[1].chan), (48, 96, 0));
        assert!(parse(b"junk", 96, 0).is_err());
    }

    #[test]
    fn every_score_in_the_corpus_reads() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::PathBuf::from(root).join("Patches/Scores")];
        let (mut seen, mut ok) = (0, 0);
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fsc")) {
                    seen += 1;
                    ok += read(&p, 96, 0).is_ok() as usize;
                }
            }
        }
        if seen > 0 {
            assert!(ok * 10 >= seen * 9, "{ok} of {seen} scores read");
            eprintln!("{ok} of {seen} scores read");
        }
    }
}
