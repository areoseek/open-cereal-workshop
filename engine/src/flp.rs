//! Reading FL Studio project files (.flp).
//!
//! The .flp container is a public, widely documented event-stream format (the same structure open projects such
//! as LMMS and PyFLP read). This module is an independent reader written from that public description: a "FLhd"
//! header chunk followed by a "FLdt" chunk of tagged events. Nothing here is taken from the original program's
//! code; we only parse a documented file layout, the way any program reads a file format it did not invent.
//!
//! Event encoding, by tag id:
//!   0..=63    one byte of data
//!   64..=127  two bytes (little-endian u16)
//!   128..=191 four bytes (little-endian u32)
//!   192..=255 a length (7-bit varint) then that many bytes (text or a binary blob)
//!
//! We decode every event generically, then pull out the fields we understand today (version, title, tempo,
//! channel names, pattern notes). Unknown events are kept as raw bytes so later code can grow into them.

use crate::project::ChannelData;
use crate::{Drum, Project, STEPS};

// Event ids we read. These are the public, long-stable ids; best-effort ones are marked.
const VERSION: u8 = 199; // text: "major.minor.patch.build"
const TITLE: u8 = 194; // text
const DEF_PLUGIN_NAME: u8 = 201; // text: channel's generator name
const CHAN_NAME: u8 = 203; // text: channel display name
const TEMPO_FINE: u8 = 156; // int: bpm * 1000 (modern)
const TEMPO_WORD: u8 = 66; // word: whole bpm (old files)
// NOTE: files written by very recent FL versions (25.2+) store tempo somewhere this reader does not yet find; such
// files currently fall back to 130 bpm in to_project(). Tracked in docs/ROADMAP.md.
const NEW_CHANNEL: u8 = 64; // word: start of a channel, value = its rack index
const PATTERN_NOTES: u8 = 224; // data: array of 24-byte notes for the current pattern

#[derive(Debug, Clone)]
pub enum Value {
    Byte(u8),
    Word(u16),
    Int(u32),
    Data(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Event {
    pub id: u8,
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Header {
    pub format: i16,
    pub channels: u16,
    pub ppq: u16,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub position: u32, // ticks from the start of the pattern
    pub rack_channel: u16,
    pub duration: u32,
    pub key: u8, // MIDI note, 60 = middle C
    pub velocity: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Flp {
    pub header: Header,
    pub version: String,
    pub title: String,
    pub tempo: f32,
    pub channel_names: Vec<String>,
    pub notes: Vec<Note>,
    pub events: Vec<Event>,
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, i: 0 }
    }
    fn left(&self) -> usize {
        self.b.len() - self.i
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.left() < n {
            return Err("unexpected end of file".into());
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    /// The 7-bit, little-endian varint used for text/data event lengths.
    fn varint(&mut self) -> Result<usize, String> {
        let mut out = 0usize;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            out |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                return Ok(out);
            }
            shift += 7;
            if shift > 35 {
                return Err("varint too long".into());
            }
        }
    }
}

fn utf16le_or_ascii(bytes: &[u8]) -> String {
    // FL text is UTF-16LE in modern files, Latin-1 in old ones. If it looks like UTF-16 (zero high bytes on
    // ASCII text), decode as such; otherwise read it as Latin-1. Trailing NULs are dropped.
    let looks_utf16 = bytes.len() >= 2 && bytes.len() % 2 == 0 && bytes.iter().skip(1).step_by(2).filter(|&&b| b == 0).count() * 3 >= bytes.len() / 2;
    let s = if looks_utf16 {
        let u: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    };
    s.trim_end_matches('\0').to_string()
}

pub fn parse(bytes: &[u8]) -> Result<Flp, String> {
    let mut c = Cursor::new(bytes);
    if c.take(4)? != b"FLhd" {
        return Err("not an FLP file (no FLhd header)".into());
    }
    let hsize = c.u32()?;
    if hsize != 6 {
        return Err(format!("unexpected header size {hsize}"));
    }
    let format = c.u16()? as i16;
    let channels = c.u16()?;
    let ppq = c.u16()?;
    let mut flp = Flp { header: Header { format, channels, ppq }, tempo: 0.0, ..Default::default() };

    if c.take(4)? != b"FLdt" {
        return Err("missing FLdt data chunk".into());
    }
    let dsize = c.u32()? as usize;
    if c.left() < dsize {
        return Err("data chunk is shorter than its declared size".into());
    }

    let mut current_channel: Option<u16> = None;
    let end = c.i + dsize;
    while c.i < end {
        let id = c.u8()?;
        let value = match id {
            0..=63 => Value::Byte(c.u8()?),
            64..=127 => Value::Word(c.u16()?),
            128..=191 => Value::Int(c.u32()?),
            192..=255 => {
                let n = c.varint()?;
                Value::Data(c.take(n)?.to_vec())
            }
        };
        match (id, &value) {
            (VERSION, Value::Data(d)) => flp.version = utf16le_or_ascii(d),
            (TITLE, Value::Data(d)) => flp.title = utf16le_or_ascii(d),
            (TEMPO_FINE, Value::Int(v)) => flp.tempo = *v as f32 / 1000.0,
            (TEMPO_WORD, Value::Word(v)) if flp.tempo == 0.0 => flp.tempo = *v as f32,
            (NEW_CHANNEL, Value::Word(v)) => current_channel = Some(*v),
            (CHAN_NAME, Value::Data(d)) | (DEF_PLUGIN_NAME, Value::Data(d)) => {
                let name = utf16le_or_ascii(d);
                if id == CHAN_NAME || current_channel.map(|i| i as usize) == Some(flp.channel_names.len()) {
                    // prefer the explicit channel name; fall back to the generator name
                    if let Some(idx) = current_channel {
                        while flp.channel_names.len() <= idx as usize {
                            flp.channel_names.push(String::new());
                        }
                        if id == CHAN_NAME || flp.channel_names[idx as usize].is_empty() {
                            flp.channel_names[idx as usize] = name;
                        }
                    }
                }
            }
            (PATTERN_NOTES, Value::Data(d)) => flp.notes.extend(parse_notes(d)),
            _ => {}
        }
        flp.events.push(Event { id, value });
    }
    Ok(flp)
}

fn parse_notes(d: &[u8]) -> Vec<Note> {
    // Each note is 24 bytes. Layout (public): pos u32, flags u16, rack u16, dur u32, key u32, then per-note
    // fine pitch / release / midi channel / pan / velocity / mod x / mod y. We read the fields we use.
    let mut notes = vec![];
    for n in d.chunks_exact(24) {
        let u32at = |o: usize| u32::from_le_bytes([n[o], n[o + 1], n[o + 2], n[o + 3]]);
        let u16at = |o: usize| u16::from_le_bytes([n[o], n[o + 1]]);
        notes.push(Note {
            position: u32at(0),
            rack_channel: u16at(6),
            duration: u32at(8),
            key: (u32at(12) & 0xff) as u8,
            velocity: n[20],
        });
    }
    notes
}

impl Flp {
    /// A best-effort conversion into our own project model: tempo and channel names come across directly; notes
    /// are folded onto the 16-step grid of their rack channel so a simple beat opens and plays. Full fidelity
    /// (piano roll, playlist, plugin state) arrives as those systems are built (docs/ROADMAP.md).
    pub fn to_project(&self) -> Project {
        let bpm = if self.tempo > 0.0 { self.tempo } else { 130.0 };
        let ppq = self.header.ppq.max(1) as u32;
        let step_ticks = (ppq / 4).max(1); // one 16th note
        let n_chan = self.header.channels.max(1) as usize;

        let mut channels: Vec<ChannelData> = (0..n_chan)
            .map(|i| ChannelData {
                name: self.channel_names.get(i).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| format!("Channel {}", i + 1)),
                drum: match i % 4 {
                    0 => Drum::Kick,
                    1 => Drum::Clap,
                    2 => Drum::Hat,
                    _ => Drum::Snare,
                },
                steps: [false; STEPS],
                volume: 0.8,
                muted: false,
            })
            .collect();

        for note in &self.notes {
            let ch = note.rack_channel as usize;
            if ch < channels.len() {
                let step = (note.position / step_ticks) as usize % STEPS;
                channels[ch].steps[step] = true;
            }
        }

        Project { title: self.title.clone(), bpm, master: 0.8, channels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_event(id: u8, s: &str) -> Vec<u8> {
        let mut body: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        body.extend_from_slice(&[0, 0]); // trailing NUL as FL writes
        let mut out = vec![id];
        let mut len = body.len();
        loop {
            let mut b = (len & 0x7f) as u8;
            len >>= 7;
            if len > 0 {
                b |= 0x80;
            }
            out.push(b);
            if len == 0 {
                break;
            }
        }
        out.extend(body);
        out
    }

    fn build_flp(channels: u16, ppq: u16, body: &[u8]) -> Vec<u8> {
        let mut out = vec![];
        out.extend_from_slice(b"FLhd");
        out.extend_from_slice(&6u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // format
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&ppq.to_le_bytes());
        out.extend_from_slice(b"FLdt");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn reads_header_version_title_and_tempo() {
        let mut body = vec![];
        body.extend(text_event(VERSION, "25.2.3.5164"));
        body.extend(text_event(TITLE, "My Song"));
        body.push(TEMPO_FINE);
        body.extend_from_slice(&140_000u32.to_le_bytes()); // 140.0 bpm
        let bytes = build_flp(45, 96, &body);

        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.header.channels, 45);
        assert_eq!(flp.header.ppq, 96);
        assert_eq!(flp.version, "25.2.3.5164");
        assert_eq!(flp.title, "My Song");
        assert_eq!(flp.tempo, 140.0);
    }

    #[test]
    fn folds_notes_onto_the_step_grid() {
        let mut body = vec![];
        body.push(TEMPO_FINE);
        body.extend_from_slice(&120_000u32.to_le_bytes());
        // two notes on rack channel 1, at tick 0 and tick 48 (ppq 96 => 48 ticks = step 2)
        let mut notes = vec![0u8; 48];
        notes[6] = 1; // rack channel (u16 at offset 6)
        notes[12] = 60; // key
        notes[20] = 100; // velocity
                         // second note at offset 24: position 48
        notes[24..28].copy_from_slice(&48u32.to_le_bytes());
        notes[30] = 1; // rack channel
        notes[36] = 62;
        notes[44] = 100;
        body.extend(super::tests::data_event(PATTERN_NOTES, &notes));
        let bytes = build_flp(4, 96, &body);

        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.notes.len(), 2);
        let p = flp.to_project();
        assert_eq!(p.bpm, 120.0);
        assert!(p.channels[1].steps[0], "first note at step 0");
        assert!(p.channels[1].steps[2], "second note at step 2");
    }

    #[test]
    fn rejects_non_flp() {
        assert!(parse(b"not a flp file at all").is_err());
    }

    fn data_event(id: u8, d: &[u8]) -> Vec<u8> {
        let mut out = vec![id];
        let mut len = d.len();
        loop {
            let mut b = (len & 0x7f) as u8;
            len >>= 7;
            if len > 0 {
                b |= 0x80;
            }
            out.push(b);
            if len == 0 {
                break;
            }
        }
        out.extend_from_slice(d);
        out
    }
}
