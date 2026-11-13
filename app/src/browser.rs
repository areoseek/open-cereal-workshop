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
    /// Frames left in which every folder is being shut (Ctrl+Up).
    collapse: u8,
    /// The tag list is on show, with the tags picked and whether a file must carry all of them.
    tag_on: bool,
    tag_pick: std::collections::BTreeSet<String>,
    tag_all: bool,
    /// The file whose tags are being typed, and the text so far.
    tag_ask: Option<(PathBuf, String)>,
    /// Favourites are listed under the folders they lie in.
    star_parents: bool,
    /// A page of the user's own being renamed: its number and the text so far.
    tab_name: Option<(usize, String)>,
    /// Where the browser has been, and which of those places is on show.
    hist: Vec<(Tab, Option<PathBuf>)>,
    hist_at: usize,
}

/// A plug-in dragged out of the plug-in page.
#[derive(Clone, Copy)]
pub(crate) enum PlugDrag {
    Gen(cereal_engine::synth::Kind),
    Fx(cereal_engine::fx::Kind, &'static str),
}

impl PlugDrag {
    fn title(&self) -> &'static str {
        match self {
            PlugDrag::Gen(k) => cereal_engine::synth::def(*k).title,
            PlugDrag::Fx(k, ident) => cereal_engine::fx::DEFS.iter().find(|d| d.kind == *k && d.ident == *ident).map(|d| d.title).unwrap_or("Effect"),
        }
    }
}

/// The browser's pages.
#[derive(Default, Clone, Copy, PartialEq)]
enum Tab {
    #[default]
    All,
    Project,
    Plugins,
    Starred,
    /// One of the user's own pages, by its place in the settings.
    User(usize),
}

const SHOWN: [&str; 9] = ["flp", "fst", "fsc", "mid", "gmsynth", "tbio", "mrp", "dwp", "speech"];

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
                self.browser_folder(ui, &path, RichText::new(&name).color(TEXT), depth);
            } else {
                self.browser_file(ui, &path, &name);
            }
        }
    }

    /// A folder's row and, when open, what lies in it. A click anywhere on the row opens or shuts it.
    fn browser_folder(&mut self, ui: &mut Ui, path: &Path, label: RichText, depth: usize) {
        let id = ui.make_persistent_id(path);
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        if self.browser.collapse > 0 {
            state.set_open(false);
        }
        let head = ui.horizontal(|ui| {
            state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
            let r = ui.add(egui::Label::new(label).selectable(false).sense(Sense::click()).truncate());
            if r.clicked() {
                state.toggle(ui);
            }
            r.context_menu(|ui| {
                if ui.button("Open in new tab").clicked() {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Folder".into());
                    self.settings.tabs.push((name, 'F', path.display().to_string()));
                    self.browser.tab = Tab::User(self.settings.tabs.len() - 1);
                    let _ = self.settings.save();
                    ui.close_menu();
                }
                if ui.button("Reread").clicked() {
                    self.browser.refresh();
                    ui.close_menu();
                }
            });
        });
        state.show_body_indented(&head.response, ui, |ui| {
            if depth < 24 {
                self.browser_dir(ui, path, depth + 1);
            }
        });
    }

    fn is_starred(&self, path: &Path) -> bool {
        self.settings.starred.iter().any(|p| p == path)
    }

    fn star(&mut self, path: &Path) {
        if self.is_starred(path) {
            self.settings.starred.retain(|p| p != path);
        } else {
            self.settings.starred.push(path.to_path_buf());
        }
        let _ = self.settings.save();
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
                let starred = self.is_starred(&path);
                let row = ui.horizontal(|ui| {
                    let w = ui.available_width() - 16.0;
                    ui.set_max_width(w.max(40.0));
                    ui.add(egui::Label::new(text).selectable(false).sense(Sense::click_and_drag()).truncate())
                });
                let resp = row.inner;
                {
                    // the star at the end of the row: there for a favourite, or while the pointer is on the row
                    let at = egui::Rect::from_center_size(egui::pos2(ui.max_rect().right() - 8.0, row.response.rect.center().y), egui::vec2(13.0, 13.0));
                    let over = ui.rect_contains_pointer(row.response.rect.union(at));
                    if starred || over {
                        let star = ui.interact(at, ui.id().with(("star", &path)), Sense::click());
                        let pts: Vec<egui::Pos2> = (0..10)
                            .map(|k| {
                                let a = std::f32::consts::PI * (k as f32 / 5.0 - 0.5);
                                let r = if k % 2 == 0 { 5.5 } else { 2.4 };
                                at.center() + egui::vec2(a.cos() * r, a.sin() * r)
                            })
                            .collect();
                        let col = if starred { egui::Color32::from_rgb(240, 200, 70) } else { TEXT_DIM.gamma_multiply(0.6) };
                        ui.painter().add(egui::Shape::closed_line(pts, egui::Stroke::new(if starred { 1.6_f32 } else { 1.0_f32 }, col)));
                        if star.hint(if starred { "A favourite (click to take the star away)" } else { "Mark as a favourite" }).clicked() {
                            self.star(&path);
                        }
                    }
                }
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
                if resp.middle_clicked() {
                    self.focus = crate::Win::Browser;
                    self.browser.selected = Some(path.clone());
                    self.browser_replace();
                }
                if resp.double_clicked() && project {
                    self.actions.push(Action::Open(path.clone()));
                }
                if resp.drag_started() && !project {
                    egui::DragAndDrop::set_payload(ui.ctx(), path.clone());
                }
                resp.context_menu(|ui| {
                    if ui.button(if starred { "Favorite (remove)" } else { "Favorite" }).clicked() {
                        self.star(&path);
                        ui.close_menu();
                    }
                    if ui.button("Add a tag...").clicked() {
                        let now = self.settings.tags.iter().find(|t| t.0 == path).map(|t| t.1.join(", ")).unwrap_or_default();
                        self.browser.tag_ask = Some((path.clone(), now));
                        ui.close_menu();
                    }
                    if ui.button("Send to selected channel").clicked() {
                        self.browser.selected = Some(path.clone());
                        self.browser_replace();
                        ui.close_menu();
                    }
                    ui.separator();
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
        if m.command && !m.alt && !m.shift {
            match key {
                egui::Key::ArrowUp => self.browser.collapse = 12,
                egui::Key::R => self.browser.refresh(),
                _ => return false,
            }
            return true;
        }
        if m.command || m.alt {
            return false;
        }
        let key = match key {
            egui::Key::F2 if !self.browser.query.is_empty() => egui::Key::ArrowUp,
            egui::Key::F3 if !self.browser.query.is_empty() => egui::Key::ArrowDown,
            k => k,
        };
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
                if m.shift {
                    // straight into the selected channel
                    self.browser_replace();
                }
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
            let b = &mut self.browser;
            let now = (b.tab, b.selected.clone());
            if b.hist.is_empty() {
                b.hist.push(now);
                b.hist_at = 0;
            } else if b.hist[b.hist_at] != now {
                // a new file on the same page takes the place of the last one, so the list holds places, not every click
                if b.hist[b.hist_at].0 == now.0 && b.hist[b.hist_at].1.as_ref().and_then(|p| p.parent()) == now.1.as_ref().and_then(|p| p.parent()) {
                    b.hist[b.hist_at] = now;
                } else {
                    b.hist.truncate(b.hist_at + 1);
                    b.hist.push(now);
                    if b.hist.len() > 200 {
                        b.hist.remove(0);
                    }
                    b.hist_at = b.hist.len() - 1;
                }
            }
            let mut go = None;
            if ui.add_enabled(b.hist_at > 0, egui::Button::new("<").small()).hint("Back to where the browser was before").clicked() {
                go = Some(b.hist_at - 1);
            }
            if ui.add_enabled(b.hist_at + 1 < b.hist.len(), egui::Button::new(">").small()).hint("Forward again").clicked() {
                go = Some(b.hist_at + 1);
            }
            if let Some(k) = go {
                b.hist_at = k;
                let (tab, sel) = b.hist[k].clone();
                b.tab = match tab {
                    Tab::User(n) if n >= self.settings.tabs.len() => Tab::All,
                    t => t,
                };
                b.hist[k].0 = b.tab;
                b.selected = sel;
                b.reveal = b.selected.is_some();
            }
            if ui.small_button("Refresh").clicked() {
                self.browser.refresh();
            }
            if ui.small_button("Add folder").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.add_search_folder(dir);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (tab, label, tip) in [(Tab::All, "All", "Every folder"), (Tab::Project, "Current project", "What this project is made of"), (Tab::Plugins, "Plug-in database", "The instruments and effects"), (Tab::Starred, "Starred", "The files marked as favourites")] {
                if ui.selectable_label(self.browser.tab == tab, RichText::new(label).small()).hint(tip).clicked() {
                    self.browser.tab = tab;
                }
            }
            let mut gone = None;
            for k in 0..self.settings.tabs.len() {
                let r = ui.selectable_label(self.browser.tab == Tab::User(k), RichText::new(&self.settings.tabs[k].0).small()).hint("A page of your own (right-click to rename or delete)");
                if r.clicked() {
                    self.browser.tab = Tab::User(k);
                }
                r.context_menu(|ui| {
                    if ui.button("Rename...").clicked() {
                        self.browser.tab_name = Some((k, self.settings.tabs[k].0.clone()));
                        ui.close_menu();
                    }
                    if k > 0 && ui.button("Move left").clicked() {
                        self.settings.tabs.swap(k, k - 1);
                        let _ = self.settings.save();
                        ui.close_menu();
                    }
                    if ui.button("Clone").clicked() {
                        let c = self.settings.tabs[k].clone();
                        self.settings.tabs.insert(k + 1, c);
                        let _ = self.settings.save();
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        gone = Some(k);
                        ui.close_menu();
                    }
                });
            }
            if let Some(k) = gone {
                self.settings.tabs.remove(k);
                self.browser.tab = Tab::All;
                let _ = self.settings.save();
            }
        });
        self.browser_tag_window(ui.ctx());
        if self.browser.collapse > 0 {
            self.browser.collapse -= 1;
            ui.ctx().request_repaint();
        }
        match self.browser.tab {
            Tab::All => {}
            Tab::Project => return self.browser_project(ui),
            Tab::Plugins => return self.browser_plugins(ui),
            Tab::Starred => return self.browser_starred(ui),
            Tab::User(k) => return self.browser_user(ui, k),
        }
        ui.horizontal(|ui| {
            if ui.selectable_label(self.browser.tag_on, RichText::new("Tags").small()).hint("Show the tags and find by them").clicked() {
                self.browser.tag_on = !self.browser.tag_on;
            }
            let find = ui.add(egui::TextEdit::singleline(&mut self.browser.query).hint_text("Find (Ctrl+F)").desired_width(f32::INFINITY));
            if std::mem::take(&mut self.browser.find) {
                find.request_focus();
            }
        });
        if self.browser.tag_on {
            let mut all: Vec<String> = self.settings.tags.iter().flat_map(|t| t.1.iter().cloned()).collect();
            all.sort();
            all.dedup();
            self.browser.tag_pick.retain(|t| all.contains(t));
            ui.horizontal_wrapped(|ui| {
                if all.is_empty() {
                    ui.label(RichText::new("No tags yet. Right-click a file and pick Add a tag.").small().color(TEXT_DIM));
                } else if ui.small_button(if self.browser.tag_all { "All" } else { "Any" }).hint("Whether a file must carry all the picked tags or any one of them").clicked() {
                    self.browser.tag_all = !self.browser.tag_all;
                }
                for t in all {
                    let on = self.browser.tag_pick.contains(&t);
                    if ui.selectable_label(on, RichText::new(&t).small()).clicked() && !self.browser.tag_pick.remove(&t) {
                        self.browser.tag_pick.insert(t);
                    }
                }
            });
        }
        ui.separator();
        let query = self.browser.query.trim().to_string();
        if self.browser.tag_on && !self.browser.tag_pick.is_empty() {
            let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
            let pick = &self.browser.tag_pick;
            let hits: Vec<PathBuf> = self
                .settings
                .tags
                .iter()
                .filter(|(_, tags)| if self.browser.tag_all { pick.iter().all(|p| tags.contains(p)) } else { pick.iter().any(|p| tags.contains(p)) })
                .map(|t| t.0.clone())
                .filter(|p| {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                    words.iter().all(|w| name.contains(w.as_str()))
                })
                .collect();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if hits.is_empty() {
                    ui.label(RichText::new("Nothing carries those tags.").color(TEXT_DIM));
                }
                for path in hits {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.browser_file(ui, &path, &name);
                }
            });
            return self.browser_drag(ui);
        }
        if !query.is_empty() {
            if ui.small_button("Save search result as tab").clicked() {
                self.settings.tabs.push((query.clone(), 'S', query.clone()));
                self.browser.tab = Tab::User(self.settings.tabs.len() - 1);
                let _ = self.settings.save();
            }
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
                self.browser_folder(ui, &path, RichText::new(&label).strong().color(TEXT), 0);
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
                    defs.sort_by_key(|d| (!self.settings.plugfav.contains(&format!("g:{}", d.title)), d.title));
                    for d in defs {
                        let key = format!("g:{}", d.title);
                        let fav = self.settings.plugfav.contains(&key);
                        let r = ui.add(egui::Label::new(RichText::new(d.title).color(if fav { TEXT } else { TEXT_DIM })).selectable(false).sense(Sense::click_and_drag()).truncate()).hint("Click to add a channel with this instrument, or drag it onto the channel rack");
                        if r.drag_started() {
                            egui::DragAndDrop::set_payload(ui.ctx(), PlugDrag::Gen(d.kind));
                        }
                        r.context_menu(|ui| self.plugfav_entry(ui, &key, fav));
                        if r.clicked() {
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
                    let mut defs = Self::effect_defs();
                    defs.sort_by_key(|d| (!self.settings.plugfav.contains(&format!("f:{}", d.title)), d.title));
                    for d in defs {
                        let key = format!("f:{}", d.title);
                        let fav = self.settings.plugfav.contains(&key);
                        let r = ui.add(egui::Label::new(RichText::new(d.title).color(if fav { TEXT } else { TEXT_DIM })).selectable(false).sense(Sense::click_and_drag()).truncate()).hint("Click to put it in the first free slot of the selected mixer track, or drag it onto a mixer track or slot");
                        if r.drag_started() {
                            egui::DragAndDrop::set_payload(ui.ctx(), PlugDrag::Fx(d.kind, d.ident));
                        }
                        r.context_menu(|ui| self.plugfav_entry(ui, &key, fav));
                        if r.clicked() {
                            let ins = self.sel_insert.min(self.song.inserts.len().saturating_sub(1));
                            self.effect_to_free_slot(ins, d.kind, d.ident);
                        }
                    }
                });
        });
        self.browser_drag(ui);
    }

    /// The favourite switch in a plug-in's right-click menu. Favourites lead the plug-in page and the menus that add plug-ins.
    fn plugfav_entry(&mut self, ui: &mut Ui, key: &str, fav: bool) {
        if ui.button(if fav { "Remove from favorites" } else { "Add to favorites" }).clicked() {
            if fav {
                self.settings.plugfav.retain(|k| k != key);
            } else {
                self.settings.plugfav.push(key.to_string());
            }
            let _ = self.settings.save();
            ui.close_menu();
        }
    }

    /// Put an effect in the first free slot of a mixer track.
    pub(crate) fn effect_to_free_slot(&mut self, ins: usize, kind: cereal_engine::fx::Kind, ident: &'static str) {
        let used: Vec<u16> = self.song.inserts.get(ins).map(|i| i.slots.iter().map(|s| s.index).collect()).unwrap_or_default();
        if let Some(slot) = (0..10u16).find(|s| !used.contains(s)) {
            self.set_effect(ins, slot, kind, ident);
            self.sel_insert = ins;
            self.show_mixer = true;
            self.dirty = true;
        } else {
            self.status = "Every slot of that mixer track is in use".into();
        }
    }

    /// The favourites page.
    fn browser_starred(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.browser.star_parents, RichText::new("Show parent folders").small());
        ui.separator();
        let mut files = self.settings.starred.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if files.is_empty() {
                ui.label(RichText::new("No favourites yet. Click the star at the end of a file's row.").color(TEXT_DIM));
            }
            if self.browser.star_parents {
                files.sort();
                let mut last: Option<PathBuf> = None;
                for path in files {
                    let dir = path.parent().map(Path::to_path_buf);
                    if dir != last {
                        let name = dir.as_ref().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        ui.label(RichText::new(name).strong().color(TEXT)).hint(dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default());
                        last = dir;
                    }
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    ui.indent(("starred", &path), |ui| self.browser_file(ui, &path, &name));
                }
            } else {
                for path in files {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.browser_file(ui, &path, &name);
                }
            }
        });
        self.browser_drag(ui);
    }

    /// A page of the user's own: one folder, or what a search finds.
    fn browser_user(&mut self, ui: &mut Ui, k: usize) {
        let Some((_, kind, what)) = self.settings.tabs.get(k).cloned() else {
            self.browser.tab = Tab::All;
            return;
        };
        if let Some((n, mut name)) = self.browser.tab_name.take() {
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
            r.request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(t) = self.settings.tabs.get_mut(n).filter(|_| !name.trim().is_empty()) {
                    t.0 = name.trim().to_string();
                    let _ = self.settings.save();
                }
            } else if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.browser.tab_name = Some((n, name));
            }
        }
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if kind == 'F' {
                self.browser_dir(ui, Path::new(&what), 0);
            } else {
                if self.browser.hits.as_ref().is_none_or(|h| h.0 != what) {
                    self.browser.hits = Some((what.clone(), search(&self.browser.roots, &what)));
                }
                for path in self.browser.hits.as_ref().map(|h| h.1.clone()).unwrap_or_default() {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.browser_file(ui, &path, &name);
                }
            }
        });
        self.browser_drag(ui);
    }

    /// The small window for typing a file's tags.
    fn browser_tag_window(&mut self, ctx: &egui::Context) {
        let Some((path, mut text)) = self.browser.tag_ask.take() else { return };
        let mut keep = true;
        egui::Window::new("Tags").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).color(TEXT));
            ui.label(RichText::new("Several tags are set apart by commas. Empty takes them all away.").small().color(TEXT_DIM));
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(260.0)).request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let tags: Vec<String> = text.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
                self.settings.tags.retain(|t| t.0 != path);
                if !tags.is_empty() {
                    self.settings.tags.push((path.clone(), tags));
                }
                let _ = self.settings.save();
                keep = false;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                keep = false;
            }
        });
        if keep {
            self.browser.tag_ask = Some((path, text));
        }
    }

    fn browser_drag(&mut self, ui: &mut Ui) {
        if let Some(p) = egui::DragAndDrop::payload::<PlugDrag>(ui.ctx()) {
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag-plug"))).text(pos + egui::vec2(12.0, 4.0), egui::Align2::LEFT_TOP, p.title(), egui::FontId::proportional(12.0), TEXT);
            }
        }
        // a sample being dragged follows the pointer
        if let Some(path) = egui::DragAndDrop::payload::<PathBuf>(ui.ctx()) {
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag-sample"))).text(pos + egui::vec2(12.0, 4.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(12.0), ACCENT);
            }
        }
    }
}
