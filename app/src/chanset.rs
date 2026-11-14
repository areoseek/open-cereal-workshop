//! The channel settings window: sample treatment, envelopes and filter, and the miscellaneous note functions of
//! the selected sampler channel.

use crate::widgets::Hint as _;
use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::flp::ChannelKind;
use cereal_engine::song::{Env, Sampler};

use crate::widgets::*;
use crate::App;

#[derive(Default, Clone, Copy, PartialEq)]
enum Tab {
    #[default]
    Sample,
    Envelope,
    Misc,
}

impl State {
    /// Open on the Miscellaneous page.
    pub fn show_misc(&mut self) {
        self.tab = Tab::Misc;
    }
}

#[derive(Default)]
pub struct State {
    tab: Tab,
    /// 0 volume, 1 filter cutoff, 2 pitch.
    env: usize,
}

const KEY_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub fn key_name(key: u8) -> String {
    format!("{}{}", KEY_NAMES[key as usize % 12], key / 12)
}

/// The controls of an instrument stored as a list of numbers, group by group. True when one was moved.
fn float_panel(ui: &mut Ui, ci: usize, g: &mut cereal_engine::synth::Gen, groups: &[&str], params: &[cereal_engine::duo::Info]) -> bool {
    float_panel_at(ui, ci, g, groups, params, 0)
}

/// The same for a run of settings that starts `base` numbers into the list.
fn float_panel_at(ui: &mut Ui, ci: usize, g: &mut cereal_engine::synth::Gen, groups: &[&str], params: &[cereal_engine::duo::Info], base: usize) -> bool {
    use cereal_engine::duo::Ctl;
    let mut changed = false;
    for (gi, title) in groups.iter().enumerate() {
        ui.label(RichText::new(*title).small().color(TEXT_DIM));
        ui.horizontal_wrapped(|ui| {
            for (i, info) in params.iter().enumerate().filter(|(_, p)| p.group as usize == gi) {
                let mut v = g.p[base + i];
                match info.ctl {
                    Ctl::Knob => {
                        if cap_knob(ui, &info.name.to_uppercase(), &mut v, 0.0, 1.0, info.def, |v| format!("{:.0} %", v * 100.0)) {
                            g.p[base + i] = v;
                            changed = true;
                        }
                    }
                    Ctl::Switch => {
                        let mut on = v >= 0.5;
                        if ui.toggle_value(&mut on, info.name).changed() {
                            g.p[base + i] = if on { 1.0 } else { 0.0 };
                            changed = true;
                        }
                    }
                    Ctl::Pick(names) => {
                        let now = cereal_engine::duo::pick(v, names.len());
                        egui::ComboBox::from_id_salt(("floats", ci, base + i)).selected_text(format!("{}: {}", info.name, names[now])).show_ui(ui, |ui| {
                            for (k, name) in names.iter().enumerate() {
                                if ui.selectable_label(k == now, *name).clicked() && k != now {
                                    g.p[base + i] = k as f32 / (names.len() - 1) as f32;
                                    changed = true;
                                }
                            }
                        });
                    }
                    Ctl::Whole(top) => {
                        let mut n = v.round().clamp(0.0, top as f32) as u32 + 1;
                        if ui.add(egui::DragValue::new(&mut n).range(1..=top + 1).prefix(format!("{} ", info.name))).changed() {
                            g.p[base + i] = (n - 1) as f32;
                            changed = true;
                        }
                    }
                    Ctl::Choice(names) => {
                        let now = (v.round().max(0.0) as usize).min(names.len() - 1);
                        egui::ComboBox::from_id_salt(("floats", ci, base + i)).selected_text(format!("{}: {}", info.name, names[now])).show_ui(ui, |ui| {
                            for (k, name) in names.iter().enumerate() {
                                if ui.selectable_label(k == now, *name).clicked() && k != now {
                                    g.p[base + i] = k as f32;
                                    changed = true;
                                }
                            }
                        });
                    }
                    Ctl::Tenths(names) => {
                        let now = ((v * 10.0).round().max(0.0) as usize).min(names.len() - 1);
                        egui::ComboBox::from_id_salt(("floats", ci, base + i)).selected_text(format!("{}: {}", info.name, names[now])).show_ui(ui, |ui| {
                            for (k, name) in names.iter().enumerate() {
                                if ui.selectable_label(k == now, *name).clicked() && k != now {
                                    g.p[base + i] = k as f32 / 10.0;
                                    changed = true;
                                }
                            }
                        });
                    }
                    Ctl::Kept => {}
                }
            }
        });
        ui.add_space(3.0);
    }
    changed
}

/// A knob with a caption under it. Returns true when the value moved.
fn cap_knob(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, default: f32, hint: impl Fn(f32) -> String) -> bool {
    cap_knob_for(ui, label, v, min, max, default, hint, None)
}

/// As `cap_knob`, for a control that clips and events can drive.
#[allow(clippy::too_many_arguments)]
fn cap_knob_for(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, default: f32, hint: impl Fn(f32) -> String, target: Option<cereal_engine::song::Target>) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.set_width(44.0);
        ui.vertical_centered(|ui| {
            let r = knob(ui, v, min, max, default, 26.0).hint(hint(*v));
            if let Some(t) = target {
                control_menu(&r, t);
            }
            changed = r.changed();
            ui.label(RichText::new(label).small().color(TEXT_DIM));
        });
    });
    changed
}

fn secs(v: f32) -> String {
    if v < 1.0 {
        format!("{:.0} ms", v * 1000.0)
    } else {
        format!("{v:.2} s")
    }
}

/// Envelope times sit on a cubic knob so the short end has room.
fn time_knob(ui: &mut Ui, label: &str, v: &mut f32) -> bool {
    let mut x = (*v / 16.0).cbrt();
    let hit = cap_knob(ui, label, &mut x, 0.0, 1.0, 0.0, |v: f32| secs(16.0 * v * v * v));
    if hit {
        *v = 16.0 * x * x * x;
    }
    hit
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).small().strong().color(TEXT_DIM));
}

fn draw_env(ui: &mut Ui, e: &Env, on: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 90.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, PANEL_DARK);
    let held = e.delay + e.attack + e.hold + e.decay;
    let sustain_for = (held * 0.3).max(0.15);
    let total = (held + sustain_for + e.release).max(0.05);
    let n = rect.width() as usize;
    let mut pts = Vec::with_capacity(n);
    let top = held + sustain_for;
    let from = e.held(top);
    for i in 0..n {
        let t = i as f32 / n as f32 * total;
        let y = if t < top { e.held(t) } else { e.released(from, t - top).unwrap_or(0.0) };
        pts.push(Pos2::new(rect.left() + i as f32, rect.bottom() - 4.0 - y * (rect.height() - 8.0)));
    }
    p.add(egui::Shape::line(pts, Stroke::new(1.5_f32, if on { ACCENT } else { TEXT_DIM })));
    let x = rect.left() + top / total * rect.width();
    p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
}

fn draw_wave(ui: &mut Ui, frames: &[[f32; 2]], color: Color32) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 70.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, PANEL_DARK);
    if frames.is_empty() {
        return rect;
    }
    let n = rect.width() as usize;
    let mid = rect.center().y;
    let half = rect.height() * 0.5 - 2.0;
    for i in 0..n {
        let a = i * frames.len() / n;
        let b = ((i + 1) * frames.len() / n).max(a + 1).min(frames.len());
        let stride = ((b - a) / 64).max(1);
        let (mut lo, mut hi) = (0f32, 0f32);
        for f in frames[a..b].iter().step_by(stride) {
            let m = (f[0] + f[1]) * 0.5;
            lo = lo.min(m);
            hi = hi.max(m);
        }
        let x = rect.left() + i as f32;
        p.line_segment([Pos2::new(x, mid - hi.min(1.0) * half), Pos2::new(x, mid - lo.max(-1.0) * half + 1.0)], Stroke::new(1.0_f32, color));
    }
    rect
}

impl App {
    pub(crate) fn chanset_ui(&mut self, ui: &mut Ui) {
        let ci = self.sel_chan;
        if ci >= self.song.channels.len() {
            ui.label("No channel selected.");
            return;
        }
        let mut changed = false;
        let mut rebake = false;
        let n_ins = self.song.inserts.len().saturating_sub(1);
        let kind = self.song.channels[ci].kind;

        // the strip every channel has: level, pan, pitch, mixer track
        ui.horizontal(|ui| {
            ui.menu_button("\u{23F7}", |ui| {
                if crate::menus::mi(ui, "Open channel state...", "").clicked() {
                    self.state_ask = Some(crate::StateAsk::OpenChan(ci));
                    ui.close_menu();
                }
                if crate::menus::mi(ui, "Save preset as...", "").clicked() {
                    self.state_ask = Some(crate::StateAsk::SaveChan(ci));
                    ui.close_menu();
                }
            })
            .response
            .hint("Channel menu");
            let c = &mut self.song.channels[ci];
            use cereal_engine::song::Target;
            changed |= cap_knob_for(ui, "PAN", &mut c.pan, -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0), Some(Target::ChanPan(ci)));
            changed |= cap_knob_for(ui, "VOL", &mut c.volume, 0.0, 1.0, 10000.0 / 12800.0, |v: f32| format!("{:.0} %", v * 128.0), Some(Target::ChanVolume(ci)));
            changed |= cap_knob_for(ui, "PITCH", &mut c.pitch_cents, -1200.0, 1200.0, 0.0, |v: f32| format!("{:+.0} cents", v), Some(Target::ChanPitch(ci)));
            ui.vertical(|ui| {
                ui.set_width(52.0);
                let mut ins = c.insert;
                let dv = egui::DragValue::new(&mut ins).range(0..=n_ins).speed(0.1).custom_formatter(|v, _| if v < 0.5 { "---".into() } else { format!("{v:.0}") });
                if ui.add_sized([44.0, 22.0], dv).changed() {
                    c.insert = ins;
                    changed = true;
                }
                ui.label(RichText::new("TRACK").small().color(TEXT_DIM));
            });
            ui.vertical(|ui| {
                ui.label(RichText::new(&c.name).strong());
                let what = match c.kind {
                    ChannelKind::Sampler => "Sampler".to_string(),
                    ChannelKind::AudioClip => "Audio clip".to_string(),
                    ChannelKind::Layer => "Layer".to_string(),
                    ChannelKind::AutomationClip => "Automation clip".to_string(),
                    _ if c.bay().is_some() => format!("Patch Bay ({})", c.gen.as_ref().map_or("", |g| cereal_engine::synth::def(g.kind).title)),
                    _ if c.ext.is_some() => c.ext.as_ref().map(|h| h.0.found.name.clone()).unwrap_or_default(),
                    _ if c.missing_plugin() => format!("{} (not installed)", cereal_engine::host::block_name(c.gen.as_ref().map_or(&[][..], |g| &g.raw)).unwrap_or_else(|| "Plug-in".into())),
                    _ => cereal_engine::synth::title_for(&c.plugin).map(str::to_string).unwrap_or_else(|| c.plugin.clone()),
                };
                ui.label(RichText::new(what).small().color(TEXT_DIM));
            });
        });
        ui.separator();
        // a bay is played from its boards: they come before the instrument inside
        if self.song.channels[ci].bay().is_some() {
            let mut moved = false;
            if let Some(sub) = self.song.channels[ci].bay.as_mut().and_then(|b| b.fx.sub.as_mut()) {
                for i in 0..sub.nodes.len() {
                    let Some(mut board) = sub.nodes[i].fx.clone().filter(|g| g.kind == cereal_engine::fx::Kind::Surface && !g.p.is_empty()) else { continue };
                    if crate::fxwin::surface_ui(ui, &mut board, 560.0) {
                        std::sync::Arc::make_mut(sub).nodes[i].fx = Some(board);
                        moved = true;
                    }
                }
                // the units on the note cables, each with its settings
                for i in 0..sub.nodes.len() {
                    if sub.nodes[i].notes.is_none() {
                        continue;
                    }
                    let ident = sub.nodes[i].ident.clone();
                    let head = format!("{}. {}", i + 1, crate::fxwin::note_unit_title(&ident));
                    egui::CollapsingHeader::new(RichText::new(head).small()).id_salt(("chanbaynote", ci, i)).show(ui, |ui| {
                        let mut block = sub.nodes[i].notes.clone().unwrap();
                        if crate::fxwin::note_unit_ui(ui, &ident, &mut block) && std::sync::Arc::make_mut(sub).set_note_block(i, block) {
                            moved = true;
                        }
                    });
                }
            }
            if moved {
                self.dirty = true;
            }
        }
        let built_in = self.song.channels[ci].gen.is_some();
        if !built_in && !matches!(kind, ChannelKind::Sampler | ChannelKind::AudioClip) {
            let c = &self.song.channels[ci];
            match kind {
                ChannelKind::Layer => {
                    ui.label("This layer plays these channels:");
                    for k in &c.children {
                        if let Some(ch) = self.song.channels.get(*k as usize) {
                            ui.label(format!("  {}", ch.name));
                        }
                    }
                }
                ChannelKind::AutomationClip => {
                    ui.label("Automation clip. Its curve is drawn in the playlist.");
                }
                _ if c.zones.as_ref().is_some_and(|z| z.bank.is_some()) => {
                    // a sound bank player: the file, and the preset picked from it
                    let info = c.zones.as_ref().and_then(|z| z.bank.clone()).unwrap();
                    let count = c.zones.as_ref().map_or(0, |z| z.zones.len());
                    let mut want = info.setting.clone();
                    section(ui, "SOUND BANK");
                    ui.horizontal(|ui| {
                        if ui.button("Load...").hint("Pick a sound bank file (.sf2)").clicked() {
                            if let Some(f) = rfd::FileDialog::new().add_filter("Sound bank", &["sf2", "SF2"]).pick_file() {
                                want.path = f.to_string_lossy().to_string();
                                want.bank = 0;
                                want.patch = 0;
                            }
                        }
                        let name = info.setting.path.rsplit(['/', '\\']).next().unwrap_or("");
                        ui.label(if name.is_empty() { "(no file loaded)" } else { name });
                    });
                    if info.found {
                        let now = info.presets.iter().find(|p| (p.0, p.1) == (want.bank, want.patch)).or(info.presets.first());
                        let text = now.map_or(String::new(), |p| format!("{}:{}  {}", p.0, p.1, p.2));
                        ui.horizontal(|ui| {
                            ui.label("Patch");
                            egui::ComboBox::from_id_salt(("bank_patch", ci)).width(240.0).selected_text(text).show_ui(ui, |ui| {
                                for p in &info.presets {
                                    if ui.selectable_label((p.0, p.1) == (want.bank, want.patch), format!("{}:{}  {}", p.0, p.1, p.2)).clicked() {
                                        want.bank = p.0;
                                        want.patch = p.1;
                                    }
                                }
                            });
                        });
                        ui.label(RichText::new(format!("{count} zones in this patch.")).small().color(TEXT_DIM));
                    } else if !info.setting.path.is_empty() {
                        ui.label(RichText::new("The sound bank file was not found.").small().color(Color32::from_rgb(255, 120, 110)));
                    }
                    if want != info.setting {
                        let block = cereal_engine::sf2::with(&info.block, &want);
                        let dir = self.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
                        if let Some(m) = cereal_engine::sf2::open("SoundFont Player", &block, Some(&mut self.lib), dir.as_deref()) {
                            self.song.channels[ci].zones = Some(std::sync::Arc::new(m));
                            self.dirty = true;
                        }
                    }
                }
                _ if c.zones.as_ref().is_some_and(|z| z.kit.as_deref().and_then(cereal_engine::zones::Kit::parse).is_some()) => {
                    // a pad machine: its pads and their layers, all editable
                    let map = c.zones.clone().unwrap();
                    let (block, hit) = pad_kit_panel(ui, &map);
                    if let Some(k) = hit {
                        self.handle.send(cereal_engine::player::Cmd::NoteOn { chan: ci as u16, key: k.0, vel: k.1 });
                        self.handle.send(cereal_engine::player::Cmd::NoteOff { chan: ci as u16, key: k.0 });
                    }
                    if let Some(block) = block {
                        if let Some(mut m) = cereal_engine::zones::Map::parse(cereal_engine::zones::Kit::IDENT, &block) {
                            let dir = self.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
                            m.load(&mut self.lib, dir.as_deref());
                            self.song.channels[ci].zones = Some(std::sync::Arc::new(m));
                            changed = true;
                        }
                    }
                }
                _ if c.zones.is_some() => {
                    // a multi-sample player: one button per key that has sounds, its layers on hover
                    let map = c.zones.clone().unwrap();
                    let mut keys: Vec<u8> = map.zones.iter().map(|z| z.key_lo).collect();
                    keys.sort_unstable();
                    keys.dedup();
                    section(ui, "PADS");
                    let mut hit = None;
                    egui::Grid::new("pads").spacing([4.0, 4.0]).show(ui, |ui| {
                        for (i, k) in keys.iter().enumerate() {
                            let zs: Vec<_> = map.zones.iter().filter(|z| z.key_lo == *k).collect();
                            let missing = zs.iter().all(|z| z.sample.is_none());
                            let name = if zs[0].name.is_empty() { key_name(*k) } else { zs[0].name.clone() };
                            let b = ui.add_sized([92.0, 44.0], egui::Button::new(RichText::new(name).small().color(if missing { Color32::from_rgb(255, 120, 110) } else { TEXT })));
                            let tip = zs.iter().map(|z| format!("{}  (velocity {} to {})", z.path.rsplit(['/', '\\']).next().unwrap_or(""), z.vel_lo, z.vel_hi)).collect::<Vec<_>>().join("\n");
                            if b.hint(format!("{}\n{}", key_name(*k), tip)).clicked() {
                                hit = Some(*k);
                            }
                            if i % 4 == 3 {
                                ui.end_row();
                            }
                        }
                    });
                    if let Some(k) = hit {
                        self.handle.send(cereal_engine::player::Cmd::NoteOn { chan: ci as u16, key: k, vel: 1.0 });
                        self.handle.send(cereal_engine::player::Cmd::NoteOff { chan: ci as u16, key: k });
                    }
                    let lost = map.zones.iter().filter(|z| z.sample.is_none()).count();
                    if lost > 0 {
                        ui.label(RichText::new(format!("{lost} of {} sound files were not found.", map.zones.len())).small().color(Color32::from_rgb(255, 120, 110)));
                    }
                }
                _ if c.slices.is_some() => {
                    let sl = c.slices.clone().unwrap();
                    section(ui, "SLICES");
                    ui.label(RichText::new(format!("{}  ({} slices, {:.0} beats at {:.0} BPM)", sl.path.rsplit(['/', '\\']).next().unwrap_or(""), sl.cuts.len(), sl.beats, sl.bpm)).small().color(TEXT_DIM));
                    if let Some(s) = &c.sample {
                        let r = draw_wave(ui, &s.frames, rgb(c.color).lerp_to_gamma(Color32::WHITE, 0.4));
                        // the cuts over the wave, each with its number; reversed slices are tinted
                        let total = (s.frames.len() as f64 * sl.rate as f64 / s.rate.max(1) as f64).max(1.0);
                        // the first kind keeps its cuts in its own block, so they can be moved by hand: drag a
                        // cut line, right-click a slice to turn it around
                        if c.plugin.eq_ignore_ascii_case("Fruity Slicer") {
                            let resp = ui.interact(r, ui.id().with(("cuts", ci)), egui::Sense::click_and_drag());
                            let grab = egui::Id::new(("cut grab", ci));
                            let frame_at = |x: f32| (((x - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0) as f64 * total) as usize;
                            let mut edit = None;
                            if let Some(pos) = resp.interact_pointer_pos() {
                                if resp.drag_started() {
                                    let near = sl.cuts.iter().enumerate().skip(1).map(|(i, c)| (i, (r.left() + (c.pos as f64 / total) as f32 * r.width() - pos.x).abs())).filter(|&(_, d)| d < 5.0).min_by(|a, b| a.1.total_cmp(&b.1));
                                    ui.data_mut(|d| d.insert_temp(grab, near.map_or(usize::MAX, |n| n.0)));
                                }
                                let held: usize = ui.data(|d| d.get_temp(grab)).unwrap_or(usize::MAX);
                                if resp.dragged() && held < sl.cuts.len() {
                                    // a cut stays between its neighbours
                                    let lo = sl.cuts.iter().map(|c| c.pos).filter(|&p| p < sl.cuts[held].pos).max().map_or(1, |p| p + 1);
                                    let hi = sl.cuts.iter().map(|c| c.pos).filter(|&p| p > sl.cuts[held].pos).min().map_or(total as usize, |p| p).saturating_sub(1).max(lo);
                                    let to = frame_at(pos.x).clamp(lo, hi);
                                    if to != sl.cuts[held].pos {
                                        edit = Some((held, to, sl.cuts[held].reverse));
                                    }
                                }
                                if resp.secondary_clicked() {
                                    let f = frame_at(pos.x);
                                    if let Some((i, c)) = sl.cuts.iter().enumerate().filter(|(_, c)| c.pos <= f).max_by_key(|(_, c)| c.pos) {
                                        edit = Some((i, c.pos, !c.reverse));
                                    }
                                }
                            }
                            if resp.hovered() {
                                let over = ui.ctx().pointer_hover_pos().is_some_and(|p| sl.cuts.iter().skip(1).any(|c| (r.left() + (c.pos as f64 / total) as f32 * r.width() - p.x).abs() < 5.0));
                                if over {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                                }
                            }
                            if let Some((i, pos, reverse)) = edit {
                                let mut new = (*sl).clone();
                                new.cuts[i].pos = pos;
                                new.cuts[i].reverse = reverse;
                                let ch = &mut self.song.channels[ci];
                                ch.slices = Some(std::sync::Arc::new(new));
                                changed = true;
                            }
                        }
                        let sl = self.song.channels[ci].slices.clone().unwrap();
                        let p = ui.painter_at(r);
                        for (i, cut) in sl.cuts.iter().enumerate() {
                            let x = r.left() + (cut.pos as f64 / total) as f32 * r.width();
                            if cut.reverse {
                                let end = sl.cuts.iter().map(|c| c.pos).filter(|&p| p > cut.pos).min().map_or(r.right(), |e| r.left() + (e as f64 / total) as f32 * r.width());
                                p.rect_filled(egui::Rect::from_min_max(Pos2::new(x, r.top()), Pos2::new(end, r.bottom())), 0.0, Color32::from_rgba_unmultiplied(255, 140, 60, 28));
                            }
                            p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0_f32, ACCENT));
                            if sl.cuts.len() <= 48 {
                                p.text(Pos2::new(x + 2.0, r.top() + 1.0), egui::Align2::LEFT_TOP, format!("{}", i + 1), egui::FontId::proportional(8.0), TEXT_DIM);
                            }
                        }
                    } else {
                        ui.label(RichText::new("The loop file was not found.").small().color(Color32::from_rgb(255, 120, 110)));
                    }
                    let mut hit = None;
                    ui.horizontal_wrapped(|ui| {
                        for i in 0..sl.cuts.len() {
                            let key = if sl.cuts[i].key >= 0 { sl.cuts[i].key } else { sl.base + i as i32 };
                            if ui.small_button(format!("{}", i + 1)).hint(key_name(key.clamp(0, 127) as u8)).clicked() {
                                hit = Some(key.clamp(0, 127) as u8);
                            }
                        }
                    });
                    if let Some(k) = hit {
                        self.handle.send(cereal_engine::player::Cmd::NoteOn { chan: ci as u16, key: k, vel: 1.0 });
                        self.handle.send(cereal_engine::player::Cmd::NoteOff { chan: ci as u16, key: k });
                    }
                }
                _ if c.ext.is_some() => {
                    let h = c.ext.clone().unwrap();
                    let title = c.name.clone();
                    changed |= crate::hostui::panel(ui, &h.0, &title, &|k| cereal_engine::song::Target::ChanParam(ci, k));
                }
                _ => {
                    ui.label(format!("Instrument: {}", c.plugin));
                    ui.label(RichText::new("This instrument is not built in yet, so a plain stand-in voice plays its notes.").color(TEXT_DIM).small());
                }
            }
            if changed {
                self.dirty = true;
            }
            return;
        }

        ui.horizontal(|ui| {
            for (t, name) in [(Tab::Sample, if built_in { "Instrument" } else { "Sample" }), (Tab::Envelope, "Envelope / filter"), (Tab::Misc, "Miscellaneous")] {
                if ui.selectable_label(self.chanset.tab == t, name).clicked() {
                    self.chanset.tab = t;
                }
            }
        });
        ui.add_space(4.0);

        match self.chanset.tab {
            Tab::Sample if built_in => {
                let ident = self.song.channels[ci].plugin.clone();
                if let Some(p) = self.preset_bar(ui, &ident, true) {
                    self.actions.push(crate::Action::LoadPreset(p, crate::PresetTo::Chan(ci)));
                }
                ui.add_space(4.0);
                if ident.eq_ignore_ascii_case(cereal_engine::fm::ENV_SENDER) {
                    changed |= env_sender_panel(ui, &mut self.song.channels[ci].gen.as_mut().unwrap().raw);
                } else if ident.eq_ignore_ascii_case(cereal_engine::midiout::IDENT) {
                    changed |= midi_sender_panel(ui, &mut self.song.channels[ci].gen.as_mut().unwrap().raw, &self.handle.transport);
                } else if ident.eq_ignore_ascii_case(cereal_engine::synth::KEY_SENDER) {
                    changed |= key_sender_panel(ui, &mut self.song.channels[ci].gen.as_mut().unwrap().raw);
                } else if ident.eq_ignore_ascii_case(cereal_engine::dash::IDENT) {
                    changed |= board_panel(ui, &mut self.song.channels[ci].gen.as_mut().unwrap().raw);
                } else {
                    if ident.eq_ignore_ascii_case("Fruity Granulizer") {
                        // the grain player works on the channel's own sample
                        ui.horizontal(|ui| {
                            if ui.button("Sample...").hint("Pick the sample the grains are cut from").clicked() {
                                if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]).pick_file() {
                                    match self.lib.load_path(&p) {
                                        Ok(s) => {
                                            let c = &mut self.song.channels[ci];
                                            c.sample_path = p.to_string_lossy().to_string();
                                            c.set_sample(Some(s.clone()));
                                            if let Some(g) = c.gen.as_mut() {
                                                g.smp = Some(s);
                                            }
                                            changed = true;
                                        }
                                        Err(e) => self.status = e,
                                    }
                                }
                            }
                            let c = &self.song.channels[ci];
                            ui.label(RichText::new(c.sample_path.rsplit(['/', '\\']).next().unwrap_or("").to_string()).strong());
                        });
                    }
                    changed |= instrument_panel(ui, ci, self.song.channels[ci].gen.as_mut().unwrap());
                }
            }
            Tab::Sample => {
                let mut pick = false;
                let mut stems = false;
                {
                    let c = &mut self.song.channels[ci];
                    ui.horizontal(|ui| {
                        if ui.button("Open...").hint("Load a sample into this channel").clicked() {
                            pick = true;
                        }
                        let file = c.sample_path.rsplit(['/', '\\']).next().unwrap_or("").to_string();
                        let missing = c.sample.is_none() && !c.sample_path.is_empty();
                        ui.label(RichText::new(if file.is_empty() { "(no sample)".to_string() } else { file }).color(if missing { Color32::from_rgb(255, 120, 110) } else { TEXT })).hint(c.sample_path.clone());
                    });
                    if let Some(s) = &c.sample {
                        let r = draw_wave(ui, &s.frames, rgb(c.color).lerp_to_gamma(Color32::WHITE, 0.4));
                        ui.interact(r, ui.id().with(("wave menu", ci)), Sense::click()).context_menu(|ui| {
                            if ui.button("Extract stems from sample").clicked() {
                                stems = true;
                                ui.close_menu();
                            }
                        });
                        let len = s.frames.len() as f32 / s.rate.max(1) as f32;
                        ui.label(RichText::new(format!("{} Hz, {}, {}", s.rate, if s.mono { "mono" } else { "stereo" }, secs(len))).small().color(TEXT_DIM));
                    }
                    let f = &mut c.smp;
                    section(ui, "LOOP");
                    rebake |= ui.checkbox(&mut f.use_loop, "Use loop points").on_hover_text("Go round the loop stored in the sample while the note is held").changed();
                    rebake |= ui.checkbox(&mut f.ping_pong, "Ping pong loop").on_hover_text("Play the loop forth and back instead of jumping to its start").changed();
                    section(ui, "PRECOMPUTED EFFECTS");
                    ui.horizontal_wrapped(|ui| {
                        rebake |= ui.checkbox(&mut f.normalize, "Normalize").changed();
                        rebake |= ui.checkbox(&mut f.reverse, "Reverse").changed();
                        rebake |= ui.checkbox(&mut f.invert, "Reverse polarity").changed();
                        rebake |= ui.checkbox(&mut f.swap_stereo, "Swap stereo").changed();
                    });
                    ui.horizontal(|ui| {
                        rebake |= cap_knob(ui, "IN", &mut f.fade_in, 0.0, 1.0, 0.0, |v: f32| format!("Fade in {:.0} %", v * 100.0));
                        rebake |= cap_knob(ui, "OUT", &mut f.fade_out, 0.0, 1.0, 0.0, |v: f32| format!("Fade out {:.0} %", v * 100.0));
                    });
                    section(ui, "TIME STRETCHING");
                    ui.horizontal(|ui| {
                        changed |= cap_knob(ui, "PITCH", &mut f.stretch_cents, -1200.0, 1200.0, 0.0, |v: f32| format!("{:+.0} cents", v));
                        let mut m = f.stretch_mul.max(1e-6).log2();
                        if cap_knob(ui, "MUL", &mut m, -2.0, 2.0, 0.0, |v: f32| format!("x {:.3}", v.exp2())) {
                            f.stretch_mul = m.exp2();
                            changed = true;
                        }
                        ui.vertical(|ui| {
                            ui.set_width(70.0);
                            changed |= ui.add(egui::DragValue::new(&mut f.stretch_beats).range(0.0..=1024.0).speed(0.05).custom_formatter(|v, _| if v <= 0.0 { "none".into() } else { format!("{v:.2} beats") })).hint("Length the sample is fitted to; none keeps its own length").changed();
                            ui.label(RichText::new("TIME").small().color(TEXT_DIM));
                        });
                        ui.vertical(|ui| {
                            egui::ComboBox::from_id_salt("stretch-mode").width(110.0).selected_text(if f.keep_pitch { "Stretch" } else { "Resample" }).show_ui(ui, |ui| {
                                changed |= ui.selectable_value(&mut f.keep_pitch, false, "Resample").changed();
                                changed |= ui.selectable_value(&mut f.keep_pitch, true, "Stretch").changed();
                            });
                            ui.label(RichText::new("MODE").small().color(TEXT_DIM));
                        });
                    });
                }
                if stems {
                    self.ask_stems(ci);
                }
                if pick {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]).pick_file() {
                        self.actions.push(crate::Action::LoadSample(p, Some(ci)));
                    }
                }
            }
            Tab::Envelope => {
                ui.horizontal(|ui| {
                    for (k, name) in ["Volume", "Cutoff", "Pitch"].into_iter().enumerate() {
                        if ui.selectable_label(self.chanset.env == k, name).clicked() {
                            self.chanset.env = k;
                        }
                    }
                });
                let which = self.chanset.env;
                let c = &mut self.song.channels[ci];
                let mut on = if which == 0 { c.env_on } else if which == 1 { c.smp.cut_env.on } else { c.smp.pitch_env.on };
                let e: &mut Env = match which {
                    0 => &mut c.smp.vol_env,
                    1 => &mut c.smp.cut_env,
                    _ => &mut c.smp.pitch_env,
                };
                let shown = *e;
                draw_env(ui, &shown, on);
                ui.horizontal(|ui| {
                    if led(ui, on, 18.0).hint("Envelope on / off").clicked() {
                        on = !on;
                        changed = true;
                    }
                    changed |= time_knob(ui, "DEL", &mut e.delay);
                    changed |= time_knob(ui, "ATT", &mut e.attack);
                    changed |= time_knob(ui, "HOLD", &mut e.hold);
                    changed |= time_knob(ui, "DEC", &mut e.decay);
                    changed |= cap_knob(ui, "SUS", &mut e.sustain, 0.0, 1.0, 0.5, |v: f32| format!("{:.0} %", v * 100.0));
                    changed |= time_knob(ui, "REL", &mut e.release);
                    match which {
                        1 => changed |= cap_knob(ui, "AMT", &mut e.amount, -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0)),
                        2 => changed |= cap_knob(ui, "AMT", &mut e.amount, -4800.0, 4800.0, 0.0, |v: f32| format!("{:+.0} cents", v)),
                        _ => {}
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    for (k, name) in ["ATT T", "DEC T", "REL T"].into_iter().enumerate() {
                        changed |= cap_knob(ui, name, &mut e.tension[k], -1.0, 1.0, 0.0, |v: f32| format!("Tension {:+.0} %", v * 100.0));
                    }
                });
                e.on = on;
                if which == 0 {
                    c.env_on = on;
                }
                let f = &mut c.smp;
                section(ui, "FILTER");
                ui.horizontal(|ui| {
                    changed |= cap_knob(ui, "MOD X", &mut f.cutoff, 0.0, 1.0, 1.0, |v: f32| format!("Cutoff {:.0} %", v * 100.0));
                    changed |= cap_knob(ui, "MOD Y", &mut f.resonance, 0.0, 1.0, 0.0, |v: f32| format!("Resonance {:.0} %", v * 100.0));
                    const TYPES: [&str; 8] = ["Fast LP", "LP", "BP", "HP", "BS", "LPx2", "SVF LP", "SVF LPx2"];
                    ui.vertical(|ui| {
                        egui::ComboBox::from_id_salt("filter-type").width(100.0).selected_text(TYPES[f.filter.min(7) as usize]).show_ui(ui, |ui| {
                            for (k, name) in TYPES.into_iter().enumerate() {
                                changed |= ui.selectable_value(&mut f.filter, k as u8, name).changed();
                            }
                        });
                        ui.label(RichText::new("TYPE").small().color(TEXT_DIM));
                    });
                });
            }
            Tab::Misc => {
                let c = &mut self.song.channels[ci];
                let mut key = c.root_key;
                let f: &mut Sampler = &mut c.smp;
                section(ui, "LEVELS ADJUSTMENT");
                ui.horizontal(|ui| {
                    changed |= cap_knob(ui, "VOL", &mut f.gain, 0.0, 4.0, 1.0, |v: f32| format!("x {:.2}", v));
                    ui.vertical(|ui| {
                        ui.set_width(60.0);
                        changed |= ui.add(egui::DragValue::new(&mut key).range(0..=131).speed(0.1).custom_formatter(|v, _| key_name(v as u8))).hint("Root note").changed();
                        ui.label(RichText::new("ROOT").small().color(TEXT_DIM));
                    });
                    changed |= cap_knob(ui, "SWING", &mut f.swing, 0.0, 1.0, 1.0, |v: f32| format!("Swing mix {:.0} %", v * 100.0));
                    ui.vertical(|ui| {
                        changed |= ui.checkbox(&mut f.add_to_key, "Add to key").hint("The root note's shift is added to the key of each note instead of to its pitch").changed();
                        ui.horizontal(|ui| {
                            let (mut lo, mut hi) = (f.key_lo.min(131), f.key_hi.min(131));
                            ui.label(RichText::new("KEY RANGE").small().color(TEXT_DIM));
                            let a = ui.add(egui::DragValue::new(&mut lo).range(0..=131).speed(0.15).custom_formatter(|v, _| key_name(v as u8))).hint("The lowest key this channel answers to. Right-click for every key");
                            let b = ui.add(egui::DragValue::new(&mut hi).range(0..=131).speed(0.15).custom_formatter(|v, _| key_name(v as u8))).hint("The highest key this channel answers to. Right-click for every key");
                            if a.changed() || b.changed() {
                                f.key_lo = lo.min(hi);
                                f.key_hi = if hi >= 131 { 256 } else { hi.max(lo) };
                                changed = true;
                            }
                            if a.secondary_clicked() || b.secondary_clicked() {
                                f.key_lo = 0;
                                f.key_hi = 256;
                                changed = true;
                            }
                        });
                    });
                });
                section(ui, "POLYPHONY");
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut f.mono, "Mono").changed();
                    changed |= ui.checkbox(&mut f.porta, "Porta").changed();
                    changed |= cap_knob(ui, "SLIDE", &mut f.slide, 0.0, 2.0, 0.1, |v: f32| secs(v));
                });
                section(ui, "GROUP");
                ui.horizontal(|ui| {
                    ui.label("Cut");
                    changed |= ui.add(egui::DragValue::new(&mut f.cut).range(0..=99).speed(0.1).custom_formatter(|v, _| if v < 0.5 { "--".into() } else { format!("{v:.0}") })).changed();
                    ui.label("Cut by");
                    changed |= ui.add(egui::DragValue::new(&mut f.cut_by).range(0..=99).speed(0.1).custom_formatter(|v, _| if v < 0.5 { "--".into() } else { format!("{v:.0}") })).changed();
                    if ui.button("Cut itself").clicked() {
                        let g = ci as u16 + 1;
                        f.cut = g;
                        f.cut_by = g;
                        changed = true;
                    }
                });
                section(ui, "TIME");
                ui.horizontal(|ui| {
                    changed |= cap_knob(ui, "GATE", &mut f.gate_steps, 0.0, 2.0, 2.0, |v: f32| {
                        if v >= cereal_engine::song::GATE_OFF { "Gate off".into() } else { format!("Gate {}:{:02}", v as u32, (v.fract() * 24.0) as u32) }
                    });
                    changed |= cap_knob(ui, "SHIFT", &mut f.shift, 0.0, 1.0, 0.0, |v: f32| format!("Shift 0:{:02}", (v * 24.0) as u32));
                    changed |= ui.checkbox(&mut f.full_porta, "Full porta").hint("Portamento notes ignore the gate and play their whole length").changed();
                });
                section(ui, "ARPEGGIATOR");
                ui.horizontal(|ui| {
                    for (i, (face, tip)) in [("Off", "Off"), ("Up", "Up"), ("Down", "Down"), ("Up-Dn", "Up and down"), ("Up-Dn 2", "Up and down, lowest and highest twice"), ("Rnd", "Random")].iter().enumerate() {
                        if ui.selectable_label(f.arp_dir == i as u8, *face).hint(*tip).clicked() {
                            f.arp_dir = i as u8;
                            changed = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    changed |= cap_knob(ui, "TIME", &mut f.arp_steps, 0.01, 2.0, 1.0, |v: f32| format!("Time {}:{:02}", v as u32, (v.fract() * 24.0) as u32));
                    changed |= cap_knob(ui, "GATE", &mut f.arp_gate, 0.0, 1.0, 1.0, |v: f32| format!("Gate {:.0} %", v * 100.0));
                    ui.vertical(|ui| {
                        ui.set_width(50.0);
                        changed |= ui.add(egui::DragValue::new(&mut f.arp_range).range(1..=10).speed(0.1)).hint("Range in octaves").changed();
                        ui.label(RichText::new("RANGE").small().color(TEXT_DIM));
                    });
                    ui.vertical(|ui| {
                        ui.set_width(50.0);
                        changed |= ui.add(egui::DragValue::new(&mut f.arp_repeat).range(1..=16).speed(0.1)).hint("Times each octave plays before the next").changed();
                        ui.label(RichText::new("REPEAT").small().color(TEXT_DIM));
                    });
                    changed |= ui.checkbox(&mut f.arp_slide, "Slide").changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Chord");
                    let chords = &cereal_engine::song::ARP_CHORDS;
                    let face = |c: i16| match c {
                        -2 => "Auto (sustain)".to_string(),
                        -1 => "Auto".to_string(),
                        0 => "None".to_string(),
                        n => chords[(n as usize - 1).min(chords.len() - 1)].0.to_string(),
                    };
                    egui::ComboBox::from_id_salt(("arp chord", ci)).selected_text(face(f.arp_chord)).show_ui(ui, |ui| {
                        for c in [0i16, -1, -2].into_iter().chain(1..=chords.len() as i16) {
                            changed |= ui.selectable_value(&mut f.arp_chord, c, face(c)).changed();
                        }
                    });
                });
                section(ui, "TRACKING");
                for (name, t, tip) in [("Vol", &mut f.track_vel, "Hit strength where nothing moves"), ("Key", &mut f.track_key, "Key where nothing moves")] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).color(TEXT_DIM));
                        changed |= cap_knob(ui, "PAN", &mut t[1], -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0));
                        changed |= cap_knob(ui, "MOD X", &mut t[2], -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0));
                        changed |= cap_knob(ui, "MOD Y", &mut t[3], -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0));
                        ui.vertical(|ui| {
                            ui.set_width(50.0);
                            let is_key = name == "Key";
                            changed |= ui.add(egui::DragValue::new(&mut t[0]).range(0.0..=127.0).speed(0.2).custom_formatter(move |v, _| if is_key { key_name(v as u8) } else { format!("{v:.0}") })).hint(tip).changed();
                            ui.label(RichText::new("MID").small().color(TEXT_DIM));
                        });
                    });
                }
                section(ui, "ECHO DELAY / FAT MODE");
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut f.echo_ping, "Ping pong").hint("The echo pan turns round when it reaches a side").changed();
                    changed |= ui.checkbox(&mut f.echo_fat, "Fat mode").hint("Echoes swap from side to side").changed();
                });
                ui.horizontal(|ui| {
                    changed |= cap_knob(ui, "FEED", &mut f.echo_feedback, 0.0, 1.0, 0.0, |v: f32| format!("Feedback {:.0} %", v * 100.0));
                    changed |= cap_knob(ui, "PAN", &mut f.echo_pan, -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0));
                    changed |= cap_knob(ui, "PITCH", &mut f.echo_cents, -1200.0, 1200.0, 0.0, |v: f32| format!("{:+.0} cents", v));
                    changed |= cap_knob(ui, "TIME", &mut f.echo_steps, 0.0, 8.0, 3.0, |v: f32| format!("{:.2} steps", v));
                    ui.vertical(|ui| {
                        ui.set_width(50.0);
                        changed |= ui.add(egui::DragValue::new(&mut f.echoes).range(1..=10).speed(0.1)).changed();
                        ui.label(RichText::new("ECHOES").small().color(TEXT_DIM));
                    });
                });
                c.root_key = key;
            }
        }
        changed |= self.key_strip(ui, ci);
        if rebake {
            self.song.channels[ci].bake();
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }
}

/// Where key `k` sits on a keyboard whose white keys are `ww` wide: left edge, width, and whether it is black.
fn key_place(k: u8, ww: f32) -> (f32, f32, bool) {
    const WHITE: [u8; 12] = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6];
    let (o, pc) = ((k / 12) as f32, (k % 12) as usize);
    let black = matches!(pc, 1 | 3 | 6 | 8 | 10);
    let x = (o * 7.0 + WHITE[pc] as f32) * ww;
    if black {
        (x + ww * 0.7, ww * 0.6, true)
    } else {
        (x, ww, false)
    }
}

/// The key under a point on that keyboard, `fy` being how far down the keys it is (0 top, 1 bottom).
fn key_under(x: f32, fy: f32, ww: f32) -> u8 {
    if fy < 0.6 {
        if let Some(k) = (0..132u8).find(|&k| {
            let (kx, kw, black) = key_place(k, ww);
            black && x >= kx && x < kx + kw
        }) {
            return k;
        }
    }
    (0..132u8).rev().find(|&k| {
        let (kx, _, black) = key_place(k, ww);
        !black && x >= kx
    }).unwrap_or(0)
}

impl App {
    /// The preview keyboard under the channel settings: a click plays the key (harder lower down), a
    /// right-click makes it the root note, and the bar above it holds the range of keys the channel answers to.
    fn key_strip(&mut self, ui: &mut Ui, ci: usize) -> bool {
        let mut changed = false;
        ui.add_space(4.0);
        let w = ui.available_width().clamp(200.0, 900.0);
        let ww = w / 77.0;
        let (bar_rect, bar) = ui.allocate_exact_size(Vec2::new(w, 9.0), Sense::click_and_drag());
        let (keys_rect, keys) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click_and_drag());
        let c = &mut self.song.channels[ci];
        // the range bar
        let id = ui.id().with(("keyrange", ci));
        if let Some(pos) = bar.interact_pointer_pos().filter(|_| bar.dragged_by(egui::PointerButton::Primary) || bar.drag_started_by(egui::PointerButton::Primary)) {
            let k = key_under(pos.x - bar_rect.left(), 1.0, ww);
            let from: u8 = if bar.drag_started() { k } else { ui.data(|d| d.get_temp(id)).unwrap_or(k) };
            ui.data_mut(|d| d.insert_temp(id, from));
            let (a, b) = (from.min(k) as u32, from.max(k) as u32);
            let b = if b >= 131 { 256 } else { b };
            if (a, b) != (c.smp.key_lo, c.smp.key_hi) {
                c.smp.key_lo = a;
                c.smp.key_hi = b;
                changed = true;
            }
        }
        if bar.secondary_clicked() && (c.smp.key_lo, c.smp.key_hi) != (0, 256) {
            c.smp.key_lo = 0;
            c.smp.key_hi = 256;
            changed = true;
        }
        bar.on_hover_text("Key range: drag across the keys this channel answers to. Right-click for all of them.");
        let (lo, hi) = (c.smp.key_lo.min(131) as u8, c.smp.key_hi.min(131) as u8);
        let root = c.root_key.min(131);
        let held_id = ui.id().with(("keyheld", ci));
        let held: Option<u8> = ui.data(|d| d.get_temp(held_id)).flatten();
        let p = ui.painter();
        p.rect_filled(bar_rect, 1.0, PANEL_DARK);
        let edge = |k: u8, right: bool| {
            let (x, kw, _) = key_place(k, ww);
            bar_rect.left() + if right { x + kw } else { x }
        };
        let span = egui::Rect::from_min_max(Pos2::new(edge(lo, false), bar_rect.top() + 2.0), Pos2::new(edge(hi, true).min(bar_rect.right()), bar_rect.bottom() - 2.0));
        p.rect_filled(span, 1.0, if (lo, hi) == (0, 131) { TEXT_DIM } else { ACCENT });
        // the keys: white ones first, the black ones over them
        for pass in [false, true] {
            for k in 0..132u8 {
                let (x, kw, black) = key_place(k, ww);
                if black != pass {
                    continue;
                }
                let h = if black { 18.0 } else { 30.0 };
                let r = egui::Rect::from_min_size(Pos2::new(keys_rect.left() + x, keys_rect.top()), Vec2::new(kw, h));
                let out = k < lo || k > hi;
                let fill = match (held == Some(k), black, out) {
                    (true, ..) => ACCENT,
                    (_, true, _) => Color32::from_rgb(28, 32, 35),
                    (_, false, true) => Color32::from_rgb(150, 156, 160),
                    (_, false, false) => Color32::from_rgb(232, 236, 238),
                };
                p.rect_filled(r.shrink2(Vec2::new(0.4, 0.0)), 1.0, fill);
                if k == root {
                    p.rect_filled(egui::Rect::from_min_size(Pos2::new(r.left() + 0.5, r.bottom() - 6.0), Vec2::new((kw - 1.0).max(2.0), 5.0)), 1.0, ACCENT);
                } else if !black && k % 12 == 0 && ww >= 4.5 {
                    p.text(Pos2::new(r.left() + 1.0, r.bottom() - 1.0), egui::Align2::LEFT_BOTTOM, format!("{}", k / 12), egui::FontId::proportional(7.0), Color32::from_rgb(90, 96, 100));
                }
            }
        }
        // the key under the pointer, and how hard: full strength at the bottom of the keys
        let now = ui.input(|i| i.pointer.latest_pos()).filter(|pos| keys_rect.contains(*pos)).map(|pos| {
            let fy = (pos.y - keys_rect.top()) / keys_rect.height();
            (key_under(pos.x - keys_rect.left(), fy, ww).min(131), (0.25 + 0.75 * fy).clamp(0.05, 1.0))
        });
        let down = keys.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
        let want = if down { now } else { None };
        if want.map(|w| w.0) != held {
            if let Some(k) = held {
                self.handle.send(cereal_engine::player::Cmd::NoteOff { chan: ci as u16, key: k });
            }
            if let Some((k, vel)) = want {
                self.handle.send(cereal_engine::player::Cmd::NoteOn { chan: ci as u16, key: k, vel });
            }
            ui.data_mut(|d| d.insert_temp(held_id, want.map(|w| w.0)));
        }
        if keys.secondary_clicked() {
            if let Some((k, _)) = now {
                if self.song.channels[ci].root_key != k {
                    self.song.channels[ci].root_key = k;
                    changed = true;
                }
            }
        }
        keys.on_hover_text("Click to hear a key, harder the lower you click. Right-click to make it the root note.");
        changed
    }
}

/// The envelope sender: per output its three knobs, how its lines combine, its key range and a picture of
/// what a held and let-go note sends.
fn env_sender_panel(ui: &mut Ui, raw: &mut Vec<u8>) -> bool {
    let Some(set) = cereal_engine::fm::Sender::parse(raw) else {
        ui.label(RichText::new("The stored settings could not be read. The block is kept untouched.").color(TEXT_DIM));
        return false;
    };
    let mut changed = false;
    ui.label(RichText::new("Makes no sound. Each note runs the envelopes below and sends them to the controls linked to this channel's outputs. Link a control with right-click, Link to controller.").color(TEXT_DIM));
    let sel_id = ui.id().with("envsel");
    let mut sel: usize = ui.data(|d| d.get_temp(sel_id)).unwrap_or(0).min(set.arts.len().saturating_sub(1));
    ui.horizontal(|ui| {
        ui.label("Output");
        for k in 0..set.arts.len() {
            ui.selectable_value(&mut sel, k, format!("{}", k + 1));
        }
    });
    ui.data_mut(|d| d.insert_temp(sel_id, sel));
    let Some(a) = set.arts.get(sel) else { return false };
    let put = |raw: &mut Vec<u8>, at: usize, v: i32| raw[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let h = 4 + sel * 44;
    ui.horizontal(|ui| {
        let mut bip = a.art.bipolar;
        egui::ComboBox::from_id_salt("envmode").selected_text(if bip { "Added (swings around the base)" } else { "Multiplied (rises from the base)" }).show_ui(ui, |ui| {
            ui.selectable_value(&mut bip, true, "Added (swings around the base)");
            ui.selectable_value(&mut bip, false, "Multiplied (rises from the base)");
        });
        if bip != a.art.bipolar {
            put(raw, a.mode_at, bip as i32);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let mut base = a.base * 200.0 - 100.0;
        if ui.add(egui::Slider::new(&mut base, -100.0..=100.0).text("Base").suffix(" %")).changed() {
            put(raw, h, ((base + 100.0) / 200.0 * 256.0).round() as i32);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let mut env = a.env * 100.0;
        if ui.add(egui::Slider::new(&mut env, -100.0..=100.0).text("Envelope").suffix(" %")).changed() {
            put(raw, h + 4, (128.0 + env / 100.0 * 128.0).round() as i32);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let mut lfo = a.lfo * 100.0;
        if ui.add(egui::Slider::new(&mut lfo, -100.0..=100.0).text("Slow oscillator").suffix(" %")).changed() {
            put(raw, h + 8, (128.0 + lfo / 100.0 * 128.0).round() as i32);
            changed = true;
        }
        ui.label(RichText::new(format!("{:.2} Hz", a.art.lfo_hz)).color(TEXT_DIM));
    });
    ui.horizontal(|ui| {
        const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let name = |v: f64, _: std::ops::RangeInclusive<usize>| if v >= 132.0 { "top".to_string() } else { format!("{}{}", NAMES[v as usize % 12], v as usize / 12) };
        let (mut lo, mut hi) = (a.lo.clamp(0, 256), a.hi.clamp(0, 256));
        ui.label("Keys");
        let c1 = ui.add(egui::DragValue::new(&mut lo).range(0..=131).custom_formatter(name)).changed();
        ui.label("to");
        let c2 = ui.add(egui::DragValue::new(&mut hi).range(0..=256).custom_formatter(name)).changed();
        if c1 || c2 {
            put(raw, a.range_at, lo.min(hi));
            put(raw, a.range_at + 4, hi.max(lo));
            changed = true;
        }
    });
    // what a note held for a while and then let go sends on this output
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 110.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, PANEL_DARK);
    let key = a.lo.clamp(0, 127) as u8;
    let held = (a.art.env.length() * 1.2).clamp(0.5, 20.0);
    let total = held + set.tail(sel, held).clamp(0.2, 10.0);
    let n = rect.width().max(2.0) as usize;
    let pts: Vec<Pos2> = (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * total;
            let y = set.value(sel, key, 1.0, 0.5, t, (t >= held).then_some(held)).unwrap_or(0.0);
            Pos2::new(rect.left() + i as f32, rect.bottom() - 4.0 - y * (rect.height() - 8.0))
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.5_f32, ACCENT)));
    let x = rect.left() + held / total * rect.width();
    p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
    ui.label(RichText::new(format!("The picture shows a note held {held:.1} s and let go at the line.")).small().color(TEXT_DIM));
    if changed {
        // the knobs above already rewrote the block this frame; the lines are read afresh on the next one
        return true;
    }
    ui.separator();
    let line_id = ui.id().with("envline");
    let n_maps = a.art.maps.iter().filter(|m| m.span.1 > 0).count();
    let mut which: usize = ui.data(|d| d.get_temp(line_id)).unwrap_or(0).min(1 + n_maps);
    ui.horizontal(|ui| {
        ui.label("Line");
        ui.selectable_value(&mut which, 0, "Envelope");
        ui.selectable_value(&mut which, 1, "Oscillator shape");
        for (k, name) in ["Key", "Velocity", "X", "Y", "Random"].iter().enumerate().take(n_maps) {
            ui.selectable_value(&mut which, 2 + k, format!("{name} map"));
        }
    });
    ui.data_mut(|d| d.insert_temp(line_id, which));
    let (env_end, lfo_end) = (a.art.env.span.1, a.art.lfo.span.1);
    if which == 0 && env_end + 16 <= raw.len() {
        ui.horizontal(|ui| {
            for (o, name) in [(0usize, "Attack length"), (4, "Decay length"), (12, "Release length")] {
                let mut v = i32::from_le_bytes(raw[env_end + o..env_end + o + 4].try_into().unwrap()).clamp(0, 256);
                if ui.add(egui::DragValue::new(&mut v).range(0..=256).prefix(format!("{name} ")).custom_formatter(|v, _| format!("{:.0} %", (v / 128.0) * (v / 128.0) * 100.0))).changed() {
                    put(raw, env_end + o, v);
                    changed = true;
                }
            }
        });
    }
    if which == 1 && lfo_end + 4 <= raw.len() {
        ui.horizontal(|ui| {
            let mut v = i32::from_le_bytes(raw[lfo_end..lfo_end + 4].try_into().unwrap()).clamp(0, 65536);
            if ui.add(egui::Slider::new(&mut v, 0..=65536).text("Speed").custom_formatter(|v, _| format!("{:.2} Hz", 0.05 * (v / 65536.0 * 11.0).exp2()))).changed() {
                put(raw, lfo_end, v);
                changed = true;
            }
        });
    }
    if changed {
        return true;
    }
    let (mut c, kind) = match which {
        0 => (a.art.env.clone(), crate::curve::Line::Envelope),
        1 => (a.art.lfo.clone(), crate::curve::Line::Envelope),
        k => (a.art.maps[k - 2].clone(), crate::curve::Line::Mapping),
    };
    ui.push_id(("envcurve", sel, which), |ui| {
        if crate::curve::curve_editor(ui, &mut c, kind, 170.0) && c.store(raw) {
            changed = true;
        }
    });
    ui.label(RichText::new("Drag a point to move it, drag a ring to bend the stretch before it (double-click straightens it). Right-click empty space for a new point, right-click a point for its menu.").small().color(TEXT_DIM));
    changed
}

/// The key sender: what each key sends to the controls linked to it, drawn as a bar per key.
/// The MIDI sender's page: where its notes go, and eight pages of nine knobs that send controller values.
/// The pad machine's page. Returns the block when something was changed, and a key and velocity to sound.
fn pad_kit_panel(ui: &mut Ui, map: &cereal_engine::zones::Map) -> (Option<Vec<u8>>, Option<(u8, f32)>) {
    use cereal_engine::zones::{Kit, Layer, Pad};
    let Some(mut kit) = map.kit.as_deref().and_then(Kit::parse) else { return (None, None) };
    let was = kit.clone();
    let red = Color32::from_rgb(255, 120, 110);
    let orange = Color32::from_rgb(255, 160, 60);
    let sel_id = ui.id().with("padsel");
    let lay_id = ui.id().with("padlayer");
    let mut sel: usize = ui.data(|d| d.get_temp(sel_id)).unwrap_or(0).min(kit.pads.len().saturating_sub(1));
    let mut lay: usize = ui.data(|d| d.get_temp(lay_id)).unwrap_or(0);
    let mut hit = None;
    let lost = |path: &str| !path.is_empty() && !map.zones.iter().any(|z| z.path == path && z.sample.is_some());
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            section(ui, "PADS");
            let banks = kit.pads.len().div_ceil(16).max(1);
            let mut bank = sel / 16;
            ui.horizontal(|ui| {
                for b in 0..banks {
                    if ui.selectable_label(bank == b, format!("Bank {}", (b'A' + b as u8) as char)).clicked() {
                        bank = b;
                        sel = b * 16 + sel % 16;
                    }
                }
            });
            sel = sel.min(kit.pads.len().saturating_sub(1));
            egui::Grid::new("kitpads").spacing([4.0, 4.0]).show(ui, |ui| {
                // pad 1 sits at the lower left
                for row in (0..4).rev() {
                    for col in 0..4 {
                        let i = bank * 16 + row * 4 + col;
                        let Some(p) = kit.pads.get(i) else {
                            ui.label("");
                            continue;
                        };
                        let key = p.val(Pad::KEY).clamp(0, 127) as u8;
                        let empty = p.layers.iter().all(|l| l.path().is_empty());
                        let missing = !empty && p.layers.iter().all(|l| l.path().is_empty() || lost(&l.path()));
                        let name = if p.name().is_empty() { format!("Pad {}", i + 1) } else { p.name() };
                        let color = if missing { red } else if empty { TEXT_DIM } else { TEXT };
                        let mut b = egui::Button::new(RichText::new(name).small().color(color));
                        if i == sel {
                            b = b.stroke(egui::Stroke::new(1.5_f32, orange));
                        }
                        let r = ui.add_sized([86.0, 46.0], b).hint(format!("{}\nLeft-click selects and plays, right-click only plays", key_name(key)));
                        if r.clicked() {
                            sel = i;
                            lay = 0;
                        }
                        if r.clicked() || r.secondary_clicked() {
                            hit = Some((key, 1.0));
                        }
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(10.0);
        ui.vertical(|ui| {
            let Some(p) = kit.pads.get_mut(sel) else { return };
            section(ui, &format!("PAD {}", sel + 1));
            let int = |ui: &mut Ui, p: &mut Pad, at: usize, lo: i32, hi: i32, label: &str, tip: &str| {
                let mut v = p.val(at);
                ui.label(label);
                if ui.add(egui::DragValue::new(&mut v).range(lo..=hi).speed(0.3)).hint(tip).changed() {
                    p.put(at, v);
                }
            };
            ui.horizontal(|ui| {
                ui.label("Name");
                let mut name = p.name();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(150.0)).changed() {
                    p.put_name(&name);
                }
                if ui.button("Clear pad").hint("Back to an empty pad on the same key").clicked() {
                    p.clear();
                }
            });
            ui.horizontal(|ui| {
                int(ui, p, Pad::LEVEL, 0, 128, "Volume", "The pad's level");
                int(ui, p, Pad::PAN, -64, 64, "Pan", "Left to right");
                int(ui, p, Pad::PITCH, -1200, 1200, "Pitch", "Tunes every layer of the pad");
                let mut on = p.scale_volume();
                if ui.checkbox(&mut on, "Scale volume").hint("How hard a note is hit sets how loud the pad plays. Switch off to leave loudness to the layers alone").changed() {
                    p.put_scale_volume(on);
                }
            });
            ui.horizontal(|ui| {
                let mut key = p.val(Pad::KEY);
                ui.label("Key");
                if ui.add(egui::DragValue::new(&mut key).range(0..=127).speed(0.15).custom_formatter(|x, _| key_name(x as u8))).hint("The note that plays this pad").changed() {
                    p.put(Pad::KEY, key);
                }
                for (at, label, tip) in [(Pad::CUT, "Cut", "The cut group this pad is in"), (Pad::CUT_BY, "By", "The group whose pads stop this one")] {
                    let mut v = p.val(at);
                    ui.label(label);
                    if num_or_none(ui, &mut v, 99, 0).hint(tip).changed() {
                        p.put(at, v);
                    }
                }
                let mut out = p.val(Pad::OUT);
                ui.label("Output");
                if ui.add(egui::DragValue::new(&mut out).range(0..=125).speed(0.15).custom_formatter(|x, _| if x < 1.0 { "--".into() } else { format!("{}", x as i32) })).hint("Mixer tracks above the channel's own track that this pad plays through").changed() {
                    p.put(Pad::OUT, out);
                }
            });
            section(ui, "LAYERS");
            lay = lay.min(p.layers.len().saturating_sub(1));
            ui.horizontal(|ui| {
                if ui.button("Create").hint("Add an empty layer at the bottom").clicked() {
                    p.layers.push(Layer::fresh());
                    lay = p.layers.len() - 1;
                }
                if ui.add_enabled(p.layers.len() > 1, egui::Button::new("Delete")).hint("Remove the selected layer").clicked() {
                    p.layers.remove(lay);
                    lay = lay.min(p.layers.len() - 1);
                }
                if ui.add_enabled(lay > 0, egui::Button::new("Up")).clicked() {
                    p.layers.swap(lay, lay - 1);
                    lay -= 1;
                }
                if ui.add_enabled(lay + 1 < p.layers.len(), egui::Button::new("Down")).clicked() {
                    p.layers.swap(lay, lay + 1);
                    lay += 1;
                }
                if ui.button("Spread even").hint("Share the velocity range out among the layers with no overlap").clicked() {
                    p.spread_even();
                }
                let mut on = p.random_layers();
                if ui.checkbox(&mut on, "Random layers").hint("Layers whose velocity ranges overlap take turns instead of sounding together").changed() {
                    p.put_random_layers(on);
                }
            });
            let key = p.val(Pad::KEY).clamp(0, 127) as u8;
            egui::Grid::new("kitlayers").spacing([6.0, 3.0]).show(ui, |ui| {
                for h in ["", "Sound", "Volume", "Pan", "Tune", "Velocity", "", ""] {
                    ui.label(RichText::new(h).small().color(TEXT_DIM));
                }
                ui.end_row();
                for (i, l) in p.layers.iter_mut().enumerate() {
                    if ui.selectable_label(i == lay, format!("{}", i + 1)).clicked() {
                        lay = i;
                    }
                    let path = l.path();
                    let file = path.rsplit(['/', '\\']).next().unwrap_or("").to_string();
                    let text = RichText::new(if file.is_empty() { "(load a sound)".to_string() } else { file }).color(if lost(&path) { red } else { TEXT });
                    let r = ui.add_sized([200.0, 18.0], egui::Button::new(text)).hint(if lost(&path) { format!("Not found: {path}") } else { path.clone() });
                    if r.clicked() {
                        if let Some(f) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]).pick_file() {
                            l.put_path(&f.to_string_lossy());
                            lay = i;
                        }
                    }
                    for (at, lo, hi) in [(Layer::LEVEL, 0, 127), (Layer::PAN, -64, 64), (Layer::TUNE, -100, 100), (Layer::VEL_LO, 0, 127)] {
                        let mut v = l.val(at);
                        if ui.add(egui::DragValue::new(&mut v).range(lo..=hi).speed(0.3)).changed() {
                            l.put(at, v);
                            if at == Layer::VEL_LO && v > l.val(Layer::VEL_HI) {
                                l.put(Layer::VEL_HI, v);
                            }
                        }
                    }
                    let mut v = l.val(Layer::VEL_HI);
                    if ui.add(egui::DragValue::new(&mut v).range(0..=127).speed(0.3).prefix("to ")).changed() {
                        l.put(Layer::VEL_HI, v);
                        if v < l.val(Layer::VEL_LO) {
                            l.put(Layer::VEL_LO, v);
                        }
                    }
                    if ui.small_button("Play").hint("Sound the pad at the middle of this layer's velocity range").clicked() {
                        hit = Some((key, (l.val(Layer::VEL_LO) + l.val(Layer::VEL_HI)) as f32 / 200.0));
                    }
                    ui.end_row();
                }
            });
        });
    });
    ui.data_mut(|d| {
        d.insert_temp(sel_id, sel);
        d.insert_temp(lay_id, lay);
    });
    ((kit != was).then(|| kit.write()), hit)
}

fn midi_sender_panel(ui: &mut Ui, raw: &mut Vec<u8>, transport: &cereal_engine::player::Transport) -> bool {
    use cereal_engine::midiout::{Msg, Set, KINDS};
    let mut set = Set::parse(raw).unwrap_or_else(Set::fresh);
    let was = set.clone();
    let orange = Color32::from_rgb(255, 160, 60);
    let page_id = ui.id().with("midipage");
    let sel_id = ui.id().with("midisel");
    let mut page: usize = ui.data(|d| d.get_temp(page_id)).unwrap_or(0);
    let mut sel: Option<usize> = ui.data(|d| d.get_temp(sel_id)).unwrap_or(None);
    ui.horizontal(|ui| {
        let mut chan = set.chan as i32 + 1;
        ui.label("Channel");
        ui.add(egui::DragValue::new(&mut chan).range(1..=16).speed(0.1));
        set.chan = (chan - 1).clamp(0, 15) as u8;
        ui.label("Patch").hint("The program the device is switched to; none leaves it alone");
        num_or_none(ui, &mut set.patch, 127, 1);
        let chan0 = set.chan;
        ui.menu_button(RichText::new(cereal_engine::midiout::gm_name(chan0, set.patch).unwrap_or("Names")).small(), |ui| {
            if chan0 == 9 {
                for (n, name) in cereal_engine::midiout::GM_KITS {
                    if ui.selectable_label(set.patch == n, name).clicked() {
                        set.patch = n;
                        ui.close_menu();
                    }
                }
            } else {
                for (f, fam) in cereal_engine::midiout::GM_FAMILIES.iter().enumerate() {
                    ui.menu_button(*fam, |ui| {
                        for n in f * 8..f * 8 + 8 {
                            if ui.selectable_label(set.patch == n as i32, format!("{}  {}", n + 1, cereal_engine::midiout::GM[n])).clicked() {
                                set.patch = n as i32;
                                ui.close_menu();
                            }
                        }
                    });
                }
            }
        })
        .response
        .hint("The General MIDI names; channel 10 lists the drum kits");
        ui.label("Bank");
        num_or_none(ui, &mut set.bank[0], 127, 0).hint("Coarse");
        num_or_none(ui, &mut set.bank[1], 127, 0).hint("Fine");
        let mut port = set.port as i32;
        ui.label("Port").hint(if cfg!(windows) { "Port N plays through the Nth MIDI output device of the system" } else { "Each port in use shows up as a port of the client \"Open Cereal Workshop Out\"; connect it to a synth or an interface" });
        ui.add(egui::DragValue::new(&mut port).range(0..=255).speed(0.1));
        set.port = port.clamp(0, 255) as u8;
        let lit = crate::midi::SENT.load(std::sync::atomic::Ordering::Relaxed);
        let lit_id = ui.id().with("midilit");
        let (last, at): (usize, f64) = ui.data(|d| d.get_temp(lit_id)).unwrap_or((lit, -1.0));
        let now = ui.input(|i| i.time);
        let at = if last != lit { now } else { at };
        ui.data_mut(|d| d.insert_temp(lit_id, (lit, at)));
        let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, if now - at < 0.15 { orange } else { Color32::from_rgb(60, 60, 66) });
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut set.by_color, "Map note color to MIDI channel").hint("Each note goes out on the channel of its piano roll colour, counted from the channel above");
        ui.checkbox(&mut set.send_mix, "Send pan, volume and pitch").hint("The channel's volume, pan and pitch wheel go out as controllers 7 and 10 and pitch bend");
        if ui.button("Reset").hint("Sends the reset-all-controllers message (CC 121)").clicked() {
            transport.send_midi(Msg::new(set.port, &[0xb0 | set.chan, 121, 0]));
        }
        if ui.button("Deactivate all").hint("Switches every knob off; their set-up is kept").clicked() {
            for c in set.ctls.iter_mut() {
                c.on = false;
            }
        }
    });
    section(ui, "CONTROLLERS");
    ui.horizontal(|ui| {
        for p in 0..8 {
            let used = set.ctls.iter().any(|c| c.slot as usize / 9 == p && c.on);
            if ui.selectable_label(page == p, RichText::new(format!("{}", p + 1)).color(if used { orange } else { TEXT })).hint(set.pages[p].clone()).clicked() {
                page = p;
                sel = None;
            }
        }
        ui.add(egui::TextEdit::singleline(&mut set.pages[page]).desired_width(180.0)).hint("Page name");
    });
    ui.horizontal(|ui| {
        for col in 0..9 {
            let slot = page * 9 + col;
            ui.vertical(|ui| {
                ui.set_width(58.0);
                let c = set.ctl(slot).cloned();
                let (lo, hi) = c.as_ref().map(|c| (c.lo.min(c.hi), c.hi.max(c.lo))).unwrap_or((0, 127));
                let mut v = set.values[slot] as f32;
                let r = knob(ui, &mut v, lo as f32, hi.max(lo + 1) as f32, lo as f32, 34.0).hint(match &c {
                    Some(c) if !c.name.is_empty() => format!("{}: {}", c.name, set.values[slot]),
                    _ => format!("{}", set.values[slot]),
                });
                if r.changed() {
                    set.values[slot] = v.round() as i32;
                }
                if r.secondary_clicked() || r.clicked() {
                    sel = Some(slot);
                }
                let (text, on) = match &c {
                    Some(c) => (if c.short.is_empty() { format!("{} {}", KINDS.get(c.kind as usize).unwrap_or(&"CC"), c.num) } else { c.short.clone() }, c.on),
                    None => ("-".to_string(), false),
                };
                let label = ui.add(egui::Label::new(RichText::new(text).small().color(if on { orange } else if sel == Some(slot) { TEXT } else { TEXT_DIM })).truncate().sense(Sense::click()));
                if label.hint("Click to switch this knob on or off; right-click the knob to set it up").clicked() {
                    match set.ctls.iter_mut().find(|c| c.slot as usize == slot) {
                        Some(c) => c.on = !c.on,
                        None => sel = Some(slot),
                    }
                }
            });
        }
    });
    if let Some(slot) = sel.filter(|s| s / 9 == page) {
        section(ui, "CONTROL SETTINGS");
        let made = set.ctl(slot).is_some();
        if !made {
            if ui.button(format!("Set up knob {}", slot % 9 + 1)).clicked() {
                set.ctl_mut(slot);
            }
        } else {
            let own = set.chan as i32;
            let c = set.ctl_mut(slot);
            egui::Grid::new("midictl").num_columns(2).show(ui, |ui| {
                ui.label("Full name");
                ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(240.0));
                ui.end_row();
                ui.label("Short name");
                ui.add(egui::TextEdit::singleline(&mut c.short).desired_width(120.0));
                ui.end_row();
                ui.label("Controller");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut c.num).range(0..=16383).speed(0.2));
                    let mut kind = (c.kind.clamp(0, 4)) as usize;
                    egui::ComboBox::from_id_salt("midikind").width(70.0).selected_text(KINDS[kind]).show_ui(ui, |ui| {
                        for (i, k) in KINDS.iter().enumerate() {
                            ui.selectable_value(&mut kind, i, *k);
                        }
                    });
                    c.kind = kind as i32;
                    ui.checkbox(&mut c.on, "Active");
                });
                ui.end_row();
                ui.label("Range");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut c.lo).range(0..=127).speed(0.2));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut c.hi).range(0..=127).speed(0.2));
                });
                ui.end_row();
                ui.label("Channel");
                ui.horizontal(|ui| {
                    let mut own_chan = c.chan < 0;
                    if ui.checkbox(&mut own_chan, "The unit's own").changed() {
                        c.chan = if own_chan { -1 } else { own };
                    }
                    if c.chan >= 0 {
                        let mut ch = c.chan + 1;
                        ui.add(egui::DragValue::new(&mut ch).range(1..=16).speed(0.1));
                        c.chan = (ch - 1).clamp(0, 15);
                    }
                });
                ui.end_row();
            });
            if ui.button("Remove this knob's set-up").clicked() {
                set.ctls.retain(|c| c.slot as usize != slot);
            }
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(page_id, page);
        d.insert_temp(sel_id, sel);
    });
    ui.add_space(4.0);
    ui.label(RichText::new("Makes no sound of its own. Notes on this channel, with their echoes and arpeggios, go out as MIDI; record what the device plays back through a mixer track's input.").small().color(TEXT_DIM));
    if set != was {
        *raw = set.write();
        return true;
    }
    false
}

/// A number box that reads "none" below zero. `shift` is added for show (patches count from one).
fn num_or_none(ui: &mut Ui, v: &mut i32, max: i32, shift: i32) -> egui::Response {
    let r = ui.add(egui::DragValue::new(v).range(-1..=max).speed(0.15).custom_formatter(move |x, _| if x < 0.0 { "none".into() } else { format!("{}", x as i32 + shift) }).custom_parser(move |s| s.trim().parse::<f64>().ok().map(|x| x - shift as f64).or(Some(-1.0))));
    r
}

/// The older control board, drawn in our own look where the block places its controls.
fn board_panel(ui: &mut Ui, raw: &mut [u8]) -> bool {
    use cereal_engine::dash;
    let Some(board) = dash::Board::parse(raw) else {
        ui.label(RichText::new("This board's layout could not be read. The block is kept untouched.").color(TEXT_DIM));
        return false;
    };
    let mut changed = false;
    ui.label(RichText::new("Makes no sound. Each control sends its position to the controls linked to this channel, one output per control.").color(TEXT_DIM));
    let (area, _) = ui.allocate_exact_size(egui::vec2(board.w as f32, board.h as f32), egui::Sense::hover());
    let p = ui.painter_at(area);
    p.rect_filled(area, 3.0, Color32::from_rgb(52, 56, 62));
    p.rect_stroke(area, 3.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(24, 26, 30)), egui::StrokeKind::Inside);
    for (i, c) in board.controls.iter().enumerate() {
        let r = egui::Rect::from_min_size(area.min + egui::vec2(c.rect[0] as f32, c.rect[1] as f32), egui::vec2(c.rect[2].max(4) as f32, c.rect[3].max(4) as f32));
        let v = c.out.and_then(|o| dash::value(raw, o)).unwrap_or(0.0);
        let id = ui.id().with(("board", i));
        match c.kind {
            1 | 2 => {
                let resp = ui.interact(r, id, egui::Sense::click_and_drag()).hint(format!("{}: {:.0} %", c.name, v * 100.0));
                let mut now = v;
                if resp.dragged() {
                    let span = if c.kind == 1 { r.height().max(r.width()).max(20.0) } else { 150.0 };
                    let d = if c.kind == 1 && r.width() > r.height() { resp.drag_delta().x } else { -resp.drag_delta().y };
                    now = (v + d / span).clamp(0.0, 1.0);
                }
                if resp.double_clicked() {
                    now = 0.5;
                }
                if now != v {
                    if let Some(o) = c.out {
                        dash::set_value(raw, o, now);
                        changed = true;
                    }
                }
                if c.kind == 1 {
                    // a slider: a slot with a cap, upright or lying down as its rectangle is
                    let flat = r.width() > r.height();
                    let slot = if flat { egui::Rect::from_center_size(r.center(), egui::vec2(r.width() - 6.0, 4.0)) } else { egui::Rect::from_center_size(r.center(), egui::vec2(4.0, r.height() - 6.0)) };
                    p.rect_filled(slot, 2.0, Color32::from_rgb(20, 22, 26));
                    let cap = if flat {
                        egui::Rect::from_center_size(egui::pos2(r.left() + 5.0 + now * (r.width() - 10.0), r.center().y), egui::vec2(8.0, r.height().min(22.0)))
                    } else {
                        egui::Rect::from_center_size(egui::pos2(r.center().x, r.bottom() - 5.0 - now * (r.height() - 10.0)), egui::vec2(r.width().min(22.0), 8.0))
                    };
                    p.rect_filled(cap, 2.0, Color32::from_rgb(200, 204, 210));
                    p.line_segment([cap.left_center(), cap.right_center()], egui::Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 66)));
                } else {
                    let rad = r.width().min(r.height()) * 0.5;
                    p.circle_filled(r.center(), rad, Color32::from_rgb(34, 37, 42));
                    p.circle_stroke(r.center(), rad, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 124, 130)));
                    let a = (-225.0 + now * 270.0_f32).to_radians();
                    p.line_segment([r.center() + egui::vec2(a.cos(), a.sin()) * rad * 0.35, r.center() + egui::vec2(a.cos(), a.sin()) * rad * 0.9], egui::Stroke::new(2.0_f32, ACCENT));
                }
            }
            4 => {
                let resp = ui.interact(r, id, egui::Sense::click()).hint(format!("{}: {}", c.name, if v > 0.5 { "on" } else { "off" }));
                if resp.clicked() {
                    if let Some(o) = c.out {
                        dash::set_value(raw, o, if v > 0.5 { 0.0 } else { 1.0 });
                        changed = true;
                    }
                }
                p.rect_filled(r, 2.0, if v > 0.5 { ACCENT } else { Color32::from_rgb(30, 33, 38) });
                p.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 124, 130)), egui::StrokeKind::Inside);
            }
            5 => {
                p.text(r.left_top(), egui::Align2::LEFT_TOP, &c.name, egui::FontId::proportional((r.height() - 1.0).clamp(8.0, 14.0)), Color32::from_rgb(210, 212, 216));
            }
            _ => {
                // the patch picker belongs to hardware lists we do not have: shown as an empty box
                p.rect_filled(r, 2.0, Color32::from_rgb(24, 26, 30));
                p.text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, "(no patch list)", egui::FontId::proportional(10.0), TEXT_DIM);
            }
        }
    }
    changed
}

fn key_sender_panel(ui: &mut Ui, raw: &mut Vec<u8>) -> bool {
    use cereal_engine::synth::{key_sender_range, key_sender_set, key_sender_value};
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let black = |k: u8| matches!(k % 12, 1 | 3 | 6 | 8 | 10);
    let mut changed = false;
    ui.label(RichText::new("Makes no sound. Each note sends its key's value (output: note) and how hard it was hit (output: velocity) to the controls linked to this channel. Link a control with right-click, Link to controller.").color(TEXT_DIM));
    let sel_id = ui.id().with("keysel");
    let mut sel: u8 = ui.data(|d| d.get_temp(sel_id)).unwrap_or(60);
    let (mut lo, mut hi) = key_sender_range(raw);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(120.0 * 5.0, 120.0), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Color32::from_rgb(28, 32, 36));
    for k in 0..120u8 {
        let x = rect.left() + k as f32 * 5.0;
        let v = key_sender_value(raw, k);
        let inside = k >= lo && k <= hi;
        let col = match (k == sel, black(k), inside) {
            (true, _, _) => Color32::from_rgb(90, 160, 240),
            (_, true, true) => Color32::from_rgb(110, 110, 120),
            (_, false, true) => Color32::from_rgb(200, 200, 210),
            (_, true, false) => Color32::from_rgb(60, 60, 66),
            (_, false, false) => Color32::from_rgb(95, 95, 102),
        };
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x + 0.5, rect.bottom() - v * rect.height()), egui::pos2(x + 4.5, rect.bottom())), 0.0, col);
        if k % 12 == 0 {
            p.text(egui::pos2(x + 1.0, rect.top() + 1.0), egui::Align2::LEFT_TOP, format!("C{}", k / 12), egui::FontId::proportional(9.0), TEXT_DIM);
        }
    }
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let k = (((pos.x - rect.left()) / 5.0).floor().clamp(0.0, 119.0)) as u8;
        sel = k;
        if resp.dragged() {
            key_sender_set(raw, k, ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0));
            changed = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(sel_id, sel));
    ui.horizontal(|ui| {
        ui.label(format!("Value for {}{}", NAMES[sel as usize % 12], sel / 12));
        let mut v = key_sender_value(raw, sel) * 100.0;
        if ui.add(egui::Slider::new(&mut v, 0.0..=100.0).suffix(" %")).changed() {
            key_sender_set(raw, sel, v / 100.0);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        use cereal_engine::synth::{key_sender_extra, key_sender_set_extra};
        let (a, r, off) = key_sender_extra(raw);
        let (mut a, mut r) = (a / 2.0, r / 2.0);
        let mut on = off.is_some();
        let mut v = if raw.len() >= 500 { f32::from_le_bytes([raw[496], raw[497], raw[498], raw[499]]).clamp(0.0, 1.0) * 100.0 } else { 0.0 };
        let mut hit = ui.checkbox(&mut on, "Note off value").hint("Send a value of its own when the last key is let go").changed();
        hit |= ui.add_enabled(on, egui::Slider::new(&mut v, 0.0..=100.0).suffix(" %")).changed();
        ui.separator();
        ui.label("Smoothing");
        let link_id = ui.id().with("keysmooth_link");
        let mut link: bool = ui.data(|d| d.get_temp(link_id)).unwrap_or(false);
        let ha = ui.add(egui::Slider::new(&mut a, 0.0..=1.0).text("attack").custom_formatter(|x, _| format!("{:.0} ms", x * 2000.0))).hint("Time the value takes to reach a pressed key's value").changed();
        let hr = ui.add(egui::Slider::new(&mut r, 0.0..=1.0).text("release").custom_formatter(|x, _| format!("{:.0} ms", x * 2000.0))).hint("Time the value takes to reach the note-off value").changed();
        if ui.selectable_label(link, "Link").hint("Move attack and release together").clicked() {
            link = !link;
            ui.data_mut(|d| d.insert_temp(link_id, link));
        }
        if link && ha {
            r = a;
        } else if link && hr {
            a = r;
        }
        if hit || ha || hr {
            key_sender_set_extra(raw, a, r, None);
            raw[496..500].copy_from_slice(&(v / 100.0).to_le_bytes());
            raw[500] = on as u8;
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Key range");
        let mut range_changed = false;
        range_changed |= ui.add(egui::DragValue::new(&mut lo).range(0..=119).custom_formatter(|v, _| format!("{}{}", NAMES[v as usize % 12], v as usize / 12))).changed();
        ui.label("to");
        range_changed |= ui.add(egui::DragValue::new(&mut hi).range(0..=119).custom_formatter(|v, _| format!("{}{}", NAMES[v as usize % 12], v as usize / 12))).changed();
        if range_changed {
            key_sender_set(raw, 255, 0.0);
            let (a, b) = (lo.min(hi), hi.max(lo));
            raw[488..492].copy_from_slice(&(a as u32).to_le_bytes());
            raw[492..496].copy_from_slice(&(b as u32).to_le_bytes());
            changed = true;
        }
        ui.menu_button("Auto mapping", |ui| {
            let mut map: Option<bool> = None;
            if ui.button("White and black keys").clicked() {
                map = Some(false);
            }
            if ui.button("White keys only").clicked() {
                map = Some(true);
            }
            if let Some(white_only) = map {
                // the range runs from nothing at its lowest key to full at its highest; with white keys only a black key sends what the white key under it sends
                let keys: Vec<u8> = (lo..=hi).filter(|&k| !white_only || !black(k)).collect();
                let top = keys.len().saturating_sub(1).max(1) as f32;
                for k in 0..120u8 {
                    let v = if k < lo {
                        0.0
                    } else if k > hi {
                        1.0
                    } else {
                        keys.iter().rposition(|&q| q <= k).map(|i| i as f32 / top).unwrap_or(0.0)
                    };
                    key_sender_set(raw, k, v);
                }
                changed = true;
                ui.close_menu();
            }
        });
    });
    ui.label(RichText::new("Drag over the bars to draw values. The note-off value and smoothing settings are kept in the file but are not worked yet.").small().color(TEXT_DIM));
    changed
}

/// The path scratcher: a drawn phrase for every key.
fn travel_panel(ui: &mut Ui, g: &mut cereal_engine::synth::Gen) -> bool {
    use cereal_engine::travel::{self, Phrase, Point, Set};
    const KINDS: [&str; 6] = ["Half cosine", "Straight", "Handles", "Tension", "Quarter sine", "Quarter cosine"];
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.button("Sample...").hint("Pick the sample the phrases move through").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]).pick_file() {
                if let Ok(s) = cereal_engine::sample::Sample::load(&p) {
                    g.smp = Some(std::sync::Arc::new(s));
                    g.raw = travel::with_sample(&g.raw, &p.to_string_lossy());
                    changed = true;
                }
            }
        }
        let name = cereal_engine::synth::sample_named_in("Wave Traveller", &g.raw).unwrap_or_else(|| "(no sample)".into());
        ui.label(RichText::new(name.rsplit(['/', '\\']).next().unwrap_or("").to_string()).strong());
        if g.smp.is_none() {
            ui.label(RichText::new("not found").color(TEXT_DIM));
        }
    });
    let mut set = Set::parse(&g.raw);
    let was = set.clone();
    let mut key = set.shown.min(127) as u8;
    let mut ph = set.phrase(key).cloned().unwrap_or_default();
    let id = ui.id().with("travel");
    let mut level: bool = ui.data(|d| d.get_temp(id.with("level"))).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Key").color(TEXT_DIM));
        let drawn: Vec<u8> = set.phrases.iter().filter(|p| p.1 != Phrase::default()).map(|p| p.0).collect();
        if ui.small_button("<").hint("The drawn phrase below this key").clicked() {
            if let Some(&k) = drawn.iter().rev().find(|&&k| k < key) {
                key = k;
            }
        }
        let mut k = key as i32;
        if ui.add(egui::DragValue::new(&mut k).range(0..=127).custom_formatter(|v, _| key_name(v as u8))).hint("The key whose phrase is shown. Every key plays its own phrase").changed() {
            key = k.clamp(0, 127) as u8;
        }
        if ui.small_button(">").hint("The drawn phrase above this key").clicked() {
            if let Some(&k) = drawn.iter().find(|&&k| k > key) {
                key = k;
            }
        }
        if key as usize != set.shown {
            ph = set.phrase(key).cloned().unwrap_or_default();
        }
        ui.add(egui::TextEdit::singleline(&mut ph.name).desired_width(110.0).hint_text("phrase name"));
        ui.add_space(8.0);
        if ui.selectable_label(!level, "WAVE").hint("Draw where the play head is along the phrase").clicked() {
            level = false;
        }
        if ui.selectable_label(level, "VOL").hint("Draw the level along the phrase").clicked() {
            level = true;
        }
        ui.menu_button("Phrase", |ui| {
            if ui.button("Copy").clicked() {
                ui.data_mut(|d| d.insert_temp(egui::Id::new("travel_clip"), ph.clone()));
                ui.close_menu();
            }
            let clip: Option<Phrase> = ui.data(|d| d.get_temp(egui::Id::new("travel_clip")));
            if ui.add_enabled(clip.is_some(), egui::Button::new("Paste")).clicked() {
                ph = clip.unwrap();
                ui.close_menu();
            }
            for (label, top) in [("Normalise to 100 %", 1.0f32), ("Normalise to 50 %", 0.5)] {
                if ui.button(label).hint("Stretch the path so that it covers this much of the stretch between start and end").clicked() {
                    let lo = ph.wave.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                    let hi = ph.wave.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                    if hi - lo > 1e-6 {
                        for p in &mut ph.wave {
                            p.y = (p.y - lo) / (hi - lo) * top;
                            p.before[1] *= top / (hi - lo);
                            p.after[1] *= top / (hi - lo);
                        }
                    }
                    ui.close_menu();
                }
            }
            if ui.button("Reset").hint("Back to the sample played straight through").clicked() {
                ph = Phrase::default();
                ui.close_menu();
            }
        });
    });
    set.shown = key as usize;
    ui.data_mut(|d| d.insert_temp(id.with("level"), level));
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.checkbox(&mut ph.own_speed, "Own speed").hint("Off: the phrase lasts a number of beats at the song's tempo. On: it runs at a share of the sample's own speed");
        });
        if ph.own_speed {
            cap_knob(ui, "Speed", &mut ph.speed, 0.05, 4.0, 1.0, |v| format!("Speed: {:.0} % of the sample's own with a straight path", v * 100.0));
        } else {
            cap_knob(ui, "Length", &mut ph.beats, 0.25, 64.0, 4.0, |v| format!("Length: {v:.2} beats"));
        }
        cap_knob(ui, "Start", &mut ph.start, 0.0, 1.0, 0.0, |v| format!("Start of the stretch the path moves in: {:.1} %", v * 100.0));
        cap_knob(ui, "End", &mut ph.end, 0.0, 1.0, 1.0, |v| format!("End of the stretch the path moves in: {:.1} %", v * 100.0));
        cap_knob(ui, "Attack", &mut ph.attack, 0.0, 500.0, 1.0, |v| format!("Fade in: {v:.0} ms"));
        cap_knob(ui, "Release", &mut ph.release, 0.0, 500.0, 1.0, |v| format!("Fade out once the key is let go: {v:.0} ms"));
        if ph.end < ph.start {
            ph.end = ph.start;
        }
    });
    // the whole wave, with the stretch in use lit
    if let Some(s) = &g.smp {
        let at = ui.cursor().min;
        let wide = ui.available_width().max(200.0);
        draw_wave(ui, &s.frames, TEXT_DIM);
        let r = egui::Rect::from_min_size(at, Vec2::new(wide, 70.0));
        let shade = Color32::from_black_alpha(120);
        ui.painter().rect_filled(egui::Rect::from_min_max(r.min, Pos2::new(r.left() + r.width() * ph.start, r.bottom())), 0.0, shade);
        ui.painter().rect_filled(egui::Rect::from_min_max(Pos2::new(r.left() + r.width() * ph.end, r.top()), r.max), 0.0, shade);
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(360.0), 190.0), Sense::click_and_drag());
    let (w, h) = (rect.width() - 10.0, rect.height() - 10.0);
    let px = |x: f32, y: f32| Pos2::new(rect.left() + 5.0 + x * w, rect.bottom() - 5.0 - y * h);
    let un = |p: Pos2| (((p.x - rect.left() - 5.0) / w).clamp(0.0, 1.0), ((rect.bottom() - 5.0 - p.y) / h).clamp(0.0, 1.0));
    let mutes = ph.mutes.clone();
    let line = if level { &mut ph.level } else { &mut ph.wave };
    // what is under the pointer: (point, 0 the point itself / 1 the handle before / 2 the handle after)
    let grab = |line: &[Point], at: Pos2| {
        let mut best: Option<((usize, u8), f32)> = None;
        for (i, p) in line.iter().enumerate() {
            let mut spots = vec![(0u8, px(p.x, p.y))];
            if p.kind == 2 && i > 0 {
                spots.push((1, px(p.x + p.before[0], p.y + p.before[1])));
            }
            if line.get(i + 1).is_some_and(|n| n.kind == 2) {
                spots.push((2, px(p.x + p.after[0], p.y + p.after[1])));
            }
            for (which, spot) in spots {
                let d = spot.distance(at);
                if d < 9.0 && best.is_none_or(|b| d < b.1) {
                    best = Some(((i, which), d));
                }
            }
        }
        best.map(|b| b.0)
    };
    let held_id = id.with("held");
    let menu_id = id.with("menu");
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let hit = resp.interact_pointer_pos().and_then(|at| grab(line, at));
        ui.data_mut(|d| d.insert_temp(held_id, hit));
    }
    let held: Option<(usize, u8)> = ui.data(|d| d.get_temp(held_id)).flatten();
    if let (true, Some((i, which)), Some(at)) = (resp.dragged_by(egui::PointerButton::Primary), held, resp.interact_pointer_pos()) {
        if i < line.len() {
            let (x, y) = un(at);
            let last = line.len() - 1;
            match which {
                0 => {
                    line[i].x = if i == 0 { 0.0 } else if i == last { 1.0 } else { x.clamp(line[i - 1].x, line[i + 1].x) };
                    line[i].y = y;
                }
                1 => {
                    line[i].before = [(x - line[i].x).min(0.0), y - line[i].y];
                    if line[i].locked {
                        let far = (line[i].after[0].powi(2) + line[i].after[1].powi(2)).sqrt();
                        let near = (line[i].before[0].powi(2) + line[i].before[1].powi(2)).sqrt().max(1e-6);
                        line[i].after = [-line[i].before[0] / near * far, -line[i].before[1] / near * far];
                    }
                }
                _ => {
                    line[i].after = [(x - line[i].x).max(0.0), y - line[i].y];
                    if line[i].locked {
                        let far = (line[i].before[0].powi(2) + line[i].before[1].powi(2)).sqrt();
                        let near = (line[i].after[0].powi(2) + line[i].after[1].powi(2)).sqrt().max(1e-6);
                        line[i].before = [-line[i].after[0] / near * far, -line[i].after[1] / near * far];
                    }
                }
            }
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.insert_temp(held_id, None::<(usize, u8)>));
    }
    if resp.clicked() {
        if let Some(at) = resp.interact_pointer_pos().filter(|&at| grab(line, at).is_none()) {
            let (x, y) = un(at);
            let i = line.partition_point(|p| p.x <= x).clamp(1, line.len().max(2) - 1);
            let kind = line.get(i).map_or(1, |p| p.kind);
            line.insert(i.min(line.len()), Point { kind, ..Point::plain(x, y) });
        }
    }
    if resp.secondary_clicked() {
        let hit = resp.interact_pointer_pos().and_then(|at| grab(line, at)).filter(|h| h.1 == 0).map(|h| h.0);
        ui.data_mut(|d| d.insert_temp(menu_id, hit));
    }
    let picked: Option<usize> = ui.data(|d| d.get_temp(menu_id)).flatten();
    resp.context_menu(|ui| match picked.filter(|&i| i < line.len()) {
        Some(i) => {
            ui.label(RichText::new("The piece arriving at this point").small().color(TEXT_DIM));
            for (k, name) in KINDS.iter().enumerate() {
                if ui.radio(line[i].kind as usize == k, *name).clicked() {
                    line[i].kind = k as u8;
                }
            }
            if line[i].kind == 3 {
                ui.add(egui::Slider::new(&mut line[i].tension, 0.02..=0.98).text("Tension"));
            }
            ui.checkbox(&mut line[i].locked, "Handles in one line");
            if ui.add_enabled(i > 0 && i + 1 < line.len(), egui::Button::new("Delete")).clicked() {
                line.remove(i);
                ui.close_menu();
            }
        }
        None => {
            ui.label(RichText::new("Click an empty place to add a point. Right-click a point for its shape.").color(TEXT_DIM));
        }
    });
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, PANEL_DARK);
    for q in 1..4 {
        let x = rect.left() + 5.0 + w * q as f32 / 4.0;
        p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
    }
    let n = w as usize;
    let mut run: Vec<Pos2> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let x = i as f32 / n.max(1) as f32;
        let open = travel::open_at(&mutes, x);
        if !open {
            p.line_segment([px(x, 0.0), px(x, 1.0)], Stroke::new(1.0_f32, Color32::from_black_alpha(90)));
        }
        run.push(px(x, travel::at(line, x).clamp(0.0, 1.0)));
    }
    p.add(egui::Shape::line(run, Stroke::new(1.5_f32, ACCENT)));
    for (i, pt) in line.iter().enumerate() {
        let c = px(pt.x, pt.y);
        if pt.kind == 2 && i > 0 {
            let hnd = px(pt.x + pt.before[0], pt.y + pt.before[1]);
            p.line_segment([c, hnd], Stroke::new(1.0_f32, TEXT_DIM));
            p.circle_stroke(hnd, 3.0, Stroke::new(1.0_f32, TEXT_DIM));
        }
        if line.get(i + 1).is_some_and(|nx| nx.kind == 2) {
            let hnd = px(pt.x + pt.after[0], pt.y + pt.after[1]);
            p.line_segment([c, hnd], Stroke::new(1.0_f32, TEXT_DIM));
            p.circle_stroke(hnd, 3.0, Stroke::new(1.0_f32, TEXT_DIM));
        }
        p.circle_filled(c, 3.5, if held == Some((i, 0)) { Color32::WHITE } else { ACCENT });
    }
    // the mute bar: drag with the left button to silence a stretch, with the right to let it sound again
    let (bar, bresp) = ui.allocate_exact_size(Vec2::new(rect.width(), 14.0), Sense::click_and_drag());
    const CELLS: usize = 256;
    let mut cells: Vec<bool> = (0..CELLS).map(|i| travel::open_at(&ph.mutes, (i as f32 + 0.5) / CELLS as f32)).collect();
    let (left, right) = ui.input(|i| (i.pointer.primary_down(), i.pointer.secondary_down()));
    if let (true, Some(at)) = (bresp.is_pointer_button_down_on() && (left || right), ui.input(|i| i.pointer.latest_pos())) {
        let c = (((at.x - bar.left() - 5.0) / w).clamp(0.0, 0.9999) * CELLS as f32) as usize;
        if cells[c] != right {
            cells[c] = right;
            let mut m = vec![(cells[0], 0.0)];
            for i in 1..CELLS {
                if cells[i] != cells[i - 1] {
                    m.push((cells[i], i as f32 / CELLS as f32));
                }
            }
            ph.mutes = m;
        }
    }
    let bp = ui.painter_at(bar);
    bp.rect_filled(bar, 2.0, PANEL_DARK);
    for (i, on) in cells.iter().enumerate() {
        if *on {
            let x0 = bar.left() + 5.0 + w * i as f32 / CELLS as f32;
            bp.rect_filled(egui::Rect::from_min_max(Pos2::new(x0, bar.top() + 3.0), Pos2::new(x0 + w / CELLS as f32 + 0.5, bar.bottom() - 3.0)), 0.0, TEXT_DIM);
        }
    }
    bresp.hint("Muted stretches. Drag with the left button to silence, with the right button to let it sound again");
    ui.label(RichText::new("Across is time along the phrase, up is the place in the sample between start and end. Fade times are read as milliseconds, which is an estimate.").small().color(TEXT_DIM));
    if set.phrase(key).map_or(ph != Phrase::default(), |old| *old != ph) {
        *set.phrase_mut(key) = ph;
    }
    if set != was {
        g.raw = set.store(&g.raw);
        changed = true;
    }
    changed
}

/// The knobs of a built-in instrument.
fn instrument_panel(ui: &mut Ui, ci: usize, g: &mut cereal_engine::synth::Gen) -> bool {
    use cereal_engine::synth::{def, Kind, OSC_FIELDS, SHAPES};
    let mut changed = false;
    match g.kind {
        Kind::Osc3 => {
            let d = def(g.kind);
            for k in 0..3 {
                let base = k * OSC_FIELDS;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("OSC {}", k + 1)).strong());
                    ui.add_space(6.0);
                    // shape buttons
                    for (i, name) in SHAPES.iter().enumerate() {
                        let on = g.p[base + 1] as usize == i;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(22.0, 18.0), Sense::click());
                        let fill = if on { ACCENT } else { Color32::from_rgb(52, 58, 62) };
                        ui.painter().rect_filled(rect, 2.0, fill);
                        let ink = if on { Color32::BLACK } else { TEXT_DIM };
                        let pts: Vec<Pos2> = (0..=16)
                            .map(|j| {
                                let t = j as f32 / 16.0;
                                let y = match i {
                                    0 => (t * std::f32::consts::TAU).sin(),
                                    1 => 1.0 - 4.0 * ((t + 0.25).fract() - 0.5).abs(),
                                    2 => if t < 0.5 { 0.9 } else { -0.9 },
                                    3 => 2.0 * t - 1.0,
                                    4 => {
                                        let x = 2.0 * t - 1.0;
                                        x * (2.0 - x.abs())
                                    }
                                    5 => ((j * 7919 % 13) as f32 / 6.0 - 1.0) * 0.9,
                                    _ => (t * std::f32::consts::TAU).sin() * (1.0 - t),
                                };
                                Pos2::new(rect.left() + 3.0 + t * (rect.width() - 6.0), rect.center().y - y * 6.0)
                            })
                            .collect();
                        ui.painter().add(egui::Shape::line(pts, Stroke::new(1.2_f32, ink)));
                        if resp.hint(*name).clicked() && !on {
                            g.p[base + 1] = i as f32;
                            changed = true;
                        }
                    }
                    let inv = &mut g.p[21 + k];
                    if ui.selectable_label(*inv != 0.0, "INV").hint("Invert this oscillator").clicked() {
                        *inv = if *inv != 0.0 { 0.0 } else { 1.0 };
                        changed = true;
                    }
                    if k == 2 {
                        let am = &mut g.p[24];
                        if ui.selectable_label(*am != 0.0, "AM").hint("Oscillator 3 shapes the level of the other two").clicked() {
                            *am = if *am != 0.0 { 0.0 } else { 1.0 };
                            changed = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let mut kn = |ui: &mut Ui, label: &str, idx: usize, hint: &dyn Fn(f32) -> String| {
                        let p = &d.params[idx];
                        let mut v = g.p[idx];
                        if cap_knob_for(ui, label, &mut v, p.min as f32, p.max as f32, p.def as f32, hint, Some(cereal_engine::song::Target::ChanParam(ci, idx as u16))) {
                            g.p[idx] = v.round();
                            changed = true;
                        }
                    };
                    if k > 0 {
                        kn(ui, "LEVEL", base - 1, &|v| format!("{:.0} %", v / 128.0 * 100.0));
                    } else {
                        ui.add_space(52.0);
                    }
                    kn(ui, "PAN", base, &|v| format!("{:+.0} %", v / 64.0 * 100.0));
                    kn(ui, "COARSE", base + 2, &|v| format!("{v:+.0} semitones"));
                    kn(ui, "FINE", base + 3, &|v| format!("{v:+.0} cents"));
                    kn(ui, "PHASE", base + 4, &|v| format!("{:.0} %", v / 64.0 * 100.0));
                    kn(ui, "DETUNE", base + 5, &|v| format!("{:.0} %", v / 64.0 * 100.0));
                    if k == 2 {
                        kn(ui, "RANDOM", 20, &|v| format!("Phase randomness {:.0} %", v / 64.0 * 100.0));
                    }
                });
                ui.add_space(4.0);
            }
        }
        Kind::Additive => {
            use cereal_engine::additive::{ix, LFO_SHAPES, TIMBRES};
            let d = def(g.kind);
            // (heading, settings shown as knobs, on/off settings)
            let groups: [(&str, &[usize], &[usize]); 9] = [
                ("TIMBRE", &[ix::PHASE, ix::LHP, ix::SUB, ix::SUB + 1, ix::SUB + 2, ix::MASK_MIX, ix::MASK_LFO, ix::TREMOLO], &[]),
                ("EQ", &[5, 6, 7, 8, 9, 10], &[]),
                ("MASK", &[11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], &[]),
                ("AMP", &[ix::ATTACK, ix::DECAY, ix::RELEASE, ix::PLUCK, ix::VOLUME, ix::PITCH, 36, 37, ix::VIBRATO, ix::PITCH_LFO, 42], &[30, 31, 32, 34, 40, 41]),
                ("FILTER", &[ix::CUTOFF, ix::F_ATTACK, ix::F_DECAY, 46, 47, ix::F_AMOUNT, 52, ix::KEY_TRACK, ix::CUTOFF_LFO, ix::RESONANCE, ix::RES_OFFSET, 58], &[ix::FILTER_ON, 49, 51, 56, 59, 60]),
                ("UNISON", &[ix::UNI_ORDER, ix::UNI_PAN, ix::UNI_PITCH, ix::UNI_PHASE, 65, 66], &[]),
                ("PHASER", &[ix::PH_AMOUNT, 96, ix::PH_SCALE, ix::PH_WIDTH, ix::PH_WIDTH_LFO, ix::PH_OFFSET, ix::PH_OFFSET_LFO, ix::PH_SPEED, 74], &[]),
                ("HARMONIZER", &[ix::HZ_AMOUNT, ix::HZ_WIDTH, ix::HZ_LFO, ix::HZ_STRENGTH, ix::HZ_TYPE, 81], &[78]),
                ("LFO AND EFFECTS", &[ix::LFO_ATTACK, ix::LFO_SPEED, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95], &[83]),
            ];
            ui.horizontal(|ui| {
                for (label, idx, names) in [("Timbre", ix::TIMBRE, &TIMBRES[..]), ("LFO", ix::LFO_SHAPE, &LFO_SHAPES[..])] {
                    ui.label(RichText::new(label).color(TEXT_DIM));
                    let cur = (g.p[idx] as usize).min(names.len() - 1);
                    egui::ComboBox::from_id_salt(("additive", idx)).width(120.0).selected_text(names[cur]).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(i == cur, *n).clicked() && i != cur {
                                g.p[idx] = i as f32;
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(10.0);
                }
            });
            for (title, knobs, toggles) in groups {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).strong());
                    for &i in toggles {
                        let on = g.p[i] != 0.0;
                        if ui.selectable_label(on, d.params[i].name).clicked() {
                            g.p[i] = if on { 0.0 } else { 1.0 };
                            changed = true;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for &i in knobs {
                        let p = &d.params[i];
                        let mut v = g.p[i];
                        // a short cap from the setting's name: the last word, or its number
                        let cap: String = p.name.rsplit(' ').next().unwrap_or(p.name).chars().take(7).collect::<String>().to_uppercase();
                        let name = p.name;
                        if cap_knob_for(ui, &cap, &mut v, p.min as f32, p.max as f32, p.def as f32, &|v| format!("{name}: {v:.0}"), Some(cereal_engine::song::Target::ChanParam(ci, i as u16))) {
                            g.p[i] = v.round();
                            changed = true;
                        }
                    }
                });
            }
        }
        Kind::Fm => {
            changed |= fm_panel(ui, g);
        }
        Kind::Packs => {
            let (pack, preset) = g.pack_and_preset();
            ui.label(RichText::new(format!("{preset}  ({pack})")).strong());
            ui.label(RichText::new("Pack sounds are protected files and are not read. A stand-in voice chosen by the preset's name plays instead.").color(TEXT_DIM));
        }
        Kind::Image => {
            use cereal_engine::beep;
            let mut fresh = false;
            ui.horizontal(|ui| {
                if ui.button("Open picture...").hint("Load a picture (PNG, JPEG or BMP); it is kept inside the project").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "bmp"]).pick_file() {
                        if let Ok(pic) = image::open(&p) {
                            // wide pictures are squeezed to a sensible size; the row limit squeezes the height when played
                            let pic = if pic.width() > 2048 || pic.height() > 256 { pic.resize(2048, 256, image::imageops::FilterType::Triangle) } else { pic };
                            let rgb = pic.to_rgb8();
                            g.raw = beep::with_picture(&g.raw, rgb.width() as usize, rgb.height() as usize, rgb.as_raw());
                            fresh = true;
                        }
                    }
                }
                if g.raw.len() >= 68 {
                    for &(name, place, _, _) in beep::FIELDS.iter().filter(|f| f.3 == 1) {
                        let mut on = beep::get(&g.raw, place) != 0;
                        if ui.checkbox(&mut on, name).changed() {
                            beep::put(&mut g.raw, place, on as i32);
                            fresh = true;
                        }
                    }
                }
            });
            if g.raw.len() >= 68 {
                ui.horizontal(|ui| {
                    for &(name, place, lo, hi) in beep::FIELDS.iter().filter(|f| f.3 > 1) {
                        let mut v = beep::get(&g.raw, place) as f32;
                        let cents = beep::get(&g.raw, 3) != 0;
                        let say = move |v: f32| match place {
                            1 if cents => format!("Range: {v:.0} cents"),
                            1 => format!("Range: {v:.0} Hz at C5"),
                            2 => format!("Length: {:.1} ms a pixel", v / 44.1),
                            _ => format!("At most {v:.0} sine waves a note"),
                        };
                        if cap_knob(ui, name, &mut v, lo as f32, hi as f32, if place == 1 { 7200.0 } else if place == 2 { 2048.0 } else { 64.0 }, say) {
                            beep::put(&mut g.raw, place, v.round() as i32);
                            fresh = true;
                        }
                    }
                });
            }
            if fresh {
                g.img = beep::Image::parse(&g.raw).map(std::sync::Arc::new);
                changed = true;
            }
            match &g.img {
                Some(im) => {
                    // the picture is drawn from a texture made once for each picture
                    let key = egui::Id::new("picture_synth_tex");
                    let at = std::sync::Arc::as_ptr(im) as usize;
                    let held: Option<(usize, egui::TextureHandle)> = ui.ctx().data(|d| d.get_temp(key));
                    let tex = match held {
                        Some((p, t)) if p == at => t,
                        _ => {
                            let mut rgb = vec![0u8; im.w * im.h * 3];
                            for y in 0..im.h {
                                for x in 0..im.w {
                                    for c in 0..3 {
                                        rgb[((im.h - 1 - y) * im.w + x) * 3 + c] = (im.px[(y * im.w + x) * 3 + c] * 255.0) as u8;
                                    }
                                }
                            }
                            let t = ui.ctx().load_texture("picture_synth", egui::ColorImage::from_rgb([im.w, im.h], &rgb), egui::TextureOptions::NEAREST);
                            ui.ctx().data_mut(|d| d.insert_temp(key, (at, t.clone())));
                            t
                        }
                    };
                    let w = ui.available_width().min(520.0);
                    ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(w, 150.0)).maintain_aspect_ratio(false));
                    ui.label(RichText::new(format!("Picture {} x {}, {:.1} s a pass. Red plays on the left, green on the right; each row is one sine wave above the played key.", im.w, im.h, im.seconds())).color(TEXT_DIM));
                }
                None => {
                    ui.label(RichText::new("The stored picture could not be read. The block is kept untouched.").color(TEXT_DIM));
                }
            }
        }
        Kind::Grain => {
            use cereal_engine::grain::{self, FIELDS, KEY_TO, TRANSIENTS};
            let knobs = |ui: &mut Ui, raw: &mut Vec<u8>, places: &[usize]| {
                let mut hit = false;
                for &(name, place, lo, hi, new) in FIELDS.iter().filter(|f| places.contains(&f.1)) {
                    let mut v = grain::get(raw, place) as f32;
                    if cap_knob(ui, name, &mut v, lo as f32, hi as f32, new as f32, |v| grain::says(place, v.round() as i32)) {
                        grain::put(raw, place, v.round() as i32);
                        hit = true;
                    }
                }
                hit
            };
            if let Some(s) = &g.smp {
                draw_wave(ui, &s.frames, TEXT_DIM);
            } else {
                ui.label(RichText::new("No sample yet: pick one with the button above, or drop one on the channel.").color(TEXT_DIM));
            }
            section(ui, "GRAINS");
            ui.horizontal(|ui| changed |= knobs(ui, &mut g.raw, &[2, 1, 3, 4]));
            section(ui, "EFFECTS");
            ui.horizontal(|ui| changed |= knobs(ui, &mut g.raw, &[5, 7, 6, 8]));
            section(ui, "TIME");
            ui.horizontal(|ui| {
                for (place, label, tip) in [(9, "Loop", "Start over from the top once the wave runs out"), (10, "Hold", "Keep the place in the wave standing still")] {
                    let mut on = grain::get(&g.raw, place) != 0;
                    if ui.checkbox(&mut on, label).hint(tip).changed() {
                        grain::put(&mut g.raw, place, on as i32);
                        changed = true;
                    }
                }
                let cur = (grain::get(&g.raw, 14).clamp(0, 3)) as usize;
                egui::ComboBox::from_id_salt("grain_key").width(130.0).selected_text(KEY_TO[cur]).show_ui(ui, |ui| {
                    for (i, name) in KEY_TO.iter().enumerate() {
                        if ui.selectable_label(cur == i, *name).clicked() {
                            grain::put(&mut g.raw, 14, i as i32);
                            changed = true;
                        }
                    }
                });
                changed |= knobs(ui, &mut g.raw, &[11]);
            });
            section(ui, "TRANSIENTS");
            ui.horizontal(|ui| {
                changed |= knobs(ui, &mut g.raw, &[12]);
                let cur = (grain::get(&g.raw, 13).clamp(0, 2)) as usize;
                egui::ComboBox::from_id_salt("grain_trans").width(110.0).selected_text(TRANSIENTS[cur]).show_ui(ui, |ui| {
                    for (i, name) in TRANSIENTS.iter().enumerate() {
                        if ui.selectable_label(cur == i, *name).clicked() {
                            grain::put(&mut g.raw, 13, i as i32);
                            changed = true;
                        }
                    }
                });
                ui.label(RichText::new("kept with the project; grains are not lined up to transients yet").small().color(TEXT_DIM));
            });
        }
        Kind::Travel => changed |= travel_panel(ui, g),
        Kind::Quiet => {
            ui.label(RichText::new("Makes no sound of its own: it sends control values or notes to other units. Its settings are kept untouched; sending is still to come.").color(TEXT_DIM));
        }
        Kind::DrumBank => {
            ui.label(RichText::new("One drum patch per key, played from the stored settings. The recorded drum waves are replaced by stand-ins of our own. The patch editor is still to come.").color(TEXT_DIM));
            let names = cereal_engine::drumbank::names(&g.raw);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for (k, n) in names.iter().enumerate() {
                    ui.label(format!("{}{}  {n}", ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][k % 12], k / 12));
                }
            });
        }
        Kind::Trio | Kind::Pocket | Kind::SawSyn => {
            let (groups, params) = cereal_engine::analog::page(g.kind);
            if g.p.len() < params.len() {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default sound plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, groups, &params);
            ui.label(RichText::new("Only the settings we could place are shown; the others stay as stored.").small().color(TEXT_DIM));
        }
        Kind::Poly => {
            use cereal_engine::analog::{POLY_COUNT, POLY_GROUPS, POLY_PARAMS};
            if g.p.len() < POLY_COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default sound plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, &POLY_GROUPS, &POLY_PARAMS);
        }
        Kind::Duo => {
            use cereal_engine::duo::{COUNT, GROUPS, PARAMS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default sound plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, &GROUPS, &PARAMS);
            ui.label(RichText::new("The gate and the echo keep step with the song. The built-in note runner and the tempo choices of the gate and the slow oscillator are kept in the file but not worked yet.").small().color(TEXT_DIM));
        }
        Kind::Skin => {
            use cereal_engine::membrane::{params, COUNT, GROUPS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default drum plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, &GROUPS[..8], params());
            ui.label(RichText::new("The drum rings out by itself whatever the length of the note. How hard a hit lands only matters through the four links.").small().color(TEXT_DIM));
        }
        Kind::Kit => {
            use cereal_engine::drumkit as kit;
            if g.p.len() < kit::COUNT || !kit::fits(&g.raw) {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and a default kit plays.").color(TEXT_DIM));
                return false;
            }
            let pick_id = egui::Id::new(("kit_pad", ci));
            let pat_id = egui::Id::new(("kit_pat", ci));
            let mut pad: usize = ui.data(|d| d.get_temp(pick_id)).unwrap_or(0).min(kit::PADS - 1);
            let mut pat: usize = ui.data(|d| d.get_temp(pat_id)).unwrap_or(0).min(kit::PATTERNS - 1);
            ui.label(RichText::new("Kit").small().color(TEXT_DIM));
            ui.horizontal_wrapped(|ui| {
                for (i, name) in kit::KIT_NAMES {
                    if i == 14 {
                        continue;
                    }
                    let mut v = g.p[kit::KIT + i];
                    if cap_knob(ui, &name.to_uppercase(), &mut v, 0.0, 1.0, if i == 1 { 0.36 } else { 0.0 }, |v| format!("{:.0} %", v * 100.0)) {
                        g.p[kit::KIT + i] = v;
                        changed = true;
                    }
                }
                let mut on = g.p[kit::KIT + 3] >= 0.5;
                if ui.toggle_value(&mut on, "Tone bands").changed() {
                    g.p[kit::KIT + 3] = if on { 1.0 } else { 0.0 };
                    changed = true;
                }
                for (b, hz) in kit::BANDS.iter().enumerate() {
                    let mut v = (g.p[kit::KIT + 4 + b] - 0.5) * 200.0;
                    let label = if *hz >= 1000.0 { format!("{:.1} kHz: ", hz / 1000.0) } else { format!("{hz:.0} Hz: ") };
                    if ui.add(egui::DragValue::new(&mut v).range(-100.0..=100.0).speed(1.0).prefix(label).suffix(" %")).changed() {
                        g.p[kit::KIT + 4 + b] = v / 200.0 + 0.5;
                        changed = true;
                    }
                }
                let mut on = g.p[kit::KIT + 12] >= 0.5;
                if ui.toggle_value(&mut on, "Ceiling").hint("Holds the output of the kit under full level").changed() {
                    g.p[kit::KIT + 12] = if on { 1.0 } else { 0.0 };
                    changed = true;
                }
                for (i, name) in [(13usize, "DRIVE"), (14, "LET GO")] {
                    let mut v = g.p[kit::KIT + i];
                    if cap_knob(ui, name, &mut v, 0.0, 1.0, if i == 14 { 0.5 } else { 0.0 }, |v| format!("{:.0} %", v * 100.0)) {
                        g.p[kit::KIT + i] = v;
                        changed = true;
                    }
                }
            });
            ui.add_space(3.0);
            ui.label(RichText::new("Pads").small().color(TEXT_DIM));
            egui::Grid::new(("kit_pads", ci)).num_columns(9).spacing([6.0, 2.0]).show(ui, |ui| {
                for k in 0..kit::PADS {
                    let at = k * kit::REC;
                    let name = kit::name(&g.raw, k);
                    if ui.selectable_label(k == pad, format!("{:>2}  {}", k + 1, if name.is_empty() { "..." } else { &name })).clicked() {
                        pad = k;
                    }
                    let mut key = g.p[at + 4].round().clamp(0.0, 127.0) as u8;
                    if ui.add(egui::DragValue::new(&mut key).range(0..=127).custom_formatter(|v, _| if v < 1.0 { "no key".into() } else { key_name(v as u8) })).hint("The key that strikes this pad").changed() {
                        g.p[at + 4] = key as f32;
                        changed = true;
                    }
                    for (i, label) in [(0usize, "vol "), (1, "pan ")] {
                        let mut v = g.p[at + i] * 100.0;
                        if ui.add(egui::DragValue::new(&mut v).range(0.0..=100.0).speed(0.5).prefix(label).max_decimals(0)).changed() {
                            g.p[at + i] = v / 100.0;
                            changed = true;
                        }
                    }
                    for (i, label, tip) in [(2usize, "M", "Mute"), (3, "S", "Solo")] {
                        let mut on = g.p[at + i] >= 0.5;
                        if ui.toggle_value(&mut on, label).hint(tip).changed() {
                            g.p[at + i] = if on { 1.0 } else { 0.0 };
                            changed = true;
                        }
                    }
                    let mut cut = kit::cut(&g.raw, k);
                    if ui.add(egui::DragValue::new(&mut cut).range(0..=16).prefix("cut ")).hint("Pads of one cut group stop each other. 0 is none.").changed() {
                        kit::set_cut(&mut g.raw, k, cut);
                        changed = true;
                    }
                    let mut off = kit::out_offset(&g.raw, k);
                    if ui.add(egui::DragValue::new(&mut off).range(-98..=98).prefix("track ")).hint("Mixer track offset. Kept in the file; pads are not sent to their own tracks yet.").changed() {
                        kit::set_out_offset(&mut g.raw, k, off);
                        changed = true;
                    }
                    ui.end_row();
                }
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Pad {}", pad + 1)).strong());
                let mut name = kit::name(&g.raw, pad);
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0)).changed() {
                    kit::set_name(&mut g.raw, pad, &name);
                    changed = true;
                }
                if ui.button("Load drum...").hint("Put a saved drum of your own on this pad").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Saved drum", &["dmpatch", "fst"]).pick_file() {
                        let bytes = if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                            match cereal_engine::preset::read(&p).ok().and_then(|f| f.unit()) {
                                Some(cereal_engine::preset::Unit::Gen(d)) => d.raw,
                                _ => Vec::new(),
                            }
                        } else {
                            std::fs::read(&p).unwrap_or_default()
                        };
                        let mut raw = std::mem::take(&mut g.raw);
                        changed |= kit::load_pad(&mut g.p, &mut raw, pad, &bytes);
                        g.raw = raw;
                    }
                }
            });
            let drum = &cereal_engine::membrane::params()[..38];
            changed |= float_panel_at(ui, ci, g, &cereal_engine::membrane::GROUPS[..8], drum, pad * kit::REC + kit::DRUM);
            ui.add_space(3.0);
            ui.label(RichText::new("Step patterns").small().color(TEXT_DIM));
            ui.horizontal(|ui| {
                let mut n = pat + 1;
                if ui.add(egui::DragValue::new(&mut n).range(1..=kit::PATTERNS).prefix("Pattern ")).changed() {
                    pat = n - 1;
                }
                let used = kit::pat_len(&g.raw, pat);
                let mut len = used.unwrap_or(16);
                if ui.add(egui::DragValue::new(&mut len).range(1..=kit::STEPS).prefix("Steps ")).changed() || (used.is_none() && ui.button("Use").clicked()) {
                    if used.is_none() {
                        kit::reset_pattern(&mut g.raw, pat);
                    }
                    kit::set_pat_len(&mut g.raw, pat, len);
                    changed = true;
                }
                let mut key = kit::pat_key(&g.raw, pat).unwrap_or(127);
                if ui.add(egui::DragValue::new(&mut key).range(0..=127).custom_formatter(|v, _| if v >= 127.0 { "no key".into() } else { key_name(v as u8) })).hint("Hold this key to run the pattern").changed() {
                    kit::set_pat_key(&mut g.raw, pat, (key < 127).then_some(key));
                    changed = true;
                }
                if ui.button("Clear").clicked() {
                    kit::reset_pattern(&mut g.raw, pat);
                    changed = true;
                }
                if ui.button("Load pattern...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Saved pattern", &["dmptrn"]).pick_file() {
                        changed |= kit::import_pattern(&mut g.raw, pat, &std::fs::read(&p).unwrap_or_default());
                    }
                }
            });
            if let Some(len) = kit::pat_len(&g.raw, pat) {
                egui::ScrollArea::horizontal().id_salt(("kit_steps", ci)).show(ui, |ui| {
                    for k in 0..kit::PADS {
                        if g.p[k * kit::REC + 4] < 1.0 && (0..len).all(|i| !kit::step(&g.raw, pat, k, i).0) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            ui.add_sized([86.0, 16.0], egui::Label::new(RichText::new(kit::name(&g.raw, k)).small()).truncate());
                            for i in 0..len {
                                if i > 0 && i % 4 == 0 {
                                    ui.add_space(4.0);
                                }
                                let (on, hard) = kit::step(&g.raw, pat, k, i);
                                let r = ui.add_sized([15.0, 16.0], egui::SelectableLabel::new(on, "")).hint(format!("Step {}: strength {hard}. Scroll over it to change the strength.", i + 1));
                                if r.clicked() {
                                    kit::set_step(&mut g.raw, pat, k, i, !on, hard);
                                    changed = true;
                                }
                                if r.hovered() {
                                    let turn = ui.input(|inp| inp.raw_scroll_delta.y);
                                    if turn != 0.0 {
                                        let h = (hard as f32 + turn.signum() * 4.0).clamp(1.0, 127.0) as u8;
                                        kit::set_step(&mut g.raw, pat, k, i, on, h);
                                        changed = true;
                                    }
                                }
                            }
                        });
                    }
                });
            } else {
                ui.label(RichText::new("This pattern is not in use.").small().color(TEXT_DIM));
            }
            ui.data_mut(|d| {
                d.insert_temp(pick_id, pad);
                d.insert_temp(pat_id, pat);
            });
            ui.label(RichText::new("A pad sounds when its key is played; holding the key of a pattern runs its steps in time with the song. The tone bands and the ceiling work on each held key by itself, not on the whole kit together.").small().color(TEXT_DIM));
        }
        Kind::KickD => {
            use cereal_engine::kickd::{params, sample_list, sample_name, sample_path, set_sample_path, COUNT, GROUPS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default drum plays.").color(TEXT_DIM));
                return false;
            }
            ui.horizontal(|ui| {
                let now = sample_name(&g.raw);
                let shown = if now.is_empty() { "None".to_string() } else { now.clone() };
                egui::ComboBox::from_id_salt(("kickd_sample", ci)).selected_text(format!("Layered sample: {shown}")).show_ui(ui, |ui| {
                    if ui.selectable_label(now.is_empty(), "None").clicked() && !now.is_empty() {
                        set_sample_path(&mut g.raw, "");
                        changed = true;
                    }
                    for (name, path) in sample_list() {
                        if ui.selectable_label(name.eq_ignore_ascii_case(&now), &name).clicked() && path != sample_path(&g.raw) {
                            set_sample_path(&mut g.raw, &path);
                            changed = true;
                        }
                    }
                });
                if ui.button("Load...").hint("Pick a sample file of your own for the layer").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aiff", "wv"]).pick_file() {
                        set_sample_path(&mut g.raw, &p.to_string_lossy());
                        changed = true;
                    }
                }
            });
            changed |= float_panel(ui, ci, g, &GROUPS, params());
            ui.label(RichText::new("The drum sounds for its duration whatever the length of the note. The layered samples in the list come from your own imported content.").small().color(TEXT_DIM));
        }
        Kind::Morph => {
            use cereal_engine::morph::{params, COUNT, GROUPS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and nothing plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, &GROUPS, params());
            ui.label(RichText::new("The overtone snapshots, the keyboard zones and the path between the four generators play as stored and are kept in the file; their drawing pages are not built yet. The noise layer, the four extra envelopes and the table of links are kept but not worked.").small().color(TEXT_DIM));
        }
        Kind::Hybrid => {
            use cereal_engine::hybrid::{params, COUNT, GROUPS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default sound plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, &GROUPS, params());
            ui.label(RichText::new("Only the sine, the saw and the squares are placed in the list of built shapes; the other places play stand-in shapes of our own. The step runner, the two effect units and the links to outside controllers are kept in the file but not worked yet.").small().color(TEXT_DIM));
        }
        Kind::Triple => {
            use cereal_engine::triple::{params, set_wave_name, wave_name, wave_names, COUNT, GROUPS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default sound plays.").color(TEXT_DIM));
                return false;
            }
            ui.horizontal(|ui| {
                let names = wave_names();
                for k in 0..3 {
                    let now = wave_name(&g.raw, k);
                    egui::ComboBox::from_id_salt(("triple_wave", ci, k)).selected_text(format!("Wave {}: {now}", k + 1)).show_ui(ui, |ui| {
                        for name in &names {
                            if ui.selectable_label(name.eq_ignore_ascii_case(&now), name).clicked() && !name.eq_ignore_ascii_case(&now) {
                                set_wave_name(&mut g.raw, k, name);
                                changed = true;
                            }
                        }
                    });
                }
            });
            changed |= float_panel(ui, ci, g, &GROUPS, params());
            ui.label(RichText::new("Waves beyond the four built ones come from your own imported content. The slow oscillators of the single effects and the wheel and key-strength links are kept in the file but not worked yet.").small().color(TEXT_DIM));
        }
        Kind::StrModel => {
            use cereal_engine::strmodel::{params, COUNT, GROUPS};
            if g.p.len() < COUNT {
                ui.label(RichText::new("The stored settings could not be read. They are kept untouched and the default sound plays.").color(TEXT_DIM));
                return false;
            }
            changed |= float_panel(ui, ci, g, &GROUPS, params());
            ui.label(RichText::new("The wheels and key pressure are not worked yet, so links from them rest. A few stored settings are not placed yet: they are kept in the file as they are.").small().color(TEXT_DIM));
        }
        Kind::Sub => {
            match cereal_engine::host::block_name(&g.raw) {
                Some(n) => ui.label(RichText::new(format!("The plug-in \"{n}\" is not installed on this machine. A plain voice of ours plays its notes; its settings are kept untouched and it comes back once the plug-in is installed. Settings > Audio switches the stand-in voice off.")).color(TEXT_DIM)),
                None => ui.label(RichText::new("Plays a stand-in voice of our own, shaped by the few stored settings we can read so far. Its block is kept untouched.").color(TEXT_DIM)),
            };
        }
        Kind::Metal | Kind::MetalBank => {
            ui.label(RichText::new("Plays from its stored settings: seeds, fullness, decay, release, pitch and level. Editing pages are still to come.").color(TEXT_DIM));
        }
        _ => {
            // the small instruments: one knob per stored setting
            let d = def(g.kind);
            for row in (0..d.params.len()).collect::<Vec<_>>().chunks(8) {
                ui.horizontal(|ui| {
                    for &idx in row {
                        let p = &d.params[idx];
                        let mut v = g.p[idx];
                        let span = (p.max - p.min) as f32;
                        let label = p.name.to_uppercase();
                        let names = (g.kind == Kind::Guitar).then(|| cereal_engine::guitar::CHOICES.iter().find(|c| c.0 == idx).map(|c| c.1)).flatten();
                        if let Some(names) = names {
                            // a choice, not an amount: a list of names
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&label).small().color(TEXT_DIM));
                                let cur = (v as usize).min(names.len() - 1);
                                egui::ComboBox::from_id_salt(("guitar_choice", idx)).width(120.0).selected_text(names[cur]).show_ui(ui, |ui| {
                                    for (k, n) in names.iter().enumerate() {
                                        if ui.selectable_label(k == cur, *n).clicked() {
                                            g.p[idx] = k as f32;
                                            changed = true;
                                        }
                                    }
                                });
                            });
                            continue;
                        }
                        if cap_knob_for(ui, &label, &mut v, p.min as f32, p.max as f32, p.def as f32, |v| if span > 1000.0 { format!("{:.0} %", (v - p.min as f32) / span * 100.0) } else { format!("{v:.0}") }, Some(cereal_engine::song::Target::ChanParam(ci, idx as u16))) {
                            g.p[idx] = v.round();
                            changed = true;
                        }
                    }
                });
                ui.add_space(4.0);
            }
        }
    }
    changed
}

/// The six-operator instrument: its matrix, operators, filters and drawn lines, all edited in the stored block.
fn fm_panel(ui: &mut Ui, g: &mut cereal_engine::synth::Gen) -> bool {
    use cereal_engine::fm::{Patch, FILTER_TARGETS, MODULES, OP_TARGETS};
    let (Some(p), Some(d)) = (g.fm.clone(), Patch::unpack(&g.raw)) else {
        ui.label(RichText::new("The stored settings could not be read. They are kept untouched and play as far as they were understood.").color(TEXT_DIM));
        return false;
    };
    let int = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    let mut sets: Vec<(usize, i32)> = Vec::new();
    let mut bytes: Vec<(usize, u8)> = Vec::new();
    let mut floats: Vec<(usize, f32)> = Vec::new();
    let mut line: Option<cereal_engine::fm::Curve> = None;
    // one number: shown scaled, stored as the whole number the block holds
    let num = |ui: &mut Ui, sets: &mut Vec<(usize, i32)>, o: usize, lo: i32, hi: i32, scale: f32, dec: usize, label: &str, unit: &str| {
        let mut v = int(o).clamp(lo, hi) as f32 * scale;
        let speed = ((hi - lo) as f32 * scale / 200.0).max(scale);
        if ui.add(egui::DragValue::new(&mut v).range(lo as f32 * scale..=hi as f32 * scale).speed(speed).fixed_decimals(dec).prefix(format!("{label} ")).suffix(unit)).changed() {
            sets.push((o, (v / scale).round() as i32));
        }
    };
    let page_id = ui.id().with("fmpage");
    let mut page: usize = ui.data(|d| d.get_temp(page_id)).unwrap_or(0);
    let used = p.used();
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut page, 0, "Main");
        for (k, name) in MODULES.iter().enumerate() {
            let heard = k >= 6 || used[k];
            ui.selectable_value(&mut page, 1 + k, RichText::new(*name).color(if heard { TEXT } else { TEXT_DIM }));
        }
    });
    ui.data_mut(|d| d.insert_temp(page_id, page));
    ui.separator();
    // which drawn lines belong to this page, and what they steer
    let (first_art, targets): (usize, &[&str]) = match page {
        0 => (60, &["Pan", "Volume"]),
        k @ 1..=6 => ((k - 1) * 6, &OP_TARGETS),
        k => (36 + (k - 7) * 8, &FILTER_TARGETS),
    };
    match page {
        0 => {
            ui.horizontal(|ui| {
                num(ui, &mut sets, 0, 0, 256, 100.0 / 128.0, 0, "Volume", " %");
                num(ui, &mut sets, 8, -48, 48, 1.0, 0, "Pitch", " st");
                num(ui, &mut sets, 4, -128, 128, 100.0 / 128.0, 0, "Oscillator amount", " %");
                num(ui, &mut sets, 728, 0, 1024, 100.0 / 1024.0, 0, "X", " %");
                num(ui, &mut sets, 732, 0, 1024, 100.0 / 1024.0, 0, "Y", " %");
            });
            ui.horizontal(|ui| {
                num(ui, &mut sets, 692, 1, 9, 1.0, 0, "Unison voices", "");
                num(ui, &mut sets, 696, 0, 128, 100.0 / 128.0, 0, "spread", " %");
                num(ui, &mut sets, 700, 0, 128, 100.0 / 128.0, 0, "level", " %");
                num(ui, &mut sets, 704, 0, 256, 100.0 / 256.0, 0, "detune", " %");
                num(ui, &mut sets, 712, 0, 128, 100.0 / 128.0, 0, "phase", " %");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Envelope lengths, volume").color(TEXT_DIM));
                num(ui, &mut sets, 660, 0, 256, 1.0, 0, "A", "");
                num(ui, &mut sets, 664, 0, 256, 1.0, 0, "D", "");
                num(ui, &mut sets, 672, 0, 256, 1.0, 0, "R", "");
                ui.label(RichText::new("filter").color(TEXT_DIM));
                num(ui, &mut sets, 676, 0, 256, 1.0, 0, "A", "");
                num(ui, &mut sets, 680, 0, 256, 1.0, 0, "D", "");
                num(ui, &mut sets, 688, 0, 256, 1.0, 0, "R", "");
            });
            ui.add_space(4.0);
            let ring_id = ui.id().with("fmring");
            let mut ring: bool = ui.data(|d| d.get_temp(ring_id)).unwrap_or(false);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Matrix (percent): each row takes from the operators in the columns").strong());
                ui.selectable_value(&mut ring, false, "Frequency");
                ui.selectable_value(&mut ring, true, "Ring");
            });
            ui.data_mut(|d| d.insert_temp(ring_id, ring));
            egui::Grid::new("fm_matrix").striped(true).min_col_width(38.0).spacing([2.0, 2.0]).show(ui, |ui| {
                ui.label("");
                for k in 0..6 {
                    ui.label(RichText::new(format!("{}", k + 1)).strong());
                }
                if !ring {
                    for h in ["Pan", "Send", "Out"] {
                        ui.label(RichText::new(h).strong());
                    }
                }
                ui.end_row();
                let (base, rows, cols) = if ring { (336usize, 6usize, 6usize) } else { (12, 9, 9) };
                for r in 0..rows {
                    ui.label(RichText::new(MODULES[r]).strong());
                    for c in 0..cols {
                        let o = base + r * 36 + c * 4;
                        let mut v = (int(o).clamp(-256, 256) as f32 / 2.56).round();
                        // every knob of the main grid has a mute switch of its own, kept beside the grid
                        let mute_at = (!ring).then_some(800 + r * 9 + c);
                        let muted = mute_at.is_some_and(|m| d.get(m).copied().unwrap_or(0) != 0);
                        let shown = egui::DragValue::new(&mut v).range(-100.0..=100.0).speed(0.5).custom_formatter(|v, _| if v == 0.0 { "·".to_string() } else { format!("{v:.0}") });
                        let resp = ui.add_enabled_ui(true, |ui| {
                            if muted {
                                ui.visuals_mut().override_text_color = Some(TEXT_DIM.gamma_multiply(0.5));
                            }
                            ui.add(shown)
                        })
                        .inner;
                        if resp.changed() {
                            sets.push((o, (v * 2.56).round() as i32));
                        }
                        if resp.secondary_clicked() {
                            match mute_at {
                                Some(m) => bytes.push((m, !muted as u8)),
                                None => sets.push((o, 0)),
                            }
                        }
                    }
                    ui.end_row();
                }
            });
            ui.label(RichText::new("Drag a cell to set it, right-click to mute it or bring it back. A row's own column is its feedback.").small().color(TEXT_DIM));
        }
        k @ 1..=6 => {
            let at = p.op_at[k - 1];
            let lay = p.lay;
            let tail = lay.tail;
            ui.horizontal(|ui| {
                num(ui, &mut sets, at + 40, 0, 640_000, 1.0 / 20000.0, 4, "Ratio", "");
                num(ui, &mut sets, at + 44, -2_000_000, 2_000_000, 0.01, 2, "Offset", " Hz");
                num(ui, &mut sets, at, 0, 256, 100.0 / 256.0, 0, "Phase", " %");
                num(ui, &mut sets, at + 4, -256, 512, 100.0 / 256.0, 0, "Level", " %");
            });
            ui.horizontal(|ui| {
                num(ui, &mut sets, at + 8, 0, 9600, 1.0, 0, "Pitch range", " cents");
                num(ui, &mut sets, at + lay.noise, 0, 128, 100.0 / 128.0, 0, "Noise", " %");
                if let Some(o) = lay.pluck {
                    let mut pluck = d.get(at + o).copied().unwrap_or(0) != 0;
                    if ui.checkbox(&mut pluck, "Plucked string").changed() {
                        bytes.push((at + o, pluck as u8));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Wave").color(TEXT_DIM));
                num(ui, &mut sets, at + tail + 0, 0, 512, 100.0 / 256.0, 0, "Shape", " %");
                num(ui, &mut sets, at + tail + 4, -128, 128, 100.0 / 128.0, 0, "Tension", " %");
                num(ui, &mut sets, at + tail + 8, 0, 128, 100.0 / 128.0, 0, "Skew", " %");
                num(ui, &mut sets, at + tail + 12, 0, 128, 100.0 / 128.0, 0, "Sine shaper", " %");
                num(ui, &mut sets, at + tail + 16, 0, 128, 100.0 / 128.0, 0, "Brightness", " %");
            });
            ui.horizontal(|ui| {
                for (o, name) in [(Some(lay.flags), "Half wave"), (Some(lay.flags + 2), "Fold up"), (lay.centre, "Centre")] {
                    let Some(o) = o else { continue };
                    let mut on = d.get(at + o).copied().unwrap_or(0) != 0;
                    if ui.checkbox(&mut on, name).changed() {
                        bytes.push((at + o, on as u8));
                    }
                }
            });
            // the wave itself, then the overtones it is built from
            let fl = |o: usize| d.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|v| v.is_finite()).unwrap_or(0.0);
            let shown = lay.harmonics.min(64);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(shown as f32 * 7.0, 70.0), Sense::click_and_drag());
            let pt = ui.painter_at(rect);
            pt.rect_filled(rect, 2.0, PANEL_DARK);
            for h in 0..shown {
                let m = fl(at + lay.mags + h * 4).clamp(0.0, 1.0);
                let x = rect.left() + h as f32 * 7.0;
                pt.rect_filled(egui::Rect::from_min_max(Pos2::new(x + 1.0, rect.bottom() - m * rect.height()), Pos2::new(x + 6.0, rect.bottom())), 0.0, if h % 2 == 0 { ACCENT } else { ACCENT.gamma_multiply(0.75) });
            }
            if resp.dragged() || resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let h = (((pos.x - rect.left()) / 7.0) as usize).min(shown - 1);
                    let m = if ui.input(|i| i.pointer.secondary_down()) { 0.0 } else { ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0) };
                    floats.push((at + lay.mags + h * 4, m));
                }
            }
            ui.label(RichText::new("Overtones, the first on the left: drag to set how strong each is, drag with the right button to silence.").small().color(TEXT_DIM));
        }
        k => {
            let at = p.filt_at[k - 7];
            const KINDS: [&str; 13] = ["Low / band / high mix", "Low-pass 1", "Low-pass 2", "Low-pass 3", "Band-pass", "Notch", "High-pass", "All-pass", "Low shelf", "Peak", "High shelf", "Driven low-pass", "Phaser"];
            const SLOPES: [&str; 6] = ["Off", "1 stage", "2 stages", "3 stages", "2 stages, alternative", "3 stages, alternative"];
            ui.horizontal(|ui| {
                let mut kind = if p.lay.first { 0 } else { int(at + 84).clamp(0, 12) as usize };
                ui.add_enabled_ui(!p.lay.first, |ui| egui::ComboBox::from_id_salt("fmfkind").selected_text(KINDS[kind]).show_ui(ui, |ui| {
                    for (i, n) in KINDS.iter().enumerate() {
                        if ui.selectable_value(&mut kind, i, *n).clicked() {
                            sets.push((at + 84, i as i32));
                        }
                    }
                }));
                let mut slope = int(at).clamp(0, 5) as usize;
                egui::ComboBox::from_id_salt("fmfslope").selected_text(SLOPES[slope]).show_ui(ui, |ui| {
                    for (i, n) in SLOPES.iter().enumerate() {
                        if ui.selectable_value(&mut slope, i, *n).clicked() {
                            sets.push((at, i as i32));
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                num(ui, &mut sets, at + 8, 0, 256, 100.0 / 256.0, 0, "Cutoff", " %");
                num(ui, &mut sets, at + 12, 0, 256, 100.0 / 256.0, 0, "Resonance", " %");
                num(ui, &mut sets, at + 4, 0, 256, 100.0 / 256.0, 0, "Envelope", " %");
                num(ui, &mut sets, at + 36, -256, 256, 100.0 / 256.0, 0, "To next filter", " %");
            });
            ui.horizontal(|ui| {
                num(ui, &mut sets, at + 16, 0, 128, 100.0 / 128.0, 0, "Low / amount", " %");
                num(ui, &mut sets, at + 20, 0, 128, 100.0 / 128.0, 0, "Band", " %");
                num(ui, &mut sets, at + 24, 0, 128, 100.0 / 128.0, 0, "High", " %");
            });
            ui.horizontal(|ui| {
                let mut on = if p.lay.first { d.get(at + 80).copied().unwrap_or(0) != 0 } else { int(at + 80) != 0 };
                if ui.checkbox(&mut on, "Shaper").changed() {
                    if p.lay.first {
                        bytes.push((at + 80, on as u8));
                    } else {
                        sets.push((at + 80, on as i32));
                    }
                }
                num(ui, &mut sets, at + 28, 0, 256, 1.0, 0, "drive", "");
                num(ui, &mut sets, at + 32, 0, 128, 100.0 / 128.0, 0, "mix", " %");
            });
        }
    }
    ui.separator();
    // the drawn lines of this page
    let tgt_id = ui.id().with(("fmtarget", page));
    let mut tgt: usize = ui.data(|d| d.get_temp(tgt_id)).unwrap_or(1).min(targets.len() - 1);
    let which_id = ui.id().with("fmline");
    let mut which: usize = ui.data(|d| d.get_temp(which_id)).unwrap_or(0).min(7);
    ui.horizontal_wrapped(|ui| {
        ui.label("Lines for");
        for (k, name) in targets.iter().enumerate() {
            let live = p.arts.get(first_art + k).is_some_and(|a| a.env.on() || a.lfo.on());
            ui.selectable_value(&mut tgt, k, RichText::new(*name).color(if live { TEXT } else { TEXT_DIM }));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut which, 0, "Envelope");
        ui.selectable_value(&mut which, 1, "Oscillator shape");
        for (k, name) in ["Key", "Velocity", "X", "Y", "Random", "Unison"].iter().enumerate() {
            ui.selectable_value(&mut which, 2 + k, format!("{name} map"));
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(tgt_id, tgt);
        d.insert_temp(which_id, which);
    });
    if let Some(a) = p.arts.get(first_art + tgt) {
        let (mut c, kind) = match which {
            0 => (a.env.clone(), crate::curve::Line::Envelope),
            1 => (a.lfo.clone(), crate::curve::Line::Envelope),
            k => (a.maps[k - 2].clone(), crate::curve::Line::Mapping),
        };
        let mut touched = false;
        if which < 2 {
            let end = c.span.1;
            ui.horizontal(|ui| {
                let mut on = c.on();
                if ui.checkbox(&mut on, "On").changed() {
                    c.live = on;
                    touched = true;
                }
                if which == 0 {
                    num(ui, &mut sets, end, 0, 256, 1.0, 0, "Attack length", "");
                    num(ui, &mut sets, end + 4, 0, 256, 1.0, 0, "Decay length", "");
                    num(ui, &mut sets, end + 12, 0, 256, 1.0, 0, "Release length", "");
                } else {
                    num(ui, &mut sets, end, 0, 65536, 1.0, 0, "Speed", "");
                    ui.label(RichText::new(format!("{:.2} Hz", a.lfo_hz)).color(TEXT_DIM));
                }
            });
        }
        ui.push_id(("fmcurve", page, tgt, which), |ui| {
            touched |= crate::curve::curve_editor(ui, &mut c, kind, 170.0);
        });
        if touched && c.span.1 > 0 {
            line = Some(c);
        }
        ui.label(RichText::new("Drag a point to move it, drag a ring to bend the stretch before it (double-click straightens it). Right-click empty space for a new point, right-click a point for its menu.").small().color(TEXT_DIM));
    }
    if sets.is_empty() && bytes.is_empty() && floats.is_empty() && line.is_none() {
        return false;
    }
    let done = Patch::edited(&g.raw, |d| {
        for (o, v) in sets {
            if let Some(b) = d.get_mut(o..o + 4) {
                b.copy_from_slice(&v.to_le_bytes());
            }
        }
        for (o, v) in floats {
            if let Some(b) = d.get_mut(o..o + 4) {
                b.copy_from_slice(&v.to_le_bytes());
            }
        }
        for (o, v) in bytes {
            if let Some(b) = d.get_mut(o) {
                *b = v;
            }
        }
        // the line goes in last: it can change the length of the block, and every other place was read before that
        if let Some(c) = line {
            c.store(d);
        }
    });
    match done {
        Some((raw, patch)) => {
            g.raw = raw;
            g.fm = Some(std::sync::Arc::new(patch));
            true
        }
        None => false,
    }
}
