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
    /// The files on show, top to bottom, for the arrow keys to walk.
    visible: Vec<PathBuf>,
    /// The picked file has just moved by key and wants scrolling into view.
    reveal: bool,
    tab: Tab,
    /// What the search box holds, and the files found for it.
    query: String,
    hits: Option<(String, Vec<PathBuf>)>,
    /// Ctrl+F: the search box takes the keyboard.
    find: bool,
}

/// The browser's pages.
#[derive(Default, Clone, Copy, PartialEq)]
enum Tab {
    #[default]
    All,
    Project,
    Plugins,
}

const SHOWN: [&str; 7] = ["flp", "fst", "fsc", "mid", "gmsynth", "tbio", "mrp"];

/// Every file under the folders whose name holds all the words asked for (a few hundred at most).
fn search(roots: &[(String, PathBuf)], query: &str) -> Vec<PathBuf> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
    let mut out = Vec::new();
    let mut todo: Vec<(PathBuf, u32)> = roots.iter().map(|r| (r.1.clone(), 0)).collect();
    let mut seen = 0usize;
    while let Some((dir, depth)) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > 400_000 || out.len() >= 400 {
                out.sort();
                return out;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with('.') {
                continue;
            }
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                if depth < 24 {
                    todo.push((p, depth + 1));
                }
            } else if words.iter().all(|w| name.contains(w.as_str())) {
                let ext = ext_of(&p);
                if AUDIO.contains(&ext.as_str()) || SHOWN.contains(&ext.as_str()) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
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

    pub fn remove_root(&mut self, path: &Path) {
        self.roots.retain(|r| r.1 != path);
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
                            (!hidden && (is_dir || AUDIO.contains(&ext.as_str()) || SHOWN.contains(&ext.as_str()))).then_some((p, is_dir))
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
                self.browser_file(ui, &path, &name);
            }
        }
    }

    /// One file's row: click to hear or load it, drag it out, right-click for more.
    fn browser_file(&mut self, ui: &mut Ui, path: &Path, name: &str) {
        let path = path.to_path_buf();
        {
            {
                let ext = ext_of(&path);
                let project = ext == "flp";
                let preset = cereal_engine::preset::EXTS.contains(&ext.as_str());
                let score = ext == "fsc" || ext == "mid";
                let selected = self.browser.selected.as_deref() == Some(path.as_path());
                let text = RichText::new(name).color(if project { ACCENT } else if selected { GREEN } else { TEXT_DIM });
                let resp = ui.add(egui::Label::new(text).selectable(false).sense(Sense::click_and_drag()).truncate());
                self.browser.visible.push(path.clone());
                if selected && self.browser.reveal {
                    resp.scroll_to_me(None);
                    self.browser.reveal = false;
                }
                let mods = ui.input(|i| i.modifiers);
                if resp.clicked() && mods.command && mods.alt {
                    self.focus = crate::Win::Browser;
                    self.browser.selected = Some(path.clone());
                    self.browser_replace();
                } else if resp.clicked() {
                    self.focus = crate::Win::Browser;
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
                        if ui.button("Add to the playlist").hint("As an audio clip where the song stands").clicked() {
                            let at = if self.handle.transport.song_mode() { self.handle.transport.tick() as u32 } else { 0 };
                            self.add_audio_clip(&path, at, None);
                            ui.close_menu();
                        }
                    }
                });
            }
        }
    }

    /// Put the picked file in the selected channel: a sound replaces its sample, a preset its settings.
    fn browser_replace(&mut self) {
        let Some(path) = self.browser.selected.clone() else { return };
        let ext = ext_of(&path);
        if ext == "flp" {
            self.actions.push(Action::Open(path));
        } else if ext == "fsc" || ext == "mid" {
            self.actions.push(Action::LoadScore(path, None));
        } else if cereal_engine::preset::EXTS.contains(&ext.as_str()) {
            self.actions.push(Action::LoadPreset(path, crate::PresetTo::Auto));
        } else {
            self.actions.push(Action::LoadSample(path, Some(self.sel_chan)));
        }
    }

    /// Keys while the browser has them: the arrows walk the files on show and play each, Enter loads the one picked.
    pub(crate) fn browser_key(&mut self, key: egui::Key, m: egui::Modifiers) -> bool {
        if m.command && !m.alt && !m.shift && key == egui::Key::F {
            self.browser.tab = Tab::All;
            self.browser.find = true;
            return true;
        }
        if m.command || m.alt {
            return false;
        }
        match key {
            egui::Key::ArrowUp | egui::Key::ArrowDown => {
                let v = &self.browser.visible;
                if v.is_empty() {
                    return true;
                }
                let at = self.browser.selected.as_ref().and_then(|s| v.iter().position(|p| p == s));
                let to = match (at, key == egui::Key::ArrowDown) {
                    (Some(i), true) => (i + 1).min(v.len() - 1),
                    (Some(i), false) => i.saturating_sub(1),
                    (None, _) => 0,
                };
                let path = v[to].clone();
                if AUDIO.contains(&ext_of(&path).as_str()) {
                    self.actions.push(Action::Preview(path.clone()));
                }
                self.browser.selected = Some(path);
                self.browser.reveal = true;
            }
            egui::Key::Enter => self.browser_replace(),
            egui::Key::Escape if !self.browser.query.is_empty() => self.browser.query.clear(),
            _ => return false,
        }
        true
    }

    pub(crate) fn browser_ui(&mut self, ui: &mut Ui) {
        self.browser.visible.clear();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Browser").strong().color(TEXT));
            if ui.small_button("Refresh").clicked() {
                self.browser.refresh();
            }
            if ui.small_button("Add folder").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.add_search_folder(dir);
                }
            }
        });
        ui.horizontal(|ui| {
            for (tab, label, tip) in [(Tab::All, "All", "Every folder"), (Tab::Project, "Current project", "What this project is made of"), (Tab::Plugins, "Plug-in database", "The instruments and effects")] {
                if ui.selectable_label(self.browser.tab == tab, RichText::new(label).small()).hint(tip).clicked() {
                    self.browser.tab = tab;
                }
            }
        });
        match self.browser.tab {
            Tab::All => {}
            Tab::Project => return self.browser_project(ui),
            Tab::Plugins => return self.browser_plugins(ui),
        }
        let find = ui.add(egui::TextEdit::singleline(&mut self.browser.query).hint_text("Find (Ctrl+F)").desired_width(f32::INFINITY));
        if std::mem::take(&mut self.browser.find) {
            find.request_focus();
        }
        ui.separator();
        let query = self.browser.query.trim().to_string();
        if !query.is_empty() {
            if self.browser.hits.as_ref().is_none_or(|h| h.0 != query) {
                self.browser.hits = Some((query.clone(), search(&self.browser.roots, &query)));
            }
            let hits = self.browser.hits.as_ref().map(|h| h.1.clone()).unwrap_or_default();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if hits.is_empty() {
                    ui.label(RichText::new("Nothing found.").color(TEXT_DIM));
                }
                for path in hits {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.browser_file(ui, &path, &name);
                }
            });
            return self.browser_drag(ui);
        }
        self.browser.hits = None;
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
        self.browser_drag(ui);
    }

    /// The current project page: its history, patterns, channels, effects and sounds, each a click away.
    fn browser_project(&mut self, ui: &mut Ui) {
        ui.separator();
        let tr = self.handle.transport.clone();
        let head = |ui: &mut Ui, name: &str, open: bool, body: &mut dyn FnMut(&mut Ui)| {
            let id = ui.make_persistent_id(("project-page", name));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open)
                .show_header(ui, |ui| {
                    ui.label(RichText::new(name).strong().color(TEXT));
                })
                .body(|ui| body(ui));
        };
        let row = |ui: &mut Ui, text: String, lit: bool| ui.add(egui::Label::new(RichText::new(text).color(if lit { GREEN } else { TEXT_DIM })).selectable(false).sense(Sense::click()).truncate());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let mut jump = None;
            head(ui, "History", false, &mut |ui| {
                for k in (0..self.history.len()).rev() {
                    let text = if k == 0 { "Start".to_string() } else { format!("Edit {k}") };
                    if row(ui, text, k == self.hist_at).hint("Go back or forward to this point").clicked() {
                        jump = Some(k);
                    }
                }
            });
            if let Some(k) = jump {
                self.hist_at = k;
                self.restore();
            }
            let pat = tr.pattern();
            head(ui, "Patterns", true, &mut |ui| {
                for i in 0..self.song.patterns.len() {
                    let p = &self.song.patterns[i];
                    if p.notes.is_empty() && p.name.is_empty() && i != pat {
                        continue;
                    }
                    let r = row(ui, self.song.pattern_name(i), i == pat);
                    if r.clicked() {
                        tr.set_pattern(i);
                    }
                    if r.double_clicked() {
                        self.show_piano = true;
                    }
                }
            });
            let mut pick = None;
            head(ui, "Generators", true, &mut |ui| {
                for (i, c) in self.song.channels.iter().enumerate() {
                    if c.kind != cereal_engine::flp::ChannelKind::AutomationClip && row(ui, c.name.clone(), i == self.sel_chan).clicked() {
                        pick = Some(i);
                    }
                }
            });
            head(ui, "Automation", false, &mut |ui| {
                for (i, c) in self.song.channels.iter().enumerate() {
                    if c.kind == cereal_engine::flp::ChannelKind::AutomationClip && row(ui, c.name.clone(), i == self.sel_chan).clicked() {
                        pick = Some(i);
                    }
                }
            });
            if let Some(i) = pick {
                self.sel_chan = i;
                self.show_chanset = true;
            }
            let mut open = None;
            head(ui, "Effects", true, &mut |ui| {
                for (i, ins) in self.song.inserts.iter().enumerate() {
                    for s in &ins.slots {
                        if row(ui, format!("{}: {}", self.song.insert_name(i), s.title()), self.fx_open == Some((i, s.index))).clicked() {
                            open = Some((i, s.index));
                        }
                    }
                }
            });
            if let Some((i, s)) = open {
                self.fx_open = Some((i, s));
                self.sel_insert = i;
            }
            let mut hear = None;
            head(ui, "Sounds", false, &mut |ui| {
                let mut seen: Vec<&str> = Vec::new();
                for c in &self.song.channels {
                    if c.sample_path.is_empty() || seen.contains(&c.sample_path.as_str()) {
                        continue;
                    }
                    seen.push(&c.sample_path);
                    let name = c.sample_path.rsplit(['/', '\\']).next().unwrap_or("").to_string();
                    if row(ui, name, false).hint(c.sample_path.clone()).clicked() {
                        hear = Some(PathBuf::from(&c.sample_path));
                    }
                }
            });
            if let Some(p) = hear.filter(|p| p.is_file()) {
                self.actions.push(Action::Preview(p));
            }
        });
    }

    /// The plug-in page: every instrument and effect; a click adds one.
    fn browser_plugins(&mut self, ui: &mut Ui) {
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let id = ui.make_persistent_id("plug-generators");
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    ui.label(RichText::new("Generators").strong().color(TEXT));
                })
                .body(|ui| {
                    let mut defs: Vec<_> = cereal_engine::synth::DEFS.iter().filter(|d| !d.arrives_only()).collect();
                    defs.sort_by_key(|d| d.title);
                    for d in defs {
                        if ui.add(egui::Label::new(RichText::new(d.title).color(TEXT_DIM)).selectable(false).sense(Sense::click()).truncate()).hint("Click to add a channel with this instrument").clicked() {
                            self.song.channels.push(cereal_engine::song::Chan::instrument(d.kind));
                            self.sel_chan = self.song.channels.len() - 1;
                            self.dirty = true;
                        }
                    }
                });
            let id = ui.make_persistent_id("plug-effects");
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    ui.label(RichText::new("Effects").strong().color(TEXT));
                })
                .body(|ui| {
                    for d in Self::effect_defs() {
                        if ui.add(egui::Label::new(RichText::new(d.title).color(TEXT_DIM)).selectable(false).sense(Sense::click()).truncate()).hint("Click to put it in the first free slot of the selected mixer track").clicked() {
                            let ins = self.sel_insert.min(self.song.inserts.len().saturating_sub(1));
                            let used: Vec<u16> = self.song.inserts.get(ins).map(|i| i.slots.iter().map(|s| s.index).collect()).unwrap_or_default();
                            if let Some(slot) = (0..10u16).find(|s| !used.contains(s)) {
                                self.set_effect(ins, slot, d.kind, d.ident);
                                self.show_mixer = true;
                                self.dirty = true;
                            } else {
                                self.status = "Every slot of the selected mixer track is in use".into();
                            }
                        }
                    }
                });
        });
    }

    fn browser_drag(&mut self, ui: &mut Ui) {
        // a sample being dragged follows the pointer
        if let Some(path) = egui::DragAndDrop::payload::<PathBuf>(ui.ctx()) {
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag-sample"))).text(pos + egui::vec2(12.0, 4.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(12.0), ACCENT);
            }
        }
    }
}
