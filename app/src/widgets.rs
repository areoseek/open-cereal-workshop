//! Small hand-drawn controls and the colours shared by every window.

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

pub const BG: Color32 = Color32::from_rgb(52, 60, 65);
pub const PANEL: Color32 = Color32::from_rgb(63, 72, 78);
pub const PANEL_DARK: Color32 = Color32::from_rgb(40, 47, 51);
pub const GRID_DARK: Color32 = Color32::from_rgb(46, 54, 59);
pub const GRID_LIGHT: Color32 = Color32::from_rgb(54, 63, 68);
/// The dark and the light shade of the playlist and piano roll grid: the usual ones, or the user's colour.
pub fn grid_colours(own: Option<[u8; 3]>) -> (Color32, Color32) {
    match own {
        Some([r, g, b]) => (Color32::from_rgb(r, g, b), Color32::from_rgb(r.saturating_add(8), g.saturating_add(9), b.saturating_add(9))),
        None => (GRID_DARK, GRID_LIGHT),
    }
}
pub const LINE: Color32 = Color32::from_rgb(33, 39, 43);
pub const LINE_SOFT: Color32 = Color32::from_rgb(43, 50, 55);
pub const TEXT: Color32 = Color32::from_rgb(205, 212, 216);
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 150, 156);
pub const ACCENT: Color32 = Color32::from_rgb(255, 160, 60);
pub const GREEN: Color32 = Color32::from_rgb(120, 230, 110);
pub const NOTE: Color32 = Color32::from_rgb(150, 215, 160);
pub const STEP_A: Color32 = Color32::from_rgb(92, 101, 106);
pub const STEP_B: Color32 = Color32::from_rgb(112, 88, 88);
pub const STEP_A_ON: Color32 = Color32::from_rgb(205, 214, 219);
pub const STEP_B_ON: Color32 = Color32::from_rgb(232, 200, 196);

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// A rotary knob: drag up and down, double-click to reset. Ctrl slows the drag.
pub fn knob(ui: &mut Ui, v: &mut f32, min: f32, max: f32, default: f32, d: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::splat(d), Sense::click_and_drag());
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.command) { 0.2 } else { 1.0 };
        *v = (*v - resp.drag_delta().y * (max - min) / 160.0 * fine).clamp(min, max);
        resp.mark_changed();
    }
    if resp.double_clicked() {
        *v = default;
        resp.mark_changed();
    }
    let p = ui.painter();
    let c = rect.center();
    let r = d * 0.5 - 1.0;
    p.circle_filled(c, r, PANEL_DARK);
    p.circle_stroke(c, r, Stroke::new(1.0_f32, LINE));
    let t = ((*v - min) / (max - min).max(1e-9)).clamp(0.0, 1.0);
    let a = (-225.0 + t * 270.0f32).to_radians();
    let tip = c + Vec2::new(a.cos(), a.sin()) * (r - 1.0);
    let base = c + Vec2::new(a.cos(), a.sin()) * (r * 0.25);
    p.line_segment([base, tip], Stroke::new(1.6_f32, if resp.hovered() || resp.dragged() { ACCENT } else { TEXT }));
    resp
}

/// A round on/off light. Returns the response; the caller flips the value.
/// What the right-click menu of a control asked for.
#[derive(Clone, Copy, PartialEq)]
pub enum CtlAsk {
    AutoClip,
    Events,
    Link,
}

/// The right-click menu every automatable control has. The choice is left for the app to pick up after the
/// frame, since the control itself is usually borrowed from the song while it is drawn.
pub fn control_menu(resp: &Response, target: cereal_engine::song::Target) {
    resp.context_menu(|ui| {
        let mut ask = None;
        if ui.button("Create automation clip").clicked() {
            ask = Some(CtlAsk::AutoClip);
        }
        if ui.button("Edit events").clicked() {
            ask = Some(CtlAsk::Events);
        }
        if ui.button("Link to controller...").clicked() {
            ask = Some(CtlAsk::Link);
        }
        if let Some(a) = ask {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ctl_ask"), Some((target, a))));
            ui.close_menu();
        }
    });
}

pub fn led(ui: &mut Ui, on: bool, d: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(d), Sense::click());
    let p = ui.painter();
    p.circle_filled(rect.center(), d * 0.32, if on { GREEN } else { PANEL_DARK });
    p.circle_stroke(rect.center(), d * 0.32, Stroke::new(1.0_f32, LINE));
    resp
}

/// A vertical fader with a stereo peak meter behind it. `v` runs 0..=max.
pub fn fader(ui: &mut Ui, v: &mut f32, max: f32, default: f32, size: Vec2, peak: (f32, f32)) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.command) { 0.2 } else { 1.0 };
        *v = (*v - resp.drag_delta().y / size.y * max * fine).clamp(0.0, max);
        resp.mark_changed();
    }
    if resp.double_clicked() {
        *v = default;
        resp.mark_changed();
    }
    let p = ui.painter();
    p.rect_filled(rect, 2.0, PANEL_DARK);
    let level = |a: f32| -> f32 {
        if a <= 1e-5 {
            0.0
        } else {
            ((20.0 * a.log10() + 54.0) / 60.0).clamp(0.0, 1.0)
        }
    };
    let half = (rect.width() - 3.0) / 2.0;
    for (k, a) in [peak.0, peak.1].into_iter().enumerate() {
        let h = level(a) * rect.height();
        let x0 = rect.left() + 1.0 + k as f32 * (half + 1.0);
        let col = if a > 1.0 { Color32::from_rgb(240, 80, 70) } else { GREEN.gamma_multiply(0.75) };
        p.rect_filled(Rect::from_min_max(Pos2::new(x0, rect.bottom() - h), Pos2::new(x0 + half, rect.bottom())), 0.0, col);
    }
    let y = rect.bottom() - (*v / max).clamp(0.0, 1.0) * rect.height();
    let cap = Rect::from_center_size(Pos2::new(rect.center().x, y), Vec2::new(rect.width() + 4.0, 7.0));
    p.rect_filled(cap, 2.0, if resp.hovered() || resp.dragged() { ACCENT } else { TEXT });
    p.line_segment([cap.left_center(), cap.right_center()], Stroke::new(1.0_f32, LINE));
    resp
}

/// Text cut to fit a width, with a trailing dot run when it had to be cut.
pub fn fit(ui: &Ui, text: &str, width: f32, font: &egui::FontId) -> String {
    let w = |s: &str| ui.fonts(|f| f.layout_no_wrap(s.to_string(), font.clone(), TEXT).size().x);
    if w(text) <= width {
        return text.to_string();
    }
    let mut out: String = text.to_string();
    while !out.is_empty() && w(&format!("{out}..")) > width {
        out.pop();
    }
    format!("{out}..")
}

/// Give the groups among the items at `idx` numbers no other item uses, so copies of a group stand apart
/// from the group they were copied from.
pub(crate) fn fresh_groups<T>(items: &mut [T], idx: &[usize], group: impl Fn(&mut T) -> &mut u16) {
    let mut next = items.iter_mut().map(|i| *group(i)).max().unwrap_or(0);
    let mut map: Vec<(u16, u16)> = vec![];
    for &i in idx {
        let Some(g) = items.get_mut(i).map(&group) else { continue };
        if *g != 0 {
            let old = *g;
            *g = match map.iter().find(|m| m.0 == old) {
                Some(m) => m.1,
                None => {
                    next = next.saturating_add(1);
                    map.push((old, next));
                    next
                }
            };
        }
    }
}

/// Make the items at `idx` one group, or with `on` false take them out of their groups.
pub(crate) fn set_group<T>(items: &mut [T], idx: &[usize], on: bool, group: impl Fn(&mut T) -> &mut u16) {
    let next = if on { items.iter_mut().map(|i| *group(i)).max().unwrap_or(0).saturating_add(1) } else { 0 };
    for &i in idx {
        if let Some(it) = items.get_mut(i) {
            *group(it) = next;
        }
    }
}

/// The one text line of a rename window: it takes the keys as it appears, with the old name selected so typing replaces it.
pub fn name_field(ui: &mut Ui, text: &mut String) -> Response {
    let id = ui.make_persistent_id("name_field");
    let fresh = !ui.memory(|m| m.has_focus(id));
    let mut out = egui::TextEdit::singleline(text).id(id).show(ui);
    if fresh {
        out.response.request_focus();
        let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()));
        out.state.cursor.set_char_range(Some(all));
        out.state.store(ui.ctx(), id);
    }
    out.response
}
