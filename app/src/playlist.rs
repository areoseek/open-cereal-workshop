//! The playlist: pattern clips and audio clips on tracks.

use crate::widgets::Hint as _;
use std::collections::BTreeSet;

use egui::{Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::{Clip, ClipSrc, Target, PERF_MOTION, PERF_PRESS, PERF_SYNC};
use cereal_engine::{Cmd, PerfSeg};

use crate::pianoroll::{snap_label, SNAPS, SNAP_MAIN};
use crate::widgets::*;
use crate::{App, Win};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Draw,
    Paint,
    Delete,
    Mute,
    Slip,
    Slice,
    Select,
    Zoom,
    Playback,
}

const TOOLS: [(Tool, &str, &str); 9] = [
    (Tool::Draw, "Draw", "Draw (P)"),
    (Tool::Paint, "Paint", "Paint (B)"),
    (Tool::Delete, "Delete", "Delete (D)"),
    (Tool::Mute, "Mute", "Mute (T)"),
    (Tool::Slip, "Slip", "Slip: move what is inside a clip (S)"),
    (Tool::Slice, "Slice", "Slice (C)"),
    (Tool::Select, "Select", "Select (E)"),
    (Tool::Zoom, "Zoom", "Zoom to selection (Z)"),
    (Tool::Playback, "Play", "Playback (Y)"),
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Move,
    Left,
    Right,
    Slip,
}

enum Drag {
    Clips { mode: Mode, anchor_tick: f32, anchor_track: i32, orig: Vec<(usize, Clip)> },
    Rect { from: Pos2, add: bool, zoom: bool },
    Sweep { mute: Option<bool>, right: bool },
    Paint,
    /// `del`: held with the right button, the shorter piece of the cut clip goes
    Slice { tick: u32, del: bool },
    Seek,
    /// Picking out a stretch of time on the ruler, from this tick.
    TimeSel { from: u32 },
    Marker(usize),
    /// A point of an automation clip: channel, point, clip, and where the clip's picture sits.
    AutoPt { chan: usize, idx: usize, clip: usize, left: f32, top: f32, bottom: f32, offset: u32 },
    /// The bend of the line arriving at a point.
    AutoBend { chan: usize, idx: usize, y0: f32, t0: f32 },
    /// A handle of a clip of sound: 0 fade in, 1 fade out, 2 level, 3 and 4 the bends of the two fades.
    /// With it the clip, the picture's left and right edge, and where the drag began (height and value).
    Fade { clip: usize, which: u8, left: f32, right: f32, y0: f32, v0: f32 },
    /// A press that has done its work and waits for the button to come up.
    Spent,
}

/// Where the handles of a clip of sound sit in its picture: fade in, fade out, level, and the two bends.
fn fade_handles(fx: &cereal_engine::song::ClipFx, body: Rect, left: f32, right: f32, px_per_ms: f32) -> [Pos2; 5] {
    use cereal_engine::song::ClipFx;
    let w = (right - left).max(1.0);
    let fi = (fx.fade_in * px_per_ms).clamp(0.0, w);
    let fo = (fx.fade_out * px_per_ms).clamp(0.0, w);
    let top = body.top() + 4.0;
    let y_of = |v: f32| body.bottom() - v * (body.bottom() - top);
    [
        Pos2::new(left + fi, top),
        Pos2::new(right - fo, top),
        Pos2::new((left + right) * 0.5, top),
        Pos2::new(left + fi * 0.5, y_of(ClipFx::curve(0.5, fx.in_tension))),
        Pos2::new(right - fo * 0.5, y_of(ClipFx::curve(0.5, fx.out_tension))),
    ]
}

/// The shapes a line between two automation points can take, with the number the file gives each.
const AUTO_MODES: [(u8, &str); 13] = [
    (0, "Single curve"),
    (1, "Double curve"),
    (2, "Hold"),
    (3, "Stairs"),
    (4, "Smooth stairs"),
    (5, "Pulse"),
    (6, "Wave"),
    (7, "Single curve 2"),
    (8, "Double curve 2"),
    (9, "Half sine"),
    (10, "Smooth"),
    (11, "Single curve 3"),
    (12, "Double curve 3"),
];

pub struct State {
    scroll_x: f32,
    scroll_y: f32,
    /// Pixels per tick at 96 ppq.
    zoom: f32,
    track_h: f32,
    /// A track being dragged taller or shorter by the lower edge of its name: the track, where the drag began
    /// and the size it had.
    size_drag: Option<(usize, f32, f32)>,
    drag: Option<Drag>,
    pub tool: Tool,
    pub(crate) snap: usize,
    pub sel: BTreeSet<usize>,
    /// The stretch of time picked out on the ruler; it plays round and round and edits by time work on it.
    pub(crate) time_sel: Option<(u32, u32)>,
    /// The grid point and the clip under the pointer, as of the last frame.
    hover: Option<(u32, Option<usize>)>,
    clip: Vec<Clip>,
    /// What the draw and paint tools put down: the last clip touched, else the current pattern.
    brush: Option<(ClipSrc, u32, u32)>,
    /// Stretch mode: dragging an audio clip's end fits the sound to the new length instead of showing more of it.
    pub(crate) stretch: bool,
    /// The picker panel at the left: every pattern, audio clip and automation clip, to pick what is drawn next.
    pub(crate) picker: bool,
    /// The clip length a stretch drag began from, by channel, and the beats the whole sound took then.
    stretch_from: Option<(usize, u32, f32)>,
    brush_pat: usize,
    grid_line: u32,
    left_tick: u32,
    /// The open menu of an automation point: channel, point, and where it was opened.
    auto_menu: Option<(usize, usize, Pos2)>,
    /// The open menu of a track: the track and where it was opened.
    track_menu: Option<(usize, Pos2)>,
    /// The clip whose menu is open (its top left corner was clicked), and where.
    clip_menu: Option<(usize, Pos2)>,
    /// Group heads whose tracks are folded away.
    pub(crate) collapsed: BTreeSet<usize>,
    /// A move of the play position that waits for its grid line: (when, where to).
    pub(crate) jump: Option<(u32, u32)>,
    /// What the picker panel's tools marked.
    pub(crate) marked: Vec<ClipSrc>,
    /// Performance mode: the tracks whose clips the mouse button is holding, and the keys holding tracks.
    perf_press: Vec<u16>,
    pub(crate) perf_held: std::collections::HashMap<Key, u16>,
    view_w: f32,
    snap_back: usize,
}

impl Default for State {
    fn default() -> Self {
        Self { scroll_x: 0.0, scroll_y: 0.0, zoom: 0.09, track_h: 30.0, size_drag: None, drag: None, tool: Tool::Draw, snap: SNAP_MAIN, time_sel: None, sel: BTreeSet::new(), hover: None, clip: vec![], brush: None, stretch: false, picker: false, stretch_from: None, brush_pat: usize::MAX, grid_line: 96, left_tick: 0, auto_menu: None, track_menu: None, clip_menu: None, collapsed: BTreeSet::new(), jump: None, marked: vec![], perf_press: vec![], perf_held: Default::default(), view_w: 800.0, snap_back: 0 }
    }
}

const HEAD_W: f32 = 112.0;

/// The playlist's option switches, kept in the settings as one number.
pub(crate) mod plo {
    pub const RESIZE_LEFT: u64 = 1;
    pub const WHEEL_NUDGE: u64 = 1 << 1;
    pub const HEAD_LOCK: u64 = 1 << 2;
    pub const AUTO_XFADE: u64 = 1 << 4;
    pub const MANUAL_FADES: u64 = 1 << 5;
    pub const SNAP_FADES: u64 = 1 << 6;
    pub const STRETCH_VARIANT: u64 = 1 << 7;
    pub const SELECT_ON_DROP: u64 = 1 << 8;
    pub const INVERT_GRID: u64 = 1 << 9;
    pub const TRACK_SEP: u64 = 1 << 10;
    pub const TIME_SEG: u64 = 1 << 11;
    pub const KEEP_LABELS: u64 = 1 << 12;
    pub const TITLE_CONTENT: u64 = 1 << 13;
    pub const SHADOW: u64 = 1 << 15;
    pub const HIDE_COLLAPSED: u64 = 1 << 16;
    pub const FADE_PREVIEW: u64 = 1 << 17;
    pub const GAIN_VALUE: u64 = 1 << 18;
    pub const GAIN_SCALE: u64 = 1 << 19;
    pub const INCR_SCROLL: u64 = 1 << 20;
    pub const PRECISE_TIME: u64 = 1 << 21;
    pub const PERF_CLIP: u64 = 1 << 22;
    pub const PERF_TRACK: u64 = 1 << 23;
    pub const AUDIO_CTRL: u64 = 1 << 24;
    pub const AUDIO_LEVELS: u64 = 1 << 25;
    pub const INST_LEVELS: u64 = 1 << 26;
    pub const LASSO: u64 = 1 << 27;
    pub const RESET_REMOTE: u64 = 1 << 28;
    pub const PICKER_RIGHT: u64 = 1 << 29;
    pub const PICKER_EMPTY: u64 = 1 << 30;
    pub const PICKER_DRAG: u64 = 1 << 31;
    pub const PICKER_GROUP: u64 = 1 << 32;
    pub const SCROLL_TO_POS: u64 = 1 << 33;
    pub const PICKER_LINKED: u64 = 1 << 34;
    pub const PICKER_FOLLOW: u64 = 1 << 35;
    pub const PICKER_SORT: u64 = 1 << 36;
    /// Two bits: what audio dropped on a track name becomes (0 audio clip, 1 audio track, 2 instrument track, 3 ask).
    pub const DROP_SHIFT: u32 = 40;
    /// Three bits: what lies behind the clips (0 nothing .. 5 solid).
    pub const BEHIND_SHIFT: u32 = 44;
    /// Two bits: grid contrast (0 low, 1 normal, 2 high).
    pub const CONTRAST_SHIFT: u32 = 48;
    pub const DEFAULT: u64 = (1 << CONTRAST_SHIFT) | RESIZE_LEFT | SNAP_FADES | STRETCH_VARIANT | TRACK_SEP | TIME_SEG | KEEP_LABELS | PERF_CLIP | PRECISE_TIME | RESET_REMOTE | PICKER_EMPTY | PICKER_FOLLOW | SCROLL_TO_POS | (1 << BEHIND_SHIFT);
}
const RULER_H: f32 = 18.0;

impl App {
    /// The menu of an automation point: the shape of the line arriving at it, and its removal.
    fn auto_point_menu(&mut self, ctx: &egui::Context) {
        let Some((chan, idx, at)) = self.playlist.auto_menu else { return };
        let Some(mode) = self.song.channels.get(chan).and_then(|c| c.auto.as_ref()).and_then(|a| a.points.get(idx)).map(|q| q.mode) else {
            self.playlist.auto_menu = None;
            return;
        };
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("auto_point_menu")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(130.0);
                let mut edit: Option<Box<dyn FnOnce(&mut cereal_engine::song::Automation)>> = None;
                if ui.button("Delete").clicked() {
                    edit = Some(Box::new(move |a| {
                        if a.points.len() > 1 {
                            a.points.remove(idx);
                        }
                    }));
                }
                ui.separator();
                for (m, name) in AUTO_MODES {
                    if ui.selectable_label(mode == m, name).clicked() {
                        edit = Some(Box::new(move |a| a.points[idx].mode = m));
                    }
                }
                if let Some(f) = edit {
                    if let Some(a) = self.song.channels[chan].auto.as_mut() {
                        f(std::sync::Arc::make_mut(a));
                        self.dirty = true;
                    }
                    close = true;
                }
            });
        });
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().map(|p| !area.response.rect.contains(p)).unwrap_or(true));
        if close || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.playlist.auto_menu = None;
        }
    }

    /// What a click on an empty spot of a track puts down: the picked clip source, or on a track locked
    /// to its content the clip that already lies on it.
    fn brush_on(&self, track: usize, pat: usize) -> (ClipSrc, u32, u32) {
        if self.song.tracks.get(track).map(|t| t.lock_content).unwrap_or(false) {
            if let Some(c) = self.song.clips.iter().filter(|c| c.track as usize == track).min_by_key(|c| c.pos) {
                return (c.src, c.len, c.offset);
            }
        }
        self.playlist.brush.unwrap_or((ClipSrc::Pattern(pat), self.song.pattern_length(pat), 0))
    }

    /// The menu of a playlist track: its name and how it behaves in performance mode.
    /// The menu behind a clip's top left corner.
    /// Stretch the sound of an audio clip over `beats` and give the clip that length.
    fn fit_clip(&mut self, clip: usize, ch: usize, beats: f32) {
        let f = &mut self.song.channels[ch].smp;
        f.stretch_beats = beats;
        f.stretch_mul = 1.0;
        f.keep_pitch = true;
        let ticks = (beats * self.song.ppq as f32).round().max(1.0) as u32;
        for (n, c) in self.song.clips.iter_mut().enumerate() {
            if c.src == ClipSrc::Audio(ch) && (n == clip || c.offset == 0) {
                c.len = ticks;
            }
        }
        self.dirty = true;
        self.handle.publish(self.song.clone());
    }

    /// Open the sound of a channel in a Wave Recorder on its mixer track.
    fn edit_clip_sample(&mut self, ch: usize) {
        let Some(sm) = self.song.channels[ch].raw.clone().or_else(|| self.song.channels[ch].sample.clone()) else { return };
        let ins = self.song.channels[ch].insert.min(self.song.inserts.len().saturating_sub(1));
        let name = self.song.channels[ch].name.clone();
        self.edit_sound_on(ins, &sm, &name);
    }

    /// Open a sound in the Wave Recorder of a mixer track, putting one in its first free slot if it has none.
    pub(crate) fn edit_sound_on(&mut self, ins: usize, sm: &cereal_engine::sample::Sample, name: &str) {
        if let Some(slot) = self.audio_editor_on(ins) {
            self.wave_eds.entry((ins, slot)).or_default().take(sm, name);
        }
    }

    /// Show the audio editor of a mixer track, putting one in its first free slot when it has none. The slot.
    pub(crate) fn audio_editor_on(&mut self, ins: usize) -> Option<u16> {
        let have = self.song.inserts.get(ins).and_then(|i| i.slots.iter().find(|s| s.plugin.eq_ignore_ascii_case("Edison")).map(|s| s.index));
        let free = || (0..10u16).find(|n| !self.song.inserts[ins].slots.iter().any(|s| s.index == *n));
        let slot = have.or_else(free);
        match slot {
            Some(slot) => {
                if have.is_none() {
                    self.set_effect(ins, slot, cereal_engine::fx::Kind::Thru, "Edison");
                    self.dirty = true;
                    self.handle.publish(self.song.clone());
                }
                self.wave_eds.entry((ins, slot)).or_default();
                self.fx_open = Some((ins, slot));
            }
            None => self.status = "No free effect slot on this mixer track".into(),
        }
        slot
    }

    /// Colour the pattern or channel a clip plays.
    fn clip_color(&mut self, src: ClipSrc, c: [u8; 3]) {
        match src {
            ClipSrc::Pattern(p) => {
                self.song.ensure_pattern(p);
                self.song.patterns[p].color = Some(c);
            }
            ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => {
                if let Some(ch) = self.song.channels.get_mut(ch) {
                    ch.color = c;
                }
            }
        }
        self.dirty = true;
    }

    fn clip_menu(&mut self, ctx: &egui::Context) {
        let Some((i, at)) = self.playlist.clip_menu else { return };
        let Some(clip) = self.song.clips.get(i).cloned() else {
            self.playlist.clip_menu = None;
            return;
        };
        let audio = self.clip_sample_chan(i);
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("pl_clip_menu")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(240.0);
                let v = ui.visuals_mut();
                v.button_frame = false;
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                crate::menus::sect(ui, match clip.src {
                    ClipSrc::Pattern(_) => "Pattern clip",
                    ClipSrc::Audio(_) => "Audio clip",
                    ClipSrc::Automation(_) => "Automation clip",
                });
                if ui.button("Preview").hint("Hear this clip").clicked() {
                    match (audio, clip.src) {
                        (Some(ch), _) => {
                            if let Some(sm) = self.song.channels[ch].sample.clone() {
                                self.handle.send(cereal_engine::Cmd::Preview(sm));
                            }
                        }
                        (None, ClipSrc::Pattern(p)) => {
                            let tr = self.handle.transport.clone();
                            tr.set_pattern(p);
                            tr.set_song_mode(false);
                            if !tr.playing() {
                                self.play();
                            }
                        }
                        _ => {}
                    }
                    close = true;
                }
                ui.separator();
                if crate::menus::tick(ui, clip.muted, "Muted", "").clicked() {
                    self.song.clips[i].muted = !clip.muted;
                    self.dirty = true;
                    close = true;
                }
                ui.separator();
                if ui.button("Rename and color...").clicked() {
                    match clip.src {
                        ClipSrc::Pattern(p) => {
                            self.song.ensure_pattern(p);
                            self.rename_pattern = Some((p, self.song.patterns[p].name.clone()));
                        }
                        ClipSrc::Audio(c) | ClipSrc::Automation(c) => {
                            if let Some(ch) = self.song.channels.get(c) {
                                self.rack.rename = Some((c, ch.name.clone()));
                                self.show_rack = true;
                            }
                        }
                    }
                    close = true;
                }
                ui.menu_button("Change color...", |ui| {
                    let mut c = match clip.src {
                        ClipSrc::Pattern(p) => self.song.patterns.get(p).and_then(|p| p.color).unwrap_or([86, 110, 130]),
                        ClipSrc::Audio(c) | ClipSrc::Automation(c) => self.song.channels.get(c).map_or([86, 110, 130], |c| c.color),
                    };
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        self.clip_color(clip.src, c);
                    }
                });
                if ui.button("Random color").clicked() {
                    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
                    let c = std::array::from_fn(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        70 + (seed % 150) as u8
                    });
                    self.clip_color(clip.src, c);
                    close = true;
                }
                ui.separator();
                if let Some(ch) = audio {
                    ui.menu_button("Automate", |ui| {
                        for (name, t) in [("Panning", Target::ChanPan(ch)), ("Volume", Target::ChanVolume(ch)), ("Pitch", Target::ChanPitch(ch))] {
                            if ui.button(name).clicked() {
                                self.make_auto_clip_at(t, Some((clip.pos, clip.len)));
                                close = true;
                            }
                        }
                    });
                }
                match clip.src {
                    ClipSrc::Pattern(now) => {
                        ui.menu_button("Select source pattern", |ui| {
                            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                for p in 0..self.song.patterns.len().max(1) {
                                    if ui.radio(p == now, self.song.pattern_name(p)).clicked() {
                                        self.song.clips[i].src = ClipSrc::Pattern(p);
                                        self.handle.transport.set_pattern(p);
                                        self.dirty = true;
                                        close = true;
                                    }
                                }
                            });
                        });
                        if ui.button("Edit pattern").hint("Open the pattern in the piano roll").clicked() {
                            self.handle.transport.set_pattern(now);
                            self.show_piano = true;
                            self.focus = Win::Piano;
                            close = true;
                        }
                    }
                    ClipSrc::Audio(now) | ClipSrc::Automation(now) => {
                        let is_audio = matches!(clip.src, ClipSrc::Audio(_));
                        ui.menu_button("Select source channel", |ui| {
                            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                for c in 0..self.song.channels.len() {
                                    use cereal_engine::flp::ChannelKind as K;
                                    let fits = if is_audio { matches!(self.song.channels[c].kind, K::AudioClip) } else { matches!(self.song.channels[c].kind, K::AutomationClip) };
                                    if fits && ui.radio(c == now, self.song.channels[c].name.clone()).clicked() {
                                        self.song.clips[i].src = if is_audio { ClipSrc::Audio(c) } else { ClipSrc::Automation(c) };
                                        self.dirty = true;
                                        close = true;
                                    }
                                }
                            });
                        });
                        let label = if is_audio { "Edit sample" } else { "Channel settings" };
                        if ui.button(label).clicked() {
                            match audio {
                                Some(ch) if is_audio => self.edit_clip_sample(ch),
                                _ => {
                                    self.sel_chan = now;
                                    self.rack.sel.clear();
                                    self.rack.sel.insert(now);
                                    self.show_chanset = true;
                                }
                            }
                            close = true;
                        }
                    }
                }
                let shared = self.song.clips.iter().filter(|c| c.src == clip.src).count() > 1;
                if ui.add_enabled(shared, egui::Button::new("Make unique")).hint("Give this clip a copy of its own to change").clicked() {
                    match clip.src {
                        ClipSrc::Pattern(p) => {
                            let n = self.song.clone_pattern(p);
                            self.song.clips[i].src = ClipSrc::Pattern(n);
                            self.handle.transport.set_pattern(n);
                        }
                        ClipSrc::Audio(c) | ClipSrc::Automation(c) => {
                            if let Some(copy) = self.song.channels.get(c).cloned() {
                                self.song.channels.push(copy);
                                let n = self.song.channels.len() - 1;
                                self.song.clips[i].src = if matches!(clip.src, ClipSrc::Audio(_)) { ClipSrc::Audio(n) } else { ClipSrc::Automation(n) };
                            }
                        }
                    }
                    self.dirty = true;
                    close = true;
                }
                if ui.button("Select all similar clips").clicked() {
                    self.playlist.sel = (0..self.song.clips.len()).filter(|&k| self.song.clips[k].src == clip.src).collect();
                    close = true;
                }
                if ui.button("Delete").clicked() {
                    self.song.clips.remove(i);
                    self.playlist.sel.clear();
                    self.dirty = true;
                    close = true;
                }
                if let ClipSrc::Pattern(p) = clip.src {
                    ui.separator();
                    ui.menu_button("Use current time signature from", |ui| {
                        if ui.button("Playlist to pattern").hint("The pattern takes the time signature the playlist has here").clicked() {
                            let sig = self.song.sig_at(clip.pos).2;
                            self.song.ensure_pattern(p);
                            let ms = &mut self.song.patterns[p].markers;
                            ms.retain(|m| !(m.kind == 8 && m.pos == 0));
                            ms.insert(0, cereal_engine::song::TimeMarker { pos: 0, name: String::new(), kind: 8, sig });
                            self.dirty = true;
                            close = true;
                        }
                        if ui.button("Pattern to playlist").hint("The playlist takes the pattern's time signature from here on").clicked() {
                            let own = self.song.patterns.get(p).and_then(|p| p.markers.iter().find(|m| m.kind == 8 && m.pos == 0)).map(|m| m.sig);
                            let sig = own.unwrap_or(self.song.time_sig);
                            self.song.markers.retain(|m| !(m.kind == 8 && m.pos == clip.pos));
                            self.song.markers.push(cereal_engine::song::TimeMarker { pos: clip.pos, name: String::new(), kind: 8, sig });
                            self.song.markers.sort_by_key(|m| m.pos);
                            self.dirty = true;
                            close = true;
                        }
                    });
                }
                if let Some(ch) = audio {
                    crate::menus::sect(ui, "Tools");
                    if ui.button("Fit to tempo").hint("Stretch the sound to the nearest whole number of beats at the tempo of the song").clicked() {
                        if let Some(sm) = self.song.channels[ch].sample.clone() {
                            let beats = (sm.seconds() * self.song.tempo / 60.0).round().max(1.0);
                            self.fit_clip(i, ch, beats);
                            self.status = format!("Stretched to {beats:.0} beats");
                        }
                        close = true;
                    }
                    if ui.button("Detect tempo").hint("Find the tempo of the sound and stretch it to keep time with the song").clicked() {
                        let sm = self.song.channels[ch].raw.clone().or_else(|| self.song.channels[ch].sample.clone());
                        match sm.as_ref().and_then(|sm| cereal_engine::waveed::tempo(&sm.frames, sm.rate).map(|t| (t, sm.seconds()))) {
                            Some((bpm, secs)) => {
                                // half and double time sound alike to the ear: take the reading nearest the song
                                let bpm = [bpm * 0.5, bpm, bpm * 2.0].into_iter().min_by(|a, b| (a / self.song.tempo).ln().abs().total_cmp(&(b / self.song.tempo).ln().abs())).unwrap_or(bpm);
                                // a loop is cut on the beat far more often than not
                                let own = secs * bpm / 60.0;
                                let beats = if (own - own.round()).abs() < 0.12 { own.round() } else { own };
                                self.fit_clip(i, ch, beats.max(0.25));
                                self.status = format!("The sound runs at {bpm:.1} beats a minute; stretched to the song");
                            }
                            None => self.status = "No steady tempo found in this sound".into(),
                        }
                        close = true;
                    }
                }
                if let Some(ch) = audio {
                    if ui.button("Extract stems from sample").hint("Split the sound into drums, bass, instruments and vocals on new tracks").clicked() {
                        self.ask_stems(ch);
                        close = true;
                    }
                }
                crate::menus::sect(ui, "Region");
                if let Some(ch) = audio {
                    if self.regions.as_ref().is_none_or(|r| r.0 != ch || r.1 != self.song.channels[ch].sample_path) {
                        let c = &self.song.channels[ch];
                        let frames = c.sample.as_ref().map_or(0, |s| s.frames.len());
                        let dir = self.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
                        let found = self.lib.resolve(&c.sample_path, dir.as_deref()).and_then(|f| std::fs::read(f).ok()).map(|b| cereal_engine::slicer::regions(&b, frames)).unwrap_or_default();
                        self.regions = Some((ch, c.sample_path.clone(), found));
                    }
                    let list = self.regions.as_ref().map(|r| r.2.clone()).unwrap_or_default();
                    ui.add_enabled_ui(true, |ui| {
                        ui.menu_button("Select region", |ui| {
                            let Some(sm) = self.song.channels[ch].sample.clone() else { return };
                            let per_tick = sm.rate as f64 * 60.0 / (self.song.tempo as f64 * self.song.ppq as f64);
                            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                for r in &list {
                                    if ui.button(&r.name).hint("Play only this part of the sound").clicked() {
                                        if let Some(c) = self.song.clips.get_mut(i) {
                                            c.offset = (r.start as f64 / per_tick).round() as u32;
                                            c.len = (((r.end - r.start) as f64 / per_tick).round() as u32).max(1);
                                            self.dirty = true;
                                        }
                                        close = true;
                                    }
                                }
                                ui.separator();
                                if ui.button("Full").clicked() {
                                    if let Some(c) = self.song.clips.get_mut(i) {
                                        c.offset = 0;
                                        c.len = ((sm.frames.len() as f64 / per_tick).round() as u32).max(1);
                                        self.dirty = true;
                                    }
                                    close = true;
                                }
                            });
                        });
                    });
                }
                if audio.is_none() {
                    ui.menu_button("Select region", |ui| {
                        if ui.radio(true, "Full").clicked() {
                            close = true;
                        }
                    });
                }
                ui.menu_button("Chop", |ui| {
                    for (name, grid) in [("In bars", self.song.bar()), ("In beats", self.song.beat_ticks()), ("In steps", self.song.step())] {
                        if ui.button(name).clicked() {
                            let g = grid.max(1);
                            let cuts: Vec<u32> = ((clip.pos / g + 1)..).map(|k| k * g).take_while(|&t| t < clip.pos + clip.len).collect();
                            self.pl_chop(i, &cuts);
                            close = true;
                        }
                    }
                    if let Some(ch) = audio {
                        ui.separator();
                        for (name, sense) in [("Dull auto-slicing", 0.25f32), ("Medium auto-slicing", 0.5), ("Sharp auto-slicing", 0.8)] {
                            if ui.button(name).hint("Cut where the sound has its hits").clicked() {
                                if let Some(sm) = self.song.channels[ch].sample.clone() {
                                    // an unstretched clip: its ticks run at the song's tempo over the sound's own rate
                                    let per_tick = sm.rate as f64 * 60.0 / (self.song.tempo as f64 * self.song.ppq as f64);
                                    let cuts: Vec<u32> = cereal_engine::waveed::onsets(&sm.frames, 0, sm.frames.len(), sm.rate, sense)
                                        .into_iter()
                                        .map(|f| f as f64 / per_tick)
                                        .filter(|&t| t > clip.offset as f64 + 1.0 && t < (clip.offset + clip.len) as f64 - 1.0)
                                        .map(|t| clip.pos + (t - clip.offset as f64).round() as u32)
                                        .collect();
                                    self.pl_chop(i, &cuts);
                                }
                                close = true;
                            }
                        }
                    }
                });
                });
            });
        });
        // the sub-menus are layers of their own in front: a press on one of them is not a press outside
        let outside = ctx.input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten().map(|p| !area.response.rect.contains(p)))
            .map(|out| out && ctx.pointer_interact_pos().and_then(|p| ctx.layer_id_at(p)).is_none_or(|l| l.order != egui::Order::Foreground))
            .unwrap_or(false);
        if close || (outside && !ctx.input(|i| i.pointer.primary_down() && i.pointer.press_origin() == Some(at))) || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.playlist.clip_menu = None;
        }
    }

    fn track_menu(&mut self, ctx: &egui::Context) {
        let Some((tr, at)) = self.playlist.track_menu else { return };
        if tr >= self.song.tracks.len() {
            self.playlist.track_menu = None;
            return;
        }
        let mut close = false;
        let mut changed = false;
        let mut act: Option<u8> = None;
        let area = egui::Area::new(egui::Id::new("pl_track_menu")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                use crate::menus::{mi, tick};
                ui.set_width(230.0);
                let v = ui.visuals_mut();
                v.button_frame = false;
                for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                    w.bg_stroke = Stroke::NONE;
                }
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                let t = &mut self.song.tracks[tr];
                ui.menu_button("Rename, color and icon...", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        let r = ui.add(egui::TextEdit::singleline(&mut t.name).hint_text(format!("Track {}", tr + 1)).desired_width(180.0));
                        changed |= r.changed();
                        close |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        changed |= ui.color_edit_button_srgb(&mut t.color).changed();
                    });
                });
                for (k, label, hint) in [(15u8, "Random color", ""), (0, "Auto name", "Name the track after the first clip on it"), (1, "Auto color", "Take the colour of the first clip on it"), (18, "Auto name clips", "The patterns and channels of the clips on the track take the track's name"), (16, "Reset", "Name and colour back to how a new track has them")] {
                    let r = mi(ui, label, "");
                    if (if hint.is_empty() { r } else { r.hint(hint) }).clicked() {
                        act = Some(k);
                    }
                }
                ui.separator();
                ui.menu_button("Performance settings", |ui| {
                    let row = |ui: &mut Ui, title: &str, names: &[&str], v: &mut u8| {
                        let mut changed = false;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(title).small().color(TEXT_DIM));
                            for (i, n) in names.iter().enumerate() {
                                if ui.selectable_label(*v as usize == i, RichText::new(*n).small()).clicked() {
                                    *v = i as u8;
                                    changed = true;
                                }
                            }
                        });
                        changed
                    };
                    ui.set_max_width(380.0);
                    changed |= row(ui, "Press", &PERF_PRESS, &mut t.perf.press);
                    changed |= row(ui, "Motion", &PERF_MOTION, &mut t.perf.motion);
                    changed |= row(ui, "Trigger sync", &PERF_SYNC, &mut t.perf.sync);
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut t.perf.queue, "Queue").hint("A new clip waits for the playing one to end").changed();
                        changed |= ui.checkbox(&mut t.perf.tolerant, "Tolerant").hint("A press that is a little late starts at once, inside the clip").changed();
                    });
                    changed |= row(ui, "Position sync", &PERF_SYNC, &mut t.perf.pos_sync);
                });
                ui.separator();
                ui.menu_button("Size", |ui| {
                    for pc in [25u32, 50, 75, 100, 150, 200, 300, 400] {
                        if ui.radio(((t.size * 100.0).round() as u32) == pc, format!("{pc}%")).clicked() {
                            t.size = pc as f32 / 100.0;
                            changed = true;
                            close = true;
                        }
                    }
                    ui.separator();
                    if ui.button("Resize all tracks").hint("Give every track this one's height").clicked() {
                        act = Some(9);
                    }
                });
                ui.separator();
                let grouped = t.grouped;
                if ui.add_enabled(tr > 0 && !grouped, egui::Button::new("Group with above track")).clicked() {
                    t.grouped = true;
                    changed = true;
                    close = true;
                }
                if ui.add_enabled(grouped, egui::Button::new("Ungroup")).clicked() {
                    t.grouped = false;
                    changed = true;
                    close = true;
                }
                if mi(ui, "Auto color group", "").hint("Every track of this group takes the colour of the group's first track").clicked() {
                    act = Some(10);
                }
                ui.separator();
                if tick(ui, t.lock_content, "Lock to content", "").hint("Drawing on this track puts down the clip that already lies on it").clicked() {
                    t.lock_content = !t.lock_content;
                    changed = true;
                    close = true;
                }
                if mi(ui, "Merge clips", "").hint("The pattern clips on this track become one clip of a new pattern").clicked() {
                    act = Some(12);
                }
                if mi(ui, "Consolidate track(s)", "").hint("Render the clips on this track into one audio clip").clicked() {
                    act = Some(11);
                }
                for (k, label) in [(3u8, "Select all clips"), (8, "Mute all clips"), (14, "Unmute all clips"), (2, "Solo")] {
                    if mi(ui, label, "").clicked() {
                        act = Some(k);
                    }
                }
                ui.separator();
                for (k, label, hint) in [(4u8, "Insert one", "An empty track here; the ones below move down"), (13, "Clone", "A copy of the track and its clips just below"), (5, "Delete", "Remove this track and its clips; the ones below move up")] {
                    if mi(ui, label, "").hint(hint).clicked() {
                        act = Some(k);
                    }
                }
                ui.separator();
                if ui.add_enabled(tr > 0, egui::Button::new("Move up")).clicked() {
                    act = Some(6);
                }
                if ui.add_enabled(tr + 1 < self.song.tracks.len(), egui::Button::new("Move down")).clicked() {
                    act = Some(7);
                }
                });
            });
        });
        if let Some(k) = act {
            let n = self.song.tracks.len();
            let first = self.song.clips.iter().filter(|c| c.track as usize == tr).min_by_key(|c| c.pos).map(|c| c.src);
            match k {
                9 => {
                    let size = self.song.tracks[tr].size;
                    self.song.tracks.iter_mut().for_each(|t| t.size = size);
                }
                10 => {
                    let head = (0..=tr).rev().find(|&i| !self.song.tracks[i].grouped).unwrap_or(0);
                    let col = self.song.tracks[head].color;
                    let mut i = head + 1;
                    while i < n && self.song.tracks[i].grouped {
                        self.song.tracks[i].color = col;
                        i += 1;
                    }
                }
                11 => {
                    self.playlist.sel = self.song.clips.iter().enumerate().filter(|(_, c)| c.track as usize == tr).map(|(i, _)| i).collect();
                    self.pl_consolidate(false);
                }
                12 => self.merge_pattern_clips(tr),
                0 => {
                    if let Some(src) = first {
                        self.song.tracks[tr].name = match src {
                            ClipSrc::Pattern(p) => self.song.pattern_name(p),
                            ClipSrc::Audio(c) | ClipSrc::Automation(c) => self.song.channels.get(c).map(|c| c.name.clone()).unwrap_or_default(),
                        };
                    }
                }
                1 => {
                    let col = first.and_then(|src| match src {
                        ClipSrc::Pattern(p) => self.song.patterns.get(p).and_then(|p| p.color),
                        ClipSrc::Audio(c) | ClipSrc::Automation(c) => self.song.channels.get(c).map(|c| c.color),
                    });
                    if let Some(col) = col {
                        self.song.tracks[tr].color = col;
                    }
                }
                2 => {
                    // solo; a second press on the only track left on brings them all back
                    let alone = self.song.tracks.iter().enumerate().all(|(i, t)| t.enabled == (i == tr));
                    for (i, t) in self.song.tracks.iter_mut().enumerate() {
                        t.enabled = alone || i == tr;
                    }
                }
                3 => {
                    self.playlist.sel = self.song.clips.iter().enumerate().filter(|(_, c)| c.track as usize == tr).map(|(i, _)| i).collect();
                }
                8 | 14 => {
                    for c in self.song.clips.iter_mut().filter(|c| c.track as usize == tr) {
                        c.muted = k == 8;
                    }
                }
                13 => {
                    // the last track falls off the end only when it is empty
                    if tr + 1 < n && !self.song.clips.iter().any(|c| c.track as usize == n - 1) {
                        self.song.tracks.pop();
                        let copy = self.song.tracks[tr].clone();
                        self.song.tracks.insert(tr + 1, copy);
                        for c in self.song.clips.iter_mut().filter(|c| c.track as usize > tr) {
                            c.track += 1;
                        }
                        let twins: Vec<Clip> = self.song.clips.iter().filter(|c| c.track as usize == tr).cloned().map(|mut c| {
                            c.track += 1;
                            c
                        }).collect();
                        self.song.clips.extend(twins);
                    }
                }
                15 => {
                    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
                    self.song.tracks[tr].color = std::array::from_fn(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        70 + (seed % 150) as u8
                    });
                }
                16 => {
                    let d = cereal_engine::song::Track::default();
                    self.song.tracks[tr].name = d.name;
                    self.song.tracks[tr].color = d.color;
                }
                18 => {
                    let name = self.song.tracks[tr].name.clone();
                    let srcs: Vec<ClipSrc> = self.song.clips.iter().filter(|c| c.track as usize == tr).map(|c| c.src).collect();
                    for src in srcs.into_iter().filter(|_| !name.is_empty()) {
                        match src {
                            ClipSrc::Pattern(p) => {
                                self.song.ensure_pattern(p);
                                self.song.patterns[p].name = name.clone();
                            }
                            ClipSrc::Audio(c) | ClipSrc::Automation(c) => {
                                if let Some(c) = self.song.channels.get_mut(c) {
                                    c.name = name.clone();
                                }
                            }
                        }
                    }
                }
                4 => {
                    // the last track falls off the end only when it is empty
                    if !self.song.clips.iter().any(|c| c.track as usize == n - 1) {
                        self.song.tracks.pop();
                        self.song.tracks.insert(tr, cereal_engine::song::Track::default());
                        for c in self.song.clips.iter_mut().filter(|c| c.track as usize >= tr) {
                            c.track += 1;
                        }
                    }
                }
                5 => {
                    self.song.clips.retain(|c| c.track as usize != tr);
                    for c in self.song.clips.iter_mut().filter(|c| c.track as usize > tr) {
                        c.track -= 1;
                    }
                    self.song.tracks.remove(tr);
                    self.song.tracks.push(cereal_engine::song::Track::default());
                    self.playlist.sel.clear();
                }
                6 if tr > 0 => {
                    self.song.swap_tracks(tr, tr - 1);
                    self.playlist.track_menu = None;
                }
                7 if tr + 1 < n => {
                    self.song.swap_tracks(tr, tr + 1);
                    self.playlist.track_menu = None;
                }
                _ => {}
            }
            changed = true;
            close = true;
        }
        self.dirty |= changed;
        // a press on one of its own submenus is not a press outside
        let outside = ctx.input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten()).is_some_and(|p| !area.response.rect.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l.order < egui::Order::Foreground));
        if close || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.playlist.track_menu = None;
        }
    }

    /// The pattern clips of a track become one clip of a new pattern that holds what they played.
    fn merge_pattern_clips(&mut self, tr: usize) {
        let mine: Vec<usize> = self.song.clips.iter().enumerate().filter(|(_, c)| c.track as usize == tr && matches!(c.src, ClipSrc::Pattern(_))).map(|(i, _)| i).collect();
        if mine.len() < 2 {
            self.status = "This track needs two pattern clips or more to merge".into();
            return;
        }
        let start = mine.iter().map(|&i| self.song.clips[i].pos).min().unwrap_or(0);
        let end = mine.iter().map(|&i| self.song.clips[i].pos + self.song.clips[i].len).max().unwrap_or(start);
        let mut pat = cereal_engine::song::Pattern { name: "Merged".into(), ..Default::default() };
        for &i in &mine {
            let c = &self.song.clips[i];
            let ClipSrc::Pattern(pi) = c.src else { continue };
            let Some(src) = self.song.patterns.get(pi) else { continue };
            if c.muted {
                continue;
            }
            let (from, to) = (c.offset, c.offset + c.len);
            for nt in src.notes.iter().filter(|nt| nt.pos >= from && nt.pos < to) {
                let mut nt = nt.clone();
                // a note runs to the end of its clip at most
                if nt.len > 0 {
                    nt.len = nt.len.min(to - nt.pos).max(1);
                }
                nt.pos = nt.pos - from + c.pos - start;
                pat.notes.push(nt);
            }
            for e in src.events.iter().filter(|e| e.tick >= from && e.tick < to) {
                pat.events.push(cereal_engine::song::PatEvent { tick: e.tick - from + c.pos - start, ..*e });
            }
        }
        pat.sort();
        self.song.patterns.push(pat);
        let pi = self.song.patterns.len() - 1;
        let group = 0;
        self.song.clips.retain(|c| !(c.track as usize == tr && matches!(c.src, ClipSrc::Pattern(_))));
        self.song.clips.push(Clip { pos: start, len: end - start, track: tr as u16, src: ClipSrc::Pattern(pi), offset: 0, muted: false, group, fine: 0.0, fx: Default::default() });
        self.song.clips.sort_by_key(|c| c.pos);
        self.playlist.sel.clear();
        self.dirty = true;
        self.status = format!("Merged {} clips into pattern {}", mine.len(), pi + 1);
    }

    /// The scene a place of the performance zone belongs to: from the marker at or before it to the next one.
    fn perf_region(&self, at: u32, zone: u32) -> (u32, u32) {
        let lo = self.song.markers.iter().filter(|m| m.pos <= at).map(|m| m.pos).max().unwrap_or(0);
        let hi = self.song.markers.iter().filter(|m| m.pos > at).map(|m| m.pos).min().unwrap_or(zone).min(zone);
        (lo, hi)
    }

    /// Play a scene: the first clip of every track between two places of the zone.
    fn perf_scene(&mut self, lo: u32, hi: u32, m: Modifiers) {
        let mut first: std::collections::BTreeMap<u16, u32> = Default::default();
        for c in self.song.clips.iter().filter(|c| c.pos >= lo && c.pos < hi) {
            let e = first.entry(c.track).or_insert(c.pos);
            *e = (*e).min(c.pos);
        }
        for (track, pos) in first {
            self.handle.send(Cmd::Perf { track, clip: Some(pos), now: m.alt, queue: m.shift });
            self.playlist.perf_press.push(track);
        }
    }

    /// A press on a clip of the performance zone: the clip plays; with Ctrl its whole scene or group does.
    fn perf_click(&mut self, i: usize, m: Modifiers) {
        let Some(zone) = self.song.perf_start() else { return };
        let c = self.song.clips[i].clone();
        if !self.handle.transport.song_mode() {
            self.handle.transport.set_song_mode(true);
        }
        if m.command && c.group != 0 {
            let mine: Vec<(u16, u32)> = self.song.clips.iter().filter(|k| k.group == c.group && k.pos < zone).map(|k| (k.track, k.pos)).collect();
            for (track, pos) in mine {
                self.handle.send(Cmd::Perf { track, clip: Some(pos), now: m.alt, queue: m.shift });
                self.playlist.perf_press.push(track);
            }
        } else if m.command {
            let (lo, hi) = self.perf_region(c.pos, zone);
            self.perf_scene(lo, hi, m);
        } else {
            self.handle.send(Cmd::Perf { track: c.track, clip: Some(c.pos), now: m.alt, queue: m.shift });
            self.playlist.perf_press.push(c.track);
        }
    }

    /// Switch performance mode. Turning it on in a song that has no start marker lays the zone out first.
    pub(crate) fn toggle_performance(&mut self) {
        self.song.performance = !self.song.performance;
        if self.song.performance {
            if !self.song.markers.iter().any(|m| m.kind == 5 && m.pos > 0) {
                self.prepare_performance();
            }
            self.handle.transport.set_song_mode(true);
        }
        self.dirty = true;
    }

    /// Lay a song out for performing: sixteen bars of zone with a marker on each, then the start marker.
    /// Clips already in the playlist stay where they are and become the zone's clips.
    pub(crate) fn prepare_performance(&mut self) {
        let bar = self.song.bar().max(1);
        let used = self.song.clips.iter().map(|c| c.pos + c.len).max().unwrap_or(0).div_ceil(bar);
        let bars = used.max(16);
        self.song.markers.retain(|m| m.kind != 5);
        for b in 0..bars {
            if !self.song.markers.iter().any(|m| m.pos == b * bar) {
                self.song.markers.push(cereal_engine::song::TimeMarker { pos: b * bar, name: format!("{}", b + 1), kind: 0, sig: (4, 4) });
            }
        }
        self.song.markers.push(cereal_engine::song::TimeMarker { pos: bars * bar, name: "Start".into(), kind: 5, sig: (4, 4) });
        self.song.markers.sort_by_key(|m| m.pos);
        self.song.performance = true;
        self.handle.transport.set_song_mode(true);
        self.dirty = true;
    }

    /// Write the clips played by hand while recording into the playlist, where they sounded.
    pub(crate) fn write_performance(&mut self, segs: Vec<PerfSeg>) {
        for g in segs {
            let Some(c) = self.song.clips.iter().find(|c| c.track == g.track && c.pos == g.clip).cloned() else { continue };
            let (from, to) = (g.from.round().max(0.0) as u32, g.to.round().max(0.0) as u32);
            if to <= from {
                continue;
            }
            self.song.clips.push(Clip { pos: from, len: to - from, offset: c.offset + g.offset.round().max(0.0) as u32, group: 0, muted: false, ..c });
            self.dirty = true;
        }
    }

    /// The typing keyboard as a clip grid: five clips on each of the first eight tracks.
    pub(crate) fn perf_typing(key: Key) -> Option<(u16, u8)> {
        const ROWS: [[Key; 5]; 8] = [
            [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5],
            [Key::Q, Key::W, Key::E, Key::R, Key::T],
            [Key::A, Key::S, Key::D, Key::F, Key::G],
            [Key::Z, Key::X, Key::C, Key::V, Key::B],
            [Key::Num6, Key::Num7, Key::Num8, Key::Num9, Key::Num0],
            [Key::Y, Key::U, Key::I, Key::O, Key::P],
            [Key::H, Key::J, Key::K, Key::L, Key::Semicolon],
            [Key::N, Key::M, Key::Comma, Key::Period, Key::Slash],
        ];
        ROWS.iter().enumerate().find_map(|(t, row)| row.iter().position(|k| *k == key).map(|n| (t as u16, n as u8)))
    }

    /// How far along its travel a control stands in the song, 0..=1.
    pub(crate) fn target_travel(&self, t: Target) -> f32 {
        let s = &self.song;
        let v = match t {
            Target::ChanVolume(c) => s.channels.get(c).map(|c| c.volume),
            Target::ChanPan(c) => s.channels.get(c).map(|c| c.pan * 0.5 + 0.5),
            Target::ChanPitch(c) => s.channels.get(c).map(|c| c.pitch_cents / (c.smp.bend_range.max(1) as f32 * 200.0) + 0.5),
            Target::InsertVolume(i) => s.inserts.get(i).map(|i| i.volume / 1.25),
            Target::InsertPan(i) => s.inserts.get(i).map(|i| i.pan * 0.5 + 0.5),
            Target::Tempo => Some((s.tempo - 60.0) / 120.0),
            Target::SlotMix(i, k) => s.inserts.get(i).and_then(|i| i.slots.iter().find(|x| x.index == k)).map(|x| x.mix),
            Target::SlotOn(i, k) => s.inserts.get(i).and_then(|i| i.slots.iter().find(|x| x.index == k)).map(|x| x.enabled as u8 as f32),
            Target::SlotParam(i, k, p) => s.inserts.get(i).and_then(|i| i.slots.iter().find(|x| x.index == k)).and_then(|x| x.fx.as_ref()).and_then(|f| {
                if let Some(h) = &f.ext {
                    return h.0.travel(p as usize);
                }
                let q = cereal_engine::fx::def(f.kind).params.get(p as usize)?;
                Some((f.p.get(p as usize)? - q.min as f32) / (q.max - q.min).max(1) as f32)
            }),
            Target::ChanParam(c, p) if s.channels.get(c).is_some_and(|c| c.ext.is_some()) => s.channels[c].ext.as_ref().and_then(|h| h.0.travel(p as usize)),
            Target::ChanParam(c, p) => s.channels.get(c).and_then(|c| c.gen.as_ref()).and_then(|g| {
                let q = cereal_engine::synth::def(g.kind).params.get(p as usize)?;
                Some((g.p.get(p as usize)? - q.min as f32) / (q.max - q.min).max(1) as f32)
            }),
            Target::Other { .. } => None,
        };
        v.unwrap_or(0.5).clamp(0.0, 1.0)
    }

    /// Make an automation clip for a control: a flat line at where the control stands now, as long as the song,
    /// on the first track with room for it.
    pub(crate) fn make_auto_clip(&mut self, t: Target) {
        self.make_auto_clip_at(t, None);
    }

    /// The same over one stretch of the song (a clip's own span), or the whole song.
    pub(crate) fn make_auto_clip_at(&mut self, t: Target, span: Option<(u32, u32)>) {
        let v = self.target_travel(t);
        let (pos, len) = span.unwrap_or((0, self.song.length().max(self.song.bar() * 4)));
        let pt = |tick: f64| cereal_engine::song::AutoPt { tick, value: v, tension: 0.0, mode: 0 };
        self.add_auto_clip(t, pos, len, vec![pt(0.0), pt(len as f64)]);
    }

    /// A new automation clip with these points, on the first track with room for it.
    pub(crate) fn add_auto_clip(&mut self, t: Target, pos: u32, len: u32, points: Vec<cereal_engine::song::AutoPt>) {
        let mut c = cereal_engine::song::Chan::automation();
        c.name = self.song.target_name(t);
        c.auto = Some(std::sync::Arc::new(cereal_engine::song::Automation { points, targets: vec![t] }));
        self.song.channels.push(c);
        let ci = self.song.channels.len() - 1;
        let ntracks = self.song.tracks.len().max(1) as u16;
        let track = (0..ntracks).find(|&tr| !self.song.clips.iter().any(|c| c.track == tr)).unwrap_or(0);
        self.song.clips.push(Clip { pos, len, track, src: ClipSrc::Automation(ci), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
        self.show_playlist = true;
        self.dirty = true;
        self.status = format!("Automation clip made for {}", self.song.target_name(t));
    }

    fn clip_look(&self, c: &Clip) -> (String, Color32) {
        match c.src {
            ClipSrc::Pattern(p) => (self.song.pattern_name(p), self.song.patterns.get(p).and_then(|p| p.color).map(rgb).unwrap_or(Color32::from_rgb(86, 110, 130))),
            ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => match self.song.channels.get(ch) {
                Some(c) => (c.name.clone(), rgb(c.color)),
                None => ("?".into(), STEP_A),
            },
        }
    }

    /// Every pattern, audio clip and automation clip of the project; a click makes it the thing to draw. True when one was picked.
    pub(crate) fn project_list(&mut self, ui: &mut Ui) -> bool {
        let pat = self.handle.transport.pattern();
        let ppq = self.song.ppq as f32;
        let bar = self.song.bar();
        let mut picked = false;
            let brush = self.playlist.brush.map(|b| b.0).unwrap_or(ClipSrc::Pattern(pat));
            ui.label(RichText::new("Patterns").small().color(TEXT_DIM));
            let mut order: Vec<usize> = (0..self.song.patterns.len()).collect();
            if self.settings.pl_opts & plo::PICKER_SORT != 0 {
                order.sort_by_key(|&p| (self.song.patterns[p].name.is_empty(), self.song.patterns[p].name.to_lowercase(), p));
            }
            for p in order {
                if self.settings.pl_opts & plo::PICKER_EMPTY == 0 && p != pat && self.song.patterns[p].notes.is_empty() && !self.song.clips.iter().any(|c| c.src == ClipSrc::Pattern(p)) {
                    continue;
                }
                let name = if self.song.patterns[p].name.is_empty() { format!("Pattern {}", p + 1) } else { self.song.patterns[p].name.clone() };
                let name = if self.playlist.marked.contains(&ClipSrc::Pattern(p)) { RichText::new(name).color(Color32::from_rgb(236, 150, 60)) } else { RichText::new(name) };
                if ui.selectable_label(brush == ClipSrc::Pattern(p), name).clicked() {
                    self.handle.transport.set_pattern(p);
                    self.playlist.brush = None;
                    self.playlist.brush_pat = p;
                    if self.settings.pl_opts & plo::PICKER_LINKED != 0 {
                        // the first channel the pattern plays becomes the selected one
                        if let Some(c) = self.song.patterns[p].notes.iter().map(|n| n.chan as usize).min() {
                            self.sel_chan = c;
                        }
                    }
                picked = true;
                }
            }
            for (title, audio) in [("Audio", true), ("Automation", false)] {
                let list: Vec<usize> = (0..self.song.channels.len()).filter(|&c| if audio { self.song.channels[c].kind == cereal_engine::flp::ChannelKind::AudioClip } else { self.song.channels[c].auto.is_some() }).collect();
                if list.is_empty() {
                    continue;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(title).small().color(TEXT_DIM));
                for c in list {
                    let src = if audio { ClipSrc::Audio(c) } else { ClipSrc::Automation(c) };
                    let name = RichText::new(self.song.channels[c].name.clone());
                    let name = if self.playlist.marked.contains(&src) { name.color(Color32::from_rgb(236, 150, 60)) } else { name };
                    if ui.selectable_label(brush == src, name).clicked() {
                        // as long as a clip of it that is already there, else the sound's own length, else four bars
                        let ch = &self.song.channels[c];
                        let own = ch.sample.as_ref().filter(|_| audio).map(|s| {
                            let beats = if ch.smp.stretch_beats > 0.0 { ch.smp.stretch_beats * ch.smp.stretch_mul } else { s.seconds() * self.song.tempo / 60.0 };
                            (beats * ppq).ceil() as u32
                        });
                        let len = self.song.clips.iter().find(|k| k.src == src).map(|k| k.len).or(own).unwrap_or(bar * 4).max(1);
                        self.playlist.brush = Some((src, len, 0));
                        if self.settings.pl_opts & plo::PICKER_LINKED != 0 {
                            self.sel_chan = c;
                        }
                    picked = true;
                    }
                }
            }
        picked
    }

    pub(crate) fn playlist_ui(&mut self, ui: &mut Ui) {
        self.pl_whole_groups();
        let pat = self.handle.transport.pattern();
        let ppq = self.song.ppq as f32;
        let bar = self.song.bar();
        let beat = self.song.ppq.max(1);
        if self.playlist.brush_pat != pat {
            // picking another pattern makes it the thing to draw
            self.playlist.brush_pat = pat;
            self.playlist.brush = None;
        }
        if self.playlist.sel.iter().any(|&i| i >= self.song.clips.len()) {
            self.playlist.sel.clear();
            self.playlist.drag = None;
        }
        self.playlist_header(ui, pat);
        if self.playlist.picker {
            let side = if self.settings.pl_opts & plo::PICKER_RIGHT != 0 { egui::panel::Side::Right } else { egui::panel::Side::Left };
            egui::SidePanel::new(side, "pl_picker").resizable(true).default_width(150.0).frame(egui::Frame::NONE.inner_margin(4.0)).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("pl_picker_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    self.project_list(ui);
                });
            });
        }
        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(Vec2::new(200.0, 120.0)), Sense::click_and_drag());
        let grid = Rect::from_min_max(rect.min + Vec2::new(HEAD_W, RULER_H), rect.max);
        let heads = Rect::from_min_max(Pos2::new(rect.left(), grid.top()), Pos2::new(grid.left(), rect.bottom()));
        let ruler = Rect::from_min_max(Pos2::new(grid.left(), rect.top()), Pos2::new(rect.right(), grid.top()));
        let th = self.playlist.track_h;
        let ntracks = self.song.tracks.len();
        // where each track starts, counted from the first: tracks may each have their own height
        let mut tops: Vec<f32> = Vec::with_capacity(ntracks + 1);
        tops.push(0.0);
        let mut folded = false;
        for (i, t) in self.song.tracks.iter().enumerate() {
            folded = if t.grouped { folded } else { self.playlist.collapsed.contains(&i) };
            let h = if folded && t.grouped { if self.settings.pl_opts & plo::HIDE_COLLAPSED != 0 { 0.0 } else { 2.0 } } else { (t.size.clamp(0.01, 8.0) * th).max(6.0).round() };
            tops.push(tops[tops.len() - 1] + h);
        }
        let opts = self.settings.pl_opts;
        // a move of the play position that was waiting for its grid line
        if let Some((when, to)) = self.playlist.jump {
            if !self.handle.transport.playing() || self.handle.transport.tick() >= when as f64 {
                self.handle.transport.seek(to);
                self.playlist.jump = None;
            }
        }
        let all_h = tops[ntracks];
        let row_at = |off: f32| tops.partition_point(|&t| t <= off).saturating_sub(1).min(ntracks.saturating_sub(1));
        // performance mode: the clips ahead of the start marker are played by hand
        let zone = self.song.perf_start();

        if resp.hovered() {
            // Shift and the wheel over a track name moves the track; with Alt as well, over a clip, it nudges the clip
            if let Some(pp) = ui.input(|i| i.pointer.hover_pos()) {
                if heads.contains(pp) {
                    let tr = row_at(pp.y - grid.top() + self.playlist.scroll_y);
                    self.wheel_over = Some(crate::keys::WheelMove::Track(tr, false));
                } else if let Some((_, Some(i))) = self.playlist.hover {
                    self.wheel_over = Some(crate::keys::WheelMove::Clip(i, false));
                }
            }
            let (scroll, zoom, mods) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
            if zoom != 1.0 {
                if mods.alt {
                    self.playlist.track_h = (self.playlist.track_h * zoom).clamp(14.0, 120.0);
                } else {
                    let old = self.playlist.zoom;
                    self.playlist.zoom = (old * zoom).clamp(0.005, 4.0);
                    if let Some(pp) = ui.input(|i| i.pointer.hover_pos()) {
                        let off = pp.x - grid.left();
                        self.playlist.scroll_x = (self.playlist.scroll_x + off) * self.playlist.zoom / old - off;
                    }
                }
            } else if mods.shift {
                self.playlist.scroll_x -= scroll.y + scroll.x;
            } else {
                self.playlist.scroll_y -= scroll.y;
                self.playlist.scroll_x -= scroll.x;
            }
            let (mid, delta) = ui.input(|i| (i.pointer.middle_down(), i.pointer.delta()));
            if mid {
                self.playlist.scroll_x -= delta.x;
                self.playlist.scroll_y -= delta.y;
            }
        }
        let px = self.playlist.zoom * 96.0 / ppq;
        if self.auto_scroll && self.handle.transport.playing() && self.handle.transport.song_mode() {
            // auto scrolling: the view turns the page when the play position leaves it
            let x = self.handle.transport.tick() as f32 * px;
            if self.settings.pl_opts & plo::INCR_SCROLL != 0 {
                // gliding: the play position stays in the middle once it gets there
                self.playlist.scroll_x = (x - grid.width() * 0.5).max(0.0);
            } else if x < self.playlist.scroll_x || x > self.playlist.scroll_x + grid.width() {
                self.playlist.scroll_x = x;
            }
        }
        self.playlist.scroll_x = self.playlist.scroll_x.max(0.0);
        self.playlist.scroll_y = self.playlist.scroll_y.clamp(0.0, (all_h - grid.height()).max(0.0));
        let (sx, sy) = (self.playlist.scroll_x, self.playlist.scroll_y);
        let x_of = |tick: f32| grid.left() + tick * px - sx;
        let tick_of = |x: f32| ((x - grid.left() + sx) / px).max(0.0);
        let y_of = |track: usize| grid.top() + tops[track.min(ntracks)] - sy;
        let h_of = |track: usize| tops[(track + 1).min(ntracks)] - tops[track.min(ntracks)];
        let track_of = |y: f32| row_at(y - grid.top() + sy);

        let p = ui.painter_at(rect);
        let (grid_dark, grid_light) = crate::widgets::grid_colours(self.settings.grid);
        p.rect_filled(rect, 0.0, grid_dark);
        let first_track = track_of(grid.top());
        let last_track = track_of(grid.bottom());

        // grid
        let last_tick = tick_of(grid.right()) as u32 + bar;
        let bar_px = bar as f32 * px;
        let every = if bar_px >= 14.0 { 1 } else { (14.0 / bar_px).ceil() as u32 };
        // time signature markers change the length of the bars after them
        let lines = self.song.bar_lines(tick_of(grid.left()) as u32, last_tick);
        for &(t, n, len) in &lines {
            let x = x_of(t as f32);
            if opts & plo::TIME_SEG != 0 && ((n / 4) % 2 == 1) != (opts & plo::INVERT_GRID != 0) {
                p.rect_filled(Rect::from_min_max(Pos2::new(x.max(grid.left()), grid.top()), Pos2::new((x + len as f32 * px).max(grid.left()), grid.bottom())), 0.0, grid_light);
            }
            if n % every == 0 && x >= grid.left() {
                let line = match (opts >> plo::CONTRAST_SHIFT) & 3 {
                    0 => LINE_SOFT.gamma_multiply(0.5),
                    1 => LINE_SOFT,
                    _ => LINE,
                };
                p.line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, line));
            }
        }
        for tr in (first_track..=last_track).filter(|_| opts & plo::TRACK_SEP != 0) {
            let y = y_of(tr);
            p.line_segment([Pos2::new(grid.left(), y), Pos2::new(grid.right(), y)], Stroke::new(1.0_f32, LINE_SOFT));
        }

        if let Some(z) = zone {
            let (xa, xb) = (x_of(0.0).max(grid.left()), x_of(z as f32).min(grid.right()));
            if xb > xa {
                p.rect_filled(Rect::from_min_max(Pos2::new(xa, grid.top()), Pos2::new(xb, grid.bottom())), 0.0, Color32::from_rgb(236, 150, 60).gamma_multiply(0.07));
            }
        }

        // clips
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let now_tick = self.handle.transport.tick() as f32;
        let blink = ui.input(|i| i.time).fract() < 0.5;
        let mut hit: Option<(usize, i8)> = None;
        let mut hit_corner = false;
        // the automation clip whose picture the pointer is over: clip, channel, picture, left edge, offset
        let mut auto_under: Option<(usize, usize, Rect, f32, u32)> = None;
        // the clip of sound whose picture the pointer is over: clip, picture, left and right edge
        let mut fade_under: Option<(usize, Rect, f32, f32)> = None;
        // milliseconds a tick lasts at the song tempo
        let ms_per_tick = 60_000.0 / self.song.tempo.max(1.0) / ppq;
        let gp = ui.painter_at(grid);
        let font = FontId::proportional(10.5);
        for (i, c) in self.song.clips.iter().enumerate() {
            let tr = c.track as usize;
            if tr < first_track || tr > last_track || h_of(tr) < 3.0 {
                continue;
            }
            let r = Rect::from_min_max(Pos2::new(x_of(c.pos as f32), y_of(tr) + 1.0), Pos2::new(x_of((c.pos + c.len) as f32) - 1.0, y_of(tr) + h_of(tr)));
            if r.right() < grid.left() || r.left() > grid.right() {
                continue;
            }
            let (name, col) = self.clip_look(c);
            let enabled = (!c.muted || zone.is_some_and(|z| c.pos < z)) && self.song.tracks.get(tr).map(|t| t.enabled).unwrap_or(true);
            let col = if enabled { col } else { col.gamma_multiply(0.35) };
            if opts & plo::SHADOW != 0 {
                gp.rect_filled(r.translate(Vec2::new(2.0, 2.0)), 2.0, Color32::from_black_alpha(90));
            }
            let behind = [0.0, 0.55, 0.45, 0.35, 0.65, 1.0][((opts >> plo::BEHIND_SHIFT) & 7).min(5) as usize];
            if behind > 0.0 {
                gp.rect_filled(r, 2.0, col.gamma_multiply(behind));
            }
            let title = Rect::from_min_max(r.min, Pos2::new(r.right(), (r.top() + 12.0).min(r.bottom())));
            gp.rect_filled(title, 2.0, col);
            let selected = self.playlist.sel.contains(&i);
            gp.rect_stroke(r, 2.0, Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, if selected { Color32::from_rgb(235, 96, 88) } else { LINE }), egui::StrokeKind::Inside);
            if zone.is_some_and(|z| c.pos < z) {
                // what the track is playing by hand, and what waits for its grid line
                let (live, next) = self.handle.transport.perf_state(tr);
                if let Some((_, from)) = live.filter(|l| l.0 == c.pos && opts & plo::PERF_CLIP != 0) {
                    let part = ((now_tick - from) / c.len.max(1) as f32).clamp(0.0, 1.0);
                    let done = Rect::from_min_max(r.min, Pos2::new(r.left() + r.width() * part, r.bottom()));
                    gp.rect_filled(done, 2.0, Color32::from_white_alpha(46));
                    gp.rect_stroke(r, 2.0, Stroke::new(2.0_f32, GREEN), egui::StrokeKind::Inside);
                    ui.ctx().request_repaint();
                }
                if next == Some(Some(c.pos)) || (next == Some(None) && live.is_some_and(|l| l.0 == c.pos)) {
                    if blink {
                        gp.rect_stroke(r, 2.0, Stroke::new(2.0_f32, Color32::from_rgb(236, 150, 60)), egui::StrokeKind::Inside);
                    }
                    ui.ctx().request_repaint();
                }
            }
            if r.width() > 16.0 {
                // the name slides along a clip that starts off the left of the view
                let lx = if opts & plo::KEEP_LABELS != 0 { title.left().max(grid.left()) } else { title.left() };
                gp.with_clip_rect(r.intersect(grid)).text(Pos2::new(lx + 3.0, title.center().y), Align2::LEFT_CENTER, &name, font.clone(), Color32::from_gray(245));
            }
            // a picture of what is inside
            let body = Rect::from_min_max(Pos2::new(r.left(), if opts & plo::TITLE_CONTENT != 0 { r.top() + 1.0 } else { title.bottom() }), r.max);
            if body.height() > 6.0 && r.width() > 6.0 {
                match c.src {
                    ClipSrc::Pattern(pi) => {
                        if let Some(pt) = self.song.patterns.get(pi) {
                            if !pt.notes.is_empty() {
                                let lo = pt.notes.iter().map(|n| n.key).min().unwrap_or(60) as f32;
                                let hi = pt.notes.iter().map(|n| n.key).max().unwrap_or(60) as f32;
                                let span = (hi - lo).max(1.0);
                                let cp = gp.with_clip_rect(body.intersect(grid));
                                for n in &pt.notes {
                                    if n.pos < c.offset || n.pos >= c.offset + c.len {
                                        continue;
                                    }
                                    let x0 = x_of((c.pos + n.pos - c.offset) as f32);
                                    let x1 = (x0 + (n.len.max(beat / 4) as f32 * px)).max(x0 + 1.5);
                                    let y = body.bottom() - 2.0 - (n.key as f32 - lo) / span * (body.height() - 4.0);
                                    cp.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.5_f32, Color32::from_gray(240)));
                                }
                            }
                        }
                    }
                    ClipSrc::Audio(ch) => {
                        if let Some((chan, smp)) = self.song.channels.get(ch).and_then(|c| c.sample.as_ref().map(|s| (c, s))) {
                            // frames of the recording per tick at the song tempo, stretch and pitch counted in
                            let fpt = cereal_engine::song::clip_ms_per_tick(chan, self.song.tempo, self.song.ppq) / c.fx.speed * smp.rate as f32 / 1000.0;
                            let lead = c.fine * smp.rate as f32 / 1000.0;
                            let cp = gp.with_clip_rect(body.intersect(grid));
                            let x_from = r.left().max(grid.left()) as i32;
                            let x_to = r.right().min(grid.right()) as i32;
                            let mid = body.center().y;
                            let fx = c.fx;
                            let shaped = opts & plo::FADE_PREVIEW != 0 && fx.shaped();
                            let env = |x: f32| {
                                let ms = (x - r.left()) / px * ms_per_tick;
                                let whole = c.len as f32 * ms_per_tick;
                                let a = if ms < fx.fade_in { cereal_engine::song::ClipFx::curve(ms / fx.fade_in, fx.in_tension) } else { 1.0 };
                                let b = if whole - ms < fx.fade_out { cereal_engine::song::ClipFx::curve((whole - ms) / fx.fade_out, fx.out_tension) } else { 1.0 };
                                a * b
                            };
                            for x in x_from..x_to {
                                let t0 = (x as f32 - r.left()) / px + c.offset as f32;
                                let a = (t0 * fpt + lead) as usize;
                                let b = (((t0 + 1.0 / px) * fpt + lead) as usize).min(smp.frames.len());
                                if a >= b {
                                    break;
                                }
                                let stride = ((b - a) / 32).max(1);
                                let peak = smp.frames[a..b].iter().step_by(stride).fold(0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                                let peak = if shaped { (peak * fx.gain * env(x as f32)).min(1.0) } else { peak };
                                let h = (peak * body.height() * 0.5).max(0.5);
                                cp.line_segment([Pos2::new(x as f32, mid - h), Pos2::new(x as f32, mid + h)], Stroke::new(1.0_f32, Color32::from_gray(240)));
                            }
                            if body.height() > 18.0 && r.width() > 24.0 {
                                // the fades as lines, and the handles that set them
                                let hs = fade_handles(&fx, body, r.left(), r.right(), px / ms_per_tick);
                                let top = body.top() + 4.0;
                                let line = Stroke::new(1.0_f32, Color32::from_white_alpha(170));
                                for (on, from, to) in [(fx.fade_in > 0.0, r.left(), hs[0].x), (fx.fade_out > 0.0, r.right(), hs[1].x)] {
                                    if on && (to - from).abs() > 2.0 {
                                        let n = ((to - from).abs() / 3.0).clamp(4.0, 64.0) as usize;
                                        let tension = if from < to { fx.in_tension } else { fx.out_tension };
                                        let pts: Vec<Pos2> = (0..=n)
                                            .map(|k| {
                                                let u = k as f32 / n as f32;
                                                Pos2::new(from + (to - from) * u, body.bottom() - cereal_engine::song::ClipFx::curve(u, tension) * (body.bottom() - top))
                                            })
                                            .collect();
                                        cp.add(egui::Shape::line(pts, line));
                                    }
                                }
                                let near = pointer.map(|pp| body.expand(4.0).contains(pp) && grid.contains(pp)).unwrap_or(false);
                                if near {
                                    fade_under = Some((i, body, r.left(), r.right()));
                                }
                                if near || fx.shaped() {
                                    let fill = Color32::from_gray(250);
                                    cp.add(egui::Shape::convex_polygon(vec![hs[0] + Vec2::new(-4.0, -3.0), hs[0] + Vec2::new(4.0, -3.0), hs[0] + Vec2::new(-4.0, 5.0)], fill, Stroke::NONE));
                                    cp.add(egui::Shape::convex_polygon(vec![hs[1] + Vec2::new(-4.0, -3.0), hs[1] + Vec2::new(4.0, -3.0), hs[1] + Vec2::new(4.0, 5.0)], fill, Stroke::NONE));
                                    cp.rect_filled(Rect::from_center_size(hs[2], Vec2::new(12.0, 4.0)), 1.0, fill);
                                    if fx.fade_in > 0.0 && hs[0].x - r.left() > 14.0 {
                                        cp.circle_stroke(hs[3], 3.0, Stroke::new(1.0_f32, fill));
                                    }
                                    if fx.fade_out > 0.0 && r.right() - hs[1].x > 14.0 {
                                        cp.circle_stroke(hs[4], 3.0, Stroke::new(1.0_f32, fill));
                                    }
                                    if opts & plo::GAIN_VALUE != 0 && fx.gain != 1.0 {
                                        let db = 20.0 * fx.gain.max(1e-5).log10();
                                        cp.text(hs[2] + Vec2::new(9.0, 0.0), Align2::LEFT_CENTER, format!("{db:+.1} dB"), FontId::proportional(9.0), fill);
                                    }
                                }
                            }
                        }
                    }
                    ClipSrc::Automation(ch) => {
                        if let Some(auto) = self.song.channels.get(ch).and_then(|c| c.auto.as_ref()) {
                            let cp = gp.with_clip_rect(body.intersect(grid));
                            let x_from = r.left().max(grid.left());
                            let x_to = r.right().min(grid.right());
                            let y_at = |x: f32| {
                                let t = (x - r.left()) / px + c.offset as f32;
                                body.bottom() - 1.0 - auto.value_at(t as f64).clamp(0.0, 1.0) * (body.height() - 2.0)
                            };
                            let mut pts = Vec::with_capacity(((x_to - x_from) / 2.0) as usize + 2);
                            let mut x = x_from;
                            while x <= x_to {
                                pts.push(Pos2::new(x, y_at(x)));
                                x += 2.0;
                            }
                            cp.add(egui::Shape::line(pts, Stroke::new(1.5_f32, Color32::from_gray(245))));
                            if body.height() > 14.0 {
                                // the points, and between each pair the handle that bends the line
                                let mut before: Option<&cereal_engine::song::AutoPt> = None;
                                for q in &auto.points {
                                    let x = r.left() + (q.tick as f32 - c.offset as f32) * px;
                                    if let Some(a) = before {
                                        let mid = (a.tick + q.tick) * 0.5;
                                        let mx = r.left() + (mid as f32 - c.offset as f32) * px;
                                        if x - mx > 7.0 && q.mode != 2 {
                                            cp.circle_stroke(Pos2::new(mx, y_at(mx)), 2.5, Stroke::new(1.0_f32, Color32::from_gray(245)));
                                        }
                                    }
                                    cp.circle_filled(Pos2::new(x, body.bottom() - 1.0 - q.value.clamp(0.0, 1.0) * (body.height() - 2.0)), 3.0, Color32::from_gray(250));
                                    before = Some(q);
                                }
                                if pointer.map(|pp| body.expand(4.0).contains(pp) && grid.contains(pp)).unwrap_or(false) {
                                    auto_under = Some((i, ch, body, r.left(), c.offset));
                                }
                            }
                        }
                    }
                }
            }
            if let Some(pp) = pointer {
                if r.contains(pp) && grid.contains(pp) {
                    let edge = if r.width() < 12.0 { 0 } else if pp.x > r.right() - 5.0 { 1 } else if pp.x < r.left() + 4.0 && opts & plo::RESIZE_LEFT != 0 { -1 } else { 0 };
                    hit = Some((i, edge));
                    hit_corner = r.width() >= 24.0 && pp.x >= r.left() + 1.0 && pp.x < r.left() + 16.0 && pp.y < r.top() + 12.0;
                }
            }
        }

        // play head
        if self.handle.transport.song_mode() {
            let x = x_of(self.handle.transport.tick() as f32);
            if x >= grid.left() && x <= grid.right() {
                let foot = if opts & plo::PRECISE_TIME != 0 { grid.bottom() } else { ruler.bottom() };
                p.line_segment([Pos2::new(x, ruler.top()), Pos2::new(x, foot)], Stroke::new(1.0_f32, GREEN));
            }
        }

        // ruler
        p.rect_filled(ruler, 0.0, PANEL_DARK);
        if let Some((a, b)) = self.playlist.time_sel {
            let (xa, xb) = (x_of(a as f32).max(grid.left()), x_of(b as f32).min(grid.right()));
            if xb > xa {
                let red = Color32::from_rgb(200, 70, 60);
                p.rect_filled(Rect::from_min_max(Pos2::new(xa, ruler.top()), Pos2::new(xb, ruler.bottom())), 0.0, red.gamma_multiply(0.7));
                p.rect_filled(Rect::from_min_max(Pos2::new(xa, grid.top()), Pos2::new(xb, grid.bottom())), 0.0, red.gamma_multiply(0.08));
            }
        }
        let label_every = every.max((40.0 / bar_px).ceil() as u32).max(1);
        for &(b, n, _) in &lines {
            let x = x_of(b as f32);
            if n % label_every == 0 && x >= grid.left() - 1.0 {
                p.text(Pos2::new(x + 3.0, ruler.center().y), Align2::LEFT_CENTER, format!("{}", n + 1), FontId::proportional(10.5), TEXT_DIM);
            }
        }
        // time markers sit on the ruler; their labels take clicks
        let mut marker_tabs: Vec<(Rect, usize)> = vec![];
        for (i, m) in self.song.markers.iter().enumerate() {
            let x = x_of(m.pos as f32);
            if x < grid.left() - 1.0 || x > grid.right() {
                continue;
            }
            let colour = Color32::from_rgb(236, 150, 60);
            p.line_segment([Pos2::new(x, ruler.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, colour.gamma_multiply(0.45)));
            let label = if m.kind == 8 && m.name.is_empty() { format!("{}/{}", m.sig.0, m.sig.1) } else { m.name.clone() };
            let galley = p.layout_no_wrap(label, FontId::proportional(10.5), Color32::BLACK);
            let tab = Rect::from_min_size(Pos2::new(x, ruler.top() + 1.0), Vec2::new(galley.size().x + 8.0, ruler.height() - 2.0));
            p.rect_filled(tab, 2.0, colour);
            p.galley(Pos2::new(tab.left() + 4.0, tab.center().y - galley.size().y * 0.5), galley, Color32::BLACK);
            marker_tabs.push((tab, i));
        }

        // track heads
        p.rect_filled(Rect::from_min_max(rect.min, Pos2::new(grid.left(), grid.top())), 0.0, PANEL_DARK);
        let hp = ui.painter_at(heads);
        let mut toggle: Option<usize> = None;
        let clicked = resp.clicked();
        for tr in first_track..=last_track {
            let y = y_of(tr);
            if h_of(tr) < 3.0 {
                continue;
            }
            let r = Rect::from_min_max(Pos2::new(heads.left(), y + 1.0), Pos2::new(heads.right() - 1.0, y + h_of(tr)));
            // the lower edge of a name drags the track taller or shorter
            if let Some(pp) = pointer.filter(|pp| self.playlist.size_drag.is_none() && resp.hovered() && heads.contains(*pp) && (pp.y - r.bottom()).abs() <= 3.0) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                if ui.input(|i| i.pointer.primary_pressed()) {
                    self.playlist.size_drag = Some((tr, pp.y, self.song.tracks[tr].size));
                }
            }
            let t = &self.song.tracks[tr];
            hp.rect_filled(r, 2.0, rgb(t.color).gamma_multiply(if t.enabled { 1.0 } else { 0.5 }));
            let name = if t.name.is_empty() { format!("Track {}", tr + 1) } else { t.name.clone() };
            let name = if self.playlist.collapsed.contains(&tr) { format!("\u{25b8} {name}") } else { name };
            hp.text(r.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, fit(ui, &name, r.width() - 24.0, &font), font.clone(), TEXT);
            let dot = Pos2::new(r.right() - 9.0, r.center().y);
            hp.circle_filled(dot, 4.0, if t.enabled { GREEN } else { PANEL_DARK });
            hp.circle_stroke(dot, 4.0, Stroke::new(1.0_f32, LINE));
            if let Some(pp) = pointer.filter(|pp| r.contains(*pp) && resp.hovered()) {
                if clicked && zone.is_some() && pp.x < r.right() - 18.0 {
                    // in performance mode a click on the name stops the track (with Alt, at once)
                    let now = ui.input(|i| i.modifiers.alt);
                    self.handle.send(Cmd::Perf { track: tr as u16, clip: None, now, queue: false });
                } else if clicked {
                    toggle = Some(tr);
                }
                if resp.secondary_clicked() {
                    self.playlist.track_menu = Some((tr, pp));
                }
            }
        }
        if let Some((tr, y0, was)) = self.playlist.size_drag {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            toggle = None;
            match pointer.filter(|_| ui.input(|i| i.pointer.primary_down())) {
                Some(pp) if tr < ntracks => {
                    let size = (was + (pp.y - y0) / th.max(1.0)).clamp(0.2, 8.0);
                    if (size - self.song.tracks[tr].size).abs() > 0.004 {
                        self.song.tracks[tr].size = size;
                        self.dirty = true;
                    }
                }
                _ => self.playlist.size_drag = None,
            }
        }
        if let Some(tr) = toggle {
            self.song.tracks[tr].enabled = !self.song.tracks[tr].enabled;
            self.dirty = true;
        }

        // ---- interaction
        let (pressed, down, sec_pressed, sec_down, mods) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.secondary_pressed(), i.pointer.secondary_down(), i.modifiers));
        if resp.hovered() && (pressed || sec_pressed) {
            self.focus = Win::Playlist;
        }
        self.playlist.grid_line = if bar_px >= 120.0 { beat } else { bar };
        self.playlist.left_tick = tick_of(grid.left()) as u32;
        self.playlist.view_w = grid.width();
        let g = if mods.alt { 1 } else { self.pl_snap() };
        // Events and Markers snap pull to the nearest clip edge or time marker instead of a grid
        let points: Vec<u32> = if mods.alt { vec![] } else { self.pl_snap_points() };
        // (the pull reaches 16 pixels; further out the pointer moves freely)
        let near = |t: f32, skip: &[u32]| points.iter().copied().filter(|p| !skip.contains(p) && (*p as f32 - t).abs() * px <= 16.0).min_by_key(|p| (*p as i64 - t as i64).abs());
        let g = if points.is_empty() { g } else { 1 };
        let snap = |t: f32| near(t.max(0.0), &[]).unwrap_or(((t.max(0.0) as u32) / g) * g);
        let snap_delta = |d: f32| ((d / g as f32).round() as i64) * g as i64;
        // a sound dragged from the browser becomes an audio clip where it is dropped
        if let Some(path) = resp.dnd_release_payload::<std::path::PathBuf>() {
            if let Some(pp) = pointer.filter(|pp| grid.contains(*pp)) {
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !["fst", "fsc", "mid", "flp", "gmsynth", "tbio", "mrp", "dwp"].contains(&ext.as_str()) {
                    self.add_audio_clip(&path, snap(tick_of(pp.x)), Some(track_of(pp.y) as u16));
                }
            }
            // dropped on a track's name: the clip goes to the start of the track, which can lock itself to it
            if let Some(pp) = pointer.filter(|pp| heads.contains(*pp)) {
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !["fst", "fsc", "mid", "flp", "gmsynth", "tbio", "mrp", "dwp"].contains(&ext.as_str()) {
                    let tr = track_of(pp.y);
                    self.add_audio_clip(&path, 0, Some(tr as u16));
                    if self.settings.pl_opts & plo::HEAD_LOCK != 0 {
                        if let Some(t) = self.song.tracks.get_mut(tr) {
                            t.lock_content = true;
                        }
                    }
                }
            }
        }
        // the right Shift and right Alt keys turn a click into a slice or a mute, whichever tool is picked
        let ralt = egui_winit::key_side::right_alt();
        let tool = if egui_winit::key_side::right_shift() { Tool::Slice } else if ralt { Tool::Mute } else { self.playlist.tool };
        let over = pointer.filter(|pp| resp.hovered() && rect.contains(*pp));
        // magic lasso: a loop drawn round clips with no button held picks them
        if self.settings.pl_opts & plo::LASSO != 0 {
            let free = self.playlist.drag.is_none() && !down && !sec_down;
            if let Some(poly) = crate::widgets::magic_lasso(ui, egui::Id::new("pl-lasso"), over.filter(|pp| free && grid.contains(*pp))) {
                let got: BTreeSet<usize> = self.song.clips.iter().enumerate().filter(|(_, c)| {
                    let tr = (c.track as usize).min(ntracks.saturating_sub(1));
                    crate::widgets::in_outline(&poly, egui::pos2((x_of(c.pos as f32) + x_of((c.pos + c.len) as f32)) * 0.5, (y_of(tr) + y_of(tr + 1)) * 0.5))
                }).map(|(i, _)| i).collect();
                if !got.is_empty() {
                    self.playlist.sel = got;
                }
            }
        }
        // performance mode: presses in the zone play clips; nothing there is edited while the mode is on
        let mut over = over;
        if !down && !self.playlist.perf_press.is_empty() {
            for t in std::mem::take(&mut self.playlist.perf_press) {
                self.handle.send(Cmd::PerfRelease { track: t });
            }
        }
        if let (Some(z), Some(pp)) = (zone, over) {
            let in_zone = grid.contains(pp) && tick_of(pp.x) < z as f32;
            let tab = marker_tabs.iter().rev().find(|(r, _)| r.contains(pp)).map(|(_, i)| *i).filter(|&i| self.song.markers[i].pos < z);
            if in_zone {
                auto_under = None;
            }
            if self.playlist.drag.is_none() && in_zone && (pressed || sec_pressed) {
                let track = track_of(pp.y) as u16;
                if sec_pressed {
                    self.handle.send(Cmd::Perf { track, clip: None, now: mods.alt, queue: false });
                } else if let Some((i, _)) = hit {
                    self.perf_click(i, mods);
                }
                self.playlist.drag = Some(Drag::Spent);
                over = None;
            } else if let (Some(i), true, true) = (tab, pressed, self.playlist.drag.is_none()) {
                // a marker of the zone plays its scene: the first clip under it on every track
                let (lo, hi) = self.perf_region(self.song.markers[i].pos, z);
                self.perf_scene(lo, hi, mods);
                self.playlist.drag = Some(Drag::Spent);
                over = None;
            } else if in_zone {
                over = None;
            }
        }
        // the points of an automation clip come before the clip itself
        if let (Some(pp), Some((clip, chan, body, left, offset))) = (over, auto_under) {
            if self.playlist.drag.is_none() && matches!(tool, Tool::Draw | Tool::Paint) && (pressed || sec_pressed) {
                let auto = self.song.channels[chan].auto.clone().unwrap();
                let pos_of = |tick: f64, value: f32| Pos2::new(left + (tick as f32 - offset as f32) * px, body.bottom() - 1.0 - value.clamp(0.0, 1.0) * (body.height() - 2.0));
                let on_point = auto.points.iter().position(|q| pos_of(q.tick, q.value).distance(pp) <= 6.0);
                let on_bend = (1..auto.points.len()).find(|&k| {
                    let mid = (auto.points[k - 1].tick + auto.points[k].tick) * 0.5;
                    auto.points[k].mode != 2 && pos_of(mid, auto.value_at(mid)).distance(pp) <= 5.0
                });
                if pressed {
                    if let Some(idx) = on_point {
                        self.playlist.drag = Some(Drag::AutoPt { chan, idx, clip, left, top: body.top(), bottom: body.bottom(), offset });
                    } else if let Some(idx) = on_bend {
                        self.playlist.drag = Some(Drag::AutoBend { chan, idx, y0: pp.y, t0: auto.points[idx].tension });
                    }
                } else if let Some(idx) = on_point {
                    self.playlist.auto_menu = Some((chan, idx, pp));
                    self.playlist.drag = Some(Drag::Spent);
                } else if let Some(idx) = on_bend {
                    // a right click on the handle straightens the line
                    std::sync::Arc::make_mut(self.song.channels[chan].auto.as_mut().unwrap()).points[idx].tension = 0.0;
                    self.dirty = true;
                    self.playlist.drag = Some(Drag::Spent);
                } else if body.contains(pp) {
                    // a right click on the picture puts a point there
                    let tick = (snap((pp.x - left) / px + offset as f32) as f64).max(0.0);
                    let value = ((body.bottom() - 1.0 - pp.y) / (body.height() - 2.0)).clamp(0.0, 1.0);
                    let a = std::sync::Arc::make_mut(self.song.channels[chan].auto.as_mut().unwrap());
                    let at = a.points.partition_point(|q| q.tick <= tick);
                    a.points.insert(at, cereal_engine::song::AutoPt { tick, value, tension: 0.0, mode: 0 });
                    self.dirty = true;
                    self.playlist.drag = Some(Drag::AutoPt { chan, idx: at, clip, left, top: body.top(), bottom: body.bottom(), offset });
                }
            }
        }
        // and so do the fade and level handles of a clip of sound
        if let (Some(pp), Some((clip, body, left, right))) = (over, fade_under) {
            if self.playlist.drag.is_none() && matches!(tool, Tool::Draw | Tool::Paint) && (pressed || sec_pressed) {
                let fx = self.song.clips[clip].fx;
                let hs = fade_handles(&fx, body, left, right, px / ms_per_tick);
                let shown = [true, true, true, fx.fade_in > 0.0 && hs[0].x - left > 14.0, fx.fade_out > 0.0 && right - hs[1].x > 14.0];
                let on = [3usize, 4, 0, 1, 2].into_iter().find(|&k| shown[k] && hs[k].distance(pp) <= 6.0);
                if let Some(k) = on {
                    if pressed {
                        let v0 = [fx.fade_in, fx.fade_out, fx.gain, fx.in_tension, fx.out_tension][k];
                        self.playlist.drag = Some(Drag::Fade { clip, which: k as u8, left, right, y0: pp.y, v0 });
                    } else {
                        // a right click puts the handle back
                        let fx = &mut self.song.clips[clip].fx;
                        match k {
                            0 => fx.fade_in = 0.0,
                            1 => fx.fade_out = 0.0,
                            2 => fx.gain = 1.0,
                            3 => fx.in_tension = 0.0,
                            _ => fx.out_tension = 0.0,
                        }
                        self.dirty = true;
                        self.playlist.drag = Some(Drag::Spent);
                    }
                }
            }
        }
        if let Some(pp) = over {
            let tab = marker_tabs.iter().rev().find(|(r, _)| r.contains(pp)).map(|(_, i)| *i);
            if let (Some(i), true, true) = (tab, sec_pressed, self.playlist.drag.is_none()) {
                self.rename_marker = Some((i, self.song.markers[i].name.clone()));
            }
            if pressed && self.playlist.drag.is_none() {
                if let Some(i) = tab {
                    self.handle.transport.seek(self.song.markers[i].pos);
                    self.playlist.drag = Some(Drag::Marker(i));
                } else if ruler.contains(pp) && mods.command {
                    self.playlist.time_sel = None;
                    self.playlist.drag = Some(Drag::TimeSel { from: snap(tick_of(pp.x)) });
                } else if ruler.contains(pp) {
                    self.playlist.drag = Some(Drag::Seek);
                } else if grid.contains(pp) {
                    let track = track_of(pp.y);
                    match tool {
                        Tool::Draw | Tool::Paint | Tool::Slip if mods.command => self.playlist.drag = Some(Drag::Rect { from: pp, add: mods.shift, zoom: false }),
                        Tool::Draw | Tool::Paint | Tool::Slip => match hit {
                            Some((i, _)) if !hit_corner && ui.input(|x| x.pointer.button_double_clicked(egui::PointerButton::Primary)) => {
                                self.pl_open_clip(i);
                                self.playlist.drag = Some(Drag::Spent);
                            }
                            Some((i, _)) if hit_corner => {
                                self.playlist.clip_menu = Some((i, pp));
                                self.playlist.drag = Some(Drag::Spent);
                            }
                            Some((i, edge)) => {
                                let c = self.song.clips[i].clone();
                                self.playlist.brush = Some((c.src, c.len, c.offset));
                                if let ClipSrc::Pattern(pi) = c.src {
                                    self.handle.transport.set_pattern(pi);
                                    self.playlist.brush_pat = pi;
                                }
                                if !self.playlist.sel.contains(&i) {
                                    self.playlist.sel.clear();
                                }
                                let mut targets: Vec<usize> = if !self.playlist.sel.is_empty() {
                                    self.playlist.sel.iter().copied().collect()
                                } else if self.song.clips[i].group != 0 {
                                    // a grouped clip takes its group along
                                    (0..self.song.clips.len()).filter(|&k| self.song.clips[k].group == self.song.clips[i].group).collect()
                                } else {
                                    vec![i]
                                };
                                let mode = if tool == Tool::Slip { Mode::Slip } else { match edge { -1 => Mode::Left, 1 => Mode::Right, _ => Mode::Move } };
                                if mods.shift && mode == Mode::Move {
                                    // the originals stay; copies travel
                                    let was_sel = !self.playlist.sel.is_empty();
                                    let mut copies = vec![];
                                    for &t in &targets {
                                        copies.push(self.song.clips.len());
                                        let c = self.song.clips[t].clone();
                                        self.song.clips.push(c);
                                    }
                                    crate::widgets::fresh_groups(&mut self.song.clips, &copies, |c| &mut c.group);
                                    self.playlist.sel = if was_sel { copies.iter().copied().collect() } else { BTreeSet::new() };
                                    targets = copies;
                                    self.dirty = true;
                                }
                                let orig = targets.iter().map(|&t| (t, self.song.clips[t].clone())).collect();
                                self.playlist.drag = Some(Drag::Clips { mode, anchor_tick: tick_of(pp.x), anchor_track: track as i32, orig });
                            }
                            None if tool == Tool::Slip => {}
                            None => {
                                let (src, len, offset) = self.brush_on(track, pat);
                                let c = Clip { pos: snap(tick_of(pp.x)), len, track: track as u16, src, offset, muted: false, group: 0, fine: 0.0, fx: Default::default() };
                                self.song.clips.push(c.clone());
                                self.playlist.sel.clear();
                                self.playlist.drag = Some(if tool == Tool::Paint { Drag::Paint } else { Drag::Clips { mode: Mode::Move, anchor_tick: tick_of(pp.x), anchor_track: track as i32, orig: vec![(self.song.clips.len() - 1, c)] } });
                                self.dirty = true;
                            }
                        },
                        Tool::Delete => self.playlist.drag = Some(Drag::Sweep { mute: None, right: false }),
                        Tool::Mute => {
                            let to = hit.map(|(i, _)| !self.song.clips[i].muted).unwrap_or(true);
                            self.playlist.drag = Some(Drag::Sweep { mute: Some(to), right: false });
                        }
                        Tool::Slice => {
                            let t = tick_of(pp.x);
                            self.playlist.drag = Some(Drag::Slice { tick: (((t / g as f32).round() as u32) * g).max(1), del: false });
                        }
                        Tool::Select => self.playlist.drag = Some(Drag::Rect { from: pp, add: mods.shift, zoom: false }),
                        Tool::Zoom => self.playlist.drag = Some(Drag::Rect { from: pp, add: false, zoom: true }),
                        Tool::Playback => self.playlist.drag = Some(Drag::Seek),
                    }
                }
            } else if sec_pressed && self.playlist.drag.is_none() && grid.contains(pp) {
                match tool {
                    Tool::Mute if ralt => {
                        // right Alt and a right-click: hear the clip
                        if let Some(ClipSrc::Audio(ch)) = hit.map(|h| self.song.clips[h.0].src) {
                            if let Some(sm) = self.song.channels.get(ch).and_then(|c| c.sample.clone()) {
                                self.handle.send(cereal_engine::Cmd::Preview(sm));
                            }
                        }
                    }
                    Tool::Slice => {
                        let t = tick_of(pp.x);
                        self.playlist.drag = Some(Drag::Slice { tick: (((t / g as f32).round() as u32) * g).max(1), del: true });
                    }
                    Tool::Zoom => {
                        let off = pp.x - grid.left();
                        let old = self.playlist.zoom;
                        self.playlist.zoom = (old * 0.5).max(0.005);
                        self.playlist.scroll_x = ((self.playlist.scroll_x + off) * self.playlist.zoom / old - off).max(0.0);
                    }
                    Tool::Select if hit.is_none() => self.playlist.sel.clear(),
                    _ => self.playlist.drag = Some(Drag::Sweep { mute: None, right: true }),
                }
            }
            if self.playlist.drag.is_none() && hit.map(|h| h.1 != 0).unwrap_or(false) && matches!(tool, Tool::Draw | Tool::Paint) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }

        self.playlist.hover = over.filter(|pp| grid.contains(*pp)).map(|pp| (snap(tick_of(pp.x)), hit.map(|h| h.0)));

        let mut done = false;
        let mut cut: Option<(usize, u32, bool)> = None;
        if let (Some(pp), Some(drag)) = (pointer, self.playlist.drag.as_ref()) {
            match drag {
                Drag::Clips { mode, anchor_tick, anchor_track, orig } => {
                    if down {
                        let raw = tick_of(pp.x) - anchor_tick;
                        let dt = if points.is_empty() {
                            snap_delta(raw)
                        } else {
                            // the moved clips' own edges are not targets; the first clip's start does the snapping
                            let own: Vec<u32> = orig.iter().flat_map(|(_, c)| [c.pos, c.pos + c.len]).collect();
                            let first = orig.iter().map(|(_, c)| c.pos).min().unwrap_or(0);
                            match near((first as f32 + raw).max(0.0), &own) {
                                Some(p) => p as i64 - first as i64,
                                None => snap_delta(raw),
                            }
                        };
                        let dtr = track_of(pp.y) as i32 - anchor_track;
                        let dt_move = dt.max(-(orig.iter().map(|(_, c)| c.pos).min().unwrap_or(0) as i64));
                        let dtr = dtr.clamp(-(orig.iter().map(|(_, c)| c.track as i32).min().unwrap_or(0)), ntracks as i32 - 1 - orig.iter().map(|(_, c)| c.track as i32).max().unwrap_or(0));
                        for (i, o) in orig {
                            let c = &mut self.song.clips[*i];
                            let mut n = o.clone();
                            match mode {
                                Mode::Move => {
                                    n.pos = (o.pos as i64 + dt_move) as u32;
                                    n.track = (o.track as i32 + dtr) as u16;
                                }
                                Mode::Right => {
                                    n.len = (o.len as i64 + dt).max(g.min(beat) as i64) as u32;
                                    if let (true, ClipSrc::Audio(ch)) = (self.playlist.stretch, o.src) {
                                        if let Some(chan) = self.song.channels.get_mut(ch) {
                                            let f = &mut chan.smp;
                                            let was = match self.playlist.stretch_from {
                                                Some((c, l, b)) if c == ch && l == o.len => b,
                                                _ => {
                                                    let own = chan.sample.as_ref().map(|s| s.seconds() * self.song.tempo / 60.0).unwrap_or(0.0);
                                                    let b = if f.stretch_beats > 0.0 { f.stretch_beats } else { own / f.stretch_mul.max(1e-6) };
                                                    self.playlist.stretch_from = Some((ch, o.len, b));
                                                    b
                                                }
                                            };
                                            if was > 0.0 {
                                                f.stretch_beats = (was * n.len as f32 / o.len.max(1) as f32).max(0.01);
                                                f.keep_pitch = true;
                                            }
                                        }
                                    }
                                }
                                Mode::Left => {
                                    let d = dt.clamp(-(o.offset.min(o.pos) as i64), o.len as i64 - g.min(beat) as i64);
                                    n.pos = (o.pos as i64 + d) as u32;
                                    n.offset = (o.offset as i64 + d) as u32;
                                    n.len = (o.len as i64 - d) as u32;
                                }
                                Mode::Slip => n.offset = (o.offset as i64 - dt).max(0) as u32,
                            }
                            if c.pos != n.pos || c.track != n.track || c.len != n.len || c.offset != n.offset {
                                *c = n.clone();
                                self.dirty = true;
                            }
                            self.playlist.brush = Some((n.src, n.len, n.offset));
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Rect { from, add, zoom } => {
                    let r = Rect::from_two_pos(*from, pp).intersect(grid);
                    if down {
                        ui.painter_at(grid).rect(r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18), Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
                    } else {
                        let (t0, t1) = (tick_of(r.left()), tick_of(r.right()));
                        if *zoom {
                            if r.width() > 6.0 {
                                let new_px = (grid.width() / (t1 - t0).max(1.0)).min(4.0 * 96.0 / ppq);
                                self.playlist.zoom = new_px * ppq / 96.0;
                                self.playlist.scroll_x = t0 * new_px;
                            }
                        } else {
                            if !add {
                                self.playlist.sel.clear();
                            }
                            let (r0, r1) = (track_of(r.top()), track_of(r.bottom()));
                            for (i, c) in self.song.clips.iter().enumerate() {
                                if (c.pos as f32) < t1.max(t0 + 1.0) && (c.pos + c.len) as f32 > t0 && (r0..=r1).contains(&(c.track as usize)) {
                                    self.playlist.sel.insert(i);
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
                                self.pl_remove(&gone);
                            }
                            Some(to) => {
                                if self.song.clips[i].muted != *to {
                                    self.song.clips[i].muted = *to;
                                    self.dirty = true;
                                }
                            }
                        }
                    }
                }
                Drag::Slice { tick, del } => {
                    if if *del { sec_down } else { down } {
                        let x = x_of(*tick as f32);
                        ui.painter_at(grid).line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, Color32::WHITE));
                        if let Some((i, _)) = hit {
                            cut = Some((i, *tick, *del));
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Paint => {
                    if down && grid.contains(pp) {
                        let track = track_of(pp.y) as u16;
                        let (src, len, offset) = self.brush_on(track as usize, pat);
                        let unit = len.max(g).max(1);
                        let pos = snap(tick_of(pp.x)) / unit * unit;
                        if !self.song.clips.iter().any(|c| c.track == track && c.pos < pos + len && pos < c.pos + c.len) {
                            self.song.clips.push(Clip { pos, len, track, src, offset, muted: false, group: 0, fine: 0.0, fx: Default::default() });
                            self.dirty = true;
                        }
                    } else if !down {
                        done = true;
                    }
                }
                Drag::AutoPt { chan, idx, clip, left, top, bottom, offset } => {
                    let (chan, idx, clip) = (*chan, *idx, *clip);
                    if down || sec_down {
                        let value = ((bottom - 1.0 - pp.y) / (bottom - top - 2.0)).clamp(0.0, 1.0);
                        let want = snap((pp.x - left) / px + *offset as f32) as f64;
                        if let Some(a) = self.song.channels.get_mut(chan).and_then(|c| c.auto.as_mut()) {
                            let a = std::sync::Arc::make_mut(a);
                            if idx < a.points.len() {
                                // a point stays between its neighbours, and the first one stays at the start
                                let lo = if idx == 0 { 0.0 } else { a.points[idx - 1].tick };
                                let hi = a.points.get(idx + 1).map(|q| q.tick).unwrap_or(f64::MAX);
                                let tick = if idx == 0 { 0.0 } else { want.clamp(lo, hi) };
                                let was = a.points[idx];
                                // Ctrl keeps the time, Shift keeps the level
                                a.points[idx].value = if mods.shift { was.value } else { value };
                                a.points[idx].tick = if mods.command { was.tick } else { tick };
                                let end = a.length().ceil() as u32;
                                if let Some(c) = self.song.clips.get_mut(clip) {
                                    if idx + 1 == a.points.len() && end > c.offset {
                                        c.len = (end - c.offset).max(1);
                                    }
                                }
                                self.dirty = true;
                            }
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::AutoBend { chan, idx, y0, t0 } => {
                    if down {
                        if let Some(a) = self.song.channels.get_mut(*chan).and_then(|c| c.auto.as_mut()) {
                            let a = std::sync::Arc::make_mut(a);
                            if let Some(k) = (*idx).checked_sub(1).filter(|_| *idx < a.points.len()) {
                                // pull towards the higher end to reach it sooner
                                let rising = a.points[*idx].value >= a.points[k].value;
                                let pull = (y0 - pp.y) / 60.0 * if rising { -1.0 } else { 1.0 };
                                a.points[*idx].tension = (t0 + pull).clamp(-1.0, 1.0);
                                self.dirty = true;
                            }
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Fade { clip, which, left, right, y0, v0 } => {
                    if down {
                        if let Some(c) = self.song.clips.get_mut(*clip) {
                            let len_ms = c.len as f32 * ms_per_tick;
                            // a fade's length keeps to the grid when the option asks for it
                            let ticks = |x: f32| {
                                let t = (x / px).max(0.0);
                                if opts & plo::SNAP_FADES != 0 && !mods.alt { snap(t) as f32 } else { t }
                            };
                            // a bend runs from none to a great deal over a hand's width of travel
                            let bend = |v0: f32, pull: f32| {
                                let at = v0.signum() * v0.abs().ln_1p() + pull / 30.0;
                                (at.signum() * (at.abs().exp() - 1.0)).clamp(-1000.0, 1000.0)
                            };
                            match *which {
                                0 => c.fx.fade_in = (ticks(pp.x - *left) * ms_per_tick).clamp(0.0, len_ms),
                                1 => c.fx.fade_out = (ticks(*right - pp.x) * ms_per_tick).clamp(0.0, len_ms),
                                2 => {
                                    let db = 20.0 * v0.max(1e-5).log10() + (*y0 - pp.y) / 4.0;
                                    c.fx.gain = if db <= -60.0 { 0.0 } else { 10f32.powf(db.min(18.0) / 20.0) };
                                }
                                3 => c.fx.in_tension = bend(*v0, *y0 - pp.y),
                                _ => c.fx.out_tension = bend(*v0, *y0 - pp.y),
                            }
                            self.dirty = true;
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Spent => {
                    if !down && !sec_down {
                        done = true;
                    }
                }
                Drag::Marker(i) => {
                    if down {
                        let to = snap(tick_of(pp.x));
                        if let Some(m) = self.song.markers.get_mut(*i).filter(|m| m.pos != to && (pp.x - x_of(m.pos as f32)).abs() > 3.0) {
                            m.pos = to;
                            self.dirty = true;
                        }
                    } else {
                        self.song.markers.sort_by_key(|m| m.pos);
                        done = true;
                    }
                }
                Drag::TimeSel { from } => {
                    if down {
                        let to = snap(tick_of(pp.x) + g as f32 * 0.5);
                        let (a, b) = ((*from).min(to), (*from).max(to));
                        self.playlist.time_sel = (b > a).then_some((a, b));
                    } else {
                        done = true;
                    }
                }
                Drag::Seek => {
                    if down {
                        if !self.handle.transport.song_mode() {
                            self.handle.transport.set_song_mode(true);
                        }
                        let to = snap(tick_of(pp.x));
                        let q = [0, beat, bar, bar * 2, bar * 4, bar * 8][self.settings.pl_jump_q.min(5) as usize];
                        if q > 0 && self.handle.transport.playing() {
                            let now = self.handle.transport.tick().max(0.0) as u32;
                            self.playlist.jump = Some(((now / q + 1) * q, to));
                        } else {
                            self.handle.transport.seek(to);
                        }
                    } else {
                        done = true;
                    }
                }
            }
        } else if self.playlist.drag.is_some() && !down && !sec_down {
            done = true;
        }
        if let Some((i, at, del)) = cut {
            match self.song.clips.get_mut(i) {
                Some(c) if del && at > c.pos && at < c.pos + c.len => {
                    let (head, rest) = (at - c.pos, c.pos + c.len - at);
                    if head >= rest {
                        c.len = head;
                    } else {
                        c.offset += head;
                        c.pos = at;
                        c.len = rest;
                    }
                    self.dirty = true;
                }
                _ => self.pl_slice_clip(i, at),
            }
        }
        if done {
            self.playlist.drag = None;
        }
        self.auto_point_menu(ui.ctx());
        self.track_menu(ui.ctx());
        self.clip_menu(ui.ctx());
    }

    /// The places Events snap (clip edges) or Markers snap (time markers) pull to; empty for grid snaps.
    fn pl_snap_points(&self) -> Vec<u32> {
        let pick = if self.playlist.snap == SNAP_MAIN { self.main_snap } else { self.playlist.snap };
        let mut v: Vec<u32> = match SNAPS[pick.min(SNAPS.len() - 1)].0 {
            "Events" => self.song.clips.iter().flat_map(|c| [c.pos, c.pos + c.len]).collect(),
            "Markers" => self.song.markers.iter().map(|m| m.pos).collect(),
            _ => vec![],
        };
        v.sort_unstable();
        v.dedup();
        v
    }

    fn pl_snap(&self) -> u32 {
        let pick = if self.playlist.snap == SNAP_MAIN { self.main_snap } else { self.playlist.snap };
        let (_, unit, div) = SNAPS[pick.min(SNAPS.len() - 1)];
        let base = match unit {
            0 => self.playlist.grid_line,
            1 => 1,
            2 => self.song.step(),
            3 => self.song.ppq,
            _ => self.song.bar(),
        };
        (base / div).max(1)
    }

    fn pl_remove(&mut self, gone: &BTreeSet<usize>) {
        if gone.is_empty() {
            return;
        }
        let sel = std::mem::take(&mut self.playlist.sel);
        let old: Vec<Clip> = self.song.clips.drain(..).collect();
        for (i, c) in old.into_iter().enumerate() {
            if gone.contains(&i) {
                continue;
            }
            if sel.contains(&i) {
                self.playlist.sel.insert(self.song.clips.len());
            }
            self.song.clips.push(c);
        }
        self.dirty = true;
    }

    fn pl_targets(&self) -> Vec<usize> {
        if self.playlist.sel.is_empty() {
            (0..self.song.clips.len()).collect()
        } else {
            self.playlist.sel.iter().copied().filter(|&i| i < self.song.clips.len()).collect()
        }
    }

    fn pl_delete_selected(&mut self) {
        let gone = self.playlist.sel.clone();
        self.pl_remove(&gone);
        self.playlist.sel.clear();
    }

    fn pl_nudge(&mut self, ticks: i64, tracks: i32) {
        let t = self.pl_targets();
        let n = self.song.tracks.len() as i32;
        let clips = &mut self.song.clips;
        if t.is_empty() || t.iter().any(|&i| (clips[i].pos as i64 + ticks) < 0 || !(0..n).contains(&(clips[i].track as i32 + tracks))) {
            return;
        }
        for &i in &t {
            clips[i].pos = (clips[i].pos as i64 + ticks) as u32;
            clips[i].track = (clips[i].track as i32 + tracks) as u16;
        }
        self.dirty = true;
    }

    /// Where the selection starts and ends in time, pulled out to whole bars.
    fn pl_span(&self) -> Option<(u32, u32)> {
        self.playlist.time_sel.or_else(|| self.pl_clip_span())
    }

    /// The bars the selected clips cover.
    fn pl_clip_span(&self) -> Option<(u32, u32)> {
        let bar = self.song.bar();
        let clips = &self.song.clips;
        let sel: Vec<usize> = self.playlist.sel.iter().copied().filter(|&i| i < clips.len()).collect();
        let a = sel.iter().map(|&i| clips[i].pos).min()? / bar * bar;
        let b = sel.iter().map(|&i| clips[i].pos + clips[i].len.max(1)).max()?.div_ceil(bar) * bar;
        Some((a, b.max(a + bar)))
    }

    /// Open what a clip plays in its editor, the way the "Edit patterns in" choice asks.
    pub(crate) fn pl_open_clip(&mut self, i: usize) {
        let Some(c) = self.song.clips.get(i).cloned() else { return };
        match c.src {
            ClipSrc::Pattern(p) => {
                self.handle.transport.set_pattern(p);
                self.playlist.brush_pat = p;
                let mut chans: Vec<u16> = self.song.patterns.get(p).map(|p| p.notes.iter().map(|n| n.chan).collect()).unwrap_or_default();
                chans.sort_unstable();
                chans.dedup();
                let roll = match self.settings.pl_edit_in {
                    1 => false,
                    2 | 3 => true,
                    _ => chans.len() == 1,
                };
                if roll {
                    if let Some(&ch) = chans.first() {
                        self.sel_chan = ch as usize;
                    }
                    self.show_piano = true;
                    self.focus = Win::Piano;
                    if self.settings.pl_opts & plo::SCROLL_TO_POS != 0 {
                        self.pr_scroll_to(c.offset);
                    }
                } else {
                    self.show_rack = true;
                    self.focus = Win::Rack;
                }
            }
            ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => {
                self.sel_chan = ch;
                self.rack.sel.clear();
                self.rack.sel.insert(ch);
                self.show_chanset = true;
            }
        }
    }

    /// Cut every clip that crosses the start of the selection, then open the gap there.
    pub(crate) fn pl_slice_space(&mut self) {
        let Some((from, _)) = self.pl_span() else { return };
        for i in 0..self.song.clips.len() {
            self.pl_slice_clip(i, from);
        }
        self.pl_space(true);
    }

    /// Cut the selected clips down to the picked stretch of time.
    pub(crate) fn pl_trim(&mut self) {
        let Some((a, b)) = self.playlist.time_sel else {
            self.status = "Pick a stretch of time on the ruler first (Ctrl and drag)".into();
            return;
        };
        let mut gone = BTreeSet::new();
        for i in self.pl_targets() {
            let c = &mut self.song.clips[i];
            let (lo, hi) = (c.pos.max(a), (c.pos + c.len).min(b));
            if hi <= lo {
                gone.insert(i);
            } else {
                c.offset += lo - c.pos;
                c.pos = lo;
                c.len = hi - lo;
            }
        }
        self.pl_remove(&gone);
        self.dirty = true;
    }

    /// The selected automation clips of one track become a single clip with one curve.
    pub(crate) fn pl_merge_auto(&mut self) {
        let mut picked: Vec<usize> = self.playlist.sel.iter().copied().filter(|&i| matches!(self.song.clips.get(i).map(|c| c.src), Some(ClipSrc::Automation(_)))).collect();
        picked.sort_by_key(|&i| self.song.clips[i].pos);
        if picked.len() < 2 {
            self.status = "Select two or more automation clips to merge".into();
            return;
        }
        let start = self.song.clips[picked[0]].pos;
        let end = picked.iter().map(|&i| self.song.clips[i].pos + self.song.clips[i].len).max().unwrap_or(start);
        let track = self.song.clips[picked[0]].track;
        let ClipSrc::Automation(first) = self.song.clips[picked[0]].src else { return };
        let mut points = Vec::new();
        for &i in &picked {
            let c = self.song.clips[i].clone();
            let ClipSrc::Automation(ch) = c.src else { continue };
            let Some(a) = self.song.channels.get(ch).and_then(|c| c.auto.clone()) else { continue };
            let (lo, hi) = (c.offset as f64, (c.offset + c.len) as f64);
            let at = |t: f64| (c.pos - start) as f64 + (t - lo);
            // the clip's own ends are kept as points, so the joined curve sounds as the parts did
            let mut part = vec![cereal_engine::song::AutoPt { tick: at(lo), value: a.value_at(lo), tension: 0.0, mode: 2 }];
            part.extend(a.points.iter().filter(|p| p.tick > lo && p.tick < hi).map(|p| cereal_engine::song::AutoPt { tick: at(p.tick), ..p.clone() }));
            if let Some(next) = a.points.iter().find(|p| p.tick >= hi) {
                part.push(cereal_engine::song::AutoPt { tick: at(hi), value: a.value_at(hi), ..next.clone() });
            }
            points.extend(part);
        }
        let Some(mut made) = self.song.channels.get(first).cloned() else { return };
        let targets = made.auto.as_ref().map(|a| a.targets.clone()).unwrap_or_default();
        made.auto = Some(std::sync::Arc::new(cereal_engine::song::Automation { points, targets }));
        made.name = format!("{} (merged)", made.name);
        self.song.channels.push(made);
        let n = self.song.channels.len() - 1;
        let gone: BTreeSet<usize> = picked.into_iter().collect();
        self.pl_remove(&gone);
        self.song.clips.push(Clip { pos: start, len: end - start, track, src: ClipSrc::Automation(n), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
        self.playlist.sel = [self.song.clips.len() - 1].into_iter().collect();
        self.dirty = true;
    }

    /// A copy of each selected audio clip's sound in a file of its own, for editing without touching the first.
    pub(crate) fn pl_unique_sample(&mut self) {
        let picked: Vec<usize> = self.playlist.sel.iter().copied().filter(|&i| matches!(self.song.clips.get(i).map(|c| c.src), Some(ClipSrc::Audio(_)))).collect();
        let mut made = 0;
        for i in picked {
            let ClipSrc::Audio(ch) = self.song.clips[i].src else { continue };
            let Some(mut copy) = self.song.channels.get(ch).cloned() else { continue };
            let old = std::path::PathBuf::from(&copy.sample_path);
            if old.is_file() {
                let stem = old.file_stem().and_then(|s| s.to_str()).unwrap_or("sample").to_string();
                let ext = old.extension().and_then(|s| s.to_str()).unwrap_or("wav").to_string();
                let new = (2..1000).map(|k| old.with_file_name(format!("{stem} ({k}).{ext}"))).find(|p| !p.exists());
                if let Some(new) = new.filter(|new| std::fs::copy(&old, new).is_ok()) {
                    copy.sample_path = new.to_string_lossy().into_owned();
                    copy.name = new.file_stem().and_then(|s| s.to_str()).unwrap_or(&copy.name).to_string();
                }
            }
            self.song.channels.push(copy);
            self.song.clips[i].src = ClipSrc::Audio(self.song.channels.len() - 1);
            made += 1;
        }
        self.dirty |= made > 0;
        self.status = if made > 0 { format!("{made} audio clips made unique with their own sample files") } else { "Select audio clips first".into() };
    }

    /// Clips that lie wholly under another clip on their track (`whole`), or that share any time with one.
    /// The lowest clip of each pile is left out, so deleting the selection clears the pile.
    pub(crate) fn pl_select_piled(&mut self, whole: bool) {
        let c = &self.song.clips;
        self.playlist.sel = (0..c.len())
            .filter(|&i| {
                (0..i).any(|j| c[j].track == c[i].track && if whole { c[j].pos <= c[i].pos && c[j].pos + c[j].len >= c[i].pos + c[i].len } else { c[j].pos < c[i].pos + c[i].len && c[i].pos < c[j].pos + c[j].len })
            })
            .collect();
    }

    /// Fold every group of tracks away under its first track, or open them all again.
    pub(crate) fn pl_collapse_groups(&mut self, fold: bool) {
        self.playlist.collapsed.clear();
        if fold {
            let t = &self.song.tracks;
            self.playlist.collapsed = (0..t.len()).filter(|&i| !t[i].grouped && t.get(i + 1).is_some_and(|n| n.grouped)).collect();
        }
    }

    /// The markers the time marker menu works on: those inside the picked time, else the one at or before
    /// the play position.
    fn pl_markers_picked(&self) -> Vec<usize> {
        let m = &self.song.markers;
        match self.playlist.time_sel {
            Some((a, b)) => (0..m.len()).filter(|&i| m[i].pos >= a && m[i].pos < b).collect(),
            None => {
                let now = self.handle.transport.tick().max(0.0) as u32;
                (0..m.len()).filter(|&i| m[i].pos <= now).max_by_key(|&i| m[i].pos).into_iter().collect()
            }
        }
    }

    /// The extra entries of the time marker menu.
    pub(crate) fn pl_marker_tool(&mut self, what: u8, every: u32) {
        use cereal_engine::song::TimeMarker;
        let bar = self.song.bar().max(1);
        let now = if self.handle.transport.song_mode() { self.handle.transport.tick().max(0.0) as u32 / bar * bar } else { 0 };
        let put = |song: &mut cereal_engine::song::Song, pos: u32, name: String, kind: u8| {
            if !song.markers.iter().any(|m| m.pos == pos && m.kind == kind) {
                song.markers.push(TimeMarker { pos, name, kind, sig: (4, 4) });
            }
        };
        match what {
            // auto
            0 => put(&mut self.song, now, "Auto".into(), 0),
            // two, at the ends of the picked time
            1 => match self.playlist.time_sel {
                Some((a, b)) => {
                    put(&mut self.song, a, "Start".into(), 0);
                    put(&mut self.song, b, "End".into(), 0);
                }
                None => self.status = "Pick a stretch of time on the ruler first (Ctrl and drag)".into(),
            },
            // one every so many bars across the picked time
            2 => match self.playlist.time_sel {
                Some((a, b)) => {
                    let mut t = a;
                    while t < b {
                        let name = format!("Marker #{}", self.song.markers.len() + 1);
                        put(&mut self.song, t, name, 0);
                        t += bar * every.max(1);
                    }
                }
                None => self.status = "Pick a stretch of time on the ruler first (Ctrl and drag)".into(),
            },
            // skip ahead, loop return, key
            3 => put(&mut self.song, self.playlist.time_sel.map(|t| t.0).unwrap_or(now), "Jump".into(), 2),
            4 => put(&mut self.song, self.playlist.time_sel.map(|t| t.0).unwrap_or(now), "Loop".into(), 4),
            5 => {
                const KEYS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                put(&mut self.song, self.playlist.time_sel.map(|t| t.0).unwrap_or(now), format!("{} Major", KEYS[every as usize % 12]), 0);
            }
            6 => {
                let gone = self.pl_markers_picked();
                let mut i = 0;
                self.song.markers.retain(|_| {
                    i += 1;
                    !gone.contains(&(i - 1))
                });
            }
            _ => {
                for i in self.pl_markers_picked() {
                    self.song.markers[i].kind = 1;
                }
            }
        }
        self.song.markers.sort_by_key(|m| m.pos);
        self.dirty = true;
    }

    /// A copy of each track that holds a selected clip, with its clips, just below it.
    pub(crate) fn pl_clone_tracks(&mut self) {
        let mut picked: Vec<usize> = self.playlist.sel.iter().filter_map(|&i| self.song.clips.get(i)).map(|c| c.track as usize).collect();
        picked.sort_unstable();
        picked.dedup();
        if picked.is_empty() {
            self.status = "Select a clip on each track to clone first".into();
            return;
        }
        let mut made = 0;
        // from the bottom up, so the tracks above keep their numbers
        for &tr in picked.iter().rev() {
            let n = self.song.tracks.len();
            if tr + 1 >= n || self.song.clips.iter().any(|c| c.track as usize == n - 1) {
                continue;
            }
            self.song.tracks.pop();
            let copy = self.song.tracks[tr].clone();
            self.song.tracks.insert(tr + 1, copy);
            for c in self.song.clips.iter_mut().filter(|c| c.track as usize > tr) {
                c.track += 1;
            }
            let twins: Vec<Clip> = self.song.clips.iter().filter(|c| c.track as usize == tr).cloned().map(|mut c| {
                c.track += 1;
                c
            }).collect();
            self.song.clips.extend(twins);
            made += 1;
        }
        self.playlist.sel.clear();
        self.dirty |= made > 0;
        self.status = format!("{made} tracks cloned");
    }

    /// Every track with clips on it (or every group of tracks) rendered to a file of its own.
    /// `from`: 0 the song start, 1 each track's first clip, 2 the picked time.
    pub(crate) fn pl_export_tracks(&mut self, groups: bool, from: u8) {
        let nt = self.song.tracks.len();
        // which set each track belongs to: itself, or the head of its group
        let mut head = vec![0usize; nt];
        for t in 0..nt {
            head[t] = if groups && self.song.tracks[t].grouped && t > 0 { head[t - 1] } else { t };
        }
        let span = self.playlist.time_sel.filter(|_| from == 2);
        if from == 2 && span.is_none() {
            self.status = "Pick a stretch of time on the ruler first (Ctrl and drag)".into();
            return;
        }
        let dir = crate::record::folder(self.path.as_deref()).join("Tracks");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = format!("Could not make {}: {e}", dir.display());
            return;
        }
        let rate = 44_100u32;
        let mut made = 0;
        for h in 0..nt {
            let own: Vec<Clip> = self.song.clips.iter().filter(|c| head.get(c.track as usize) == Some(&h) && !c.muted).cloned().collect();
            if own.is_empty() || (groups && head[h] != h) {
                continue;
            }
            let a = match (from, span) {
                (2, Some((a, _))) => a,
                (1, _) => own.iter().map(|c| c.pos).min().unwrap_or(0),
                _ => 0,
            };
            let b = span.map(|s| s.1).unwrap_or(u32::MAX);
            let mut part = self.song.clone();
            part.clips = own.into_iter().filter(|c| c.pos + c.len > a && c.pos < b).map(|mut c| {
                // cut to the stretch that is wanted, then slid to its start
                let lo = c.pos.max(a);
                let hi = (c.pos + c.len).min(b);
                c.offset += lo - c.pos;
                c.len = hi - lo;
                c.pos = lo - a;
                c
            }).collect();
            if part.clips.is_empty() {
                continue;
            }
            part.markers.retain(|m| m.kind == 8);
            for m in &mut part.markers {
                m.pos = m.pos.saturating_sub(a);
            }
            part.prepare();
            let buf = cereal_engine::render_song(&part, rate as f32, true, 0, 2.0);
            let t = &self.song.tracks[h];
            let name: String = if t.name.is_empty() { format!("Track {}", h + 1) } else { t.name.clone() }.chars().map(|c| if c.is_alphanumeric() || " -_#()".contains(c) { c } else { '_' }).collect();
            let path = dir.join(format!("{:02} {name}.wav", h + 1));
            let wrote = std::fs::File::create(&path).and_then(|f| crate::wav::write(std::io::BufWriter::new(f), &buf, rate, 2, 24));
            if wrote.is_ok() {
                made += 1;
            }
        }
        self.status = format!("{made} tracks written to {}", dir.display());
    }

    /// Patterns and clip channels with no clip on the playlist get marked in the picker panel.
    pub(crate) fn pl_mark_unused(&mut self) {
        use cereal_engine::flp::ChannelKind as K;
        let used = |s: ClipSrc| self.song.clips.iter().any(|c| c.src == s);
        let mut m: Vec<ClipSrc> = (0..self.song.patterns.len()).map(ClipSrc::Pattern).filter(|s| !used(*s)).collect();
        for (c, ch) in self.song.channels.iter().enumerate() {
            let s = if ch.kind == K::AudioClip { ClipSrc::Audio(c) } else if ch.auto.is_some() { ClipSrc::Automation(c) } else { continue };
            if !used(s) {
                m.push(s);
            }
        }
        self.status = format!("{} unused sources marked in the picker panel", m.len());
        self.playlist.marked = m;
        self.playlist.picker = true;
    }

    /// Open a gap as long as the selection where it starts, or close the selection's time up.
    /// With nothing selected it is one bar at the left of the view.
    fn pl_space(&mut self, add: bool) {
        let bar = self.song.bar();
        let (from, to) = self.pl_span().unwrap_or_else(|| {
            let a = self.playlist.left_tick.div_ceil(bar) * bar;
            (a, a + bar)
        });
        let len = to - from;
        if add {
            for c in self.song.clips.iter_mut().filter(|c| c.pos >= from) {
                c.pos += len;
            }
        } else {
            let gone: BTreeSet<usize> = (0..self.song.clips.len()).filter(|&i| self.song.clips[i].pos >= from && self.song.clips[i].pos < to).collect();
            for c in self.song.clips.iter_mut().filter(|c| c.pos >= to) {
                c.pos -= len;
            }
            self.pl_remove(&gone);
            self.playlist.sel.clear();
        }
        self.dirty = true;
    }

    /// Select the clips in the stretch of time just before or just after the selection, of the same length.
    fn pl_select_beside(&mut self, after: bool) {
        let Some((a, b)) = self.pl_span() else { return };
        let (lo, hi) = if after { (b, b + (b - a)) } else { (a.saturating_sub(b - a), a) };
        if self.playlist.time_sel.is_some() && hi > lo {
            self.playlist.time_sel = Some((lo, hi));
        }
        self.playlist.sel = (0..self.song.clips.len()).filter(|&i| self.song.clips[i].pos >= lo && self.song.clips[i].pos < hi).collect();
    }

    /// Make the selected clips one group, or take them out of their groups.
    fn pl_group(&mut self, on: bool) {
        let t: Vec<usize> = self.playlist.sel.iter().copied().collect();
        if !t.is_empty() {
            crate::widgets::set_group(&mut self.song.clips, &t, on, |c| &mut c.group);
            self.dirty = true;
        }
    }

    /// A selection that holds part of a group holds all of it.
    fn pl_whole_groups(&mut self) {
        let clips = &self.song.clips;
        let groups: BTreeSet<u16> = self.playlist.sel.iter().filter_map(|&i| clips.get(i)).map(|c| c.group).filter(|g| *g != 0).collect();
        if !groups.is_empty() {
            self.playlist.sel.extend((0..clips.len()).filter(|&i| groups.contains(&clips[i].group)));
        }
    }

    /// Every clip that plays the same thing as one of the selected clips.
    fn pl_select_same(&mut self) {
        let clips = &self.song.clips;
        let srcs: Vec<ClipSrc> = self.playlist.sel.iter().filter_map(|&i| clips.get(i)).map(|c| c.src).collect();
        if !srcs.is_empty() {
            self.playlist.sel = (0..clips.len()).filter(|&i| srcs.contains(&clips[i].src)).collect();
        }
    }


    /// Every selected clip gets a copy of its pattern or channel for itself.
    fn pl_make_unique(&mut self) {
        for i in self.pl_targets() {
            match self.song.clips[i].src {
                ClipSrc::Pattern(p) => {
                    let n = self.song.clone_pattern(p);
                    self.song.clips[i].src = ClipSrc::Pattern(n);
                }
                ClipSrc::Audio(c) | ClipSrc::Automation(c) => {
                    if let Some(copy) = self.song.channels.get(c).cloned() {
                        self.song.channels.push(copy);
                        let n = self.song.channels.len() - 1;
                        self.song.clips[i].src = if matches!(self.song.clips[i].src, ClipSrc::Audio(_)) { ClipSrc::Audio(n) } else { ClipSrc::Automation(n) };
                    }
                }
            }
            self.dirty = true;
        }
    }

    /// The selected pattern clips become one clip of a new pattern that holds what they played; with
    /// `similar`, clips of the same pattern on the same track are merged among themselves only.
    fn pl_merge(&mut self, similar: bool) {
        let picked: Vec<usize> = self.playlist.sel.iter().copied().filter(|&i| matches!(self.song.clips.get(i).map(|c| c.src), Some(ClipSrc::Pattern(_)))).collect();
        let mut sets: Vec<Vec<usize>> = Vec::new();
        if similar {
            for &i in &picked {
                let c = &self.song.clips[i];
                match sets.iter_mut().find(|s| self.song.clips[s[0]].src == c.src && self.song.clips[s[0]].track == c.track) {
                    Some(s) => s.push(i),
                    None => sets.push(vec![i]),
                }
            }
        } else {
            sets.push(picked);
        }
        sets.retain(|s| s.len() > 1);
        if sets.is_empty() {
            self.status = "Select two or more pattern clips to merge".into();
            return;
        }
        let mut gone = BTreeSet::new();
        let mut made = Vec::new();
        for set in &sets {
            let start = set.iter().map(|&i| self.song.clips[i].pos).min().unwrap_or(0);
            let end = set.iter().map(|&i| self.song.clips[i].pos + self.song.clips[i].len).max().unwrap_or(0);
            let track = set.iter().map(|&i| self.song.clips[i].track).min().unwrap_or(0);
            let mut notes = Vec::new();
            for &i in set {
                let c = self.song.clips[i].clone();
                let ClipSrc::Pattern(p) = c.src else { continue };
                for n in self.song.patterns.get(p).map(|p| p.notes.as_slice()).unwrap_or(&[]) {
                    if n.pos >= c.offset && n.pos < c.offset + c.len {
                        let mut n = n.clone();
                        let room = c.offset + c.len - n.pos;
                        n.len = n.len.min(room);
                        n.pos = c.pos - start + (n.pos - c.offset);
                        notes.push(n);
                    }
                }
            }
            notes.sort_by_key(|n| n.pos);
            let p = self.song.patterns.len();
            self.song.ensure_pattern(p);
            self.song.patterns[p].notes = notes;
            self.song.patterns[p].name = "Merged".into();
            made.push(Clip { pos: start, len: end - start, track, src: ClipSrc::Pattern(p), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
            gone.extend(set.iter().copied());
        }
        self.pl_remove(&gone);
        let n = self.song.clips.len();
        self.song.clips.extend(made);
        self.playlist.sel = (n..self.song.clips.len()).collect();
        self.dirty = true;
    }

    /// Pull clip starts onto the snap grid.
    fn pl_quantise(&mut self) {
        let g = self.pl_snap().max(1);
        let t = self.pl_targets();
        for &i in &t {
            let c = &mut self.song.clips[i];
            c.pos = (c.pos + g / 2) / g * g;
        }
        self.dirty |= !t.is_empty();
    }

    /// Show the stretch from tick `a` to tick `b` across the whole width.
    fn pl_show(&mut self, a: u32, b: u32) {
        let ppq = self.song.ppq as f32;
        let px = (self.playlist.view_w.max(100.0) / (b.max(a + 1) - a) as f32).clamp(0.005 * 96.0 / ppq, 4.0 * 96.0 / ppq);
        self.playlist.zoom = px * ppq / 96.0;
        self.playlist.scroll_x = a as f32 * px;
    }

    /// The zoom keys: Shift and a digit, and the plain 5.
    fn pl_zoom_key(&mut self, d: usize) {
        let bar = self.song.bar();
        let left = self.playlist.left_tick;
        match d {
            0 => {
                let px = self.playlist.zoom * 96.0 / self.song.ppq as f32;
                self.playlist.scroll_x = (self.handle.transport.tick() as f32 * px - self.playlist.view_w * 0.5).max(0.0);
            }
            1 => self.pl_show(left, left + bar * 4),
            2 => self.pl_show(left, left + bar * 16),
            3 => self.pl_show(left, left + bar * 64),
            4 => {
                let end = self.song.length();
                self.pl_show(0, end.max(bar * 4));
            }
            _ => {
                if let Some((a, b)) = self.pl_span() {
                    self.pl_show(a, b);
                }
            }
        }
    }

    fn pl_duplicate(&mut self) {
        let t = self.pl_targets();
        if t.is_empty() {
            return;
        }
        let bar = self.song.bar();
        let clips = &mut self.song.clips;
        let start = t.iter().map(|&i| clips[i].pos).min().unwrap_or(0) / bar * bar;
        let end = t.iter().map(|&i| clips[i].pos + clips[i].len).max().unwrap_or(0);
        let by = (end - start).div_ceil(bar).max(1) * bar;
        self.playlist.sel.clear();
        for &i in &t {
            let mut c = clips[i].clone();
            c.pos += by;
            self.playlist.sel.insert(clips.len());
            clips.push(c);
        }
        let new: Vec<usize> = self.playlist.sel.iter().copied().collect();
        crate::widgets::fresh_groups(clips, &new, |c| &mut c.group);
        self.dirty = true;
    }

    /// Cut one clip in two at a tick inside it.
    fn pl_slice_clip(&mut self, i: usize, at: u32) {
        let Some(c) = self.song.clips.get_mut(i) else { return };
        if at > c.pos && at < c.pos + c.len {
            let mut tail = c.clone();
            tail.pos = at;
            tail.offset = c.offset + (at - c.pos);
            tail.len = c.pos + c.len - at;
            c.len = at - c.pos;
            self.song.clips.push(tail);
            self.dirty = true;
        }
    }

    /// Cut one clip at each of these song positions.
    fn pl_chop(&mut self, i: usize, cuts: &[u32]) {
        let mut cuts = cuts.to_vec();
        cuts.sort_unstable();
        cuts.dedup();
        // from the far end, so the clip at `i` stays the head that is cut next
        for &at in cuts.iter().rev() {
            self.pl_slice_clip(i, at);
        }
        self.playlist.sel.clear();
    }

    /// Shift one clip by the snap value.
    pub(crate) fn pl_nudge_clip(&mut self, i: usize, later: bool) {
        let g = self.pl_snap().max(1);
        if let Some(c) = self.song.clips.get_mut(i) {
            c.pos = if later { c.pos + g } else { c.pos.saturating_sub(g) };
            self.dirty = true;
        }
    }

    /// A new time marker at the play position, named straight away.
    pub(crate) fn add_marker(&mut self) {
        let bar = self.song.bar().max(1);
        let pos = if self.handle.transport.song_mode() { (self.handle.transport.tick().max(0.0) as u32 / bar) * bar } else { 0 };
        let at = match self.song.markers.iter().position(|m| m.pos == pos && m.kind == 0) {
            Some(i) => i,
            None => {
                let name = format!("Marker #{}", self.song.markers.len() + 1);
                self.song.markers.push(cereal_engine::song::TimeMarker { pos, name, kind: 0, sig: (4, 4) });
                self.song.markers.sort_by_key(|m| m.pos);
                self.dirty = true;
                self.song.markers.iter().position(|m| m.pos == pos && m.kind == 0).unwrap_or(0)
            }
        };
        self.rename_marker = Some((at, self.song.markers[at].name.clone()));
    }

    /// A time signature change at the start of the picked time, or on the bar under the play position.
    pub(crate) fn add_time_sig(&mut self) {
        let now = if self.handle.transport.song_mode() { self.handle.transport.tick().max(0.0) as u32 } else { 0 };
        let pos = match self.playlist.time_sel {
            Some((a, _)) => a,
            None => self.song.bar_lines(now, now).last().map(|l| l.0).unwrap_or(0),
        };
        let at = match self.song.markers.iter().position(|m| m.pos == pos && m.kind == 8) {
            Some(i) => i,
            None => {
                let sig = self.song.markers.iter().filter(|m| m.kind == 8 && m.pos < pos).max_by_key(|m| m.pos).map(|m| m.sig).unwrap_or(self.song.time_sig);
                self.song.markers.push(cereal_engine::song::TimeMarker { pos, name: String::new(), kind: 8, sig });
                self.song.markers.sort_by_key(|m| m.pos);
                self.dirty = true;
                self.song.markers.iter().position(|m| m.pos == pos && m.kind == 8).unwrap_or(0)
            }
        };
        self.rename_marker = Some((at, self.song.markers[at].name.clone()));
    }

    /// Put the play position on the next or the previous time marker.
    pub(crate) fn jump_marker(&mut self, forward: bool) {
        let now = self.handle.transport.tick().max(0.0) as u32;
        let to = if forward { self.song.markers.iter().map(|m| m.pos).find(|&p| p > now) } else { self.song.markers.iter().rev().map(|m| m.pos).find(|&p| p < now) };
        if let Some(to) = to {
            if !self.handle.transport.song_mode() {
                self.handle.transport.set_song_mode(true);
            }
            self.handle.transport.seek(to);
        }
    }

    fn pl_mute(&mut self, muted: bool) {
        let t = self.pl_targets();
        for &i in &t {
            self.song.clips[i].muted = muted;
        }
        self.dirty |= !t.is_empty();
    }

    pub(crate) fn pl_copy(&mut self, ctx: &egui::Context, cut: bool) {
        if self.playlist.sel.is_empty() {
            return;
        }
        let t = self.pl_targets();
        self.playlist.clip = t.iter().map(|&i| self.song.clips[i].clone()).collect();
        ctx.copy_text(format!("{} clips", t.len()));
        if cut {
            self.pl_delete_selected();
        }
    }

    pub(crate) fn pl_paste(&mut self) {
        if self.playlist.clip.is_empty() {
            return;
        }
        let bar = self.song.bar();
        let first = self.playlist.clip.iter().map(|c| c.pos).min().unwrap_or(0) / bar * bar;
        let to = self.playlist.left_tick.div_ceil(bar) * bar;
        self.playlist.sel.clear();
        for c in &self.playlist.clip {
            // a clip whose pattern or channel has gone since the copy is left out
            let ok = match c.src {
                ClipSrc::Pattern(p) => p < self.song.patterns.len(),
                ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => ch < self.song.channels.len(),
            };
            if ok {
                self.playlist.sel.insert(self.song.clips.len());
                self.song.clips.push(Clip { pos: c.pos - first + to, ..c.clone() });
            }
        }
        let new: Vec<usize> = self.playlist.sel.iter().copied().collect();
        crate::widgets::fresh_groups(&mut self.song.clips, &new, |c| &mut c.group);
        self.dirty = true;
    }

    /// Bounce the selected clips into one audio clip. The sound is rendered off line with only those
    /// clips playing, the clips are muted and the new clip is laid over them. `from_start` renders
    /// from the top of the song so that the clip starts there.
    pub(crate) fn pl_consolidate(&mut self, from_start: bool) {
        let sel: Vec<usize> = self.playlist.sel.iter().copied().filter(|&i| i < self.song.clips.len()).collect();
        if sel.is_empty() {
            self.status = "Select the clips to consolidate first".into();
            return;
        }
        let clips = &self.song.clips;
        let a = if from_start { 0 } else { sel.iter().map(|&i| clips[i].pos).min().unwrap_or(0) };
        let b = sel.iter().map(|&i| clips[i].pos + clips[i].len.max(1)).max().unwrap_or(a + 1);
        let track = sel.iter().map(|&i| clips[i].track).min().unwrap_or(0);
        let mut part = self.song.clone();
        part.clips = sel.iter().map(|&i| Clip { pos: clips[i].pos - a, muted: false, ..clips[i].clone() }).collect();
        part.clips.sort_by_key(|c| c.pos);
        part.markers.retain(|m| m.kind == 8);
        for m in &mut part.markers {
            m.pos = m.pos.saturating_sub(a);
        }
        part.prepare();
        let rate = 44_100u32;
        let buf = cereal_engine::render_song(&part, rate as f32, true, 0, 2.0);
        let dir = crate::record::folder(self.path.as_deref());
        let made = std::fs::create_dir_all(&dir).and_then(|_| {
            let mut n = 1;
            let path = loop {
                let p = dir.join(format!("Consolidated {n}.wav"));
                if !p.exists() {
                    break p;
                }
                n += 1;
            };
            let f = std::fs::File::create(&path)?;
            crate::wav::write(std::io::BufWriter::new(f), &buf, rate, 2, 24)?;
            Ok(path)
        });
        match made {
            Ok(path) => {
                for &i in &sel {
                    self.song.clips[i].muted = true;
                }
                let before = self.song.channels.len();
                self.add_audio_clip(&path, a, Some(track));
                if self.song.channels.len() > before {
                    let ci = self.song.channels.len() - 1;
                    self.playlist.sel.clear();
                    for (i, c) in self.song.clips.iter_mut().enumerate() {
                        if c.src == ClipSrc::Audio(ci) {
                            // the tail rings on in the file; the clip itself covers what it replaces
                            c.len = (b - a).max(1);
                            c.track = track;
                            self.playlist.sel.insert(i);
                        }
                    }
                    self.status = format!("Consolidated into {}", path.display());
                }
                self.dirty = true;
            }
            Err(e) => self.status = format!("Consolidate failed: {e}"),
        }
    }

    /// PATTERNS > Render as audio clip: the current pattern is played into a sound file that becomes an audio
    /// clip channel. With `replace` every clip of the pattern on the playlist gives way to the sound.
    pub(crate) fn render_pattern(&mut self, replace: bool) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let len = self.song.pattern_length(pat).max(1);
        let mut part = self.song.clone();
        part.clips = vec![Clip { pos: 0, len, track: 0, src: ClipSrc::Pattern(pat), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() }];
        part.markers.clear();
        part.prepare();
        let rate = 44_100u32;
        let buf = cereal_engine::render_song(&part, rate as f32, true, 0, 2.0);
        let dir = crate::record::folder(self.path.as_deref());
        let name = self.song.pattern_name(pat).replace(['/', '\\', ':'], "_");
        let made = std::fs::create_dir_all(&dir).and_then(|_| {
            let mut n = 1;
            let path = loop {
                let p = if n == 1 { dir.join(format!("{name}.wav")) } else { dir.join(format!("{name} {n}.wav")) };
                if !p.exists() {
                    break p;
                }
                n += 1;
            };
            let f = std::fs::File::create(&path)?;
            crate::wav::write(std::io::BufWriter::new(f), &buf, rate, 2, 24)?;
            Ok(path)
        });
        match made {
            Ok(path) => {
                let before = self.song.channels.len();
                let clips_before = self.song.clips.len();
                self.add_audio_clip(&path, 0, None);
                if self.song.channels.len() > before {
                    let ci = self.song.channels.len() - 1;
                    // the sound is wanted as a channel; where it lies is decided here
                    self.song.clips.truncate(clips_before);
                    if replace {
                        for c in self.song.clips.iter_mut().filter(|c| c.src == ClipSrc::Pattern(pat)) {
                            c.src = ClipSrc::Audio(ci);
                        }
                        self.song.clips.sort_by_key(|c| c.pos);
                        self.playlist.sel.clear();
                    }
                    self.playlist.brush = None;
                    self.status = format!("Rendered into {}", path.display());
                }
                self.dirty = true;
            }
            Err(e) => self.status = format!("Render failed: {e}"),
        }
    }

    pub(crate) fn pl_tool_names() -> Vec<&'static str> {
        TOOLS.iter().map(|t| t.2).collect()
    }
    pub(crate) fn pl_pick_tool(&mut self, i: usize) {
        if let Some(t) = TOOLS.get(i) {
            self.playlist.tool = t.0;
        }
    }

    /// A key press while the playlist has the focus. True when it was ours.
    pub(crate) fn playlist_key(&mut self, key: Key, m: Modifiers, repeat: bool) -> bool {
        if m.command && !m.alt && !m.shift && key == Key::P {
            if !repeat {
                self.toggle_performance();
            }
            return true;
        }
        if m.shift && !m.command && !m.alt && key == Key::Num6 {
            // fit the performance zone to the window
            if let Some(z) = self.song.perf_start() {
                self.playlist.zoom = (self.playlist.view_w.max(100.0) / (z as f32 * 96.0 / self.song.ppq.max(1) as f32)).clamp(0.005, 4.0);
                self.playlist.scroll_x = 0.0;
            }
            return true;
        }
        if m.command && m.alt {
            match key {
                Key::C if !repeat => self.pl_consolidate(m.shift),
                Key::Insert if !repeat => self.pl_slice_space(),
                Key::G if !repeat => self.pl_merge_auto(),
                Key::Home if !repeat => {
                    self.settings.pl_opts ^= plo::RESIZE_LEFT;
                    let _ = self.settings.save();
                }
                _ => return false,
            }
            return true;
        }
        if m.command && !m.alt {
            match key {
                Key::A => self.playlist.sel = (0..self.song.clips.len()).collect(),
                Key::D => {
                    self.playlist.sel.clear();
                    self.playlist.time_sel = None;
                }
                Key::Enter if !repeat => self.playlist.time_sel = self.pl_clip_span(),
                Key::B if m.shift => {
                    if !repeat {
                        self.pl_clone_tracks();
                    }
                }
                Key::B => self.pl_duplicate(),
                Key::G if !repeat => self.pl_merge(m.shift),
                Key::T if !m.shift && !repeat => self.pl_marker_tool(0, 0),
                Key::V if m.shift && !repeat => self.pl_make_unique(),
                Key::Insert if !m.shift => self.pl_space(true),
                Key::Delete => self.pl_space(false),
                Key::ArrowLeft => self.pl_select_beside(false),
                Key::ArrowRight => self.pl_select_beside(true),
                _ => return false,
            }
            return true;
        }
        if m.alt && !m.command {
            match key {
                Key::T if !m.shift && !repeat => self.add_marker(),
                Key::T if !repeat => self.add_time_sig(),
                Key::P if !repeat => self.playlist.picker = !self.playlist.picker,
                Key::Slash => self.jump_marker(false),
                Key::M => self.pl_mute(!m.shift),
                Key::G if !repeat => self.pl_group(false),
                _ => return false,
            }
            return true;
        }
        if m.shift {
            match key {
                Key::I => {
                    self.playlist.sel = (0..self.song.clips.len()).filter(|i| !self.playlist.sel.contains(i)).collect();
                    return true;
                }
                Key::C => {
                    self.pl_select_same();
                    return true;
                }
                Key::M if !repeat => {
                    self.playlist.stretch = !self.playlist.stretch;
                    return true;
                }
                Key::Q => {
                    self.pl_quantise();
                    return true;
                }
                Key::T if !repeat => {
                    self.pl_marker_tool(4, 0);
                    return true;
                }
                Key::G if !repeat => {
                    self.pl_group(true);
                    return true;
                }
                k => {
                    if let Some(d) = crate::DIGITS.iter().position(|d| *d == k).filter(|d| *d <= 5) {
                        self.pl_zoom_key(d);
                        return true;
                    }
                }
            }
            if self.playlist.sel.is_empty() {
                return false;
            }
            match key {
                Key::ArrowLeft => self.pl_nudge(-(self.pl_snap() as i64), 0),
                Key::ArrowRight => self.pl_nudge(self.pl_snap() as i64, 0),
                Key::ArrowUp => self.pl_nudge(0, -1),
                Key::ArrowDown => self.pl_nudge(0, 1),
                _ => return false,
            }
            return true;
        }
        match key {
            Key::Delete => self.pl_delete_selected(),
            Key::PageUp | Key::PageDown => {
                let old = self.playlist.zoom;
                self.playlist.zoom = (old * if key == Key::PageUp { 1.25 } else { 0.8 }).clamp(0.005, 4.0);
                let off = self.playlist.view_w * 0.5;
                self.playlist.scroll_x = ((self.playlist.scroll_x + off) * self.playlist.zoom / old - off).max(0.0);
            }
            Key::Home => self.handle.transport.seek(0),
            Key::Insert if !repeat => {
                if let Some((at, Some(i))) = self.playlist.hover {
                    self.pl_slice_clip(i, at);
                }
            }
            Key::Backspace if !repeat => {
                if self.playlist.snap == 1 {
                    self.playlist.snap = self.playlist.snap_back;
                } else {
                    self.playlist.snap_back = self.playlist.snap;
                    self.playlist.snap = 1;
                }
            }
            _ if self.typing_keys || repeat => return false,
            Key::Num5 => self.pl_zoom_key(5),
            Key::Num7 => self.pl_audio_keys(-100.0, false),
            Key::Num8 => self.pl_audio_keys(100.0, false),
            Key::Num9 => self.pl_audio_keys(0.0, true),
            Key::P => self.playlist.tool = Tool::Draw,
            Key::B => self.playlist.tool = Tool::Paint,
            Key::D => self.playlist.tool = Tool::Delete,
            Key::T => self.playlist.tool = Tool::Mute,
            Key::S => self.playlist.tool = Tool::Slip,
            Key::C => self.playlist.tool = Tool::Slice,
            Key::E => self.playlist.tool = Tool::Select,
            Key::Z => self.playlist.tool = Tool::Zoom,
            Key::Y => self.playlist.tool = Tool::Playback,
            _ => return false,
        }
        true
    }

    /// Transpose or reverse the sound of the selected audio clips (or the one under the pointer).
    fn pl_audio_keys(&mut self, cents: f32, reverse: bool) {
        let mut clips: Vec<usize> = self.playlist.sel.iter().copied().collect();
        if clips.is_empty() {
            if let Some((_, Some(i))) = self.playlist.hover {
                clips.push(i);
            }
        }
        let mut chans: Vec<usize> = clips.iter().filter_map(|&i| match self.song.clips.get(i).map(|c| c.src) {
            Some(ClipSrc::Audio(ch)) if ch < self.song.channels.len() => Some(ch),
            _ => None,
        }).collect();
        chans.sort();
        chans.dedup();
        for ch in chans {
            let c = &mut self.song.channels[ch];
            if reverse {
                c.smp.reverse = !c.smp.reverse;
                c.bake();
            } else {
                c.smp.stretch_cents = (c.smp.stretch_cents + cents).clamp(-1200.0, 1200.0);
            }
            self.dirty = true;
        }
    }

    fn playlist_header(&mut self, ui: &mut Ui, pat: usize) {
        ui.horizontal(|ui| {
            ui.menu_button("\u{23F7}", |ui| self.playlist_menu(ui)).response.hint("Playlist menu");
            for (t, label, hint) in TOOLS {
                if ui.selectable_label(self.playlist.tool == t, RichText::new(label).small()).hint(hint).clicked() {
                    self.playlist.tool = t;
                }
            }
            if ui.selectable_label(self.playlist.stretch, RichText::new("Stretch").small()).hint("Stretch mode: dragging the end of an audio clip fits its sound to the new length (Shift+M)").clicked() {
                self.playlist.stretch = !self.playlist.stretch;
            }
            ui.separator();
            egui::ComboBox::from_id_salt("pl_snap").width(74.0).selected_text(snap_label(self.playlist.snap)).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.playlist.snap, SNAP_MAIN, "Main");
                for (i, label) in crate::pianoroll::snap_order() {
                    ui.selectable_value(&mut self.playlist.snap, i, label);
                }
            })
            .response
            .hint("Snap");
            ui.separator();
            let brush = match self.playlist.brush {
                Some((ClipSrc::Pattern(p), _, _)) => self.song.pattern_name(p),
                Some((ClipSrc::Audio(c), _, _)) | Some((ClipSrc::Automation(c), _, _)) => self.song.channels.get(c).map(|c| c.name.clone()).unwrap_or_default(),
                None => self.song.pattern_name(pat),
            };
            let cur = self.song.cur_arr.min(self.song.arrangements.len().saturating_sub(1));
            let name = self.song.arrangements.get(cur).map(|a| a.name.clone()).unwrap_or_else(|| "Arrangement".into());
            let mut turn = None;
            ui.menu_button(RichText::new(name).color(TEXT), |ui| {
                for (i, a) in self.song.arrangements.iter().enumerate() {
                    if ui.radio(i == cur, a.name.as_str()).clicked() {
                        turn = Some(i);
                        ui.close_menu();
                    }
                }
                ui.separator();
                let n = self.song.arrangements.len();
                if ui.button("Add one").clicked() {
                    self.song.add_arrangement(&format!("Arrangement {}", n + 1), false);
                    turn = Some(usize::MAX);
                    ui.close_menu();
                }
                if ui.button("Clone").clicked() {
                    let name = format!("{} #2", self.song.arrangements[cur].name);
                    self.song.add_arrangement(&name, true);
                    turn = Some(usize::MAX);
                    ui.close_menu();
                }
                if ui.add_enabled(n > 1, egui::Button::new("Delete")).clicked() {
                    self.song.remove_arrangement(cur);
                    turn = Some(usize::MAX);
                    ui.close_menu();
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if let Some(a) = self.song.arrangements.get_mut(cur) {
                        if ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(160.0)).changed() {
                            self.dirty = true;
                        }
                    }
                });
            })
            .response
            .hint("Arrangement: each one is a playlist of its own over the same patterns and channels");
            if let Some(i) = turn {
                self.song.set_arrangement(i);
                self.playlist.sel.clear();
                self.dirty = true;
            }
            ui.label(RichText::new(format!(">  {brush}")).color(TEXT));
        });
    }
}
