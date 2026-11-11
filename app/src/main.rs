//! Breakfast Cereal DAW: the application window. Toolbar and transport on top, browser on the left, and the
//! channel rack, piano roll, playlist and mixer as windows on the workspace.

mod browser;
mod mixer;
mod pianoroll;
mod playlist;
mod rack;
mod widgets;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use egui::{Key, RichText, Vec2};

use cereal_engine::flp::{ChannelKind, Flp};
use cereal_engine::song::Chan;
use cereal_engine::{new_player, wav, Cmd, Handle, Library, Player, Song};

use widgets::*;

pub(crate) enum Action {
    Preview(PathBuf),
    Open(PathBuf),
    /// Load a sample into a channel, or into a new one.
    LoadSample(PathBuf, Option<usize>),
}

fn start_audio() -> Result<(cpal::Stream, String, f32, Handle), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let channels = config.channels() as usize;
    let rate = config.sample_rate().0 as f32;
    let name = format!("{} ({} Hz, {} ch)", device.name().unwrap_or_default(), rate as u32, channels);
    let (handle, mut player): (Handle, Player) = new_player(Song::new(), rate);
    let err = |e| eprintln!("audio stream error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(&config.into(), move |out: &mut [f32], _| player.render(out, channels), err, None),
        cpal::SampleFormat::I16 => {
            let mut scratch = vec![0.0f32; 1 << 16];
            device.build_output_stream(
                &config.into(),
                move |out: &mut [i16], _| {
                    let n = out.len().min(scratch.len());
                    player.render(&mut scratch[..n], channels);
                    for (o, s) in out.iter_mut().zip(&scratch[..n]) {
                        *o = (s * 32767.0) as i16;
                    }
                },
                err,
                None,
            )
        }
        other => return Err(format!("unsupported sample format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name, rate, handle))
}

pub(crate) struct App {
    song: Song,
    dirty: bool,
    /// Changed since the last save.
    unsaved: bool,
    /// The song was just loaded or created: publish it without counting that as an edit.
    fresh: bool,
    handle: Handle,
    lib: Library,
    /// The file the song came from, kept whole so saving loses nothing we do not understand.
    flp: Option<Flp>,
    path: Option<PathBuf>,
    _stream: Option<cpal::Stream>,
    audio: String,
    status: String,
    show_rack: bool,
    show_piano: bool,
    show_playlist: bool,
    show_mixer: bool,
    show_browser: bool,
    typing_keys: bool,
    sel_chan: usize,
    sel_insert: usize,
    rack: rack::State,
    piano: pianoroll::State,
    playlist: playlist::State,
    browser: browser::State,
    peaks: Vec<(f32, f32)>,
    held: HashMap<Key, (u16, u8)>,
    actions: Vec<Action>,
    /// Undo history: snapshots of the song, oldest first; `hist_at` is the one on screen.
    history: Vec<Song>,
    hist_at: usize,
    /// An edit is under way (mouse still down); it becomes one history step when it ends.
    hist_pending: bool,
    show_info: bool,
    show_about: bool,
    rename_pattern: Option<(usize, String)>,
}

const HISTORY_MAX: usize = 100;

/// Computer keys that play notes, as semitones above C5.
const TYPING: [(Key, u8); 30] = [
    (Key::Z, 0),
    (Key::S, 1),
    (Key::X, 2),
    (Key::D, 3),
    (Key::C, 4),
    (Key::V, 5),
    (Key::G, 6),
    (Key::B, 7),
    (Key::H, 8),
    (Key::N, 9),
    (Key::J, 10),
    (Key::M, 11),
    (Key::Comma, 12),
    (Key::Q, 12),
    (Key::Num2, 13),
    (Key::W, 14),
    (Key::Num3, 15),
    (Key::E, 16),
    (Key::R, 17),
    (Key::Num5, 18),
    (Key::T, 19),
    (Key::Num6, 20),
    (Key::Y, 21),
    (Key::Num7, 22),
    (Key::U, 23),
    (Key::I, 24),
    (Key::Num9, 25),
    (Key::O, 26),
    (Key::Num0, 27),
    (Key::P, 28),
];

impl App {
    fn new(open: Option<PathBuf>) -> Self {
        let (stream, audio, handle) = match start_audio() {
            Ok((s, name, _rate, h)) => (Some(s), name, h),
            Err(e) => {
                // no device: keep a player nobody pulls so the rest of the app still works
                let (h, _p) = new_player(Song::new(), 44_100.0);
                (None, format!("NO AUDIO: {e}"), h)
            }
        };
        let lib = Library::new();
        let mut browser = browser::State::default();
        for r in &lib.factory_roots {
            browser.add_root("Packs", r.join("Data/Patches/Packs"));
            browser.add_root("Demo projects", r.join("Data/Demo projects"));
            browser.add_root("Templates", r.join("Data/Templates"));
        }
        for d in &lib.search_dirs {
            browser.add_root(&d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), d.clone());
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) {
            browser.add_root("Music", home.join("Music"));
            browser.add_root("Documents", home.join("Documents"));
        }
        let mut app = Self {
            song: Song::new(),
            dirty: true,
            unsaved: false,
            fresh: true,
            handle,
            lib,
            flp: None,
            path: None,
            _stream: stream,
            audio,
            status: String::new(),
            show_rack: true,
            show_piano: false,
            show_playlist: true,
            show_mixer: false,
            show_browser: true,
            typing_keys: true,
            sel_chan: 0,
            sel_insert: 0,
            rack: Default::default(),
            piano: Default::default(),
            playlist: Default::default(),
            browser,
            peaks: vec![],
            held: HashMap::new(),
            actions: vec![],
            history: vec![],
            hist_at: 0,
            hist_pending: false,
            show_info: false,
            show_about: false,
            rename_pattern: None,
        };
        if let Some(p) = open {
            app.open(&p);
        }
        app
    }

    fn stop(&mut self) {
        self.handle.transport.set_playing(false);
        self.handle.transport.seek(0);
    }

    fn new_project(&mut self) {
        self.stop();
        self.handle.send(Cmd::Panic);
        self.song = Song::new();
        self.flp = None;
        self.path = None;
        self.handle.transport.set_pattern(0);
        self.handle.transport.set_song_mode(false);
        self.sel_chan = 0;
        self.sel_insert = 0;
        self.dirty = true;
        self.fresh = true;
        self.unsaved = false;
        self.status = "New project".into();
    }

    fn open(&mut self, path: &Path) {
        self.stop();
        self.handle.send(Cmd::Panic);
        self.lib.missing.clear();
        match cereal_engine::load_flp(path, &mut self.lib) {
            Ok((song, f)) => {
                let missing = self.lib.missing.len();
                self.handle.transport.set_pattern((f.current_pattern as usize).saturating_sub(1).min(song.patterns.len().saturating_sub(1)));
                self.handle.transport.set_song_mode(!song.clips.is_empty());
                self.status = format!("Opened {} (made with {}){}", path.display(), f.version, if missing > 0 { format!(", {missing} samples not found") } else { String::new() });
                self.song = song;
                self.flp = Some(f);
                self.path = Some(path.to_path_buf());
                self.sel_chan = 0;
                self.sel_insert = 0;
                self.dirty = true;
                self.fresh = true;
                self.unsaved = false;
                if let Some(dir) = path.parent() {
                    self.browser.add_root("Project folder", dir.to_path_buf());
                }
            }
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    fn open_dialog(&mut self) {
        let mut d = rfd::FileDialog::new().add_filter("Project files", &["flp"]);
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        if let Some(p) = d.pick_file() {
            self.open(&p);
        }
    }

    fn save(&mut self, ask: bool) {
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let mut d = rfd::FileDialog::new().add_filter("Project files", &["flp"]).set_file_name(self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "untitled.flp".into()));
                if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
                    d = d.set_directory(dir);
                }
                d.save_file().map(|p| if p.extension().is_none() { p.with_extension("flp") } else { p })
            }
        };
        let Some(path) = path else { return };
        let saved = cereal_engine::save::save(&mut self.song, self.flp.as_ref()).and_then(|(bytes, flp)| std::fs::write(&path, bytes).map(|_| flp).map_err(|e| e.to_string()));
        match saved {
            Ok(flp) => {
                self.flp = Some(flp);
                self.status = format!("Saved {}", path.display());
                self.path = Some(path);
                self.unsaved = false;
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    fn export(&mut self) {
        let default = self.path.as_ref().map(|p| p.with_extension("wav")).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "untitled.wav".into());
        let Some(path) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).set_file_name(default).save_file() else { return };
        let rate = 44_100.0;
        let song_mode = self.handle.transport.song_mode();
        let buf = cereal_engine::render_song(&self.song, rate, song_mode, self.handle.transport.pattern(), 6.0);
        let r = std::fs::File::create(&path).and_then(|f| wav::write_pcm16(std::io::BufWriter::new(f), &buf, rate as u32, 2));
        self.status = match r {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    fn load_sample(&mut self, path: &Path, into: Option<usize>) {
        match self.lib.load_path(path) {
            Ok(s) => {
                let ci = match into.filter(|i| *i < self.song.channels.len()) {
                    Some(i) => i,
                    None => {
                        self.song.channels.push(Chan::default());
                        self.song.channels.len() - 1
                    }
                };
                let c = &mut self.song.channels[ci];
                c.name = s.name.clone();
                c.kind = ChannelKind::Sampler;
                c.plugin.clear();
                c.sample_path = path.to_string_lossy().to_string();
                c.sample = Some(s);
                self.sel_chan = ci;
                self.dirty = true;
            }
            Err(e) => self.status = e,
        }
    }

    fn run_actions(&mut self) {
        for a in std::mem::take(&mut self.actions) {
            match a {
                Action::Preview(p) => match self.lib.load_path(&p) {
                    Ok(s) => self.handle.send(Cmd::Preview(s)),
                    Err(e) => self.status = e,
                },
                Action::Open(p) => self.open(&p),
                Action::LoadSample(p, into) => self.load_sample(&p, into),
            }
        }
    }

    /// Step back through the history.
    fn undo(&mut self) {
        if self.hist_at > 0 {
            self.hist_at -= 1;
            self.restore();
        }
    }

    fn redo(&mut self) {
        if self.hist_at + 1 < self.history.len() {
            self.hist_at += 1;
            self.restore();
        }
    }

    /// Ctrl+Z: undo the last edit, or bring it back if it was just undone.
    fn undo_toggle(&mut self) {
        if self.hist_at + 1 < self.history.len() {
            self.redo();
        } else {
            self.undo();
        }
    }

    fn restore(&mut self) {
        self.song = self.history[self.hist_at].clone();
        self.sel_chan = self.sel_chan.min(self.song.channels.len().saturating_sub(1));
        self.sel_insert = self.sel_insert.min(self.song.inserts.len().saturating_sub(1));
        let tr = &self.handle.transport;
        tr.set_pattern(tr.pattern().min(self.song.patterns.len().saturating_sub(1)));
        self.unsaved = true;
        self.handle.publish(self.song.clone());
    }

    fn new_pattern(&mut self) {
        let p = self.song.next_empty_pattern();
        self.handle.transport.set_pattern(p);
        self.dirty = true;
    }

    fn clone_pattern(&mut self) {
        let p = self.song.clone_pattern(self.handle.transport.pattern());
        self.handle.transport.set_pattern(p);
        self.dirty = true;
    }

    fn delete_pattern(&mut self) {
        let p = self.handle.transport.pattern();
        self.song.remove_pattern(p);
        self.handle.transport.set_pattern(p.min(self.song.patterns.len() - 1));
        self.dirty = true;
    }

    fn start_rename_pattern(&mut self) {
        let p = self.handle.transport.pattern();
        self.song.ensure_pattern(p);
        self.rename_pattern = Some((p, self.song.patterns[p].name.clone()));
    }

    fn close_all(&mut self) {
        self.show_playlist = false;
        self.show_rack = false;
        self.show_piano = false;
        self.show_mixer = false;
        self.show_info = false;
    }

    fn step_pattern(&mut self, by: i32) {
        let p = (self.handle.transport.pattern() as i32 + by).max(0) as usize;
        self.song.ensure_pattern(p);
        self.handle.transport.set_pattern(p);
        self.dirty = true;
    }

    fn keys(&mut self, ctx: &egui::Context) {
        let typing_text = ctx.wants_keyboard_input();
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed, repeat, modifiers, .. } = e else { continue };
            if !pressed {
                if let Some((chan, k)) = self.held.remove(&key) {
                    self.handle.send(Cmd::NoteOff { chan, key: k });
                }
                continue;
            }
            if typing_text {
                continue;
            }
            if modifiers.command {
                if repeat {
                    continue;
                }
                match key {
                    Key::Z if modifiers.alt => self.undo(),
                    Key::Z => self.undo_toggle(),
                    Key::N => self.new_project(),
                    Key::O => self.open_dialog(),
                    Key::S => self.save(modifiers.shift),
                    Key::R => self.export(),
                    Key::T => self.typing_keys = !self.typing_keys,
                    Key::M => {
                        let tr = &self.handle.transport;
                        tr.set_metronome(!tr.metronome());
                    }
                    Key::H => self.handle.send(Cmd::Panic),
                    Key::C if modifiers.shift => self.clone_pattern(),
                    Key::Space => {
                        let tr = &self.handle.transport;
                        tr.set_playing(!tr.playing());
                    }
                    _ => {}
                }
                continue;
            }
            if modifiers.alt {
                if key == Key::F8 && !repeat {
                    self.show_browser = !self.show_browser;
                }
                continue;
            }
            match key {
                Key::Space if !repeat => {
                    let tr = &self.handle.transport;
                    if tr.playing() {
                        self.stop();
                    } else {
                        tr.set_playing(true);
                    }
                }
                Key::F5 if !repeat => self.show_playlist = !self.show_playlist,
                Key::F6 if !repeat => self.show_rack = !self.show_rack,
                Key::F7 if !repeat => self.show_piano = !self.show_piano,
                Key::F9 if !repeat => self.show_mixer = !self.show_mixer,
                Key::F4 if !repeat => self.new_pattern(),
                Key::F2 if !repeat => self.start_rename_pattern(),
                Key::F11 if !repeat => self.show_info = !self.show_info,
                Key::F12 if !repeat => self.close_all(),
                Key::Plus | Key::Equals => self.step_pattern(1),
                Key::Minus => self.step_pattern(-1),
                Key::L if !self.typing_keys && !repeat => {
                    let tr = &self.handle.transport;
                    tr.set_song_mode(!tr.song_mode());
                }
                _ => {
                    if self.typing_keys && !repeat {
                        if let Some((_, semi)) = TYPING.iter().find(|(k, _)| *k == key) {
                            let chan = self.sel_chan as u16;
                            let k = 60 + semi;
                            self.handle.send(Cmd::NoteOn { chan, key: k, vel: 1.0 });
                            self.held.insert(key, (chan, k));
                        }
                    }
                }
            }
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(8, 4))).show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("FILE", |ui| {
                    if ui.button("New                    Ctrl+N").clicked() {
                        self.new_project();
                        ui.close_menu();
                    }
                    if ui.button("Open...               Ctrl+O").clicked() {
                        ui.close_menu();
                        self.open_dialog();
                    }
                    if ui.button("Save                   Ctrl+S").clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui.button("Save as...           Ctrl+Shift+S").clicked() {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    if ui.button("Export WAV...     Ctrl+R").clicked() {
                        ui.close_menu();
                        self.export();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("EDIT", |ui| {
                    let can_undo = self.hist_at > 0;
                    let can_redo = self.hist_at + 1 < self.history.len();
                    if ui.add_enabled(can_undo || can_redo, egui::Button::new("Undo / redo last edit     Ctrl+Z")).clicked() {
                        self.undo_toggle();
                        ui.close_menu();
                    }
                    if ui.add_enabled(can_undo, egui::Button::new("Undo step by step         Ctrl+Alt+Z")).clicked() {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui.add_enabled(can_redo, egui::Button::new("Redo")).clicked() {
                        self.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(RichText::new(format!("History: step {} of {}", self.hist_at + 1, self.history.len().max(1))).color(TEXT_DIM).small());
                });
                ui.menu_button("ADD", |ui| {
                    if ui.button("Sampler channel").clicked() {
                        self.song.channels.push(Chan::default());
                        self.sel_chan = self.song.channels.len() - 1;
                        self.dirty = true;
                        ui.close_menu();
                    }
                    if ui.button("Pattern                F4").clicked() {
                        self.new_pattern();
                        ui.close_menu();
                    }
                });
                ui.menu_button("PATTERNS", |ui| {
                    if ui.button("Find first empty               F4").clicked() {
                        self.new_pattern();
                        ui.close_menu();
                    }
                    if ui.button("Clone                               Ctrl+Shift+C").clicked() {
                        self.clone_pattern();
                        ui.close_menu();
                    }
                    if ui.button("Rename and colour...      F2").clicked() {
                        self.start_rename_pattern();
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        self.delete_pattern();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Next                                 +").clicked() {
                        self.step_pattern(1);
                        ui.close_menu();
                    }
                    if ui.button("Previous                           -").clicked() {
                        self.step_pattern(-1);
                        ui.close_menu();
                    }
                });
                ui.menu_button("VIEW", |ui| {
                    ui.checkbox(&mut self.show_playlist, "Playlist            F5");
                    ui.checkbox(&mut self.show_rack, "Channel rack     F6");
                    ui.checkbox(&mut self.show_piano, "Piano roll          F7");
                    ui.checkbox(&mut self.show_mixer, "Mixer                 F9");
                    ui.checkbox(&mut self.show_browser, "Browser            Alt+F8");
                    ui.checkbox(&mut self.show_info, "Project info       F11");
                    ui.separator();
                    if ui.button("Close all windows     F12").clicked() {
                        self.close_all();
                        ui.close_menu();
                    }
                });
                ui.menu_button("OPTIONS", |ui| {
                    ui.checkbox(&mut self.typing_keys, "Typing keyboard to piano     Ctrl+T");
                    let mut m = self.handle.transport.metronome();
                    if ui.checkbox(&mut m, "Metronome                              Ctrl+M").changed() {
                        self.handle.transport.set_metronome(m);
                    }
                    if ui.button("Stop all sound                          Ctrl+H").clicked() {
                        self.handle.send(Cmd::Panic);
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(RichText::new(&self.audio).color(TEXT_DIM));
                });
                ui.menu_button("HELP", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.status).color(TEXT_DIM).small());
                });
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let tr = self.handle.transport.clone();
                let song_mode = tr.song_mode();
                if ui.selectable_label(!song_mode, RichText::new("PAT").strong()).on_hover_text("Pattern mode (L)").clicked() {
                    tr.set_song_mode(false);
                }
                if ui.selectable_label(song_mode, RichText::new("SONG").strong()).on_hover_text("Song mode (L)").clicked() {
                    tr.set_song_mode(true);
                }
                let playing = tr.playing();
                if ui.add_sized([44.0, 24.0], egui::Button::new(RichText::new(if playing { "||" } else { ">" }).strong().color(if playing { GREEN } else { TEXT }))).on_hover_text("Play / stop (Space)").clicked() {
                    if playing {
                        tr.set_playing(false);
                    } else {
                        tr.set_playing(true);
                    }
                }
                if ui.add_sized([34.0, 24.0], egui::Button::new(RichText::new("[]").strong())).on_hover_text("Stop").clicked() {
                    self.stop();
                }
                ui.separator();
                let live = tr.tempo();
                let mut tempo = if playing && song_mode && live > 0.0 { live } else { self.song.tempo };
                if ui.add(egui::DragValue::new(&mut tempo).range(10.0..=522.0).speed(0.2).fixed_decimals(3)).on_hover_text("Tempo").changed() {
                    self.song.tempo = tempo;
                    self.dirty = true;
                }
                ui.separator();
                // pattern picker
                let pat = tr.pattern();
                self.song.ensure_pattern(pat);
                egui::ComboBox::from_id_salt("pattern").width(150.0).selected_text(self.song.pattern_name(pat)).show_ui(ui, |ui| {
                    for i in 0..self.song.patterns.len() {
                        if ui.selectable_label(i == pat, self.song.pattern_name(i)).clicked() {
                            tr.set_pattern(i);
                        }
                    }
                });
                if ui.button("+").on_hover_text("New pattern (F4)").clicked() {
                    self.new_pattern();
                }
                let mut met = tr.metronome();
                if ui.toggle_value(&mut met, "Metro").on_hover_text("Metronome (Ctrl+M)").changed() {
                    tr.set_metronome(met);
                }
                ui.separator();
                // time
                let tick = tr.tick() as u32;
                let (bar, step) = (self.song.bar(), self.song.step());
                ui.label(RichText::new(format!("{:03}:{:02}:{:02}", tick / bar + 1, (tick % bar) / step + 1, tick % step)).monospace().size(18.0).color(GREEN));
                ui.separator();
                let mut master = tr.master();
                ui.label(RichText::new("Master").color(TEXT_DIM));
                if knob(ui, &mut master, 0.0, 1.25, 1.0, 22.0).changed() {
                    tr.set_master(master);
                }
                ui.separator();
                for (flag, label, tip) in [
                    (&mut self.show_playlist, "Playlist", "F5"),
                    (&mut self.show_piano, "Piano roll", "F7"),
                    (&mut self.show_rack, "Channel rack", "F6"),
                    (&mut self.show_mixer, "Mixer", "F9"),
                    (&mut self.show_browser, "Browser", "Alt+F8"),
                ] {
                    if ui.selectable_label(*flag, label).on_hover_text(tip).clicked() {
                        *flag = !*flag;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} voices", tr.voices())).color(TEXT_DIM).small());
                });
            });
            ui.add_space(2.0);
        });
    }
}

impl App {
    /// Project info, the pattern rename box and the about box.
    fn small_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.show_info;
        egui::Window::new("Project info").open(&mut open).collapsible(false).default_width(360.0).show(ctx, |ui| {
            let mut changed = false;
            egui::Grid::new("info").num_columns(2).show(ui, |ui| {
                ui.label("Title");
                changed |= ui.text_edit_singleline(&mut self.song.title).changed();
                ui.end_row();
                ui.label("Artist");
                changed |= ui.text_edit_singleline(&mut self.song.artist).changed();
                ui.end_row();
                ui.label("Genre");
                changed |= ui.text_edit_singleline(&mut self.song.genre).changed();
                ui.end_row();
            });
            ui.label("Comments");
            changed |= ui.add(egui::TextEdit::multiline(&mut self.song.comment).desired_rows(6).desired_width(f32::INFINITY)).changed();
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Time signature");
                let (mut n, mut d) = self.song.time_sig;
                changed |= ui.add(egui::DragValue::new(&mut n).range(1..=16)).changed();
                ui.label("/");
                egui::ComboBox::from_id_salt("beat").width(48.0).selected_text(d.to_string()).show_ui(ui, |ui| {
                    for v in [2u8, 4, 8, 16] {
                        changed |= ui.selectable_value(&mut d, v, v.to_string()).changed();
                    }
                });
                self.song.time_sig = (n, d);
            });
            if changed {
                self.dirty = true;
            }
        });
        self.show_info = open;

        if let Some((pi, mut name)) = self.rename_pattern.take() {
            let mut keep = true;
            let mut open = true;
            egui::Window::new("Pattern name and colour").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                let mut col = self.song.patterns.get(pi).and_then(|p| p.color).unwrap_or([72, 81, 86]);
                if ui.color_edit_button_srgb(&mut col).changed() {
                    if let Some(p) = self.song.patterns.get_mut(pi) {
                        p.color = Some(col);
                        self.dirty = true;
                    }
                }
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    if let Some(p) = self.song.patterns.get_mut(pi) {
                        p.name = name.trim().to_string();
                        self.dirty = true;
                    }
                    keep = false;
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    keep = false;
                }
            });
            if keep && open {
                self.rename_pattern = Some((pi, name));
            }
        }

        let mut open = self.show_about;
        egui::Window::new("About").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("Breakfast Cereal DAW");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("A free, open source music workstation for Linux and Windows.");
            ui.label(RichText::new(&self.audio).color(TEXT_DIM).small());
        });
        self.show_about = open;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.keys(ctx);
        // files dropped on the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            if p.extension().map(|e| e.eq_ignore_ascii_case("flp")).unwrap_or(false) {
                self.actions.push(Action::Open(p));
            } else {
                self.actions.push(Action::LoadSample(p, None));
            }
        }
        self.toolbar(ctx);
        if self.show_browser {
            egui::SidePanel::left("browser").default_width(230.0).frame(egui::Frame::new().fill(PANEL_DARK).inner_margin(6.0)).show(ctx, |ui| self.browser_ui(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |_| {});

        let frame = egui::Frame::window(&ctx.style()).fill(PANEL);
        let mut open = self.show_playlist;
        egui::Window::new("Playlist").open(&mut open).frame(frame).default_pos([260.0, 90.0]).default_size([980.0, 420.0]).show(ctx, |ui| self.playlist_ui(ui));
        self.show_playlist = open;
        let mut open = self.show_rack;
        egui::Window::new("Channel rack").open(&mut open).frame(frame).default_pos([280.0, 540.0]).default_size([640.0, 260.0]).show(ctx, |ui| self.rack_ui(ui));
        self.show_rack = open;
        let mut open = self.show_piano;
        egui::Window::new("Piano roll").open(&mut open).frame(frame).default_pos([340.0, 140.0]).default_size([900.0, 460.0]).show(ctx, |ui| self.piano_ui(ui));
        self.show_piano = open;
        let mut open = self.show_mixer;
        egui::Window::new("Mixer").open(&mut open).frame(frame).default_pos([300.0, 420.0]).default_size([980.0, 360.0]).show(ctx, |ui| self.mixer_ui(ui));
        self.show_mixer = open;

        self.small_windows(ctx);
        self.run_actions();
        if ctx.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ctx);
        }
        if self.dirty {
            self.dirty = false;
            if std::mem::take(&mut self.fresh) {
                self.history = vec![self.song.clone()];
                self.hist_at = 0;
                self.hist_pending = false;
            } else {
                self.unsaved = true;
                self.hist_pending = true;
            }
            self.handle.publish(self.song.clone());
        }
        // an edit becomes one undo step once the mouse is up and no text box is being typed in
        if self.hist_pending && !ctx.input(|i| i.pointer.any_down()) && !ctx.wants_keyboard_input() {
            self.hist_pending = false;
            self.history.truncate(self.hist_at + 1);
            self.history.push(self.song.clone());
            if self.history.len() > HISTORY_MAX {
                self.history.remove(0);
            }
            self.hist_at = self.history.len() - 1;
        }
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{name}{} - Breakfast Cereal DAW", if self.unsaved { " *" } else { "" })));
        if self.handle.transport.playing() || self.handle.transport.voices() > 0 {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

fn main() -> eframe::Result {
    let open = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1500.0, 900.0]).with_maximized(true).with_min_inner_size(Vec2::new(800.0, 500.0)).with_title("Breakfast Cereal DAW"),
        ..Default::default()
    };
    eframe::run_native(
        "breakfast-cereal",
        options,
        Box::new(move |cc| {
            let mut v = egui::Visuals::dark();
            v.panel_fill = PANEL;
            v.window_fill = PANEL;
            v.override_text_color = Some(TEXT);
            v.widgets.noninteractive.bg_stroke.color = LINE;
            v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
            cc.egui_ctx.set_visuals(v);
            Ok(Box::new(App::new(open)))
        }),
    )
}
