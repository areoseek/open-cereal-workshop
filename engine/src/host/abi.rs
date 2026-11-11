//! The C layout of the open CLAP plug-in standard (version 1.2), written out from its public description.
//! Only what the host and the test plug-in use.

use std::ffi::{c_char, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
}

pub const VERSION: Version = Version { major: 1, minor: 2, revision: 0 };

#[repr(C)]
pub struct Entry {
    pub version: Version,
    pub init: Option<unsafe extern "C" fn(path: *const c_char) -> bool>,
    pub deinit: Option<unsafe extern "C" fn()>,
    pub get_factory: Option<unsafe extern "C" fn(id: *const c_char) -> *const c_void>,
}

// SAFETY: a table of functions and numbers, never written after it is made.
unsafe impl Sync for Entry {}

pub const FACTORY_ID: &[u8] = b"clap.plugin-factory\0";

#[repr(C)]
pub struct Factory {
    pub count: Option<unsafe extern "C" fn(f: *const Factory) -> u32>,
    pub descriptor: Option<unsafe extern "C" fn(f: *const Factory, index: u32) -> *const Descriptor>,
    pub create: Option<unsafe extern "C" fn(f: *const Factory, host: *const Host, id: *const c_char) -> *const Plugin>,
}

unsafe impl Sync for Factory {}

#[repr(C)]
pub struct Descriptor {
    pub version: Version,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub manual_url: *const c_char,
    pub support_url: *const c_char,
    pub plugin_version: *const c_char,
    pub description: *const c_char,
    /// Ends with a null pointer.
    pub features: *const *const c_char,
}

unsafe impl Sync for Descriptor {}

#[repr(C)]
pub struct Host {
    pub version: Version,
    pub data: *mut c_void,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub host_version: *const c_char,
    pub get_extension: Option<unsafe extern "C" fn(h: *const Host, id: *const c_char) -> *const c_void>,
    pub request_restart: Option<unsafe extern "C" fn(h: *const Host)>,
    pub request_process: Option<unsafe extern "C" fn(h: *const Host)>,
    pub request_callback: Option<unsafe extern "C" fn(h: *const Host)>,
}

#[repr(C)]
pub struct Plugin {
    pub desc: *const Descriptor,
    pub data: *mut c_void,
    pub init: Option<unsafe extern "C" fn(p: *const Plugin) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(p: *const Plugin)>,
    pub activate: Option<unsafe extern "C" fn(p: *const Plugin, rate: f64, min_frames: u32, max_frames: u32) -> bool>,
    pub deactivate: Option<unsafe extern "C" fn(p: *const Plugin)>,
    pub start_processing: Option<unsafe extern "C" fn(p: *const Plugin) -> bool>,
    pub stop_processing: Option<unsafe extern "C" fn(p: *const Plugin)>,
    pub reset: Option<unsafe extern "C" fn(p: *const Plugin)>,
    pub process: Option<unsafe extern "C" fn(p: *const Plugin, process: *const Process) -> i32>,
    pub get_extension: Option<unsafe extern "C" fn(p: *const Plugin, id: *const c_char) -> *const c_void>,
    pub on_main_thread: Option<unsafe extern "C" fn(p: *const Plugin)>,
}

#[repr(C)]
pub struct Process {
    pub steady_time: i64,
    pub frames: u32,
    pub transport: *const EventTransport,
    pub audio_in: *const AudioBuffer,
    pub audio_out: *mut AudioBuffer,
    pub audio_in_count: u32,
    pub audio_out_count: u32,
    pub in_events: *const InputEvents,
    pub out_events: *const OutputEvents,
}

#[repr(C)]
pub struct AudioBuffer {
    pub data32: *mut *mut f32,
    pub data64: *mut *mut f64,
    pub channels: u32,
    pub latency: u32,
    pub constant_mask: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventHeader {
    pub size: u32,
    pub time: u32,
    pub space: u16,
    pub kind: u16,
    pub flags: u32,
}

pub const EV_NOTE_ON: u16 = 0;
pub const EV_NOTE_OFF: u16 = 1;
pub const EV_PARAM_VALUE: u16 = 5;
pub const EV_MIDI: u16 = 10;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventNote {
    pub header: EventHeader,
    pub note_id: i32,
    pub port: i16,
    pub channel: i16,
    pub key: i16,
    pub velocity: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventParam {
    pub header: EventHeader,
    pub param: u32,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventMidi {
    pub header: EventHeader,
    pub port: u16,
    pub data: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventTransport {
    pub header: EventHeader,
    pub flags: u32,
    /// Beats and seconds are whole numbers with 31 bits after the point.
    pub song_pos_beats: i64,
    pub song_pos_seconds: i64,
    pub tempo: f64,
    pub tempo_inc: f64,
    pub loop_start_beats: i64,
    pub loop_end_beats: i64,
    pub loop_start_seconds: i64,
    pub loop_end_seconds: i64,
    pub bar_start: i64,
    pub bar_number: i32,
    pub tsig_num: u16,
    pub tsig_denom: u16,
}

pub const TRANSPORT_HAS_TEMPO: u32 = 1;
pub const TRANSPORT_HAS_BEATS: u32 = 2;
pub const TRANSPORT_HAS_SIGNATURE: u32 = 8;
pub const TRANSPORT_PLAYING: u32 = 16;
pub const BEAT_ONE: f64 = (1u64 << 31) as f64;

/// Any event the host hands over, in one size.
#[repr(C)]
#[derive(Clone, Copy)]
pub union AnyEvent {
    pub header: EventHeader,
    pub note: EventNote,
    pub param: EventParam,
    pub midi: EventMidi,
}

#[repr(C)]
pub struct InputEvents {
    pub ctx: *mut c_void,
    pub size: Option<unsafe extern "C" fn(l: *const InputEvents) -> u32>,
    pub get: Option<unsafe extern "C" fn(l: *const InputEvents, index: u32) -> *const EventHeader>,
}

#[repr(C)]
pub struct OutputEvents {
    pub ctx: *mut c_void,
    pub try_push: Option<unsafe extern "C" fn(l: *const OutputEvents, ev: *const EventHeader) -> bool>,
}

pub const EXT_PARAMS: &[u8] = b"clap.params\0";

#[repr(C)]
pub struct ParamInfo {
    pub id: u32,
    pub flags: u32,
    pub cookie: *mut c_void,
    pub name: [c_char; 256],
    pub module: [c_char; 1024],
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

pub const PARAM_STEPPED: u32 = 1;
pub const PARAM_HIDDEN: u32 = 4;
pub const PARAM_READONLY: u32 = 8;

#[repr(C)]
pub struct PluginParams {
    pub count: Option<unsafe extern "C" fn(p: *const Plugin) -> u32>,
    pub get_info: Option<unsafe extern "C" fn(p: *const Plugin, index: u32, info: *mut ParamInfo) -> bool>,
    pub get_value: Option<unsafe extern "C" fn(p: *const Plugin, id: u32, out: *mut f64) -> bool>,
    pub value_to_text: Option<unsafe extern "C" fn(p: *const Plugin, id: u32, value: f64, out: *mut c_char, capacity: u32) -> bool>,
    pub text_to_value: Option<unsafe extern "C" fn(p: *const Plugin, id: u32, text: *const c_char, out: *mut f64) -> bool>,
    pub flush: Option<unsafe extern "C" fn(p: *const Plugin, input: *const InputEvents, output: *const OutputEvents)>,
}

unsafe impl Sync for PluginParams {}

#[repr(C)]
pub struct HostParams {
    pub rescan: Option<unsafe extern "C" fn(h: *const Host, flags: u32)>,
    pub clear: Option<unsafe extern "C" fn(h: *const Host, id: u32, flags: u32)>,
    pub request_flush: Option<unsafe extern "C" fn(h: *const Host)>,
}

pub const EXT_STATE: &[u8] = b"clap.state\0";

#[repr(C)]
pub struct IStream {
    pub ctx: *mut c_void,
    pub read: Option<unsafe extern "C" fn(s: *const IStream, buffer: *mut c_void, size: u64) -> i64>,
}

#[repr(C)]
pub struct OStream {
    pub ctx: *mut c_void,
    pub write: Option<unsafe extern "C" fn(s: *const OStream, buffer: *const c_void, size: u64) -> i64>,
}

#[repr(C)]
pub struct PluginState {
    pub save: Option<unsafe extern "C" fn(p: *const Plugin, s: *const OStream) -> bool>,
    pub load: Option<unsafe extern "C" fn(p: *const Plugin, s: *const IStream) -> bool>,
}

unsafe impl Sync for PluginState {}

#[repr(C)]
pub struct HostState {
    pub mark_dirty: Option<unsafe extern "C" fn(h: *const Host)>,
}

pub const EXT_AUDIO_PORTS: &[u8] = b"clap.audio-ports\0";

#[repr(C)]
pub struct AudioPortInfo {
    pub id: u32,
    pub name: [c_char; 256],
    pub flags: u32,
    pub channels: u32,
    pub port_type: *const c_char,
    pub in_place_pair: u32,
}

#[repr(C)]
pub struct PluginAudioPorts {
    pub count: Option<unsafe extern "C" fn(p: *const Plugin, is_input: bool) -> u32>,
    pub get: Option<unsafe extern "C" fn(p: *const Plugin, index: u32, is_input: bool, info: *mut AudioPortInfo) -> bool>,
}

unsafe impl Sync for PluginAudioPorts {}

pub const EXT_NOTE_PORTS: &[u8] = b"clap.note-ports\0";

#[repr(C)]
pub struct NotePortInfo {
    pub id: u32,
    pub supported: u32,
    pub preferred: u32,
    pub name: [c_char; 256],
}

pub const DIALECT_CLAP: u32 = 1;
pub const DIALECT_MIDI: u32 = 2;

#[repr(C)]
pub struct PluginNotePorts {
    pub count: Option<unsafe extern "C" fn(p: *const Plugin, is_input: bool) -> u32>,
    pub get: Option<unsafe extern "C" fn(p: *const Plugin, index: u32, is_input: bool, info: *mut NotePortInfo) -> bool>,
}

unsafe impl Sync for PluginNotePorts {}

pub const EXT_THREAD_CHECK: &[u8] = b"clap.thread-check\0";

#[repr(C)]
pub struct HostThreadCheck {
    pub is_main: Option<unsafe extern "C" fn(h: *const Host) -> bool>,
    pub is_audio: Option<unsafe extern "C" fn(h: *const Host) -> bool>,
}

pub const EXT_TIMER: &[u8] = b"clap.timer-support\0";

#[repr(C)]
pub struct HostTimer {
    pub register: Option<unsafe extern "C" fn(h: *const Host, period_ms: u32, id: *mut u32) -> bool>,
    pub unregister: Option<unsafe extern "C" fn(h: *const Host, id: u32) -> bool>,
}

#[repr(C)]
pub struct PluginTimer {
    pub on_timer: Option<unsafe extern "C" fn(p: *const Plugin, id: u32)>,
}

pub const EXT_FD: &[u8] = b"clap.posix-fd-support\0";

#[repr(C)]
pub struct HostFd {
    pub register: Option<unsafe extern "C" fn(h: *const Host, fd: i32, flags: u32) -> bool>,
    pub modify: Option<unsafe extern "C" fn(h: *const Host, fd: i32, flags: u32) -> bool>,
    pub unregister: Option<unsafe extern "C" fn(h: *const Host, fd: i32) -> bool>,
}

#[repr(C)]
pub struct PluginFd {
    pub on_fd: Option<unsafe extern "C" fn(p: *const Plugin, fd: i32, flags: u32)>,
}

pub const EXT_GUI: &[u8] = b"clap.gui\0";
pub const WINDOW_X11: &[u8] = b"x11\0";
pub const WINDOW_WAYLAND: &[u8] = b"wayland\0";
pub const WINDOW_WIN32: &[u8] = b"win32\0";

#[repr(C)]
pub struct Window {
    pub api: *const c_char,
    /// An X11 window number, or a window handle on Windows.
    pub handle: usize,
}

#[repr(C)]
pub struct PluginGui {
    pub is_api_supported: Option<unsafe extern "C" fn(p: *const Plugin, api: *const c_char, floating: bool) -> bool>,
    pub get_preferred_api: Option<unsafe extern "C" fn(p: *const Plugin, api: *mut *const c_char, floating: *mut bool) -> bool>,
    pub create: Option<unsafe extern "C" fn(p: *const Plugin, api: *const c_char, floating: bool) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(p: *const Plugin)>,
    pub set_scale: Option<unsafe extern "C" fn(p: *const Plugin, scale: f64) -> bool>,
    pub get_size: Option<unsafe extern "C" fn(p: *const Plugin, w: *mut u32, h: *mut u32) -> bool>,
    pub can_resize: Option<unsafe extern "C" fn(p: *const Plugin) -> bool>,
    pub get_resize_hints: Option<unsafe extern "C" fn(p: *const Plugin, hints: *mut c_void) -> bool>,
    pub adjust_size: Option<unsafe extern "C" fn(p: *const Plugin, w: *mut u32, h: *mut u32) -> bool>,
    pub set_size: Option<unsafe extern "C" fn(p: *const Plugin, w: u32, h: u32) -> bool>,
    pub set_parent: Option<unsafe extern "C" fn(p: *const Plugin, w: *const Window) -> bool>,
    pub set_transient: Option<unsafe extern "C" fn(p: *const Plugin, w: *const Window) -> bool>,
    pub suggest_title: Option<unsafe extern "C" fn(p: *const Plugin, title: *const c_char)>,
    pub show: Option<unsafe extern "C" fn(p: *const Plugin) -> bool>,
    pub hide: Option<unsafe extern "C" fn(p: *const Plugin) -> bool>,
}

#[repr(C)]
pub struct HostGui {
    pub resize_hints_changed: Option<unsafe extern "C" fn(h: *const Host)>,
    pub request_resize: Option<unsafe extern "C" fn(h: *const Host, w: u32, h2: u32) -> bool>,
    pub request_show: Option<unsafe extern "C" fn(h: *const Host) -> bool>,
    pub request_hide: Option<unsafe extern "C" fn(h: *const Host) -> bool>,
    pub closed: Option<unsafe extern "C" fn(h: *const Host, was_destroyed: bool)>,
}
