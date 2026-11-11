//! The wave recorder: records the sound passing its mixer slot, shows it, and edits it.

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
    undo: Vec<(Vec<[f32; 2]>, (usize, usize))>,
    clip: Vec<[f32; 2]>,
    name: String,
    on_play: bool,
    amp_db: f32,
}

impl Default for WaveEd {
    fn default() -> Self {
        Self { frames: Vec::new(), rate: 48_000, sel: (0, 0), view: (0, 0), undo: Vec::new(), clip: Vec::new(), name: "Take".into(), on_play: false, amp_db: 3.0 }
    }
}

/// Writes frames to a new WAV file in `dir` and gives its path.
pub fn save_take(dir: &Path, name: &str, frames: &[[f32; 2]], rate: u32) -> Result<PathBuf, String> {
    if frames.is_empty() {
        return Err("there is nothing to save yet".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let clean = if clean.trim().is_empty() { "Take".to_string() } else { clean.trim().to_string() };
    let path = (0..10_000).map(|i| dir.join(if i == 0 { format!("{clean}.wav") } else { format!("{clean} {}.wav", i + 1) })).find(|p| !p.exists()).ok_or("no free file name")?;
    write_wav(&path, frames, rate)?;
    Ok(path)
}

fn write_wav(path: &Path, frames: &[[f32; 2]], rate: u32) -> Result<(), String> {
    let flat: Vec<f32> = frames.iter().flat_map(|f| [f[0], f[1]]).collect();
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    cereal_engine::wav::write(std::io::BufWriter::new(f), &flat, rate, 2, 24).map_err(|e| e.to_string())
}

impl WaveEd {
    /// The stretch the tools work on: what is marked, or everything.
    fn range(&self) -> (usize, usize) {
        if self.sel.1 > self.sel.0 { (self.sel.0, self.sel.1.min(self.frames.len())) } else { (0, self.frames.len()) }
    }

    fn keep(&mut self) {
        self.undo.push((self.frames.clone(), self.sel));
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
        ui.horizontal(|ui| {
            let label = if recording { RichText::new("Stop recording").color(Color32::from_rgb(240, 110, 90)) } else { RichText::new("Record") };
            if ui.button(label).on_hover_text("Record the sound arriving at this slot. New sound joins the end of the take.").clicked() {
                if recording {
                    tr.record_slot(None);
                    self.on_play = false;
                } else {
                    self.keep();
                    tr.record_slot(Some(slot));
                }
            }
            ui.checkbox(&mut self.on_play, "with the song").on_hover_text("Start recording when the song starts and stop when it stops");
            ui.separator();
            if ui.button("▶ Play").on_hover_text("Play the marked stretch, or everything").clicked() {
                let (a, b) = self.range();
                let (a, b) = if self.sel.1 == self.sel.0 && self.sel.0 < self.frames.len() { (self.sel.0, self.frames.len()) } else { (a, b) };
                if b > a {
                    handle.send(Cmd::Preview(Arc::new(Sample { name: self.name.clone(), rate: self.rate, mono: false, frames: self.frames[a..b].to_vec() })));
                }
            }
            if ui.button("Stop").clicked() {
                handle.send(Cmd::StopPreview);
            }
            ui.separator();
            if ui.button("Open...").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Sound", &["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"]).pick_file() {
                    match Sample::load(&p) {
                        Ok(s) => {
                            self.keep();
                            self.rate = s.rate;
                            self.name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Take".into());
                            self.frames = s.frames;
                            self.sel = (0, 0);
                            self.fit();
                        }
                        Err(e) => act = Some(WaveAct::Status(e)),
                    }
                }
            }
            if ui.add_enabled(!self.frames.is_empty(), egui::Button::new("Save as...")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).set_file_name(format!("{}.wav", self.name)).save_file() {
                    let (a, b) = self.range();
                    act = Some(WaveAct::Status(match write_wav(&p, &self.frames[a..b], self.rate) {
                        Ok(()) => format!("Saved {}", p.display()),
                        Err(e) => e,
                    }));
                }
            }
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(120.0)).on_hover_text("Name of the take");
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
                    if ui.button(name).on_hover_text(tip).clicked() {
                        edit = Some(k);
                    }
                }
                if ui.button("Amplify").on_hover_text("Make the marked stretch louder or softer by the amount beside").clicked() {
                    edit = Some(13);
                }
                ui.add(egui::DragValue::new(&mut self.amp_db).range(-48.0..=48.0).speed(0.1).suffix(" dB"));
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() {
                    edit = Some(20);
                }
            });
        });
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
        let at_of = |x: f32| (v0 as f64 + ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * span as f64).round() as usize;
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
            if span <= cols * 2 {
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
        // marking, zooming and moving about
        let anchor_id = resp.id.with("anchor");
        if has && !recording {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                if let Some(pos) = resp.interact_pointer_pos() {
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
                self.sel = (0, n);
            }
        }
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
                for (m, k, e) in [
                    (M::CTRL, Key::Delete, 1u8),
                    (M::NONE, Key::Delete, 2),
                    (M::CTRL, Key::X, 3),
                    (M::CTRL, Key::C, 4),
                    (M::CTRL, Key::V, 5),
                    (M::ALT, Key::ArrowLeft, 7),
                    (M::CTRL, Key::N, 8),
                    (M::CTRL, Key::F, 9),
                    (M::ALT, Key::F, 10),
                    (M::CTRL, Key::Z, 20),
                    (M::CTRL, Key::A, 21),
                ] {
                    if key(ui, m, k) {
                        edit = Some(e);
                    }
                }
            }
        }
        if let Some(e) = edit {
            let (a, b) = self.range();
            match e {
                20 => {
                    if let Some((f, s)) = self.undo.pop() {
                        self.frames = f;
                        self.sel = s;
                        self.fit();
                    }
                }
                21 => self.sel = (0, self.frames.len()),
                4 => self.clip = self.frames[a..b].to_vec(),
                _ => {
                    self.keep();
                    match e {
                        1 => {
                            ops::trim(&mut self.frames, a, b);
                            self.sel = (0, 0);
                            self.fit();
                        }
                        2 | 3 => {
                            let cut = ops::remove(&mut self.frames, a, b);
                            if e == 3 {
                                self.clip = cut;
                            }
                            self.sel = (a, a);
                            self.fit();
                        }
                        5 => {
                            let at = if self.sel.1 > self.sel.0 {
                                ops::remove(&mut self.frames, a, b);
                                a
                            } else {
                                self.sel.0.min(self.frames.len())
                            };
                            ops::insert(&mut self.frames, at, &self.clip);
                            self.sel = (at, at + self.clip.len());
                            self.fit();
                        }
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
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has && !recording, |ui| {
                let (a, b) = self.range();
                if ui.button("Send to playlist").on_hover_text("Save the marked stretch (or everything) beside the project and place it as an audio clip").clicked() {
                    act = Some(match save_take(dir, &self.name, &self.frames[a..b], self.rate) {
                        Ok(p) => WaveAct::ToPlaylist(p),
                        Err(e) => WaveAct::Status(e),
                    });
                }
                if ui.button("Send to a new channel").on_hover_text("Save the marked stretch (or everything) beside the project and load it into a new sampler channel").clicked() {
                    act = Some(match save_take(dir, &self.name, &self.frames[a..b], self.rate) {
                        Ok(p) => WaveAct::ToChannel(p),
                        Err(e) => WaveAct::Status(e),
                    });
                }
            });
        });
        ui.label(RichText::new("Drag to mark a stretch, click to set the cursor, double-click to mark everything. Scroll to zoom, Shift+scroll to move along.").small().color(TEXT_DIM));
        act
    }
}
