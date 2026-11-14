//! The main menu bar: FILE, EDIT, ADD, PATTERNS, VIEW, OPTIONS, TOOLS and HELP, each entry where a user of
//! the usual layout expects it, with the same captions between groups and the same keys beside the entries.

use crate::widgets::{Hint as _, TEXT_DIM};
use crate::{settings, App, Leave, Win};
use cereal_engine::song::Chan;
use cereal_engine::Cmd;
use eframe::egui::{self, RichText, Ui};
use std::path::PathBuf;

/// A caption strip between groups of entries.
fn sect(ui: &mut Ui, name: &str) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(60.0), 15.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, ui.visuals().extreme_bg_color);
    ui.painter().text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(10.5), TEXT_DIM);
}

/// A menu entry with its keys on the right.
fn mi(ui: &mut Ui, label: &str, keys: &str) -> egui::Response {
    ui.add(egui::Button::new(label).shortcut_text(RichText::new(keys).color(TEXT_DIM)))
}

/// A menu entry that is a switch.
fn tick(ui: &mut Ui, on: bool, label: &str, keys: &str) -> egui::Response {
    ui.add(egui::Button::new(format!("{}  {label}", if on { "\u{2714}" } else { "    " })).shortcut_text(RichText::new(keys).color(TEXT_DIM)))
}

/// One column of a menu that has several side by side.
fn col<R>(ui: &mut Ui, width: f32, body: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
        ui.set_width(width);
        body(ui)
    })
    .inner
}

/// What an entry of the ADD menu's channel list makes.
#[derive(Clone, Copy)]
pub(crate) enum Adds {
    Sampler,
    Speech,
    Built(cereal_engine::synth::Kind),
    MidiSender,
    EnvelopeSender,
    KeyController,
    PadMachine,
    SoundBank,
}

const GROUPS: [&str; 9] = ["Controller", "Drum", "MIDI", "Misc", "Patcher", "Sampler", "Synth classic", "Synth special", "Visual"];

/// The group an instrument sits under, by the name project files give it.
fn group_of(ident: &str) -> usize {
    match ident {
        "Fruity Envelope Controller" | "Fruity Keyboard Controller" => 0,
        "BassDrum" | "Drumaxx" | "Drumpad" | "FPC" | "Fruit Kick" | "Fruity DrumSynth Live" | "Fruity Slicer" | "Fruity Slicer 2" | "Slicex" => 1,
        "Dashboard" | "MIDI Out" => 2,
        "Patcher" => 4,
        "DirectWave" | "Fruity Granulizer" | "Wave Traveller" => 5,
        "3x Osc" | "FLEX" | "Fruity DX10" | "GMS" | "Harmless" | "Kepler" | "Kepler Exo" | "MiniSynth" | "PoiZone" | "Sawer" | "SimSynth" | "Sytrus" | "Toxic Biohazard" | "Transistor Bass" | "Wasp" | "Wasp XT" => 6,
        "Autogun" | "BeepMap" | "Harmor" | "Morphine" | "Ogun" | "Plucked!" | "Sakura" | "FL Slayer" => 7,
        "Fruity Dance" | "Fruity Video Player" => 8,
        x if x.starts_with("VFX ") => 4,
        _ => 3,
    }
}

impl App {
    /// Every built-in thing a new channel can be: (group, title, what it makes), in menu order.
    fn add_entries(&self) -> Vec<(usize, &'static str, Adds)> {
        let mut v: Vec<(usize, &'static str, Adds)> = cereal_engine::synth::DEFS.iter().filter(|d| !d.arrives_only()).map(|d| (group_of(d.ident), d.title, Adds::Built(d.kind))).collect();
        v.push((0, "Envelope Sender", Adds::EnvelopeSender));
        v.push((0, "Key Controller", Adds::KeyController));
        v.push((1, "Pad Machine", Adds::PadMachine));
        v.push((2, "MIDI Sender", Adds::MidiSender));
        v.push((3, "Sound Bank Player", Adds::SoundBank));
        v.push((5, "Sampler", Adds::Sampler));
        v.sort_by(|a, b| (a.0, a.1.to_lowercase()).cmp(&(b.0, b.1.to_lowercase())));
        v.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        v
    }

    pub(crate) fn add_channel(&mut self, what: Adds) {
        use cereal_engine::synth::Kind;
        let quiet = |plugin: &str, name: &str| {
            let mut c = Chan::instrument(Kind::Quiet);
            c.plugin = plugin.into();
            c.name = name.into();
            c
        };
        let c = match what {
            Adds::Sampler => Chan::default(),
            Adds::Speech => {
                self.speech_dlg.get_or_insert_with(Default::default);
                return;
            }
            Adds::Built(k) => Chan::instrument(k),
            Adds::MidiSender => {
                let mut c = quiet(cereal_engine::midiout::IDENT, "MIDI Sender");
                if let Some(g) = c.gen.as_mut() {
                    g.raw = cereal_engine::midiout::Set::fresh().write();
                }
                c
            }
            Adds::EnvelopeSender => {
                let mut c = quiet(cereal_engine::fm::ENV_SENDER, "Envelope Sender");
                if let Some(g) = c.gen.as_mut() {
                    g.raw = cereal_engine::fm::Sender::fresh();
                }
                c
            }
            Adds::KeyController => {
                let mut c = quiet(cereal_engine::synth::KEY_SENDER, "Key Controller");
                if let Some(g) = c.gen.as_mut() {
                    cereal_engine::synth::key_sender_set(&mut g.raw, 255, 0.0);
                }
                c
            }
            Adds::PadMachine => {
                let mut c = Chan { name: "Pad Machine".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: cereal_engine::zones::Kit::IDENT.into(), ..Chan::default() };
                c.zones = cereal_engine::zones::Map::parse(&c.plugin, &cereal_engine::zones::Kit::fresh().write()).map(std::sync::Arc::new);
                c
            }
            Adds::SoundBank => {
                let mut c = Chan { name: "Sound Bank Player".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: "SoundFont Player".into(), ..Chan::default() };
                c.zones = cereal_engine::sf2::open(&c.plugin, &[], None, None).map(std::sync::Arc::new);
                c
            }
        };
        self.song.channels.push(c);
        self.sel_chan = self.song.channels.len() - 1;
        self.dirty = true;
    }

    /// The channel list of the ADD menu and of the channel rack's + button: by group, as one plain list, or
    /// as a tree, whichever the three switches at its end ask for.
    pub(crate) fn add_columns(&mut self, ui: &mut Ui) {
        let view_id = egui::Id::new("add-menu-view");
        let mut view: u8 = ui.ctx().data_mut(|d| d.get_persisted(view_id).unwrap_or(0));
        let entries = self.add_entries();
        let mut picked = None;
        let hosted = |me: &mut Self, ui: &mut Ui| {
            ui.menu_button("More plugins...", |ui| {
                if let Some(inst) = crate::hostui::menu(ui, true) {
                    me.song.channels.push(Chan::hosted(inst));
                    me.sel_chan = me.song.channels.len() - 1;
                    me.dirty = true;
                }
            });
        };
        let switches = |ui: &mut Ui, view: &mut u8| {
            ui.separator();
            for (k, name) in ["Categories", "Simple", "Tree"].into_iter().enumerate() {
                if ui.radio(*view == k as u8, name).clicked() {
                    *view = k as u8;
                }
            }
        };
        match view {
            1 => {
                egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                    hosted(self, ui);
                    if ui.button("Speech").clicked() {
                        picked = Some(Adds::Speech);
                    }
                    ui.separator();
                    let mut flat = entries.clone();
                    flat.sort_by_key(|e| e.1.to_lowercase());
                    for (_, title, what) in flat {
                        if ui.button(title).clicked() {
                            picked = Some(what);
                        }
                    }
                    switches(ui, &mut view);
                });
            }
            2 => {
                hosted(self, ui);
                if ui.button("Speech").clicked() {
                    picked = Some(Adds::Speech);
                }
                ui.separator();
                for (g, name) in GROUPS.iter().enumerate() {
                    ui.menu_button(*name, |ui| {
                        for (_, title, what) in entries.iter().filter(|e| e.0 == g) {
                            if ui.button(*title).clicked() {
                                picked = Some(*what);
                            }
                        }
                    });
                }
                switches(ui, &mut view);
            }
            _ => {
                // the groups run down a column and carry on in the next one, about thirty rows each
                let mut rows: Vec<(Option<&str>, Option<(&str, Adds)>)> = vec![(Some("Channel"), None)];
                rows.push((None, None));
                rows.push((None, Some(("Speech", Adds::Speech))));
                for (g, name) in GROUPS.iter().enumerate() {
                    if entries.iter().any(|e| e.0 == g) {
                        rows.push((Some(*name), None));
                        rows.extend(entries.iter().filter(|e| e.0 == g).map(|e| (None, Some((e.1, e.2)))));
                    }
                }
                let per = 32;
                let n = rows.len();
                ui.horizontal_top(|ui| {
                    for (c, chunk) in rows.chunks(per).enumerate() {
                        col(ui, 170.0, |ui| {
                            for row in chunk {
                                match row {
                                    (Some(cap), _) => sect(ui, cap),
                                    (None, None) => hosted(self, ui),
                                    (None, Some((title, what))) => {
                                        if ui.button(*title).clicked() {
                                            picked = Some(*what);
                                        }
                                    }
                                }
                            }
                            if (c + 1) * per >= n {
                                switches(ui, &mut view);
                            }
                        });
                    }
                });
            }
        }
        ui.ctx().data_mut(|d| d.insert_persisted(view_id, view));
        if let Some(what) = picked {
            self.add_channel(what);
            ui.close_menu();
        }
    }

    pub(crate) fn main_menus(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.menu_button("FILE", |ui| self.menu_file(ui, ctx));
        ui.menu_button("EDIT", |ui| self.menu_edit(ui));
        ui.menu_button("ADD", |ui| self.menu_add(ui));
        ui.menu_button("PATTERNS", |ui| self.menu_patterns(ui));
        ui.menu_button("VIEW", |ui| self.menu_view(ui));
        ui.menu_button("OPTIONS", |ui| self.menu_options(ui));
        ui.menu_button("TOOLS", |ui| self.menu_tools(ui));
        ui.menu_button("HELP", |ui| {
            if mi(ui, "Help index", "F1").clicked() {
                self.help_open();
                ui.close_menu();
            }
            ui.separator();
            if mi(ui, "About...", "").clicked() {
                self.show_about = true;
                ui.close_menu();
            }
        });
    }

    fn user_templates() -> Option<PathBuf> {
        Some(crate::setup::settings_path()?.parent()?.join("Templates"))
    }

    fn menu_file(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.set_min_width(270.0);
        if mi(ui, "New", "").clicked() {
            self.leave(Leave::New);
            ui.close_menu();
        }
        ui.menu_button("New from template", |ui| {
            if self.templates.is_none() {
                let mut found = Vec::new();
                for root in &self.lib.factory_roots {
                    let mut todo = vec![(root.join("Data/Templates"), 0)];
                    while let Some((dir, depth)) = todo.pop() {
                        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                            let p = e.path();
                            if p.is_dir() && depth < 4 {
                                todo.push((p, depth + 1));
                            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp")) {
                                let kind = p.parent().and_then(|d| d.parent()).and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                found.push((format!("{kind} / {}", p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), p));
                            }
                        }
                    }
                }
                if let Some(dir) = Self::user_templates() {
                    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                        let p = e.path();
                        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp")) {
                            found.push((format!("User / {}", p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), p));
                        }
                    }
                }
                found.sort();
                self.templates = Some(found);
            }
            let list = self.templates.clone().unwrap_or_default();
            let mut kinds: Vec<&str> = list.iter().filter_map(|(n, _)| n.split_once(" / ").map(|x| x.0)).collect();
            kinds.dedup();
            for kind in kinds {
                ui.menu_button(kind, |ui| {
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (name, path) in list.iter().filter(|(n, _)| n.split_once(" / ").is_some_and(|x| x.0 == kind)) {
                            if ui.button(name.split_once(" / ").map(|x| x.1).unwrap_or(name)).clicked() {
                                ui.close_menu();
                                self.leave(Leave::Template(path.clone()));
                            }
                        }
                    });
                });
            }
            if !list.is_empty() {
                ui.separator();
            }
            if ui.button("Empty").hint("An empty project to start from nothing").clicked() {
                self.leave(Leave::New);
                ui.close_menu();
            }
            if list.is_empty() {
                ui.label(RichText::new("More templates come with the sounds and presets from your own copy (OPTIONS menu).").color(TEXT_DIM));
            }
        });
        if mi(ui, "Open...", "Ctrl+O").clicked() {
            ui.close_menu();
            self.leave(Leave::OpenDialog);
        }
        ui.separator();
        if mi(ui, "Save", "Ctrl+S").clicked() {
            ui.close_menu();
            self.save(false);
        }
        if mi(ui, "Save as...", "Shift+Ctrl+S").clicked() {
            ui.close_menu();
            self.save(true);
        }
        if mi(ui, "Save new version", "Ctrl+N").clicked() {
            ui.close_menu();
            self.save_new_version();
        }
        if mi(ui, "Save to new project folder...", "").hint("The project and every sound file it uses, together in a folder you pick").clicked() {
            ui.close_menu();
            self.save_to_folder(true);
        }
        if mi(ui, "Save as template...", "").hint("Keep this project as a start for new ones: it shows under New from template").clicked() {
            ui.close_menu();
            self.save_template();
        }
        ui.separator();
        ui.menu_button("Import", |ui| {
            if mi(ui, "MIDI file...", "").clicked() {
                ui.close_menu();
                self.import_midi_dialog();
            }
        });
        ui.menu_button("Export", |ui| {
            sect(ui, "Audio");
            let last_id = egui::Id::new("last-export-format");
            for (label, keys, fmt) in [("Wave file...", "Ctrl+R", 0u8), ("MP3 file...", "Shift+Ctrl+R", 1), ("OGG file...", "", 2), ("FLAC file...", "", 3)] {
                if mi(ui, label, keys).clicked() {
                    ui.close_menu();
                    ui.ctx().data_mut(|d| d.insert_temp(last_id, fmt));
                    self.export_as(fmt);
                }
            }
            if mi(ui, "MIDI file...", "Shift+Ctrl+M").clicked() {
                ui.close_menu();
                self.export_midi();
            }
            if mi(ui, "Last exported format(s)...", "").clicked() {
                ui.close_menu();
                let fmt = ui.ctx().data(|d| d.get_temp(last_id)).unwrap_or(0u8);
                self.export_as(fmt);
            }
            sect(ui, "Project");
            if mi(ui, "Zipped loop package...", "").clicked() {
                ui.close_menu();
                self.export_package();
            }
            if mi(ui, "Project bones...", "").clicked() {
                ui.close_menu();
                self.export_bones();
            }
            if mi(ui, "Project data files...", "").hint("Every sound file the project uses, copied into a folder you pick").clicked() {
                ui.close_menu();
                self.save_to_folder(false);
            }
        });
        ui.separator();
        if !self.settings.recent.is_empty() {
            for (k, p) in self.settings.recent.clone().into_iter().enumerate().take(10) {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if mi(ui, &format!("{} {name}", k + 1), &format!("Alt+{}", (k + 1) % 10)).hint(p.display().to_string()).clicked() {
                    self.actions.push(crate::Action::Open(p));
                    ui.close_menu();
                }
            }
            ui.separator();
        }
        let last = Self::last_backup();
        if ui.add_enabled(last.is_some(), egui::Button::new("Revert to last backup")).hint("Open the newest copy in the backup folder: unsaved work put there on a timer, or a project as it was before it was saved over").clicked() {
            ui.close_menu();
            if let Some(p) = last {
                self.open(&p);
            }
        }
        ui.separator();
        if mi(ui, "Exit", "").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The export window, set to one of the sound file kinds: 0 WAV, 1 MP3, 2 Ogg Vorbis, 3 FLAC.
    pub(crate) fn export_as(&mut self, fmt: u8) {
        self.export();
        if let Some(r) = self.render.as_mut() {
            (r.flac, r.lossy) = match fmt {
                1 => (false, 1),
                2 => (false, 2),
                3 => (true, 0),
                _ => (false, 0),
            };
        }
    }

    /// The project with its sound files in a folder of its own (`project` on), or only the sound files.
    fn save_to_folder(&mut self, project: bool) {
        let Some(root) = rfd::FileDialog::new().set_title(if project { "New project folder" } else { "Folder for the project's data files" }).pick_folder() else { return };
        let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        let dir = self.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        let zip = root.join(format!(".{name}.packing.zip"));
        let made = cereal_engine::package(&mut self.song, self.flp.as_ref(), &name, &self.lib, dir.as_deref())
            .and_then(|(bytes, n)| std::fs::write(&zip, bytes).map(|_| n).map_err(|e| e.to_string()))
            .and_then(|n| cereal_engine::unpack(&zip, &root).map(|p| (p, n)));
        let _ = std::fs::remove_file(&zip);
        match made {
            Ok((p, n)) if project => {
                self.open(&p);
                self.status = format!("Saved the project and {n} sound files to {}", root.display());
            }
            Ok((p, n)) => {
                let _ = std::fs::remove_file(&p);
                self.status = format!("Copied {n} sound files to {}", root.display());
            }
            Err(e) => self.status = format!("Saving to the folder failed: {e}"),
        }
    }

    fn save_template(&mut self) {
        let Some(dir) = Self::user_templates() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let Some(path) = rfd::FileDialog::new().add_filter("Project files", &["flp"]).set_directory(&dir).set_file_name("My template.flp").save_file() else { return };
        let path = if path.extension().is_none() { path.with_extension("flp") } else { path };
        self.song.song_mode = self.handle.transport.song_mode();
        self.status = match cereal_engine::save::save(&mut self.song, self.flp.as_ref()).and_then(|(bytes, _)| std::fs::write(&path, bytes).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.templates = None;
                format!("Template saved as {}", path.display())
            }
            Err(e) => format!("Saving the template failed: {e}"),
        };
    }

    fn menu_edit(&mut self, ui: &mut Ui) {
        let can = self.hist_at > 0 || self.hist_at + 1 < self.history.len();
        if ui.add_enabled(can, egui::Button::new("Undo").shortcut_text(RichText::new("Ctrl+Z").color(TEXT_DIM))).hint("Undo or redo the last edit; Ctrl+Alt+Z steps further back").clicked() {
            self.undo_toggle();
            ui.close_menu();
        }
        ui.separator();
        for (label, keys, ev) in [("Cut", "Ctrl+X", egui::Event::Cut), ("Copy", "Ctrl+C", egui::Event::Copy), ("Paste", "Ctrl+V", egui::Event::Paste(String::new()))] {
            if mi(ui, label, keys).clicked() {
                self.menu_events.push(ev);
                ui.close_menu();
            }
        }
    }

    fn menu_add(&mut self, ui: &mut Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| self.add_columns(ui));
            col(ui, 230.0, |ui| {
                sect(ui, "Other");
                ui.menu_button("Effect", |ui| {
                    let ins = self.sel_insert.min(self.song.inserts.len().saturating_sub(1));
                    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                        for d in Self::effect_defs() {
                            if ui.button(d.title).hint("Into the first free slot of the selected mixer track").clicked() {
                                match (0..10u16).find(|n| !self.song.inserts[ins].slots.iter().any(|s| s.index == *n)) {
                                    Some(slot) => {
                                        self.set_effect(ins, slot, d.kind, d.ident);
                                        self.dirty = true;
                                    }
                                    None => self.status = "No free effect slot on the selected mixer track".into(),
                                }
                                ui.close_menu();
                            }
                        }
                    });
                });
                if mi(ui, "Plugin picker", "F8").clicked() {
                    self.show_picker = true;
                    ui.close_menu();
                }
                for label in ["Browse plugin database", "Browse all installed plugins", "Browse presets"] {
                    if mi(ui, label, "").clicked() {
                        self.show_browser = true;
                        self.focus = Win::Browser;
                        self.browser_show_plugins();
                        ui.close_menu();
                    }
                }
                ui.separator();
                let tweaked: crate::widgets::Tweaked = ui.ctx().data(|d| d.get_temp(egui::Id::new("tweaked"))).unwrap_or_default();
                if ui.add_enabled(tweaked[0].is_some(), egui::Button::new("Automation for last tweaked parameter")).clicked() {
                    if let Some(target) = tweaked[0] {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ctl_ask"), Some((target, crate::widgets::CtlAsk::AutoClip))));
                    }
                    ui.close_menu();
                }
                ui.separator();
                if mi(ui, "Pattern", "").clicked() {
                    self.new_pattern();
                    self.start_rename_pattern();
                    ui.close_menu();
                }
            });
        });
    }

    fn menu_patterns(&mut self, ui: &mut Ui) {
        let cur = self.handle.transport.pattern();
        ui.horizontal_top(|ui| {
            col(ui, 150.0, |ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    let n = self.song.patterns.len().max(1);
                    for p in 0..n {
                        let name = self.song.patterns.get(p).map(|x| x.name.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| format!("Pattern {}", p + 1));
                        if ui.add(egui::Button::new(name).selected(p == cur)).clicked() {
                            self.song.ensure_pattern(p);
                            self.handle.transport.set_pattern(p);
                            ui.close_menu();
                        }
                    }
                });
            });
            col(ui, 270.0, |ui| {
                if mi(ui, "Find first empty...", "Shift+F4").clicked() {
                    self.new_pattern();
                    self.start_rename_pattern();
                    ui.close_menu();
                }
                if mi(ui, "Find next empty...", "F4").clicked() {
                    self.next_empty_after();
                    self.start_rename_pattern();
                    ui.close_menu();
                }
                if mi(ui, "Find next empty (no naming)", "Ctrl+F4").clicked() {
                    self.next_empty_after();
                    ui.close_menu();
                }
                ui.separator();
                if mi(ui, "Select in playlist", "").hint("Every clip of this pattern on the playlist becomes selected").clicked() {
                    self.playlist.sel = self.song.clips.iter().enumerate().filter(|(_, c)| matches!(c.src, cereal_engine::song::ClipSrc::Pattern(p) if p == cur)).map(|(i, _)| i).collect();
                    self.show_playlist = true;
                    self.focus = Win::Playlist;
                    ui.close_menu();
                }
                if mi(ui, "Rename and color...", "F2").clicked() {
                    self.start_rename_pattern();
                    ui.close_menu();
                }
                if mi(ui, "Change color...", "").clicked() {
                    self.start_rename_pattern();
                    ui.close_menu();
                }
                if mi(ui, "Random color", "").clicked() {
                    self.song.ensure_pattern(cur);
                    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
                    let h = (t % 360) as f32 / 360.0;
                    let c = egui::Color32::from(egui::ecolor::Hsva::new(h, 0.45, 0.6, 1.0));
                    self.song.patterns[cur].color = Some([c.r(), c.g(), c.b()]);
                    self.dirty = true;
                    ui.close_menu();
                }
                if mi(ui, "Open in project browser", "Shift+Ctrl+P").clicked() {
                    self.browser_show_project();
                    ui.close_menu();
                }
                if mi(ui, "Transpose...", "").hint("Move every note of the pattern up or down by a number of semitones").clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("transpose-ask"), Some(0i32)));
                    ui.close_menu();
                }
                ui.separator();
                if mi(ui, "Insert one", "Shift+Ctrl+Ins").hint("An empty pattern here; this one and the ones after it move down").clicked() {
                    self.insert_pattern();
                    ui.close_menu();
                }
                if mi(ui, "Clone", "Shift+Ctrl+C").clicked() {
                    self.clone_pattern();
                    ui.close_menu();
                }
                if mi(ui, "Delete", "Shift+Ctrl+Del").clicked() {
                    self.delete_pattern();
                    ui.close_menu();
                }
                ui.separator();
                for (label, keys, up) in [("Move up", "Shift+Ctrl+Up", true), ("Move down", "Shift+Ctrl+Down", false)] {
                    if mi(ui, label, keys).clicked() {
                        self.move_pattern(up);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if mi(ui, "Split by channel", "").hint("Every channel playing in this pattern gets a pattern of its own").clicked() {
                    let made = self.song.split_pattern(cur);
                    self.status = if made.is_empty() { "Only one channel plays in this pattern".into() } else { format!("Split into {} patterns", made.len() + 1) };
                    self.dirty |= !made.is_empty();
                    ui.close_menu();
                }
                if mi(ui, "Quick render as audio clip", "").hint("The pattern is played into a sound file that becomes an audio clip channel").clicked() {
                    self.render_pattern(false);
                    ui.close_menu();
                }
                if mi(ui, "Render as audio clip...", "").hint("The pattern is played into a sound file that becomes an audio clip channel").clicked() {
                    self.render_pattern(false);
                    ui.close_menu();
                }
                if mi(ui, "Render and replace...", "").hint("As above, and every clip of the pattern on the playlist becomes the sound").clicked() {
                    self.render_pattern(true);
                    ui.close_menu();
                }
            });
        });
    }

    pub(crate) fn move_pattern(&mut self, up: bool) {
        let p = self.handle.transport.pattern();
        self.song.ensure_pattern(p);
        let to = self.song.move_pattern(p, up);
        self.handle.transport.set_pattern(to);
        self.dirty = true;
    }

    /// Windows the menus open that have no other home: the transpose question and the tempo tapper.
    pub(crate) fn menu_windows(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("transpose-ask");
        if let Some(mut by) = ctx.data(|d| d.get_temp::<Option<i32>>(id)).flatten() {
            let mut keep = true;
            egui::Window::new("Transpose pattern").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Semitones");
                    ui.add(egui::DragValue::new(&mut by).range(-48..=48));
                });
                ui.horizontal(|ui| {
                    if ui.button("Accept").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let p = self.handle.transport.pattern();
                        self.song.ensure_pattern(p);
                        for n in self.song.patterns[p].notes.iter_mut() {
                            n.key = (n.key as i32 + by).clamp(0, 131) as u8;
                        }
                        self.dirty = true;
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep = false;
                    }
                });
            });
            ctx.data_mut(|d| d.insert_temp(id, if keep { Some(by) } else { None }));
        }
        if self.show_tapper {
            let mut open = true;
            egui::Window::new("Tempo tapper").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(RichText::new(format!("{:.0} BPM", self.song.tempo)).size(22.0));
                if ui.add_sized([150.0, 40.0], egui::Button::new("Tap")).hint("Click in time, four times or more").clicked() {
                    self.tap_tempo();
                }
            });
            self.show_tapper = open;
        }
    }

    /// One tap of the tempo tapper: from the fourth tap on the tempo follows the taps.
    pub(crate) fn tap_tempo(&mut self) {
        let now = std::time::Instant::now();
        if self.taps.last().is_some_and(|t| now.duration_since(*t).as_secs_f32() > 2.5) {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > 8 {
            self.taps.remove(0);
        }
        if self.taps.len() >= 4 {
            let span = now.duration_since(self.taps[0]).as_secs_f32();
            let tempo = (60.0 * (self.taps.len() - 1) as f32 / span.max(0.01)).round().clamp(10.0, 522.0);
            self.song.tempo = tempo;
            self.dirty = true;
        }
    }

    pub(crate) fn close_plugin_windows(&mut self) {
        self.fx_open = None;
        self.show_chanset = false;
        for inst in self.song.hosted() {
            inst.hide_window();
        }
    }

    /// Every window but the one in use goes away.
    pub(crate) fn close_unfocused(&mut self) {
        let f = self.focus;
        self.show_playlist &= f == Win::Playlist;
        self.show_rack &= f == Win::Rack;
        self.show_piano &= f == Win::Piano;
        self.show_mixer &= f == Win::Mixer;
        self.show_events &= f == Win::Events;
        self.show_info = false;
        self.close_plugin_windows();
    }

    pub(crate) fn realign(&mut self, ctx: &egui::Context) {
        self.layout = None;
        ctx.memory_mut(|m| m.reset_areas());
    }

    fn menu_view(&mut self, ui: &mut Ui) {
        ui.set_min_width(270.0);
        sect(ui, "Windows");
        for (on, label, keys) in [
            (&mut self.show_playlist, "Playlist", "F5"),
            (&mut self.show_piano, "Piano roll", "F7"),
            (&mut self.show_rack, "Channel rack", "F6"),
            (&mut self.show_mixer, "Mixer", "F9"),
            (&mut self.show_browser, "Browser", "Alt+F8"),
            (&mut self.show_project, "Project picker", "Ctrl+F8"),
            (&mut self.show_picker, "Plugin picker", "F8"),
            (&mut self.show_tapper, "Tempo tapper", "Ctrl+F5"),
            (&mut self.show_touch, "Touch controller", "Alt+F7"),
            (&mut self.show_script_out, "Script output", ""),
        ] {
            if tick(ui, *on, label, keys).clicked() {
                *on = !*on;
                ui.close_menu();
            }
        }
        ui.menu_button("      Toolbars", |ui| self.toolbar_menu(ui));
        if tick(ui, self.show_load, "Plugin performance monitor", "").clicked() {
            self.show_load = !self.show_load;
            ui.close_menu();
        }
        sect(ui, "Layout");
        if mi(ui, "      Close all windows", "F12").clicked() {
            self.close_all();
            ui.close_menu();
        }
        if mi(ui, "      Close all plugin windows", "Alt+F12").clicked() {
            self.close_plugin_windows();
            ui.close_menu();
        }
        if mi(ui, "      Close all unfocused windows", "Ctrl+F12").clicked() {
            self.close_unfocused();
            ui.close_menu();
        }
        ui.menu_button("      Arrange windows", |ui| {
            if mi(ui, "Realign", "Shift+Ctrl+H").clicked() {
                self.realign(ui.ctx());
                ui.close_menu();
            }
            sect(ui, "Layouts");
            if mi(ui, "Desktop (default)", "").clicked() {
                self.realign(ui.ctx());
                (self.show_playlist, self.show_rack, self.show_mixer, self.show_browser) = (true, true, true, true);
                ui.close_menu();
            }
            let mut gone = None;
            for k in 0..self.settings.layouts.len() {
                ui.horizontal(|ui| {
                    if ui.button(self.settings.layouts[k].0.clone()).clicked() {
                        self.apply_layout(k);
                        ui.close_menu();
                    }
                    if ui.small_button("x").hint("Forget this layout").clicked() {
                        gone = Some(k);
                    }
                });
            }
            if let Some(k) = gone {
                self.settings.layouts.remove(k);
                let _ = self.settings.save();
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.layout_name).desired_width(120.0).hint_text("Name"));
                if ui.add_enabled(!self.layout_name.trim().is_empty(), egui::Button::new("Save custom layout")).hint("Remember where the windows are and which are open, under this name").clicked() {
                    let name = std::mem::take(&mut self.layout_name);
                    self.save_layout(&name);
                    ui.close_menu();
                }
            });
        });
        ui.menu_button("      Background", |ui| {
            if ui.button("Set image wallpaper...").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Picture", &["png", "jpg", "jpeg", "bmp"]).pick_file() {
                    self.settings.backdrop = Some(p);
                    let _ = self.settings.save();
                }
                ui.close_menu();
            }
            if ui.add_enabled(self.settings.backdrop.is_some(), egui::Button::new("None")).clicked() {
                self.settings.backdrop = None;
                let _ = self.settings.save();
                ui.close_menu();
            }
        });
        sect(ui, "Browser");
        for label in ["Undo history", "Patterns", "Generators in use", "Effects in use", "Remote control"] {
            if mi(ui, &format!("      {label}"), "").clicked() {
                self.browser_show_project();
                ui.close_menu();
            }
        }
        if mi(ui, "      Plugin database", "Shift+F8").clicked() {
            self.show_browser = true;
            self.focus = Win::Browser;
            self.browser_show_plugins();
            ui.close_menu();
        }
    }

    fn menu_options(&mut self, ui: &mut Ui) {
        ui.set_min_width(270.0);
        sect(ui, "System");
        for (label, keys, page) in [("MIDI settings", "F10", settings::Page::Midi), ("Audio settings", "", settings::Page::Audio), ("General settings", "", settings::Page::General), ("File settings", "", settings::Page::File)] {
            if mi(ui, &format!("      {label}"), keys).clicked() {
                self.prefs_win.open = true;
                self.prefs_win.page = page;
                ui.close_menu();
            }
        }
        sect(ui, "Project");
        if mi(ui, "      Project general settings", "").clicked() {
            self.show_info = true;
            ui.close_menu();
        }
        if mi(ui, "      Project info", "F11").clicked() {
            self.show_info = true;
            ui.close_menu();
        }
        sect(ui, "MIDI");
        if tick(ui, self.settings.midi_sync, "Enable MIDI master sync", "").hint("Send the clock, start and stop to the MIDI outputs").clicked() {
            self.settings.midi_sync = !self.settings.midi_sync;
            let _ = self.settings.save();
        }
        sect(ui, "Switches");
        if tick(ui, self.typing_keys, "Typing keyboard to piano", "Ctrl+T").clicked() {
            self.typing_keys = !self.typing_keys;
        }
        let tr = self.handle.transport.clone();
        if tick(ui, tr.metronome(), "Metronome", "Ctrl+M").clicked() {
            tr.set_metronome(!tr.metronome());
        }
        if tick(ui, tr.countdown(), "Recording precount", "Ctrl+P").clicked() {
            tr.set_countdown(!tr.countdown());
        }
        if tick(ui, tr.waiting(), "Start on input", "Ctrl+I").clicked() {
            tr.set_waiting(!tr.waiting());
        }
        if tick(ui, self.rec_blend, "Blend recorded notes", "Ctrl+B").clicked() {
            self.rec_blend = !self.rec_blend;
        }
        if tick(ui, tr.step_edit(), "Step edit", "Ctrl+E").clicked() {
            tr.set_step_edit(!tr.step_edit());
        }
        if tick(ui, self.rec_loop, "Loop record", "").clicked() {
            self.rec_loop = !self.rec_loop;
        }
        if tick(ui, self.rec_quantise, "Input quantize", "").hint("Recorded notes land on the snap lines").clicked() {
            self.rec_quantise = !self.rec_quantise;
        }
        if tick(ui, self.multilink, "Multilink to controllers", "Ctrl+J").hint("Turn the controls you want, then move a knob of the MIDI device for each, in the same order").clicked() {
            self.toggle_multilink();
        }
        sect(ui, "Content");
        if mi(ui, "      Sounds and presets from your own copy...", "").clicked() {
            self.setup = Default::default();
            self.setup.open = true;
            ui.close_menu();
        }
        ui.separator();
        ui.label(RichText::new(&self.audio).color(TEXT_DIM));
    }

    fn menu_tools(&mut self, ui: &mut Ui) {
        ui.horizontal_top(|ui| {
            col(ui, 250.0, |ui| self.tools_left(ui));
            col(ui, 290.0, |ui| self.tools_macros(ui));
        });
    }

    fn tools_left(&mut self, ui: &mut Ui) {
        if mi(ui, "Browser find...", "Ctrl+F").clicked() {
            self.show_browser = true;
            self.focus = Win::Browser;
            self.browser_key(egui::Key::F, egui::Modifiers::COMMAND);
            ui.close_menu();
        }
        ui.menu_button("One-click audio recording", |ui| {
            sect(ui, "I want to record audio...");
            if ui.button("Into the audio editor/recorder").hint("A Wave Recorder on the selected mixer track starts taking down what arrives there").clicked() {
                let ins = self.sel_insert.min(self.song.inserts.len().saturating_sub(1));
                let have = self.song.inserts.get(ins).and_then(|i| i.slots.iter().find(|s| s.plugin.eq_ignore_ascii_case("Edison")).map(|s| s.index));
                let free = || (0..10u16).find(|n| !self.song.inserts[ins].slots.iter().any(|s| s.index == *n));
                match have.or_else(free) {
                    Some(slot) => {
                        if have.is_none() {
                            self.set_effect(ins, slot, cereal_engine::fx::Kind::Thru, "Edison");
                            self.dirty = true;
                            self.handle.publish(self.song.clone());
                        }
                        self.fx_open = Some((ins, slot));
                        self.handle.transport.record_slot(Some((ins, slot)));
                        self.status = "Recording into the Wave Recorder".into();
                    }
                    None => self.status = "No free effect slot on the selected mixer track".into(),
                }
                ui.close_menu();
            }
            if ui.button("Into playlist as an audio clip").hint("The song starts and the input device is recorded onto the playlist until you stop").clicked() {
                self.rec_audio = true;
                self.handle.transport.set_recording(true);
                if !self.handle.transport.playing() {
                    self.play();
                }
                ui.close_menu();
            }
        });
        if mi(ui, "Remix a song...", "").hint("Split the selected channel's sound into vocals, drums, bass and the rest").clicked() {
            self.ask_stems(self.sel_chan);
            ui.close_menu();
        }
        if mi(ui, "Riff machine...", "").clicked() {
            self.show_piano = true;
            self.focus = Win::Piano;
            self.pr_helper(0);
            ui.close_menu();
        }
        sect(ui, "Score logger");
        ui.menu_button("Dump score log to selected pattern", |ui| {
            for m in [2u32, 5, 10, 20, 30] {
                if ui.button(format!("Last {m} minutes")).hint("Every key played by hand is remembered, recording or not; this writes the last minutes of it into the pattern on show").clicked() {
                    self.score_minutes = m;
                    let n = self.dump_score_log();
                    self.status = if n == 0 { "Nothing was played in that time".into() } else { format!("{n} played notes written to the pattern") };
                    ui.close_menu();
                }
            }
        });
        if mi(ui, "Clear log", "").clicked() {
            self.handle.transport.clear_score_log();
            ui.close_menu();
        }
        sect(ui, "Audio logger");
        if mi(ui, "Dump to playlist", "").hint("What came out of the program in the last two minutes becomes an audio clip").clicked() {
            self.dump_audio_log();
            ui.close_menu();
        }
        if mi(ui, "Clear log", "").clicked() {
            self.handle.transport.clear_audio_log();
            ui.close_menu();
        }
        sect(ui, "Last tweaked parameters");
        let tweaked: crate::widgets::Tweaked = ui.ctx().data(|d| d.get_temp(egui::Id::new("tweaked"))).unwrap_or_default();
        for (k, label) in ["Last tweaked", "Next to last tweaked"].into_iter().enumerate() {
            ui.add_enabled_ui(tweaked[k].is_some(), |ui| {
                ui.menu_button(label, |ui| {
                    let Some(target) = tweaked[k] else { return };
                    ui.label(RichText::new(self.song.target_name(target)).color(TEXT_DIM).small());
                    if let Some(a) = crate::widgets::control_items(ui) {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ctl_ask"), Some((target, a))));
                        ui.close_menu();
                    }
                });
            });
        }
    }

    fn tools_macros(&mut self, ui: &mut Ui) {
        sect(ui, "Macros");
        if mi(ui, "Stop sound", "Ctrl+H").clicked() {
            self.handle.send(Cmd::Panic);
            ui.close_menu();
        }
        ui.separator();
        if mi(ui, "Prepare for performance mode", "").clicked() {
            self.prepare_performance();
            ui.close_menu();
        }
        if mi(ui, "Prepare for MIDI export", "").hint("Every sampler and instrument channel becomes a MIDI Sender on its own MIDI channel; the sounds themselves are given up").clicked() {
            let n = self.song.prepare_midi_export();
            if n > 0 {
                self.dirty = true;
            }
            self.status = format!("{n} channels turned into MIDI Senders");
            ui.close_menu();
        }
        if mi(ui, "Select unused channels", "").hint("Channels with no notes in any pattern and no clip on the playlist").clicked() {
            let n = self.select_unused_channels();
            self.status = format!("{n} unused channels selected");
            ui.close_menu();
        }
        if mi(ui, "Purge unused audio clips", "").hint("Audio clip channels that lie nowhere on the playlist are removed").clicked() {
            let mut n = 0;
            for ci in (0..self.song.channels.len()).rev() {
                let unused = self.song.channels[ci].kind == cereal_engine::flp::ChannelKind::AudioClip && !self.song.all_clips().any(|c| matches!(c.src, cereal_engine::song::ClipSrc::Audio(a) if a == ci));
                if unused && self.song.channels.len() > 1 {
                    self.delete_channel(ci);
                    n += 1;
                }
            }
            self.status = format!("{n} unused audio clips removed");
            ui.close_menu();
        }
        ui.menu_button("Switch all audio clip channels to", |ui| {
            for (keep, label) in [(false, "Resample"), (true, "Stretch")] {
                if ui.button(label).hint(if keep { "Every audio clip keeps its pitch when its length is changed" } else { "Every audio clip changes pitch with its speed" }).clicked() {
                    let mut n = 0;
                    for ch in self.song.channels.iter_mut().filter(|c| c.kind == cereal_engine::flp::ChannelKind::AudioClip) {
                        if ch.smp.keep_pitch != keep {
                            ch.smp.keep_pitch = keep;
                            n += 1;
                        }
                    }
                    self.dirty |= n > 0;
                    self.status = format!("{n} audio clips switched");
                    ui.close_menu();
                }
            }
        });
        for (on, label) in [(true, "Lock all playlist tracks to content"), (false, "Unlock all playlist tracks from content")] {
            if mi(ui, label, "").clicked() {
                let mut n = 0;
                for (t, tr) in self.song.tracks.iter_mut().enumerate() {
                    let used = !on || self.song.clips.iter().any(|c| c.track as usize == t);
                    if used && tr.lock_content != on {
                        tr.lock_content = on;
                        n += 1;
                    }
                }
                self.dirty |= n > 0;
                self.status = format!("{n} tracks {}", if on { "locked to their content" } else { "unlocked" });
                ui.close_menu();
            }
        }
        if mi(ui, "Reset empty playlist tracks", "").hint("Tracks with no clip on them get their plain name and colour back").clicked() {
            let mut n = 0;
            let plain = cereal_engine::song::Track::default();
            for (t, tr) in self.song.tracks.iter_mut().enumerate() {
                let used = self.song.clips.iter().any(|c| c.track as usize == t);
                if !used && (tr.name != plain.name || tr.color != plain.color) {
                    tr.name = plain.name.clone();
                    tr.color = plain.color;
                    n += 1;
                }
            }
            self.dirty |= n > 0;
            self.status = format!("{n} empty tracks reset");
            ui.close_menu();
        }
        if mi(ui, "Remove all Wave Recorder instances", "").hint("Takes every Wave Recorder out of the mixer's effect slots").clicked() {
            let mut n = 0;
            for ins in self.song.inserts.iter_mut() {
                let before = ins.slots.len();
                ins.slots.retain(|s| !s.plugin.eq_ignore_ascii_case("Edison"));
                n += before - ins.slots.len();
            }
            if n > 0 {
                self.fx_open = None;
                self.dirty = true;
            }
            self.status = format!("{n} Wave Recorders removed");
            ui.close_menu();
        }
        if mi(ui, "Delete empty patterns", "").hint("Patterns with no notes or events that lie nowhere on the playlist; named ones are kept").clicked() {
            let n = self.purge_patterns();
            self.status = format!("{n} empty patterns removed");
            ui.close_menu();
        }
    }
}
