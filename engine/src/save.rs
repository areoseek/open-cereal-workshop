//! Writing a project back to a .flp.
//!
//! A project that was opened from a file is saved by patching that file's own event list: only what was edited
//! is rewritten, and everything this program does not understand yet (plugin state, automation, settings) is
//! carried over untouched. A project that was not edited at all saves byte for byte identical.
//!
//! A project started here has no file to patch, so a plain skeleton is built first and patched the same way.
//!
//! When a channel is deleted the channels after it are renumbered in the channel list and in every note,
//! playlist clip, link (automation target) and layer child.

use std::collections::HashMap;

use crate::flp::{self, id, latin1, u16at, u32at, version_tuple, ChannelKind, Event, Flp, FlpFile, Header, Value, NOTE_SIZE};
use crate::song::{ClipSrc, Note, Sampler, Song};

const ITEM_BASE: u16 = 0x5000;

/// Ids that can never belong to a channel's own run of events.
pub(crate) fn ends_channel(e: u8) -> bool {
    matches!(e, 64 | 65 | 99 | 233 | 236 | 238 | 27 | 29 | 31 | 42 | 95 | 100 | 133 | 149 | 204 | 225)
}

fn text_event(eid: u8, s: &str, unicode: bool) -> Event {
    if unicode {
        Event::text(eid, s)
    } else {
        let mut b: Vec<u8> = s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
        b.push(0);
        Event::data(eid, b)
    }
}

fn put_u32(d: &mut [u8], o: usize, v: u32) {
    if let Some(s) = d.get_mut(o..o + 4) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}
fn put_u16(d: &mut [u8], o: usize, v: u16) {
    if let Some(s) = d.get_mut(o..o + 2) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

fn color_bits(c: [u8; 3], keep_top: u32) -> u32 {
    c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16 | (keep_top & 0xff00_0000)
}

fn item_size(d: &[u8]) -> Option<usize> {
    [32usize, 60, 80, 88, 96, 104, 112, 120, 128].into_iter().find(|&s| !d.is_empty() && d.len() % s == 0 && d.chunks_exact(s).all(|it| u16at(it, 4) == ITEM_BASE))
}

fn item_size_for(ver: (u32, u32, u32, u32)) -> usize {
    if ver >= (24, 2, 99, 0) {
        80
    } else if ver >= (20, 99, 0, 0) {
        60
    } else {
        32
    }
}

// ---------------------------------------------------------------------------------------------------------------
// File units, so "did this change" is judged on what would be written
// ---------------------------------------------------------------------------------------------------------------

fn vol_u(v: f32) -> u32 {
    (v * 12800.0).round().clamp(0.0, 12800.0) as u32
}
fn pan_u(v: f32) -> u32 {
    (v * 6400.0 + 6400.0).round().clamp(0.0, 12800.0) as u32
}
fn ins_vol(v: f32) -> i32 {
    (v * 12800.0).round().clamp(0.0, 16000.0) as i32
}
fn ins_pan(v: f32) -> i32 {
    (v * 6400.0).round().clamp(-6400.0, 6400.0) as i32
}
fn vel_u(v: f32) -> u8 {
    (v * 100.0).round().clamp(0.0, 128.0) as u8
}
fn npan_u(v: f32) -> u8 {
    (v * 64.0 + 64.0).round().clamp(0.0, 128.0) as u8
}
fn fine_u(v: f32) -> u8 {
    (v / 10.0 + 120.0).round().clamp(0.0, 240.0) as u8
}

type NoteKey = ((u32, u32, u16, u8, u8, u8, u8, bool), (i32, i32, i32, bool, bool, u8, u16));
fn note_key(n: &Note) -> NoteKey {
    let q = |v: f32| (v * 128.0).round() as i32;
    ((n.pos, n.len, n.chan, n.key, vel_u(n.vel), npan_u(n.pan), fine_u(n.fine_cents), n.muted), (q(n.release), q(n.mod_x), q(n.mod_y), n.slide, n.porta, n.color, n.group))
}
fn notes_key(notes: &[Note]) -> Vec<NoteKey> {
    let mut v: Vec<NoteKey> = notes.iter().map(note_key).collect();
    v.sort();
    v
}

// ---------------------------------------------------------------------------------------------------------------
// A plain project to patch when there is no original
// ---------------------------------------------------------------------------------------------------------------

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect()
}

/// The events of one empty sampler channel with every setting at its default.
fn blank_channel(index: u16, unicode: bool, word_insert: bool) -> Vec<Event> {
    let mut ev = vec![
        Event::word(id::NEW_CHANNEL, index),
        Event::byte(id::CHAN_TYPE, 0),
        text_event(id::PLUGIN_INTERNAL_NAME, "", unicode),
        text_event(id::PLUGIN_NAME, "Sampler", unicode),
        Event::int(id::COLOR, 0x006a_655c),
        Event::byte(id::CHAN_ENABLED, 1),
        // the run of older per-channel settings a project made by the original always carries
        Event::data(id::CHAN_ECHO, hex("0000000000190000000000000400000090000000")),
        Event::int(138, 0x0080_0080),
        Event::int(139, 0x0001_0000),
        Event::word(id::CHAN_TIME_SHIFT, 0),
        Event::word(id::CHAN_SWING, 0x80),
        Event::word(69, 0x80),
        Event::word(86, 0x100),
        Event::word(71, 0x400),
        Event::word(83, 0),
        Event::word(id::CHAN_PREAMP, 0),
        Event::word(id::CHAN_FADE_OUT, 0),
        Event::word(id::CHAN_FADE_IN, 0),
        Event::word(85, 0x800),
        Event::int(131, 0x0080_0000),
        Event::word(id::CHAN_FX_FLAGS, 0),
        if word_insert { Event::word(id::CHAN_TARGET_INSERT_WORD, 0) } else { Event::byte(id::CHAN_TARGET_INSERT, 0) },
        Event::data(id::CHAN_LEVELS, hex("001900001027000000000000000100000000000000000000")),
        Event::data(id::CHAN_ADJUST, hex("0000000000320000000000000000000000000000")),
        Event::data(
            id::CHAN_PARAMS,
            hex("ffffffff0000000001000001ffffffff3c0000000000803f0000803f0000803f0000803f0000803f0000000001000000ffffffff000400003000000000000100a7050000000000000001000000000000000000000000000000000000010000000000000000000000000000000000000002000000feffffffffffffff000000000000000000000000000000000000f03f0000000000000000ffffffff01"),
        ),
    ];
    ev.push(Event::int(id::CHAN_CUT_GROUP, 0));
    ev.push(Event::int(144, 0));
    ev.push(Event::int(id::CHAN_GROUP, 0));
    ev.push(Event::byte(32, 0));
    ev.push(Event::data(id::CHAN_TRACKING, hex("64000000000000000000000000000000")));
    ev.push(Event::data(id::CHAN_TRACKING, hex("3c000000000000000000000000000000")));
    for i in 0..5 {
        let mut d = hex("000000000000000064000000204e0000204e00003075000032000000204e00000000000064000000204e000000000000b680000000000000000000000000000000000000");
        if i == 1 {
            d[0] = 4;
        }
        ev.push(Event::data(id::CHAN_ENVELOPE, d));
    }
    ev.push(Event::int(id::SAMPLE_FLAGS, 3));
    ev.push(Event::byte(id::CHAN_PING_PONG, 0));
    ev
}

/// A project with `channels` empty samplers, one empty playlist and a default mixer, in the layout version 20
/// wrote: 32-byte playlist items, 500 tracks, 127 inserts.
fn skeleton(song: &Song) -> Flp {
    let mut ev = vec![
        Event::data(id::VERSION, b"20.8.4.2576\0".to_vec()),
        Event::int(id::BUILD, 2576),
        Event::int(id::TEMPO, 140_000),
        Event::word(id::CURRENT_PATTERN, 1),
        Event::byte(id::LOOP_ACTIVE, song.song_mode as u8),
        Event::byte(id::SHUFFLE, 0),
        Event::word(id::MAIN_PITCH, 0),
        Event::byte(id::TIMESIG_NUM, 4),
        Event::byte(id::TIMESIG_BEAT, 4),
        Event::byte(id::PAN_LAW, 0),
        Event::text(id::TITLE, ""),
        Event::text(id::GENRE, ""),
        Event::text(id::ARTIST, ""),
        Event::text(id::COMMENT, ""),
        Event::text(id::GROUP_NAME, "Unsorted"),
        Event::int(id::CUR_GROUP, u32::MAX),
    ];
    for i in 0..song.channels.len() {
        ev.extend(blank_channel(i as u16, true, false));
    }
    ev.push(Event::word(id::NEW_ARRANGEMENT, 0));
    ev.push(Event::text(id::ARRANGEMENT_NAME, "Arrangement"));
    ev.push(Event::data(id::PLAYLIST, vec![]));
    let n_tracks = song.tracks.len().max(1);
    for t in 0..n_tracks {
        let mut d = (t as u32 + 1).to_le_bytes().to_vec();
        d.extend(hex("4851560000000000010000803fffffffff0000000000000000000500000000000000010000000000000000000000000000ffffffffffffffff0100000000"));
        ev.push(Event::data(id::TRACK, d));
    }
    ev.push(Event::word(id::CURRENT_ARRANGEMENT, 0));
    let n_ins = song.inserts.len().max(1);
    for i in 0..n_ins {
        let flags: u32 = insert_look(song.inserts.get(i).unwrap_or(&crate::song::Insert::default()), 0x08);
        let mut d = vec![0u8; 12];
        put_u32(&mut d, 4, flags);
        ev.push(Event::data(id::INSERT_FLAGS, d));
        for s in 0..10 {
            ev.push(Event::word(id::SLOT_INDEX, s));
        }
        let mut routes = vec![0u8; n_ins];
        // every track feeds the master except the master itself and the last one of a full mixer (the "current" track)
        if i != 0 && !(n_ins == 127 && i == 126) {
            routes[0] = 1;
        }
        ev.push(Event::data(id::INSERT_ROUTING, routes));
        ev.push(Event::int(id::INSERT_INPUT, u32::MAX));
        ev.push(Event::int(id::INSERT_OUTPUT, if i == 0 { 0 } else { u32::MAX }));
    }
    // every mixer control at its resting value; a file without these records plays about 6 dB hot in the reference
    let mut params: Vec<u8> = vec![];
    let mut rec = |pid: u8, kind: u8, cd: usize, value: i32| {
        params.extend_from_slice(&0u32.to_le_bytes());
        params.push(pid);
        params.push(kind);
        params.extend_from_slice(&(cd as u16).to_le_bytes());
        params.extend_from_slice(&value.to_le_bytes());
    };
    rec(0, 0, (128 + 128) << 6, 12800);
    for i in 0..n_ins.min(127) {
        let at = (128 + i) << 6;
        for s in 0..10 {
            rec(0, 0x1f, at | s, 1);
            rec(1, 0x1f, at | s, 12800);
        }
        for (pid, v) in [(192u8, 12800), (193, 0), (194, 0), (208, 0), (209, 0), (210, 0), (216, 5777), (217, 33145), (218, 55825), (224, 17500), (225, 17500), (226, 17500)] {
            rec(pid, 0x1f, at, v);
        }
        if i < 100 {
            for pid in 164..=167u8 {
                rec(pid, 0x1f, at, 0);
            }
        }
        rec(190, 0x1f, at, 0);
    }
    ev.push(Event::data(id::MIXER_PARAMS, params));
    Flp::from_file(FlpFile { header: Header { format: 0, channels: song.channels.len() as u16, ppq: song.ppq.clamp(24, 960) as u16 }, events: ev, trailer: vec![] })
}

// ---------------------------------------------------------------------------------------------------------------
// Channels added, removed, cloned or reordered
// ---------------------------------------------------------------------------------------------------------------

fn restructure_channels(file: &mut FlpFile, orig: &Flp, song: &Song, unicode: bool) {
    let n_orig = orig.channels.len();
    let srcs: Vec<Option<usize>> = song.channels.iter().map(|c| c.src.map(|s| s as usize).filter(|&s| s < n_orig)).collect();
    if srcs.len() == n_orig && srcs.iter().enumerate().all(|(i, s)| *s == Some(i)) {
        return;
    }

    // cut the stream into channel runs and everything else
    enum Part {
        Other(Event),
        Slot(usize),
    }
    let mut parts: Vec<Part> = Vec::with_capacity(file.events.len());
    let mut blocks: Vec<Vec<Event>> = vec![];
    let mut open = false;
    let mut word_insert = false;
    for e in file.events.drain(..) {
        if e.id == id::NEW_CHANNEL {
            parts.push(Part::Slot(blocks.len()));
            blocks.push(vec![e]);
            open = true;
        } else if open && !ends_channel(e.id) {
            word_insert |= e.id == id::CHAN_TARGET_INSERT_WORD;
            blocks.last_mut().unwrap().push(e);
        } else {
            open = false;
            parts.push(Part::Other(e));
        }
    }

    // a model for brand new channels: an existing sampler stripped of its sample, else the plain default
    let model: Vec<Event> = orig
        .channels
        .iter()
        .position(|c| c.kind == ChannelKind::Sampler)
        .and_then(|i| blocks.get(i))
        .map(|b| {
            b.iter()
                .filter(|e| e.id != id::SAMPLE_PATH && e.id != id::CHAN_AUTOMATION)
                .cloned()
                .map(|mut e| {
                    match e.id {
                        id::PLUGIN_NAME => e = text_event(id::PLUGIN_NAME, "Sampler", unicode),
                        id::CHAN_LEVELS => {
                            if let Value::Data(d) = &mut e.value {
                                put_u32(d, 0, 6400);
                                put_u32(d, 4, 10000);
                                put_u32(d, 8, 0);
                            }
                        }
                        id::CHAN_ENABLED => e.value = Value::Byte(1),
                        _ => {}
                    }
                    e
                })
                .collect()
        })
        .unwrap_or_else(|| blank_channel(0, unicode, word_insert));

    let mut new_blocks: Vec<Vec<Event>> = srcs
        .iter()
        .enumerate()
        .map(|(pos, s)| {
            let mut b = match s {
                Some(k) => blocks[*k].clone(),
                None => model.clone(),
            };
            if let Some(first) = b.first_mut() {
                first.value = Value::Word(pos as u16);
            }
            b
        })
        .collect();

    // old number -> new number (the first place a kept channel sits)
    let mut remap: HashMap<u16, u16> = HashMap::new();
    for (pos, s) in srcs.iter().enumerate() {
        if let Some(k) = s {
            remap.entry(*k as u16).or_insert(pos as u16);
        }
    }

    let n_slots = blocks.len();
    let mut out: Vec<Event> = Vec::with_capacity(parts.len() + 64);
    let mut it = new_blocks.drain(..);
    for p in parts {
        match p {
            Part::Other(e) => out.push(e),
            Part::Slot(k) => {
                if let Some(b) = it.next() {
                    out.extend(b);
                }
                if k + 1 == n_slots {
                    for b in it.by_ref() {
                        out.extend(b);
                    }
                }
            }
        }
    }
    let rest: Vec<Vec<Event>> = it.collect();
    if !rest.is_empty() {
        // the file had no channels at all: put them ahead of the playlist and mixer
        let at = out.iter().position(|e| matches!(e.id, 99 | 233 | 236 | 238 | 42 | 149 | 204)).unwrap_or(out.len());
        let tail = out.split_off(at);
        out.extend(rest.into_iter().flatten());
        out.extend(tail);
    }

    // renumber what points at channels
    for e in &mut out {
        match (e.id, &mut e.value) {
            (id::PATTERN_NOTES, Value::Data(d)) => {
                let mut nd = Vec::with_capacity(d.len());
                for n in d.chunks_exact(flp::note_size(orig.version_tuple())) {
                    if let Some(&to) = remap.get(&u16at(n, 6)) {
                        let mut n = n.to_vec();
                        put_u16(&mut n, 6, to);
                        nd.extend(n);
                    }
                }
                *d = nd;
            }
            (id::PATTERN_CTRL, Value::Data(d)) => {
                // control events aimed at a channel follow it, and go with it when it is deleted
                let mut nd = Vec::with_capacity(d.len());
                for r in d.chunks_exact(12) {
                    let dest = u16at(r, 6);
                    if dest >= 0x2000 {
                        nd.extend_from_slice(r);
                    } else if let Some(&to) = remap.get(&dest) {
                        let mut r = r.to_vec();
                        put_u16(&mut r, 6, to);
                        nd.extend(r);
                    }
                }
                *d = nd;
            }
            (id::PLAYLIST, Value::Data(d)) => {
                if let Some(size) = crate::flp::old_playlist_size(d, (0, 0, 0, 0)).or(item_size(d)) {
                    let mut nd = Vec::with_capacity(d.len());
                    for item in d.chunks_exact(size) {
                        let ix = u16at(item, 6);
                        if ix > ITEM_BASE || u16at(item, 4) == 0 {
                            nd.extend_from_slice(item);
                        } else if let Some(&to) = remap.get(&ix) {
                            let mut item = item.to_vec();
                            put_u16(&mut item, 6, to);
                            nd.extend(item);
                        }
                    }
                    *d = nd;
                }
            }
            _ => {}
        }
    }
    // links (automation clips and what they drive) and layer children also name channels
    let known = |c: u16| (c as usize) < n_orig;
    out.retain_mut(|e| match (e.id, &mut e.value) {
        (id::LINK, Value::Data(d)) if d.len() >= 12 => {
            if u16at(d, 0) == 0 && u32at(d, 4) == 0 {
                let src = u16at(d, 2);
                match remap.get(&src) {
                    Some(&to) => put_u16(d, 2, to),
                    None if known(src) => return false,
                    None => {}
                }
            }
            let dest = u16at(d, 10);
            if dest < 0x2000 {
                match remap.get(&dest) {
                    Some(&to) => put_u16(d, 10, to),
                    None if known(dest) => return false,
                    None => {}
                }
            }
            true
        }
        (id::LAYER_CHILD, Value::Word(c)) => match remap.get(c) {
            Some(&to) => {
                *c = to;
                true
            }
            None => !known(*c),
        },
        _ => true,
    });
    file.events = out;
    file.header.channels = song.channels.len() as u16;
}

// ---------------------------------------------------------------------------------------------------------------
// The patch
// ---------------------------------------------------------------------------------------------------------------

fn set_or_insert(ev: &mut Vec<Event>, range: std::ops::Range<usize>, e: Event, fallback_at: usize) {
    match (range.start..range.end.min(ev.len())).find(|&i| ev[i].id == e.id) {
        Some(i) => ev[i] = e,
        None => ev.insert(fallback_at.min(ev.len()), e),
    }
}

fn patch_head(ev: &mut Vec<Event>, song: &Song, base: &Song, unicode: bool) {
    let head_end = ev.iter().position(|e| matches!(e.id, 64 | 65 | 99 | 233 | 236)).unwrap_or(ev.len());
    let after_version = ev.iter().position(|e| e.id == id::VERSION).map(|i| i + 1).unwrap_or(0).min(head_end);
    let milli = |t: f32| (t as f64 * 1000.0).round() as u32;
    if milli(song.tempo) != milli(base.tempo) {
        if let Some(e) = ev.iter_mut().find(|e| e.id == id::TEMPO) {
            e.value = Value::Int(milli(song.tempo));
        } else if let Some(e) = ev.iter_mut().find(|e| e.id == id::TEMPO_WORD) {
            e.value = Value::Word(song.tempo.round() as u16);
        } else {
            ev.insert(after_version, Event::int(id::TEMPO, milli(song.tempo)));
        }
    }
    if song.main_pitch != base.main_pitch {
        set_or_insert(ev, 0..head_end, Event::word(id::MAIN_PITCH, song.main_pitch as u16), after_version);
    }
    if song.song_mode != base.song_mode {
        set_or_insert(ev, 0..head_end, Event::byte(id::LOOP_ACTIVE, song.song_mode as u8), after_version);
    }
    if song.performance != base.performance {
        set_or_insert(ev, 0..head_end, Event::byte(id::PERFORMANCE, song.performance as u8), after_version);
    }
    let head_end = ev.iter().position(|e| matches!(e.id, 64 | 65 | 99 | 233 | 236)).unwrap_or(ev.len());
    if song.time_sig != base.time_sig {
        set_or_insert(ev, 0..head_end, Event::byte(id::TIMESIG_NUM, song.time_sig.0), after_version);
        let head_end = ev.iter().position(|e| matches!(e.id, 64 | 65 | 99 | 233 | 236)).unwrap_or(ev.len());
        set_or_insert(ev, 0..head_end, Event::byte(id::TIMESIG_BEAT, song.time_sig.1), after_version);
    }
    for (eid, now, was) in [(id::TITLE, &song.title, &base.title), (id::GENRE, &song.genre, &base.genre), (id::ARTIST, &song.artist, &base.artist), (id::COMMENT, &song.comment, &base.comment)] {
        if now != was {
            let e = text_event(eid, now, unicode);
            match ev.iter().position(|x| x.id == eid) {
                Some(i) => ev[i] = e,
                None => {
                    let at = ev.iter().position(|e| matches!(e.id, 64 | 65 | 99 | 233 | 236)).unwrap_or(ev.len());
                    ev.insert(at, e);
                }
            }
        }
    }
}

fn patch_channels(ev: &mut Vec<Event>, song: &Song, base: &Song, unicode: bool, word_insert: bool) {
    let old_levels = ev.iter().find(|e| e.id == id::VERSION).and_then(|e| e.bytes()).map(|b| version_tuple(&latin1(b))).is_some_and(|v| v < (7, 0, 0, 0));
    // channel runs, last first so earlier indices stay good while events are added or removed
    let starts: Vec<usize> = ev.iter().enumerate().filter(|(_, e)| e.id == id::NEW_CHANNEL).map(|(i, _)| i).collect();
    for (ci, &start) in starts.iter().enumerate().rev() {
        let (Some(now), Some(was)) = (song.channels.get(ci), base.channels.get(ci)) else { continue };
        let mut end = start + 1;
        while end < ev.len() && !ends_channel(ev[end].id) {
            end += 1;
        }
        let find = |ev: &Vec<Event>, end: usize, eid: u8| (start..end).find(|&i| ev[i].id == eid);

        // a sample path sits at the very end of the run; do it first so `end` stays right for the rest
        if now.sample_path != was.sample_path {
            match (find(ev, end, id::SAMPLE_PATH), now.sample_path.is_empty()) {
                (Some(i), true) => {
                    ev.remove(i);
                    end -= 1;
                }
                (Some(i), false) => ev[i] = text_event(id::SAMPLE_PATH, &now.sample_path, unicode),
                (None, true) => {}
                (None, false) => {
                    let at = find(ev, end, id::CHAN_PING_PONG).map(|i| i + 1).unwrap_or(end);
                    ev.insert(at, text_event(id::SAMPLE_PATH, &now.sample_path, unicode));
                    end += 1;
                }
            }
        }
        if now.kind != was.kind && matches!(now.kind, ChannelKind::AudioClip | ChannelKind::Sampler) {
            if let Some(i) = find(ev, end, id::CHAN_TYPE) {
                ev[i].value = Value::Byte(if now.kind == ChannelKind::AudioClip { 4 } else { 0 });
            }
        }
        let (group, group_was) = (song.group_of(ci), base.group_of(ci));
        if group != group_was && !song.groups.is_empty() {
            match find(ev, end, id::CHAN_GROUP) {
                Some(i) => ev[i].value = Value::Int(group as u32),
                None => {
                    let at = find(ev, end, id::CHAN_CUT_GROUP).map(|i| i + 1).unwrap_or(start + 1);
                    ev.insert(at, Event::int(id::CHAN_GROUP, group as u32));
                    end += 1;
                }
            }
        }
        if now.zipped != was.zipped {
            match find(ev, end, id::CHAN_ZIPPED) {
                Some(i) => ev[i].value = Value::Byte(now.zipped as u8),
                None => {
                    ev.insert(end, Event::byte(id::CHAN_ZIPPED, now.zipped as u8));
                    end += 1;
                }
            }
        }
        if now.enabled != was.enabled {
            match find(ev, end, id::CHAN_ENABLED) {
                Some(i) => ev[i].value = Value::Byte(now.enabled as u8),
                None => {
                    ev.insert(start + 1, Event::byte(id::CHAN_ENABLED, now.enabled as u8));
                    end += 1;
                }
            }
        }
        if now.name != was.name {
            // the display name is the first name event of the run (effect slots come much later in the file)
            match find(ev, end, id::CHAN_NAME).or(find(ev, end, id::PLUGIN_NAME)) {
                Some(i) => ev[i] = text_event(ev[i].id, &now.name, unicode),
                None => {
                    let at = find(ev, end, id::NEW_PLUGIN).or(find(ev, end, id::PLUGIN_INTERNAL_NAME)).or(find(ev, end, id::CHAN_TYPE)).map(|i| i + 1).unwrap_or(start + 1);
                    ev.insert(at, text_event(id::PLUGIN_NAME, &now.name, unicode));
                    end += 1;
                }
            }
        }
        if let (Some(sl), true) = (now.slices.as_ref(), now.slices != was.slices && now.plugin == was.plugin) {
            // a loop slicer whose cuts were moved: the new places go into its own block
            if let Some(i) = find(ev, end, id::PLUGIN_PARAMS) {
                if let Value::Data(d) = &mut ev[i].value {
                    if now.plugin.eq_ignore_ascii_case("Fruity Slicer") {
                        sl.store(d);
                    }
                }
            }
        }
        if let (Some(bay), Some(g), true) = (now.bay(), now.gen.as_ref(), was.bay().is_some()) {
            // still a patch bay: only its instrument's settings go back, inside the bay's own block
            let held = bay.fx.sub != was.bay().and_then(|b| b.fx.sub.clone());
            if now.gen != was.gen || held {
                if let Some(i) = find(ev, end, id::PLUGIN_PARAMS) {
                    if let Value::Data(d) = &mut ev[i].value {
                        // the held units that changed first, then the instrument's own settings
                        if let (true, Some(sub)) = (held, bay.fx.sub.as_ref()) {
                            *d = crate::fxpatch::encode(sub, Some(d));
                        }
                        if now.gen != was.gen {
                            *d = crate::fxpatch::encode_gen(d, bay.lead, g);
                        }
                    }
                }
            }
        } else if let Some(block) = now.zones.as_ref().and_then(|z| z.kit.as_ref()).filter(|b| was.zones.as_ref().and_then(|z| z.kit.as_ref()) != Some(b) || now.kind != was.kind) {
            // a pad machine whose pads were edited (or a new one): the channel carries the block as it now stands
            let ident = crate::zones::Kit::IDENT;
            if let Some(i) = find(ev, end, id::CHAN_TYPE) {
                ev[i].value = Value::Byte(2);
            }
            let at = match find(ev, end, id::PLUGIN_INTERNAL_NAME) {
                Some(i) => {
                    ev[i] = text_event(id::PLUGIN_INTERNAL_NAME, ident, unicode);
                    i
                }
                None => {
                    let at = find(ev, end, id::CHAN_TYPE).map(|i| i + 1).unwrap_or(start + 1);
                    ev.insert(at, text_event(id::PLUGIN_INTERNAL_NAME, ident, unicode));
                    end += 1;
                    at
                }
            };
            if find(ev, end, id::NEW_PLUGIN).is_none() {
                let mut h = vec![0u8; 52];
                put_u32(&mut h, 8, 2);
                put_u32(&mut h, 16, 0x50);
                put_u32(&mut h, 36, 128);
                put_u32(&mut h, 40, 410);
                put_u32(&mut h, 44, 558);
                put_u32(&mut h, 48, 389);
                ev.insert(at + 1, Event::data(id::NEW_PLUGIN, h));
                end += 1;
            }
            match find(ev, end, id::PLUGIN_PARAMS) {
                Some(i) => ev[i].value = Value::Data(block.clone()),
                None => {
                    let at = find(ev, end, id::COLOR).or(find(ev, end, id::PLUGIN_NAME)).map(|i| i + 1).unwrap_or(at + 2);
                    ev.insert(at, Event::data(id::PLUGIN_PARAMS, block.clone()));
                    end += 1;
                }
            }
        } else if let Some(info) = now.zones.as_ref().and_then(|z| z.bank.as_ref()) {
            // a sound bank player: its block goes back when the file or the preset was changed
            let ident = "SoundFont Player";
            if !was.zones.as_ref().is_some_and(|z| z.bank.is_some()) {
                if let Some(i) = find(ev, end, id::CHAN_TYPE) {
                    ev[i].value = Value::Byte(2);
                }
                let at = match find(ev, end, id::PLUGIN_INTERNAL_NAME) {
                    Some(i) => {
                        ev[i] = text_event(id::PLUGIN_INTERNAL_NAME, ident, unicode);
                        i
                    }
                    None => {
                        let at = find(ev, end, id::CHAN_TYPE).map(|i| i + 1).unwrap_or(start + 1);
                        ev.insert(at, text_event(id::PLUGIN_INTERNAL_NAME, ident, unicode));
                        end += 1;
                        at
                    }
                };
                if find(ev, end, id::NEW_PLUGIN).is_none() {
                    let mut h = vec![0u8; 52];
                    put_u32(&mut h, 8, 2);
                    put_u32(&mut h, 16, 0x50);
                    put_u32(&mut h, 36, 128);
                    put_u32(&mut h, 40, 410);
                    put_u32(&mut h, 44, 558);
                    put_u32(&mut h, 48, 389);
                    ev.insert(at + 1, Event::data(id::NEW_PLUGIN, h));
                    end += 1;
                }
                if find(ev, end, id::PLUGIN_PARAMS).is_none() {
                    let at = find(ev, end, id::COLOR).or(find(ev, end, id::PLUGIN_NAME)).map(|i| i + 1).unwrap_or(at + 2);
                    ev.insert(at, Event::data(id::PLUGIN_PARAMS, info.block.clone()));
                    end += 1;
                }
            }
            if let Some(i) = find(ev, end, id::PLUGIN_PARAMS) {
                if let Value::Data(d) = &mut ev[i].value {
                    if crate::sf2::setting(d).as_ref() != Some(&info.setting) {
                        *d = info.block.clone();
                    }
                }
            }
        } else if let Some(g) = now.ext.as_ref().map(Ok).or_else(|| now.gen.as_ref().filter(|_| now.gen != was.gen || now.plugin != was.plugin || now.kind != was.kind).map(Err)) {
            // a built-in instrument, or a hosted one (whose settings live in the plug-in, so it is always
            // written): the channel becomes (or stays) a generator channel carrying its settings
            let ident = match g {
                Ok(_) => crate::song::WRAPPER,
                // the silent kinds are many units told apart by the channel's own plug-in name
                Err(g) if g.kind == crate::synth::Kind::Quiet && !now.plugin.is_empty() => now.plugin.as_str(),
                Err(g) => crate::synth::def(g.kind).ident,
            };
            let encode = |base: Option<&[u8]>| match g {
                Ok(h) => crate::host::block(&h.0, base),
                Err(g) => g.encode(base),
            };
            if let Some(i) = find(ev, end, id::CHAN_TYPE) {
                ev[i].value = Value::Byte(2);
            }
            let at = match find(ev, end, id::PLUGIN_INTERNAL_NAME) {
                Some(i) => {
                    ev[i] = text_event(id::PLUGIN_INTERNAL_NAME, ident, unicode);
                    i
                }
                None => {
                    let at = find(ev, end, id::CHAN_TYPE).map(|i| i + 1).unwrap_or(start + 1);
                    ev.insert(at, text_event(id::PLUGIN_INTERNAL_NAME, ident, unicode));
                    end += 1;
                    at
                }
            };
            if find(ev, end, id::NEW_PLUGIN).is_none() {
                let mut h = vec![0u8; 52];
                put_u32(&mut h, 8, 2);
                put_u32(&mut h, 16, 0x50);
                put_u32(&mut h, 36, 128);
                put_u32(&mut h, 40, 410);
                put_u32(&mut h, 44, 558);
                put_u32(&mut h, 48, 389);
                ev.insert(at + 1, Event::data(id::NEW_PLUGIN, h));
                end += 1;
            }
            match find(ev, end, id::PLUGIN_PARAMS) {
                Some(i) => {
                    let keep = was.plugin == now.plugin;
                    if let Value::Data(d) = &mut ev[i].value {
                        *d = encode(keep.then_some(d.as_slice()));
                    }
                }
                None => {
                    let at = find(ev, end, id::COLOR).or(find(ev, end, id::PLUGIN_NAME)).map(|i| i + 1).unwrap_or(at + 2);
                    ev.insert(at, Event::data(id::PLUGIN_PARAMS, encode(None)));
                    end += 1;
                }
            }
        }
        if now.color != was.color {
            match find(ev, end, id::COLOR) {
                Some(i) => {
                    let top = ev[i].as_u32().unwrap_or(0);
                    ev[i].value = Value::Int(color_bits(now.color, top));
                }
                None => {
                    ev.insert(start + 1, Event::int(id::COLOR, color_bits(now.color, 0)));
                    end += 1;
                }
            }
        }
        if now.insert != was.insert {
            if let Some(i) = find(ev, end, id::CHAN_TARGET_INSERT_WORD) {
                ev[i].value = Value::Word(now.insert as u16);
            } else if let Some(i) = find(ev, end, id::CHAN_TARGET_INSERT) {
                ev[i].value = Value::Byte(now.insert.min(127) as u8);
            } else {
                let at = find(ev, end, id::CHAN_LEVELS).unwrap_or(end);
                ev.insert(at, if word_insert { Event::word(id::CHAN_TARGET_INSERT_WORD, now.insert as u16) } else { Event::byte(id::CHAN_TARGET_INSERT, now.insert.min(127) as u8) });
                end += 1;
            }
        }
        let mut levels = (pan_u(now.pan), vol_u(now.volume), now.pitch_cents.round() as i32);
        if old_levels {
            // up to version 6 pan and volume are stored in 128 steps
            levels.0 = (levels.0 + 50) / 100;
            levels.1 = (levels.1 + 50) / 100;
        }
        if levels != (pan_u(was.pan), vol_u(was.volume), was.pitch_cents.round() as i32) {
            match find(ev, end, id::CHAN_LEVELS) {
                Some(i) => {
                    if let Value::Data(d) = &mut ev[i].value {
                        if d.len() < 12 {
                            d.resize(12, 0);
                        }
                        if pan_u(now.pan) != pan_u(was.pan) {
                            put_u32(d, 0, levels.0);
                        }
                        if vol_u(now.volume) != vol_u(was.volume) {
                            put_u32(d, 4, levels.1);
                        }
                        put_u32(d, 8, levels.2 as u32);
                    }
                }
                None => {
                    let mut d = hex("001900001027000000000000000100000000000000000000");
                    put_u32(&mut d, 0, levels.0);
                    put_u32(&mut d, 4, levels.1);
                    put_u32(&mut d, 8, levels.2 as u32);
                    ev.insert(end, Event::data(id::CHAN_LEVELS, d));
                    end += 1;
                }
            }
        }
        if now.root_key != was.root_key {
            if let Some(i) = find(ev, end, id::CHAN_PARAMS) {
                if let Value::Data(d) = &mut ev[i].value {
                    put_u32(d, 16, now.root_key as u32);
                }
            }
            // early files keep the root note in an event of its own
            if let Some(i) = find(ev, end, id::CHAN_ROOT) {
                ev[i].value = Value::Int(now.root_key as u32);
            }
        }
        if now.env_on != was.env_on {
            if let Some(i) = (start..end).filter(|&i| ev[i].id == id::CHAN_ENVELOPE).nth(1) {
                if let Value::Data(d) = &mut ev[i].value {
                    put_u32(d, 4, now.env_on as u32);
                }
            }
        }
        if now.smp != was.smp {
            patch_sampler(ev, start, end, &now.smp, &was.smp, old_levels);
        }
    }
}

/// The record of an automation clip's points (event 234). The head and the tail of `old` are kept; a fresh
/// record gets the ones every clip in a current project carries.
fn automation_bytes(points: &[crate::song::AutoPt], ppq: u32, old: Option<&[u8]>) -> Vec<u8> {
    let old_n = old.filter(|d| d.len() >= 21).map(|d| u32at(d, 17) as usize);
    let mut d: Vec<u8> = match old.filter(|d| d.len() >= 21) {
        Some(o) => o[..17].to_vec(),
        None => hex("0100000040000000000400000003000000"),
    };
    d.extend_from_slice(&(points.len() as u32).to_le_bytes());
    let mut prev = 0.0f64;
    for (i, p) in points.iter().enumerate() {
        let delta = if i == 0 { 0.0 } else { (p.tick - prev) / ppq.max(1) as f64 };
        prev = p.tick;
        d.extend_from_slice(&delta.to_le_bytes());
        d.extend_from_slice(&(p.value as f64).to_le_bytes());
        d.extend_from_slice(&p.tension.to_le_bytes());
        // the shape, then which way the line bends: 1 up, 255 down, 2 for a straight one
        let bend = if i == 0 { 0 } else if p.tension > 0.0 { 1 } else if p.tension < 0.0 { 255 } else { 2 };
        d.extend_from_slice(&[p.mode, 0, 0, bend]);
    }
    match (old, old_n) {
        (Some(o), Some(n)) if o.len() >= 21 + 24 * n => d.extend_from_slice(&o[21 + 24 * n..]),
        _ => d.extend(hex("01000000ffffffffffffffffffffffffffffffff800000008000000000000000800000000500000003000000010000000000000000000000000000000000f03f00000000000000000100000000000000fffffffffffffffffffffffffbb2000000000000000000000000000000000000")),
    }
    d
}

/// Automation clips: the points of each and the controls it drives.
fn patch_auto(ev: &mut Vec<Event>, song: &Song, base: &Song) {
    let starts: Vec<usize> = ev.iter().enumerate().filter(|(_, e)| e.id == id::NEW_CHANNEL).map(|(i, _)| i).collect();
    let mut relink: Vec<usize> = vec![];
    for (ci, &start) in starts.iter().enumerate().rev() {
        let Some(now) = song.channels.get(ci) else { continue };
        let Some(auto) = now.auto.as_ref().filter(|_| now.kind == ChannelKind::AutomationClip) else { continue };
        let was = base.channels.get(ci);
        let was_auto = was.and_then(|c| c.auto.as_ref());
        let mut end = start + 1;
        while end < ev.len() && !ends_channel(ev[end].id) {
            end += 1;
        }
        let find = |ev: &Vec<Event>, eid: u8| (start..end).find(|&i| ev[i].id == eid);
        if was.map(|c| &c.kind) != Some(&ChannelKind::AutomationClip) {
            if let Some(i) = find(ev, id::CHAN_TYPE) {
                ev[i].value = Value::Byte(5);
            }
        }
        if was_auto.map(|a| &a.points) != Some(&auto.points) {
            let at = find(ev, id::CHAN_AUTOMATION);
            let old = at.and_then(|i| match &ev[i].value {
                Value::Data(d) => Some(d.clone()),
                _ => None,
            });
            let d = automation_bytes(&auto.points, song.ppq, old.as_deref());
            match at {
                Some(i) => ev[i] = Event::data(id::CHAN_AUTOMATION, d),
                None => {
                    let i = find(ev, id::CHAN_ENVELOPE).unwrap_or(end);
                    ev.insert(i, Event::data(id::CHAN_AUTOMATION, d));
                }
            }
        }
        if was_auto.map(|a| &a.targets) != Some(&auto.targets) {
            relink.push(ci);
        }
    }
    if relink.is_empty() {
        return;
    }
    // the links of the clips whose targets changed are written afresh, ahead of the first channel
    ev.retain(|e| match (e.id, &e.value) {
        (id::LINK, Value::Data(d)) if d.len() >= 12 => !(u16at(d, 0) == 0 && u32at(d, 4) == 0 && relink.contains(&(u16at(d, 2) as usize))),
        _ => true,
    });
    let at = ev.iter().position(|e| e.id == id::NEW_CHANNEL).unwrap_or(ev.len());
    let mut fresh = vec![];
    relink.sort_unstable();
    for ci in relink {
        for t in &song.channels[ci].auto.as_ref().unwrap().targets {
            let (param, dest) = song.event_address(*t);
            let mut d = vec![0u8; 12];
            put_u16(&mut d, 2, ci as u16);
            put_u16(&mut d, 8, param);
            put_u16(&mut d, 10, dest);
            d.extend(hex("08000000d5010000"));
            fresh.push(Event::data(id::LINK, d));
        }
    }
    ev.splice(at..at, fresh);
}

/// Hardware controller links: when the list changed, the records are written again where the old ones
/// stood (ahead of the channels when there were none). A record that still stands keeps its own bytes.
/// Links from a sender unit in an effect slot to the control it drives: records for links that went are taken
/// out, new links get a record of their own, and the rest stay as they were.
fn patch_ctl_links(ev: &mut Vec<Event>, song: &Song, base: &Song, mixer_base: usize) {
    if song.ctl_links == base.ctl_links {
        return;
    }
    let record = |l: &crate::song::CtlLink| {
        let (param, dest) = song.event_address(l.target);
        let mut d = vec![0u8; 12];
        match l.chan() {
            Some(c) => {
                put_u16(&mut d, 0, 0x2000 | l.out);
                put_u16(&mut d, 2, c as u16);
            }
            None => {
                put_u16(&mut d, 0, 0x8000 | l.out);
                put_u16(&mut d, 2, (((l.ins + mixer_base) as u16) << 6) | (l.slot & 63));
            }
        }
        put_u16(&mut d, 8, param);
        put_u16(&mut d, 10, dest);
        d
    };
    let gone: Vec<Vec<u8>> = base.ctl_links.iter().filter(|l| !song.ctl_links.contains(l)).map(|l| {
        let (param, dest) = base.event_address(l.target);
        let mut d = record(l);
        put_u16(&mut d, 8, param);
        put_u16(&mut d, 10, dest);
        d
    }).collect();
    ev.retain(|e| match (&e.value, e.id) {
        (Value::Data(d), id::LINK) if d.len() >= 12 => !gone.iter().any(|g| g[..4] == d[..4] && g[8..12] == d[8..12]),
        _ => true,
    });
    let at = ev.iter().rposition(|e| e.id == id::LINK).map(|i| i + 1).unwrap_or_else(|| ev.iter().position(|e| e.id == id::NEW_CHANNEL).unwrap_or(ev.len()));
    let fresh: Vec<Event> = song.ctl_links.iter().filter(|l| !base.ctl_links.contains(l)).map(|l| {
        let mut d = record(l);
        d.extend(hex("08000000d5010000"));
        Event::data(id::LINK, d)
    }).collect();
    ev.splice(at..at, fresh);
}

/// Mapping formulas sit right after the link they belong to. One that still says what the song says keeps
/// its bytes; the rest are written, changed or taken out, and one left without its link goes.
fn patch_link_formulas(ev: &mut Vec<Event>, song: &Song, unicode: bool) {
    let mut after_link: Option<Vec<&str>> = None;
    let mut out: Vec<Event> = Vec::with_capacity(ev.len());
    let all = std::mem::take(ev);
    for e in all {
        if e.id == id::LINK_FORMULA {
            let keep = match (&after_link, &e.value) {
                (Some(want), Value::Data(d)) => want.contains(&flp::formula_text(d).as_str()),
                _ => false,
            };
            if keep {
                after_link = None;
                out.push(e);
            }
            // one that no longer fits is dropped here; the wanted text is written when the next record comes
            continue;
        }
        if let Some(want) = after_link.take() {
            if let Some(text) = want.first() {
                let d: Vec<u8> = if unicode { text.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect() } else { text.bytes().chain([0]).collect() };
                out.push(Event::data(id::LINK_FORMULA, d));
            }
        }
        if let (id::LINK, Value::Data(d)) = (e.id, &e.value) {
            if d.len() >= 12 {
                let addr = (u16at(d, 8), u16at(d, 10));
                let want: Vec<&str> = song.link_maps.iter().filter(|m| song.event_address(m.target) == addr).map(|m| m.text.as_str()).collect();
                after_link = Some(want);
            }
        }
        out.push(e);
    }
    *ev = out;
}

fn patch_midi_links(ev: &mut Vec<Event>, song: &Song, base: &Song) {
    if song.midi_links == base.midi_links {
        return;
    }
    let key = |d: &[u8]| (d.len() >= 12).then(|| [d[0], d[1], d[2], d[3], d[8], d[9], d[10], d[11]]);
    let mut old: Vec<([u8; 8], Vec<u8>)> = vec![];
    let mut at = None;
    let mut i = 0;
    while i < ev.len() {
        if ev[i].id == id::MIDI_LINK {
            at.get_or_insert(i);
            if let Value::Data(d) = ev.remove(i).value {
                if let Some(k) = key(&d) {
                    old.push((k, d));
                }
            }
        } else {
            i += 1;
        }
    }
    let at = at.unwrap_or_else(|| ev.iter().position(|e| e.id == id::LINK || e.id == id::NEW_CHANNEL).unwrap_or(ev.len()));
    let fresh: Vec<Event> = song
        .midi_links
        .iter()
        .map(|l| {
            let (param, dest) = song.event_address(l.target);
            let mut d = vec![0u8; 12];
            put_u16(&mut d, 0, l.ctrl);
            put_u16(&mut d, 2, l.port);
            put_u16(&mut d, 8, param);
            put_u16(&mut d, 10, dest);
            d.extend(hex("04000000d5010000"));
            match old.iter().position(|o| Some(o.0) == key(&d)) {
                Some(k) => Event::data(id::MIDI_LINK, old.remove(k).1),
                None => Event::data(id::MIDI_LINK, d),
            }
        })
        .collect();
    ev.splice(at..at, fresh);
}

fn env_raw(seconds: f32) -> u32 {
    if seconds <= 0.0 {
        return 100;
    }
    ((seconds / 16.0).cbrt() * 65536.0).round().clamp(100.0, 65536.0) as u32
}

/// Write the sampler settings that changed into the events of one channel (`start..end`).
fn patch_sampler(ev: &mut Vec<Event>, start: usize, mut end: usize, now: &Sampler, was: &Sampler, old_levels: bool) {
    use crate::song::Env;
    // a word or int setting: replace it, or add it ahead of the levels block
    fn put(ev: &mut Vec<Event>, start: usize, end: &mut usize, e: Event) {
        match (start..*end).find(|&i| ev[i].id == e.id) {
            Some(i) => ev[i].value = e.value,
            None => {
                let at = (start..*end).find(|&i| ev[i].id == id::CHAN_LEVELS).unwrap_or(*end);
                ev.insert(at, e);
                *end += 1;
            }
        }
    }
    // a block: change it in place, or add `blank` after the channel parameters first
    fn block(ev: &mut Vec<Event>, start: usize, end: &mut usize, eid: u8, blank: &str, f: impl FnOnce(&mut Vec<u8>)) {
        let at = match (start..*end).find(|&i| ev[i].id == eid) {
            Some(i) => i,
            None => {
                let at = (start..*end).find(|&i| ev[i].id == id::CHAN_PARAMS).map(|i| i + 1).unwrap_or(*end);
                ev.insert(at, Event::data(eid, hex(blank)));
                *end += 1;
                at
            }
        };
        if let Value::Data(d) = &mut ev[at].value {
            f(d);
        }
    }
    let q = |v: f32, scale: f32| (v * scale).round() as i32;

    let flags = |f: &Sampler| (f.reverse as u16) << 1 | (f.swap_stereo as u16) << 8;
    if flags(now) != flags(was) {
        let old = (start..end).find(|&i| ev[i].id == id::CHAN_FX_FLAGS).and_then(|i| ev[i].as_u32()).unwrap_or(0) as u16;
        put(ev, start, &mut end, Event::word(id::CHAN_FX_FLAGS, (old & !0x102) | flags(now)));
    }
    if q(now.fade_in, 1024.0) != q(was.fade_in, 1024.0) {
        put(ev, start, &mut end, Event::word(id::CHAN_FADE_IN, q(now.fade_in, 1024.0) as u16));
    }
    if q(now.fade_out, 1024.0) != q(was.fade_out, 1024.0) {
        put(ev, start, &mut end, Event::word(id::CHAN_FADE_OUT, q(now.fade_out, 1024.0) as u16));
    }
    if q(now.swing, 128.0) != q(was.swing, 128.0) {
        put(ev, start, &mut end, Event::word(id::CHAN_SWING, q(now.swing, 128.0) as u16));
    }
    if (now.cut, now.cut_by) != (was.cut, was.cut_by) {
        put(ev, start, &mut end, Event::int(id::CHAN_CUT_GROUP, now.cut as u32 | (now.cut_by as u32) << 16));
    }
    let filt = |f: &Sampler| (q(f.cutoff, 256.0), q(f.resonance, 256.0), f.filter);
    if filt(now) != filt(was) {
        let (c, r, t) = filt(now);
        block(ev, start, &mut end, id::CHAN_LEVELS, "001900001027000000000000000100000000000000000000", |d| {
            if d.len() < 24 {
                d.resize(24, 0);
            }
            put_u32(d, 12, c as u32);
            put_u32(d, 16, r as u32);
            put_u32(d, 20, t as u32);
        });
    }
    let poly = |f: &Sampler| (q(f.slide, 5000.0), f.mono as u8 | (f.porta as u8) << 1);
    if poly(now) != poly(was) {
        let (slide, fl) = poly(now);
        block(ev, start, &mut end, id::CHAN_POLY, "00000000f401000000", |d| {
            if d.len() < 9 {
                d.resize(9, 0);
            }
            put_u32(d, 4, slide as u32);
            d[8] = (d[8] & !3) | fl;
        });
    }
    let echo = |f: &Sampler| (q(f.echo_feedback, 12800.0), q(f.echo_pan, 6400.0) + 6400, q(f.echo_cents, 1.0), f.echoes, q(f.echo_steps, 48.0));
    if echo(now) != echo(was) {
        let (mut fb, mut pan, cents, n, time) = echo(now);
        if old_levels {
            fb = (fb + 50) / 100;
            pan = (pan + 50) / 100;
        }
        block(ev, start, &mut end, id::CHAN_ECHO, "0000000000190000000000000400000090000000", |d| {
            if d.len() < 20 {
                d.resize(20, 0);
            }
            put_u32(d, 0, fb as u32);
            put_u32(d, 4, pan as u32);
            put_u32(d, 8, cents as u32);
            put_u32(d, 12, n as u32);
            put_u32(d, 16, time as u32);
        });
    }
    if q(now.shift, 1024.0) != q(was.shift, 1024.0) {
        put(ev, start, &mut end, Event::word(id::CHAN_TIME_SHIFT, q(now.shift, 1024.0) as u16));
    }
    let arp = |f: &Sampler| (f.arp_dir, f.arp_range, f.arp_chord, crate::song::steps_knob(f.arp_steps), q(f.arp_gate, 48.0), f.arp_slide, f.arp_repeat, if f.gate_steps >= crate::song::GATE_OFF { 1447 } else { crate::song::steps_knob(f.gate_steps) }, f.full_porta);
    if arp(now) != arp(was) {
        if let Some(i) = (start..end).find(|&i| ev[i].id == id::CHAN_PARAMS) {
            if let Value::Data(d) = &mut ev[i].value {
                if d.len() >= 96 {
                    let (a, b) = (arp(now), arp(was));
                    put_u32(d, 40, a.0 as u32);
                    put_u32(d, 44, a.1 as u32);
                    put_u32(d, 48, a.2 as i32 as u32);
                    if a.3 != b.3 {
                        put_u32(d, 52, a.3 as u32);
                    }
                    put_u32(d, 56, a.4 as u32);
                    d[60] = a.5 as u8;
                    put_u32(d, 92, a.6 as u32);
                    if a.7 != b.7 {
                        d[64..66].copy_from_slice(&(a.7 as u16).to_le_bytes());
                    }
                    d[62] = a.8 as u8;
                }
            }
        }
    }
    if (now.add_to_key, now.key_lo, now.key_hi) != (was.add_to_key, was.key_lo, was.key_hi) {
        if let Some(i) = (start..end).find(|&i| ev[i].id == id::CHAN_PARAMS) {
            if let Value::Data(d) = &mut ev[i].value {
                if d.len() >= 76 {
                    d[63] = now.add_to_key as u8;
                    put_u32(d, 68, now.key_lo);
                    put_u32(d, 72, now.key_hi);
                }
            }
        }
    }
    if (now.echo_ping, now.echo_fat) != (was.echo_ping, was.echo_fat) {
        if let Some(i) = (start..end).find(|&i| ev[i].id == id::CHAN_PARAMS) {
            if let Value::Data(d) = &mut ev[i].value {
                if d.len() > 10 {
                    d[10] = (d[10] & !6) | (now.echo_ping as u8) << 1 | (now.echo_fat as u8) << 2;
                }
            }
        }
    }
    if (now.track_vel, now.track_key) != (was.track_vel, was.track_key) {
        let mut at: Vec<usize> = (start..end).filter(|&i| ev[i].id == id::CHAN_TRACKING).collect();
        if at.len() < 2 {
            // a channel made here has none yet: they sit just before the envelopes
            let first = (start..end).find(|&i| ev[i].id == id::CHAN_ENVELOPE).unwrap_or(end);
            for (k, mid) in [100u8, 60].iter().enumerate().skip(at.len()) {
                let mut d = vec![0u8; 16];
                d[0] = *mid;
                ev.insert(first + k, Event::data(id::CHAN_TRACKING, d));
                end += 1;
            }
            at = (start..end).filter(|&i| ev[i].id == id::CHAN_TRACKING).collect();
        }
        for (i, t) in at.iter().zip([&now.track_vel, &now.track_key]) {
            if let Value::Data(d) = &mut ev[*i].value {
                if d.len() >= 16 {
                    put_u32(d, 0, t[0].round() as u32);
                    for k in 1..4 {
                        put_u32(d, k * 4, q(t[k], 256.0) as u32);
                    }
                }
            }
        }
    }
    let par = |f: &Sampler| (f.normalize, f.invert, q(f.stretch_beats, 384.0), q(f.stretch_cents, 1.0), q(f.stretch_mul.max(1e-6).log2(), 10000.0), f.keep_pitch);
    if par(now) != par(was) {
        if let Some(i) = (start..end).find(|&i| ev[i].id == id::CHAN_PARAMS) {
            if let Value::Data(d) = &mut ev[i].value {
                if d.len() >= 112 {
                    let (a, b) = (par(now), par(was));
                    d[80] = a.0 as u8;
                    d[81] = a.1 as u8;
                    if a.2 != b.2 {
                        // newer files keep the length as a float, older ones as a whole number
                        let raw = u32::from_le_bytes([d[96], d[97], d[98], d[99]]);
                        let float = raw >= 0x3000_0000 || (raw == 0 && d.len() >= 156);
                        put_u32(d, 96, if float { (now.stretch_beats * 384.0).to_bits() } else { a.2 as u32 });
                    }
                    if a.3 != b.3 {
                        put_u32(d, 100, a.3 as u32);
                    }
                    if a.4 != b.4 {
                        put_u32(d, 104, a.4 as u32);
                    }
                    if a.5 != b.5 {
                        put_u32(d, 108, if a.5 { -1i32 as u32 } else { 0 });
                    }
                }
            }
        }
    }
    if now.ping_pong != was.ping_pong {
        match (start..end).find(|&i| ev[i].id == id::CHAN_PING_PONG) {
            Some(i) => ev[i].value = Value::Byte(now.ping_pong as u8),
            None => {
                ev.insert(end, Event::byte(id::CHAN_PING_PONG, now.ping_pong as u8));
                end += 1;
            }
        }
    }
    if now.use_loop != was.use_loop {
        if let Some(i) = (start..end).find(|&i| ev[i].id == id::SAMPLE_FLAGS) {
            if let Value::Int(v) = &mut ev[i].value {
                *v = (*v & !8) | if now.use_loop { 8 } else { 0 };
            }
        }
    }
    // envelopes: volume is the second block, the filter one the third, pitch the fifth
    let envs: [(usize, &Env, &Env, f32); 3] = [(1, &now.vol_env, &was.vol_env, 1.0), (2, &now.cut_env, &was.cut_env, 128.0), (4, &now.pitch_env, &was.pitch_env, 1.0)];
    for (nth, a, b, amount) in envs {
        if a == b {
            continue;
        }
        let Some(i) = (start..end).filter(|&i| ev[i].id == id::CHAN_ENVELOPE).nth(nth) else { continue };
        let Value::Data(d) = &mut ev[i].value else { continue };
        if d.len() < 36 {
            continue;
        }
        // the volume envelope's switch is the channel's own `env_on`
        if nth != 1 && a.on != b.on {
            put_u32(d, 4, if a.on { u32::MAX } else { 0 });
        }
        for (o, x, y) in [(8, a.delay, b.delay), (12, a.attack, b.attack), (16, a.hold, b.hold), (20, a.decay, b.decay), (28, a.release, b.release)] {
            if env_raw(x) != env_raw(y) {
                put_u32(d, o, env_raw(x));
            }
        }
        if q(a.sustain, 128.0) != q(b.sustain, 128.0) {
            put_u32(d, 24, q(a.sustain, 128.0) as u32);
        }
        if q(a.amount, amount) != q(b.amount, amount) {
            put_u32(d, 32, q(a.amount, amount) as u32);
        }
        if d.len() >= 68 {
            for k in 0..3 {
                if q(a.tension[k], 128.0) != q(b.tension[k], 128.0) {
                    put_u32(d, 56 + 4 * k, q(a.tension[k], 128.0) as u32);
                }
            }
        }
    }
}

fn patch_patterns(ev: &mut Vec<Event>, song: &Song, base: &Song, flp: &Flp, unicode: bool) {
    let n = song.patterns.len().max(base.patterns.len());
    for p in 0..n {
        let number = p as u16 + 1;
        let empty = crate::song::Pattern::default();
        let now = song.patterns.get(p).unwrap_or(&empty);
        let was = base.patterns.get(p).unwrap_or(&empty);

        if notes_key(&now.notes) != notes_key(&was.notes) {
            // what the file held for each note, so fields this program does not edit survive
            let mut old: HashMap<(u32, u16, u16), Vec<flp::Note>> = HashMap::new();
            if let Some(fp) = flp.patterns.iter().find(|fp| fp.index == number) {
                for note in fp.notes.iter().rev() {
                    old.entry((note.position, note.rack_channel, note.key)).or_default().push(note.clone());
                }
            }
            let mut sorted: Vec<&Note> = now.notes.iter().collect();
            sorted.sort_by_key(|n| (n.pos, n.chan, n.key));
            let old_notes = flp::note_size(flp.version_tuple()) == 20;
            let mut data = Vec::with_capacity(sorted.len() * NOTE_SIZE);
            for note in sorted {
                let mut f = old.get_mut(&(note.pos, note.chan, note.key as u16)).and_then(|v| v.pop()).unwrap_or(flp::Note {
                    position: 0,
                    flags: 0x4000,
                    rack_channel: 0,
                    length: 0,
                    key: 60,
                    group: 0,
                    fine_pitch: 120,
                    release: if old_notes { 0 } else { 64 },
                    midi_channel: 0,
                    pan: 64,
                    velocity: 100,
                    mod_x: 128,
                    mod_y: 128,
                });
                f.position = note.pos;
                f.rack_channel = note.chan;
                f.length = note.len;
                f.key = note.key as u16;
                f.velocity = vel_u(note.vel);
                f.pan = npan_u(note.pan);
                f.fine_pitch = fine_u(note.fine_cents);
                f.flags = if note.muted { f.flags | 0x2000 } else { f.flags & !0x2000 };
                f.flags = if note.slide { f.flags | 0x0008 } else { f.flags & !0x0008 };
                f.flags = if note.porta { f.flags | 0x0010 } else { f.flags & !0x0010 };
                f.release = (note.release * 128.0).round().clamp(0.0, 255.0) as u8;
                f.mod_x = (note.mod_x * 128.0 + 128.0).round().clamp(0.0, 255.0) as u8;
                f.mod_y = (note.mod_y * 128.0 + 128.0).round().clamp(0.0, 255.0) as u8;
                f.midi_channel = (f.midi_channel & !15) | (note.color & 15);
                f.group = note.group;
                if old_notes {
                    f.write_old(&mut data);
                } else {
                    f.write(&mut data);
                }
            }

            // the note events this pattern already has
            let mut cur = 0u16;
            let mut mine: Vec<usize> = vec![];
            for (i, e) in ev.iter().enumerate() {
                match (e.id, &e.value) {
                    (id::NEW_PATTERN, Value::Word(v)) => cur = *v,
                    (id::PATTERN_NOTES, _) if cur == number => mine.push(i),
                    _ => {}
                }
            }
            match mine.split_first() {
                Some((&first, rest)) => {
                    for &i in rest.iter().rev() {
                        ev.remove(i);
                    }
                    if data.is_empty() {
                        ev.remove(first);
                        // drop the selector too when nothing else hangs off it
                        if first > 0 && ev[first - 1].id == id::NEW_PATTERN && ev.get(first).map(|e| matches!(e.id, 64 | 65)).unwrap_or(true) {
                            ev.remove(first - 1);
                        }
                    } else {
                        ev[first].value = Value::Data(data);
                    }
                }
                None if !data.is_empty() => {
                    let at = ev.iter().position(|e| matches!(e.id, 64 | 99 | 233 | 236)).unwrap_or(ev.len());
                    ev.insert(at, Event::data(id::PATTERN_NOTES, data));
                    ev.insert(at, Event::word(id::NEW_PATTERN, number));
                }
                None => {}
            }
            // early projects keep step dots beside the channels; they are all notes now
            if flp.has_dots {
                let mut cur = 0u16;
                let mut in_channel = false;
                let mut i = 0;
                while i < ev.len() {
                    match (ev[i].id, &ev[i].value) {
                        (64, _) => in_channel = true,
                        (id::NEW_PATTERN, Value::Word(v)) => cur = *v,
                        (id::DOT | id::DOT_VOL | id::DOT_PAN | id::DOT_NOTE | id::DOT_PITCH | id::DOT_SHIFT | id::DOT_CUT | id::DOT_RES, Value::Word(_)) if in_channel && cur == number => {
                            ev.remove(i);
                            continue;
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
        }

        if now.events != was.events {
            let mut sorted = now.events.clone();
            sorted.sort_by_key(|e| e.tick);
            let mut data = Vec::with_capacity(sorted.len() * 12);
            for e in &sorted {
                data.extend_from_slice(&e.tick.to_le_bytes());
                data.extend_from_slice(&e.param.to_le_bytes());
                data.extend_from_slice(&e.dest.to_le_bytes());
                data.extend_from_slice(&e.raw.to_le_bytes());
            }
            // the pattern's selector in the first pattern list and the control events it already has
            let first_chan = ev.iter().position(|e| e.id == id::NEW_CHANNEL).unwrap_or(ev.len());
            let mut cur = 0u16;
            let (mut sel, mut mine): (Option<usize>, Vec<usize>) = (None, vec![]);
            for (i, e) in ev.iter().enumerate() {
                match (e.id, &e.value) {
                    (id::NEW_PATTERN, Value::Word(v)) => {
                        cur = *v;
                        if cur == number && i < first_chan && sel.is_none() {
                            sel = Some(i);
                        }
                    }
                    (id::PATTERN_CTRL, _) if cur == number => mine.push(i),
                    _ => {}
                }
            }
            match mine.split_first() {
                Some((&first, rest)) => {
                    for &i in rest.iter().rev() {
                        ev.remove(i);
                    }
                    if data.is_empty() {
                        ev.remove(first);
                        if first > 0 && ev[first - 1].id == id::NEW_PATTERN && ev.get(first).map(|e| matches!(e.id, 64 | 65)).unwrap_or(true) {
                            ev.remove(first - 1);
                        }
                    } else {
                        ev[first].value = Value::Data(data);
                    }
                }
                None if !data.is_empty() => match sel {
                    Some(i) => ev.insert(i + 1, Event::data(id::PATTERN_CTRL, data)),
                    None => {
                        let at = ev.iter().position(|e| matches!(e.id, 64 | 99 | 233 | 236)).unwrap_or(ev.len());
                        ev.insert(at, Event::data(id::PATTERN_CTRL, data));
                        ev.insert(at, Event::word(id::NEW_PATTERN, number));
                    }
                },
                None => {}
            }
        }

        if now.name != was.name || now.color != was.color {
            // name and colour live in a second pattern list after the first channel
            let first_chan = ev.iter().position(|e| e.id == id::NEW_CHANNEL).unwrap_or(0);
            let mut cur = 0u16;
            let (mut sel, mut name_at, mut color_at, mut last_meta) = (None, None, None, None);
            for i in first_chan..ev.len() {
                match (ev[i].id, &ev[i].value) {
                    (id::NEW_PATTERN, Value::Word(v)) => {
                        cur = *v;
                        if cur == number {
                            sel = Some(i);
                        }
                    }
                    (id::PATTERN_NAME, _) => {
                        last_meta = Some(i);
                        if cur == number {
                            name_at = Some(i);
                        }
                    }
                    (id::PATTERN_COLOR, _) => {
                        last_meta = Some(i);
                        if cur == number {
                            color_at = Some(i);
                        }
                    }
                    (id::NEW_ARRANGEMENT | id::INSERT_FLAGS | id::PLAYLIST, _) => break,
                    _ => {}
                }
            }
            let color = now.color.map(|c| color_bits(c, 0)).unwrap_or(0x0056_5148);
            match (sel, name_at, color_at) {
                (_, Some(ni), ci) => {
                    ev[ni] = text_event(id::PATTERN_NAME, &now.name, unicode);
                    match ci {
                        Some(ci) => {
                            if now.color != was.color {
                                ev[ci].value = Value::Int(color);
                            }
                        }
                        None => ev.insert(ni + 1, Event::int(id::PATTERN_COLOR, color)),
                    }
                }
                (Some(si), None, Some(ci)) => {
                    if now.color != was.color {
                        ev[ci].value = Value::Int(color);
                    }
                    ev.insert(si + 1, text_event(id::PATTERN_NAME, &now.name, unicode));
                }
                _ => {
                    // after the last pattern that has a name, else at the end of the first channel's run
                    let at = match last_meta {
                        Some(i) => {
                            let mut j = i + 1;
                            while j < ev.len() && !matches!(ev[j].id, 64 | 65 | 99 | 233 | 236) {
                                j += 1;
                            }
                            j
                        }
                        None => {
                            let mut j = (first_chan + 1).min(ev.len());
                            while j < ev.len() && !ends_channel(ev[j].id) {
                                j += 1;
                            }
                            j
                        }
                    };
                    ev.insert(at, Event::int(id::PATTERN_COLOR, color));
                    ev.insert(at, text_event(id::PATTERN_NAME, &now.name, unicode));
                    ev.insert(at, Event::word(id::NEW_PATTERN, number));
                }
            }
        }
    }
}

fn track_count(flp: &Flp) -> u16 {
    match flp.arrangements.first() {
        _ if flp.version_tuple() < (12, 9, 0, 0) => 199,
        Some(a) if !a.tracks.is_empty() => a.tracks.len() as u16,
        _ => 500,
    }
}

fn patch_playlist(ev: &mut Vec<Event>, song: &Song, base: &Song, flp: &Flp) {
    let sorted = |s: &Song| {
        let mut c = s.clips.clone();
        c.sort_by_key(|c| (c.pos, c.track, c.len));
        c
    };
    let (now, was) = (sorted(song), sorted(base));
    if now == was {
        return;
    }
    let tracks = track_count(flp);
    let at = ev.iter().position(|e| e.id == id::PLAYLIST);
    let old: Vec<u8> = at.and_then(|i| ev[i].bytes().map(|b| b.to_vec())).unwrap_or_default();
    let old_layout = flp.version_tuple() < (11, 0, 0, 0) && (old.is_empty() || flp.arrangements.first().is_some_and(|a| a.old_playlist));
    if old_layout {
        // Before version 11: 99 clip tracks counted from the top, in 28-byte items up to version 8 and 32 after.
        // Blocks are written back as pattern clips on the tracks they were shown on.
        let size = crate::flp::old_playlist_size(&old, flp.version_tuple()).unwrap_or(if flp.version_tuple() < (9, 0, 0, 0) { 28 } else { 32 });
        let top: u32 = if size == 28 { 30818 } else { 98 };
        let ppq = song.ppq.max(1) as f32;
        let mut data = Vec::with_capacity(now.len() * size);
        for c in &now {
            let (index, audio) = match c.src {
                ClipSrc::Pattern(p) => (ITEM_BASE + p as u16 + 1, false),
                ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => (ch as u16, true),
            };
            let mut it = vec![0u8; size];
            put_u32(&mut it, 0, c.pos);
            put_u16(&mut it, 4, ITEM_BASE);
            put_u16(&mut it, 6, index);
            put_u32(&mut it, 8, c.len);
            put_u16(&mut it, 12, top.saturating_sub(c.track.min(98) as u32) as u16);
            if size == 32 {
                put_u16(&mut it, 14, c.group);
                put_u16(&mut it, 16, 0x78);
                put_u16(&mut it, 18, if c.muted { 0x2000 } else { 0 });
            }
            it[size - 12..size - 8].copy_from_slice(&[0x40, 0x64, 0x80, 0x80]);
            let none = (-1.0f32).to_bits();
            let (a, b) = match (audio, c.offset) {
                (false, 0) if size == 32 => (0, c.len),
                (_, 0) => (none, none),
                (false, o) => (o, o + c.len),
                (true, o) => ((o as f32 / ppq).to_bits(), ((o + c.len) as f32 / ppq).to_bits()),
            };
            put_u32(&mut it, size - 8, a);
            put_u32(&mut it, size - 4, b);
            data.extend(it);
        }
        match at {
            Some(i) => ev[i].value = Value::Data(data),
            None => {
                let at = ev.iter().position(|e| matches!(e.id, 236 | 238 | 42 | 149 | 204)).unwrap_or(ev.len());
                ev.insert(at, Event::data(id::PLAYLIST, data));
            }
        }
        return;
    }
    let size = item_size(&old).unwrap_or_else(|| item_size_for(flp.version_tuple()));
    let ppq = song.ppq.max(1) as f32;

    let mut pool: HashMap<(u32, u32, u16, u16), Vec<&[u8]>> = HashMap::new();
    let mut next_uid = 0u32;
    if item_size(&old).is_some() {
        for it in old.chunks_exact(size).rev() {
            pool.entry((u32at(it, 0), u32at(it, 8), u16at(it, 12), u16at(it, 6))).or_default().push(it);
            if size >= 60 {
                next_uid = next_uid.max(u32at(it, 32));
            }
        }
    }
    // how each kept clip looked before, to tell an edited offset or mute from an untouched one
    let was_by: HashMap<(u32, u32, u16, ClipSrc), &crate::song::Clip> = was.iter().map(|c| ((c.pos, c.len, c.track, c.src), c)).collect();

    let mut data = Vec::with_capacity(now.len() * size);
    for c in &now {
        let (index, audio) = match c.src {
            ClipSrc::Pattern(p) => (ITEM_BASE + p as u16 + 1, false),
            ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => (ch as u16, true),
        };
        let track = (tracks - 1).saturating_sub(c.track);
        let reused = pool.get_mut(&(c.pos, c.len, track, index)).and_then(|v| v.pop());
        let before = was_by.get(&(c.pos, c.len, c.track, c.src));
        let mut it = match reused {
            Some(raw) => raw.to_vec(),
            None => {
                let mut it = vec![0u8; size];
                put_u32(&mut it, 0, c.pos);
                put_u16(&mut it, 4, ITEM_BASE);
                put_u16(&mut it, 6, index);
                put_u32(&mut it, 8, c.len);
                put_u16(&mut it, 12, track);
                put_u16(&mut it, 16, 0x78);
                put_u16(&mut it, 18, 0x40);
                it[20..24].copy_from_slice(&[0x40, 0x64, 0x80, 0x80]);
                if size >= 60 {
                    next_uid += 1;
                    put_u32(&mut it, 32, next_uid);
                    put_u32(&mut it, 52, 1.0f32.to_bits());
                }
                if size >= 80 {
                    it[64..72].copy_from_slice(&1.0f64.to_le_bytes());
                }
                it
            }
        };
        let fresh = reused.is_none();
        if fresh || before.map(|b| b.group != c.group).unwrap_or(true) {
            put_u16(&mut it, 14, c.group);
        }
        if fresh || before.map(|b| b.muted != c.muted).unwrap_or(true) {
            let f = u16at(&it, 18);
            put_u16(&mut it, 18, if c.muted { f | 0x2000 } else { f & !0x2000 });
        }
        if fresh || before.map(|b| b.offset != c.offset).unwrap_or(true) {
            let (a, b) = match (audio, c.offset) {
                (false, 0) => (u32::MAX, u32::MAX),
                (false, o) => (o, o + c.len),
                (true, 0) => ((-1.0f32).to_bits(), (-1.0f32).to_bits()),
                (true, o) => ((o as f32 / ppq).to_bits(), ((o + c.len) as f32 / ppq).to_bits()),
            };
            put_u32(&mut it, 24, a);
            put_u32(&mut it, 28, b);
        }
        data.extend(it);
    }
    match at {
        Some(i) => ev[i].value = Value::Data(data),
        None => {
            let at = ev.iter().position(|e| matches!(e.id, 236 | 238 | 42 | 149 | 204)).unwrap_or(ev.len());
            ev.insert(at, Event::data(id::PLAYLIST, data));
        }
    }
}

/// The rack's group names and the group on show.
fn patch_groups(ev: &mut Vec<Event>, song: &Song, base: &Song, unicode: bool) {
    if song.groups.is_empty() {
        return;
    }
    if song.groups != base.groups {
        let old: Vec<usize> = ev.iter().enumerate().filter(|(_, e)| e.id == id::GROUP_NAME).map(|(i, _)| i).collect();
        let Some(mut at) = old.first().copied().or(ev.iter().position(|e| e.id == id::CUR_GROUP)) else { return };
        for &i in old.iter().rev() {
            ev.remove(i);
        }
        for g in &song.groups {
            ev.insert(at, text_event(id::GROUP_NAME, g, unicode));
            at += 1;
        }
    }
    if song.cur_group != base.cur_group {
        let v = song.cur_group.map(|g| g as u32).unwrap_or(u32::MAX);
        match ev.iter().position(|e| e.id == id::CUR_GROUP) {
            Some(i) => ev[i].value = Value::Int(v),
            None => {
                if let Some(i) = ev.iter().rposition(|e| e.id == id::GROUP_NAME) {
                    ev.insert(i + 1, Event::int(id::CUR_GROUP, v));
                }
            }
        }
    }
}

/// The first arrangement's time markers, written again whenever any of them changed.
fn patch_markers(ev: &mut Vec<Event>, song: &Song, base: &Song, unicode: bool, with_sig: bool) {
    if song.markers == base.markers {
        return;
    }
    let (mut arrangements, mut in_pattern) = (0, false);
    let (mut old, mut playlist, mut track): (Vec<usize>, Option<usize>, Option<usize>) = (vec![], None, None);
    for (i, e) in ev.iter().enumerate() {
        match e.id {
            id::NEW_PATTERN => in_pattern = true,
            id::NEW_ARRANGEMENT => {
                in_pattern = false;
                arrangements += 1;
                if arrangements > 1 {
                    break;
                }
            }
            id::PLAYLIST => {
                in_pattern = false;
                playlist.get_or_insert(i);
            }
            id::TRACK => {
                track.get_or_insert(i);
            }
            id::MARKER_POSITION | id::MARKER_NAME | id::MARKER_NUMERATOR | id::MARKER_DENOMINATOR if !in_pattern => old.push(i),
            _ => {}
        }
    }
    let Some(mut at) = old.first().copied().or(playlist.map(|i| i + 1)).or(track) else { return };
    for &i in old.iter().rev() {
        ev.remove(i);
    }
    for m in &song.markers {
        let mut new = vec![Event::int(id::MARKER_POSITION, (m.kind as u32) << 24 | (m.pos & 0x00ff_ffff))];
        if with_sig {
            let (n, d) = if m.sig.0 == 0 || m.sig.1 == 0 { (4, 4) } else { m.sig };
            new.push(Event::byte(id::MARKER_NUMERATOR, n));
            new.push(Event::byte(id::MARKER_DENOMINATOR, d));
        }
        new.push(text_event(id::MARKER_NAME, &m.name, unicode));
        for e in new {
            ev.insert(at, e);
            at += 1;
        }
    }
}

fn patch_tracks(ev: &mut Vec<Event>, song: &Song, base: &Song, unicode: bool) {
    // only the first arrangement is shown, so only its tracks are edited
    let mut arrangements = 0;
    let mut at: Vec<usize> = vec![];
    for (i, e) in ev.iter().enumerate() {
        match e.id {
            id::NEW_ARRANGEMENT => {
                arrangements += 1;
                if arrangements > 1 {
                    break;
                }
            }
            id::TRACK => at.push(i),
            _ => {}
        }
    }
    for (t, &i) in at.iter().enumerate().rev() {
        let (Some(now), Some(was)) = (song.tracks.get(t), base.tracks.get(t)) else { continue };
        if now.name != was.name {
            let has = ev.get(i + 1).map(|e| e.id == id::TRACK_NAME).unwrap_or(false);
            match (has, now.name.is_empty()) {
                (true, true) => {
                    ev.remove(i + 1);
                }
                (true, false) => ev[i + 1] = text_event(id::TRACK_NAME, &now.name, unicode),
                (false, false) => ev.insert(i + 1, text_event(id::TRACK_NAME, &now.name, unicode)),
                (false, true) => {}
            }
        }
        if let Value::Data(d) = &mut ev[i].value {
            if now.color != was.color && d.len() >= 8 {
                let top = u32at(d, 4);
                put_u32(d, 4, color_bits(now.color, top));
            }
            if now.enabled != was.enabled && d.len() > 12 {
                d[12] = now.enabled as u8;
            }
            if now.lock_content != was.lock_content && d.len() > 21 {
                d[21] = now.lock_content as u8;
            }
            if (now.perf != was.perf || now.grouped != was.grouped) && d.len() >= 47 {
                put_u32(d, 22, now.perf.motion as u32);
                put_u32(d, 26, now.perf.press as u32);
                put_u32(d, 30, now.perf.sync as u32);
                put_u32(d, 34, now.perf.queue as u32);
                put_u32(d, 38, now.perf.tolerant as u32);
                put_u32(d, 42, now.perf.pos_sync as u32);
                d[46] = now.grouped as u8;
            }
        }
    }
}

/// Where each effect slot of the insert opened at `open` sits: its index, its events, and whether an
/// index event closes it (old files have none and keep the slot number in the plugin head).
fn slot_spans(ev: &[Event], open: usize) -> Vec<(u16, std::ops::Range<usize>, bool)> {
    let mut out: Vec<(u16, std::ops::Range<usize>, bool)> = vec![];
    let (mut begin, mut plugin, mut hint) = (open + 1, false, None::<u16>);
    let old = |out: &Vec<(u16, std::ops::Range<usize>, bool)>, hint: Option<u16>| {
        let free = |n: u16| !out.iter().any(|(i, _, _)| *i == n);
        match hint {
            Some(n) if n < 10 && free(n) => n,
            _ => (0..10).find(|&n| free(n)).unwrap_or(9),
        }
    };
    for i in open + 1..ev.len() {
        match ev[i].id {
            id::SLOT_INDEX => {
                if let Value::Word(idx) = ev[i].value {
                    out.push((idx, begin..i, true));
                }
                (begin, plugin, hint) = (i + 1, false, None);
            }
            id::PLUGIN_INTERNAL_NAME => {
                if plugin {
                    let idx = old(&out, hint.take());
                    out.push((idx, begin..i, false));
                    begin = i;
                }
                plugin = true;
            }
            id::NEW_PLUGIN => {
                if let Some(d) = ev[i].bytes().filter(|d| d.len() >= 8) {
                    hint = Some(u32at(d, 4) as u16);
                }
            }
            id::INSERT_ROUTING | id::INSERT_OUTPUT | id::INSERT_FLAGS | id::INSERT_COLOR | id::INSERT_NAME | id::INSERT_INPUT => {
                if plugin {
                    let idx = old(&out, hint.take());
                    out.push((idx, begin..i, false));
                }
                break;
            }
            _ => {}
        }
    }
    out
}

/// The flags of a mixer track with its place, gap and effect switch written in.
fn insert_look(ins: &crate::song::Insert, f: u32) -> u32 {
    let f = f & !(flp::INSERT_FLAG_DOCK_MIDDLE | flp::INSERT_FLAG_DOCK_RIGHT | flp::INSERT_FLAG_SEPARATOR | flp::INSERT_FLAG_FX);
    f | match ins.dock {
        1 => flp::INSERT_FLAG_DOCK_MIDDLE,
        2 => flp::INSERT_FLAG_DOCK_RIGHT,
        _ => 0,
    } | if ins.separator { flp::INSERT_FLAG_SEPARATOR } else { 0 }
        | if ins.fx_on { flp::INSERT_FLAG_FX } else { 0 }
}

fn patch_mixer(ev: &mut Vec<Event>, song: &Song, base: &Song, unicode: bool, mixer_base: usize) {
    // early files have no opening event: a track runs from the event after the last track's end to its own output
    let old_span = flp::old_mixer_span(ev).filter(|(a, _)| *a > 0);
    let old = old_span.is_some();
    let opens: Vec<usize> = match old_span {
        Some((a, b)) => std::iter::once(a - 1).chain(ev.iter().enumerate().filter(|(i, e)| e.id == id::INSERT_OUTPUT && *i < b).map(|(i, _)| i)).collect(),
        None => ev.iter().enumerate().filter(|(_, e)| e.id == id::INSERT_FLAGS).map(|(i, _)| i).collect(),
    };
    let mut records: Vec<(u8, u16, i32)> = vec![];
    // every slot's events as they stand in the file, so a moved slot takes its plugin with it untouched
    let mut groups: HashMap<(u16, u16), Vec<Event>> = HashMap::new();
    for (k, &open) in opens.iter().enumerate() {
        for (idx, r, _) in slot_spans(ev, open) {
            if !r.is_empty() {
                groups.insert((k as u16, idx), ev[r].to_vec());
            }
        }
    }

    for (k, &open) in opens.iter().enumerate().rev() {
        let (Some(now), Some(was)) = (song.inserts.get(k), base.inserts.get(k)) else { continue };
        let mut close = open + old as usize;
        while close + 1 < ev.len() && ev[close].id != id::INSERT_OUTPUT && ev[close + 1].id != id::INSERT_FLAGS {
            close += 1;
        }
        let cd = ((k + mixer_base) as u16) << 6;
        if ins_vol(now.volume) != ins_vol(was.volume) {
            records.push((192, cd, ins_vol(now.volume)));
        }
        if ins_pan(now.pan) != ins_pan(was.pan) {
            records.push((193, cd, ins_pan(now.pan)));
        }
        let q = |v: f32, scale: f32| (v * scale).round() as i32;
        if q(now.sep, 64.0) != q(was.sep, 64.0) {
            records.push((194, cd, q(now.sep, 64.0)));
        }
        for b in 0..3u8 {
            let (x, y) = (&now.eq[b as usize], &was.eq[b as usize]);
            if q(x.gain_db, 100.0) != q(y.gain_db, 100.0) {
                records.push((208 + b, cd, q(x.gain_db, 100.0)));
            }
            if q(x.freq, 65536.0) != q(y.freq, 65536.0) {
                records.push((216 + b, cd, q(x.freq, 65536.0)));
            }
            if q(x.q, 65536.0) != q(y.q, 65536.0) {
                records.push((224 + b, cd, q(x.q, 65536.0)));
            }
        }
        for d in 0..now.levels.len().max(was.levels.len()).min(128) {
            if q(now.send_level(d), 12800.0) != q(was.send_level(d), 12800.0) {
                records.push((64 + d as u8, cd, q(now.send_level(d), 12800.0)));
            }
        }
        if now.audio_in != was.audio_in {
            let last = ev.len();
            if let Some(e) = ev[open..(close + 1).min(last)].iter_mut().find(|e| e.id == id::INSERT_INPUT) {
                e.value = Value::Int(now.audio_in as u32);
            }
        }
        if !old && (now.dock, now.separator, now.fx_on) != (was.dock, was.separator, was.fx_on) {
            if let Value::Data(d) = &mut ev[open].value {
                let f = u32at(d, 4);
                put_u32(d, 4, insert_look(now, f));
            }
        }
        if !old && (now.invert, now.swap) != (was.invert, was.swap) {
            if let Value::Data(d) = &mut ev[open].value {
                let f = u32at(d, 4) & !(flp::INSERT_FLAG_INVERT | flp::INSERT_FLAG_SWAP);
                put_u32(d, 4, f | if now.invert { flp::INSERT_FLAG_INVERT } else { 0 } | if now.swap { flp::INSERT_FLAG_SWAP } else { 0 });
            }
        }
        // effect slots: the events of each slot sit ahead of its index event
        let shape = |i: &crate::song::Insert| i.slots.iter().map(|s| (s.index, s.src, s.plugin.clone())).collect::<Vec<_>>();
        let spans = slot_spans(ev, open);
        let slots_end = spans.iter().map(|(_, r, closed)| r.end + *closed as usize).max();
        if shape(now) != shape(was) {
            let n_slots = (open..=close).filter(|&i| ev[i].id == id::SLOT_INDEX).count().max(10);
            let mut region: Vec<Event> = vec![];
            for idx in 0..n_slots as u16 {
                if let Some(s) = now.slots.iter().find(|s| s.index == idx) {
                    let kept = s.src.and_then(|src| groups.get(&src));
                    let same = s.src.and_then(|(i, x)| base.inserts.get(i as usize)?.slots.iter().find(|w| w.index == x)).map(|w| w.plugin == s.plugin).unwrap_or(false);
                    let kept = kept.filter(|_| same);
                    let mut g = match kept {
                        Some(g) => g.clone(),
                        None => {
                            let mut head = vec![0u8; 52];
                            put_u32(&mut head, 8, 2);
                            put_u32(&mut head, 16, 0x160);
                            put_u32(&mut head, 40, 136);
                            put_u32(&mut head, 44, 178);
                            put_u32(&mut head, 48, 270);
                            let mut g = vec![text_event(id::PLUGIN_INTERNAL_NAME, &s.plugin, unicode), Event::data(212, head)];
                            if !old {
                                g.extend([Event::int(155, 0), Event::int(id::COLOR, 0x565148), Event::byte(41, 0)]);
                            }
                            g.push(Event::data(id::PLUGIN_PARAMS, s.fx.as_ref().map(|f| f.encode(None)).unwrap_or_default()));
                            g
                        }
                    };
                    for e in g.iter_mut() {
                        match (e.id, &mut e.value) {
                            (212, Value::Data(d)) if d.len() >= 8 => {
                                put_u32(d, 0, k as u32);
                                put_u32(d, 4, idx as u32);
                            }
                            (id::PLUGIN_PARAMS, Value::Data(d)) => {
                                if let Some(f) = &s.fx {
                                    *d = f.encode(Some(d));
                                }
                            }
                            _ => {}
                        }
                    }
                    region.extend(g);
                }
                if !old {
                    region.push(Event::word(id::SLOT_INDEX, idx));
                }
            }
            ev.splice(open + 1..slots_end.unwrap_or(open + 1), region);
            for idx in 0..n_slots as u16 {
                let s = now.slots.iter().find(|s| s.index == idx);
                records.push((0, cd | (idx & 63), s.map(|s| s.enabled).unwrap_or(true) as i32));
                records.push((1, cd | (idx & 63), s.map(|s| q(s.mix, 12800.0)).unwrap_or(12800)));
            }
        } else {
            for s in &now.slots {
                let Some(w) = was.slots.iter().find(|w| w.index == s.index) else { continue };
                if w.enabled != s.enabled {
                    records.push((0, cd | (s.index & 63), s.enabled as i32));
                }
                if q(w.mix, 12800.0) != q(s.mix, 12800.0) {
                    records.push((1, cd | (s.index & 63), q(s.mix, 12800.0)));
                }
                if let (true, Some(f)) = (w.fx != s.fx || s.fx.as_ref().is_some_and(|f| f.ext.is_some()), &s.fx) {
                    let Some((_, r, _)) = spans.iter().find(|(i, _, _)| *i == s.index) else { continue };
                    if let Some(i) = r.clone().find(|&i| ev[i].id == id::PLUGIN_PARAMS) {
                        if let Value::Data(d) = &mut ev[i].value {
                            *d = f.encode(Some(d));
                        }
                    }
                }
            }
        }

        // sends, inside the run
        if now.sends != was.sends && k != 0 && !old {
            let i = (open..=close).find(|&i| ev[i].id == id::INSERT_ROUTING);
            let old_len = i.and_then(|i| ev[i].bytes().map(|b| b.len())).unwrap_or(0);
            let need = now.sends.iter().max().map(|m| m + 1).unwrap_or(1);
            let mut r = vec![0u8; old_len.max(need)];
            for &d in &now.sends {
                r[d] = 1;
            }
            match i {
                Some(i) => ev[i].value = Value::Data(r),
                None => {
                    let at = slot_spans(ev, open).iter().map(|(_, r, closed)| r.end + *closed as usize).max().unwrap_or(open + 1);
                    ev.insert(at, Event::data(id::INSERT_ROUTING, r));
                }
            }
        }
        if !old && now.enabled != was.enabled {
            if let Value::Data(d) = &mut ev[open].value {
                let f = u32at(d, 4);
                put_u32(d, 4, if now.enabled { f | flp::INSERT_FLAG_ENABLED } else { f & !flp::INSERT_FLAG_ENABLED });
            }
        }

        if old {
            // name and colour sit between the track's effects and its output
            let close = (open + 1..ev.len()).find(|&i| ev[i].id == id::INSERT_OUTPUT).unwrap_or(ev.len());
            let tail = (open + 1..close).find(|&i| matches!(ev[i].id, id::INSERT_COLOR | id::INSERT_NAME | id::INSERT_INPUT)).unwrap_or(close);
            if now.name != was.name {
                match (tail..close).find(|&i| ev[i].id == id::INSERT_NAME) {
                    Some(i) => ev[i] = text_event(id::INSERT_NAME, &now.name, unicode),
                    None => ev.insert(tail, text_event(id::INSERT_NAME, &now.name, unicode)),
                }
            }
            if let (true, Some(c)) = (now.color != was.color, now.color) {
                match (tail..close).find(|&i| ev[i].id == id::INSERT_COLOR) {
                    Some(i) => ev[i].value = Value::Int(color_bits(c, 0)),
                    None => ev.insert(tail, Event::int(id::INSERT_COLOR, color_bits(c, 0))),
                }
            }
            continue;
        }
        // name and colour sit in the few events just ahead of the one that opens the insert
        let mut pre = open;
        while pre > 0 && matches!(ev[pre - 1].id, 42 | 95 | 149 | 204) {
            pre -= 1;
        }
        if now.name != was.name {
            match ((pre..open).find(|&i| ev[i].id == id::INSERT_NAME), now.name.is_empty()) {
                (Some(i), true) => {
                    ev.remove(i);
                }
                (Some(i), false) => ev[i] = text_event(id::INSERT_NAME, &now.name, unicode),
                (None, false) => ev.insert(open, text_event(id::INSERT_NAME, &now.name, unicode)),
                (None, true) => {}
            }
        }
        if now.color != was.color {
            let open = (pre..ev.len()).find(|&i| ev[i].id == id::INSERT_FLAGS).unwrap_or(open);
            match ((pre..open).find(|&i| ev[i].id == id::INSERT_COLOR), now.color) {
                (Some(i), None) => {
                    ev.remove(i);
                }
                (Some(i), Some(c)) => ev[i].value = Value::Int(color_bits(c, 0)),
                (None, Some(c)) => {
                    let at = (pre..open).find(|&i| matches!(ev[i].id, 95 | 204)).unwrap_or(open);
                    ev.insert(at, Event::int(id::INSERT_COLOR, color_bits(c, 0)));
                }
                (None, None) => {}
            }
        }
    }

    if records.is_empty() {
        return;
    }
    let at = match ev.iter().position(|e| e.id == id::MIXER_PARAMS) {
        Some(i) => i,
        None => {
            let at = ev.iter().rposition(|e| e.id == id::INSERT_OUTPUT).map(|i| i + 1).unwrap_or(ev.len());
            ev.insert(at, Event::data(id::MIXER_PARAMS, vec![]));
            at
        }
    };
    if let Value::Data(d) = &mut ev[at].value {
        for (pid, cd, value) in records {
            let (pid, value) = if old {
                match flp::old_mixer_record(pid, value) {
                    Some(r) => r,
                    None => continue,
                }
            } else {
                (pid, value)
            };
            let hit = (0..d.len() / 12).find(|r| d[r * 12 + 4] == pid && u16at(d, r * 12 + 6) == cd);
            match hit {
                Some(r) => put_u32(d, r * 12 + 8, value as u32),
                None => {
                    d.extend_from_slice(&0u32.to_le_bytes());
                    d.push(pid);
                    d.push(0x1f);
                    d.extend_from_slice(&cd.to_le_bytes());
                    d.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
    }
}

/// The project as .flp bytes. `orig` is the file it was opened from, if any.
pub fn to_bytes(song: &Song, orig: Option<&Flp>) -> Result<Vec<u8>, String> {
    if song.channels.len() > 4000 {
        return Err("too many channels".into());
    }
    let (made, song_owned);
    let (song, orig) = match orig {
        Some(o) => (song, o),
        None => {
            made = skeleton(song);
            let mut s = song.clone();
            // addresses of mixer controls are written the way the made file's version counts them
            s.mixer_base = flp::mixer_base(version_tuple(&made.version));
            for (i, c) in s.channels.iter_mut().enumerate() {
                c.src = Some(i as u16);
            }
            song_owned = s;
            (&song_owned, &made)
        }
    };

    let ver = version_tuple(&orig.version);
    let unicode = ver >= (11, 5, 0, 0);
    let mut file = orig.file.clone();
    restructure_channels(&mut file, orig, song, unicode);

    // from here the file and the song agree on which channel is which; compare and patch the rest
    let flp = Flp::from_file(file);
    let base = Song::from_flp_with(&flp, None, None);
    let mut ev = flp.file.events.clone();
    let word_insert = ev.iter().any(|e| e.id == id::CHAN_TARGET_INSERT_WORD);

    patch_mixer(&mut ev, song, &base, unicode, flp::mixer_base(ver));
    patch_tracks(&mut ev, song, &base, unicode);
    patch_playlist(&mut ev, song, &base, &flp);
    patch_markers(&mut ev, song, &base, unicode, ver >= (20, 0, 0, 0));
    patch_patterns(&mut ev, song, &base, &flp, unicode);
    patch_channels(&mut ev, song, &base, unicode, word_insert);
    patch_groups(&mut ev, song, &base, unicode);
    patch_auto(&mut ev, song, &base);
    patch_midi_links(&mut ev, song, &base);
    patch_ctl_links(&mut ev, song, &base, flp::mixer_base(ver));
    patch_link_formulas(&mut ev, song, unicode);
    patch_head(&mut ev, song, &base, unicode);

    let _ = latin1;
    Ok(FlpFile { header: flp.file.header.clone(), events: ev, trailer: flp.file.trailer.clone() }.to_bytes())
}

/// Save, then make the saved file the new original: returns the bytes to write and the parsed result, and
/// marks every channel of `song` as coming from it.
pub fn save(song: &mut Song, orig: Option<&Flp>) -> Result<(Vec<u8>, Flp), String> {
    let bytes = to_bytes(song, orig)?;
    let flp = flp::parse(&bytes)?;
    song.mixer_base = flp::mixer_base(version_tuple(&flp.version));
    for (i, c) in song.channels.iter_mut().enumerate() {
        c.src = Some(i as u16);
    }
    for (k, ins) in song.inserts.iter_mut().enumerate() {
        for s in ins.slots.iter_mut() {
            s.src = Some((k as u16, s.index));
        }
    }
    Ok((bytes, flp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::{Clip, Note};

    #[test]
    fn instruments_round_trip() {
        use crate::song::Chan;
        use crate::synth::Kind;
        let mut song = Song::new();
        let n = song.channels.len();
        let mut c = Chan::instrument(Kind::Osc3);
        c.gen.as_mut().unwrap().p[1] = 2.0;
        c.gen.as_mut().unwrap().p[6] = 77.0;
        song.channels.push(c.clone());
        let (bytes, flp) = save(&mut song, None).unwrap();
        let (back, _) = reload(&bytes);
        assert_eq!(back.channels.len(), n + 1);
        assert_eq!(back.channels[n].kind, ChannelKind::Plugin);
        assert_eq!(back.channels[n].plugin, "3x Osc");
        assert_eq!(back.channels[n].gen, c.gen);
        assert_eq!(to_bytes(&song, Some(&flp)).unwrap(), bytes);
        song.channels[n].gen.as_mut().unwrap().p[9] = 7.0;
        let (bytes, _) = save(&mut song, Some(&flp)).unwrap();
        assert_eq!(reload(&bytes).0.channels[n].gen, song.channels[n].gen);
    }

    #[test]
    fn hosted_plugins_round_trip() {
        use crate::host;
        for kind in ["clap", "vst3"] {
            // installed the way a user would have it: of the right kind, in a folder we search
            let dir = std::env::temp_dir().join(format!("cereal-host-{kind}-{}", std::process::id()));
            let Some(path) = host::test_install(&dir, kind) else {
                eprintln!("test plug-in not built; skipped");
                return;
            };
            host::add_folder(&dir);
            hosted_round_trip(&path, if kind == "clap" { 1.0 } else { 0.5 });
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// `k` scales the values to the range the kind's settings run over.
    #[cfg(test)]
    fn hosted_round_trip(path: &std::path::Path, k: f64) {
        use crate::host::Instance;
        use crate::song::{Chan, Slot};
        let mut song = Song::new();
        let n = song.channels.len();
        let a = Instance::open(path, "").unwrap();
        let b = Instance::open(path, "").unwrap();
        let gain = a.params()[0].id;
        a.activate(48_000.0);
        b.activate(48_000.0);
        a.set(gain, 1.5 * k);
        b.set(gain, 0.25 * k);
        a.idle();
        b.idle();
        song.channels.push(Chan::hosted(a));
        song.inserts[0].slots = vec![Slot::hosted(3, b)];
        let (bytes, flp) = save(&mut song, None).unwrap();
        let (back, _) = reload(&bytes);
        let chan = back.channels[n].ext.as_ref().expect("the channel's plug-in is opened again");
        assert_eq!(back.channels[n].plugin, crate::song::WRAPPER);
        assert_eq!(chan.0.value(gain), Some(1.5 * k));
        let slot = &back.inserts[0].slots[0];
        assert_eq!((slot.index, slot.title().as_str()), (3, "Test Tone"));
        assert_eq!(slot.fx.as_ref().unwrap().ext.as_ref().unwrap().0.value(gain), Some(0.25 * k));
        // nothing changed: the file comes out the same; a change in the plug-in is written
        assert_eq!(to_bytes(&song, Some(&flp)).unwrap(), bytes);
        song.channels[n].ext.as_ref().unwrap().0.set(gain, 0.75 * k);
        song.channels[n].ext.as_ref().unwrap().0.idle();
        let (bytes, _) = save(&mut song, Some(&flp)).unwrap();
        assert_eq!(reload(&bytes).0.channels[n].ext.as_ref().unwrap().0.value(gain), Some(0.75 * k));
    }

    #[test]
    fn effect_slots_round_trip() {
        use crate::fx::{Fx, Kind};
        use crate::song::Slot;
        let mut song = Song::new();
        while song.inserts.len() < 4 {
            song.inserts.push(Default::default());
        }
        let mut a = Slot::with_fx(2, Kind::Delay);
        a.fx.as_mut().unwrap().p[3] = 90.0;
        a.mix = 0.5;
        let mut b = Slot::with_fx(5, Kind::Eq);
        b.fx.as_mut().unwrap().p[2] = -600.0;
        b.enabled = false;
        song.inserts[1].slots = vec![a.clone(), b.clone()];
        let (bytes, flp) = save(&mut song, None).unwrap();
        let (back, _) = reload(&bytes);
        let got = &back.inserts[1].slots;
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].index, got[0].mix, &got[0].fx), (2, 0.5, &a.fx));
        assert_eq!((got[1].index, got[1].enabled, &got[1].fx), (5, false, &b.fx));

        // change a setting, move one slot to another insert, drop the other; unchanged projects stay byte-exact
        assert_eq!(to_bytes(&song, Some(&flp)).unwrap(), bytes);
        let mut moved = song.inserts[1].slots.remove(0);
        moved.index = 0;
        moved.fx.as_mut().unwrap().p[4] = 96.0;
        song.inserts[1].slots.clear();
        song.inserts[3].slots.push(moved.clone());
        song.inserts[2].slots.push(Slot::with_fx(1, Kind::Limiter));
        let (bytes, _) = save(&mut song, Some(&flp)).unwrap();
        let (back, _) = reload(&bytes);
        assert!(back.inserts[1].slots.is_empty());
        assert_eq!((back.inserts[3].slots[0].index, back.inserts[3].slots[0].mix, &back.inserts[3].slots[0].fx), (0, 0.5, &moved.fx));
        assert_eq!(back.inserts[2].slots[0].fx, Some(Fx::new(Kind::Limiter)));

        // a setting changed in place
        let (_, flp) = reload(&bytes);
        song.inserts[3].slots[0].fx.as_mut().unwrap().p[0] = 33.0;
        let (bytes, _) = save(&mut song, Some(&flp)).unwrap();
        assert_eq!(reload(&bytes).0.inserts[3].slots[0].fx.as_ref().unwrap().p[0], 33.0);
    }

    #[test]
    fn slots_of_old_projects_without_index_events() {
        use crate::fx::Kind;
        use crate::song::Slot;
        let mut song = Song::new();
        while song.inserts.len() < 4 {
            song.inserts.push(Default::default());
        }
        let mut a = Slot::with_fx(0, Kind::Delay);
        a.fx.as_mut().unwrap().p[3] = 90.0;
        let mut b = Slot::with_fx(3, Kind::Eq);
        b.fx.as_mut().unwrap().p[2] = -600.0;
        song.inserts[1].slots = vec![a.clone(), b.clone()];
        let (bytes, _) = save(&mut song, None).unwrap();
        // the way old versions wrote it: no index events, the slot number only in the plugin head
        let mut file = flp::FlpFile::parse(&bytes).unwrap();
        file.events.retain(|e| e.id != id::SLOT_INDEX);
        let old = file.to_bytes();
        let (mut back, flp) = reload(&old);
        let got = &back.inserts[1].slots;
        assert_eq!(got.iter().map(|s| s.index).collect::<Vec<_>>(), vec![0, 3]);
        assert_eq!((&got[0].fx, &got[1].fx), (&a.fx, &b.fx));
        assert_eq!(to_bytes(&back, Some(&flp)).unwrap(), old);

        // a setting changed in place keeps the old layout
        back.inserts[1].slots[1].fx.as_mut().unwrap().p[2] = 250.0;
        let (bytes, _) = save(&mut back, Some(&flp)).unwrap();
        assert_eq!(bytes.len(), old.len());
        let (mut back, flp) = reload(&bytes);
        assert_eq!(back.inserts[1].slots[1].fx.as_ref().unwrap().p[2], 250.0);
        assert_eq!(back.inserts[1].slots[0].fx, a.fx);

        // moving a slot rewrites the insert with index events; both plugins come through
        back.inserts[1].slots[0].index = 5;
        let (bytes, _) = save(&mut back, Some(&flp)).unwrap();
        let (again, _) = reload(&bytes);
        let got = &again.inserts[1].slots;
        assert_eq!(got.iter().map(|s| s.index).collect::<Vec<_>>(), vec![3, 5]);
        assert_eq!(got[1].fx, a.fx);
        assert_eq!(got[0].fx.as_ref().unwrap().p[2], 250.0);
    }

    fn reload(bytes: &[u8]) -> (Song, Flp) {
        let f = flp::parse(bytes).expect("our own file parses");
        (Song::from_flp_with(&f, None, None), f)
    }

    /// The mixer of the early versions: no opening event, effects first, their own parameter numbers.
    #[test]
    fn the_early_mixer_reads_and_keeps_edits() {
        let rec = |pid: u8, ins: u16, slot: u16, v: i32| {
            let mut r = vec![0u8; 4];
            r.push(pid);
            r.push(0x1f);
            r.extend_from_slice(&(((128 + ins) << 6) | slot).to_le_bytes());
            r.extend_from_slice(&v.to_le_bytes());
            r
        };
        let head = |ins: u32, slot: u32| {
            let mut h = vec![0u8; 52];
            put_u32(&mut h, 0, ins);
            put_u32(&mut h, 4, slot);
            h
        };
        let mut ev = vec![Event::data(id::VERSION, b"5.0.1\0".to_vec()), Event::data(233, vec![])];
        for i in 0..6u32 {
            if i == 1 {
                ev.push(text_event(id::PLUGIN_INTERNAL_NAME, "Fruity Balance", false));
                ev.push(Event::data(id::NEW_PLUGIN, head(1, 3)));
                ev.push(Event::data(id::PLUGIN_PARAMS, vec![]));
            }
            ev.push(Event::int(id::INSERT_COLOR, 0x625d54));
            ev.push(text_event(id::INSERT_NAME, if i == 2 { "Drums" } else { "" }, false));
            ev.push(Event::int(id::INSERT_INPUT, u32::MAX));
            ev.push(Event::int(id::INSERT_OUTPUT, 0));
        }
        let mut params = vec![];
        params.extend(rec(1, 1, 3, 64));
        params.extend(rec(96, 1, 0, 64));
        params.extend(rec(97, 2, 0, -32));
        params.extend(rec(64, 1, 0, 128));
        ev.push(Event::data(id::MIXER_PARAMS, params));
        let bytes = FlpFile { header: flp::Header { format: 0, channels: 0, ppq: 96 }, events: ev, trailer: vec![] }.to_bytes();
        let (mut s, f) = reload(&bytes);
        assert_eq!(s.inserts.len(), 6);
        assert_eq!(s.inserts[2].name, "Drums");
        assert_eq!((s.inserts[1].slots.len(), s.inserts[1].slots[0].index), (1, 3));
        assert!((s.inserts[1].slots[0].mix - 0.5).abs() < 1e-6);
        assert!((s.inserts[1].volume - 0.5).abs() < 1e-6);
        assert!((s.inserts[2].pan + 0.5).abs() < 1e-6);
        assert!(s.inserts[1].sends.contains(&2) && s.inserts[1].sends.contains(&0));
        assert!((s.inserts[1].send_level(2) - 1.0).abs() < 1e-6 && s.inserts[1].send_level(3) == 0.0);
        assert_eq!(to_bytes(&s, Some(&f)).unwrap(), bytes);

        s.inserts[1].name = "Lead".into();
        s.inserts[1].volume = 1.0;
        s.inserts[1].slots[0].mix = 0.25;
        s.inserts[3].pan = 0.25;
        let out = to_bytes(&s, Some(&f)).unwrap();
        let (r, f2) = reload(&out);
        assert!(flp::old_mixer_span(&f2.file.events).is_some());
        assert_eq!((r.inserts.len(), r.inserts[1].name.as_str(), r.inserts[2].name.as_str()), (6, "Lead", "Drums"));
        assert!((r.inserts[1].volume - 1.0).abs() < 1e-6 && (r.inserts[3].pan - 0.25).abs() < 1e-6);
        assert!((r.inserts[1].slots[0].mix - 0.25).abs() < 1e-6);
        assert_eq!(r.inserts[1].slots[0].plugin, "Fruity Balance");
    }

    #[test]
    fn a_new_project_round_trips() {
        let mut s = Song::new();
        s.title = "Test".into();
        s.tempo = 128.5;
        s.channels[0].name = "Boom".into();
        s.channels[0].sample_path = "/tmp/boom.wav".into();
        s.channels[1].volume = 0.5;
        s.channels[1].pan = -0.25;
        s.channels[3].kind = ChannelKind::AudioClip;
        s.clips.push(Clip { pos: 96, len: 500, track: 4, src: ClipSrc::Audio(3), offset: 0, muted: false, group: 0 });
        s.channels[2].insert = 3;
        s.patterns[0].events = vec![crate::song::PatEvent { tick: 0, param: 0, dest: 1, raw: 12800 }, crate::song::PatEvent { tick: 48, param: 0, dest: 1, raw: 400 }];
        s.patterns[0].toggle_step(0, 0);
        s.patterns[0].toggle_step(0, 96);
        s.patterns[0].notes.push(Note { pos: 48, len: 24, chan: 2, key: 64, vel: 0.5, pan: 0.5, fine_cents: 0.0, muted: false, ..Default::default() });
        s.patterns[0].sort();
        s.ensure_pattern(2);
        s.patterns[2].name = "Fill".into();
        s.patterns[2].toggle_step(3, 24);
        s.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0 });
        s.clips.push(Clip { pos: 384, len: 192, track: 2, src: ClipSrc::Pattern(2), offset: 96, muted: true, group: 0 });
        s.inserts[3].name = "Drums".into();
        s.inserts[3].volume = 0.8;
        s.inserts[3].pan = 0.5;
        s.inserts[4].sends = vec![3];
        s.inserts[5].enabled = false;
        s.inserts[3].sep = -0.5;
        s.inserts[3].eq[0].gain_db = 6.0;
        s.inserts[3].eq[1].freq = 0.25;
        s.inserts[3].eq[2].q = 0.75;
        s.inserts[4].set_send_level(3, 0.5);
        s.inserts[4].swap = true;
        s.inserts[6].dock = 2;
        s.inserts[6].separator = true;
        s.inserts[2].fx_on = false;
        s.inserts[4].audio_in = 1;
        s.tracks[2].name = "Fills".into();
        s.prepare();

        let bytes = to_bytes(&s, None).unwrap();
        let (r, _) = reload(&bytes);
        assert_eq!(r.title, "Test");
        assert!((r.tempo - 128.5).abs() < 1e-3);
        assert_eq!(r.channels.len(), 4);
        assert_eq!(r.channels[0].name, "Boom");
        assert_eq!(r.channels[0].sample_path, "/tmp/boom.wav");
        assert_eq!(r.channels[3].kind, ChannelKind::AudioClip);
        assert!(r.clips.iter().any(|c| c.src == ClipSrc::Audio(3) && (c.pos, c.len, c.track) == (96, 500, 4)));
        assert!((r.channels[1].volume - 0.5).abs() < 1e-3);
        assert!((r.channels[1].pan + 0.25).abs() < 1e-3);
        assert_eq!(r.channels[2].insert, 3);
        assert_eq!(notes_key(&r.patterns[0].notes), notes_key(&s.patterns[0].notes));
        assert_eq!(r.patterns[0].events, s.patterns[0].events);
        assert_eq!(r.patterns[2].name, "Fill");
        assert_eq!(r.patterns[2].notes.len(), 1);
        assert_eq!(r.clips, s.clips);
        assert_eq!(r.inserts[3].name, "Drums");
        assert!((r.inserts[3].volume - 0.8).abs() < 1e-3);
        assert!((r.inserts[3].pan - 0.5).abs() < 1e-3);
        assert_eq!(r.inserts[4].sends, vec![3]);
        assert!(!r.inserts[5].enabled);
        assert!(r.inserts[6].enabled);
        assert!((r.inserts[3].sep + 0.5).abs() < 1e-3);
        assert!((r.inserts[3].eq[0].gain_db - 6.0).abs() < 1e-3);
        assert!((r.inserts[3].eq[1].freq - 0.25).abs() < 1e-3);
        assert!((r.inserts[3].eq[2].q - 0.75).abs() < 1e-3);
        assert_eq!(r.inserts[2].eq, crate::song::EQ_DEFAULT);
        assert!((r.inserts[4].send_level(3) - 0.5).abs() < 1e-3);
        assert!(r.inserts[4].swap && !r.inserts[4].invert && !r.inserts[3].swap);
        assert_eq!((r.inserts[4].audio_in, r.inserts[3].audio_in), (1, -1));
        assert_eq!((r.inserts[0].dock, r.inserts[1].dock, r.inserts[6].dock), (0, 1, 2));
        assert!(r.inserts[6].separator && !r.inserts[5].separator && !r.inserts[2].fx_on && r.inserts[3].fx_on);
        assert_eq!(r.tracks[2].name, "Fills");

        // saving what was loaded changes nothing
        let (mut r2, f2) = reload(&bytes);
        r2.prepare();
        assert_eq!(to_bytes(&r2, Some(&f2)).unwrap(), bytes);
    }

    #[test]
    fn sampler_settings_round_trip() {
        let mut s = Song::new();
        {
            let f = &mut s.channels[1].smp;
            f.reverse = true;
            f.swap_stereo = true;
            f.normalize = true;
            f.fade_in = 0.25;
            f.fade_out = 0.5;
            f.swing = 0.5;
            f.cut = 3;
            f.cut_by = 3;
            f.cutoff = 0.5;
            f.resonance = 0.25;
            f.filter = 3;
            f.mono = true;
            f.porta = true;
            f.slide = 0.2;
            f.echo_feedback = 0.5;
            f.echoes = 6;
            f.echo_steps = 2.0;
            f.echo_cents = 100.0;
            f.stretch_beats = 4.0;
            f.stretch_cents = -200.0;
            f.keep_pitch = true;
            f.vol_env.attack = 0.5;
            f.vol_env.sustain = 0.25;
            f.vol_env.release = 1.0;
            f.vol_env.tension = [0.5, -0.5, 0.25];
            f.pitch_env.on = true;
            f.pitch_env.amount = 1200.0;
            f.pitch_env.decay = 0.3;
            f.cut_env.on = true;
            f.cut_env.amount = -0.5;
            f.arp_dir = 3;
            f.arp_range = 2;
            f.arp_chord = 2;
            f.arp_steps = 0.5;
            f.arp_gate = 0.5;
            f.arp_slide = true;
            f.arp_repeat = 3;
            f.gate_steps = 0.75;
            f.shift = 0.25;
            f.full_porta = false;
            f.add_to_key = true;
            f.key_lo = 48;
            f.key_hi = 72;
            f.echo_ping = true;
            f.track_vel = [90.0, 0.5, 0.0, -0.25];
            f.track_key = [72.0, -0.5, 0.25, 0.0];
        }
        s.channels[1].env_on = true;
        s.prepare();
        let bytes = to_bytes(&s, None).unwrap();
        let (r, f) = reload(&bytes);
        let (a, b) = (&s.channels[1].smp, &r.channels[1].smp);
        assert!(r.channels[1].env_on);
        assert_eq!((b.reverse, b.swap_stereo, b.normalize, b.mono, b.porta, b.keep_pitch), (true, true, true, true, true, true));
        assert_eq!((b.cut, b.cut_by, b.filter, b.echoes), (3, 3, 3, 6));
        assert_eq!((b.arp_dir, b.arp_range, b.arp_chord, b.arp_slide, b.arp_repeat, b.full_porta), (3, 2, 2, true, 3, false));
        assert!(b.echo_ping && !b.echo_fat);
        assert_eq!((b.add_to_key, b.key_lo, b.key_hi), (true, 48, 72));
        assert_eq!((a.track_vel, a.track_key), (b.track_vel, b.track_key));
        let near = |x: f32, y: f32| (x - y).abs() <= 0.01 * y.abs().max(1.0);
        for (x, y) in [
            (a.fade_in, b.fade_in), (a.fade_out, b.fade_out), (a.swing, b.swing), (a.cutoff, b.cutoff), (a.resonance, b.resonance), (a.slide, b.slide),
            (a.echo_feedback, b.echo_feedback), (a.echo_steps, b.echo_steps), (a.echo_cents, b.echo_cents), (a.stretch_beats, b.stretch_beats),
            (a.stretch_cents, b.stretch_cents), (a.vol_env.attack, b.vol_env.attack), (a.vol_env.sustain, b.vol_env.sustain), (a.vol_env.release, b.vol_env.release),
            (a.vol_env.tension[0], b.vol_env.tension[0]), (a.vol_env.tension[1], b.vol_env.tension[1]), (a.pitch_env.amount, b.pitch_env.amount),
            (a.pitch_env.decay, b.pitch_env.decay), (a.cut_env.amount, b.cut_env.amount), (a.arp_steps, b.arp_steps), (a.arp_gate, b.arp_gate),
            (a.gate_steps, b.gate_steps), (a.shift, b.shift),
        ] {
            assert!(near(x, y), "{x} != {y}");
        }
        assert!(b.pitch_env.on && b.cut_env.on);
        // untouched channels keep their defaults, and a second save is the same file
        assert_eq!(r.channels[0].smp, Song::new().channels[0].smp);
        let mut r2 = r.clone();
        r2.prepare();
        assert_eq!(to_bytes(&r2, Some(&f)).unwrap(), bytes);
    }

    #[test]
    fn channels_can_be_removed_added_and_cloned() {
        let mut s = Song::new();
        for (i, c) in s.channels.iter_mut().enumerate() {
            c.name = format!("C{i}");
        }
        s.patterns[0].toggle_step(1, 0);
        s.patterns[0].toggle_step(3, 24);
        let bytes = to_bytes(&s, None).unwrap();
        let (mut r, f) = reload(&bytes);

        // remove channel 1 the way the rack does, add a new one, clone the old channel 3
        r.channels.remove(1);
        for p in &mut r.patterns {
            p.notes.retain(|n| n.chan != 1);
            for n in &mut p.notes {
                if n.chan > 1 {
                    n.chan -= 1;
                }
            }
        }
        r.channels.push(crate::song::Chan { name: "New".into(), ..Default::default() });
        let mut twin = r.channels[2].clone();
        twin.name = "C3 #2".into();
        r.channels.push(twin);
        r.patterns[0].toggle_step(3, 48);
        r.prepare();

        let (bytes2, f2) = save(&mut r, Some(&f)).unwrap();
        assert_eq!(f2.header.channels, 5);
        let (r2, _) = reload(&bytes2);
        let names: Vec<&str> = r2.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["C0", "C2", "C3", "New", "C3 #2"]);
        let mut got: Vec<(u16, u32)> = r2.patterns[0].notes.iter().map(|n| (n.chan, n.pos)).collect();
        got.sort();
        assert_eq!(got, [(2, 24), (3, 48)]);
        // and it is stable from there
        assert_eq!(to_bytes(&r, Some(&f2)).unwrap(), bytes2);
    }

    #[test]
    fn a_sound_bank_channel_is_saved() {
        use crate::song::Chan;
        let s = Song::new();
        let bytes = to_bytes(&s, None).unwrap();
        let (mut r, f) = reload(&bytes);
        let at = r.channels.len();
        let want = crate::sf2::Setting { path: "C:\\Banks\\Strings.sf2".into(), bank: 0, patch: 48 };
        let block = crate::sf2::with(&[], &want);
        let mut c = Chan { name: "Strings".into(), kind: ChannelKind::Plugin, plugin: "SoundFont Player".into(), ..Chan::default() };
        c.zones = crate::sf2::open(&c.plugin, &block, None, None).map(std::sync::Arc::new);
        r.channels.push(c);
        r.prepare();
        let (bytes2, f2) = save(&mut r, Some(&f)).unwrap();
        let (mut r2, _) = reload(&bytes2);
        let info = r2.channels[at].zones.clone().and_then(|z| z.bank.clone()).expect("a sound bank channel");
        assert_eq!(info.setting, want);
        // picking another preset goes back into the same block
        let again = crate::sf2::Setting { patch: 5, ..want.clone() };
        r2.channels[at].zones = crate::sf2::open("SoundFont Player", &crate::sf2::with(&info.block, &again), None, None).map(std::sync::Arc::new);
        let (bytes3, _) = save(&mut r2, Some(&f2)).unwrap();
        let (r3, _) = reload(&bytes3);
        assert_eq!(r3.channels[at].zones.clone().and_then(|z| z.bank.clone()).unwrap().setting, again);
    }

    #[test]
    fn channels_made_midi_senders_are_saved_as_such() {
        let mut s = Song::new();
        s.channels.push(crate::song::Chan::instrument(crate::synth::Kind::Fm));
        let bytes = to_bytes(&s, None).unwrap();
        let (mut r, f) = reload(&bytes);
        let n = r.channels.len();
        let names: Vec<String> = r.channels.iter().map(|c| c.name.clone()).collect();
        assert_eq!(r.prepare_midi_export(), n);
        assert_eq!(r.prepare_midi_export(), 0);
        let (bytes2, _) = save(&mut r, Some(&f)).unwrap();
        let (r2, _) = reload(&bytes2);
        assert_eq!(r2.channels.len(), n);
        for (i, c) in r2.channels.iter().enumerate() {
            assert!(c.plugin.eq_ignore_ascii_case(crate::midiout::IDENT), "{i}: {}", c.plugin);
            assert_eq!(c.name, names[i]);
            let set = crate::midiout::Set::parse(&c.gen.as_ref().unwrap().raw).unwrap();
            assert_eq!(set.chan as usize, i);
        }
    }

    #[test]
    fn performance_settings_are_saved_and_read_back() {
        let bytes = to_bytes(&Song::new(), None).unwrap();
        let (mut r, f) = reload(&bytes);
        assert!(!r.performance);
        r.performance = true;
        r.main_pitch = -350;
        r.tracks[2].perf = crate::song::Perf { press: 3, motion: 5, sync: 2, pos_sync: 4, queue: true, tolerant: false };
        r.tracks[3].grouped = true;
        r.tracks[4].lock_content = true;
        let (bytes2, _) = save(&mut r, Some(&f)).unwrap();
        let (r2, _) = reload(&bytes2);
        assert!(r2.performance);
        assert_eq!(r2.main_pitch, -350);
        assert_eq!(r2.tracks[2].perf, r.tracks[2].perf);
        assert_eq!(r2.tracks[1].perf, crate::song::Perf::default());
        assert!(r2.tracks[3].grouped && !r2.tracks[2].grouped);
        assert!(r2.tracks[4].lock_content && !r2.tracks[3].lock_content);
    }

    #[test]
    fn rack_groups_are_saved_and_read_back() {
        let bytes = to_bytes(&Song::new(), None).unwrap();
        let (mut r, f) = reload(&bytes);
        assert!(!r.groups.is_empty(), "the starting project has its catch-all group");
        let drums = r.group_channels(&[0, 1], "Drums");
        r.channels[2].zipped = true;
        r.cur_group = Some(drums);
        let (bytes2, _) = save(&mut r, Some(&f)).unwrap();
        let (r2, _) = reload(&bytes2);
        assert_eq!(r2.groups, r.groups);
        assert_eq!(r2.cur_group, Some(drums));
        assert_eq!((0..4).map(|c| r2.group_of(c)).collect::<Vec<_>>(), (0..4).map(|c| r.group_of(c)).collect::<Vec<_>>());
        assert_eq!(r2.groups[r2.group_of(0)], "Drums");
        assert_eq!(r2.groups[r2.group_of(3)], "Unsorted");
        assert!(r2.channels[2].zipped && !r2.channels[1].zipped);
    }

    #[test]
    fn time_markers_are_saved_and_read_back() {
        use crate::song::TimeMarker;
        let bytes = to_bytes(&Song::new(), None).unwrap();
        let (mut r, f) = reload(&bytes);
        r.markers = vec![TimeMarker { pos: 0, name: "Intro".into(), kind: 0, sig: (4, 4) }, TimeMarker { pos: 384 * 8, name: "Drop".into(), kind: 0, sig: (4, 4) }];
        let (bytes2, f2) = save(&mut r, Some(&f)).unwrap();
        let (mut r2, _) = reload(&bytes2);
        assert_eq!(r2.markers, r.markers);
        r2.markers.remove(0);
        r2.markers[0].name = "Chorus".into();
        let (bytes3, _) = save(&mut r2, Some(&f2)).unwrap();
        let (r3, _) = reload(&bytes3);
        assert_eq!(r3.markers, r2.markers);
        assert_eq!(r3.patterns.len(), r2.patterns.len());
    }

    #[test]
    fn note_and_clip_groups_are_saved_and_read_back() {
        let bytes = to_bytes(&Song::new(), None).unwrap();
        let (mut r, f) = reload(&bytes);
        r.patterns[0].notes.push(Note { pos: 0, len: 24, key: 60, group: 3, ..Default::default() });
        r.patterns[0].notes.push(Note { pos: 24, len: 24, key: 62, group: 3, ..Default::default() });
        r.patterns[0].notes.push(Note { pos: 48, len: 24, key: 64, ..Default::default() });
        r.clips.push(Clip { pos: 0, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 2 });
        r.clips.push(Clip { pos: 384, len: 384, track: 1, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 2 });
        r.clips.push(Clip { pos: 768, len: 384, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0 });
        let (bytes2, f2) = save(&mut r, Some(&f)).unwrap();
        let (mut r2, _) = reload(&bytes2);
        assert_eq!(r2.patterns[0].notes.iter().map(|n| n.group).collect::<Vec<_>>(), [3, 3, 0]);
        assert_eq!(r2.clips.iter().map(|c| c.group).collect::<Vec<_>>(), [2, 2, 0]);
        r2.clips[1].group = 0;
        r2.patterns[0].notes[2].group = 5;
        let (bytes3, _) = save(&mut r2, Some(&f2)).unwrap();
        let (r3, _) = reload(&bytes3);
        assert_eq!(r3.patterns[0].notes.iter().map(|n| n.group).collect::<Vec<_>>(), [3, 3, 5]);
        assert_eq!(r3.clips.iter().map(|c| c.group).collect::<Vec<_>>(), [2, 0, 0]);
    }

    #[test]
    fn a_pad_machine_is_saved_with_its_edits() {
        use crate::zones::{Kit, Map};
        use crate::song::Chan;
        let bytes = to_bytes(&Song::new(), None).unwrap();
        let (mut r, f) = reload(&bytes);
        let at = r.channels.len();
        let mut kit = Kit::fresh();
        kit.pads[1].layers[0].put_path("kick.wav");
        let mut c = Chan { name: "Pads".into(), kind: ChannelKind::Plugin, plugin: Kit::IDENT.into(), ..Chan::default() };
        c.zones = Map::parse(Kit::IDENT, &kit.write()).map(std::sync::Arc::new);
        r.channels.push(c);
        r.prepare();
        let (bytes2, f2) = save(&mut r, Some(&f)).unwrap();
        let (mut r2, _) = reload(&bytes2);
        assert_eq!(r2.channels[at].zones.as_ref().and_then(|z| z.kit.clone()), Some(kit.write()));
        kit.pads[1].put_random_layers(true);
        kit.pads[1].put_name("Kick");
        r2.channels[at].zones = Map::parse(Kit::IDENT, &kit.write()).map(std::sync::Arc::new);
        let (bytes3, _) = save(&mut r2, Some(&f2)).unwrap();
        let (r3, _) = reload(&bytes3);
        let back = Kit::parse(r3.channels[at].zones.as_ref().and_then(|z| z.kit.as_ref()).unwrap()).unwrap();
        assert_eq!(back, kit);
    }

    #[test]
    fn sender_links_are_saved() {
        use crate::song::{CtlLink, Target};
        let mut song = Song::new();
        let mut slot = crate::song::Slot::with_fx(2, crate::fx::Kind::Formula);
        slot.fx.as_mut().unwrap().set_formula("a*b");
        song.inserts[4].slots.push(slot);
        song.ctl_links.push(CtlLink { ins: 4, slot: 2, out: 0, target: Target::InsertVolume(7) });
        song.ctl_links.push(CtlLink { ins: 4, slot: 2, out: 0, target: Target::InsertPan(3) });
        song.ctl_links.push(CtlLink::from_chan(0, 1, Target::InsertVolume(2)));
        let (bytes, flp) = save(&mut song, None).unwrap();
        let back = Song::from_flp_with(&flp::parse(&bytes).unwrap(), None, None);
        assert_eq!(back.ctl_links, song.ctl_links);
        assert_eq!(back.inserts[4].slots[0].fx.as_ref().unwrap().formula(), "a*b");
        // taking one away leaves the other
        song.ctl_links.remove(0);
        let (bytes, flp) = save(&mut song, Some(&flp)).unwrap();
        let back = Song::from_flp_with(&flp::parse(&bytes).unwrap(), None, None);
        assert_eq!(back.ctl_links, song.ctl_links);
        // a mapping formula goes with its link, can be changed, and leaves with it
        song.link_maps.push(crate::song::LinkMap::new(Target::InsertPan(3), "1-Input*0.5"));
        assert!((song.mapped(Target::InsertPan(3), 0.5) - 0.75).abs() < 1e-6 && song.mapped(Target::InsertVolume(2), 0.5) == 0.5);
        let (bytes, flp) = save(&mut song, Some(&flp)).unwrap();
        let back = Song::from_flp_with(&flp::parse(&bytes).unwrap(), None, None);
        assert_eq!(back.link_maps, song.link_maps);
        song.link_maps[0] = crate::song::LinkMap::new(Target::InsertPan(3), "Tension(Input,10)");
        let (bytes, flp) = save(&mut song, Some(&flp)).unwrap();
        let back = Song::from_flp_with(&flp::parse(&bytes).unwrap(), None, None);
        assert_eq!(back.link_maps, song.link_maps);
        song.link_maps.clear();
        let (bytes, _) = save(&mut song, Some(&flp)).unwrap();
        assert!(flp::parse(&bytes).unwrap().link_formulas.is_empty());
    }

    #[test]
    fn controller_links_are_saved() {
        use crate::song::{MidiLink, Target};
        let s = Song::new();
        let (mut r, f) = reload(&to_bytes(&s, None).unwrap());
        r.midi_links.push(MidiLink { ctrl: 74, port: 0, target: Target::ChanVolume(1) });
        r.midi_links.push(MidiLink { ctrl: 7, port: 0, target: Target::InsertVolume(0) });
        let (bytes, f2) = save(&mut r, Some(&f)).unwrap();
        let (mut r2, _) = reload(&bytes);
        assert_eq!(r2.midi_links, r.midi_links);
        r2.midi_links.remove(0);
        let (bytes, _) = save(&mut r2, Some(&f2)).unwrap();
        let (r3, _) = reload(&bytes);
        assert_eq!(r3.midi_links, vec![MidiLink { ctrl: 7, port: 0, target: Target::InsertVolume(0) }]);
        let mut s2 = r3.clone();
        s2.set_travel(Target::InsertVolume(0), 0.4);
        assert!((s2.inserts[0].volume - 0.5).abs() < 1e-6);
    }

    #[test]
    fn automation_clips_can_be_made_and_edited() {
        use crate::song::{AutoPt, Automation, Chan, ClipSrc, Target};
        let s = Song::new();
        let bytes = to_bytes(&s, None).unwrap();
        let (mut r, f) = reload(&bytes);
        let pts = vec![AutoPt { tick: 0.0, value: 0.25, tension: 0.0, mode: 0 }, AutoPt { tick: 96.0, value: 1.0, tension: 0.5, mode: 0 }, AutoPt { tick: 384.0, value: 0.0, tension: -0.25, mode: 2 }];
        let ac = r.channels.len();
        r.channels.push(Chan { name: "Kick - Volume".into(), kind: ChannelKind::AutomationClip, auto: Some(std::sync::Arc::new(Automation { points: pts.clone(), targets: vec![Target::ChanVolume(0), Target::InsertPan(2)] })), ..Default::default() });
        r.clips.push(Clip { pos: 0, len: 384, track: 3, src: ClipSrc::Automation(ac), offset: 0, muted: false, group: 0 });
        r.prepare();
        let (bytes2, f2) = save(&mut r, Some(&f)).unwrap();
        let (mut r2, _) = reload(&bytes2);
        let a = r2.channels[ac].auto.clone().expect("an automation clip");
        assert_eq!(r2.channels[ac].kind, ChannelKind::AutomationClip);
        assert_eq!(a.points, pts);
        assert_eq!(a.targets, vec![Target::ChanVolume(0), Target::InsertPan(2)]);
        assert!(r2.clips.iter().any(|c| c.src == ClipSrc::Automation(ac)));
        assert_eq!(to_bytes(&r, Some(&f2)).unwrap(), bytes2);

        // move a point and change what it drives
        let mut a2 = (*a).clone();
        a2.points[1].value = 0.5;
        a2.targets = vec![Target::Tempo];
        r2.channels[ac].auto = Some(std::sync::Arc::new(a2.clone()));
        let (bytes3, _) = save(&mut r2, Some(&f2)).unwrap();
        let (r3, _) = reload(&bytes3);
        assert_eq!(**r3.channels[ac].auto.as_ref().unwrap(), a2);
    }

    /// With CEREAL_FLP_CORPUS set: every real project saves byte-identical when untouched, and an edit to each
    /// survives a reload without disturbing anything else.
    #[test]
    fn corpus_saves_exactly_and_edits_survive() {
        let Ok(dir) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        static SLICERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().map(|x| x.eq_ignore_ascii_case("flp")).unwrap_or(false) {
                    out.push(p);
                }
            }
        }
        let mut files = vec![];
        walk(std::path::Path::new(&dir), &mut files);
        assert!(!files.is_empty());
        for p in files {
            let bytes = std::fs::read(&p).unwrap();
            let (song, f) = reload(&bytes);
            assert_eq!(to_bytes(&song, Some(&f)).unwrap(), bytes, "untouched save differs: {}", p.display());

            // a loop slicer's cut moved by hand comes back where it was put
            if let Some(ci) = song.channels.iter().position(|c| c.plugin.eq_ignore_ascii_case("Fruity Slicer") && c.slices.as_ref().is_some_and(|s| s.cuts.len() > 1)) {
                let mut s = song.clone();
                let mut sl = (**s.channels[ci].slices.as_ref().unwrap()).clone();
                sl.cuts[1].pos += 3;
                sl.cuts[0].reverse = !sl.cuts[0].reverse;
                s.channels[ci].slices = Some(std::sync::Arc::new(sl.clone()));
                let (back, _) = reload(&to_bytes(&s, Some(&f)).unwrap());
                assert_eq!(back.channels[ci].slices.as_deref(), Some(&sl), "moved cut lost: {}", p.display());
                SLICERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            let mut s = song.clone();
            s.tempo = 133.25;
            s.title = "Edited".into();
            s.ensure_pattern(0);
            s.patterns[0].notes.push(Note { pos: 7, len: 11, chan: 0, key: 61, vel: 0.9, pan: 0.0, fine_cents: 0.0, muted: false, ..Default::default() });
            let extra = s.patterns.len();
            s.ensure_pattern(extra);
            s.patterns[extra].name = "Brand new".into();
            s.patterns[extra].notes.push(Note::step(0, 0));
            s.clips.push(Clip { pos: 96 * 4 * 300, len: 96, track: 1, src: ClipSrc::Pattern(extra), offset: 0, muted: false, group: 0 });
            if s.inserts.len() > 2 {
                s.inserts[1].name = "Renamed".into();
                s.inserts[1].volume = 0.5;
                s.inserts[2].color = Some([1, 2, 3]);
            }
            if let Some(c) = s.channels.first_mut() {
                c.name = "First".into();
                c.volume = 0.25;
                c.enabled = !c.enabled;
            }
            s.prepare();
            let out = to_bytes(&s, Some(&f)).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            let (r, _) = reload(&out);
            let name = p.display();
            assert!((r.tempo - 133.25).abs() < 0.6, "{name}: tempo {}", r.tempo);
            assert_eq!(r.title, "Edited", "{name}");
            assert_eq!(r.channels.len(), song.channels.len(), "{name}");
            assert_eq!(notes_key(&r.patterns[0].notes), notes_key(&s.patterns[0].notes), "{name}: pattern 1");
            assert_eq!(r.patterns[extra].name, "Brand new", "{name}");
            assert_eq!(r.patterns[extra].notes.len(), 1, "{name}");
            for i in 1..extra {
                assert_eq!(notes_key(&r.patterns[i].notes), notes_key(&song.patterns[i].notes), "{name}: pattern {}", i + 1);
                assert_eq!(r.patterns[i].name, song.patterns[i].name, "{name}");
            }
            let mut want = s.clips.clone();
            want.sort_by_key(|c| (c.pos, c.track, c.len));
            let mut got = r.clips.clone();
            got.sort_by_key(|c| (c.pos, c.track, c.len));
            assert_eq!(got.len(), want.len(), "{name}: clip count");
            if s.inserts.len() > 2 {
                assert_eq!(r.inserts[1].name, "Renamed", "{name}");
                assert!((r.inserts[1].volume - 0.5).abs() < 1e-3, "{name}");
                assert_eq!(r.inserts[2].color, Some([1, 2, 3]), "{name}");
                assert_eq!(r.inserts.len(), song.inserts.len(), "{name}");
                for i in 3..song.inserts.len() {
                    assert_eq!(r.inserts[i].name, song.inserts[i].name, "{name}: insert {i}");
                    assert_eq!(r.inserts[i].sends, song.inserts[i].sends, "{name}: insert {i}");
                }
            }
            if let (Some(c), Some(o)) = (r.channels.first(), song.channels.first()) {
                assert_eq!(c.name, "First", "{name}");
                assert!((c.volume - 0.25).abs() < 1e-3, "{name}");
                assert_eq!(c.enabled, !o.enabled, "{name}");
            }
            for (c, o) in r.channels.iter().zip(&song.channels).skip(1) {
                assert_eq!((c.name.as_str(), c.sample_path.as_str(), c.insert), (o.name.as_str(), o.sample_path.as_str(), o.insert), "{name}");
            }
        }
        eprintln!("slicers with a moved cut checked: {}", SLICERS.load(std::sync::atomic::Ordering::Relaxed));
    }
}
