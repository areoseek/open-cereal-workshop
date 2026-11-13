//! Plug-ins of the older single-table kind (the second generation of the common plug-in format): one
//! table of functions per plug-in and numbered requests both ways.
//!
//! The layouts and request numbers below are written out from public descriptions of the interface;
//! nothing of the maker's own kit is used.

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;

use super::view3::os::{Win, KIND};
use super::{Clock, Ev, Found, Param, MAX_FRAMES};

type Call = unsafe extern "C" fn(*mut Effect, i32, i32, isize, *mut c_void, f32) -> isize;

/// The table a plug-in hands over.
#[repr(C)]
pub struct Effect {
    magic: i32,
    dispatch: Option<Call>,
    _old_run: *const c_void,
    set_param: Option<unsafe extern "C" fn(*mut Effect, i32, f32)>,
    get_param: Option<unsafe extern "C" fn(*mut Effect, i32) -> f32>,
    programs: i32,
    params: i32,
    ins: i32,
    outs: i32,
    flags: i32,
    _host1: isize,
    /// Ours to use: the way back from a request to the `Shared` of its plug-in.
    host2: isize,
    delay: i32,
    _quality: [i32; 2],
    _ratio: f32,
    _object: *mut c_void,
    _user: *mut c_void,
    unique: i32,
    _version: i32,
    run: Option<unsafe extern "C" fn(*mut Effect, *mut *mut f32, *mut *mut f32, i32)>,
    _run64: *const c_void,
    _future: [u8; 56],
}

const MAGIC: i32 = 0x5673_7450;
const HAS_EDITOR: i32 = 1;
const CAN_RUN: i32 = 1 << 4;
const CHUNKS: i32 = 1 << 5;
const IS_SYNTH: i32 = 1 << 8;

// requests to the plug-in
const OPEN: i32 = 0;
const CLOSE: i32 = 1;
const SET_PROGRAM: i32 = 2;
const GET_PROGRAM: i32 = 3;
const PROGRAM_NAME: i32 = 5;
const PARAM_LABEL: i32 = 6;
const PARAM_DISPLAY: i32 = 7;
const PARAM_NAME: i32 = 8;
const SET_RATE: i32 = 10;
const SET_BLOCK: i32 = 11;
const MAINS: i32 = 12;
const EDIT_RECT: i32 = 13;
const EDIT_OPEN: i32 = 14;
const EDIT_CLOSE: i32 = 15;
const EDIT_IDLE: i32 = 19;
const GET_CHUNK: i32 = 23;
const SET_CHUNK: i32 = 24;
const EVENTS: i32 = 25;
const PROGRAM_NAME_AT: i32 = 29;
const CATEGORY: i32 = 35;
const EFFECT_NAME: i32 = 45;
const VENDOR: i32 = 47;
const PRODUCT: i32 = 48;
const BEGIN_SET_PROGRAM: i32 = 67;
const END_SET_PROGRAM: i32 = 68;
const START: i32 = 71;
const STOP: i32 = 72;
const PRECISION: i32 = 77;

/// A file of this category holds a whole shelf of plug-ins behind one door; those are left alone.
const SHELL: isize = 10;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct TimeInfo {
    sample_pos: f64,
    rate: f64,
    nanos: f64,
    beats: f64,
    tempo: f64,
    bar_start: f64,
    cycle: [f64; 2],
    sig: [i32; 2],
    smpte: [i32; 2],
    to_clock: i32,
    flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MidiEvent {
    kind: i32,
    size: i32,
    delta: i32,
    flags: i32,
    length: i32,
    offset: i32,
    data: [u8; 4],
    detune: i8,
    off_vel: u8,
    _rest: [u8; 2],
}

const MAX_EVENTS: usize = 512;

#[repr(C)]
struct Events {
    count: i32,
    _reserved: isize,
    list: [*mut MidiEvent; MAX_EVENTS],
}

#[repr(C)]
struct Rect {
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
}

/// What the plug-in tells us from any thread.
struct Shared {
    time: UnsafeCell<TimeInfo>,
    moved: Mutex<Vec<(u32, f64)>>,
    dirty: AtomicBool,
    rescan: AtomicBool,
    resize: Mutex<Option<(i32, i32)>>,
    rate: AtomicU32,
    offline: AtomicBool,
}

unsafe fn put(ptr: *mut c_void, text: &[u8]) {
    if !ptr.is_null() {
        std::ptr::copy_nonoverlapping(text.as_ptr(), ptr as *mut u8, text.len());
        *(ptr as *mut u8).add(text.len()) = 0;
    }
}

/// The plug-in asking us for something.
unsafe extern "C" fn master(e: *mut Effect, what: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
    let sh = if e.is_null() { None } else { ((*e).host2 as *const Shared).as_ref() };
    match what {
        // a setting moved in the plug-in's own window
        0 => {
            if let Some(s) = sh {
                if let Ok(mut q) = s.moved.lock() {
                    q.retain(|m| m.0 != index as u32);
                    q.push((index as u32, opt as f64));
                }
                s.dirty.store(true, Relaxed);
            }
            0
        }
        // which revision of the interface we speak
        1 => 2400,
        6 => 1,
        7 => sh.map(|s| s.time.get() as isize).unwrap_or(0),
        13 => {
            if let Some(s) = sh {
                s.rescan.store(true, Relaxed);
            }
            1
        }
        15 => {
            if let Some(s) = sh {
                if let Ok(mut r) = s.resize.lock() {
                    *r = Some((index, value as i32));
                }
            }
            1
        }
        16 => sh.map(|s| f32::from_bits(s.rate.load(Relaxed)) as isize).filter(|r| *r > 0).unwrap_or(44_100),
        17 => MAX_FRAMES as isize,
        // 2: the sound card is waiting, 4: a render to a file
        23 => match sh {
            Some(s) if s.offline.load(Relaxed) => 4,
            _ => 2,
        },
        // settings are neither being written down nor played back
        24 => 1,
        32 | 33 => {
            put(ptr, b"Open Cereal Workshop");
            1
        }
        34 => 1000,
        37 => {
            let asked = if ptr.is_null() { &b""[..] } else { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char).to_bytes() };
            const YES: [&[u8]; 7] = [b"sendVstEvents", b"sendVstMidiEvent", b"sendVstTimeInfo", b"receiveVstEvents", b"receiveVstMidiEvent", b"sizeWindow", b"startStopProcess"];
            YES.contains(&asked) as isize
        }
        38 => 1,
        42 => {
            if let Some(s) = sh {
                s.dirty.store(true, Relaxed);
                s.rescan.store(true, Relaxed);
            }
            1
        }
        43 | 44 => 1,
        _ => 0,
    }
}

struct Lib {
    _lib: libloading::Library,
    entry: unsafe extern "C" fn(Call) -> *mut Effect,
}

static LIBS: Mutex<Option<HashMap<PathBuf, Option<&'static Lib>>>> = Mutex::new(None);

/// Load a plug-in file once; it stays loaded for as long as the program runs.
fn load(path: &Path) -> Option<&'static Lib> {
    let mut g = LIBS.lock().ok()?;
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(l) = map.get(path) {
        return *l;
    }
    let made = (|| {
        // SAFETY: loading runs the file's own start-up code, as loading any plug-in must.
        let lib = unsafe { libloading::Library::new(path) }.ok()?;
        // SAFETY: the interface names these doors and gives their shape; the older name is tried second.
        let entry = unsafe {
            let got = lib.get::<unsafe extern "C" fn(Call) -> *mut Effect>(b"VSTPluginMain\0").or_else(|_| lib.get(b"main\0")).ok()?;
            *got
        };
        Some(&*Box::leak(Box::new(Lib { _lib: lib, entry })))
    })();
    map.insert(path.to_path_buf(), made);
    made
}

/// Whether a file name is one this kind of plug-in comes in on this system.
pub fn is_file(path: &Path) -> bool {
    path.extension().is_some_and(|x| x.eq_ignore_ascii_case(if cfg!(windows) { "dll" } else { "so" }))
}

/// The identifier we give a plug-in of this kind: its four-letter number.
pub fn ident(unique: u32) -> String {
    format!("vst2:{unique:08x}")
}

struct Run {
    rate: f32,
    active: bool,
    offline: bool,
    was_playing: bool,
    in_data: Vec<Vec<f32>>,
    out_data: Vec<Vec<f32>>,
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    midi: Vec<MidiEvent>,
    events: Box<Events>,
}

pub struct Plug {
    fx: *mut Effect,
    shared: Box<Shared>,
    run: Mutex<Run>,
    win: Mutex<Option<Win>>,
    /// The values the settings had when the plug-in was made.
    defaults: Vec<f32>,
}

// SAFETY: the interface lets the audio thread run the plug-in while the window thread asks it things;
// what we keep ourselves is behind locks and atomics, and the time record is written by the audio thread only.
unsafe impl Send for Plug {}
unsafe impl Sync for Plug {}

pub fn inside(path: &Path) -> Vec<Found> {
    Plug::open(path, "").map(|(_, f)| vec![f]).unwrap_or_default()
}

impl Plug {
    unsafe fn ask(&self, what: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        match (*self.fx).dispatch {
            Some(d) => d(self.fx, what, index, value, ptr, opt),
            None => 0,
        }
    }

    /// A text the plug-in writes into a buffer of ours.
    unsafe fn text(&self, what: i32, index: i32) -> String {
        let mut buf = [0u8; 512];
        self.ask(what, index, 0, buf.as_mut_ptr() as *mut c_void, 0.0);
        let end = buf.iter().position(|&b| b == 0).unwrap_or(0).min(255);
        String::from_utf8_lossy(&buf[..end]).trim().to_string()
    }

    pub fn open(path: &Path, _id: &str) -> Option<(Plug, Found)> {
        let lib = load(path)?;
        // SAFETY: the door of a loaded plug-in file, called as the interface describes; the table it
        // returns is checked before anything in it is used.
        unsafe {
            let fx = (lib.entry)(master);
            if fx.is_null() || (*fx).magic != MAGIC || (*fx).dispatch.is_none() {
                return None;
            }
            let shared = Box::new(Shared {
                time: UnsafeCell::new(TimeInfo { rate: 44_100.0, tempo: 120.0, sig: [4, 4], ..TimeInfo::default() }),
                moved: Mutex::new(Vec::new()),
                dirty: AtomicBool::new(false),
                rescan: AtomicBool::new(false),
                resize: Mutex::new(None),
                rate: AtomicU32::new(44_100f32.to_bits()),
                offline: AtomicBool::new(false),
            });
            (*fx).host2 = &*shared as *const Shared as isize;
            let events = Box::new(Events { count: 0, _reserved: 0, list: [std::ptr::null_mut(); MAX_EVENTS] });
            let run = Run { rate: 0.0, active: false, offline: false, was_playing: false, in_data: vec![], out_data: vec![], in_ptrs: vec![], out_ptrs: vec![], midi: Vec::with_capacity(MAX_EVENTS), events };
            let mut plug = Plug { fx, shared, run: Mutex::new(run), win: Mutex::new(None), defaults: vec![] };
            plug.ask(OPEN, 0, 0, std::ptr::null_mut(), 0.0);
            let category = plug.ask(CATEGORY, 0, 0, std::ptr::null_mut(), 0.0);
            if category == SHELL {
                return None;
            }
            plug.ask(SET_RATE, 0, 0, std::ptr::null_mut(), 44_100.0);
            plug.ask(SET_BLOCK, 0, MAX_FRAMES as isize, std::ptr::null_mut(), 0.0);
            // some only put their settings in order once a program is picked
            if (*fx).programs > 0 {
                plug.ask(SET_PROGRAM, 0, 0, std::ptr::null_mut(), 0.0);
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let name = [plug.text(EFFECT_NAME, 0), plug.text(PRODUCT, 0), stem].into_iter().find(|n| !n.is_empty()).unwrap_or_default();
            let found = Found { path: path.to_path_buf(), id: ident((*fx).unique as u32), name, vendor: plug.text(VENDOR, 0), instrument: (*fx).flags & IS_SYNTH != 0 || category == 2 };
            if let Some(get) = (*fx).get_param {
                plug.defaults = (0..(*fx).params.max(0)).map(|i| get(fx, i)).collect();
            }
            Some((plug, found))
        }
    }

    pub fn rate(&self) -> Option<f32> {
        self.run.lock().ok().filter(|r| r.active).map(|r| r.rate)
    }

    pub fn takes_sound(&self) -> bool {
        // SAFETY: a field of the live table.
        unsafe { (*self.fx).ins > 0 && (*self.fx).flags & IS_SYNTH == 0 }
    }

    pub fn activate(&self, rate: f32, offline: bool) -> bool {
        let Ok(mut r) = self.run.lock() else { return false };
        if r.active && r.rate == rate && r.offline == offline {
            return true;
        }
        let none = std::ptr::null_mut();
        // SAFETY: the lock keeps the audio thread out while the plug-in is stopped and started.
        unsafe {
            if (*self.fx).run.is_none() || (*self.fx).flags & CAN_RUN == 0 {
                return false;
            }
            if r.active {
                self.ask(STOP, 0, 0, none, 0.0);
                self.ask(MAINS, 0, 0, none, 0.0);
                r.active = false;
            }
            self.shared.rate.store(rate.to_bits(), Relaxed);
            self.shared.offline.store(offline, Relaxed);
            self.ask(SET_RATE, 0, 0, none, rate);
            self.ask(SET_BLOCK, 0, MAX_FRAMES as isize, none, 0.0);
            self.ask(PRECISION, 0, 0, none, 0.0);
            let r = &mut *r;
            r.in_data = (0..(*self.fx).ins.max(0)).map(|_| vec![0.0; MAX_FRAMES]).collect();
            r.out_data = (0..(*self.fx).outs.max(0)).map(|_| vec![0.0; MAX_FRAMES]).collect();
            r.in_ptrs = r.in_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
            r.out_ptrs = r.out_data.iter_mut().map(|v| v.as_mut_ptr()).collect();
            self.ask(MAINS, 0, 1, none, 0.0);
            self.ask(START, 0, 0, none, 0.0);
            r.rate = rate;
            r.offline = offline;
            r.active = true;
        }
        true
    }

    pub fn process(&self, io: &mut [[f32; 2]], evs: &[Ev], clock: &Clock, rate: f32) -> bool {
        let got = if clock.offline { self.run.lock().ok() } else { self.run.try_lock().ok() };
        let Some(mut guard) = got else { return false };
        let r = &mut *guard;
        if !r.active || r.rate != rate || r.offline != clock.offline {
            return false;
        }
        let per_frame = if clock.playing { clock.tempo / 60.0 / rate as f64 } else { 0.0 };
        let bar = (clock.sig.0.max(1) as f64) * 4.0 / (clock.sig.1.max(1) as f64);
        let total = io.len();
        let mut done = 0;
        // SAFETY: the plug-in is running, the lock makes this the only caller, and every pointer handed
        // over points into buffers that live in `r` for the length of the call.
        unsafe {
            let Some(run) = (*self.fx).run else { return false };
            let ins = r.in_data.len();
            while done < total {
                let n = (total - done).min(MAX_FRAMES);
                let part = &mut io[done..done + n];
                for (c, buf) in r.in_data.iter_mut().enumerate() {
                    for (o, s) in buf[..n].iter_mut().zip(part.iter()) {
                        *o = if ins == 1 { (s[0] + s[1]) * 0.5 } else if c < 2 { s[c] } else { 0.0 };
                    }
                }
                for buf in r.out_data.iter_mut() {
                    buf[..n].fill(0.0);
                }
                r.midi.clear();
                for e in evs {
                    let (at, data) = match *e {
                        Ev::NoteOn { at, key, chan, vel } => (at, [0x90 | (chan & 15), key & 127, ((vel * 127.0).round() as i32).clamp(1, 127) as u8]),
                        Ev::NoteOff { at, key, chan } => (at, [0x80 | (chan & 15), key & 127, 0]),
                        Ev::Midi { at, data } => (at, data),
                    };
                    let at = at as usize;
                    if at < done || at >= done + n || r.midi.len() >= MAX_EVENTS {
                        continue;
                    }
                    r.midi.push(MidiEvent { kind: 1, size: std::mem::size_of::<MidiEvent>() as i32, delta: (at - done) as i32, flags: 0, length: 0, offset: 0, data: [data[0], data[1], data[2], 0], detune: 0, off_vel: 0, _rest: [0; 2] });
                }
                if !r.midi.is_empty() {
                    r.events.count = r.midi.len() as i32;
                    for (slot, m) in r.events.list.iter_mut().zip(r.midi.iter_mut()) {
                        *slot = m;
                    }
                    self.ask(EVENTS, 0, 0, &mut *r.events as *mut Events as *mut c_void, 0.0);
                }
                let beats = clock.beats + done as f64 * per_frame;
                let t = &mut *self.shared.time.get();
                t.sample_pos = if clock.tempo > 0.0 { beats * 60.0 / clock.tempo * rate as f64 } else { 0.0 };
                t.rate = rate as f64;
                t.beats = beats;
                t.tempo = clock.tempo;
                t.bar_start = (beats / bar).floor() * bar;
                t.sig = [clock.sig.0.max(1) as i32, clock.sig.1.max(1) as i32];
                // moving, and which of the fields above are filled in: beats, tempo, bar, signature
                t.flags = if clock.playing { 2 } else { 0 } | if clock.playing != r.was_playing { 1 } else { 0 } | 1 << 9 | 1 << 10 | 1 << 11 | 1 << 13;
                r.was_playing = clock.playing;
                run(self.fx, r.in_ptrs.as_mut_ptr(), r.out_ptrs.as_mut_ptr(), n as i32);
                let outs = r.out_data.len();
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

    fn count(&self) -> i32 {
        // SAFETY: a field of the live table.
        unsafe { (*self.fx).params.max(0) }
    }

    pub fn params(&self) -> Vec<Param> {
        // SAFETY: window-thread requests to a live plug-in.
        (0..self.count())
            .map(|i| {
                let name = unsafe { self.text(PARAM_NAME, i) };
                Param { id: i as u32, name: if name.is_empty() { format!("Setting {}", i + 1) } else { name }, module: String::new(), min: 0.0, max: 1.0, default: self.defaults.get(i as usize).copied().unwrap_or(0.0) as f64, stepped: false, readonly: false }
            })
            .collect()
    }

    pub fn value(&self, id: u32) -> Option<f64> {
        if id as i32 >= self.count() {
            return None;
        }
        // SAFETY: a table function of a live plug-in; any thread may read a setting.
        unsafe { (*self.fx).get_param.map(|get| get(self.fx, id as i32) as f64) }
    }

    /// The plug-in only words the value a setting has right now.
    pub fn value_text(&self, id: u32, value: f64) -> Option<String> {
        let now = self.value(id)?;
        if (now - value).abs() > 1e-5 {
            return None;
        }
        // SAFETY: window-thread requests to a live plug-in.
        let (shown, unit) = unsafe { (self.text(PARAM_DISPLAY, id as i32), self.text(PARAM_LABEL, id as i32)) };
        (!shown.is_empty()).then(|| if unit.is_empty() { shown } else { format!("{shown} {unit}") })
    }

    pub fn set(&self, id: u32, value: f64) {
        if (id as i32) < self.count() {
            // SAFETY: a table function of a live plug-in; any thread may move a setting.
            unsafe {
                if let Some(set) = (*self.fx).set_param {
                    set(self.fx, id as i32, value.clamp(0.0, 1.0) as f32);
                }
            }
        }
    }

    pub fn changed(&self) -> Vec<(u32, f64)> {
        self.shared.moved.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default()
    }

    /// What the plug-in holds, laid out the way project files keep it: a marker, the kind (13: one lump
    /// the plug-in wrote itself, 5: the travel of every setting), then the lump's length and the program
    /// number, or the program number and the settings followed by the names of the programs.
    pub fn save(&self) -> Option<Vec<u8>> {
        let mut out = vec![0xf7, 0xff, 0xff, 0xff];
        // SAFETY: window-thread requests to a live plug-in; the lump it points at is copied at once.
        unsafe {
            let program = self.ask(GET_PROGRAM, 0, 0, std::ptr::null_mut(), 0.0).max(0) as u32;
            if (*self.fx).flags & CHUNKS != 0 {
                let mut data: *mut u8 = std::ptr::null_mut();
                let n = self.ask(GET_CHUNK, 0, 0, &mut data as *mut *mut u8 as *mut c_void, 0.0);
                if n <= 0 || data.is_null() || n > u32::MAX as isize {
                    return None;
                }
                out.push(13);
                out.extend_from_slice(&(n as u32).to_le_bytes());
                out.extend_from_slice(&program.to_le_bytes());
                out.extend_from_slice(std::slice::from_raw_parts(data, n as usize));
            } else {
                let get = (*self.fx).get_param?;
                out.push(5);
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&program.to_le_bytes());
                out.extend_from_slice(&(self.count() as u32).to_le_bytes());
                for i in 0..self.count() {
                    out.extend_from_slice(&get(self.fx, i).to_le_bytes());
                }
                let programs = (*self.fx).programs.clamp(0, 4096);
                out.extend_from_slice(&(programs as u32).to_le_bytes());
                for i in 0..programs {
                    let mut buf = [0u8; 512];
                    let known = self.ask(PROGRAM_NAME_AT, i, -1, buf.as_mut_ptr() as *mut c_void, 0.0) != 0;
                    if !known && i as u32 == program {
                        self.ask(PROGRAM_NAME, 0, 0, buf.as_mut_ptr() as *mut c_void, 0.0);
                    }
                    let end = buf.iter().position(|&b| b == 0).unwrap_or(0).min(24);
                    let mut name = [0u8; 25];
                    name[..end].copy_from_slice(&buf[..end]);
                    out.extend_from_slice(&name);
                }
            }
        }
        Some(out)
    }

    pub fn restore(&self, state: &[u8]) -> bool {
        let Some(saved) = Saved::read(state) else { return false };
        // the plug-in is not run while it takes its state in
        let Ok(_quiet) = self.run.lock() else { return false };
        let none = std::ptr::null_mut();
        // SAFETY: window-thread requests to a live plug-in, with the audio thread kept out by the lock.
        unsafe {
            let pick = |program: u32| {
                if self.ask(GET_PROGRAM, 0, 0, none, 0.0) != program as isize && (program as i32) < (*self.fx).programs {
                    self.ask(BEGIN_SET_PROGRAM, 0, 0, none, 0.0);
                    self.ask(SET_PROGRAM, 0, program as isize, none, 0.0);
                    self.ask(END_SET_PROGRAM, 0, 0, none, 0.0);
                }
            };
            match saved {
                Saved::Lump { program, data } => {
                    if (*self.fx).flags & CHUNKS == 0 {
                        return false;
                    }
                    self.ask(SET_CHUNK, 0, data.len() as isize, data.as_ptr() as *mut c_void, 0.0);
                    pick(program);
                }
                Saved::Travel { program, values } => {
                    pick(program);
                    if let Some(set) = (*self.fx).set_param {
                        for (i, v) in values.iter().take(self.count() as usize).enumerate() {
                            set(self.fx, i as i32, *v);
                        }
                    }
                }
            }
        }
        if let Ok(mut q) = self.shared.moved.lock() {
            q.clear();
        }
        true
    }

    pub fn take_dirty(&self) -> bool {
        self.shared.dirty.swap(false, Relaxed)
    }

    pub fn take_rescan(&self) -> bool {
        self.shared.rescan.swap(false, Relaxed)
    }

    pub fn latency(&self) -> u32 {
        // SAFETY: a field of the live table.
        unsafe { (*self.fx).delay.max(0) as u32 }
    }

    pub fn has_window(&self) -> bool {
        // SAFETY: a field of the live table.
        KIND.is_some() && unsafe { (*self.fx).flags & HAS_EDITOR != 0 }
    }

    /// The size the plug-in wants for its window.
    unsafe fn wanted(&self) -> Option<(i32, i32)> {
        let mut rect: *mut Rect = std::ptr::null_mut();
        self.ask(EDIT_RECT, 0, 0, &mut rect as *mut *mut Rect as *mut c_void, 0.0);
        let r = rect.as_ref()?;
        let (w, h) = ((r.right - r.left) as i32, (r.bottom - r.top) as i32);
        (w > 0 && h > 0).then_some((w, h))
    }

    pub fn show_window(&self, title: &str) -> bool {
        if !self.has_window() {
            return false;
        }
        let Ok(mut slot) = self.win.lock() else { return false };
        if slot.is_some() {
            return true;
        }
        // SAFETY: window-thread requests to a live plug-in; the window outlives the plug-in's use of it,
        // as `hide_window` closes the plug-in's side first.
        unsafe {
            let first = self.wanted().unwrap_or((640, 400));
            let Some(win) = Win::open(title, first.0, first.1, false) else { return false };
            self.ask(EDIT_OPEN, 0, 0, win.handle(), 0.0);
            // many only know their size once the window is there
            if let Some(now) = self.wanted().filter(|now| *now != first) {
                win.resize(now.0, now.1, false);
            }
            *slot = Some(win);
        }
        true
    }

    pub fn hide_window(&self) {
        let Ok(mut slot) = self.win.lock() else { return };
        if slot.is_some() {
            // SAFETY: as in `show_window`.
            unsafe { self.ask(EDIT_CLOSE, 0, 0, std::ptr::null_mut(), 0.0) };
            *slot = None;
        }
    }

    pub fn window_open(&self) -> bool {
        self.win.lock().map(|w| w.is_some()).unwrap_or(false)
    }

    /// Window thread, every frame or so.
    pub fn idle(&self, _stalled: bool) {
        let mut closed = false;
        if let Ok(mut slot) = self.win.lock() {
            if let Some(win) = slot.as_mut() {
                // SAFETY: as in `show_window`.
                unsafe { self.ask(EDIT_IDLE, 0, 0, std::ptr::null_mut(), 0.0) };
                closed = win.poll().0;
                if let Some((w, h)) = self.shared.resize.lock().ok().and_then(|mut r| r.take()) {
                    if w > 0 && h > 0 {
                        win.resize(w, h, false);
                    }
                }
            }
        }
        if closed {
            self.hide_window();
        }
    }
}

impl Drop for Plug {
    fn drop(&mut self) {
        self.hide_window();
        let none = std::ptr::null_mut();
        // SAFETY: nothing else can reach the plug-in any more; it is stopped, then told to end.
        unsafe {
            if self.run.get_mut().map(|r| r.active).unwrap_or(false) {
                self.ask(STOP, 0, 0, none, 0.0);
                self.ask(MAINS, 0, 0, none, 0.0);
            }
            self.ask(CLOSE, 0, 0, none, 0.0);
        }
    }
}

/// What a project file keeps of a plug-in of this kind.
#[derive(Debug, PartialEq)]
pub enum Saved<'a> {
    Lump { program: u32, data: &'a [u8] },
    Travel { program: u32, values: Vec<f32> },
}

impl<'a> Saved<'a> {
    pub fn read(state: &'a [u8]) -> Option<Saved<'a>> {
        let word = |at: usize| state.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if state.get(..4)? == [0xf9, 0xff, 0xff, 0xff] {
            // the layout of early versions: a lump when its length is not nothing, else the settings
            let (len, program) = (word(5)? as usize, word(9)?);
            if len > 0 {
                return Some(Saved::Lump { program, data: state.get(13..13usize.checked_add(len)?)? });
            }
            let n = word(13)? as usize;
            let body = state.get(17..17usize.checked_add(n.checked_mul(4)?)?)?;
            return Some(Saved::Travel { program, values: body.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() });
        }
        if state.get(..4)? != [0xf7, 0xff, 0xff, 0xff] {
            return None;
        }
        let kind = *state.get(4)?;
        // the longer form carries a marker and a length of eight bytes where the short one has four
        let (len, at) = if word(5)? == 0xffff_fffe { (u64::from_le_bytes(state.get(9..17)?.try_into().ok()?) as usize, 17) } else { (word(5)? as usize, 9) };
        let program = word(at)?;
        match kind {
            13 => Some(Saved::Lump { program, data: state.get(at + 4..(at + 4).checked_add(len)?)? }),
            5 => {
                let n = word(at + 4)? as usize;
                let body = state.get(at + 8..(at + 8).checked_add(n.checked_mul(4)?)?)?;
                Some(Saved::Travel { program, values: body.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() })
            }
            _ => None,
        }
    }
}

/// A wrapper block of early versions, which names the plug-in by the name it was listed under and
/// carries its four-letter number after its state.
pub struct Old<'a> {
    pub name: String,
    pub unique: Option<u32>,
    pub state: &'a [u8],
}

pub fn old(block: &[u8]) -> Option<Old<'_>> {
    let word = |at: usize| block.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if (word(0)?, word(4)?, word(8)?, word(12)?) != (4, u32::MAX, 1, 0) {
        return None;
    }
    let len = *block.get(20)? as usize;
    let name = String::from_utf8_lossy(block.get(21..21 + len)?).trim().to_string();
    let at = 21 + len;
    let size = word(at)? as usize;
    let state = block.get(at + 4..(at + 4).checked_add(size)?)?;
    let after = at + 4 + size;
    let unique = (word(after) == Some(4)).then(|| word(after + 4)).flatten();
    (!name.is_empty()).then_some(Old { name, unique, state })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_stored_forms_are_read() {
        let mut lump = vec![0xf7, 0xff, 0xff, 0xff, 13, 3, 0, 0, 0, 7, 0, 0, 0, b'a', b'b', b'c'];
        assert_eq!(Saved::read(&lump), Some(Saved::Lump { program: 7, data: b"abc" }));
        lump.pop();
        assert_eq!(Saved::read(&lump), None);
        let mut travel = vec![0xf7, 0xff, 0xff, 0xff, 5, 0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
        travel.extend_from_slice(&0.5f32.to_le_bytes());
        travel.extend_from_slice(&1.0f32.to_le_bytes());
        travel.extend_from_slice(&[1, 0, 0, 0]);
        travel.extend_from_slice(&[0; 25]);
        assert_eq!(Saved::read(&travel), Some(Saved::Travel { program: 2, values: vec![0.5, 1.0] }));
        let mut long = vec![0xf7, 0xff, 0xff, 0xff, 5, 0xfe, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
        long.extend_from_slice(&0.25f32.to_le_bytes());
        assert_eq!(Saved::read(&long), Some(Saved::Travel { program: 1, values: vec![0.25] }));
        assert_eq!(Saved::read(b"0\x000\x00"), None);
        // the early layout, inside the early block
        let mut block = vec![4, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 4];
        block.extend_from_slice(b"Tone");
        let early = [0xf9, 0xff, 0xff, 0xff, 0, 2, 0, 0, 0, 5, 0, 0, 0, b'h', b'i'];
        block.extend_from_slice(&(early.len() as u32).to_le_bytes());
        block.extend_from_slice(&early);
        block.extend_from_slice(&[4, 0, 0, 0, b'd', b'c', b'b', b'a', 11, 0, 0, 0]);
        let got = old(&block).expect("an early block");
        assert_eq!((got.name.as_str(), got.unique), ("Tone", Some(u32::from_be_bytes(*b"abcd"))));
        assert_eq!(Saved::read(got.state), Some(Saved::Lump { program: 5, data: b"hi" }));
        let mut early = vec![0xf9, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0];
        early.extend_from_slice(&0.75f32.to_le_bytes());
        assert_eq!(Saved::read(&early), Some(Saved::Travel { program: 3, values: vec![0.75] }));
        assert_eq!(std::mem::size_of::<MidiEvent>(), 32);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<Effect>(), 192);
    }
}
