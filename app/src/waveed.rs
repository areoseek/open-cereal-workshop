//! The wave recorder: records the sound passing its mixer slot, shows it, and edits it.

use crate::widgets::Hint as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use cereal_engine::sample::Sample;
use cereal_engine::waveed as ops;
use cereal_engine::{Cmd, Handle};

use crate::widgets::*;

/// What the window asks the rest of the program to do with a finished take.
pub enum WaveAct {
    ToPlaylist(PathBuf),
    ToChannel(PathBuf),
    Status(String),
}

pub struct WaveEd {
    pub frames: Vec<[f32; 2]>,
    pub rate: u32,
    /// The marked stretch; equal ends mean a bare cursor.
    sel: (usize, usize),
    /// The stretch in view.
    view: (usize, usize),
    undo: Vec<Snap>,
    clip: Vec<[f32; 2]>,
    name: String,
    on_play: bool,
    amp_db: f32,
    /// Markers and regions, in time order.
    marks: Vec<ops::Mark>,
    /// The loop stretch; equal ends mean none.
    lp: (usize, usize),
    /// The level below which sound counts as noise.
    thresh: f32,
    /// The measured noise, for the clean-up tool.
    profile: Vec<f32>,
    clean: f32,
    /// The flag under the right-click menu, and the one being renamed.
    menu_mark: Option<usize>,
    rename: Option<usize>,
    /// The tool whose settings are on show under the picture (its edit number).
    tool: Option<u8>,
    /// The claw: the tempo, the stretch in beats, and which stretch of every so many goes.
    claw_bpm: f32,
    claw_beats: f32,
    claw_every: u32,
    time: f32,
    semis: f32,
    eq: [f32; 10],
    blur: f32,
    room_secs: f32,
    room_wet: f32,
    room_clip: bool,
    join_ms: f32,
    /// Frequencies in place of the wave.
    spec: bool,
    /// Counts the changes, so the frequency picture knows when it is stale.
    rev: u64,
    spec_tex: Option<((usize, usize, usize, u64), egui::TextureHandle)>,
}

/// One step back.
struct Snap {
    frames: Vec<[f32; 2]>,
    sel: (usize, usize),
    marks: Vec<ops::Mark>,
    lp: (usize, usize),
}

impl Default for WaveEd {
    fn default() -> Self {
        Self { frames: Vec::new(), rate: 48_000, sel: (0, 0), view: (0, 0), undo: Vec::new(), clip: Vec::new(), name: "Take".into(), on_play: false, amp_db: 3.0, marks: Vec::new(), lp: (0, 0), thresh: 0.02, profile: Vec::new(), clean: 1.5, menu_mark: None, rename: None, tool: None, claw_bpm: 120.0, claw_beats: 1.0, claw_every: 4, time: 1.0, semis: 0.0, eq: [0.0; 10], blur: 0.5, room_secs: 1.5, room_wet: 0.3, room_clip: false, join_ms: 30.0, spec: false, rev: 0, spec_tex: None }
    }
}

/// Writes frames to a new WAV file in `dir` and gives its path.
pub fn save_take(dir: &Path, name: &str, frames: &[[f32; 2]], rate: u32, marks: &[ops::Mark]) -> Result<PathBuf, String> {
    if frames.is_empty() {
        return Err("there is nothing to save yet".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let clean = if clean.trim().is_empty() { "Take".to_string() } else { clean.trim().to_string() };
    let path = (0..10_000).map(|i| dir.join(if i == 0 { format!("{clean}.wav") } else { format!("{clean} {}.wav", i + 1) })).find(|p| !p.exists()).ok_or("no free file name")?;
    write_wav(&path, frames, rate, marks)?;
    Ok(path)
}

fn write_wav(path: &Path, frames: &[[f32; 2]], rate: u32, marks: &[ops::Mark]) -> Result<(), String> {
    let flat: Vec<f32> = frames.iter().flat_map(|f| [f[0], f[1]]).collect();
    let mut cur = std::io::Cursor::new(Vec::new());
    cereal_engine::wav::write(&mut cur, &flat, rate, 2, 24).map_err(|e| e.to_string())?;
    let mut bytes = cur.into_inner();
    ops::write_marks(&mut bytes, marks);
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

impl WaveEd {
    /// The stretch the tools work on: what is marked, or everything.
    fn range(&self) -> (usize, usize) {
        if self.sel.1 > self.sel.0 { (self.sel.0, self.sel.1.min(self.frames.len())) } else { (0, self.frames.len()) }
    }

    /// The flags inside `a..b`, counted from `a`, for saving that stretch on its own.
    fn marks_in(&self, a: usize, b: usize) -> Vec<ops::Mark> {
        self.marks.iter().filter(|m| m.at >= a && m.at < b).map(|m| ops::Mark { at: m.at - a, len: m.len.min(b - m.at), name: m.name.clone() }).collect()
    }

    /// The stretch a flag stands for: its own length, or up to the next flag.
    fn mark_span(&self, k: usize) -> (usize, usize) {
        let m = &self.marks[k];
        let end = if m.len > 0 { m.at + m.len } else { self.marks.iter().map(|o| o.at).find(|&o| o > m.at).unwrap_or(self.frames.len()) };
        (m.at, end.min(self.frames.len()))
    }

    fn add_mark(&mut self, at: usize, len: usize, name: String) {
        if self.marks.iter().any(|m| m.at == at && m.len == len) {
            return;
        }
        self.marks.push(ops::Mark { at, len, name });
        self.marks.sort_by_key(|m| m.at);
    }

    fn took_out(&mut self, a: usize, b: usize) {
        ops::marks_removed(&mut self.marks, a, b);
        let mut l = vec![ops::Mark { at: self.lp.0, len: self.lp.1 - self.lp.0, name: String::new() }];
        ops::marks_removed(&mut l, a, b);
        self.lp = l.first().map_or((0, 0), |m| (m.at, m.at + m.len));
    }

    fn put_in(&mut self, at: usize, n: usize) {
        ops::marks_inserted(&mut self.marks, at, n);
        let mut l = [ops::Mark { at: self.lp.0, len: self.lp.1 - self.lp.0, name: String::new() }];
        ops::marks_inserted(&mut l, at, n);
        self.lp = (l[0].at, l[0].at + l[0].len);
    }

    pub fn open(&mut self, p: &Path) -> Result<(), String> {
        let s = Sample::load(p)?;
        self.keep();
        self.rate = s.rate;
        self.name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Take".into());
        self.frames = s.frames;
        self.marks = std::fs::read(p).map(|b| ops::read_marks(&b)).unwrap_or_default();
        let n = self.frames.len();
        self.marks.retain(|m| m.at < n);
        self.lp = (0, 0);
        self.sel = (0, 0);
        self.fit();
        Ok(())
    }

    /// Put `new` where `a..b` was; the flags inside move in step with the change of length.
    fn swap_in(&mut self, a: usize, b: usize, new: Vec<[f32; 2]>) {
        let inside: Vec<ops::Mark> = self.marks.iter().filter(|m| m.at >= a && m.at < b).cloned().collect();
        let scale = new.len() as f64 / (b - a).max(1) as f64;
        ops::remove(&mut self.frames, a, b);
        self.took_out(a, b);
        ops::insert(&mut self.frames, a, &new);
        self.put_in(a, new.len());
        for m in inside {
            let at = a + ((m.at - a) as f64 * scale) as usize;
            self.add_mark(at.min(self.frames.len().saturating_sub(1)), (m.len as f64 * scale) as usize, m.name);
        }
        self.sel = (a, a + new.len());
        self.fit();
    }

    fn keep(&mut self) {
        self.rev += 1;
        self.undo.push(Snap { frames: self.frames.clone(), sel: self.sel, marks: self.marks.clone(), lp: self.lp });
        // a handful of steps back is plenty and keeps long takes from piling up in memory
        let cap = if self.frames.len() > 20_000_000 { 2 } else { 12 };
        while self.undo.len() > cap {
            self.undo.remove(0);
        }
    }

    fn fit(&mut self) {
        self.view = (0, self.frames.len());
    }

    fn time(&self, at: usize) -> String {
        let s = at as f64 / self.rate.max(1) as f64;
        format!("{}:{:06.3}", (s / 60.0) as u32, s % 60.0)
    }

    pub fn ui(&mut self, ui: &mut Ui, handle: &Handle, slot: (usize, u16), live_rate: f32, dir: &Path) -> Option<WaveAct> {
        let tr = &handle.transport;
        let mut act = None;
        let recording = tr.recording_slot() == Some(slot);
        // sound gathered since the last frame joins the end of the take
        if recording {
            let got = tr.take_recorded();
            if !got.is_empty() {
                let at_end = self.view.1 >= self.frames.len();
                if self.frames.is_empty() {
                    self.rate = live_rate.round().max(1.0) as u32;
                }
                self.frames.extend_from_slice(&got);
                if at_end {
                    self.view.1 = self.frames.len();
                }
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        } else if self.on_play && tr.playing() {
            // armed: the take starts with the song
            self.keep();
            tr.record_slot(Some(slot));
        }
        if recording && self.on_play && !tr.playing() {
            tr.record_slot(None);
        }
        let over_id = ui.id().with("wave over");
        let over = ui.data(|d| d.get_temp::<bool>(over_id)).unwrap_or(false) && ui.memory(|m| m.focused().is_none());
        let open_key = over && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O));
        let save_key = over && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S));
        ui.horizontal(|ui| {
            let label = if recording { RichText::new("Stop recording").color(Color32::from_rgb(240, 110, 90)) } else { RichText::new("Record") };
            if ui.button(label).hint("Record the sound arriving at this slot. New sound joins the end of the take.").clicked() {
                if recording {
                    tr.record_slot(None);
                    self.on_play = false;
                } else {
                    self.keep();
                    tr.record_slot(Some(slot));
                }
            }
            ui.checkbox(&mut self.on_play, "with the song").hint("Start recording when the song starts and stop when it stops");
            ui.separator();
            if ui.button("▶ Play").hint("Play the marked stretch, or everything").clicked() {
                let (a, b) = self.range();
                let (a, b) = if self.sel.1 == self.sel.0 && self.sel.0 < self.frames.len() { (self.sel.0, self.frames.len()) } else { (a, b) };
                if b > a {
                    handle.send(Cmd::Preview(Arc::new(Sample { name: self.name.clone(), rate: self.rate, mono: false, frames: self.frames[a..b].to_vec(), loop_pts: None })));
                }
            }
            if ui.button("Stop").clicked() {
                handle.send(Cmd::StopPreview);
            }
            ui.separator();
            if ui.button("Open...").hint("Ctrl+O").clicked() || open_key {
                if let Some(p) = rfd::FileDialog::new().add_filter("Sound", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]).pick_file() {
                    if let Err(e) = self.open(&p) {
                        act = Some(WaveAct::Status(e));
                    }
                }
            }
            if ui.add_enabled(!self.frames.is_empty(), egui::Button::new("Save as...")).hint("Ctrl+S. Markers and regions are kept in the file.").clicked() || (save_key && !self.frames.is_empty()) {
                if let Some(p) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).set_file_name(format!("{}.wav", self.name)).save_file() {
                    let (a, b) = self.range();
                    act = Some(WaveAct::Status(match write_wav(&p, &self.frames[a..b], self.rate, &self.marks_in(a, b)) {
                        Ok(()) => format!("Saved {}", p.display()),
                        Err(e) => e,
                    }));
                }
            }
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(120.0)).hint("Name of the take");
        });
        let has = !self.frames.is_empty();
        let mut edit: Option<u8> = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(has && !recording, |ui| {
                for (k, name, tip) in [
                    (1u8, "Trim", "Keep only the marked stretch (Ctrl+Del)"),
                    (2, "Delete", "Take the marked stretch out (Del)"),
                    (3, "Cut", "Ctrl+X"),
                    (4, "Copy", "Ctrl+C"),
                    (5, "Paste", "Put the copied sound in at the cursor, over the marked stretch if there is one (Ctrl+V)"),
                    (6, "Silence", "Replace the marked stretch with silence"),
                    (7, "Reverse", "Alt+Left"),
                    (8, "Normalize", "Bring the loudest point to full level (Ctrl+N)"),
                    (9, "Fade in", "Ctrl+F"),
                    (10, "Fade out", "Alt+F"),
                    (11, "Centre", "Take out a standing offset"),
                    (12, "Swap sides", "Left and right change places"),
                ] {
                    if ui.button(name).hint(tip).clicked() {
                        edit = Some(k);
                    }
                }
                if ui.button("Amplify").hint("Make the marked stretch louder or softer by the amount beside").clicked() {
                    edit = Some(13);
                }
                ui.add(egui::DragValue::new(&mut self.amp_db).range(-48.0..=48.0).speed(0.1).suffix(" dB"));
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).hint("Ctrl+Z").clicked() {
                    edit = Some(20);
                }
                ui.menu_button("Paste as", |ui| {
                    for (k, name) in [(14u8, "Paste over what is there (Alt+V)"), (15, "Paste mixed in (Shift+V)"), (16, "Insert silence (Ins)"), (17, "Delete all after the loop end (Alt+Del)")] {
                        if ui.button(name).clicked() {
                            edit = Some(k);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Regions", |ui| {
                    for (k, name) in [
                        (23u8, "Add a marker at the cursor (M)"),
                        (22, "Make the marked stretch a region (Alt+M)"),
                        (24, "Make the marked stretch the loop (Alt+L)"),
                        (25, "Delete the markers, regions and loop in the marked stretch (Shift+Ctrl+Del)"),
                        (26, "Previous region (Left)"),
                        (27, "Next region (Right)"),
                        (39, "Slice at the hardest hits"),
                        (38, "Slice at the hits (Alt+S)"),
                        (40, "Slice at every hit, soft ones too"),
                    ] {
                        if ui.button(name).clicked() {
                            edit = Some(k);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Tools", |ui| {
                    for (k, name) in [
                        (50u8, "Time stretch and pitch shift... (Alt+T)"),
                        (51, "Drum stretch... (Ctrl+T)"),
                        (52, "Reverb... (Ctrl+R)"),
                        (53, "Blur... (Ctrl+B)"),
                        (54, "Equalise... (Ctrl+Q)"),
                        (55, "Tune the loop... (Ctrl+L)"),
                        (56, "Claw machine... (Alt+W)"),
                    ] {
                        if ui.button(name).clicked() {
                            self.tool = Some(k);
                            ui.close_menu();
                        }
                    }
                    if ui.button(if self.spec { "Show the wave (S)" } else { "Show the frequencies (S)" }).clicked() {
                        self.spec = !self.spec;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Noise", |ui| {
                    for (k, name) in [
                        (30u8, "Take the noise level from the marked stretch (Alt+N)"),
                        (33, "Silence everything below the noise level"),
                        (34, "Trim the noise off both ends"),
                        (35, "Take out all the noise and slice what is left"),
                        (31, "Learn the noise from the marked stretch (Alt+U)"),
                        (32, "Clean up: take the learned noise out (Ctrl+U)"),
                        (36, "Take the click off the start"),
                        (37, "Take the click off the end"),
                    ] {
                        if ui.add_enabled(k != 32 || !self.profile.is_empty(), egui::Button::new(name)).clicked() {
                            edit = Some(k);
                            ui.close_menu();
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label("Noise level");
                        let mut db = 20.0 * self.thresh.max(1e-5).log10();
                        if ui.add(egui::DragValue::new(&mut db).range(-90.0..=0.0).speed(0.2).suffix(" dB")).changed() {
                            self.thresh = 10f32.powf(db / 20.0);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Clean-up strength");
                        ui.add(egui::DragValue::new(&mut self.clean).range(0.2..=4.0).speed(0.02));
                    });
                });
            });
        });
        if let Some(k) = self.tool {
            ui.horizontal_wrapped(|ui| {
                match k {
                    50 | 51 => {
                        ui.label(if k == 50 { "Time stretch:" } else { "Drum stretch (moves the slices apart, leaves them whole):" });
                        ui.add(egui::DragValue::new(&mut self.time).range(0.1..=10.0).speed(0.005).prefix("length x"));
                        if k == 50 {
                            ui.add(egui::DragValue::new(&mut self.semis).range(-36.0..=36.0).speed(0.05).suffix(" semitones"));
                        }
                    }
                    52 => {
                        ui.label("Reverb:");
                        ui.add_enabled(!self.room_clip, egui::DragValue::new(&mut self.room_secs).range(0.1..=8.0).speed(0.01).suffix(" s"));
                        let mut pc = self.room_wet * 100.0;
                        if ui.add(egui::DragValue::new(&mut pc).range(0.0..=100.0).speed(0.3).suffix(" % wet")).changed() {
                            self.room_wet = pc / 100.0;
                        }
                        ui.add_enabled(!self.clip.is_empty(), egui::Checkbox::new(&mut self.room_clip, "use the copied sound as the room")).hint("Copy a recorded room echo (Ctrl+C) and it shapes the reverb in place of the built-in one");
                    }
                    53 => {
                        ui.label("Blur:");
                        ui.add(egui::Slider::new(&mut self.blur, 0.0..=1.0).show_value(false));
                    }
                    54 => {
                        ui.label("Equalise:");
                        for (i, hz) in ops::EQ_BANDS.iter().enumerate() {
                            let name = if *hz >= 1000.0 { format!("{}k ", hz / 1000.0) } else { format!("{hz} ") };
                            ui.add(egui::DragValue::new(&mut self.eq[i]).range(-36.0..=18.0).speed(0.1).prefix(name).suffix(" dB"));
                        }
                    }
                    56 => {
                        ui.label("Claw machine: throw away every");
                        ui.add(egui::DragValue::new(&mut self.claw_every).range(2..=64).speed(0.05).suffix(". stretch"));
                        ui.label("of");
                        ui.add(egui::DragValue::new(&mut self.claw_beats).range(0.0625..=64.0).speed(0.01).suffix(" beats"));
                        ui.label("at");
                        ui.add(egui::DragValue::new(&mut self.claw_bpm).range(20.0..=400.0).speed(0.1).suffix(" BPM"));
                    }
                    _ => {
                        ui.label("Tune the loop (or the marked stretch) so its end runs into its start. Join:");
                        ui.add(egui::DragValue::new(&mut self.join_ms).range(1.0..=2000.0).speed(0.5).suffix(" ms"));
                    }
                }
                if ui.add_enabled(has && !recording, egui::Button::new("Apply")).clicked() {
                    edit = Some(k);
                }
                if ui.button("Close").clicked() {
                    self.tool = None;
                }
            });
        }
        // the picture
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(780.0, 230.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 2.0, PANEL_DARK);
        let n = self.frames.len();
        if self.view.1 > n || self.view.1 <= self.view.0 {
            self.fit();
        }
        let (v0, v1) = self.view;
        let span = (v1 - v0).max(1);
        let x_of = |at: usize| rect.left() + (at.saturating_sub(v0)) as f32 / span as f32 * rect.width();
        if self.sel.1 > self.sel.0 {
            let (a, b) = (x_of(self.sel.0.max(v0)), x_of(self.sel.1.min(v1)));
            if b > a {
                p.rect_filled(egui::Rect::from_min_max(Pos2::new(a, rect.top()), Pos2::new(b, rect.bottom())), 0.0, ACCENT.gamma_multiply(0.22));
            }
        }
        p.line_segment([Pos2::new(rect.left(), rect.center().y), Pos2::new(rect.right(), rect.center().y)], Stroke::new(1.0_f32, LINE_SOFT));
        if has {
            let cols = rect.width() as usize;
            let mid = rect.center().y;
            let half = rect.height() * 0.48;
            let col = if recording { Color32::from_rgb(240, 110, 90) } else { ACCENT };
            if self.spec {
                let key = (v0, v1, n, self.rev);
                if self.spec_tex.as_ref().is_none_or(|t| t.0 != key) {
                    let (w, h) = (cols / 2, 115);
                    let cells = ops::spectrum(&self.frames, v0, v1, w, h, self.rate);
                    let px: Vec<Color32> = cells
                        .iter()
                        .map(|&v| {
                            // dark, through the accent colour, to white at full level
                            let (lo, hi) = if v < 0.6 { (PANEL_DARK, ACCENT) } else { (ACCENT, Color32::WHITE) };
                            let t = if v < 0.6 { v / 0.6 } else { (v - 0.6) / 0.4 };
                            let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
                            Color32::from_rgb(m(lo.r(), hi.r()), m(lo.g(), hi.g()), m(lo.b(), hi.b()))
                        })
                        .collect();
                    let img = egui::ColorImage { size: [w, h], pixels: px };
                    self.spec_tex = Some((key, ui.ctx().load_texture("wave frequencies", img, egui::TextureOptions::LINEAR)));
                }
                if let Some((_, tex)) = &self.spec_tex {
                    p.image(tex.id(), rect, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
            } else if span <= cols * 2 {
                let pts: Vec<Pos2> = (v0..v1).map(|i| Pos2::new(x_of(i), mid - (self.frames[i][0] + self.frames[i][1]) * 0.5 * half)).collect();
                p.add(egui::Shape::line(pts, Stroke::new(1.2_f32, col)));
            } else {
                for (c, (lo, hi)) in ops::columns(&self.frames, v0, v1, cols).into_iter().enumerate() {
                    let x = rect.left() + c as f32 + 0.5;
                    p.line_segment([Pos2::new(x, mid - hi.clamp(-1.0, 1.0) * half), Pos2::new(x, mid - lo.clamp(-1.0, 1.0) * half + 1.0)], Stroke::new(1.0_f32, col));
                }
            }
            if self.sel.1 == self.sel.0 && (v0..=v1).contains(&self.sel.0) {
                let x = x_of(self.sel.0);
                p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, Color32::WHITE));
            }
        } else {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "Nothing recorded yet. Press Record, or open a sound.", egui::FontId::proportional(13.0), TEXT_DIM);
        }
        // the loop along the bottom, the flags along the top
        if self.lp.1 > self.lp.0 && self.lp.1 > v0 && self.lp.0 < v1 {
            let (a, b) = (x_of(self.lp.0.max(v0)), x_of(self.lp.1.min(v1)));
            p.rect_filled(egui::Rect::from_min_max(Pos2::new(a, rect.bottom() - 5.0), Pos2::new(b, rect.bottom())), 0.0, Color32::from_rgb(110, 200, 120));
        }
        let flag = Color32::from_rgb(240, 190, 90);
        let mut over_mark = None;
        let hover_pos = resp.hover_pos().or(resp.interact_pointer_pos());
        for (k, m) in self.marks.iter().enumerate() {
            if m.at + m.len < v0 || m.at > v1 {
                continue;
            }
            let x = x_of(m.at.max(v0));
            if m.len > 0 {
                let e = x_of((m.at + m.len).min(v1));
                p.rect_filled(egui::Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new(e, rect.top() + 4.0)), 0.0, flag.gamma_multiply(0.7));
            }
            if m.at >= v0 {
                p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, flag.gamma_multiply(0.6)));
                let text = if m.name.is_empty() { format!("{}", k + 1) } else { m.name.clone() };
                let g = p.text(Pos2::new(x + 3.0, rect.top() + 5.0), egui::Align2::LEFT_TOP, text, egui::FontId::proportional(10.0), flag);
                if hover_pos.is_some_and(|h| h.y < rect.top() + 18.0 && h.x >= x - 4.0 && h.x <= g.right() + 2.0) {
                    over_mark = Some(k);
                }
            }
        }
        if resp.secondary_clicked() {
            self.menu_mark = over_mark;
        }
        let mut zoom_to = None;
        resp.context_menu(|ui| {
            if let Some(k) = self.menu_mark.filter(|&k| k < self.marks.len()) {
                if ui.button("Rename").clicked() {
                    self.rename = Some(k);
                    ui.close_menu();
                }
                if ui.button("Select").clicked() {
                    self.sel = self.mark_span(k);
                    ui.close_menu();
                }
                if ui.button("Zoom to").clicked() {
                    zoom_to = Some(self.mark_span(k));
                    ui.close_menu();
                }
                if ui.button("Clone").clicked() {
                    let m = self.marks[k].clone();
                    let (a, b) = self.mark_span(k);
                    self.add_mark(b.min(self.frames.len().saturating_sub(1)), if m.len > 0 { (b - a).min(self.frames.len() - b) } else { 0 }, m.name);
                    ui.close_menu();
                }
                if self.marks[k].len > 0 && ui.button("Turn into a marker").clicked() {
                    self.marks[k].len = 0;
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    self.marks.remove(k);
                    self.menu_mark = None;
                    ui.close_menu();
                }
            } else {
                for (k, name) in [(23u8, "Add a marker here"), (22, "Make the marked stretch a region"), (24, "Make the marked stretch the loop")] {
                    if ui.button(name).clicked() {
                        edit = Some(k);
                        ui.close_menu();
                    }
                }
            }
        });
        if let Some((a, b)) = zoom_to.filter(|z| z.1 > z.0) {
            self.view = (a, b);
        }
        let (v0, v1) = self.view;
        let span = (v1 - v0).max(1);
        let at_of = |x: f32| (v0 as f64 + ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * span as f64).round() as usize;
        // marking, zooming and moving about
        let anchor_id = resp.id.with("anchor");
        if has && !recording {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                // from where the button went down, not from where the drag was noticed
                if let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                    let a = at_of(pos.x);
                    // shift carries on from the far end of what is marked
                    let anchor = if ui.input(|i| i.modifiers.shift) && self.sel.1 > self.sel.0 { if a < (self.sel.0 + self.sel.1) / 2 { self.sel.1 } else { self.sel.0 } } else { a };
                    ui.data_mut(|d| d.insert_temp(anchor_id, anchor));
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let (Some(pos), Some(anchor)) = (resp.interact_pointer_pos(), ui.data(|d| d.get_temp::<usize>(anchor_id))) {
                    let a = at_of(pos.x);
                    self.sel = (a.min(anchor), a.max(anchor));
                }
            }
            if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let a = at_of(pos.x);
                    self.sel = (a, a);
                }
            }
            if resp.double_clicked() {
                // a flag marks its own stretch; anywhere else marks from the flag before to the flag after
                self.sel = match (over_mark, resp.interact_pointer_pos()) {
                    (Some(k), _) => self.mark_span(k),
                    (None, Some(pos)) => {
                        let a = at_of(pos.x);
                        (self.marks.iter().map(|m| m.at).filter(|&m| m <= a).max().unwrap_or(0), self.marks.iter().map(|m| m.at).find(|&m| m > a).unwrap_or(n))
                    }
                    _ => (0, n),
                };
            }
        }
        ui.data_mut(|d| d.insert_temp(over_id, resp.hovered()));
        let mut send_key = false;
        if has && resp.hovered() {
            let (scroll, zoom, hover) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            let wheel = if zoom != 1.0 { zoom } else if scroll.y != 0.0 && !ui.input(|i| i.modifiers.shift) { (scroll.y * 0.004).exp() } else { 1.0 };
            if wheel != 1.0 {
                let centre = hover.map(|h| at_of(h.x)).unwrap_or((v0 + v1) / 2) as f64;
                let new = ((span as f64 / wheel as f64).round() as usize).clamp(32.min(n), n);
                let left = (centre - (centre - v0 as f64) * new as f64 / span as f64).max(0.0) as usize;
                let left = left.min(n - new);
                self.view = (left, left + new);
            }
            let pan = if ui.input(|i| i.modifiers.shift) { scroll.y + scroll.x } else { scroll.x };
            if pan != 0.0 {
                let d = (pan as f64 / rect.width() as f64 * span as f64) as i64;
                let left = (v0 as i64 - d).clamp(0, (n - span.min(n)) as i64) as usize;
                self.view = (left, left + span.min(n));
            }
            // the editor's own keys, while the pointer is over the picture
            let key = |ui: &mut Ui, m: egui::Modifiers, k: egui::Key| ui.input_mut(|i| i.consume_key(m, k));
            use egui::{Key, Modifiers as M};
            if !recording {
                let typing = ui.memory(|m| m.focused().is_some());
                send_key = !typing && key(ui, M::SHIFT, Key::C);
                // the ones with more keys held come first, so the plain ones do not take them
                for (m, k, e) in [
                    (M::CTRL | M::SHIFT, Key::Delete, 25u8),
                    (M::CTRL, Key::Delete, 1u8),
                    (M::SHIFT, Key::Delete, 6),
                    (M::ALT, Key::Delete, 17),
                    (M::NONE, Key::Delete, 2),
                    (M::CTRL, Key::X, 3),
                    (M::CTRL, Key::C, 4),
                    (M::CTRL, Key::V, 5),
                    (M::ALT, Key::V, 14),
                    (M::SHIFT, Key::V, 15),
                    (M::NONE, Key::Insert, 16),
                    (M::ALT, Key::ArrowLeft, 7),
                    (M::CTRL, Key::ArrowLeft, 43),
                    (M::CTRL, Key::ArrowRight, 44),
                    (M::NONE, Key::ArrowLeft, 26),
                    (M::NONE, Key::ArrowRight, 27),
                    (M::NONE, Key::ArrowUp, 28),
                    (M::ALT, Key::M, 22),
                    (M::NONE, Key::M, 23),
                    (M::ALT, Key::L, 24),
                    (M::ALT, Key::N, 30),
                    (M::ALT, Key::U, 31),
                    (M::CTRL, Key::U, 32),
                    (M::ALT, Key::S, 38),
                    (M::ALT, Key::A, 13),
                    (M::ALT, Key::T, 60),
                    (M::ALT, Key::W, 67),
                    (M::CTRL, Key::T, 61),
                    (M::CTRL, Key::R, 62),
                    (M::CTRL, Key::B, 63),
                    (M::CTRL, Key::Q, 64),
                    (M::CTRL, Key::L, 65),
                    (M::NONE, Key::S, 66),
                    (M::NONE, Key::PageUp, 45),
                    (M::NONE, Key::PageDown, 46),
                    (M::NONE, Key::Home, 47),
                    (M::NONE, Key::End, 48),
                    (M::CTRL, Key::N, 8),
                    (M::CTRL, Key::F, 9),
                    (M::ALT, Key::F, 10),
                    (M::CTRL, Key::Z, 20),
                    (M::CTRL, Key::A, 21),
                ] {
                    let plain = m == M::NONE || m == M::SHIFT;
                    if !(plain && typing) && key(ui, m, k) {
                        edit = Some(e);
                    }
                }
            }
        }
        if let Some(e) = edit {
            let (a, b) = self.range();
            match e {
                20 => {
                    if let Some(s) = self.undo.pop() {
                        self.rev += 1;
                        self.frames = s.frames;
                        self.sel = s.sel;
                        self.marks = s.marks;
                        self.lp = s.lp;
                        self.fit();
                    }
                }
                21 => self.sel = (0, self.frames.len()),
                4 => self.clip = self.frames[a..b].to_vec(),
                28 => self.sel = (self.sel.0, self.sel.0),
                60..=65 => self.tool = Some(e - 10),
                67 => self.tool = Some(56),
                66 => self.spec = !self.spec,
                26 | 27 => {
                    // step to the stretch before or after, flag to flag
                    let here = self.sel.0;
                    let k = if e == 27 { self.marks.iter().position(|m| m.at > here) } else { self.marks.iter().rposition(|m| m.at < here) };
                    if let Some(k) = k {
                        self.sel = self.mark_span(k);
                        if self.sel.0 < self.view.0 || self.sel.1 > self.view.1 {
                            let w = (self.view.1 - self.view.0).max(self.sel.1 - self.sel.0);
                            let left = self.sel.0.min(self.frames.len() - w.min(self.frames.len()));
                            self.view = (left, (left + w).min(self.frames.len()));
                        }
                    }
                }
                30 => {
                    self.thresh = ops::peak(&self.frames, a, b).max(1e-5);
                    act = Some(WaveAct::Status(format!("Noise level set to {:.1} dB", 20.0 * self.thresh.log10())));
                }
                31 => {
                    self.profile = ops::noise_profile(&self.frames, a, b);
                    act = Some(WaveAct::Status("Noise learned. Mark what to clean and press Ctrl+U.".into()));
                }
                43..=48 => {
                    let n = self.frames.len();
                    let (v0, v1) = self.view;
                    let w = v1 - v0;
                    let around = |mid: usize, w: usize| {
                        let w = w.clamp(32.min(n), n);
                        let left = mid.saturating_sub(w / 2).min(n - w);
                        (left, left + w)
                    };
                    self.view = match e {
                        43 => around(self.sel.0, w),
                        44 => around(self.sel.1, w),
                        45 => around((v0 + v1) / 2, w / 2),
                        46 => around((v0 + v1) / 2, w * 2),
                        47 => (0, n),
                        _ if self.sel.1 > self.sel.0 => (self.sel.0, self.sel.1.min(n)),
                        _ => (v0, v1),
                    };
                }
                22..=25 | 38..=40 => {
                    self.keep();
                    match e {
                        22 if self.sel.1 > self.sel.0 => self.add_mark(a, b - a, String::new()),
                        23 => self.add_mark(self.sel.0.min(self.frames.len().saturating_sub(1)), 0, String::new()),
                        24 => self.lp = if self.sel.1 > self.sel.0 { (a, b) } else { (0, 0) },
                        25 => {
                            self.marks.retain(|m| m.at < a || m.at >= b);
                            if self.lp.0 >= a && self.lp.0 < b {
                                self.lp = (0, 0);
                            }
                        }
                        38..=40 => {
                            let sense = [0.5, 0.2, 0.85][(e - 38) as usize];
                            let hits = ops::onsets(&self.frames, a, b, self.rate, sense);
                            let had = self.marks.len();
                            for h in hits {
                                self.add_mark(h, 0, String::new());
                            }
                            act = Some(WaveAct::Status(format!("{} slices marked", self.marks.len() - had)));
                        }
                        _ => {}
                    }
                }
                _ => {
                    self.keep();
                    match e {
                        1 => {
                            let n = self.frames.len();
                            ops::trim(&mut self.frames, a, b);
                            self.took_out(b, n);
                            self.took_out(0, a);
                            self.sel = (0, 0);
                            self.fit();
                        }
                        2 | 3 => {
                            let cut = ops::remove(&mut self.frames, a, b);
                            self.took_out(a, b);
                            if e == 3 {
                                self.clip = cut;
                            }
                            self.sel = (a, a);
                            self.fit();
                        }
                        5 => {
                            let at = if self.sel.1 > self.sel.0 {
                                ops::remove(&mut self.frames, a, b);
                                self.took_out(a, b);
                                a
                            } else {
                                self.sel.0.min(self.frames.len())
                            };
                            ops::insert(&mut self.frames, at, &self.clip);
                            self.put_in(at, self.clip.len());
                            self.sel = (at, at + self.clip.len());
                            self.fit();
                        }
                        14 | 15 => {
                            let at = self.sel.0.min(self.frames.len());
                            let grew = self.frames.len() < at + self.clip.len();
                            if e == 14 {
                                let end = (at + self.clip.len()).min(self.frames.len());
                                ops::silence(&mut self.frames, at, end);
                            }
                            ops::mix(&mut self.frames, at, &self.clip);
                            self.sel = (at, at + self.clip.len());
                            if grew {
                                self.fit();
                            }
                        }
                        16 => {
                            let (at, len) = if self.sel.1 > self.sel.0 { (a, b - a) } else { (self.sel.0.min(self.frames.len()), self.rate as usize) };
                            ops::insert(&mut self.frames, at, &vec![[0.0; 2]; len]);
                            self.put_in(at, len);
                            self.sel = (at, at + len);
                            self.fit();
                        }
                        17 if self.lp.1 > self.lp.0 && self.lp.1 < self.frames.len() => {
                            let n = self.frames.len();
                            ops::remove(&mut self.frames, self.lp.1, n);
                            self.took_out(self.lp.1, n);
                            self.sel = (0, 0);
                            self.fit();
                        }
                        32 => ops::denoise(&mut self.frames, a, b, &self.profile, self.clean),
                        33 => ops::gate(&mut self.frames, a, b, self.thresh, self.rate),
                        34 | 35 => {
                            let loud = ops::loud_parts(&self.frames, a, b, self.thresh, self.rate);
                            if let (Some(first), Some(last)) = (loud.first().copied(), loud.last().copied()) {
                                // from the back forward, so the places further on stay where they were
                                let mut holes = vec![(last.1, b)];
                                if e == 35 {
                                    holes.extend(loud.windows(2).rev().map(|w| (w[0].1, w[1].0)));
                                }
                                holes.push((a, first.0));
                                let mut joins = vec![];
                                for (s, t) in holes {
                                    if t > s {
                                        ops::remove(&mut self.frames, s, t);
                                        self.took_out(s, t);
                                        for j in joins.iter_mut() {
                                            *j -= t - s;
                                        }
                                    }
                                    joins.push(s);
                                }
                                if e == 35 {
                                    // every piece that was kept starts at a join
                                    for j in joins.into_iter().skip(1) {
                                        if j < self.frames.len() {
                                            self.add_mark(j, 0, String::new());
                                        }
                                    }
                                }
                                self.sel = (0, 0);
                                self.fit();
                            }
                        }
                        50 => {
                            let new = ops::stretch(&self.frames[a..b], self.time, self.semis, self.rate);
                            self.swap_in(a, b, new);
                        }
                        51 => {
                            let cuts: Vec<usize> = self.marks.iter().filter(|m| m.at > a && m.at < b).map(|m| m.at - a).collect();
                            let new = ops::drum_stretch(&self.frames[a..b], &cuts, self.time, self.rate);
                            self.swap_in(a, b, new);
                        }
                        52 => {
                            let room = if self.room_clip && !self.clip.is_empty() { self.clip.clone() } else { ops::noise_impulse(self.rate, self.room_secs) };
                            ops::convolve(&mut self.frames, a, b, &room, self.room_wet);
                            self.fit();
                        }
                        53 => ops::blur(&mut self.frames, a, b, self.blur),
                        54 => ops::equalize(&mut self.frames, a, b, self.rate, &self.eq),
                        56 => {
                            let period = (60.0 / self.claw_bpm.max(1.0) * self.claw_beats * self.rate as f32) as usize;
                            let new = ops::claw(&self.frames[a..b], period, self.claw_every as usize, self.rate);
                            self.swap_in(a, b, new);
                        }
                        55 => {
                            let (la, lb) = if self.lp.1 > self.lp.0 { self.lp } else { (a, b) };
                            ops::tune_loop(&mut self.frames, la, lb, (self.join_ms / 1000.0 * self.rate as f32) as usize);
                        }
                        36 => ops::declick(&mut self.frames, a, b, self.rate, true),
                        37 => ops::declick(&mut self.frames, a, b, self.rate, false),
                        6 => ops::silence(&mut self.frames, a, b),
                        7 => ops::reverse(&mut self.frames, a, b),
                        8 => ops::normalize(&mut self.frames, a, b),
                        9 => ops::fade(&mut self.frames, a, b, true),
                        10 => ops::fade(&mut self.frames, a, b, false),
                        11 => ops::centre(&mut self.frames, a, b),
                        12 => ops::swap(&mut self.frames, a, b),
                        13 => ops::gain(&mut self.frames, a, b, 10f32.powf(self.amp_db / 20.0)),
                        _ => {}
                    }
                }
            }
        }
        ui.horizontal(|ui| {
            let (a, b) = self.range();
            let what = if self.sel.1 > self.sel.0 { format!("Marked {} to {} ({})", self.time(a), self.time(b), self.time(b - a)) } else { format!("Cursor {}", self.time(self.sel.0.min(self.frames.len()))) };
            ui.label(RichText::new(format!("{}   Length {}   {} Hz   peak {:.1} dB", what, self.time(self.frames.len()), self.rate, 20.0 * ops::peak(&self.frames, a, b).max(1e-6).log10())).small().color(TEXT_DIM));
            if ui.small_button("Show all").clicked() {
                self.fit();
            }
            if ui.add_enabled(self.sel.1 > self.sel.0, egui::Button::new("Show marked").small()).clicked() {
                self.view = (self.sel.0, self.sel.1.min(self.frames.len()));
            }
        });
        if let Some(k) = self.rename.filter(|&k| k < self.marks.len()) {
            ui.horizontal(|ui| {
                ui.label("Name of the flag");
                let r = ui.add(egui::TextEdit::singleline(&mut self.marks[k].name).desired_width(200.0));
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                if ui.button("Done").clicked() || r.lost_focus() {
                    self.rename = None;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has && !recording, |ui| {
                let (a, b) = self.range();
                if ui.button("Send to playlist").hint("Save the marked stretch (or everything) beside the project and place it as an audio clip (Shift+C)").clicked() || (send_key && has && !recording) {
                    act = Some(match save_take(dir, &self.name, &self.frames[a..b], self.rate, &self.marks_in(a, b)) {
                        Ok(p) => WaveAct::ToPlaylist(p),
                        Err(e) => WaveAct::Status(e),
                    });
                }
                if ui.button("Send to a new channel").hint("Save the marked stretch (or everything) beside the project and load it into a new sampler channel").clicked() {
                    act = Some(match save_take(dir, &self.name, &self.frames[a..b], self.rate, &self.marks_in(a, b)) {
                        Ok(p) => WaveAct::ToChannel(p),
                        Err(e) => WaveAct::Status(e),
                    });
                }
            });
        });
        ui.label(RichText::new("Drag to mark a stretch, click to set the cursor, double-click to mark from flag to flag. Right-click for markers and regions. Scroll to zoom, Shift+scroll to move along.").small().color(TEXT_DIM));
        act
    }
}
