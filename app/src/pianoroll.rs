//! The piano roll: notes of the selected channel in the current pattern.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::Note;
use cereal_engine::Cmd;

use crate::widgets::*;
use crate::App;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Move,
    Resize,
}

struct Drag {
    idx: usize,
    mode: Mode,
    grab_dt: f32,
}

pub struct State {
    scroll_x: f32,
    scroll_y: f32,
    /// Pixels per tick at 96 ppq; scaled for other resolutions.
    zoom: f32,
    last_len: u32,
    drag: Option<Drag>,
    sounding: Option<(u16, u8)>,
}

impl Default for State {
    fn default() -> Self {
        Self { scroll_x: 0.0, scroll_y: (131.0 - 72.0) * KEY_H, zoom: 0.5, last_len: 0, drag: None, sounding: None }
    }
}

const KEY_H: f32 = 14.0;
const KEYS_W: f32 = 64.0;
const RULER_H: f32 = 18.0;
const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

fn is_black(key: i32) -> bool {
    matches!(key.rem_euclid(12), 1 | 3 | 6 | 8 | 10)
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

    pub(crate) fn piano_ui(&mut self, ui: &mut Ui) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let chan = self.sel_chan as u16;
        let ppq = self.song.ppq as f32;
        let step = self.song.step();
        let bar = self.song.bar();
        if self.piano.last_len == 0 {
            self.piano.last_len = step;
        }
        let name = self.song.channels.get(self.sel_chan).map(|c| c.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{}  -  {}", self.song.pattern_name(pat), name)).color(TEXT));
            ui.label(egui::RichText::new("   left-click: draw / move, drag the right edge: resize, right-click: delete, Ctrl+wheel: zoom, Alt: no snap").color(TEXT_DIM).small());
        });

        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(Vec2::new(200.0, 120.0)), Sense::click_and_drag());
        let grid = Rect::from_min_max(rect.min + Vec2::new(KEYS_W, RULER_H), rect.max);
        let keys = Rect::from_min_max(Pos2::new(rect.left(), grid.top()), Pos2::new(grid.left(), rect.bottom()));
        let ruler = Rect::from_min_max(Pos2::new(grid.left(), rect.top()), Pos2::new(rect.right(), grid.top()));
        let px = self.piano.zoom * 96.0 / ppq; // pixels per tick

        // scrolling and zoom
        if resp.hovered() {
            let (scroll, zoom, mods) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
            if zoom != 1.0 {
                self.piano.zoom = (self.piano.zoom * zoom).clamp(0.05, 8.0);
            } else if mods.shift {
                self.piano.scroll_x -= scroll.y + scroll.x;
            } else {
                self.piano.scroll_y -= scroll.y;
                self.piano.scroll_x -= scroll.x;
            }
        }
        self.piano.scroll_x = self.piano.scroll_x.max(0.0);
        self.piano.scroll_y = self.piano.scroll_y.clamp(0.0, (132.0 * KEY_H - grid.height()).max(0.0));
        let (sx, sy) = (self.piano.scroll_x, self.piano.scroll_y);

        let x_of = |tick: f32| grid.left() + tick * px - sx;
        let tick_of = |x: f32| ((x - grid.left() + sx) / px).max(0.0);
        let y_of = |key: i32| grid.top() + (131 - key) as f32 * KEY_H - sy;
        let key_of = |y: f32| (131 - ((y - grid.top() + sy) / KEY_H).floor() as i32).clamp(0, 131);

        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, GRID_DARK);

        // rows
        let top_key = key_of(grid.top());
        let bottom_key = key_of(grid.bottom());
        for key in bottom_key..=top_key {
            let y = y_of(key);
            let row = Rect::from_min_max(Pos2::new(grid.left(), y), Pos2::new(grid.right(), y + KEY_H));
            if !is_black(key) {
                p.rect_filled(row, 0.0, GRID_LIGHT);
            }
            p.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0_f32, if key % 12 == 11 { LINE } else { LINE_SOFT }));
        }
        // columns
        let first = (tick_of(grid.left()) as u32 / step) * step;
        let last = tick_of(grid.right()) as u32 + step;
        let step_px = step as f32 * px;
        let mut t = first;
        while t <= last {
            let x = x_of(t as f32);
            let on_bar = t % bar == 0;
            let on_beat = t % self.song.ppq == 0;
            if on_bar || on_beat || step_px >= 6.0 {
                let col = if on_bar { LINE } else if on_beat { LINE_SOFT } else { LINE_SOFT.gamma_multiply(0.6) };
                p.line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, col));
            }
            t += step;
        }

        // notes
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut hit: Option<(usize, bool)> = None;
        let gp = ui.painter_at(grid);
        for (i, n) in self.song.patterns[pat].notes.iter().enumerate() {
            if n.chan != chan {
                continue;
            }
            let len = if n.len == 0 { step } else { n.len };
            let r = Rect::from_min_max(Pos2::new(x_of(n.pos as f32), y_of(n.key as i32) + 1.0), Pos2::new(x_of((n.pos + len) as f32) - 1.0, y_of(n.key as i32) + KEY_H));
            if r.right() < grid.left() || r.left() > grid.right() {
                continue;
            }
            let dragging = self.piano.drag.as_ref().map(|d| d.idx == i).unwrap_or(false);
            let base = if n.muted { TEXT_DIM } else { NOTE.gamma_multiply(0.55 + 0.45 * n.vel.clamp(0.0, 1.28) / 1.28) };
            gp.rect_filled(r, 2.0, if dragging { base.lerp_to_gamma(Color32::WHITE, 0.3) } else { base });
            gp.rect_stroke(r, 2.0, Stroke::new(1.0_f32, LINE), egui::StrokeKind::Inside);
            if r.width() > 24.0 {
                gp.text(r.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, format!("{}{}", NAMES[n.key as usize % 12], n.key / 12), FontId::proportional(9.5), LINE);
            }
            if let Some(pp) = pointer {
                if r.contains(pp) {
                    hit = Some((i, pp.x > r.right() - 5.0));
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
        let mut b = (tick_of(grid.left()) as u32 / bar) * bar;
        while b <= last {
            let x = x_of(b as f32);
            if x >= grid.left() - 1.0 {
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

        // interaction
        let (pressed, down, released, sec_down, alt) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.secondary_down(), i.modifiers.alt));
        let snap = |t: f32| if alt { t.round().max(0.0) as u32 } else { ((t / step as f32).floor().max(0.0) as u32) * step };
        if let Some(pp) = pointer.filter(|_| resp.hovered() || self.piano.drag.is_some()) {
            if ruler.contains(pp) && down && !self.handle.transport.song_mode() {
                self.handle.transport.seek(snap(tick_of(pp.x)));
            } else if keys.contains(pp) && down {
                self.pr_sound(Some(key_of(pp.y) as u8));
            } else if grid.contains(pp) || self.piano.drag.is_some() {
                let key = key_of(pp.y) as u8;
                if sec_down {
                    if let Some((i, _)) = hit {
                        self.song.patterns[pat].notes.remove(i);
                        self.piano.drag = None;
                        self.dirty = true;
                    }
                } else if pressed && grid.contains(pp) {
                    match hit {
                        Some((i, edge)) => {
                            let n = &self.song.patterns[pat].notes[i];
                            self.piano.last_len = if n.len == 0 { step } else { n.len };
                            self.piano.drag = Some(Drag { idx: i, mode: if edge { Mode::Resize } else { Mode::Move }, grab_dt: tick_of(pp.x) - n.pos as f32 });
                            let k = n.key;
                            self.pr_sound(Some(k));
                        }
                        None => {
                            let pos = snap(tick_of(pp.x));
                            self.song.patterns[pat].notes.push(Note { pos, len: self.piano.last_len, chan, key, vel: 1.0, pan: 0.0, fine_cents: 0.0, muted: false });
                            let idx = self.song.patterns[pat].notes.len() - 1;
                            self.piano.drag = Some(Drag { idx, mode: Mode::Move, grab_dt: tick_of(pp.x) - pos as f32 });
                            self.dirty = true;
                            self.pr_sound(Some(key));
                        }
                    }
                } else if down {
                    if let Some(d) = &self.piano.drag {
                        if let Some(n) = self.song.patterns[pat].notes.get_mut(d.idx) {
                            match d.mode {
                                Mode::Move => {
                                    let pos = snap(tick_of(pp.x) - d.grab_dt + if alt { 0.0 } else { step as f32 * 0.5 });
                                    if n.pos != pos || n.key != key {
                                        n.pos = pos;
                                        n.key = key;
                                        self.dirty = true;
                                    }
                                }
                                Mode::Resize => {
                                    let end = snap(tick_of(pp.x) + if alt { 0.0 } else { step as f32 * 0.5 });
                                    let len = end.saturating_sub(n.pos).max(if alt { 1 } else { step });
                                    if n.len != len {
                                        n.len = len;
                                        self.dirty = true;
                                    }
                                    self.piano.last_len = len;
                                }
                            }
                        }
                        if d.mode == Mode::Move {
                            self.pr_sound(Some(key));
                        }
                    }
                }
                if hit.map(|h| h.1).unwrap_or(false) && self.piano.drag.is_none() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
        }
        if released || !down {
            if self.piano.drag.take().is_some() {
                self.song.patterns[pat].sort();
                self.dirty = true;
            }
            self.pr_sound(None);
        }
    }
}
