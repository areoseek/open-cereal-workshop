//! The piano roll: notes of the selected channel in the current pattern.
//!
//! Tools and bindings (behaviour compiled from normal use and the public manual):
//!   Draw (P)      left: add a note or move one, right edge: resize, right button: delete (sweep to delete many),
//!                 Shift+drag a note: move a copy, Ctrl+drag: select with a rectangle (Ctrl+Shift adds)
//!   Paint (B)     like Draw, and dragging lays down a row of notes
//!   Delete (D)    left or right: delete what the pointer touches
//!   Mute (T)      click or sweep: mute / unmute
//!   Slice (C)     drag a line across notes to cut them
//!   Select (E)    drag a rectangle (Shift adds), click empty space to deselect
//!   Zoom (Z)      drag a range to zoom to it, right-click to zoom out
//!   Playback (Y)  click or drag to move the play position
//! Alt held: no snap. Wheel: scroll, Shift+wheel: scroll sideways, Ctrl+wheel: zoom, middle button: pan.
//! Keys: Ctrl+A all, Ctrl+D none, Ctrl+C/X/V, Ctrl+B duplicate, Delete, Shift+Left/Right nudge, Shift+Up/Down
//! semitone, Ctrl+Up/Down octave, Ctrl+Q quantise, Ctrl+L legato, Ctrl+U chop, Ctrl+G glue, Alt+M / Shift+Alt+M
//! mute / unmute. Tool letters work while the typing keyboard is off (Ctrl+T), as in the reference.

use crate::widgets::Hint as _;
use std::collections::BTreeSet;

use egui::{Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::Note;
use cereal_engine::Cmd;

use crate::prtools::{self, Helper, Prop, PROPS, SCALES};
use crate::widgets::*;
use crate::{App, Win};

/// A helper tool's window while it is open: the notes as they were, and which of them it works on.
pub struct HelperRun {
    h: Helper,
    base: Vec<Note>,
    targets: Vec<usize>,
    pat: usize,
    chan: u16,
    fresh: bool,
}

/// The helper tools in menu order: label with its key, and the Alt+key that opens it.
const HELPERS: [(&str, Key); 13] = [
    ("Riff machine                Alt+E", Key::E),
    ("Quantise...                 Alt+Q", Key::Q),
    ("Chop...                     Alt+U", Key::U),
    ("Arpeggiate...               Alt+A", Key::A),
    ("Strum...                    Alt+S", Key::S),
    ("Flam...                     Alt+F", Key::F),
    ("Claw machine...             Alt+W", Key::W),
    ("Limit...                    Alt+K", Key::K),
    ("Flip...                     Alt+Y", Key::Y),
    ("Randomise...                Alt+R", Key::R),
    ("Scale levels...             Alt+X", Key::X),
    ("LFO...                      Alt+O", Key::O),
    ("Articulate...               Alt+L", Key::L),
];

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Draw,
    Paint,
    Delete,
    Mute,
    Slice,
    Select,
    Zoom,
    Playback,
}

const TOOLS: [(Tool, &str, &str); 8] = [
    (Tool::Draw, "Draw", "Draw (P)"),
    (Tool::Paint, "Paint", "Paint (B)"),
    (Tool::Delete, "Delete", "Delete (D)"),
    (Tool::Mute, "Mute", "Mute (T)"),
    (Tool::Slice, "Slice", "Slice (C)"),
    (Tool::Select, "Select", "Select (E)"),
    (Tool::Zoom, "Zoom", "Zoom to selection (Z)"),
    (Tool::Playback, "Play", "Playback (Y)"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    Vel,
    Pan,
    Fine,
    Release,
    ModX,
    ModY,
}

const LANES: [(Lane, &str); 6] = [(Lane::Vel, "Velocity"), (Lane::Pan, "Pan"), (Lane::Fine, "Fine pitch"), (Lane::Release, "Release"), (Lane::ModX, "Mod X"), (Lane::ModY, "Mod Y")];

/// The sixteen note colours; a colour is also the MIDI channel the note goes out on.
pub const NOTE_COLORS: [Color32; 16] = [
    Color32::from_rgb(158, 209, 165),
    Color32::from_rgb(159, 211, 186),
    Color32::from_rgb(161, 208, 213),
    Color32::from_rgb(162, 186, 214),
    Color32::from_rgb(164, 167, 216),
    Color32::from_rgb(187, 166, 218),
    Color32::from_rgb(211, 168, 219),
    Color32::from_rgb(221, 169, 201),
    Color32::from_rgb(223, 171, 174),
    Color32::from_rgb(224, 190, 172),
    Color32::from_rgb(226, 216, 174),
    Color32::from_rgb(207, 228, 175),
    Color32::from_rgb(182, 229, 177),
    Color32::from_rgb(178, 231, 196),
    Color32::from_rgb(180, 232, 228),
    Color32::from_rgb(181, 209, 234),
];

/// Snap choices: name, then the grid as (unit, divisor) where unit 0 = follow the lines, 1 = none, 2 = step,
/// 3 = beat, 4 = bar.
pub const SNAPS: [(&str, u8, u32); 13] = [
    ("Line", 0, 1),
    ("(none)", 1, 1),
    ("1/6 step", 2, 6),
    ("1/4 step", 2, 4),
    ("1/3 step", 2, 3),
    ("1/2 step", 2, 2),
    ("Step", 2, 1),
    ("1/6 beat", 3, 6),
    ("1/4 beat", 3, 4),
    ("1/3 beat", 3, 3),
    ("1/2 beat", 3, 2),
    ("Beat", 3, 1),
    ("Bar", 4, 1),
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Move,
    Resize,
}

#[derive(Clone, Copy, PartialEq)]
enum RectUse {
    Select,
    Zoom,
}

enum Drag {
    Notes { mode: Mode, anchor_tick: f32, anchor_key: i32, orig: Vec<(usize, Note)> },
    Rect { from: Pos2, add: bool, what: RectUse },
    /// Delete (None) or set the mute state (Some) of what the pointer touches; `right` = started with the right button.
    Sweep { mute: Option<bool>, right: bool },
    Slice { from: Pos2 },
    Paint,
    Lane,
    Seek,
    /// Picking out a stretch of time on the ruler, from this tick.
    TimeSel { from: u32 },
    Keys,
}

pub struct State {
    scroll_x: f32,
    scroll_y: f32,
    /// Pixels per tick at 96 ppq; scaled for other resolutions.
    zoom: f32,
    last_len: u32,
    last_vel: f32,
    last_pan: f32,
    /// What notes drawn next will be: slide notes, portamento notes, and their colour.
    slide: bool,
    porta: bool,
    color: u8,
    helper: Option<HelperRun>,
    /// The scale whose keys the grid lights up: root (0 = C) and an index into the scale list (0 = none).
    scale_root: u8,
    scale: usize,
    drag: Option<Drag>,
    sounding: Option<(u16, u8)>,
    pub tool: Tool,
    snap: usize,
    /// The stretch of time picked out on the ruler; it plays round and round and edits by time work on it.
    pub(crate) time_sel: Option<(u32, u32)>,
    lane: Lane,
    ghosts: bool,
    /// The key strip shows every note's name instead of a keyboard.
    names: bool,
    /// The light rows that show the white keys or the scale, and the channel's sample drawn behind the notes.
    rows: bool,
    wave: bool,
    /// Chord laid down with every drawn note (index into STAMPS), 0 for none.
    stamp: usize,
    /// Selected notes, as indices into the pattern's note list.
    pub sel: BTreeSet<usize>,
    sel_for: (usize, u16),
    clip: Vec<Note>,
    props: Option<usize>,
    /// Ticks between the grid lines on screen, and the tick at the left edge (for key commands).
    grid_line: u32,
    left_tick: u32,
    lane_x: Option<f32>,
    /// Width of the note grid on screen, for the zoom keys.
    view_w: f32,
    /// The snap choice the Backspace key goes back to.
    snap_back: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: (131.0 - 72.0) * KEY_H,
            zoom: 0.5,
            last_len: 0,
            last_vel: 1.0,
            last_pan: 0.0,
            slide: false,
            porta: false,
            color: 0,
            helper: None,
            scale_root: 0,
            scale: 0,
            drag: None,
            sounding: None,
            tool: Tool::Draw,
            snap: 0,
            time_sel: None,
            lane: Lane::Vel,
            ghosts: true,
            names: false,
            rows: true,
            wave: false,
            stamp: 0,
            sel: BTreeSet::new(),
            sel_for: (usize::MAX, 0),
            clip: vec![],
            props: None,
            grid_line: 24,
            left_tick: 0,
            lane_x: None,
            view_w: 600.0,
            snap_back: 0,
        }
    }
}

const KEY_H: f32 = 14.0;
const KEYS_W: f32 = 64.0;
const RULER_H: f32 = 18.0;
const LANE_H: f32 = 84.0;
const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const SELECTED: Color32 = Color32::from_rgb(235, 96, 88);

fn is_black(key: i32) -> bool {
    matches!(key.rem_euclid(12), 1 | 3 | 6 | 8 | 10)
}

fn note_len(n: &Note, step: u32) -> u32 {
    if n.len == 0 {
        step
    } else {
        n.len
    }
}

/// Chords the draw tool can lay down in one click: name and the keys above the clicked one.
const STAMPS: [(&str, &[u8]); 19] = [
    ("None", &[0]),
    ("Major", &[0, 4, 7]),
    ("Minor", &[0, 3, 7]),
    ("Diminished", &[0, 3, 6]),
    ("Augmented", &[0, 4, 8]),
    ("Sus 2", &[0, 2, 7]),
    ("Sus 4", &[0, 5, 7]),
    ("Fifth", &[0, 7]),
    ("Octave", &[0, 12]),
    ("Major 6", &[0, 4, 7, 9]),
    ("Minor 6", &[0, 3, 7, 9]),
    ("Seventh", &[0, 4, 7, 10]),
    ("Major 7", &[0, 4, 7, 11]),
    ("Minor 7", &[0, 3, 7, 10]),
    ("Diminished 7", &[0, 3, 6, 9]),
    ("Minor 7 flat 5", &[0, 3, 6, 10]),
    ("Ninth", &[0, 4, 7, 10, 14]),
    ("Major 9", &[0, 4, 7, 11, 14]),
    ("Minor 9", &[0, 3, 7, 10, 14]),
];

impl App {
    fn pr_sound(&mut self, key: Option<u8>) {
        let chan = self.sel_chan as u16;
        let want = key.map(|k| (chan, k));
        if self.piano.sounding == want {
            return;
        }
        if let Some((c, k)) = self.piano.sounding.take() {
            self.handle.send(Cmd::NoteOff { chan: c, key: k });
        }
        if let Some((c, k)) = want {
            self.handle.send(Cmd::NoteOn { chan: c, key: k, vel: 1.0 });
            self.piano.sounding = want;
        }
    }

    fn pr_pat(&mut self) -> usize {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        pat
    }

    /// The snap grid in ticks.
    pub(crate) fn pr_snap(&self) -> u32 {
        let (_, unit, div) = SNAPS[self.piano.snap.min(SNAPS.len() - 1)];
        let base = match unit {
            0 => self.piano.grid_line,
            1 => 1,
            2 => self.song.step(),
            3 => self.song.ppq,
            _ => self.song.bar(),
        };
        (base / div).max(1)
    }

    /// Sort the pattern's notes and keep the selection on the same notes.
    fn pr_sort(&mut self, pat: usize) {
        let sel = std::mem::take(&mut self.piano.sel);
        let notes = &mut self.song.patterns[pat].notes;
        let mut tagged: Vec<(Note, bool)> = notes.drain(..).enumerate().map(|(i, n)| (n, sel.contains(&i))).collect();
        tagged.sort_by_key(|(n, _)| (n.pos, n.chan, n.key));
        for (i, (n, s)) in tagged.into_iter().enumerate() {
            if s {
                self.piano.sel.insert(i);
            }
            notes.push(n);
        }
    }

    fn pr_remove(&mut self, pat: usize, gone: &BTreeSet<usize>) {
        if gone.is_empty() {
            return;
        }
        let sel = std::mem::take(&mut self.piano.sel);
        let notes = &mut self.song.patterns[pat].notes;
        let old: Vec<Note> = notes.drain(..).collect();
        for (i, n) in old.into_iter().enumerate() {
            if gone.contains(&i) {
                continue;
            }
            if sel.contains(&i) {
                self.piano.sel.insert(notes.len());
            }
            notes.push(n);
        }
        self.piano.props = None;
        self.dirty = true;
    }

    /// The notes a command works on: the selection, or every note of the channel when nothing is selected.
    fn pr_targets(&self, pat: usize) -> Vec<usize> {
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        if self.piano.sel.is_empty() {
            (0..notes.len()).filter(|&i| notes[i].chan == chan).collect()
        } else {
            self.piano.sel.iter().copied().filter(|&i| i < notes.len() && notes[i].chan == chan).collect()
        }
    }

    fn pr_transpose(&mut self, semis: i32) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        if t.iter().any(|&i| !(0..=131).contains(&(notes[i].key as i32 + semis))) {
            return;
        }
        for &i in &t {
            notes[i].key = (notes[i].key as i32 + semis) as u8;
        }
        self.dirty |= !t.is_empty();
    }

    fn pr_nudge(&mut self, dir: i32) {
        let pat = self.pr_pat();
        let by = self.pr_snap() as i64 * dir as i64;
        let t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        if t.iter().any(|&i| (notes[i].pos as i64 + by) < 0) {
            return;
        }
        for &i in &t {
            notes[i].pos = (notes[i].pos as i64 + by) as u32;
        }
        if !t.is_empty() {
            self.pr_sort(pat);
            self.dirty = true;
        }
    }

    /// Move the notes by a number of ticks, whatever the snap says.
    fn pr_nudge_by(&mut self, by: i64) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        if t.is_empty() || t.iter().any(|&i| (notes[i].pos as i64 + by) < 0) {
            return;
        }
        for &i in &t {
            notes[i].pos = (notes[i].pos as i64 + by) as u32;
        }
        self.pr_sort(pat);
        self.dirty = true;
    }

    /// Where the selection starts and ends in time, pulled out to the snap grid.
    fn pr_span(&mut self) -> Option<(u32, u32)> {
        self.piano.time_sel.or_else(|| self.pr_note_span())
    }

    /// The stretch the selected notes cover.
    fn pr_note_span(&mut self) -> Option<(u32, u32)> {
        let pat = self.pr_pat();
        let g = self.pr_snap().max(1);
        let notes = &self.song.patterns[pat].notes;
        let sel: Vec<usize> = self.piano.sel.iter().copied().filter(|&i| i < notes.len()).collect();
        let a = sel.iter().map(|&i| notes[i].pos).min()? / g * g;
        let b = sel.iter().map(|&i| notes[i].pos + notes[i].len.max(1)).max()?.div_ceil(g) * g;
        Some((a, b.max(a + g)))
    }

    /// Open a gap as long as the selection where it starts, or close the selection's time up.
    /// With nothing selected it is one bar at the left of the view.
    fn pr_space(&mut self, add: bool) {
        let pat = self.pr_pat();
        let chan = self.sel_chan as u16;
        let bar = self.song.bar();
        let (from, to) = self.pr_span().unwrap_or_else(|| {
            let a = self.piano.left_tick.div_ceil(bar) * bar;
            (a, a + bar)
        });
        let len = to - from;
        let notes = &mut self.song.patterns[pat].notes;
        if add {
            for n in notes.iter_mut().filter(|n| n.chan == chan && n.pos >= from) {
                n.pos += len;
            }
        } else {
            let gone: BTreeSet<usize> = (0..notes.len()).filter(|&i| notes[i].chan == chan && notes[i].pos >= from && notes[i].pos < to).collect();
            for n in notes.iter_mut().filter(|n| n.chan == chan && n.pos >= to) {
                n.pos -= len;
            }
            self.pr_remove(pat, &gone);
            self.piano.sel.clear();
        }
        self.pr_sort(pat);
        self.dirty = true;
    }

    /// Make the selected notes one group, or take them out of their groups.
    fn pr_group(&mut self, on: bool) {
        let pat = self.pr_pat();
        let t: Vec<usize> = self.piano.sel.iter().copied().collect();
        if !t.is_empty() {
            crate::widgets::set_group(&mut self.song.patterns[pat].notes, &t, on, |n| &mut n.group);
            self.dirty = true;
        }
    }

    /// A selection that holds part of a group holds all of it.
    fn pr_whole_groups(&mut self, pat: usize) {
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        let groups: BTreeSet<u16> = self.piano.sel.iter().filter_map(|&i| notes.get(i)).map(|n| n.group).filter(|g| *g != 0).collect();
        if !groups.is_empty() {
            self.piano.sel.extend((0..notes.len()).filter(|&i| notes[i].chan == chan && groups.contains(&notes[i].group)));
        }
    }

    /// Select the notes in the stretch of time just before or just after the selection, of the same length.
    fn pr_select_beside(&mut self, after: bool) {
        let Some((a, b)) = self.pr_span() else { return };
        let (lo, hi) = if after { (b, b + (b - a)) } else { (a.saturating_sub(b - a), a) };
        if self.piano.time_sel.is_some() && hi > lo {
            self.piano.time_sel = Some((lo, hi));
        }
        let pat = self.pr_pat();
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        self.piano.sel = (0..notes.len()).filter(|&i| notes[i].chan == chan && notes[i].pos >= lo && notes[i].pos < hi).collect();
    }

    /// Every note of the colours the selection holds, or of the colour being drawn with.
    fn pr_select_colour(&mut self) {
        let pat = self.pr_pat();
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        let mut cols: Vec<u8> = self.piano.sel.iter().filter_map(|&i| notes.get(i)).map(|n| n.color).collect();
        if cols.is_empty() {
            cols.push(self.piano.color);
        }
        self.piano.sel = (0..notes.len()).filter(|&i| notes[i].chan == chan && cols.contains(&notes[i].color)).collect();
    }

    /// Roughly every other note, picked by chance.
    fn pr_select_random(&mut self) {
        let pat = self.pr_pat();
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
        self.piano.sel = (0..notes.len())
            .filter(|&i| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                notes[i].chan == chan && seed & 0x100 != 0
            })
            .collect();
    }

    /// Turn the notes into plain steps, which last as long as their sound does.
    fn pr_discard_lengths(&mut self) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        for &i in &t {
            self.song.patterns[pat].notes[i].len = 0;
        }
        self.dirty |= !t.is_empty();
    }

    /// Give the notes the colour being drawn with.
    fn pr_recolour(&mut self) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        for &i in &t {
            self.song.patterns[pat].notes[i].color = self.piano.color;
        }
        self.dirty |= !t.is_empty();
    }

    /// Show the stretch from tick `a` to tick `b` across the whole width.
    fn pr_show(&mut self, a: u32, b: u32) {
        let ppq = self.song.ppq as f32;
        let px = (self.piano.view_w.max(100.0) / (b.max(a + 1) - a) as f32).clamp(0.02 * 96.0 / ppq, 12.0 * 96.0 / ppq);
        self.piano.zoom = px * ppq / 96.0;
        self.piano.scroll_x = a as f32 * px;
    }

    /// The zoom keys: Shift and a digit.
    fn pr_zoom_key(&mut self, d: usize) {
        let pat = self.pr_pat();
        let bar = self.song.bar();
        let left = self.piano.left_tick;
        match d {
            0 => {
                let px = self.piano.zoom * 96.0 / self.song.ppq as f32;
                self.piano.scroll_x = (self.handle.transport.tick() as f32 * px - self.piano.view_w * 0.5).max(0.0);
            }
            1 => self.pr_show(left, left + bar),
            2 => self.pr_show(left, left + bar * 4),
            3 => self.pr_show(left, left + bar * 16),
            4 => {
                let end = self.song.pattern_length(pat);
                self.pr_show(0, end.max(bar));
            }
            _ => {
                if let Some((a, b)) = self.pr_span() {
                    self.pr_show(a, b);
                }
            }
        }
    }

    /// Step to the next channel up or down; `with_notes` passes over the ones with nothing in this pattern.
    fn pr_step_channel(&mut self, by: i32, with_notes: bool) {
        let pat = self.pr_pat();
        let n = self.song.channels.len() as i32;
        let mut c = self.sel_chan as i32 + by;
        while (0..n).contains(&c) {
            if !with_notes || self.song.patterns[pat].notes.iter().any(|x| x.chan as i32 == c) {
                self.sel_chan = c as usize;
                self.piano.sel.clear();
                return;
            }
            c += by;
        }
    }

    fn pr_select_all(&mut self) {
        let pat = self.pr_pat();
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        self.piano.sel = (0..notes.len()).filter(|&i| notes[i].chan == chan).collect();
    }

    fn pr_invert_selection(&mut self) {
        let pat = self.pr_pat();
        let chan = self.sel_chan as u16;
        let notes = &self.song.patterns[pat].notes;
        self.piano.sel = (0..notes.len()).filter(|&i| notes[i].chan == chan && !self.piano.sel.contains(&i)).collect();
    }

    fn pr_delete_selected(&mut self) {
        let pat = self.pr_pat();
        let gone = self.piano.sel.clone();
        self.pr_remove(pat, &gone);
        self.piano.sel.clear();
    }

    pub(crate) fn pr_copy(&mut self, ctx: &egui::Context, cut: bool) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        if t.is_empty() {
            return;
        }
        self.piano.clip = t.iter().map(|&i| self.song.patterns[pat].notes[i].clone()).collect();
        // something has to sit on the system clipboard for the paste key to reach us
        ctx.copy_text(format!("{} notes", t.len()));
        if cut {
            self.pr_remove(pat, &t.into_iter().collect());
            self.piano.sel.clear();
        }
    }

    /// Paste from the MIDI clipboard: the notes of a MIDI file another program put on the clipboard (as
    /// the file's path or address), all onto this channel from the first bar in view.
    pub(crate) fn pr_paste_midi(&mut self, text: &str) {
        let name = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or("");
        let name = name.strip_prefix("file://").map(|p| p.replace("%20", " ")).unwrap_or_else(|| name.to_string());
        let path = std::path::Path::new(&name);
        let is_midi = path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["mid", "midi", "rmi", "kar"].contains(&e.to_ascii_lowercase().as_str()));
        if !is_midi || !path.is_file() {
            self.status = "The clipboard holds no MIDI file".into();
            return;
        }
        let got = std::fs::read(path).map_err(|e| e.to_string()).and_then(|d| cereal_engine::midifile::import(&d, self.song.ppq));
        let parts = match got {
            Ok(m) if m.parts.iter().any(|p| !p.notes.is_empty()) => m.parts,
            Ok(_) => {
                self.status = "That MIDI file holds no notes".into();
                return;
            }
            Err(e) => {
                self.status = format!("MIDI paste failed: {e}");
                return;
            }
        };
        let pat = self.pr_pat();
        let bar = self.song.bar();
        let chan = self.sel_chan as u16;
        let first = parts.iter().flat_map(|p| p.notes.iter()).map(|n| n.pos).min().unwrap_or(0) / bar * bar;
        let to = self.piano.left_tick.div_ceil(bar) * bar;
        let notes = &mut self.song.patterns[pat].notes;
        self.piano.sel.clear();
        let mut count = 0;
        for n in parts.into_iter().flat_map(|p| p.notes) {
            self.piano.sel.insert(notes.len());
            notes.push(Note { pos: n.pos - first + to, chan, ..n });
            count += 1;
        }
        self.pr_sort(pat);
        self.dirty = true;
        self.status = format!("Pasted {count} notes from the MIDI clipboard");
    }

    /// Paste at the first bar in view, keeping each note's place inside its bar.
    pub(crate) fn pr_paste(&mut self) {
        if self.piano.clip.is_empty() {
            return;
        }
        let pat = self.pr_pat();
        let bar = self.song.bar();
        let chan = self.sel_chan as u16;
        let first = self.piano.clip.iter().map(|n| n.pos).min().unwrap_or(0) / bar * bar;
        let to = self.piano.left_tick.div_ceil(bar) * bar;
        let notes = &mut self.song.patterns[pat].notes;
        self.piano.sel.clear();
        for n in &self.piano.clip {
            self.piano.sel.insert(notes.len());
            notes.push(Note { pos: n.pos - first + to, chan, ..n.clone() });
        }
        let new: Vec<usize> = self.piano.sel.iter().copied().collect();
        crate::widgets::fresh_groups(notes, &new, |n| &mut n.group);
        self.pr_sort(pat);
        self.dirty = true;
    }

    /// Copy the selection (or everything) to just after itself.
    fn pr_duplicate(&mut self) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        if t.is_empty() {
            return;
        }
        let step = self.song.step();
        let grid = self.pr_snap().max(step);
        let notes = &mut self.song.patterns[pat].notes;
        let start = t.iter().map(|&i| notes[i].pos).min().unwrap_or(0) / grid * grid;
        let end = t.iter().map(|&i| notes[i].pos + note_len(&notes[i], step)).max().unwrap_or(0);
        let by = (end - start).div_ceil(grid).max(1) * grid;
        self.piano.sel.clear();
        for &i in &t {
            let mut n = notes[i].clone();
            n.pos += by;
            self.piano.sel.insert(notes.len());
            notes.push(n);
        }
        let new: Vec<usize> = self.piano.sel.iter().copied().collect();
        crate::widgets::fresh_groups(notes, &new, |n| &mut n.group);
        self.pr_sort(pat);
        self.dirty = true;
    }

    /// Pull note starts onto the snap grid, keeping lengths.
    fn pr_quantise(&mut self) {
        let pat = self.pr_pat();
        let g = self.pr_snap().max(1);
        let t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        for &i in &t {
            notes[i].pos = (notes[i].pos + g / 2) / g * g;
        }
        if !t.is_empty() {
            self.pr_sort(pat);
            self.dirty = true;
        }
    }

    /// Snap the starts and leave the ends where they are.
    fn pr_quantise_starts(&mut self) {
        let pat = self.pr_pat();
        let g = self.pr_snap().max(1);
        let t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        for &i in &t {
            let end = notes[i].pos + notes[i].len;
            notes[i].pos = (notes[i].pos + g / 2) / g * g;
            if notes[i].len > 0 {
                notes[i].len = end.saturating_sub(notes[i].pos).max(1);
            }
        }
        if !t.is_empty() {
            self.pr_sort(pat);
            self.dirty = true;
        }
    }

    /// Stretch every note to the start of the next one.
    fn pr_legato(&mut self) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        let mut starts: Vec<u32> = t.iter().map(|&i| notes[i].pos).collect();
        starts.sort_unstable();
        starts.dedup();
        for &i in &t {
            if let Some(next) = starts.iter().find(|&&s| s > notes[i].pos) {
                notes[i].len = next - notes[i].pos;
            }
        }
        self.dirty |= !t.is_empty();
    }

    /// Cut notes into pieces one snap unit long.
    fn pr_chop(&mut self) {
        let pat = self.pr_pat();
        let step = self.song.step();
        let g = self.pr_snap().max(2);
        let t = self.pr_targets(pat);
        let had_sel = !self.piano.sel.is_empty();
        let notes = &mut self.song.patterns[pat].notes;
        for &i in &t {
            let len = note_len(&notes[i], step);
            if len <= g {
                continue;
            }
            let (pos, mut at) = (notes[i].pos, notes[i].pos + g);
            notes[i].len = g;
            while at < pos + len {
                let mut n = notes[i].clone();
                n.pos = at;
                n.len = g.min(pos + len - at);
                if had_sel {
                    self.piano.sel.insert(notes.len());
                }
                notes.push(n);
                at += g;
            }
        }
        if !t.is_empty() {
            self.pr_sort(pat);
            self.dirty = true;
        }
    }

    /// Join notes of the same key that touch or overlap.
    fn pr_glue(&mut self) {
        let pat = self.pr_pat();
        let step = self.song.step();
        let mut t = self.pr_targets(pat);
        let notes = &mut self.song.patterns[pat].notes;
        t.sort_by_key(|&i| (notes[i].key, notes[i].pos));
        let mut gone = BTreeSet::new();
        let mut head: Option<usize> = None;
        for &i in &t {
            match head {
                Some(h) if notes[h].key == notes[i].key && notes[i].pos <= notes[h].pos + note_len(&notes[h], step) => {
                    let end = (notes[i].pos + note_len(&notes[i], step)).max(notes[h].pos + note_len(&notes[h], step));
                    notes[h].len = end - notes[h].pos;
                    gone.insert(i);
                }
                _ => head = Some(i),
            }
        }
        self.pr_remove(pat, &gone);
    }

    /// Open one of the helper tools on the selected notes (all of the channel's when nothing is selected).
    fn pr_helper(&mut self, which: usize) {
        let pat = self.pr_pat();
        let (ppq, step, snap) = (self.song.ppq, self.song.step(), self.pr_snap().max(1));
        let grid = if snap <= 1 { step } else { snap };
        let (root, scale) = (self.piano.scale_root, self.piano.scale);
        let h = match which {
            0 => Helper::Riff { root, scale: scale.max(1), bars: 2, every: step, gate: 0.9, range: 2, density: 0.6, chord: 1, seed: 1 },
            1 => Helper::Quantise { start: 1.0, end: 0.0, grid },
            2 => Helper::Chop { every: grid, gate: 1.0, fade: 0.0 },
            3 => Helper::Arp { dir: 0, octaves: 1, every: step, gate: 1.0, seed: 1 },
            4 => Helper::Strum { spread: (ppq / 16) as i32, fade: 0.0 },
            5 => Helper::Flam { offset: -((ppq / 16) as i32), vel: 0.6, count: 1 },
            6 => Helper::Claw { period: 2, skip: 0 },
            7 => Helper::Limit { lo: 48, hi: 84, root, scale },
            8 => Helper::Flip { vertical: true, horizontal: false },
            9 => Helper::Random { amount: [0.3, 0.0, 0.0, 0.0, 0.0, 0.0], seed: 1, notes: 0, every: step, len: step, lo: 48, hi: 72, root, scale, bars: 1 },
            10 => Helper::Levels { prop: Prop::Vel, offset: 0.0, mul: 1.0, tension: 0.0 },
            11 => Helper::Lfo { prop: Prop::Vel, base: 0.6, depth: 0.5, period: ppq * 4, shape: 0, phase: 0.0 },
            _ => Helper::Articulate { mul: 1.0, gap: 0, vary: 0.0, seed: 1 },
        };
        // the riff machine writes its own notes, so with nothing selected it replaces nothing
        let targets = if which == 0 && self.piano.sel.is_empty() { vec![] } else { self.pr_targets(pat) };
        self.piano.helper = Some(HelperRun { h, base: self.song.patterns[pat].notes.clone(), targets, pat, chan: self.sel_chan as u16, fresh: true });
        self.piano.sel.clear();
    }

    /// The open helper tool's window: every change shows at once, Accept keeps it and Cancel puts the notes back.
    fn helper_window(&mut self, ctx: &egui::Context) {
        let Some(mut run) = self.piano.helper.take() else { return };
        if run.pat >= self.song.patterns.len() {
            return;
        }
        let (ppq, step) = (self.song.ppq, self.song.step().max(1));
        let before = run.h.clone();
        let (mut accept, mut cancel) = (false, false);
        let mut open = true;
        let ticks = |ui: &mut Ui, label: &str, v: &mut u32| {
            ui.label(label);
            let mut steps = *v as f32 / step as f32;
            if ui.add(egui::Slider::new(&mut steps, 0.125..=16.0).logarithmic(true).suffix(" steps")).changed() {
                *v = ((steps * step as f32).round() as u32).max(1);
            }
            ui.end_row();
        };
        let unit = |ui: &mut Ui, label: &str, v: &mut f32, lo: f32, hi: f32| {
            ui.label(label);
            let mut pc = *v * 100.0;
            if ui.add(egui::Slider::new(&mut pc, lo * 100.0..=hi * 100.0).suffix(" %").fixed_decimals(0)).changed() {
                *v = pc / 100.0;
            }
            ui.end_row();
        };
        let key = |ui: &mut Ui, label: &str, v: &mut u8| {
            ui.label(label);
            ui.add(egui::DragValue::new(v).range(0..=131).custom_formatter(|k, _| format!("{}{}", NAMES[k as usize % 12], k as usize / 12)));
            ui.end_row();
        };
        let prop = |ui: &mut Ui, v: &mut Prop| {
            ui.label("Property");
            egui::ComboBox::from_id_salt("hp_prop").selected_text(PROPS.iter().find(|p| p.0 == *v).map(|p| p.1).unwrap_or("")).show_ui(ui, |ui| {
                for (p, name) in PROPS {
                    ui.selectable_value(v, p, name);
                }
            });
            ui.end_row();
        };
        let scale_pick = |ui: &mut Ui, root: &mut u8, scale: &mut usize| {
            ui.label("Scale");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("hp_root").width(40.0).selected_text(NAMES[*root as usize % 12]).show_ui(ui, |ui| {
                    for (i, n) in NAMES.iter().enumerate() {
                        ui.selectable_value(root, i as u8, *n);
                    }
                });
                egui::ComboBox::from_id_salt("hp_scale").selected_text(SCALES[*scale].0).show_ui(ui, |ui| {
                    for (i, (n, _)) in SCALES.iter().enumerate() {
                        ui.selectable_value(scale, i, *n);
                    }
                });
            });
            ui.end_row();
        };
        let seed_row = |ui: &mut Ui, seed: &mut u32| {
            ui.label("Seed");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(seed).range(1..=9999));
                if ui.button("Throw dice").clicked() {
                    *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) % 9999 + 1;
                }
            });
            ui.end_row();
        };
        egui::Window::new(run.h.title()).id(egui::Id::new("pr_helper")).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("hp").num_columns(2).show(ui, |ui| match &mut run.h {
                Helper::Quantise { start, end, grid } => {
                    unit(ui, "Start time", start, 0.0, 1.0);
                    unit(ui, "End time", end, 0.0, 1.0);
                    ticks(ui, "Grid", grid);
                }
                Helper::Chop { every, gate, fade } => {
                    ticks(ui, "Time", every);
                    unit(ui, "Gate", gate, 0.05, 1.0);
                    unit(ui, "Fade", fade, 0.0, 1.0);
                }
                Helper::Arp { dir, octaves, every, gate, seed } => {
                    ui.label("Direction");
                    egui::ComboBox::from_id_salt("hp_dir").selected_text(["Up", "Down", "Up and down", "Random"][*dir as usize % 4]).show_ui(ui, |ui| {
                        for (i, n) in ["Up", "Down", "Up and down", "Random"].iter().enumerate() {
                            ui.selectable_value(dir, i as u8, *n);
                        }
                    });
                    ui.end_row();
                    ui.label("Range");
                    ui.add(egui::Slider::new(octaves, 1..=4).suffix(" oct"));
                    ui.end_row();
                    ticks(ui, "Time", every);
                    unit(ui, "Gate", gate, 0.05, 1.0);
                    if *dir == 3 {
                        seed_row(ui, seed);
                    }
                }
                Helper::Strum { spread, fade } => {
                    ui.label("Spread");
                    ui.add(egui::Slider::new(spread, -(ppq as i32) / 2..=ppq as i32 / 2).suffix(" ticks"));
                    ui.end_row();
                    unit(ui, "Fade", fade, 0.0, 1.0);
                }
                Helper::Flam { offset, vel, count } => {
                    ui.label("Offset");
                    ui.add(egui::Slider::new(offset, -(ppq as i32)..=ppq as i32).suffix(" ticks"));
                    ui.end_row();
                    unit(ui, "Velocity", vel, 0.0, 1.0);
                    ui.label("Count");
                    ui.add(egui::Slider::new(count, 1..=4));
                    ui.end_row();
                }
                Helper::Limit { lo, hi, root, scale } => {
                    key(ui, "Lowest", lo);
                    key(ui, "Highest", hi);
                    scale_pick(ui, root, scale);
                }
                Helper::Flip { vertical, horizontal } => {
                    ui.label("Vertical");
                    ui.checkbox(vertical, "");
                    ui.end_row();
                    ui.label("Horizontal");
                    ui.checkbox(horizontal, "");
                    ui.end_row();
                }
                Helper::Random { amount, seed, notes, every, len, lo, hi, root, scale, bars } => {
                    ui.label("New notes");
                    ui.add(egui::Slider::new(notes, 0..=100).suffix(" %"));
                    ui.end_row();
                    if *notes > 0 {
                        ui.label("Length");
                        ui.add(egui::Slider::new(bars, 1..=16).suffix(" bars"));
                        ui.end_row();
                        ticks(ui, "Grid", every);
                        ticks(ui, "Note length", len);
                        key(ui, "Lowest", lo);
                        key(ui, "Highest", hi);
                        scale_pick(ui, root, scale);
                    }
                    for (i, (_, name)) in PROPS.iter().enumerate() {
                        unit(ui, name, &mut amount[i], 0.0, 1.0);
                    }
                    seed_row(ui, seed);
                }
                Helper::Levels { prop: p, offset, mul, tension } => {
                    prop(ui, p);
                    unit(ui, "Offset", offset, -1.0, 1.0);
                    unit(ui, "Multiply", mul, 0.0, 2.0);
                    unit(ui, "Tension", tension, -1.0, 1.0);
                }
                Helper::Lfo { prop: p, base, depth, period, shape, phase } => {
                    prop(ui, p);
                    unit(ui, "Start value", base, 0.0, 1.0);
                    unit(ui, "Range", depth, -1.0, 1.0);
                    ticks(ui, "Speed", period);
                    ui.label("Shape");
                    egui::ComboBox::from_id_salt("hp_shape").selected_text(["Sine", "Triangle", "Square", "Saw"][*shape as usize % 4]).show_ui(ui, |ui| {
                        for (i, n) in ["Sine", "Triangle", "Square", "Saw"].iter().enumerate() {
                            ui.selectable_value(shape, i as u8, *n);
                        }
                    });
                    ui.end_row();
                    unit(ui, "Phase", phase, 0.0, 1.0);
                }
                Helper::Articulate { mul, gap, vary, seed } => {
                    unit(ui, "Multiply", mul, 0.05, 2.0);
                    ui.label("Gap");
                    ui.add(egui::Slider::new(gap, 0..=ppq as i32).suffix(" ticks"));
                    ui.end_row();
                    unit(ui, "Variation", vary, 0.0, 1.0);
                    seed_row(ui, seed);
                }
                Helper::Claw { period, skip } => {
                    ui.label("Drop every");
                    ui.add(egui::Slider::new(period, 2..=16));
                    ui.end_row();
                    ui.label("Shift");
                    ui.add(egui::Slider::new(skip, 0..=15));
                    ui.end_row();
                }
                Helper::Riff { root, scale, bars, every, gate, range, density, chord, seed } => {
                    scale_pick(ui, root, scale);
                    ui.label("Length");
                    ui.add(egui::Slider::new(bars, 1..=16).suffix(" bars"));
                    ui.end_row();
                    ticks(ui, "Time", every);
                    unit(ui, "Gate", gate, 0.05, 1.0);
                    ui.label("Range");
                    ui.add(egui::Slider::new(range, 1..=4).suffix(" oct"));
                    ui.end_row();
                    unit(ui, "Density", density, 0.0, 1.0);
                    ui.label("Chord");
                    ui.add(egui::Slider::new(chord, 1..=4).suffix(" notes"));
                    ui.end_row();
                    seed_row(ui, seed);
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                accept = ui.button("Accept").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        let keys = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        accept |= keys.0;
        cancel |= keys.1 || !open;
        if cancel {
            self.song.patterns[run.pat].notes = run.base;
            self.piano.sel.clear();
            return;
        }
        if run.fresh || run.h != before {
            run.fresh = false;
            self.song.patterns[run.pat].notes = prtools::apply(&run.h, &run.base, &run.targets, run.chan, ppq);
            self.piano.sel.clear();
            self.dirty = true;
        }
        if !accept {
            self.piano.helper = Some(run);
        }
    }

    fn pr_mute(&mut self, muted: bool) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        for &i in &t {
            self.song.patterns[pat].notes[i].muted = muted;
        }
        self.dirty |= !t.is_empty();
    }

    /// A key press while the piano roll has the focus. True when it was ours.
    pub(crate) fn pr_tool_names() -> Vec<&'static str> {
        TOOLS.iter().map(|t| t.2).collect()
    }
    pub(crate) fn pr_pick_tool(&mut self, i: usize) {
        if let Some(t) = TOOLS.get(i) {
            self.piano.tool = t.0;
        }
    }

    pub(crate) fn piano_key(&mut self, key: Key, m: Modifiers, repeat: bool) -> bool {
        if m.command && !m.alt {
            match key {
                Key::A => self.pr_select_all(),
                Key::D => {
                    self.piano.sel.clear();
                    self.piano.time_sel = None;
                }
                Key::Enter if !repeat => self.piano.time_sel = self.pr_note_span(),
                Key::B => self.pr_duplicate(),
                Key::Q => self.pr_quantise(),
                Key::M if !m.shift => self.open_score_dialog(),
                Key::L => self.pr_legato(),
                Key::U => self.pr_chop(),
                Key::G => self.pr_glue(),
                Key::ArrowUp => self.pr_transpose(12),
                Key::ArrowDown => self.pr_transpose(-12),
                Key::Insert if !m.shift => self.pr_space(true),
                Key::Delete => self.pr_space(false),
                Key::ArrowLeft => self.pr_select_beside(false),
                Key::ArrowRight => self.pr_select_beside(true),
                _ => return false,
            }
            return true;
        }
        if m.alt && !m.command {
            match key {
                Key::M => self.pr_mute(!m.shift),
                Key::ArrowLeft => self.pr_nudge_by(-1),
                Key::ArrowRight => self.pr_nudge_by(1),
                Key::C => self.pr_recolour(),
                Key::V => self.piano.ghosts = !self.piano.ghosts,
                Key::G if !repeat => self.pr_group(false),
                Key::B if !repeat => self.piano.rows = !self.piano.rows,
                Key::N if !repeat => self.piano.wave = !self.piano.wave,
                k => match HELPERS.iter().position(|h| h.1 == k) {
                    Some(i) => self.pr_helper(i),
                    None => return false,
                },
            }
            return true;
        }
        if m.shift {
            match key {
                Key::ArrowUp => self.pr_transpose(1),
                Key::ArrowDown => self.pr_transpose(-1),
                Key::ArrowLeft => self.pr_nudge(-1),
                Key::ArrowRight => self.pr_nudge(1),
                Key::I => self.pr_invert_selection(),
                Key::G if !repeat => self.pr_group(true),
                Key::N if !repeat => self.piano.stamp = 0,
                Key::M if !repeat => {
                    self.pr_select_random();
                    let pat = self.pr_pat();
                    self.pr_whole_groups(pat);
                }
                Key::C => self.pr_select_colour(),
                Key::R => self.pr_select_random(),
                Key::D => self.pr_discard_lengths(),
                Key::Q => self.pr_quantise_starts(),
                Key::F => {
                    let at = LANES.iter().position(|l| l.0 == self.piano.lane).unwrap_or(0);
                    self.piano.lane = LANES[(at + LANES.len() - 1) % LANES.len()].0;
                }
                Key::H => self.pr_step_channel(-1, true),
                Key::K => self.pr_step_channel(1, true),
                k => match crate::DIGITS.iter().position(|d| *d == k) {
                    Some(d) if d <= 5 => self.pr_zoom_key(d),
                    _ => return false,
                },
            }
            return true;
        }
        match key {
            Key::Delete => self.pr_delete_selected(),
            Key::PageUp | Key::PageDown => {
                let old = self.piano.zoom;
                self.piano.zoom = (old * if key == Key::PageUp { 1.25 } else { 0.8 }).clamp(0.02, 12.0);
                let off = self.piano.view_w * 0.5;
                self.piano.scroll_x = ((self.piano.scroll_x + off) * self.piano.zoom / old - off).max(0.0);
            }
            Key::Backspace if !repeat => {
                if self.piano.snap == 1 {
                    self.piano.snap = self.piano.snap_back;
                } else {
                    self.piano.snap_back = self.piano.snap;
                    self.piano.snap = 1;
                }
            }
            _ if self.typing_keys || repeat => return false,
            Key::F => {
                let at = LANES.iter().position(|l| l.0 == self.piano.lane).unwrap_or(0);
                self.piano.lane = LANES[(at + 1) % LANES.len()].0;
            }
            Key::G => self.pr_step_channel(-1, false),
            Key::K => self.pr_step_channel(1, false),
            Key::H => self.pr_step_channel(-1, true),
            Key::J => self.pr_step_channel(1, true),
            Key::P => self.piano.tool = Tool::Draw,
            Key::B => self.piano.tool = Tool::Paint,
            Key::D => self.piano.tool = Tool::Delete,
            Key::T => self.piano.tool = Tool::Mute,
            Key::C => self.piano.tool = Tool::Slice,
            Key::E => self.piano.tool = Tool::Select,
            Key::Z => self.piano.tool = Tool::Zoom,
            Key::Y => self.piano.tool = Tool::Playback,
            Key::S => self.piano.slide = !self.piano.slide,
            Key::O => self.piano.porta = !self.piano.porta,
            Key::M => self.piano.names = !self.piano.names,
            _ => return false,
        }
        true
    }

    fn piano_header(&mut self, ui: &mut Ui, pat: usize) {
        let name = self.song.channels.get(self.sel_chan).map(|c| c.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open score...").clicked() {
                    ui.close_menu();
                    self.open_score_dialog();
                }
                if ui.button("Save score as...").clicked() {
                    ui.close_menu();
                    self.save_score_dialog();
                }
                if ui.button("Import MIDI file...     Ctrl+M").clicked() {
                    ui.close_menu();
                    self.open_score_dialog();
                }
                if ui.button("Export as MIDI file...  Shift+Ctrl+M").clicked() {
                    ui.close_menu();
                    self.export_midi();
                }
            });
            // every entry presses its own key, so the menus and the keyboard can never drift apart
            const N: Modifiers = Modifiers::NONE;
            const C: Modifiers = Modifiers::COMMAND;
            const A: Modifiers = Modifiers::ALT;
            const S: Modifiers = Modifiers::SHIFT;
            const SA: Modifiers = Modifiers { alt: true, ctrl: false, shift: true, mac_cmd: false, command: false };
            type Item = (&'static str, Modifiers, Key);
            const EDIT: [Item; 16] = [
                ("Delete                      Del", N, Key::Delete),
                ("Duplicate                   Ctrl+B", C, Key::B),
                ("", N, Key::F35),
                ("Shift left                  Shift+Left", S, Key::ArrowLeft),
                ("Shift right                 Shift+Right", S, Key::ArrowRight),
                ("Transpose up                Shift+Up", S, Key::ArrowUp),
                ("Transpose down              Shift+Down", S, Key::ArrowDown),
                ("Transpose one octave up     Ctrl+Up", C, Key::ArrowUp),
                ("Transpose one octave down   Ctrl+Down", C, Key::ArrowDown),
                ("", N, Key::F35),
                ("Discard lengths             Shift+D", S, Key::D),
                ("Change color                Alt+C", A, Key::C),
                ("Mute                        Alt+M", A, Key::M),
                ("Unmute                      Shift+Alt+M", SA, Key::M),
                ("Insert space                Ctrl+Ins", C, Key::Insert),
                ("Delete space                Ctrl+Del", C, Key::Delete),
            ];
            const QUICK: [Item; 5] = [
                ("Quick legato                Ctrl+L", C, Key::L),
                ("Quick quantise              Ctrl+Q", C, Key::Q),
                ("Quick quantise start times  Shift+Q", S, Key::Q),
                ("Quick chop                  Ctrl+U", C, Key::U),
                ("Glue                        Ctrl+G", C, Key::G),
            ];
            const SELECT: [Item; 8] = [
                ("Select all                  Ctrl+A", C, Key::A),
                ("Deselect                    Ctrl+D", C, Key::D),
                ("Invert selection            Shift+I", S, Key::I),
                ("Select time around selection  Ctrl+Enter", C, Key::Enter),
                ("Select previous             Ctrl+Left", C, Key::ArrowLeft),
                ("Select next                 Ctrl+Right", C, Key::ArrowRight),
                ("Select by colour            Shift+C", S, Key::C),
                ("Select random               Shift+R", S, Key::R),
            ];
            const GROUP: [Item; 2] = [("Group                       Shift+G", S, Key::G), ("Ungroup                     Alt+G", A, Key::G)];
            const ZOOM: [Item; 8] = [
                ("Zoom in                     PgUp", N, Key::PageUp),
                ("Zoom out                    PgDn", N, Key::PageDown),
                ("One bar                     Shift+1", S, Key::Num1),
                ("Four bars                   Shift+2", S, Key::Num2),
                ("Sixteen bars                Shift+3", S, Key::Num3),
                ("Show all                    Shift+4", S, Key::Num4),
                ("Zoom on selection           Shift+5", S, Key::Num5),
                ("Centre on the play position Shift+0", S, Key::Num0),
            ];
            let mut press: Option<(Modifiers, Key)> = None;
            let list = |ui: &mut Ui, items: &[Item], press: &mut Option<(Modifiers, Key)>| {
                for (label, m, k) in items {
                    if label.is_empty() {
                        ui.separator();
                    } else if ui.button(RichText::new(*label).monospace().size(11.0)).clicked() {
                        *press = Some((*m, *k));
                        ui.close_menu();
                    }
                }
            };
            ui.menu_button("Edit", |ui| {
                for (label, cut) in [("Cut                         Ctrl+X", true), ("Copy                        Ctrl+C", false)] {
                    if ui.button(RichText::new(label).monospace().size(11.0)).clicked() {
                        self.pr_copy(ui.ctx(), cut);
                        ui.close_menu();
                    }
                }
                if ui.button(RichText::new("Paste                       Ctrl+V").monospace().size(11.0)).clicked() {
                    self.pr_paste();
                    ui.close_menu();
                }
                list(ui, &EDIT, &mut press);
            });
            ui.menu_button("Tools", |ui| {
                let mut helper = None;
                if ui.button(RichText::new(HELPERS[0].0).monospace().size(11.0)).clicked() {
                    helper = Some(0);
                    ui.close_menu();
                }
                ui.separator();
                list(ui, &QUICK, &mut press);
                ui.separator();
                for (i, (label, _)) in HELPERS.iter().enumerate().skip(1) {
                    if ui.button(RichText::new(*label).monospace().size(11.0)).clicked() {
                        helper = Some(i);
                        ui.close_menu();
                    }
                }
                if let Some(i) = helper {
                    self.pr_helper(i);
                }
            });
            ui.menu_button("View", |ui| {
                let f = |t: &str| RichText::new(t.to_string()).monospace().size(11.0);
                ui.checkbox(&mut self.piano.ghosts, f("Ghost channels              Alt+V"));
                ui.checkbox(&mut self.piano.rows, f("Row highlights              Alt+B"));
                ui.checkbox(&mut self.piano.wave, f("Background waveform         Alt+N"));
                ui.checkbox(&mut self.piano.names, f("Note names                  M"));
            });
            ui.menu_button("Select", |ui| list(ui, &SELECT, &mut press));
            ui.menu_button("Group", |ui| list(ui, &GROUP, &mut press));
            ui.menu_button("Zoom", |ui| list(ui, &ZOOM, &mut press));
            ui.menu_button("Target channel", |ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for c in 0..self.song.channels.len() {
                        if ui.selectable_label(c == self.sel_chan, self.song.channels[c].name.clone()).clicked() {
                            self.sel_chan = c;
                            ui.close_menu();
                        }
                    }
                });
            });
            if let Some((m, k)) = press {
                self.piano_key(k, m, false);
            }
            for (t, label, hint) in TOOLS {
                if ui.selectable_label(self.piano.tool == t, RichText::new(label).small()).hint(hint).clicked() {
                    self.piano.tool = t;
                }
            }
            ui.separator();
            egui::ComboBox::from_id_salt("pr_snap").width(74.0).selected_text(SNAPS[self.piano.snap].0).show_ui(ui, |ui| {
                for (i, (label, _, _)) in SNAPS.iter().enumerate() {
                    ui.selectable_value(&mut self.piano.snap, i, *label);
                }
            })
            .response
            .hint("Snap");
            if ui.selectable_label(self.piano.slide, RichText::new("Slide").small()).hint("Slide (S): notes drawn next bend the sounding notes to their pitch").clicked() {
                self.piano.slide = !self.piano.slide;
            }
            if ui.selectable_label(self.piano.porta, RichText::new("Porta").small()).hint("Portamento (O): notes drawn next glide in from the note before").clicked() {
                self.piano.porta = !self.piano.porta;
            }
            let cur = self.piano.color;
            egui::ComboBox::from_id_salt("pr_color").width(34.0).selected_text(RichText::new(format!("{}", cur + 1)).small().color(NOTE_COLORS[cur as usize & 15])).show_ui(ui, |ui| {
                for (i, col) in NOTE_COLORS.iter().enumerate() {
                    if ui.selectable_label(cur as usize == i, RichText::new(format!("Colour {}", i + 1)).color(*col)).clicked() {
                        self.piano.color = i as u8;
                        // the selected notes take the colour too
                        let pat = self.piano.sel_for.0;
                        let sel = self.piano.sel.clone();
                        for k in sel {
                            if let Some(n) = self.song.patterns.get_mut(pat).and_then(|p| p.notes.get_mut(k)) {
                                n.color = i as u8;
                                self.dirty = true;
                            }
                        }
                    }
                }
            });
            let stamp_name = if self.piano.stamp == 0 { "Stamp" } else { STAMPS[self.piano.stamp.min(STAMPS.len() - 1)].0 };
            ui.menu_button(RichText::new(stamp_name).small(), |ui| {
                for (i, (n, _)) in STAMPS.iter().enumerate() {
                    let label = if i == 0 { "None        Shift+N" } else { n };
                    if ui.selectable_label(self.piano.stamp == i, label).clicked() {
                        self.piano.stamp = i;
                        ui.close_menu();
                    }
                }
            });
            let (root, scale) = (self.piano.scale_root, self.piano.scale);
            let scale_name = if scale == 0 { "Scale".to_string() } else { format!("{} {}", NAMES[root as usize % 12], SCALES[scale].0) };
            ui.menu_button(RichText::new(scale_name).small(), |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        for (i, n) in NAMES.iter().enumerate() {
                            ui.selectable_value(&mut self.piano.scale_root, i as u8, *n);
                        }
                    });
                    ui.vertical(|ui| {
                        for (i, (n, _)) in SCALES.iter().enumerate() {
                            ui.selectable_value(&mut self.piano.scale, i, *n);
                        }
                    });
                });
            })
            .response
            .hint("Scale highlighting");
            if ui.selectable_label(self.piano.ghosts, RichText::new("Ghosts").small()).hint("Show the other channels' notes behind").clicked() {
                self.piano.ghosts = !self.piano.ghosts;
            }
            ui.separator();
            ui.label(RichText::new(format!("{}  -  {}", self.song.pattern_name(pat), name)).color(TEXT));
        });
    }

    pub(crate) fn piano_ui(&mut self, ui: &mut Ui) {
        let pat = self.pr_pat();
        self.pr_whole_groups(pat);
        let chan = self.sel_chan as u16;
        let ppq = self.song.ppq as f32;
        let step = self.song.step();
        let bar = self.song.bar();
        if self.piano.last_len == 0 {
            self.piano.last_len = step;
        }
        // the selection belongs to one channel of one pattern, and only to notes that still exist
        {
            let notes = &self.song.patterns[pat].notes;
            if self.piano.sel_for != (pat, chan) || self.piano.sel.iter().any(|&i| i >= notes.len() || notes[i].chan != chan) {
                self.piano.sel.clear();
                self.piano.sel_for = (pat, chan);
                self.piano.props = None;
                if !matches!(self.piano.drag, None | Some(Drag::Keys) | Some(Drag::Seek)) {
                    self.piano.drag = None;
                }
            }
        }
        self.piano_header(ui, pat);

        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(Vec2::new(240.0, 200.0)), Sense::click_and_drag());
        let lane_top = rect.bottom() - LANE_H;
        let grid = Rect::from_min_max(rect.min + Vec2::new(KEYS_W, RULER_H), Pos2::new(rect.right(), lane_top - 2.0));
        let keys = Rect::from_min_max(Pos2::new(rect.left(), grid.top()), Pos2::new(grid.left(), grid.bottom()));
        let ruler = Rect::from_min_max(Pos2::new(grid.left(), rect.top()), Pos2::new(rect.right(), grid.top()));
        let lane = Rect::from_min_max(Pos2::new(grid.left(), lane_top), rect.max);
        let lane_head = Rect::from_min_max(Pos2::new(rect.left(), lane_top), Pos2::new(grid.left(), rect.bottom()));

        let pointer = ui.input(|i| i.pointer.hover_pos());
        let inp = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.secondary_pressed(),
                i.pointer.secondary_down(),
                i.pointer.middle_down(),
                i.pointer.delta(),
                i.modifiers,
                i.pointer.button_double_clicked(egui::PointerButton::Primary),
            )
        });
        let (pressed, down, sec_pressed, sec_down, mid_down, delta, mods, double) = inp;
        if resp.hovered() && (pressed || sec_pressed) {
            self.focus = Win::Piano;
        }

        // scrolling and zoom
        let mut alt_wheel = 0.0_f32;
        if resp.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if mods.alt && !mods.shift && zoom == 1.0 {
                // Alt+wheel turns the property of the note under the pointer
                alt_wheel = scroll.y + scroll.x;
            } else if zoom != 1.0 {
                let old = self.piano.zoom;
                self.piano.zoom = (old * zoom).clamp(0.02, 12.0);
                if let Some(pp) = pointer {
                    // keep the tick under the pointer where it is
                    let off = pp.x - grid.left();
                    self.piano.scroll_x = (self.piano.scroll_x + off) * self.piano.zoom / old - off;
                }
            } else if mods.shift {
                self.piano.scroll_x -= scroll.y + scroll.x;
            } else {
                self.piano.scroll_y -= scroll.y;
                self.piano.scroll_x -= scroll.x;
            }
            if mid_down {
                self.piano.scroll_x -= delta.x;
                self.piano.scroll_y -= delta.y;
            }
        }
        let px = self.piano.zoom * 96.0 / ppq; // pixels per tick
        self.piano.scroll_x = self.piano.scroll_x.max(0.0);
        self.piano.scroll_y = self.piano.scroll_y.clamp(0.0, (132.0 * KEY_H - grid.height()).max(0.0));
        let (sx, sy) = (self.piano.scroll_x, self.piano.scroll_y);

        let x_of = |tick: f32| grid.left() + tick * px - sx;
        let tick_of = |x: f32| ((x - grid.left() + sx) / px).max(0.0);
        let y_of = |key: i32| grid.top() + (131 - key) as f32 * KEY_H - sy;
        let key_of = |y: f32| (131 - ((y - grid.top() + sy) / KEY_H).floor() as i32).clamp(0, 131);

        // the grid on screen: steps, split further when there is room, or only beats / bars when there is not
        let mut line = step;
        while line as f32 * px >= 36.0 && line % 2 == 0 && line > 1 {
            line /= 2;
        }
        if (step as f32 * px) < 6.0 {
            line = if ppq * px < 6.0 { bar } else { self.song.ppq };
        }
        self.piano.grid_line = line;
        self.piano.left_tick = tick_of(grid.left()) as u32;
        self.piano.view_w = grid.width();
        let g = if mods.alt { 1 } else { self.pr_snap() };
        let snap_floor = |t: f32| ((t.max(0.0) as u32) / g) * g;
        let snap_delta = |d: f32| ((d / g as f32).round() as i64) * g as i64;

        let p = ui.painter_at(rect);
        let (grid_dark, grid_light) = crate::widgets::grid_colours(self.settings.grid);
        p.rect_filled(rect, 0.0, grid_dark);

        // rows
        let top_key = key_of(grid.top());
        let bottom_key = key_of(grid.bottom());
        let gp = ui.painter_at(grid);
        for key in bottom_key..=top_key {
            let y = y_of(key);
            let row = Rect::from_min_max(Pos2::new(grid.left(), y), Pos2::new(grid.right(), y + KEY_H));
            // with a scale chosen its keys are the light rows, else the white keys are
            let lit = if self.piano.scale == 0 { !is_black(key) } else { prtools::in_scale(key as u8, self.piano.scale_root, self.piano.scale) };
            if lit && self.piano.rows {
                gp.rect_filled(row, 0.0, grid_light);
            }
            gp.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0_f32, if key % 12 == 11 { LINE } else { LINE_SOFT }));
        }
        if self.piano.wave {
            // the channel's sample behind the notes, laid out from the start of the pattern at the song tempo
            if let Some(smp) = self.song.channels.get(chan as usize).and_then(|c| c.sample.as_ref()) {
                let fpt = smp.rate as f32 * 60.0 / self.song.tempo.max(1.0) / ppq;
                let (mid, half) = (grid.center().y, grid.height() * 0.4);
                for x in grid.left() as i32..grid.right() as i32 {
                    let t0 = tick_of(x as f32);
                    let a = (t0 * fpt) as usize;
                    let b = (((t0 + 1.0 / px) * fpt) as usize).min(smp.frames.len());
                    if a >= b {
                        break;
                    }
                    let stride = ((b - a) / 32).max(1);
                    let peak = smp.frames[a..b].iter().step_by(stride).fold(0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                    let h = (peak * half).max(0.5);
                    gp.line_segment([Pos2::new(x as f32, mid - h), Pos2::new(x as f32, mid + h)], Stroke::new(1.0_f32, Color32::from_white_alpha(38)));
                }
            }
        }
        // columns (also through the lane)
        let last = tick_of(grid.right()) as u32 + line;
        let lp = ui.painter_at(lane);
        lp.rect_filled(lane, 0.0, PANEL_DARK);
        let mut t = (tick_of(grid.left()) as u32 / line) * line;
        while t <= last {
            let x = x_of(t as f32);
            let col = if t % bar == 0 {
                LINE
            } else if t % self.song.ppq == 0 {
                LINE_SOFT
            } else {
                LINE_SOFT.gamma_multiply(0.6)
            };
            gp.line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, col));
            if t % self.song.ppq == 0 {
                lp.line_segment([Pos2::new(x, lane.top()), Pos2::new(x, lane.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
            }
            t += line;
        }

        // notes: the other channels behind, then ours
        let mut hit: Option<(usize, bool)> = None;
        let lane_frac = |n: &Note, which: Lane| match which {
            Lane::Vel => (n.vel / 1.28).clamp(0.0, 1.0),
            Lane::Pan => (n.pan * 0.5 + 0.5).clamp(0.0, 1.0),
            Lane::Fine => (n.fine_cents / 2400.0 + 0.5).clamp(0.0, 1.0),
            Lane::Release => n.release.clamp(0.0, 1.0),
            Lane::ModX => (n.mod_x * 0.5 + 0.5).clamp(0.0, 1.0),
            Lane::ModY => (n.mod_y * 0.5 + 0.5).clamp(0.0, 1.0),
        };
        let which = self.piano.lane;
        let from_foot = matches!(which, Lane::Vel | Lane::Release);
        for ghost in [true, false] {
            if ghost && !self.piano.ghosts {
                continue;
            }
            for (i, n) in self.song.patterns[pat].notes.iter().enumerate() {
                if (n.chan == chan) == ghost {
                    continue;
                }
                let len = note_len(n, step);
                let r = Rect::from_min_max(Pos2::new(x_of(n.pos as f32), y_of(n.key as i32) + 1.0), Pos2::new((x_of((n.pos + len) as f32) - 1.0).max(x_of(n.pos as f32) + 2.0), y_of(n.key as i32) + KEY_H));
                if r.right() < grid.left() || r.left() > grid.right() {
                    continue;
                }
                if ghost {
                    gp.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(150, 170, 180, 34));
                    continue;
                }
                let selected = self.piano.sel.contains(&i);
                let base = if n.muted {
                    TEXT_DIM.gamma_multiply(0.7)
                } else if selected {
                    SELECTED
                } else {
                    let c = if n.color == 0 { NOTE } else { NOTE_COLORS[n.color as usize & 15] };
                    c.gamma_multiply(0.55 + 0.45 * n.vel.clamp(0.0, 1.28) / 1.28)
                };
                gp.rect_filled(r, 2.0, base);
                if n.slide {
                    // the small triangle that marks a slide note
                    let (a, h) = (r.right_top() + Vec2::new(-1.0, 1.0), r.height() - 2.0);
                    gp.add(egui::Shape::convex_polygon(vec![a, a + Vec2::new(0.0, h), a + Vec2::new(-h.min(r.width() - 2.0), h)], LINE, Stroke::NONE));
                }
                if n.porta {
                    let c = r.right_center() - Vec2::new(5.0, 0.0);
                    gp.line_segment([c + Vec2::new(-3.0, 3.0), c + Vec2::new(3.0, -3.0)], Stroke::new(1.5_f32, LINE));
                }
                gp.rect_stroke(r, 2.0, Stroke::new(1.0_f32, if selected { Color32::WHITE } else { LINE }), egui::StrokeKind::Inside);
                if r.width() > 24.0 {
                    gp.text(r.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, format!("{}{}", NAMES[n.key as usize % 12], n.key / 12), FontId::proportional(9.5), LINE);
                }
                if let Some(pp) = pointer {
                    if r.contains(pp) && grid.contains(pp) {
                        hit = Some((i, pp.x > r.right() - 5.0 && r.width() > 8.0));
                    }
                }
                // the note's stick in the lane
                let x = x_of(n.pos as f32);
                let y = lane.bottom() - 3.0 - lane_frac(n, which) * (LANE_H - 8.0);
                let col = if selected { SELECTED } else { NOTE };
                let foot = if from_foot { lane.bottom() } else { lane.center().y };
                lp.line_segment([Pos2::new(x, foot), Pos2::new(x, y)], Stroke::new(1.5_f32, col));
                lp.line_segment([Pos2::new(x, y), Pos2::new((x + len as f32 * px).min(x + 40.0), y)], Stroke::new(1.5_f32, col));
                lp.circle_filled(Pos2::new(x, y), 2.5, col);
            }
        }
        if alt_wheel != 0.0 {
            if let Some((i, _)) = hit {
                let step = alt_wheel / 400.0;
                let sel = &self.piano.sel;
                let all = sel.contains(&i);
                for (j, n) in self.song.patterns[pat].notes.iter_mut().enumerate() {
                    if j != i && !(all && sel.contains(&j) && n.chan == chan) {
                        continue;
                    }
                    match which {
                        Lane::Vel => n.vel = (n.vel + step * 1.28).clamp(0.0, 1.28),
                        Lane::Pan => n.pan = (n.pan + step * 2.0).clamp(-1.0, 1.0),
                        Lane::Fine => n.fine_cents = (n.fine_cents + step * 2400.0).round().clamp(-1200.0, 1200.0),
                        Lane::Release => n.release = (n.release + step).clamp(0.0, 1.0),
                        Lane::ModX => n.mod_x = (n.mod_x + step * 2.0).clamp(-1.0, 1.0),
                        Lane::ModY => n.mod_y = (n.mod_y + step * 2.0).clamp(-1.0, 1.0),
                    }
                    self.dirty = true;
                }
            }
        }

        // play head
        if !self.handle.transport.song_mode() {
            let x = x_of(self.handle.transport.tick() as f32);
            if x >= grid.left() && x <= grid.right() {
                p.line_segment([Pos2::new(x, ruler.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, GREEN));
            }
        }

        // ruler
        p.rect_filled(ruler, 0.0, PANEL_DARK);
        if let Some((a, b)) = self.piano.time_sel {
            let (xa, xb) = (x_of(a as f32).max(grid.left()), x_of(b as f32).min(grid.right()));
            if xb > xa {
                let red = Color32::from_rgb(200, 70, 60);
                p.rect_filled(Rect::from_min_max(Pos2::new(xa, ruler.top()), Pos2::new(xb, ruler.bottom())), 0.0, red.gamma_multiply(0.7));
                p.rect_filled(Rect::from_min_max(Pos2::new(xa, grid.top()), Pos2::new(xb, grid.bottom())), 0.0, red.gamma_multiply(0.08));
            }
        }
        let bars_px = bar as f32 * px;
        let every = if bars_px < 14.0 { 8 } else if bars_px < 28.0 { 4 } else if bars_px < 50.0 { 2 } else { 1 };
        let mut b = (tick_of(grid.left()) as u32 / bar) * bar;
        while b <= last {
            let x = x_of(b as f32);
            if x >= grid.left() - 1.0 && (b / bar) % every == 0 {
                p.text(Pos2::new(x + 3.0, ruler.center().y), Align2::LEFT_CENTER, format!("{}", b / bar + 1), FontId::proportional(10.5), TEXT_DIM);
            }
            b += bar;
        }

        // keyboard
        p.rect_filled(Rect::from_min_max(rect.min, Pos2::new(grid.left(), grid.top())), 0.0, PANEL_DARK);
        let kp = ui.painter_at(keys);
        for key in bottom_key..=top_key {
            let y = y_of(key);
            let r = Rect::from_min_max(Pos2::new(keys.left(), y), Pos2::new(keys.right(), y + KEY_H));
            let sounding = self.piano.sounding.map(|s| s.1 as i32 == key).unwrap_or(false);
            let names = self.piano.names;
            let fill = if sounding {
                ACCENT
            } else if names {
                Color32::from_gray(if is_black(key) { 46 } else { 62 })
            } else if is_black(key) {
                Color32::from_gray(38)
            } else {
                Color32::from_gray(225)
            };
            kp.rect_filled(r.shrink2(Vec2::new(0.0, 0.5)), 0.0, fill);
            if names {
                kp.text(r.right_center() - Vec2::new(4.0, 0.0), Align2::RIGHT_CENTER, format!("{}{}", NAMES[key as usize % 12], key / 12), FontId::proportional(9.5), Color32::from_gray(215));
            } else if key % 12 == 0 {
                kp.text(r.right_center() - Vec2::new(4.0, 0.0), Align2::RIGHT_CENTER, format!("C{}", key / 12), FontId::proportional(10.0), Color32::from_gray(60));
            }
        }

        // lane head: which property the lane shows; click to step through them
        p.rect_filled(lane_head, 0.0, PANEL_DARK);
        p.line_segment([Pos2::new(rect.left(), lane_top - 1.0), Pos2::new(rect.right(), lane_top - 1.0)], Stroke::new(2.0_f32, LINE));
        let lane_name = LANES.iter().find(|l| l.0 == which).map(|l| l.1).unwrap_or("");
        p.text(lane_head.left_top() + Vec2::new(6.0, 6.0), Align2::LEFT_TOP, "Control", FontId::proportional(9.5), TEXT_DIM);
        p.text(lane_head.left_top() + Vec2::new(6.0, 20.0), Align2::LEFT_TOP, lane_name, FontId::proportional(11.0), TEXT);
        if !from_foot {
            lp.line_segment([Pos2::new(lane.left(), lane.center().y), Pos2::new(lane.right(), lane.center().y)], Stroke::new(1.0_f32, LINE));
        }

        // ---- interaction
        let over = pointer.filter(|pp| resp.hovered() && rect.contains(*pp));
        if let Some(pp) = over {
            let tool = self.piano.tool;
            if pressed && self.piano.drag.is_none() {
                if lane_head.contains(pp) {
                    let at = LANES.iter().position(|l| l.0 == which).unwrap_or(0);
                    self.piano.lane = LANES[(at + 1) % LANES.len()].0;
                } else if ruler.contains(pp) && mods.command {
                    self.piano.time_sel = None;
                    self.piano.drag = Some(Drag::TimeSel { from: snap_floor(tick_of(pp.x)) });
                } else if ruler.contains(pp) {
                    self.piano.drag = Some(Drag::Seek);
                } else if keys.contains(pp) {
                    self.piano.drag = Some(Drag::Keys);
                } else if lane.contains(pp) {
                    self.piano.lane_x = None;
                    self.piano.drag = Some(Drag::Lane);
                } else if grid.contains(pp) {
                    let key = key_of(pp.y) as u8;
                    match tool {
                        Tool::Draw | Tool::Paint if mods.command => self.piano.drag = Some(Drag::Rect { from: pp, add: mods.shift, what: RectUse::Select }),
                        Tool::Draw | Tool::Paint => match hit {
                            Some((i, _)) if double => self.piano.props = Some(i),
                            Some((i, edge)) => {
                                let notes = &mut self.song.patterns[pat].notes;
                                if !self.piano.sel.contains(&i) {
                                    self.piano.sel.clear();
                                }
                                let mut targets: Vec<usize> = if !self.piano.sel.is_empty() {
                                    self.piano.sel.iter().copied().collect()
                                } else if notes[i].group != 0 {
                                    // a grouped note takes its group along
                                    (0..notes.len()).filter(|&k| notes[k].chan == chan && notes[k].group == notes[i].group).collect()
                                } else {
                                    vec![i]
                                };
                                let mut grabbed = i;
                                if mods.shift && !edge {
                                    // leave the originals where they are and carry copies
                                    let was_sel = !self.piano.sel.is_empty();
                                    self.piano.sel.clear();
                                    let mut copies = vec![];
                                    for &t in &targets {
                                        if t == i {
                                            grabbed = notes.len();
                                        }
                                        copies.push(notes.len());
                                        let n = notes[t].clone();
                                        notes.push(n);
                                    }
                                    crate::widgets::fresh_groups(notes, &copies, |n| &mut n.group);
                                    if was_sel {
                                        self.piano.sel = copies.iter().copied().collect();
                                    }
                                    targets = copies;
                                    self.dirty = true;
                                }
                                let n = notes[grabbed].clone();
                                self.piano.last_len = note_len(&n, step);
                                self.piano.last_vel = n.vel;
                                self.piano.last_pan = n.pan;
                                let orig = targets.iter().map(|&t| (t, notes[t].clone())).collect();
                                self.piano.drag = Some(Drag::Notes { mode: if edge { Mode::Resize } else { Mode::Move }, anchor_tick: tick_of(pp.x), anchor_key: key as i32, orig });
                                self.pr_sound(Some(n.key));
                            }
                            None => {
                                let pos = snap_floor(tick_of(pp.x));
                                let n = Note { pos, len: self.piano.last_len, chan, key, vel: self.piano.last_vel, pan: self.piano.last_pan, slide: self.piano.slide, porta: self.piano.porta, color: self.piano.color, ..Default::default() };
                                let notes = &mut self.song.patterns[pat].notes;
                                let mut orig = vec![];
                                for add in STAMPS[self.piano.stamp.min(STAMPS.len() - 1)].1 {
                                    let k = key as i32 + *add as i32;
                                    if (0..=131).contains(&k) {
                                        let n = Note { key: k as u8, ..n.clone() };
                                        notes.push(n.clone());
                                        orig.push((notes.len() - 1, n));
                                    }
                                }
                                self.piano.sel.clear();
                                self.piano.drag = Some(if tool == Tool::Paint { Drag::Paint } else { Drag::Notes { mode: Mode::Move, anchor_tick: tick_of(pp.x), anchor_key: key as i32, orig } });
                                self.dirty = true;
                                self.pr_sound(Some(key));
                            }
                        },
                        Tool::Delete => self.piano.drag = Some(Drag::Sweep { mute: None, right: false }),
                        Tool::Mute => {
                            let to = hit.map(|(i, _)| !self.song.patterns[pat].notes[i].muted).unwrap_or(true);
                            self.piano.drag = Some(Drag::Sweep { mute: Some(to), right: false });
                        }
                        Tool::Slice => self.piano.drag = Some(Drag::Slice { from: pp }),
                        Tool::Select => self.piano.drag = Some(Drag::Rect { from: pp, add: mods.shift, what: RectUse::Select }),
                        Tool::Zoom => self.piano.drag = Some(Drag::Rect { from: pp, add: false, what: RectUse::Zoom }),
                        Tool::Playback => self.piano.drag = Some(Drag::Seek),
                    }
                }
            } else if sec_pressed && self.piano.drag.is_none() && grid.contains(pp) {
                match tool {
                    Tool::Zoom => {
                        let off = pp.x - grid.left();
                        let old = self.piano.zoom;
                        self.piano.zoom = (old * 0.5).max(0.02);
                        self.piano.scroll_x = ((self.piano.scroll_x + off) * self.piano.zoom / old - off).max(0.0);
                    }
                    Tool::Select if hit.is_none() => self.piano.sel.clear(),
                    _ => self.piano.drag = Some(Drag::Sweep { mute: None, right: true }),
                }
            } else if sec_pressed && lane.contains(pp) {
                // right-click in the lane puts the property back to its default
                let near: Vec<usize> = self.song.patterns[pat].notes.iter().enumerate().filter(|(_, n)| n.chan == chan && (x_of(n.pos as f32) - pp.x).abs() < 6.0).map(|(i, _)| i).collect();
                for i in near {
                    if self.piano.sel.is_empty() || self.piano.sel.contains(&i) {
                        let n = &mut self.song.patterns[pat].notes[i];
                        match which {
                            Lane::Vel => n.vel = 1.0,
                            Lane::Pan => n.pan = 0.0,
                            Lane::Fine => n.fine_cents = 0.0,
                            Lane::Release => n.release = 0.5,
                            Lane::ModX => n.mod_x = 0.0,
                            Lane::ModY => n.mod_y = 0.0,
                        }
                        self.dirty = true;
                    }
                }
            }
            if self.piano.drag.is_none() && hit.map(|h| h.1).unwrap_or(false) && matches!(tool, Tool::Draw | Tool::Paint) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }

        // a drag in progress follows the pointer even outside the window
        let mut done = false;
        if let (Some(pp), Some(drag)) = (pointer, self.piano.drag.as_ref()) {
            match drag {
                Drag::Notes { mode, anchor_tick, anchor_key, orig } => {
                    if down {
                        let dt = snap_delta(tick_of(pp.x) - anchor_tick);
                        let dk = key_of(pp.y) - anchor_key;
                        let notes = &mut self.song.patterns[pat].notes;
                        match mode {
                            Mode::Move => {
                                let dt = dt.max(-(orig.iter().map(|(_, n)| n.pos).min().unwrap_or(0) as i64));
                                let dk = dk.clamp(-(orig.iter().map(|(_, n)| n.key as i32).min().unwrap_or(0)), 131 - orig.iter().map(|(_, n)| n.key as i32).max().unwrap_or(0));
                                let mut heard = None;
                                for (i, o) in orig {
                                    let n = &mut notes[*i];
                                    let (pos, key) = ((o.pos as i64 + dt) as u32, (o.key as i32 + dk) as u8);
                                    if n.pos != pos || n.key != key {
                                        n.pos = pos;
                                        n.key = key;
                                        self.dirty = true;
                                    }
                                    if o.key as i32 == *anchor_key || heard.is_none() {
                                        heard = Some(key);
                                    }
                                }
                                if dk != 0 || self.piano.sounding.is_some() {
                                    self.pr_sound(heard);
                                }
                            }
                            Mode::Resize => {
                                for (i, o) in orig {
                                    let len = (note_len(o, step) as i64 + dt).max(g.min(step) as i64) as u32;
                                    if notes[*i].len != len {
                                        notes[*i].len = len;
                                        self.dirty = true;
                                    }
                                    self.piano.last_len = len;
                                }
                            }
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Rect { from, add, what } => {
                    let r = Rect::from_two_pos(*from, pp).intersect(grid);
                    if down {
                        ui.painter_at(grid).rect(r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18), Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
                    } else {
                        match what {
                            RectUse::Select => {
                                if !add {
                                    self.piano.sel.clear();
                                }
                                let (t0, t1) = (tick_of(r.left()), tick_of(r.right()));
                                let (k0, k1) = (key_of(r.bottom()), key_of(r.top()));
                                for (i, n) in self.song.patterns[pat].notes.iter().enumerate() {
                                    let end = (n.pos + note_len(n, step)) as f32;
                                    if n.chan == chan && (n.pos as f32) < t1.max(t0 + 1.0) && end > t0 && (k0..=k1).contains(&(n.key as i32)) {
                                        self.piano.sel.insert(i);
                                    }
                                }
                            }
                            RectUse::Zoom => {
                                if r.width() > 6.0 {
                                    let (t0, t1) = (tick_of(r.left()), tick_of(r.right()));
                                    let new_px = (grid.width() / (t1 - t0).max(1.0)).min(12.0 * 96.0 / ppq);
                                    self.piano.zoom = new_px * ppq / 96.0;
                                    self.piano.scroll_x = t0 * new_px;
                                }
                            }
                        }
                        done = true;
                    }
                }
                Drag::Sweep { mute, right } => {
                    let held = if *right { sec_down } else { down };
                    if !held {
                        done = true;
                    } else if let Some((i, _)) = hit {
                        match mute {
                            None => {
                                let gone: BTreeSet<usize> = [i].into();
                                self.pr_remove(pat, &gone);
                            }
                            Some(to) => {
                                let n = &mut self.song.patterns[pat].notes[i];
                                if n.muted != *to {
                                    n.muted = *to;
                                    self.dirty = true;
                                }
                            }
                        }
                    }
                }
                Drag::Slice { from } => {
                    if down {
                        ui.painter_at(grid).line_segment([*from, pp], Stroke::new(1.5_f32, Color32::WHITE));
                    } else {
                        let (a, b) = (*from, pp);
                        let notes = &mut self.song.patterns[pat].notes;
                        let n0 = notes.len();
                        for i in 0..n0 {
                            if notes[i].chan != chan {
                                continue;
                            }
                            let yc = y_of(notes[i].key as i32) + KEY_H * 0.5;
                            if (yc - a.y) * (yc - b.y) > 0.0 || (a.y - b.y).abs() < 0.5 {
                                continue;
                            }
                            let x = a.x + (yc - a.y) / (b.y - a.y) * (b.x - a.x);
                            let t = tick_of(x);
                            let at = (((t / g as f32).round() as u32) * g).max(1);
                            let len = note_len(&notes[i], step);
                            if at > notes[i].pos && at < notes[i].pos + len {
                                let mut tail = notes[i].clone();
                                tail.pos = at;
                                tail.len = notes[i].pos + len - at;
                                notes[i].len = at - notes[i].pos;
                                if self.piano.sel.contains(&i) {
                                    self.piano.sel.insert(notes.len());
                                }
                                notes.push(tail);
                                self.dirty = true;
                            }
                        }
                        done = true;
                    }
                }
                Drag::Paint => {
                    if down {
                        let key = key_of(pp.y) as u8;
                        let pos = snap_floor(tick_of(pp.x));
                        let len = self.piano.last_len.max(1);
                        // lay notes end to end from the grid position under the pointer
                        let pos = pos / len.max(g) * len.max(g);
                        let notes = &mut self.song.patterns[pat].notes;
                        let taken = notes.iter().any(|n| n.chan == chan && n.key == key && n.pos < pos + len && pos < n.pos + note_len(n, step));
                        if !taken && grid.contains(pp) {
                            notes.push(Note { pos, len, chan, key, vel: self.piano.last_vel, pan: self.piano.last_pan, slide: self.piano.slide, porta: self.piano.porta, color: self.piano.color, ..Default::default() });
                            self.dirty = true;
                            self.pr_sound(Some(key));
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Lane => {
                    if down {
                        let frac = ((lane.bottom() - 3.0 - pp.y) / (LANE_H - 8.0)).clamp(0.0, 1.0);
                        let from = self.piano.lane_x.unwrap_or(pp.x);
                        let (x0, x1) = (from.min(pp.x) - 5.0, from.max(pp.x) + 5.0);
                        self.piano.lane_x = Some(pp.x);
                        let sel = &self.piano.sel;
                        for (i, n) in self.song.patterns[pat].notes.iter_mut().enumerate() {
                            let x = x_of(n.pos as f32);
                            if n.chan != chan || x < x0 || x > x1 || !(sel.is_empty() || sel.contains(&i)) {
                                continue;
                            }
                            match which {
                                Lane::Vel => n.vel = frac * 1.28,
                                Lane::Pan => n.pan = if (frac - 0.5).abs() < 0.03 { 0.0 } else { frac * 2.0 - 1.0 },
                                Lane::Fine => n.fine_cents = if (frac - 0.5).abs() < 0.03 { 0.0 } else { ((frac - 0.5) * 2400.0).round() },
                                Lane::Release => n.release = frac,
                                Lane::ModX => n.mod_x = if (frac - 0.5).abs() < 0.03 { 0.0 } else { frac * 2.0 - 1.0 },
                                Lane::ModY => n.mod_y = if (frac - 0.5).abs() < 0.03 { 0.0 } else { frac * 2.0 - 1.0 },
                            }
                            self.dirty = true;
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::TimeSel { from } => {
                    if down {
                        let to = snap_floor(tick_of(pp.x) + g as f32 * 0.5);
                        let (a, b) = ((*from).min(to), (*from).max(to));
                        self.piano.time_sel = (b > a).then_some((a, b));
                    } else {
                        done = true;
                    }
                }
                Drag::Seek => {
                    if down {
                        if !self.handle.transport.song_mode() {
                            self.handle.transport.seek(snap_floor(tick_of(pp.x)));
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Keys => {
                    if down {
                        self.pr_sound(Some(key_of(pp.y) as u8));
                    } else {
                        done = true;
                    }
                }
            }
        } else if self.piano.drag.is_some() && !down && !sec_down {
            done = true;
        }
        if done {
            let was_notes = matches!(self.piano.drag, Some(Drag::Notes { .. }) | Some(Drag::Paint) | Some(Drag::Slice { .. }));
            self.piano.drag = None;
            if was_notes {
                self.pr_sort(pat);
                self.dirty = true;
            }
            self.pr_sound(None);
        }

        self.note_props(ui.ctx(), pat);
        self.helper_window(ui.ctx());
    }

    /// The small window a double-click on a note opens.
    fn note_props(&mut self, ctx: &egui::Context, pat: usize) {
        let Some(i) = self.piano.props else { return };
        let ppq = self.song.ppq;
        let step = self.song.step();
        let Some(n) = self.song.patterns[pat].notes.get_mut(i) else {
            self.piano.props = None;
            return;
        };
        let mut open = true;
        let mut changed = false;
        egui::Window::new("Note properties").id(egui::Id::new("noteprops")).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("np").num_columns(2).show(ui, |ui| {
                ui.label("Note");
                ui.label(format!("{}{}", NAMES[n.key as usize % 12], n.key / 12));
                ui.end_row();
                ui.label("Velocity");
                let mut v = n.vel / 1.28 * 100.0;
                if ui.add(egui::DragValue::new(&mut v).range(0.0..=100.0).speed(0.5).suffix(" %")).changed() {
                    n.vel = v / 100.0 * 1.28;
                    changed = true;
                }
                ui.end_row();
                ui.label("Pan");
                let mut v = n.pan * 100.0;
                if ui.add(egui::DragValue::new(&mut v).range(-100.0..=100.0).speed(0.5).suffix(" %")).changed() {
                    n.pan = v / 100.0;
                    changed = true;
                }
                ui.end_row();
                ui.label("Fine pitch");
                changed |= ui.add(egui::DragValue::new(&mut n.fine_cents).range(-1200.0..=1200.0).speed(1.0).suffix(" cents")).changed();
                ui.end_row();
                ui.label("Start");
                changed |= ui.add(egui::DragValue::new(&mut n.pos).speed(1.0).suffix(" ticks")).changed();
                ui.end_row();
                ui.label("Length");
                let mut len = if n.len == 0 { step } else { n.len };
                if ui.add(egui::DragValue::new(&mut len).range(1..=ppq * 64).speed(1.0).suffix(" ticks")).changed() {
                    n.len = len;
                    changed = true;
                }
                ui.end_row();
                ui.label("Release");
                let mut v = n.release * 100.0;
                if ui.add(egui::DragValue::new(&mut v).range(0.0..=100.0).speed(0.5).suffix(" %")).changed() {
                    n.release = v / 100.0;
                    changed = true;
                }
                ui.end_row();
                for (name, m) in [("Mod X", &mut n.mod_x), ("Mod Y", &mut n.mod_y)] {
                    ui.label(name);
                    let mut v = *m * 100.0;
                    if ui.add(egui::DragValue::new(&mut v).range(-100.0..=100.0).speed(0.5).suffix(" %")).changed() {
                        *m = v / 100.0;
                        changed = true;
                    }
                    ui.end_row();
                }
                ui.label("Colour");
                let mut c = n.color as u32 + 1;
                if ui.add(egui::DragValue::new(&mut c).range(1..=16)).changed() {
                    n.color = (c - 1) as u8;
                    changed = true;
                }
                ui.end_row();
                ui.label("Slide");
                changed |= ui.checkbox(&mut n.slide, "").changed();
                ui.end_row();
                ui.label("Portamento");
                changed |= ui.checkbox(&mut n.porta, "").changed();
                ui.end_row();
                ui.label("Muted");
                changed |= ui.checkbox(&mut n.muted, "").changed();
                ui.end_row();
            });
        });
        if changed {
            self.dirty = true;
        }
        if !open {
            self.piano.props = None;
        }
    }
}
