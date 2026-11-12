//! The playlist: pattern clips and audio clips on tracks.

use std::collections::BTreeSet;

use egui::{Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::{Clip, ClipSrc, Target};

use crate::pianoroll::SNAPS;
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
    Slice { tick: u32 },
    Seek,
    /// Picking out a stretch of time on the ruler, from this tick.
    TimeSel { from: u32 },
    Marker(usize),
    /// A point of an automation clip: channel, point, clip, and where the clip's picture sits.
    AutoPt { chan: usize, idx: usize, clip: usize, left: f32, top: f32, bottom: f32, offset: u32 },
    /// The bend of the line arriving at a point.
    AutoBend { chan: usize, idx: usize, y0: f32, t0: f32 },
    /// A press that has done its work and waits for the button to come up.
    Spent,
}

/// The shapes a line between two automation points can take, with the number the file gives each.
const AUTO_MODES: [(u8, &str); 6] = [(0, "Single curve"), (1, "Double curve"), (2, "Hold"), (3, "Stairs"), (4, "Smooth stairs"), (9, "Half sine")];

pub struct State {
    scroll_x: f32,
    scroll_y: f32,
    /// Pixels per tick at 96 ppq.
    zoom: f32,
    track_h: f32,
    drag: Option<Drag>,
    pub tool: Tool,
    snap: usize,
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
    view_w: f32,
    snap_back: usize,
}

impl Default for State {
    fn default() -> Self {
        Self { scroll_x: 0.0, scroll_y: 0.0, zoom: 0.09, track_h: 30.0, drag: None, tool: Tool::Draw, snap: 0, time_sel: None, sel: BTreeSet::new(), hover: None, clip: vec![], brush: None, stretch: false, picker: false, stretch_from: None, brush_pat: usize::MAX, grid_line: 96, left_tick: 0, auto_menu: None, view_w: 800.0, snap_back: 0 }
    }
}

const HEAD_W: f32 = 112.0;
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

    /// How far along its travel a control stands in the song, 0..=1.
    fn target_travel(&self, t: Target) -> f32 {
        let s = &self.song;
        let v = match t {
            Target::ChanVolume(c) => s.channels.get(c).map(|c| c.volume),
            Target::ChanPan(c) => s.channels.get(c).map(|c| c.pan * 0.5 + 0.5),
            Target::ChanPitch(c) => s.channels.get(c).map(|c| c.pitch_cents / 400.0 + 0.5),
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
        let v = self.target_travel(t);
        let len = self.song.length().max(self.song.bar() * 4);
        let pt = |tick: f64| cereal_engine::song::AutoPt { tick, value: v, tension: 0.0, mode: 0 };
        let mut c = cereal_engine::song::Chan::default();
        c.name = self.song.target_name(t);
        c.kind = cereal_engine::flp::ChannelKind::AutomationClip;
        c.auto = Some(std::sync::Arc::new(cereal_engine::song::Automation { points: vec![pt(0.0), pt(len as f64)], targets: vec![t] }));
        self.song.channels.push(c);
        let ci = self.song.channels.len() - 1;
        let ntracks = self.song.tracks.len().max(1) as u16;
        let track = (0..ntracks).find(|&tr| !self.song.clips.iter().any(|c| c.track == tr)).unwrap_or(0);
        self.song.clips.push(Clip { pos: 0, len, track, src: ClipSrc::Automation(ci), offset: 0, muted: false, group: 0 });
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
            egui::SidePanel::left("pl_picker").resizable(true).default_width(150.0).frame(egui::Frame::NONE.inner_margin(4.0)).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("pl_picker_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    let brush = self.playlist.brush.map(|b| b.0).unwrap_or(ClipSrc::Pattern(pat));
                    ui.label(RichText::new("Patterns").small().color(TEXT_DIM));
                    for p in 0..self.song.patterns.len() {
                        let name = if self.song.patterns[p].name.is_empty() { format!("Pattern {}", p + 1) } else { self.song.patterns[p].name.clone() };
                        if ui.selectable_label(brush == ClipSrc::Pattern(p), name).clicked() {
                            self.handle.transport.set_pattern(p);
                            self.playlist.brush = None;
                            self.playlist.brush_pat = p;
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
                            if ui.selectable_label(brush == src, self.song.channels[c].name.clone()).clicked() {
                                // as long as a clip of it that is already there, else the sound's own length, else four bars
                                let ch = &self.song.channels[c];
                                let own = ch.sample.as_ref().filter(|_| audio).map(|s| {
                                    let beats = if ch.smp.stretch_beats > 0.0 { ch.smp.stretch_beats * ch.smp.stretch_mul } else { s.seconds() * self.song.tempo / 60.0 };
                                    (beats * ppq).ceil() as u32
                                });
                                let len = self.song.clips.iter().find(|k| k.src == src).map(|k| k.len).or(own).unwrap_or(bar * 4).max(1);
                                self.playlist.brush = Some((src, len, 0));
                            }
                        }
                    }
                });
            });
        }
        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(Vec2::new(200.0, 120.0)), Sense::click_and_drag());
        let grid = Rect::from_min_max(rect.min + Vec2::new(HEAD_W, RULER_H), rect.max);
        let heads = Rect::from_min_max(Pos2::new(rect.left(), grid.top()), Pos2::new(grid.left(), rect.bottom()));
        let ruler = Rect::from_min_max(Pos2::new(grid.left(), rect.top()), Pos2::new(rect.right(), grid.top()));
        let th = self.playlist.track_h;
        let ntracks = self.song.tracks.len();

        if resp.hovered() {
            // Shift and the wheel over a track name moves the track; with Alt as well, over a clip, it nudges the clip
            if let Some(pp) = ui.input(|i| i.pointer.hover_pos()) {
                if heads.contains(pp) {
                    let tr = (((pp.y - grid.top() + self.playlist.scroll_y) / th).floor().max(0.0) as usize).min(ntracks.saturating_sub(1));
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
        self.playlist.scroll_x = self.playlist.scroll_x.max(0.0);
        self.playlist.scroll_y = self.playlist.scroll_y.clamp(0.0, (ntracks as f32 * th - grid.height()).max(0.0));
        let (sx, sy) = (self.playlist.scroll_x, self.playlist.scroll_y);
        let x_of = |tick: f32| grid.left() + tick * px - sx;
        let tick_of = |x: f32| ((x - grid.left() + sx) / px).max(0.0);
        let y_of = |track: usize| grid.top() + track as f32 * th - sy;
        let track_of = |y: f32| (((y - grid.top() + sy) / th).floor().max(0.0) as usize).min(ntracks.saturating_sub(1));

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
            if (n / 4) % 2 == 1 {
                p.rect_filled(Rect::from_min_max(Pos2::new(x.max(grid.left()), grid.top()), Pos2::new((x + len as f32 * px).max(grid.left()), grid.bottom())), 0.0, grid_light);
            }
            if n % every == 0 && x >= grid.left() {
                p.line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
            }
        }
        for tr in first_track..=last_track {
            let y = y_of(tr);
            p.line_segment([Pos2::new(grid.left(), y), Pos2::new(grid.right(), y)], Stroke::new(1.0_f32, LINE_SOFT));
        }

        // clips
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut hit: Option<(usize, i8)> = None;
        // the automation clip whose picture the pointer is over: clip, channel, picture, left edge, offset
        let mut auto_under: Option<(usize, usize, Rect, f32, u32)> = None;
        let gp = ui.painter_at(grid);
        let font = FontId::proportional(10.5);
        for (i, c) in self.song.clips.iter().enumerate() {
            let tr = c.track as usize;
            if tr < first_track || tr > last_track {
                continue;
            }
            let r = Rect::from_min_max(Pos2::new(x_of(c.pos as f32), y_of(tr) + 1.0), Pos2::new(x_of((c.pos + c.len) as f32) - 1.0, y_of(tr) + th));
            if r.right() < grid.left() || r.left() > grid.right() {
                continue;
            }
            let (name, col) = self.clip_look(c);
            let enabled = !c.muted && self.song.tracks.get(tr).map(|t| t.enabled).unwrap_or(true);
            let col = if enabled { col } else { col.gamma_multiply(0.35) };
            gp.rect_filled(r, 2.0, col.gamma_multiply(0.55));
            let title = Rect::from_min_max(r.min, Pos2::new(r.right(), (r.top() + 12.0).min(r.bottom())));
            gp.rect_filled(title, 2.0, col);
            let selected = self.playlist.sel.contains(&i);
            gp.rect_stroke(r, 2.0, Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, if selected { Color32::from_rgb(235, 96, 88) } else { LINE }), egui::StrokeKind::Inside);
            if r.width() > 16.0 {
                gp.with_clip_rect(r.intersect(grid)).text(title.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, &name, font.clone(), Color32::from_gray(245));
            }
            // a picture of what is inside
            let body = Rect::from_min_max(Pos2::new(r.left(), title.bottom()), r.max);
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
                        if let Some(smp) = self.song.channels.get(ch).and_then(|c| c.sample.as_ref()) {
                            // frames per tick at the song tempo
                            let fpt = smp.rate as f32 * 60.0 / self.song.tempo.max(1.0) / ppq;
                            let cp = gp.with_clip_rect(body.intersect(grid));
                            let x_from = r.left().max(grid.left()) as i32;
                            let x_to = r.right().min(grid.right()) as i32;
                            let mid = body.center().y;
                            for x in x_from..x_to {
                                let t0 = (x as f32 - r.left()) / px + c.offset as f32;
                                let a = (t0 * fpt) as usize;
                                let b = (((t0 + 1.0 / px) * fpt) as usize).min(smp.frames.len());
                                if a >= b {
                                    break;
                                }
                                let stride = ((b - a) / 32).max(1);
                                let peak = smp.frames[a..b].iter().step_by(stride).fold(0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                                let h = (peak * body.height() * 0.5).max(0.5);
                                cp.line_segment([Pos2::new(x as f32, mid - h), Pos2::new(x as f32, mid + h)], Stroke::new(1.0_f32, Color32::from_gray(240)));
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
                    let edge = if r.width() < 12.0 { 0 } else if pp.x > r.right() - 5.0 { 1 } else if pp.x < r.left() + 4.0 { -1 } else { 0 };
                    hit = Some((i, edge));
                }
            }
        }

        // play head
        if self.handle.transport.song_mode() {
            let x = x_of(self.handle.transport.tick() as f32);
            if x >= grid.left() && x <= grid.right() {
                p.line_segment([Pos2::new(x, ruler.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, GREEN));
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
            let r = Rect::from_min_max(Pos2::new(heads.left(), y + 1.0), Pos2::new(heads.right() - 1.0, y + th));
            let t = &self.song.tracks[tr];
            hp.rect_filled(r, 2.0, rgb(t.color).gamma_multiply(if t.enabled { 1.0 } else { 0.5 }));
            let name = if t.name.is_empty() { format!("Track {}", tr + 1) } else { t.name.clone() };
            hp.text(r.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, fit(ui, &name, r.width() - 24.0, &font), font.clone(), TEXT);
            let dot = Pos2::new(r.right() - 9.0, r.center().y);
            hp.circle_filled(dot, 4.0, if t.enabled { GREEN } else { PANEL_DARK });
            hp.circle_stroke(dot, 4.0, Stroke::new(1.0_f32, LINE));
            if clicked && pointer.map(|pp| r.contains(pp)).unwrap_or(false) {
                toggle = Some(tr);
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
        let snap = |t: f32| ((t.max(0.0) as u32) / g) * g;
        let snap_delta = |d: f32| ((d / g as f32).round() as i64) * g as i64;
        // a sound dragged from the browser becomes an audio clip where it is dropped
        if let Some(path) = resp.dnd_release_payload::<std::path::PathBuf>() {
            if let Some(pp) = pointer.filter(|pp| grid.contains(*pp)) {
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !["fst", "fsc", "mid", "flp", "gmsynth", "tbio", "mrp"].contains(&ext.as_str()) {
                    self.add_audio_clip(&path, snap(tick_of(pp.x)), Some(track_of(pp.y) as u16));
                }
            }
        }
        let tool = self.playlist.tool;
        let over = pointer.filter(|pp| resp.hovered() && rect.contains(*pp));
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
                                let (src, len, offset) = self.playlist.brush.unwrap_or((ClipSrc::Pattern(pat), self.song.pattern_length(pat), 0));
                                let c = Clip { pos: snap(tick_of(pp.x)), len, track: track as u16, src, offset, muted: false, group: 0 };
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
                            self.playlist.drag = Some(Drag::Slice { tick: (((t / g as f32).round() as u32) * g).max(1) });
                        }
                        Tool::Select => self.playlist.drag = Some(Drag::Rect { from: pp, add: mods.shift, zoom: false }),
                        Tool::Zoom => self.playlist.drag = Some(Drag::Rect { from: pp, add: false, zoom: true }),
                        Tool::Playback => self.playlist.drag = Some(Drag::Seek),
                    }
                }
            } else if sec_pressed && self.playlist.drag.is_none() && grid.contains(pp) {
                match tool {
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
        let mut cut: Option<(usize, u32)> = None;
        if let (Some(pp), Some(drag)) = (pointer, self.playlist.drag.as_ref()) {
            match drag {
                Drag::Clips { mode, anchor_tick, anchor_track, orig } => {
                    if down {
                        let dt = snap_delta(tick_of(pp.x) - anchor_tick);
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
                Drag::Slice { tick } => {
                    if down {
                        let x = x_of(*tick as f32);
                        ui.painter_at(grid).line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, Color32::WHITE));
                        if let Some((i, _)) = hit {
                            cut = Some((i, *tick));
                        }
                    } else {
                        done = true;
                    }
                }
                Drag::Paint => {
                    if down && grid.contains(pp) {
                        let (src, len, offset) = self.playlist.brush.unwrap_or((ClipSrc::Pattern(pat), self.song.pattern_length(pat), 0));
                        let unit = len.max(g).max(1);
                        let pos = snap(tick_of(pp.x)) / unit * unit;
                        let track = track_of(pp.y) as u16;
                        if !self.song.clips.iter().any(|c| c.track == track && c.pos < pos + len && pos < c.pos + c.len) {
                            self.song.clips.push(Clip { pos, len, track, src, offset, muted: false, group: 0 });
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
                        self.handle.transport.seek(snap(tick_of(pp.x)));
                    } else {
                        done = true;
                    }
                }
            }
        } else if self.playlist.drag.is_some() && !down && !sec_down {
            done = true;
        }
        if let Some((i, at)) = cut {
            self.pl_slice_clip(i, at);
        }
        if done {
            self.playlist.drag = None;
        }
        self.auto_point_menu(ui.ctx());
    }

    fn pl_snap(&self) -> u32 {
        let (_, unit, div) = SNAPS[self.playlist.snap.min(SNAPS.len() - 1)];
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
        if m.command && m.alt {
            match key {
                Key::C if !repeat => self.pl_consolidate(m.shift),
                Key::G if !repeat => self.grid_win = !self.grid_win,
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
                Key::B => self.pl_duplicate(),
                Key::Insert => self.pl_space(true),
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
            ui.menu_button("Tools", |ui| {
                let mut act: Option<fn(&mut App)> = None;
                let items: [(&str, fn(&mut App)); 7] = [
                    ("Select all                  Ctrl+A", |a| a.playlist.sel = (0..a.song.clips.len()).collect()),
                    ("Deselect                    Ctrl+D", |a| a.playlist.sel.clear()),
                    ("Delete                      Del", |a| a.pl_delete_selected()),
                    ("Duplicate                   Ctrl+B", |a| a.pl_duplicate()),
                    ("Paste                       Ctrl+V", |a| a.pl_paste()),
                    ("Mute                        Alt+M", |a| a.pl_mute(true)),
                    ("Unmute                      Shift+Alt+M", |a| a.pl_mute(false)),
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
                if ui.selectable_label(self.playlist.tool == t, RichText::new(label).small()).on_hover_text(hint).clicked() {
                    self.playlist.tool = t;
                }
            }
            if ui.selectable_label(self.playlist.stretch, RichText::new("Stretch").small()).on_hover_text("Stretch mode: dragging the end of an audio clip fits its sound to the new length (Shift+M)").clicked() {
                self.playlist.stretch = !self.playlist.stretch;
            }
            ui.separator();
            egui::ComboBox::from_id_salt("pl_snap").width(74.0).selected_text(SNAPS[self.playlist.snap].0).show_ui(ui, |ui| {
                for (i, (label, _, _)) in SNAPS.iter().enumerate() {
                    ui.selectable_value(&mut self.playlist.snap, i, *label);
                }
            })
            .response
            .on_hover_text("Snap");
            ui.separator();
            let brush = match self.playlist.brush {
                Some((ClipSrc::Pattern(p), _, _)) => self.song.pattern_name(p),
                Some((ClipSrc::Audio(c), _, _)) | Some((ClipSrc::Automation(c), _, _)) => self.song.channels.get(c).map(|c| c.name.clone()).unwrap_or_default(),
                None => self.song.pattern_name(pat),
            };
            ui.label(RichText::new(format!("Arrangement  -  {brush}")).color(TEXT));
        });
    }
}
