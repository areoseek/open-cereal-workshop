//! First-run content setup.
//!
//! This program ships none of the reference program's sounds, presets or demo songs. A user who downloaded the
//! installer themselves (the free trial counts) can point us at that one file; `unpack` reads the content files
//! out of it into this program's data folder on their own disk. An already installed copy can be used in place
//! instead. Nothing is bundled or uploaded, and their use of those files stays under the maker's licence.

use crate::widgets::Hint as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};

use egui::{Align2, Color32, RichText};

use crate::unpack::{self, Progress};
use crate::widgets::*;
use crate::App;

#[derive(Default)]
pub struct State {
    pub open: bool,
    msg: String,
    found: Vec<PathBuf>,
    searched: bool,
    job: Option<Job>,
}

struct Job {
    progress: Arc<Progress>,
    result: mpsc::Receiver<Result<usize, String>>,
    dest: PathBuf,
}

#[derive(Default)]
pub struct Settings {
    pub content: Vec<PathBuf>,
    pub never_ask: bool,
    /// The sound card to play through and to record from, by name; empty = the system's default.
    pub audio_out: String,
    pub audio_in: String,
    /// Sample rate and buffer length in samples; 0 = whatever the device suggests.
    pub rate: u32,
    pub buffer: u32,
    /// MIDI inputs that are switched off, by name.
    pub midi_off: Vec<String>,
    /// MIDI outputs that have a port number, by name.
    pub midi_out: Vec<(String, u8)>,
    /// Send the clock and start/stop to the outputs that have a port number.
    pub midi_sync: bool,
    /// The MIDI channel whose notes play the clips of a performance (1..=16), 0 for none.
    pub midi_perf: u8,
    /// Size of the whole interface; 0 = as the system says.
    pub scale: f32,
    /// The colour of the playlist and piano roll grid, when it is not the usual one.
    pub grid: Option<[u8; 3]>,
    /// Extra folders for the browser and the sound search.
    pub folders: Vec<PathBuf>,
    /// The projects opened or saved last, newest first.
    pub recent: Vec<PathBuf>,
    /// The browser files marked as favourites.
    pub starred: Vec<PathBuf>,
    /// Tags given to browser files.
    pub tags: Vec<(PathBuf, Vec<String>)>,
    /// The browser pages of the user's own: name, then a folder ('F') or a search ('S') and what it holds.
    pub tabs: Vec<(String, char, String)>,
    /// Saved window layouts: name, the four big windows (left, top, width, height each), and which of
    /// playlist, rack, piano roll, mixer and browser are on show.
    pub layouts: Vec<(String, [f32; 16], [bool; 5])>,
    /// A picture behind the windows.
    pub backdrop: Option<PathBuf>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").or_else(|| std::env::var_os("APPDATA")).map(PathBuf::from).or_else(|| home().map(|h| h.join(".config")))?;
    Some(base.join("breakfast-cereal").join("settings.txt"))
}

/// Where the note scripts the user has let run are kept, one text to a file.
fn scripts_dir() -> Option<PathBuf> {
    Some(settings_path()?.parent()?.join("scripts-allowed"))
}

/// Tell the engine which note scripts the user let run on earlier days.
pub fn load_trusted_scripts(content: &[PathBuf]) {
    let mut set = std::collections::HashSet::new();
    // the stock scripts of the content the user brought in themselves need no asking
    fn walk(d: &Path, depth: u8, set: &mut std::collections::HashSet<String>) {
        for f in std::fs::read_dir(d).into_iter().flatten().flatten().map(|f| f.path()) {
            if f.is_dir() {
                if depth < 4 {
                    walk(&f, depth + 1, set);
                }
            } else if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("fst")) {
                if let Some(d) = cereal_engine::preset::read(&f).ok().and_then(|p| cereal_engine::script::ScriptDef::decode(&p.state)) {
                    if !d.source.is_empty() {
                        set.insert(d.source);
                    }
                }
            }
        }
    }
    for root in content {
        walk(&root.join("Patches/Plugin presets/Effects/VFX Script"), 0, &mut set);
    }
    if let Some(list) = scripts_dir().and_then(|d| std::fs::read_dir(d).ok()) {
        for f in list.flatten() {
            if let Ok(text) = std::fs::read_to_string(f.path()) {
                set.insert(text);
            }
        }
    }
    cereal_engine::script::set_trust(cereal_engine::script::Trust::These(set));
}

/// Let a note script run, today and from now on.
pub fn trust_script(source: &str) {
    cereal_engine::script::allow(source);
    let Some(dir) = scripts_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(source, &mut h);
    let _ = std::fs::write(dir.join(format!("{:016x}.py", std::hash::Hasher::finish(&h))), source);
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Some(text) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) else { return s };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("content", v)) if !v.trim().is_empty() => s.content.push(PathBuf::from(v.trim())),
                Some(("folder", v)) if !v.trim().is_empty() => s.folders.push(PathBuf::from(v.trim())),
                Some(("recent", v)) if !v.trim().is_empty() => s.recent.push(PathBuf::from(v.trim())),
                Some(("star", v)) if !v.trim().is_empty() => s.starred.push(PathBuf::from(v.trim())),
                Some(("tag", v)) => {
                    if let Some((tags, path)) = v.split_once('\t') {
                        let tags: Vec<String> = tags.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
                        if !tags.is_empty() && !path.is_empty() {
                            s.tags.push((PathBuf::from(path), tags));
                        }
                    }
                }
                Some(("tab", v)) => {
                    let mut it = v.splitn(3, '\t');
                    if let (Some(name), Some(kind), Some(what)) = (it.next(), it.next(), it.next()) {
                        if let Some(k) = kind.chars().next().filter(|k| *k == 'F' || *k == 'S') {
                            s.tabs.push((name.to_string(), k, what.to_string()));
                        }
                    }
                }
                Some(("layout", v)) => {
                    let mut it = v.splitn(3, '\t');
                    if let (Some(name), Some(nums), Some(flags)) = (it.next(), it.next(), it.next()) {
                        let n: Vec<f32> = nums.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                        let f: Vec<bool> = flags.trim().chars().map(|c| c == '1').collect();
                        if let (Ok(n), Ok(f)) = (<[f32; 16]>::try_from(n), <[bool; 5]>::try_from(f)) {
                            s.layouts.push((name.to_string(), n, f));
                        }
                    }
                }
                Some(("backdrop", v)) => s.backdrop = Some(PathBuf::from(v.trim())).filter(|p| !p.as_os_str().is_empty()),
                Some(("content_prompt", v)) => s.never_ask = v.trim() == "never",
                Some(("audio_out", v)) => s.audio_out = v.trim().to_string(),
                Some(("audio_in", v)) => s.audio_in = v.trim().to_string(),
                Some(("rate", v)) => s.rate = v.trim().parse().unwrap_or(0),
                Some(("buffer", v)) => s.buffer = v.trim().parse().unwrap_or(0),
                Some(("midi_off", v)) if !v.trim().is_empty() => s.midi_off.push(v.trim().to_string()),
                Some(("midi_out", v)) => {
                    // port number, then the name (which may hold any character)
                    if let Some((n, name)) = v.trim().split_once(' ') {
                        if let Ok(n) = n.parse() {
                            s.midi_out.push((name.to_string(), n));
                        }
                    }
                }
                Some(("midi_sync", v)) => s.midi_sync = v.trim() == "1",
                Some(("midi_perf", v)) => s.midi_perf = v.trim().parse::<u8>().unwrap_or(0).min(16),
                Some(("scale", v)) => s.scale = v.trim().parse().unwrap_or(0.0),
                Some(("grid", v)) => {
                    let c: Vec<u8> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    s.grid = (c.len() == 3).then(|| [c[0], c[1], c[2]]);
                }
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
        for (k, v) in [("audio_out", &self.audio_out), ("audio_in", &self.audio_in)] {
            if !v.is_empty() {
                out += &format!("{k}={v}\n");
            }
        }
        for f in &self.folders {
            out += &format!("folder={}\n", f.display());
        }
        for f in &self.recent {
            out += &format!("recent={}\n", f.display());
        }
        for f in &self.starred {
            out += &format!("star={}\n", f.display());
        }
        for (f, tags) in &self.tags {
            out += &format!("tag={}\t{}\n", tags.join(","), f.display());
        }
        for (name, kind, what) in &self.tabs {
            out += &format!("tab={}\t{kind}\t{what}\n", name.replace(['\t', '\n'], " "));
        }
        for (name, n, f) in &self.layouts {
            let nums: Vec<String> = n.iter().map(|x| format!("{x:.0}")).collect();
            let flags: String = f.iter().map(|b| if *b { '1' } else { '0' }).collect();
            out += &format!("layout={}\t{}\t{flags}\n", name.replace(['\t', '\n'], " "), nums.join(","));
        }
        if let Some(p) = &self.backdrop {
            out += &format!("backdrop={}\n", p.display());
        }
        if self.rate != 0 {
            out += &format!("rate={}\n", self.rate);
        }
        if self.buffer != 0 {
            out += &format!("buffer={}\n", self.buffer);
        }
        for m in &self.midi_off {
            out += &format!("midi_off={m}\n");
        }
        for (name, n) in &self.midi_out {
            out += &format!("midi_out={n} {name}\n");
        }
        if self.midi_sync {
            out += "midi_sync=1\n";
        }
        if self.midi_perf > 0 {
            out += &format!("midi_perf={}\n", self.midi_perf);
        }
        if self.scale > 0.0 {
            out += &format!("scale={}\n", self.scale);
        }
        if let Some([r, g, b]) = self.grid {
            out += &format!("grid={r},{g},{b}\n");
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

/// Where unpacked content lives: this program's own data folder.
fn content_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").or_else(|| std::env::var_os("LOCALAPPDATA")).map(PathBuf::from).or_else(|| home().map(|h| h.join(".local/share")))?;
    Some(base.join("breakfast-cereal").join("content"))
}

fn start_unpack(installer: PathBuf) -> Result<Job, String> {
    let dest = content_dir().ok_or("No home folder to put the files in.")?;
    let progress = Arc::new(Progress::default());
    let (tx, rx) = mpsc::channel();
    let (p, d) = (progress.clone(), dest.clone());
    std::thread::Builder::new().name("unpack".into()).spawn(move || {
        let _ = tx.send(unpack::unpack(&installer, &d, &p));
    }).map_err(|e| e.to_string())?;
    Ok(Job { progress, result: rx, dest })
}

impl App {
    /// Start reading a user's installed copy: sample paths, and the browser folders.
    pub(crate) fn add_content_root(&mut self, root: &Path) {
        if !self.lib.factory_roots.iter().any(|r| r == root) {
            self.lib.factory_roots.push(root.to_path_buf());
        }
        cereal_engine::keys::add_root(root);
        self.preset_lists.clear();
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
            // for tests: start on an installer without the file dialog
            if let Some(exe) = std::env::var_os("CEREAL_SETUP_INSTALLER") {
                match start_unpack(PathBuf::from(exe)) {
                    Ok(job) => self.setup.job = Some(job),
                    Err(e) => self.setup.msg = e,
                }
            }
        }
        let mut pick: Option<PathBuf> = None;
        let mut close = false;
        if let Some(res) = self.setup.job.as_ref().and_then(|j| j.result.try_recv().ok()) {
            let job = self.setup.job.take().unwrap();
            match res {
                Ok(_) => pick = Some(job.dest),
                Err(e) => self.setup.msg = e,
            }
        }
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
            ui.label("All this needs is the FL Studio installer file, which you download yourself from Image-Line (the free trial is the same file). Choose it below and the sounds, presets, demo projects and templates are read out of it into this program's data folder on your disk. Nothing is installed or run, nothing is uploaded, and your use of those files stays under Image-Line's licence.");
            ui.add_space(8.0);
            if let Some(job) = &self.setup.job {
                let (done, total) = (job.progress.done.load(Ordering::Relaxed), job.progress.total.load(Ordering::Relaxed));
                let frac = if total == 0 { 0.0 } else { done as f32 / total as f32 };
                ui.add(egui::ProgressBar::new(frac).text(if total == 0 { "Reading the installer...".to_string() } else { format!("{done} of {total} files") }));
                if ui.button("Cancel").clicked() {
                    job.progress.cancel.store(true, Ordering::Relaxed);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            } else {
                if ui.add(egui::Button::new(RichText::new("Choose the installer file...").size(14.0)).min_size(egui::vec2(220.0, 30.0))).clicked() {
                    if let Some(exe) = rfd::FileDialog::new().set_title("Choose the installer you downloaded").add_filter("Installer", &["exe"]).pick_file() {
                        match start_unpack(exe) {
                            Ok(job) => {
                                self.setup.job = Some(job);
                                self.setup.msg.clear();
                            }
                            Err(e) => self.setup.msg = e,
                        }
                    }
                }
                ui.add_space(6.0);
                ui.collapsing("Already installed on this computer?", |ui| {
                    for f in self.setup.found.clone() {
                        ui.horizontal(|ui| {
                            if ui.button("Use this").clicked() {
                                pick = Some(f.clone());
                            }
                            ui.label(RichText::new(f.display().to_string()).monospace().size(11.0));
                        });
                    }
                    if ui.button("Choose the installed folder...").hint("The folder that holds the Data folder").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title("Choose your installed copy").pick_folder() {
                            match install_root(&dir) {
                                Some(root) => pick = Some(root),
                                None => self.setup.msg = format!("{} does not look like an installed copy (no Data/Patches folder inside).", dir.display()),
                            }
                        }
                    }
                });
            }
            if !self.setup.msg.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.setup.msg).color(Color32::from_rgb(240, 170, 70)));
            }
            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Not now").hint("Ask again next time the program starts").clicked() {
                    close = true;
                }
                if ui.button("Do not ask again").hint("You can still do this later from Options").clicked() {
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
            if let Some(job) = self.setup.job.take() {
                job.progress.cancel.store(true, Ordering::Relaxed);
            }
            self.setup.open = false;
        }
    }
}
