//! Notes from MIDI keyboards and pads. Every input device is listened to (as with all inputs enabled in the
//! reference) and what is played goes to the selected channel. The hold pedal keeps notes ringing.
//!
//! Linux: a sequencer client with one input port that connects itself to every readable port, including
//! devices plugged in later. Other systems: nothing yet.

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
            EventType::Controller => {
                if let Some(cc) = ev.get_data::<EvCtrl>() {
                    if cc.param == 64 {
                        held.set_hold(&to, cc.value >= 64);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(not(target_os = "linux"))]
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
    }
}
