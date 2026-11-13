//! The event editor: the recorded movements of one control inside the current pattern, drawn as a graph.
//!
//! Left drag draws values, right drag draws a straight line between where it starts and ends, and
//! Shift+right drag wipes the events it passes over. The control is picked from the list at the top: those the
//! pattern already moves, then the usual ones of the selected channel and mixer track.

use egui::{Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::{PatEvent, Target};

use crate::widgets::*;
use crate::App;

#[derive(Default)]
pub struct State {
    /// The control shown: its number and destination as the file writes them.
    pub target: Option<(u16, u16)>,
    /// A control asked for from elsewhere; it is on offer even when the lists below would not name it.
    pub wanted: Option<(u16, u16)>,
    /// Where a line drag began: tick and raw value.
    line: Option<(u32, i32)>,
    /// The last point a free-hand drag wrote, so fast strokes leave no holes.
    last: Option<(u32, i32)>,
    /// The events last copied: tick and raw value.
    copied: Vec<(u32, i32)>,
    /// The LFO or scale tool while its box is open.
    tool: Option<Tool>,
}

/// One of the two tools at work: what the pattern held before it, and its settings.
pub struct Tool {
    lfo: bool,
    before: Vec<PatEvent>,
    /// Sine, triangle, square.
    shape: u8,
    /// Value and range as 0..=1 of the travel, and the length of one cycle in beats: at the start and at the end.
    start: [f32; 3],
    end: [f32; 3],
    end_on: bool,
    phase: f32,
    mul: f32,
    offset: f32,
    center: f32,
}

impl Tool {
    fn new(lfo: bool, before: Vec<PatEvent>) -> Self {
        Tool { lfo, before, shape: 0, start: [0.5, 0.5, 4.0], end: [0.5, 0.5, 4.0], end_on: false, phase: 0.0, mul: 1.0, offset: 0.0, center: 0.5 }
    }

    /// The pattern's events with the tool applied to one control.
    fn apply(&self, param: u16, dest: u16, lo: i32, hi: i32, total: u32, snap: u32, ppq: u32) -> Vec<PatEvent> {
        let span = (hi - lo).max(1) as f32;
        let raw_of = |v: f32| lo + (v.clamp(0.0, 1.0) * span).round() as i32;
        let mut out: Vec<PatEvent> = self.before.iter().filter(|e| (e.param, e.dest) != (param, dest)).copied().collect();
        if self.lfo {
            let mut turn = self.phase;
            let mut t = 0;
            while t < total {
                let f = if self.end_on { t as f32 / total.max(1) as f32 } else { 0.0 };
                let at = |k: usize| self.start[k] + (self.end[k] - self.start[k]) * f;
                let x = turn.fract();
                let wave = match self.shape {
                    0 => (x * std::f32::consts::TAU).sin(),
                    1 => 4.0 * ((x + 0.75).fract() - 0.5).abs() - 1.0,
                    _ => {
                        if x < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                };
                out.push(PatEvent { tick: t, param, dest, raw: raw_of(at(0) + at(1) * 0.5 * wave) });
                turn += snap as f32 / (at(2).max(0.01) * ppq.max(1) as f32);
                t += snap;
            }
        } else {
            for e in self.before.iter().filter(|e| (e.param, e.dest) == (param, dest)) {
                let v = (e.raw - lo) as f32 / span;
                out.push(PatEvent { raw: raw_of((v - self.center) * self.mul + self.center + self.offset), ..*e });
            }
        }
        out.sort_by_key(|e| e.tick);
        out
    }
}

impl App {
    /// The controls on offer: the ones this pattern already has events for, then the selected channel's and
    /// mixer track's own.
    fn event_targets(&self, pat: usize) -> Vec<(u16, u16)> {
        let mut out: Vec<(u16, u16)> = vec![];
        let mut add = |a: (u16, u16)| {
            if !out.contains(&a) {
                out.push(a);
            }
        };
        if let Some(w) = self.events.wanted {
            add(w);
        }
        for e in &self.song.patterns[pat].events {
            add((e.param, e.dest));
        }
        let c = self.sel_chan;
        if c < self.song.channels.len() {
            for t in [Target::ChanVolume(c), Target::ChanPan(c), Target::ChanPitch(c)] {
                add(self.song.event_address(t));
            }
            if let Some(h) = &self.song.channels[c].ext {
                for k in 0..h.0.param_count().min(256) {
                    add(self.song.event_address(Target::ChanParam(c, k as u16)));
                }
            }
        }
        let i = self.sel_insert;
        if let Some(ins) = self.song.inserts.get(i) {
            for t in [Target::InsertVolume(i), Target::InsertPan(i)] {
                add(self.song.event_address(t));
            }
            for s in &ins.slots {
                add(self.song.event_address(Target::SlotMix(i, s.index)));
                if let Some(fx) = &s.fx {
                    let count = fx.ext.as_ref().map(|h| h.0.param_count().min(256)).unwrap_or(cereal_engine::fx::def(fx.kind).params.len());
                    for k in 0..count {
                        add(self.song.event_address(Target::SlotParam(i, s.index, k as u16)));
                    }
                }
            }
        }
        add(self.song.event_address(Target::Tempo));
        out
    }

    /// Ctrl+I: write where the shown control stands now as an event at the play position of the pattern.
    pub(crate) fn events_insert_now(&mut self) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let Some((param, dest)) = self.events.target else { return };
        let Some(raw) = self.song.event_raw(self.song.event_target(param, dest)) else { return };
        let total = self.song.pattern_length(pat).max(self.song.bar().max(1));
        let tick = if self.handle.transport.song_mode() { 0 } else { (self.handle.transport.tick().max(0.0) as u32) % total };
        let ev = &mut self.song.patterns[pat].events;
        ev.retain(|e| (e.param, e.dest, e.tick) != (param, dest, tick));
        ev.push(PatEvent { tick, param, dest, raw });
        ev.sort_by_key(|e| e.tick);
        self.dirty = true;
    }

    /// Ctrl+C and Ctrl+X: the shown control's events of this pattern.
    pub(crate) fn events_copy(&mut self, cut: bool) {
        let pat = self.handle.transport.pattern();
        let Some((param, dest)) = self.events.target else { return };
        let Some(p) = self.song.patterns.get_mut(pat) else { return };
        self.events.copied = p.events.iter().filter(|e| (e.param, e.dest) == (param, dest)).map(|e| (e.tick, e.raw)).collect();
        if cut && !self.events.copied.is_empty() {
            p.events.retain(|e| (e.param, e.dest) != (param, dest));
            self.dirty = true;
        }
        self.status = format!("{} events {}", self.events.copied.len(), if cut { "cut" } else { "copied" });
    }

    /// Ctrl+V: the copied events onto the shown control, in place of what it had.
    pub(crate) fn events_paste(&mut self) {
        let pat = self.handle.transport.pattern();
        let Some((param, dest)) = self.events.target else { return };
        if self.events.copied.is_empty() {
            return;
        }
        self.song.ensure_pattern(pat);
        let range = self.song.event_range(self.song.event_target(param, dest));
        let ev = &mut self.song.patterns[pat].events;
        ev.retain(|e| (e.param, e.dest) != (param, dest));
        for &(tick, raw) in &self.events.copied {
            ev.push(PatEvent { tick, param, dest, raw: range.map(|(lo, hi)| raw.clamp(lo, hi)).unwrap_or(raw) });
        }
        ev.sort_by_key(|e| e.tick);
        self.dirty = true;
    }

    /// The shown control's events as an automation clip: one held step per event, laid where the pattern first
    /// plays in the song.
    fn events_to_clip(&mut self, pat: usize, param: u16, dest: u16, lo: i32, hi: i32) {
        let mine: Vec<PatEvent> = self.song.patterns[pat].events.iter().filter(|e| (e.param, e.dest) == (param, dest)).copied().collect();
        if mine.is_empty() {
            self.status = "No events to turn into a clip".into();
            return;
        }
        let total = self.song.pattern_length(pat).max(self.song.bar().max(1));
        let val = |raw: i32| ((raw - lo) as f32 / (hi - lo).max(1) as f32).clamp(0.0, 1.0);
        let mut points = vec![];
        if mine[0].tick > 0 {
            points.push(cereal_engine::song::AutoPt { tick: 0.0, value: val(mine[0].raw), tension: 0.0, mode: 2 });
        }
        for e in &mine {
            points.push(cereal_engine::song::AutoPt { tick: e.tick as f64, value: val(e.raw), tension: 0.0, mode: 2 });
        }
        if mine.last().is_some_and(|e| e.tick < total) {
            points.push(cereal_engine::song::AutoPt { tick: total as f64, value: val(mine[mine.len() - 1].raw), tension: 0.0, mode: 2 });
        }
        let pos = self.song.clips.iter().filter(|c| c.src == cereal_engine::song::ClipSrc::Pattern(pat)).map(|c| c.pos).min().unwrap_or(0);
        let target = self.song.event_target(param, dest);
        self.add_auto_clip(target, pos, total, points);
        self.song.patterns[pat].events.retain(|e| (e.param, e.dest) != (param, dest));
        self.status = format!("Events of {} turned into an automation clip", self.song.target_name(target));
    }

    /// The LFO and scale boxes: what they show is written to the pattern as it is set, Accept keeps it and
    /// Cancel puts the old events back.
    #[allow(clippy::too_many_arguments)]
    fn events_tool(&mut self, ctx: &egui::Context, open: Option<bool>, pat: usize, param: u16, dest: u16, lo: i32, hi: i32, total: u32) {
        if let Some(lfo) = open {
            if let Some(old) = self.events.tool.take() {
                self.song.patterns[pat].events = old.before;
            }
            self.events.tool = Some(Tool::new(lfo, self.song.patterns[pat].events.clone()));
        }
        let Some(mut tool) = self.events.tool.take() else { return };
        let (mut keep, mut drop) = (false, false);
        egui::Window::new(if tool.lfo { "Event LFO" } else { "Scale event levels" }).collapsible(false).resizable(false).show(ctx, |ui| {
            if tool.lfo {
                ui.horizontal(|ui| {
                    ui.label("Shape");
                    for (k, name) in ["Sine", "Triangle", "Square"].iter().enumerate() {
                        ui.selectable_value(&mut tool.shape, k as u8, *name);
                    }
                });
                egui::Grid::new("ev_lfo").show(ui, |ui| {
                    ui.label("");
                    ui.label("Start");
                    ui.checkbox(&mut tool.end_on, "End");
                    ui.end_row();
                    for (k, name, top) in [(0, "Value", 1.0), (1, "Range", 1.0), (2, "Speed (beats)", 64.0)] {
                        ui.label(name);
                        let lowest = if k == 2 { 0.0625 } else { 0.0 };
                        ui.add(egui::Slider::new(&mut tool.start[k], lowest..=top).logarithmic(k == 2));
                        ui.add_enabled(tool.end_on, egui::Slider::new(&mut tool.end[k], lowest..=top).logarithmic(k == 2));
                        ui.end_row();
                    }
                    ui.label("Phase");
                    ui.add(egui::Slider::new(&mut tool.phase, 0.0..=1.0));
                    ui.end_row();
                });
            } else {
                egui::Grid::new("ev_scale").show(ui, |ui| {
                    ui.label("Multiply");
                    ui.add(egui::Slider::new(&mut tool.mul, -2.0..=2.0));
                    ui.end_row();
                    ui.label("Offset");
                    ui.add(egui::Slider::new(&mut tool.offset, -1.0..=1.0));
                    ui.end_row();
                    ui.label("Center");
                    ui.add(egui::Slider::new(&mut tool.center, 0.0..=1.0));
                    ui.end_row();
                });
            }
            ui.horizontal(|ui| {
                keep = ui.button("Accept").clicked();
                drop = ui.button("Cancel").clicked();
            });
        });
        if drop || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.song.patterns[pat].events = tool.before;
            return;
        }
        let snap = (self.song.step() / 4).max(1);
        self.song.patterns[pat].events = tool.apply(param, dest, lo, hi, total, snap, self.song.ppq);
        self.dirty = true;
        if !keep {
            self.events.tool = Some(tool);
        }
    }

    pub(crate) fn events_ui(&mut self, ui: &mut Ui) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let targets = self.event_targets(pat);
        if self.events.target.is_none_or(|t| !targets.contains(&t)) {
            self.events.target = targets.first().copied();
        }
        let Some((param, dest)) = self.events.target else { return };
        let target = self.song.event_target(param, dest);
        let range = self.song.event_range(target);
        // what the file's values run between: the control's own travel when we know it, else what is there
        let (lo, hi) = range.unwrap_or_else(|| {
            let vals = self.song.patterns[pat].events.iter().filter(|e| (e.param, e.dest) == (param, dest)).map(|e| e.raw);
            let (a, b) = (vals.clone().min().unwrap_or(0), vals.max().unwrap_or(1));
            (a.min(0), b.max(a + 1))
        });
        let focused = self.focus == crate::Win::Events && !ui.ctx().wants_keyboard_input();
        let (key_lfo, key_scale, key_del) = ui.input(|i| {
            (focused && i.modifiers.alt && i.key_pressed(egui::Key::O), focused && i.modifiers.alt && i.key_pressed(egui::Key::X), focused && i.key_pressed(egui::Key::Delete))
        });
        let mut open_tool = if key_lfo { Some(true) } else if key_scale { Some(false) } else { None };
        if key_del {
            self.song.patterns[pat].events.retain(|e| (e.param, e.dest) != (param, dest));
            self.dirty = true;
        }

        ui.horizontal(|ui| {
            ui.menu_button("Edit", |ui| {
                if ui.button("Cut   Ctrl+X").clicked() {
                    self.events_copy(true);
                    ui.close_menu();
                }
                if ui.button("Copy   Ctrl+C").clicked() {
                    self.events_copy(false);
                    ui.close_menu();
                }
                if ui.add_enabled(!self.events.copied.is_empty(), egui::Button::new("Paste   Ctrl+V")).clicked() {
                    self.events_paste();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Insert current control value   Ctrl+I").clicked() {
                    self.events_insert_now();
                    ui.close_menu();
                }
                if ui.button("Turn into automation clip").clicked() {
                    self.events_to_clip(pat, param, dest, lo, hi);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Delete all events of this control   Del").clicked() {
                    self.song.patterns[pat].events.retain(|e| (e.param, e.dest) != (param, dest));
                    self.dirty = true;
                    ui.close_menu();
                }
                if ui.button("Delete every event in the pattern").clicked() {
                    self.song.patterns[pat].events.clear();
                    self.dirty = true;
                    ui.close_menu();
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui.button("LFO...   Alt+O").clicked() {
                    open_tool = Some(true);
                    ui.close_menu();
                }
                if ui.button("Scale levels...   Alt+X").clicked() {
                    open_tool = Some(false);
                    ui.close_menu();
                }
            });
            egui::ComboBox::from_id_salt("ev_target").width(300.0).selected_text(self.song.target_name(target)).show_ui(ui, |ui| {
                for t in &targets {
                    let name = self.song.target_name(self.song.event_target(t.0, t.1));
                    let has = self.song.patterns[pat].events.iter().any(|e| (e.param, e.dest) == *t);
                    ui.selectable_value(&mut self.events.target, Some(*t), if has { format!("{name}  *") } else { name });
                }
            });
            ui.label(RichText::new(self.song.pattern_name(pat)).color(TEXT));
        });

        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(Vec2::new(300.0, 140.0)), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, PANEL_DARK);
        let bar = self.song.bar().max(1);
        let total = self.song.pattern_length(pat).max(bar);
        self.events_tool(ui.ctx(), open_tool, pat, param, dest, lo, hi, total);
        let px = rect.width() / total as f32;
        let x_of = |t: u32| rect.left() + t as f32 * px;
        let y_of = |v: i32| rect.bottom() - 2.0 - (v - lo) as f32 / (hi - lo).max(1) as f32 * (rect.height() - 4.0);
        let tick_of = |x: f32| (((x - rect.left()) / px).max(0.0) as u32).min(total.saturating_sub(1));
        let val_of = |y: f32| lo + (((rect.bottom() - 2.0 - y) / (rect.height() - 4.0)).clamp(0.0, 1.0) * (hi - lo) as f32).round() as i32;

        // beats and bars
        let beat = self.song.ppq.max(1);
        let mut t = 0;
        while t <= total {
            let strong = t % bar == 0;
            if strong || beat as f32 * px > 6.0 {
                p.line_segment([Pos2::new(x_of(t), rect.top()), Pos2::new(x_of(t), rect.bottom())], Stroke::new(1.0_f32, if strong { LINE } else { LINE_SOFT }));
            }
            if strong {
                p.text(Pos2::new(x_of(t) + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("{}", t / bar + 1), FontId::proportional(9.5), TEXT_DIM);
            }
            t += beat;
        }

        // the events as steps: each value holds until the next
        let mine: Vec<PatEvent> = self.song.patterns[pat].events.iter().filter(|e| (e.param, e.dest) == (param, dest)).copied().collect();
        for (k, e) in mine.iter().enumerate() {
            let x0 = x_of(e.tick);
            let x1 = mine.get(k + 1).map(|n| x_of(n.tick)).unwrap_or(rect.right()).max(x0 + 1.0);
            let y = y_of(e.raw);
            p.rect_filled(Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, rect.bottom())), 0.0, NOTE.gamma_multiply(0.35));
            p.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.5_f32, NOTE));
        }
        if mine.is_empty() {
            p.text(rect.center(), Align2::CENTER_CENTER, "No events for this control here. Drag to draw some.", FontId::proportional(11.0), TEXT_DIM);
        }

        // drawing
        let snap = (self.song.step() / 4).max(1);
        let (pointer, down, sec_down, shift) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.primary_down(), i.pointer.secondary_down(), i.modifiers.shift));
        let write = |events: &mut Vec<PatEvent>, from: (u32, i32), to: (u32, i32)| {
            // a run of events along the line between two points, one per snap cell
            let (a, b) = if from.0 <= to.0 { (from, to) } else { (to, from) };
            let (t0, t1) = (a.0 / snap * snap, b.0 / snap * snap);
            events.retain(|e| (e.param, e.dest) != (param, dest) || e.tick < t0 || e.tick >= t1 + snap);
            let mut t = t0;
            while t <= t1 {
                let f = if t1 == t0 { 1.0 } else { (t - t0) as f32 / (t1 - t0) as f32 };
                events.push(PatEvent { tick: t, param, dest, raw: (a.1 as f32 + (b.1 - a.1) as f32 * f).round() as i32 });
                t += snap;
            }
            events.sort_by_key(|e| e.tick);
        };
        if let Some(pp) = pointer.filter(|pp| rect.contains(*pp) || resp.dragged()) {
            let here = (tick_of(pp.x), val_of(pp.y));
            if resp.hovered() || resp.dragged() {
                p.text(rect.right_top() + Vec2::new(-4.0, 2.0), Align2::RIGHT_TOP, format!("{}", here.1), FontId::monospace(10.0), TEXT);
            }
            if down && (resp.dragged() || resp.is_pointer_button_down_on()) {
                let from = self.events.last.unwrap_or(here);
                write(&mut self.song.patterns[pat].events, from, here);
                self.events.last = Some(here);
                self.dirty = true;
            } else if sec_down && shift && rect.contains(pp) {
                let t0 = here.0 / snap * snap;
                let before = self.song.patterns[pat].events.len();
                self.song.patterns[pat].events.retain(|e| (e.param, e.dest) != (param, dest) || e.tick < t0 || e.tick >= t0 + snap);
                self.dirty |= self.song.patterns[pat].events.len() != before;
            } else if sec_down && rect.contains(pp) {
                let from = *self.events.line.get_or_insert(here);
                p.line_segment([Pos2::new(x_of(from.0), y_of(from.1)), Pos2::new(x_of(here.0), y_of(here.1))], Stroke::new(1.5_f32, ACCENT));
            } else if let Some(from) = self.events.line.take() {
                write(&mut self.song.patterns[pat].events, from, here);
                self.dirty = true;
            }
        }
        if !down {
            self.events.last = None;
        }
        if !sec_down && pointer.is_none() {
            self.events.line = None;
        }
    }
}
