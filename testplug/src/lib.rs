//! A tiny plug-in for the host's tests, with a CLAP face (this file) and a VST3 one (`v3`): sound in is passed on times the Gain setting, and every held note
//! adds a sine at its pitch. Its state is the gain, eight bytes.

use std::ffi::{c_char, c_void, CStr};

use cereal_engine::host::abi::*;

mod v3;

struct Tone {
    plugin: Plugin,
    rate: f64,
    gain: f64,
    /// Held notes: key, level, phase.
    held: Vec<(i16, f64, f64)>,
}

const GAIN: u32 = 7;

struct Texts {
    features: [*const c_char; 3],
}
unsafe impl Sync for Texts {}

static TEXTS: Texts = Texts { features: [c"instrument".as_ptr(), c"audio-effect".as_ptr(), std::ptr::null()] };

static DESC: Descriptor = Descriptor {
    version: VERSION,
    id: c"org.breakfast-cereal.test-tone".as_ptr(),
    name: c"Test Tone".as_ptr(),
    vendor: c"Breakfast Cereal".as_ptr(),
    url: c"".as_ptr(),
    manual_url: c"".as_ptr(),
    support_url: c"".as_ptr(),
    plugin_version: c"0.1.0".as_ptr(),
    description: c"Gain and a sine voice, for tests".as_ptr(),
    features: TEXTS.features.as_ptr(),
};

unsafe fn me<'a>(p: *const Plugin) -> &'a mut Tone {
    &mut *((*p).data as *mut Tone)
}

unsafe extern "C" fn init(_p: *const Plugin) -> bool {
    true
}
unsafe extern "C" fn destroy(p: *const Plugin) {
    drop(Box::from_raw((*p).data as *mut Tone));
}
unsafe extern "C" fn activate(p: *const Plugin, rate: f64, _min: u32, _max: u32) -> bool {
    me(p).rate = rate;
    true
}
unsafe extern "C" fn deactivate(_p: *const Plugin) {}
unsafe extern "C" fn start(_p: *const Plugin) -> bool {
    true
}
unsafe extern "C" fn stop(_p: *const Plugin) {}
unsafe extern "C" fn reset(p: *const Plugin) {
    me(p).held.clear();
}

unsafe fn hear(t: &mut Tone, ev: *const EventHeader) {
    if (*ev).space != 0 {
        return;
    }
    match (*ev).kind {
        EV_NOTE_ON => {
            let n = &*(ev as *const EventNote);
            t.held.push((n.key, n.velocity * 0.5, 0.0));
        }
        EV_NOTE_OFF => {
            let n = &*(ev as *const EventNote);
            t.held.retain(|h| h.0 != n.key);
        }
        EV_PARAM_VALUE => {
            let v = &*(ev as *const EventParam);
            if v.param == GAIN {
                t.gain = v.value;
            }
        }
        _ => {}
    }
}

unsafe extern "C" fn process(p: *const Plugin, pr: *const Process) -> i32 {
    let t = me(p);
    let pr = &*pr;
    let events = &*pr.in_events;
    let count = events.size.map(|f| f(events)).unwrap_or(0);
    let mut next = 0;
    let n = pr.frames as usize;
    let input = (pr.audio_in_count > 0).then(|| &*pr.audio_in);
    let output = &*pr.audio_out;
    for i in 0..n {
        while next < count {
            let ev = (events.get.unwrap())(events, next);
            if (*ev).time as usize > i {
                break;
            }
            hear(t, ev);
            next += 1;
        }
        let mut voice = 0.0;
        for h in t.held.iter_mut() {
            let hz = 440.0 * 2f64.powf((h.0 as f64 - 69.0) / 12.0);
            h.2 += hz / t.rate;
            voice += (h.2 * std::f64::consts::TAU).sin() * h.1;
        }
        for c in 0..output.channels as usize {
            let dry = match input {
                Some(b) if (c as u32) < b.channels => *(*b.data32.add(c)).add(i) as f64,
                _ => 0.0,
            };
            *(*output.data32.add(c)).add(i) = ((dry + voice) * t.gain) as f32;
        }
    }
    1
}

unsafe extern "C" fn param_count(_p: *const Plugin) -> u32 {
    1
}
unsafe extern "C" fn param_info(_p: *const Plugin, index: u32, info: *mut ParamInfo) -> bool {
    if index != 0 {
        return false;
    }
    let info = &mut *info;
    info.id = GAIN;
    info.flags = 1 << 5;
    info.cookie = std::ptr::null_mut();
    info.name = [0; 256];
    for (o, b) in info.name.iter_mut().zip(b"Gain") {
        *o = *b as c_char;
    }
    info.module = [0; 1024];
    info.min = 0.0;
    info.max = 2.0;
    info.default = 1.0;
    true
}
unsafe extern "C" fn param_value(p: *const Plugin, id: u32, out: *mut f64) -> bool {
    if id != GAIN {
        return false;
    }
    *out = me(p).gain;
    true
}
unsafe extern "C" fn param_text(_p: *const Plugin, id: u32, value: f64, out: *mut c_char, capacity: u32) -> bool {
    if id != GAIN || capacity == 0 {
        return false;
    }
    let s = format!("{:.0} %", value * 100.0);
    let n = s.len().min(capacity as usize - 1);
    for (i, b) in s.bytes().take(n).enumerate() {
        *out.add(i) = b as c_char;
    }
    *out.add(n) = 0;
    true
}
unsafe extern "C" fn param_parse(_p: *const Plugin, _id: u32, _text: *const c_char, _out: *mut f64) -> bool {
    false
}
unsafe extern "C" fn param_flush(p: *const Plugin, input: *const InputEvents, _output: *const OutputEvents) {
    let events = &*input;
    for i in 0..events.size.map(|f| f(events)).unwrap_or(0) {
        hear(me(p), (events.get.unwrap())(events, i));
    }
}

static PARAMS: PluginParams = PluginParams { count: Some(param_count), get_info: Some(param_info), get_value: Some(param_value), value_to_text: Some(param_text), text_to_value: Some(param_parse), flush: Some(param_flush) };

unsafe extern "C" fn save(p: *const Plugin, s: *const OStream) -> bool {
    let bytes = me(p).gain.to_le_bytes();
    (*s).write.is_some_and(|w| w(s, bytes.as_ptr() as *const c_void, 8) == 8)
}
unsafe extern "C" fn load(p: *const Plugin, s: *const IStream) -> bool {
    let mut bytes = [0u8; 8];
    if (*s).read.is_some_and(|r| r(s, bytes.as_mut_ptr() as *mut c_void, 8) == 8) {
        me(p).gain = f64::from_le_bytes(bytes);
        true
    } else {
        false
    }
}

static STATE: PluginState = PluginState { save: Some(save), load: Some(load) };

unsafe extern "C" fn port_count(_p: *const Plugin, _input: bool) -> u32 {
    1
}
unsafe extern "C" fn port_get(_p: *const Plugin, index: u32, _input: bool, info: *mut AudioPortInfo) -> bool {
    if index != 0 {
        return false;
    }
    let info = &mut *info;
    info.id = 0;
    info.name = [0; 256];
    info.flags = 1;
    info.channels = 2;
    info.port_type = c"stereo".as_ptr();
    info.in_place_pair = u32::MAX;
    true
}

static PORTS: PluginAudioPorts = PluginAudioPorts { count: Some(port_count), get: Some(port_get) };

unsafe extern "C" fn note_count(_p: *const Plugin, input: bool) -> u32 {
    input as u32
}
unsafe extern "C" fn note_get(_p: *const Plugin, index: u32, input: bool, info: *mut NotePortInfo) -> bool {
    if index != 0 || !input {
        return false;
    }
    let info = &mut *info;
    info.id = 0;
    info.supported = DIALECT_CLAP;
    info.preferred = DIALECT_CLAP;
    info.name = [0; 256];
    true
}

static NOTES: PluginNotePorts = PluginNotePorts { count: Some(note_count), get: Some(note_get) };

unsafe extern "C" fn extension(_p: *const Plugin, id: *const c_char) -> *const c_void {
    let id = CStr::from_ptr(id).to_bytes_with_nul();
    if id == EXT_PARAMS {
        &PARAMS as *const _ as *const c_void
    } else if id == EXT_STATE {
        &STATE as *const _ as *const c_void
    } else if id == EXT_AUDIO_PORTS {
        &PORTS as *const _ as *const c_void
    } else if id == EXT_NOTE_PORTS {
        &NOTES as *const _ as *const c_void
    } else {
        std::ptr::null()
    }
}
unsafe extern "C" fn on_main(_p: *const Plugin) {}

unsafe extern "C" fn count(_f: *const Factory) -> u32 {
    1
}
unsafe extern "C" fn descriptor(_f: *const Factory, index: u32) -> *const Descriptor {
    if index == 0 {
        &DESC
    } else {
        std::ptr::null()
    }
}
unsafe extern "C" fn create(_f: *const Factory, _host: *const Host, id: *const c_char) -> *const Plugin {
    if id.is_null() || CStr::from_ptr(id) != CStr::from_ptr(DESC.id) {
        return std::ptr::null();
    }
    let t = Box::into_raw(Box::new(Tone {
        plugin: Plugin {
            desc: &DESC,
            data: std::ptr::null_mut(),
            init: Some(init),
            destroy: Some(destroy),
            activate: Some(activate),
            deactivate: Some(deactivate),
            start_processing: Some(start),
            stop_processing: Some(stop),
            reset: Some(reset),
            process: Some(process),
            get_extension: Some(extension),
            on_main_thread: Some(on_main),
        },
        rate: 44_100.0,
        gain: 1.0,
        held: Vec::new(),
    }));
    (*t).plugin.data = t as *mut c_void;
    &(*t).plugin
}

static FACTORY: Factory = Factory { count: Some(count), descriptor: Some(descriptor), create: Some(create) };

unsafe extern "C" fn entry_init(_path: *const c_char) -> bool {
    true
}
unsafe extern "C" fn entry_deinit() {}
unsafe extern "C" fn get_factory(id: *const c_char) -> *const c_void {
    if CStr::from_ptr(id).to_bytes_with_nul() == FACTORY_ID {
        &FACTORY as *const _ as *const c_void
    } else {
        std::ptr::null()
    }
}

/// What a host looks for in the file.
#[no_mangle]
#[allow(non_upper_case_globals)]
pub static clap_entry: Entry = Entry { version: VERSION, init: Some(entry_init), deinit: Some(entry_deinit), get_factory: Some(get_factory) };
