//! The mixer: one strip per insert, the master first.

use crate::widgets::Hint as _;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::widgets::*;
use crate::App;


#[derive(Clone, Copy)]
enum SlotAct {
    Toggle(u16),
    Open(u16),
    Delete(u16),
    Move(u16, u16),
    Set(u16, cereal_engine::fx::Kind, &'static str),
    Host(u16),
}

/// The mixer's option switches as kept in the settings.
pub(crate) mod mxo {
    pub const AUTO_UNARM: u64 = 1;
    pub const AUTO_CLIP: u64 = 1 << 1;
    pub const LAT_COMP: u64 = 1 << 2;
    pub const AUTO_NAME: u64 = 1 << 3;
    pub const GROUP_TAKES: u64 = 1 << 4;
    /// Two bits: 16 bit, 24 bit, 32 bit float.
    pub const FORMAT_SHIFT: u32 = 5;
    pub const PDC_AUTOM: u64 = 1 << 7;
    pub const NAMES_TOP: u64 = 1 << 8;
    pub const CABLES: u64 = 1 << 9;
    pub const COMPACT_LIST: u64 = 1 << 10;
    pub const SMALL_TEXT: u64 = 1 << 11;
    pub const INSPECTOR_LEFT: u64 = 1 << 12;
    /// Two bits: off, low, medium, high.
    pub const COLOR_SHIFT: u32 = 13;
    pub const ALT_HL: u64 = 1 << 15;
    pub const LINES: u64 = 1 << 16;
    pub const EXT_LEVELS: u64 = 1 << 17;
    pub const MASTER_LAT: u64 = 1 << 18;
    pub const THREADED: u64 = 1 << 19;
    pub const AUTO_COLOR: u64 = 1 << 20;
    pub const DEFAULT: u64 = AUTO_CLIP | LAT_COMP | AUTO_NAME | GROUP_TAKES | PDC_AUTOM | CABLES | COMPACT_LIST | (1 << COLOR_SHIFT) | LINES | EXT_LEVELS | MASTER_LAT | THREADED;
}

impl App {
    pub(crate) fn mixer_ui(&mut self, ui: &mut Ui) {
        let n = self.song.inserts.len();
        self.mixer_sel.retain(|i| *i < n);
        if self.peaks.len() != n {
            self.peaks = vec![(0.0, 0.0); n];
        }
        if self.peak_top.len() != n {
            self.peak_top = vec![0.0; n];
        }
        if self.peak_hist.len() != n {
            self.peak_hist = vec![Default::default(); n];
        }
        for (i, p) in self.peaks.iter_mut().enumerate() {
            let (l, r) = self.handle.transport.take_peak(i);
            if self.mixer_wave {
                let h = &mut self.peak_hist[i];
                h.push_front(l.max(r));
                h.truncate(110);
            }
            self.peak_top[i] = self.peak_top[i].max(l.max(r));
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
        let font = FontId::proportional(if self.settings.mixer_view == 1 { 9.0 } else { 10.5 });
        let mut route: Option<(usize, usize, bool)> = None;
        let sw = self.strip_w();
        ui.horizontal(|ui| {
            ui.menu_button("\u{23F7}", |ui| self.mixer_menu(ui)).response.hint("Mixer menu");
            if ui.selectable_label(self.mixer_wave, "Waves").hint("Waveforms in the meters (Alt+W)").clicked() {
                self.mixer_wave = !self.mixer_wave;
            }
            if ui.selectable_label(self.settings.mixer_big, "Meter").hint("A tall meter for the selected track at the left of the mixer").clicked() {
                self.settings.mixer_big = !self.settings.mixer_big;
                let _ = self.settings.save();
            }
            let names = ["Wide", "Compact", "Extra large"];
            egui::ComboBox::from_id_salt("mixer-layout").width(86.0).selected_text(names[(self.settings.mixer_view as usize).min(2)]).show_ui(ui, |ui| {
                for k in [1u8, 0, 2] {
                    if ui.selectable_label(self.settings.mixer_view == k, names[k as usize]).clicked() {
                        self.settings.mixer_view = k;
                        let _ = self.settings.save();
                    }
                }
            });
        });
        ui.horizontal_top(|ui| {
            if self.settings.mixer_big {
                self.big_meter(ui);
            }
            let side = 190.0;
            let held = |d: u8| -> Vec<usize> { (0..n).filter(|&i| self.song.inserts[i].dock.min(2) == d).collect() };
            let (left, middle, right) = (held(0), held(1), held(2));
            let width_of = |v: &[usize]| -> f32 { v.iter().map(|&i| sw + 2.0 + if self.song.inserts[i].separator { 8.0 } else { 0.0 }).sum() };
            let (lw, rw) = (width_of(&left), width_of(&right));
            ui.spacing_mut().item_spacing.x = 2.0;
            for &i in &left {
                self.strip(ui, i, n, used[i], &font, &mut route);
            }
            if !left.is_empty() {
                ui.separator();
            }
            let strips_w = (ui.available_width() - side - rw - 20.0).max(120.0);
            let _ = lw;
            ui.allocate_ui(Vec2::new(strips_w, ui.available_height()), |ui| {
                egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.horizontal_top(|ui| {
                        for &i in &middle {
                            self.strip(ui, i, n, used[i], &font, &mut route);
                        }
                    });
                });
            });
            if !right.is_empty() {
                ui.separator();
            }
            for &i in &right {
                self.strip(ui, i, n, used[i], &font, &mut route);
            }
            ui.spacing_mut().item_spacing.x = 8.0;

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
                        if let Some(p) = resp.dnd_release_payload::<crate::browser::PlugDrag>() {
                            if let crate::browser::PlugDrag::Fx(kind, ident) = *p {
                                act = Some(SlotAct::Set(slot, kind, ident));
                            }
                        }
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
                                let mut defs = Self::effect_defs();
                                let favs = defs.iter().filter(|d| self.settings.plugfav.contains(&format!("f:{}", d.title))).count();
                                defs.sort_by_key(|d| !self.settings.plugfav.contains(&format!("f:{}", d.title)));
                                for (n, d) in defs.into_iter().enumerate() {
                                    if n == favs && favs > 0 {
                                        ui.separator();
                                    }
                                    if ui.button(d.title).clicked() {
                                        act = Some(SlotAct::Set(slot, d.kind, d.ident));
                                    }
                                }
                            });
                        });
                        if filled {
                            resp.hint("Click to open, click the light to switch on or off, right-click for more");
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
                            SlotAct::Set(s, kind, ident) => self.set_effect(i, s, kind, ident),
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
                        .hint("The sound card input heard on this track. It passes the track's effects, so a recorder in a slot takes it down.");
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
                                    changed |= knob(ui, &mut band.gain_db, -18.0, 18.0, 0.0, 22.0).hint(format!("{} gain {:+.1} dB", ["Low", "Mid", "High"][b], band.gain_db)).changed();
                                    changed |= knob(ui, &mut band.freq, 0.0, 1.0, def[b].freq, 16.0).hint(format!("Frequency {hz:.0} Hz")).changed();
                                    changed |= knob(ui, &mut band.q, 0.0, 1.0, def[b].q, 16.0).hint("Width").changed();
                                });
                            }
                            ui.vertical(|ui| {
                                changed |= knob(ui, &mut ins.sep, -1.0, 1.0, 0.0, 22.0).hint(format!("Stereo separation {:+.0} %", -ins.sep * 100.0)).changed();
                                changed |= ui.selectable_label(ins.invert, RichText::new("INV").small()).hint("Reverse polarity").clicked().then(|| ins.invert = !ins.invert).is_some();
                                changed |= ui.selectable_label(ins.swap, RichText::new("SWAP").small()).hint("Swap left and right").clicked().then(|| ins.swap = !ins.swap).is_some();
                            });
                        });
                    }
                    if changed {
                        self.dirty = true;
                    }
                    ui.add_space(4.0);
                    {
                        let ms = self.song.inserts[i].latency_ms;
                        let title = if ms == 0.0 { "Latency: none".to_string() } else { format!("Latency: {ms:.2} ms") };
                        let mut set = None;
                        ui.menu_button(RichText::new(title).small(), |ui| {
                            if ui.radio(ms == 0.0, "None").clicked() {
                                set = Some(0.0);
                                ui.close_menu();
                            }
                            ui.separator();
                            let mut v = ms;
                            ui.horizontal(|ui| {
                                ui.label("Type in value");
                                if ui.add(egui::DragValue::new(&mut v).range(0.0..=2000.0).speed(0.1).suffix(" ms")).changed() {
                                    set = Some(v);
                                }
                            });
                            let block = (if self.settings.buffer == 0 { 512 } else { self.settings.buffer }) as f32 / (if self.settings.rate == 0 { 44100 } else { self.settings.rate }) as f32 * 1000.0;
                            if ui.button("Set from mixing buffer").clicked() {
                                set = Some(block);
                                ui.close_menu();
                            }
                        })
                        .response
                        .hint("How late this track's plug-ins answer, set by hand; the other tracks wait for it");
                        if let Some(v) = set {
                            self.song.inserts[i].latency_ms = v;
                            self.song.inserts[i].latency_real = false;
                            self.dirty = true;
                        }
                    }
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
            let (only, side) = (std::mem::take(&mut self.route_only), std::mem::take(&mut self.route_side));
            let ins = &mut self.song.inserts[src];
            if on && only && !reach.contains(&src) {
                ins.sends = vec![dst];
                self.dirty = true;
            } else if !on {
                ins.sends.retain(|&d| d != dst);
                self.dirty = true;
            } else if !reach.contains(&src) {
                if !ins.sends.contains(&dst) {
                    ins.sends.push(dst);
                }
                ins.sends.sort_unstable();
                if side {
                    // heard by the other track's effects only, not in its sound
                    ins.set_send_level(dst, 0.0);
                }
                self.dirty = true;
            } else {
                self.status = "That routing would feed the track back into itself".into();
            }
        }
    }
}

impl App {
    /// One mixer track as a strip: name plate, mute light, pan, fader with its meter, and the routing arrow.
    /// How wide a track is in the chosen view.
    fn strip_w(&self) -> f32 {
        [50.0, 36.0, 78.0][(self.settings.mixer_view as usize).min(2)]
    }

    /// The selected track's level, both sides, tall and with a scale.
    fn big_meter(&mut self, ui: &mut Ui) {
        let h = (ui.available_height() - 8.0).clamp(120.0, 330.0);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(64.0, h), Sense::click());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, PANEL_DARK);
        let i = self.sel_insert.min(self.peaks.len().saturating_sub(1));
        let (l, r) = self.peaks.get(i).copied().unwrap_or((0.0, 0.0));
        // from 60 below full level at the bottom to 6 over at the top
        let y_of = |db: f32| rect.bottom() - 4.0 - ((db + 60.0) / 66.0).clamp(0.0, 1.0) * (rect.height() - 22.0);
        for db in [6, 0, -6, -12, -18, -24, -36, -48, -60] {
            let y = y_of(db as f32);
            p.line_segment([egui::Pos2::new(rect.left() + 3.0, y), egui::Pos2::new(rect.left() + 8.0, y)], Stroke::new(1.0_f32, LINE_SOFT));
            p.text(egui::Pos2::new(rect.left() + 10.0, y), Align2::LEFT_CENTER, db.to_string(), FontId::proportional(8.5), TEXT_DIM);
        }
        for (k, v) in [l, r].into_iter().enumerate() {
            let x = rect.left() + 30.0 + k as f32 * 15.0;
            let db = 20.0 * v.max(1e-6).log10();
            let bar = egui::Rect::from_min_max(egui::Pos2::new(x, y_of(db)), egui::Pos2::new(x + 12.0, rect.bottom() - 4.0));
            p.rect_filled(egui::Rect::from_min_max(egui::Pos2::new(x, y_of(6.0)), bar.max), 1.0, PANEL);
            p.rect_filled(bar, 1.0, GREEN);
            if db > 0.0 {
                p.rect_filled(egui::Rect::from_min_max(bar.min, egui::Pos2::new(x + 12.0, y_of(0.0))), 1.0, Color32::from_rgb(240, 80, 70));
            }
        }
        let top = self.peak_top.get(i).copied().unwrap_or(0.0);
        let text = if top <= 1e-5 { "-inf".to_string() } else { format!("{:.1}", 20.0 * top.log10()) };
        p.text(rect.center_top() + Vec2::new(0.0, 3.0), Align2::CENTER_TOP, text, FontId::proportional(11.0), if top > 1.0 { Color32::from_rgb(240, 80, 70) } else { TEXT });
        if resp.hint("Level of the selected track; the number is its highest peak (click to reset)").clicked() {
            self.peak_top.iter_mut().for_each(|p| *p = 0.0);
        }
    }

    fn strip(&mut self, ui: &mut Ui, i: usize, n: usize, used: usize, font: &FontId, route: &mut Option<(usize, usize, bool)>) {
        let sw = self.strip_w();
        let view = (self.settings.mixer_view as usize).min(2);
        let selected = self.sel_insert == i || self.mixer_sel.contains(&i);
        let name = self.song.insert_name(i);
        let frame = egui::Frame::new().fill(if selected { PANEL.lerp_to_gamma(Color32::WHITE, 0.08) } else { PANEL }).inner_margin(3.0).corner_radius(3.0);
        let inner = frame.show(ui, |ui| {
            ui.set_width(sw - 6.0);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                // name plate
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(sw - 6.0, 30.0), Sense::click());
                let col = self.song.inserts[i].color.map(rgb).unwrap_or(if i == 0 { Color32::from_rgb(110, 96, 70) } else { STEP_A });
                ui.painter().rect_filled(rect, 2.0, col);
                ui.painter().text(rect.center_top() + Vec2::new(0.0, 2.0), Align2::CENTER_TOP, if i == 0 { "M".to_string() } else { i.to_string() }, font.clone(), TEXT_DIM);
                ui.painter().text(rect.center_bottom() - Vec2::new(0.0, 2.0), Align2::CENTER_BOTTOM, fit(ui, &name, rect.width() - 2.0, &font), font.clone(), Color32::from_gray(240));
                if self.mixer_armed.contains(&i) {
                    ui.painter().circle_filled(rect.right_top() + Vec2::new(-5.0, 5.0), 3.0, Color32::from_rgb(235, 70, 60));
                }
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
                // the right button picks the track and opens the mixer's menu for it
                if resp.secondary_clicked() && !self.mixer_sel.contains(&i) {
                    self.mixer_sel.clear();
                    self.sel_insert = i;
                }
                resp.context_menu(|ui| self.mixer_menu(ui));
                let ins = &mut self.song.inserts[i];
                let before = (ins.volume, ins.pan, ins.enabled);
                if led(ui, ins.enabled, 14.0).hint("Mute").clicked() {
                    ins.enabled = !ins.enabled;
                    self.dirty = true;
                }
                let r = knob(ui, &mut ins.pan, -1.0, 1.0, 0.0, [20.0, 15.0, 26.0][view]).hint("Panning");
                control_menu(&r, cereal_engine::song::Target::InsertPan(i));
                if r.changed() {
                    self.dirty = true;
                }
                {
                // the highest level seen since the last click on it
                let top = self.peak_top.get(i).copied().unwrap_or(0.0);
                let text = if top <= 1e-5 { "-inf".to_string() } else { format!("{:.1}", 20.0 * top.log10()) };
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(sw - 6.0, 11.0), Sense::click());
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(9.0), if top > 1.0 { Color32::from_rgb(240, 80, 70) } else { TEXT_DIM });
                if resp.hint("Highest peak in dB (click to reset)").clicked() {
                    self.peak_top.iter_mut().for_each(|p| *p = 0.0);
                }
            }
            let h = (ui.available_height() - 56.0).clamp(60.0, 220.0);
                let r = fader(ui, &mut ins.volume, 1.25, 1.0, Vec2::new([16.0, 11.0, 28.0][view], h), if self.mixer_wave { (0.0, 0.0) } else { self.peaks[i] });
                if self.mixer_wave {
                    // the sound of the last moments, newest at the top, as a wave down the meter
                    let (p, at) = (ui.painter(), r.rect);
                    for (k, a) in self.peak_hist[i].iter().enumerate() {
                        let y = at.top() + 1.0 + k as f32 * 2.0;
                        if y > at.bottom() - 1.0 {
                            break;
                        }
                        let w = (a.min(1.0).sqrt() * (at.width() * 0.5 - 1.0)).max(0.5);
                        let col = if *a > 1.0 { Color32::from_rgb(240, 80, 70) } else { GREEN.gamma_multiply(0.6) };
                        p.line_segment([egui::Pos2::new(at.center().x - w, y), egui::Pos2::new(at.center().x + w, y)], Stroke::new(1.5_f32, col));
                    }
                }
                control_menu(&r, cereal_engine::song::Target::InsertVolume(i));
                if r.changed() {
                    self.dirty = true;
                }
                let db = 20.0 * cereal_engine::law::fader_gain(ins.volume).max(1e-6).log10();
                r.hint(if ins.volume <= 0.0 { "-inf dB".to_string() } else { format!("{db:.1} dB") });
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
                let dot = if used > 0 { GREEN } else { PANEL_DARK };
                let (rect, _) = ui.allocate_exact_size(Vec2::new(sw - 6.0, 8.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.0, dot);
                // routing from the selected insert into this one
                let src = self.sel_insert;
                if src != i && src != 0 && src < n {
                    let routed = self.song.inserts[src].sends.contains(&i);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(sw - 6.0, 12.0), Sense::click());
                    let c = rect.center();
                    let col = if routed { ACCENT } else if resp.hovered() { TEXT } else { TEXT_DIM.gamma_multiply(0.5) };
                    ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-5.0, 4.0), c + Vec2::new(5.0, 4.0), c + Vec2::new(0.0, -4.0)], col, Stroke::NONE));
                    let resp = resp.hint(if routed { "Routed from the selected track (click to remove)" } else { "Route the selected track here" });
                    if resp.clicked() {
                        *route = Some((src, i, !routed));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Route to this track only").clicked() {
                            *route = Some((src, i, true));
                            self.route_only = true;
                            ui.close_menu();
                        }
                        if ui.button("Sidechain to this track").clicked() {
                            *route = Some((src, i, true));
                            self.route_side = true;
                            ui.close_menu();
                        }
                    });
                    if routed {
                        let mut lv = self.song.inserts[src].send_level(i);
                        if knob(ui, &mut lv, 0.0, 1.25, 1.0, 16.0).hint(format!("Send level {:.0} %", lv * 100.0)).changed() {
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
        if let Some(p) = inner.response.dnd_release_payload::<crate::browser::PlugDrag>() {
            if let crate::browser::PlugDrag::Fx(kind, ident) = *p {
                self.effect_to_free_slot(i, kind, ident);
            }
        }
        if self.song.inserts[i].separator {
            ui.add_space(8.0);
        }
    }
}

impl App {
    /// Put a built-in effect in a mixer slot and open it.
    pub(crate) fn set_effect(&mut self, insert: usize, slot: u16, kind: cereal_engine::fx::Kind, ident: &str) {
        let Some(ins) = self.song.inserts.get_mut(insert) else { return };
        let slots = &mut ins.slots;
        slots.retain(|x| x.index != slot);
        let mut new = cereal_engine::song::Slot::with_fx(slot, kind);
        // several units share a kind; the name is what tells them apart
        new.plugin = ident.to_string();
        if let (cereal_engine::fx::Kind::Notes, Some(f), Some(fl)) = (kind, &mut new.fx, cereal_engine::notes::flavour_of(ident)) {
            f.p = cereal_engine::notes::Notes { flavour: fl, active: 1, pages: vec![(1, String::new())] }.pack();
        }
        slots.push(new);
        slots.sort_by_key(|x| x.index);
        self.fx_open = Some((insert, slot));
    }

    /// The built-in effects a slot can take, by title.
    pub(crate) fn effect_defs() -> Vec<&'static cereal_engine::fx::Def> {
        let mut defs: Vec<&cereal_engine::fx::Def> = cereal_engine::fx::DEFS.iter().filter(|d| !d.ident.starts_with('(')).filter(|d| d.kind != cereal_engine::fx::Kind::Thru || ["Tuner", "Wave Candy", "Edison", "Fruity dB Meter", "Fruity Spectroman", "Fruity Big Clock"].contains(&d.ident)).collect();
        defs.sort_by_key(|d| d.title);
        defs
    }
}


impl App {
    /// The selected tracks play into the first empty track and nowhere else.
    /// The regular tracks: all but the master (first) and the one that follows the selection (last).
    fn last_regular(&self) -> usize {
        self.song.inserts.len().saturating_sub(2)
    }

    /// Empty mixer track `k`: nothing plays into it, it holds nothing and plays into the master.
    fn clear_track(&mut self, k: usize) {
        for c in &mut self.song.channels {
            if c.insert == k {
                c.insert = 0;
            }
        }
        for ins in &mut self.song.inserts {
            ins.sends.retain(|d| *d != k);
        }
        self.song.inserts[k] = cereal_engine::song::Insert { sends: vec![0], ..Default::default() };
        self.mixer_armed.remove(&k);
    }

    /// Take the selected tracks out; the ones after them move up and empty ones come in at the end.
    pub(crate) fn mixer_delete_tracks(&mut self) {
        let last = self.last_regular();
        let i = self.sel_insert;
        let mut gone: Vec<usize> = if self.mixer_sel.contains(&i) { self.mixer_sel.iter().copied().collect() } else { vec![i] };
        gone.retain(|&k| k > 0 && k <= last);
        gone.sort_unstable();
        if gone.is_empty() {
            return;
        }
        for &k in gone.iter().rev() {
            self.clear_track(k);
            for j in k..last {
                self.song.swap_inserts(j, j + 1);
            }
        }
        self.mixer_sel.clear();
        self.wave_eds.clear();
        self.dirty = true;
        self.status = if gone.len() == 1 { "Mixer track deleted".into() } else { format!("{} mixer tracks deleted", gone.len()) };
    }

    /// Put `count` empty tracks in before the selected one; as many drop off the end, which have to be unused.
    pub(crate) fn mixer_insert_tracks(&mut self, count: usize) {
        let last = self.last_regular();
        let at = self.sel_insert;
        if at == 0 || at > last || count == 0 {
            return;
        }
        let count = count.min(last + 1 - at);
        let busy = (last + 1 - count..=last).any(|k| {
            let x = &self.song.inserts[k];
            !x.slots.is_empty() || self.song.channels.iter().any(|c| c.insert == k) || self.song.inserts.iter().any(|o| o.sends.contains(&k))
        });
        if busy {
            self.status = "The last mixer tracks are in use: there is no room to insert".into();
            return;
        }
        for _ in 0..count {
            self.clear_track(last);
            for j in (at + 1..=last).rev() {
                self.song.swap_inserts(j, j - 1);
            }
        }
        self.wave_eds.clear();
        self.dirty = true;
        self.status = if count == 1 { "Mixer track inserted".into() } else { format!("{count} mixer tracks inserted") };
    }

    pub(crate) fn create_submix(&mut self) {
        let n_ins = self.song.inserts.len();
        let i = self.sel_insert;
        let mut from: Vec<usize> = if self.mixer_sel.contains(&i) { self.mixer_sel.iter().copied().collect() } else { vec![i] };
        from.retain(|&k| k > 0 && k < n_ins);
        from.sort_unstable();
        let fed = |d: usize| self.song.inserts.iter().enumerate().any(|(k, x)| k != d && x.sends.contains(&d));
        let empty = |d: usize| {
            let x = &self.song.inserts[d];
            !from.contains(&d) && x.slots.is_empty() && x.name.is_empty() && self.song.channels.iter().all(|c| c.insert != d) && !fed(d)
        };
        let after = from.last().copied().unwrap_or(i);
        let last = n_ins.saturating_sub(1);
        match (after + 1..last).chain(1..after).find(|&d| empty(d)) {
            Some(d) => {
                for &k in &from {
                    self.song.inserts[k].sends = vec![d];
                    self.song.inserts[k].levels.clear();
                }
                self.song.inserts[d].name = "Submix".into();
                self.song.inserts[d].color = self.song.inserts[from[0]].color;
                self.sel_insert = d;
                self.mixer_sel.clear();
                self.dirty = true;
            }
            None => self.status = "No empty mixer track for the submix".into(),
        }
    }
}
