//! Standard MIDI files: writing a song's notes out and reading one in.

use crate::song::{ClipSrc, Note, Song};

fn var(out: &mut Vec<u8>, mut v: u32) {
    let mut stack = [0u8; 5];
    let mut n = 0;
    loop {
        stack[n] = (v & 0x7f) as u8;
        n += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        out.push(stack[n] | if n > 0 { 0x80 } else { 0 });
    }
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
}

fn meta(out: &mut Vec<u8>, kind: u8, body: &[u8]) {
    out.extend_from_slice(&[0xff, kind]);
    var(out, body.len() as u32);
    out.extend_from_slice(body);
}

/// Every sounding note of the song (or of one pattern) with its absolute position.
fn collect(song: &Song, song_mode: bool, pattern: usize) -> Vec<Note> {
    let step = (song.ppq / 4).max(1);
    let mut all = Vec::new();
    let mut take = |n: &Note, at: i64, lo: i64, hi: i64| {
        let pos = n.pos as i64 + at;
        if n.muted || pos < lo || pos >= hi {
            return;
        }
        let len = if n.len == 0 { step } else { n.len };
        let len = (len as i64).min(hi - pos).max(1) as u32;
        all.push(Note { pos: pos as u32, len, ..n.clone() });
    };
    if song_mode {
        for c in &song.clips {
            let ClipSrc::Pattern(p) = c.src else { continue };
            if c.muted || song.tracks.get(c.track as usize).is_some_and(|t| !t.enabled) {
                continue;
            }
            let Some(pat) = song.patterns.get(p) else { continue };
            let at = c.pos as i64 - c.offset as i64;
            for n in &pat.notes {
                take(n, at, c.pos as i64, c.pos as i64 + c.len as i64);
            }
        }
    } else if let Some(pat) = song.patterns.get(pattern) {
        for n in &pat.notes {
            take(n, 0, 0, i64::MAX);
        }
    }
    all
}

/// A format 1 file: a tempo track, then one track per channel that has notes.
pub fn export(song: &Song, song_mode: bool, pattern: usize) -> Vec<u8> {
    let notes = collect(song, song_mode, pattern);
    let mut used: Vec<u16> = notes.iter().map(|n| n.chan).collect();
    used.sort_unstable();
    used.dedup();
    let mut out = Vec::new();
    let mut head = Vec::new();
    head.extend_from_slice(&1u16.to_be_bytes());
    head.extend_from_slice(&(used.len() as u16 + 1).to_be_bytes());
    head.extend_from_slice(&(song.ppq.min(0x7fff) as u16).to_be_bytes());
    chunk(&mut out, b"MThd", &head);

    let mut t = Vec::new();
    if !song.title.is_empty() {
        t.push(0);
        meta(&mut t, 3, song.title.as_bytes());
    }
    let us = (60_000_000.0 / song.tempo.max(1.0) as f64).round() as u32;
    t.push(0);
    meta(&mut t, 0x51, &us.to_be_bytes()[1..]);
    let (num, den) = song.time_sig;
    t.push(0);
    meta(&mut t, 0x58, &[num.max(1), (den.max(1) as f32).log2().round() as u8, 24, 8]);
    t.push(0);
    meta(&mut t, 0x2f, &[]);
    chunk(&mut out, b"MTrk", &t);

    for (i, &ch) in used.iter().enumerate() {
        // The drum channel is skipped so nothing turns into a kit by accident.
        let mc = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15][i % 15];
        let mut ev: Vec<(u32, u8, u8, u8)> = Vec::new();
        for n in notes.iter().filter(|n| n.chan == ch) {
            let vel = (n.vel * 100.0).round().clamp(1.0, 127.0) as u8;
            ev.push((n.pos, 1, n.key.min(127), vel));
            ev.push((n.pos + n.len, 0, n.key.min(127), 0));
        }
        // Offs before ons at the same tick, so repeated keys retrigger cleanly.
        ev.sort_by_key(|e| (e.0, e.1, e.2));
        let mut t = Vec::new();
        if let Some(c) = song.channels.get(ch as usize) {
            t.push(0);
            meta(&mut t, 3, c.name.as_bytes());
        }
        let mut at = 0;
        for (pos, on, key, vel) in ev {
            var(&mut t, pos - at);
            at = pos;
            t.extend_from_slice(&[if on == 1 { 0x90 } else { 0x80 } | mc, key, vel]);
        }
        t.push(0);
        meta(&mut t, 0x2f, &[]);
        chunk(&mut out, b"MTrk", &t);
    }
    out
}

/// What a MIDI file holds, with positions already in the asked-for resolution.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Imported {
    /// The first tempo in the file, if it sets one.
    pub tempo: Option<f32>,
    pub time_sig: Option<(u8, u8)>,
    /// One part per track and MIDI channel that has notes. `chan` on the notes is 0.
    pub parts: Vec<Part>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    /// True for the drum channel.
    pub drums: bool,
    pub notes: Vec<Note>,
}

struct Rd<'a> {
    d: &'a [u8],
    i: usize,
}

impl Rd<'_> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.d.get(self.i)?;
        self.i += 1;
        Some(b)
    }
    fn var(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..5 {
            let b = self.u8()?;
            v = (v << 7) | (b & 0x7f) as u32;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.d.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }
}

pub fn import(data: &[u8], ppq: u32) -> Result<Imported, String> {
    // Some files sit inside a RIFF wrapper; the real file starts at its header tag.
    let start = data.windows(4).position(|w| w == b"MThd").ok_or("not a MIDI file")?;
    let data = &data[start..];
    if data.len() < 14 {
        return Err("MIDI file is cut short".into());
    }
    let hlen = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let div = u16::from_be_bytes([data[12], data[13]]);
    if div & 0x8000 != 0 || div == 0 {
        return Err("MIDI files timed in frames are not supported".into());
    }
    let scale = |t: u64| (t * ppq as u64 / div as u64).min(u32::MAX as u64) as u32;
    let mut out = Imported::default();
    let mut at = 8 + hlen;
    while at + 8 <= data.len() {
        let tag = &data[at..at + 4];
        let len = u32::from_be_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let body = &data[at + 8..(at + 8 + len).min(data.len())];
        at += 8 + len;
        if tag != b"MTrk" {
            continue;
        }
        let mut r = Rd { d: body, i: 0 };
        let mut time = 0u64;
        let mut status = 0u8;
        let mut name = String::new();
        let mut parts: Vec<Option<Part>> = vec![None; 16];
        // Per channel: keys held, with where and how hard they started.
        let mut held: Vec<Vec<(u8, u64, u8)>> = vec![Vec::new(); 16];
        let off = |parts: &mut Vec<Option<Part>>, ch: usize, key: u8, start: u64, vel: u8, end: u64| {
            let p = parts[ch].get_or_insert_with(|| Part { drums: ch == 9, ..Part::default() });
            let pos = scale(start);
            p.notes.push(Note { pos, len: (scale(end).saturating_sub(pos)).max(1), chan: 0, key, vel: vel as f32 / 100.0, pan: 0.0, fine_cents: 0.0, muted: false, ..Default::default() });
        };
        loop {
            let Some(d) = r.var() else { break };
            time += d as u64;
            let Some(mut b) = r.u8() else { break };
            if b < 0x80 {
                r.i -= 1;
                b = status;
                if b < 0x80 {
                    break;
                }
            } else if b < 0xf0 {
                status = b;
            }
            match b {
                0xff => {
                    let (Some(kind), Some(n)) = (r.u8(), r.var()) else { break };
                    let Some(body) = r.take(n as usize) else { break };
                    match kind {
                        3 if name.is_empty() => name = String::from_utf8_lossy(body).trim().to_string(),
                        0x51 if body.len() == 3 && out.tempo.is_none() => {
                            let us = u32::from_be_bytes([0, body[0], body[1], body[2]]).max(1);
                            out.tempo = Some((60_000_000.0 / us as f64) as f32);
                        }
                        0x58 if body.len() >= 2 && out.time_sig.is_none() => {
                            out.time_sig = Some((body[0].max(1), 1u8 << body[1].min(6)));
                        }
                        0x2f => break,
                        _ => {}
                    }
                }
                0xf0 | 0xf7 => {
                    let Some(n) = r.var() else { break };
                    if r.take(n as usize).is_none() {
                        break;
                    }
                }
                _ => {
                    let ch = (b & 15) as usize;
                    let two = !matches!(b & 0xf0, 0xc0 | 0xd0);
                    let Some(k) = r.u8() else { break };
                    let v = if two { r.u8().unwrap_or(0) } else { 0 };
                    let kind = b & 0xf0;
                    if kind == 0x90 && v > 0 {
                        // The same key struck again ends the one still sounding.
                        if let Some(i) = held[ch].iter().position(|h| h.0 == k) {
                            let (_, s, hv) = held[ch].remove(i);
                            off(&mut parts, ch, k, s, hv, time);
                        }
                        held[ch].push((k, time, v));
                    } else if kind == 0x80 || kind == 0x90 {
                        if let Some(i) = held[ch].iter().position(|h| h.0 == k) {
                            let (_, s, hv) = held[ch].remove(i);
                            off(&mut parts, ch, k, s, hv, time);
                        }
                    }
                }
            }
        }
        for ch in 0..16 {
            for (k, s, v) in std::mem::take(&mut held[ch]) {
                off(&mut parts, ch, k, s, v, time.max(s + 1));
            }
        }
        let many = parts.iter().flatten().count() > 1;
        for (ch, p) in parts.into_iter().enumerate() {
            let Some(mut p) = p else { continue };
            p.notes.sort_by_key(|n| (n.pos, n.key));
            p.name = match (name.is_empty(), many) {
                (true, _) => format!("MIDI {}", ch + 1),
                (false, true) => format!("{name} {}", ch + 1),
                (false, false) => name.clone(),
            };
            out.parts.push(p);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::Pattern;

    #[test]
    fn a_pattern_survives_the_trip_out_and_back() {
        let mut song = Song::default();
        song.ppq = 96;
        song.tempo = 137.0;
        song.time_sig = (3, 4);
        let mk = |pos, len, chan, key, vel| Note { pos, len, chan, key, vel, pan: 0.0, fine_cents: 0.0, muted: false, ..Default::default() };
        let notes = vec![mk(0, 48, 0, 60, 1.0), mk(48, 48, 0, 60, 0.5), mk(96, 200, 0, 67, 1.2), mk(24, 0, 1, 36, 1.0), mk(500, 10, 1, 40, 0.8)];
        song.patterns = vec![Pattern { name: "P".into(), notes: notes.clone(), ..Default::default() }];
        let bytes = export(&song, false, 0);
        assert_eq!(&bytes[..4], b"MThd");
        let back = import(&bytes, 96).unwrap();
        assert!((back.tempo.unwrap() - 137.0).abs() < 0.01);
        assert_eq!(back.time_sig, Some((3, 4)));
        assert_eq!(back.parts.len(), 2);
        let a: Vec<_> = back.parts[0].notes.iter().map(|n| (n.pos, n.len, n.key, (n.vel * 100.0).round() as i32)).collect();
        assert_eq!(a, vec![(0, 48, 60, 100), (48, 48, 60, 50), (96, 200, 67, 120)]);
        let b: Vec<_> = back.parts[1].notes.iter().map(|n| (n.pos, n.len, n.key)).collect();
        assert_eq!(b, vec![(24, 24, 36), (500, 10, 40)]);
        // Twice the resolution doubles every position.
        let fine = import(&bytes, 192).unwrap();
        assert_eq!(fine.parts[0].notes[2].pos, 192);
        assert_eq!(fine.parts[0].notes[2].len, 400);
    }

    #[test]
    fn running_status_and_broken_files_are_handled() {
        let mut f = Vec::new();
        chunk(&mut f, b"MThd", &[0, 0, 0, 1, 0, 96]);
        // Note on, then running status with velocity 0 as the off, then a second pair.
        chunk(&mut f, b"MTrk", &[0, 0x99, 36, 100, 96, 36, 0, 0, 38, 90, 48, 38, 0, 0, 0xff, 0x2f, 0]);
        let m = import(&f, 96).unwrap();
        assert_eq!(m.parts.len(), 1);
        assert!(m.parts[0].drums);
        assert_eq!(m.parts[0].notes.iter().map(|n| (n.pos, n.len, n.key)).collect::<Vec<_>>(), vec![(0, 96, 36), (96, 48, 38)]);
        assert!(import(b"nothing", 96).is_err());
        assert!(import(&f[..20], 96).unwrap().parts.is_empty());
    }
}
