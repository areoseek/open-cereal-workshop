//! The note script unit of the patch bay: a short Python program that is handed the notes on their way
//! to an instrument and plays whatever notes it likes in their place.
//!
//! The stored block is a length and then a text in the shape of a settings file: the program itself under
//! `[Script]`, its knobs under `[Parameters]`. The program runs in the Python found on the machine
//! (see `pyrt`), against a module of our own that stands where the program expects its host.
//!
//! A program inside a project is somebody's code. It is only run when the person at the machine has said
//! that this very text may run; until then the unit lets notes through as they are and the text is put on
//! a list the application can show.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptDef {
    pub source: String,
    /// `Name=value` lines as stored, one knob each.
    pub params: String,
}

impl ScriptDef {
    pub fn decode(b: &[u8]) -> Option<Self> {
        let n = u32::from_le_bytes(b.get(..4)?.try_into().ok()?) as usize;
        let text = String::from_utf8_lossy(b.get(4..4 + n)?);
        let text = text.trim_start_matches('\u{feff}');
        let mut section = "";
        let mut d = ScriptDef::default();
        for line in text.split('\n') {
            let line = line.trim_end_matches('\r');
            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }
            match section {
                "Script" => {
                    if let Some(s) = line.strip_prefix("Source=") {
                        d.source = s.replace("<linebreak>", "\n");
                    }
                }
                "Parameters" if line.contains('=') => {
                    d.params.push_str(line);
                    d.params.push('\n');
                }
                _ => {}
            }
        }
        Some(d)
    }

    /// The title the program gives itself, without running it: the first quoted text handed to its panel.
    pub fn title(&self) -> String {
        let Some(at) = self.source.find("ScriptDialog(") else { return String::new() };
        let rest = &self.source[at + 13..];
        let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { return String::new() };
        rest[1..].split(q).next().unwrap_or("").chars().take(60).collect()
    }
}

/// Which programs may run.
pub enum Trust {
    All,
    /// Only these very texts.
    These(HashSet<String>),
}

static TRUST: RwLock<Option<Trust>> = RwLock::new(None);
static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn set_trust(t: Trust) {
    *TRUST.write().unwrap() = Some(t);
    ASKED.lock().unwrap().clear();
}

/// Let one more text run.
pub fn allow(source: &str) {
    let mut t = TRUST.write().unwrap();
    match t.as_mut() {
        Some(Trust::All) => {}
        Some(Trust::These(s)) => {
            s.insert(source.to_string());
        }
        None => *t = Some(Trust::These(HashSet::from([source.to_string()]))),
    }
    ASKED.lock().unwrap().retain(|s| s != source);
}

pub fn allowed(source: &str) -> bool {
    match TRUST.read().unwrap().as_ref() {
        Some(Trust::All) => true,
        Some(Trust::These(s)) => s.contains(source),
        None => false,
    }
}

/// The programs that asked to run and were not let: for the application to put before the user.
pub fn waiting() -> Vec<String> {
    ASKED.lock().unwrap().clone()
}

fn ask(source: &str) {
    let mut a = ASKED.lock().unwrap();
    if a.len() < 64 && !a.iter().any(|s| s == source) {
        a.push(source.to_string());
    }
}

/// A note as a program sees it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V {
    /// The key, fractions and all.
    pub note: f32,
    /// On top of the key, in keys.
    pub fine: f32,
    /// 0..1, where a stored note's default is 0.78.
    pub vel: f32,
    pub pan: f32,
    /// The socket of the unit the note leaves by.
    pub output: i32,
    pub fcut: f32,
    pub fres: f32,
    pub color: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// A note starts: the tick of the block it starts on (1 = the first), its number, what it is.
    On(u32, u32, V),
    Change(u32, u32, V),
    Off(u32, u32),
    /// An output setting moved: which, and where to.
    Ctl(usize, f32),
    Err(String),
}

/// One control of a program's panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Control {
    /// knob, int, check, combo, text or free.
    pub kind: String,
    pub name: String,
    pub value: String,
    pub lo: f32,
    pub hi: f32,
    /// Where it stands in its travel, 0..1.
    pub normal: f32,
    pub opts: Vec<String>,
    pub hint: String,
    /// The value as a preset keeps it.
    pub stored: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Panel {
    pub title: String,
    pub description: String,
    pub controls: Vec<Control>,
    pub outputs: Vec<(String, f32)>,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// A program, running.
pub struct Inst {
    id: String,
    py: &'static crate::pyrt::Py,
    /// What went wrong, the first time each thing did.
    pub errors: Vec<String>,
}

fn parse(text: &str, errors: &mut Vec<String>) -> Vec<Out> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut w = line.split(' ');
        let op = w.next().unwrap_or("");
        if op == "!" {
            let e = line[1..].trim().to_string();
            if errors.len() < 32 {
                errors.push(e.clone());
            }
            out.push(Out::Err(e));
            continue;
        }
        let mut num = || w.next().and_then(|s| s.parse::<f32>().ok()).filter(|v| v.is_finite());
        let got = (|| match op {
            "+" | "~" => {
                let (k, id) = (num()? as u32, num()? as u32);
                let v = V { note: num()?, fine: num()?, vel: num()?, pan: num()?, output: num()? as i32, fcut: num()?, fres: num()?, color: num()? as u8 & 15 };
                Some(if op == "+" { Out::On(k, id, v) } else { Out::Change(k, id, v) })
            }
            "-" => Some(Out::Off(num()? as u32, num()? as u32)),
            "c" => Some(Out::Ctl(num()? as usize, num()?)),
            _ => None,
        })();
        out.extend(got);
    }
    out
}

impl Inst {
    /// Start the program, if there is a Python to run it and it has been let.
    pub fn new(def: &ScriptDef) -> Option<Inst> {
        if !allowed(&def.source) {
            ask(&def.source);
            return None;
        }
        let py = crate::pyrt::get()?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed).to_string();
        let said = py.call(&format!("new {id}"), &def.source, &def.params)?;
        let mut inst = Inst { id, py, errors: Vec::new() };
        parse(&said, &mut inst.errors);
        Some(inst)
    }

    /// Hand the program a block's worth of lines and take what it did.
    pub fn step(&mut self, lines: &str) -> Vec<Out> {
        match self.py.call(&format!("step {}", self.id), lines, "") {
            Some(s) => parse(&s, &mut self.errors),
            None => Vec::new(),
        }
    }

    /// The panel the program asked for, as it stands.
    pub fn panel(&self) -> Panel {
        let mut p = Panel::default();
        let Some(s) = self.py.call(&format!("describe {}", self.id), "", "") else { return p };
        for row in s.lines() {
            let c: Vec<&str> = row.split('\t').collect();
            match c[0] {
                "d" if c.len() >= 3 => {
                    p.title = c[1].to_string();
                    p.description = c[2].to_string();
                }
                "k" if c.len() >= 10 => p.controls.push(Control {
                    kind: c[1].to_string(),
                    name: c[2].to_string(),
                    value: c[3].to_string(),
                    lo: c[4].parse().unwrap_or(0.0),
                    hi: c[5].parse().unwrap_or(1.0),
                    normal: c[6].parse().unwrap_or(0.0),
                    opts: if c[7].is_empty() { Vec::new() } else { c[7].split('|').map(str::to_string).collect() },
                    hint: c[8].to_string(),
                    stored: c[9].to_string(),
                }),
                "o" if c.len() >= 3 => p.outputs.push((c[1].to_string(), c[2].parse().unwrap_or(0.0))),
                _ => {}
            }
        }
        p
    }

    /// What the program printed since this was last asked.
    pub fn printed(&self) -> String {
        self.py.call("log 0", "", "").unwrap_or_default()
    }
}

impl Drop for Inst {
    fn drop(&mut self) {
        self.py.call(&format!("drop {}", self.id), "", "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(source: &str, params: &str) -> Vec<u8> {
        let text = format!("\u{feff}[Script]\r\nSource={}\r\n[Parameters]\r\n{}", source.replace('\n', "<linebreak>"), params);
        let mut b = (text.len() as u32).to_le_bytes().to_vec();
        b.extend_from_slice(text.as_bytes());
        b
    }

    const ECHO: &str = "import flvfx as vfx\nimport math\n\ndef createDialog():\n    form = vfx.ScriptDialog('Echo up', 'Plays each note again, higher.')\n    form.addInputKnobInt('Keys', 12, 0, 24, hint='How far up')\n    form.addGroup('Late')\n    form.addInputKnob('Ticks', 0.0, 0.0, 96.0)\n    form.endGroup()\n    form.addInputCombo('Way', 'up,down', 0)\n    return form\n\nlate = []\n\ndef onTriggerVoice(incoming):\n    v = vfx.Voice(incoming)\n    incoming.mine = v\n    v.trigger()\n    late.append([vfx.context.ticks + int(vfx.context.form.getInputValue('Ticks')), incoming])\n\ndef onTick():\n    for e in list(late):\n        if vfx.context.ticks >= e[0]:\n            late.remove(e)\n            up = vfx.Voice(e[1])\n            sign = -1 if vfx.context.form.getInputValue('Way') else 1\n            up.note += sign * vfx.context.form.getInputValue('Keys')\n            up.length = 10\n            up.trigger()\n\ndef onReleaseVoice(incoming):\n    incoming.mine.release()\n";

    #[test]
    fn a_stored_block_gives_back_the_program_and_its_knobs() {
        let d = ScriptDef::decode(&block(ECHO, "Keys=7\r\nLate: Ticks=131072\r\n")).unwrap();
        assert_eq!(d.source, ECHO);
        assert_eq!(d.params, "Keys=7\nLate: Ticks=131072\n");
        assert_eq!(d.title(), "Echo up");
    }

    #[test]
    fn a_program_nobody_let_run_does_not_run_and_is_put_on_the_list() {
        let d = ScriptDef { source: "raise SystemExit # never let".into(), params: String::new() };
        assert!(Inst::new(&d).is_none());
        assert!(waiting().contains(&d.source));
    }

    #[test]
    fn a_program_hears_notes_and_ticks_and_plays_its_own() {
        if crate::pyrt::get().is_none() {
            eprintln!("no Python on this machine: skipped");
            return;
        }
        let d = ScriptDef::decode(&block(ECHO, "Keys=7\r\nLate: Ticks=131072\r\n")).unwrap();
        allow(&d.source);
        let mut i = Inst::new(&d).unwrap();
        assert!(i.errors.is_empty(), "{:?}", i.errors);
        let p = i.panel();
        assert_eq!(p.title, "Echo up");
        assert_eq!(p.controls.len(), 3);
        assert_eq!((p.controls[0].name.as_str(), p.controls[0].value.as_str()), ("Keys", "7"));
        // half the travel of 0..96
        assert_eq!((p.controls[1].name.as_str(), p.controls[1].value.as_str()), ("Late: Ticks", "48.0"));
        assert_eq!(p.controls[2].opts, ["up", "down"]);

        let out = i.step("c 96 1 140.0 0\nn 1 60.0 0.78 0.0 0.0 0.0 3 0\nt 40");
        assert_eq!(out, [Out::On(0, 1, V { note: 60.0, vel: 0.78, color: 3, ..Default::default() })]);
        // the late copy comes 48 ticks on, seven keys up, and lets itself go ten ticks after that
        let out = i.step("t 30");
        assert_eq!(out, [Out::On(8, 2, V { note: 67.0, vel: 0.78, color: 3, ..Default::default() }), Out::Off(18, 2)]);
        // a knob turned from outside is heard by the program
        i.step("p Way\t1.0\nn 2 60.0 0.5 0.0 0.0 0.0 0 0");
        let out = i.step("t 60\nr 1\nr 2");
        assert!(out.contains(&Out::On(48, 4, V { note: 53.0, vel: 0.5, ..Default::default() })), "{out:?}");
        assert!(out.contains(&Out::Off(60, 1)) && out.contains(&Out::Off(60, 3)), "{out:?}");
        assert!(i.errors.is_empty(), "{:?}", i.errors);
    }

    #[test]
    fn every_stock_script_starts_and_lives_through_notes_without_going_wrong() {
        let Ok(dir) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        if crate::pyrt::get().is_none() {
            return;
        }
        let dir = std::path::PathBuf::from(dir).join("Patches/Plugin presets/Effects/VFX Script");
        fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for f in std::fs::read_dir(d).into_iter().flatten().flatten().map(|f| f.path()) {
                if f.is_dir() {
                    walk(&f, out);
                } else if f.extension().is_some_and(|e| e == "fst") {
                    out.push(f);
                }
            }
        }
        let mut list = Vec::new();
        walk(&dir, &mut list);
        let (mut seen, mut played, mut bad) = (0, 0, Vec::new());
        for f in list {
            let pre = crate::preset::read(&f).unwrap();
            let d = ScriptDef::decode(&pre.state).unwrap();
            assert!(!d.source.is_empty(), "{f:?}");
            allow(&d.source);
            let mut i = Inst::new(&d).unwrap();
            seen += 1;
            let mut outs = i.step("c 96 1 140.0 0\nn 1 60.0 0.78 0.0 0.0 0.0 0 0\nn 2 64.0 0.5 0.0 0.0 0.0 0 0\nt 50");
            outs.extend(i.step("n 3 67.0 0.78 0.0 0.0 0.0 0 0\nt 200\nr 1"));
            outs.extend(i.step("t 200\nr 2\nr 3\nt 800"));
            if outs.iter().any(|o| matches!(o, Out::On(..))) {
                played += 1;
            }
            // every stored knob belongs to a control the program made, unless it has a drawn panel with controls of its own
            let p = i.panel();
            for line in d.params.lines().filter(|_| !d.source.contains("InputSurface")) {
                let name = line.split('=').next().unwrap();
                if !p.controls.iter().any(|c| c.name == name) {
                    bad.push(format!("{}: no control {name:?}", f.file_name().unwrap().to_string_lossy()));
                }
            }
            for e in &i.errors {
                bad.push(format!("{}: {e}", f.file_name().unwrap().to_string_lossy()));
            }
        }
        assert!(bad.is_empty(), "{seen} seen\n{}", bad.join("\n"));
        eprintln!("{seen} seen, {played} played");
        assert!(seen > 50, "{seen} seen, {played} played");
    }

    #[test]
    fn a_program_that_goes_wrong_says_so_once_and_the_rest_carries_on() {
        if crate::pyrt::get().is_none() {
            return;
        }
        let d = ScriptDef { source: "import flvfx\ndef onTick():\n    print('tick')\n    nothing_here()\ndef onTriggerVoice(v):\n    flvfx.Voice(v).trigger()\n".into(), params: String::new() };
        allow(&d.source);
        let mut i = Inst::new(&d).unwrap();
        let out = i.step("t 5\nn 1 40.0 1.0 0.0 0.0 0.0 0 0");
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(&out[0], Out::Err(e) if e.contains("NameError")), "{out:?}");
        assert!(matches!(out[1], Out::On(5, 1, _)));
        assert_eq!(i.errors.len(), 1);
        assert!(i.printed().contains("tick"));
    }
}
