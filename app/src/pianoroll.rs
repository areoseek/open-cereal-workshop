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

use std::collections::BTreeSet;

use egui::{Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::Note;
use cereal_engine::Cmd;

use crate::widgets::*;
use crate::{App, Win};

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
}

const LANES: [(Lane, &str); 3] = [(Lane::Vel, "Velocity"), (Lane::Pan, "Pan"), (Lane::Fine, "Fine pitch")];

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
    drag: Option<Drag>,
    sounding: Option<(u16, u8)>,
    pub tool: Tool,
    snap: usize,
    lane: Lane,
    ghosts: bool,
    /// Selected notes, as indices into the pattern's note list.
    pub sel: BTreeSet<usize>,
    sel_for: (usize, u16),
    clip: Vec<Note>,
    props: Option<usize>,
    /// Ticks between the grid lines on screen, and the tick at the left edge (for key commands).
    grid_line: u32,
    left_tick: u32,
    lane_x: Option<f32>,
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
            drag: None,
            sounding: None,
            tool: Tool::Draw,
            snap: 0,
            lane: Lane::Vel,
            ghosts: true,
            sel: BTreeSet::new(),
            sel_for: (usize::MAX, 0),
            clip: vec![],
            props: None,
            grid_line: 24,
            left_tick: 0,
            lane_x: None,
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
    fn pr_snap(&self) -> u32 {
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

    fn pr_mute(&mut self, muted: bool) {
        let pat = self.pr_pat();
        let t = self.pr_targets(pat);
        for &i in &t {
            self.song.patterns[pat].notes[i].muted = muted;
        }
        self.dirty |= !t.is_empty();
    }

    /// A key press while the piano roll has the focus. True when it was ours.
    pub(crate) fn piano_key(&mut self, key: Key, m: Modifiers, repeat: bool) -> bool {
        if m.command && !m.alt {
            match key {
                Key::A => self.pr_select_all(),
                Key::D => self.piano.sel.clear(),
                Key::B => self.pr_duplicate(),
                Key::Q => self.pr_quantise(),
                Key::L => self.pr_legato(),
                Key::U => self.pr_chop(),
                Key::G => self.pr_glue(),
                Key::ArrowUp => self.pr_transpose(12),
                Key::ArrowDown => self.pr_transpose(-12),
                _ => return false,
            }
            return true;
        }
        if m.alt {
            match key {
                Key::M => self.pr_mute(!m.shift),
                _ => return false,
            }
            return true;
        }
        if m.shift {
            match key {
                Key::ArrowUp => self.pr_transpose(1),
                Key::ArrowDown => self.pr_transpose(-1),
                Key::ArrowLeft => self.pr_nudge(-1),
                Key::ArrowRight => self.pr_nudge(1),
                _ => return false,
            }
            return true;
        }
        match key {
            Key::Delete => self.pr_delete_selected(),
            _ if self.typing_keys || repeat => return false,
            Key::P => self.piano.tool = Tool::Draw,
            Key::B => self.piano.tool = Tool::Paint,
            Key::D => self.piano.tool = Tool::Delete,
            Key::T => self.piano.tool = Tool::Mute,
            Key::C => self.piano.tool = Tool::Slice,
            Key::E => self.piano.tool = Tool::Select,
            Key::Z => self.piano.tool = Tool::Zoom,
            Key::Y => self.piano.tool = Tool::Playback,
            _ => return false,
        }
        true
    }

    fn piano_header(&mut self, ui: &mut Ui, pat: usize) {
        let name = self.song.channels.get(self.sel_chan).map(|c| c.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.menu_button("Tools", |ui| {
                let mut act: Option<fn(&mut App)> = None;
                let items: [(&str, fn(&mut App)); 14] = [
                    ("Select all                  Ctrl+A", |a| a.pr_select_all()),
                    ("Deselect                    Ctrl+D", |a| a.piano.sel.clear()),
                    ("Invert selection", |a| a.pr_invert_selection()),
                    ("Delete                      Del", |a| a.pr_delete_selected()),
                    ("Duplicate                   Ctrl+B", |a| a.pr_duplicate()),
                    ("Paste                       Ctrl+V", |a| a.pr_paste()),
                    ("Quick quantise              Ctrl+Q", |a| a.pr_quantise()),
                    ("Quick legato                Ctrl+L", |a| a.pr_legato()),
                    ("Quick chop                  Ctrl+U", |a| a.pr_chop()),
                    ("Glue                        Ctrl+G", |a| a.pr_glue()),
                    ("Transpose up                Shift+Up", |a| a.pr_transpose(1)),
                    ("Transpose down              Shift+Down", |a| a.pr_transpose(-1)),
                    ("Mute                        Alt+M", |a| a.pr_mute(true)),
                    ("Unmute                      Shift+Alt+M", |a| a.pr_mute(false)),
                ];
                for (label, f) in items {
                    if ui.button(RichText::new(label).monospace().size(11.0)).clicked() {
                        act = Some(f);
                        ui.close_menu();
                    }
                }
                if let Some(f) = act {
                    f(self);
                }
            });
            for (t, label, hint) in TOOLS {
                if ui.selectable_label(self.piano.tool == t, RichText::new(label).small()).on_hover_text(hint).clicked() {
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
            .on_hover_text("Snap");
            if ui.selectable_label(self.piano.ghosts, RichText::new("Ghosts").small()).on_hover_text("Show the other channels' notes behind").clicked() {
                self.piano.ghosts = !self.piano.ghosts;
            }
            ui.separator();
            ui.label(RichText::new(format!("{}  -  {}", self.song.pattern_name(pat), name)).color(TEXT));
        });
    }

    pub(crate) fn piano_ui(&mut self, ui: &mut Ui) {
        let pat = self.pr_pat();
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
        if resp.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom != 1.0 {
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
        let g = if mods.alt { 1 } else { self.pr_snap() };
        let snap_floor = |t: f32| ((t.max(0.0) as u32) / g) * g;
        let snap_delta = |d: f32| ((d / g as f32).round() as i64) * g as i64;

        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, GRID_DARK);

        // rows
        let top_key = key_of(grid.top());
        let bottom_key = key_of(grid.bottom());
        let gp = ui.painter_at(grid);
        for key in bottom_key..=top_key {
            let y = y_of(key);
            let row = Rect::from_min_max(Pos2::new(grid.left(), y), Pos2::new(grid.right(), y + KEY_H));
            if !is_black(key) {
                gp.rect_filled(row, 0.0, GRID_LIGHT);
            }
            gp.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0_f32, if key % 12 == 11 { LINE } else { LINE_SOFT }));
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
        };
        let which = self.piano.lane;
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
                    NOTE.gamma_multiply(0.55 + 0.45 * n.vel.clamp(0.0, 1.28) / 1.28)
                };
                gp.rect_filled(r, 2.0, base);
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
                let foot = if which == Lane::Vel { lane.bottom() } else { lane.center().y };
                lp.line_segment([Pos2::new(x, foot), Pos2::new(x, y)], Stroke::new(1.5_f32, col));
                lp.line_segment([Pos2::new(x, y), Pos2::new((x + len as f32 * px).min(x + 40.0), y)], Stroke::new(1.5_f32, col));
                lp.circle_filled(Pos2::new(x, y), 2.5, col);
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
            let fill = if sounding { ACCENT } else if is_black(key) { Color32::from_gray(38) } else { Color32::from_gray(225) };
            kp.rect_filled(r.shrink2(Vec2::new(0.0, 0.5)), 0.0, fill);
            if key % 12 == 0 {
                kp.text(r.right_center() - Vec2::new(4.0, 0.0), Align2::RIGHT_CENTER, format!("C{}", key / 12), FontId::proportional(10.0), Color32::from_gray(60));
            }
        }

        // lane head: which property the lane shows; click to step through them
        p.rect_filled(lane_head, 0.0, PANEL_DARK);
        p.line_segment([Pos2::new(rect.left(), lane_top - 1.0), Pos2::new(rect.right(), lane_top - 1.0)], Stroke::new(2.0_f32, LINE));
        let lane_name = LANES.iter().find(|l| l.0 == which).map(|l| l.1).unwrap_or("");
        p.text(lane_head.left_top() + Vec2::new(6.0, 6.0), Align2::LEFT_TOP, "Control", FontId::proportional(9.5), TEXT_DIM);
        p.text(lane_head.left_top() + Vec2::new(6.0, 20.0), Align2::LEFT_TOP, lane_name, FontId::proportional(11.0), TEXT);
        if which != Lane::Vel {
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
                                let mut targets: Vec<usize> = if self.piano.sel.is_empty() { vec![i] } else { self.piano.sel.iter().copied().collect() };
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
                                let n = Note { pos, len: self.piano.last_len, chan, key, vel: self.piano.last_vel, pan: self.piano.last_pan, fine_cents: 0.0, muted: false };
                                let notes = &mut self.song.patterns[pat].notes;
                                notes.push(n.clone());
                                self.piano.sel.clear();
                                self.piano.drag = Some(if tool == Tool::Paint { Drag::Paint } else { Drag::Notes { mode: Mode::Move, anchor_tick: tick_of(pp.x), anchor_key: key as i32, orig: vec![(notes.len() - 1, n)] } });
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
                            notes.push(Note { pos, len, chan, key, vel: self.piano.last_vel, pan: self.piano.last_pan, fine_cents: 0.0, muted: false });
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
                            }
                            self.dirty = true;
                        }
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
