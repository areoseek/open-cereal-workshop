//! The channel rack: one row per channel with its step sequencer.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::flp::ChannelKind;
use cereal_engine::song::Chan;

use crate::widgets::*;
use crate::App;

#[derive(Default)]
pub struct State {
    rename: Option<(usize, String)>,
}

const STEP_W: f32 = 15.0;
const STEP_H: f32 = 24.0;

impl App {
    pub(crate) fn rack_ui(&mut self, ui: &mut Ui) {
        let pat = self.handle.transport.pattern();
        self.song.ensure_pattern(pat);
        let step = self.song.step();
        let nsteps = (self.song.pattern_length(pat) / step).max(16) as usize;
        let playing = self.handle.transport.playing() && !self.handle.transport.song_mode();
        let now = (self.handle.transport.tick() as u32 / step) as usize;
        let (primary, secondary) = ui.input(|i| (i.pointer.primary_down(), i.pointer.secondary_down()));
        let mut delete: Option<usize> = None;
        let mut clone: Option<usize> = None;

        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
            for ci in 0..self.song.channels.len() {
                ui.horizontal(|ui| {
                    let on = self.song.channels[ci].enabled;
                    let r = led(ui, on, 16.0).on_hover_text("Mute / solo (right-click)");
                    if r.clicked() {
                        self.song.channels[ci].enabled = !on;
                        self.dirty = true;
                    }
                    if r.secondary_clicked() {
                        let solo = self.song.channels.iter().enumerate().all(|(k, c)| c.enabled == (k == ci));
                        for (k, c) in self.song.channels.iter_mut().enumerate() {
                            c.enabled = solo || k == ci;
                        }
                        self.dirty = true;
                    }
                    let c = &mut self.song.channels[ci];
                    let r = knob(ui, &mut c.pan, -1.0, 1.0, 0.0, 18.0).on_hover_text("Channel panning");
                    control_menu(&r, cereal_engine::song::Target::ChanPan(ci));
                    if r.changed() {
                        self.dirty = true;
                    }
                    let r = knob(ui, &mut c.volume, 0.0, 1.0, 10000.0 / 12800.0, 18.0).on_hover_text("Channel volume");
                    control_menu(&r, cereal_engine::song::Target::ChanVolume(ci));
                    if r.changed() {
                        self.dirty = true;
                    }
                    let n_ins = self.song.inserts.len().saturating_sub(1);
                    let mut ins = c.insert;
                    let dv = egui::DragValue::new(&mut ins).range(0..=n_ins).speed(0.1).custom_formatter(|v, _| if v < 0.5 { "---".into() } else { format!("{v:.0}") });
                    if ui.add_sized([30.0, STEP_H - 4.0], dv).on_hover_text("Target mixer track").changed() {
                        c.insert = ins;
                        self.dirty = true;
                    }

                    // name button
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(120.0, STEP_H), Sense::click());
                    let selected = self.sel_chan == ci;
                    let c = &self.song.channels[ci];
                    let base = rgb(c.color);
                    let fill = if selected { base.lerp_to_gamma(Color32::WHITE, 0.25) } else { base };
                    ui.painter().rect_filled(rect, 3.0, fill);
                    let font = egui::FontId::proportional(12.0);
                    let label = fit(ui, &c.name, rect.width() - 10.0, &font);
                    let missing = matches!(c.kind, ChannelKind::Sampler | ChannelKind::AudioClip) && c.sample.is_none() && !c.sample_path.is_empty();
                    ui.painter().text(rect.left_center() + Vec2::new(6.0, 0.0), egui::Align2::LEFT_CENTER, label, font, if missing { Color32::from_rgb(255, 120, 110) } else { Color32::from_gray(235) });
                    if selected {
                        ui.painter().rect_filled(Rect::from_min_size(rect.right_top() + Vec2::new(2.0, 4.0), Vec2::new(3.0, STEP_H - 8.0)), 1.0, GREEN);
                    }
                    if resp.clicked() {
                        // a click on the name opens that channel's settings; a second click closes them
                        self.show_chanset = !(self.show_chanset && self.sel_chan == ci);
                        self.sel_chan = ci;
                    }
                    if let Some(path) = resp.dnd_release_payload::<std::path::PathBuf>() {
                        if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                            self.actions.push(crate::Action::LoadPreset((*path).clone(), crate::PresetTo::Chan(ci)));
                        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fsc") || x.eq_ignore_ascii_case("mid")) {
                            self.actions.push(crate::Action::LoadScore((*path).clone(), Some(ci)));
                        } else {
                            self.actions.push(crate::Action::LoadSample((*path).clone(), Some(ci)));
                        }
                    }
                    let hover = if missing { format!("Missing sample: {}", c.sample_path) } else if !c.sample_path.is_empty() { c.sample_path.clone() } else { c.plugin.clone() };
                    let resp = if hover.is_empty() { resp } else { resp.on_hover_text(hover) };
                    resp.context_menu(|ui| {
                        if ui.button("Piano roll").clicked() {
                            self.sel_chan = ci;
                            self.show_piano = true;
                            ui.close_menu();
                        }
                        if ui.button("Rename").clicked() {
                            self.rack.rename = Some((ci, self.song.channels[ci].name.clone()));
                            ui.close_menu();
                        }
                        ui.separator();
                        for every in [2u32, 4, 8] {
                            if ui.button(format!("Fill each {every} steps")).clicked() {
                                for s in (0..nsteps as u32).step_by(every as usize) {
                                    self.song.patterns[pat].set_step(ci as u16, s * step, true);
                                }
                                self.dirty = true;
                                ui.close_menu();
                            }
                        }
                        if ui.button("Clear steps").clicked() {
                            self.song.patterns[pat].notes.retain(|n| n.chan != ci as u16);
                            self.dirty = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Open channel state...").clicked() {
                            self.state_ask = Some(crate::StateAsk::OpenChan(ci));
                            ui.close_menu();
                        }
                        if ui.button("Save channel state as...").clicked() {
                            self.state_ask = Some(crate::StateAsk::SaveChan(ci));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Clone").clicked() {
                            clone = Some(ci);
                            ui.close_menu();
                        }
                        if ui.button("Delete").clicked() {
                            delete = Some(ci);
                            ui.close_menu();
                        }
                    });
                    ui.add_space(8.0);

                    // steps, or a small piano roll picture when the channel has real notes
                    let notes: Vec<(u32, u32, u8)> = self.song.patterns[pat].notes.iter().filter(|n| n.chan == ci as u16).map(|n| (n.pos, n.len, n.key)).collect();
                    let melodic = notes.iter().any(|n| n.1 != 0 || n.2 != 60 || n.0 % step != 0);
                    if melodic {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(nsteps as f32 * (STEP_W + 3.0) - 3.0, STEP_H), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(rect, 2.0, PANEL_DARK);
                        let lo = notes.iter().map(|n| n.2).min().unwrap_or(60) as f32;
                        let hi = notes.iter().map(|n| n.2).max().unwrap_or(60) as f32;
                        let span = (hi - lo).max(1.0);
                        let total = (nsteps as u32 * step) as f32;
                        for (pos, len, key) in &notes {
                            let x0 = rect.left() + *pos as f32 / total * rect.width();
                            let x1 = rect.left() + (*pos + (*len).max(step / 2)) as f32 / total * rect.width();
                            let y = rect.bottom() - 3.0 - (*key as f32 - lo) / span * (rect.height() - 6.0);
                            p.line_segment([Pos2::new(x0, y), Pos2::new(x1.min(rect.right()), y)], Stroke::new(2.0_f32, NOTE));
                        }
                        if resp.clicked() {
                            self.sel_chan = ci;
                            self.show_piano = true;
                        }
                    } else {
                        for s in 0..nsteps {
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(STEP_W, STEP_H), Sense::hover());
                            let tick = s as u32 * step;
                            let lit = notes.iter().any(|n| n.0 == tick);
                            if ui.rect_contains_pointer(rect) && (primary || secondary) && lit != primary {
                                self.song.patterns[pat].set_step(ci as u16, tick, primary);
                                self.dirty = true;
                                if primary {
                                    self.sel_chan = ci;
                                }
                            }
                            let alt = (s / 4) % 2 == 1;
                            let mut fill = match (lit, alt) {
                                (true, false) => STEP_A_ON,
                                (true, true) => STEP_B_ON,
                                (false, false) => STEP_A,
                                (false, true) => STEP_B,
                            };
                            if playing && s == now {
                                fill = fill.lerp_to_gamma(ACCENT, 0.6);
                            }
                            ui.painter().rect_filled(rect, 2.0, fill);
                        }
                    }
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("  +  ").strong()).on_hover_text("Add a sampler channel").clicked() {
                    self.song.channels.push(Chan::default());
                    self.sel_chan = self.song.channels.len() - 1;
                    self.dirty = true;
                }
                ui.menu_button("Instrument", |ui| {
                    // some instruments have no fresh block of their own yet: they arrive with projects and presets
                    for d in cereal_engine::synth::DEFS.iter().filter(|d| !d.arrives_only()) {
                        if ui.button(d.title).clicked() {
                            self.song.channels.push(Chan::instrument(d.kind));
                            self.sel_chan = self.song.channels.len() - 1;
                            self.dirty = true;
                            ui.close_menu();
                        }
                    }
                    if ui.button("Sound Bank Player").clicked() {
                        let mut c = Chan { name: "Sound Bank Player".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: "SoundFont Player".into(), ..Chan::default() };
                        c.zones = cereal_engine::sf2::open(&c.plugin, &[], None, None).map(std::sync::Arc::new);
                        self.song.channels.push(c);
                        self.sel_chan = self.song.channels.len() - 1;
                        self.dirty = true;
                        ui.close_menu();
                    }
                })
                .response
                .on_hover_text("Add a channel playing a built-in instrument");
                ui.label(RichText::new("Drop a sample from the browser onto a channel name to load it.").color(TEXT_DIM).small());
            });
        });

        if let Some(ci) = clone {
            let mut c = self.song.channels[ci].clone();
            c.name = format!("{} #2", c.name);
            self.song.channels.push(c);
            self.dirty = true;
        }
        if let Some(ci) = delete {
            self.delete_channel(ci);
        }
        if let Some((ci, mut name)) = self.rack.rename.take() {
            let mut keep = true;
            egui::Window::new("Rename channel").collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(c) = self.song.channels.get_mut(ci) {
                        c.name = name.clone();
                    }
                    self.dirty = true;
                    keep = false;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    keep = false;
                }
            });
            if keep {
                self.rack.rename = Some((ci, name));
            }
        }
    }

    /// Remove a channel and renumber everything that refers to the ones after it.
    pub(crate) fn delete_channel(&mut self, ci: usize) {
        if ci >= self.song.channels.len() {
            return;
        }
        self.song.remove_channel(ci);
        self.sel_chan = self.sel_chan.min(self.song.channels.len().saturating_sub(1));
        self.dirty = true;
    }
}
