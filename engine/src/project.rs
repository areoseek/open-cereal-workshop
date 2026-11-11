//! The project model and our own save format.
//!
//! A project is plain data: no atomics, no audio state. It is what we save, load and render offline. The live
//! `Shared` is built from it and written back to it. The on-disk format is our own, line based and versioned, so
//! it owes nothing to any other product; `.flp` import/export is a later, separate concern (docs/ROADMAP.md).

use crate::{Channel, Drum, Shared, STEPS};
use std::fmt::Write as _;
use std::sync::Arc;

pub const FORMAT: &str = "breakfast-cereal-project";
pub const FORMAT_VERSION: u32 = 1;
pub const EXT: &str = "cereal";

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelData {
    pub name: String,
    pub drum: Drum,
    pub steps: [bool; STEPS],
    pub volume: f32,
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub title: String,
    pub bpm: f32,
    pub master: f32,
    pub channels: Vec<ChannelData>,
}

impl Drum {
    pub fn tag(self) -> &'static str {
        match self {
            Drum::Kick => "kick",
            Drum::Clap => "clap",
            Drum::Hat => "hat",
            Drum::Snare => "snare",
        }
    }
    pub fn from_tag(s: &str) -> Option<Self> {
        Some(match s {
            "kick" => Drum::Kick,
            "clap" => Drum::Clap,
            "hat" => Drum::Hat,
            "snare" => Drum::Snare,
            _ => return None,
        })
    }
}

impl Project {
    pub fn from_shared(sh: &Shared, title: &str) -> Self {
        Self {
            title: title.to_string(),
            bpm: sh.bpm(),
            master: sh.master(),
            channels: sh
                .channels
                .iter()
                .map(|c| ChannelData {
                    name: c.name.to_string(),
                    drum: c.drum,
                    steps: std::array::from_fn(|i| c.step(i)),
                    volume: c.volume(),
                    muted: c.muted(),
                })
                .collect(),
        }
    }

    /// Write our values onto an existing live state (same channel order and count).
    pub fn apply_to(&self, sh: &Shared) {
        sh.set_bpm(self.bpm);
        sh.set_master(self.master);
        for (c, d) in sh.channels.iter().zip(&self.channels) {
            for i in 0..STEPS {
                c.set_step(i, d.steps[i]);
            }
            c.set_volume(d.volume);
            c.set_muted(d.muted);
        }
    }

    /// Build a fresh live state from this project. `name` is a leaked static because `Channel::name`
    /// is `&'static str`; projects are few and long lived, so this is fine.
    pub fn to_shared(&self) -> Arc<Shared> {
        let channels = self
            .channels
            .iter()
            .map(|d| {
                let on: Vec<usize> = (0..STEPS).filter(|&i| d.steps[i]).collect();
                let name: &'static str = Box::leak(d.name.clone().into_boxed_str());
                let c = Channel::new(name, d.drum, &on, d.volume);
                c.set_muted(d.muted);
                c
            })
            .collect();
        Shared::from_parts(channels, self.bpm, self.master)
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{FORMAT} {FORMAT_VERSION}");
        let _ = writeln!(s, "title {}", escape(&self.title));
        let _ = writeln!(s, "bpm {}", self.bpm);
        let _ = writeln!(s, "master {}", self.master);
        for c in &self.channels {
            let bits: String = (0..STEPS).map(|i| if c.steps[i] { '1' } else { '0' }).collect();
            let _ = writeln!(s, "channel {} {} {} {} {}", c.drum.tag(), bits, c.volume, c.muted as u8, escape(&c.name));
        }
        s
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default();
        let mut hp = header.split_whitespace();
        if hp.next() != Some(FORMAT) {
            return Err("not a Breakfast Cereal project file".into());
        }
        let ver: u32 = hp.next().and_then(|v| v.parse().ok()).ok_or("missing format version")?;
        if ver > FORMAT_VERSION {
            return Err(format!("this file is version {ver}; this build reads up to {FORMAT_VERSION}"));
        }
        let mut p = Project { title: String::new(), bpm: 130.0, master: 0.8, channels: vec![] };
        for (n, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            let bad = |what: &str| format!("line {}: bad {what}", n + 2);
            match key {
                "title" => p.title = unescape(rest),
                "bpm" => p.bpm = rest.parse().map_err(|_| bad("bpm"))?,
                "master" => p.master = rest.parse().map_err(|_| bad("master"))?,
                "channel" => {
                    let mut f = rest.splitn(5, char::is_whitespace);
                    let drum = f.next().and_then(Drum::from_tag).ok_or_else(|| bad("channel drum"))?;
                    let bits = f.next().ok_or_else(|| bad("channel steps"))?;
                    if bits.len() != STEPS {
                        return Err(bad("channel step count"));
                    }
                    let mut steps = [false; STEPS];
                    for (i, ch) in bits.chars().enumerate() {
                        steps[i] = ch == '1';
                    }
                    let volume = f.next().and_then(|v| v.parse().ok()).ok_or_else(|| bad("channel volume"))?;
                    let muted = f.next().map(|v| v == "1").unwrap_or(false);
                    let name = f.next().map(unescape).unwrap_or_else(|| drum.tag().to_string());
                    p.channels.push(ChannelData { name, drum, steps, volume, muted });
                }
                _ => {} // unknown keys are ignored so older builds tolerate newer files
            }
        }
        if p.channels.is_empty() {
            return Err("project has no channels".into());
        }
        Ok(p)
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let sh = Shared::new();
        sh.set_bpm(142.5);
        sh.channels[3].toggle(3);
        sh.channels[0].set_volume(1.1);
        sh.channels[1].set_muted(true);
        let p = Project::from_shared(&sh, "My beat \\ test");
        let p2 = Project::from_text(&p.to_text()).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.title, "My beat \\ test");
        assert_eq!(p2.bpm, 142.5);
        assert!(p2.channels[3].steps[3]);
        assert!(p2.channels[1].muted);
    }

    #[test]
    fn to_shared_reproduces_state() {
        let sh = Shared::new();
        sh.set_bpm(99.0);
        sh.channels[2].toggle(5);
        let p = Project::from_shared(&sh, "t");
        let sh2 = p.to_shared();
        assert_eq!(sh2.bpm(), 99.0);
        assert!(sh2.channels[2].step(5));
        assert_eq!(sh2.channels.len(), sh.channels.len());
    }

    #[test]
    fn rejects_foreign_and_newer_files() {
        assert!(Project::from_text("some other file\n").is_err());
        assert!(Project::from_text(&format!("{FORMAT} 999\n")).is_err());
    }
}
