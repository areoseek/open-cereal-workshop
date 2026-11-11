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

/// A knob with a caption under it. Returns true when the value moved.
fn cap_knob(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, default: f32, hint: impl Fn(f32) -> String) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.set_width(44.0);
        ui.vertical_centered(|ui| {
            changed = knob(ui, v, min, max, default, 26.0).on_hover_text(hint(*v)).changed();
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
            changed |= cap_knob(ui, "PAN", &mut c.pan, -1.0, 1.0, 0.0, |v: f32| format!("{:+.0} %", v * 100.0));
            changed |= cap_knob(ui, "VOL", &mut c.volume, 0.0, 1.0, 10000.0 / 12800.0, |v: f32| format!("{:.0} %", v * 128.0));
            changed |= cap_knob(ui, "PITCH", &mut c.pitch_cents, -1200.0, 1200.0, 0.0, |v: f32| format!("{:+.0} cents", v));
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
                let ident = cereal_engine::synth::def(self.song.channels[ci].gen.as_ref().unwrap().kind).ident;
                if let Some(p) = self.preset_bar(ui, ident, true) {
                    self.actions.push(crate::Action::LoadPreset(p, crate::PresetTo::Chan(ci)));
                }
                ui.add_space(4.0);
                changed |= instrument_panel(ui, self.song.channels[ci].gen.as_mut().unwrap());
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
                    if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff"]).pick_file() {
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

/// The knobs of a built-in instrument.
fn instrument_panel(ui: &mut Ui, g: &mut cereal_engine::synth::Gen) -> bool {
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
                        if cap_knob(ui, label, &mut v, p.min as f32, p.max as f32, p.def as f32, hint) {
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
                        if cap_knob(ui, &cap, &mut v, p.min as f32, p.max as f32, p.def as f32, &|v| format!("{name}: {v:.0}")) {
                            g.p[i] = v.round();
                            changed = true;
                        }
                    }
                });
            }
        }
        Kind::Fm => {
            // shown as stored for now: the settings play, the editing pages are still to come
            use cereal_engine::fm::MODULES;
            let Some(p) = g.fm.clone() else { return false };
            let pct = |v: f32| if v == 0.0 { String::new() } else { format!("{:.0}", v * 100.0) };
            ui.label(RichText::new("Modulation matrix (percent)").strong());
            egui::Grid::new("fm_matrix").striped(true).min_col_width(34.0).show(ui, |ui| {
                ui.label("");
                for k in 0..6 {
                    ui.label(RichText::new(format!("{}", k + 1)).strong());
                }
                ui.label(RichText::new("Pan").strong());
                ui.label(RichText::new("Out").strong());
                ui.end_row();
                for r in 0..9 {
                    ui.label(RichText::new(MODULES[r]).strong());
                    for c in 0..6 {
                        ui.label(pct(if r < 6 { p.fm[r][c] } else { p.feed[r - 6][c] }));
                    }
                    ui.label(pct(p.pan[r]));
                    ui.label(pct(p.out[r]));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Operators").strong());
            let used = p.used();
            egui::Grid::new("fm_ops").striped(true).show(ui, |ui| {
                for h in ["", "Ratio", "Offset Hz", "Level", "Noise", "Plucked", "Heard"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (k, o) in p.ops.iter().enumerate() {
                    ui.label(MODULES[k]);
                    ui.label(format!("{:.4}", o.ratio));
                    ui.label(format!("{:.2}", o.offset_hz));
                    ui.label(format!("{:.0} %", o.vol * 100.0));
                    ui.label(format!("{:.0} %", o.noise * 100.0));
                    ui.label(if o.pluck { "yes" } else { "" });
                    ui.label(if used[k] { "yes" } else { "" });
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Filters").strong());
            egui::Grid::new("fm_filters").striped(true).show(ui, |ui| {
                for h in ["", "Stages", "Type", "Cutoff", "Resonance", "Envelope", "To next", "Shaper"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (k, f) in p.filts.iter().enumerate() {
                    ui.label(MODULES[6 + k]);
                    ui.label(if f.slope == 0 { "off".to_string() } else { format!("{}", f.slope) });
                    ui.label(format!("{}", f.kind));
                    ui.label(format!("{:.0} %", f.cut * 100.0));
                    ui.label(format!("{:.0} %", f.res * 100.0));
                    ui.label(format!("{:.0} %", f.env_amt * 100.0));
                    ui.label(pct(f.next));
                    ui.label(if f.ws_on { "on" } else { "" });
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(format!("Volume {:.0} %    Pitch {:+.0} st    Unison {}", p.volume * 100.0, p.pitch, p.uni));
        }
        Kind::Packs => {
            let (pack, preset) = g.pack_and_preset();
            ui.label(RichText::new(format!("{preset}  ({pack})")).strong());
            ui.label(RichText::new("Pack sounds are protected files and are not read. A stand-in voice chosen by the preset's name plays instead.").color(TEXT_DIM));
        }
        Kind::DrumBank => {
            ui.label(RichText::new("One drum patch per key, played from the stored settings. The recorded drum waves are replaced by noise. The patch editor is still to come.").color(TEXT_DIM));
            let names = cereal_engine::drumbank::names(&g.raw);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for (k, n) in names.iter().enumerate() {
                    ui.label(format!("{}{}  {n}", ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][k % 12], k / 12));
                }
            });
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
                        if cap_knob(ui, &label, &mut v, p.min as f32, p.max as f32, p.def as f32, |v| if span > 1000.0 { format!("{:.0} %", (v - p.min as f32) / span * 100.0) } else { format!("{v:.0}") }) {
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
