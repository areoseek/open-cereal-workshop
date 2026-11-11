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
        egui::Window::new(title).id(egui::Id::new("fxwin")).open(&mut open).frame(frame).resizable(false).default_pos([420.0, 160.0]).show(ctx, |ui| {
            let s = &mut self.song.inserts[ins].slots[si];
            ui.horizontal(|ui| {
                if led(ui, s.enabled, 12.0).on_hover_text("Effect on / off").clicked() {
                    s.enabled = !s.enabled;
                    changed = true;
                }
                changed |= knob(ui, &mut s.mix, 0.0, 1.0, 1.0, 20.0).on_hover_text(format!("Mix level: {:.0} %", s.mix * 100.0)).changed();
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
            let per_row = if f.kind == Kind::Eq { 7 } else { 6 };
            let rows = d.params.len().div_ceil(per_row);
            for r in 0..rows {
                ui.horizontal(|ui| {
                    for k in r * per_row..((r + 1) * per_row).min(d.params.len()) {
                        let p = &d.params[k];
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
                                r.on_hover_text(hint(d.kind, k, p.name, *v));
                                ui.label(RichText::new(p.name).small().color(TEXT_DIM));
                            });
                        });
                    }
                });
            }
        });
        if changed {
            self.dirty = true;
        }
        if !open {
            self.fx_open = None;
        }
    }
}

/// The equaliser's response picture with a handle per band. Returns true when a handle moved a setting.
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
