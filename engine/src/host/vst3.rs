//! Plug-ins of the VST3 kind, behind the same `Instance` the rest of the program uses.
//!
//! The interfaces are written out here from their public description: tables of functions in a fixed
//! order, each starting with the three of the common base (ask for another interface, keep, let go).
//! A plug-in of this kind has two halves, one that makes the sound and one that knows the settings;
//! often they are one object, sometimes two that the host introduces to each other.

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;

use super::abi3::*;
use super::view3::Frame;
use super::{AudioRole, Clock, Ev, Found, Param, MAX_FRAMES};

/// The table of an object the plug-in gave us.
unsafe fn vt<T>(o: Obj) -> &'static T {
    &**(o as *mut *const T)
}

unsafe fn query(o: Obj, want: &Tuid) -> Obj {
    let mut out: Obj = std::ptr::null_mut();
    if o.is_null() || (vt::<Unknown>(o).query)(o, want, &mut out) != OK {
        return std::ptr::null_mut();
    }
    out
}

unsafe fn let_go(o: Obj) {
    if !o.is_null() {
        (vt::<Unknown>(o).drop)(o);
    }
}

fn field(raw: &[c_char]) -> String {
    let bytes: Vec<u8> = raw.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

fn wide(raw: &[u16]) -> String {
    let n = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..n])
}

// ---- our side: the objects the plug-in talks to ----

unsafe extern "system" fn one(_: Obj) -> u32 {
    1
}

unsafe fn answer(this: Obj, want: *const Tuid, out: *mut Obj, mine: &Tuid) -> i32 {
    if out.is_null() || want.is_null() {
        return BAD_ARG;
    }
    if *want == FUNKNOWN || *want == *mine {
        *out = this;
        OK
    } else {
        *out = std::ptr::null_mut();
        NO_INTERFACE
    }
}

#[repr(C)]
struct HostApp {
    vt: *const HostVt,
}

unsafe extern "system" fn host_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    answer(this, want, out, &IHOSTAPP)
}
unsafe extern "system" fn host_name(_: Obj, name: *mut u16) -> i32 {
    if name.is_null() {
        return BAD_ARG;
    }
    let text: Vec<u16> = "Breakfast Cereal DAW".encode_utf16().chain(Some(0)).collect();
    std::ptr::copy_nonoverlapping(text.as_ptr(), name, text.len());
    OK
}
unsafe extern "system" fn host_create(_: Obj, class: *const Tuid, want: *const Tuid, out: *mut Obj) -> i32 {
    if class.is_null() || want.is_null() || out.is_null() {
        return BAD_ARG;
    }
    *out = std::ptr::null_mut();
    if *class == IMESSAGE && *want == IMESSAGE {
        *out = Message::make() as Obj;
        OK
    } else if *class == IATTRS && *want == IATTRS {
        *out = Attrs::make() as Obj;
        OK
    } else {
        NO_INTERFACE
    }
}
static HOST_VT: HostVt = HostVt { base: Unknown { query: host_query, keep: one, drop: one }, name: host_name, create: host_create };

/// What the plug-in tells us, and the settings travelling between its halves.
#[repr(C)]
struct Handler {
    vt: *const HandlerVt,
    dirty: AtomicBool,
    rescan: AtomicBool,
    restart: AtomicBool,
    /// Changed in the plug-in's own window: for the sound half.
    edits: Mutex<Vec<(u32, f64)>>,
    /// Changed by the sound half: for the settings half.
    to_ctrl: Mutex<Vec<(u32, f64)>>,
    /// Changed by the plug-in: for whoever asks `changed`.
    news: Mutex<Vec<(u32, f64)>>,
}

unsafe extern "system" fn handler_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    answer(this, want, out, &IHANDLER)
}
unsafe extern "system" fn handler_edge(_: Obj, _: u32) -> i32 {
    OK
}
unsafe extern "system" fn handler_perform(this: Obj, id: u32, value: f64) -> i32 {
    let h = &*(this as *const Handler);
    for q in [&h.edits, &h.news] {
        if let Ok(mut q) = q.lock() {
            q.retain(|x| x.0 != id);
            q.push((id, value));
        }
    }
    h.dirty.store(true, Relaxed);
    OK
}
unsafe extern "system" fn handler_restart(this: Obj, flags: i32) -> i32 {
    let h = &*(this as *const Handler);
    // reload, ins and outs changed, delay changed: stop and start again
    if flags & 0b1011 != 0 {
        h.restart.store(true, Relaxed);
    }
    // values or names changed
    if flags & 0b10100 != 0 {
        h.rescan.store(true, Relaxed);
        h.dirty.store(true, Relaxed);
    }
    OK
}
static HANDLER_VT: HandlerVt = HandlerVt { base: Unknown { query: handler_query, keep: one, drop: one }, begin: handler_edge, perform: handler_perform, end: handler_edge, restart: handler_restart };

struct Side {
    host: HostApp,
    handler: Handler,
}

/// Bytes in memory the plug-in reads its state from or writes it to.
#[repr(C)]
struct Stream {
    vt: *const StreamVt,
    data: Vec<u8>,
    at: usize,
}

impl Stream {
    fn new(data: Vec<u8>) -> Stream {
        Stream { vt: &STREAM_VT, data, at: 0 }
    }
    fn obj(&mut self) -> Obj {
        self as *mut Stream as Obj
    }
}

unsafe extern "system" fn stream_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    answer(this, want, out, &ISTREAM)
}
unsafe extern "system" fn stream_read(this: Obj, buf: *mut c_void, n: i32, got: *mut i32) -> i32 {
    let s = &mut *(this as *mut Stream);
    if buf.is_null() || n < 0 {
        return BAD_ARG;
    }
    let take = (s.data.len().saturating_sub(s.at)).min(n as usize);
    std::ptr::copy_nonoverlapping(s.data.as_ptr().add(s.at.min(s.data.len())), buf as *mut u8, take);
    s.at += take;
    if !got.is_null() {
        *got = take as i32;
    }
    OK
}
unsafe extern "system" fn stream_write(this: Obj, buf: *mut c_void, n: i32, wrote: *mut i32) -> i32 {
    let s = &mut *(this as *mut Stream);
    if buf.is_null() || n < 0 {
        return BAD_ARG;
    }
    let n = n as usize;
    if s.data.len() < s.at + n {
        s.data.resize(s.at + n, 0);
    }
    std::ptr::copy_nonoverlapping(buf as *const u8, s.data.as_mut_ptr().add(s.at), n);
    s.at += n;
    if !wrote.is_null() {
        *wrote = n as i32;
    }
    OK
}
unsafe extern "system" fn stream_seek(this: Obj, pos: i64, mode: i32, result: *mut i64) -> i32 {
    let s = &mut *(this as *mut Stream);
    let base = match mode {
        0 => 0,
        1 => s.at as i64,
        2 => s.data.len() as i64,
        _ => return BAD_ARG,
    };
    s.at = (base + pos).max(0) as usize;
    if !result.is_null() {
        *result = s.at as i64;
    }
    OK
}
unsafe extern "system" fn stream_tell(this: Obj, pos: *mut i64) -> i32 {
    if pos.is_null() {
        return BAD_ARG;
    }
    *pos = (*(this as *const Stream)).at as i64;
    OK
}
static STREAM_VT: StreamVt = StreamVt { base: Unknown { query: stream_query, keep: one, drop: one }, read: stream_read, write: stream_write, seek: stream_seek, tell: stream_tell };

#[repr(C)]
struct EventList {
    vt: *const EventsVt,
    list: Vec<Event>,
}

unsafe extern "system" fn events_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    answer(this, want, out, &IEVENTS)
}
unsafe extern "system" fn events_count(this: Obj) -> i32 {
    (&*(this as *const EventList)).list.len() as i32
}
unsafe extern "system" fn events_get(this: Obj, index: i32, out: *mut Event) -> i32 {
    match (&*(this as *const EventList)).list.get(index as usize) {
        Some(e) if !out.is_null() && index >= 0 => {
            *out = *e;
            OK
        }
        _ => BAD_ARG,
    }
}
unsafe extern "system" fn events_add(this: Obj, ev: *mut Event) -> i32 {
    let l = &mut (&mut *(this as *mut EventList)).list;
    // never grows on the audio thread
    if ev.is_null() || l.len() == l.capacity() {
        return FALSE;
    }
    l.push(*ev);
    OK
}
static EVENTS_VT: EventsVt = EventsVt { base: Unknown { query: events_query, keep: one, drop: one }, count: events_count, get: events_get, add: events_add };

const QUEUES: usize = 256;
const POINTS: usize = 16;

/// The values one setting takes during one call.
#[repr(C)]
struct Queue {
    vt: *const QueueVt,
    id: u32,
    used: usize,
    points: [(i32, f64); POINTS],
}

unsafe extern "system" fn queue_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    answer(this, want, out, &IQUEUE)
}
unsafe extern "system" fn queue_id(this: Obj) -> u32 {
    (*(this as *const Queue)).id
}
unsafe extern "system" fn queue_count(this: Obj) -> i32 {
    (*(this as *const Queue)).used as i32
}
unsafe extern "system" fn queue_get(this: Obj, index: i32, offset: *mut i32, value: *mut f64) -> i32 {
    let q = &*(this as *const Queue);
    if index < 0 || index as usize >= q.used || offset.is_null() || value.is_null() {
        return BAD_ARG;
    }
    *offset = q.points[index as usize].0;
    *value = q.points[index as usize].1;
    OK
}
unsafe extern "system" fn queue_add(this: Obj, offset: i32, value: f64, index: *mut i32) -> i32 {
    let q = &mut *(this as *mut Queue);
    // when full the last point is replaced, so the newest value still arrives
    let at = q.used.min(POINTS - 1);
    q.points[at] = (offset, value);
    q.used = at + 1;
    if !index.is_null() {
        *index = at as i32;
    }
    OK
}
static QUEUE_VT: QueueVt = QueueVt { base: Unknown { query: queue_query, keep: one, drop: one }, id: queue_id, count: queue_count, get: queue_get, add: queue_add };

/// The settings changes of one call, in or out.
#[repr(C)]
struct Changes {
    vt: *const ChangesVt,
    used: usize,
    queues: Vec<Queue>,
}

impl Changes {
    fn new() -> Box<Changes> {
        Box::new(Changes { vt: &CHANGES_VT, used: 0, queues: (0..QUEUES).map(|_| Queue { vt: &QUEUE_VT, id: 0, used: 0, points: [(0, 0.0); POINTS] }).collect() })
    }
    fn add(&mut self, id: u32) -> Option<&mut Queue> {
        if let Some(i) = self.queues[..self.used].iter().position(|q| q.id == id) {
            return Some(&mut self.queues[i]);
        }
        if self.used == QUEUES {
            return None;
        }
        self.used += 1;
        let q = &mut self.queues[self.used - 1];
        q.id = id;
        q.used = 0;
        Some(q)
    }
    fn point(&mut self, id: u32, offset: i32, value: f64) -> bool {
        match self.add(id) {
            Some(q) => {
                let at = q.used.min(POINTS - 1);
                q.points[at] = (offset, value);
                q.used = at + 1;
                true
            }
            None => false,
        }
    }
}

unsafe extern "system" fn changes_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    answer(this, want, out, &ICHANGES)
}
unsafe extern "system" fn changes_count(this: Obj) -> i32 {
    (*(this as *const Changes)).used as i32
}
unsafe extern "system" fn changes_get(this: Obj, index: i32) -> Obj {
    let c = &mut *(this as *mut Changes);
    if index < 0 || index as usize >= c.used {
        return std::ptr::null_mut();
    }
    &mut c.queues[index as usize] as *mut Queue as Obj
}
unsafe extern "system" fn changes_add(this: Obj, id: *const u32, index: *mut i32) -> Obj {
    let c = &mut *(this as *mut Changes);
    if id.is_null() {
        return std::ptr::null_mut();
    }
    match c.add(*id) {
        Some(q) => {
            let q = q as *mut Queue;
            if !index.is_null() {
                *index = q.offset_from(c.queues.as_ptr()) as i32;
            }
            q as Obj
        }
        None => std::ptr::null_mut(),
    }
}
static CHANGES_VT: ChangesVt = ChangesVt { base: Unknown { query: changes_query, keep: one, drop: one }, count: changes_count, get: changes_get, add: changes_add };

// Messages the two halves send each other through us. These are counted, as the plug-in owns them.

enum Val {
    Int(i64),
    Float(f64),
    Text(Vec<u16>),
    Bytes(Vec<u8>),
}

#[repr(C)]
struct Attrs {
    vt: *const AttrsVt,
    refs: AtomicU32,
    items: HashMap<Vec<u8>, Val>,
}

impl Attrs {
    fn make() -> *mut Attrs {
        Box::into_raw(Box::new(Attrs { vt: &ATTRS_VT, refs: AtomicU32::new(1), items: HashMap::new() }))
    }
    unsafe fn put(this: Obj, key: *const c_char, v: Val) -> i32 {
        if key.is_null() {
            return BAD_ARG;
        }
        (&mut *(this as *mut Attrs)).items.insert(std::ffi::CStr::from_ptr(key).to_bytes().to_vec(), v);
        OK
    }
    unsafe fn at<'a>(this: Obj, key: *const c_char) -> Option<&'a Val> {
        if key.is_null() {
            return None;
        }
        (&*(this as *const Attrs)).items.get(std::ffi::CStr::from_ptr(key).to_bytes())
    }
}

unsafe extern "system" fn attrs_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    let r = answer(this, want, out, &IATTRS);
    if r == OK {
        attrs_keep(this);
    }
    r
}
unsafe extern "system" fn attrs_keep(this: Obj) -> u32 {
    (&*(this as *const Attrs)).refs.fetch_add(1, Relaxed) + 1
}
unsafe extern "system" fn attrs_drop(this: Obj) -> u32 {
    let left = (&*(this as *const Attrs)).refs.fetch_sub(1, Relaxed) - 1;
    if left == 0 {
        drop(Box::from_raw(this as *mut Attrs));
    }
    left
}
unsafe extern "system" fn attrs_set_int(this: Obj, key: *const c_char, v: i64) -> i32 {
    Attrs::put(this, key, Val::Int(v))
}
unsafe extern "system" fn attrs_get_int(this: Obj, key: *const c_char, out: *mut i64) -> i32 {
    match Attrs::at(this, key) {
        Some(Val::Int(v)) if !out.is_null() => {
            *out = *v;
            OK
        }
        _ => FALSE,
    }
}
unsafe extern "system" fn attrs_set_float(this: Obj, key: *const c_char, v: f64) -> i32 {
    Attrs::put(this, key, Val::Float(v))
}
unsafe extern "system" fn attrs_get_float(this: Obj, key: *const c_char, out: *mut f64) -> i32 {
    match Attrs::at(this, key) {
        Some(Val::Float(v)) if !out.is_null() => {
            *out = *v;
            OK
        }
        _ => FALSE,
    }
}
unsafe extern "system" fn attrs_set_text(this: Obj, key: *const c_char, text: *const u16) -> i32 {
    if text.is_null() {
        return BAD_ARG;
    }
    let mut v = Vec::new();
    let mut p = text;
    while *p != 0 {
        v.push(*p);
        p = p.add(1);
    }
    Attrs::put(this, key, Val::Text(v))
}
unsafe extern "system" fn attrs_get_text(this: Obj, key: *const c_char, out: *mut u16, bytes: u32) -> i32 {
    match Attrs::at(this, key) {
        Some(Val::Text(v)) if !out.is_null() && bytes >= 2 => {
            let n = v.len().min(bytes as usize / 2 - 1);
            std::ptr::copy_nonoverlapping(v.as_ptr(), out, n);
            *out.add(n) = 0;
            OK
        }
        _ => FALSE,
    }
}
unsafe extern "system" fn attrs_set_bytes(this: Obj, key: *const c_char, data: *const c_void, size: u32) -> i32 {
    if data.is_null() && size > 0 {
        return BAD_ARG;
    }
    let v = if size == 0 { Vec::new() } else { std::slice::from_raw_parts(data as *const u8, size as usize).to_vec() };
    Attrs::put(this, key, Val::Bytes(v))
}
unsafe extern "system" fn attrs_get_bytes(this: Obj, key: *const c_char, out: *mut *const c_void, size: *mut u32) -> i32 {
    match Attrs::at(this, key) {
        Some(Val::Bytes(v)) if !out.is_null() && !size.is_null() => {
            *out = v.as_ptr() as *const c_void;
            *size = v.len() as u32;
            OK
        }
        _ => FALSE,
    }
}
static ATTRS_VT: AttrsVt = AttrsVt {
    base: Unknown { query: attrs_query, keep: attrs_keep, drop: attrs_drop },
    set_int: attrs_set_int,
    get_int: attrs_get_int,
    set_float: attrs_set_float,
    get_float: attrs_get_float,
    set_text: attrs_set_text,
    get_text: attrs_get_text,
    set_bytes: attrs_set_bytes,
    get_bytes: attrs_get_bytes,
};

#[repr(C)]
struct Message {
    vt: *const MessageVt,
    refs: AtomicU32,
    id: CString,
    attrs: *mut Attrs,
}

impl Message {
    fn make() -> *mut Message {
        Box::into_raw(Box::new(Message { vt: &MESSAGE_VT, refs: AtomicU32::new(1), id: CString::default(), attrs: Attrs::make() }))
    }
}

unsafe extern "system" fn message_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    let r = answer(this, want, out, &IMESSAGE);
    if r == OK {
        message_keep(this);
    }
    r
}
unsafe extern "system" fn message_keep(this: Obj) -> u32 {
    (&*(this as *const Message)).refs.fetch_add(1, Relaxed) + 1
}
unsafe extern "system" fn message_drop(this: Obj) -> u32 {
    let left = (&*(this as *const Message)).refs.fetch_sub(1, Relaxed) - 1;
    if left == 0 {
        let m = Box::from_raw(this as *mut Message);
        attrs_drop(m.attrs as Obj);
    }
    left
}
unsafe extern "system" fn message_id(this: Obj) -> *const c_char {
    (&*(this as *const Message)).id.as_ptr()
}
unsafe extern "system" fn message_set_id(this: Obj, id: *const c_char) {
    (*(this as *mut Message)).id = if id.is_null() { CString::default() } else { std::ffi::CStr::from_ptr(id).to_owned() };
}
unsafe extern "system" fn message_attrs(this: Obj) -> Obj {
    (*(this as *const Message)).attrs as Obj
}
static MESSAGE_VT: MessageVt = MessageVt { base: Unknown { query: message_query, keep: message_keep, drop: message_drop }, id: message_id, set_id: message_set_id, attrs: message_attrs };

// ---- files ----

struct Module {
    factory: Obj,
}

// SAFETY: the factory is only called from the window's thread.
unsafe impl Send for Module {}
unsafe impl Sync for Module {}

static MODULES: Mutex<Option<HashMap<PathBuf, Option<&'static Module>>>> = Mutex::new(None);

/// The file with the code: a plug-in of this kind is a folder with the code some levels down, or (on
/// Windows, older ones) the file itself.
fn code_of(path: &Path) -> PathBuf {
    if !path.is_dir() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().unwrap_or_default();
    let arch = std::env::consts::ARCH;
    if cfg!(windows) {
        let mut name = stem.to_os_string();
        name.push(".vst3");
        path.join("Contents").join(format!("{arch}-win")).join(name)
    } else {
        let mut name = stem.to_os_string();
        name.push(".so");
        path.join("Contents").join(format!("{arch}-linux")).join(name)
    }
}

fn load(path: &Path) -> Option<&'static Module> {
    let mut g = MODULES.lock().ok()?;
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(m) = map.get(path) {
        return *m;
    }
    let made = (|| {
        let file = code_of(path);
        if !file.is_file() {
            return None;
        }
        type Get = unsafe extern "system" fn() -> Obj;
        // SAFETY: loading runs the file's own start-up code, as loading any plug-in must; the entry
        // points are the ones the standard names.
        #[cfg(unix)]
        let factory = unsafe {
            let lib = libloading::os::unix::Library::new(&file).ok()?;
            let handle = lib.into_raw();
            let lib = libloading::os::unix::Library::from_raw(handle);
            let entry = *lib.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0").ok()?;
            let get = *lib.get::<Get>(b"GetPluginFactory\0").ok()?;
            if !entry(handle) {
                return None;
            }
            std::mem::forget(lib);
            get()
        };
        #[cfg(not(unix))]
        let factory = unsafe {
            let lib = libloading::Library::new(&file).ok()?;
            let get = *lib.get::<Get>(b"GetPluginFactory\0").ok()?;
            if let Ok(init) = lib.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0") {
                if !init() {
                    return None;
                }
            }
            std::mem::forget(lib);
            get()
        };
        if factory.is_null() {
            return None;
        }
        Some(&*Box::leak(Box::new(Module { factory })))
    })();
    map.insert(path.to_path_buf(), made);
    made
}

/// The sound-making classes of a file, with the identifier of each as it lies in memory.
fn classes(path: &Path) -> Vec<(Found, Tuid)> {
    let Some(m) = load(path) else { return vec![] };
    let mut out = Vec::new();
    // SAFETY: the factory's own functions, called as the standard describes.
    unsafe {
        let f = vt::<FactoryVt>(m.factory);
        let mut info: FactoryInfo = std::mem::zeroed();
        (f.info)(m.factory, &mut info);
        let maker = field(&info.vendor);
        let second = query(m.factory, &IFACTORY2);
        for i in 0..(f.count)(m.factory) {
            let mut c: ClassInfo = std::mem::zeroed();
            if (f.class)(m.factory, i, &mut c) != OK || field(&c.category) != "Audio Module Class" {
                continue;
            }
            let (mut vendor, mut instrument) = (maker.clone(), false);
            if !second.is_null() {
                let mut c2: ClassInfo2 = std::mem::zeroed();
                if (vt::<FactoryVt>(second).class2)(second, i, &mut c2) == OK {
                    instrument = field(&c2.sub).split('|').any(|s| s == "Instrument");
                    let v = field(&c2.vendor);
                    if !v.is_empty() {
                        vendor = v;
                    }
                }
            }
            out.push((Found { path: path.to_path_buf(), id: id_text(c.cid), name: field(&c.name), vendor, instrument }, c.cid));
        }
        let_go(second);
    }
    out
}

pub(super) fn inside(path: &Path) -> Vec<Found> {
    classes(path).into_iter().map(|c| c.0).collect()
}

// ---- one plug-in ----

struct Run {
    rate: f32,
    active: bool,
    offline: bool,
    ins: Vec<u32>,
    outs: Vec<u32>,
    in_data: Vec<Vec<f32>>,
    out_data: Vec<Vec<f32>>,
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    in_bufs: Vec<BusBuffers>,
    out_bufs: Vec<BusBuffers>,
    in_events: Box<EventList>,
    out_events: Box<EventList>,
    in_changes: Box<Changes>,
    out_changes: Box<Changes>,
    steady: i64,
}

pub(super) struct Plug {
    component: Obj,
    processor: Obj,
    controller: Obj,
    /// The settings half is an object of its own, started and ended by us.
    separate: bool,
    links: (Obj, Obj),
    side: *mut Side,
    /// Which setting each MIDI controller of each channel moves, when the plug-in says.
    midi_map: Vec<u32>,
    /// Of each setting: its step count (stepped ones are shown as whole numbers) and its unit.
    meta: Mutex<HashMap<u32, (i32, String)>>,
    pub has_note_in: bool,
    /// Our side of the plug-in's editor window.
    frame: *mut Frame,
    /// Whether it offers an editor we can show, once asked.
    editor: std::sync::OnceLock<bool>,
    run: Mutex<Run>,
}

impl Plug {
    pub fn open(path: &Path, id: &str) -> Option<(Plug, Found)> {
        let (found, cid) = classes(path).into_iter().find(|c| id.is_empty() || c.0.id == id || id_of(id) == Some(c.1))?;
        let m = load(path)?;
        // SAFETY: the plug-in is made and started in the order the standard asks, on the window's thread;
        // everything handed to it lives in `side`, which is freed after the plug-in is.
        unsafe {
            let f = vt::<FactoryVt>(m.factory);
            let mut component: Obj = std::ptr::null_mut();
            if (f.create)(m.factory, &cid, &ICOMPONENT, &mut component) != OK || component.is_null() {
                return None;
            }
            let side = Box::into_raw(Box::new(Side {
                host: HostApp { vt: &HOST_VT },
                handler: Handler { vt: &HANDLER_VT, dirty: AtomicBool::new(false), rescan: AtomicBool::new(false), restart: AtomicBool::new(false), edits: Mutex::new(Vec::with_capacity(512)), to_ctrl: Mutex::new(Vec::with_capacity(512)), news: Mutex::new(Vec::with_capacity(512)) },
            }));
            let host = &mut (*side).host as *mut HostApp as Obj;
            let cv = vt::<ComponentVt>(component);
            if (cv.initialize)(component, host) != OK {
                let_go(component);
                drop(Box::from_raw(side));
                return None;
            }
            let processor = query(component, &IPROCESSOR);
            if processor.is_null() {
                (cv.terminate)(component);
                let_go(component);
                drop(Box::from_raw(side));
                return None;
            }
            let mut controller = query(component, &ICONTROLLER);
            let mut separate = false;
            if controller.is_null() {
                let mut ccid: Tuid = [0; 16];
                if (cv.controller_class)(component, &mut ccid) == OK && ccid != [0; 16] {
                    let mut made: Obj = std::ptr::null_mut();
                    if (f.create)(m.factory, &ccid, &ICONTROLLER, &mut made) == OK && !made.is_null() {
                        if (vt::<ControllerVt>(made).initialize)(made, host) == OK {
                            controller = made;
                            separate = true;
                        } else {
                            let_go(made);
                        }
                    }
                }
            }
            let mut links: (Obj, Obj) = (std::ptr::null_mut(), std::ptr::null_mut());
            if !controller.is_null() {
                (vt::<ControllerVt>(controller).set_handler)(controller, &mut (*side).handler as *mut Handler as Obj);
                if separate {
                    let (a, b) = (query(component, &ICONNECTION), query(controller, &ICONNECTION));
                    if !a.is_null() && !b.is_null() {
                        (vt::<ConnectionVt>(a).connect)(a, b);
                        (vt::<ConnectionVt>(b).connect)(b, a);
                        links = (a, b);
                    } else {
                        let_go(a);
                        let_go(b);
                    }
                }
            }
            let mut counts = [Vec::new(), Vec::new()];
            for dir in [IN, OUT] {
                for i in 0..(cv.bus_count)(component, AUDIO, dir) {
                    let mut info: BusInfo = std::mem::zeroed();
                    (cv.bus_info)(component, AUDIO, dir, i, &mut info);
                    (cv.activate_bus)(component, AUDIO, dir, i, 1);
                    counts[dir as usize].push(info.channels.clamp(0, 64) as u32);
                }
            }
            let has_note_in = (cv.bus_count)(component, EVENT, IN) > 0;
            for dir in [IN, OUT] {
                for i in 0..(cv.bus_count)(component, EVENT, dir) {
                    (cv.activate_bus)(component, EVENT, dir, i, 1);
                }
            }
            let mut midi_map = Vec::new();
            let map = query(controller, &IMIDIMAP);
            if !map.is_null() {
                if has_note_in {
                    midi_map = vec![u32::MAX; 16 * 130];
                    for ch in 0..16i16 {
                        for cc in 0..130i16 {
                            let mut id = 0u32;
                            if (vt::<MidiMapVt>(map).assignment)(map, 0, ch, cc, &mut id) == OK {
                                midi_map[ch as usize * 130 + cc as usize] = id;
                            }
                        }
                    }
                }
                let_go(map);
            }
            let [ins, outs] = counts;
            let run = Run {
                rate: 0.0,
                active: false,
                offline: false,
                ins,
                outs,
                in_data: vec![],
                out_data: vec![],
                in_ptrs: vec![],
                out_ptrs: vec![],
                in_bufs: vec![],
                out_bufs: vec![],
                in_events: Box::new(EventList { vt: &EVENTS_VT, list: Vec::with_capacity(1024) }),
                out_events: Box::new(EventList { vt: &EVENTS_VT, list: Vec::with_capacity(1024) }),
                in_changes: Changes::new(),
                out_changes: Changes::new(),
                steady: 0,
            };
            let plug = Plug { component, processor, controller, separate, links, side, midi_map, meta: Mutex::new(HashMap::new()), has_note_in, frame: Frame::make(), editor: std::sync::OnceLock::new(), run: Mutex::new(run) };
            plug.sync_controller();
            plug.params();
            Some((plug, found))
        }
    }

    fn handler(&self) -> &Handler {
        // SAFETY: lives as long as the plug-in.
        unsafe { &(*self.side).handler }
    }

    /// Show the settings half what the sound half holds.
    fn sync_controller(&self) {
        if self.controller.is_null() {
            return;
        }
        // SAFETY: window-thread calls into a live plug-in; the stream outlives them.
        unsafe {
            let mut s = Stream::new(Vec::new());
            if (vt::<ComponentVt>(self.component).get_state)(self.component, s.obj()) == OK {
                s.at = 0;
                (vt::<ControllerVt>(self.controller).set_component_state)(self.controller, s.obj());
            }
        }
    }

    pub fn rate(&self) -> Option<f32> {
        self.run.lock().ok().filter(|r| r.active).map(|r| r.rate)
    }

    pub fn takes_sound(&self) -> bool {
        self.run.lock().map(|r| r.ins.first().is_some_and(|&c| c > 0)).unwrap_or(false)
    }

    pub fn activate(&self, rate: f32, offline: bool) -> bool {
        let Ok(mut r) = self.run.lock() else { return false };
        if r.active && r.rate == rate && r.offline == offline && !self.handler().restart.load(Relaxed) {
            return true;
        }
        self.handler().restart.store(false, Relaxed);
        // SAFETY: the lock keeps the audio thread out while the plug-in is stopped and started.
        unsafe {
            let (cv, pv) = (vt::<ComponentVt>(self.component), vt::<ProcessorVt>(self.processor));
            if r.active {
                (pv.set_processing)(self.processor, 0);
                (cv.set_active)(self.component, 0);
                r.active = false;
            }
            let r = &mut *r;
            r.in_data = (0..r.ins.iter().sum::<u32>()).map(|_| vec![0.0; MAX_FRAMES]).collect();
            r.out_data = (0..r.outs.iter().sum::<u32>()).map(|_| vec![0.0; MAX_FRAMES]).collect();
            r.in_ptrs = r.in_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
            r.out_ptrs = r.out_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
            let bufs = |counts: &[u32], ptrs: &mut [*mut f32]| {
                let mut at = 0;
                counts
                    .iter()
                    .map(|&c| {
                        let b = BusBuffers { channels: c as i32, silence: 0, data: ptrs[at..].as_mut_ptr() };
                        at += c as usize;
                        b
                    })
                    .collect::<Vec<_>>()
            };
            r.in_bufs = bufs(&r.ins, &mut r.in_ptrs);
            r.out_bufs = bufs(&r.outs, &mut r.out_ptrs);
            r.rate = rate;
            r.offline = offline;
            let mut setup = Setup { mode: if offline { 2 } else { 0 }, sample_size: 0, max_frames: MAX_FRAMES as i32, rate: rate as f64 };
            if (pv.setup)(self.processor, &mut setup) != OK {
                return false;
            }
            r.active = (cv.set_active)(self.component, 1) == OK;
            if r.active {
                (pv.set_processing)(self.processor, 1);
            }
            r.active
        }
    }

    /// One call into the sound half. `frames` zero carries settings over with no sound.
    unsafe fn call(&self, r: &mut Run, frames: usize, context: &mut Context) {
        r.out_changes.used = 0;
        r.out_events.list.clear();
        for b in r.in_bufs.iter_mut().chain(r.out_bufs.iter_mut()) {
            b.silence = 0;
        }
        let mut data = ProcessData {
            mode: if r.offline { 2 } else { 0 },
            sample_size: 0,
            frames: frames as i32,
            n_in: if frames > 0 { r.in_bufs.len() as i32 } else { 0 },
            n_out: if frames > 0 { r.out_bufs.len() as i32 } else { 0 },
            ins: if frames > 0 && !r.in_bufs.is_empty() { r.in_bufs.as_mut_ptr() } else { std::ptr::null_mut() },
            outs: if frames > 0 && !r.out_bufs.is_empty() { r.out_bufs.as_mut_ptr() } else { std::ptr::null_mut() },
            in_changes: &mut *r.in_changes as *mut Changes as Obj,
            out_changes: &mut *r.out_changes as *mut Changes as Obj,
            in_events: &mut *r.in_events as *mut EventList as Obj,
            out_events: &mut *r.out_events as *mut EventList as Obj,
            context,
        };
        (vt::<ProcessorVt>(self.processor).process)(self.processor, &mut data);
        r.in_changes.used = 0;
        r.in_events.list.clear();
        // what the sound half changed by itself goes to the settings half and to whoever listens
        if r.out_changes.used > 0 {
            let h = self.handler();
            if let (Ok(mut a), Ok(mut b)) = (h.to_ctrl.try_lock(), h.news.try_lock()) {
                for q in &r.out_changes.queues[..r.out_changes.used] {
                    if q.used > 0 && a.len() < a.capacity() && b.len() < b.capacity() {
                        let v = (q.id, q.points[q.used - 1].1);
                        a.retain(|x| x.0 != v.0);
                        a.push(v);
                        b.retain(|x| x.0 != v.0);
                        b.push(v);
                    }
                }
            }
        }
    }

    /// Settings waiting to go over, into the next call.
    fn take_changes(&self, r: &mut Run, wait: bool) {
        let q = &self.handler().edits;
        let got = if wait { q.lock().ok() } else { q.try_lock().ok() };
        if let Some(mut q) = got {
            let mut kept = 0;
            for i in 0..q.len() {
                let (id, v) = q[i];
                if !r.in_changes.point(id, 0, v) {
                    q[kept] = (id, v);
                    kept += 1;
                }
            }
            q.truncate(kept);
        }
    }

    fn steps_of(&self, id: u32) -> i32 {
        self.meta.lock().ok().and_then(|s| s.get(&id).map(|m| m.0)).unwrap_or(0)
    }

    /// A value as we show it (whole steps for stepped settings) to the plug-in's 0..1.
    fn to_normal(&self, id: u32, v: f64) -> f64 {
        match self.steps_of(id) {
            n if n > 0 => (v / n as f64).clamp(0.0, 1.0),
            _ => v.clamp(0.0, 1.0),
        }
    }

    fn from_normal(&self, id: u32, v: f64) -> f64 {
        match self.steps_of(id) {
            n if n > 0 => (v * n as f64).round().min(n as f64),
            _ => v,
        }
    }

    pub fn process(&self, io: &mut [[f32; 2]], evs: &[Ev], clock: &Clock, rate: f32) -> bool {
        let got = if clock.offline { self.run.lock().ok() } else { self.run.try_lock().ok() };
        let Some(mut guard) = got else { return false };
        let _role = AudioRole::take();
        let r = &mut *guard;
        if !r.active || r.rate != rate || r.offline != clock.offline {
            return false;
        }
        let per_frame = if clock.playing { clock.tempo / 60.0 / rate as f64 } else { 0.0 };
        let bar = (clock.sig.0.max(1) as f64) * 4.0 / (clock.sig.1.max(1) as f64);
        let total = io.len();
        let mut done = 0;
        // SAFETY: the plug-in is active, the lock makes this the only caller, and every pointer handed
        // over points into buffers that live in `r` for the length of the call.
        unsafe {
            while done < total {
                let n = (total - done).min(MAX_FRAMES);
                let part = &mut io[done..done + n];
                let first = r.ins.first().copied().unwrap_or(0) as usize;
                for (c, buf) in r.in_data.iter_mut().enumerate() {
                    if c < first {
                        for (o, s) in buf[..n].iter_mut().zip(part.iter()) {
                            *o = if first == 1 { (s[0] + s[1]) * 0.5 } else if c < 2 { s[c] } else { 0.0 };
                        }
                    } else {
                        buf[..n].fill(0.0);
                    }
                }
                for buf in r.out_data.iter_mut() {
                    buf[..n].fill(0.0);
                }
                if done == 0 {
                    self.take_changes(r, clock.offline);
                }
                let beats = clock.beats + done as f64 * per_frame;
                for e in evs {
                    let at = match e {
                        Ev::NoteOn { at, .. } | Ev::NoteOff { at, .. } | Ev::Midi { at, .. } => *at as usize,
                    };
                    if at < done || at >= done + n {
                        continue;
                    }
                    let offset = (at - done) as i32;
                    let mut note = |on: bool, key: u8, chan: u8, vel: f32| {
                        let body = if on { EventBody { on: NoteOn { channel: chan as i16, pitch: key as i16, tuning: 0.0, velocity: vel, length: 0, note_id: -1 } } } else { EventBody { off: NoteOff { channel: chan as i16, pitch: key as i16, velocity: vel, note_id: -1, tuning: 0.0 } } };
                        let l = &mut r.in_events.list;
                        if l.len() < l.capacity() {
                            l.push(Event { bus: 0, offset, beats, flags: 0, kind: if on { 0 } else { 1 }, body });
                        }
                    };
                    match *e {
                        Ev::NoteOn { key, chan, vel, .. } => note(true, key, chan, vel),
                        Ev::NoteOff { key, chan, .. } => note(false, key, chan, 0.0),
                        Ev::Midi { data, .. } => {
                            let chan = data[0] & 0x0F;
                            let moved = match data[0] & 0xF0 {
                                0x90 if data[2] > 0 => {
                                    note(true, data[1], chan, data[2] as f32 / 127.0);
                                    None
                                }
                                0x80 | 0x90 => {
                                    note(false, data[1], chan, 0.0);
                                    None
                                }
                                0xB0 => Some((data[1] as usize, data[2] as f64 / 127.0)),
                                0xD0 => Some((128, data[1] as f64 / 127.0)),
                                0xE0 => Some((129, ((data[2] as u32) << 7 | data[1] as u32) as f64 / 16383.0)),
                                _ => None,
                            };
                            if let Some((cc, v)) = moved {
                                if let Some(&id) = self.midi_map.get(chan as usize * 130 + cc).filter(|&&id| id != u32::MAX) {
                                    r.in_changes.point(id, offset, v);
                                }
                            }
                        }
                    }
                }
                let mut context = Context {
                    state: if clock.playing { CTX_PLAYING } else { 0 } | CTX_BEATS | CTX_TEMPO | CTX_BAR | CTX_SIG | CTX_STEADY,
                    rate: rate as f64,
                    project_samples: if clock.tempo > 0.0 { (beats * 60.0 / clock.tempo * rate as f64) as i64 } else { 0 },
                    steady_samples: r.steady,
                    beats,
                    bar_start: (beats / bar).floor() * bar,
                    tempo: clock.tempo,
                    sig_top: clock.sig.0.max(1) as i32,
                    sig_bottom: clock.sig.1.max(1) as i32,
                    ..Context::default()
                };
                self.call(r, n, &mut context);
                r.steady += n as i64;
                let outs = r.outs.first().copied().unwrap_or(0) as usize;
                for (i, s) in part.iter_mut().enumerate() {
                    *s = match outs {
                        0 => [0.0, 0.0],
                        1 => [r.out_data[0][i], r.out_data[0][i]],
                        _ => [r.out_data[0][i], r.out_data[1][i]],
                    };
                }
                done += n;
            }
        }
        true
    }

    pub fn params(&self) -> Vec<Param> {
        let mut out = Vec::new();
        if self.controller.is_null() {
            return out;
        }
        let mut meta = HashMap::new();
        // SAFETY: the settings half of a live plug-in, on the window's thread.
        unsafe {
            let c = vt::<ControllerVt>(self.controller);
            for i in 0..(c.param_count)(self.controller) {
                let mut info: ParamInfo = std::mem::zeroed();
                if (c.param_info)(self.controller, i, &mut info) != OK || info.flags & 16 != 0 {
                    continue;
                }
                let stepped = info.steps > 0;
                let top = if stepped { info.steps as f64 } else { 1.0 };
                meta.insert(info.id, (info.steps.max(0), wide(&info.units)));
                out.push(Param { id: info.id, name: wide(&info.title), module: String::new(), min: 0.0, max: top, default: if stepped { (info.default * top).round() } else { info.default }, stepped, readonly: info.flags & 2 != 0 });
            }
        }
        if let Ok(mut s) = self.meta.lock() {
            *s = meta;
        }
        out
    }

    pub fn value(&self, id: u32) -> Option<f64> {
        if self.controller.is_null() {
            return None;
        }
        // SAFETY: as in `params`.
        let v = unsafe { (vt::<ControllerVt>(self.controller).get)(self.controller, id) };
        Some(self.from_normal(id, v))
    }

    pub fn value_text(&self, id: u32, value: f64) -> Option<String> {
        if self.controller.is_null() {
            return None;
        }
        let mut buf = [0u16; 128];
        let v = self.to_normal(id, value);
        // SAFETY: as in `params`; the buffer is the size the standard gives.
        unsafe {
            if (vt::<ControllerVt>(self.controller).text_of)(self.controller, id, v, buf.as_mut_ptr()) != OK {
                return None;
            }
        }
        let text = wide(&buf);
        // the unit is kept apart from the number
        let unit = self.meta.lock().ok().and_then(|m| m.get(&id).map(|x| x.1.clone())).unwrap_or_default();
        Some(if unit.is_empty() || text.ends_with(&unit) { text } else { format!("{text} {unit}") })
    }

    /// Tell the settings half at once; the sound half hears with its next call.
    pub fn set(&self, id: u32, value: f64) {
        let v = self.to_normal(id, value);
        if !self.controller.is_null() {
            // SAFETY: as in `params`.
            unsafe {
                (vt::<ControllerVt>(self.controller).set)(self.controller, id, v);
            }
        }
        if let Ok(mut q) = self.handler().edits.lock() {
            q.retain(|x| x.0 != id);
            q.push((id, v));
        }
    }

    pub fn changed(&self) -> Vec<(u32, f64)> {
        let got = self.handler().news.lock().map(|mut q| q.drain(..).collect::<Vec<_>>()).unwrap_or_default();
        got.into_iter().map(|(id, v)| (id, self.from_normal(id, v))).collect()
    }

    /// Both halves' states, the sound half's first with its length before it.
    pub fn save(&self) -> Option<Vec<u8>> {
        // SAFETY: window-thread calls into a live plug-in; the streams outlive them.
        unsafe {
            let mut a = Stream::new(Vec::new());
            if (vt::<ComponentVt>(self.component).get_state)(self.component, a.obj()) != OK {
                return None;
            }
            let mut b = Stream::new(Vec::new());
            if !self.controller.is_null() {
                (vt::<ControllerVt>(self.controller).get_state)(self.controller, b.obj());
            }
            let mut out = (a.data.len() as u32).to_le_bytes().to_vec();
            out.extend_from_slice(&a.data);
            out.extend_from_slice(&b.data);
            Some(out)
        }
    }

    pub fn restore(&self, state: &[u8]) -> bool {
        let Some(n) = state.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else { return false };
        let Some(first) = state.get(4..4 + n) else { return false };
        let rest = &state[4 + n..];
        // SAFETY: as in `save`.
        unsafe {
            let mut a = Stream::new(first.to_vec());
            if (vt::<ComponentVt>(self.component).set_state)(self.component, a.obj()) != OK {
                return false;
            }
            if !self.controller.is_null() {
                a.at = 0;
                (vt::<ControllerVt>(self.controller).set_component_state)(self.controller, a.obj());
                if !rest.is_empty() {
                    let mut b = Stream::new(rest.to_vec());
                    (vt::<ControllerVt>(self.controller).set_state)(self.controller, b.obj());
                }
            }
        }
        true
    }

    pub fn take_dirty(&self) -> bool {
        self.handler().dirty.swap(false, Relaxed)
    }

    pub fn take_rescan(&self) -> bool {
        self.handler().rescan.swap(false, Relaxed)
    }

    pub fn has_window(&self) -> bool {
        // SAFETY: window-thread call into a live plug-in.
        *self.editor.get_or_init(|| unsafe { Frame::offered(self.controller) })
    }

    pub fn show_window(&self, title: &str) -> bool {
        // SAFETY: as in `has_window`; the frame lives as long as the plug-in.
        self.has_window() && unsafe { Frame::show(self.frame, self.controller, title) }
    }

    pub fn hide_window(&self) {
        // SAFETY: as in `show_window`.
        unsafe { Frame::hide(self.frame) }
    }

    pub fn window_open(&self) -> bool {
        // SAFETY: as in `show_window`.
        unsafe { Frame::is_open(self.frame) }
    }

    /// Window thread, every frame or so. `stalled` says no sound ran since the last time.
    pub fn idle(&self, stalled: bool) {
        // SAFETY: as in `show_window`.
        unsafe { Frame::idle(self.frame) };
        let h = self.handler();
        let moved: Vec<(u32, f64)> = h.to_ctrl.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
        if !self.controller.is_null() {
            for (id, v) in moved {
                // SAFETY: as in `params`.
                unsafe {
                    (vt::<ControllerVt>(self.controller).set)(self.controller, id, v);
                }
            }
        }
        if h.restart.load(Relaxed) {
            let now = self.run.lock().ok().filter(|r| r.active).map(|r| (r.rate, r.offline));
            if let Some((rate, offline)) = now {
                self.activate(rate, offline);
            }
        }
        let waiting = h.edits.lock().map(|q| !q.is_empty()).unwrap_or(false);
        if stalled && waiting {
            if let Ok(mut r) = self.run.try_lock() {
                if r.active {
                    let r = &mut *r;
                    self.take_changes(r, true);
                    let mut context = Context { rate: r.rate as f64, ..Context::default() };
                    // SAFETY: the lock makes this the only caller; a call with no frames carries settings.
                    unsafe { self.call(r, 0, &mut context) };
                }
            }
        }
    }
}

impl Drop for Plug {
    fn drop(&mut self) {
        // SAFETY: nothing else can reach the plug-in any more; it is stopped and ended in the order the
        // standard asks, and our side is freed after it.
        unsafe {
            Frame::hide(self.frame);
            let (cv, pv) = (vt::<ComponentVt>(self.component), vt::<ProcessorVt>(self.processor));
            if self.run.get_mut().map(|r| r.active).unwrap_or(false) {
                (pv.set_processing)(self.processor, 0);
                (cv.set_active)(self.component, 0);
            }
            if !self.links.0.is_null() {
                (vt::<ConnectionVt>(self.links.0).disconnect)(self.links.0, self.links.1);
                (vt::<ConnectionVt>(self.links.1).disconnect)(self.links.1, self.links.0);
                let_go(self.links.0);
                let_go(self.links.1);
            }
            if !self.controller.is_null() {
                (vt::<ControllerVt>(self.controller).set_handler)(self.controller, std::ptr::null_mut());
                if self.separate {
                    (vt::<ControllerVt>(self.controller).terminate)(self.controller);
                }
                let_go(self.controller);
            }
            let_go(self.processor);
            (cv.terminate)(self.component);
            let_go(self.component);
            drop(Box::from_raw(self.side));
            Frame::free(self.frame);
        }
    }
}
