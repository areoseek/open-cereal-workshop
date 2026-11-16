//! The additive instrument's own panel: every control at a fixed place on one board, drawn here from plain
//! shapes (no pictures). Four bands from top to bottom: timbre and amp, filter, phaser and friends, effects.

use crate::widgets::{control_menu, Hint as _};
use cereal_engine::additive::{choices, ix};
use cereal_engine::song::Target;
use cereal_engine::synth::{def, Gen, Kind};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

const W: f32 = 830.0;
const H: f32 = 500.0;
const SLATE: Color32 = Color32::from_rgb(37, 38, 46);
const PLUM: Color32 = Color32::from_rgb(72, 53, 63);
const PLUM_LOW: Color32 = Color32::from_rgb(64, 47, 57);
const FLOOR: Color32 = Color32::from_rgb(28, 28, 37);
const EDGE: Color32 = Color32::from_rgb(20, 20, 26);
const WELL: Color32 = Color32::from_rgb(34, 36, 42);
const INK: Color32 = Color32::from_rgb(214, 216, 222);
const INK_DIM: Color32 = Color32::from_rgb(150, 150, 160);
const CYAN: Color32 = Color32::from_rgb(51, 193, 225);
const LEAF: Color32 = Color32::from_rgb(74, 200, 96);
const AMBER: Color32 = Color32::from_rgb(240, 160, 50);
const VIOLET: Color32 = Color32::from_rgb(150, 70, 215);
const ROSE: Color32 = Color32::from_rgb(214, 60, 120);
const BLUE: Color32 = Color32::from_rgb(60, 130, 230);
const LIME: Color32 = Color32::from_rgb(170, 210, 60);
const EMBER: Color32 = Color32::from_rgb(237, 121, 83);
const MINT: Color32 = Color32::from_rgb(190, 250, 225);

struct Board<'a> {
    ui: &'a mut Ui,
    at: Pos2,
    ci: usize,
    g: &'a mut Gen,
    changed: bool,
}

impl Board<'_> {
    fn pos(&self, x: f32, y: f32) -> Pos2 {
        self.at + Vec2::new(x, y)
    }

    fn range(&self, i: usize) -> (f32, f32, f32, &'static str) {
        let p = &def(Kind::Additive).params[i];
        (p.min as f32, p.max as f32, p.def as f32, p.name)
    }

    fn text(&self, x: f32, y: f32, s: &str) {
        self.ui.painter().text(self.pos(x, y), Align2::CENTER_CENTER, s, FontId::proportional(11.0), INK);
    }

    /// A band's name, slanted.
    fn title(&self, x: f32, y: f32, s: &str) {
        let job = egui::text::LayoutJob::simple_singleline(s.to_owned(), FontId::proportional(15.0), INK);
        let mut job = job;
        for sec in &mut job.sections {
            sec.format.italics = true;
        }
        let galley = self.ui.painter().layout_job(job);
        let at = self.pos(x, y) - Vec2::new(0.0, galley.size().y * 0.5);
        self.ui.painter().galley(at, galley, INK);
    }

    /// Drag handling shared by knobs and sliders: up is more, double-click resets, Ctrl is fine.
    fn drag(&mut self, rect: Rect, i: usize, reach: f32) -> egui::Response {
        let (min, max, default, name) = self.range(i);
        let resp = self.ui.interact(rect, self.ui.id().with(("additive", i)), Sense::click_and_drag());
        let mut v = self.g.p[i];
        if resp.dragged() {
            let fine = if self.ui.input(|i| i.modifiers.command) { 0.2 } else { 1.0 };
            let id = resp.id.with("exact");
            let mut exact: f32 = self.ui.data(|d| d.get_temp(id)).unwrap_or(v);
            if resp.drag_started() {
                exact = v;
            }
            exact = (exact - resp.drag_delta().y * (max - min) / reach * fine).clamp(min, max);
            self.ui.data_mut(|d| d.insert_temp(id, exact));
            v = exact.round();
        }
        if resp.double_clicked() {
            v = default;
        }
        if v != self.g.p[i] {
            self.g.p[i] = v;
            self.changed = true;
        }
        control_menu(&resp, Target::ChanParam(self.ci, i as u16));
        resp.hint(format!("{name}: {v:.0}"))
    }

    /// A knob of diameter `d` with a coloured arc around it showing the value; `label` goes underneath.
    fn knob(&mut self, i: usize, x: f32, y: f32, d: f32, arc: Color32, label: &str) {
        let c = self.pos(x, y);
        let resp = self.drag(Rect::from_center_size(c, Vec2::splat(d + 6.0)), i, 160.0);
        let (min, max, _, _) = self.range(i);
        let t = ((self.g.p[i] - min) / (max - min).max(1e-9)).clamp(0.0, 1.0);
        let p = self.ui.painter();
        let r = d * 0.5;
        let ang = |t: f32| (-225.0 + t * 270.0f32).to_radians();
        let sweep = |from: f32, to: f32, col: Color32, w: f32| {
            let n = (((to - from).abs() * 40.0) as usize).max(2);
            let pts: Vec<Pos2> = (0..=n).map(|j| ang(from + (to - from) * j as f32 / n as f32)).map(|a| c + Vec2::new(a.cos(), a.sin()) * (r + 2.0)).collect();
            p.add(egui::Shape::line(pts, Stroke::new(w, col)));
        };
        sweep(0.0, 1.0, EDGE, 3.0);
        // two-way settings grow from the middle
        let from = if min < 0.0 { (0.0 - min) / (max - min) } else { 0.0 };
        if (t - from).abs() > 0.004 {
            sweep(from, t, arc, 2.6);
        }
        p.circle_filled(c, r, EDGE);
        let big = d >= 30.0;
        let (rim, cap) = if big { (Color32::from_rgb(120, 126, 134), Color32::from_rgb(196, 204, 212)) } else { (Color32::from_rgb(58, 60, 68), Color32::from_rgb(44, 46, 54)) };
        p.circle_filled(c, r - 1.5, rim);
        p.circle_filled(c + Vec2::new(0.0, -0.5), r - 3.5, cap);
        let a = ang(t);
        let dir = Vec2::new(a.cos(), a.sin());
        let lit = resp.hovered() || resp.dragged();
        let mark = if big { Color32::from_rgb(40, 42, 50) } else if lit { Color32::WHITE } else { INK };
        p.line_segment([c + dir * (r * 0.35), c + dir * (r - 3.0)], Stroke::new(1.8_f32, mark));
        if !label.is_empty() {
            self.text(x, y + r + 11.0, label);
        }
    }

    /// An upright slider from `y0` (full) down to `y1`.
    fn slider(&mut self, i: usize, x: f32, y0: f32, y1: f32) {
        let rect = Rect::from_min_max(self.pos(x - 7.0, y0 - 4.0), self.pos(x + 7.0, y1 + 4.0));
        let resp = self.drag(rect, i, y1 - y0);
        let (min, max, _, _) = self.range(i);
        let t = ((self.g.p[i] - min) / (max - min).max(1e-9)).clamp(0.0, 1.0);
        let p = self.ui.painter();
        p.rect_filled(Rect::from_min_max(self.pos(x - 6.0, y0 - 3.0), self.pos(x + 6.0, y1 + 3.0)), 1.0, Color32::from_rgb(30, 31, 38));
        p.line_segment([self.pos(x, y0), self.pos(x, y1)], Stroke::new(2.0_f32, EDGE));
        for k in 0..=4 {
            let y = y0 + (y1 - y0) * k as f32 / 4.0;
            p.line_segment([self.pos(x - 5.0, y), self.pos(x - 3.0, y)], Stroke::new(1.0_f32, Color32::from_rgb(70, 72, 82)));
        }
        let y = y1 + (y0 - y1) * t;
        let cap = Rect::from_center_size(self.pos(x, y), Vec2::new(12.0, 7.0));
        let lit = resp.hovered() || resp.dragged();
        p.rect_filled(cap, 1.0, if lit { Color32::from_rgb(200, 204, 212) } else { Color32::from_rgb(150, 154, 164) });
        p.line_segment([cap.left_center(), cap.right_center()], Stroke::new(1.0_f32, EDGE));
    }

    /// A small round light with its name to the right; click flips the switch.
    fn light(&mut self, i: usize, x: f32, y: f32, label: &str) {
        let font = FontId::proportional(11.0);
        let w = self.ui.painter().layout_no_wrap(label.to_owned(), font.clone(), INK).size().x;
        let rect = Rect::from_min_max(self.pos(x - 6.0, y - 7.0), self.pos(x + 10.0 + w, y + 7.0));
        let name = self.range(i).3;
        let resp = self.ui.interact(rect, self.ui.id().with(("additive", i)), Sense::click()).hint(name);
        let on = self.g.p[i] != 0.0;
        if resp.clicked() {
            self.g.p[i] = if on { 0.0 } else { 1.0 };
            self.changed = true;
        }
        control_menu(&resp, Target::ChanParam(self.ci, i as u16));
        let p = self.ui.painter();
        p.circle_filled(self.pos(x, y), 4.5, EDGE);
        p.circle_stroke(self.pos(x, y), 4.5, Stroke::new(1.0_f32, Color32::from_rgb(110, 110, 122)));
        if on {
            p.circle_filled(self.pos(x, y), 2.6, EMBER);
        }
        p.text(self.pos(x + 9.0, y), Align2::LEFT_CENTER, label, font, INK);
    }

    /// A lettered button that lights when on (the velocity links, the octave switches).
    fn button(&mut self, i: usize, x: f32, y: f32, w: f32, label: &str, flip: bool) {
        let rect = Rect::from_center_size(self.pos(x, y), Vec2::new(w, 14.0));
        let name = self.range(i).3;
        let resp = self.ui.interact(rect, self.ui.id().with(("additive", i)), Sense::click()).hint(name);
        let stored = self.g.p[i] != 0.0;
        if resp.clicked() {
            self.g.p[i] = if stored { 0.0 } else { 1.0 };
            self.changed = true;
        }
        control_menu(&resp, Target::ChanParam(self.ci, i as u16));
        let on = stored != flip;
        let p = self.ui.painter();
        p.rect_filled(rect, 2.0, if on { MINT } else { Color32::from_rgb(86, 86, 98) });
        p.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, EDGE), egui::StrokeKind::Inside);
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(9.5), if on { Color32::from_rgb(20, 60, 50) } else { Color32::from_rgb(190, 190, 200) });
    }

    /// A dark box showing the picked kind; click opens the list.
    fn chooser(&mut self, i: usize, x0: f32, y0: f32, x1: f32) {
        let rect = Rect::from_min_max(self.pos(x0, y0), self.pos(x1, y0 + 22.0));
        let name = self.range(i).3;
        let resp = self.ui.interact(rect, self.ui.id().with(("additive", i)), Sense::click()).hint(name);
        let list = choices(i).unwrap_or(&[]);
        let cur = self.g.p[i] as u8;
        // a stored value past the list shows as the list's last entry (it plays as that one)
        let shown = list.iter().find(|c| c.0 == cur).or(list.last()).map_or("", |c| c.1);
        let p = self.ui.painter();
        p.rect_filled(rect, 2.0, WELL);
        p.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, EDGE), egui::StrokeKind::Inside);
        let font = FontId::proportional(12.0);
        let room = rect.width() - 20.0;
        p.text(rect.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, crate::widgets::fit(self.ui, shown, room, &font), font, INK);
        let tip = rect.right_center() - Vec2::new(7.0, 0.0);
        p.add(egui::Shape::convex_polygon(vec![tip + Vec2::new(-3.0, -4.0), tip + Vec2::new(3.0, 0.0), tip + Vec2::new(-3.0, 4.0)], INK_DIM, Stroke::NONE));
        let id = resp.id.with("list");
        if resp.clicked() {
            self.ui.memory_mut(|m| m.toggle_popup(id));
        }
        let mut pick = None;
        egui::popup_below_widget(self.ui, id, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(140.0);
            for &(v, n) in list {
                if ui.selectable_label(v == cur, n).clicked() {
                    pick = Some(v);
                }
            }
        });
        if let Some(v) = pick.filter(|&v| v != cur) {
            self.g.p[i] = v as f32;
            self.changed = true;
        }
    }

    /// A column of lights of which one is on: the stored number picks the row.
    fn rows(&mut self, i: usize, x: f32, y: f32, names: &[&str]) {
        let name = self.range(i).3;
        let cur = self.g.p[i] as usize;
        for (k, n) in names.iter().enumerate() {
            let yy = y + 13.0 * k as f32;
            let rect = Rect::from_min_max(self.pos(x - 6.0, yy - 6.0), self.pos(x + 36.0, yy + 6.0));
            let resp = self.ui.interact(rect, self.ui.id().with(("additive", i, k)), Sense::click()).hint(name);
            if resp.clicked() && cur != k {
                self.g.p[i] = k as f32;
                self.changed = true;
            }
            let p = self.ui.painter();
            p.circle_filled(self.pos(x, yy), 4.0, EDGE);
            p.circle_stroke(self.pos(x, yy), 4.0, Stroke::new(1.0_f32, Color32::from_rgb(110, 110, 122)));
            if cur == k {
                p.circle_filled(self.pos(x, yy), 2.4, Color32::from_rgb(220, 222, 230));
            }
            p.text(self.pos(x + 9.0, yy), Align2::LEFT_CENTER, *n, FontId::proportional(10.0), INK);
        }
    }
}

/// Draws the board; true when a setting was changed.
pub fn panel(ui: &mut Ui, ci: usize, g: &mut Gen) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(W, H), Sense::hover());
    let at = rect.min;
    {
        let p = ui.painter();
        let band = |y0: f32, y1: f32, c: Color32| p.rect_filled(Rect::from_min_max(at + Vec2::new(0.0, y0), at + Vec2::new(W, y1)), 0.0, c);
        band(0.0, 166.0, SLATE);
        band(166.0, 297.0, PLUM);
        band(297.0, 426.0, PLUM_LOW);
        band(426.0, H, FLOOR);
        for y in [166.0, 297.0, 426.0] {
            p.line_segment([at + Vec2::new(0.0, y), at + Vec2::new(W, y)], Stroke::new(1.0_f32, EDGE));
        }
        // wells behind the slider banks
        for (x0, x1, y0, y1) in [(30.0, 54.0, 18.0, 110.0), (62.0, 120.0, 18.0, 110.0), (128.0, 238.0, 18.0, 110.0), (250.0, 470.0, 12.0, 78.0), (588.0, 682.0, 18.0, 138.0), (728.0, 816.0, 182.0, 280.0)] {
            p.rect_filled(Rect::from_min_max(at + Vec2::new(x0, y0), at + Vec2::new(x1, y1)), 3.0, Color32::from_rgba_unmultiplied(0, 0, 0, 46));
        }
        // boxes around each effect
        for (x0, x1) in [(26.0, 176.0), (186.0, 402.0), (410.0, 556.0), (564.0, 700.0)] {
            p.rect_stroke(Rect::from_min_max(at + Vec2::new(x0, 434.0), at + Vec2::new(x1, 496.0)), 4.0, Stroke::new(1.0_f32, Color32::from_rgb(48, 48, 60)), egui::StrokeKind::Inside);
        }
    }
    let mut b = Board { ui, at, ci, g, changed: false };

    // timbre band
    b.slider(ix::LHP, 42.0, 30.0, 98.0);
    b.text(42.0, 118.0, "lhp");
    for k in 0..3 {
        b.slider(ix::SUB + k, 74.0 + 17.0 * k as f32, 30.0, 98.0);
    }
    b.text(91.0, 118.0, "sub");
    for k in 0..6 {
        let x = 139.0 + 17.4 * k as f32;
        b.slider(ix::EQ + k, x, 30.0, 98.0);
        b.ui.painter().text(b.pos(x, 14.0), Align2::CENTER_CENTER, (k + 1).to_string(), FontId::proportional(9.0), INK_DIM);
    }
    b.text(183.0, 118.0, "equalizer");
    for k in 0..12 {
        let x = 261.0 + 18.0 * k as f32;
        b.slider(ix::MASK + k, x, 22.0, 70.0);
        b.ui.painter().text(b.pos(x, 8.0), Align2::CENTER_CENTER, (k + 1).to_string(), FontId::proportional(9.0), INK_DIM);
    }
    b.text(300.0, 88.0, "harmonic mask");
    b.knob(ix::MASK_MIX, 497.0, 34.0, 22.0, CYAN, "mix");
    b.knob(ix::MASK_LFO, 537.0, 34.0, 18.0, AMBER, "lfo");
    b.knob(ix::TREMOLO, 298.0, 108.0, 22.0, CYAN, "trem");
    b.knob(ix::ATTACK, 351.0, 108.0, 24.0, CYAN, "att");
    b.knob(ix::DECAY, 394.0, 108.0, 24.0, CYAN, "dec");
    b.knob(ix::RELEASE, 437.0, 108.0, 24.0, CYAN, "rel");
    b.knob(ix::PLUCK, 495.0, 106.0, 26.0, CYAN, "pluck");
    b.button(ix::VEL_ATTACK, 351.0, 154.0, 26.0, "VEL", false);
    b.button(ix::VEL_RELEASE, 437.0, 154.0, 26.0, "VEL", false);
    b.button(ix::VEL_PLUCK, 495.0, 154.0, 26.0, "VEL", false);
    b.light(ix::PLUCK_MASKED, 533.0, 90.0, "masked");
    b.light(ix::PLUCK_ALT, 533.0, 104.0, "alt");
    b.light(ix::PLUCK_RELEASE, 533.0, 119.0, "release");
    b.title(546.0, 152.0, "Amp");
    b.button(ix::VEL_VOLUME, 602.0, 152.0, 26.0, "VEL", false);
    b.text(602.0, 12.0, "vol");
    b.slider(ix::VOLUME, 602.0, 30.0, 130.0);
    b.text(677.0, 12.0, "detune");
    b.slider(ix::PITCH, 645.0, 30.0, 130.0);
    b.slider(ix::DETUNE, 667.0, 30.0, 130.0);
    for (k, s) in ["+24", "+12", "0", "-12", "-24"].iter().enumerate() {
        b.ui.painter().text(b.pos(626.0, 30.0 + 25.0 * k as f32), Align2::CENTER_CENTER, *s, FontId::proportional(8.0), INK_DIM);
    }
    b.text(645.0, 146.0, "pitch");
    b.title(764.0, 26.0, "Pitch");
    for (i, y, col, s) in [(ix::GRIT, 38.0, CYAN, "grit"), (ix::VIBRATO, 80.0, CYAN, "vib"), (ix::PITCH_LFO, 122.0, AMBER, "lfo")] {
        b.knob(i, 731.0, y, 20.0, col, "");
        b.ui.painter().text(b.pos(714.0, y), Align2::RIGHT_CENTER, s, FontId::proportional(11.0), INK);
    }
    b.light(ix::PORTAMENTO, 768.0, 54.0, "porta");
    b.light(ix::LEGATO, 768.0, 70.0, "legato");
    b.knob(ix::SLIDE_TIME, 772.0, 104.0, 22.0, BLUE, "");
    b.text(803.0, 106.0, "time");
    b.button(ix::VEL_SLIDE, 805.0, 126.0, 26.0, "VEL", false);
    b.title(38.0, 144.0, "Timbre");
    b.chooser(ix::TIMBRE, 94.0, 133.0, 200.0);
    b.knob(ix::PHASE, 227.0, 144.0, 22.0, BLUE, "");
    b.text(261.0, 154.0, "phase");

    // filter band
    b.light(ix::F_ADAPTIVE, 41.0, 200.0, "alt");
    b.knob(ix::F_ATTACK, 118.0, 199.0, 24.0, LEAF, "att");
    b.knob(ix::F_DECAY, 160.0, 199.0, 24.0, LEAF, "dec");
    b.knob(ix::F_AMOUNT, 210.0, 199.0, 34.0, LEAF, "amt");
    b.knob(ix::F_ATT_SLOPE, 118.0, 262.0, 20.0, LEAF, "");
    b.knob(ix::F_DEC_SLOPE, 160.0, 262.0, 20.0, LEAF, "");
    b.button(ix::VEL_ENV, 210.0, 259.0, 26.0, "VEL", false);
    b.knob(ix::CUTOFF_LFO, 289.0, 199.0, 20.0, AMBER, "lfo");
    b.knob(ix::KEY_TRACK, 323.0, 199.0, 20.0, LEAF, "kb.t");
    b.knob(ix::F_WIDTH, 359.0, 199.0, 24.0, LEAF, "width");
    b.knob(ix::CUTOFF, 413.0, 199.0, 36.0, LEAF, "freq");
    b.knob(ix::RESONANCE, 465.0, 199.0, 26.0, ROSE, "res");
    b.knob(ix::RES_OFFSET, 510.0, 199.0, 20.0, ROSE, "ofs");
    b.knob(ix::SELF_OSC, 544.0, 199.0, 20.0, ROSE, "osc");
    b.chooser(ix::FILTER_TYPE, 313.0, 248.0, 428.0);
    b.button(ix::CUT_HZ, 410.0, 284.0, 30.0, "Oct", true);
    b.title(270.0, 284.0, "Filter");
    b.chooser(ix::RES_TYPE, 449.0, 248.0, 539.0);
    b.button(ix::RES_HZ, 467.0, 284.0, 30.0, "Oct", true);
    b.light(ix::MOTION, 553.0, 250.0, "motion t.");
    b.light(ix::RES_ADAPTIVE, 553.0, 265.0, "adaptive");
    b.title(540.0, 284.0, "Resonance");
    b.title(626.0, 205.0, "Unison");
    {
        // the order: a number box, drag or scroll to change
        let rect = Rect::from_center_size(b.pos(704.0, 207.0), Vec2::new(20.0, 20.0));
        let resp = b.drag(rect, ix::UNI_ORDER, 120.0);
        let p = b.ui.painter();
        p.rect_filled(rect, 3.0, Color32::from_rgb(92, 92, 104));
        p.text(rect.center(), Align2::CENTER_CENTER, format!("{:.0}", b.g.p[ix::UNI_ORDER]), FontId::proportional(13.0), if resp.hovered() { Color32::WHITE } else { INK });
    }
    b.text(704.0, 226.0, "order");
    b.chooser(ix::UNI_TYPE, 650.0, 248.0, 716.0);
    b.button(ix::VEL_UNI, 702.0, 284.0, 26.0, "VEL", false);
    for (i, x, top, bottom) in [(ix::UNI_PAN, 740.0, "pan", ""), (ix::UNI_PITCH, 762.0, "", "pitch"), (ix::UNI_PHASE, 782.0, "phase", ""), (ix::UNI_VAR, 804.0, "", "var")] {
        b.slider(i, x, 192.0, 268.0);
        if !top.is_empty() {
            b.text(x, 176.0, top);
        }
        if !bottom.is_empty() {
            b.text(x, 287.0, bottom);
        }
    }

    // phaser band
    b.title(38.0, 330.0, "Phaser");
    b.knob(ix::PH_AMOUNT, 122.0, 329.0, 34.0, LEAF, "amt");
    b.knob(ix::PH_WIDTH, 170.0, 329.0, 26.0, LEAF, "width");
    b.knob(ix::PH_OFFSET, 213.0, 329.0, 26.0, LEAF, "ofs");
    b.knob(ix::PH_SPEED, 255.0, 329.0, 22.0, LIME, "speed");
    b.knob(ix::PH_KEY, 291.0, 329.0, 20.0, LEAF, "kb.t");
    b.chooser(ix::PH_TYPE, 33.0, 377.0, 100.0);
    b.rows(ix::PH_SCALE, 113.0, 383.0, &["oct", "Hz", "harm"]);
    b.knob(ix::PH_WIDTH_LFO, 171.0, 386.0, 20.0, AMBER, "");
    b.knob(ix::PH_OFFSET_LFO, 213.0, 386.0, 20.0, AMBER, "lfo");
    b.knob(ix::HZ_AMOUNT, 409.0, 329.0, 34.0, VIOLET, "amt");
    b.knob(ix::HZ_WIDTH, 455.0, 329.0, 26.0, VIOLET, "width");
    b.knob(ix::HZ_STRENGTH, 496.0, 329.0, 20.0, VIOLET, "str");
    b.chooser(ix::HZ_TYPE, 524.0, 308.0, 609.0);
    b.chooser(ix::HZ_POS, 524.0, 336.0, 609.0);
    b.knob(ix::HZ_LFO, 439.0, 386.0, 20.0, AMBER, "lfo");
    b.button(ix::VEL_HZ, 475.0, 387.0, 26.0, "VEL", false);
    b.title(534.0, 388.0, "Harmonizer");
    b.title(688.0, 330.0, "LFO");
    b.knob(ix::LFO_ATTACK, 744.0, 329.0, 20.0, AMBER, "att");
    b.knob(ix::LFO_SPEED, 790.0, 329.0, 34.0, AMBER, "speed");
    b.chooser(ix::LFO_SHAPE, 689.0, 377.0, 790.0);
    b.light(ix::LFO_GLOBAL, 700.0, 412.0, "global");

    // effects band
    b.chooser(ix::CHORUS_TYPE, 33.0, 442.0, 122.0);
    b.title(36.0, 484.0, "Chorus");
    b.knob(ix::CHORUS_MIX, 149.0, 456.0, 24.0, VIOLET, "mix");
    b.chooser(ix::DELAY_TYPE, 194.0, 442.0, 275.0);
    b.title(196.0, 484.0, "Delay");
    b.knob(ix::DELAY_VOL, 300.0, 456.0, 24.0, LEAF, "vol");
    b.knob(ix::DELAY_FB, 342.0, 458.0, 20.0, LEAF, "fb");
    b.knob(ix::DELAY_TIME, 377.0, 458.0, 20.0, LEAF, "time");
    b.chooser(ix::REVERB_TYPE, 416.0, 442.0, 501.0);
    b.title(418.0, 484.0, "Reverb");
    b.knob(ix::REVERB_VOL, 527.0, 456.0, 24.0, VIOLET, "vol");
    b.chooser(ix::COMP_TYPE, 571.0, 442.0, 645.0);
    b.title(572.0, 484.0, "Comp");
    b.knob(ix::COMP_MIX, 672.0, 456.0, 24.0, CYAN, "mix");
    b.rows(ix::COUNT - 1, 722.0, 453.0, &["normal", "high quality"]);
    b.changed
}
