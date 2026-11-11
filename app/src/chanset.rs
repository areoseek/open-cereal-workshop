//! The channel settings window: sample treatment, envelopes and filter, and the miscellaneous note functions of
//! the selected sampler channel.

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
    use cereal_engine::duo::Ctl;
    let mut changed = false;
    for (gi, title) in groups.iter().enumerate() {
        ui.label(RichText::new(*title).small().color(TEXT_DIM));
        ui.horizontal_wrapped(|ui| {
            for (i, info) in params.iter().enumerate().filter(|(_, p)| p.group as usize == gi) {
                let mut v = g.p[i];
                match info.ctl {
                    Ctl::Knob => {
                        if cap_knob(ui, &info.name.to_uppercase(), &mut v, 0.0, 1.0, info.def, |v| format!("{:.0} %", v * 100.0)) {
                            g.p[i] = v;
                            changed = true;
                        }
                    }
                    Ctl::Switch => {
                        let mut on = v >= 0.5;
                        if ui.toggle_value(&mut on, info.name).changed() {
                            g.p[i] = if on { 1.0 } else { 0.0 };
                            changed = true;
                        }
                    }
                    Ctl::Pick(names) => {
                        let now = cereal_engine::duo::pick(v, names.len());
                        egui::ComboBox::from_id_salt(("floats", ci, i)).selected_text(format!("{}: {}", info.name, names[now])).show_ui(ui, |ui| {
                            for (k, name) in names.iter().enumerate() {
                                if ui.selectable_label(k == now, *name).clicked() && k != now {
                                    g.p[i] = k as f32 / (names.len() - 1) as f32;
                                    changed = true;
                                }
                            }
                        });
                    }
                    Ctl::Whole(top) => {
                        let mut n = v.round().clamp(0.0, top as f32) as u32 + 1;
                        if ui.add(egui::DragValue::new(&mut n).range(1..=top + 1).prefix(format!("{} ", info.name))).changed() {
                            g.p[i] = (n - 1) as f32;
                            changed = true;
                        }
                    }
                    Ctl::Choice(names) => {
                        let now = (v.round().max(0.0) as usize).min(names.len() - 1);
                        egui::ComboBox::from_id_salt(("floats", ci, i)).selected_text(format!("{}: {}", info.name, names[now])).show_ui(ui, |ui| {
                            for (k, name) in names.iter().enumerate() {
                                if ui.selectable_label(k == now, *name).clicked() && k != now {
                                    g.p[i] = k as f32;
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
            let r = knob(ui, v, min, max, default, 26.0).on_hover_text(hint(*v));
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

fn draw_wave(ui: &mut Ui, frames: &[[f32; 2]], color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 70.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, PANEL_DARK);
    if frames.is_empty() {
        return;
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
                    _ => cereal_engine::synth::title_for(&c.plugin).map(str::to_string).unwrap_or_else(|| c.plugin.clone()),
                };
                ui.label(RichText::new(what).small().color(TEXT_DIM));
            });
        });
        ui.separator();
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
                        if ui.button("Load...").on_hover_text("Pick a sound bank file (.sf2)").clicked() {
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
                _ if c.zones.is_some() => {
                    // a pad machine or multi-sample player: one button per key that has sounds, its layers on hover
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
                            if b.on_hover_text(format!("{}\n{}", key_name(*k), tip)).clicked() {
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
                        draw_wave(ui, &s.frames, rgb(c.color).lerp_to_gamma(Color32::WHITE, 0.4));
                    } else {
                        ui.label(RichText::new("The loop file was not found.").small().color(Color32::from_rgb(255, 120, 110)));
                    }
                    let mut hit = None;
                    ui.horizontal_wrapped(|ui| {
                        for i in 0..sl.cuts.len() {
                            let key = if sl.cuts[i].key >= 0 { sl.cuts[i].key } else { sl.base + i as i32 };
                            if ui.small_button(format!("{}", i + 1)).on_hover_text(key_name(key.clamp(0, 127) as u8)).clicked() {
                                hit = Some(key.clamp(0, 127) as u8);
                            }
                        }
                    });
                    if let Some(k) = hit {
                        self.handle.send(cereal_engine::player::Cmd::NoteOn { chan: ci as u16, key: k, vel: 1.0 });
                        self.handle.send(cereal_engine::player::Cmd::NoteOff { chan: ci as u16, key: k });
                    }
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
                } else if ident.eq_ignore_ascii_case(cereal_engine::synth::KEY_SENDER) {
                    changed |= key_sender_panel(ui, &mut self.song.channels[ci].gen.as_mut().unwrap().raw);
                } else {
                    changed |= instrument_panel(ui, ci, self.song.channels[ci].gen.as_mut().unwrap());
                }
            }
            Tab::Sample => {
                let mut pick = false;
                {
                    let c = &mut self.song.channels[ci];
                    ui.horizontal(|ui| {
                        if ui.button("Open...").on_hover_text("Load a sample into this channel").clicked() {
                            pick = true;
                        }
                        let file = c.sample_path.rsplit(['/', '\\']).next().unwrap_or("").to_string();
                        let missing = c.sample.is_none() && !c.sample_path.is_empty();
                        ui.label(RichText::new(if file.is_empty() { "(no sample)".to_string() } else { file }).color(if missing { Color32::from_rgb(255, 120, 110) } else { TEXT })).on_hover_text(c.sample_path.clone());
                    });
                    if let Some(s) = &c.sample {
                        draw_wave(ui, &s.frames, rgb(c.color).lerp_to_gamma(Color32::WHITE, 0.4));
                        let len = s.frames.len() as f32 / s.rate.max(1) as f32;
                        ui.label(RichText::new(format!("{} Hz, {}, {}", s.rate, if s.mono { "mono" } else { "stereo" }, secs(len))).small().color(TEXT_DIM));
                    }
                    let f = &mut c.smp;
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
                            changed |= ui.add(egui::DragValue::new(&mut f.stretch_beats).range(0.0..=1024.0).speed(0.05).custom_formatter(|v, _| if v <= 0.0 { "none".into() } else { format!("{v:.2} beats") })).on_hover_text("Length the sample is fitted to; none keeps its own length").changed();
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
                    if led(ui, on, 18.0).on_hover_text("Envelope on / off").clicked() {
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
                        changed |= ui.add(egui::DragValue::new(&mut key).range(0..=131).speed(0.1).custom_formatter(|v, _| key_name(v as u8))).on_hover_text("Root note").changed();
                        ui.label(RichText::new("ROOT").small().color(TEXT_DIM));
                    });
                    changed |= cap_knob(ui, "SWING", &mut f.swing, 0.0, 1.0, 1.0, |v: f32| format!("Swing mix {:.0} %", v * 100.0));
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
                section(ui, "ECHO DELAY / FAT MODE");
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
        if rebake {
            self.song.channels[ci].bake();
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
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
                        if resp.on_hover_text(*name).clicked() && !on {
                            g.p[base + 1] = i as f32;
                            changed = true;
                        }
                    }
                    let inv = &mut g.p[21 + k];
                    if ui.selectable_label(*inv != 0.0, "INV").on_hover_text("Invert this oscillator").clicked() {
                        *inv = if *inv != 0.0 { 0.0 } else { 1.0 };
                        changed = true;
                    }
                    if k == 2 {
                        let am = &mut g.p[24];
                        if ui.selectable_label(*am != 0.0, "AM").on_hover_text("Oscillator 3 shapes the level of the other two").clicked() {
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
            match &g.img {
                Some(im) => {
                    ui.label(RichText::new(format!("Picture {} x {}, {:.1} s a pass, {}", im.w, im.h, im.seconds(), if im.looped { "looped" } else { "played once" })).strong());
                    ui.label(RichText::new("Red plays on the left, green on the right; each row is one sine wave above the played key. Loading and pasting pictures is still to come.").color(TEXT_DIM));
                }
                None => {
                    ui.label(RichText::new("The stored picture could not be read. The block is kept untouched.").color(TEXT_DIM));
                }
            }
        }
        Kind::Grain => {
            let s = cereal_engine::grain::Set::parse(&g.raw);
            ui.label(RichText::new(format!("Grains hold {:.0} ms, fade {:.0} ms, grain spacing {:.0} %, wave spacing {:.0} %", s.hold * 1000.0, s.attack * 1000.0, s.grain_spacing * 100.0, s.wave_spacing * 100.0)).strong());
            ui.label(RichText::new("Plays the channel's sample as grains from the stored settings. Pan and depth effects, random start, transients and the knobs are still to come.").color(TEXT_DIM));
        }
        Kind::Travel => {
            ui.label(RichText::new(cereal_engine::synth::sample_named_in("Wave Traveller", &g.raw).unwrap_or_else(|| "(no sample)".into())).strong());
            ui.label(RichText::new("Plays its sample straight through for now. The travel paths drawn per key are kept in the block but are not read yet.").color(TEXT_DIM));
        }
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
                if ui.button("Load...").on_hover_text("Pick a sample file of your own for the layer").clicked() {
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
        Kind::Sub | Kind::Thump => {
            ui.label(RichText::new("Plays a stand-in voice of our own, shaped by the few stored settings we can read so far. Its block is kept untouched.").color(TEXT_DIM));
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
                        let shown = egui::DragValue::new(&mut v).range(-100.0..=100.0).speed(0.5).custom_formatter(|v, _| if v == 0.0 { "·".to_string() } else { format!("{v:.0}") });
                        let resp = ui.add(shown);
                        if resp.changed() {
                            sets.push((o, (v * 2.56).round() as i32));
                        }
                        if resp.secondary_clicked() {
                            sets.push((o, 0));
                        }
                    }
                    ui.end_row();
                }
            });
            ui.label(RichText::new("Drag a cell to set it, right-click to clear it. A row's own column is its feedback.").small().color(TEXT_DIM));
        }
        k @ 1..=6 => {
            let at = p.op_at[k - 1];
            ui.horizontal(|ui| {
                num(ui, &mut sets, at + 40, 0, 640_000, 1.0 / 20000.0, 4, "Ratio", "");
                num(ui, &mut sets, at + 44, -2_000_000, 2_000_000, 0.01, 2, "Offset", " Hz");
                num(ui, &mut sets, at, 0, 256, 100.0 / 256.0, 0, "Phase", " %");
                num(ui, &mut sets, at + 4, -256, 512, 100.0 / 256.0, 0, "Level", " %");
            });
            ui.horizontal(|ui| {
                num(ui, &mut sets, at + 8, 0, 9600, 1.0, 0, "Pitch range", " cents");
                num(ui, &mut sets, at + 48, 0, 128, 100.0 / 128.0, 0, "Noise", " %");
                let mut pluck = d.get(at + 2126).copied().unwrap_or(0) != 0;
                if ui.checkbox(&mut pluck, "Plucked string").changed() {
                    bytes.push((at + 2126, pluck as u8));
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Wave").color(TEXT_DIM));
                num(ui, &mut sets, at + 2100, 0, 512, 100.0 / 256.0, 0, "Shape", " %");
                num(ui, &mut sets, at + 2104, -128, 128, 100.0 / 128.0, 0, "Tension", " %");
                num(ui, &mut sets, at + 2108, 0, 128, 100.0 / 128.0, 0, "Skew", " %");
                num(ui, &mut sets, at + 2112, 0, 128, 100.0 / 128.0, 0, "Sine shaper", " %");
                num(ui, &mut sets, at + 2116, 0, 128, 100.0 / 128.0, 0, "Brightness", " %");
            });
            ui.horizontal(|ui| {
                for (o, name) in [(2120usize, "Half wave"), (2122, "Fold up"), (2123, "Centre")] {
                    let mut on = d.get(at + o).copied().unwrap_or(0) != 0;
                    if ui.checkbox(&mut on, name).changed() {
                        bytes.push((at + o, on as u8));
                    }
                }
            });
            // the wave itself, then the overtones it is built from
            let fl = |o: usize| d.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|v| v.is_finite()).unwrap_or(0.0);
            const SHOWN: usize = 64;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(SHOWN as f32 * 7.0, 70.0), Sense::click_and_drag());
            let pt = ui.painter_at(rect);
            pt.rect_filled(rect, 2.0, PANEL_DARK);
            for h in 0..SHOWN {
                let m = fl(at + 52 + h * 4).clamp(0.0, 1.0);
                let x = rect.left() + h as f32 * 7.0;
                pt.rect_filled(egui::Rect::from_min_max(Pos2::new(x + 1.0, rect.bottom() - m * rect.height()), Pos2::new(x + 6.0, rect.bottom())), 0.0, if h % 2 == 0 { ACCENT } else { ACCENT.gamma_multiply(0.75) });
            }
            if resp.dragged() || resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let h = (((pos.x - rect.left()) / 7.0) as usize).min(SHOWN - 1);
                    let m = if ui.input(|i| i.pointer.secondary_down()) { 0.0 } else { ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0) };
                    floats.push((at + 52 + h * 4, m));
                }
            }
            ui.label(RichText::new("Overtones 1 to 64: drag to set how strong each is, drag with the right button to silence.").small().color(TEXT_DIM));
        }
        k => {
            let at = p.filt_at[k - 7];
            const KINDS: [&str; 13] = ["Low / band / high mix", "Low-pass 1", "Low-pass 2", "Low-pass 3", "Band-pass", "Notch", "High-pass", "All-pass", "Low shelf", "Peak", "High shelf", "Driven low-pass", "Phaser"];
            const SLOPES: [&str; 6] = ["Off", "1 stage", "2 stages", "3 stages", "2 stages, alternative", "3 stages, alternative"];
            ui.horizontal(|ui| {
                let mut kind = int(at + 84).clamp(0, 12) as usize;
                egui::ComboBox::from_id_salt("fmfkind").selected_text(KINDS[kind]).show_ui(ui, |ui| {
                    for (i, n) in KINDS.iter().enumerate() {
                        if ui.selectable_value(&mut kind, i, *n).clicked() {
                            sets.push((at + 84, i as i32));
                        }
                    }
                });
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
                let mut on = int(at + 80) != 0;
                if ui.checkbox(&mut on, "Shaper").changed() {
                    sets.push((at + 80, on as i32));
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
                    c.flags ^= 1;
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
