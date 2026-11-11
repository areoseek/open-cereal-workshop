//! The editor window of an effect slot: one knob per setting, laid out from the unit's table.

use cereal_engine::fx::{self, Kind};
use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use crate::widgets::*;
use crate::App;

const EQ_TYPES: [&str; 8] = ["Off", "Low pass", "Band pass", "High pass", "Notch", "Low shelf", "Peaking", "High shelf"];

fn hz(x: f32) -> f32 {
    20.0 * 1000f32.powf(x.clamp(0.0, 1.0))
}

/// The value of a setting in words, for the hint under the pointer.
fn hint(kind: Kind, i: usize, name: &str, v: f32) -> String {
    let pct = |x: f32| format!("{name}: {:.0} %", x * 100.0);
    match (kind, name) {
        (Kind::Eq, n) if n.starts_with("Level") || n == "Main level" => format!("{name}: {:+.1} dB", v / 100.0),
        (Kind::Eq, n) if n.starts_with("Freq") => format!("{name}: {:.0} Hz", hz(v / 65536.0)),
        (Kind::Eq, n) if n.starts_with("Width") => pct(v / 65536.0),
        (Kind::Eq, n) if n.starts_with("Type") => format!("{name}: {}", EQ_TYPES[(v as usize).min(7)]),
        (Kind::Balance | Kind::Enhancer, "Pan") | (Kind::Delay, "Input pan") => format!("{name}: {:+.0} %", v / 1.28),
        (Kind::Balance | Kind::Enhancer, "Volume") => pct(v / 256.0),
        (Kind::AutoPan, "Pan") => format!("Pan: {:+.0} %", (v - 128.0) / 1.28),
        (Kind::AutoPan, "Volume") => pct(v / 256.0),
        (Kind::AutoPan, "Amount") => format!("Amount: {:+.0} %", (v - 64.0) / 0.64),
        (Kind::AutoPan, "Shape") => format!("Shape: {}", ["sine", "triangle", "square"][(v as usize).min(2)]),
        (Kind::AutoPan, "Target") => format!("Target: {}", ["pan", "volume", "pan"][(v as usize).min(2)]),
        (Kind::Echo, "Time") => format!("Time: {:.2} steps", v / 48.0),
        (Kind::Echo, "Model") => format!("Model: {}", ["mono", "stereo", "ping pong"][(v as usize).min(2)]),
        (Kind::Echo, "Tempo sync" | "Keep pitch") => format!("{name}: {}", if v >= 0.5 { "on" } else { "off" }),
        (Kind::Echo, "Stereo offset") => format!("Stereo offset: {:+.0} %", (v - 3000.0) / 30.0),
        (Kind::Echo, "Tone") => format!("Tone: {:+.0} %", v / 60.0),
        (Kind::Echo, "Sample rate") => pct(v / 60000.0),
        (Kind::Echo, _) => pct(v / 6000.0),
        (Kind::Flanger, "Delay") => format!("Delay: {v:.0} ms"),
        (Kind::Flanger, "Phase") => format!("Phase: {v:.0} degrees"),
        (Kind::Flanger, "Depth" | "Rate") => pct(v / 1024.0),
        (Kind::Flanger, _) => format!("{name}: {:+.0} %", v / 1.28),
        (Kind::Chorus, n) if n.ends_with("rate") => format!("{name}: {:.2} Hz", v / 1000.0),
        (Kind::Chorus, n) if n.ends_with("wave") => format!("{name}: {}", if v >= 512.0 { "triangle" } else { "sine" }),
        (Kind::Chorus, "Wet only" | "Cross type") => format!("{name}: {}", if v >= 512.0 { "on" } else { "off" }),
        (Kind::Chorus, "Depth" | "Cross cutoff") => pct(v / 5000.0),
        (Kind::Chorus, _) => pct(v / 1024.0),
        (Kind::Spread, "Low bypass") => format!("Low bypass: {:.0} Hz", 10.0 * 1000f32.powf(v / 1_000_000.0)),
        (Kind::Spread, "Mono lows") => format!("Mono lows: {}", if v >= 0.5 { "on" } else { "off" }),
        (Kind::Spread, "Separation") => format!("Separation: {:+.0} %", v / 100.0),
        (Kind::Spread, _) => pct(v / 10000.0),
        (Kind::Transient, n) if n.ends_with("shape") => format!("{name}: {}", ["sharp", "medium", "soft"][(v as usize).min(2)]),
        (Kind::Transient, "Drive") => pct(v / 160.0),
        (Kind::Transient, "Split balance") => format!("Split balance: {:.0} % dry / {:.0} % wet", (v / 80.0).min(1.0) * 100.0, ((160.0 - v) / 80.0).min(1.0) * 100.0),
        (Kind::Transient, "Split frequency") => {
            let x = (v - 80.0) / 80.0;
            format!("Split frequency: {:.0} Hz", if x >= 0.0 { 260.0 * (1000.0f32 / 260.0).powf(x) } else { 260.0 * (5.0f32 / 260.0).powf(-x) })
        }
        (Kind::Transient, _) => format!("{name}: {:+.0} %", (v - 80.0) / 0.8),
        (Kind::Swarm, "Low pass" | "High pass") => format!("{name}: {v:.0} Hz"),
        (Kind::Swarm, "Eight lines") => format!("Delay lines: {}", if v >= 0.5 { 8 } else { 4 }),
        (Kind::Swarm, _) => pct(v / 1000.0),
        (Kind::Delay, "Time") => format!("Time: {:.2} steps", v / 48.0),
        (Kind::Delay, "Mode") => format!("Mode: {}", ["normal", "inverted stereo", "ping pong"][(v as usize).min(2)]),
        (Kind::Delay, _) => pct(v / 128.0),
        (Kind::Comp, "Threshold") => format!("Threshold: {:.1} dB", v / 10.0),
        (Kind::Comp, "Ratio") => format!("Ratio: {:.1}:1", v / 10.0),
        (Kind::Comp, "Gain") => format!("Gain: {:+.1} dB", v / 10.0),
        (Kind::Comp, "Attack") => format!("Attack: {:.1} ms", v / 10.0),
        (Kind::Comp, "Release") => format!("Release: {v:.0} ms"),
        (Kind::Comp, "Type") => format!("Type: {}", ["hard", "medium", "vintage", "soft", "hard R", "medium R", "vintage R", "soft R"][(v as usize).min(7)]),
        (Kind::Reverb, "Low cut") => format!("Low cut: {v:.0} Hz"),
        (Kind::Reverb, "High cut" | "High damping") => format!("{name}: {:.1} kHz", v / 10.0),
        (Kind::Reverb, "Predelay") => format!("Predelay: {v:.0} ms"),
        (Kind::Reverb, "Decay") => format!("Decay: {:.1} s", v / 10.0),
        (Kind::Reverb, "Dry" | "Early" | "Wet") => pct(v / 128.0),
        (Kind::Reverb, "Crossover") => format!("Crossover: {v:.0} Hz"),
        (Kind::Limiter, "Gain") => format!("Gain: {:+.1} dB", 40.0 * (v / 1000.0).max(1e-4).log10()),
        (Kind::Limiter, "Ceiling") => format!("Ceiling: {:+.1} dB", 20.0 * (v / 1000.0).max(1e-4).log10()),
        (Kind::Loud, "Mode") => format!("Mode: {}", ["A", "B", "C", "D"][(v as usize).min(3)]),
        (Kind::Loud, "Amount") => pct(v / 1000.0),
        (Kind::SoftClip, "Threshold") => pct(v / 127.0),
        (Kind::SoftClip, _) | (Kind::FastDist, "Mix" | "Post" | "Pre") => pct(v / 128.0),
        (Kind::FastDist, "Type") => format!("Type: {}", ["A", "B"][(v as usize).min(1)]),
        (Kind::Shaper, "Right to left" | "Left" | "Right" | "Left to right") => pct(v / 12800.0),
        (Kind::Overdrive, "x100") | (Kind::Filter, "x2") => format!("{name}: {}", if v >= 0.5 { "on" } else { "off" }),
        (Kind::Overdrive, _) => pct(v / 10000.0),
        (Kind::Filter | Kind::FastLp, "Cutoff") => format!("Cutoff: {:.0} Hz", hz(v / 1024.0)),
        (Kind::Filter | Kind::FastLp, _) => pct(v / 1024.0),
        _ => {
            let _ = i;
            format!("{name}: {v:.0}")
        }
    }
}

/// Magnitude in dB of one equaliser band at `f` Hz (an analogue-prototype approximation, for the picture only).
fn band_db(kind: i32, f0: f32, gain: f32, width: f32, f: f32) -> f32 {
    let x = (f / f0).log2();
    let bw = 0.2 * (width.clamp(0.0, 1.0) * 6.0).exp2();
    match kind {
        0 => 0.0,
        1 => -12.0 * x.max(0.0),
        3 => -12.0 * (-x).max(0.0),
        2 => -6.0 * (x.abs() / bw.max(0.05)),
        4 => -40.0 * (1.0 - (x.abs() / (bw * 0.25).max(0.02)).min(1.0)),
        5 => gain / (1.0 + (x * 2.5).exp()),
        7 => gain / (1.0 + (-x * 2.5).exp()),
        _ => gain / (1.0 + (2.0 * x / bw.max(0.05)).powi(2)),
    }
}

impl App {
    /// Show the editor of the open effect slot, if any.
    pub(crate) fn fx_window(&mut self, ctx: &egui::Context, frame: egui::Frame) {
        let Some((ins, slot)) = self.fx_open else { return };
        let Some(si) = self.song.inserts.get(ins).and_then(|i| i.slots.iter().position(|s| s.index == slot)) else {
            self.fx_open = None;
            return;
        };
        let title = format!("{} - {}", self.song.inserts[ins].slots[si].title(), self.song.insert_name(ins));
        let mut open = true;
        let mut changed = false;
        let ident = self.song.inserts[ins].slots[si].fx.as_ref().map(|f| fx::def(f.kind).ident);
        let mut pick = None;
        let mut pick_ir = false;
        let mut cab_changed = false;
        egui::Window::new(title).id(egui::Id::new("fxwin")).open(&mut open).frame(frame).resizable(false).default_pos([420.0, 160.0]).show(ctx, |ui| {
            if let Some(ident) = ident {
                pick = self.preset_bar(ui, ident, false);
            }
            let s = &mut self.song.inserts[ins].slots[si];
            ui.horizontal(|ui| {
                if led(ui, s.enabled, 12.0).on_hover_text("Effect on / off").clicked() {
                    s.enabled = !s.enabled;
                    changed = true;
                }
                let r = knob(ui, &mut s.mix, 0.0, 1.0, 1.0, 20.0).on_hover_text(format!("Mix level: {:.0} %", s.mix * 100.0));
                control_menu(&r, cereal_engine::song::Target::SlotMix(ins, slot));
                changed |= r.changed();
                ui.label(RichText::new("Mix").small().color(TEXT_DIM));
            });
            ui.separator();
            let Some(f) = s.fx.as_mut() else {
                ui.label(RichText::new(format!("\"{}\" has no built-in unit yet. Its settings are kept as they are\nand saved back untouched; the sound passes through.", s.plugin)).color(TEXT_DIM));
                return;
            };
            let d = fx::def(f.kind);
            if f.kind == Kind::Eq {
                changed |= eq_curve(ui, f);
            }
            if f.kind == Kind::Formula {
                changed |= formula_ui(ui, f);
            }
            if f.kind == Kind::Xy {
                changed |= xy_pad(ui, f);
            }
            if f.kind == Kind::Conv {
                ui.horizontal(|ui| {
                    let name = match &f.ir {
                        Some(ir) if !ir.smp.name.is_empty() => format!("{} ({:.2} s)", ir.smp.name, ir.smp.seconds()),
                        Some(ir) => format!("Held in the project ({:.2} s)", ir.smp.seconds()),
                        None => "No impulse loaded".to_string(),
                    };
                    ui.label(RichText::new(name).small());
                    if ui.button(RichText::new("Load impulse...").small()).clicked() {
                        pick_ir = true;
                    }
                });
            }
            if f.kind == Kind::Rack {
                use cereal_engine::fxrack as rk;
                let mut soft = f.p[0] >= 0.5;
                let mut n = rk::count(&f.p);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut soft, RichText::new("Soft clip output").small()).changed() {
                        f.p[0] = soft as u8 as f32;
                        changed = true;
                    }
                    if n < rk::SLOTS && ui.button(RichText::new("+ Module").small()).clicked() {
                        rk::blank(&mut f.p[rk::HEAD + n * rk::PER..rk::HEAD + (n + 1) * rk::PER], 0);
                        n += 1;
                        f.p[1] = n as f32;
                        changed = true;
                    }
                    if n > 0 && ui.button(RichText::new("- Module").small()).clicked() {
                        n -= 1;
                        f.p[1] = n as f32;
                        changed = true;
                        cab_changed = true;
                    }
                });
                for slot in 0..n {
                    let m = &mut f.p[rk::HEAD + slot * rk::PER..rk::HEAD + (slot + 1) * rk::PER];
                    ui.separator();
                    ui.horizontal(|ui| {
                        let mut on = m[1] >= 0.5;
                        if ui.checkbox(&mut on, "").changed() {
                            m[1] = on as u8 as f32;
                            changed = true;
                        }
                        let kind = (m[0] as usize).min(3);
                        egui::ComboBox::from_id_salt(("racktype", ins, slot)).width(86.0).selected_text(RichText::new(rk::TYPES[kind]).small()).show_ui(ui, |ui| {
                            for (i, name) in rk::TYPES.iter().enumerate() {
                                if ui.selectable_label(kind == i, *name).clicked() && kind != i {
                                    m[0] = i as f32;
                                    if i == 0 {
                                        m[2] = 3.0;
                                    } else {
                                        m[2] = 0.0;
                                    }
                                    changed = true;
                                    cab_changed = true;
                                }
                            }
                        });
                        let kind = (m[0] as usize).min(3);
                        let (at, names): (usize, Vec<String>) = match kind {
                            0 => (2, rk::DRIVES.iter().map(|s| s.to_string()).collect()),
                            1 => (3, rk::FILTERS.iter().map(|s| s.to_string()).collect()),
                            2 => (2, rk::CHORUSES.iter().map(|s| s.to_string()).collect()),
                            _ => (4, (0..rk::CABINETS).map(|i| rk::cabinet_file(i).unwrap_or_default().trim_start_matches("IMP ").trim_end_matches(".wv").to_string()).collect()),
                        };
                        let cur = (m[at].max(0.0) as usize).min(names.len() - 1);
                        egui::ComboBox::from_id_salt(("rackmodel", ins, slot)).width(120.0).selected_text(RichText::new(&names[cur]).small()).show_ui(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                if ui.selectable_label(cur == i, name).clicked() && cur != i {
                                    m[at] = i as f32;
                                    changed = true;
                                    cab_changed |= kind == 3;
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        let kind = m[0] as i32;
                        let model = m[2] as i32;
                        let common: [(&str, usize, i32, i32); 3] = [("In", rk::IN, 0, 10000), ("Mix", rk::MIX, 0, 10000), ("Out", rk::OUT, 0, 10000)];
                        for &(name, i, lo, hi) in common.iter().chain(rk::shown(kind, model)) {
                            let v = &mut m[rk::LEAD + i];
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.set_width(58.0);
                                ui.vertical_centered(|ui| {
                                    let before = *v;
                                    let mid = if i == rk::IN || i == rk::OUT { 5000.0 } else { before };
                                    let r = knob(ui, v, lo as f32, hi as f32, mid, 26.0);
                                    *v = v.round();
                                    if r.changed() && *v != before {
                                        changed = true;
                                    }
                                    r.on_hover_text(format!("{name}: {}", *v as i32));
                                    ui.label(RichText::new(name).small().color(TEXT_DIM));
                                });
                            });
                        }
                    });
                }
                if n > 0 && rk::cabinet_of(&f.p).is_some() && f.ir.is_none() {
                    // a rack made here has not looked for its cabinet yet: try once
                    let id = egui::Id::new(("racktried", ins, slot));
                    if !ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
                        ui.data_mut(|d| d.insert_temp(id, true));
                        cab_changed = true;
                    }
                    ui.label(RichText::new("The cabinet sounds come with your own content; none was found, so the cabinet passes the sound on.").small().color(TEXT_DIM));
                }
            }
            if f.kind == Kind::Voc {
                use cereal_engine::fxvoc as vc;
                let pick = |ui: &mut egui::Ui, f: &mut fx::Fx, salt: &str, at: usize, none: &str| -> bool {
                    let mut hit = false;
                    let cur = f.p[at] as i32;
                    let name = |v: i32| match v {
                        0 => "This track".to_string(),
                        v if v < 0 => none.to_string(),
                        v => format!("Routed track {v}"),
                    };
                    egui::ComboBox::from_id_salt((salt, ins, slot)).width(110.0).selected_text(RichText::new(name(cur)).small()).show_ui(ui, |ui| {
                        for v in -1..=8 {
                            if ui.selectable_label(cur == v, name(v)).clicked() && cur != v {
                                f.p[at] = v as f32;
                                hit = true;
                            }
                        }
                    });
                    hit
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Modulator").small().color(TEXT_DIM));
                    changed |= pick(ui, f, "vocmod", vc::MOD_SEL, "None");
                    ui.label(RichText::new("Carrier").small().color(TEXT_DIM));
                    changed |= pick(ui, f, "voccar", vc::CAR_SEL, "Built-in synth");
                    let mut lr = f.p[vc::FLAGS + vc::F_LR] >= 0.5;
                    if ui.checkbox(&mut lr, RichText::new("Left = modulator, right = carrier").small()).changed() {
                        f.p[vc::FLAGS + vc::F_LR] = lr as u8 as f32;
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Built-in sound").small().color(TEXT_DIM));
                    let cur = (f.p[vc::CARRIER] as usize).min(vc::CARRIERS.len() - 1);
                    egui::ComboBox::from_id_salt(("vocsound", ins, slot)).width(110.0).selected_text(RichText::new(vc::CARRIERS[cur].1).small()).show_ui(ui, |ui| {
                        for (i, c) in vc::CARRIERS.iter().enumerate() {
                            if ui.selectable_label(cur == i, c.1).clicked() && cur != i {
                                f.p[vc::CARRIER] = i as f32;
                                changed = true;
                            }
                        }
                    });
                    let n = (f.p[vc::NOTES] as usize).min(vc::MAXNOTES);
                    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                    let held: Vec<String> = (0..n).map(|i| f.p[vc::NOTES + 1 + i] as i32).map(|k| format!("{}{}", names[k.rem_euclid(12) as usize], k.div_euclid(12))).collect();
                    ui.label(RichText::new(if held.is_empty() { "no notes held".to_string() } else { format!("holding {}", held.join(" ")) }).small());
                    for (label, k) in [("+3rd", 4), ("+5th", 7), ("+Oct", 12)] {
                        if n > 0 && n < vc::MAXNOTES && ui.button(RichText::new(label).small()).clicked() {
                            f.p[vc::NOTES + 1 + n] = f.p[vc::NOTES + 1] + k as f32;
                            f.p[vc::NOTES] = (n + 1) as f32;
                            changed = true;
                        }
                    }
                    if n > 1 && ui.button(RichText::new("- Note").small()).clicked() {
                        f.p[vc::NOTES] = (n - 1) as f32;
                        changed = true;
                    }
                    if n == 0 && ui.button(RichText::new("+ Note").small()).clicked() {
                        f.p[vc::NOTES] = 1.0;
                        f.p[vc::NOTES + 1] = 60.0;
                        changed = true;
                    }
                    if n > 0 {
                        let mut root = f.p[vc::NOTES + 1];
                        if ui.add(egui::DragValue::new(&mut root).range(24.0..=96.0).speed(0.1).prefix("root ")).changed() {
                            let by = root.round() - f.p[vc::NOTES + 1];
                            for i in 0..n {
                                f.p[vc::NOTES + 1 + i] += by;
                            }
                            changed = true;
                        }
                    }
                });
                let rows: [&[(&str, usize, f32, f32)]; 3] = [
                    &[("Wet", vc::WET, 0.0, 2000.0), ("Finish", vc::FINISH, 0.0, 1000.0), ("Mod thru", vc::MOD_THRU, 0.0, 65536.0), ("Mod high-pass", vc::MOD_HP, 0.0, 65536.0), ("Car thru", vc::CAR_THRU, 0.0, 65536.0), ("Car low-pass", vc::CAR_LP, 0.0, 65536.0), ("Noise", vc::NOISE, 0.0, 65536.0), ("Noise high-pass", vc::NOISE_HP, 0.0, 65536.0)],
                    &[("Hold", vc::HOLD, 0.0, 10000.0), ("Attack", vc::ATT, 0.0, 10000.0), ("Release", vc::REL, 0.0, 10000.0), ("Bands", vc::BANDS, 5.0, 100.0), ("Order", vc::ORDER, 1.0, 4.0), ("Band width", vc::WIDTH, 50.0, 1000.0), ("Mod width", vc::MOD_WIDTH, 200.0, 8000.0)],
                    &[("Pitch shift", vc::SHIFT, -1200.0, 1200.0), ("Unison", vc::UNISON, 0.0, 5.0), ("Unison shift", vc::UNI_SHIFT, 0.0, 1000.0), ("Unison pan", vc::UNI_PAN, 0.0, 256.0)],
                ];
                for row in rows {
                    ui.horizontal(|ui| {
                        for &(name, i, lo, hi) in row {
                            let v = &mut f.p[i];
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.set_width(62.0);
                                ui.vertical_centered(|ui| {
                                    let before = *v;
                                    let r = knob(ui, v, lo, hi, before, 26.0);
                                    *v = v.round();
                                    if r.changed() && *v != before {
                                        changed = true;
                                    }
                                    r.on_hover_text(format!("{name}: {}", *v as i32));
                                    ui.label(RichText::new(name).small().color(TEXT_DIM));
                                });
                            });
                        }
                        if row.len() == 4 {
                            for (label, at) in [("Smooth follower", vc::RMS), ("Contour from carrier", vc::FLAGS + vc::F_FROM_CAR), ("from consonants", vc::FLAGS + vc::F_FROM_CONS), ("to modulator", vc::FLAGS + vc::F_TO_MOD), ("to noise", vc::FLAGS + vc::F_TO_NOISE)] {
                                let mut on = f.p[at] >= 0.5;
                                if ui.checkbox(&mut on, RichText::new(label).small()).changed() {
                                    f.p[at] = on as u8 as f32;
                                    changed = true;
                                }
                            }
                        }
                    });
                }
                // the band map: where each band sits and how much it is turned up
                let n = (f.p[vc::BANDS] as usize).clamp(5, 100);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(560.0), 54.0), egui::Sense::hover());
                let pt = ui.painter_at(rect);
                pt.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
                for b in 0..n {
                    let t = (b as f32 + 0.5) / n as f32;
                    let y = vc::env_level(&f.p, 8, t).clamp(0.0, 1.0);
                    let g = vc::env_level(&f.p, 0, t).clamp(0.0, 1.0);
                    let x = rect.left() + y * rect.width();
                    pt.line_segment([egui::pos2(x, rect.bottom()), egui::pos2(x, rect.bottom() - g * rect.height())], egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(120, 190, 230)));
                }
            }
            if f.kind == Kind::OldVoc {
                use cereal_engine::fxvoc::classic as cv;
                ui.label(RichText::new("The left channel is the modulator, the right channel the carrier.").small().color(TEXT_DIM));
                ui.horizontal(|ui| {
                    for (name, i, lo, hi) in [("Formant", cv::FORMANT, -1200.0, 1200.0), ("Min", cv::MIN, 0.0, 65536.0), ("Max", cv::MAX, 0.0, 65536.0), ("Scale", cv::SCALE, 0.0, 128.0), ("Width", cv::WIDTH, 5.0, 300.0), ("Attack", cv::ATTACK, 0.0, 500.0), ("Decay", cv::DECAY, 0.0, 1000.0), ("Modulator", cv::MOD, 0.0, 256.0), ("Vocoded", cv::CAR, 0.0, 256.0), ("Bands", cv::BANDS, 4.0, 128.0), ("Order", cv::ORDER, 1.0, 4.0)] {
                        let v = &mut f.p[i];
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.set_width(52.0);
                            ui.vertical_centered(|ui| {
                                let before = *v;
                                let r = knob(ui, v, lo, hi, before, 26.0);
                                *v = v.round();
                                if r.changed() && *v != before {
                                    changed = true;
                                }
                                r.on_hover_text(format!("{name}: {}", *v as i32));
                                ui.label(RichText::new(name).small().color(TEXT_DIM));
                            });
                        });
                    }
                });
                ui.horizontal(|ui| {
                    for (label, at) in [("Invert", cv::INVERT), ("Hold", cv::HOLD)] {
                        let mut on = f.p[at] >= 0.5;
                        if ui.checkbox(&mut on, RichText::new(label).small()).changed() {
                            f.p[at] = on as u8 as f32;
                            changed = true;
                        }
                    }
                });
                // a level per band: drag up and down
                let n = (f.p[cv::BANDS] as usize).clamp(1, cv::MOST);
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(560.0), 64.0), egui::Sense::click_and_drag());
                let pt = ui.painter_at(rect);
                pt.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
                let w = rect.width() / n as f32;
                if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
                    let b = (((pos.x - rect.left()) / w) as usize).min(n - 1);
                    let v = ((rect.bottom() - pos.y) / rect.height() * 2.0).clamp(0.0, 2.0);
                    if f.p[cv::GAINS + b] != v {
                        f.p[cv::GAINS + b] = v;
                        changed = true;
                    }
                }
                for b in 0..n {
                    let h = (f.p[cv::GAINS + b] / 2.0).clamp(0.0, 1.0) * rect.height();
                    let x = rect.left() + b as f32 * w;
                    pt.rect_filled(egui::Rect::from_min_max(egui::pos2(x + 0.5, rect.bottom() - h), egui::pos2(x + w - 0.5, rect.bottom())), 0.0, egui::Color32::from_rgb(120, 190, 230));
                }
            }
            let per_row = if f.kind == Kind::Eq { 7 } else { 6 };
            // units made of several like sections show one section at a time: (settings in front, settings per
            // section, section names)
            let paged: Option<(usize, usize, &[&str])> = match f.kind {
                Kind::Maxi => Some((7, 13, &["Low", "Mid", "High", "Main"])),
                Kind::EchoBank => Some((4, 20, &["1", "2", "3", "4", "5", "6", "7", "8"])),
                _ => None,
            };
            if f.kind == Kind::Patch {
                changed |= bay_ui(ui, f);
            }
            let mut shown: Vec<usize> = (0..d.params.len()).collect();
            let mut cut = 0;
            if let Some((head, per, names)) = paged {
                let id = egui::Id::new(("fxpage", ins, slot));
                let mut page = ui.data(|m| m.get_temp::<usize>(id)).unwrap_or(0).min(names.len() - 1);
                ui.horizontal(|ui| {
                    for (i, n) in names.iter().enumerate() {
                        if ui.selectable_label(page == i, RichText::new(*n).small()).clicked() {
                            page = i;
                        }
                    }
                });
                ui.data_mut(|m| m.insert_temp(id, page));
                shown = (0..head).chain(head + page * per..head + (page + 1) * per).collect();
                cut = names[page].len() + 1;
            }
            let rows = shown.len().div_ceil(per_row);
            for r in 0..rows {
                ui.horizontal(|ui| {
                    for &k in &shown[r * per_row..((r + 1) * per_row).min(shown.len())] {
                        let p = &d.params[k];
                        let head = paged.is_none_or(|(h, _, _)| k < h);
                        let label = if head { p.name } else { &p.name[cut.min(p.name.len())..] };
                        let v = &mut f.p[k];
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.set_width(58.0);
                            ui.vertical_centered(|ui| {
                                let before = *v;
                                let r = knob(ui, v, p.min as f32, p.max as f32, p.def as f32, 26.0);
                                // settings are whole numbers in the file; switches and lists step one at a time
                                *v = v.round();
                                if r.changed() && *v != before {
                                    changed = true;
                                }
                                control_menu(&r, cereal_engine::song::Target::SlotParam(ins, slot, k as u16));
                                r.on_hover_text(hint(d.kind, k, p.name, *v));
                                ui.label(RichText::new(label).small().color(TEXT_DIM));
                            });
                        });
                    }
                });
            }
        });
        if cab_changed {
            let want = self.song.inserts[ins].slots[si].fx.as_ref().and_then(|f| cereal_engine::fxrack::cabinet_of(&f.p)).and_then(cereal_engine::fxrack::cabinet_file);
            let ir = want.and_then(|name| self.lib.load_stored(&name, None).map(|smp| fx::Ir { smp, path: name }));
            if let Some(f) = self.song.inserts[ins].slots[si].fx.as_mut() {
                f.ir = ir;
            }
        }
        if pick_ir {
            let mut dlg = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]);
            if let Some(dir) = self.lib.factory_roots.iter().map(|r| r.join("Data/Patches/Impulses")).find(|d| d.is_dir()) {
                dlg = dlg.set_directory(dir);
            }
            if let Some(path) = dlg.pick_file() {
                match self.lib.load_path(&path) {
                    Ok(smp) => {
                        if let Some(f) = self.song.inserts[ins].slots[si].fx.as_mut() {
                            f.ir = Some(fx::Ir { smp, path: path.to_string_lossy().to_string() });
                            changed = true;
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if changed {
            self.dirty = true;
        }
        if let Some(p) = pick {
            self.actions.push(crate::Action::LoadPreset(p, crate::PresetTo::Slot(ins, slot)));
        }
        if !open {
            self.fx_open = None;
        }
    }
}

/// The pad controller's pad: drag the dot to send X (left to right) and Y (bottom to top).
fn xy_pad(ui: &mut Ui, f: &mut fx::Fx) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(240.0, 240.0), egui::Sense::click_and_drag());
    let mut changed = false;
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
        if f.p.len() >= 2 {
            f.p[0] = (x * 65536.0).round();
            f.p[1] = (y * 65536.0).round();
            changed = true;
        }
    }
    if resp.double_clicked() && f.p.len() >= 2 {
        f.p[0] = 32768.0;
        f.p[1] = 32768.0;
        changed = true;
    }
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, Color32::from_rgb(24, 28, 32));
    for k in 1..4 {
        let t = k as f32 / 4.0;
        let c = Color32::from_rgb(44, 50, 56);
        p.line_segment([egui::pos2(rect.left() + t * rect.width(), rect.top()), egui::pos2(rect.left() + t * rect.width(), rect.bottom())], egui::Stroke::new(1.0_f32, c));
        p.line_segment([egui::pos2(rect.left(), rect.top() + t * rect.height()), egui::pos2(rect.right(), rect.top() + t * rect.height())], egui::Stroke::new(1.0_f32, c));
    }
    let x = f.p.first().copied().unwrap_or(32768.0) / 65536.0;
    let y = f.p.get(1).copied().unwrap_or(32768.0) / 65536.0;
    let at = egui::pos2(rect.left() + x * rect.width(), rect.bottom() - y * rect.height());
    p.line_segment([egui::pos2(at.x, rect.top()), egui::pos2(at.x, rect.bottom())], egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 90, 110)));
    p.line_segment([egui::pos2(rect.left(), at.y), egui::pos2(rect.right(), at.y)], egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 90, 110)));
    p.circle_filled(at, 7.0, Color32::from_rgb(240, 160, 60));
    ui.label(RichText::new(format!("X {:.0} %   Y {:.0} %   (double-click to centre; link controls to X or Y with right-click, Link to controller)", x * 100.0, y * 100.0)).small().color(TEXT_DIM));
    changed
}

/// The formula sender's formula box, with what the formula gives right now or what is wrong with it.
fn formula_ui(ui: &mut Ui, f: &mut fx::Fx) -> bool {
    use cereal_engine::formula;
    let mut text = f.formula();
    let mut changed = false;
    ui.label(RichText::new("Formula (a, b and c are the knobs below; the result is cut to 0..1)").small().color(TEXT_DIM));
    if ui.add(egui::TextEdit::singleline(&mut text).desired_width(430.0).font(egui::TextStyle::Monospace)).changed() {
        f.set_formula(&text);
        changed = true;
    }
    let knob = |k: usize| (f.p.get(k).copied().unwrap_or(0.0) / 65536.0) as f64;
    match formula::compile(&text) {
        Ok(c) => {
            let v = c.value(&formula::Vars { a: knob(0), b: knob(1), c: knob(2), tempo: 130.0, ..Default::default() });
            ui.label(RichText::new(format!("Compiled ok. At the song start it gives {v:.3}")).small().color(Color32::from_rgb(120, 200, 120)));
        }
        Err(e) => {
            ui.label(RichText::new(format!("Not understood: {e}")).small().color(Color32::from_rgb(230, 120, 100)));
        }
    }
    ui.collapsing("What a formula can use", |ui| {
        ui.label(RichText::new("+  -  *  /  ^  =  <>  >  >=  <  <=  %  !  div  mod  fmod  not  and  or  xor  shl  shr\nabs  sin  cos  tan  tg  ctg  sec  cosec  arcsin  arccos  arctg  atan2  sinh  cosh  tanh\nexp  ln  log2  log10  logn  sqrt  sqr  pow  power  intpower  min  max  sum  neg\nint  floor  ceil  round  trunc  frac  fold  warp  map  tension  degtorad  radtodeg\nif  case  ife  ifg  ifge  ifl  ifle  rand  seededrand  pi  songtime  gettempo  time").small().monospace().color(TEXT_DIM));
        ui.label(RichText::new("To drive a control with the result: right-click the control, Link to controller, and pick this unit.").small().color(TEXT_DIM));
    });
    ui.add_space(4.0);
    changed
}

/// The equaliser's response picture with a handle per band. Returns true when a handle moved a setting.
/// The held units of a patch bay, in the order they run, each with its own settings to turn.
pub fn bay_ui(ui: &mut Ui, f: &mut fx::Fx) -> bool {
    use cereal_engine::fxpatch::End;
    let Some(sub) = f.sub.as_mut() else { return false };
    let mut changed = false;
    let title = |n: &cereal_engine::fxpatch::Node| match &n.fx {
        Some(g) => fx::def(g.kind).title.to_string(),
        None => cereal_engine::synth::title_for(&n.ident).map(str::to_string).unwrap_or_else(|| "Unit not built yet".to_string()),
    };
    let end = |e: End, bay: &str| match e {
        End::Bay => bay.to_string(),
        End::Node(i) => format!("{}", i + 1),
    };
    let cables: Vec<String> = sub.cables.iter().map(|c| format!("{} > {}", end(c.from, "in"), end(c.to, "out"))).collect();
    ui.label(RichText::new(format!("{} units, {} sound cables", sub.nodes.len(), sub.cables.len())).small().color(TEXT_DIM));
    if !cables.is_empty() {
        ui.add(egui::Label::new(RichText::new(cables.join("   ")).small().color(TEXT_DIM)).wrap());
    }
    ui.set_max_width(520.0);
    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
        for at in 0..sub.order.len() {
            let i = sub.order[at];
            let head = format!("{}. {}", i + 1, title(&sub.nodes[i]));
            let Some(kind) = sub.nodes[i].fx.as_ref().map(|g| g.kind) else {
                ui.label(RichText::new(head).small());
                continue;
            };
            let d = fx::def(kind);
            if d.params.is_empty() && kind != Kind::Patch {
                ui.label(RichText::new(head).small());
                continue;
            }
            egui::CollapsingHeader::new(RichText::new(head).small()).id_salt(("bayunit", i)).show(ui, |ui| {
                if kind == Kind::Patch {
                    // a bay inside a bay
                    let mut inner = sub.nodes[i].fx.clone().unwrap();
                    if bay_ui(ui, &mut inner) {
                        std::sync::Arc::make_mut(sub).nodes[i].fx = Some(inner);
                        changed = true;
                    }
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    for (k, p) in d.params.iter().enumerate() {
                        let mut v = sub.nodes[i].fx.as_ref().unwrap().p[k];
                        let before = v;
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.set_width(58.0);
                            ui.vertical_centered(|ui| {
                                let r = knob(ui, &mut v, p.min as f32, p.max as f32, p.def as f32, 26.0);
                                v = v.round();
                                r.on_hover_text(hint(kind, k, p.name, v));
                                ui.label(RichText::new(p.name).small().color(TEXT_DIM));
                            });
                        });
                        if v != before {
                            std::sync::Arc::make_mut(sub).nodes[i].fx.as_mut().unwrap().p[k] = v;
                            changed = true;
                        }
                    }
                });
            });
        }
    });
    changed
}

fn eq_curve(ui: &mut Ui, f: &mut fx::Fx) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(7.0 * 66.0 - 8.0, 150.0), Sense::click_and_drag());
    let pt = ui.painter_at(rect);
    pt.rect_filled(rect, 3.0, PANEL_DARK);
    let x_of = |n: f32| rect.left() + n * rect.width();
    let y_of = |db: f32| rect.center().y - db / 18.0 * (rect.height() / 2.0 - 6.0);
    for hzv in [50.0f32, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        let x = x_of((hzv / 20.0).ln() / 1000f32.ln());
        pt.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, Color32::from_white_alpha(10)));
    }
    for dbv in [-12.0f32, -6.0, 0.0, 6.0, 12.0] {
        pt.line_segment([Pos2::new(rect.left(), y_of(dbv)), Pos2::new(rect.right(), y_of(dbv))], Stroke::new(1.0_f32, Color32::from_white_alpha(if dbv == 0.0 { 28 } else { 10 })));
    }
    let main = f.p[35] / 100.0;
    let pts: Vec<Pos2> = (0..=160)
        .map(|i| {
            let n = i as f32 / 160.0;
            let fr = hz(n);
            let mut db = main;
            for b in 0..7 {
                db += band_db(f.p[21 + b] as i32, hz(f.p[7 + b] / 65536.0), f.p[b] / 100.0, f.p[14 + b] / 65536.0, fr);
            }
            Pos2::new(x_of(n), y_of(db.clamp(-20.0, 20.0)))
        })
        .collect();
    pt.add(egui::Shape::line(pts, Stroke::new(1.5_f32, GREEN)));
    // handles: drag to set frequency and level
    let mut changed = false;
    let id = ui.id().with("eq-drag");
    let mut held: Option<usize> = ui.data(|d| d.get_temp(id)).unwrap_or(None);
    let pos_of = |f: &fx::Fx, b: usize| Pos2::new(x_of(f.p[7 + b] / 65536.0), y_of(f.p[b] / 100.0));
    if resp.drag_started() {
        if let Some(p) = resp.interact_pointer_pos() {
            held = (0..7).filter(|&b| f.p[21 + b] as i32 != 0).min_by(|&a, &b| pos_of(f, a).distance(p).total_cmp(&pos_of(f, b).distance(p))).filter(|&b| pos_of(f, b).distance(p) < 24.0);
        }
    }
    if let (Some(b), Some(p), true) = (held, resp.interact_pointer_pos(), resp.dragged()) {
        f.p[7 + b] = (((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * 65536.0).round();
        f.p[b] = ((rect.center().y - p.y) / (rect.height() / 2.0 - 6.0) * 1800.0).clamp(-1800.0, 1800.0).round();
        changed = true;
    }
    if resp.drag_stopped() {
        held = None;
    }
    ui.data_mut(|d| d.insert_temp(id, held));
    for b in 0..7 {
        let on = f.p[21 + b] as i32 != 0;
        let c = pos_of(f, b);
        pt.circle_filled(c, 7.0, if on { Color32::from_rgb(90, 140, 190) } else { PANEL });
        pt.text(c, egui::Align2::CENTER_CENTER, format!("{}", b + 1), egui::FontId::proportional(9.0), Color32::WHITE);
    }
    changed
}
