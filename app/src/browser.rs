//! The browser: folders of samples and projects down the left edge.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{RichText, Sense, Ui};

use crate::widgets::*;
use crate::{Action, App};

#[derive(Default)]
pub struct State {
    pub roots: Vec<(String, PathBuf)>,
    listing: HashMap<PathBuf, Vec<(PathBuf, bool)>>,
    selected: Option<PathBuf>,
}

const AUDIO: [&str; 7] = ["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"];

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

impl State {
    pub fn add_root(&mut self, label: &str, path: PathBuf) {
        if path.is_dir() && !self.roots.iter().any(|r| r.1 == path) {
            self.roots.push((label.to_string(), path));
        }
    }

    pub fn refresh(&mut self) {
        self.listing.clear();
    }

    fn list(&mut self, dir: &Path) -> &Vec<(PathBuf, bool)> {
        self.listing.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut v: Vec<(PathBuf, bool)> = std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten()
                        .filter_map(|e| {
                            let p = e.path();
                            let is_dir = p.is_dir();
                            let ext = ext_of(&p);
                            let hidden = e.file_name().to_string_lossy().starts_with('.');
                            (!hidden && (is_dir || AUDIO.contains(&ext.as_str()) || ["flp", "fst", "fsc", "mid"].contains(&ext.as_str()))).then_some((p, is_dir))
                        })
                        .collect()
                })
                .unwrap_or_default();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.file_name().map(|n| n.to_string_lossy().to_lowercase()).cmp(&b.0.file_name().map(|n| n.to_string_lossy().to_lowercase()))));
            v
        })
    }
}

impl App {
    fn browser_dir(&mut self, ui: &mut Ui, dir: &Path, depth: usize) {
        let entries = self.browser.list(dir).clone();
        for (path, is_dir) in entries {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if is_dir {
                let id = ui.make_persistent_id(&path);
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                    .show_header(ui, |ui| {
                        ui.label(RichText::new(&name).color(TEXT));
                    })
                    .body(|ui| {
                        if depth < 24 {
                            self.browser_dir(ui, &path, depth + 1);
                        }
                    });
            } else {
                let ext = ext_of(&path);
                let project = ext == "flp";
                let preset = ext == "fst";
                let score = ext == "fsc" || ext == "mid";
                let selected = self.browser.selected.as_deref() == Some(path.as_path());
                let text = RichText::new(&name).color(if project { ACCENT } else if selected { GREEN } else { TEXT_DIM });
                let resp = ui.add(egui::Label::new(text).selectable(false).sense(Sense::click_and_drag()).truncate());
                if resp.clicked() {
                    self.browser.selected = Some(path.clone());
                    if preset {
                        self.actions.push(Action::LoadPreset(path.clone(), crate::PresetTo::Auto));
                    } else if !project && !score {
                        self.actions.push(Action::Preview(path.clone()));
                    }
                }
                if resp.double_clicked() && project {
                    self.actions.push(Action::Open(path.clone()));
                }
                if resp.drag_started() && !project {
                    egui::DragAndDrop::set_payload(ui.ctx(), path.clone());
                }
                resp.context_menu(|ui| {
                    if project {
                        if ui.button("Open project").clicked() {
                            self.actions.push(Action::Open(path.clone()));
                            ui.close_menu();
                        }
                    } else if score {
                        if ui.button("Open in piano roll").clicked() {
                            self.actions.push(Action::LoadScore(path.clone(), None));
                            ui.close_menu();
                        }
                    } else if preset {
                        if ui.button("Load preset").clicked() {
                            self.actions.push(Action::LoadPreset(path.clone(), crate::PresetTo::Auto));
                            ui.close_menu();
                        }
                    } else {
                        if ui.button("Open in new channel").clicked() {
                            self.actions.push(Action::LoadSample(path.clone(), None));
                            ui.close_menu();
                        }
                        if ui.button("Replace selected channel").clicked() {
                            self.actions.push(Action::LoadSample(path.clone(), Some(self.sel_chan)));
                            ui.close_menu();
                        }
                    }
                });
            }
        }
    }

    pub(crate) fn browser_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Browser").strong().color(TEXT));
            if ui.small_button("Refresh").clicked() {
                self.browser.refresh();
            }
            if ui.small_button("Add folder").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    let label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string());
                    self.lib.search_dirs.push(dir.clone());
                    self.lib.rescan();
                    self.browser.add_root(&label, dir);
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let roots = self.browser.roots.clone();
            if roots.is_empty() {
                ui.label(RichText::new("No folders yet. Use Add folder to point at your samples and projects.").color(TEXT_DIM));
            }
            for (label, path) in roots {
                let id = ui.make_persistent_id(("root", &path));
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                    .show_header(ui, |ui| {
                        ui.label(RichText::new(&label).strong().color(TEXT));
                    })
                    .body(|ui| self.browser_dir(ui, &path, 0));
            }
        });
        // a sample being dragged follows the pointer
        if let Some(path) = egui::DragAndDrop::payload::<PathBuf>(ui.ctx()) {
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag-sample"))).text(pos + egui::vec2(12.0, 4.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(12.0), ACCENT);
            }
        }
    }
}
