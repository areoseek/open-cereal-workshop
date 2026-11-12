//! The same test plug-in as a VST3 one: one object that is the sound half and the settings half at once.
//! Gain goes over as 0..1 standing for 0..2; the state is the gain, eight bytes.

use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

use cereal_engine::host::abi3::*;

const GAIN: u32 = 7;
const CLASS: Tuid = iid(0xB7EAFA57, 0xCE4EA100, 0x7E577011, 0x00000003);

/// The object: three tables, one for each interface it answers to, then what it holds.
#[repr(C)]
struct Tone {
    faces: [*const c_void; 3],
    refs: AtomicU32,
    rate: f64,
    gain: f64,
    /// What the settings half believes the gain is, 0..1.
    shown: f64,
    held: Vec<(i16, f64, f64)>,
}

/// The object behind one of its faces.
unsafe fn me<'a>(this: Obj, face: usize) -> &'a mut Tone {
    &mut *((this as *mut *const c_void).sub(face) as *mut Tone)
}

unsafe fn ask(t: &mut Tone, want: *const Tuid, out: *mut Obj) -> i32 {
    let face = match *want {
        w if w == FUNKNOWN || w == ICOMPONENT || w == iid(0x22888DDB, 0x156E45AE, 0x8358B348, 0x08190625) => 0,
        w if w == IPROCESSOR => 1,
        w if w == ICONTROLLER => 2,
        _ => {
            *out = std::ptr::null_mut();
            return NO_INTERFACE;
        }
    };
    t.refs.fetch_add(1, Relaxed);
    *out = &mut t.faces[face] as *mut *const c_void as Obj;
    OK
}

unsafe fn keep(t: &mut Tone) -> u32 {
    t.refs.fetch_add(1, Relaxed) + 1
}

unsafe fn let_go(t: &mut Tone) -> u32 {
    let left = t.refs.fetch_sub(1, Relaxed) - 1;
    if left == 0 {
        drop(Box::from_raw(t as *mut Tone));
    }
    left
}

macro_rules! base {
    ($face:expr, $q:ident, $k:ident, $d:ident) => {
        unsafe extern "system" fn $q(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
            ask(me(this, $face), want, out)
        }
        unsafe extern "system" fn $k(this: Obj) -> u32 {
            keep(me(this, $face))
        }
        unsafe extern "system" fn $d(this: Obj) -> u32 {
            let_go(me(this, $face))
        }
    };
}
base!(0, q0, k0, d0);
base!(1, q1, k1, d1);
base!(2, q2, k2, d2);

unsafe fn table<T>(o: Obj) -> &'static T {
    &**(o as *mut *const T)
}

fn put(to: &mut [c_char], text: &str) {
    for (o, b) in to.iter_mut().zip(text.bytes()) {
        *o = b as c_char;
    }
}

fn put_wide(to: &mut [u16], text: &str) {
    for (o, c) in to.iter_mut().zip(text.encode_utf16().chain(Some(0))) {
        *o = c;
    }
}

unsafe fn read_gain(t: &mut Tone, stream: Obj) -> i32 {
    let mut raw = [0u8; 8];
    let mut got = 0;
    (table::<StreamVt>(stream).read)(stream, raw.as_mut_ptr() as *mut c_void, 8, &mut got);
    if got != 8 {
        return FALSE;
    }
    t.gain = f64::from_le_bytes(raw);
    OK
}

unsafe extern "system" fn yes(_: Obj) -> i32 {
    OK
}
unsafe extern "system" fn start(_: Obj, _host: Obj) -> i32 {
    OK
}

// the sound half
unsafe extern "system" fn controller_class(_: Obj, _: *mut Tuid) -> i32 {
    FALSE
}
unsafe extern "system" fn io_mode(_: Obj, _: i32) -> i32 {
    OK
}
unsafe extern "system" fn bus_count(_: Obj, media: i32, dir: i32) -> i32 {
    if media == 0 || dir == 0 {
        1
    } else {
        0
    }
}
unsafe extern "system" fn bus_info(_: Obj, media: i32, dir: i32, index: i32, info: *mut BusInfo) -> i32 {
    if index != 0 {
        return BAD_ARG;
    }
    let i = &mut *info;
    i.media = media;
    i.dir = dir;
    i.channels = if media == 0 { 2 } else { 1 };
    put_wide(&mut i.name, "Main");
    i.kind = 0;
    i.flags = 1;
    OK
}
unsafe extern "system" fn routing(_: Obj, _: Obj, _: Obj) -> i32 {
    FALSE
}
unsafe extern "system" fn activate_bus(_: Obj, _: i32, _: i32, _: i32, _: u8) -> i32 {
    OK
}
unsafe extern "system" fn set_active(this: Obj, on: u8) -> i32 {
    if on == 0 {
        me(this, 0).held.clear();
    }
    OK
}
unsafe extern "system" fn set_state(this: Obj, stream: Obj) -> i32 {
    read_gain(me(this, 0), stream)
}
unsafe extern "system" fn get_state(this: Obj, stream: Obj) -> i32 {
    let mut raw = me(this, 0).gain.to_le_bytes();
    (table::<StreamVt>(stream).write)(stream, raw.as_mut_ptr() as *mut c_void, 8, std::ptr::null_mut())
}
static COMPONENT: ComponentVt = ComponentVt {
    base: Unknown { query: q0, keep: k0, drop: d0 },
    initialize: start,
    terminate: yes,
    controller_class,
    set_io_mode: io_mode,
    bus_count,
    bus_info,
    routing,
    activate_bus,
    set_active,
    set_state,
    get_state,
};

unsafe extern "system" fn set_arrangements(_: Obj, _: *mut u64, _: i32, _: *mut u64, _: i32) -> i32 {
    FALSE
}
unsafe extern "system" fn get_arrangement(_: Obj, _: i32, _: i32, out: *mut u64) -> i32 {
    *out = 3;
    OK
}
unsafe extern "system" fn can_sample_size(_: Obj, size: i32) -> i32 {
    if size == 0 {
        OK
    } else {
        FALSE
    }
}
unsafe extern "system" fn zero(_: Obj) -> u32 {
    0
}
unsafe extern "system" fn setup(this: Obj, s: *mut Setup) -> i32 {
    me(this, 1).rate = (*s).rate;
    OK
}
unsafe extern "system" fn set_processing(_: Obj, _: u8) -> i32 {
    OK
}
unsafe extern "system" fn process(this: Obj, data: *mut ProcessData) -> i32 {
    let t = me(this, 1);
    let d = &*data;
    if !d.in_changes.is_null() {
        let c = table::<ChangesVt>(d.in_changes);
        for i in 0..(c.count)(d.in_changes) {
            let q = (c.get)(d.in_changes, i);
            let qv = table::<QueueVt>(q);
            let n = (qv.count)(q);
            let (mut at, mut v) = (0, 0.0);
            if (qv.id)(q) == GAIN && n > 0 && (qv.get)(q, n - 1, &mut at, &mut v) == OK {
                t.gain = v * 2.0;
            }
        }
    }
    let mut events = Vec::new();
    if !d.in_events.is_null() {
        let e = table::<EventsVt>(d.in_events);
        for i in 0..(e.count)(d.in_events) {
            let mut ev: Event = std::mem::zeroed();
            if (e.get)(d.in_events, i, &mut ev) == OK {
                events.push(ev);
            }
        }
    }
    let mut next = 0;
    for i in 0..d.frames.max(0) as usize {
        while next < events.len() && events[next].offset as usize <= i {
            let ev = &events[next];
            match ev.kind {
                0 => t.held.push((ev.body.on.pitch, ev.body.on.velocity as f64 * 0.5, 0.0)),
                1 => {
                    let key = ev.body.off.pitch;
                    t.held.retain(|h| h.0 != key);
                }
                _ => {}
            }
            next += 1;
        }
        let mut voice = 0.0;
        for h in t.held.iter_mut() {
            let hz = 440.0 * 2f64.powf((h.0 as f64 - 69.0) / 12.0);
            h.2 += hz / t.rate;
            voice += (h.2 * std::f64::consts::TAU).sin() * h.1;
        }
        if d.n_out > 0 {
            let out = &*d.outs;
            for c in 0..out.channels as usize {
                let dry = if d.n_in > 0 && (c as i32) < (*d.ins).channels { *(*(*d.ins).data.add(c)).add(i) as f64 } else { 0.0 };
                *(*out.data.add(c)).add(i) = ((dry + voice) * t.gain) as f32;
            }
        }
    }
    OK
}
static PROCESSOR: ProcessorVt = ProcessorVt { base: Unknown { query: q1, keep: k1, drop: d1 }, set_arrangements, get_arrangement, can_sample_size, latency: zero, setup, set_processing, process, tail: zero };

// the settings half
unsafe extern "system" fn stream_ok(_: Obj, _: Obj) -> i32 {
    OK
}
unsafe extern "system" fn component_state(this: Obj, stream: Obj) -> i32 {
    let t = me(this, 2);
    let r = read_gain(t, stream);
    t.shown = t.gain / 2.0;
    r
}
unsafe extern "system" fn param_count(_: Obj) -> i32 {
    1
}
unsafe extern "system" fn param_info(_: Obj, index: i32, info: *mut ParamInfo) -> i32 {
    if index != 0 {
        return BAD_ARG;
    }
    let i = &mut *info;
    i.id = GAIN;
    put_wide(&mut i.title, "Gain");
    put_wide(&mut i.short, "Gain");
    put_wide(&mut i.units, "%");
    i.steps = 0;
    i.default = 0.5;
    i.unit = 0;
    i.flags = 1;
    OK
}
unsafe extern "system" fn text_of(_: Obj, id: u32, v: f64, out: *mut u16) -> i32 {
    if id != GAIN {
        return BAD_ARG;
    }
    put_wide(std::slice::from_raw_parts_mut(out, 128), &format!("{:.0}", v * 200.0));
    OK
}
unsafe extern "system" fn value_of(_: Obj, _: u32, _: *mut u16, _: *mut f64) -> i32 {
    FALSE
}
unsafe extern "system" fn to_plain(_: Obj, _: u32, v: f64) -> f64 {
    v * 2.0
}
unsafe extern "system" fn to_normal(_: Obj, _: u32, v: f64) -> f64 {
    v / 2.0
}
unsafe extern "system" fn get(this: Obj, id: u32) -> f64 {
    if id == GAIN {
        me(this, 2).shown
    } else {
        0.0
    }
}
unsafe extern "system" fn set(this: Obj, id: u32, v: f64) -> i32 {
    // the sound half hears of it through the host, as the standard has it
    if id == GAIN {
        me(this, 2).shown = v;
    }
    OK
}
unsafe extern "system" fn create_view(_: Obj, _: *const c_char) -> Obj {
    std::ptr::null_mut()
}
static CONTROLLER: ControllerVt = ControllerVt {
    base: Unknown { query: q2, keep: k2, drop: d2 },
    initialize: start,
    terminate: yes,
    set_component_state: component_state,
    set_state: stream_ok,
    get_state: stream_ok,
    param_count,
    param_info,
    text_of,
    value_of,
    to_plain,
    to_normal,
    get,
    set,
    set_handler: stream_ok,
    create_view,
};

// the factory: one for the whole file
struct Factory {
    table: *const FactoryVt,
}
unsafe impl Sync for Factory {}

unsafe extern "system" fn f_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    if *want == FUNKNOWN || *want == IFACTORY || *want == IFACTORY2 {
        *out = this;
        OK
    } else {
        *out = std::ptr::null_mut();
        NO_INTERFACE
    }
}
unsafe extern "system" fn f_one(_: Obj) -> u32 {
    1
}
unsafe extern "system" fn f_info(_: Obj, info: *mut FactoryInfo) -> i32 {
    put(&mut (*info).vendor, "Open Cereal Workshop");
    OK
}
unsafe extern "system" fn f_count(_: Obj) -> i32 {
    1
}
unsafe extern "system" fn f_class(_: Obj, index: i32, info: *mut ClassInfo) -> i32 {
    if index != 0 {
        return BAD_ARG;
    }
    let i = &mut *info;
    i.cid = CLASS;
    i.cardinality = 0x7FFF_FFFF;
    put(&mut i.category, "Audio Module Class");
    put(&mut i.name, "Test Tone");
    OK
}
unsafe extern "system" fn f_class2(_: Obj, index: i32, info: *mut ClassInfo2) -> i32 {
    if index != 0 {
        return BAD_ARG;
    }
    let i = &mut *info;
    i.cid = CLASS;
    i.cardinality = 0x7FFF_FFFF;
    put(&mut i.category, "Audio Module Class");
    put(&mut i.name, "Test Tone");
    put(&mut i.sub, "Instrument|Fx");
    put(&mut i.vendor, "Open Cereal Workshop");
    put(&mut i.version, "0.1.0");
    put(&mut i.sdk, "VST 3.7");
    OK
}
unsafe extern "system" fn f_create(_: Obj, class: *const Tuid, want: *const Tuid, out: *mut Obj) -> i32 {
    *out = std::ptr::null_mut();
    if *class != CLASS {
        return BAD_ARG;
    }
    let t = Box::into_raw(Box::new(Tone {
        faces: [&COMPONENT as *const ComponentVt as *const c_void, &PROCESSOR as *const ProcessorVt as *const c_void, &CONTROLLER as *const ControllerVt as *const c_void],
        refs: AtomicU32::new(1),
        rate: 44_100.0,
        gain: 1.0,
        shown: 0.5,
        held: Vec::new(),
    }));
    let r = ask(&mut *t, want, out);
    let_go(&mut *t);
    r
}
static FACTORY_VT: FactoryVt = FactoryVt { base: Unknown { query: f_query, keep: f_one, drop: f_one }, info: f_info, count: f_count, class: f_class, create: f_create, class2: f_class2 };
static FACTORY: Factory = Factory { table: &FACTORY_VT };

/// # Safety
/// Called by a host when it loads the file.
#[no_mangle]
pub unsafe extern "system" fn GetPluginFactory() -> Obj {
    &FACTORY.table as *const *const FactoryVt as Obj
}

/// # Safety
/// Called by a host when it loads the file.
#[no_mangle]
pub unsafe extern "C" fn ModuleEntry(_handle: *mut c_void) -> bool {
    true
}

/// # Safety
/// Called by a host when it lets the file go.
#[no_mangle]
pub unsafe extern "C" fn ModuleExit() -> bool {
    true
}
