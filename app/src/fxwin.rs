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
        (Kind::Xyz, "X" | "Y" | "Z") => pct(v / 65536.0),
        (Kind::Xyz, "Global speed") if v <= 0.0 => "Global speed: at once".to_string(),
        (Kind::Xyz, "Global speed") => format!("Global speed: {:.0} ms", cereal_engine::fx::xyz_time(v, 2048.0) * 1000.0),
        (Kind::Xyz, _) => pct(v / 4096.0),
        (Kind::AutoPan, "Pan") => format!("Pan: {:+.0} %", (v - 128.0) / 1.28),
        (Kind::AutoPan, "Volume") => pct(v / 256.0),
        (Kind::AutoPan, "Amount") => format!("Amount: {:+.0} %", (v - 64.0) / 0.64),
        (Kind::AutoPan, "Shape") => format!("Shape: {}", ["sine", "triangle", "square"][(v as usize).min(2)]),
        (Kind::AutoPan, "Target") => format!("Target: {}", ["pan", "volume", "pan"][(v as usize).min(2)]),
        (Kind::Echo, "Time") => format!("Time: {:.2} steps", v / 48.0),
        (Kind::Split, n) if n.ends_with(" effect") => format!("{name}: {}", cereal_engine::fxsplit::EFFECTS[(v as usize).min(5)]),
        (Kind::Split, "Mode") => format!("Mode: {}", cereal_engine::fxsplit::MODES[(v as usize).min(2)]),
        (Kind::Split, "Hold") => format!("Hold: {:.0} ms", cereal_engine::fxsplit::hold_time(v / 10000.0) * 1000.0),
        (Kind::Split, "Release") => format!("Release: {:.0} ms", cereal_engine::fxsplit::release_time(v / 10000.0) * 1000.0),
        (Kind::Split, n) if n.ends_with(" level") => format!("{name}: {:+.1} dB", 20.0 * cereal_engine::fxsplit::level(v / 10000.0).max(1e-5).log10()),
        (Kind::Split, n) if n.ends_with(" tilt") || n.ends_with(" times") || n.ends_with(" levels") => format!("{name}: {:+.0} %", (v - 5000.0) / 50.0),
        (Kind::Split, n) if n.ends_with(" echo steps") => format!("{name}: {:.2} steps", v / 48.0),
        (Kind::Split, n) if n.ends_with(" echo time") => format!("{name}: {:.0} ms", cereal_engine::fxsplit::echo_time(v / 10000.0) * 1000.0),
        (Kind::Split, n) if n.ends_with(" on") || n.ends_with(" to tempo") || n.starts_with("Link") || n == "Look ahead" => format!("{name}: {}", if v >= 0.5 { "on" } else { "off" }),
        (Kind::Split, n) if !n.starts_with("Spare") && !n.starts_with("Display") => pct(v / 10000.0),
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
        // a take only runs while its recorder is the window in front
        let taking = self.handle.transport.recording_slot();
        if taking.is_some() && taking != self.fx_open {
            self.handle.transport.record_slot(None);
        }
        let Some((ins, slot)) = self.fx_open else {
            self.handle.transport.watch_slot(None);
            return;
        };
        let Some(si) = self.song.inserts.get(ins).and_then(|i| i.slots.iter().position(|s| s.index == slot)) else {
            self.fx_open = None;
            return;
        };
        // the meter units draw the sound passing through them: ask the engine to keep it for us
        let meter = match self.song.inserts[ins].slots[si].plugin.as_str() {
            p if p.eq_ignore_ascii_case("Tuner") => 1,
            p if p.eq_ignore_ascii_case("Wave Candy") => 2,
            p if p.eq_ignore_ascii_case("Edison") => 3,
            p if p.eq_ignore_ascii_case("Fruity dB Meter") => 4,
            p if p.eq_ignore_ascii_case("Fruity Spectroman") => 5,
            p if p.eq_ignore_ascii_case("Fruity Big Clock") => 6,
            _ => 0,
        };
        self.handle.transport.watch_slot((meter != 0 && meter != 6).then_some((ins, slot)));
        let title = format!("{} - {}", self.song.inserts[ins].slots[si].title(), self.song.insert_name(ins));
        let shown = title.clone();
        let mut open = true;
        let mut changed = false;
        // several pass-through units share one kind, so the name in the slot decides which presets belong here
        let ident = self.song.inserts[ins].slots[si].fx.as_ref().map(|f| {
            let named = &self.song.inserts[ins].slots[si].plugin;
            fx::DEFS.iter().find(|d| d.kind == f.kind && d.ident.eq_ignore_ascii_case(named)).map(|d| d.ident).unwrap_or(fx::def(f.kind).ident)
        });
        // a hosted plug-in keeps its own presets
        let ident = ident.filter(|_| self.song.inserts[ins].slots[si].fx.as_ref().is_some_and(|f| f.ext.is_none()));
        let mut pick = None;
        let mut pick_ir = false;
        let mut cab_changed = false;
        let mut wave_act = None;
        let take_dir = crate::record::folder(self.path.as_deref());
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
            if let Some(h) = &f.ext {
                changed |= crate::hostui::panel(ui, &h.0, &shown, &|k| cereal_engine::song::Target::SlotParam(ins, slot, k));
                return;
            }
            let d = fx::def(f.kind);
            if meter != 0 {
                let (frames, count, rate) = self.handle.transport.watched(8192);
                // a count that has stood still for a moment means nothing is coming through
                let now = ui.input(|i| i.time);
                let id = ui.id().with("tapseen");
                let (last, since): (u64, f64) = ui.data(|d| d.get_temp(id)).unwrap_or((0, now));
                let since = if count != last { now } else { since };
                ui.data_mut(|d| d.insert_temp(id, (count, since)));
                let quiet = now - since > 0.25;
                let frames = if quiet { vec![[0.0; 2]; frames.len()] } else { frames };
                if meter == 3 {
                    wave_act = self.wave_eds.entry((ins, slot)).or_default().ui(ui, &self.handle, (ins, slot), rate, &take_dir);
                    return;
                }
                match meter {
                    1 => tuner_ui(ui, &frames, rate),
                    4 => scope_ui(ui, &frames, rate, Some(2)),
                    5 => scope_ui(ui, &frames, rate, Some(1)),
                    6 => {
                        let tr = &self.handle.transport;
                        let tempo = if tr.tempo() > 0.0 { tr.tempo() } else { self.song.tempo };
                        let tick = if tr.playing() { tr.tick().max(0.0) } else { 0.0 };
                        clock_ui(ui, tick, self.song.ppq, tempo, self.song.bar());
                    }
                    _ => scope_ui(ui, &frames, rate, None),
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                return;
            }
            if f.kind == Kind::Eq {
                changed |= eq_curve(ui, f);
            }
            if f.kind == Kind::Formula {
                changed |= formula_ui(ui, f);
            }
            if f.kind == Kind::Surface {
                changed |= surface_ui(ui, f, 900.0);
            }
            if f.kind == Kind::Notes {
                changed |= notes_ui(ui, f);
            }
            if f.line_count() > 0 {
                changed |= lines_ui(ui, f);
            }
            if matches!(f.kind, Kind::Xy | Kind::Xyz) {
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
            if f.kind == Kind::Deck && f.p.len() >= 9 {
                ui.horizontal(|ui| {
                    let name = match &f.ir {
                        Some(ir) => format!("{} ({:.2} s)", ir.smp.name, ir.smp.seconds()),
                        None => "No record loaded".to_string(),
                    };
                    ui.label(RichText::new(name).small());
                    if ui.button(RichText::new("Load sample...").small()).clicked() {
                        pick_ir = true;
                    }
                });
                ui.horizontal(|ui| {
                    let dir = if f.p[1] > 7500.0 { 2 } else if f.p[1] < 2500.0 { 0 } else { 1 };
                    for (k, label, tip) in [(0, "<<", "Play backwards"), (1, "||", "Pause"), (2, ">>", "Play forwards")] {
                        if ui.selectable_label(dir == k, RichText::new(label).small()).on_hover_text(tip).clicked() {
                            f.p[1] = k as f32 * 5000.0;
                            changed = true;
                        }
                    }
                    for (i, label, tip) in [(6, "Hold", "Stop the record turning by itself, to move it by the position alone"), (8, "Mute", "Silence the record")] {
                        let mut on = f.p[i] >= 5000.0;
                        if ui.checkbox(&mut on, RichText::new(label).small()).on_hover_text(tip).changed() {
                            f.p[i] = if on { 10000.0 } else { 0.0 };
                            changed = true;
                        }
                    }
                });
                // the platter: drag up and down to scratch
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(260.0, 44.0), egui::Sense::drag());
                let paint = ui.painter_at(rect);
                paint.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
                let at = rect.left() + rect.width() * (f.p[0] / 10000.0).clamp(0.0, 1.0);
                paint.line_segment([egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())], egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill));
                paint.text(rect.center(), egui::Align2::CENTER_CENTER, "drag to scratch", egui::FontId::proportional(10.0), ui.visuals().weak_text_color());
                if resp.dragged() {
                    let d = resp.drag_delta();
                    let turn = (d.x - d.y) * (0.5 + f.p[4] / 2500.0);
                    f.p[0] = (f.p[0] + turn).clamp(0.0, 10000.0);
                    changed = true;
                }
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
                // the lines across the bands: pick one and draw it
                let which_id = egui::Id::new(("vocline", ins, slot));
                let mut which: usize = ui.data(|d| d.get_temp(which_id)).unwrap_or(0).min(vc::NENV - 1);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Line across the bands").small().color(TEXT_DIM));
                    egui::ComboBox::from_id_salt(("voclinepick", ins, slot)).width(140.0).selected_text(RichText::new(vc::ENV_NAMES[which]).small()).show_ui(ui, |ui| {
                        for (i, name) in vc::ENV_NAMES.iter().enumerate() {
                            ui.selectable_value(&mut which, i, *name);
                        }
                    });
                });
                ui.data_mut(|d| d.insert_temp(which_id, which));
                if let Some(pts) = vc::env_points(&f.p, which) {
                    use cereal_engine::fm::{Curve, Pt};
                    let mut c = Curve::default();
                    let mut last = 0.0;
                    for q in &pts {
                        c.pts.push(Pt { x: (q[0] - last).max(0.0), y: q[1], ..Default::default() });
                        last = q[0];
                    }
                    let done = ui.push_id(("vocenv", ins, slot, which), |ui| crate::curve::curve_editor(ui, &mut c, crate::curve::Line::Mapping, 110.0)).inner;
                    if done {
                        let mut x = 0.0;
                        let now: Vec<[f32; 2]> = c.pts.iter().enumerate().map(|(i, q)| { if i > 0 { x += q.x.max(0.0); } [x, q.y] }).collect();
                        if vc::set_env_points(&mut f.p, which, &now) {
                            changed = true;
                        }
                    }
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
                if f.kind == Kind::EchoBank {
                    // slopes and grain sit after the eight sections in the settings list
                    shown.extend(165 + page * 4..169 + page * 4);
                }
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
        match wave_act {
            Some(crate::waveed::WaveAct::ToPlaylist(p)) => {
                let at = if self.handle.transport.song_mode() { self.handle.transport.tick().max(0.0) as u32 } else { 0 };
                self.add_audio_clip(&p, at, None);
                self.status = format!("Placed {}", p.display());
            }
            Some(crate::waveed::WaveAct::ToChannel(p)) => {
                self.load_sample(&p, None);
                self.status = format!("Loaded {}", p.display());
            }
            Some(crate::waveed::WaveAct::Status(s)) => self.status = s,
            None => {}
        }
        if cab_changed {
            let want = self.song.inserts[ins].slots[si].fx.as_ref().and_then(|f| cereal_engine::fxrack::cabinet_of(&f.p)).and_then(cereal_engine::fxrack::cabinet_file);
            let ir = want.and_then(|name| self.lib.load_stored(&name, None).map(|smp| fx::Ir { smp, path: name }));
            if let Some(f) = self.song.inserts[ins].slots[si].fx.as_mut() {
                f.ir = ir;
            }
        }
        if pick_ir {
            let mut dlg = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]);
            let deck = self.song.inserts[ins].slots[si].fx.as_ref().is_some_and(|f| f.kind == Kind::Deck);
            if let Some(dir) = self.lib.factory_roots.iter().map(|r| r.join("Data/Patches/Impulses")).find(|d| d.is_dir() && !deck) {
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

/// The pitch meter: the note being played, how far off it is, and its frequency.
fn tuner_ui(ui: &mut Ui, frames: &[[f32; 2]], rate: f32) {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let mono: Vec<f32> = frames.iter().rev().take(4096).rev().map(|f| (f[0] + f[1]) * 0.5).collect();
    let got = cereal_engine::scope::pitch(&mono, rate).filter(|g| g.1 > 0.8);
    // hold the last steady reading for a moment so the display does not flicker between notes
    let id = ui.id().with("tunerheld");
    let now = ui.input(|i| i.time);
    let held: Option<(f32, f64)> = ui.data(|d| d.get_temp(id));
    let show = match (got, held) {
        (Some((hz, _)), Some((old, _))) => Some(if (hz / old - 1.0).abs() < 0.03 { old + (hz - old) * 0.35 } else { hz }),
        (Some((hz, _)), None) => Some(hz),
        (None, Some((old, t))) if now - t < 0.6 => Some(old),
        _ => None,
    };
    match (show, got) {
        (Some(hz), Some(_)) => ui.data_mut(|d| d.insert_temp(id, (hz, now))),
        (Some(_), None) => {}
        _ => ui.data_mut(|d| d.remove::<(f32, f64)>(id)),
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(420.0, 170.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, Color32::from_rgb(20, 24, 28));
    let mid = rect.center().x;
    let bar_y = rect.bottom() - 34.0;
    let half = rect.width() * 0.5 - 24.0;
    for c in (-50..=50).step_by(10) {
        let x = mid + c as f32 / 50.0 * half;
        let tall = if c == 0 { 16.0 } else if c % 50 == 0 { 12.0 } else { 7.0 };
        p.line_segment([egui::pos2(x, bar_y - tall), egui::pos2(x, bar_y + tall)], egui::Stroke::new(1.0_f32, Color32::from_rgb(90, 100, 110)));
    }
    p.text(egui::pos2(mid - half, bar_y + 18.0), egui::Align2::CENTER_TOP, "-50", egui::FontId::proportional(10.0), TEXT_DIM);
    p.text(egui::pos2(mid + half, bar_y + 18.0), egui::Align2::CENTER_TOP, "+50", egui::FontId::proportional(10.0), TEXT_DIM);
    match show {
        Some(hz) => {
            let (key, cents) = cereal_engine::scope::nearest_key(hz);
            let good = cents.abs() < 5.0;
            let col = if good { Color32::from_rgb(110, 220, 120) } else if cents.abs() < 20.0 { Color32::from_rgb(240, 200, 80) } else { Color32::from_rgb(240, 120, 90) };
            let name = format!("{}{}", NAMES[key.rem_euclid(12) as usize], key.div_euclid(12));
            p.text(egui::pos2(mid, rect.top() + 48.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(64.0), col);
            p.text(egui::pos2(rect.left() + 10.0, rect.top() + 8.0), egui::Align2::LEFT_TOP, format!("{hz:.1} Hz"), egui::FontId::proportional(13.0), TEXT_DIM);
            p.text(egui::pos2(rect.right() - 10.0, rect.top() + 8.0), egui::Align2::RIGHT_TOP, format!("{cents:+.0} cents"), egui::FontId::proportional(13.0), TEXT_DIM);
            let x = mid + (cents / 50.0).clamp(-1.0, 1.0) * half;
            p.rect_filled(egui::Rect::from_center_size(egui::pos2(x, bar_y), egui::vec2(6.0, 30.0)), 2.0, col);
        }
        None => {
            p.text(egui::pos2(mid, rect.top() + 48.0), egui::Align2::CENTER_CENTER, "--", egui::FontId::proportional(64.0), Color32::from_rgb(70, 78, 86));
        }
    }
    ui.label(RichText::new("Shows the note of the sound on this mixer track. Key numbers count C5 as the middle C, as the piano roll does.").small().color(TEXT_DIM));
}

/// The big clock: where the song is, as bar, beat and tick or as minutes and seconds. A click changes between them.
fn clock_ui(ui: &mut Ui, tick: f64, ppq: u32, tempo: f32, per_bar: u32) {
    let id = ui.id().with("clockmode");
    let mut as_time: bool = ui.data(|d| d.get_temp(id)).unwrap_or(false);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(420.0, 120.0), egui::Sense::click());
    if resp.on_hover_text("Click to change between bar : beat : tick and minutes : seconds").clicked() {
        as_time = !as_time;
        ui.data_mut(|d| d.insert_temp(id, as_time));
    }
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, Color32::from_rgb(20, 24, 28));
    let text = if as_time {
        let secs = tick / (tempo.max(1.0) as f64 * ppq.max(1) as f64 / 60.0);
        format!("{}:{:02}:{:02}", (secs / 60.0) as u32, secs as u32 % 60, ((secs * 100.0) as u64 % 100))
    } else {
        let t = tick as u64;
        let (bar, rest) = (t / per_bar.max(1) as u64, t % per_bar.max(1) as u64);
        format!("{:03}:{:02}:{:03}", bar + 1, rest / ppq.max(1) as u64 + 1, rest % ppq.max(1) as u64)
    };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(72.0), Color32::from_rgb(240, 160, 60));
}

/// The scope meter: the wave, the frequencies, the levels or the left-right picture of the sound passing through.
fn scope_ui(ui: &mut Ui, frames: &[[f32; 2]], rate: f32, only: Option<u8>) {
    let id = ui.id().with("scopemode");
    let mut mode: u8 = only.unwrap_or_else(|| ui.data(|d| d.get_temp(id)).unwrap_or(0));
    if only.is_none() {
        ui.horizontal(|ui| {
            for (k, name) in ["Wave", "Spectrum", "Levels", "Left / right"].iter().enumerate() {
                ui.selectable_value(&mut mode, k as u8, *name);
            }
        });
        ui.data_mut(|d| d.insert_temp(id, mode));
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(520.0, 240.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, Color32::from_rgb(20, 24, 28));
    let line = Color32::from_rgb(240, 160, 60);
    let grid = Color32::from_rgb(44, 50, 56);
    let db = |v: f32| 20.0 * v.max(1e-6).log10();
    match mode {
        0 => {
            // start the picture where the wave crosses upward, so a steady tone stands still
            let n = 2048.min(frames.len());
            let mono: Vec<f32> = frames.iter().map(|f| (f[0] + f[1]) * 0.5).collect();
            let latest = mono.len().saturating_sub(n);
            let start = (1..=latest).rev().find(|&i| mono[i - 1] <= 0.0 && mono[i] > 0.0).unwrap_or(latest);
            p.line_segment([egui::pos2(rect.left(), rect.center().y), egui::pos2(rect.right(), rect.center().y)], egui::Stroke::new(1.0_f32, grid));
            let pts: Vec<egui::Pos2> = (0..n).filter_map(|i| mono.get(start + i).map(|v| egui::pos2(rect.left() + i as f32 / n as f32 * rect.width(), rect.center().y - v.clamp(-1.0, 1.0) * rect.height() * 0.48))).collect();
            p.add(egui::Shape::line(pts, egui::Stroke::new(1.3_f32, line)));
        }
        1 => {
            let mono: Vec<f32> = frames.iter().map(|f| (f[0] + f[1]) * 0.5).collect();
            let s = cereal_engine::scope::spectrum(&mono);
            let (f_lo, f_hi) = (20.0f32, (rate * 0.5).max(1000.0));
            let x_of = |f: f32| rect.left() + (f / f_lo).ln() / (f_hi / f_lo).ln() * rect.width();
            for f in [50.0f32, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0] {
                let x = x_of(f);
                p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.0_f32, grid));
                p.text(egui::pos2(x + 2.0, rect.bottom() - 12.0), egui::Align2::LEFT_TOP, if f >= 1000.0 { format!("{}k", f as u32 / 1000) } else { format!("{}", f as u32) }, egui::FontId::proportional(9.0), TEXT_DIM);
            }
            if s.len() > 2 {
                let per = rate / (s.len() as f32 * 2.0);
                // one reading per pixel column: the loudest frequency that falls in it
                let cols = rect.width() as usize;
                let mut pts = Vec::with_capacity(cols);
                for c in 0..cols {
                    let f0 = f_lo * (f_hi / f_lo).powf(c as f32 / cols as f32);
                    let f1 = f_lo * (f_hi / f_lo).powf((c + 1) as f32 / cols as f32);
                    let (a, b) = (((f0 / per) as usize).clamp(1, s.len() - 1), ((f1 / per).ceil() as usize).clamp(2, s.len()));
                    let v = s[a..b.max(a + 1)].iter().cloned().fold(0.0f32, f32::max);
                    let y = ((db(v) + 90.0) / 90.0).clamp(0.0, 1.0);
                    pts.push(egui::pos2(rect.left() + c as f32, rect.bottom() - y * rect.height()));
                }
                p.add(egui::Shape::line(pts, egui::Stroke::new(1.3_f32, line)));
            }
        }
        2 => {
            let n = 4096.min(frames.len());
            let tail = &frames[frames.len() - n..];
            for (c, name) in ["L", "R"].iter().enumerate() {
                let peak = tail.iter().map(|f| f[c].abs()).fold(0.0f32, f32::max);
                let rms = (tail.iter().map(|f| f[c] * f[c]).sum::<f32>() / n.max(1) as f32).sqrt();
                let y = rect.top() + 40.0 + c as f32 * 90.0;
                let bar = egui::Rect::from_min_size(egui::pos2(rect.left() + 30.0, y), egui::vec2(rect.width() - 130.0, 50.0));
                p.rect_filled(bar, 2.0, Color32::from_rgb(34, 38, 44));
                let w_of = |v: f32| ((db(v) + 60.0) / 66.0).clamp(0.0, 1.0) * bar.width();
                p.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(w_of(rms), bar.height())), 2.0, if peak >= 1.0 { Color32::from_rgb(230, 90, 70) } else { Color32::from_rgb(110, 200, 120) });
                let px = bar.left() + w_of(peak);
                p.line_segment([egui::pos2(px, bar.top()), egui::pos2(px, bar.bottom())], egui::Stroke::new(2.0_f32, line));
                let zero = bar.left() + w_of(1.0);
                p.line_segment([egui::pos2(zero, bar.top() - 4.0), egui::pos2(zero, bar.bottom() + 4.0)], egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 60, 60)));
                p.text(egui::pos2(rect.left() + 10.0, bar.center().y), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(14.0), TEXT_DIM);
                p.text(egui::pos2(bar.right() + 8.0, bar.center().y), egui::Align2::LEFT_CENTER, format!("{:.1} dB peak\n{:.1} dB average", db(peak), db(rms)), egui::FontId::proportional(10.0), TEXT_DIM);
            }
        }
        _ => {
            // left against right, turned so that sound in the middle draws straight up
            let c = rect.center();
            let r = rect.height() * 0.46;
            p.line_segment([egui::pos2(c.x, c.y - r), egui::pos2(c.x, c.y + r)], egui::Stroke::new(1.0_f32, grid));
            p.line_segment([egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)], egui::Stroke::new(1.0_f32, grid));
            let n = 2048.min(frames.len());
            for f in &frames[frames.len() - n..] {
                let (m, s) = ((f[0] + f[1]) * 0.5, (f[1] - f[0]) * 0.5);
                p.circle_filled(egui::pos2(c.x + s.clamp(-1.0, 1.0) * r, c.y - m.clamp(-1.0, 1.0) * r), 1.0, line);
            }
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
    if f.kind == Kind::Xyz && resp.hovered() && f.p.len() >= 3 {
        // depth is on the wheel
        let turn = ui.input(|i| i.smooth_scroll_delta.y);
        if turn != 0.0 {
            f.p[2] = (f.p[2] + turn * 64.0).clamp(0.0, 65536.0).round();
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
    let depth = if f.kind == Kind::Xyz { f.p.get(2).copied().unwrap_or(32768.0) / 65536.0 } else { 0.5 };
    p.circle_filled(at, 3.0 + 8.0 * depth, Color32::from_rgb(240, 160, 60));
    let more = if f.kind == Kind::Xyz { format!("   Z {:.0} % (wheel)", depth * 100.0) } else { String::new() };
    ui.label(RichText::new(format!("X {:.0} %   Y {:.0} %{more}   (double-click to centre; link controls to an axis with right-click, Link to controller)", x * 100.0, y * 100.0)).small().color(TEXT_DIM));
    changed
}

/// The formula sender's formula box, with what the formula gives right now or what is wrong with it.
/// The drawn lines of a unit, each in the line editor.
fn lines_ui(ui: &mut Ui, f: &mut fx::Fx) -> bool {
    let mut changed = false;
    // which lines to show, and what to call them
    let shown: Vec<(usize, String)> = match f.kind {
        Kind::Warp => {
            let slot = |k: usize| f.p.get(k).copied().unwrap_or(0.0).round().clamp(0.0, 35.0) as usize;
            vec![(slot(0), format!("Time line, slot {}", slot(0) + 1)), (36 + slot(1), format!("Volume line, slot {}", slot(1) + 1))]
        }
        Kind::Maxi => {
            let id = ui.id().with("maxiline");
            let mut k: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Level line of").small().color(TEXT_DIM));
                for (i, name) in ["Low", "Mid", "High", "Main"].iter().enumerate() {
                    ui.selectable_value(&mut k, i, *name);
                }
            });
            ui.data_mut(|d| d.insert_temp(id, k));
            vec![(k, String::new())]
        }
        Kind::Xyz => {
            let id = ui.id().with("xyzline");
            let mut k: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mapping of").small().color(TEXT_DIM));
                for (i, name) in ["X in", "X out", "Y in", "Y out", "Z in", "Z out"].iter().enumerate() {
                    ui.selectable_value(&mut k, i, *name);
                }
            });
            ui.data_mut(|d| d.insert_temp(id, k));
            vec![(k, String::new())]
        }
        Kind::Bank => {
            let id = ui.id().with("bankline");
            let (mut bank, mut what, mut art): (usize, usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0, 0));
            const WHAT: [&str; 9] = ["Shaper", "Pan", "Volume", "Cutoff", "Resonance", "Low", "Band", "High", "Shaper mix"];
            const ART: [&str; 14] = ["Pattern 1", "Pattern 2", "Pattern 3", "Pattern 4", "Pattern 5", "Pattern 6", "Pattern 7", "Pattern 8", "Pattern 9", "Pattern 10", "Slow wave", "From pad X", "From pad Y", "From input level"];
            ui.horizontal(|ui| {
                ui.label(RichText::new("Line of bank").small().color(TEXT_DIM));
                for b in 0..cereal_engine::fxbank::BANKS {
                    ui.selectable_value(&mut bank, b, format!("{}", b + 1));
                }
                egui::ComboBox::from_id_salt(id.with("what")).width(100.0).selected_text(RichText::new(WHAT[what]).small()).show_ui(ui, |ui| {
                    for (i, name) in WHAT.iter().enumerate() {
                        ui.selectable_value(&mut what, i, *name);
                    }
                });
                if what > 0 {
                    egui::ComboBox::from_id_salt(id.with("art")).width(120.0).selected_text(RichText::new(ART[art]).small()).show_ui(ui, |ui| {
                        for (i, name) in ART.iter().enumerate() {
                            ui.selectable_value(&mut art, i, *name);
                        }
                    });
                }
            });
            ui.data_mut(|d| d.insert_temp(id, (bank, what, art)));
            let k = bank * cereal_engine::fxbank::LINES + if what == 0 { 0 } else { 1 + (what - 1) * 14 + art };
            vec![(k, String::new())]
        }
        _ => vec![(0, "Shape".to_string())],
    };
    let is_bank = f.kind == Kind::Bank;
    let timed = move |k: usize| is_bank && { let r = k % cereal_engine::fxbank::LINES; r > 0 && (r - 1) % 14 < 11 };
    for (k, name) in shown {
        if !name.is_empty() {
            ui.label(RichText::new(name).small().color(TEXT_DIM));
        }
        ui.push_id(("line", k), |ui| match f.line(k) {
            Some(mut c) => {
                let kind = if timed(k) { crate::curve::Line::Envelope } else { crate::curve::Line::Mapping };
                if crate::curve::curve_editor(ui, &mut c, kind, 150.0) {
                    changed |= f.set_line(k, &c);
                }
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("This line was drawn in an old version and is played as it is.").small().color(TEXT_DIM));
                    if ui.button("Draw it afresh").clicked() {
                        let mut c = cereal_engine::fm::Curve::flat(1.0);
                        c.pts[0].y = 0.0;
                        changed |= f.set_line(k, &c);
                    }
                });
            }
        });
    }
    changed
}

/// The note units: pages of text to read and write, or a web address to keep and open.
fn notes_ui(ui: &mut Ui, f: &mut fx::Fx) -> bool {
    use cereal_engine::notes::Notes;
    let mut n = Notes::unpack(&f.p);
    let was = n.clone();
    if n.flavour == 2 {
        ui.label(RichText::new("Web address kept with the project").small().color(TEXT_DIM));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut n.pages[0].1).desired_width(380.0));
            let url = n.pages[0].1.trim().to_string();
            if ui.add_enabled(url.starts_with("http://") || url.starts_with("https://"), egui::Button::new("Open in browser")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        });
    } else {
        let mut at = n.shown();
        ui.horizontal(|ui| {
            if ui.add_enabled(at > 0, egui::Button::new("<")).on_hover_text("Previous page").clicked() {
                at -= 1;
            }
            ui.label(format!("Page {} of {}", at + 1, n.pages.len()));
            if ui.add_enabled(at + 1 < n.pages.len(), egui::Button::new(">")).on_hover_text("Next page").clicked() {
                at += 1;
            }
            if ui.button("Add page").clicked() {
                let num = n.pages.iter().map(|p| p.0).max().unwrap_or(0) + 1;
                n.pages.insert(at + 1, (num, String::new()));
                at += 1;
            }
            if ui.add_enabled(n.pages.len() > 1, egui::Button::new("Delete page")).clicked() {
                n.pages.remove(at);
                at = at.min(n.pages.len() - 1);
            }
        });
        n.active = n.pages[at].0;
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut n.pages[at].1).desired_width(460.0).desired_rows(16));
        });
        if n.flavour == 1 {
            ui.label(RichText::new("A page you change is saved as plain words; pages left alone keep their fonts and colours in the file.").small().color(TEXT_DIM));
        }
    }
    if n != was {
        f.p = n.pack();
        return true;
    }
    false
}

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

/// Where a drawn board sits on the screen: its area, the board point at its corner, and its scale.
struct BoardAt {
    area: egui::Rect,
    x0: f32,
    y0: f32,
    s: f32,
}

/// A control board with its editing bar: played with the mouse, or, with editing on, rearranged with it.
pub fn surface_ui(ui: &mut Ui, f: &mut fx::Fx, max_w: f32) -> bool {
    use cereal_engine::surface::{self, What};
    if f.raw.is_none() {
        return false;
    }
    let id = ui.id().with(("board edit", ui.next_auto_id()));
    let (mut editing, mut sel): (bool, Option<usize>) = ui.data(|d| d.get_temp(id)).unwrap_or((std::env::var_os("CEREAL_BOARD_EDIT").is_some(), Some(0)));
    let mut changed = false;
    // an edit works on the block with the current positions written in, and the positions are read back from it
    let mut apply = |f: &mut fx::Fx, edit: &dyn Fn(&[u8]) -> Vec<u8>| {
        let now = surface::encode(&f.p, f.raw.as_ref().map(|r| &r[..]));
        let new = edit(&now);
        if let Some(p) = surface::decode(&new) {
            f.p = p;
            f.raw = Some(std::sync::Arc::new(new));
            changed = true;
        }
    };
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut editing, "Edit board").on_hover_text("Move, resize, restyle, add and remove the controls. Turn it off to play them.");
        if editing {
            for (w, label) in [(What::Knob, "Knob"), (What::Slider, "Slider"), (What::Button, "Button"), (What::Check, "Switch"), (What::Digits, "Number"), (What::Pad, "Pad"), (What::Label, "Label"), (What::Bevel, "Frame")] {
                if ui.small_button(format!("+ {label}")).clicked() {
                    let n = f.raw.as_ref().map_or(0, |r| surface::controls(r).len());
                    let at = (16.0 + (n % 8) as f32 * 12.0, 16.0 + (n % 8) as f32 * 12.0);
                    apply(f, &|b| surface::add(b, w, at));
                    sel = Some(n);
                }
            }
        }
    });
    if !editing {
        let (moved, _) = board_ui(ui, f, max_w, false);
        ui.data_mut(|d| d.insert_temp(id, (editing, None::<usize>)));
        return moved || changed;
    }
    let geo = ui.add_enabled_ui(false, |ui| board_ui(ui, f, max_w, true).1).inner;
    let raw = f.raw.clone().unwrap();
    let controls = surface::controls(&raw);
    sel = sel.filter(|&i| i < controls.len());
    if let Some(g) = geo {
        let pt = ui.painter_at(g.area);
        let snap = |v: f32| (v / 2.0).round() * 2.0;
        // a click on bare board lets go of the chosen control
        if ui.interact(g.area, id.with("bare"), Sense::click()).clicked() {
            sel = None;
        }
        for (n, c) in controls.iter().enumerate() {
            let r = egui::Rect::from_min_size(g.area.min + Vec2::new((c.rect[0] - g.x0) * g.s, (c.rect[1] - g.y0) * g.s), Vec2::new(c.rect[2].max(4.0) * g.s, c.rect[3].max(4.0) * g.s));
            let resp = ui.interact(r, id.with(("move", n)), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
            if resp.clicked() || resp.drag_started() {
                sel = Some(n);
            }
            if resp.dragged() {
                let d = resp.drag_delta() / g.s;
                if d != Vec2::ZERO {
                    let acc: Vec2 = ui.data(|m| m.get_temp(id.with("carry"))).unwrap_or(Vec2::ZERO) + d;
                    let step = Vec2::new(snap(acc.x), snap(acc.y));
                    ui.data_mut(|m| m.insert_temp(id.with("carry"), acc - step));
                    if step != Vec2::ZERO {
                        let rect = [(c.rect[0] + step.x).max(0.0), (c.rect[1] + step.y).max(0.0), c.rect[2], c.rect[3]];
                        apply(f, &|b| surface::set_rect(b, n, rect));
                    }
                }
            }
            if sel == Some(n) {
                pt.rect_stroke(r, 0.0, Stroke::new(1.0_f32, ACCENT), egui::StrokeKind::Outside);
                // the corner that sizes it
                let h = egui::Rect::from_center_size(r.max, Vec2::splat(9.0));
                let hr = ui.interact(h, id.with(("size", n)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                pt.rect_filled(h, 1.0, ACCENT);
                if hr.dragged() {
                    let d = hr.drag_delta() / g.s;
                    let acc: Vec2 = ui.data(|m| m.get_temp(id.with("carry"))).unwrap_or(Vec2::ZERO) + d;
                    let step = Vec2::new(snap(acc.x), snap(acc.y));
                    ui.data_mut(|m| m.insert_temp(id.with("carry"), acc - step));
                    if step != Vec2::ZERO {
                        let rect = [c.rect[0], c.rect[1], (c.rect[2] + step.x).max(6.0), (c.rect[3] + step.y).max(6.0)];
                        apply(f, &|b| surface::set_rect(b, n, rect));
                    }
                }
            }
        }
    }
    if ui.input(|i| !i.pointer.any_down()) {
        ui.data_mut(|m| m.insert_temp(id.with("carry"), Vec2::ZERO));
    }
    // the chosen control's own settings
    if let Some((n, c)) = sel.and_then(|n| controls.get(n).map(|c| (n, c))) {
        let mut gone = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Name").small().color(TEXT_DIM));
            let mut name = c.name.clone();
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(110.0)).changed() {
                apply(f, &|b| surface::set_name(b, n, &name));
            }
            if matches!(c.what, What::Label | What::Button | What::Check) {
                ui.label(RichText::new("Text").small().color(TEXT_DIM));
                let mut cap = c.caption.clone();
                if ui.add(egui::TextEdit::singleline(&mut cap).desired_width(110.0)).changed() {
                    apply(f, &|b| surface::set_look(b, n, "Caption", &cap));
                }
                let mut size = c.font.abs();
                if ui.add(egui::DragValue::new(&mut size).range(6.0..=96.0).speed(0.25).prefix("size ")).changed() {
                    let v = format!("{}", size.round() as i32);
                    apply(f, &|b| surface::set_look(b, n, "QuickFont.Height", &v));
                }
            }
            if c.what != What::Other {
                let mut rgb = if c.what == What::Label { c.text_color } else { c.color }.unwrap_or([240, 160, 60]);
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    let v = (rgb[0] as u32 | (rgb[1] as u32) << 8 | (rgb[2] as u32) << 16).to_string();
                    apply(f, &|b| surface::set_look(b, n, surface::color_key(c.what), &v));
                }
            }
            match c.what {
                What::Button => {
                    let mut stays = c.stays;
                    if ui.checkbox(&mut stays, "Stays down").changed() {
                        apply(f, &|b| surface::set_look(b, n, "StayDown", if stays { "True" } else { "False" }));
                    }
                }
                What::Slider => {
                    let mut up = c.upright;
                    if ui.checkbox(&mut up, "Upright").changed() {
                        apply(f, &|b| surface::set_look(b, n, "Orientation", if up { "trVertical" } else { "trHorizontal" }));
                    }
                }
                What::Digits => {
                    let (mut lo, mut hi) = c.range;
                    let a = ui.add(egui::DragValue::new(&mut lo).prefix("from ")).changed();
                    let z = ui.add(egui::DragValue::new(&mut hi).prefix("to ")).changed();
                    if a || z {
                        let (lo, hi) = (format!("{}", lo.round() as i64), format!("{}", hi.round() as i64));
                        apply(f, &|b| surface::set_look(&surface::set_look(b, n, "Min", &lo), n, "Max", &hi));
                    }
                }
                _ => {}
            }
            let mut rect = c.rect;
            let mut moved = false;
            for (k, pre) in ["x ", "y ", "w ", "h "].iter().enumerate() {
                moved |= ui.add(egui::DragValue::new(&mut rect[k]).range(if k < 2 { 0.0 } else { 6.0 }..=8192.0).prefix(*pre)).changed();
            }
            if moved {
                apply(f, &|b| surface::set_rect(b, n, rect));
            }
            if ui.small_button("To front").clicked() {
                apply(f, &|b| surface::raise(b, n));
                sel = Some(controls.len() - 1);
            }
            gone = ui.small_button("Remove").clicked() || ui.input(|i| i.key_pressed(egui::Key::Delete)) && !ui.ctx().wants_keyboard_input();
        });
        if gone {
            apply(f, &|b| surface::remove(b, n));
            sel = None;
        }
    } else {
        ui.horizontal(|ui| {
            let (w, h) = surface::size(&raw);
            let (mut bw, mut bh) = (w, h);
            ui.label(RichText::new("Board").small().color(TEXT_DIM));
            let a = ui.add(egui::DragValue::new(&mut bw).range(64.0..=4096.0).prefix("w ")).changed();
            let z = ui.add(egui::DragValue::new(&mut bh).range(48.0..=4096.0).prefix("h ")).changed();
            if a || z {
                apply(f, &|b| surface::set_size(b, bw, bh));
            }
            ui.label(RichText::new("Click a control to choose it; drag it to move it, drag its corner to size it.").small().color(TEXT_DIM));
        });
    }
    ui.data_mut(|d| d.insert_temp(id, (editing, sel)));
    changed
}

/// The board itself: every stored control drawn where it sits, and played with the mouse. `full` shows the
/// whole board at its stored size instead of only the part the controls take up.
fn board_ui(ui: &mut Ui, f: &mut fx::Fx, max_w: f32, full: bool) -> (bool, Option<BoardAt>) {
    use cereal_engine::surface::{self, What};
    let Some(raw) = f.raw.clone() else { return (false, None) };
    let controls = surface::controls(&raw);
    if controls.is_empty() && !full {
        ui.label(RichText::new("This board has no controls on it yet.").small().color(TEXT_DIM));
        return (false, None);
    }
    // the part of the board the controls take up, with a little air round it
    let (x0, y0) = if full { (0.0, 0.0) } else { (controls.iter().fold(f32::MAX, |m, c| m.min(c.rect[0])) - 8.0, controls.iter().fold(f32::MAX, |m, c| m.min(c.rect[1])) - 8.0) };
    let (fw, fh) = if full { surface::size(&raw) } else { (64.0, 32.0) };
    let bw = controls.iter().fold(fw, |m, c| m.max(c.rect[0] + c.rect[2] - x0 + 8.0));
    let bh = controls.iter().fold(fh, |m, c| m.max(c.rect[1] + c.rect[3] - y0 + 8.0));
    let s = (max_w / bw).min(1.0);
    let (area, _) = ui.allocate_exact_size(Vec2::new(bw * s, bh * s), Sense::hover());
    let pt = ui.painter_at(area);
    pt.rect_filled(area, 3.0, PANEL_DARK);
    let col = |c: Option<[u8; 3]>, or: Color32| c.map_or(or, |c| Color32::from_rgb(c[0], c[1], c[2]));
    let mut changed = false;
    let base = ui.id().with(("board", std::sync::Arc::as_ptr(&raw) as usize));
    for (n, c) in controls.iter().enumerate() {
        let r = egui::Rect::from_min_size(area.min + Vec2::new((c.rect[0] - x0) * s, (c.rect[1] - y0) * s), Vec2::new(c.rect[2].max(1.0) * s, c.rect[3].max(1.0) * s));
        let get = |k: usize, p: &[f32]| c.outs.get(k).and_then(|&o| p.get(o as usize)).copied().unwrap_or(0.0);
        let mut put = |k: usize, v: f32, p: &mut Vec<f32>| {
            if let Some(slot) = c.outs.get(k).and_then(|&o| p.get_mut(o as usize)) {
                let v = v.clamp(0.0, 1.0);
                if (*slot - v).abs() > 1e-7 {
                    *slot = v;
                    changed = true;
                }
            }
        };
        let main = col(c.color, Color32::from_rgb(240, 160, 60));
        match c.what {
            What::Bevel => {
                pt.rect_filled(r, 4.0 * s, col(c.color, Color32::from_rgb(60, 64, 70)));
            }
            What::Label => {
                if !c.caption.is_empty() {
                    let size = (c.font.abs() * 0.75 * s).clamp(7.0, 40.0);
                    pt.text(r.center(), egui::Align2::CENTER_CENTER, &c.caption, egui::FontId::proportional(size), col(c.text_color, Color32::from_gray(220)));
                }
            }
            What::Knob | What::Digits => {
                let resp = ui.interact(r, base.with(n), Sense::click_and_drag());
                let mut v = get(0, &f.p);
                let steps = (c.range.1 - c.range.0).abs().max(1.0);
                if resp.dragged() {
                    let fine = if ui.input(|i| i.modifiers.ctrl) { 0.1 } else { 1.0 };
                    v -= resp.drag_delta().y / 160.0 * fine;
                }
                if resp.hovered() {
                    let turn = ui.input(|i| i.smooth_scroll_delta.y);
                    if turn != 0.0 {
                        v += if c.what == What::Digits { turn.signum() / steps } else { turn / 1200.0 };
                    }
                }
                if resp.double_clicked() || resp.clicked_by(egui::PointerButton::Middle) {
                    v = c.rest.first().copied().unwrap_or(0.5);
                }
                put(0, v, &mut f.p);
                let v = get(0, &f.p);
                if c.what == What::Digits {
                    pt.rect_filled(r, 3.0 * s, col(c.color, Color32::from_rgb(40, 44, 50)));
                    let shown = (c.range.0 + (c.range.1 - c.range.0) * v).round();
                    pt.text(r.center(), egui::Align2::CENTER_CENTER, format!("{shown:.0}"), egui::FontId::monospace((r.height() * 0.6).clamp(8.0, 40.0)), col(c.text_color, Color32::from_gray(230)));
                } else {
                    let rad = r.width().min(r.height()) / 2.0 - 1.0;
                    pt.circle_filled(r.center(), rad, Color32::from_rgb(52, 56, 62));
                    let ang = |t: f32| std::f32::consts::PI * (0.75 + 1.5 * t);
                    let pts: Vec<Pos2> = (0..=24).map(|k| ang(v * k as f32 / 24.0)).map(|a| r.center() + Vec2::new(a.cos(), a.sin()) * rad).collect();
                    pt.add(egui::Shape::line(pts, Stroke::new((2.0 * s).max(1.5), main)));
                    let a = ang(v);
                    pt.line_segment([r.center() + Vec2::new(a.cos(), a.sin()) * rad * 0.35, r.center() + Vec2::new(a.cos(), a.sin()) * rad * 0.85], Stroke::new((2.0 * s).max(1.5), Color32::from_gray(235)));
                }
                resp.on_hover_text(format!("{}: {:.0} %", c.name, v * 100.0));
            }
            What::Slider => {
                let resp = ui.interact(r, base.with(n), Sense::click_and_drag());
                let upright = c.upright && r.height() >= r.width() || !c.upright && r.height() > r.width() * 1.5;
                if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.is_pointer_button_down_on()) {
                    let v = if upright { (r.bottom() - pos.y) / r.height() } else { (pos.x - r.left()) / r.width() };
                    put(0, v, &mut f.p);
                }
                if resp.double_clicked() || resp.clicked_by(egui::PointerButton::Middle) {
                    put(0, c.rest.first().copied().unwrap_or(0.5), &mut f.p);
                }
                let v = get(0, &f.p);
                let rail = if upright { egui::Rect::from_center_size(r.center(), Vec2::new((r.width() * 0.3).max(3.0), r.height())) } else { egui::Rect::from_center_size(r.center(), Vec2::new(r.width(), (r.height() * 0.3).max(3.0))) };
                pt.rect_filled(rail, 2.0, Color32::from_rgb(40, 44, 50));
                let (fill, knob) = if upright {
                    let y = r.bottom() - v * r.height();
                    (egui::Rect::from_min_max(Pos2::new(rail.left(), y), rail.max), egui::Rect::from_center_size(Pos2::new(r.center().x, y), Vec2::new(r.width(), (6.0 * s).max(4.0))))
                } else {
                    let x = r.left() + v * r.width();
                    (egui::Rect::from_min_max(rail.min, Pos2::new(x, rail.bottom())), egui::Rect::from_center_size(Pos2::new(x, r.center().y), Vec2::new((6.0 * s).max(4.0), r.height())))
                };
                pt.rect_filled(fill, 2.0, main);
                pt.rect_filled(knob.intersect(area), 2.0, Color32::from_gray(225));
                resp.on_hover_text(format!("{}: {:.0} %", c.name, v * 100.0));
            }
            What::Button | What::Check => {
                let resp = ui.interact(r, base.with(n), Sense::click_and_drag());
                let on = get(0, &f.p) >= 0.5;
                if c.what == What::Check || c.stays {
                    if resp.clicked() {
                        put(0, if on { 0.0 } else { 1.0 }, &mut f.p);
                    }
                } else {
                    // held only while it is pressed
                    let down = resp.is_pointer_button_down_on();
                    if down != on {
                        put(0, if down { 1.0 } else { 0.0 }, &mut f.p);
                    }
                }
                let on = get(0, &f.p) >= 0.5;
                let lit = if on { main } else { main.linear_multiply(0.35) };
                if c.what == What::Check {
                    let d = r.height().min(r.width()) * 0.7;
                    pt.circle_filled(Pos2::new(r.left() + d / 2.0 + 1.0, r.center().y), d / 2.0, lit);
                    if !c.caption.is_empty() {
                        pt.text(Pos2::new(r.left() + d + 5.0, r.center().y), egui::Align2::LEFT_CENTER, &c.caption, egui::FontId::proportional((c.font.abs() * s).clamp(7.0, 24.0)), col(c.text_color, Color32::from_gray(220)));
                    }
                } else {
                    pt.rect_filled(r, 4.0 * s, lit);
                    if !c.caption.is_empty() {
                        pt.text(r.center(), egui::Align2::CENTER_CENTER, &c.caption, egui::FontId::proportional((c.font.abs() * s).clamp(7.0, 24.0)), col(c.text_color, Color32::from_gray(20)));
                    }
                }
                resp.on_hover_text(c.name.clone());
            }
            What::Pad => {
                let resp = ui.interact(r, base.with(n), Sense::click_and_drag());
                if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.is_pointer_button_down_on()) {
                    put(0, (pos.x - r.left()) / r.width(), &mut f.p);
                    put(1, (r.bottom() - pos.y) / r.height(), &mut f.p);
                }
                let (x, y) = (get(0, &f.p), get(1, &f.p));
                pt.rect_filled(r, 3.0, col(c.color, Color32::from_rgb(24, 28, 32)));
                let at = Pos2::new(r.left() + x * r.width(), r.bottom() - y * r.height());
                pt.line_segment([Pos2::new(at.x, r.top()), Pos2::new(at.x, r.bottom())], Stroke::new(1.0_f32, Color32::from_white_alpha(40)));
                pt.line_segment([Pos2::new(r.left(), at.y), Pos2::new(r.right(), at.y)], Stroke::new(1.0_f32, Color32::from_white_alpha(40)));
                pt.circle_filled(at, (6.0 * s).max(3.0), Color32::from_gray(235));
                resp.on_hover_text(format!("{}: X {:.0} %, Y {:.0} %", c.name, x * 100.0, y * 100.0));
            }
            What::Other => {
                pt.rect_stroke(r, 2.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
            }
        }
    }
    (changed, Some(BoardAt { area, x0, y0, s }))
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
    // the boards first: they are what the bay is played from
    for i in 0..sub.nodes.len() {
        if sub.nodes[i].fx.as_ref().is_some_and(|g| g.kind == Kind::Surface) {
            let mut board = sub.nodes[i].fx.clone().unwrap();
            if surface_ui(ui, &mut board, 520.0) {
                std::sync::Arc::make_mut(sub).nodes[i].fx = Some(board);
                changed = true;
            }
        }
    }
    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
        for at in 0..sub.order.len() {
            let i = sub.order[at];
            let head = format!("{}. {}", i + 1, title(&sub.nodes[i]));
            if sub.nodes[i].notes.is_some() {
                let head = format!("{}. {}", i + 1, note_unit_title(&sub.nodes[i].ident));
                egui::CollapsingHeader::new(RichText::new(head).small()).id_salt(("baynote", i)).show(ui, |ui| {
                    let ident = sub.nodes[i].ident.clone();
                    let mut block = sub.nodes[i].notes.clone().unwrap();
                    if note_unit_ui(ui, &ident, &mut block) && std::sync::Arc::make_mut(sub).set_note_block(i, block) {
                        changed = true;
                    }
                });
                continue;
            }
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

/// Our names for the units that work on notes.
pub fn note_unit_title(ident: &str) -> &'static str {
    match ident.to_ascii_lowercase().as_str() {
        "vfx key mapper" => "Key Remapper",
        "vfx keyboard splitter" => "Keyboard Zones",
        "vfx color mapper" => "Colour Router",
        "vfx level scaler" => "Level Scaler",
        "vfx envelope" => "Note Mover",
        "vfx script" => "Note Script",
        _ => "Note unit",
    }
}

/// The settings of a unit that works on notes, edited in its stored block. True when the block changed.
pub fn note_unit_ui(ui: &mut Ui, ident: &str, block: &mut Vec<u8>) -> bool {
    use cereal_engine::noteshape::{Shaper, KEYS};
    const NOTE: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let Some(mut s) = Shaper::decode(ident, block) else { return false };
    let before = s.clone();
    let mut lines = false;
    match &mut s {
        Shaper::Keys { on, offset, mode, glide, rows } => {
            ui.horizontal(|ui| {
                ui.checkbox(on, "On");
                ui.label("Offset");
                ui.add(egui::DragValue::new(offset).range(-12..=12));
                ui.label("Several targets");
                const MODES: [&str; 5] = ["All", "All, one loudness", "One at random", "Next up", "Next down"];
                egui::ComboBox::from_id_salt("km_mode").selected_text(MODES[*mode as usize % 5]).show_ui(ui, |ui| {
                    for (k, n) in MODES.iter().enumerate() {
                        ui.selectable_value(mode, k as u8, *n);
                    }
                });
                ui.label("Glide");
                ui.add(egui::DragValue::new(glide).range(0.0..=64.0).speed(0.05).suffix(" steps"));
            });
            // one row per key of the octave: the keys it plays, counted from the octave's first key
            for (k, row) in rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_sized([26.0, 16.0], egui::Label::new(RichText::new(NOTE[k]).small()));
                    let mut drop = None;
                    for (j, t) in row.iter_mut().enumerate() {
                        let r = ui.add(egui::DragValue::new(t).range(-48..=60).custom_formatter(|v, _| {
                            let v = v as i32;
                            format!("{}{:+}", NOTE[v.rem_euclid(12) as usize], v.div_euclid(12))
                        }).custom_parser(|t| t.trim().parse::<f64>().ok()));
                        if r.on_hover_text("Drag to change the key it plays; right-click to take it out").secondary_clicked() {
                            drop = Some(j);
                        }
                    }
                    if let Some(j) = drop {
                        row.remove(j);
                    }
                    if row.len() < 64 && ui.small_button("+").on_hover_text("Play one more key").clicked() {
                        row.push(k as i16);
                    }
                });
            }
        }
        Shaper::Zones(zones) => {
            let id = ui.id().with("zone-pick");
            let mut pick: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0).min(zones.len().saturating_sub(1));
            ui.horizontal_wrapped(|ui| {
                for k in 0..zones.len() {
                    let used = zones[k].keys.iter().any(|v| *v > 0.0);
                    let text = RichText::new(format!("{}", k + 1)).small();
                    if ui.selectable_label(pick == k, if used { text.strong() } else { text.color(TEXT_DIM) }).clicked() {
                        pick = k;
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(id, pick));
            if let Some(z) = zones.get_mut(pick) {
                ui.horizontal(|ui| {
                    ui.label("Transpose");
                    ui.add(egui::DragValue::new(&mut z.transpose).range(-48..=48).suffix(" keys"));
                    ui.checkbox(&mut z.absolute, "Absolute velocity");
                    ui.checkbox(&mut z.random, "Random");
                });
                let heads = Shaper::line_heads(ident, block);
                for (which, label) in [(0usize, format!("Keys (C0 at the left, {KEYS} keys across): how much of the zone each key gets")), (1, "Velocity: what each velocity becomes".to_string())] {
                    ui.label(RichText::new(label).small().color(TEXT_DIM));
                    let Some(&head) = heads.get(2 * pick + which) else { continue };
                    match cereal_engine::fm::Curve::read_at(block, head) {
                        Some(mut c) => {
                            ui.push_id(("zone-line", pick, which), |ui| {
                                if crate::curve::curve_editor(ui, &mut c, crate::curve::Line::Mapping, 90.0) && cereal_engine::fm::Curve::replace_at(block, head, &c) {
                                    lines = true;
                                }
                            });
                        }
                        None => {
                            ui.label(RichText::new("stored the old way; not editable here").small().color(TEXT_DIM));
                        }
                    }
                }
            }
        }
        Shaper::Colours { port, colour } => {
            egui::Grid::new("colour-router").spacing([6.0, 2.0]).show(ui, |ui| {
                ui.label(RichText::new("Colour").small().color(TEXT_DIM));
                ui.label(RichText::new("Leaves by").small().color(TEXT_DIM));
                ui.label(RichText::new("As colour").small().color(TEXT_DIM));
                ui.end_row();
                for k in 0..16 {
                    ui.label(RichText::new(format!("{}", k + 1)).small());
                    let mut p = port[k] as i32 + 1;
                    ui.add(egui::DragValue::new(&mut p).range(1..=16).prefix("output "));
                    port[k] = (p - 1) as u8;
                    let mut c = colour[k] as i32 + 1;
                    ui.add(egui::DragValue::new(&mut c).range(1..=16));
                    colour[k] = (c - 1) as u8;
                    ui.end_row();
                }
            });
        }
        Shaper::Levels(levels) => {
            const NAMES: [&str; 6] = ["Velocity", "Release", "Fine pitch", "Pan", "Mod X", "Mod Y"];
            const MODES: [&str; 3] = ["Add", "Both ways", "Take away"];
            egui::Grid::new("level-scaler").spacing([6.0, 2.0]).show(ui, |ui| {
                for h in ["", "Centre", "Multiply", "Tension", "Offset", "Random", ""] {
                    ui.label(RichText::new(h).small().color(TEXT_DIM));
                }
                ui.end_row();
                for (k, l) in levels.iter_mut().enumerate() {
                    ui.label(RichText::new(NAMES[k]).small());
                    let pc = |ui: &mut Ui, v: &mut f32, lo: f32, hi: f32| {
                        let mut x = *v * 100.0;
                        if ui.add(egui::DragValue::new(&mut x).range(lo..=hi).speed(0.5).suffix(" %").max_decimals(1)).changed() {
                            *v = x / 100.0;
                        }
                    };
                    pc(ui, &mut l.centre, 0.0, 100.0);
                    pc(ui, &mut l.multiply, -400.0, 400.0);
                    pc(ui, &mut l.tension, -100.0, 100.0);
                    pc(ui, &mut l.offset, -100.0, 100.0);
                    pc(ui, &mut l.random, 0.0, 100.0);
                    egui::ComboBox::from_id_salt(("ls_mode", k)).width(80.0).selected_text(MODES[l.mode as usize % 3]).show_ui(ui, |ui| {
                        for (j, n) in MODES.iter().enumerate() {
                            ui.selectable_value(&mut l.mode, j as u8, *n);
                        }
                    });
                    ui.end_row();
                }
            });
        }
        Shaper::Motion(m) => {
            use cereal_engine::fm::Curve;
            use cereal_engine::noteshape::Motion;
            let pieces = Motion::pieces(block);
            let id = ui.id().with("mover-pick");
            let (mut art, mut part): (usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0));
            art = art.min(pieces.len().saturating_sub(1));
            ui.horizontal_wrapped(|ui| {
                for k in 0..pieces.len() {
                    let used = m.arts.get(k).is_some_and(|a| a.parts.iter().any(|p| p.env_on || p.lfo_on));
                    let text = RichText::new(format!("{}", k + 1)).small();
                    if ui.selectable_label(art == k, if used { text.strong() } else { text.color(TEXT_DIM) }).clicked() {
                        art = k;
                    }
                }
            });
            let int = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0);
            let put = |d: &mut [u8], o: usize, v: [u8; 4]| {
                if let Some(t) = d.get_mut(o..o + 4) {
                    t.copy_from_slice(&v);
                }
            };
            if let Some((head, parts)) = pieces.get(art).cloned() {
                // which notes this mover answers to
                let mut d = block[head.clone()].to_vec();
                let was = d.clone();
                ui.horizontal(|ui| {
                    let (mut c, mut lo, mut hi) = (int(&d, 4) + 1, int(&d, 8), int(&d, 12));
                    ui.label("Note colour");
                    ui.add(egui::DragValue::new(&mut c).range(1..=16));
                    ui.label("Keys");
                    let name = |v: f64, _: std::ops::RangeInclusive<usize>| format!("{}{}", NOTE[(v as i32).rem_euclid(12) as usize], (v as i32).div_euclid(12));
                    ui.add(egui::DragValue::new(&mut lo).range(0..=131).custom_formatter(name));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut hi).range(0..=131).custom_formatter(name));
                    put(&mut d, 4, (c - 1).to_le_bytes());
                    put(&mut d, 8, lo.to_le_bytes());
                    put(&mut d, 12, hi.to_le_bytes());
                });
                if d != was {
                    *block = Motion::splice(block, head, &d);
                    lines = true;
                }
                const PARTS: [&str; 5] = ["Loudness", "Pitch", "Pan", "Mod X", "Mod Y"];
                ui.horizontal(|ui| {
                    for (k, n) in PARTS.iter().enumerate() {
                        let on = m.arts.get(art).and_then(|a| a.parts.get(k)).is_some_and(|p| p.env_on || p.lfo_on);
                        let text = RichText::new(*n).small();
                        if ui.selectable_label(part == k, if on { text.strong() } else { text }).clicked() {
                            part = k;
                        }
                    }
                });
                // the head edit above never moves the pieces after it when nothing changed; read them afresh if it did
                let parts = if lines { Motion::pieces(block).get(art).map(|a| a.1.clone()).unwrap_or(parts) } else { parts };
                match parts.get(part.min(4)).cloned().flatten() {
                    None => {
                        ui.label(RichText::new("This property holds no lines in the stored unit.").small().color(TEXT_DIM));
                    }
                    Some(r) => {
                        let mut d = block[r.clone()].to_vec();
                        let was = d.clone();
                        let flt = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0.0);
                        ui.horizontal(|ui| {
                            for (o, label, lo) in [(4usize, "Rest", 0.0f32), (8, "Amount", -100.0), (12, "Wave share", -100.0)] {
                                let mut v = flt(&d, o) * 100.0;
                                ui.label(RichText::new(label).small());
                                if ui.add(egui::DragValue::new(&mut v).range(lo..=100.0).speed(0.5).suffix(" %").max_decimals(1)).changed() {
                                    put(&mut d, o, (v / 100.0).to_le_bytes());
                                }
                            }
                        });
                        if let Some(mut env) = Curve::read_at(&d, 16) {
                            let e = env.span.1;
                            let mut on = int(&d, e - 16) != 0;
                            if ui.checkbox(&mut on, "Line").changed() {
                                put(&mut d, e - 16, (on as i32).to_le_bytes());
                            }
                            let done = ui.push_id(("mover-env", art, part), |ui| crate::curve::curve_editor(ui, &mut env, crate::curve::Line::Envelope, 90.0)).inner;
                            if done {
                                Curve::replace_at(&mut d, 16, &env);
                            }
                        }
                        // the slow wave sits behind the line, wherever the line now ends
                        if let Some(e) = Curve::read_at(&d, 16).map(|c| c.span.1) {
                            if let Some(mut lfo) = Curve::read_at(&d, e + 16) {
                                let l = lfo.span.1;
                                ui.horizontal(|ui| {
                                    let mut on = int(&d, l - 16) != 0;
                                    if ui.checkbox(&mut on, "Slow wave").changed() {
                                        put(&mut d, l - 16, (on as i32).to_le_bytes());
                                    }
                                    let mut sp = int(&d, l).clamp(0, 65536);
                                    let hz = |v: i32| 0.05 * (v as f32 / 65536.0 * 11.0).exp2();
                                    if ui.add(egui::Slider::new(&mut sp, 0..=65536).custom_formatter(move |v, _| format!("{:.2} Hz", hz(v as i32)))).changed() {
                                        put(&mut d, l, sp.to_le_bytes());
                                    }
                                });
                                let done = ui.push_id(("mover-lfo", art, part), |ui| crate::curve::curve_editor(ui, &mut lfo, crate::curve::Line::Envelope, 70.0)).inner;
                                if done {
                                    Curve::replace_at(&mut d, e + 16, &lfo);
                                }
                            }
                        }
                        if d != was {
                            *block = Motion::splice(block, r, &d);
                            lines = true;
                        }
                    }
                }
            }
            ui.data_mut(|d| d.insert_temp(id, (art, part)));
        }
        Shaper::Script(_) => {
            ui.label(RichText::new("The program and its knobs are shown in the channel's own window.").small().color(TEXT_DIM));
        }
    }
    if s != before {
        if let Some(b) = s.encode(ident, block) {
            *block = b;
            return true;
        }
    }
    lines
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
