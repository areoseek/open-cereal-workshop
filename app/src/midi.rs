//! Notes from MIDI keyboards and pads. Every input device is listened to (as with all inputs enabled in the
//! reference) and what is played goes to the selected channel. The hold pedal keeps notes ringing.
//!
//! Linux: a sequencer client with one input port that connects itself to every readable port, including
//! devices plugged in later. Windows: every device of the multimedia system, reopened when one comes or goes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cereal_engine::CmdSender;

/// What the settings window decides for the two MIDI threads.
#[derive(Default, Clone)]
pub struct Prefs {
    /// Inputs that are switched off, by name.
    pub off: Vec<String>,
    /// Outputs that have a port number, by name: what a sender channel on that port plays through.
    pub out: Vec<(String, u8)>,
    /// Send the clock and start/stop to those outputs.
    pub sync: bool,
}

static PREFS: std::sync::Mutex<(Prefs, u64)> = std::sync::Mutex::new((Prefs { off: Vec::new(), out: Vec::new(), sync: false }, 0));
/// The devices the threads found, for the settings window's lists.
static INPUTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static OUTPUTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
/// Ticks per quarter note of the open song, for the clock.
pub static PPQ: AtomicUsize = AtomicUsize::new(96);
/// The MIDI channel (1..=16) whose notes play the clips of a performance; 0 for none.
pub static PERF_CHAN: AtomicUsize = AtomicUsize::new(0);

/// MIDI channels (1..=16, 0 for none) with a job of their own: notes on the first play the rack's channels
/// one per key, on the second they switch rack channels on and off, on the third they jump to time markers.
/// The middle C stands for the first channel or marker.
pub static OMNI_CHAN: AtomicUsize = AtomicUsize::new(0);
pub static MUTE_CHAN: AtomicUsize = AtomicUsize::new(0);
pub static MARKER_CHAN: AtomicUsize = AtomicUsize::new(0);
/// Every note plays at the default velocity, however hard the key was struck.
pub static FLAT_VEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The foot pedal is an ordinary controller that can be linked, not a note hold.
pub static PEDAL_FREE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Presses on the muting (0) and marker (1) channels since the app last looked, with the key's number from
/// the middle C.
static JOBS: std::sync::Mutex<Vec<(u8, u8)>> = std::sync::Mutex::new(Vec::new());

pub fn take_jobs() -> Vec<(u8, u8)> {
    JOBS.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// A note on the performance channel while a performance runs plays a clip: each octave is a track, each
/// key of it a clip of the zone. True when the note was taken.
fn perf_note(to: &CmdSender, midi_chan: u8, key: u8, down: bool) -> bool {
    let on = midi_chan as usize + 1;
    if on == OMNI_CHAN.load(Ordering::Relaxed) {
        if key >= 60 {
            let chan = (key - 60) as u16;
            to.send(if down { cereal_engine::Cmd::NoteOn { chan, key: 60, vel: 1.0 } } else { cereal_engine::Cmd::NoteOff { chan, key: 60 } });
        }
        return true;
    }
    for (kind, of) in [(0u8, &MUTE_CHAN), (1, &MARKER_CHAN)] {
        if on == of.load(Ordering::Relaxed) {
            if down && key >= 60 {
                if let Ok(mut q) = JOBS.lock() {
                    q.push((kind, key - 60));
                }
            }
            return true;
        }
    }
    let want = PERF_CHAN.load(Ordering::Relaxed);
    if want == 0 || want != midi_chan as usize + 1 || !to.performing() {
        return false;
    }
    to.send(cereal_engine::Cmd::PerfKey { track: (key / 12) as u16, nth: key % 12, down });
    true
}

/// Devices tied to one channel whatever is selected: device name, channel.
static ROUTES: std::sync::Mutex<Vec<(String, u16)>> = std::sync::Mutex::new(Vec::new());

pub fn set_routes(r: Vec<(String, u16)>) {
    if let Ok(mut g) = ROUTES.lock() {
        *g = r;
    }
}

/// The channel a device's notes go to: its own when it is tied to one, else the selected one.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn routed(device: Option<&str>, selected: u16) -> u16 {
    let Some(d) = device else { return selected };
    ROUTES.lock().ok().and_then(|g| g.iter().find(|r| r.0 == d).map(|r| r.1)).unwrap_or(selected)
}

pub fn set_prefs(p: Prefs) {
    if let Ok(mut g) = PREFS.lock() {
        g.0 = p;
        g.1 += 1;
    }
}

/// The preferences and a number that goes up whenever they change.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn prefs() -> (Prefs, u64) {
    PREFS.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn inputs() -> Vec<String> {
    INPUTS.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn outputs() -> Vec<String> {
    OUTPUTS.lock().map(|g| g.clone()).unwrap_or_default()
}

#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn found(list: &std::sync::Mutex<Vec<String>>, names: Vec<String>) {
    if let Ok(mut g) = list.lock() {
        *g = names;
    }
}

/// Which clock messages are due: start or stop when the transport changes, and 24 ticks to the quarter note.
#[derive(Default)]
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
struct Clock {
    running: bool,
    last: i64,
}

#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
impl Clock {
    /// Status bytes to send now (0xfa start, 0xfc stop, 0xf8 clock).
    fn due(&mut self, playing: bool, tick: f64, ppq: f64, out: &mut Vec<u8>) {
        let at = (tick * 24.0 / ppq.max(1.0)).floor() as i64;
        if playing && !self.running {
            out.push(0xfa);
            self.last = at;
        } else if !playing && self.running {
            out.push(0xfc);
        }
        self.running = playing;
        if playing {
            if at < self.last {
                // jumped back (a loop): carry on from there
                self.last = at;
            }
            // never a burst after a jump forward
            for _ in 0..(at - self.last).min(4) {
                out.push(0xf8);
            }
            self.last = at;
        }
    }
}

/// What the input thread shares with the window.
pub struct MidiIn {
    /// The channel played notes go to.
    pub chan: Arc<AtomicUsize>,
    /// Counts every message received, so the window can blink its activity light.
    pub seen: Arc<AtomicUsize>,
}

impl MidiIn {
    pub fn start(to: CmdSender) -> Self {
        let me = Self { chan: Arc::new(AtomicUsize::new(0)), seen: Arc::new(AtomicUsize::new(0)) };
        let (chan, seen) = (me.chan.clone(), me.seen.clone());
        if std::env::var_os("CEREAL_NO_MIDI").is_none() {
            let _ = std::thread::Builder::new().name("midi-in".into()).spawn(move || {
                let _ = run(to, chan, seen);
            });
        }
        me
    }
}

/// Keys held by hand or by the pedal, so a release goes to the channel the note started on.
/// Knob and slider moves heard since the app last looked: controller number and value, 0..=127.
static CONTROLS: std::sync::Mutex<Vec<(u8, u8)>> = std::sync::Mutex::new(Vec::new());

fn heard_control(number: u8, value: u8) {
    if let Ok(mut q) = CONTROLS.lock() {
        // only the latest value of a controller matters
        q.retain(|c| c.0 != number);
        q.push((number, value.min(127)));
    }
}

/// The controller moves that came in since the last call.
pub fn take_controls() -> Vec<(u8, u8)> {
    CONTROLS.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

struct Held {
    down: Vec<(u8, u16)>,
    /// Released while the pedal was down.
    pedal: Vec<(u8, u16)>,
    hold: bool,
}

impl Held {
    fn on(&mut self, to: &CmdSender, chan: u16, key: u8, vel: u8) {
        // a key struck again while the pedal still holds it restarts
        if let Some(i) = self.pedal.iter().position(|(k, _)| *k == key) {
            let (k, c) = self.pedal.remove(i);
            to.send(cereal_engine::Cmd::NoteOff { chan: c, key: k });
        }
        self.down.push((key, chan));
        let vel = if FLAT_VEL.load(Ordering::Relaxed) { 100 } else { vel };
        to.send(cereal_engine::Cmd::NoteOn { chan, key, vel: (vel as f32 / 127.0).powf(0.8) });
    }

    fn off(&mut self, to: &CmdSender, key: u8) {
        if let Some(i) = self.down.iter().position(|(k, _)| *k == key) {
            let (k, c) = self.down.remove(i);
            if self.hold {
                self.pedal.push((k, c));
            } else {
                to.send(cereal_engine::Cmd::NoteOff { chan: c, key: k });
            }
        }
    }

    fn set_hold(&mut self, to: &CmdSender, on: bool) {
        self.hold = on;
        if !on {
            for (k, c) in self.pedal.drain(..) {
                to.send(cereal_engine::Cmd::NoteOff { chan: c, key: k });
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run(to: CmdSender, chan: Arc<AtomicUsize>, seen: Arc<AtomicUsize>) -> Result<(), Box<dyn std::error::Error>> {
    use alsa::seq::{Addr, ClientIter, EvCtrl, EvNote, EventType, PortCap, PortIter, PortSubscribe, PortType};
    use std::ffi::CString;

    let seq = alsa::Seq::open(None, Some(alsa::Direction::Capture), true)?;
    seq.set_client_name(&CString::new("Open Cereal Workshop")?)?;
    let port = seq.create_simple_port(&CString::new("Input")?, PortCap::WRITE | PortCap::SUBS_WRITE, PortType::MIDI_GENERIC | PortType::APPLICATION)?;
    let me = seq.client_id()?;
    let mut linked: Vec<(i32, i32)> = Vec::new();
    let mut held = Held { down: vec![], pedal: vec![], hold: false };
    let mut input = seq.input();
    let mut since_scan = std::time::Instant::now() - std::time::Duration::from_secs(10);
    let mut seen_prefs = u64::MAX;
    let mut link_names: Vec<((i32, i32), String)> = Vec::new();
    loop {
        if since_scan.elapsed().as_secs() >= 2 || PREFS.lock().map(|g| g.1 != seen_prefs).unwrap_or(false) {
            since_scan = std::time::Instant::now();
            let (pref, gen) = prefs();
            seen_prefs = gen;
            let mut names = Vec::new();
            let mut found = Vec::new();
            let mut found_names = Vec::new();
            for client in ClientIter::new(&seq) {
                let id = client.get_client();
                // not ourselves, not the system's own ports, not the loop-back port
                // nor our own sending client: what we send must not come back in as played notes
                if id == me || id == 0 || id == 14 || client.get_name().map(|n| n.starts_with("Open Cereal Workshop")).unwrap_or(false) {
                    continue;
                }
                for p in PortIter::new(&seq, id) {
                    let caps = p.get_capability();
                    if caps.contains(PortCap::READ | PortCap::SUBS_READ) && !caps.contains(PortCap::NO_EXPORT) {
                        let name = port_name(&client, &p);
                        if !pref.off.contains(&name) {
                            found.push((id, p.get_port()));
                            found_names.push(((id, p.get_port()), name.clone()));
                        }
                        names.push(name);
                    }
                }
            }
            self::found(&INPUTS, names);
            // an input that was switched off is let go
            for a in &linked {
                if !found.contains(a) {
                    let _ = seq.unsubscribe_port(Addr { client: a.0, port: a.1 }, Addr { client: me, port });
                }
            }
            for a in &found {
                if !linked.contains(a) {
                    if let Ok(sub) = PortSubscribe::empty() {
                        sub.set_sender(Addr { client: a.0, port: a.1 });
                        sub.set_dest(Addr { client: me, port });
                        let _ = seq.subscribe_port(&sub);
                    }
                }
            }
            linked = found;
            link_names = found_names;
        }
        if input.event_input_pending(true).unwrap_or(0) == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }
        let Ok(ev) = input.event_input() else { continue };
        seen.fetch_add(1, Ordering::Relaxed);
        let src = ev.get_source();
        let c = routed(link_names.iter().find(|l| l.0 == (src.client, src.port)).map(|l| l.1.as_str()), chan.load(Ordering::Relaxed) as u16);
        match ev.get_type() {
            EventType::Noteon => {
                if let Some(n) = ev.get_data::<EvNote>() {
                    if perf_note(&to, n.channel, n.note, n.velocity > 0) {
                    } else if n.velocity == 0 {
                        held.off(&to, n.note);
                    } else {
                        held.on(&to, c, n.note, n.velocity);
                    }
                }
            }
            EventType::Noteoff => {
                if let Some(n) = ev.get_data::<EvNote>() {
                    if !perf_note(&to, n.channel, n.note, false) {
                        held.off(&to, n.note);
                    }
                }
            }
            EventType::Pitchbend => {
                // two semitones each way, the usual wheel range
                if let Some(cc) = ev.get_data::<EvCtrl>() {
                    to.send(cereal_engine::Cmd::Bend { chan: c, octaves: cc.value as f32 / 8192.0 * 2.0 / 12.0 });
                }
            }
            EventType::Controller => {
                if let Some(cc) = ev.get_data::<EvCtrl>() {
                    if cc.param == 64 && !PEDAL_FREE.load(Ordering::Relaxed) {
                        held.set_hold(&to, cc.value >= 64);
                    } else {
                        heard_control(cc.param.min(127) as u8, cc.value.clamp(0, 127) as u8);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A device's name as the lists show it: the client, and the port when it says more.
#[cfg(target_os = "linux")]
fn port_name(client: &alsa::seq::ClientInfo, port: &alsa::seq::PortInfo) -> String {
    let c = client.get_name().unwrap_or("").to_string();
    let p = port.get_name().unwrap_or("").to_string();
    if p.is_empty() || p == c || p.starts_with(&c) {
        if p.is_empty() { c } else { p }
    } else {
        format!("{c}: {p}")
    }
}

impl Held {
    /// One raw three-byte message, as the Windows driver hands them over.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    fn message(&mut self, to: &CmdSender, chan: u16, status: u8, d1: u8, d2: u8) {
        if matches!(status & 0xf0, 0x80 | 0x90) && perf_note(to, status & 15, d1 & 127, status & 0xf0 == 0x90 && d2 > 0) {
            return;
        }
        match status & 0xf0 {
            0x90 if d2 > 0 => self.on(to, chan, d1 & 127, d2 & 127),
            0x80 | 0x90 => self.off(to, d1 & 127),
            0xb0 if d1 == 64 && !PEDAL_FREE.load(Ordering::Relaxed) => self.set_hold(to, d2 >= 64),
            0xb0 => heard_control(d1 & 127, d2 & 127),
            0xe0 => {
                let v = ((d2 as i32 & 127) << 7 | (d1 as i32 & 127)) - 8192;
                to.send(cereal_engine::Cmd::Bend { chan, octaves: v as f32 / 8192.0 * 2.0 / 12.0 });
            }
            _ => {}
        }
    }
}

/// Windows: every input device of the multimedia system is opened, and reopened when the count changes.
#[cfg(windows)]
fn run(to: CmdSender, chan: Arc<AtomicUsize>, seen: Arc<AtomicUsize>) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Mutex;

    #[link(name = "winmm")]
    extern "system" {
        fn midiInGetNumDevs() -> u32;
        fn midiInOpen(handle: *mut isize, device: u32, callback: usize, instance: usize, flags: u32) -> u32;
        fn midiInStart(handle: isize) -> u32;
        fn midiInStop(handle: isize) -> u32;
        fn midiInClose(handle: isize) -> u32;
        fn midiInGetDevCapsW(device: usize, caps: *mut InCaps, size: u32) -> u32;
    }
    #[repr(C)]
    struct InCaps {
        mid: u16,
        pid: u16,
        version: u32,
        name: [u16; 32],
        support: u32,
    }
    let name_of = |id: u32| {
        let mut c = InCaps { mid: 0, pid: 0, version: 0, name: [0; 32], support: 0 };
        // SAFETY: the system fills the structure we hand it, whose size we state.
        if unsafe { midiInGetDevCapsW(id as usize, &mut c, std::mem::size_of::<InCaps>() as u32) } != 0 {
            return format!("Input {}", id + 1);
        }
        let _ = (c.mid, c.pid, c.version, c.support);
        wide_name(&c.name)
    };
    const CALLBACK_FUNCTION: u32 = 0x0003_0000;
    const MIM_DATA: u32 = 0x3c3;

    struct Shared {
        held: Mutex<Held>,
        to: CmdSender,
        chan: Arc<AtomicUsize>,
        seen: Arc<AtomicUsize>,
    }

    /// The open inputs: handle and device name.
    static OPEN: Mutex<Vec<(isize, String)>> = Mutex::new(Vec::new());

    extern "system" fn heard(handle: isize, msg: u32, instance: usize, p1: usize, _p2: usize) {
        if msg != MIM_DATA || instance == 0 {
            return;
        }
        // SAFETY: `instance` is the leaked `Shared` below, which lives as long as the program.
        let sh = unsafe { &*(instance as *const Shared) };
        sh.seen.fetch_add(1, Ordering::Relaxed);
        let name = OPEN.try_lock().ok().and_then(|o| o.iter().find(|x| x.0 == handle).map(|x| x.1.clone()));
        let c = routed(name.as_deref(), sh.chan.load(Ordering::Relaxed) as u16);
        if let Ok(mut h) = sh.held.lock() {
            h.message(&sh.to, c, p1 as u8, (p1 >> 8) as u8, (p1 >> 16) as u8);
        }
    }

    let shared: &'static Shared = Box::leak(Box::new(Shared { held: Mutex::new(Held { down: vec![], pedal: vec![], hold: false }), to, chan, seen }));
    let mut open: Vec<isize> = Vec::new();
    let mut count = u32::MAX;
    let mut seen_prefs = u64::MAX;
    loop {
        // SAFETY: plain calls into the system's MIDI library with valid pointers.
        let now = unsafe { midiInGetNumDevs() };
        let (pref, gen) = prefs();
        if now != count || gen != seen_prefs {
            seen_prefs = gen;
            let names: Vec<String> = (0..now).map(name_of).collect();
            found(&INPUTS, names.clone());
            for h in open.drain(..) {
                unsafe {
                    midiInStop(h);
                    midiInClose(h);
                }
            }
            let mut opened = Vec::new();
            for id in 0..now {
                if pref.off.contains(&names[id as usize]) {
                    continue;
                }
                let mut h: isize = 0;
                let ok = unsafe { midiInOpen(&mut h, id, heard as *const () as usize, shared as *const Shared as usize, CALLBACK_FUNCTION) };
                if ok == 0 {
                    unsafe { midiInStart(h) };
                    open.push(h);
                    opened.push((h, names[id as usize].clone()));
                }
            }
            if let Ok(mut o) = OPEN.lock() {
                *o = opened;
            }
            count = now;
        }
        // look again soon, so a change in the settings window is felt
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if PREFS.lock().map(|g| g.1 != seen_prefs).unwrap_or(false) {
                break;
            }
        }
    }
}

/// A device name out of the fixed wide-character field the multimedia system fills.
#[cfg(windows)]
fn wide_name(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

#[cfg(not(any(target_os = "linux", windows)))]
fn run(_to: CmdSender, _chan: Arc<AtomicUsize>, _seen: Arc<AtomicUsize>) -> Result<(), Box<dyn std::error::Error>> {
    let _ = Held { down: vec![], pedal: vec![], hold: false };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_starts_ticks_and_stops_with_the_transport() {
        let mut c = Clock::default();
        let mut out = Vec::new();
        c.due(false, 0.0, 96.0, &mut out);
        assert!(out.is_empty());
        c.due(true, 0.0, 96.0, &mut out);
        assert_eq!(out, vec![0xfa]);
        out.clear();
        // a quarter note later in three steps: 24 clocks in all
        for t in [30.0, 61.0, 96.0] {
            for _ in 0..8 {
                c.due(true, t, 96.0, &mut out);
            }
        }
        assert!(out.len() <= 24 && out.iter().all(|&b| b == 0xf8));
        out.clear();
        c.due(true, 100.0, 96.0, &mut out);
        assert_eq!(out, vec![0xf8]);
        out.clear();
        c.due(false, 100.0, 96.0, &mut out);
        assert_eq!(out, vec![0xfc]);
    }

    #[test]
    fn the_pedal_keeps_released_keys_until_it_lifts() {
        let (handle, _player) = cereal_engine::new_player(cereal_engine::Song::default(), 44100.0);
        let to = handle.sender();
        let mut h = Held { down: vec![], pedal: vec![], hold: false };
        h.on(&to, 2, 60, 100);
        h.set_hold(&to, true);
        h.off(&to, 60);
        assert_eq!(h.pedal, vec![(60, 2)]);
        h.set_hold(&to, false);
        assert!(h.pedal.is_empty() && h.down.is_empty());
        h.message(&to, 1, 0x90, 64, 90);
        h.message(&to, 1, 0x90, 64, 0);
        assert!(h.down.is_empty(), "a note on at zero velocity is a release");
    }
}

/// Counts every message sent, so a sender's page can blink its light.
pub static SENT: AtomicUsize = AtomicUsize::new(0);

/// Starts the thread that puts the sender channels' messages on the wire.
///
/// Linux: a sequencer client "Open Cereal Workshop Out" with one readable port per port number in use, for
/// synths and hardware to be connected to. Windows: port N plays through the Nth output device.
pub fn start_out(from: Arc<cereal_engine::player::Transport>) {
    if std::env::var_os("CEREAL_NO_MIDI").is_some() {
        return;
    }
    let _ = std::thread::Builder::new().name("midi-out".into()).spawn(move || {
        let _ = run_out(from);
    });
}

#[cfg(target_os = "linux")]
fn run_out(from: Arc<cereal_engine::player::Transport>) -> Result<(), Box<dyn std::error::Error>> {
    use alsa::seq::{EvCtrl, EvNote, Event, EventType, PortCap, PortType};
    use std::ffi::CString;

    let seq = alsa::Seq::open(None, Some(alsa::Direction::Playback), true)?;
    seq.set_client_name(&CString::new("Open Cereal Workshop Out")?)?;
    use alsa::seq::{Addr, ClientIter, PortIter, PortSubscribe};
    let me = seq.client_id()?;
    let mut ports: Vec<(u8, i32)> = Vec::new();
    // our port wired to a device: port number, device address
    let mut wired: Vec<(u8, i32, i32)> = Vec::new();
    let mut got = Vec::with_capacity(256);
    let mut since_scan = std::time::Instant::now() - std::time::Duration::from_secs(10);
    let mut seen_prefs = u64::MAX;
    let mut pref = Prefs::default();
    let mut clock = Clock::default();
    let mut due = Vec::with_capacity(8);
    let make = |seq: &alsa::Seq, n: u8| seq.create_simple_port(&CString::new(format!("Port {n}")).unwrap_or_default(), PortCap::READ | PortCap::SUBS_READ, PortType::MIDI_GENERIC | PortType::APPLICATION);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1));
        if since_scan.elapsed().as_secs() >= 2 || PREFS.lock().map(|g| g.1 != seen_prefs).unwrap_or(false) {
            since_scan = std::time::Instant::now();
            (pref, seen_prefs) = prefs();
            let mut names = Vec::new();
            let mut want: Vec<(u8, i32, i32)> = Vec::new();
            for client in ClientIter::new(&seq) {
                let id = client.get_client();
                if id == me || id == 0 || client.get_name().map(|n| n.starts_with("Open Cereal Workshop")).unwrap_or(false) {
                    continue;
                }
                for p in PortIter::new(&seq, id) {
                    let caps = p.get_capability();
                    if caps.contains(PortCap::WRITE | PortCap::SUBS_WRITE) && !caps.contains(PortCap::NO_EXPORT) {
                        let name = port_name(&client, &p);
                        if let Some(o) = pref.out.iter().find(|o| o.0 == name) {
                            want.push((o.1, id, p.get_port()));
                        }
                        names.push(name);
                    }
                }
            }
            found(&OUTPUTS, names);
            for w in &wired {
                if !want.contains(w) {
                    if let Some(p) = ports.iter().find(|p| p.0 == w.0) {
                        let _ = seq.unsubscribe_port(Addr { client: me, port: p.1 }, Addr { client: w.1, port: w.2 });
                    }
                }
            }
            for w in &want {
                if !wired.contains(w) {
                    let ours = match ports.iter().find(|p| p.0 == w.0) {
                        Some(p) => p.1,
                        None => {
                            let p = make(&seq, w.0)?;
                            ports.push((w.0, p));
                            p
                        }
                    };
                    if let Ok(sub) = PortSubscribe::empty() {
                        sub.set_sender(Addr { client: me, port: ours });
                        sub.set_dest(Addr { client: w.1, port: w.2 });
                        let _ = seq.subscribe_port(&sub);
                    }
                }
            }
            wired = want;
        }
        if pref.sync {
            due.clear();
            clock.due(from.playing(), from.tick(), PPQ.load(Ordering::Relaxed) as f64, &mut due);
            for &status in &due {
                let kind = match status {
                    0xfa => EventType::Start,
                    0xfc => EventType::Stop,
                    _ => EventType::Clock,
                };
                for p in &ports {
                    let mut ev = Event::new(kind, &());
                    ev.set_source(p.1);
                    ev.set_subs();
                    ev.set_direct();
                    let _ = seq.event_output_direct(&mut ev);
                }
            }
        }
        got.clear();
        from.take_midi(&mut got);
        for m in &got {
            let port = match ports.iter().find(|p| p.0 == m.port) {
                Some(p) => p.1,
                None => {
                    let p = make(&seq, m.port)?;
                    ports.push((m.port, p));
                    p
                }
            };
            let d = m.data;
            let channel = d[0] & 15;
            let note = EvNote { channel, note: d[1], velocity: d[2], off_velocity: 0, duration: 0 };
            let ctl = |param: u32, value: i32| EvCtrl { channel, param, value };
            let mut ev = match d[0] & 0xf0 {
                0x80 => Event::new(EventType::Noteoff, &note),
                0x90 => Event::new(EventType::Noteon, &note),
                0xb0 => Event::new(EventType::Controller, &ctl(d[1] as u32, d[2] as i32)),
                0xc0 => Event::new(EventType::Pgmchange, &ctl(0, d[1] as i32)),
                0xd0 => Event::new(EventType::Chanpress, &ctl(0, d[1] as i32)),
                0xe0 => Event::new(EventType::Pitchbend, &ctl(0, (d[1] as i32 | (d[2] as i32) << 7) - 8192)),
                _ => continue,
            };
            ev.set_source(port);
            ev.set_subs();
            ev.set_direct();
            let _ = seq.event_output_direct(&mut ev);
            SENT.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(windows)]
fn run_out(from: Arc<cereal_engine::player::Transport>) -> Result<(), Box<dyn std::error::Error>> {
    #[link(name = "winmm")]
    extern "system" {
        fn midiOutGetNumDevs() -> u32;
        fn midiOutOpen(handle: *mut isize, device: u32, callback: usize, instance: usize, flags: u32) -> u32;
        fn midiOutShortMsg(handle: isize, msg: u32) -> u32;
        fn midiOutClose(handle: isize) -> u32;
        fn midiOutGetDevCapsW(device: usize, caps: *mut OutCaps, size: u32) -> u32;
    }
    #[repr(C)]
    struct OutCaps {
        mid: u16,
        pid: u16,
        version: u32,
        name: [u16; 32],
        technology: u16,
        voices: u16,
        notes: u16,
        mask: u16,
        support: u32,
    }
    let name_of = |id: u32| {
        let mut c = OutCaps { mid: 0, pid: 0, version: 0, name: [0; 32], technology: 0, voices: 0, notes: 0, mask: 0, support: 0 };
        // SAFETY: the system fills the structure we hand it, whose size we state.
        if unsafe { midiOutGetDevCapsW(id as usize, &mut c, std::mem::size_of::<OutCaps>() as u32) } != 0 {
            return format!("Output {}", id + 1);
        }
        let _ = (c.mid, c.pid, c.version, c.technology, c.voices, c.notes, c.mask, c.support);
        wide_name(&c.name)
    };
    // port number -> open device (0 = could not be opened; not tried again)
    let mut open: Vec<(u8, isize)> = Vec::new();
    let mut got = Vec::with_capacity(256);
    let mut names: Vec<String> = Vec::new();
    let mut since_scan = std::time::Instant::now() - std::time::Duration::from_secs(10);
    let mut seen_prefs = u64::MAX;
    let mut pref = Prefs::default();
    let mut clock = Clock::default();
    let mut due = Vec::with_capacity(8);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1));
        let changed = PREFS.lock().map(|g| g.1 != seen_prefs).unwrap_or(false);
        if since_scan.elapsed().as_secs() >= 2 || changed {
            since_scan = std::time::Instant::now();
            // SAFETY: a plain call into the multimedia system.
            let now: Vec<String> = (0..unsafe { midiOutGetNumDevs() }).map(name_of).collect();
            if changed || now != names {
                // the ports are worked out afresh
                for (_, h) in open.drain(..) {
                    if h != 0 {
                        // SAFETY: the handle came from midiOutOpen and is not used again.
                        unsafe { midiOutClose(h) };
                    }
                }
                (pref, seen_prefs) = prefs();
                names = now;
                found(&OUTPUTS, names.clone());
                for (name, port) in &pref.out {
                    if let Some(id) = names.iter().position(|n| n == name).filter(|_| !open.iter().any(|p| p.0 == *port)) {
                        let mut h: isize = 0;
                        // SAFETY: plain calls into the multimedia system; `h` outlives the call that fills it.
                        let ok = unsafe { midiOutOpen(&mut h, id as u32, 0, 0, 0) == 0 };
                        if ok {
                            open.push((*port, h));
                        }
                    }
                }
            }
        }
        if pref.sync {
            due.clear();
            clock.due(from.playing(), from.tick(), PPQ.load(Ordering::Relaxed) as f64, &mut due);
            for &status in &due {
                for (_, h) in &open {
                    if *h != 0 {
                        // SAFETY: the handle came from midiOutOpen above.
                        unsafe { midiOutShortMsg(*h, status as u32) };
                    }
                }
            }
        }
        got.clear();
        from.take_midi(&mut got);
        for m in &got {
            let handle = match open.iter().find(|p| p.0 == m.port) {
                Some(p) => p.1,
                None => {
                    // no device was given this port number: the Nth device, when no port list was made at all
                    let mut h: isize = 0;
                    // SAFETY: plain calls into the multimedia system; `h` outlives the call that fills it.
                    let ok = pref.out.is_empty() && unsafe { (m.port as u32) < midiOutGetNumDevs() && midiOutOpen(&mut h, m.port as u32, 0, 0, 0) == 0 };
                    let h = if ok { h } else { 0 };
                    open.push((m.port, h));
                    h
                }
            };
            if handle != 0 {
                let d = m.data;
                // SAFETY: the handle came from midiOutOpen above and is never closed while the thread lives.
                unsafe { midiOutShortMsg(handle, d[0] as u32 | (d[1] as u32) << 8 | (d[2] as u32) << 16) };
                SENT.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn run_out(_from: Arc<cereal_engine::player::Transport>) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
