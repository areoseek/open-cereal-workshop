//! The mixer: one strip per insert, the master first.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::widgets::*;
use crate::App;

const STRIP_W: f32 = 50.0;

impl App {
    pub(crate) fn mixer_ui(&mut self, ui: &mut Ui) {
        let n = self.song.inserts.len();
        if self.peaks.len() != n {
            self.peaks = vec![(0.0, 0.0); n];
        }
        for (i, p) in self.peaks.iter_mut().enumerate() {
            let (l, r) = self.handle.transport.take_peak(i);
            p.0 = l.max(p.0 * 0.88);
            p.1 = r.max(p.1 * 0.88);
        }
        let used: Vec<usize> = {
            let mut u = vec![0usize; n];
            for c in &self.song.channels {
                if c.insert < n {
                    u[c.insert] += 1;
                }
            }
            u
        };
        let font = FontId::proportional(10.5);
        ui.horizontal_top(|ui| {
            let side = 190.0;
            let strips_w = (ui.available_width() - side - 8.0).max(120.0);
            ui.allocate_ui(Vec2::new(strips_w, ui.available_height()), |ui| {
                egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.horizontal_top(|ui| {
                        for i in 0..n {
                            let selected = self.sel_insert == i;
                            let name = self.song.insert_name(i);
                            let frame = egui::Frame::new().fill(if selected { PANEL.lerp_to_gamma(Color32::WHITE, 0.08) } else { PANEL }).inner_margin(3.0).corner_radius(3.0);
                            let inner = frame.show(ui, |ui| {
                                ui.set_width(STRIP_W - 6.0);
                                ui.vertical_centered(|ui| {
                                    ui.spacing_mut().item_spacing.y = 3.0;
                                    // name plate
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(STRIP_W - 6.0, 30.0), Sense::click());
                                    let col = self.song.inserts[i].color.map(rgb).unwrap_or(if i == 0 { Color32::from_rgb(110, 96, 70) } else { STEP_A });
                                    ui.painter().rect_filled(rect, 2.0, col);
                                    ui.painter().text(rect.center_top() + Vec2::new(0.0, 2.0), Align2::CENTER_TOP, if i == 0 { "M".to_string() } else { i.to_string() }, font.clone(), TEXT_DIM);
                                    ui.painter().text(rect.center_bottom() - Vec2::new(0.0, 2.0), Align2::CENTER_BOTTOM, fit(ui, &name, rect.width() - 2.0, &font), font.clone(), Color32::from_gray(240));
                                    if resp.clicked() {
                                        self.sel_insert = i;
                                    }
                                    let ins = &mut self.song.inserts[i];
                                    if led(ui, ins.enabled, 14.0).on_hover_text("Mute").clicked() {
                                        ins.enabled = !ins.enabled;
                                        self.dirty = true;
                                    }
                                    if knob(ui, &mut ins.pan, -1.0, 1.0, 0.0, 20.0).on_hover_text("Panning").changed() {
                                        self.dirty = true;
                                    }
                                    let h = (ui.available_height() - 22.0).clamp(60.0, 220.0);
                                    let r = fader(ui, &mut ins.volume, 1.25, 1.0, Vec2::new(16.0, h), self.peaks[i]);
                                    if r.changed() {
                                        self.dirty = true;
                                    }
                                    let db = 20.0 * cereal_engine::law::fader_gain(ins.volume).max(1e-6).log10();
                                    r.on_hover_text(if ins.volume <= 0.0 { "-inf dB".to_string() } else { format!("{db:.1} dB") });
                                    let dot = if used[i] > 0 { GREEN } else { PANEL_DARK };
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(STRIP_W - 6.0, 8.0), Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 3.0, dot);
                                });
                            });
                            if selected {
                                ui.painter().rect_stroke(inner.response.rect, 3.0, Stroke::new(1.0_f32, GREEN), egui::StrokeKind::Inside);
                            }
                        }
                    });
                });
            });

            // the selected insert: its effect slots, sends and channels
            ui.allocate_ui(Vec2::new(side, ui.available_height()), |ui| {
                ui.vertical(|ui| {
                    let i = self.sel_insert.min(n.saturating_sub(1));
                    ui.label(RichText::new(self.song.insert_name(i)).strong().color(TEXT));
                    ui.separator();
                    let ins = &self.song.inserts[i];
                    for slot in 0..10u16 {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(side - 8.0, 18.0), Sense::click());
                        ui.painter().rect_filled(rect, 2.0, PANEL_DARK);
                        match ins.slots.iter().find(|s| s.index == slot) {
                            Some(s) => {
                                let label = if s.name.is_empty() { s.plugin.clone() } else { s.name.clone() };
                                ui.painter().text(rect.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, fit(ui, &label, rect.width() - 22.0, &font), font.clone(), if s.enabled { TEXT } else { TEXT_DIM });
                                ui.painter().circle_filled(rect.right_center() - Vec2::new(8.0, 0.0), 3.0, if s.enabled { GREEN } else { PANEL });
                            }
                            None => {
                                ui.painter().text(rect.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, format!("Slot {}", slot + 1), font.clone(), TEXT_DIM.gamma_multiply(0.6));
                            }
                        }
                    }
                    ui.add_space(4.0);
                    if i != 0 {
                        let to: Vec<String> = ins.sends.iter().map(|&d| self.song.insert_name(d)).collect();
                        ui.label(RichText::new(format!("Out: {}", if to.is_empty() { "(none)".into() } else { to.join(", ") })).small().color(TEXT_DIM));
                    }
                    let chans: Vec<&str> = self.song.channels.iter().filter(|c| c.insert == i).map(|c| c.name.as_str()).collect();
                    if !chans.is_empty() {
                        ui.label(RichText::new(format!("In: {}", chans.join(", "))).small().color(TEXT_DIM));
                    }
                    let _ = Rect::NOTHING;
                });
            });
        });
    }
}
