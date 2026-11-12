//! The mixer: one strip per insert, the master first.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::widgets::*;
use crate::App;

const STRIP_W: f32 = 50.0;

#[derive(Clone, Copy)]
enum SlotAct {
    Toggle(u16),
    Open(u16),
    Delete(u16),
    Move(u16, u16),
    Set(u16, cereal_engine::fx::Kind, &'static str),
    Host(u16),
}

impl App {
    pub(crate) fn mixer_ui(&mut self, ui: &mut Ui) {
        let n = self.song.inserts.len();
        self.mixer_sel.retain(|i| *i < n);
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
        let mut route: Option<(usize, usize, bool)> = None;
        ui.horizontal_top(|ui| {
            let side = 190.0;
            let strips_w = (ui.available_width() - side - 8.0).max(120.0);
            ui.allocate_ui(Vec2::new(strips_w, ui.available_height()), |ui| {
                egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.horizontal_top(|ui| {
                        for i in 0..n {
                            let selected = self.sel_insert == i || self.mixer_sel.contains(&i);
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
                                        let m = ui.input(|x| x.modifiers);
                                        if m.command {
                                            // add this track to the selection, or drop it
                                            if !self.mixer_sel.remove(&i) {
                                                self.mixer_sel.insert(self.sel_insert);
                                                self.mixer_sel.insert(i);
                                                self.sel_insert = i;
                                            }
                                        } else if m.shift {
                                            let (a, b) = (self.sel_insert.min(i), self.sel_insert.max(i));
                                            self.mixer_sel.extend(a..=b);
                                            self.sel_insert = i;
                                        } else {
                                            self.mixer_sel.clear();
                                            self.sel_insert = i;
                                        }
                                    }
                                    if resp.hovered() {
                                        self.wheel_over = Some(crate::keys::WheelMove::Insert(i, false));
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button("Open mixer track state...").clicked() {
                                            self.state_ask = Some(crate::StateAsk::OpenTrack(i));
                                            ui.close_menu();
                                        }
                                        if ui.button("Save mixer track state as...").clicked() {
                                            self.state_ask = Some(crate::StateAsk::SaveTrack(i));
                                            ui.close_menu();
                                        }
                                    });
                                    let ins = &mut self.song.inserts[i];
                                    let before = (ins.volume, ins.pan, ins.enabled);
                                    if led(ui, ins.enabled, 14.0).on_hover_text("Mute").clicked() {
                                        ins.enabled = !ins.enabled;
                                        self.dirty = true;
                                    }
                                    let r = knob(ui, &mut ins.pan, -1.0, 1.0, 0.0, 20.0).on_hover_text("Panning");
                                    control_menu(&r, cereal_engine::song::Target::InsertPan(i));
                                    if r.changed() {
                                        self.dirty = true;
                                    }
                                    let h = (ui.available_height() - 56.0).clamp(60.0, 220.0);
                                    let r = fader(ui, &mut ins.volume, 1.25, 1.0, Vec2::new(16.0, h), self.peaks[i]);
                                    control_menu(&r, cereal_engine::song::Target::InsertVolume(i));
                                    if r.changed() {
                                        self.dirty = true;
                                    }
                                    let db = 20.0 * cereal_engine::law::fader_gain(ins.volume).max(1e-6).log10();
                                    r.on_hover_text(if ins.volume <= 0.0 { "-inf dB".to_string() } else { format!("{db:.1} dB") });
                                    let now = (ins.volume, ins.pan, ins.enabled);
                                    if now != before && self.mixer_sel.contains(&i) {
                                        // the other selected tracks follow by the same amount
                                        for k in self.mixer_sel.iter().copied().filter(|k| *k != i && *k < n) {
                                            let s = &mut self.song.inserts[k];
                                            s.volume = (s.volume + now.0 - before.0).clamp(0.0, 1.25);
                                            s.pan = (s.pan + now.1 - before.1).clamp(-1.0, 1.0);
                                            if now.2 != before.2 {
                                                s.enabled = now.2;
                                            }
                                        }
                                    }
                                    let dot = if used[i] > 0 { GREEN } else { PANEL_DARK };
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(STRIP_W - 6.0, 8.0), Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 3.0, dot);
                                    // routing from the selected insert into this one
                                    let src = self.sel_insert;
                                    if src != i && src != 0 && src < n {
                                        let routed = self.song.inserts[src].sends.contains(&i);
                                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(STRIP_W - 6.0, 12.0), Sense::click());
                                        let c = rect.center();
                                        let col = if routed { ACCENT } else if resp.hovered() { TEXT } else { TEXT_DIM.gamma_multiply(0.5) };
                                        ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-5.0, 4.0), c + Vec2::new(5.0, 4.0), c + Vec2::new(0.0, -4.0)], col, Stroke::NONE));
                                        if resp.on_hover_text(if routed { "Routed from the selected track (click to remove)" } else { "Route the selected track here" }).clicked() {
                                            route = Some((src, i, !routed));
                                        }
                                        if routed {
                                            let mut lv = self.song.inserts[src].send_level(i);
                                            if knob(ui, &mut lv, 0.0, 1.25, 1.0, 16.0).on_hover_text(format!("Send level {:.0} %", lv * 100.0)).changed() {
                                                self.song.inserts[src].set_send_level(i, lv);
                                                self.dirty = true;
                                            }
                                        }
                                    }
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
                    let mut changed = false;
                    let mut act: Option<SlotAct> = None;
                    let mut hosted = None;
                    for slot in 0..10u16 {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(side - 8.0, 18.0), Sense::click());
                        ui.painter().rect_filled(rect, 2.0, PANEL_DARK);
                        let dot = Rect::from_center_size(rect.right_center() - Vec2::new(9.0, 0.0), Vec2::splat(16.0));
                        let over_dot = resp.hover_pos().map(|p| dot.contains(p)).unwrap_or(false);
                        let filled = match self.song.inserts[i].slots.iter().find(|s| s.index == slot) {
                            Some(s) => {
                                let open = self.fx_open == Some((i, slot));
                                ui.painter().text(rect.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, fit(ui, &s.title(), rect.width() - 24.0, &font), font.clone(), if !s.enabled { TEXT_DIM } else if open { GREEN } else { TEXT });
                                ui.painter().circle_filled(dot.center(), 3.5, if s.enabled { GREEN } else { PANEL });
                                true
                            }
                            None => {
                                ui.painter().text(rect.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, format!("Slot {}", slot + 1), font.clone(), TEXT_DIM.gamma_multiply(0.6));
                                false
                            }
                        };
                        if filled && resp.clicked() {
                            act = Some(if over_dot { SlotAct::Toggle(slot) } else { SlotAct::Open(slot) });
                        }
                        let popup = ui.make_persistent_id(("slot-menu", slot));
                        if (!filled && resp.clicked()) || resp.secondary_clicked() {
                            ui.memory_mut(|m| m.toggle_popup(popup));
                        }
                        egui::popup::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.set_min_width(170.0);
                            if filled {
                                if ui.button("Delete").clicked() {
                                    act = Some(SlotAct::Delete(slot));
                                }
                                if slot > 0 && ui.button("Move up").clicked() {
                                    act = Some(SlotAct::Move(slot, slot - 1));
                                }
                                if slot < 9 && ui.button("Move down").clicked() {
                                    act = Some(SlotAct::Move(slot, slot + 1));
                                }
                                ui.separator();
                                ui.label(RichText::new("Replace with").small().color(TEXT_DIM));
                            }
                            ui.menu_button("Plug-ins", |ui| {
                                if let Some(inst) = crate::hostui::menu(ui, false) {
                                    hosted = Some(inst);
                                    act = Some(SlotAct::Host(slot));
                                }
                            });
                            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                let mut defs: Vec<&cereal_engine::fx::Def> = cereal_engine::fx::DEFS.iter().filter(|d| d.kind != cereal_engine::fx::Kind::Thru || ["Tuner", "Wave Candy", "Edison", "Fruity dB Meter", "Fruity Spectroman", "Fruity Big Clock"].contains(&d.ident)).collect();
                                defs.sort_by_key(|d| d.title);
                                for d in defs {
                                    if ui.button(d.title).clicked() {
                                        act = Some(SlotAct::Set(slot, d.kind, d.ident));
                                    }
                                }
                            });
                        });
                        if filled {
                            resp.on_hover_text("Click to open, click the light to switch on or off, right-click for more");
                        }
                    }
                    if let Some(a) = act {
                        let slots = &mut self.song.inserts[i].slots;
                        match a {
                            SlotAct::Toggle(s) => {
                                if let Some(x) = slots.iter_mut().find(|x| x.index == s) {
                                    x.enabled = !x.enabled;
                                }
                            }
                            SlotAct::Open(s) => self.fx_open = if self.fx_open == Some((i, s)) { None } else { Some((i, s)) },
                            SlotAct::Delete(s) => {
                                slots.retain(|x| x.index != s);
                                if self.fx_open == Some((i, s)) {
                                    self.fx_open = None;
                                }
                            }
                            SlotAct::Move(a, b) => {
                                for x in slots.iter_mut() {
                                    if x.index == a {
                                        x.index = b;
                                    } else if x.index == b {
                                        x.index = a;
                                    }
                                }
                                slots.sort_by_key(|x| x.index);
                                self.fx_open = match self.fx_open {
                                    Some((ii, s)) if ii == i && s == a => Some((i, b)),
                                    Some((ii, s)) if ii == i && s == b => Some((i, a)),
                                    o => o,
                                };
                            }
                            SlotAct::Host(s) => {
                                if let Some(inst) = hosted.take() {
                                    slots.retain(|x| x.index != s);
                                    slots.push(cereal_engine::song::Slot::hosted(s, inst));
                                    slots.sort_by_key(|x| x.index);
                                    self.fx_open = Some((i, s));
                                }
                            }
                            SlotAct::Set(s, kind, ident) => {
                                slots.retain(|x| x.index != s);
                                let mut new = cereal_engine::song::Slot::with_fx(s, kind);
                                // several units share a kind; the name is what tells them apart
                                new.plugin = ident.to_string();
                                if let (cereal_engine::fx::Kind::Notes, Some(f), Some(fl)) = (kind, &mut new.fx, cereal_engine::notes::flavour_of(ident)) {
                                    f.p = cereal_engine::notes::Notes { flavour: fl, active: 1, pages: vec![(1, String::new())] }.pack();
                                }
                                slots.push(new);
                                slots.sort_by_key(|x| x.index);
                                self.fx_open = Some((i, s));
                            }
                        }
                        if !matches!(a, SlotAct::Open(_)) {
                            changed = true;
                        }
                    }
                    // which sound card input is heard on this track
                    {
                        let names = crate::record::input_names(crate::record::input_channels());
                        let ins = &mut self.song.inserts[i];
                        let now = match ins.audio_in {
                            v if v < 0 => "(none)".to_string(),
                            v => names.get(v as usize).cloned().unwrap_or_else(|| format!("Input {}", v + 1)),
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Audio in:").small().color(TEXT_DIM));
                            egui::ComboBox::from_id_salt(("insert input", i)).selected_text(RichText::new(now).small()).width(120.0).show_ui(ui, |ui| {
                                changed |= ui.selectable_value(&mut ins.audio_in, -1, "(none)").changed();
                                for (k, n) in names.iter().enumerate() {
                                    changed |= ui.selectable_value(&mut ins.audio_in, k as i32, n).changed();
                                }
                            });
                        })
                        .response
                        .on_hover_text("The sound card input heard on this track. It passes the track's effects, so a recorder in a slot takes it down.");
                    }
                    ui.add_space(4.0);
                    // the track's own equaliser: gain on top, frequency and width under it
                    {
                        let ins = &mut self.song.inserts[i];
                        let def = cereal_engine::song::EQ_DEFAULT;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            for b in 0..3 {
                                ui.vertical(|ui| {
                                    let band = &mut ins.eq[b];
                                    let hz = band.hz();
                                    changed |= knob(ui, &mut band.gain_db, -18.0, 18.0, 0.0, 22.0).on_hover_text(format!("{} gain {:+.1} dB", ["Low", "Mid", "High"][b], band.gain_db)).changed();
                                    changed |= knob(ui, &mut band.freq, 0.0, 1.0, def[b].freq, 16.0).on_hover_text(format!("Frequency {hz:.0} Hz")).changed();
                                    changed |= knob(ui, &mut band.q, 0.0, 1.0, def[b].q, 16.0).on_hover_text("Width").changed();
                                });
                            }
                            ui.vertical(|ui| {
                                changed |= knob(ui, &mut ins.sep, -1.0, 1.0, 0.0, 22.0).on_hover_text(format!("Stereo separation {:+.0} %", -ins.sep * 100.0)).changed();
                                changed |= ui.selectable_label(ins.invert, RichText::new("INV").small()).on_hover_text("Reverse polarity").clicked().then(|| ins.invert = !ins.invert).is_some();
                                changed |= ui.selectable_label(ins.swap, RichText::new("SWAP").small()).on_hover_text("Swap left and right").clicked().then(|| ins.swap = !ins.swap).is_some();
                            });
                        });
                    }
                    if changed {
                        self.dirty = true;
                    }
                    ui.add_space(4.0);
                    let ins = &self.song.inserts[i];
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
        if let Some((src, dst, on)) = route {
            // never let a track feed itself through others
            let mut reach = vec![dst];
            let mut k = 0;
            while k < reach.len() {
                for &d in &self.song.inserts[reach[k]].sends {
                    if !reach.contains(&d) {
                        reach.push(d);
                    }
                }
                k += 1;
            }
            let ins = &mut self.song.inserts[src];
            if !on {
                ins.sends.retain(|&d| d != dst);
                self.dirty = true;
            } else if !reach.contains(&src) {
                ins.sends.push(dst);
                ins.sends.sort_unstable();
                self.dirty = true;
            } else {
                self.status = "That routing would feed the track back into itself".into();
            }
        }
    }
}
