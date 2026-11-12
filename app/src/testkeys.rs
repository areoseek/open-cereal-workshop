//! Scripted input for tests on a hidden display: CEREAL_KEYS="1.0:key=Alt+T;1.5:text=Verse;2:key=Enter;3:click=400,300".
//! Each step is `seconds:what=argument`; the seconds count from the first frame.
//!
//! key=Ctrl+Shift+S   a key press and release, with modifiers
//! text=abc           typed text
//! move=x,y           pointer to a point
//! click=x,y          left click (rclick, dclick likewise)
//! down=x,y / up=x,y  press and release apart, for drags
//! wheel=Shift+1      wheel notches (negative is towards the user) at the pointer, with modifiers

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};

pub(crate) struct Script {
    steps: Vec<(f64, String, String)>,
    next: usize,
    start: Option<f64>,
    at: Pos2,
}

impl Script {
    pub(crate) fn from_env() -> Option<Self> {
        let text = std::env::var("CEREAL_KEYS").ok()?;
        let steps: Vec<(f64, String, String)> = text
            .split(';')
            .filter_map(|s| {
                let (t, rest) = s.trim().split_once(':')?;
                let (what, arg) = rest.split_once('=')?;
                Some((t.trim().parse().ok()?, what.trim().to_string(), arg.to_string()))
            })
            .collect();
        (!steps.is_empty()).then_some(Self { steps, next: 0, start: None, at: Pos2::ZERO })
    }

    pub(crate) fn pending(&self) -> bool {
        self.next < self.steps.len()
    }

    pub(crate) fn feed(&mut self, raw: &mut egui::RawInput) {
        let now = raw.time.unwrap_or(0.0);
        let start = *self.start.get_or_insert(now);
        // one step a frame, so a press is seen before what follows it
        let Some((t, what, arg)) = self.steps.get(self.next).cloned() else { return };
        if now - start < t {
            return;
        }
        self.next += 1;
        let point = |s: &str| {
            let (x, y) = s.split_once(',')?;
            Some(Pos2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
        };
        let (mods, last) = split_mods(&arg);
        let button = |raw: &mut egui::RawInput, at: Pos2, b: PointerButton, down: bool, up: bool| {
            raw.events.push(Event::PointerMoved(at));
            if down {
                raw.events.push(Event::PointerButton { pos: at, button: b, pressed: true, modifiers: Modifiers::NONE });
            }
            if up {
                raw.events.push(Event::PointerButton { pos: at, button: b, pressed: false, modifiers: Modifiers::NONE });
            }
        };
        match what.as_str() {
            "key" => {
                if let Some(key) = Key::from_name(last) {
                    raw.modifiers = mods;
                    raw.events.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods });
                    raw.events.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods });
                }
            }
            "text" => raw.events.push(Event::Text(arg.clone())),
            "alttext" => {
                raw.modifiers = Modifiers::ALT;
                raw.events.push(Event::Text(arg.clone()));
            }
            "wheel" => {
                raw.modifiers = mods;
                raw.events.push(Event::PointerMoved(self.at));
                raw.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: Vec2::new(0.0, last.parse().unwrap_or(0.0)), modifiers: mods });
            }
            "move" | "click" | "rclick" | "dclick" | "down" | "up" => {
                let Some(at) = point(&arg) else { return };
                self.at = at;
                match what.as_str() {
                    "move" => raw.events.push(Event::PointerMoved(at)),
                    "click" => button(raw, at, PointerButton::Primary, true, true),
                    "rclick" => button(raw, at, PointerButton::Secondary, true, true),
                    "dclick" => {
                        button(raw, at, PointerButton::Primary, true, true);
                        button(raw, at, PointerButton::Primary, true, true);
                    }
                    "down" => button(raw, at, PointerButton::Primary, true, false),
                    _ => button(raw, at, PointerButton::Primary, false, true),
                }
            }
            _ => {}
        }
    }
}

fn split_mods(arg: &str) -> (Modifiers, &str) {
    let mut mods = Modifiers::NONE;
    let mut rest = arg;
    loop {
        let Some((head, tail)) = rest.split_once('+') else { break };
        if tail.is_empty() {
            break;
        }
        match head {
            "Ctrl" => {
                mods.ctrl = true;
                mods.command = true;
            }
            "Shift" => mods.shift = true,
            "Alt" => mods.alt = true,
            _ => break,
        }
        rest = tail;
    }
    (mods, rest)
}
