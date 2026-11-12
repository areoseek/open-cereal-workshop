//! The channel rack: one row per channel with its step sequencer.

use crate::widgets::Hint as _;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::flp::ChannelKind;
use cereal_engine::song::Chan;

use crate::widgets::*;
use crate::App;

#[derive(Default)]
pub struct State {
    pub(crate) rename: Option<(usize, String)>,
    /// What the rack's copy took: one channel's notes of a pattern.
    pub(crate) clip: Vec<cereal_engine::song::Note>,
    /// Leave out the mixer track boxes beside the channel names.
    pub(crate) hide_selectors: bool,
    /// Channels selected besides the one that has the keys.
    pub(crate) sel: std::collections::BTreeSet<usize>,
    /// The name being typed for a new group of the selected channels.
    pub(crate) group_name: Option<String>,
    /// The group that the name window renames, when it is not making a new one.
    group_edit: Option<usize>,
    /// The graph editor under the channels: open or not, and which property of the steps it shows.
    graph: bool,
    graph_kind: usize,
    /// The keyboard editor under the channels: every step of the selected channel against the keys.
    keys: bool,
}

const GRAPH: [&str; 7] = ["Note pitch", "Velocity", "Release", "Fine pitch", "Pan", "Mod X", "Mod Y"];
const GRAPH_H: f32 = 64.0;
/// The keyboard editor: the height of one key's row, and of the part of the keyboard in view.
const KEY_H: f32 = 7.0;
const KEYS_H: f32 = 147.0;

/// A step's property as a share of the graph's height, and back.
fn graph_get(n: &cereal_engine::song::Note, kind: usize) -> f32 {
    match kind {
        0 => n.key as f32 / 131.0,
        1 => n.vel / 1.28,
        2 => n.release,
        3 => n.fine_cents / 2400.0 + 0.5,
        4 => n.pan * 0.5 + 0.5,
        5 => n.mod_x * 0.5 + 0.5,
        _ => n.mod_y * 0.5 + 0.5,
    }
    .clamp(0.0, 1.0)
}
fn graph_set(n: &mut cereal_engine::song::Note, kind: usize, f: f32) {
    // the two-sided ones catch on their middle
    let mid = |f: f32| if (f - 0.5).abs() < 0.04 { 0.0 } else { f * 2.0 - 1.0 };
    match kind {
        0 => n.key = (f * 131.0).round() as _,
        1 => n.vel = f * 1.28,
        2 => n.release = f,
        3 => n.fine_cents = (mid(f) * 1200.0).round(),
        4 => n.pan = mid(f),
        5 => n.mod_x = mid(f),
        _ => n.mod_y = mid(f),
    }
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

        if !self.song.groups.is_empty() {
            ui.horizontal(|ui| {
                let shown = self.song.cur_group.and_then(|g| self.song.groups.get(g).cloned()).unwrap_or_else(|| "All".into());
                let mut pick: Option<Option<usize>> = None;
                egui::ComboBox::from_id_salt("rack_group").selected_text(shown).width(130.0).show_ui(ui, |ui| {
                    if ui.selectable_label(self.song.cur_group.is_none(), "All").clicked() {
                        pick = Some(None);
                    }
                    for (g, name) in self.song.groups.iter().enumerate() {
                        if ui.selectable_label(self.song.cur_group == Some(g), name).clicked() {
                            pick = Some(Some(g));
                        }
                    }
                })
                .response
                .hint("Channel group on show (Page Up / Page Down)");
                if let Some(p) = pick {
                    self.song.cur_group = p;
                    self.dirty = true;
                }
                let cur = self.song.cur_group.filter(|g| *g < self.song.groups.len());
                ui.menu_button("...", |ui| {
                    if ui.add_enabled(cur.is_some(), egui::Button::new("Rename group...")).clicked() {
                        if let Some(g) = cur {
                            self.rack.group_edit = Some(g);
                            self.rack.group_name = Some(self.song.groups[g].clone());
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(cur.is_some_and(|g| self.song.groups[g] != "Unsorted"), egui::Button::new("Delete group")).hint("Its channels go to Unsorted").clicked() {
                        if let Some(g) = cur {
                            self.song.delete_group(g);
                            self.dirty = true;
                        }
                        ui.close_menu();
                    }
                    if ui.button("Group selected channels...        Alt+G").clicked() {
                        self.rack.group_edit = None;
                        self.rack.group_name = Some(String::new());
                        ui.close_menu();
                    }
                })
                .response
                .hint("Group options");
            });
        }
        self.rack.sel.retain(|c| *c < self.song.channels.len());
        egui::TopBottomPanel::bottom("rack_graph").frame(egui::Frame::NONE).show_separator_line(false).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.rack.graph, "Graph editor").hint("The selected channel's steps as bars: drag to set each step's property").clicked() {
                    self.rack.graph = !self.rack.graph;
                }
                if ui.selectable_label(self.rack.keys, "Keyboard editor").hint("The selected channel's steps against a keyboard: click sets a step's note, right-click takes the step away").clicked() {
                    self.rack.keys = !self.rack.keys;
                }
                if self.rack.graph {
                    egui::ComboBox::from_id_salt("rack_graph_kind").width(110.0).selected_text(GRAPH[self.rack.graph_kind.min(6)]).show_ui(ui, |ui| {
                        for (i, name) in GRAPH.iter().enumerate() {
                            ui.selectable_value(&mut self.rack.graph_kind, i, *name);
                        }
                    });
                }
                if self.rack.graph || self.rack.keys {
                    ui.label(RichText::new(self.song.channels.get(self.sel_chan).map(|c| c.name.clone()).unwrap_or_default()).color(TEXT_DIM));
                }
            });
            if self.rack.keys {
                let chan = self.sel_chan as u16;
                let pitch = STEP_W + 3.0;
                // opened on the octave the steps are made in
                egui::ScrollArea::both().id_salt("rack_keys_scroll").max_height(KEYS_H).min_scrolled_height(KEYS_H).auto_shrink([false, false]).show(ui, |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(234.0 + nsteps as f32 * pitch, 132.0 * KEY_H), Sense::click_and_drag());
                    if ui.data_mut(|d| !std::mem::replace(d.get_temp_mut_or_default::<bool>(resp.id.with("placed")), true)) {
                        let y = rect.top() + (131.0 - 66.0) * KEY_H;
                        ui.scroll_to_rect(Rect::from_min_max(Pos2::new(rect.left(), y - KEYS_H * 0.5), Pos2::new(rect.left() + 1.0, y + KEYS_H * 0.5)), Some(egui::Align::Center));
                    }
                    let area = Rect::from_min_max(Pos2::new(rect.left() + 234.0, rect.top()), rect.max);
                    let p = ui.painter_at(rect);
                    let row = |key: i32| area.top() + (131 - key) as f32 * KEY_H;
                    for key in 0..132 {
                        let black = matches!(key % 12, 1 | 3 | 6 | 8 | 10);
                        let r = Rect::from_min_max(Pos2::new(area.left(), row(key)), Pos2::new(area.right(), row(key) + KEY_H));
                        p.rect_filled(r, 0.0, if black { GRID_DARK } else { GRID_DARK.gamma_multiply(1.35) });
                        // the keyboard itself, down the left, with each octave named
                        let k = Rect::from_min_max(Pos2::new(area.left() - 40.0, row(key)), Pos2::new(area.left() - 2.0, row(key) + KEY_H - 1.0));
                        p.rect_filled(k, 0.0, if black { Color32::from_gray(30) } else { Color32::from_gray(200) });
                        if key % 12 == 0 {
                            p.text(Pos2::new(area.left() - 46.0, row(key) + KEY_H * 0.5), egui::Align2::RIGHT_CENTER, format!("C{}", key / 12), egui::FontId::proportional(9.0), TEXT_DIM);
                            p.line_segment([Pos2::new(area.left(), row(key) + KEY_H), Pos2::new(area.right(), row(key) + KEY_H)], Stroke::new(1.0_f32, LINE_SOFT));
                        }
                    }
                    for s in (0..nsteps).step_by(4) {
                        let x = area.left() + s as f32 * pitch - 1.5;
                        p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
                    }
                    let at = resp.interact_pointer_pos().filter(|pp| area.contains(*pp));
                    let cell = at.map(|pp| (((pp.x - area.left()) / pitch) as u32, (131 - ((pp.y - area.top()) / KEY_H) as i32).clamp(0, 131)));
                    if let Some((s, key)) = cell.filter(|c| (c.0 as usize) < nsteps) {
                        let pos = s * step;
                        let notes = &mut self.song.patterns[pat];
                        if resp.secondary_clicked() || (resp.dragged_by(egui::PointerButton::Secondary)) {
                            if notes.has_step(chan, pos) {
                                notes.set_step(chan, pos, false);
                                self.dirty = true;
                            }
                        } else if resp.clicked() || resp.dragged_by(egui::PointerButton::Primary) || resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down()) {
                            notes.set_step(chan, pos, true);
                            for n in notes.notes.iter_mut().filter(|n| n.chan == chan && n.pos == pos) {
                                n.key = key as _;
                            }
                            self.dirty = true;
                        }
                    }
                    for n in self.song.patterns[pat].notes.iter().filter(|n| n.chan == chan && n.pos % step == 0 && ((n.pos / step) as usize) < nsteps) {
                        let x = area.left() + (n.pos / step) as f32 * pitch;
                        let y = row(n.key as i32);
                        p.rect_filled(Rect::from_min_max(Pos2::new(x, y), Pos2::new(x + STEP_W, y + KEY_H - 1.0)), 1.0, ACCENT);
                    }
                });
            }
            if self.rack.graph {
                let (chan, kind) = (self.sel_chan as u16, self.rack.graph_kind.min(6));
                let two_sided = kind >= 3;
                egui::ScrollArea::horizontal().id_salt("rack_graph_scroll").show(ui, |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(234.0 + nsteps as f32 * (STEP_W + 3.0), GRAPH_H), Sense::click_and_drag());
                    let area = Rect::from_min_max(Pos2::new(rect.left() + 234.0, rect.top()), rect.max);
                    let p = ui.painter_at(rect);
                    p.rect_filled(area, 2.0, GRID_DARK);
                    if two_sided {
                        p.line_segment([area.left_center(), area.right_center()], Stroke::new(1.0_f32, LINE_SOFT));
                    }
                    let drag = resp.interact_pointer_pos().filter(|pp| area.contains(*pp) && (resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on()));
                    let hit = drag.map(|pp| (((pp.x - area.left()) / (STEP_W + 3.0)) as u32, ((area.bottom() - pp.y) / area.height()).clamp(0.0, 1.0)));
                    for n in self.song.patterns[pat].notes.iter_mut().filter(|n| n.chan == chan && n.pos % step == 0) {
                        let s = n.pos / step;
                        if s as usize >= nsteps {
                            continue;
                        }
                        if let Some((hs, f)) = hit {
                            if hs == s {
                                graph_set(n, kind, f);
                                self.dirty = true;
                            }
                        }
                        let x = area.left() + s as f32 * (STEP_W + 3.0);
                        let y = area.bottom() - graph_get(n, kind) * area.height();
                        let foot = if two_sided { area.center().y } else { area.bottom() };
                        let bar = Rect::from_min_max(Pos2::new(x, y.min(foot)), Pos2::new(x + STEP_W, y.max(foot).max(y.min(foot) + 2.0)));
                        p.rect_filled(bar, 1.0, ACCENT);
                    }
                });
            }
        });
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
            for ci in 0..self.song.channels.len() {
                if !self.song.in_view(ci) {
                    continue;
                }
                if self.song.channels[ci].zipped {
                    // a zipped channel is a thin strip in its colour; a click opens it out again
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(220.0 + nsteps as f32 * (STEP_W + 3.0), 5.0), Sense::click());
                    let picked = self.sel_chan == ci || self.rack.sel.contains(&ci);
                    ui.painter().rect_filled(rect, 1.0, rgb(self.song.channels[ci].color).gamma_multiply(if picked { 1.0 } else { 0.7 }));
                    if resp.hint(self.song.channels[ci].name.clone()).clicked() {
                        self.song.channels[ci].zipped = false;
                        self.sel_chan = ci;
                        self.dirty = true;
                    }
                    continue;
                }
                ui.horizontal(|ui| {
                    let on = self.song.channels[ci].enabled;
                    let r = led(ui, on, 16.0).hint("Mute / solo (right-click)");
                    if r.clicked() {
                        self.song.channels[ci].enabled = !on;
                        self.dirty = true;
                    }
                    if r.secondary_clicked() {
                        self.solo_channel(ci);
                    }
                    let c = &mut self.song.channels[ci];
                    let r = knob(ui, &mut c.pan, -1.0, 1.0, 0.0, 18.0).hint("Channel panning");
                    control_menu(&r, cereal_engine::song::Target::ChanPan(ci));
                    if r.changed() {
                        self.dirty = true;
                    }
                    let r = knob(ui, &mut c.volume, 0.0, 1.0, 10000.0 / 12800.0, 18.0).hint("Channel volume");
                    control_menu(&r, cereal_engine::song::Target::ChanVolume(ci));
                    if r.changed() {
                        self.dirty = true;
                    }
                    let n_ins = self.song.inserts.len().saturating_sub(1);
                    let mut ins = c.insert;
                    let dv = egui::DragValue::new(&mut ins).range(0..=n_ins).speed(0.1).custom_formatter(|v, _| if v < 0.5 { "---".into() } else { format!("{v:.0}") });
                    if !self.rack.hide_selectors && ui.add_sized([30.0, STEP_H - 4.0], dv).hint("Target mixer track").changed() {
                        c.insert = ins;
                        self.dirty = true;
                    }

                    // name button
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(120.0, STEP_H), Sense::click());
                    let selected = self.sel_chan == ci || self.rack.sel.contains(&ci);
                    let c = &self.song.channels[ci];
                    let base = rgb(c.color);
                    let fill = if selected { base.lerp_to_gamma(Color32::WHITE, 0.25) } else { base };
                    ui.painter().rect_filled(rect, 3.0, fill);
                    let font = egui::FontId::proportional(12.0);
                    let label = fit(ui, &c.name, rect.width() - 10.0, &font);
                    let missing = matches!(c.kind, ChannelKind::Sampler | ChannelKind::AudioClip) && c.sample.is_none() && !c.sample_path.is_empty();
                    ui.painter().text(rect.left_center() + Vec2::new(6.0, 0.0), egui::Align2::LEFT_CENTER, label, font, if missing { Color32::from_rgb(255, 120, 110) } else { Color32::from_gray(235) });
                    // the selector beside the name: click for this one alone, Shift+click to add or drop it, right-click for all on show
                    let sel_rect = Rect::from_min_size(rect.right_top() + Vec2::new(1.0, 3.0), Vec2::new(6.0, STEP_H - 6.0));
                    let sel_resp = ui.interact(sel_rect, ui.id().with(("rack_sel", ci)), Sense::click());
                    ui.painter().rect_filled(sel_rect.shrink2(Vec2::new(1.5, 1.0)), 1.0, if selected { GREEN } else { PANEL_DARK });
                    if sel_resp.clicked() {
                        if ui.input(|i| i.modifiers.shift) {
                            if !self.rack.sel.remove(&ci) {
                                self.rack.sel.insert(ci);
                                self.rack.sel.insert(self.sel_chan);
                            }
                        } else {
                            self.rack.sel.clear();
                            self.sel_chan = ci;
                        }
                    }
                    if sel_resp.secondary_clicked() {
                        self.rack.sel = (0..self.song.channels.len()).filter(|c| self.song.in_view(*c)).collect();
                    }
                    if resp.hovered() {
                        self.wheel_over = Some(crate::keys::WheelMove::Channel(ci, false));
                    }
                    if resp.clicked() {
                        // a click on the name opens that channel's settings; a second click closes them
                        self.show_chanset = !(self.show_chanset && self.sel_chan == ci);
                        self.sel_chan = ci;
                        self.rack.sel.clear();
                    }
                    if let Some(path) = resp.dnd_release_payload::<std::path::PathBuf>() {
                        if cereal_engine::preset::is_file(&path) {
                            self.actions.push(crate::Action::LoadPreset((*path).clone(), crate::PresetTo::Chan(ci)));
                        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("fsc") || x.eq_ignore_ascii_case("mid")) {
                            self.actions.push(crate::Action::LoadScore((*path).clone(), Some(ci)));
                        } else {
                            self.actions.push(crate::Action::LoadSample((*path).clone(), Some(ci)));
                        }
                    }
                    let hover = if missing { format!("Missing sample: {}", c.sample_path) } else if !c.sample_path.is_empty() { c.sample_path.clone() } else { c.plugin.clone() };
                    let resp = if hover.is_empty() { resp } else { resp.hint(hover) };
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
                if ui.button(RichText::new("  +  ").strong()).hint("Add a sampler channel").clicked() {
                    self.song.channels.push(Chan::default());
                    self.sel_chan = self.song.channels.len() - 1;
                    self.dirty = true;
                }
                ui.menu_button("Instrument", |ui| {
                    self.instrument_menu(ui);
                })
                .response
                .hint("Add a channel playing a built-in instrument");
                ui.label(RichText::new("Drop a sample from the browser onto a channel name to load it.").color(TEXT_DIM).small());
            });
        });

        if let Some(ci) = clone {
            self.clone_channel(ci);
        }
        if let Some(ci) = delete {
            self.delete_channel(ci);
        }
        if let Some(mut name) = self.rack.group_name.take() {
            let mut keep = true;
            egui::Window::new("Group name").collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                crate::widgets::name_field(ui, &mut name);
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let name = name.trim();
                    if !name.is_empty() {
                        if let Some(g) = self.rack.group_edit {
                            self.song.rename_group(g, name);
                        } else {
                            let t = self.rack_targets();
                            let g = self.song.group_channels(&t, name);
                            self.song.cur_group = Some(g);
                            self.rack.sel.clear();
                        }
                        self.dirty = true;
                    }
                    keep = false;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    keep = false;
                }
            });
            if keep {
                self.rack.group_name = Some(name);
            } else {
                self.rack.group_edit = None;
            }
        }
        if let Some((ci, mut name)) = self.rack.rename.take() {
            let mut keep = true;
            egui::Window::new("Rename channel").collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                if let (Some(c), Some(ch)) = (crate::widgets::walk_colour(ui), self.song.channels.get_mut(ci)) {
                    ch.color = c;
                    self.dirty = true;
                }
                crate::widgets::name_field(ui, &mut name);
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
        self.rack.sel.clear();
        self.sel_chan = self.sel_chan.min(self.song.channels.len().saturating_sub(1));
        self.dirty = true;
    }
}

impl App {
    /// The built-in instruments and the installed plug-ins, as menu rows that each add a channel.
    pub(crate) fn instrument_menu(&mut self, ui: &mut Ui) {
        // some instruments have no fresh block of their own yet: they arrive with projects and presets
        for d in cereal_engine::synth::DEFS.iter().filter(|d| !d.arrives_only()) {
            if ui.button(d.title).clicked() {
                self.song.channels.push(Chan::instrument(d.kind));
                self.sel_chan = self.song.channels.len() - 1;
                self.dirty = true;
                ui.close_menu();
            }
        }
        ui.menu_button("Plug-ins", |ui| {
            if let Some(inst) = crate::hostui::menu(ui, true) {
                self.song.channels.push(Chan::hosted(inst));
                self.sel_chan = self.song.channels.len() - 1;
                self.dirty = true;
            }
        });
        if ui.button("MIDI Sender").hint("A silent channel whose notes and knobs go out as MIDI to other programs and hardware").clicked() {
            let mut c = Chan::instrument(cereal_engine::synth::Kind::Quiet);
            c.plugin = cereal_engine::midiout::IDENT.into();
            c.name = "MIDI Sender".into();
            if let Some(g) = c.gen.as_mut() {
                g.raw = cereal_engine::midiout::Set::fresh().write();
            }
            self.song.channels.push(c);
            self.sel_chan = self.song.channels.len() - 1;
            self.dirty = true;
            ui.close_menu();
        }
        if ui.button("Envelope Sender").hint("A silent channel whose notes run envelopes into the controls linked to it").clicked() {
            let mut c = Chan::instrument(cereal_engine::synth::Kind::Quiet);
            c.plugin = cereal_engine::fm::ENV_SENDER.into();
            c.name = "Envelope Sender".into();
            if let Some(g) = c.gen.as_mut() {
                g.raw = cereal_engine::fm::Sender::fresh();
            }
            self.song.channels.push(c);
            self.sel_chan = self.song.channels.len() - 1;
            self.dirty = true;
            ui.close_menu();
        }
        if ui.button("Key Controller").hint("A silent channel whose notes move the controls linked to it").clicked() {
            let mut c = Chan::instrument(cereal_engine::synth::Kind::Quiet);
            c.plugin = cereal_engine::synth::KEY_SENDER.into();
            c.name = "Key Controller".into();
            if let Some(g) = c.gen.as_mut() {
                cereal_engine::synth::key_sender_set(&mut g.raw, 255, 0.0);
            }
            self.song.channels.push(c);
            self.sel_chan = self.song.channels.len() - 1;
            self.dirty = true;
            ui.close_menu();
        }
        if ui.button("Pad Machine").clicked() {
            let mut c = Chan { name: "Pad Machine".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: cereal_engine::zones::Kit::IDENT.into(), ..Chan::default() };
            c.zones = cereal_engine::zones::Map::parse(&c.plugin, &cereal_engine::zones::Kit::fresh().write()).map(std::sync::Arc::new);
            self.song.channels.push(c);
            self.sel_chan = self.song.channels.len() - 1;
            self.dirty = true;
            ui.close_menu();
        }
        if ui.button("Sound Bank Player").clicked() {
            let mut c = Chan { name: "Sound Bank Player".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: "SoundFont Player".into(), ..Chan::default() };
            c.zones = cereal_engine::sf2::open(&c.plugin, &[], None, None).map(std::sync::Arc::new);
            self.song.channels.push(c);
            self.sel_chan = self.song.channels.len() - 1;
            self.dirty = true;
            ui.close_menu();
        }
    }
}
