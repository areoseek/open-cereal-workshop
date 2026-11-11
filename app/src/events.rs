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

        ui.horizontal(|ui| {
            ui.menu_button("Edit", |ui| {
                if ui.button("Delete all events of this control").clicked() {
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
        // what the file's values run between: the control's own travel when we know it, else what is there
        let (lo, hi) = range.unwrap_or_else(|| {
            let vals = self.song.patterns[pat].events.iter().filter(|e| (e.param, e.dest) == (param, dest)).map(|e| e.raw);
            let (a, b) = (vals.clone().min().unwrap_or(0), vals.max().unwrap_or(1));
            (a.min(0), b.max(a + 1))
        });
        let bar = self.song.bar().max(1);
        let total = self.song.pattern_length(pat).max(bar);
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
