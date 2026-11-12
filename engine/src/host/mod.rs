//! Other makers' plug-ins of the CLAP and VST3 kinds, loaded and run inside our own channels and effect slots.
//!
//! One `Instance` is one loaded plug-in. The song holds it behind an `Arc`, so every snapshot of the song
//! (the playing one, the undo steps) points at the same live plug-in. The audio thread calls `process`;
//! everything else is called from the window's thread, which must also call `idle` regularly.

pub mod abi;
pub mod abi3;
mod view3;
mod vst3;

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use abi::*;

/// The most frames handed over in one call.
const MAX_FRAMES: usize = 4096;

/// What a plug-in file says about one of the plug-ins inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub instrument: bool,
}

/// One setting of a plug-in.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub stepped: bool,
    pub readonly: bool,
}

/// Something for the plug-in to hear during one call, `at` frames into it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ev {
    NoteOn { at: u32, key: u8, chan: u8, vel: f32 },
    NoteOff { at: u32, key: u8, chan: u8 },
    Midi { at: u32, data: [u8; 3] },
}

/// Where the song is, for plug-ins that follow the tempo.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clock {
    pub playing: bool,
    pub tempo: f64,
    pub beats: f64,
    pub sig: (u8, u8),
    /// The call comes from a render to a file, not from the sound card.
    pub offline: bool,
}

struct Loaded {
    _lib: libloading::Library,
    factory: *const Factory,
}

// SAFETY: the factory is a table the plug-in file promises to keep; it is only called from the window's thread.
unsafe impl Send for Loaded {}
unsafe impl Sync for Loaded {}

static LIBS: Mutex<Option<HashMap<PathBuf, Option<&'static Loaded>>>> = Mutex::new(None);

/// Load a plug-in file once; it stays loaded for as long as the program runs.
fn load(path: &Path) -> Option<&'static Loaded> {
    let mut g = LIBS.lock().ok()?;
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(l) = map.get(path) {
        return *l;
    }
    let made = (|| {
        // SAFETY: loading runs the file's own start-up code, as loading any plug-in must.
        let lib = unsafe { libloading::Library::new(path) }.ok()?;
        // SAFETY: the standard names this symbol and gives its layout.
        let entry: &Entry = unsafe { &**lib.get::<*const Entry>(b"clap_entry\0").ok()? };
        if entry.version.major < 1 {
            return None;
        }
        let cpath = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        // SAFETY: calls into the table just read, with a text that outlives them.
        let factory = unsafe {
            if !(entry.init?)(cpath.as_ptr()) {
                return None;
            }
            (entry.get_factory?)(FACTORY_ID.as_ptr() as *const c_char) as *const Factory
        };
        if factory.is_null() {
            return None;
        }
        Some(&*Box::leak(Box::new(Loaded { _lib: lib, factory })))
    })();
    map.insert(path.to_path_buf(), made);
    made
}

unsafe fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

fn field(raw: &[c_char]) -> String {
    let bytes: Vec<u8> = raw.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn is_vst3(path: &Path) -> bool {
    path.extension().is_some_and(|x| x.eq_ignore_ascii_case("vst3"))
}

/// The plug-ins inside one file.
pub fn inside(path: &Path) -> Vec<Found> {
    if is_vst3(path) {
        return vst3::inside(path);
    }
    let Some(l) = load(path) else { return vec![] };
    let mut out = Vec::new();
    // SAFETY: the factory's own functions, called as the standard describes.
    unsafe {
        let f = &*l.factory;
        let (Some(count), Some(desc)) = (f.count, f.descriptor) else { return out };
        for i in 0..count(l.factory) {
            let d = desc(l.factory, i);
            if d.is_null() {
                continue;
            }
            let d = &*d;
            let mut instrument = false;
            let mut feat = d.features;
            while !feat.is_null() && !(*feat).is_null() {
                instrument |= CStr::from_ptr(*feat).to_bytes() == b"instrument";
                feat = feat.add(1);
            }
            out.push(Found { path: path.to_path_buf(), id: text(d.id), name: text(d.name), vendor: text(d.vendor), instrument });
        }
    }
    out
}

/// The folders plug-ins of this kind are installed in.
pub fn folders() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(list) = std::env::var_os("CLAP_PATH") {
        v.extend(std::env::split_paths(&list));
    }
    if cfg!(windows) {
        for k in ["COMMONPROGRAMFILES", "LOCALAPPDATA"] {
            if let Some(d) = std::env::var_os(k) {
                let d = PathBuf::from(d);
                for kind in ["CLAP", "VST3"] {
                    v.push(if k == "LOCALAPPDATA" { d.join("Programs").join("Common").join(kind) } else { d.join(kind) });
                }
            }
        }
    } else {
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(&h).join(".clap"));
            v.push(PathBuf::from(h).join(".vst3"));
        }
        for kind in ["clap", "vst3"] {
            for at in ["/usr/lib", "/usr/local/lib", "/usr/lib64"] {
                v.push(Path::new(at).join(kind));
            }
        }
    }
    if let Ok(more) = EXTRA.lock() {
        v.extend(more.iter().cloned());
    }
    v
}

static EXTRA: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static KNOWN: Mutex<Option<Arc<Vec<Found>>>> = Mutex::new(None);

/// Look in this folder too (a search folder the user added).
pub fn add_folder(dir: &Path) {
    if let Ok(mut more) = EXTRA.lock() {
        if !more.iter().any(|d| d == dir) {
            more.push(dir.to_path_buf());
            forget();
        }
    }
}

/// Drop the list of installed plug-ins, so the next look scans the folders again. Files that failed the
/// last time are tried once more.
pub fn forget() {
    if let Ok(mut k) = KNOWN.lock() {
        *k = None;
    }
    RETRY.store(true, Relaxed);
}

/// A program that looks inside one plug-in file for us (`<program> --scan-plugin <file>`), and the file the
/// answers are remembered in. With these set, a plug-in that crashes or hangs while it is being looked at
/// only loses its own place in the list.
static SCANNER: Mutex<Option<(PathBuf, Option<PathBuf>)>> = Mutex::new(None);
static FAILED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static RETRY: AtomicBool = AtomicBool::new(false);

pub fn scan_with(program: &Path, remember_in: Option<&Path>) {
    if let Ok(mut s) = SCANNER.lock() {
        *s = Some((program.to_path_buf(), remember_in.map(Path::to_path_buf)));
    }
}

/// The files that could not be looked into at the last scan.
pub fn failed() -> Vec<PathBuf> {
    FAILED.lock().map(|f| f.clone()).unwrap_or_default()
}

fn one_line(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// What the scanning program prints for one file: a line per plug-in, then "ok".
pub fn scan_report(path: &Path) -> String {
    let mut out = String::new();
    for f in inside(path) {
        out += &format!("{}\t{}\t{}\t{}\n", one_line(&f.id), one_line(&f.name), one_line(&f.vendor), f.instrument as u8);
    }
    out + "ok\n"
}

fn read_report(path: &Path, text: &str) -> Option<Vec<Found>> {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.pop() != Some("ok") {
        return None;
    }
    lines
        .into_iter()
        .map(|l| {
            let mut p = l.split('\t');
            Some(Found { path: path.to_path_buf(), id: p.next()?.into(), name: p.next()?.into(), vendor: p.next()?.into(), instrument: p.next()? == "1" })
        })
        .collect()
}

/// Run the scanning program on one file. None when it crashed, hung or made no sense.
fn scan_apart(program: &Path, file: &Path) -> Option<Vec<Found>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program).arg("--scan-plugin").arg(file).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut pipe = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        pipe.read_to_string(&mut s).ok().map(|_| s)
    });
    let began = std::time::Instant::now();
    let done = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) if began.elapsed().as_secs() < 30 => std::thread::sleep(std::time::Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let text = reader.join().ok().flatten()?;
    done.then(|| read_report(file, &text)).flatten()
}

/// A file's age and size, to tell whether a remembered answer still holds.
fn stamp(file: &Path) -> String {
    let m = std::fs::metadata(file).ok();
    let age = m.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    format!("{age}:{}", m.map_or(0, |m| m.len()))
}

/// The remembered answers: file, stamp, and its plug-ins (None for a file that failed).
type Memory = HashMap<PathBuf, (String, Option<Vec<Found>>)>;

fn read_memory(text: &str) -> Memory {
    let mut out = Memory::new();
    let mut cur: Option<PathBuf> = None;
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix("F\t") {
            let mut p = rest.split('\t');
            let (Some(path), Some(st), Some(state)) = (p.next(), p.next(), p.next()) else { continue };
            let path = PathBuf::from(path);
            out.insert(path.clone(), (st.to_string(), (state == "ok").then(Vec::new)));
            cur = Some(path);
        } else if let (Some(rest), Some(path)) = (l.strip_prefix("P\t"), &cur) {
            let mut p = rest.split('\t');
            let (Some(id), Some(name), Some(vendor), Some(inst)) = (p.next(), p.next(), p.next(), p.next()) else { continue };
            if let Some((_, Some(list))) = out.get_mut(path) {
                list.push(Found { path: path.clone(), id: id.into(), name: name.into(), vendor: vendor.into(), instrument: inst == "1" });
            }
        }
    }
    out
}

fn write_memory(mem: &Memory) -> String {
    let mut files: Vec<_> = mem.iter().collect();
    files.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::new();
    for (path, (st, list)) in files {
        out += &format!("F\t{}\t{st}\t{}\n", one_line(&path.to_string_lossy()), if list.is_some() { "ok" } else { "bad" });
        for f in list.iter().flatten() {
            out += &format!("P\t{}\t{}\t{}\t{}\n", one_line(&f.id), one_line(&f.name), one_line(&f.vendor), f.instrument as u8);
        }
    }
    out
}

fn scan_all(program: &Path, remember_in: Option<&Path>) -> Vec<Found> {
    let retry = RETRY.swap(false, Relaxed);
    let old = remember_in.and_then(|p| std::fs::read_to_string(p).ok()).map(|t| read_memory(&t)).unwrap_or_default();
    let mut new = Memory::new();
    let mut failed = vec![];
    let mut out = vec![];
    for file in files() {
        let st = stamp(&file);
        let got = match old.get(&file) {
            Some((was, list)) if *was == st && (list.is_some() || !retry) => list.clone(),
            _ => scan_apart(program, &file),
        };
        match &got {
            Some(list) => out.extend(list.iter().cloned()),
            None => failed.push(file.clone()),
        }
        new.insert(file, (st, got));
    }
    if let Some(p) = remember_in {
        if new != old {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, write_memory(&new));
        }
    }
    if let Ok(mut f) = FAILED.lock() {
        *f = failed;
    }
    out
}

/// Every installed plug-in. The folders are scanned the first time this is asked.
pub fn known() -> Arc<Vec<Found>> {
    if let Some(k) = KNOWN.lock().ok().and_then(|k| k.clone()) {
        return k;
    }
    let scanner = SCANNER.lock().ok().and_then(|s| s.clone());
    let list = Arc::new(match scanner {
        Some((program, memory)) => scan_all(&program, memory.as_deref()),
        None => files().iter().flat_map(|f| inside(f)).collect::<Vec<_>>(),
    });
    if let Ok(mut k) = KNOWN.lock() {
        *k = Some(list.clone());
    }
    list
}

/// The parts of a wrapper block: a number, then pieces of (id, length, bytes).
fn pieces(state: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let mut at = 4;
    let mut out = Vec::new();
    while at + 12 <= state.len() {
        let id = u32::from_le_bytes(state[at..at + 4].try_into().ok()?);
        let size = u64::from_le_bytes(state[at + 4..at + 12].try_into().ok()?) as usize;
        at += 12;
        out.push((id, state.get(at..at.checked_add(size)?)?));
        at += size;
    }
    (at == state.len() && !out.is_empty()).then_some(out)
}

const P_ID: u32 = 52;
const P_STATE: u32 = 53;
const P_NAME: u32 = 54;
const P_PATH: u32 = 55;
const P_VENDOR: u32 = 56;

/// The wrapper block of a plug-in, for a project file: its name, where it was found, who made it, its
/// identifier and what it saved of itself. Given the block it was read from, only the saved part changes.
pub fn block(inst: &Instance, base: Option<&[u8]>) -> Vec<u8> {
    let state = inst.save().unwrap_or_default();
    let path = inst.found.path.to_string_lossy().into_owned();
    let mut parts: Vec<(u32, Vec<u8>)> = match base.and_then(pieces) {
        Some(p) if from_parts(&p).is_some_and(|(_, id)| id == inst.found.id) => p.into_iter().map(|(i, d)| (i, d.to_vec())).collect(),
        _ => vec![(P_ID, inst.found.id.clone().into_bytes()), (P_NAME, inst.found.name.clone().into_bytes()), (P_PATH, path.into_bytes()), (P_VENDOR, inst.found.vendor.clone().into_bytes())],
    };
    parts.retain(|p| p.0 != P_STATE);
    parts.push((P_STATE, state));
    let mut out = base.filter(|b| b.len() >= 4 && pieces(b).is_some()).map(|b| b[..4].to_vec()).unwrap_or_else(|| 10u32.to_le_bytes().to_vec());
    for (id, data) in parts {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&data);
    }
    out
}

/// The file and identifier a wrapper block names, when it is a plug-in of the kind we host.
fn from_parts(parts: &[(u32, &[u8])]) -> Option<(PathBuf, String)> {
    let text = |want: u32| parts.iter().find(|p| p.0 == want).map(|p| String::from_utf8_lossy(p.1).trim_end_matches('\0').to_string());
    let path = PathBuf::from(text(P_PATH)?);
    if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("clap")) && !is_vst3(&path) {
        return None;
    }
    Some((path, text(P_ID).unwrap_or_default()))
}

/// Open the plug-in a wrapper block names and hand it what it saved. Only installed plug-ins are opened:
/// the one with the block's identifier, wherever it lives on this machine.
pub fn from_block(state: &[u8]) -> Option<Arc<Instance>> {
    let parts = pieces(state)?;
    let (path, id) = from_parts(&parts)?;
    let known = known();
    let name = path.file_name();
    let hit = known.iter().find(|f| f.id == id && f.path == path).or_else(|| known.iter().find(|f| !id.is_empty() && f.id == id)).or_else(|| known.iter().find(|f| id.is_empty() && f.path.file_name() == name))?;
    let inst = Instance::open(&hit.path, &hit.id)?;
    if let Some((_, saved)) = parts.iter().find(|p| p.0 == P_STATE).filter(|p| !p.1.is_empty()) {
        inst.restore(saved);
    }
    Some(inst)
}

/// Every plug-in file under the install folders.
pub fn files() -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if is_vst3(&p) {
                // a folder of this name is one plug-in, with its code inside
                out.push(p);
            } else if p.is_dir() {
                if depth < 4 {
                    walk(&p, depth + 1, out);
                }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("clap")) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for d in folders() {
        walk(&d, 0, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

/// What the plug-in tells the host from any thread; picked up by `idle` and `process`.
struct Shared {
    callback: AtomicBool,
    restart: AtomicBool,
    flush: AtomicBool,
    rescan: AtomicBool,
    dirty: AtomicBool,
    gui_closed: AtomicBool,
    resize: Mutex<Option<(u32, u32)>>,
    timers: Mutex<Vec<(u32, u32, std::time::Instant)>>,
    next_timer: AtomicU32,
    fds: Mutex<Vec<(i32, u32)>>,
    /// Settings the plug-in changed by itself (its own window), for ours to show.
    from_plugin: Mutex<Vec<(u32, f64)>>,
}

unsafe fn shared<'a>(h: *const Host) -> &'a Shared {
    &*((*h).data as *const Shared)
}

unsafe extern "C" fn host_restart(h: *const Host) {
    shared(h).restart.store(true, Relaxed);
}
unsafe extern "C" fn host_process(_h: *const Host) {}
unsafe extern "C" fn host_callback(h: *const Host) {
    shared(h).callback.store(true, Relaxed);
}
unsafe extern "C" fn params_rescan(h: *const Host, _flags: u32) {
    shared(h).rescan.store(true, Relaxed);
}
unsafe extern "C" fn params_clear(_h: *const Host, _id: u32, _flags: u32) {}
unsafe extern "C" fn params_flush(h: *const Host) {
    shared(h).flush.store(true, Relaxed);
}
unsafe extern "C" fn state_dirty(h: *const Host) {
    shared(h).dirty.store(true, Relaxed);
}
thread_local! {
    /// How deep this thread is in work the standard gives to the audio thread.
    static AUDIO_ROLE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// While this lives, the thread answers as the audio thread. Whatever thread runs the sound is the audio
/// thread for that long; every other call is main-thread work.
struct AudioRole;

impl AudioRole {
    fn take() -> Self {
        AUDIO_ROLE.with(|r| r.set(r.get() + 1));
        AudioRole
    }
}

impl Drop for AudioRole {
    fn drop(&mut self) {
        AUDIO_ROLE.with(|r| r.set(r.get().saturating_sub(1)));
    }
}

unsafe extern "C" fn is_main(_h: *const Host) -> bool {
    AUDIO_ROLE.with(|r| r.get() == 0)
}
unsafe extern "C" fn is_audio(_h: *const Host) -> bool {
    AUDIO_ROLE.with(|r| r.get() > 0)
}
unsafe extern "C" fn timer_register(h: *const Host, period_ms: u32, id: *mut u32) -> bool {
    let s = shared(h);
    let n = s.next_timer.fetch_add(1, Relaxed);
    let Ok(mut t) = s.timers.lock() else { return false };
    t.push((n, period_ms.max(1), std::time::Instant::now()));
    if !id.is_null() {
        *id = n;
    }
    true
}
unsafe extern "C" fn timer_unregister(h: *const Host, id: u32) -> bool {
    let Ok(mut t) = shared(h).timers.lock() else { return false };
    let before = t.len();
    t.retain(|x| x.0 != id);
    t.len() != before
}
unsafe extern "C" fn fd_register(h: *const Host, fd: i32, flags: u32) -> bool {
    let Ok(mut f) = shared(h).fds.lock() else { return false };
    f.retain(|x| x.0 != fd);
    f.push((fd, flags));
    true
}
unsafe extern "C" fn fd_unregister(h: *const Host, fd: i32) -> bool {
    let Ok(mut f) = shared(h).fds.lock() else { return false };
    f.retain(|x| x.0 != fd);
    true
}
unsafe extern "C" fn gui_hints(_h: *const Host) {}
unsafe extern "C" fn gui_resize(h: *const Host, w: u32, h2: u32) -> bool {
    if let Ok(mut r) = shared(h).resize.lock() {
        *r = Some((w, h2));
    }
    true
}
unsafe extern "C" fn gui_show(_h: *const Host) -> bool {
    false
}
unsafe extern "C" fn gui_closed(h: *const Host, _destroyed: bool) {
    shared(h).gui_closed.store(true, Relaxed);
}

struct Tables {
    params: HostParams,
    state: HostState,
    thread: HostThreadCheck,
    timer: HostTimer,
    fd: HostFd,
    gui: HostGui,
}

// SAFETY: tables of functions, never written.
unsafe impl Sync for Tables {}

static TABLES: Tables = Tables {
    params: HostParams { rescan: Some(params_rescan), clear: Some(params_clear), request_flush: Some(params_flush) },
    state: HostState { mark_dirty: Some(state_dirty) },
    thread: HostThreadCheck { is_main: Some(is_main), is_audio: Some(is_audio) },
    timer: HostTimer { register: Some(timer_register), unregister: Some(timer_unregister) },
    fd: HostFd { register: Some(fd_register), modify: Some(fd_register), unregister: Some(fd_unregister) },
    gui: HostGui { resize_hints_changed: Some(gui_hints), request_resize: Some(gui_resize), request_show: Some(gui_show), request_hide: Some(gui_show), closed: Some(gui_closed) },
};

unsafe extern "C" fn host_extension(_h: *const Host, id: *const c_char) -> *const c_void {
    if id.is_null() {
        return std::ptr::null();
    }
    let id = CStr::from_ptr(id).to_bytes_with_nul();
    let t = &TABLES;
    if id == EXT_PARAMS {
        &t.params as *const _ as *const c_void
    } else if id == EXT_STATE {
        &t.state as *const _ as *const c_void
    } else if id == EXT_THREAD_CHECK {
        &t.thread as *const _ as *const c_void
    } else if id == EXT_TIMER {
        &t.timer as *const _ as *const c_void
    } else if id == EXT_FD && cfg!(unix) {
        &t.fd as *const _ as *const c_void
    } else if id == EXT_GUI {
        &t.gui as *const _ as *const c_void
    } else {
        std::ptr::null()
    }
}

/// The audio side: buffers and what state the plug-in's processing is in.
struct Proc {
    rate: f32,
    active: bool,
    processing: bool,
    /// Channel counts of the input and output ports.
    ins: Vec<u32>,
    outs: Vec<u32>,
    in_data: Vec<Vec<f32>>,
    out_data: Vec<Vec<f32>>,
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    in_bufs: Vec<AudioBuffer>,
    out_bufs: Vec<AudioBuffer>,
    events: Vec<AnyEvent>,
    steady: i64,
    /// Whether notes go over as notes of the standard, or as MIDI messages.
    midi_notes: bool,
}

unsafe extern "C" fn in_size(l: *const InputEvents) -> u32 {
    let list: &Vec<AnyEvent> = &*((*l).ctx as *const Vec<AnyEvent>);
    list.len() as u32
}
unsafe extern "C" fn in_get(l: *const InputEvents, index: u32) -> *const EventHeader {
    let list: &Vec<AnyEvent> = &*((*l).ctx as *const Vec<AnyEvent>);
    match list.get(index as usize) {
        Some(e) => e as *const AnyEvent as *const EventHeader,
        None => std::ptr::null(),
    }
}
unsafe extern "C" fn out_push(l: *const OutputEvents, ev: *const EventHeader) -> bool {
    if ev.is_null() || (*l).ctx.is_null() {
        return false;
    }
    if (*ev).space == 0 && (*ev).kind == EV_PARAM_VALUE {
        let p = &*(ev as *const EventParam);
        let s = &*((*l).ctx as *const Shared);
        if let Ok(mut q) = s.from_plugin.try_lock() {
            q.retain(|x| x.0 != p.param);
            q.push((p.param, p.value));
        }
    }
    true
}

pub struct Instance {
    pub found: Found,
    plugin: *const Plugin,
    host: *mut Host,
    shared: *mut Shared,
    _names: Vec<CString>,
    params: *const PluginParams,
    state: *const PluginState,
    gui: *const PluginGui,
    timer: *const PluginTimer,
    #[cfg_attr(not(unix), allow(dead_code))]
    fd: *const PluginFd,
    proc: Mutex<Proc>,
    /// Settings changed in our window, waiting for the next call.
    to_plugin: Mutex<Vec<(u32, f64)>>,
    calls: AtomicU64,
    seen_calls: AtomicU64,
    gui_open: AtomicBool,
    offline: AtomicBool,
    /// How many frames late its sound comes out, as it said when last started.
    latency: AtomicU32,
    pub has_note_in: bool,
    /// Set when the plug-in is of the VST3 kind; the fields above then stay empty.
    v3: Option<vst3::Plug>,
    /// The settings in the plug-in's own order as last listed, and the travel last sent to each, for
    /// clips and controllers that drive a setting by its place.
    table: Mutex<(Vec<Param>, Vec<f32>)>,
}

// SAFETY: the standard lets the audio thread call `process` while the main thread calls the rest; what we
// keep ourselves is behind locks and atomics.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Instance({})", self.found.id)
    }
}

impl Instance {
    /// Make one plug-in out of a file. `id` picks among several in the file; empty takes the first.
    pub fn open(path: &Path, id: &str) -> Option<Arc<Instance>> {
        let inst = Self::open_raw(path, id)?;
        inst.params();
        Some(inst)
    }

    fn open_raw(path: &Path, id: &str) -> Option<Arc<Instance>> {
        if is_vst3(path) {
            let (plug, found) = vst3::Plug::open(path, id)?;
            let none = std::ptr::null();
            let proc = Proc { rate: 0.0, active: false, processing: false, ins: vec![], outs: vec![], in_data: vec![], out_data: vec![], in_ptrs: vec![], out_ptrs: vec![], in_bufs: vec![], out_bufs: vec![], events: Vec::new(), steady: 0, midi_notes: false };
            return Some(Arc::new(Instance {
                found,
                plugin: none,
                host: std::ptr::null_mut(),
                shared: std::ptr::null_mut(),
                _names: vec![],
                params: none as *const PluginParams,
                state: none as *const PluginState,
                gui: none as *const PluginGui,
                timer: none as *const PluginTimer,
                fd: none as *const PluginFd,
                proc: Mutex::new(proc),
                to_plugin: Mutex::new(Vec::new()),
                calls: AtomicU64::new(0),
                seen_calls: AtomicU64::new(0),
                gui_open: AtomicBool::new(false),
                offline: AtomicBool::new(false),
                latency: AtomicU32::new(0),
                has_note_in: plug.has_note_in,
                v3: Some(plug),
                table: Mutex::new((Vec::new(), Vec::new())),
            }));
        }
        let found = inside(path).into_iter().find(|f| id.is_empty() || f.id == id)?;
        let l = load(path)?;
        let names: Vec<CString> = ["Breakfast Cereal DAW", "Breakfast Cereal", "", env!("CARGO_PKG_VERSION")].iter().filter_map(|s| CString::new(*s).ok()).collect();
        let shared = Box::into_raw(Box::new(Shared {
            callback: AtomicBool::new(false),
            restart: AtomicBool::new(false),
            flush: AtomicBool::new(false),
            rescan: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            gui_closed: AtomicBool::new(false),
            resize: Mutex::new(None),
            timers: Mutex::new(Vec::new()),
            next_timer: AtomicU32::new(1),
            fds: Mutex::new(Vec::new()),
            from_plugin: Mutex::new(Vec::new()),
        }));
        let host = Box::into_raw(Box::new(Host {
            version: VERSION,
            data: shared as *mut c_void,
            name: names[0].as_ptr(),
            vendor: names[1].as_ptr(),
            url: names[2].as_ptr(),
            host_version: names[3].as_ptr(),
            get_extension: Some(host_extension),
            request_restart: Some(host_restart),
            request_process: Some(host_process),
            request_callback: Some(host_callback),
        }));
        let free = |host: *mut Host, shared: *mut Shared| {
            // SAFETY: both came from Box::into_raw just above and nothing else holds them.
            unsafe {
                drop(Box::from_raw(host));
                drop(Box::from_raw(shared));
            }
        };
        let cid = CString::new(found.id.as_bytes()).ok()?;
        // SAFETY: the factory and plug-in tables are called as the standard describes, from this one thread.
        unsafe {
            let plugin = match (*l.factory).create {
                Some(create) => create(l.factory, host, cid.as_ptr()),
                None => std::ptr::null(),
            };
            if plugin.is_null() {
                free(host, shared);
                return None;
            }
            let p = &*plugin;
            if !p.init.is_some_and(|init| init(plugin)) {
                if let Some(d) = p.destroy {
                    d(plugin);
                }
                free(host, shared);
                return None;
            }
            let ext = |id: &[u8]| p.get_extension.map(|g| g(plugin, id.as_ptr() as *const c_char)).unwrap_or(std::ptr::null());
            let ports = ext(EXT_AUDIO_PORTS) as *const PluginAudioPorts;
            let count = |input: bool| {
                let mut v = Vec::new();
                if let Some(pt) = ports.as_ref() {
                    let n = pt.count.map(|c| c(plugin, input)).unwrap_or(0);
                    for i in 0..n {
                        let mut info: AudioPortInfo = std::mem::zeroed();
                        let ok = pt.get.is_some_and(|g| g(plugin, i, input, &mut info));
                        v.push(if ok { info.channels.min(16) } else { 0 });
                    }
                }
                v
            };
            let (ins, outs) = (count(true), count(false));
            let notes = ext(EXT_NOTE_PORTS) as *const PluginNotePorts;
            let mut has_note_in = false;
            let mut midi_notes = false;
            if let Some(np) = notes.as_ref() {
                if np.count.map(|c| c(plugin, true)).unwrap_or(0) > 0 {
                    has_note_in = true;
                    let mut info: NotePortInfo = std::mem::zeroed();
                    if np.get.is_some_and(|g| g(plugin, 0, true, &mut info)) {
                        midi_notes = info.supported & DIALECT_CLAP == 0 && info.supported & DIALECT_MIDI != 0;
                    }
                }
            }
            let proc = Proc { rate: 0.0, active: false, processing: false, ins, outs, in_data: vec![], out_data: vec![], in_ptrs: vec![], out_ptrs: vec![], in_bufs: vec![], out_bufs: vec![], events: Vec::with_capacity(256), steady: 0, midi_notes };
            Some(Arc::new(Instance {
                found,
                plugin,
                host,
                shared,
                _names: names,
                params: ext(EXT_PARAMS) as *const PluginParams,
                state: ext(EXT_STATE) as *const PluginState,
                gui: ext(EXT_GUI) as *const PluginGui,
                timer: ext(EXT_TIMER) as *const PluginTimer,
                fd: ext(EXT_FD) as *const PluginFd,
                proc: Mutex::new(proc),
                to_plugin: Mutex::new(Vec::new()),
                calls: AtomicU64::new(0),
                seen_calls: AtomicU64::new(0),
                gui_open: AtomicBool::new(false),
                offline: AtomicBool::new(false),
                latency: AtomicU32::new(0),
                has_note_in,
                v3: None,
                table: Mutex::new((Vec::new(), Vec::new())),
            }))
        }
    }

    fn shared(&self) -> &Shared {
        // SAFETY: lives as long as the instance.
        unsafe { &*self.shared }
    }

    /// Hand the plug-in to a render to a file, or back to the sound card.
    pub fn set_offline(&self, on: bool) {
        self.offline.store(on, Relaxed);
    }

    /// The rate the plug-in is running at, when it is.
    pub fn rate(&self) -> Option<f32> {
        if let Some(v) = &self.v3 {
            return v.rate();
        }
        self.proc.lock().ok().filter(|p| p.active).map(|p| p.rate)
    }

    /// Whether the plug-in takes sound in (an effect) or only gives it out.
    pub fn takes_sound(&self) -> bool {
        if let Some(v) = &self.v3 {
            return v.takes_sound();
        }
        self.proc.lock().map(|p| p.ins.first().is_some_and(|&c| c > 0)).unwrap_or(false)
    }

    /// A second plug-in of the same make with the same settings, for a copied channel or slot, so the copy
    /// does not share its sound and settings with the first.
    pub fn fork(&self) -> Option<Arc<Instance>> {
        let twin = Instance::open(&self.found.path, &self.found.id)?;
        if let Some(v) = &self.v3 {
            // hand over changes still waiting, so the saved state has them
            v.idle(true);
        }
        if let Some(state) = self.save() {
            twin.restore(&state);
        }
        // changes of ours still waiting to go over are not in that state yet
        let waiting: Vec<(u32, f64)> = self.to_plugin.lock().map(|q| q.clone()).unwrap_or_default();
        for (id, v) in waiting {
            twin.set(id, v);
        }
        if let Some(rate) = self.rate() {
            twin.activate(rate);
        }
        Some(twin)
    }

    /// Get ready to run at this rate. Window thread; cheap when nothing changes.
    pub fn activate(&self, rate: f32) -> bool {
        if let Some(v) = &self.v3 {
            return v.activate(rate, self.offline.load(Relaxed));
        }
        let Ok(mut p) = self.proc.lock() else { return false };
        if p.active && p.rate == rate && !self.shared().restart.load(Relaxed) {
            return true;
        }
        self.shared().restart.store(false, Relaxed);
        // SAFETY: the lock keeps the audio thread out while the plug-in is stopped and started.
        unsafe {
            let pl = &*self.plugin;
            if p.processing {
                if let Some(f) = pl.stop_processing {
                    let _role = AudioRole::take();
                    f(self.plugin);
                }
                p.processing = false;
            }
            if p.active {
                if let Some(f) = pl.deactivate {
                    f(self.plugin);
                }
                p.active = false;
            }
            let p = &mut *p;
            p.in_data = (0..p.ins.iter().sum::<u32>()).map(|_| vec![0.0; MAX_FRAMES]).collect();
            p.out_data = (0..p.outs.iter().sum::<u32>()).map(|_| vec![0.0; MAX_FRAMES]).collect();
            p.in_ptrs = p.in_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
            p.out_ptrs = p.out_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
            let bufs = |counts: &[u32], ptrs: &mut [*mut f32]| {
                let mut at = 0;
                counts
                    .iter()
                    .map(|&c| {
                        let b = AudioBuffer { data32: ptrs[at..].as_mut_ptr(), data64: std::ptr::null_mut(), channels: c, latency: 0, constant_mask: 0 };
                        at += c as usize;
                        b
                    })
                    .collect::<Vec<_>>()
            };
            p.in_bufs = bufs(&p.ins, &mut p.in_ptrs);
            p.out_bufs = bufs(&p.outs, &mut p.out_ptrs);
            p.rate = rate;
            p.active = pl.activate.is_some_and(|f| f(self.plugin, rate as f64, 1, MAX_FRAMES as u32));
            if p.active {
                let table = pl.get_extension.map(|f| f(self.plugin, EXT_LATENCY.as_ptr() as *const c_char) as *const PluginLatency);
                let late = table.and_then(|t| t.as_ref()).and_then(|t| t.get).map(|f| f(self.plugin)).unwrap_or(0);
                self.latency.store(late, Relaxed);
            }
            p.active
        }
    }

    /// How many frames late the plug-in's sound comes out (look-ahead and the like), for the mixer to make up.
    pub fn latency(&self) -> u32 {
        match &self.v3 {
            Some(v) => v.latency(),
            None => self.latency.load(Relaxed),
        }
    }

    /// Run the plug-in over `io` (sound in, sound out). Audio thread. False when it could not run, and then
    /// `io` is untouched.
    pub fn process(&self, io: &mut [[f32; 2]], evs: &[Ev], clock: &Clock, rate: f32) -> bool {
        // while a file is being rendered the plug-in belongs to the render; the sound card goes without
        if self.offline.load(Relaxed) != clock.offline {
            return false;
        }
        if let Some(v) = &self.v3 {
            let ran = v.process(io, evs, clock, rate);
            if ran {
                self.calls.fetch_add(1, Relaxed);
            }
            return ran;
        }
        let got = if clock.offline { self.proc.lock().ok() } else { self.proc.try_lock().ok() };
        let Some(mut guard) = got else { return false };
        let _role = AudioRole::take();
        let p = &mut *guard;
        if !p.active || p.rate != rate {
            return false;
        }
        // SAFETY: the plug-in is active, the lock makes this the only caller, and every pointer handed over
        // points into buffers that live in `p` for the length of the call.
        unsafe {
            let pl = &*self.plugin;
            if !p.processing {
                p.processing = pl.start_processing.is_none_or(|f| f(self.plugin));
                if !p.processing {
                    return false;
                }
            }
            let Some(run) = pl.process else { return false };
            let mut done = 0;
            let total = io.len();
            while done < total {
                let n = (total - done).min(MAX_FRAMES);
                let part = &mut io[done..done + n];
                // sound in: the first port takes the channel's or track's sound, the others silence
                let first = p.ins.first().copied().unwrap_or(0) as usize;
                for (c, buf) in p.in_data.iter_mut().enumerate() {
                    if c < first {
                        for (o, s) in buf[..n].iter_mut().zip(part.iter()) {
                            *o = if first == 1 { (s[0] + s[1]) * 0.5 } else if c < 2 { s[c] } else { 0.0 };
                        }
                    } else {
                        buf[..n].fill(0.0);
                    }
                }
                for b in p.out_data.iter_mut() {
                    b[..n].fill(0.0);
                }
                p.events.clear();
                if done == 0 {
                    if let Ok(mut q) = self.to_plugin.try_lock() {
                        for (id, value) in q.drain(..) {
                            let header = EventHeader { size: std::mem::size_of::<EventParam>() as u32, time: 0, space: 0, kind: EV_PARAM_VALUE, flags: 0 };
                            p.events.push(AnyEvent { param: EventParam { header, param: id, cookie: std::ptr::null_mut(), note_id: -1, port: -1, channel: -1, key: -1, value } });
                        }
                    }
                }
                for e in evs {
                    let at = match e {
                        Ev::NoteOn { at, .. } | Ev::NoteOff { at, .. } | Ev::Midi { at, .. } => *at as usize,
                    };
                    // an event belongs to the stretch it falls in (the last stretch takes what is left)
                    if at < done || (at >= done + n && done + n < total) {
                        continue;
                    }
                    let time = (at - done).min(n - 1) as u32;
                    let note = |kind: u16, key: u8, chan: u8, vel: f32| {
                        let header = EventHeader { size: std::mem::size_of::<EventNote>() as u32, time, space: 0, kind, flags: 0 };
                        AnyEvent { note: EventNote { header, note_id: -1, port: 0, channel: chan as i16, key: key as i16, velocity: vel as f64 } }
                    };
                    let midi = |data: [u8; 3]| {
                        let header = EventHeader { size: std::mem::size_of::<EventMidi>() as u32, time, space: 0, kind: EV_MIDI, flags: 0 };
                        AnyEvent { midi: EventMidi { header, port: 0, data } }
                    };
                    p.events.push(match *e {
                        Ev::NoteOn { key, chan, vel, .. } if p.midi_notes => midi([0x90 | (chan & 15), key & 127, ((vel * 127.0) as u8).clamp(1, 127)]),
                        Ev::NoteOff { key, chan, .. } if p.midi_notes => midi([0x80 | (chan & 15), key & 127, 0]),
                        Ev::NoteOn { key, chan, vel, .. } => note(EV_NOTE_ON, key, chan, vel.clamp(0.0, 1.0)),
                        Ev::NoteOff { key, chan, .. } => note(EV_NOTE_OFF, key, chan, 0.0),
                        Ev::Midi { data, .. } => midi(data),
                    });
                }
                p.events.sort_by_key(|e| e.header.time);
                let beats = clock.beats + done as f64 / rate as f64 * clock.tempo / 60.0;
                let transport = EventTransport {
                    header: EventHeader { size: std::mem::size_of::<EventTransport>() as u32, time: 0, space: 0, kind: 9, flags: 0 },
                    flags: TRANSPORT_HAS_TEMPO | TRANSPORT_HAS_BEATS | TRANSPORT_HAS_SIGNATURE | if clock.playing { TRANSPORT_PLAYING } else { 0 },
                    song_pos_beats: (beats * BEAT_ONE) as i64,
                    song_pos_seconds: 0,
                    tempo: clock.tempo,
                    tempo_inc: 0.0,
                    loop_start_beats: 0,
                    loop_end_beats: 0,
                    loop_start_seconds: 0,
                    loop_end_seconds: 0,
                    bar_start: 0,
                    bar_number: 0,
                    tsig_num: clock.sig.0.max(1) as u16,
                    tsig_denom: clock.sig.1.max(1) as u16,
                };
                let input = InputEvents { ctx: &mut p.events as *mut Vec<AnyEvent> as *mut c_void, size: Some(in_size), get: Some(in_get) };
                let output = OutputEvents { ctx: self.shared as *mut c_void, try_push: Some(out_push) };
                let call = Process {
                    steady_time: p.steady,
                    frames: n as u32,
                    transport: &transport,
                    audio_in: p.in_bufs.as_ptr(),
                    audio_out: p.out_bufs.as_mut_ptr(),
                    audio_in_count: p.in_bufs.len() as u32,
                    audio_out_count: p.out_bufs.len() as u32,
                    in_events: &input,
                    out_events: &output,
                };
                run(self.plugin, &call);
                p.steady += n as i64;
                let outs = p.outs.first().copied().unwrap_or(0) as usize;
                for (i, s) in part.iter_mut().enumerate() {
                    let l = if outs > 0 { p.out_data[0][i] } else { 0.0 };
                    let r = if outs > 1 { p.out_data[1][i] } else { l };
                    *s = [if l.is_finite() { l } else { 0.0 }, if r.is_finite() { r } else { 0.0 }];
                }
                done += n;
            }
        }
        self.calls.fetch_add(1, Relaxed);
        true
    }

    /// The plug-in's settings. Window thread.
    pub fn params(&self) -> Vec<Param> {
        let out = self.params_now();
        if let Ok(mut t) = self.table.lock() {
            if t.0 != out {
                t.1 = vec![f32::NAN; out.len()];
                t.0 = out.clone();
            }
        }
        out
    }

    /// How many settings the plug-in listed last time.
    pub fn param_count(&self) -> usize {
        self.table.lock().map(|t| t.0.len()).unwrap_or(0)
    }

    /// The name of the setting at place `k`.
    pub fn param_name(&self, k: usize) -> Option<String> {
        self.table.lock().ok()?.0.get(k).map(|p| p.name.clone())
    }

    /// Where the setting at place `k` stands, 0..1 of its run.
    pub fn travel(&self, k: usize) -> Option<f32> {
        let (id, min, max) = self.table.lock().ok()?.0.get(k).map(|p| (p.id, p.min, p.max))?;
        let v = self.value(id)?;
        Some(if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 })
    }

    /// Put the setting at place `k` at 0..1 of its run. Cheap when it is there already, so a clip may call
    /// it every block; safe from the sound thread.
    pub fn set_travel(&self, k: usize, v: f32) {
        let v = v.clamp(0.0, 1.0);
        let Ok(mut t) = self.table.lock() else { return };
        let Some((id, min, max, stepped, readonly)) = t.0.get(k).map(|p| (p.id, p.min, p.max, p.stepped, p.readonly)) else { return };
        if readonly || t.1.get(k).is_some_and(|last| (last - v).abs() < 1e-6) {
            return;
        }
        t.1[k] = v;
        drop(t);
        let x = min + (max - min) * v as f64;
        self.set_now(id, if stepped { x.round() } else { x });
    }

    fn params_now(&self) -> Vec<Param> {
        if let Some(v) = &self.v3 {
            return v.params();
        }
        let mut out = Vec::new();
        // SAFETY: the settings table of a live plug-in, called from the window's thread.
        unsafe {
            let Some(t) = self.params.as_ref() else { return out };
            let (Some(count), Some(get)) = (t.count, t.get_info) else { return out };
            for i in 0..count(self.plugin) {
                let mut info: ParamInfo = std::mem::zeroed();
                if get(self.plugin, i, &mut info) && info.flags & PARAM_HIDDEN == 0 {
                    out.push(Param { id: info.id, name: field(&info.name), module: field(&info.module), min: info.min, max: info.max, default: info.default, stepped: info.flags & PARAM_STEPPED != 0, readonly: info.flags & PARAM_READONLY != 0 });
                }
            }
        }
        out
    }

    pub fn value(&self, id: u32) -> Option<f64> {
        if let Some(v) = &self.v3 {
            return v.value(id);
        }
        // a change of ours that has not gone over yet is the value
        if let Some(v) = self.to_plugin.lock().ok().and_then(|q| q.iter().rev().find(|x| x.0 == id).map(|x| x.1)) {
            return Some(v);
        }
        let mut v = 0.0;
        // SAFETY: as in `params`.
        unsafe { self.params.as_ref()?.get_value?(self.plugin, id, &mut v).then_some(v) }
    }

    /// How the plug-in itself writes a value (with its unit).
    pub fn value_text(&self, id: u32, value: f64) -> Option<String> {
        if let Some(v) = &self.v3 {
            return v.value_text(id, value);
        }
        let mut buf = [0 as c_char; 128];
        // SAFETY: as in `params`; the buffer's size is handed over.
        unsafe { self.params.as_ref()?.value_to_text?(self.plugin, id, value, buf.as_mut_ptr(), buf.len() as u32).then(|| field(&buf)) }
    }

    /// Change a setting; it goes over with the next stretch of sound, or at the next `idle` when none runs.
    pub fn set(&self, id: u32, value: f64) {
        // a change by hand: whatever a clip sent last no longer stands
        if let Ok(mut t) = self.table.lock() {
            if let Some(k) = t.0.iter().position(|p| p.id == id) {
                t.1[k] = f32::NAN;
            }
        }
        self.set_now(id, value);
    }

    fn set_now(&self, id: u32, value: f64) {
        if let Some(v) = &self.v3 {
            return v.set(id, value);
        }
        if let Ok(mut q) = self.to_plugin.lock() {
            q.retain(|x| x.0 != id);
            q.push((id, value));
        }
    }

    /// Settings the plug-in changed by itself since the last call.
    pub fn changed(&self) -> Vec<(u32, f64)> {
        if let Some(v) = &self.v3 {
            return v.changed();
        }
        self.shared().from_plugin.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }

    /// Everything the plug-in wants kept with the song.
    pub fn save(&self) -> Option<Vec<u8>> {
        if let Some(v) = &self.v3 {
            return v.save();
        }
        unsafe extern "C" fn write(s: *const OStream, buffer: *const c_void, size: u64) -> i64 {
            let out = &mut *((*s).ctx as *mut Vec<u8>);
            out.extend_from_slice(std::slice::from_raw_parts(buffer as *const u8, size as usize));
            size as i64
        }
        let mut out: Vec<u8> = Vec::new();
        let stream = OStream { ctx: &mut out as *mut Vec<u8> as *mut c_void, write: Some(write) };
        // SAFETY: the state table of a live plug-in; the stream writes into `out`, which outlives the call.
        unsafe { self.state.as_ref()?.save?(self.plugin, &stream).then_some(out) }
    }

    pub fn restore(&self, state: &[u8]) -> bool {
        if let Some(v) = &self.v3 {
            return v.restore(state);
        }
        unsafe extern "C" fn read(s: *const IStream, buffer: *mut c_void, size: u64) -> i64 {
            let (data, at) = &mut *((*s).ctx as *mut (&[u8], usize));
            let n = (data.len() - *at).min(size as usize);
            std::ptr::copy_nonoverlapping(data[*at..].as_ptr(), buffer as *mut u8, n);
            *at += n;
            n as i64
        }
        let mut from = (state, 0usize);
        let stream = IStream { ctx: &mut from as *mut (&[u8], usize) as *mut c_void, read: Some(read) };
        // SAFETY: as in `save`.
        unsafe { self.state.as_ref().and_then(|t| t.load).is_some_and(|f| f(self.plugin, &stream)) }
    }

    /// Whether the plug-in says its state changed since the last ask.
    pub fn take_dirty(&self) -> bool {
        if let Some(v) = &self.v3 {
            return v.take_dirty();
        }
        self.shared().dirty.swap(false, Relaxed)
    }

    /// Whether the plug-in says its list of settings changed since the last ask.
    pub fn take_rescan(&self) -> bool {
        if let Some(v) = &self.v3 {
            return v.take_rescan();
        }
        self.shared().rescan.swap(false, Relaxed)
    }

    /// Housekeeping the plug-in asked for: its callbacks, timers, watched files, and settings changes when
    /// no sound is running. Window thread, every frame or so.
    pub fn idle(&self) {
        if let Some(v) = &self.v3 {
            let calls = self.calls.load(Relaxed);
            return v.idle(self.seen_calls.swap(calls, Relaxed) == calls);
        }
        let s = self.shared();
        // SAFETY: main-thread calls into a live plug-in.
        unsafe {
            if s.callback.swap(false, Relaxed) {
                if let Some(f) = (*self.plugin).on_main_thread {
                    f(self.plugin);
                }
            }
            if let Some(t) = self.timer.as_ref().and_then(|t| t.on_timer) {
                let now = std::time::Instant::now();
                let due: Vec<u32> = s
                    .timers
                    .lock()
                    .map(|mut l| {
                        l.iter_mut()
                            .filter(|x| now >= x.2)
                            .map(|x| {
                                x.2 = now + std::time::Duration::from_millis(x.1 as u64);
                                x.0
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                for id in due {
                    t(self.plugin, id);
                }
            }
            #[cfg(unix)]
            if let Some(on_fd) = self.fd.as_ref().and_then(|t| t.on_fd) {
                let fds: Vec<(i32, u32)> = s.fds.lock().map(|l| l.clone()).unwrap_or_default();
                for (fd, flags) in fds {
                    let mut p = PollFd { fd, events: if flags & 1 != 0 { 1 } else { 0 } | if flags & 2 != 0 { 4 } else { 0 }, revents: 0 };
                    if poll(&mut p, 1, 0) > 0 && p.revents != 0 {
                        let got = if p.revents & 1 != 0 { 1 } else { 0 } | if p.revents & 4 != 0 { 2 } else { 0 } | if p.revents & 0x18 != 0 { 4 } else { 0 };
                        on_fd(self.plugin, fd, got);
                    }
                }
            }
            // settings changes with no sound running go over by hand
            let calls = self.calls.load(Relaxed);
            let stalled = self.seen_calls.swap(calls, Relaxed) == calls;
            let waiting = self.to_plugin.lock().map(|q| !q.is_empty()).unwrap_or(false) || s.flush.load(Relaxed);
            if stalled && waiting {
                if let (Ok(mut p), Some(flush)) = (self.proc.try_lock(), self.params.as_ref().and_then(|t| t.flush)) {
                    s.flush.store(false, Relaxed);
                    p.events.clear();
                    if let Ok(mut q) = self.to_plugin.lock() {
                        for (id, value) in q.drain(..) {
                            let header = EventHeader { size: std::mem::size_of::<EventParam>() as u32, time: 0, space: 0, kind: EV_PARAM_VALUE, flags: 0 };
                            p.events.push(AnyEvent { param: EventParam { header, param: id, cookie: std::ptr::null_mut(), note_id: -1, port: -1, channel: -1, key: -1, value } });
                        }
                    }
                    let input = InputEvents { ctx: &mut p.events as *mut Vec<AnyEvent> as *mut c_void, size: Some(in_size), get: Some(in_get) };
                    let output = OutputEvents { ctx: self.shared as *mut c_void, try_push: Some(out_push) };
                    flush(self.plugin, &input, &output);
                }
            }
        }
    }

    /// Whether the plug-in has a window of its own that can float beside ours.
    pub fn has_window(&self) -> bool {
        if let Some(v) = &self.v3 {
            return v.has_window();
        }
        self.window_api().is_some()
    }

    fn window_api(&self) -> Option<&'static [u8]> {
        if self.v3.is_some() {
            return None;
        }
        let apis: &[&'static [u8]] = if cfg!(windows) { &[WINDOW_WIN32] } else { &[WINDOW_WAYLAND, WINDOW_X11] };
        // SAFETY: the window table of a live plug-in, on the window's thread.
        unsafe {
            let g = self.gui.as_ref()?;
            let ok = g.is_api_supported?;
            apis.iter().copied().find(|a| ok(self.plugin, a.as_ptr() as *const c_char, true))
        }
    }

    /// Show the plug-in's own window, floating. False when it has none it can show this way.
    pub fn show_window(&self, title: &str) -> bool {
        if let Some(v) = &self.v3 {
            guard_x_errors();
            return v.show_window(title);
        }
        if self.gui_open.load(Relaxed) && !self.shared().gui_closed.load(Relaxed) {
            return true;
        }
        let Some(api) = self.window_api() else { return false };
        guard_x_errors();
        // SAFETY: as in `window_api`.
        unsafe {
            let Some(g) = self.gui.as_ref() else { return false };
            if self.gui_open.swap(false, Relaxed) {
                if let Some(d) = g.destroy {
                    d(self.plugin);
                }
            }
            self.shared().gui_closed.store(false, Relaxed);
            if !g.create.is_some_and(|f| f(self.plugin, api.as_ptr() as *const c_char, true)) {
                return false;
            }
            if let (Some(f), Ok(t)) = (g.suggest_title, CString::new(title)) {
                f(self.plugin, t.as_ptr());
            }
            let shown = g.show.is_some_and(|f| f(self.plugin));
            if !shown {
                if let Some(d) = g.destroy {
                    d(self.plugin);
                }
            }
            self.gui_open.store(shown, Relaxed);
            shown
        }
    }

    pub fn hide_window(&self) {
        if let Some(v) = &self.v3 {
            return v.hide_window();
        }
        if self.v3.is_none() && self.gui_open.swap(false, Relaxed) {
            // SAFETY: as in `window_api`.
            unsafe {
                if let Some(g) = self.gui.as_ref() {
                    if let Some(h) = g.hide {
                        h(self.plugin);
                    }
                    if let Some(d) = g.destroy {
                        d(self.plugin);
                    }
                }
            }
        }
    }

    pub fn window_open(&self) -> bool {
        if let Some(v) = &self.v3 {
            return v.window_open();
        }
        self.v3.is_none() && self.gui_open.load(Relaxed) && !self.shared().gui_closed.load(Relaxed)
    }
}

#[cfg(unix)]
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[cfg(unix)]
extern "C" {
    fn poll(fds: *mut PollFd, count: std::ffi::c_ulong, timeout: i32) -> i32;
}

impl Drop for Instance {
    fn drop(&mut self) {
        if self.v3.is_some() {
            return;
        }
        self.hide_window();
        // SAFETY: nothing else can reach the plug-in any more; it is stopped in the order the standard asks.
        unsafe {
            let pl = &*self.plugin;
            if let Ok(p) = self.proc.get_mut() {
                if p.processing {
                    if let Some(f) = pl.stop_processing {
                        let _role = AudioRole::take();
                        f(self.plugin);
                    }
                }
                if p.active {
                    if let Some(f) = pl.deactivate {
                        f(self.plugin);
                    }
                }
            }
            if let Some(f) = pl.destroy {
                f(self.plugin);
            }
            drop(Box::from_raw(self.host));
            drop(Box::from_raw(self.shared));
        }
    }
}

/// A drawing error in a plug-in's window would end the whole program, as the X library's own handler exits.
/// Ours lets the program carry on; the plug-in's window may stay blank, the song is safe.
#[cfg(all(unix, not(target_os = "macos")))]
fn guard_x_errors() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    unsafe extern "C" fn quiet(_display: *mut c_void, _event: *mut c_void) -> i32 {
        0
    }
    ONCE.call_once(|| {
        // SAFETY: the X library's documented call; the library stays loaded for the life of the program.
        unsafe {
            if let Ok(lib) = libloading::Library::new("libX11.so.6") {
                type Handler = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;
                if let Ok(set) = lib.get::<unsafe extern "C" fn(Option<Handler>) -> Option<Handler>>(b"XSetErrorHandler\0") {
                    set(Some(quiet));
                }
                std::mem::forget(lib);
            }
        }
    });
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn guard_x_errors() {}

/// An instance as the song holds it: two are the same when they are the same live plug-in.
#[derive(Debug, Clone)]
pub struct Hosted(pub Arc<Instance>);

impl Hosted {
    /// A plug-in of its own for a copy. Falls back to sharing when the file has gone.
    pub fn fork(&self) -> Hosted {
        Hosted(self.0.fork().unwrap_or_else(|| self.0.clone()))
    }
}

impl PartialEq for Hosted {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
}

/// The test plug-in built beside the engine, when it has been built.
#[cfg(test)]
pub(crate) fn test_plugin() -> Option<PathBuf> {
    let name = if cfg!(windows) { "cereal_testplug.dll" } else { "libcereal_testplug.so" };
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.join("target");
    ["release", "debug"].iter().flat_map(|p| [target.join(p).join(name), target.join(p).join("deps").join(name)]).find(|p| p.is_file())
}

/// The test plug-in put into `dir` the way a user would have a plug-in of that kind ("clap" or "vst3").
#[cfg(test)]
pub(crate) fn test_install(dir: &Path, kind: &str) -> Option<PathBuf> {
    let built = test_plugin()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("test-tone.{kind}"));
    if kind == "vst3" && cfg!(not(windows)) {
        // a folder with the code some levels down
        let inner = path.join("Contents").join(format!("{}-linux", std::env::consts::ARCH));
        std::fs::create_dir_all(&inner).ok()?;
        std::fs::copy(&built, inner.join("test-tone.so")).ok()?;
    } else {
        std::fs::copy(&built, &path).ok()?;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_answers_are_remembered_and_read_back() {
        let file = PathBuf::from("/x/My Synth.clap");
        let found = vec![Found { path: file.clone(), id: "a.b".into(), name: "My\tSynth".into(), vendor: "".into(), instrument: true }];
        let report = format!("{}\t{}\t\t1\nok\n", "a.b", one_line("My\tSynth"));
        let back = read_report(&file, &report).unwrap();
        assert_eq!(back[0].name, "My Synth");
        assert_eq!((back[0].id.as_str(), back[0].instrument), ("a.b", true));
        // a report cut short by a crash is no answer
        assert!(read_report(&file, "a.b\tMy Synth\t\t1\n").is_none());
        assert!(read_report(&file, "").is_none());
        let mut mem = Memory::new();
        mem.insert(file.clone(), ("5:9".into(), Some(back.clone())));
        mem.insert(PathBuf::from("/x/broken.vst3"), ("1:2".into(), None));
        mem.insert(PathBuf::from("/x/empty.clap"), ("3:4".into(), Some(vec![])));
        assert_eq!(read_memory(&write_memory(&mem)), mem);
        let _ = found;
    }

    #[test]
    fn a_copy_gets_a_plugin_of_its_own_with_the_same_settings() {
        for kind in ["clap", "vst3"] {
            let dir = std::env::temp_dir().join(format!("cereal-fork-{kind}-{}", std::process::id()));
            let Some(path) = test_install(&dir, kind) else {
                eprintln!("test plug-in not built; skipped");
                return;
            };
            let a = Instance::open(&path, "").expect("opens");
            assert!(a.activate(44_100.0));
            let id = a.params()[0].id;
            a.set(id, 0.25);
            let b = a.fork().expect("forks");
            assert!(!Arc::ptr_eq(&a, &b));
            assert_eq!(b.rate(), Some(44_100.0), "{kind}");
            assert_eq!(b.value(id), Some(0.25), "{kind}");
            // moving one leaves the other alone
            b.set(id, 0.75);
            assert_eq!(a.value(id), Some(0.25), "{kind}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_vst3_plugin_is_loaded_played_set_and_saved() {
        let dir = std::env::temp_dir().join(format!("cereal-vst3-{}", std::process::id()));
        let Some(path) = test_install(&dir, "vst3") else {
            eprintln!("test plug-in not built; skipped");
            return;
        };
        let found = inside(&path);
        assert_eq!(found.len(), 1);
        assert!(found[0].instrument && found[0].name == "Test Tone" && found[0].vendor == "Breakfast Cereal");
        assert_eq!(found[0].id, "B7EAFA57CE4EA1007E57701100000003");
        let inst = Instance::open(&path, "").expect("opens");
        assert!(inst.has_note_in && inst.takes_sound() && !inst.has_window());
        let params = inst.params();
        assert_eq!(params.len(), 1);
        // settings of this kind run 0..1; the plug-in's own text says what that stands for
        assert_eq!((params[0].name.as_str(), params[0].max, params[0].default), ("Gain", 1.0, 0.5));
        assert_eq!(inst.value_text(params[0].id, 0.25).as_deref(), Some("50 %"));
        assert_eq!(inst.value(params[0].id), Some(0.5));

        let mut io = vec![[0.25f32; 2]; 512];
        let clock = Clock { playing: true, tempo: 120.0, beats: 0.0, sig: (4, 4), offline: false };
        assert!(!inst.process(&mut io, &[], &clock, 48_000.0));
        assert!(inst.activate(48_000.0));
        assert_eq!(inst.rate(), Some(48_000.0));
        assert!(inst.process(&mut io, &[], &clock, 48_000.0));
        assert!(io.iter().all(|s| (s[0] - 0.25).abs() < 1e-6));
        // a note half way in, by name and as a MIDI message; then off
        for on in [Ev::NoteOn { at: 256, key: 69, chan: 0, vel: 1.0 }, Ev::Midi { at: 256, data: [0x90, 69, 127] }] {
            let mut io = vec![[0.0f32; 2]; 512];
            inst.process(&mut io, &[on], &clock, 48_000.0);
            assert!(io[..256].iter().all(|s| s[0] == 0.0));
            assert!(io[256..].iter().any(|s| s[0].abs() > 0.1));
            let mut io = vec![[0.0f32; 2]; 512];
            inst.process(&mut io, &[Ev::NoteOff { at: 0, key: 69, chan: 0 }], &clock, 48_000.0);
            assert!(io.iter().all(|s| s[0] == 0.0));
        }

        // a setting goes over with the next stretch of sound, and is read back at once
        inst.set(params[0].id, 1.0);
        assert_eq!(inst.value(params[0].id), Some(1.0));
        let mut io = vec![[0.25f32; 2]; 64];
        inst.process(&mut io, &[], &clock, 48_000.0);
        assert!((io[10][1] - 0.5).abs() < 1e-6);
        // and by hand when no sound runs
        inst.set(params[0].id, 0.25);
        inst.idle();
        inst.idle();
        let state = inst.save().expect("has state");
        assert_eq!(state[..4], 8u32.to_le_bytes());
        assert_eq!(state[4..], 0.5f64.to_le_bytes());

        // a second one takes the state over; a longer stretch than one call holds is cut up
        let other = Instance::open(&path, "B7EAFA57CE4EA1007E57701100000003").expect("opens again");
        assert!(other.restore(&state));
        assert_eq!(other.value(params[0].id), Some(0.25));
        assert!(!other.restore(&[1, 2]));
        assert!(other.activate(44_100.0));
        let mut io = vec![[1.0f32; 2]; MAX_FRAMES * 2 + 100];
        assert!(other.process(&mut io, &[Ev::NoteOn { at: MAX_FRAMES as u32 + 5, key: 60, chan: 0, vel: 0.5 }], &clock, 44_100.0));
        assert!(io[..MAX_FRAMES].iter().all(|s| (s[0] - 0.5).abs() < 1e-6));
        assert!(io[MAX_FRAMES + 50..].iter().any(|s| (s[0] - 0.5).abs() > 0.01));
        // a render borrows it: the sound card's calls are turned away meanwhile
        other.set_offline(true);
        assert!(other.activate(44_100.0));
        let mut io = vec![[1.0f32; 2]; 64];
        assert!(!other.process(&mut io, &[], &clock, 44_100.0));
        assert!(other.process(&mut io, &[], &Clock { offline: true, ..clock }, 44_100.0));
        assert!(Instance::open(&path, "00000000000000000000000000000000").is_none());
        assert!(inside(Path::new("/nonexistent/thing.vst3")).is_empty());
        drop((inst, other));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Run every plug-in under the folders in CEREAL_PLUGIN_TRY: a check against other makers' work.
    #[test]
    #[ignore]
    fn plugins_of_other_makers_run() {
        let Some(dirs) = std::env::var_os("CEREAL_PLUGIN_TRY") else { return };
        for d in std::env::split_paths(&dirs) {
            add_folder(&d);
        }
        let known = known();
        assert!(!known.is_empty(), "nothing found");
        for f in known.iter() {
            let inst = Instance::open(&f.path, &f.id).unwrap_or_else(|| panic!("{} does not open", f.name));
            assert!(inst.activate(48_000.0), "{} does not start", f.name);
            let params = inst.params();
            let clock = Clock { playing: true, tempo: 120.0, beats: 0.0, sig: (4, 4), offline: false };
            let mut peak = 0.0f32;
            for block in 0..200 {
                let mut io = vec![[0.0f32; 2]; 512];
                if block == 0 {
                    io[0] = [0.9, 0.9];
                }
                let evs: &[Ev] = if block == 0 { &[Ev::NoteOn { at: 0, key: 60, chan: 0, vel: 0.8 }] } else if block == 100 { &[Ev::NoteOff { at: 0, key: 60, chan: 0 }] } else { &[] };
                assert!(inst.process(&mut io, evs, &clock, 48_000.0), "{} does not run", f.name);
                assert!(io.iter().all(|x| x[0].is_finite() && x[1].is_finite()), "{} gives bad numbers", f.name);
                if block > 0 {
                    peak = io.iter().fold(peak, |m, x| m.max(x[0].abs()).max(x[1].abs()));
                }
                inst.idle();
            }
            // a setting we move is heard by the plug-in, kept in what it saves, and comes back from there
            let moved = params.iter().find(|p| !p.readonly && !p.stepped && p.max > p.min && !p.name.to_lowercase().contains("program")).map(|p| (p.id, p.min + (p.max - p.min) * 0.3125));
            if let Some((id, v)) = moved {
                inst.set(id, v);
                let mut io = vec![[0.0f32; 2]; 512];
                assert!(inst.process(&mut io, &[], &clock, 48_000.0));
                inst.idle();
                let now = inst.value(id).unwrap_or(f64::NAN);
                assert!((now - v).abs() < 1e-3 * (1.0 + v.abs()), "{}: set {v}, reads {now}", f.name);
                eprintln!("  moved {:?}", params.iter().find(|p| p.id == id).map(|p| p.name.clone()));
            }
            let saved = inst.save();
            let text = params.first().and_then(|p| inst.value(p.id).and_then(|v| inst.value_text(p.id, v)));
            eprintln!("{:28} {:12} instrument {:5} notes {:5} sound in {:5} settings {:4} window {:5} saved {:?} peak {:.4} first {:?} {:?}", f.name, f.vendor, f.instrument, inst.has_note_in, inst.takes_sound(), params.len(), inst.has_window(), saved.as_ref().map(|s| s.len()), peak, params.first().map(|p| p.name.clone()), text);
            if let Some(s) = saved {
                let again = Instance::open(&f.path, &f.id).unwrap();
                assert!(again.restore(&s), "{} does not take its saved settings back", f.name);
                if let Some((id, v)) = moved {
                    // some only tell their settings half once sound runs
                    assert!(again.activate(48_000.0));
                    let mut io = vec![[0.0f32; 2]; 512];
                    again.process(&mut io, &[], &clock, 48_000.0);
                    again.idle();
                    let now = again.value(id).unwrap_or(f64::NAN);
                    // a synth's first settings are often stand-ins for the wheels, which no state keeps
                    if f.instrument && (now - v).abs() >= 1e-3 * (1.0 + v.abs()) {
                        eprintln!("  {} does not keep that one", f.name);
                    } else {
                        assert!((now - v).abs() < 1e-3 * (1.0 + v.abs()), "{}: saved {v}, restored {now}", f.name);
                    }
                }
            }
            assert!(peak > 1e-5, "{} is silent", f.name);
        }
    }

    #[test]
    fn a_plugin_is_loaded_played_set_and_saved() {
        let Some(path) = test_plugin() else {
            eprintln!("test plug-in not built; skipped");
            return;
        };
        let found = inside(&path);
        assert_eq!(found.len(), 1);
        assert!(found[0].instrument && found[0].name == "Test Tone");
        let inst = Instance::open(&path, "").expect("opens");
        assert!(inst.has_note_in && inst.takes_sound());
        let params = inst.params();
        assert_eq!(params.len(), 1);
        assert_eq!((params[0].name.as_str(), params[0].max), ("Gain", 2.0));
        assert_eq!(inst.value_text(params[0].id, 0.5).as_deref(), Some("50 %"));

        // not active yet: nothing runs and the sound is left alone
        let mut io = vec![[0.25f32; 2]; 512];
        let clock = Clock { playing: true, tempo: 120.0, beats: 0.0, sig: (4, 4), offline: false };
        assert!(!inst.process(&mut io, &[], &clock, 48_000.0));
        assert!(inst.activate(48_000.0));
        // sound in comes out as it is at gain 1
        assert!(inst.process(&mut io, &[], &clock, 48_000.0));
        assert!(io.iter().all(|s| (s[0] - 0.25).abs() < 1e-6));
        // a note half way in: silence before it, a tone after
        let mut io = vec![[0.0f32; 2]; 512];
        inst.process(&mut io, &[Ev::NoteOn { at: 256, key: 69, chan: 0, vel: 1.0 }], &clock, 48_000.0);
        assert!(io[..256].iter().all(|s| s[0] == 0.0));
        assert!(io[256..].iter().any(|s| s[0].abs() > 0.1));
        let mut io = vec![[0.0f32; 2]; 512];
        inst.process(&mut io, &[Ev::NoteOff { at: 0, key: 69, chan: 0 }], &clock, 48_000.0);
        assert!(io.iter().all(|s| s[0] == 0.0));

        // a setting goes over with the next stretch of sound
        inst.set(params[0].id, 2.0);
        assert_eq!(inst.value(params[0].id), Some(2.0));
        let mut io = vec![[0.25f32; 2]; 64];
        inst.process(&mut io, &[], &clock, 48_000.0);
        assert!((io[10][1] - 0.5).abs() < 1e-6);
        // and by hand when no sound runs
        inst.set(params[0].id, 0.5);
        inst.idle();
        inst.idle();
        let state = inst.save().expect("has state");
        assert_eq!(state, 0.5f64.to_le_bytes());

        // a second one takes the state over; a longer stretch than one call holds is cut up
        let other = Instance::open(&path, "org.breakfast-cereal.test-tone").expect("opens again");
        assert!(other.restore(&state));
        assert!(other.activate(44_100.0));
        let mut io = vec![[1.0f32; 2]; MAX_FRAMES * 2 + 100];
        assert!(other.process(&mut io, &[Ev::NoteOn { at: MAX_FRAMES as u32 + 5, key: 60, chan: 0, vel: 0.5 }], &clock, 44_100.0));
        assert!(io[..MAX_FRAMES].iter().all(|s| (s[0] - 0.5).abs() < 1e-6));
        assert!(io[MAX_FRAMES + 50..].iter().any(|s| (s[0] - 0.5).abs() > 0.01));
        assert!(Instance::open(&path, "no.such.plugin").is_none());
        assert!(inside(Path::new("/nonexistent/thing.clap")).is_empty());
    }
}
