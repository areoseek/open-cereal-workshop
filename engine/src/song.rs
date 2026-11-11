//! The project as the engine plays it: channels, patterns of notes, the playlist and the mixer.
//!
//! A `Song` is a plain value. The UI edits its own copy and publishes a snapshot to the audio thread, which only
//! ever reads.

use std::path::Path;
use std::sync::Arc;

use crate::flp::{self, ChannelKind, ClipSource, Flp};
use crate::sample::{Library, Sample};

pub type Color = [u8; 3];

#[derive(Debug, Clone)]
pub struct Chan {
    pub name: String,
    pub kind: ChannelKind,
    /// Generator name for plugin channels (empty for the sampler).
    pub plugin: String,
    pub sample_path: String,
    pub sample: Option<Arc<Sample>>,
    pub color: Color,
    pub enabled: bool,
    /// 0..=1 of the knob (file value / 12800).
    pub volume: f32,
    /// -1..=1.
    pub pan: f32,
    pub pitch_cents: f32,
    pub root_key: u8,
    /// Mixer insert, 0 = master.
    pub insert: usize,
    /// When the volume envelope is on, notes stop at their end; otherwise one-shots play out.
    pub env_on: bool,
    /// Which channel of the file this came from (None for one added since the last load or save).
    pub src: Option<u16>,
    /// The curve and what it drives, for an automation clip channel.
    pub auto: Option<Arc<Automation>>,
    /// Channels a layer channel plays.
    pub children: Vec<u16>,
}

/// A control an automation clip can drive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    ChanVolume(usize),
    ChanPan(usize),
    InsertVolume(usize),
    InsertPan(usize),
    Tempo,
    /// Something not played back yet (plugin controls, slot levels, ...): raw control number and destination.
    Other { param: u16, dest: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoPt {
    /// Ticks from the start of the clip.
    pub tick: f64,
    /// 0..=1 of the control's travel.
    pub value: f32,
    /// Bend of the line arriving at this point, -1..=1.
    pub tension: f32,
    /// Shape of the line arriving at this point.
    pub mode: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Automation {
    pub points: Vec<AutoPt>,
    pub targets: Vec<Target>,
}

/// Bend a 0..=1 ramp. Positive tension starts slowly and finishes fast. (Approximate: see docs/PARITY.md.)
fn bend(t: f32, tension: f32) -> f32 {
    if tension.abs() < 1e-4 {
        t
    } else {
        t.powf((tension.clamp(-1.0, 1.0) * 3.0).exp2())
    }
}

impl Automation {
    /// Value at `tick` from the start of the clip; holds the first and last values outside the points.
    pub fn value_at(&self, tick: f64) -> f32 {
        let Some(first) = self.points.first() else { return 0.0 };
        if tick <= first.tick {
            return first.value;
        }
        let i = self.points.partition_point(|p| p.tick <= tick);
        if i >= self.points.len() {
            return self.points[self.points.len() - 1].value;
        }
        let (a, b) = (self.points[i - 1], self.points[i]);
        let span = b.tick - a.tick;
        if span <= 0.0 {
            return b.value;
        }
        let t = ((tick - a.tick) / span) as f32;
        let k = b.tension;
        let shaped = match b.mode {
            2 => 0.0, // hold: stay on the earlier value until this point
            1 | 8 | 11 => {
                if t < 0.5 {
                    0.5 * bend(2.0 * t, k)
                } else {
                    1.0 - 0.5 * bend(2.0 - 2.0 * t, k)
                }
            }
            3 => (t * 4.0).floor() / 4.0,
            4 | 10 => t * t * (3.0 - 2.0 * t),
            9 => 0.5 - 0.5 * (std::f32::consts::PI * t).cos(),
            _ => bend(t, k),
        };
        a.value + (b.value - a.value) * shaped
    }

    /// Ticks from the first point to the last.
    pub fn length(&self) -> f64 {
        self.points.last().map(|p| p.tick).unwrap_or(0.0)
    }
}

/// Where the clips of one automation channel sit in the playlist; built by `Song::prepare`.
#[derive(Debug, Clone, Default)]
pub struct AutoPlan {
    pub chan: usize,
    /// (position, length, offset) of every sounding clip, by position.
    pub clips: Vec<(u32, u32, u32)>,
}

impl Default for Chan {
    fn default() -> Self {
        Self { name: "Sampler".into(), kind: ChannelKind::Sampler, plugin: String::new(), sample_path: String::new(), sample: None, color: [92, 101, 106], enabled: true, volume: 10000.0 / 12800.0, pan: 0.0, pitch_cents: 0.0, root_key: 60, insert: 0, env_on: false, src: None, auto: None, children: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub pos: u32,
    /// Ticks; 0 = a step (plays the whole sample).
    pub len: u32,
    pub chan: u16,
    /// 60 = C5.
    pub key: u8,
    /// 1.0 = the default velocity (100 of 128).
    pub vel: f32,
    /// -1..=1.
    pub pan: f32,
    pub fine_cents: f32,
    pub muted: bool,
}

impl Note {
    pub fn step(pos: u32, chan: u16) -> Self {
        Self { pos, len: 0, chan, key: 60, vel: 1.0, pan: 0.0, fine_cents: 0.0, muted: false }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pattern {
    pub name: String,
    pub color: Option<Color>,
    /// Kept sorted by position.
    pub notes: Vec<Note>,
}

impl Pattern {
    pub fn sort(&mut self) {
        self.notes.sort_by_key(|n| (n.pos, n.chan, n.key));
    }
    /// Length in ticks: the notes rounded up to whole bars, at least one bar.
    pub fn length(&self, bar: u32) -> u32 {
        let end = self.notes.iter().map(|n| n.pos + n.len.max(1)).max().unwrap_or(0);
        end.div_ceil(bar).max(1) * bar
    }
    pub fn has_step(&self, chan: u16, pos: u32) -> bool {
        self.notes.iter().any(|n| n.chan == chan && n.pos == pos)
    }
    /// Toggle a step-sequencer step. Returns the new state.
    pub fn toggle_step(&mut self, chan: u16, pos: u32) -> bool {
        let before = self.notes.len();
        self.notes.retain(|n| !(n.chan == chan && n.pos == pos));
        if self.notes.len() == before {
            self.notes.push(Note::step(pos, chan));
            self.sort();
            true
        } else {
            false
        }
    }
    pub fn set_step(&mut self, chan: u16, pos: u32, on: bool) {
        if self.has_step(chan, pos) != on {
            self.toggle_step(chan, pos);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipSrc {
    /// Index into `Song::patterns`.
    Pattern(usize),
    /// Index into `Song::channels` (an audio clip channel).
    Audio(usize),
    /// Index into `Song::channels` (an automation clip channel).
    Automation(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub pos: u32,
    pub len: u32,
    /// 0-based from the top.
    pub track: u16,
    pub src: ClipSrc,
    /// Ticks into the source at which the clip starts.
    pub offset: u32,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub name: String,
    pub color: Color,
    pub enabled: bool,
}

impl Default for Track {
    fn default() -> Self {
        Self { name: String::new(), color: [72, 81, 86], enabled: true }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Slot {
    pub index: u16,
    pub plugin: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Insert {
    pub name: String,
    pub color: Option<Color>,
    /// File value / 12800; 1.0 = 0 dB.
    pub volume: f32,
    pub pan: f32,
    pub enabled: bool,
    /// Inserts this one feeds. Empty on the master.
    pub sends: Vec<usize>,
    pub slots: Vec<Slot>,
}

impl Default for Insert {
    fn default() -> Self {
        Self { name: String::new(), color: None, volume: 1.0, pan: 0.0, enabled: true, sends: vec![], slots: vec![] }
    }
}

#[derive(Debug, Clone)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub comment: String,
    pub ppq: u32,
    pub tempo: f32,
    pub time_sig: (u8, u8),
    pub channels: Vec<Chan>,
    /// patterns[0] is "Pattern 1".
    pub patterns: Vec<Pattern>,
    pub tracks: Vec<Track>,
    /// Kept sorted by position.
    pub clips: Vec<Clip>,
    /// inserts[0] is the master.
    pub inserts: Vec<Insert>,
    /// Order in which the mixer processes inserts so every sender runs before what it feeds.
    pub mix_order: Vec<usize>,
    /// Automation clips that drive something, with their playlist positions (built by `prepare`).
    pub auto_plan: Vec<AutoPlan>,
}

pub const DEFAULT_TRACKS: usize = 500;
pub const DEFAULT_INSERTS: usize = 127;

fn color(v: u32) -> Color {
    [v as u8, (v >> 8) as u8, (v >> 16) as u8]
}

impl Song {
    /// A new, empty project: four empty sampler channels, one pattern, the default mixer.
    pub fn new() -> Self {
        let names = ["Kick", "Clap", "Hat", "Snare"];
        let mut inserts: Vec<Insert> = (0..DEFAULT_INSERTS).map(|i| Insert { sends: if i == 0 { vec![] } else { vec![0] }, ..Default::default() }).collect();
        inserts[0].name = "Master".into();
        let mut s = Self {
            title: String::new(),
            artist: String::new(),
            genre: String::new(),
            comment: String::new(),
            ppq: 96,
            tempo: 130.0,
            time_sig: (4, 4),
            channels: names.iter().map(|n| Chan { name: n.to_string(), ..Default::default() }).collect(),
            patterns: vec![Pattern::default()],
            tracks: vec![Track::default(); DEFAULT_TRACKS],
            clips: vec![],
            inserts,
            mix_order: vec![],
            auto_plan: vec![],
        };
        s.prepare();
        s
    }

    /// Ticks in one bar.
    pub fn bar(&self) -> u32 {
        let (n, d) = (self.time_sig.0.max(1) as u32, self.time_sig.1.max(1) as u32);
        (self.ppq * 4 * n / d).max(1)
    }
    /// Ticks in one step (a sixteenth).
    pub fn step(&self) -> u32 {
        (self.ppq / 4).max(1)
    }
    /// Playlist length in ticks, rounded up to a whole bar (at least one).
    pub fn length(&self) -> u32 {
        let bar = self.bar();
        let end = self.clips.iter().map(|c| c.pos + c.len).max().unwrap_or(0);
        end.div_ceil(bar).max(1) * bar
    }
    pub fn pattern_length(&self, p: usize) -> u32 {
        self.patterns.get(p).map(|p| p.length(self.bar())).unwrap_or(self.bar())
    }
    pub fn pattern_name(&self, p: usize) -> String {
        match self.patterns.get(p) {
            Some(pat) if !pat.name.is_empty() => pat.name.clone(),
            _ => format!("Pattern {}", p + 1),
        }
    }
    pub fn insert_name(&self, i: usize) -> String {
        match self.inserts.get(i) {
            Some(ins) if !ins.name.is_empty() => ins.name.clone(),
            _ if i == 0 => "Master".into(),
            _ => format!("Insert {i}"),
        }
    }
    pub fn ensure_pattern(&mut self, p: usize) {
        while self.patterns.len() <= p {
            self.patterns.push(Pattern::default());
        }
    }

    /// Remove a channel and renumber everything that refers to the ones after it.
    pub fn remove_channel(&mut self, ci: usize) {
        if ci >= self.channels.len() {
            return;
        }
        self.channels.remove(ci);
        for p in &mut self.patterns {
            p.notes.retain(|n| n.chan as usize != ci);
            for n in &mut p.notes {
                if n.chan as usize > ci {
                    n.chan -= 1;
                }
            }
        }
        self.clips.retain(|c| !matches!(c.src, ClipSrc::Audio(k) | ClipSrc::Automation(k) if k == ci));
        for c in &mut self.clips {
            match &mut c.src {
                ClipSrc::Audio(k) | ClipSrc::Automation(k) if *k > ci => *k -= 1,
                _ => {}
            }
        }
        for ch in &mut self.channels {
            ch.children.retain(|&k| k as usize != ci);
            for k in &mut ch.children {
                if *k as usize > ci {
                    *k -= 1;
                }
            }
            if let Some(a) = &ch.auto {
                if a.targets.iter().any(|t| matches!(t, Target::ChanVolume(k) | Target::ChanPan(k) if *k >= ci)) {
                    let mut a2 = (**a).clone();
                    a2.targets.retain(|t| !matches!(t, Target::ChanVolume(k) | Target::ChanPan(k) if *k == ci));
                    for t in &mut a2.targets {
                        match t {
                            Target::ChanVolume(k) | Target::ChanPan(k) if *k > ci => *k -= 1,
                            _ => {}
                        }
                    }
                    ch.auto = Some(Arc::new(a2));
                }
            }
        }
    }

    /// Sort what must be sorted and work out the mixer order. Call after editing, before publishing.
    pub fn prepare(&mut self) {
        self.clips.sort_by_key(|c| (c.pos, c.track));
        let mut plan: Vec<AutoPlan> = vec![];
        let mut slot: Vec<Option<usize>> = vec![None; self.channels.len()];
        for c in &self.clips {
            let ClipSrc::Automation(ch) = c.src else { continue };
            if c.muted || !self.tracks.get(c.track as usize).map(|t| t.enabled).unwrap_or(true) {
                continue;
            }
            let Some(chan) = self.channels.get(ch) else { continue };
            if !chan.enabled || !chan.auto.as_ref().map(|a| !a.targets.is_empty() && !a.points.is_empty()).unwrap_or(false) {
                continue;
            }
            let k = *slot[ch].get_or_insert_with(|| {
                plan.push(AutoPlan { chan: ch, clips: vec![] });
                plan.len() - 1
            });
            plan[k].clips.push((c.pos, c.len, c.offset));
        }
        self.auto_plan = plan;
        if self.inserts.is_empty() {
            self.inserts.push(Insert { name: "Master".into(), ..Default::default() });
        }
        let n = self.inserts.len();
        for c in &mut self.channels {
            if c.insert >= n {
                c.insert = 0;
            }
        }
        for (i, ins) in self.inserts.iter_mut().enumerate() {
            ins.sends.retain(|&d| d < n && d != i);
            if i == 0 {
                ins.sends.clear();
            }
        }
        // depth-first topological order: an insert is placed after everything that feeds it
        let mut order = Vec::with_capacity(n);
        let mut state = vec![0u8; n]; // 0 new, 1 in progress, 2 done
        let mut feeders: Vec<Vec<usize>> = vec![vec![]; n];
        for (i, ins) in self.inserts.iter().enumerate() {
            for &d in &ins.sends {
                feeders[d].push(i);
            }
        }
        fn visit(i: usize, feeders: &[Vec<usize>], state: &mut [u8], order: &mut Vec<usize>) {
            if state[i] != 0 {
                return; // done, or a feedback loop: break it here
            }
            state[i] = 1;
            for &f in &feeders[i] {
                visit(f, feeders, state, order);
            }
            state[i] = 2;
            order.push(i);
        }
        for i in 1..n {
            visit(i, &feeders, &mut state, &mut order);
        }
        visit(0, &feeders, &mut state, &mut order);
        // the master always goes last
        order.retain(|&i| i != 0);
        order.push(0);
        self.mix_order = order;
    }

    /// Build a song from a parsed project file, loading the samples it names through `lib`.
    pub fn from_flp(f: &Flp, lib: &mut Library, project_dir: Option<&Path>) -> Self {
        Self::from_flp_with(f, Some(lib), project_dir)
    }

    /// As `from_flp`; without a library no samples are loaded (enough to compare two versions of a project).
    pub fn from_flp_with(f: &Flp, mut lib: Option<&mut Library>, project_dir: Option<&Path>) -> Self {
        let ppq = f.header.ppq.max(1) as u32;
        let n_inserts = f.inserts.len().max(1);

        let base = flp::mixer_base(f.version_tuple());
        let target = |l: &flp::FlpLink| -> Target {
            use flp::param::*;
            let raw = (l.dest >> 6) as usize;
            if l.dest < 0x2000 {
                match l.param {
                    CHAN_VOLUME => Target::ChanVolume(l.dest as usize),
                    CHAN_PAN => Target::ChanPan(l.dest as usize),
                    _ => Target::Other { param: l.param, dest: l.dest },
                }
            } else if l.dest == flp::DEST_GLOBAL {
                match l.param {
                    GLOBAL_TEMPO => Target::Tempo,
                    _ => Target::Other { param: l.param, dest: l.dest },
                }
            } else if raw >= base && raw - base < n_inserts && l.dest & 63 == 0 {
                match l.param {
                    INSERT_VOLUME => Target::InsertVolume(raw - base),
                    INSERT_PAN => Target::InsertPan(raw - base),
                    _ => Target::Other { param: l.param, dest: l.dest },
                }
            } else {
                Target::Other { param: l.param, dest: l.dest }
            }
        };

        let channels: Vec<Chan> = f
            .channels
            .iter()
            .enumerate()
            .map(|(pos, c)| {
                let sample = match (c.kind, lib.as_deref_mut()) {
                    (ChannelKind::Sampler | ChannelKind::AudioClip, Some(lib)) => lib.load_stored(&c.sample_path, project_dir),
                    _ => None,
                };
                Chan {
                    name: if c.name.is_empty() { if c.plugin.is_empty() { "Sampler".into() } else { c.plugin.clone() } } else { c.name.clone() },
                    kind: c.kind,
                    plugin: c.plugin.clone(),
                    sample_path: c.sample_path.clone(),
                    sample,
                    color: color(c.color),
                    enabled: c.enabled,
                    volume: c.volume as f32 / 12800.0,
                    pan: (c.pan as f32 - 6400.0) / 6400.0,
                    pitch_cents: c.pitch_cents as f32,
                    root_key: c.root_key.min(131) as u8,
                    insert: if c.insert < 0 || c.insert as usize >= n_inserts { 0 } else { c.insert as usize },
                    env_on: c.envelopes.get(1).map(|e| e.enabled).unwrap_or(false),
                    src: Some(pos as u16),
                    auto: (c.kind == ChannelKind::AutomationClip).then(|| {
                        let mut tick = 0.0;
                        let points = flp::parse_automation(&c.automation)
                            .iter()
                            .map(|p| {
                                tick += p.delta * ppq as f64;
                                AutoPt { tick, value: p.value as f32, tension: p.tension, mode: p.mode }
                            })
                            .collect();
                        let targets = f.links.iter().filter(|l| l.kind == 0 && l.extra == 0 && l.source as usize == pos).map(target).collect();
                        Arc::new(Automation { points, targets })
                    }),
                    children: c.children.clone(),
                }
            })
            .collect();

        let max_pat = f
            .patterns
            .iter()
            .map(|p| p.index as usize)
            .chain(f.arrangements.iter().flat_map(|a| a.items.iter()).filter_map(|i| match i.source {
                ClipSource::Pattern(p) => Some(p as usize),
                _ => None,
            }))
            .chain(std::iter::once(f.current_pattern as usize))
            .max()
            .unwrap_or(1)
            .max(1);
        let mut patterns = vec![Pattern::default(); max_pat];
        for p in &f.patterns {
            if p.index == 0 {
                continue;
            }
            let out = &mut patterns[p.index as usize - 1];
            out.name = p.name.clone();
            out.color = p.color.map(color);
            out.notes = p
                .notes
                .iter()
                .map(|n| Note {
                    pos: n.position,
                    len: n.length,
                    chan: n.rack_channel,
                    key: n.key.min(131) as u8,
                    vel: n.velocity as f32 / 100.0,
                    pan: (n.pan as f32 - 64.0) / 64.0,
                    fine_cents: (n.fine_pitch as f32 - 120.0) * 10.0,
                    muted: n.flags & 0x2000 != 0,
                })
                .collect();
            out.sort();
        }

        let mut tracks = vec![Track::default(); DEFAULT_TRACKS];
        let mut clips = vec![];
        if let Some(a) = f.arrangements.first() {
            if !a.tracks.is_empty() {
                tracks = a.tracks.iter().map(|t| Track { name: t.name.clone(), color: color(t.color), enabled: t.enabled }).collect();
            }
            for it in &a.items {
                let (src, offset) = match it.source {
                    ClipSource::Pattern(p) => {
                        let off = it.start_offset as i32;
                        (ClipSrc::Pattern(p.max(1) as usize - 1), if off > 0 { off as u32 } else { 0 })
                    }
                    ClipSource::Channel(c) => {
                        let c = c as usize;
                        let beats = f32::from_bits(it.start_offset);
                        let off = if beats.is_finite() && beats > 0.0 { (beats * ppq as f32) as u32 } else { 0 };
                        match f.channels.get(c).map(|c| c.kind) {
                            Some(ChannelKind::AutomationClip) => (ClipSrc::Automation(c), off),
                            _ => (ClipSrc::Audio(c), off),
                        }
                    }
                };
                clips.push(Clip { pos: it.position, len: it.length, track: it.track, src, offset, muted: it.muted });
            }
        }

        let inserts: Vec<Insert> = if f.inserts.is_empty() {
            Song::new().inserts
        } else {
            f.inserts
                .iter()
                .enumerate()
                .map(|(i, ins)| {
                    let mut sends: Vec<usize> = ins.routes.iter().enumerate().filter(|(_, r)| **r).map(|(k, _)| k).collect();
                    if ins.routes.is_empty() && i != 0 {
                        sends.push(0);
                    }
                    Insert {
                        name: ins.name.clone(),
                        color: ins.color.map(color),
                        volume: ins.volume as f32 / 12800.0,
                        pan: ins.pan as f32 / 6400.0,
                        enabled: ins.flags & flp::INSERT_FLAG_ENABLED != 0 || ins.flags == 0,
                        sends,
                        slots: ins.slots.iter().map(|s| Slot { index: s.index, plugin: s.plugin.clone(), name: s.name.clone(), enabled: s.enabled }).collect(),
                    }
                })
                .collect()
        };

        let mut song = Song {
            title: f.title.clone(),
            artist: f.artist.clone(),
            genre: f.genre.clone(),
            comment: f.comment.clone(),
            ppq,
            tempo: if f.tempo > 0.0 { f.tempo } else { 130.0 },
            time_sig: (f.time_sig.0.max(1), f.time_sig.1.max(1)),
            channels,
            patterns,
            tracks,
            clips,
            inserts,
            mix_order: vec![],
            auto_plan: vec![],
        };
        song.prepare();
        song
    }
}

impl Default for Song {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_song_is_sane() {
        let s = Song::new();
        assert_eq!(s.bar(), 384);
        assert_eq!(s.step(), 24);
        assert_eq!(s.length(), 384);
        assert_eq!(s.mix_order.len(), DEFAULT_INSERTS);
        assert_eq!(*s.mix_order.last().unwrap(), 0);
    }

    #[test]
    fn steps_toggle_and_length_rounds_to_bars() {
        let mut p = Pattern::default();
        assert!(p.toggle_step(0, 0));
        assert!(p.toggle_step(0, 400));
        assert!(p.has_step(0, 400));
        assert_eq!(p.length(384), 768);
        assert!(!p.toggle_step(0, 400));
        assert_eq!(p.length(384), 384);
    }

    #[test]
    fn mix_order_puts_senders_first() {
        let mut s = Song::new();
        s.inserts[5].sends = vec![3]; // 5 feeds 3, which feeds the master
        s.prepare();
        let at = |i: usize| s.mix_order.iter().position(|&x| x == i).unwrap();
        assert!(at(5) < at(3));
        assert!(at(3) < at(0));
    }
}
