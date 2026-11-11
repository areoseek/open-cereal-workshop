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
    pub const TIMESIG_BEAT: u8 = 18;
    pub const CHAN_ZIPPED: u8 = 20;
    pub const CHAN_TYPE: u8 = 21;
    pub const CHAN_TARGET_INSERT: u8 = 22;
    pub const PAN_LAW: u8 = 23;
    // word events
    pub const NEW_CHANNEL: u8 = 64;
    pub const NEW_PATTERN: u8 = 65;
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
    pub const MIXER_PARAMS: u8 = 225;
    pub const PLAYLIST: u8 = 233;
    pub const CHAN_AUTOMATION: u8 = 234;
    pub const LINK: u8 = 227;
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
}

#[derive(Debug, Clone, Default)]
pub struct FlpChannel {
    pub index: u16,
    pub kind: ChannelKind,
    /// Internal generator name (empty for the plain sampler).
    pub plugin: String,
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
}

#[derive(Debug, Clone, Default)]
pub struct Marker {
    pub position: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct FlpArrangement {
    pub index: u16,
    pub name: String,
    pub items: Vec<PlaylistItem>,
    pub tracks: Vec<FlpTrack>,
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone, Default)]
pub struct FlpSlot {
    pub index: u16,
    pub plugin: String,
    pub name: String,
    pub enabled: bool,
    /// 0..=12800.
    pub mix: i32,
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
    /// routes[i] is true when this insert sends to insert i.
    pub routes: Vec<bool>,
    pub slots: Vec<FlpSlot>,
    pub input: i32,
    pub output: i32,
}

impl Default for FlpInsert {
    fn default() -> Self {
        Self { index: 0, name: String::new(), color: None, flags: 0, volume: 12800, pan: 0, stereo_separation: 0, routes: vec![], slots: vec![], input: -1, output: -1 }
    }
}

pub const INSERT_FLAG_ENABLED: u32 = 1 << 3;
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
    pub main_pitch: i16,
    pub shuffle: u8,
    pub current_pattern: u16,
    pub channels: Vec<FlpChannel>,
    pub patterns: Vec<FlpPattern>,
    pub arrangements: Vec<FlpArrangement>,
    pub inserts: Vec<FlpInsert>,
    /// Automation clips and other controllers tied to the controls they drive.
    pub links: Vec<FlpLink>,
    pub file: FlpFile,
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
        let mut mixer_params: Vec<u8> = vec![];
        // an insert's name and colour are written just before the event that opens it
        let mut next_insert_name: Option<String> = None;
        let mut next_insert_color: Option<u32> = None;
        let mut early_markers: Vec<Marker> = vec![];

        for e in &file.events {
            let text = |d: &[u8]| if unicode { utf16(d) } else { latin1(d) };
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
                (id::TEMPO_WORD, Value::Word(v)) if flp.tempo == 0.0 => flp.tempo = *v as f32,
                (id::TIMESIG_NUM, Value::Byte(v)) => flp.time_sig.0 = *v,
                (id::TIMESIG_BEAT, Value::Byte(v)) => flp.time_sig.1 = *v,
                (id::MAIN_PITCH, Value::Word(v)) => flp.main_pitch = *v as i16,
                (id::SHUFFLE, Value::Byte(v)) => flp.shuffle = *v,
                (id::CURRENT_PATTERN, Value::Word(v)) => flp.current_pattern = *v,

                // ---- patterns
                (id::NEW_PATTERN, Value::Word(v)) => {
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
                    p.notes.extend(d.chunks_exact(NOTE_SIZE).map(Note::read));
                }

                // ---- channels
                (id::NEW_CHANNEL, Value::Word(v)) => {
                    scope = Scope::Channel;
                    flp.channels.push(FlpChannel { index: *v, enabled: true, insert: -1, pan: 6400, volume: 10000, root_key: 60, ..Default::default() });
                }
                (id::NEW_ARRANGEMENT, Value::Word(v)) => {
                    scope = Scope::Arrangement;
                    flp.arrangements.push(FlpArrangement { index: *v, ..Default::default() });
                }
                (id::PLAYLIST, Value::Data(d)) => {
                    if flp.arrangements.is_empty() {
                        // files from before arrangements existed have one unnamed playlist
                        flp.arrangements.push(FlpArrangement { name: "Arrangement".into(), ..Default::default() });
                    }
                    let a = flp.arrangements.last_mut().unwrap();
                    a.items = parse_playlist(d);
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
                    a.tracks.push(FlpTrack { index: u32at(d, 0), name: String::new(), color: u32at(d, 4), enabled: d.get(12).copied().unwrap_or(1) != 0, height: f32at(d, 13) });
                }
                (id::TRACK_NAME, Value::Data(d)) => {
                    if let Some(t) = flp.arrangements.last_mut().and_then(|a| a.tracks.last_mut()) {
                        t.name = text(d);
                    }
                }
                (id::MARKER_POSITION, Value::Int(v)) => {
                    // the top byte carries the marker kind; markers written ahead of the first arrangement wait for it
                    let m = Marker { position: *v & 0x00ff_ffff, name: String::new() };
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
                    let index = flp.inserts.len();
                    flp.inserts.push(FlpInsert { index, flags: u32at(d, 4), name: next_insert_name.take().unwrap_or_default(), color: next_insert_color.take(), ..Default::default() });
                }
                (id::INSERT_OUTPUT, Value::Int(v)) => {
                    if let (true, Some(i)) = (insert_open, flp.inserts.last_mut()) {
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
                    }
                }
                (id::MIXER_PARAMS, Value::Data(d)) => mixer_params = d.clone(),

                // ---- ids shared between channels and effect slots
                (id::PLUGIN_INTERNAL_NAME, Value::Data(d)) => match scope {
                    Scope::Channel => {
                        if let Some(c) = flp.channels.last_mut() {
                            c.plugin = text(d);
                        }
                    }
                    Scope::Insert => slot.plugin = text(d),
                    _ => {}
                },
                (id::PLUGIN_NAME, Value::Data(d)) => match scope {
                    Scope::Channel => {
                        if let Some(c) = flp.channels.last_mut() {
                            c.name = text(d);
                        }
                    }
                    Scope::Insert => slot.name = text(d),
                    _ => {}
                },
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
                            c.pitch_cents = u32at(d, 8) as i32;
                        }
                        (id::CHAN_PARAMS, Value::Data(d)) if d.len() >= 20 => c.root_key = u32at(d, 16),
                        (id::CHAN_ENVELOPE, Value::Data(d)) if d.len() >= 36 => {
                            let i = |o: usize| u32at(d, o) as i32;
                            c.envelopes.push(Envelope { enabled: i(4) != 0, predelay: i(8), attack: i(12), hold: i(16), decay: i(20), sustain: i(24), release: i(28), amount: i(32) });
                        }
                        (id::CHAN_AUTOMATION, Value::Data(d)) => c.automation = d.clone(),
                        (id::LAYER_CHILD, Value::Word(k)) => c.children.push(*k),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // patterns: the highest numbered pattern that has anything in it bounds the list
        flp.patterns = patterns.into_values().collect();

        // track numbering in playlist items is stored upside down; the track count tells us the top
        let ver = version_tuple(&flp.version);
        for a in &mut flp.arrangements {
            let n = if !a.tracks.is_empty() { a.tracks.len() as u16 } else if ver >= (12, 9, 0, 0) { 500 } else { 199 };
            for it in &mut a.items {
                it.track = (n - 1).saturating_sub(it.track);
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
        apply_mixer_params(&mut flp.inserts, &mixer_params, mixer_base(ver));
        flp.links = file
            .events
            .iter()
            .filter(|e| e.id == id::LINK)
            .filter_map(|e| match &e.value {
                Value::Data(d) if d.len() >= 12 => Some(FlpLink { kind: u16at(d, 0), source: u16at(d, 2), extra: u32at(d, 4), param: u16at(d, 8), dest: u16at(d, 10) }),
                _ => None,
            })
            .collect();
        flp.file = file;
        flp
    }
}

fn parse_playlist(d: &[u8]) -> Vec<PlaylistItem> {
    const BASE: u16 = 0x5000;
    // The item grew over the years (32, then 60, then 80 bytes, maybe more later). Every item carries the
    // constant 0x5000 at offset 4, so pick the size for which that holds for every item.
    let size = [32usize, 60, 80, 88, 96, 104, 112, 120, 128]
        .into_iter()
        .find(|&s| !d.is_empty() && d.len() % s == 0 && d.chunks_exact(s).all(|it| u16at(it, 4) == BASE));
    let Some(size) = size else { return vec![] };
    d.chunks_exact(size)
        .map(|it| {
            let item_index = u16at(it, 6);
            PlaylistItem {
                position: u32at(it, 0),
                length: u32at(it, 8),
                track: u16at(it, 12), // reversed; fixed up by the caller
                source: if item_index > BASE { ClipSource::Pattern(item_index - BASE) } else { ClipSource::Channel(item_index) },
                muted: u16at(it, 18) & 0x2000 != 0,
                start_offset: u32at(it, 24),
                end_offset: u32at(it, 28),
            }
        })
        .collect()
}

fn apply_mixer_params(inserts: &mut [FlpInsert], d: &[u8], base: usize) {
    // 12-byte records: u32 reserved, u16 parameter id, u16 (insert << 6 | slot), i32 value.
    // The insert number is offset by `base`. From 24.2 on the "current" insert uses a number past the normal
    // range and maps to the last insert; before that number 256 is the global group.
    let n = inserts.len();
    for r in d.chunks_exact(12) {
        let pid = r[4];
        let cd = u16at(r, 6);
        let raw = (cd >> 6) as usize;
        let slot = cd & 63;
        let value = u32at(r, 8) as i32;
        if raw < base || r[5] != 0x1f {
            continue;
        }
        let idx = raw - base;
        let idx = if idx < n { idx } else if n > 0 && base >= 448 { n - 1 } else { continue };
        let ins = &mut inserts[idx];
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
            _ => {}
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
