//! The line editor shared by everything that is drawn as a line of points: envelopes with their section markers,
//! slow-oscillator shapes and mappings.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use cereal_engine::fm::{Curve, Pt};

use crate::widgets::*;

/// What kind of line is being drawn. Envelopes run in seconds and push later points along when one is moved;
/// mappings run from one end to the other and keep their ends where they are.
#[derive(Clone, Copy, PartialEq)]
pub enum Line {
    Envelope,
    Mapping,
}

fn shape(u: f32, tension: f32) -> f32 {
    let k = (tension.clamp(-1.0, 1.0).abs() * 3.0).exp2();
    if tension.abs() < 1e-4 {
        u
    } else if tension > 0.0 {
        1.0 - (1.0 - u).powf(k)
    } else {
        u.powf(k)
    }
}

/// Draws the line and lets it be edited. Drag a point to move it, drag the small ring between two points to bend
/// that stretch, right-click empty space to add a point, right-click a point for its menu. Returns true when the
/// line changed.
pub fn curve_editor(ui: &mut Ui, c: &mut Curve, line: Line, height: f32) -> bool {
    let mut changed = false;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(240.0), height), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, PANEL_DARK);
    if c.pts.is_empty() {
        c.pts.push(Pt::default());
        changed = true;
    }
    let abs = |c: &Curve| -> Vec<f32> {
        let mut acc = 0.0;
        c.pts.iter().enumerate().map(|(i, p)| { if i > 0 { acc += p.x.max(0.0); } acc }).collect()
    };
    let at = abs(c);
    let total = at.last().copied().unwrap_or(0.0);
    // the view keeps its width while a point is dragged, so the picture does not run away under the pointer
    let view_id = resp.id.with("view");
    let dragging = resp.dragged() || resp.drag_started();
    let fresh = match line {
        Line::Envelope => (total * 1.25).max(0.25),
        Line::Mapping => total.max(1e-6),
    };
    let view: f32 = if dragging { ui.data(|d| d.get_temp(view_id)).unwrap_or(fresh) } else { fresh };
    ui.data_mut(|d| d.insert_temp(view_id, view));
    let inner = rect.shrink2(Vec2::new(6.0, 8.0));
    let to = |t: f32, y: f32| Pos2::new(inner.left() + t / view * inner.width(), inner.bottom() - y.clamp(0.0, 1.0) * inner.height());
    let from = |pos: Pos2| (((pos.x - inner.left()) / inner.width()).max(0.0) * view, ((inner.bottom() - pos.y) / inner.height()).clamp(0.0, 1.0));

    // grid
    for k in 1..4 {
        let y = inner.top() + inner.height() * k as f32 / 4.0;
        p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0_f32, LINE_SOFT.gamma_multiply(0.5)));
    }
    if line == Line::Envelope {
        let step = [0.01f32, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0].into_iter().find(|s| view / s <= 12.0).unwrap_or(120.0);
        let mut t = step;
        while t < view {
            let x = to(t, 0.0).x;
            p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, LINE_SOFT.gamma_multiply(0.5)));
            p.text(Pos2::new(x + 2.0, rect.top() + 1.0), egui::Align2::LEFT_TOP, if step < 1.0 { format!("{t:.2}") } else { format!("{t:.0} s") }, egui::FontId::proportional(9.0), TEXT_DIM);
            t += step;
        }
    }
    // section markers
    if line == Line::Envelope {
        for (idx, name, col) in [(c.decay, "D", Color32::from_rgb(120, 170, 230)), (c.loop_start, "L", Color32::from_rgb(120, 210, 140)), (c.sustain, "S", Color32::from_rgb(235, 190, 90))] {
            if idx >= 0 && (idx as usize) < at.len() {
                let x = to(at[idx as usize], 0.0).x;
                p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, col.gamma_multiply(0.6)));
                p.text(Pos2::new(x + 2.0, rect.bottom() - 11.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(9.0), col);
            }
        }
    }
    // the line itself
    let mut pts = Vec::with_capacity(c.pts.len() * 16);
    for i in 0..c.pts.len() {
        if i == 0 {
            pts.push(to(at[0], c.pts[0].y));
            continue;
        }
        let (a, b) = (&c.pts[i - 1], &c.pts[i]);
        if b.mode == 2 {
            pts.push(to(at[i], a.y));
            pts.push(to(at[i], b.y));
        } else {
            for s in 1..=16 {
                let u = s as f32 / 16.0;
                pts.push(to(at[i - 1] + (at[i] - at[i - 1]) * u, a.y + (b.y - a.y) * shape(u, b.tension)));
            }
        }
    }
    if let Some(last) = pts.last().copied() {
        pts.push(Pos2::new(rect.right(), last.y));
    }
    p.add(egui::Shape::line(pts, Stroke::new(1.5_f32, ACCENT)));
    let mid = |c: &Curve, at: &[f32], i: usize| {
        let (a, b) = (&c.pts[i - 1], &c.pts[i]);
        to((at[i - 1] + at[i]) * 0.5, a.y + (b.y - a.y) * shape(0.5, b.tension))
    };
    for i in 1..c.pts.len() {
        if c.pts[i].mode != 2 && at[i] - at[i - 1] > 1e-6 {
            p.circle_stroke(mid(c, &at, i), 3.0, Stroke::new(1.0_f32, TEXT_DIM));
        }
    }
    for i in 0..c.pts.len() {
        p.circle_filled(to(at[i], c.pts[i].y), 3.5, Color32::WHITE);
    }

    // what is under the pointer: a point (index) or a bend ring (index + 10000)
    let pick = |pos: Pos2, c: &Curve, at: &[f32]| -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for i in 0..c.pts.len() {
            let d = to(at[i], c.pts[i].y).distance(pos);
            if d < 9.0 && best.is_none_or(|b| d < b.1) {
                best = Some((i, d));
            }
        }
        if best.is_none() {
            for i in 1..c.pts.len() {
                if c.pts[i].mode != 2 && mid(c, at, i).distance(pos) < 8.0 {
                    return Some(i + 10_000);
                }
            }
        }
        best.map(|b| b.0)
    };
    let grab_id = resp.id.with("grab");
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let g = resp.interact_pointer_pos().and_then(|pos| pick(pos, c, &at));
        ui.data_mut(|d| d.insert_temp(grab_id, g));
    }
    if resp.dragged_by(egui::PointerButton::Primary) {
        let g: Option<usize> = ui.data(|d| d.get_temp::<Option<usize>>(grab_id)).flatten();
        if let (Some(g), Some(pos)) = (g, resp.interact_pointer_pos()) {
            if g >= 10_000 {
                let i = g - 10_000;
                if i < c.pts.len() {
                    let rising = c.pts[i].y >= c.pts[i - 1].y;
                    let dy = -resp.drag_delta().y / inner.height() * 3.0;
                    c.pts[i].tension = (c.pts[i].tension + if rising { dy } else { -dy }).clamp(-1.0, 1.0);
                    changed = true;
                }
            } else if g < c.pts.len() {
                let (t, y) = from(pos);
                c.pts[g].y = y;
                if g > 0 {
                    let lo = at[g - 1];
                    match line {
                        Line::Envelope => c.pts[g].x = (t - lo).max(0.0),
                        Line::Mapping => {
                            if g + 1 < c.pts.len() {
                                let t = t.clamp(lo, at[g + 1]);
                                c.pts[g].x = t - lo;
                                c.pts[g + 1].x = at[g + 1] - t;
                            }
                        }
                    }
                }
                changed = true;
            }
        }
    }
    if resp.double_clicked() {
        // a double click on a bend ring straightens that stretch
        if let Some(g) = resp.interact_pointer_pos().and_then(|pos| pick(pos, c, &at)) {
            if g >= 10_000 && g - 10_000 < c.pts.len() {
                c.pts[g - 10_000].tension = 0.0;
                changed = true;
            }
        }
    }
    let menu_id = resp.id.with("menu");
    if resp.secondary_clicked() {
        let pos = resp.interact_pointer_pos();
        let g = pos.and_then(|pos| pick(pos, c, &at)).filter(|g| *g < 10_000);
        ui.data_mut(|d| d.insert_temp(menu_id, g));
        if let (None, Some(pos)) = (g, pos) {
            // empty space: a new point goes in where the pointer is
            let (t, y) = from(pos);
            let i = at.iter().position(|a| *a > t).unwrap_or(at.len());
            if i == 0 {
                // nothing sits before the first point
            } else if i == at.len() {
                if line == Line::Envelope {
                    c.pts.push(Pt { x: t - total, y, ..Default::default() });
                    changed = true;
                }
            } else {
                let next = at[i] - t;
                c.pts.insert(i, Pt { x: t - at[i - 1], y, ..Default::default() });
                c.pts[i + 1].x = next;
                for m in [&mut c.decay, &mut c.loop_start, &mut c.sustain] {
                    if *m >= i as i32 {
                        *m += 1;
                    }
                }
                changed = true;
            }
        }
    }
    let target: Option<usize> = ui.data(|d| d.get_temp::<Option<usize>>(menu_id)).flatten();
    resp.context_menu(|ui| {
        let Some(i) = target.filter(|i| *i < c.pts.len()) else {
            ui.close_menu();
            return;
        };
        let ends = i == 0 || (line == Line::Mapping && i + 1 == c.pts.len());
        if ui.add_enabled(!ends && c.pts.len() > 2, egui::Button::new("Delete")).clicked() {
            let gone = c.pts.remove(i);
            if let Some(n) = c.pts.get_mut(i) {
                n.x += gone.x;
            }
            for m in [&mut c.decay, &mut c.loop_start, &mut c.sustain] {
                if *m == i as i32 {
                    *m = -1;
                } else if *m > i as i32 {
                    *m -= 1;
                }
            }
            changed = true;
            ui.close_menu();
            return;
        }
        if i > 0 {
            ui.separator();
            if ui.radio(c.pts[i].mode != 2, "Curved stretch").clicked() {
                c.pts[i].mode = 0;
                changed = true;
                ui.close_menu();
            }
            if ui.radio(c.pts[i].mode == 2, "Hold, then jump").clicked() {
                c.pts[i].mode = 2;
                changed = true;
                ui.close_menu();
            }
        }
        if line == Line::Envelope {
            ui.separator();
            let v = i as i32;
            for (m, name) in [(&mut c.decay, "Decay starts here"), (&mut c.loop_start, "Loop starts here"), (&mut c.sustain, "Sustain / loop end")] {
                let mut on = *m == v;
                if ui.checkbox(&mut on, name).clicked() {
                    *m = if on { v } else { -1 };
                    changed = true;
                    ui.close_menu();
                }
            }
        }
    });
    if c.loop_start >= 0 && c.sustain >= 0 && c.loop_start > c.sustain {
        c.loop_start = c.sustain;
    }
    changed
}
