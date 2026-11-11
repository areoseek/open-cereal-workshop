//! The layouts of the VST3 interfaces, written out from their public description: tables of functions in
//! a fixed order, each starting with the three of the common base (ask for another interface, keep, let
//! go), and the plain structures passed through them. Shared by the host and the test plug-in.

use std::ffi::{c_char, c_void};

pub type Tuid = [u8; 16];
pub type Obj = *mut c_void;

/// An interface's identifier as it lies in memory: plain on most systems, the first half turned
/// around on Windows.
pub const fn iid(a: u32, b: u32, c: u32, d: u32) -> Tuid {
    let (a, b, c, d) = (a.to_be_bytes(), b.to_be_bytes(), c.to_be_bytes(), d.to_be_bytes());
    if cfg!(windows) {
        [a[3], a[2], a[1], a[0], b[1], b[0], b[3], b[2], c[0], c[1], c[2], c[3], d[0], d[1], d[2], d[3]]
    } else {
        [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], c[0], c[1], c[2], c[3], d[0], d[1], d[2], d[3]]
    }
}

/// A class identifier between its in-memory form and the one written down (the same on every system).
pub fn canon(t: Tuid) -> Tuid {
    if cfg!(windows) {
        [t[3], t[2], t[1], t[0], t[5], t[4], t[7], t[6], t[8], t[9], t[10], t[11], t[12], t[13], t[14], t[15]]
    } else {
        t
    }
}

pub fn id_text(t: Tuid) -> String {
    canon(t).iter().map(|b| format!("{b:02X}")).collect()
}

pub fn id_of(text: &str) -> Option<Tuid> {
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let mut t = [0u8; 16];
    for (i, b) in t.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(canon(t))
}

pub const FUNKNOWN: Tuid = iid(0x00000000, 0x00000000, 0xC0000000, 0x00000046);
pub const IFACTORY: Tuid = iid(0x7A4D811C, 0x52114A1F, 0xAED9D2EE, 0x0B43BF9F);
pub const IFACTORY2: Tuid = iid(0x0007B650, 0xF24B4C0B, 0xA464EDB9, 0xF00B2ABB);
pub const ICOMPONENT: Tuid = iid(0xE831FF31, 0xF2D54301, 0x928EBBEE, 0x25697802);
pub const IPROCESSOR: Tuid = iid(0x42043F99, 0xB7DA453C, 0xA569E79D, 0x9AAEC33D);
pub const ICONTROLLER: Tuid = iid(0xDCD7BBE3, 0x7742448D, 0xA874AACC, 0x979C759E);
pub const ICONNECTION: Tuid = iid(0x70A4156F, 0x6E6E4026, 0x989148BF, 0xAA60D8D1);
pub const IMIDIMAP: Tuid = iid(0xDF0FF9F7, 0x49B74669, 0xB63AB732, 0x7ADBF5E5);
pub const IHOSTAPP: Tuid = iid(0x58E595CC, 0xDB2D4969, 0x8B6AAF8C, 0x36A664E5);
pub const IHANDLER: Tuid = iid(0x93A0BEA3, 0x0BD045DB, 0x8E890B0C, 0xC1E46AC6);
pub const ISTREAM: Tuid = iid(0xC3BF6EA2, 0x30994752, 0x9B6BF990, 0x1EE33E9B);
pub const IEVENTS: Tuid = iid(0x3A2C4214, 0x346349FE, 0xB2C4F397, 0xB9695A44);
pub const ICHANGES: Tuid = iid(0xA4779663, 0x0BB64A56, 0xB44384A8, 0x466FEB9D);
pub const IQUEUE: Tuid = iid(0x01263A18, 0xED074F6F, 0x98C9D356, 0x4686F9BA);
pub const IMESSAGE: Tuid = iid(0x936F033B, 0xC6C047DB, 0xBB0882F8, 0x13C1E613);
pub const IATTRS: Tuid = iid(0x1E5F0AEB, 0xCC7F4533, 0xA2544011, 0x38AD5EE4);

pub const OK: i32 = 0;
pub const FALSE: i32 = 1;
#[cfg(windows)]
pub const NO_INTERFACE: i32 = 0x80004002u32 as i32;
#[cfg(not(windows))]
pub const NO_INTERFACE: i32 = -1;
#[cfg(windows)]
pub const BAD_ARG: i32 = 0x80070057u32 as i32;
#[cfg(not(windows))]
pub const BAD_ARG: i32 = 2;

pub const AUDIO: i32 = 0;
pub const EVENT: i32 = 1;
pub const IN: i32 = 0;
pub const OUT: i32 = 1;

#[repr(C)]
pub struct Unknown {
    pub query: unsafe extern "system" fn(Obj, *const Tuid, *mut Obj) -> i32,
    pub keep: unsafe extern "system" fn(Obj) -> u32,
    pub drop: unsafe extern "system" fn(Obj) -> u32,
}

#[repr(C)]
pub struct FactoryVt {
    pub base: Unknown,
    pub info: unsafe extern "system" fn(Obj, *mut FactoryInfo) -> i32,
    pub count: unsafe extern "system" fn(Obj) -> i32,
    pub class: unsafe extern "system" fn(Obj, i32, *mut ClassInfo) -> i32,
    pub create: unsafe extern "system" fn(Obj, *const Tuid, *const Tuid, *mut Obj) -> i32,
    // the second edition adds:
    pub class2: unsafe extern "system" fn(Obj, i32, *mut ClassInfo2) -> i32,
}

#[repr(C)]
pub struct FactoryInfo {
    pub vendor: [c_char; 64],
    pub url: [c_char; 256],
    pub email: [c_char; 128],
    pub flags: i32,
}

#[repr(C)]
pub struct ClassInfo {
    pub cid: Tuid,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [c_char; 64],
}

#[repr(C)]
pub struct ClassInfo2 {
    pub cid: Tuid,
    pub cardinality: i32,
    pub category: [c_char; 32],
    pub name: [c_char; 64],
    pub flags: u32,
    pub sub: [c_char; 128],
    pub vendor: [c_char; 64],
    pub version: [c_char; 64],
    pub sdk: [c_char; 64],
}

#[repr(C)]
pub struct ComponentVt {
    pub base: Unknown,
    pub initialize: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub terminate: unsafe extern "system" fn(Obj) -> i32,
    pub controller_class: unsafe extern "system" fn(Obj, *mut Tuid) -> i32,
    pub set_io_mode: unsafe extern "system" fn(Obj, i32) -> i32,
    pub bus_count: unsafe extern "system" fn(Obj, i32, i32) -> i32,
    pub bus_info: unsafe extern "system" fn(Obj, i32, i32, i32, *mut BusInfo) -> i32,
    pub routing: unsafe extern "system" fn(Obj, Obj, Obj) -> i32,
    pub activate_bus: unsafe extern "system" fn(Obj, i32, i32, i32, u8) -> i32,
    pub set_active: unsafe extern "system" fn(Obj, u8) -> i32,
    pub set_state: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub get_state: unsafe extern "system" fn(Obj, Obj) -> i32,
}

#[repr(C)]
pub struct BusInfo {
    pub media: i32,
    pub dir: i32,
    pub channels: i32,
    pub name: [u16; 128],
    pub kind: i32,
    pub flags: u32,
}

#[repr(C)]
pub struct ProcessorVt {
    pub base: Unknown,
    pub set_arrangements: unsafe extern "system" fn(Obj, *mut u64, i32, *mut u64, i32) -> i32,
    pub get_arrangement: unsafe extern "system" fn(Obj, i32, i32, *mut u64) -> i32,
    pub can_sample_size: unsafe extern "system" fn(Obj, i32) -> i32,
    pub latency: unsafe extern "system" fn(Obj) -> u32,
    pub setup: unsafe extern "system" fn(Obj, *mut Setup) -> i32,
    pub set_processing: unsafe extern "system" fn(Obj, u8) -> i32,
    pub process: unsafe extern "system" fn(Obj, *mut ProcessData) -> i32,
    pub tail: unsafe extern "system" fn(Obj) -> u32,
}

#[repr(C)]
pub struct Setup {
    pub mode: i32,
    pub sample_size: i32,
    pub max_frames: i32,
    pub rate: f64,
}

#[repr(C)]
pub struct BusBuffers {
    pub channels: i32,
    pub silence: u64,
    pub data: *mut *mut f32,
}

#[repr(C)]
pub struct ProcessData {
    pub mode: i32,
    pub sample_size: i32,
    pub frames: i32,
    pub n_in: i32,
    pub n_out: i32,
    pub ins: *mut BusBuffers,
    pub outs: *mut BusBuffers,
    pub in_changes: Obj,
    pub out_changes: Obj,
    pub in_events: Obj,
    pub out_events: Obj,
    pub context: *mut Context,
}

#[repr(C)]
#[derive(Default)]
pub struct Context {
    pub state: u32,
    pub rate: f64,
    pub project_samples: i64,
    pub system_time: i64,
    pub steady_samples: i64,
    pub beats: f64,
    pub bar_start: f64,
    pub cycle_start: f64,
    pub cycle_end: f64,
    pub tempo: f64,
    pub sig_top: i32,
    pub sig_bottom: i32,
    pub chord: [u8; 4],
    pub smpte_sub: i32,
    pub frame_rate: [u32; 2],
    pub to_clock: i32,
}

pub const CTX_PLAYING: u32 = 1 << 1;
pub const CTX_BEATS: u32 = 1 << 9;
pub const CTX_TEMPO: u32 = 1 << 10;
pub const CTX_BAR: u32 = 1 << 11;
pub const CTX_SIG: u32 = 1 << 13;
pub const CTX_STEADY: u32 = 1 << 17;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Event {
    pub bus: i32,
    pub offset: i32,
    pub beats: f64,
    pub flags: u16,
    pub kind: u16,
    pub body: EventBody,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union EventBody {
    pub on: NoteOn,
    pub off: NoteOff,
    pub raw: [u64; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NoteOn {
    pub channel: i16,
    pub pitch: i16,
    pub tuning: f32,
    pub velocity: f32,
    pub length: i32,
    pub note_id: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NoteOff {
    pub channel: i16,
    pub pitch: i16,
    pub velocity: f32,
    pub note_id: i32,
    pub tuning: f32,
}

#[repr(C)]
pub struct ControllerVt {
    pub base: Unknown,
    pub initialize: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub terminate: unsafe extern "system" fn(Obj) -> i32,
    pub set_component_state: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub set_state: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub get_state: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub param_count: unsafe extern "system" fn(Obj) -> i32,
    pub param_info: unsafe extern "system" fn(Obj, i32, *mut ParamInfo) -> i32,
    pub text_of: unsafe extern "system" fn(Obj, u32, f64, *mut u16) -> i32,
    pub value_of: unsafe extern "system" fn(Obj, u32, *mut u16, *mut f64) -> i32,
    pub to_plain: unsafe extern "system" fn(Obj, u32, f64) -> f64,
    pub to_normal: unsafe extern "system" fn(Obj, u32, f64) -> f64,
    pub get: unsafe extern "system" fn(Obj, u32) -> f64,
    pub set: unsafe extern "system" fn(Obj, u32, f64) -> i32,
    pub set_handler: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub create_view: unsafe extern "system" fn(Obj, *const c_char) -> Obj,
}

#[repr(C)]
pub struct ParamInfo {
    pub id: u32,
    pub title: [u16; 128],
    pub short: [u16; 128],
    pub units: [u16; 128],
    pub steps: i32,
    pub default: f64,
    pub unit: i32,
    pub flags: i32,
}

#[repr(C)]
pub struct ConnectionVt {
    pub base: Unknown,
    pub connect: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub disconnect: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub notify: unsafe extern "system" fn(Obj, Obj) -> i32,
}

#[repr(C)]
pub struct MidiMapVt {
    pub base: Unknown,
    pub assignment: unsafe extern "system" fn(Obj, i32, i16, i16, *mut u32) -> i32,
}

#[repr(C)]
pub struct HostVt {
    pub base: Unknown,
    pub name: unsafe extern "system" fn(Obj, *mut u16) -> i32,
    pub create: unsafe extern "system" fn(Obj, *const Tuid, *const Tuid, *mut Obj) -> i32,
}

#[repr(C)]
pub struct HandlerVt {
    pub base: Unknown,
    pub begin: unsafe extern "system" fn(Obj, u32) -> i32,
    pub perform: unsafe extern "system" fn(Obj, u32, f64) -> i32,
    pub end: unsafe extern "system" fn(Obj, u32) -> i32,
    pub restart: unsafe extern "system" fn(Obj, i32) -> i32,
}

#[repr(C)]
pub struct StreamVt {
    pub base: Unknown,
    pub read: unsafe extern "system" fn(Obj, *mut c_void, i32, *mut i32) -> i32,
    pub write: unsafe extern "system" fn(Obj, *mut c_void, i32, *mut i32) -> i32,
    pub seek: unsafe extern "system" fn(Obj, i64, i32, *mut i64) -> i32,
    pub tell: unsafe extern "system" fn(Obj, *mut i64) -> i32,
}

#[repr(C)]
pub struct EventsVt {
    pub base: Unknown,
    pub count: unsafe extern "system" fn(Obj) -> i32,
    pub get: unsafe extern "system" fn(Obj, i32, *mut Event) -> i32,
    pub add: unsafe extern "system" fn(Obj, *mut Event) -> i32,
}

#[repr(C)]
pub struct QueueVt {
    pub base: Unknown,
    pub id: unsafe extern "system" fn(Obj) -> u32,
    pub count: unsafe extern "system" fn(Obj) -> i32,
    pub get: unsafe extern "system" fn(Obj, i32, *mut i32, *mut f64) -> i32,
    pub add: unsafe extern "system" fn(Obj, i32, f64, *mut i32) -> i32,
}

#[repr(C)]
pub struct ChangesVt {
    pub base: Unknown,
    pub count: unsafe extern "system" fn(Obj) -> i32,
    pub get: unsafe extern "system" fn(Obj, i32) -> Obj,
    pub add: unsafe extern "system" fn(Obj, *const u32, *mut i32) -> Obj,
}

#[repr(C)]
pub struct AttrsVt {
    pub base: Unknown,
    pub set_int: unsafe extern "system" fn(Obj, *const c_char, i64) -> i32,
    pub get_int: unsafe extern "system" fn(Obj, *const c_char, *mut i64) -> i32,
    pub set_float: unsafe extern "system" fn(Obj, *const c_char, f64) -> i32,
    pub get_float: unsafe extern "system" fn(Obj, *const c_char, *mut f64) -> i32,
    pub set_text: unsafe extern "system" fn(Obj, *const c_char, *const u16) -> i32,
    pub get_text: unsafe extern "system" fn(Obj, *const c_char, *mut u16, u32) -> i32,
    pub set_bytes: unsafe extern "system" fn(Obj, *const c_char, *const c_void, u32) -> i32,
    pub get_bytes: unsafe extern "system" fn(Obj, *const c_char, *mut *const c_void, *mut u32) -> i32,
}

#[repr(C)]
pub struct MessageVt {
    pub base: Unknown,
    pub id: unsafe extern "system" fn(Obj) -> *const c_char,
    pub set_id: unsafe extern "system" fn(Obj, *const c_char),
    pub attrs: unsafe extern "system" fn(Obj) -> Obj,
}

