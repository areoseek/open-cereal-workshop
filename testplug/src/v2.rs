//! The same test tone as a plug-in of the older single-table kind. The layout is written out from
//! public descriptions of the interface.

use std::ffi::c_void;

type Call = unsafe extern "C" fn(*mut Effect, i32, i32, isize, *mut c_void, f32) -> isize;

#[repr(C)]
pub struct Effect {
    magic: i32,
    dispatch: Option<Call>,
    old_run: *const c_void,
    set_param: Option<unsafe extern "C" fn(*mut Effect, i32, f32)>,
    get_param: Option<unsafe extern "C" fn(*mut Effect, i32) -> f32>,
    programs: i32,
    params: i32,
    ins: i32,
    outs: i32,
    flags: i32,
    host: [isize; 2],
    delay: i32,
    quality: [i32; 2],
    ratio: f32,
    object: *mut c_void,
    user: *mut c_void,
    unique: i32,
    version: i32,
    run: Option<unsafe extern "C" fn(*mut Effect, *mut *mut f32, *mut *mut f32, i32)>,
    run64: *const c_void,
    future: [u8; 56],
}

#[repr(C)]
struct MidiEvent {
    kind: i32,
    size: i32,
    delta: i32,
    flags: i32,
    length: i32,
    offset: i32,
    data: [u8; 4],
    rest: [u8; 4],
}

#[repr(C)]
struct Events {
    count: i32,
    reserved: isize,
    list: [*mut MidiEvent; 2],
}

#[repr(C)]
struct TimeInfo {
    sample_pos: f64,
    rate: f64,
    nanos: f64,
    beats: f64,
    tempo: f64,
}

struct Tone {
    master: Call,
    rate: f32,
    level: f32,
    tune: f32,
    key: Option<u8>,
    starts: i32,
    ends: Option<i32>,
    phase: f32,
    lump: Vec<u8>,
    /// The tempo the host last gave, for the test to read back as a setting.
    tempo: f32,
}

unsafe fn tone(e: *mut Effect) -> &'static mut Tone {
    &mut *((*e).object as *mut Tone)
}

unsafe fn put(ptr: *mut c_void, text: &[u8]) {
    std::ptr::copy_nonoverlapping(text.as_ptr(), ptr as *mut u8, text.len());
    *(ptr as *mut u8).add(text.len()) = 0;
}

unsafe extern "C" fn dispatch(e: *mut Effect, what: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
    let t = tone(e);
    match what {
        1 => {
            drop(Box::from_raw((*e).object as *mut Tone));
            drop(Box::from_raw(e));
            0
        }
        6 => {
            put(ptr, if index == 0 { b"dB" } else { b"" });
            0
        }
        7 => {
            let v = if index == 0 { t.level } else { t.tune };
            put(ptr, format!("{:.2}", v).as_bytes());
            0
        }
        8 => {
            put(ptr, [&b"Level"[..], b"Tune", b"Tempo seen"][index.clamp(0, 2) as usize]);
            0
        }
        10 => {
            t.rate = opt;
            0
        }
        23 => {
            t.lump = b"tone".to_vec();
            t.lump.extend_from_slice(&t.level.to_le_bytes());
            t.lump.extend_from_slice(&t.tune.to_le_bytes());
            *(ptr as *mut *const u8) = t.lump.as_ptr();
            t.lump.len() as isize
        }
        24 => {
            let d = std::slice::from_raw_parts(ptr as *const u8, value.max(0) as usize);
            if d.len() == 12 && &d[..4] == b"tone" {
                t.level = f32::from_le_bytes([d[4], d[5], d[6], d[7]]);
                t.tune = f32::from_le_bytes([d[8], d[9], d[10], d[11]]);
                1
            } else {
                0
            }
        }
        25 => {
            let evs = &*(ptr as *const Events);
            for i in 0..evs.count.max(0) as usize {
                let m = &**evs.list.as_ptr().add(i);
                if m.kind != 1 {
                    continue;
                }
                match m.data[0] & 0xf0 {
                    0x90 if m.data[2] > 0 => {
                        t.key = Some(m.data[1]);
                        t.starts = m.delta;
                        t.ends = None;
                    }
                    0x80 | 0x90 => t.ends = Some(m.delta),
                    _ => {}
                }
            }
            1
        }
        35 => 2,
        45 => {
            put(ptr, b"Test Tone Two");
            1
        }
        47 => {
            put(ptr, b"Open Cereal Workshop");
            1
        }
        _ => 0,
    }
}

unsafe extern "C" fn set_param(e: *mut Effect, index: i32, value: f32) {
    let t = tone(e);
    match index {
        0 => t.level = value,
        1 => t.tune = value,
        _ => {}
    }
}

unsafe extern "C" fn get_param(e: *mut Effect, index: i32) -> f32 {
    let t = tone(e);
    match index {
        0 => t.level,
        1 => t.tune,
        _ => t.tempo,
    }
}

unsafe extern "C" fn run(e: *mut Effect, _ins: *mut *mut f32, outs: *mut *mut f32, frames: i32) {
    let t = tone(e);
    let time = (t.master)(e, 7, 0, 0, std::ptr::null_mut(), 0.0) as *const TimeInfo;
    if let Some(time) = time.as_ref() {
        t.tempo = time.tempo as f32 / 1000.0;
    }
    let (l, r) = (*outs, *outs.add(1));
    for i in 0..frames.max(0) {
        if t.ends == Some(i) {
            t.key = None;
            t.ends = None;
        }
        let mut s = 0.0;
        if let Some(key) = t.key.filter(|_| i >= t.starts) {
            let hz = 440.0 * 2f32.powf((key as f32 - 69.0 + (t.tune - 0.5) * 2.0) / 12.0);
            t.phase = (t.phase + hz / t.rate.max(1.0)).fract();
            s = (t.phase * std::f32::consts::TAU).sin() * t.level;
        }
        *l.add(i as usize) = s;
        *r.add(i as usize) = s;
    }
    t.starts = 0;
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn VSTPluginMain(master: Call) -> *mut Effect {
    if master(std::ptr::null_mut(), 1, 0, 0, std::ptr::null_mut(), 0.0) < 2000 {
        return std::ptr::null_mut();
    }
    let tone = Box::new(Tone { master, rate: 44_100.0, level: 0.5, tune: 0.5, key: None, starts: 0, ends: None, phase: 0.0, lump: vec![], tempo: 0.0 });
    Box::into_raw(Box::new(Effect {
        magic: 0x5673_7450,
        dispatch: Some(dispatch),
        old_run: std::ptr::null(),
        set_param: Some(set_param),
        get_param: Some(get_param),
        programs: 1,
        params: 3,
        ins: 0,
        outs: 2,
        // sound through the newer call, state as one lump, an instrument
        flags: 1 << 4 | 1 << 5 | 1 << 8,
        host: [0; 2],
        delay: 0,
        quality: [0; 2],
        ratio: 1.0,
        object: Box::into_raw(tone) as *mut c_void,
        user: std::ptr::null_mut(),
        unique: i32::from_be_bytes(*b"OCWt"),
        version: 1,
        run: Some(run),
        run64: std::ptr::null(),
        future: [0; 56],
    }))
}
