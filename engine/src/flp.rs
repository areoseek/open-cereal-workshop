//! Reading and writing FL Studio project files (.flp).
//!
//! The .flp container is a public, widely documented event-stream format (the same structure open projects such
//! as LMMS and PyFLP read). This module is an independent implementation written from that public description
//! and from looking at project files: a "FLhd" header chunk followed by a "FLdt" chunk of tagged events.
//! Nothing here is taken from the original program's code.
//!
//! Event encoding, by tag id:
//!   0..=63    one byte of data
//!   64..=127  two bytes (little-endian u16)
//!   128..=191 four bytes (little-endian u32)
//!   192..=255 a length (7-bit varint) then that many bytes (text or a binary blob)
//! One exception seen in files written by 25.2.1 and later: id 172 carries three bytes, not four.
//!
//! Two layers live here:
//!   * `FlpFile`: the raw event list. Reading then writing it gives back the same bytes, so anything we do not
//!     understand yet (plugin state, new events) survives a load and save untouched.
//!   * `Flp`: a typed view decoded from the events (channels, patterns, playlist, mixer).

use std::collections::BTreeMap;

pub mod id {
    // byte events
    pub const CHAN_ENABLED: u8 = 0;
    pub const LOOP_ACTIVE: u8 = 9;
    pub const SHUFFLE: u8 = 11;
    pub const TIMESIG_NUM: u8 = 17;
    /// 1 when the playlist is in performance mode.
    pub const PERFORMANCE: u8 = 31;
    /// 1 when clips that start part-way into a pattern still sound the notes they cut into.
    pub const TRUNCATED: u8 = 30;
    pub const TIMESIG_BEAT: u8 = 18;
    pub const CHAN_ZIPPED: u8 = 15;
    /// The sample's loop goes back and forth.
    pub const CHAN_PING_PONG: u8 = 20;
    /// Fine tune of the root note in cents (signed).
    pub const CHAN_FINE_TUNE: u8 = 142;
    pub const CHAN_GROUP: u8 = 145;
    pub const CUR_GROUP: u8 = 146;
    pub const GROUP_NAME: u8 = 231;
    pub const CHAN_TYPE: u8 = 21;
    pub const CHAN_TARGET_INSERT: u8 = 22;
    pub const PAN_LAW: u8 = 23;
    // word events
    pub const NEW_CHANNEL: u8 = 64;
    pub const NEW_PATTERN: u8 = 65;
    pub const MARKER_NUMERATOR: u8 = 33;
    pub const MARKER_DENOMINATOR: u8 = 34;
    pub const TEMPO_WORD: u8 = 66;
    pub const CURRENT_PATTERN: u8 = 67;
    pub const MAIN_PITCH: u8 = 80;
    pub const CHAN_TARGET_INSERT_WORD: u8 = 104;
    pub const INSERT_ICON: u8 = 95;
    pub const SLOT_INDEX: u8 = 98;
    pub const NEW_ARRANGEMENT: u8 = 99;
    pub const CURRENT_ARRANGEMENT: u8 = 100;
    // int events
    pub const COLOR: u8 = 128;
    pub const SAMPLE_FLAGS: u8 = 143;
    pub const INSERT_OUTPUT: u8 = 147;
    pub const MARKER_POSITION: u8 = 148;
    pub const INSERT_COLOR: u8 = 149;
    pub const PATTERN_COLOR: u8 = 150;
    pub const INSERT_INPUT: u8 = 154;
    pub const TEMPO: u8 = 156;
    pub const BUILD: u8 = 159;
    pub const SHORT3: u8 = 172;
    // text and data events
    pub const CHAN_NAME: u8 = 192;
    pub const PATTERN_NAME: u8 = 193;
    pub const TITLE: u8 = 194;
    pub const COMMENT: u8 = 195;
    pub const SAMPLE_PATH: u8 = 196;
    pub const VERSION: u8 = 199;
    pub const PLUGIN_INTERNAL_NAME: u8 = 201;
    pub const PLUGIN_NAME: u8 = 203;
    pub const INSERT_NAME: u8 = 204;
    pub const MARKER_NAME: u8 = 205;
    pub const GENRE: u8 = 206;
    pub const ARTIST: u8 = 207;
    pub const NEW_PLUGIN: u8 = 212;
    pub const PLUGIN_PARAMS: u8 = 213;
    pub const CHAN_PARAMS: u8 = 215;
    pub const CHAN_ENVELOPE: u8 = 218;
    pub const CHAN_LEVELS: u8 = 219;
    pub const PATTERN_NOTES: u8 = 224;
    /// Recorded control movements of a pattern: 12-byte records of position, control, destination, value.
    pub const PATTERN_CTRL: u8 = 223;
    pub const MIXER_PARAMS: u8 = 225;
    pub const PLAYLIST: u8 = 233;
    // the step sequencer of early files, one event per step
    pub const DOT_VOL: u8 = 72;
    pub const DOT_PAN: u8 = 73;
    pub const DOT_NOTE: u8 = 77;
    pub const DOT_PITCH: u8 = 78;
    pub const DOT_RES: u8 = 87;
    pub const DOT_CUT: u8 = 88;
    pub const DOT_SHIFT: u8 = 92;
    pub const DOT: u8 = 91;
    pub const CHAN_AUTOMATION: u8 = 234;
    pub const LINK: u8 = 227;
    /// The mapping formula of the link written just before it.
    pub const LINK_FORMULA: u8 = 230;
    /// A hardware controller tied to a control: laid out like `LINK`, with the controller number first.
    pub const MIDI_LINK: u8 = 226;
    pub const CHAN_FX_FLAGS: u8 = 70;
    pub const CHAN_PREAMP: u8 = 74;
    pub const CHAN_FADE_OUT: u8 = 75;
    pub const CHAN_FADE_IN: u8 = 76;
    pub const CHAN_TIME_SHIFT: u8 = 89;
    pub const CHAN_SWING: u8 = 97;
    pub const CHAN_CUT_GROUP: u8 = 132;
    pub const CHAN_ECHO: u8 = 209;
    /// The root note up to version 11, written only when it was moved off C5.
    pub const CHAN_ROOT: u8 = 135;
    pub const CHAN_POLY: u8 = 221;
    pub const CHAN_TRACKING: u8 = 228;
    pub const CHAN_ADJUST: u8 = 229;
    pub const LAYER_CHILD: u8 = 94;
    pub const INSERT_ROUTING: u8 = 235;
    pub const INSERT_FLAGS: u8 = 236;
    pub const TRACK: u8 = 238;
    pub const TRACK_NAME: u8 = 239;
    pub const ARRANGEMENT_NAME: u8 = 241;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(u8),
    Word(u16),
    Int(u32),
    Short3([u8; 3]),
    Data(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: u8,
    pub value: Value,
}

impl Event {
    pub fn byte(id: u8, v: u8) -> Self {
        Self { id, value: Value::Byte(v) }
    }
    pub fn word(id: u8, v: u16) -> Self {
        Self { id, value: Value::Word(v) }
    }
    pub fn int(id: u8, v: u32) -> Self {
        Self { id, value: Value::Int(v) }
    }
    pub fn data(id: u8, v: Vec<u8>) -> Self {
        Self { id, value: Value::Data(v) }
    }
    /// A text event; UTF-16LE with a trailing NUL, the way current versions write text.
    pub fn text(id: u8, s: &str) -> Self {
        let mut body: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        body.extend_from_slice(&[0, 0]);
        Self::data(id, body)
    }
    pub fn as_u32(&self) -> Option<u32> {
        match self.value {
            Value::Byte(v) => Some(v as u32),
            Value::Word(v) => Some(v as u32),
            Value::Int(v) => Some(v),
            _ => None,
        }
    }
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.value {
            Value::Data(d) => Some(d),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    pub format: i16,
    pub channels: u16,
    pub ppq: u16,
}

/// The raw file: header plus every event, in order. Lossless.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlpFile {
    pub header: Header,
    pub events: Vec<Event>,
    /// Bytes after the data chunk, if any (normally none).
    pub trailer: Vec<u8>,
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
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

fn write_varint(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n > 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

/// "25.2.5.5319" -> (25, 2, 5, 5319). Missing parts are zero.
pub fn version_tuple(s: &str) -> (u32, u32, u32, u32) {
    let mut it = s.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

impl FlpFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let mut c = Cursor { b: bytes, i: 0 };
        if c.take(4).map_err(|_| "not an FLP file (too short)".to_string())? != b"FLhd" {
            return Err("not an FLP file (no FLhd header)".into());
        }
        let hsize = c.u32()?;
        if hsize != 6 {
            return Err(format!("unexpected header size {hsize}"));
        }
        let header = Header { format: c.u16()? as i16, channels: c.u16()?, ppq: c.u16()? };
        if c.take(4)? != b"FLdt" {
            return Err("missing FLdt data chunk".into());
        }
        let dsize = c.u32()? as usize;
        if c.left() < dsize {
            return Err("data chunk is shorter than its declared size".into());
        }
        let end = c.i + dsize;
        let mut events = Vec::new();
        let mut short3 = false;
        while c.i < end {
            let id = c.u8()?;
            let value = match id {
                id::SHORT3 if short3 => {
                    let s = c.take(3)?;
                    Value::Short3([s[0], s[1], s[2]])
                }
                0..=63 => Value::Byte(c.u8()?),
                64..=127 => Value::Word(c.u16()?),
                128..=191 => Value::Int(c.u32()?),
                192..=255 => {
                    let n = c.varint()?;
                    Value::Data(c.take(n)?.to_vec())
                }
            };
            if id == id::VERSION {
                if let Value::Data(d) = &value {
                    let v = version_tuple(&latin1(d));
                    short3 = v >= (25, 2, 1, 0);
                }
            }
            events.push(Event { id, value });
        }
        if c.i != end {
            return Err("an event runs past the end of the data chunk".into());
        }
        Ok(Self { header, events, trailer: bytes[end..].to_vec() })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for e in &self.events {
            body.push(e.id);
            match &e.value {
                Value::Byte(v) => body.push(*v),
                Value::Word(v) => body.extend_from_slice(&v.to_le_bytes()),
                Value::Int(v) => body.extend_from_slice(&v.to_le_bytes()),
                Value::Short3(v) => body.extend_from_slice(v),
                Value::Data(d) => {
                    write_varint(&mut body, d.len());
                    body.extend_from_slice(d);
                }
            }
        }
        let mut out = Vec::with_capacity(body.len() + 22 + self.trailer.len());
        out.extend_from_slice(b"FLhd");
        out.extend_from_slice(&6u32.to_le_bytes());
        out.extend_from_slice(&(self.header.format as u16).to_le_bytes());
        out.extend_from_slice(&self.header.channels.to_le_bytes());
        out.extend_from_slice(&self.header.ppq.to_le_bytes());
        out.extend_from_slice(b"FLdt");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(&self.trailer);
        out
    }
}

pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
}

fn utf16(bytes: &[u8]) -> String {
    let u: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let end = u.iter().position(|&x| x == 0).unwrap_or(u.len());
    String::from_utf16_lossy(&u[..end])
}

pub(crate) fn u32at(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).unwrap_or(0)
}
pub(crate) fn u16at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).unwrap_or(0)
}
fn f32at(d: &[u8], o: usize) -> f32 {
    f32::from_bits(u32at(d, o))
}

// ---------------------------------------------------------------------------------------------------------------
// Typed view
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelKind {
    #[default]
    Sampler,
    /// A generator plugin.
    Plugin,
    Layer,
    /// An audio clip (a sample placed on the playlist).
    AudioClip,
    AutomationClip,
    Other(u8),
}

impl ChannelKind {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Sampler,
            2 => Self::Plugin,
            3 => Self::Layer,
            4 => Self::AudioClip,
            5 => Self::AutomationClip,
            x => Self::Other(x),
        }
    }
}

/// One of the five per-channel envelopes (pan, volume, mod x, mod y, pitch). Times are raw file units.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Envelope {
    pub enabled: bool,
    pub predelay: i32,
    pub attack: i32,
    pub hold: i32,
    pub decay: i32,
    pub sustain: i32,
    pub release: i32,
    pub amount: i32,
    /// Bit 0: times snap to the tempo.
    pub flags: i32,
    /// Bend of the attack, decay and release lines, -128..=128.
    pub tension: [i32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct FlpChannel {
    pub index: u16,
    pub kind: ChannelKind,
    /// Internal generator name (empty for the plain sampler).
    pub plugin: String,
    /// The generator's stored settings.
    pub state: Vec<u8>,
    pub name: String,
    pub sample_path: String,
    pub color: u32,
    pub enabled: bool,
    /// Mixer insert this channel feeds; -1 means none (master).
    pub insert: i32,
    /// 0..=12800, centre 6400.
    pub pan: u32,
    /// 0..=12800, default 10000.
    pub volume: u32,
    pub pitch_cents: i32,
    /// Key that plays the sample at its recorded pitch (60 = C5 in the program's naming).
    pub root_key: u32,
    pub sample_flags: u32,
    pub envelopes: Vec<Envelope>,
    /// Raw automation-clip point data, when this is an automation clip.
    pub automation: Vec<u8>,
    /// Rack channels a layer channel plays.
    pub children: Vec<u16>,
    /// Sample treatment switches: 1 fade stereo, 2 reverse, 4 clip, 256 swap stereo.
    pub fx_flags: u16,
    pub preamp: u16,
    /// Fades, 0..=1024 of the sample.
    pub fade_in: u16,
    pub fade_out: u16,
    /// (cut, cut by) groups; 0 = none.
    pub cut_group: (u16, u16),
    /// Which of the rack's named groups the channel is filed under.
    pub rack_group: u32,
    pub zipped: bool,
    pub ping_pong: bool,
    pub fine_tune: i32,
    pub time_shift: u16,
    /// Arpeggiator: direction (0 off, up, down, up-down, up-down with the ends twice, random), range in
    /// octaves, chord (-1 from the held notes, -2 the same but lone notes hold, 0 none, then the chord list), time
    /// (1024 = one step), gate (48 = full), repeat.
    pub arp: [i32; 6],
    /// Echo switches: 2 ping pong, 4 fat mode.
    pub echo_flags: u8,
    /// Tracking, velocity first and keyboard second: middle value, then pan, mod x and mod y (-256..=256).
    pub tracking: Vec<[i32; 4]>,
    pub arp_slide: bool,
    /// Longest a note may last, on the same scale as the arpeggiator time; 1447 and up = no limit.
    pub gate: u16,
    pub full_porta: bool,
    pub add_to_key: bool,
    /// Lowest and highest key the channel answers to.
    pub key_range: (u32, u32),
    /// How much of the project swing this channel takes, 0..=128.
    pub swing: u16,
    /// Echo: feedback (0..=12800), pan (centre 6400), pitch step in cents, count, time (48 = one step).
    pub echo: [i32; 5],
    /// Filter: cutoff and resonance (0..=256) and type.
    pub filter: (u32, u32, u32),
    /// Polyphony: max voices, slide time, flags (1 mono, 2 portamento).
    pub poly: (u32, u32, u8),
    /// Level adjustments: pan, volume (12800 = none), unused, mod x, mod y.
    pub adjust: [i32; 5],
    pub normalize: bool,
    pub invert: bool,
    /// Stretch: length (384 = one beat, 0 = natural), pitch shift in cents, length multiplier as log2 * 10000, mode
    /// (0 resample, anything else keeps the pitch).
    pub stretch: (f32, i32, i32, i32),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Note {
    /// Ticks from the start of the pattern.
    pub position: u32,
    pub flags: u16,
    pub rack_channel: u16,
    /// Ticks; 0 means "play the whole sample" for one-shots.
    pub length: u32,
    /// 60 = C5.
    pub key: u16,
    pub group: u16,
    /// 120 = no fine pitch.
    pub fine_pitch: u8,
    pub release: u8,
    pub midi_channel: u8,
    /// 0..=128, 64 = centre.
    pub pan: u8,
    /// 0..=128, default 100.
    pub velocity: u8,
    pub mod_x: u8,
    pub mod_y: u8,
}

pub const NOTE_SIZE: usize = 24;

/// Bytes in one stored note: 20 up to version 8 (one byte for the key, no group), 24 since.
pub fn note_size(ver: (u32, u32, u32, u32)) -> usize {
    if ver < (9, 0, 0, 0) {
        20
    } else {
        NOTE_SIZE
    }
}

impl Note {
    fn read(n: &[u8]) -> Self {
        Self {
            position: u32at(n, 0),
            flags: u16at(n, 4),
            rack_channel: u16at(n, 6),
            length: u32at(n, 8),
            key: u16at(n, 12),
            group: u16at(n, 14),
            fine_pitch: n[16],
            release: n[18],
            midi_channel: n[19],
            pan: n[20],
            velocity: n[21],
            mod_x: n[22],
            mod_y: n[23],
        }
    }
    fn read_old(n: &[u8]) -> Self {
        Self {
            position: u32at(n, 0),
            flags: u16at(n, 4),
            rack_channel: u16at(n, 6),
            length: u32at(n, 8),
            key: n[12] as u16,
            group: 0,
            fine_pitch: n[13],
            release: n[14],
            midi_channel: n[15],
            pan: n[16],
            velocity: n[17],
            mod_x: n[18],
            mod_y: n[19],
        }
    }
    /// The 20-byte form of versions up to 8.
    pub fn write_old(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.position.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.rack_channel.to_le_bytes());
        out.extend_from_slice(&self.length.to_le_bytes());
        out.extend_from_slice(&[self.key.min(255) as u8, self.fine_pitch, self.release, self.midi_channel, self.pan, self.velocity, self.mod_x, self.mod_y]);
    }
    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.position.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.rack_channel.to_le_bytes());
        out.extend_from_slice(&self.length.to_le_bytes());
        out.extend_from_slice(&self.key.to_le_bytes());
        out.extend_from_slice(&self.group.to_le_bytes());
        out.extend_from_slice(&[self.fine_pitch, 0, self.release, self.midi_channel, self.pan, self.velocity, self.mod_x, self.mod_y]);
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlpPattern {
    /// 1-based, as shown in the pattern selector.
    pub index: u16,
    pub name: String,
    pub color: Option<u32>,
    pub notes: Vec<Note>,
    /// Control events: tick, which control (see `param`), destination, raw value.
    pub ctrl: Vec<(u32, u16, u16, i32)>,
    /// The markers on the pattern's own time line.
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipSource {
    /// 1-based pattern number.
    Pattern(u16),
    /// Rack channel index (an audio clip or automation clip channel).
    Channel(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistItem {
    pub position: u32,
    pub length: u32,
    /// 0-based from the top of the playlist.
    pub track: u16,
    pub source: ClipSource,
    pub muted: bool,
    /// Clips that share a number other than 0 are grouped and move as one.
    pub group: u16,
    /// Raw bits of the start/end offsets: ticks (i32, -1 = none) for pattern clips, f32 for audio clips.
    pub start_offset: u32,
    pub end_offset: u32,
}

#[derive(Debug, Clone, Default)]
pub struct FlpTrack {
    /// 1-based.
    pub index: u32,
    pub name: String,
    pub color: u32,
    pub enabled: bool,
    pub height: f32,
    /// Only the clip already on the track is drawn on it.
    pub content_locked: bool,
    /// Performance settings: what the track does after a clip (0 stay, 1 one shot, 2 march and wrap,
    /// 3 march and stay, 4 march and stop, 5 random, 6 random without repeats), how a press acts
    /// (0 retrigger, 1 hold and stop, 2 hold and motion, 3 latch), the trigger and position grids
    /// (0 off, 1 quarter beat, 2 half beat, 3 beat, 4 two beats, 5 four beats, 6 automatic).
    pub motion: u32,
    pub press: u32,
    pub trigger_sync: u32,
    pub queued: bool,
    pub tolerant: bool,
    pub position_sync: u32,
    pub grouped: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Marker {
    pub position: u32,
    pub name: String,
    /// What the marker does when the play position reaches it (0 = nothing, 8 = time signature change).
    pub kind: u8,
    pub sig: (u8, u8),
}

#[derive(Debug, Clone, Default)]
pub struct FlpArrangement {
    pub index: u16,
    pub name: String,
    pub items: Vec<PlaylistItem>,
    pub tracks: Vec<FlpTrack>,
    pub markers: Vec<Marker>,
    /// The playlist is in the layout from before version 11 (pattern blocks and 99 clip tracks).
    pub old_playlist: bool,
}

/// A slot that was never closed by an index event: old files end a slot at the next plugin or at the
/// insert's routing, and keep the slot number in the plugin head.
fn push_old_slot(i: &mut FlpInsert, slot: &mut FlpSlot, hint: &mut Option<u16>) {
    if slot.plugin.is_empty() {
        return;
    }
    let mut s = std::mem::take(slot);
    let free = |i: &FlpInsert, n: u16| !i.slots.iter().any(|x| x.index == n);
    s.index = match hint.take() {
        Some(n) if n < 10 && free(i, n) => n,
        _ => (0..10).find(|&n| free(i, n)).unwrap_or(9),
    };
    s.enabled = true;
    s.mix = 12800;
    i.slots.push(s);
}

#[derive(Debug, Clone, Default)]
pub struct FlpSlot {
    pub index: u16,
    pub plugin: String,
    pub name: String,
    pub enabled: bool,
    /// 0..=12800.
    pub mix: i32,
    /// The plugin's stored settings, as found.
    pub state: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FlpInsert {
    /// 0 = master, 1.. = inserts; the last one is the "current" track.
    pub index: usize,
    pub name: String,
    pub color: Option<u32>,
    pub flags: u32,
    /// 12800 = 100 % (0 dB), 16000 = maximum.
    pub volume: i32,
    /// -6400..=6400.
    pub pan: i32,
    pub stereo_separation: i32,
    /// The built-in equaliser: low, mid and high band, each gain (1/100 dB), frequency and width (0..=65536).
    pub eq: [[i32; 3]; 3],
    /// Send level per destination insert, 12800 = 100 %. Destinations not listed are at 100 %.
    pub route_levels: Vec<(usize, i32)>,
    /// routes[i] is true when this insert sends to insert i.
    pub routes: Vec<bool>,
    pub slots: Vec<FlpSlot>,
    pub input: i32,
    pub output: i32,
}

impl Default for FlpInsert {
    fn default() -> Self {
        Self { index: 0, name: String::new(), color: None, flags: 0, volume: 12800, pan: 0, stereo_separation: 0, eq: EQ_DEFAULT, route_levels: vec![], routes: vec![], slots: vec![], input: -1, output: -1 }
    }
}

pub const EQ_DEFAULT: [[i32; 3]; 3] = [[0, 5777, 17500], [0, 33145, 17500], [0, 55825, 17500]];
pub const INSERT_FLAG_INVERT: u32 = 1 << 0;
pub const INSERT_FLAG_SWAP: u32 = 1 << 1;
pub const INSERT_FLAG_FX: u32 = 1 << 2;
pub const INSERT_FLAG_ENABLED: u32 = 1 << 3;
pub const INSERT_FLAG_DOCK_MIDDLE: u32 = 1 << 6;
pub const INSERT_FLAG_DOCK_RIGHT: u32 = 1 << 7;
pub const INSERT_FLAG_SEPARATOR: u32 = 1 << 10;
pub const INSERT_FLAG_SOLO: u32 = 1 << 12;

#[derive(Debug, Clone, Default)]
pub struct Flp {
    pub header: Header,
    pub version: String,
    pub title: String,
    pub genre: String,
    pub artist: String,
    pub comment: String,
    pub tempo: f32,
    pub time_sig: (u8, u8),
    pub performance: bool,
    pub truncated: bool,
    pub pan_law: u8,
    /// The channel rack's named groups, and the one on show (-1 for all).
    pub groups: Vec<String>,
    pub cur_group: i32,
    pub main_pitch: i16,
    /// The transport was left on the whole song, not on one pattern.
    pub song_mode: bool,
    pub shuffle: u8,
    pub current_pattern: u16,
    pub channels: Vec<FlpChannel>,
    pub patterns: Vec<FlpPattern>,
    pub arrangements: Vec<FlpArrangement>,
    pub inserts: Vec<FlpInsert>,
    /// Automation clips and other controllers tied to the controls they drive.
    pub links: Vec<FlpLink>,
    /// Mapping formulas: which entry of `links` and the text written after it.
    pub link_formulas: Vec<(usize, String)>,
    /// The file keeps its step sequencer dots in the early per-step events instead of among the notes.
    pub has_dots: bool,
    pub file: FlpFile,
}

/// The text of a mapping formula record: two-byte characters, or one-byte ones in old files.
pub fn formula_text(d: &[u8]) -> String {
    let wide = d.len() >= 2 && d.len() % 2 == 0 && d.chunks(2).all(|c| c[1] == 0);
    let s: String = if wide { d.chunks(2).map(|c| c[0] as char).collect() } else { d.iter().map(|b| *b as char).collect() };
    s.trim_end_matches('\0').to_string()
}

/// One "this controls that" record (event 227).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlpLink {
    /// 0 when the source is an automation clip channel; other values are other kinds of controller.
    pub kind: u16,
    /// Rack channel of the automation clip.
    pub source: u16,
    pub extra: u32,
    /// Which control of the destination (see `param`).
    pub param: u16,
    /// A rack channel number, or a mixer insert and slot packed as in the mixer parameter records, or
    /// `DEST_GLOBAL`.
    pub dest: u16,
}

impl FlpLink {
    /// The source is an automation clip channel. Versions 9 and 10 wrote the channel a second time in
    /// the upper half of the spare field.
    pub fn from_clip(&self) -> bool {
        self.kind == 0 && (self.extra == 0 || self.extra == (self.source as u32) << 16)
    }
}

/// The same test on the bytes of a link record.
pub fn link_from_clip(d: &[u8]) -> bool {
    d.len() >= 12 && u16at(d, 0) == 0 && (u32at(d, 4) == 0 || u32at(d, 4) == (u16at(d, 2) as u32) << 16)
}

/// Control numbers used by links and by the mixer parameter records.
pub mod param {
    pub const CHAN_VOLUME: u16 = 0;
    pub const CHAN_PAN: u16 = 1;
    pub const CHAN_PITCH: u16 = 4;
    /// Set on a channel or slot control that belongs to the plugin itself.
    pub const PLUGIN_FLAG: u16 = 0x8000;
    pub const SLOT_ENABLED: u16 = 0x1f00;
    pub const SLOT_MIX: u16 = 0x1f01;
    pub const INSERT_VOLUME: u16 = 0x1fc0;
    pub const INSERT_PAN: u16 = 0x1fc1;
    pub const INSERT_SEPARATION: u16 = 0x1fc2;
    pub const GLOBAL_VOLUME: u16 = 0;
    pub const GLOBAL_PITCH: u16 = 2;
    pub const GLOBAL_TEMPO: u16 = 5;
}

pub const DEST_GLOBAL: u16 = 0x4000;

/// Mixer inserts are numbered from here in parameter records and link destinations: 448 from 24.2 on,
/// 128 before.
pub fn mixer_base(ver: (u32, u32, u32, u32)) -> usize {
    if ver >= (24, 2, 0, 0) { 448 } else { 128 }
}

/// Files from before the event that opens a mixer track: each track is its effects, then colour, name and
/// input, closed by its output. Gives the first event of the first track and the last output event.
pub fn old_mixer_span(ev: &[Event]) -> Option<(usize, usize)> {
    if ev.iter().any(|e| e.id == id::INSERT_FLAGS) {
        return None;
    }
    let first = ev.iter().position(|e| e.id == id::INSERT_OUTPUT)?;
    let last = ev.iter().rposition(|e| e.id == id::INSERT_OUTPUT)?;
    let mut start = first;
    while start > 0 && matches!(ev[start - 1].id, id::PLUGIN_INTERNAL_NAME | id::NEW_PLUGIN | id::PLUGIN_NAME | id::PLUGIN_PARAMS | id::INSERT_COLOR | id::INSERT_NAME | id::INSERT_INPUT) {
        start -= 1;
    }
    // a channel's own plug-in events may run straight into the mixer: a track's effects start at a plug-in name
    while start < first && !matches!(ev[start].id, id::PLUGIN_INTERNAL_NAME | id::INSERT_COLOR | id::INSERT_NAME | id::INSERT_INPUT) {
        start += 1;
    }
    Some((start, last))
}

/// A mixer value as the early versions store it: their own parameter number and scale.
pub fn old_mixer_record(pid: u8, value: i32) -> Option<(u8, i32)> {
    let cents = |v: i32| (v as f32 / 100.0).round() as i32;
    Some(match pid {
        0 | 1 => (pid, if pid == 1 { cents(value) } else { value }),
        192 => (96, cents(value)),
        193 => (97, cents(value)),
        194 => (98, value),
        208..=210 => (pid - 96, value),
        216..=218 => (pid - 96, value),
        224..=226 => (pid - 96, value),
        129..=132 => (pid - 65, cents(value)),
        _ => return None,
    })
}

/// One point of an automation clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoPoint {
    /// Quarter notes after the previous point.
    pub delta: f64,
    /// 0..=1 of the control's travel.
    pub value: f64,
    /// Bend of the line arriving at this point, -1..=1.
    pub tension: f32,
    /// Shape of the line arriving at this point.
    pub mode: u8,
}

/// The points of an automation clip record (event 234): a 21-byte head whose last word is the point count,
/// then 24 bytes per point.
pub fn parse_automation(d: &[u8]) -> Vec<AutoPoint> {
    if d.len() < 21 {
        return vec![];
    }
    let n = u32at(d, 17) as usize;
    let f64at = |o: usize| f64::from_le_bytes(d[o..o + 8].try_into().unwrap());
    (0..n)
        .map_while(|k| {
            let o = 21 + 24 * k;
            (o + 24 <= d.len()).then(|| AutoPoint { delta: f64at(o), value: f64at(o + 8), tension: f32::from_bits(u32at(d, o + 16)), mode: d[o + 20] })
        })
        .collect()
}

#[derive(PartialEq)]
enum Scope {
    Head,
    Channel,
    Arrangement,
    Insert,
}

/// Parse a .flp into the typed view (the raw events stay available in `.file`).
pub fn parse(bytes: &[u8]) -> Result<Flp, String> {
    Ok(Flp::from_file(FlpFile::parse(bytes)?))
}

impl Flp {
    pub fn version_tuple(&self) -> (u32, u32, u32, u32) {
        version_tuple(&self.version)
    }

    pub fn from_file(file: FlpFile) -> Self {
        let mut flp = Flp { header: file.header.clone(), time_sig: (4, 4), ..Default::default() };
        let mut unicode = false;
        let mut scope = Scope::Head;
        let mut patterns: BTreeMap<u16, FlpPattern> = BTreeMap::new();
        let mut cur_pat: u16 = 0;
        let mut insert_open = false;
        let mut slot = FlpSlot::default();
        // files from before the slot index event name the slot inside the plugin head instead
        let mut slot_hint: Option<u16> = None;
        let mut mixer_params: Vec<u8> = vec![];
        // an insert's name and colour are written just before the event that opens it
        let mut next_insert_name: Option<String> = None;
        let mut next_insert_color: Option<u32> = None;
        let mut early_markers: Vec<Marker> = vec![];
        // patterns carry markers of their own, written with the same events
        let mut in_pattern = false;
        // the channel whose name came from the early name event, which wins over a later plug-in name
        let mut named_chan: Option<usize> = None;

        // Early files keep the step sequencer apart from the notes: inside a channel, a pattern selector and then
        // one event per lit step (low byte the step, high byte the value), with the step's own level, pan, key
        // and so on in like events. Keyed by channel, pattern and step: [lit, level, pan, key, pitch, shift, x, y].
        let mut dots: BTreeMap<(u16, u16, u8), [Option<u8>; 8]> = BTreeMap::new();

        let old_mixer = old_mixer_span(&file.events);
        for (at, e) in file.events.iter().enumerate() {
            let text = |d: &[u8]| if unicode { utf16(d) } else { latin1(d) };
            if let Some((a, b)) = old_mixer {
                if at >= a && at <= b && !insert_open {
                    scope = Scope::Insert;
                    insert_open = true;
                    slot = FlpSlot::default();
                    slot_hint = None;
                    let index = flp.inserts.len();
                    flp.inserts.push(FlpInsert { index, ..Default::default() });
                }
            }
            match (e.id, &e.value) {
                (id::VERSION, Value::Data(d)) => {
                    flp.version = latin1(d);
                    unicode = version_tuple(&flp.version) >= (11, 5, 0, 0);
                }
                (id::TITLE, Value::Data(d)) => flp.title = text(d),
                (id::GENRE, Value::Data(d)) => flp.genre = text(d),
                (id::ARTIST, Value::Data(d)) => flp.artist = text(d),
                (id::COMMENT, Value::Data(d)) => flp.comment = text(d),
                (id::TEMPO, Value::Int(v)) => flp.tempo = *v as f32 / 1000.0,
                (id::GROUP_NAME, Value::Data(d)) => flp.groups.push(text(d)),
                (id::CUR_GROUP, Value::Int(v)) => flp.cur_group = *v as i32,
                (id::TEMPO_WORD, Value::Word(v)) if flp.tempo == 0.0 => flp.tempo = *v as f32,
                (id::TIMESIG_NUM, Value::Byte(v)) => flp.time_sig.0 = *v,
                (id::PERFORMANCE, Value::Byte(v)) => flp.performance = *v != 0,
                (id::TRUNCATED, Value::Byte(v)) => flp.truncated = *v != 0,
                (id::PAN_LAW, Value::Byte(v)) => flp.pan_law = *v,
                (id::TIMESIG_BEAT, Value::Byte(v)) => flp.time_sig.1 = *v,
                (id::MAIN_PITCH, Value::Word(v)) => flp.main_pitch = *v as i16,
                (id::LOOP_ACTIVE, Value::Byte(v)) => flp.song_mode = *v != 0,
                (id::SHUFFLE, Value::Byte(v)) => flp.shuffle = *v,
                (id::CURRENT_PATTERN, Value::Word(v)) => flp.current_pattern = *v,

                (id::DOT | id::DOT_VOL | id::DOT_PAN | id::DOT_NOTE | id::DOT_PITCH | id::DOT_SHIFT | id::DOT_CUT | id::DOT_RES, Value::Word(w)) if scope == Scope::Channel => {
                    if let Some(c) = flp.channels.last() {
                        let field = match e.id {
                            id::DOT => 0,
                            id::DOT_VOL => 1,
                            id::DOT_PAN => 2,
                            id::DOT_NOTE => 3,
                            id::DOT_PITCH => 4,
                            id::DOT_SHIFT => 5,
                            id::DOT_CUT => 6,
                            _ => 7,
                        };
                        dots.entry((c.index, cur_pat, *w as u8)).or_default()[field] = Some((*w >> 8) as u8);
                    }
                }

                // ---- patterns
                (id::NEW_PATTERN, Value::Word(v)) => {
                    in_pattern = true;
                    cur_pat = *v;
                    patterns.entry(cur_pat).or_insert_with(|| FlpPattern { index: cur_pat, ..Default::default() });
                }
                (id::PATTERN_NAME, Value::Data(d)) => {
                    patterns.entry(cur_pat).or_insert_with(|| FlpPattern { index: cur_pat, ..Default::default() }).name = text(d);
                }
                (id::PATTERN_COLOR, Value::Int(v)) => {
                    patterns.entry(cur_pat).or_insert_with(|| FlpPattern { index: cur_pat, ..Default::default() }).color = Some(*v);
                }
                (id::PATTERN_NOTES, Value::Data(d)) => {
                    let p = patterns.entry(cur_pat).or_insert_with(|| FlpPattern { index: cur_pat, ..Default::default() });
                    if note_size(version_tuple(&flp.version)) == 20 {
                        p.notes.extend(d.chunks_exact(20).map(Note::read_old));
                    } else {
                        p.notes.extend(d.chunks_exact(NOTE_SIZE).map(Note::read));
                    }
                }
                (id::PATTERN_CTRL, Value::Data(d)) => {
                    let p = patterns.entry(cur_pat).or_insert_with(|| FlpPattern { index: cur_pat, ..Default::default() });
                    p.ctrl.extend(d.chunks_exact(12).map(|r| (u32at(r, 0), u16at(r, 4), u16at(r, 6), u32at(r, 8) as i32)));
                }

                // ---- channels
                (id::NEW_CHANNEL, Value::Word(v)) => {
                    scope = Scope::Channel;
                    flp.channels.push(FlpChannel { index: *v, enabled: true, insert: -1, pan: 6400, volume: 10000, root_key: 60, swing: 128, echo: [0, 6400, 0, 4, 144], filter: (256, 0, 0), adjust: [0, 12800, 0, 0, 0], arp: [0, 1, -1, 1024, 48, 1], gate: 1447, full_porta: true, add_to_key: false, key_range: (0, 256), ..Default::default() });
                }
                (id::NEW_ARRANGEMENT, Value::Word(v)) => {
                    in_pattern = false;
                    scope = Scope::Arrangement;
                    flp.arrangements.push(FlpArrangement { index: *v, ..Default::default() });
                }
                (id::PLAYLIST, Value::Data(d)) => {
                    in_pattern = false;
                    if flp.arrangements.is_empty() {
                        // files from before arrangements existed have one unnamed playlist
                        flp.arrangements.push(FlpArrangement { name: "Arrangement".into(), ..Default::default() });
                    }
                    let a = flp.arrangements.last_mut().unwrap();
                    let ver = version_tuple(&flp.version);
                    (a.items, a.old_playlist) = parse_playlist(d, ver);
                }
                (id::ARRANGEMENT_NAME, Value::Data(d)) => {
                    if let Some(a) = flp.arrangements.last_mut() {
                        a.name = text(d);
                    }
                }
                (id::TRACK, Value::Data(d)) => {
                    if flp.arrangements.is_empty() {
                        flp.arrangements.push(FlpArrangement { name: "Arrangement".into(), ..Default::default() });
                    }
                    let a = flp.arrangements.last_mut().unwrap();
                    let perf = d.len() >= 47;
                    let word = |o: usize| if perf { u32at(d, o) } else { 0 };
                    a.tracks.push(FlpTrack {
                        index: u32at(d, 0),
                        name: String::new(),
                        color: u32at(d, 4),
                        enabled: d.get(12).copied().unwrap_or(1) != 0,
                        height: f32at(d, 13),
                        content_locked: d.get(21).copied().unwrap_or(0) != 0,
                        motion: word(22),
                        press: word(26),
                        trigger_sync: if perf { u32at(d, 30) } else { 5 },
                        queued: word(34) != 0,
                        tolerant: if perf { u32at(d, 38) != 0 } else { true },
                        position_sync: word(42),
                        grouped: perf && d[46] != 0,
                    });
                }
                (id::TRACK_NAME, Value::Data(d)) => {
                    if let Some(t) = flp.arrangements.last_mut().and_then(|a| a.tracks.last_mut()) {
                        t.name = text(d);
                    }
                }
                (id::MARKER_POSITION, Value::Int(v)) if in_pattern => {
                    let p = patterns.entry(cur_pat).or_insert_with(|| FlpPattern { index: cur_pat, ..Default::default() });
                    p.markers.push(Marker { position: *v & 0x00ff_ffff, name: String::new(), kind: (*v >> 24) as u8, sig: (0, 0) });
                }
                (id::MARKER_NUMERATOR | id::MARKER_DENOMINATOR, Value::Byte(v)) if in_pattern => {
                    if let Some(m) = patterns.get_mut(&cur_pat).and_then(|p| p.markers.last_mut()) {
                        if e.id == id::MARKER_NUMERATOR {
                            m.sig.0 = *v;
                        } else {
                            m.sig.1 = *v;
                        }
                    }
                }
                (id::MARKER_NAME, Value::Data(d)) if in_pattern => {
                    if let Some(m) = patterns.get_mut(&cur_pat).and_then(|p| p.markers.last_mut()) {
                        m.name = text(d);
                    }
                }
                (id::MARKER_POSITION | id::MARKER_NAME | id::MARKER_NUMERATOR | id::MARKER_DENOMINATOR, _) if in_pattern => {}
                (id::MARKER_NUMERATOR | id::MARKER_DENOMINATOR, Value::Byte(v)) => {
                    let m = match flp.arrangements.last_mut() {
                        Some(a) => a.markers.last_mut(),
                        None => early_markers.last_mut(),
                    };
                    if let Some(m) = m {
                        if e.id == id::MARKER_NUMERATOR {
                            m.sig.0 = *v;
                        } else {
                            m.sig.1 = *v;
                        }
                    }
                }
                (id::MARKER_POSITION, Value::Int(v)) => {
                    // the top byte carries the marker kind; markers written ahead of the first arrangement wait for it
                    let m = Marker { position: *v & 0x00ff_ffff, name: String::new(), kind: (*v >> 24) as u8, sig: (0, 0) };
                    match flp.arrangements.last_mut() {
                        Some(a) => a.markers.push(m),
                        None => early_markers.push(m),
                    }
                }
                (id::MARKER_NAME, Value::Data(d)) => {
                    let name = text(d);
                    match flp.arrangements.last_mut() {
                        Some(a) => {
                            if let Some(m) = a.markers.last_mut() {
                                m.name = name;
                            }
                        }
                        None => {
                            if let Some(m) = early_markers.last_mut() {
                                m.name = name;
                            }
                        }
                    }
                }

                // ---- mixer
                (id::INSERT_FLAGS, Value::Data(d)) => {
                    scope = Scope::Insert;
                    insert_open = true;
                    slot = FlpSlot::default();
                    slot_hint = None;
                    let index = flp.inserts.len();
                    flp.inserts.push(FlpInsert { index, flags: u32at(d, 4), name: next_insert_name.take().unwrap_or_default(), color: next_insert_color.take(), ..Default::default() });
                }
                (id::INSERT_OUTPUT, Value::Int(v)) => {
                    if let (true, Some(i)) = (insert_open, flp.inserts.last_mut()) {
                        push_old_slot(i, &mut slot, &mut slot_hint);
                        i.output = *v as i32;
                    }
                    insert_open = false;
                }
                (id::INSERT_INPUT, Value::Int(v)) => {
                    if let (true, Some(i)) = (insert_open, flp.inserts.last_mut()) {
                        i.input = *v as i32;
                    }
                }
                (id::INSERT_NAME, Value::Data(d)) => {
                    match (insert_open, flp.inserts.last_mut()) {
                        (true, Some(i)) => i.name = text(d),
                        _ => next_insert_name = Some(text(d)),
                    }
                }
                (id::INSERT_COLOR, Value::Int(v)) => {
                    match (insert_open, flp.inserts.last_mut()) {
                        (true, Some(i)) => i.color = Some(*v),
                        _ => next_insert_color = Some(*v),
                    }
                }
                (id::INSERT_ROUTING, Value::Data(d)) => {
                    if let (true, Some(i)) = (insert_open, flp.inserts.last_mut()) {
                        push_old_slot(i, &mut slot, &mut slot_hint);
                        i.routes = d.iter().map(|&b| b != 0).collect();
                    }
                }
                (id::SLOT_INDEX, Value::Word(v)) => {
                    if let (true, Some(i)) = (insert_open, flp.inserts.last_mut()) {
                        let mut s = std::mem::take(&mut slot);
                        s.index = *v;
                        s.enabled = true;
                        s.mix = 12800;
                        if !s.plugin.is_empty() || !s.name.is_empty() {
                            i.slots.push(s);
                        }
                        slot_hint = None;
                    }
                }
                (id::NEW_PLUGIN, Value::Data(d)) if scope == Scope::Insert && d.len() >= 8 => slot_hint = Some(u32at(d, 4) as u16),
                (id::MIXER_PARAMS, Value::Data(d)) => mixer_params = d.clone(),

                // ---- ids shared between channels and effect slots
                (id::PLUGIN_INTERNAL_NAME, Value::Data(d)) => match scope {
                    Scope::Channel => {
                        if let Some(c) = flp.channels.last_mut() {
                            c.plugin = text(d);
                        }
                    }
                    Scope::Insert => {
                        if let (true, Some(i)) = (insert_open, flp.inserts.last_mut()) {
                            push_old_slot(i, &mut slot, &mut slot_hint);
                        }
                        slot.plugin = text(d);
                    }
                    _ => {}
                },
                // up to version 11 a channel's display name is in this event
                (id::CHAN_NAME, Value::Data(d)) if scope == Scope::Channel => {
                    if let Some(c) = flp.channels.last_mut() {
                        c.name = text(d);
                        named_chan = Some(flp.channels.len() - 1);
                    }
                }
                (id::PLUGIN_NAME, Value::Data(d)) => match scope {
                    Scope::Channel => {
                        if named_chan != Some(flp.channels.len().wrapping_sub(1)) {
                            if let Some(c) = flp.channels.last_mut() {
                                c.name = text(d);
                            }
                        }
                    }
                    Scope::Insert => slot.name = text(d),
                    _ => {}
                },
                (id::PLUGIN_PARAMS, Value::Data(d)) if scope == Scope::Insert => slot.state = d.clone(),
                (id::PLUGIN_PARAMS, Value::Data(d)) if scope == Scope::Channel => {
                    if let Some(c) = flp.channels.last_mut() {
                        c.state = d.clone();
                    }
                }
                (id::COLOR, Value::Int(v)) if scope == Scope::Channel => {
                    if let Some(c) = flp.channels.last_mut() {
                        c.color = *v;
                    }
                }
                _ if scope == Scope::Channel => {
                    let Some(c) = flp.channels.last_mut() else { continue };
                    match (e.id, &e.value) {
                        (id::CHAN_TYPE, Value::Byte(v)) => c.kind = ChannelKind::from_byte(*v),
                        (id::CHAN_ENABLED, Value::Byte(v)) => c.enabled = *v != 0,
                        (id::CHAN_TARGET_INSERT, Value::Byte(v)) => c.insert = *v as i8 as i32,
                        (id::CHAN_TARGET_INSERT_WORD, Value::Word(v)) => c.insert = *v as i16 as i32,
                        (id::SAMPLE_PATH, Value::Data(d)) => c.sample_path = text(d),
                        (id::SAMPLE_FLAGS, Value::Int(v)) => c.sample_flags = *v,
                        (id::CHAN_LEVELS, Value::Data(d)) if d.len() >= 12 => {
                            c.pan = u32at(d, 0);
                            c.volume = u32at(d, 4);
                            if version_tuple(&flp.version) < (7, 0, 0, 0) && c.pan <= 128 && c.volume <= 128 {
                                // 128 steps then, a hundred times finer since
                                c.pan *= 100;
                                c.volume *= 100;
                            }
                            c.pitch_cents = u32at(d, 8) as i32;
                            if d.len() >= 24 {
                                c.filter = (u32at(d, 12), u32at(d, 16), u32at(d, 20));
                            }
                        }
                        (id::CHAN_PARAMS, Value::Data(d)) if d.len() >= 20 => {
                            if c.root_key == 60 || version_tuple(&flp.version) >= (12, 0, 0, 0) {
                                c.root_key = u32at(d, 16);
                            }
                            c.echo_flags = d[10];
                            if d.len() >= 66 {
                                let i = |o: usize| u32at(d, o) as i32;
                                c.arp = [i(40), i(44), i(48), i(52), i(56), c.arp[5]];
                                c.arp_slide = d[60] != 0;
                                c.full_porta = d[62] != 0;
                                c.gate = u32at(d, 64) as u16;
                                c.add_to_key = d[63] != 0;
                            }
                            if d.len() >= 76 {
                                c.key_range = (u32at(d, 68), u32at(d, 72));
                            }
                            if d.len() >= 96 {
                                c.arp[5] = u32at(d, 92) as i32;
                            }
                            if d.len() >= 84 {
                                c.normalize = d[80] != 0;
                                c.invert = d[81] != 0;
                            }
                            if d.len() >= 112 {
                                // the length is a whole number in older files and a float in newer ones
                                let raw = u32at(d, 96);
                                let len = if raw >= 0x3000_0000 { f32at(d, 96) } else { raw as f32 };
                                c.stretch = (if len.is_finite() { len } else { 0.0 }, u32at(d, 100) as i32, u32at(d, 104) as i32, u32at(d, 108) as i32);
                            }
                        }
                        (id::CHAN_ENVELOPE, Value::Data(d)) if d.len() >= 36 => {
                            let i = |o: usize| if d.len() >= o + 4 { u32at(d, o) as i32 } else { 0 };
                            c.envelopes.push(Envelope { enabled: i(4) != 0, predelay: i(8), attack: i(12), hold: i(16), decay: i(20), sustain: i(24), release: i(28), amount: i(32), flags: i(0), tension: [i(56), i(60), i(64)] });
                        }
                        (id::CHAN_FX_FLAGS, Value::Word(v)) => c.fx_flags = *v,
                        (id::CHAN_PREAMP, Value::Word(v)) => c.preamp = *v,
                        (id::CHAN_FADE_IN, Value::Word(v)) => c.fade_in = *v,
                        (id::CHAN_FADE_OUT, Value::Word(v)) => c.fade_out = *v,
                        (id::CHAN_TIME_SHIFT, Value::Word(v)) => c.time_shift = *v,
                        (id::CHAN_SWING, Value::Word(v)) => c.swing = *v,
                        (id::CHAN_CUT_GROUP, Value::Int(v)) => c.cut_group = ((*v & 0xffff) as u16, (*v >> 16) as u16),
                        (id::CHAN_GROUP, Value::Int(v)) => c.rack_group = *v,
                        (id::CHAN_ZIPPED, Value::Byte(v)) => c.zipped = *v != 0,
                        (id::CHAN_PING_PONG, Value::Byte(v)) => c.ping_pong = *v != 0,
                        (id::CHAN_FINE_TUNE, Value::Int(v)) => c.fine_tune = (*v as i32).clamp(-100, 100),
                        (id::CHAN_ROOT, Value::Int(v)) if *v > 0 && *v < 132 && version_tuple(&flp.version) < (12, 0, 0, 0) => c.root_key = *v,
                        (id::CHAN_ECHO, Value::Data(d)) if d.len() >= 20 => {
                            for k in 0..5 {
                                c.echo[k] = u32at(d, k * 4) as i32;
                            }
                            if version_tuple(&flp.version) < (7, 0, 0, 0) && c.echo[0] <= 128 && c.echo[1] <= 128 {
                                c.echo[0] *= 100;
                                c.echo[1] *= 100;
                            }
                        }
                        (id::CHAN_TRACKING, Value::Data(d)) if d.len() >= 16 => {
                            c.tracking.push([u32at(d, 0) as i32, u32at(d, 4) as i32, u32at(d, 8) as i32, u32at(d, 12) as i32]);
                        }
                        (id::CHAN_POLY, Value::Data(d)) if d.len() >= 9 => c.poly = (u32at(d, 0), u32at(d, 4), d[8]),
                        (id::CHAN_ADJUST, Value::Data(d)) if d.len() >= 20 => {
                            for k in 0..5 {
                                c.adjust[k] = u32at(d, k * 4) as i32;
                            }
                            if version_tuple(&flp.version) < (7, 0, 0, 0) && c.adjust[0].abs() <= 64 && c.adjust[1] <= 256 {
                                c.adjust[0] *= 100;
                                c.adjust[1] *= 100;
                            }
                        }
                        (id::CHAN_AUTOMATION, Value::Data(d)) => c.automation = d.clone(),
                        (id::LAYER_CHILD, Value::Word(k)) => c.children.push(*k),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        flp.has_dots = !dots.is_empty();
        let step = (flp.header.ppq as u32 / 4).max(1);
        for ((chan, pat, at), d) in dots {
            if d[0].unwrap_or(0) == 0 {
                continue;
            }
            let p = patterns.entry(pat).or_insert_with(|| FlpPattern { index: pat, ..Default::default() });
            p.notes.push(Note {
                position: at as u32 * step + d[5].unwrap_or(0) as u32,
                flags: 0x4000,
                rack_channel: chan,
                length: 0,
                key: d[3].unwrap_or(60) as u16,
                group: 0,
                fine_pitch: d[4].unwrap_or(120),
                release: 64,
                midi_channel: 0,
                pan: d[2].unwrap_or(64),
                velocity: d[1].unwrap_or(100),
                mod_x: d[6].unwrap_or(128),
                mod_y: d[7].unwrap_or(128),
            });
            p.notes.sort_by_key(|n| n.position);
        }

        // patterns: the highest numbered pattern that has anything in it bounds the list
        flp.patterns = patterns.into_values().collect();

        // track numbering in playlist items is stored upside down; the track count tells us the top
        let ver = version_tuple(&flp.version);
        for a in &mut flp.arrangements {
            // old projects list only some of their 199 tracks
            let n = if ver < (12, 9, 0, 0) { 199 } else if !a.tracks.is_empty() { a.tracks.len() as u16 } else { 500 };
            if !a.old_playlist {
                for it in &mut a.items {
                    it.track = (n - 1).saturating_sub(it.track);
                }
            }
            a.items.sort_by_key(|i| (i.position, i.track));
        }

        if !early_markers.is_empty() {
            if flp.arrangements.is_empty() {
                flp.arrangements.push(FlpArrangement { name: "Arrangement".into(), ..Default::default() });
            }
            let a = &mut flp.arrangements[0];
            early_markers.append(&mut a.markers);
            a.markers = early_markers;
        }
        if old_mixer.is_some() {
            // the early mixer: every track plays into the master and can feed the four send tracks at its end
            let n = flp.inserts.len();
            for (i, ins) in flp.inserts.iter_mut().enumerate().skip(1) {
                ins.routes = vec![false; n];
                ins.routes[0] = true;
                if n >= 6 && i < n - 4 {
                    for d in n - 4..n {
                        ins.routes[d] = true;
                        ins.route_levels.push((d, 0));
                    }
                }
            }
            let mut now = Vec::with_capacity(mixer_params.len());
            for r in mixer_params.chunks_exact(12) {
                let value = u32at(r, 8) as i32;
                let (pid, value) = match r[4] {
                    0 => (0, value),
                    1 => (1, value * 100),
                    64..=67 => ((64 + n.saturating_sub(4) + (r[4] as usize - 64)).min(191) as u8, value * 100),
                    96 => (192, value * 100),
                    97 => (193, value * 100),
                    98 => (194, value),
                    112..=114 | 120..=122 | 128..=130 => (r[4] + 96, value),
                    _ => continue,
                };
                now.extend_from_slice(&r[..4]);
                now.push(pid);
                now.extend_from_slice(&r[5..8]);
                now.extend_from_slice(&value.to_le_bytes());
            }
            apply_mixer_params(&mut flp.inserts, &now, mixer_base(ver));
        } else {
            apply_mixer_params(&mut flp.inserts, &mixer_params, mixer_base(ver));
        }
        for (i, e) in file.events.iter().enumerate() {
            if let (id::LINK, Value::Data(d)) = (e.id, &e.value) {
                if d.len() >= 12 {
                    if let Some(Event { id: id::LINK_FORMULA, value: Value::Data(t) }) = file.events.get(i + 1) {
                        flp.link_formulas.push((flp.links.len(), formula_text(t)));
                    }
                    flp.links.push(FlpLink { kind: u16at(d, 0), source: u16at(d, 2), extra: u32at(d, 4), param: u16at(d, 8), dest: u16at(d, 10) });
                }
            }
        }
        flp.file = file;
        flp
    }
}

/// Item size of a playlist from before version 11, when the playlist had a block area (one row per pattern)
/// next to 99 clip tracks. `None` for the later layout.
pub(crate) fn old_playlist_size(d: &[u8], ver: (u32, u32, u32, u32)) -> Option<usize> {
    if d.is_empty() || ver >= (11, 0, 0, 0) {
        return None;
    }
    // 28 bytes up to version 8 (and a few early 9 files), 32 after; the constant after the track word tells them apart
    let fits = |s: usize, at: usize| d.len() % s == 0 && d.chunks_exact(s).all(|it| matches!(u16at(it, 4), 0 | 0x5000) && u32at(it, at) == 0x8080_6440);
    if fits(28, 16) {
        Some(28)
    } else if fits(32, 20) {
        Some(32)
    } else {
        None
    }
}

const OLD_BLOCK_TOP_28: u32 = 30951;
const OLD_CLIP_TOP_28: u32 = 30818;
const OLD_BLOCK_TOP_32: u32 = 999;
const OLD_CLIP_TOP_32: u32 = 98;

/// Returns the items and whether this was the old layout (tracks already final, counted from the top).
fn parse_playlist(d: &[u8], ver: (u32, u32, u32, u32)) -> (Vec<PlaylistItem>, bool) {
    const BASE: u16 = 0x5000;
    if let Some(size) = old_playlist_size(d, ver) {
        // A block sits in the row of its pattern. Today each pattern that has blocks gets a track of its own, the
        // lowest pattern on top, sharing those tracks with the clip tracks, which also count from the top.
        let (block_top, clip_top) = if size == 28 { (OLD_BLOCK_TOP_28, OLD_CLIP_TOP_28) } else { (OLD_BLOCK_TOP_32, OLD_CLIP_TOP_32) };
        let block_pattern = |it: &[u8]| block_top.saturating_sub(u16at(it, 12) as u32).clamp(1, 999) as u16;
        let mut used: Vec<u16> = d.chunks_exact(size).filter(|it| u16at(it, 4) == 0).map(block_pattern).collect();
        used.sort_unstable();
        used.dedup();
        let items = d
            .chunks_exact(size)
            .map(|it| {
                let (start, end) = (u32at(it, size - 8), u32at(it, size - 4));
                let muted = size == 32 && u16at(it, 18) & 0x2000 != 0;
                let group = if size == 32 { u16at(it, 14) } else { 0 };
                if u16at(it, 4) == 0 {
                    let p = block_pattern(it);
                    PlaylistItem { position: u32at(it, 0), length: u32at(it, 8), track: used.binary_search(&p).unwrap_or(0) as u16, source: ClipSource::Pattern(p), muted, group, start_offset: u32::MAX, end_offset: u32::MAX }
                } else {
                    let ix = u16at(it, 6);
                    PlaylistItem {
                        position: u32at(it, 0),
                        length: u32at(it, 8),
                        track: clip_top.saturating_sub(u16at(it, 12) as u32) as u16,
                        source: if ix > BASE { ClipSource::Pattern(ix - BASE) } else { ClipSource::Channel(ix) },
                        muted,
                        group,
                        start_offset: start,
                        end_offset: end,
                    }
                }
            })
            .collect();
        return (items, true);
    }
    // The item grew over the years (32, then 60, then 80 bytes, maybe more later). Every item carries the
    // constant 0x5000 at offset 4, so pick the size for which that holds for every item.
    let size = [32usize, 60, 80, 88, 96, 104, 112, 120, 128]
        .into_iter()
        .find(|&s| !d.is_empty() && d.len() % s == 0 && d.chunks_exact(s).all(|it| u16at(it, 4) == BASE));
    let Some(size) = size else { return (vec![], false) };
    let items = d
        .chunks_exact(size)
        .map(|it| {
            let item_index = u16at(it, 6);
            PlaylistItem {
                position: u32at(it, 0),
                length: u32at(it, 8),
                track: u16at(it, 12), // reversed; fixed up by the caller
                source: if item_index > BASE { ClipSource::Pattern(item_index - BASE) } else { ClipSource::Channel(item_index) },
                muted: u16at(it, 18) & 0x2000 != 0,
                group: u16at(it, 14),
                start_offset: u32at(it, 24),
                end_offset: u32at(it, 28),
            }
        })
        .collect();
    (items, false)
}

fn apply_mixer_params(inserts: &mut [FlpInsert], d: &[u8], base: usize) {
    // 12-byte records: u32 reserved, u16 parameter id, u16 (insert << 6 | slot), i32 value.
    // The insert number is offset by `base`. From 24.2 on the "current" insert uses a number past the normal
    // range and maps to the last insert; before that number 256 is the global group.
    let n = inserts.len();
    let mut send_levels = false;
    for r in d.chunks_exact(12) {
        let pid = r[4];
        let cd = u16at(r, 6);
        let raw = (cd >> 6) as usize;
        let slot = cd & 63;
        let value = u32at(r, 8) as i32;
        if raw < base || (r[5] != 0x1f && r[5] != 0x20) {
            continue;
        }
        let idx = raw - base;
        let idx = if idx < n { idx } else if n > 0 && base >= 448 { n - 1 } else { continue };
        let ins = &mut inserts[idx];
        if r[5] == 0x20 {
            // later versions: the level of the route to insert `pid`
            let dest = pid as usize;
            match ins.route_levels.iter_mut().find(|r| r.0 == dest) {
                Some(r) => r.1 = value,
                None => ins.route_levels.push((dest, value)),
            }
            continue;
        }
        match pid {
            0 => {
                if let Some(s) = ins.slots.iter_mut().find(|s| s.index == slot) {
                    s.enabled = value != 0;
                }
            }
            1 => {
                if let Some(s) = ins.slots.iter_mut().find(|s| s.index == slot) {
                    s.mix = value;
                }
            }
            192 => ins.volume = value,
            193 => ins.pan = value,
            194 => ins.stereo_separation = value,
            208..=210 => ins.eq[pid as usize - 208][0] = value,
            216..=218 => ins.eq[pid as usize - 216][1] = value,
            224..=226 => ins.eq[pid as usize - 224][2] = value,
            64..=191 => {
                let dest = pid as usize - 64;
                send_levels |= (100..104).contains(&dest);
                match ins.route_levels.iter_mut().find(|r| r.0 == dest) {
                    Some(r) => r.1 = value,
                    None => ins.route_levels.push((dest, value)),
                }
            }
            _ => {}
        }
    }
    // Old projects (four send tracks, 100 to 103) wire every insert to the send tracks and write a
    // level only once its knob has been turned up: without any, those knobs are at zero.
    // Such a project saved again by a later version keeps the same wiring on a longer mixer, still without levels.
    let old_wiring = n > 105 && inserts[1..100].iter().all(|ins| (100..104).all(|d| ins.routes.get(d).copied().unwrap_or(false)));
    if (n == 105 || old_wiring) && !send_levels {
        for ins in inserts[..100].iter_mut() {
            for dest in 100..104 {
                if ins.routes.get(dest).copied().unwrap_or(false) {
                    ins.route_levels.push((dest, 0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(channels: u16, ppq: u16, events: Vec<Event>) -> Vec<u8> {
        FlpFile { header: Header { format: 0, channels, ppq }, events, trailer: vec![] }.to_bytes()
    }

    fn version(v: &str) -> Event {
        let mut d = v.as_bytes().to_vec();
        d.push(0);
        Event::data(id::VERSION, d)
    }

    #[test]
    fn early_channel_names_are_read() {
        let bytes = file(
            2,
            96,
            vec![
                version("11.5.0"),
                Event::word(id::NEW_CHANNEL, 0),
                Event::byte(id::CHAN_TYPE, 2),
                Event::text(id::PLUGIN_INTERNAL_NAME, "Some Synth"),
                Event::text(id::CHAN_NAME, "Lead"),
                Event::text(id::PLUGIN_NAME, "Some Synth"),
                Event::word(id::NEW_CHANNEL, 1),
                Event::byte(id::CHAN_TYPE, 0),
                Event::text(id::CHAN_NAME, "Kick"),
            ],
        );
        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.channels[0].name, "Lead");
        assert_eq!(flp.channels[1].name, "Kick");
    }

    /// One item of the playlist as early versions wrote it: 28 bytes with the row word at 12.
    fn old_item(pos: u32, base: u16, ix: u16, len: u32, row: u16) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&pos.to_le_bytes());
        d.extend_from_slice(&base.to_le_bytes());
        d.extend_from_slice(&ix.to_le_bytes());
        d.extend_from_slice(&len.to_le_bytes());
        d.extend_from_slice(&row.to_le_bytes());
        d.extend_from_slice(&3u16.to_le_bytes());
        d.extend_from_slice(&0x8080_6440u32.to_le_bytes());
        d.extend_from_slice(&u32::MAX.to_le_bytes());
        d.extend_from_slice(&0x7fff_ffffu32.to_le_bytes());
        d
    }

    #[test]
    fn links_of_versions_nine_and_ten_name_the_clip_twice() {
        let rec = |extra: u32| {
            let mut d = vec![0u8; 20];
            d[2..4].copy_from_slice(&16u16.to_le_bytes());
            d[4..8].copy_from_slice(&extra.to_le_bytes());
            d[8..10].copy_from_slice(&param::INSERT_VOLUME.to_le_bytes());
            d[10..12].copy_from_slice(&0x2000u16.to_le_bytes());
            d
        };
        let bytes = file(1, 96, vec![version("9.0.0"), Event::data(id::LINK, rec(16 << 16)), Event::data(id::LINK, rec(0)), Event::data(id::LINK, rec(0x7961_6c50))]);
        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.links.iter().map(|l| l.from_clip()).collect::<Vec<_>>(), [true, true, false]);
        assert!(link_from_clip(&rec(16 << 16)) && link_from_clip(&rec(0)) && !link_from_clip(&rec(5 << 16)));
    }

    #[test]
    fn early_playlists_put_blocks_and_clips_on_tracks_from_the_top() {
        let mut d = old_item(0, 0, 0, 768, 30951 - 9);
        d.extend(old_item(768, 0, 0, 768, 30951 - 4));
        d.extend(old_item(1536, 0, 0, 768, 30951 - 9));
        d.extend(old_item(0, 0x5000, 6, 384, 30818 - 2));
        d.extend(old_item(96, 0x5000, 0x5000 + 3, 96, 30818));
        assert_eq!(old_playlist_size(&d, (4, 1, 2, 0)), Some(28));
        assert_eq!(old_playlist_size(&d, (12, 0, 0, 0)), None);
        let bytes = file(1, 96, vec![version("4.1.2"), Event::data(id::PLAYLIST, d)]);
        let flp = parse(&bytes).unwrap();
        let arr = &flp.arrangements[0];
        assert!(arr.old_playlist);
        let got: Vec<(u32, u16, ClipSource)> = arr.items.iter().map(|i| (i.position, i.track, i.source)).collect();
        // patterns 4 and 9 have blocks: 4 takes the top track, 9 the next
        assert_eq!(
            got,
            vec![
                (0, 1, ClipSource::Pattern(9)),
                (0, 2, ClipSource::Channel(6)),
                (96, 0, ClipSource::Pattern(3)),
                (768, 0, ClipSource::Pattern(4)),
                (1536, 1, ClipSource::Pattern(9)),
            ]
        );
    }

    #[test]
    fn early_notes_are_twenty_bytes_and_go_back_the_same() {
        assert_eq!(note_size((4, 1, 2, 0)), 20);
        assert_eq!(note_size((8, 0, 0, 0)), 20);
        assert_eq!(note_size((9, 0, 0, 0)), 24);
        let raw: Vec<u8> = vec![48, 0, 0, 0, 0x08, 0x40, 7, 0, 192, 0, 0, 0, 65, 120, 0, 0, 64, 100, 124, 130];
        let bytes = file(8, 96, vec![version("4.1.2"), Event::word(id::NEW_PATTERN, 3), Event::data(id::PATTERN_NOTES, raw.clone())]);
        let flp = parse(&bytes).unwrap();
        let n = &flp.patterns[0].notes[0];
        assert_eq!((n.position, n.rack_channel, n.length, n.key, n.velocity, n.pan, n.mod_x, n.mod_y), (48, 7, 192, 65, 100, 64, 124, 130));
        let mut out = Vec::new();
        n.write_old(&mut out);
        assert_eq!(out, raw);
    }

    #[test]
    fn early_levels_count_in_128_steps() {
        let levels = |pan: u32, vol: u32| {
            let mut d = Vec::new();
            for v in [pan, vol, 0, 256, 0, 0] {
                d.extend_from_slice(&v.to_le_bytes());
            }
            d
        };
        let old = parse(&file(1, 96, vec![version("4.1.2"), Event::word(id::NEW_CHANNEL, 0), Event::data(id::CHAN_LEVELS, levels(64, 56))])).unwrap();
        assert_eq!((old.channels[0].pan, old.channels[0].volume), (6400, 5600));
        let new = parse(&file(1, 96, vec![version("12.5.0"), Event::word(id::NEW_CHANNEL, 0), Event::data(id::CHAN_LEVELS, levels(6400, 5600))])).unwrap();
        assert_eq!((new.channels[0].pan, new.channels[0].volume), (6400, 5600));
    }

    #[test]
    fn step_dots_become_notes() {
        let bytes = file(
            2,
            96,
            vec![
                version("4.1.2"),
                Event::word(id::NEW_CHANNEL, 0),
                Event::word(id::NEW_CHANNEL, 1),
                Event::word(id::NEW_PATTERN, 2),
                Event::word(id::DOT_VOL, 0x6000),
                Event::word(id::DOT, 0x0100),
                Event::word(id::DOT, 0x0104),
                Event::word(id::DOT, 0x0008),
            ],
        );
        let flp = parse(&bytes).unwrap();
        assert!(flp.has_dots);
        let p = flp.patterns.iter().find(|p| p.index == 2).unwrap();
        let got: Vec<(u32, u16, u8)> = p.notes.iter().map(|n| (n.position, n.rack_channel, n.velocity)).collect();
        // steps 0 and 4 are lit (step 8 is stored but off); the first carries its own level
        assert_eq!(got, vec![(0, 1, 0x60), (96, 1, 100)]);
    }

    #[test]
    fn reads_header_version_title_and_tempo() {
        let bytes = file(45, 96, vec![version("25.2.3.5164"), Event::text(id::TITLE, "My Song"), Event::int(id::TEMPO, 140_000)]);
        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.header.channels, 45);
        assert_eq!(flp.header.ppq, 96);
        assert_eq!(flp.version, "25.2.3.5164");
        assert_eq!(flp.title, "My Song");
        assert_eq!(flp.tempo, 140.0);
    }

    #[test]
    fn old_files_use_single_byte_text() {
        let bytes = file(1, 96, vec![version("10.9.0"), Event::data(id::TITLE, b"Old Song\0".to_vec())]);
        assert_eq!(parse(&bytes).unwrap().title, "Old Song");
    }

    #[test]
    fn three_byte_event_in_new_files_keeps_the_stream_in_step() {
        let mut bytes = file(1, 96, vec![version("25.2.5.5319")]);
        // append: id 172 + three bytes, then a text event, then tempo
        let mut extra = vec![172u8, 1, 1, 0];
        let t = Event::text(192, "FL");
        extra.push(192);
        extra.push(t.bytes().unwrap().len() as u8);
        extra.extend_from_slice(t.bytes().unwrap());
        extra.push(id::TEMPO);
        extra.extend_from_slice(&128_500u32.to_le_bytes());
        let body_len = u32::from_le_bytes(bytes[18..22].try_into().unwrap()) + extra.len() as u32;
        bytes[18..22].copy_from_slice(&body_len.to_le_bytes());
        bytes.extend(extra);
        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.tempo, 128.5);
        assert_eq!(flp.file.to_bytes(), bytes, "round trip is byte exact");
    }

    #[test]
    fn channels_patterns_and_notes() {
        let mut notes = vec![];
        Note { position: 0, rack_channel: 1, length: 24, key: 60, velocity: 100, pan: 64, fine_pitch: 120, ..Default::default() }.write(&mut notes);
        Note { position: 48, rack_channel: 1, length: 24, key: 62, velocity: 90, pan: 64, fine_pitch: 120, ..Default::default() }.write(&mut notes);
        let mut levels = vec![0u8; 24];
        levels[0..4].copy_from_slice(&3200u32.to_le_bytes());
        levels[4..8].copy_from_slice(&12800u32.to_le_bytes());
        let bytes = file(
            2,
            96,
            vec![
                version("21.0.0.3302"),
                Event::word(id::NEW_PATTERN, 1),
                Event::data(id::PATTERN_NOTES, notes),
                Event::word(id::NEW_CHANNEL, 0),
                Event::byte(id::CHAN_TYPE, 0),
                Event::text(id::PLUGIN_NAME, "Kick"),
                Event::text(id::SAMPLE_PATH, "%FLStudioFactoryData%/Data/Patches/Packs/Drums/Kicks/Kick.wav"),
                Event::byte(id::CHAN_TARGET_INSERT, 3),
                Event::word(id::NEW_CHANNEL, 1),
                Event::byte(id::CHAN_TYPE, 2),
                Event::text(id::PLUGIN_INTERNAL_NAME, "Some Synth"),
                Event::text(id::PLUGIN_NAME, "Lead"),
                Event::data(id::CHAN_LEVELS, levels),
                Event::word(id::NEW_PATTERN, 1),
                Event::text(id::PATTERN_NAME, "Intro"),
            ],
        );
        let flp = parse(&bytes).unwrap();
        assert_eq!(flp.channels.len(), 2);
        assert_eq!(flp.channels[0].name, "Kick");
        assert_eq!(flp.channels[0].kind, ChannelKind::Sampler);
        assert_eq!(flp.channels[0].insert, 3);
        assert!(flp.channels[0].sample_path.ends_with("Kick.wav"));
        assert_eq!(flp.channels[1].kind, ChannelKind::Plugin);
        assert_eq!(flp.channels[1].plugin, "Some Synth");
        assert_eq!((flp.channels[1].pan, flp.channels[1].volume), (3200, 12800));
        assert_eq!(flp.patterns.len(), 1);
        assert_eq!(flp.patterns[0].name, "Intro");
        assert_eq!(flp.patterns[0].notes.len(), 2);
        assert_eq!(flp.patterns[0].notes[1].key, 62);
        assert_eq!(flp.patterns[0].notes[1].velocity, 90);
        assert_eq!(flp.patterns[0].notes[1].position, 48);
    }

    #[test]
    fn playlist_items_of_every_size_and_track_flip() {
        for size in [32usize, 60, 80] {
            let mut d = vec![];
            for (pos, idx, rv) in [(0u32, 0x5001u16, 499u16), (384, 3, 497)] {
                let mut it = vec![0u8; size];
                it[0..4].copy_from_slice(&pos.to_le_bytes());
                it[4..6].copy_from_slice(&0x5000u16.to_le_bytes());
                it[6..8].copy_from_slice(&idx.to_le_bytes());
                it[8..12].copy_from_slice(&384u32.to_le_bytes());
                it[12..14].copy_from_slice(&rv.to_le_bytes());
                d.extend(it);
            }
            let bytes = file(1, 96, vec![version("24.2.99.4720"), Event::word(id::NEW_ARRANGEMENT, 0), Event::data(id::PLAYLIST, d)]);
            let flp = parse(&bytes).unwrap();
            let items = &flp.arrangements[0].items;
            assert_eq!(items.len(), 2, "size {size}");
            assert_eq!(items[0].source, ClipSource::Pattern(1));
            assert_eq!(items[0].track, 0);
            assert_eq!(items[1].source, ClipSource::Channel(3));
            assert_eq!(items[1].track, 2);
        }
    }

    #[test]
    fn rejects_non_flp() {
        assert!(parse(b"not a flp file at all").is_err());
        assert!(parse(b"").is_err());
    }

    /// Set CEREAL_FLP_CORPUS to a folder of real project files to check that every one of them parses and
    /// writes back byte for byte.
    #[test]
    fn corpus_round_trips_byte_exact() {
        let Some(root) = std::env::var_os("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::PathBuf::from(root)];
        let mut count = 0;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().map(|e| e.eq_ignore_ascii_case("flp")).unwrap_or(false) {
                    let bytes = std::fs::read(&p).unwrap();
                    let f = FlpFile::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                    assert!(f.to_bytes() == bytes, "{} does not round trip", p.display());
                    let flp = Flp::from_file(f);
                    assert!(flp.tempo > 0.0, "{} has no tempo", p.display());
                    assert_eq!(flp.channels.len(), flp.header.channels as usize, "{} channel count", p.display());
                    count += 1;
                }
            }
        }
        assert!(count > 0, "no .flp files found in the corpus folder");
        eprintln!("round-tripped {count} project files");
    }
}
