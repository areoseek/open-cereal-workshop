//! Key commands of the channel rack and the mixer, and the helpers the commands that work everywhere lean on.

use std::path::PathBuf;

use egui::{Key, Modifiers};

use cereal_engine::song::Note;

use crate::{App, Win};

/// Something the wheel asked to move while the windows were being drawn; done once they are.
#[derive(Clone, Copy)]
pub(crate) enum WheelMove {
    Channel(usize, bool),
    Insert(usize, bool),
    Track(usize, bool),
    Clip(usize, bool),
}

impl App {
    /// Shift and the wheel over something that can be moved: the turn is taken before anything scrolls with it.
    pub(crate) fn take_wheel(&mut self, raw: &mut egui::RawInput) {
        let Some(over) = self.wheel_over else { return };
        let wanted = raw.modifiers.shift && (raw.modifiers.alt == matches!(over, WheelMove::Clip(..)));
        if !wanted {
            return;
        }
        let mut turned = 0.0;
        raw.events.retain(|e| match e {
            egui::Event::MouseWheel { unit, delta, .. } => {
                turned += (delta.x + delta.y) / if *unit == egui::MouseWheelUnit::Point { 50.0 } else { 1.0 };
                false
            }
            _ => true,
        });
        self.wheel_acc += turned;
        let steps = self.wheel_acc.trunc();
        self.wheel_acc -= steps;
        if steps != 0.0 {
            let up = steps > 0.0;
            self.wheel_move = Some(match over {
                WheelMove::Channel(i, _) => WheelMove::Channel(i, up),
                WheelMove::Insert(i, _) => WheelMove::Insert(i, up),
                WheelMove::Track(i, _) => WheelMove::Track(i, up),
                WheelMove::Clip(i, _) => WheelMove::Clip(i, !up),
            });
        }
    }

    pub(crate) fn do_wheel_move(&mut self) {
        let Some(mv) = self.wheel_move.take() else { return };
        match mv {
            WheelMove::Channel(ci, up) => {
                let n = self.song.channels.len();
                let to = if up { ci.checked_sub(1) } else { Some(ci + 1).filter(|t| *t < n) };
                if let Some(to) = to.filter(|_| ci < n) {
                    self.song.swap_channels(ci, to);
                    if self.sel_chan == ci {
                        self.sel_chan = to;
                    } else if self.sel_chan == to {
                        self.sel_chan = ci;
                    }
                    self.dirty = true;
                }
            }
            WheelMove::Insert(i, left) => {
                let n = self.song.inserts.len();
                let to = if left { Some(i.wrapping_sub(1)).filter(|_| i > 1) } else { Some(i + 1).filter(|_| i > 0 && i + 2 < n) };
                if let Some(to) = to {
                    self.song.swap_inserts(i, to);
                    if self.sel_insert == i {
                        self.sel_insert = to;
                    } else if self.sel_insert == to {
                        self.sel_insert = i;
                    }
                    self.dirty = true;
                }
            }
            WheelMove::Track(t, up) => {
                let n = self.song.tracks.len();
                let to = if up { t.checked_sub(1) } else { Some(t + 1).filter(|x| *x < n) };
                if let Some(to) = to {
                    self.song.swap_tracks(t, to);
                    self.dirty = true;
                }
            }
            WheelMove::Clip(i, later) => self.pl_nudge_clip(i, later),
        }
    }
}

/// The digit keys in the order the first ten channels are counted: 1 to 9, then 0.
pub(crate) const DIGITS: [Key; 10] = [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];

/// The windows the Tab key walks through, with the names egui knows them by.
const WINDOWS: [(Win, &str); 4] = [(Win::Playlist, "Playlist"), (Win::Rack, "Channel rack"), (Win::Piano, "Piano roll"), (Win::Mixer, "Mixer")];

impl App {
    /// Start the song, or hold it back for the first note when that is asked for and recording is armed.
    pub(crate) fn play(&mut self) {
        let tr = &self.handle.transport;
        if self.wait_input && tr.recording() {
            tr.set_waiting(true);
            self.status = "Waiting for a note".into();
        } else {
            tr.set_playing(true);
        }
    }

    /// Only this channel sounds; asked again, all of them do.
    pub(crate) fn solo_channel(&mut self, ci: usize) {
        let solo = self.song.channels.iter().enumerate().all(|(k, c)| c.enabled == (k == ci));
        for (k, c) in self.song.channels.iter_mut().enumerate() {
            c.enabled = solo || k == ci;
        }
        self.dirty = true;
    }

    pub(crate) fn clone_channel(&mut self, ci: usize) {
        let Some(mut c) = self.song.channels.get(ci).cloned() else { return };
        c.name = format!("{} #2", c.name);
        c.ext = c.ext.as_ref().map(|h| h.fork());
        if let Some(b) = c.bay.as_mut() {
            b.fx.ext = b.fx.ext.as_ref().map(|h| h.fork());
        }
        self.song.channels.push(c);
        self.dirty = true;
    }

    /// Send a channel to a mixer track, and let the track take the channel's name and colour.
    pub(crate) fn link_channel(&mut self, ci: usize, ins: usize) {
        if ci >= self.song.channels.len() || ins >= self.song.inserts.len() {
            return;
        }
        let (name, color) = (self.song.channels[ci].name.clone(), self.song.channels[ci].color);
        self.song.channels[ci].insert = ins;
        if ins > 0 {
            self.song.inserts[ins].name = name;
            self.song.inserts[ins].color = Some(color);
        }
        self.dirty = true;
    }

    /// The first mixer track nothing plays into and nothing sits on.
    fn free_insert(&self) -> Option<usize> {
        let n = self.song.inserts.len();
        // the last track is the one new channels may be heard through, so it is never handed out
        (1..n.saturating_sub(1)).find(|&i| self.song.channels.iter().all(|c| c.insert != i) && self.song.inserts[i].slots.is_empty() && self.song.inserts[i].name.is_empty())
    }

    /// The selected channel's notes of this pattern, for the rack's copy and cut.
    pub(crate) fn rack_copy(&mut self, cut: bool) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let chan = self.sel_chan as u16;
        self.rack.clip = self.song.patterns[pat].notes.iter().filter(|n| n.chan == chan).cloned().collect();
        if cut && !self.rack.clip.is_empty() {
            self.song.patterns[pat].notes.retain(|n| n.chan != chan);
            self.dirty = true;
        }
    }

    pub(crate) fn rack_paste(&mut self) {
        if self.rack.clip.is_empty() {
            return;
        }
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let chan = self.sel_chan as u16;
        let add: Vec<Note> = self.rack.clip.iter().cloned().map(|n| Note { chan, ..n }).collect();
        let notes = &mut self.song.patterns[pat].notes;
        notes.retain(|n| n.chan != chan);
        notes.extend(add);
        self.song.patterns[pat].sort();
        self.dirty = true;
    }

    /// Turn the selected channel's notes of this pattern round the pattern by one step.
    fn rack_shift(&mut self, right: bool) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let chan = self.sel_chan as u16;
        let step = self.song.step();
        let len = self.song.pattern_length(pat).max(step);
        for n in self.song.patterns[pat].notes.iter_mut().filter(|n| n.chan == chan) {
            n.pos = if right { (n.pos + step) % len } else { (n.pos + len - step % len) % len };
        }
        self.song.patterns[pat].sort();
        self.dirty = true;
    }

    /// A key press while the channel rack has the focus. True when it was ours.
    pub(crate) fn rack_key(&mut self, key: Key, m: Modifiers, repeat: bool) -> bool {
        let n = self.song.channels.len();
        if n == 0 {
            return false;
        }
        let ci = self.sel_chan.min(n - 1);
        if let Some(d) = DIGITS.iter().position(|k| *k == key).filter(|_| !m.alt && !m.shift) {
            // with the typing keyboard on, the plain digits are piano keys
            if self.typing_keys && !m.command {
                return false;
            }
            let k = (d + 9) % 10;
            if k < n && !repeat {
                if m.command {
                    self.solo_channel(k);
                } else {
                    self.song.channels[k].enabled = !self.song.channels[k].enabled;
                    self.dirty = true;
                }
            }
            return true;
        }
        if m.command && m.shift {
            match key {
                Key::ArrowLeft => self.rack_shift(false),
                Key::ArrowRight => self.rack_shift(true),
                _ => return false,
            }
            return true;
        }
        if m.command {
            match key {
                Key::L if !repeat => match self.free_insert() {
                    Some(i) => {
                        self.link_channel(ci, i);
                        self.sel_insert = i;
                    }
                    None => self.status = "No free mixer track".into(),
                },
                Key::Enter => {
                    self.show_playlist = true;
                    self.focus_window(Win::Playlist);
                }
                _ => return false,
            }
            return true;
        }
        if m.alt {
            match key {
                Key::ArrowUp if ci > 0 => {
                    self.song.swap_channels(ci, ci - 1);
                    self.sel_chan = ci - 1;
                    self.dirty = true;
                }
                Key::ArrowDown if ci + 1 < n => {
                    self.song.swap_channels(ci, ci + 1);
                    self.sel_chan = ci + 1;
                    self.dirty = true;
                }
                Key::ArrowUp | Key::ArrowDown => {}
                Key::C if !repeat => self.clone_channel(ci),
                Key::Delete if !repeat => self.delete_channel(ci),
                Key::M if !repeat => self.rack.hide_selectors = !self.rack.hide_selectors,
                _ => return false,
            }
            return true;
        }
        if m.shift {
            return false;
        }
        match key {
            Key::ArrowUp => self.sel_chan = ci.saturating_sub(1),
            Key::ArrowDown => self.sel_chan = (ci + 1).min(n - 1),
            Key::F2 if !repeat => self.rack.rename = Some((ci, self.song.channels[ci].name.clone())),
            _ => return false,
        }
        true
    }

    /// Only this mixer track is heard (with the master); asked again, all of them are.
    /// `with_routed` keeps the tracks that feed it and the ones it feeds as well.
    fn solo_insert(&mut self, i: usize, with_routed: bool) {
        let n = self.song.inserts.len();
        if i == 0 || i >= n {
            return;
        }
        let mut keep = vec![false; n];
        keep[0] = true;
        keep[i] = true;
        if with_routed {
            loop {
                let mut grew = false;
                for a in 1..n {
                    let feeds = self.song.inserts[a].sends.iter().any(|&d| d != 0 && keep[d] && self.reaches(a, i));
                    let fed = (1..n).any(|b| keep[b] && self.reaches(i, b) && self.song.inserts[b].sends.contains(&a));
                    if !keep[a] && (feeds || fed) {
                        keep[a] = true;
                        grew = true;
                    }
                }
                if !grew {
                    break;
                }
            }
        }
        let already = (0..n).all(|k| self.song.inserts[k].enabled == keep[k]);
        for (k, ins) in self.song.inserts.iter_mut().enumerate() {
            ins.enabled = already || keep[k];
        }
        self.dirty = true;
    }

    /// Whether sound of track `a` arrives at track `b` (a track reaches itself).
    fn reaches(&self, a: usize, b: usize) -> bool {
        let mut seen = vec![false; self.song.inserts.len()];
        let mut todo = vec![a];
        while let Some(k) = todo.pop() {
            if k == b {
                return true;
            }
            if !std::mem::replace(&mut seen[k], true) {
                todo.extend(self.song.inserts[k].sends.iter().copied().filter(|&d| d < seen.len()));
            }
        }
        false
    }

    /// A key press while the mixer has the focus. True when it was ours.
    pub(crate) fn mixer_key(&mut self, key: Key, m: Modifiers, repeat: bool) -> bool {
        let n = self.song.inserts.len();
        if n == 0 {
            return false;
        }
        let i = self.sel_insert.min(n - 1);
        if m.command {
            match key {
                Key::L if !repeat => {
                    self.link_channel(self.sel_chan, i);
                }
                Key::S if m.shift && !repeat => self.state_ask = Some(crate::StateAsk::SaveTrack(i)),
                _ => return false,
            }
            return true;
        }
        if m.alt {
            match key {
                Key::S if !repeat => self.solo_insert(i, true),
                Key::ArrowLeft if i > 1 => {
                    self.song.swap_inserts(i, i - 1);
                    self.sel_insert = i - 1;
                    self.dirty = true;
                }
                Key::ArrowRight if i > 0 && i + 2 < n => {
                    self.song.swap_inserts(i, i + 1);
                    self.sel_insert = i + 1;
                    self.dirty = true;
                }
                Key::ArrowLeft | Key::ArrowRight => {}
                Key::L if !repeat => {
                    // the first channel that plays into this track becomes the selected one
                    if let Some(c) = self.song.channels.iter().position(|c| c.insert == i) {
                        self.sel_chan = c;
                    }
                }
                _ => return false,
            }
            return true;
        }
        if m.shift {
            return false;
        }
        match key {
            Key::ArrowLeft => self.sel_insert = i.saturating_sub(1),
            Key::ArrowRight => self.sel_insert = (i + 1).min(n - 1),
            Key::F2 if !repeat && i > 0 => self.rename_insert = Some((i, self.song.inserts[i].name.clone())),
            Key::S if !repeat && !self.typing_keys => self.solo_insert(i, false),
            _ => return false,
        }
        true
    }

    /// Give a window the keys, and bring it in front of the others.
    pub(crate) fn focus_window(&mut self, w: Win) {
        self.focus = w;
        self.raise = Some(w);
    }

    /// What `focus_window` asked for, done once the frame's drawing surface is at hand.
    pub(crate) fn raise_window(&mut self, ctx: &egui::Context) {
        if let Some(name) = self.raise.take().and_then(|w| WINDOWS.iter().find(|x| x.0 == w)).map(|x| x.1) {
            ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, egui::Id::new(name)));
        }
    }

    fn shown(&self, w: Win) -> bool {
        match w {
            Win::Playlist => self.show_playlist,
            Win::Rack => self.show_rack,
            Win::Piano => self.show_piano,
            Win::Mixer => self.show_mixer,
            Win::None => false,
        }
    }

    /// Tab: on to the next open window.
    pub(crate) fn cycle_windows(&mut self, back: bool) {
        let at = WINDOWS.iter().position(|x| x.0 == self.focus).unwrap_or(WINDOWS.len() - 1);
        for k in 1..=WINDOWS.len() {
            let w = WINDOWS[(at + if back { WINDOWS.len() - k } else { k }) % WINDOWS.len()].0;
            if self.shown(w) {
                self.focus_window(w);
                return;
            }
        }
    }

    /// Escape: shut the window that has the keys.
    pub(crate) fn close_focused(&mut self) {
        match self.focus {
            Win::Playlist => self.show_playlist = false,
            Win::Rack => self.show_rack = false,
            Win::Piano => self.show_piano = false,
            Win::Mixer => self.show_mixer = false,
            Win::None => return,
        }
        self.focus = Win::None;
    }

    /// Save under the next free numbered name beside the project: song.flp, song_2.flp, song_3.flp ...
    pub(crate) fn save_new_version(&mut self) {
        let Some(path) = self.path.clone() else { return self.save(true) };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let (base, mut n) = match stem.rsplit_once('_') {
            Some((b, d)) if !b.is_empty() && !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) => (b.to_string(), d.parse::<u32>().unwrap_or(1)),
            _ => (stem.clone(), 1),
        };
        let next = loop {
            n += 1;
            let p = path.with_file_name(format!("{base}_{n}.flp"));
            if !p.exists() {
                break p;
            }
        };
        self.path = Some(next);
        self.save(false);
        if self.unsaved {
            // it did not go through: the project is still the old file
            self.path = Some(path);
        }
    }

    /// Put a project at the head of the list of the ten opened last.
    pub(crate) fn remember(&mut self, path: &std::path::Path) {
        let p: PathBuf = path.to_path_buf();
        self.settings.recent.retain(|r| *r != p);
        self.settings.recent.insert(0, p);
        self.settings.recent.truncate(10);
        if std::env::var_os("CEREAL_NO_SETUP").is_none() {
            let _ = self.settings.save();
        }
    }

    /// Notes played by hand while step editing is on and the song stands still: each lands at the play
    /// position, one grid step long, and the position moves on when the keys are let go.
    pub(crate) fn step_notes(&mut self, evs: Vec<cereal_engine::Live>) {
        if evs.is_empty() {
            return;
        }
        let tr = self.handle.transport.clone();
        let pat = tr.pattern();
        self.song.ensure_pattern(pat);
        let g = self.pr_snap().max(1);
        let at = self.step_at.unwrap_or_else(|| (tr.tick().max(0.0) as u32 + g / 2) / g * g);
        let mut next = at;
        for e in evs {
            if e.on {
                self.song.patterns[pat].notes.push(Note { pos: at, len: g, chan: e.chan, key: e.key, vel: e.vel, ..Default::default() });
                self.step_down += 1;
                self.dirty = true;
            } else if self.step_down > 0 {
                self.step_down -= 1;
                if self.step_down == 0 {
                    next = at + g;
                }
            }
        }
        self.song.patterns[pat].sort();
        self.piano.sel.clear();
        self.step_at = Some(next);
        if next != at {
            tr.seek(next);
        }
    }
}
