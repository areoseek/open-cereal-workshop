//! First-run content setup.
//!
//! This program ships none of the reference program's sounds, presets or demo songs. A user who has their own
//! copy (the free trial download counts) can point us at it, and we then read those files in place from their
//! disk: nothing is copied into the project, nothing is bundled, nothing is uploaded. Their use of those files
//! stays under the licence they accepted when they installed that copy.

use std::path::{Path, PathBuf};

use egui::{Align2, Color32, RichText};

use crate::widgets::*;
use crate::App;

#[derive(Default)]
pub struct State {
    pub open: bool,
    msg: String,
    found: Vec<PathBuf>,
    searched: bool,
}

#[derive(Default)]
pub struct Settings {
    pub content: Vec<PathBuf>,
    pub never_ask: bool,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").or_else(|| std::env::var_os("APPDATA")).map(PathBuf::from).or_else(|| home().map(|h| h.join(".config")))?;
    Some(base.join("breakfast-cereal").join("settings.txt"))
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Some(text) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) else { return s };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("content", v)) if !v.trim().is_empty() => s.content.push(PathBuf::from(v.trim())),
                Some(("content_prompt", v)) => s.never_ask = v.trim() == "never",
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(p) = settings_path() else { return Ok(()) };
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = String::new();
        for c in &self.content {
            out += &format!("content={}\n", c.display());
        }
        if self.never_ask {
            out += "content_prompt=never\n";
        }
        std::fs::write(p, out)
    }
}

fn child_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_dir() {
        return Some(direct);
    }
    let want = name.to_lowercase();
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().map(|n| n.to_string_lossy().to_lowercase() == want).unwrap_or(false))
}

/// The install folder that `picked` is, or sits inside: the one holding `Data/Patches`.
pub fn install_root(picked: &Path) -> Option<PathBuf> {
    let mut cur = Some(picked);
    for _ in 0..4 {
        let dir = cur?;
        if child_ci(dir, "Data").and_then(|d| child_ci(&d, "Patches")).is_some() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    // or one level down: the vendor folder holding one or more versions
    std::fs::read_dir(picked).ok()?.flatten().map(|e| e.path()).find(|p| child_ci(p, "Data").and_then(|d| child_ci(&d, "Patches")).is_some())
}

/// Look in the usual places for an installed copy.
pub fn detect() -> Vec<PathBuf> {
    let mut drives: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        for v in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(p) = std::env::var_os(v) {
                drives.push(PathBuf::from(p));
            }
        }
    } else if let Some(h) = home() {
        let mut prefixes = vec![h.join(".wine")];
        if let Some(p) = std::env::var_os("WINEPREFIX") {
            prefixes.push(PathBuf::from(p));
        }
        for many in [h.join(".local/share/wineprefixes"), h.join(".local/share/bottles/bottles"), h.join(".var/app/com.usebottles.bottles/data/bottles/bottles"), h.join("Games")] {
            if let Ok(rd) = std::fs::read_dir(many) {
                prefixes.extend(rd.flatten().map(|e| e.path()));
            }
        }
        for p in prefixes {
            drives.push(p.join("drive_c/Program Files"));
            drives.push(p.join("drive_c/Program Files (x86)"));
        }
    }
    let mut out = vec![];
    for d in drives {
        let Some(vendor) = child_ci(&d, "Image-Line") else { continue };
        let Ok(rd) = std::fs::read_dir(vendor) else { continue };
        for e in rd.flatten() {
            if let Some(root) = install_root(&e.path()) {
                if !out.contains(&root) {
                    out.push(root);
                }
            }
        }
    }
    out.sort();
    out.reverse(); // newest version name first
    out
}

/// Start the user's own installer so it can unpack itself. We never unpack it ourselves.
fn run_installer(exe: &Path) -> Result<(), String> {
    let mut cmd = if cfg!(windows) {
        std::process::Command::new(exe)
    } else {
        let mut c = std::process::Command::new("wine");
        c.arg(exe);
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| if cfg!(windows) { format!("Could not start the installer: {e}") } else { format!("Could not start the installer with Wine ({e}). Install Wine, or run the installer on a Windows machine and copy the installed folder over.") })
}

impl App {
    /// Start reading a user's installed copy: sample paths, and the browser folders.
    pub(crate) fn add_content_root(&mut self, root: &Path) {
        if !self.lib.factory_roots.iter().any(|r| r == root) {
            self.lib.factory_roots.push(root.to_path_buf());
        }
        for (label, sub) in [("Packs", "Data/Patches/Packs"), ("Presets", "Data/Patches"), ("Demo projects", "Data/Demo projects"), ("Templates", "Data/Templates")] {
            self.browser.add_root(label, root.join(sub));
        }
    }

    fn setup_use(&mut self, root: PathBuf) {
        self.add_content_root(&root);
        if !self.settings.content.contains(&root) {
            self.settings.content.push(root.clone());
        }
        let saved = self.settings.save();
        self.status = match saved {
            Ok(()) => format!("Using your own content from {}", root.display()),
            Err(e) => format!("Using {} for this session (could not save the setting: {e})", root.display()),
        };
        self.setup.open = false;
        // a project opened before this may have had missing samples
        if let (Some(p), false) = (self.path.clone(), self.unsaved) {
            self.open(&p);
        }
    }

    pub(crate) fn setup_ui(&mut self, ctx: &egui::Context) {
        if !self.setup.open {
            return;
        }
        if !self.setup.searched {
            self.setup.searched = true;
            self.setup.found = detect();
        }
        let mut pick: Option<PathBuf> = None;
        let mut close = false;
        // dim everything behind
        egui::Area::new(egui::Id::new("setup_dim")).order(egui::Order::Middle).fixed_pos(egui::Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.allocate_rect(r, egui::Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(150));
        });
        egui::Window::new("Sounds and presets").id(egui::Id::new("setup")).order(egui::Order::Foreground).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Bring your own sounds").size(17.0).color(TEXT));
            ui.add_space(4.0);
            ui.label("Breakfast Cereal DAW does not include, and cannot include, the sounds, presets and demo songs that come with FL Studio. Projects made there usually point at those files.");
            ui.add_space(4.0);
            ui.label("If you have your own copy of FL Studio installed (the free trial from the maker's website counts), point this program at it. The files are read where they are on your disk. Nothing is copied into this program, bundled or uploaded, and your use of them stays under the licence you accepted from Image-Line.");
            ui.add_space(8.0);
            if !self.setup.found.is_empty() {
                ui.label(RichText::new("Found on this computer").color(TEXT_DIM));
                for f in self.setup.found.clone() {
                    ui.horizontal(|ui| {
                        if ui.button("Use this").clicked() {
                            pick = Some(f.clone());
                        }
                        ui.label(RichText::new(f.display().to_string()).monospace().size(11.0));
                    });
                }
                ui.add_space(6.0);
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Choose the installed folder...").on_hover_text("The folder that holds the Data folder, for example Program Files/Image-Line/<version>").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Choose your installed copy").pick_folder() {
                        match install_root(&dir) {
                            Some(root) => pick = Some(root),
                            None => self.setup.msg = format!("{} does not look like an installed copy (no Data/Patches folder inside).", dir.display()),
                        }
                    }
                }
                if ui.button("I only have the installer file...").on_hover_text("Runs your downloaded installer so it can install itself; come back here when it has finished").clicked() {
                    if let Some(exe) = rfd::FileDialog::new().set_title("Choose the installer you downloaded").add_filter("Installer", &["exe"]).pick_file() {
                        self.setup.msg = match run_installer(&exe) {
                            Ok(()) => "The installer is running. Finish it, then press Search again.".into(),
                            Err(e) => e,
                        };
                    }
                }
                if ui.button("Search again").clicked() {
                    self.setup.found = detect();
                    self.setup.msg = if self.setup.found.is_empty() { "No installed copy found in the usual places. Choose the folder by hand.".into() } else { String::new() };
                }
            });
            if !self.setup.msg.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.setup.msg).color(Color32::from_rgb(240, 170, 70)));
            }
            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Not now").on_hover_text("Ask again next time the program starts").clicked() {
                    close = true;
                }
                if ui.button("Do not ask again").on_hover_text("You can still do this later from Options").clicked() {
                    self.settings.never_ask = true;
                    let _ = self.settings.save();
                    close = true;
                }
                ui.label(RichText::new("Without this, the program works with its own instruments and your own sample folders.").small().color(TEXT_DIM));
            });
        });
        if let Some(root) = pick {
            self.setup_use(root);
        } else if close {
            self.setup.open = false;
        }
    }
}
