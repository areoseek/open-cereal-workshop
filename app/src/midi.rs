//! Notes from MIDI keyboards and pads. Every input device is listened to (as with all inputs enabled in the
//! reference) and what is played goes to the selected channel. The hold pedal keeps notes ringing.
//!
//! Linux: a sequencer client with one input port that connects itself to every readable port, including
//! devices plugged in later. Windows: every device of the multimedia system, reopened when one comes or goes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cereal_engine::CmdSender;

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
    seq.set_client_name(&CString::new("Breakfast Cereal")?)?;
    let port = seq.create_simple_port(&CString::new("Input")?, PortCap::WRITE | PortCap::SUBS_WRITE, PortType::MIDI_GENERIC | PortType::APPLICATION)?;
    let me = seq.client_id()?;
    let mut linked: Vec<(i32, i32)> = Vec::new();
    let mut held = Held { down: vec![], pedal: vec![], hold: false };
    let mut input = seq.input();
    let mut since_scan = std::time::Instant::now() - std::time::Duration::from_secs(10);
    loop {
        if since_scan.elapsed().as_secs() >= 2 {
            since_scan = std::time::Instant::now();
            let mut found = Vec::new();
            for client in ClientIter::new(&seq) {
                let id = client.get_client();
                // not ourselves, not the system's own ports, not the loop-back port
                if id == me || id == 0 || id == 14 {
                    continue;
                }
                for p in PortIter::new(&seq, id) {
                    let caps = p.get_capability();
                    if caps.contains(PortCap::READ | PortCap::SUBS_READ) && !caps.contains(PortCap::NO_EXPORT) {
                        found.push((id, p.get_port()));
                    }
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
        }
        if input.event_input_pending(true).unwrap_or(0) == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }
        let Ok(ev) = input.event_input() else { continue };
        seen.fetch_add(1, Ordering::Relaxed);
        let c = chan.load(Ordering::Relaxed) as u16;
        match ev.get_type() {
            EventType::Noteon => {
                if let Some(n) = ev.get_data::<EvNote>() {
                    if n.velocity == 0 {
                        held.off(&to, n.note);
                    } else {
                        held.on(&to, c, n.note, n.velocity);
                    }
                }
            }
            EventType::Noteoff => {
                if let Some(n) = ev.get_data::<EvNote>() {
                    held.off(&to, n.note);
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
                    if cc.param == 64 {
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

impl Held {
    /// One raw three-byte message, as the Windows driver hands them over.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    fn message(&mut self, to: &CmdSender, chan: u16, status: u8, d1: u8, d2: u8) {
        match status & 0xf0 {
            0x90 if d2 > 0 => self.on(to, chan, d1 & 127, d2 & 127),
            0x80 | 0x90 => self.off(to, d1 & 127),
            0xb0 if d1 == 64 => self.set_hold(to, d2 >= 64),
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
    }
    const CALLBACK_FUNCTION: u32 = 0x0003_0000;
    const MIM_DATA: u32 = 0x3c3;

    struct Shared {
        held: Mutex<Held>,
        to: CmdSender,
        chan: Arc<AtomicUsize>,
        seen: Arc<AtomicUsize>,
    }

    extern "system" fn heard(_handle: isize, msg: u32, instance: usize, p1: usize, _p2: usize) {
        if msg != MIM_DATA || instance == 0 {
            return;
        }
        // SAFETY: `instance` is the leaked `Shared` below, which lives as long as the program.
        let sh = unsafe { &*(instance as *const Shared) };
        sh.seen.fetch_add(1, Ordering::Relaxed);
        let c = sh.chan.load(Ordering::Relaxed) as u16;
        if let Ok(mut h) = sh.held.lock() {
            h.message(&sh.to, c, p1 as u8, (p1 >> 8) as u8, (p1 >> 16) as u8);
        }
    }

    let shared: &'static Shared = Box::leak(Box::new(Shared { held: Mutex::new(Held { down: vec![], pedal: vec![], hold: false }), to, chan, seen }));
    let mut open: Vec<isize> = Vec::new();
    let mut count = u32::MAX;
    loop {
        // SAFETY: plain calls into the system's MIDI library with valid pointers.
        let now = unsafe { midiInGetNumDevs() };
        if now != count {
            for h in open.drain(..) {
                unsafe {
                    midiInStop(h);
                    midiInClose(h);
                }
            }
            for id in 0..now {
                let mut h: isize = 0;
                let ok = unsafe { midiInOpen(&mut h, id, heard as *const () as usize, shared as *const Shared as usize, CALLBACK_FUNCTION) };
                if ok == 0 {
                    unsafe { midiInStart(h) };
                    open.push(h);
                }
            }
            count = now;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
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
