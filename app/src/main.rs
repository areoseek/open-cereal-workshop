//! Breakfast Cereal DAW: the application window. Toolbar and transport on top, browser on the left, and the
//! channel rack, piano roll, playlist and mixer as windows on the workspace.

mod browser;
mod chanset;
mod events;
mod fxwin;
mod midi;
mod record;
mod setup;
mod unpack;

/// Windows that take their own key commands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Win {
    None,
    #[allow(dead_code)]
    Rack,
    Piano,
    Playlist,
}
mod mixer;
mod pianoroll;
mod prtools;
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
    /// Load a plugin preset onto the matching unit.
    LoadPreset(PathBuf, PresetTo),
    /// Put a score or MIDI file's notes on a channel's piano roll (the selected one when not said).
    LoadScore(PathBuf, Option<usize>),
}

/// Where a preset should land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PresetTo {
    /// The open effect or the selected channel when it is the same unit, a new slot or channel otherwise.
    Auto,
    Slot(usize, u16),
    Chan(usize),
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
    /// The window key commands go to.
    pub(crate) focus: Win,
    pub(crate) setup: setup::State,
    pub(crate) settings: setup::Settings,
    sel_chan: usize,
    /// MIDI keyboards and pads.
    midi: midi::MidiIn,
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
    render: Option<RenderOpts>,
    rec_held: Vec<(u16, u8, f64, f32)>,
    /// Record sound from the input device too while recording runs.
    rec_audio: bool,
    audio_rec: Option<record::AudioRec>,
    save_preset: Option<(String, bool)>,
    show_chanset: bool,
    show_events: bool,
    events: events::State,
    /// The effect slot whose editor is open: insert, slot.
    fx_open: Option<(usize, u16)>,
    /// Preset files per plugin, found in the user's own content.
    preset_lists: std::collections::HashMap<String, std::rc::Rc<Vec<PathBuf>>>,
    /// The preset last loaded per plugin, for the previous / next buttons.
    preset_name: std::collections::HashMap<String, PathBuf>,
    chanset: chanset::State,
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
        let env_roots = lib.factory_roots.clone();
        let settings = setup::Settings::load();
        for d in &lib.search_dirs {
            browser.add_root(&d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), d.clone());
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) {
            browser.add_root("Music", home.join("Music"));
            browser.add_root("Documents", home.join("Documents"));
        }
        let midi = midi::MidiIn::start(handle.sender());
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
            focus: Win::None,
            setup: Default::default(),
            settings,
            sel_chan: 0,
            midi,
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
            render: None,
            rec_held: Vec::new(),
            rec_audio: false,
            audio_rec: None,
            save_preset: None,
            show_chanset: false,
            show_events: false,
            events: events::State::default(),
            fx_open: None,
            preset_lists: Default::default(),
            preset_name: Default::default(),
            chanset: Default::default(),
            rename_pattern: None,
        };
        let mut roots = env_roots;
        roots.extend(app.settings.content.iter().filter(|c| c.is_dir()).cloned());
        for r in &roots {
            app.add_content_root(r);
        }
        // first run (and later runs until the user settles it): offer to read sounds from their own copy
        app.setup.open = roots.is_empty() && !app.settings.never_ask && std::env::var_os("CEREAL_NO_SETUP").is_none();
        if let Some(p) = open {
            app.open(&p);
        }
        // presets to load at start, for screenshots and tests
        if let Ok(v) = std::env::var("CEREAL_PRESET") {
            for p in std::env::split_paths(&v) {
                app.actions.push(Action::LoadPreset(p, PresetTo::Auto));
            }
        }
        // windows to show at start, for screenshots and tests: CEREAL_SHOW=mixer,piano,channel
        if let Ok(show) = std::env::var("CEREAL_SHOW") {
            for w in show.split(',') {
                match w.trim() {
                    "mixer" => app.show_mixer = true,
                    "piano" => app.show_piano = true,
                    "channel" => app.show_chanset = true,
                    "events" => app.show_events = true,
                    x if x.starts_with("fx=") => {
                        let mut it = x[3..].split(':').filter_map(|v| v.parse::<usize>().ok());
                        if let (Some(i), Some(sl)) = (it.next(), it.next()) {
                            app.fx_open = Some((i, sl as u16));
                            app.sel_insert = i;
                        }
                    }
                    "noplaylist" => app.show_playlist = false,
                    "norack" => app.show_rack = false,
                    w => {
                        if let Some(n) = w.strip_prefix("chan=").and_then(|n| n.parse().ok()) {
                            app.sel_chan = n;
                        }
                    }
                }
            }
        }
        app
    }

    fn stop(&mut self) {
        self.handle.transport.set_playing(false);
        self.record_notes(true);
        self.handle.transport.seek(0);
    }

    /// Start capturing sound when recording begins and turn it into an audio clip when it ends.
    fn record_audio(&mut self) {
        let tr = self.handle.transport.clone();
        let want = self.rec_audio && tr.recording() && tr.playing();
        if want && self.audio_rec.is_none() {
            let at = if tr.song_mode() { tr.tick().max(0.0) as u32 } else { 0 };
            match record::AudioRec::start(at) {
                Ok(r) => self.audio_rec = Some(r),
                Err(e) => {
                    self.status = format!("Audio recording failed: {e}");
                    self.rec_audio = false;
                }
            }
        } else if !want {
            if let Some(r) = self.audio_rec.take() {
                let at = r.at;
                match r.finish(&record::folder(self.path.as_deref())) {
                    Ok(path) => {
                        self.add_audio_clip(&path, at, None);
                        self.status = format!("Recorded {}", path.display());
                    }
                    Err(e) => self.status = e,
                }
            }
        }
    }

    /// Write hand-played notes into the current pattern while recording runs. `flush` ends keys still held.
    fn record_notes(&mut self, flush: bool) {
        let tr = self.handle.transport.clone();
        let mut evs = tr.take_live();
        if flush || !tr.playing() || !tr.recording() {
            let now = tr.tick();
            evs.extend(self.rec_held.iter().map(|&(chan, key, _, _)| cereal_engine::Live { tick: now, on: false, chan, key, vel: 0.0 }));
        }
        if evs.is_empty() {
            return;
        }
        let pat = tr.pattern();
        self.song.ensure_pattern(pat);
        let song_mode = tr.song_mode();
        let plen = self.song.pattern_length(pat) as f64;
        // In song mode a note lands inside the clip of this pattern the play position is over.
        let clip = |song: &Song, tick: f64| -> f64 {
            if !song_mode {
                return tick;
            }
            song.clips.iter().find(|c| c.src == cereal_engine::song::ClipSrc::Pattern(pat) && tick >= c.pos as f64 && tick < (c.pos + c.len) as f64).map(|c| tick - c.pos as f64 + c.offset as f64).unwrap_or(tick)
        };
        for e in evs {
            if e.on {
                self.rec_held.retain(|h| (h.0, h.1) != (e.chan, e.key));
                self.rec_held.push((e.chan, e.key, e.tick, e.vel));
            } else if let Some(i) = self.rec_held.iter().position(|h| (h.0, h.1) == (e.chan, e.key)) {
                let (chan, key, start, vel) = self.rec_held.remove(i);
                let pos = clip(&self.song, start).round().max(0.0);
                let mut len = e.tick - start;
                if len < 0.0 && !song_mode {
                    // the loop came round while the key was down
                    len = (plen - start).max(1.0);
                }
                let note = cereal_engine::song::Note { pos: pos as u32, len: len.round().max(1.0) as u32, chan, key, vel, pan: 0.0, fine_cents: 0.0, muted: false, ..Default::default() };
                self.song.patterns[pat].notes.push(note);
                self.song.patterns[pat].sort();
                self.dirty = true;
            }
        }
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

    /// Ctrl+R opens the rendering window; its Start button asks where to save and writes the file.
    fn export(&mut self) {
        if self.render.is_none() {
            self.render = Some(RenderOpts { flac: false, bits: 16, rate: 44_100, tail: 6.0, song: self.handle.transport.song_mode() });
        }
    }

    fn render_to_file(&mut self, o: &RenderOpts) {
        let (ext, label) = if o.flac { ("flac", "FLAC audio") } else { ("wav", "WAV audio") };
        let default = self.path.as_ref().map(|p| p.with_extension(ext)).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| format!("untitled.{ext}"));
        let Some(path) = rfd::FileDialog::new().add_filter(label, &[ext]).set_file_name(default).save_file() else { return };
        let buf = cereal_engine::render_song(&self.song, o.rate as f32, o.song, self.handle.transport.pattern(), o.tail);
        let r = std::fs::File::create(&path).and_then(|f| {
            let w = std::io::BufWriter::new(f);
            if o.flac {
                cereal_engine::flac::write(w, &buf, o.rate, 2, o.bits.min(24) as u32)
            } else {
                wav::write(w, &buf, o.rate, 2, o.bits)
            }
        });
        self.status = match r {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    fn render_window(&mut self, ctx: &egui::Context) {
        let Some(mut o) = self.render.take() else { return };
        let mut open = true;
        let mut go = false;
        egui::Window::new("Rendering").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("render").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label("Mode");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut o.song, true, "Full song");
                    ui.selectable_value(&mut o.song, false, "Pattern");
                });
                ui.end_row();
                ui.label("Format");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut o.flac, false, "WAV");
                    ui.selectable_value(&mut o.flac, true, "FLAC");
                });
                ui.end_row();
                ui.label("Depth");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut o.bits, 16, "16 bit");
                    ui.selectable_value(&mut o.bits, 24, "24 bit");
                    if !o.flac {
                        ui.selectable_value(&mut o.bits, 32, "32 bit float");
                    } else if o.bits > 24 {
                        o.bits = 24;
                    }
                });
                ui.end_row();
                ui.label("Sample rate");
                ui.horizontal(|ui| {
                    for r in [44_100u32, 48_000, 88_200, 96_000] {
                        ui.selectable_value(&mut o.rate, r, format!("{r}"));
                    }
                });
                ui.end_row();
                ui.label("Tail");
                ui.add(egui::Slider::new(&mut o.tail, 0.0..=20.0).suffix(" s"));
                ui.end_row();
            });
            ui.add_space(6.0);
            if ui.button("Start").clicked() {
                go = true;
            }
        });
        if go {
            self.render_to_file(&o);
        } else if open {
            self.render = Some(o);
        }
    }

    /// Replace one channel's notes in the current pattern with a score file's (or a MIDI file's, all parts merged).
    fn load_score(&mut self, path: &Path, to: Option<usize>) {
        let ci = to.unwrap_or(self.sel_chan);
        if ci >= self.song.channels.len() {
            return;
        }
        let midi = path.extension().is_some_and(|e| !e.eq_ignore_ascii_case("fsc"));
        let got = if midi {
            std::fs::read(path).map_err(|e| e.to_string()).and_then(|d| cereal_engine::midifile::import(&d, self.song.ppq)).map(|m| m.parts.into_iter().flat_map(|p| p.notes).map(|n| cereal_engine::song::Note { chan: ci as u16, ..n }).collect::<Vec<_>>())
        } else {
            cereal_engine::score::read(path, self.song.ppq, ci as u16)
        };
        match got {
            Ok(notes) if !notes.is_empty() => {
                let pat = self.handle.transport.pattern();
                let Some(p) = self.song.patterns.get_mut(pat) else { return };
                p.notes.retain(|n| n.chan != ci as u16);
                let count = notes.len();
                p.notes.extend(notes);
                p.sort();
                self.sel_chan = ci;
                self.piano.sel.clear();
                self.dirty = true;
                self.status = format!("Loaded {count} notes from {}", path.display());
            }
            Ok(_) => self.status = "That file holds no notes".into(),
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn open_score_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Score or MIDI file", &["fsc", "mid", "midi"]).pick_file() {
            self.load_score(&p, None);
        }
    }

    pub(crate) fn save_score_dialog(&mut self) {
        let ci = self.sel_chan as u16;
        let Some(p) = self.song.patterns.get(self.handle.transport.pattern()) else { return };
        let notes: Vec<&cereal_engine::song::Note> = p.notes.iter().filter(|n| n.chan == ci).collect();
        if notes.is_empty() {
            self.status = "This piano roll has no notes to save".into();
            return;
        }
        let bytes = cereal_engine::score::write(&notes, self.song.ppq);
        let Some(path) = rfd::FileDialog::new().add_filter("Score file", &["fsc"]).set_file_name("My score.fsc").save_file() else { return };
        self.status = match std::fs::write(&path, bytes) {
            Ok(()) => format!("Saved score {}", path.display()),
            Err(e) => format!("Score not saved: {e}"),
        };
    }

    /// Put a sound on the playlist as an audio clip, on its own new channel.
    pub(crate) fn add_audio_clip(&mut self, path: &Path, pos: u32, track: Option<u16>) {
        let smp = match self.lib.load_path(path) {
            Ok(s) => s,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let len = ((smp.seconds() * self.song.tempo / 60.0 * self.song.ppq as f32).ceil() as u32).max(1);
        let mut c = Chan::default();
        c.name = smp.name.clone();
        c.kind = ChannelKind::AudioClip;
        c.sample_path = path.to_string_lossy().to_string();
        c.set_sample(Some(smp));
        self.song.channels.push(c);
        let ci = self.song.channels.len() - 1;
        let free = |song: &Song, t: u16| !song.clips.iter().any(|c| c.track == t && c.pos < pos + len && pos < c.pos + c.len);
        let ntracks = self.song.tracks.len().max(1) as u16;
        let track = track.filter(|&t| free(&self.song, t)).or_else(|| (track.unwrap_or(0)..ntracks).chain(0..ntracks).find(|&t| free(&self.song, t))).unwrap_or(0);
        self.song.clips.push(cereal_engine::song::Clip { pos, len, track, src: cereal_engine::song::ClipSrc::Audio(ci), offset: 0, muted: false });
        self.song.clips.sort_by_key(|c| c.pos);
        self.dirty = true;
    }

    fn export_midi(&mut self) {
        let default = self.path.as_ref().map(|p| p.with_extension("mid")).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "untitled.mid".into());
        let Some(path) = rfd::FileDialog::new().add_filter("MIDI file", &["mid"]).set_file_name(default).save_file() else { return };
        let bytes = cereal_engine::midifile::export(&self.song, self.handle.transport.song_mode(), self.handle.transport.pattern());
        self.status = match std::fs::write(&path, bytes) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    fn import_midi_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("MIDI file", &["mid", "midi", "rmi", "kar"]).pick_file() {
            self.import_midi(&p);
        }
    }

    /// Each part of the file lands on a new channel, all in one free pattern.
    fn import_midi(&mut self, path: &Path) {
        let got = std::fs::read(path).map_err(|e| e.to_string()).and_then(|d| cereal_engine::midifile::import(&d, self.song.ppq));
        let m = match got {
            Ok(m) if !m.parts.is_empty() => m,
            Ok(_) => {
                self.status = "That MIDI file holds no notes".into();
                return;
            }
            Err(e) => {
                self.status = format!("Import failed: {e}");
                return;
            }
        };
        let pat = self.song.next_empty_pattern();
        let count = m.parts.len();
        for part in m.parts {
            let mut c = Chan::instrument(cereal_engine::synth::Kind::Osc3);
            c.name = part.name;
            self.song.channels.push(c);
            let ci = (self.song.channels.len() - 1) as u16;
            self.song.patterns[pat].notes.extend(part.notes.into_iter().map(|n| cereal_engine::song::Note { chan: ci, ..n }));
        }
        self.song.patterns[pat].sort();
        if self.song.patterns[pat].name.is_empty() {
            self.song.patterns[pat].name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        }
        if self.song.clips.is_empty() {
            if let Some(t) = m.tempo {
                self.song.tempo = t.clamp(10.0, 522.0);
            }
            if let Some(ts) = m.time_sig {
                self.song.time_sig = ts;
            }
        }
        self.handle.transport.set_pattern(pat);
        self.sel_chan = self.song.channels.len() - 1;
        self.dirty = true;
        self.status = format!("Imported {count} part{} from {}", if count == 1 { "" } else { "s" }, path.display());
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
                c.set_sample(Some(s));
                self.sel_chan = ci;
                self.dirty = true;
            }
            Err(e) => self.status = e,
        }
    }

    /// Load a plugin preset file onto one of our units.
    fn load_preset(&mut self, path: &Path, to: PresetTo) {
        use cereal_engine::preset::{self, Unit};
        let pre = match preset::read(path) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match pre.unit() {
            None => self.status = format!("\"{}\" is a preset for {}, which has no built-in unit yet", name, pre.ident),
            Some(Unit::Fx(mut f)) => {
                if f.ir.is_none() {
                    if let Some(stored) = cereal_engine::fx::sample_named_in(cereal_engine::fx::def(f.kind).ident, &pre.state) {
                        f.ir = self.lib.load_stored(&stored, path.parent()).map(|smp| cereal_engine::fx::Ir { smp, path: stored });
                    }
                }
                let same = |song: &Song, ins: usize, slot: u16| song.inserts.get(ins).and_then(|i| i.slots.iter().find(|s| s.index == slot)).and_then(|s| s.fx.as_ref()).is_some_and(|x| x.kind == f.kind);
                let at = match to {
                    PresetTo::Slot(i, s) => Some((i, s)),
                    _ => self.fx_open.filter(|&(i, s)| same(&self.song, i, s)),
                };
                let ins = at.map(|a| a.0).unwrap_or(self.sel_insert).min(self.song.inserts.len().saturating_sub(1));
                let Some(insert) = self.song.inserts.get_mut(ins) else { return };
                let slot = match at {
                    Some((_, s)) => s,
                    None => match (0..10u16).find(|k| insert.slots.iter().all(|s| s.index != *k)) {
                        Some(k) => k,
                        None => {
                            self.status = "No free effect slot on this mixer track".into();
                            return;
                        }
                    },
                };
                let kind = f.kind;
                match insert.slots.iter_mut().find(|s| s.index == slot) {
                    Some(s) if s.fx.as_ref().is_some_and(|x| x.kind == kind) => s.fx = Some(f),
                    _ => {
                        insert.slots.retain(|s| s.index != slot);
                        let mut s = cereal_engine::song::Slot::with_fx(slot, kind);
                        s.fx = Some(f);
                        insert.slots.push(s);
                        insert.slots.sort_by_key(|s| s.index);
                    }
                }
                self.fx_open = Some((ins, slot));
                self.sel_insert = ins;
                self.show_mixer = true;
                self.preset_name.insert(cereal_engine::fx::def(kind).ident.to_string(), path.to_path_buf());
                self.status = format!("Preset: {name}");
                self.dirty = true;
            }
            Some(Unit::Gen(g)) => {
                let target = match to {
                    PresetTo::Chan(c) => Some(c),
                    _ => Some(self.sel_chan),
                }
                .filter(|&c| self.song.channels.get(c).and_then(|c| c.gen.as_ref()).is_some_and(|x| x.kind == g.kind));
                let kind = g.kind;
                let ci = match target {
                    Some(c) => c,
                    None => {
                        let mut c = Chan::instrument(kind);
                        c.name = name.clone();
                        self.song.channels.push(c);
                        self.song.channels.len() - 1
                    }
                };
                self.song.channels[ci].gen = Some(g);
                self.sel_chan = ci;
                self.preset_name.insert(cereal_engine::synth::def(kind).ident.to_string(), path.to_path_buf());
                self.status = format!("Preset: {name}");
                self.dirty = true;
            }
        }
    }

    /// The preset files for the plugin called `ident` in the user's own content, sorted by name.
    pub(crate) fn presets_for(&mut self, ident: &str, generator: bool) -> std::rc::Rc<Vec<PathBuf>> {
        if let Some(l) = self.preset_lists.get(ident) {
            return l.clone();
        }
        let mut out = Vec::new();
        for root in &self.lib.factory_roots {
            let base = root.join("Data/Patches/Plugin presets").join(if generator { "Generators" } else { "Effects" });
            // folder names differ from the plugin names in case here and there
            let dirs = std::fs::read_dir(&base).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(ident)));
            let mut stack: Vec<PathBuf> = dirs.collect();
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) {
                        out.push(p);
                    }
                }
            }
        }
        out.sort_by_key(|p| p.to_string_lossy().to_lowercase());
        let l = std::rc::Rc::new(out);
        self.preset_lists.insert(ident.to_string(), l.clone());
        l
    }

    /// The preset picker shown in an effect or instrument window. Returns the file to load, if one was chosen.
    pub(crate) fn preset_bar(&mut self, ui: &mut egui::Ui, ident: &str, generator: bool) -> Option<PathBuf> {
        let list = self.presets_for(ident, generator);
        let current = self.preset_name.get(ident).cloned();
        let at = current.as_ref().and_then(|c| list.iter().position(|p| p == c));
        let label = at.and_then(|i| list[i].file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Presets".into());
        let mut pick = None;
        ui.horizontal(|ui| {
            if ui.small_button("Save").on_hover_text("Save these settings as a preset file").clicked() {
                self.save_preset = Some((ident.to_string(), generator));
            }
            if list.is_empty() {
                ui.label(egui::RichText::new("Presets: none found (Options > Sounds and presets from your own copy)").small().color(widgets::TEXT_DIM));
                return;
            }
            let n = list.len();
            if ui.small_button("<").on_hover_text("Previous preset").clicked() {
                pick = Some(list[at.map(|i| (i + n - 1) % n).unwrap_or(n - 1)].clone());
            }
            if ui.small_button(">").on_hover_text("Next preset").clicked() {
                pick = Some(list[at.map(|i| (i + 1) % n).unwrap_or(0)].clone());
            }
            ui.menu_button(format!("{label}  v"), |ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (i, p) in list.iter().enumerate() {
                        let name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.selectable_label(Some(i) == at, name).clicked() {
                            pick = Some(p.clone());
                            ui.close_menu();
                        }
                    }
                });
            });
        });
        pick
    }

    /// Write the settings of the open effect or the selected instrument to a preset file.
    fn save_preset_file(&mut self, ident: &str, generator: bool) {
        let state = if generator {
            self.song.channels.get(self.sel_chan).and_then(|c| c.gen.as_ref()).filter(|g| cereal_engine::synth::def(g.kind).ident == ident).map(|g| g.encode(None))
        } else {
            self.fx_open.and_then(|(i, s)| self.song.inserts.get(i)?.slots.iter().find(|x| x.index == s)?.fx.as_ref()).filter(|f| cereal_engine::fx::def(f.kind).ident == ident).map(|f| f.encode(None))
        };
        let Some(state) = state else { return };
        let mut d = rfd::FileDialog::new().add_filter("Preset file", &["fst"]).set_file_name("My preset.fst");
        if let Some(dir) = self.presets_for(ident, generator).first().and_then(|p| p.parent().map(|p| p.to_path_buf())) {
            d = d.set_directory(dir);
        }
        let Some(path) = d.save_file() else { return };
        self.status = match std::fs::write(&path, cereal_engine::preset::write(ident, &state, generator)) {
            Ok(()) => {
                self.preset_lists.remove(ident);
                self.preset_name.insert(ident.to_string(), path.clone());
                format!("Saved preset {}", path.display())
            }
            Err(e) => format!("Preset not saved: {e}"),
        };
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
                Action::LoadPreset(p, to) => self.load_preset(&p, to),
                Action::LoadScore(p, to) => self.load_score(&p, to),
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
        self.show_chanset = false;
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
            // the copy, cut and paste keys arrive as their own events, never as key presses
            match &e {
                egui::Event::Copy | egui::Event::Cut if !typing_text => {
                    let cut = matches!(e, egui::Event::Cut);
                    if !cut && ctx.input(|i| i.modifiers.shift) {
                        self.clone_pattern();
                    } else if self.focus == Win::Piano && self.show_piano {
                        self.pr_copy(ctx, cut);
                    } else if self.focus == Win::Playlist && self.show_playlist {
                        self.pl_copy(ctx, cut);
                    }
                    continue;
                }
                egui::Event::Paste(_) if !typing_text => {
                    if self.focus == Win::Piano && self.show_piano {
                        self.pr_paste();
                    } else if self.focus == Win::Playlist && self.show_playlist {
                        self.pl_paste();
                    }
                    continue;
                }
                _ => {}
            }
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
            if self.focus == Win::Piano && self.show_piano && self.piano_key(key, modifiers, repeat) {
                continue;
            }
            if self.focus == Win::Playlist && self.show_playlist && self.playlist_key(key, modifiers, repeat) {
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
                    Key::M if modifiers.shift => self.export_midi(),
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
                Key::R if !self.typing_keys && !repeat => {
                    let tr = &self.handle.transport;
                    tr.set_recording(!tr.recording());
                }
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
                    if ui.button("Export audio...   Ctrl+R").clicked() {
                        ui.close_menu();
                        self.export();
                    }
                    if ui.button("Export MIDI file...     Ctrl+Shift+M").clicked() {
                        ui.close_menu();
                        self.export_midi();
                    }
                    if ui.button("Import MIDI file...").clicked() {
                        ui.close_menu();
                        self.import_midi_dialog();
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
                    ui.checkbox(&mut self.show_events, "Event editor");
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
                    if ui.button("Sounds and presets from your own copy...").clicked() {
                        self.setup = Default::default();
                        self.setup.open = true;
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
                let rec = tr.recording();
                if ui.add_sized([34.0, 24.0], egui::Button::new(RichText::new("REC").strong().color(if rec { egui::Color32::from_rgb(235, 70, 60) } else { TEXT_DIM }))).on_hover_text("Record what you play into the current pattern (R)").clicked() {
                    tr.set_recording(!rec);
                }
                if ui.toggle_value(&mut self.rec_audio, "MIC").on_hover_text("Also record sound from the input device, as an audio clip on the playlist").changed() && !self.rec_audio {
                    self.record_audio();
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

        self.render_window(ctx);
        if let Some((ident, generator)) = self.save_preset.take() {
            self.save_preset_file(&ident, generator);
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

/// Choices in the rendering window.
struct RenderOpts {
    flac: bool,
    bits: u16,
    rate: u32,
    tail: f32,
    song: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if !self.setup.open {
            self.keys(ctx);
        }
        self.midi.chan.store(self.sel_chan, std::sync::atomic::Ordering::Relaxed);
        self.setup_ui(ctx);
        // files dropped on the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            if p.extension().map(|e| e.eq_ignore_ascii_case("flp")).unwrap_or(false) {
                self.actions.push(Action::Open(p));
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fsc")) {
                self.actions.push(Action::LoadScore(p, None));
            } else if p.extension().map(|e| ["mid", "midi", "rmi", "kar"].iter().any(|x| e.eq_ignore_ascii_case(x))).unwrap_or(false) {
                self.import_midi(&p);
            } else {
                self.actions.push(Action::LoadSample(p, None));
            }
        }
        self.record_notes(false);
        self.record_audio();
        self.toolbar(ctx);
        if self.show_browser {
            egui::SidePanel::left("browser").default_width(230.0).frame(egui::Frame::new().fill(PANEL_DARK).inner_margin(6.0)).show(ctx, |ui| self.browser_ui(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |_| {});

        let frame = egui::Frame::window(&ctx.style()).fill(PANEL);
        // where windows first appear: playlist on top, rack and mixer side by side under it, channel settings
        // down the right edge, so nothing starts out covered
        let screen = ctx.screen_rect();
        let left = if self.show_browser { 246.0 } else { 8.0 };
        let top = 92.0;
        let right_col = 440.0;
        let wide = (screen.width() - left - right_col - 16.0).max(480.0);
        let upper = ((screen.height() - top) * 0.46).max(220.0);
        let low_y = top + upper + 48.0;
        let low_h = (screen.height() - low_y - 56.0).max(160.0);
        let rack_w = 640.0f32.min(wide * 0.5);
        let mut open = self.show_playlist;
        egui::Window::new("Playlist").open(&mut open).frame(frame).default_pos([left, top]).default_size([wide, upper]).show(ctx, |ui| self.playlist_ui(ui));
        self.show_playlist = open;
        let mut open = self.show_rack;
        egui::Window::new("Channel rack").open(&mut open).frame(frame).default_pos([left, low_y]).default_size([rack_w, low_h]).show(ctx, |ui| self.rack_ui(ui));
        self.show_rack = open;
        let mut open = self.show_piano;
        egui::Window::new("Piano roll").open(&mut open).frame(frame).default_pos([left + 24.0, top + 24.0]).default_size([wide - 24.0, upper + 60.0]).show(ctx, |ui| self.piano_ui(ui));
        self.show_piano = open;
        let mut open = self.show_mixer;
        egui::Window::new("Mixer").open(&mut open).frame(frame).default_pos([left + rack_w + 28.0, low_y]).default_size([(screen.width() - left - rack_w - 44.0).max(400.0), low_h]).show(ctx, |ui| self.mixer_ui(ui));
        self.show_mixer = open;
        let mut open = self.show_chanset;
        let title = self.song.channels.get(self.sel_chan).map(|c| c.name.clone()).unwrap_or_default();
        egui::Window::new(format!("Channel settings - {title}")).id(egui::Id::new("chanset")).open(&mut open).frame(frame).default_pos([screen.width() - right_col, top]).default_width(420.0).resizable(false).show(ctx, |ui| self.chanset_ui(ui));
        self.show_chanset = open;
        let mut open = self.show_events;
        egui::Window::new("Event editor").open(&mut open).frame(frame).default_pos([left + 60.0, top + 80.0]).default_size([760.0, 260.0]).show(ctx, |ui| self.events_ui(ui));
        self.show_events = open;
        self.fx_window(ctx, frame);

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
