//! Breakfast Cereal DAW: the application window. Toolbar and transport on top, browser on the left, and the
//! channel rack, piano roll, playlist and mixer as windows on the workspace.

mod browser;
mod chanset;
mod curve;
mod events;
mod fxwin;
mod hostui;
mod midi;
mod record;
mod settings;
mod setup;
mod unpack;
mod waveed;

/// Windows that take their own key commands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Win {
    None,
    Rack,
    Piano,
    Playlist,
    Mixer,
    Browser,
    Events,
}
mod keys;
mod testkeys;
pub(crate) use keys::DIGITS;
mod mixer;
mod pianoroll;
mod prtools;
mod playlist;
mod rack;
mod widgets;

use crate::widgets::Hint as _;
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

/// A state file to pick in a file window: a whole channel or a whole mixer track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum StateAsk {
    OpenChan(usize),
    SaveChan(usize),
    OpenTrack(usize),
    SaveTrack(usize),
}

/// How long sound takes from the engine to the speakers, in millionths of a second, as the output device reports it.
static OUT_LAG_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The rate the engine runs at.
static OUT_RATE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(44_100);

fn out_lag(info: &cpal::OutputCallbackInfo) {
    let t = info.timestamp();
    if let Some(d) = t.playback.duration_since(&t.callback) {
        OUT_LAG_US.store(d.as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

/// How much of each block's time the sound engine used, in thousandths, smoothed.
static CPU_LOAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn cpu_load(took: std::time::Duration, frames: usize, rate: f32) {
    use std::sync::atomic::Ordering::Relaxed;
    let now = took.as_secs_f32() / (frames.max(1) as f32 / rate.max(1.0)) * 1000.0;
    let was = CPU_LOAD.load(Relaxed) as f32;
    // quick to rise, slow to fall, like a meter
    let next = if now > was { now } else { was + (now - was) * 0.05 };
    CPU_LOAD.store(next.clamp(0.0, 9999.0) as u32, Relaxed);
}

/// Tests run on a sound device that takes blocks as fast as they come; with CEREAL_PACE set the blocks are
/// held back to the speed of the clock so that time in the song passes as it would be heard.
fn pace(frames: usize, rate: f32) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static FROM: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    static DONE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if !*ON.get_or_init(|| std::env::var_os("CEREAL_PACE").is_some()) {
        return;
    }
    let from = *FROM.get_or_init(std::time::Instant::now);
    let done = DONE.fetch_add(frames as u64, std::sync::atomic::Ordering::Relaxed);
    let due = std::time::Duration::from_secs_f64(done as f64 / rate.max(1.0) as f64);
    if let Some(wait) = due.checked_sub(from.elapsed()) {
        std::thread::sleep(wait.min(std::time::Duration::from_millis(200)));
    }
}

fn start_audio(prefs: &setup::Settings, make: impl FnOnce(f32) -> Player) -> Result<(cpal::Stream, String), String> {
    let host = cpal::default_host();
    let named = if prefs.audio_out.is_empty() { None } else { host.output_devices().ok().and_then(|mut l| l.find(|d| d.name().map(|n| n == prefs.audio_out).unwrap_or(false))) };
    let device = named.or_else(|| host.default_output_device()).ok_or("no audio output device")?;
    let mut config = device.default_output_config().map_err(|e| e.to_string())?;
    if prefs.rate != 0 && prefs.rate != config.sample_rate().0 {
        // the layout nearest the default one that can run at the asked rate
        let want = cpal::SampleRate(prefs.rate);
        let fits = device.supported_output_configs().ok().and_then(|l| {
            l.filter(|c| c.min_sample_rate() <= want && want <= c.max_sample_rate() && matches!(c.sample_format(), cpal::SampleFormat::F32 | cpal::SampleFormat::I16))
                .min_by_key(|c| (c.channels() != config.channels(), c.sample_format() != config.sample_format()))
        });
        if let Some(c) = fits {
            config = c.with_sample_rate(want);
        }
    }
    let channels = config.channels() as usize;
    let rate = config.sample_rate().0 as f32;
    let format = config.sample_format();
    let mut config: cpal::StreamConfig = config.into();
    if prefs.buffer != 0 {
        config.buffer_size = cpal::BufferSize::Fixed(prefs.buffer);
    }
    OUT_RATE.store(rate as u32, std::sync::atomic::Ordering::Relaxed);
    let name = format!("{} ({} Hz, {} ch)", device.name().unwrap_or_default(), rate as u32, channels);
    let mut player = make(rate);
    let err = |e| eprintln!("audio stream error: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(&config, move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                out_lag(info);
                pace(out.len() / channels.max(1), rate);
                let t0 = std::time::Instant::now();
                player.render(out, channels);
                cpu_load(t0.elapsed(), out.len() / channels.max(1), rate)
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut scratch = vec![0.0f32; 1 << 16];
            device.build_output_stream(
                &config,
                move |out: &mut [i16], info: &cpal::OutputCallbackInfo| {
                    out_lag(info);
                    pace(out.len() / channels.max(1), rate);
                    let n = out.len().min(scratch.len());
                    let t0 = std::time::Instant::now();
                    player.render(&mut scratch[..n], channels);
                    cpu_load(t0.elapsed(), n / channels.max(1), rate);
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
    Ok((stream, name))
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
    /// Snap hand-played notes to the piano roll's grid as they are recorded.
    rec_quantise: bool,
    /// The window key commands go to.
    pub(crate) focus: Win,
    /// A window to bring in front of the others on the next frame.
    raise: Option<Win>,
    /// The playlist fills the whole work area.
    pl_max: bool,
    /// The window for picking an instrument to add (F8).
    show_picker: bool,
    show_project: bool,
    /// Hold the song back until the first note is played, when play is pressed with recording armed.
    wait_input: bool,
    wheel_acc: f32,
    script: Option<testkeys::Script>,
    wheel_move: Option<keys::WheelMove>,
    wheel_over: Option<keys::WheelMove>,
    /// Keep the notes already there when recording over them.
    rec_blend: bool,
    /// The time display counts minutes and seconds, not bars.
    time_secs: bool,
    /// When the tempo was last tapped.
    taps: Vec<std::time::Instant>,
    /// The notes this recording pass wrote, which a pass that does not blend must not wipe again.
    rec_new: Vec<(u32, u16, u8)>,
    /// Where step editing writes next, and how many keys are down.
    step_at: Option<u32>,
    step_down: usize,
    rename_insert: Option<(usize, String)>,
    rename_marker: Option<(usize, String)>,
    /// The tool list that F3 opens at the pointer (a zero position is filled in when it is first shown).
    tool_menu: Option<egui::Pos2>,
    pub(crate) grid_win: bool,
    pub(crate) setup: setup::State,
    pub(crate) settings: setup::Settings,
    sel_chan: usize,
    /// MIDI keyboards and pads.
    midi: midi::MidiIn,
    sel_insert: usize,
    /// Mixer tracks selected besides the one that has the keys; faders, pans and mutes move together.
    mixer_sel: std::collections::BTreeSet<usize>,
    rack: rack::State,
    piano: pianoroll::State,
    playlist: playlist::State,
    browser: browser::State,
    peaks: Vec<(f32, f32)>,
    /// The mixer's wave view: the latest levels of every track, newest first, drawn down its meter.
    pub(crate) mixer_wave: bool,
    peak_hist: Vec<std::collections::VecDeque<f32>>,
    held: HashMap<Key, (u16, u8)>,
    actions: Vec<Action>,
    /// Undo history: snapshots of the song, oldest first; `hist_at` is the one on screen.
    history: Vec<Song>,
    hist_at: usize,
    /// An edit is under way (mouse still down); it becomes one history step when it ends.
    hist_pending: bool,
    show_info: bool,
    pub(crate) prefs_win: settings::State,
    show_about: bool,
    /// Note scripts the user said "not now" to since the program started.
    script_later: Vec<String>,
    render: Option<RenderOpts>,
    rec_held: Vec<(u16, u8, f64, f32)>,
    /// Record sound from the input device too while recording runs.
    rec_audio: bool,
    audio_rec: Option<record::AudioRec>,
    /// The sound card inputs, open while a mixer track listens to one; the flag stops retries after a failure.
    live_in: Option<record::LiveIn>,
    live_in_failed: bool,
    save_preset: Option<(String, bool)>,
    pub(crate) state_ask: Option<StateAsk>,
    show_chanset: bool,
    show_events: bool,
    events: events::State,
    /// The control waiting for a knob of the MIDI device to be moved, while its link window is open.
    link_learn: Option<cereal_engine::song::Target>,
    /// The effect slot whose editor is open: insert, slot.
    fx_open: Option<(usize, u16)>,
    /// The takes of the open wave recorders, by mixer track and slot.
    wave_eds: std::collections::HashMap<(usize, u16), waveed::WaveEd>,
    /// Preset files per plugin, found in the user's own content.
    preset_lists: std::collections::HashMap<String, std::rc::Rc<Vec<PathBuf>>>,
    /// The template projects found in the user's content, once looked for.
    templates: Option<Vec<(String, PathBuf)>>,
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
        let settings = setup::Settings::load();
        setup::load_trusted_scripts(&settings.content);
        record::set_input(&settings.audio_in);
        midi::set_prefs(midi::Prefs { off: settings.midi_off.clone(), out: settings.midi_out.clone(), sync: settings.midi_sync });
        midi::PERF_CHAN.store(settings.midi_perf as usize, std::sync::atomic::Ordering::Relaxed);
        let mut made = None;
        let started = start_audio(&settings, |rate| {
            let (h, p) = new_player(Song::new(), rate);
            made = Some(h);
            p
        });
        let (stream, audio, handle) = match (started, made) {
            (Ok((s, name)), Some(h)) => (Some(s), name, h),
            (r, _) => {
                // no device: keep a player nobody pulls so the rest of the app still works
                let (h, _p) = new_player(Song::new(), 44_100.0);
                (None, format!("NO AUDIO: {}", r.err().unwrap_or_default()), h)
            }
        };
        let lib = Library::new();
        let mut browser = browser::State::default();
        let env_roots = lib.factory_roots.clone();
        for d in &lib.search_dirs {
            browser.add_root(&d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), d.clone());
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) {
            browser.add_root("Music", home.join("Music"));
            browser.add_root("Documents", home.join("Documents"));
        }
        let midi = midi::MidiIn::start(handle.sender());
        midi::start_out(handle.transport.clone());
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
            rec_quantise: false,
            focus: Win::None,
            raise: None,
            pl_max: false,
            show_picker: false,
            show_project: false,
            wait_input: false,
            wheel_acc: 0.0,
            script: testkeys::Script::from_env(),
            wheel_move: None,
            wheel_over: None,
            rec_blend: true,
            time_secs: false,
            taps: Vec::new(),
            rec_new: Vec::new(),
            step_at: None,
            step_down: 0,
            rename_insert: None,
            rename_marker: None,
            tool_menu: None,
            grid_win: false,
            setup: Default::default(),
            settings,
            sel_chan: 0,
            midi,
            sel_insert: 0,
            mixer_sel: Default::default(),
            rack: Default::default(),
            piano: Default::default(),
            playlist: Default::default(),
            browser,
            peaks: vec![],
            mixer_wave: false,
            peak_hist: vec![],
            held: HashMap::new(),
            actions: vec![],
            history: vec![],
            hist_at: 0,
            hist_pending: false,
            show_info: false,
            prefs_win: Default::default(),
            show_about: false,
            script_later: Vec::new(),
            render: None,
            rec_held: Vec::new(),
            rec_audio: false,
            audio_rec: None,
            live_in: None,
            live_in_failed: false,
            save_preset: None,
            state_ask: None,
            show_chanset: false,
            show_events: false,
            events: events::State::default(),
            link_learn: None,
            fx_open: None,
            wave_eds: Default::default(),
            preset_lists: Default::default(),
            templates: None,
            preset_name: Default::default(),
            chanset: Default::default(),
            rename_pattern: None,
        };
        for f in app.settings.folders.clone() {
            if f.is_dir() {
                app.add_search_folder(f);
            }
        }
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
                    "midiset" => app.prefs_win.open = true,
                    "audioset" => {
                        app.prefs_win.open = true;
                        app.prefs_win.page = settings::Page::Audio;
                    }
                    "piano" => app.show_piano = true,
                    "channel" => app.show_chanset = true,
                    "misc" => {
                        app.show_chanset = true;
                        app.chanset.show_misc();
                    }
                    "events" => app.show_events = true,
                    x if x.starts_with("fx=") => {
                        let mut it = x[3..].split(':').filter_map(|v| v.parse::<usize>().ok());
                        if let (Some(i), Some(sl)) = (it.next(), it.next()) {
                            app.fx_open = Some((i, sl as u16));
                            app.sel_insert = i;
                        }
                    }
                    "hostfx" => {
                        // put the first installed plug-in on the master track's first slot and open it
                        let known = cereal_engine::host::known();
                        if let Some(inst) = known.first().and_then(|f| cereal_engine::host::Instance::open(&f.path, &f.id)) {
                            if let Some(ins) = app.song.inserts.first_mut() {
                                ins.slots.retain(|s| s.index != 0);
                                ins.slots.insert(0, cereal_engine::song::Slot::hosted(0, inst));
                                app.fx_open = Some((0, 0));
                                app.sel_insert = 0;
                                app.dirty = true;
                            }
                        }
                    }
                    "hostwin" => {
                        for h in app.song.hosted() {
                            eprintln!("own window: has {} shown {}", h.has_window(), h.show_window("Test"));
                        }
                    }
                    x if x.starts_with("addfx=") => {
                        // put a unit on the master track's first slot and open it
                        if let Some(d) = cereal_engine::fx::DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(&x[6..])) {
                            if let Some(ins) = app.song.inserts.first_mut() {
                                ins.slots.retain(|s| s.index != 0);
                                let mut s = cereal_engine::song::Slot::with_fx(0, d.kind);
                                s.plugin = d.ident.to_string();
                                ins.slots.insert(0, s);
                                app.fx_open = Some((0, 0));
                                app.sel_insert = 0;
                                app.handle.publish(app.song.clone());
                            }
                        }
                    }
                    x if x.starts_with("preset=") => {
                        // load a state file as if it had been picked from the browser
                        app.load_preset(std::path::Path::new(&x[7..]), PresetTo::Auto);
                        app.handle.publish(app.song.clone());
                    }
                    x if x.starts_with("gen=") => {
                        // put an instrument on the first channel and open its settings
                        if let Some(d) = cereal_engine::synth::DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(&x[4..])) {
                            if let Some(c) = app.song.channels.first_mut() {
                                *c = cereal_engine::song::Chan::instrument(d.kind);
                                if d.ident.eq_ignore_ascii_case(cereal_engine::midiout::IDENT) {
                                    c.plugin = d.ident.into();
                                    c.name = d.title.into();
                                    let mut set = cereal_engine::midiout::Set::fresh();
                                    *set.ctl_mut(0) = cereal_engine::midiout::Ctl { slot: 0, num: 74, on: true, kind: 0, lo: 0, hi: 127, chan: -1, name: "Filter cutoff".into(), short: "Cutoff".into() };
                                    set.values[0] = 90;
                                    if let Some(g) = c.gen.as_mut() {
                                        g.raw = set.write();
                                    }
                                }
                                app.sel_chan = 0;
                                app.show_chanset = true;
                                app.handle.publish(app.song.clone());
                            }
                        }
                    }
                    "kit" => {
                        // a pad machine on the first channel, with a couple of layers, and its settings open
                        use cereal_engine::zones::{Kit, Layer, Map};
                        let mut kit = Kit::fresh();
                        kit.pads[1].put_name("Kick");
                        kit.pads[1].layers[0].put_path("kick soft.wav");
                        kit.pads[1].layers.push(Layer::fresh());
                        kit.pads[1].layers[1].put_path("kick hard.wav");
                        kit.pads[1].spread_even();
                        if let Some(c) = app.song.channels.first_mut() {
                            *c = cereal_engine::song::Chan { name: "Pad Machine".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: Kit::IDENT.into(), ..Default::default() };
                            c.zones = Map::parse(Kit::IDENT, &kit.write()).map(std::sync::Arc::new);
                            app.sel_chan = 0;
                            app.show_chanset = true;
                            app.handle.publish(app.song.clone());
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
        self.handle.transport.set_waiting(false);
        self.step_at = None;
        self.handle.transport.set_playing(false);
        self.record_notes(true);
        self.handle.transport.seek(0);
    }

    /// Keep the sound card inputs open exactly while some mixer track listens to one.
    fn keep_inputs(&mut self) {
        let want = self.song.inserts.iter().any(|i| i.audio_in >= 0) && std::env::var_os("CEREAL_NO_INPUT").is_none();
        if !want {
            self.live_in = None;
            self.live_in_failed = false;
        } else if self.live_in.is_none() && !self.live_in_failed {
            match record::LiveIn::start(self.handle.transport.clone(), OUT_RATE.load(std::sync::atomic::Ordering::Relaxed)) {
                Ok(l) => self.live_in = Some(l),
                Err(e) => {
                    self.live_in_failed = true;
                    self.status = format!("Audio input failed: {e}");
                }
            }
        }
    }

    /// Start capturing sound when recording begins and turn it into an audio clip when it ends.
    fn record_audio(&mut self) {
        let tr = self.handle.transport.clone();
        let want = self.rec_audio && tr.recording() && tr.playing();
        if want && self.audio_rec.is_none() {
            let at = if tr.song_mode() { tr.tick().max(0.0) as u32 } else { 0 };
            match record::AudioRec::start(at, tr.clone()) {
                Ok(r) => self.audio_rec = Some(r),
                Err(e) => {
                    self.status = format!("Audio recording failed: {e}");
                    self.rec_audio = false;
                }
            }
        } else if !want {
            if let Some(r) = self.audio_rec.take() {
                let tps = (if tr.tempo() > 0.0 { tr.tempo() } else { self.song.tempo }) as f64 * self.song.ppq as f64 / 60.0;
                let (at, cut) = r.placed(tr.song_mode(), OUT_LAG_US.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6, tps);
                match r.finish(&record::folder(self.path.as_deref()), cut) {
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
        let played = tr.take_perf();
        if !played.is_empty() {
            self.write_performance(played);
        }
        let mut evs = tr.take_live();
        if tr.step_edit() && !tr.playing() {
            return self.step_notes(evs);
        }
        self.step_down = 0;
        if !tr.playing() || !tr.recording() {
            self.rec_new.clear();
        }
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
                let mut pos = clip(&self.song, start).round().max(0.0);
                if self.rec_quantise {
                    let grid = self.pr_snap().max(1) as f64;
                    pos = (pos / grid).round() * grid;
                    if !song_mode && pos >= plen && plen > 0.0 {
                        pos = 0.0;
                    }
                }
                let mut len = e.tick - start;
                if len < 0.0 && !song_mode {
                    // the loop came round while the key was down
                    len = (plen - start).max(1.0);
                }
                let note = cereal_engine::song::Note { pos: pos as u32, len: len.round().max(1.0) as u32, chan, key, vel, pan: 0.0, fine_cents: 0.0, muted: false, ..Default::default() };
                if !self.rec_blend {
                    // a pass that does not blend takes the place of what was there before it
                    let (a, b) = (note.pos, note.pos + note.len);
                    let new = &self.rec_new;
                    self.song.patterns[pat].notes.retain(|n| n.chan != chan || n.pos >= b || n.pos + n.len.max(1) <= a || new.contains(&(n.pos, n.chan, n.key)));
                    self.rec_new.push((note.pos, chan, key));
                    self.piano.sel.clear();
                }
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

    /// FILE > Export > Zipped loop package: the project and the sounds it uses in one archive.
    fn export_package(&mut self) {
        let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        let Some(path) = rfd::FileDialog::new().add_filter("Zipped loop package", &["zip"]).set_file_name(format!("{name}.zip")).save_file() else { return };
        let dir = self.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        let made = cereal_engine::package(&mut self.song, self.flp.as_ref(), &name, &self.lib, dir.as_deref());
        self.status = match made.and_then(|(bytes, n)| std::fs::write(&path, bytes).map(|_| n).map_err(|e| e.to_string())) {
            Ok(n) => format!("Packed the project and {n} sound files into {}", path.display()),
            Err(e) => format!("Packing failed: {e}"),
        };
    }

    /// FILE > Export > Project bones: every channel and used mixer track as a preset file and every pattern's
    /// notes as score files, in folders inside one picked folder.
    fn export_bones(&mut self) {
        let Some(root) = rfd::FileDialog::new().set_title("Folder for the project bones").pick_folder() else { return };
        let clean = |s: &str| -> String { s.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect::<String>().trim().to_string() };
        let mut n = 0;
        let mut put = |dir: &str, name: String, bytes: Vec<u8>| -> Result<(), String> {
            let d = root.join(dir);
            std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
            let to = d.join(name);
            std::fs::write(&to, bytes).map_err(|e| format!("{}: {e}", to.display()))?;
            n += 1;
            Ok(())
        };
        let mut go = || -> Result<(), String> {
            for (i, c) in self.song.channels.iter().enumerate() {
                if let Ok(b) = cereal_engine::preset::write_channel(c) {
                    put("Channels", format!("{:02} {}.fst", i + 1, clean(&c.name)), b)?;
                }
            }
            for (i, ins) in self.song.inserts.iter().enumerate() {
                if i > 0 && ins.slots.is_empty() && ins.name.is_empty() {
                    continue;
                }
                if let Ok(b) = cereal_engine::preset::write_track(ins) {
                    let name = if i == 0 { "Master".to_string() } else if ins.name.is_empty() { format!("Insert {i}") } else { clean(&ins.name) };
                    put("Mixer", format!("{i:03} {name}.fst"), b)?;
                }
            }
            for (pi, p) in self.song.patterns.iter().enumerate() {
                let pname = if p.name.is_empty() { format!("Pattern {}", pi + 1) } else { clean(&p.name) };
                for (ci, c) in self.song.channels.iter().enumerate() {
                    let notes: Vec<&cereal_engine::song::Note> = p.notes.iter().filter(|x| x.chan as usize == ci).collect();
                    if !notes.is_empty() {
                        put("Scores", format!("{pname} - {}.fsc", clean(&c.name)), cereal_engine::score::write(&notes, self.song.ppq))?;
                    }
                }
            }
            Ok(())
        };
        let done = go();
        self.status = match done {
            Ok(()) => format!("Wrote {n} files of project bones into {}", root.display()),
            Err(e) => format!("Project bones failed: {e}"),
        };
    }

    fn open(&mut self, path: &Path) {
        // a zipped package is unpacked into a folder of its own name beside it, then opened from there
        let unpacked;
        let path = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            match cereal_engine::unpack(path, &path.with_extension("")) {
                Ok(p) => {
                    unpacked = p;
                    unpacked.as_path()
                }
                Err(e) => {
                    self.status = format!("Could not open {}: {e}", path.display());
                    return;
                }
            }
        } else {
            path
        };
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
                self.remember(path);
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
        let mut d = rfd::FileDialog::new().add_filter("Project files", &["flp", "zip"]);
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
                self.remember(&path);
                self.path = Some(path);
                self.unsaved = false;
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Ctrl+R opens the rendering window; its Start button asks where to save and writes the file.
    fn export(&mut self) {
        if self.render.is_none() {
            self.render = Some(RenderOpts { flac: false, lossy: 0, kbps: 192, quality: 6.0, bits: 16, rate: 44_100, tail: 6.0, song: self.handle.transport.song_mode() });
        }
    }

    fn render_to_file(&mut self, o: &RenderOpts) {
        let (ext, label) = match (o.lossy, o.flac) {
            (1, _) => ("mp3", "MP3 audio"),
            (2, _) => ("ogg", "Ogg Vorbis audio"),
            (_, true) => ("flac", "FLAC audio"),
            _ => ("wav", "WAV audio"),
        };
        let default = self.path.as_ref().map(|p| p.with_extension(ext)).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| format!("untitled.{ext}"));
        let Some(path) = rfd::FileDialog::new().add_filter(label, &[ext]).set_file_name(default).save_file() else { return };
        let buf = cereal_engine::render_song(&self.song, o.rate as f32, o.song, self.handle.transport.pattern(), o.tail);
        if o.lossy != 0 {
            let made = if o.lossy == 1 { cereal_engine::lossy::mp3(&buf, o.rate, o.kbps) } else { cereal_engine::lossy::ogg(&buf, o.rate, o.quality) };
            self.status = match made.and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string())) {
                Ok(()) => format!("Exported {}", path.display()),
                Err(e) => format!("Export failed: {e}"),
            };
            return;
        }
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

    /// Apply what the MIDI device's knobs and sliders sent, and run the "link to controller" window.
    fn controllers_ui(&mut self, ctx: &egui::Context) {
        let moves = midi::take_controls();
        if let Some(t) = self.link_learn {
            let have = self.song.midi_links.iter().find(|l| l.target == t).map(|l| l.ctrl);
            let (mut open, mut done, mut unlink) = (true, false, false);
            egui::Window::new("Remote control settings").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.label(RichText::new(self.song.target_name(t)).strong());
                ui.label(match have {
                    Some(c) => format!("Linked to controller {c}."),
                    None => "Not linked.".to_string(),
                });
                ui.label("Move a knob or slider on your MIDI device to link it.");
                // what the control gets is this formula's answer for what was sent (Input); empty sends it on as it is
                let id = egui::Id::new(("link_formula", self.song.event_address(t)));
                let had = self.song.link_maps.iter().find(|m| m.target == t).map(|m| m.text.clone()).unwrap_or_default();
                let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| had.clone());
                ui.horizontal(|ui| {
                    ui.label("Mapping formula");
                    ui.add(egui::TextEdit::singleline(&mut text).hint_text("Input").desired_width(220.0));
                    egui::ComboBox::from_id_salt("link_formula_pick").width(18.0).selected_text("").show_ui(ui, |ui| {
                        for f in ["Input", "1-Input", "Input*0.5", "Input*0.5+0.5", "Input*2", "Input^2", "Tension(Input,10)", "Tension(1-Input,10)", "1-Cos(Input*Pi)*0.5-0.5"] {
                            if ui.selectable_label(false, f).clicked() {
                                text = f.to_string();
                            }
                        }
                    });
                });
                let t_trim = text.trim().to_string();
                if t_trim != had && (t_trim.is_empty() || cereal_engine::formula::compile(&t_trim).is_ok()) {
                    self.song.link_maps.retain(|m| m.target != t);
                    if !t_trim.is_empty() && !t_trim.eq_ignore_ascii_case("input") {
                        self.song.link_maps.push(cereal_engine::song::LinkMap::new(t, &t_trim));
                    }
                    self.dirty = true;
                } else if t_trim != had {
                    ui.label(RichText::new("The formula does not read yet.").small().color(TEXT_DIM));
                }
                ui.data_mut(|d| d.insert_temp(id, text));
                // sender units in the mixer can drive the control too
                let mut senders: Vec<(usize, u16, u16, String)> = vec![];
                for (i, ins) in self.song.inserts.iter().enumerate() {
                    for s in &ins.slots {
                        let outs: &[&str] = match s.fx.as_ref().map(|f| f.kind) {
                            Some(cereal_engine::fx::Kind::Peak) => &["peak + sweep", "peak", "sweep"],
                            Some(cereal_engine::fx::Kind::Xy) => &["X", "Y"],
                            Some(cereal_engine::fx::Kind::Xyz) => &["X", "Y", "Z"],
                            Some(cereal_engine::fx::Kind::Formula) => &["result"],
                            Some(cereal_engine::fx::Kind::Split) => &["attack gate"],
                            _ => &[],
                        };
                        for (k, o) in outs.iter().enumerate() {
                            senders.push((i, s.index, k as u16, format!("{} ({}, slot {}): {o}", s.title(), self.song.insert_name(i), s.index + 1)));
                        }
                        // a board sends one output for every control on it (two for a pad)
                        if let Some(raw) = s.fx.as_ref().filter(|f| f.kind == cereal_engine::fx::Kind::Surface).and_then(|f| f.raw.as_ref()) {
                            for c in cereal_engine::surface::controls(raw) {
                                for (n, &k) in c.outs.iter().enumerate() {
                                    let axis = if c.outs.len() > 1 { [" X", " Y", " Z"].get(n).copied().unwrap_or("") } else { "" };
                                    senders.push((i, s.index, k, format!("{} ({}, slot {}): {}{axis}", s.title(), self.song.insert_name(i), s.index + 1, c.name)));
                                }
                            }
                        }
                    }
                }
                for (c, ch) in self.song.channels.iter().enumerate() {
                    if ch.plugin.eq_ignore_ascii_case(cereal_engine::fm::ENV_SENDER) {
                        let n = ch.gen.as_ref().and_then(|g| cereal_engine::fm::Sender::parse(&g.raw)).map(|s| s.arts.len()).unwrap_or(0);
                        for k in 0..n {
                            senders.push((cereal_engine::song::CtlLink::CHAN, c as u16, k as u16, format!("{} (channel {}): output {}", ch.name, c + 1, k + 1)));
                        }
                    }
                    if ch.plugin.eq_ignore_ascii_case(cereal_engine::synth::KEY_SENDER) {
                        for (k, o) in ["note", "velocity"].iter().enumerate() {
                            senders.push((cereal_engine::song::CtlLink::CHAN, c as u16, k as u16, format!("{} (channel {}): {o}", ch.name, c + 1)));
                        }
                    }
                }
                let held = self.song.ctl_links.iter().find(|l| l.target == t).copied();
                if !senders.is_empty() {
                    ui.separator();
                    ui.label("Or pick a sender unit:");
                    for (i, slot, out, name) in &senders {
                        let on = held.is_some_and(|l| (l.ins, l.slot, l.out) == (*i, *slot, *out));
                        if ui.selectable_label(on, name).clicked() && !on {
                            self.song.ctl_links.retain(|l| l.target != t);
                            self.song.ctl_links.push(cereal_engine::song::CtlLink { ins: *i, slot: *slot, out: *out, target: t });
                            self.status = format!("{} linked to {name}", self.song.target_name(t));
                            self.dirty = true;
                            done = true;
                        }
                    }
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    if held.is_some() && ui.button("Remove sender link").clicked() {
                        self.song.ctl_links.retain(|l| l.target != t);
                        self.dirty = true;
                        done = true;
                    }
                    if have.is_some() && ui.button("Remove link").clicked() {
                        unlink = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
            });
            if unlink || !moves.is_empty() {
                self.song.midi_links.retain(|l| l.target != t);
                if let (Some(&(cc, v)), false) = (moves.first(), unlink) {
                    self.song.midi_links.push(cereal_engine::song::MidiLink { ctrl: cc as u16, port: 0, target: t });
                    self.song.set_travel(t, self.song.mapped(t, v as f32 / 127.0));
                    self.status = format!("Controller {cc} linked to {}", self.song.target_name(t));
                }
                self.dirty = true;
                done = true;
            }
            if done || !open {
                ctx.data_mut(|d| d.remove::<String>(egui::Id::new(("link_formula", self.song.event_address(t)))));
                self.link_learn = None;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        for (cc, v) in moves {
            let hit: Vec<_> = self.song.midi_links.iter().filter(|l| l.ctrl == cc as u16 && !matches!(l.target, cereal_engine::song::Target::Other { .. })).map(|l| l.target).collect();
            for t in hit {
                self.song.set_travel(t, self.song.mapped(t, v as f32 / 127.0));
                self.dirty = true;
            }
        }
        if !self.song.midi_links.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
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
                    let mut f = if o.lossy != 0 { o.lossy + 1 } else { o.flac as u8 };
                    for (k, name) in ["WAV", "FLAC", "MP3", "OGG"].iter().enumerate() {
                        ui.selectable_value(&mut f, k as u8, *name);
                    }
                    o.flac = f == 1;
                    o.lossy = f.saturating_sub(1);
                });
                ui.end_row();
                match o.lossy {
                    1 => {
                        ui.label("Bitrate");
                        ui.horizontal(|ui| {
                            for k in [128u32, 160, 192, 256, 320] {
                                ui.selectable_value(&mut o.kbps, k, format!("{k} kbps"));
                            }
                        });
                    }
                    2 => {
                        ui.label("Quality");
                        ui.add(egui::Slider::new(&mut o.quality, 0.0..=10.0).step_by(1.0));
                    }
                    _ => {
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
                    }
                }
                ui.end_row();
                ui.label("Sample rate");
                ui.horizontal(|ui| {
                    // the lossy formats stop at 48 kHz
                    if o.lossy != 0 && o.rate > 48_000 {
                        o.rate = 48_000;
                    }
                    for r in [44_100u32, 48_000, 88_200, 96_000] {
                        if o.lossy == 0 || r <= 48_000 {
                            ui.selectable_value(&mut o.rate, r, format!("{r}"));
                        }
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
        self.song.clips.push(cereal_engine::song::Clip { pos, len, track, src: cereal_engine::song::ClipSrc::Audio(ci), offset: 0, muted: false, group: 0 });
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
        use cereal_engine::preset::{self, Holds, Unit};
        let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("{}: {e}", path.display());
                return;
            }
        };
        match preset::holds(&bytes) {
            Holds::Channel => {
                match preset::channel(&bytes, Some(&mut self.lib), path.parent()) {
                    Ok(mut c) => {
                        if c.name.is_empty() {
                            c.name = name.clone();
                        }
                        let at = match to {
                            PresetTo::Chan(ci) if ci < self.song.channels.len() => {
                                // the channel stays on its mixer track and keeps the notes it has
                                c.insert = self.song.channels[ci].insert;
                                c.src = None;
                                self.song.channels[ci] = c;
                                ci
                            }
                            _ => {
                                c.insert = 0;
                                self.song.channels.push(c);
                                self.song.channels.len() - 1
                            }
                        };
                        self.sel_chan = at;
                        self.status = format!("Channel state: {name}");
                        self.dirty = true;
                    }
                    Err(e) => self.status = e,
                }
                return;
            }
            Holds::Track => {
                match preset::track(&bytes, Some(&mut self.lib), path.parent()) {
                    Ok(mut t) => {
                        let i = match to {
                            PresetTo::Slot(i, _) => i,
                            _ => self.sel_insert,
                        }
                        .min(self.song.inserts.len().saturating_sub(1));
                        let Some(old) = self.song.inserts.get_mut(i) else { return };
                        // where the track sends its sound belongs to the project
                        t.sends = std::mem::take(&mut old.sends);
                        t.levels = std::mem::take(&mut old.levels);
                        if t.name.is_empty() {
                            t.name = std::mem::take(&mut old.name);
                        }
                        *old = t;
                        self.fx_open = None;
                        self.sel_insert = i;
                        self.show_mixer = true;
                        self.status = format!("Mixer track state: {name}");
                        self.dirty = true;
                    }
                    Err(e) => self.status = e,
                }
                return;
            }
            Holds::Plugin => {}
        }
        let pre = match preset::read(path) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        // a patch bay can stand as an instrument or as an effect: where the file lives, or where it is dropped, says which
        if pre.ident.eq_ignore_ascii_case(cereal_engine::song::BAY) {
            let as_gen = match to {
                PresetTo::Chan(_) => true,
                PresetTo::Slot(..) => false,
                PresetTo::Auto => path.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case("Generators")),
            };
            if let Some((p, lead, g, layers)) = Some(()).filter(|_| as_gen).and_then(|_| cereal_engine::fxpatch::decode_gen(&pre.state)) {
                let mut c = Chan::instrument(g.kind);
                c.plugin = cereal_engine::song::BAY.into();
                c.name = name.clone();
                c.gen = Some(g);
                c.bay = Some(cereal_engine::song::Bay { fx: cereal_engine::fx::Fx { kind: cereal_engine::fx::Kind::Patch, p: vec![], ir: None, sub: Some(std::sync::Arc::new(p)), raw: None, ext: None }, lead, layers });
                let ci = match to {
                    PresetTo::Chan(ci) if ci < self.song.channels.len() => {
                        c.insert = self.song.channels[ci].insert;
                        self.song.channels[ci] = c;
                        ci
                    }
                    _ => {
                        self.song.channels.push(c);
                        self.song.channels.len() - 1
                    }
                };
                self.sel_chan = ci;
                self.preset_name.insert(cereal_engine::song::BAY.to_string(), path.to_path_buf());
                self.status = format!("Preset: {name}");
                self.dirty = true;
                return;
            }
        }
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
                .filter(|&c| self.song.channels.get(c).is_some_and(|c| c.plugin.eq_ignore_ascii_case(&pre.ident) && c.gen.as_ref().is_some_and(|x| x.kind == g.kind)));
                let kind = g.kind;
                // several silent units share one kind, so the name the file gives decides which this is
                let ident = cereal_engine::synth::DEFS.iter().find(|d| d.ident.eq_ignore_ascii_case(&pre.ident)).map(|d| d.ident).unwrap_or(cereal_engine::synth::def(kind).ident);
                let ci = match target {
                    Some(c) => c,
                    None => {
                        let mut c = Chan::instrument(kind);
                        c.plugin = ident.into();
                        c.name = name.clone();
                        self.song.channels.push(c);
                        self.song.channels.len() - 1
                    }
                };
                // instruments made of recordings: the sounds the block names are found and loaded now
                let bank = cereal_engine::sf2::open(ident, &pre.state, Some(&mut self.lib), path.parent()).map(std::sync::Arc::new);
                let zones = bank.is_none().then(|| cereal_engine::zones::Map::parse(ident, &pre.state)).flatten().map(|mut m| {
                    m.load(&mut self.lib, path.parent());
                    std::sync::Arc::new(m)
                });
                let c = &mut self.song.channels[ci];
                c.gen = Some(g);
                if bank.is_some() || zones.is_some() {
                    c.zones = bank.or(zones);
                }
                self.sel_chan = ci;
                self.preset_name.insert(ident.to_string(), path.to_path_buf());
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
                    } else if cereal_engine::preset::is_file(&p) {
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
            if ui.small_button("Save").hint("Save these settings as a preset file").clicked() {
                self.save_preset = Some((ident.to_string(), generator));
            }
            if list.is_empty() {
                ui.label(egui::RichText::new("Presets: none found (Options > Sounds and presets from your own copy)").small().color(widgets::TEXT_DIM));
                return;
            }
            let n = list.len();
            if ui.small_button("<").hint("Previous preset").clicked() {
                pick = Some(list[at.map(|i| (i + n - 1) % n).unwrap_or(n - 1)].clone());
            }
            if ui.small_button(">").hint("Next preset").clicked() {
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

    /// Open or save a whole channel or a whole mixer track as a state file.
    fn state_file(&mut self, ask: StateAsk) {
        use cereal_engine::preset;
        let folder = |lib: &cereal_engine::sample::Library, sub: &str| lib.factory_roots.iter().map(|r| r.join("Data/Patches").join(sub)).find(|p| p.is_dir());
        let (sub, kind) = match ask {
            StateAsk::OpenChan(_) | StateAsk::SaveChan(_) => ("Channel presets", "Channel state file"),
            _ => ("Mixer presets", "Mixer track state file"),
        };
        let mut d = rfd::FileDialog::new().add_filter(kind, &cereal_engine::preset::EXTS);
        if let Some(dir) = folder(&self.lib, sub) {
            d = d.set_directory(dir);
        }
        match ask {
            StateAsk::OpenChan(ci) => {
                if let Some(p) = d.pick_file() {
                    self.load_preset(&p, PresetTo::Chan(ci));
                }
            }
            StateAsk::OpenTrack(i) => {
                if let Some(p) = d.pick_file() {
                    self.load_preset(&p, PresetTo::Slot(i, 0));
                }
            }
            StateAsk::SaveChan(ci) => {
                let Some(c) = self.song.channels.get(ci) else { return };
                let made = preset::write_channel(c);
                let Some(path) = d.set_file_name(format!("{}.fst", if c.name.is_empty() { "Channel" } else { c.name.as_str() })).save_file() else { return };
                self.status = match made.and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string())) {
                    Ok(()) => format!("Saved channel state {}", path.display()),
                    Err(e) => format!("Channel state not saved: {e}"),
                };
            }
            StateAsk::SaveTrack(i) => {
                let Some(t) = self.song.inserts.get(i) else { return };
                let made = preset::write_track(t);
                let Some(path) = d.set_file_name(format!("{}.fst", self.song.insert_name(i))).save_file() else { return };
                self.status = match made.and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string())) {
                    Ok(()) => format!("Saved mixer track state {}", path.display()),
                    Err(e) => format!("Mixer track state not saved: {e}"),
                };
            }
        }
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
        let tracks = self.song.inserts.len();
        self.mixer_sel.retain(|i| *i < tracks);
        let tr = &self.handle.transport;
        tr.set_pattern(tr.pattern().min(self.song.patterns.len().saturating_sub(1)));
        self.unsaved = true;
        self.song.activate_hosted(OUT_RATE.load(std::sync::atomic::Ordering::Relaxed) as f32);
        self.handle.publish(self.song.clone());
    }

    /// Mark the channels nothing plays. Returns how many.
    fn select_unused_channels(&mut self) -> usize {
        let mut used = vec![false; self.song.channels.len()];
        for n in self.song.patterns.iter().flat_map(|p| &p.notes) {
            if let Some(u) = used.get_mut(n.chan as usize) {
                *u = true;
            }
        }
        for c in &self.song.clips {
            if let cereal_engine::song::ClipSrc::Audio(ch) | cereal_engine::song::ClipSrc::Automation(ch) = c.src {
                if let Some(u) = used.get_mut(ch) {
                    *u = true;
                }
            }
        }
        // a layer's children sound through it
        for (i, c) in self.song.channels.iter().enumerate() {
            if used[i] {
                for &k in &c.children {
                    if let Some(u) = used.get_mut(k as usize) {
                        *u = true;
                    }
                }
            }
        }
        let idle: Vec<usize> = (0..used.len()).filter(|i| !used[*i]).collect();
        self.rack.sel = idle.iter().copied().collect();
        idle.len()
    }

    /// Drop the nameless patterns that hold nothing and lie nowhere, except the one on show.
    fn purge_patterns(&mut self) -> usize {
        let cur = self.handle.transport.pattern();
        let mut cur_now = cur;
        let mut n = 0;
        for i in (0..self.song.patterns.len()).rev() {
            let p = &self.song.patterns[i];
            let placed = self.song.clips.iter().any(|c| c.src == cereal_engine::song::ClipSrc::Pattern(i));
            if i != cur && p.notes.is_empty() && p.events.is_empty() && p.name.is_empty() && !placed && self.song.patterns.len() > 1 {
                self.song.remove_pattern(i);
                if i < cur_now {
                    cur_now -= 1;
                }
                n += 1;
            }
        }
        self.handle.transport.set_pattern(cur_now);
        self.dirty |= n > 0;
        n
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

    /// Ctrl+F8: everything the project holds, to pick what is drawn next.
    fn project_ui(&mut self, ctx: &egui::Context) {
        if !self.show_project {
            return;
        }
        let mut open = true;
        let mut picked = false;
        egui::Window::new("Project picker").open(&mut open).collapsible(false).default_size([320.0, 420.0]).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| picked = self.project_list(ui));
        });
        self.show_project = open && !picked;
    }

    /// F8: the instruments there are, to pick one for a new channel.
    fn picker_ui(&mut self, ctx: &egui::Context) {
        if !self.show_picker {
            return;
        }
        let mut open = true;
        let mut pick = None;
        egui::Window::new("Plug-in picker").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(RichText::new("Click an instrument to add it as a new channel.").color(TEXT_DIM).small());
            egui::Grid::new("picker").num_columns(4).spacing([6.0, 6.0]).show(ui, |ui| {
                for (k, d) in cereal_engine::synth::DEFS.iter().filter(|d| !d.arrives_only()).enumerate() {
                    if ui.add_sized([150.0, 34.0], egui::Button::new(d.title)).clicked() {
                        pick = Some(d.kind);
                    }
                    if k % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
        });
        if let Some(kind) = pick {
            self.song.channels.push(Chan::instrument(kind));
            self.sel_chan = self.song.channels.len() - 1;
            self.dirty = true;
            open = false;
        }
        self.show_picker = open;
    }

    /// Move the play position on or back by whole bars.
    fn jump_bar(&mut self, by: i32) {
        let bar = self.song.bar().max(1) as i64;
        let now = self.handle.transport.tick().max(0.0) as i64;
        let to = if by > 0 { (now / bar + 1) * bar } else { ((now - 1).max(0) / bar) * bar };
        self.handle.transport.seek(to.max(0) as u32);
    }

    fn keys(&mut self, ctx: &egui::Context) {
        self.do_wheel_move();
        let typing_text = ctx.wants_keyboard_input();
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            // the copy, cut and paste keys arrive as their own events, never as key presses
            match &e {
                egui::Event::Copy | egui::Event::Cut if !typing_text => {
                    let cut = matches!(e, egui::Event::Cut);
                    if !cut && ctx.input(|i| i.modifiers.shift) {
                        self.clone_pattern();
                    } else if self.focus == Win::Rack && self.show_rack {
                        self.rack_copy(cut);
                    } else if self.focus == Win::Piano && self.show_piano {
                        self.pr_copy(ctx, cut);
                    } else if self.focus == Win::Playlist && self.show_playlist {
                        self.pl_copy(ctx, cut);
                    }
                    continue;
                }
                egui::Event::Paste(text) if !typing_text => {
                    if self.focus == Win::Piano && self.show_piano && ctx.input(|i| i.modifiers.shift) {
                        self.pr_paste_midi(text);
                    } else if self.focus == Win::Rack && self.show_rack {
                        self.rack_paste();
                    } else if self.focus == Win::Piano && self.show_piano {
                        self.pr_paste();
                    } else if self.focus == Win::Playlist && self.show_playlist {
                        self.pl_paste();
                    }
                    continue;
                }
                // the star of the number pad has no key of its own here; it arrives as the character
                egui::Event::Text(t) if !typing_text && t == "*" && !ctx.input(|i| i.modifiers.shift) => {
                    if ctx.input(|i| i.modifiers.alt) {
                        self.jump_marker(true);
                    } else {
                        self.jump_bar(1);
                    }
                    continue;
                }
                _ => {}
            }
            let egui::Event::Key { key, pressed, repeat, modifiers, .. } = e else { continue };
            if !pressed {
                if let Some(track) = self.playlist.perf_held.remove(&key) {
                    self.handle.send(Cmd::PerfRelease { track });
                }
                if let Some((chan, k)) = self.held.remove(&key) {
                    self.handle.send(Cmd::NoteOff { chan, key: k });
                }
                continue;
            }
            if typing_text {
                continue;
            }
            if self.typing_keys && modifiers.is_none() && self.handle.transport.performing() {
                // while a performance runs the typing keyboard plays the clips of the zone
                if let Some((track, nth)) = App::perf_typing(key) {
                    if !repeat {
                        self.handle.send(Cmd::PerfKey { track, nth, down: true });
                        self.playlist.perf_held.insert(key, track);
                    }
                    continue;
                }
            }
            if self.focus == Win::Piano && self.show_piano && self.piano_key(key, modifiers, repeat) {
                continue;
            }
            if self.focus == Win::Playlist && self.show_playlist && self.playlist_key(key, modifiers, repeat) {
                continue;
            }
            if self.focus == Win::Rack && self.show_rack && self.rack_key(key, modifiers, repeat) {
                continue;
            }
            if self.focus == Win::Mixer && self.show_mixer && self.mixer_key(key, modifiers, repeat) {
                continue;
            }
            if self.focus == Win::Browser && self.show_browser && self.browser_key(key, modifiers) {
                continue;
            }
            if modifiers.command {
                if repeat {
                    continue;
                }
                match key {
                    Key::Z if modifiers.alt => self.undo(),
                    Key::Z => self.undo_toggle(),
                    Key::N => self.save_new_version(),
                    Key::O => self.open_dialog(),
                    Key::S => self.save(modifiers.shift),
                    Key::R if modifiers.shift => {
                        self.export();
                        if let Some(r) = self.render.as_mut() {
                            r.lossy = 1;
                        }
                    }
                    Key::R => self.export(),
                    Key::F4 => self.new_pattern(),
                    Key::ArrowUp | Key::ArrowDown if modifiers.shift && !matches!(self.focus, Win::Piano | Win::Playlist) => {
                        let p = self.handle.transport.pattern();
                        self.song.ensure_pattern(p);
                        let to = self.song.move_pattern(p, key == Key::ArrowUp);
                        self.handle.transport.set_pattern(to);
                        self.dirty = true;
                    }
                    Key::H if modifiers.shift => ctx.memory_mut(|m| m.reset_areas()),
                    Key::B => {
                        self.rec_blend = !self.rec_blend;
                        self.status = format!("Blend recorded notes: {}", if self.rec_blend { "on" } else { "off" });
                    }
                    Key::E => {
                        let tr = &self.handle.transport;
                        tr.set_step_edit(!tr.step_edit());
                        self.step_at = None;
                        self.status = format!("Step editing: {}", if tr.step_edit() { "on" } else { "off" });
                    }
                    Key::I if self.focus == Win::Events && self.show_events => self.events_insert_now(),
                    Key::I => {
                        self.wait_input = !self.wait_input;
                        self.status = format!("Wait for input to start playing: {}", if self.wait_input { "on" } else { "off" });
                    }
                    Key::T => self.typing_keys = !self.typing_keys,
                    Key::M if modifiers.shift => self.export_midi(),
                    Key::M => {
                        let tr = &self.handle.transport;
                        tr.set_metronome(!tr.metronome());
                    }
                    Key::H => self.handle.send(Cmd::Panic),
                    Key::F8 if !repeat => self.show_project = !self.show_project,
                    Key::P => {
                        let tr = &self.handle.transport;
                        tr.set_countdown(!tr.countdown());
                    }
                    Key::C if modifiers.shift => self.clone_pattern(),
                    Key::Space => {
                        if self.handle.transport.playing() {
                            self.handle.transport.set_playing(false);
                        } else {
                            self.play();
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if modifiers.alt {
                if key == Key::F8 && !repeat {
                    self.show_browser = !self.show_browser;
                } else if key == Key::Slash {
                    self.jump_marker(false);
                } else if key == Key::T && !repeat && !modifiers.shift && !modifiers.command && self.focus != Win::Piano {
                    self.add_marker();
                } else if let Some(d) = DIGITS.iter().position(|k| *k == key).filter(|_| !repeat) {
                    // the newest project is 1, the tenth is 0
                    if let Some(p) = self.settings.recent.get((d + 9) % 10).cloned() {
                        self.actions.push(Action::Open(p));
                    }
                }
                continue;
            }
            match key {
                Key::Space if !repeat => {
                    if self.handle.transport.playing() || self.handle.transport.waiting() {
                        self.stop();
                    } else {
                        self.play();
                    }
                }
                Key::Tab if !repeat => self.cycle_windows(modifiers.shift),
                Key::Escape if !repeat && !ctx.memory(|m| m.any_popup_open()) => {
                    if self.show_project {
                        self.show_project = false;
                    } else if self.show_picker {
                        self.show_picker = false;
                    } else if self.pl_max {
                        self.pl_max = false;
                    } else {
                        self.close_focused();
                    }
                }
                Key::Enter if !repeat && self.show_playlist && matches!(self.focus, Win::Playlist | Win::None) => self.pl_max = !self.pl_max,
                Key::F1 if !repeat => self.show_about = !self.show_about,
                Key::F8 if !repeat => self.show_picker = !self.show_picker,
                Key::Slash => self.jump_bar(-1),
                Key::Num0 if !self.typing_keys => self.jump_bar(1),
                k if !self.typing_keys && !repeat && !modifiers.shift && DIGITS[1..].contains(&k) => {
                    let p = DIGITS.iter().position(|d| *d == k).unwrap_or(1) - 1;
                    self.song.ensure_pattern(p);
                    self.handle.transport.set_pattern(p);
                    self.dirty = true;
                }
                Key::F5 if !repeat => self.show_playlist = !self.show_playlist,
                Key::F6 if !repeat => self.show_rack = !self.show_rack,
                Key::F7 if !repeat => self.show_piano = !self.show_piano,
                Key::F9 if !repeat => self.show_mixer = !self.show_mixer,
                Key::F3 if !repeat => self.tool_menu = if self.tool_menu.is_some() { None } else { Some(egui::Pos2::ZERO) },
                Key::F4 if !repeat => {
                    self.new_pattern();
                    self.start_rename_pattern();
                }
                Key::F2 if !repeat => self.start_rename_pattern(),
                Key::F10 if !repeat => self.prefs_win.open = !self.prefs_win.open,
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
                    if ui.button("New").clicked() {
                        self.new_project();
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
                            found.sort();
                            self.templates = Some(found);
                        }
                        let list = self.templates.clone().unwrap_or_default();
                        if list.is_empty() {
                            ui.label(RichText::new("Templates come with the sounds and presets from your own copy (OPTIONS menu).").color(TEXT_DIM));
                        }
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            for (name, path) in list {
                                if ui.button(name).clicked() {
                                    ui.close_menu();
                                    self.open(&path);
                                    // a template is a start, not the file being worked on
                                    self.path = None;
                                    self.status = "New project from a template".into();
                                }
                            }
                        });
                    });
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
                    if ui.button("Save new version      Ctrl+N").clicked() {
                        ui.close_menu();
                        self.save_new_version();
                    }
                    ui.menu_button("Recent projects", |ui| {
                        if self.settings.recent.is_empty() {
                            ui.label(RichText::new("None yet").color(TEXT_DIM));
                        }
                        for (k, p) in self.settings.recent.clone().into_iter().enumerate() {
                            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            if ui.button(format!("{name}     Alt+{}", (k + 1) % 10)).hint(p.display().to_string()).clicked() {
                                self.actions.push(Action::Open(p));
                                ui.close_menu();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Export audio...   Ctrl+R").clicked() {
                        ui.close_menu();
                        self.export();
                    }
                    if ui.add_enabled(self.path.is_some(), egui::Button::new("Revert to last saved")).clicked() {
                        ui.close_menu();
                        if let Some(p) = self.path.clone() {
                            self.open(&p);
                        }
                    }
                    ui.separator();
                    if ui.button("Export MIDI file...     Ctrl+Shift+M").clicked() {
                        ui.close_menu();
                        self.export_midi();
                    }
                    if ui.button("Import MIDI file...").clicked() {
                        ui.close_menu();
                        self.import_midi_dialog();
                    }
                    if ui.button("Export zipped loop package...").clicked() {
                        ui.close_menu();
                        self.export_package();
                    }
                    if ui.button("Export project bones...").clicked() {
                        ui.close_menu();
                        self.export_bones();
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
                    ui.menu_button("Instrument", |ui| self.instrument_menu(ui));
                    ui.separator();
                    if ui.button("Pattern                F4").clicked() {
                        self.new_pattern();
                        ui.close_menu();
                    }
                    if ui.button("Time marker         Alt+T").clicked() {
                        self.add_marker();
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
                    if ui.button("Open in piano roll              F7").clicked() {
                        self.show_piano = true;
                        ui.close_menu();
                    }
                    for (label, up) in [("Move up                           Shift+Ctrl+Up", true), ("Move down                      Shift+Ctrl+Down", false)] {
                        if ui.button(label).clicked() {
                            let p = self.handle.transport.pattern();
                            self.song.ensure_pattern(p);
                            let to = self.song.move_pattern(p, up);
                            self.handle.transport.set_pattern(to);
                            self.dirty = true;
                            ui.close_menu();
                        }
                    }
                    if ui.button("Split by channel").hint("Every channel playing in this pattern gets a pattern of its own").clicked() {
                        let p = self.handle.transport.pattern();
                        let made = self.song.split_pattern(p);
                        self.status = if made.is_empty() { "Only one channel plays in this pattern".into() } else { format!("Split into {} patterns", made.len() + 1) };
                        self.dirty |= !made.is_empty();
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
                    let mut c = self.handle.transport.countdown();
                    if ui.checkbox(&mut c, "Countdown before recording   Ctrl+P").changed() {
                        self.handle.transport.set_countdown(c);
                    }
                    ui.checkbox(&mut self.rec_quantise, "Input quantise (snap recorded notes)");
                    if ui.button("Stop all sound                          Ctrl+H").clicked() {
                        self.handle.send(Cmd::Panic);
                        ui.close_menu();
                    }
                    ui.separator();
                    for (label, page) in [("MIDI settings                          F10", settings::Page::Midi), ("Audio settings", settings::Page::Audio), ("General settings", settings::Page::General), ("File settings", settings::Page::File)] {
                        if ui.button(label).clicked() {
                            self.prefs_win.open = true;
                            self.prefs_win.page = page;
                            ui.close_menu();
                        }
                    }
                    if ui.button("Project general settings         F11").clicked() {
                        self.show_info = true;
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
                ui.menu_button("TOOLS", |ui| {
                    if ui.button("Find in the browser              Ctrl+F").clicked() {
                        self.show_browser = true;
                        self.focus = Win::Browser;
                        self.browser_key(egui::Key::F, egui::Modifiers::COMMAND);
                        ui.close_menu();
                    }
                    if ui.button("Project picker                       Ctrl+F8").clicked() {
                        self.show_project = true;
                        ui.close_menu();
                    }
                    ui.menu_button("Macros", |ui| {
                        if ui.button("Prepare for performance mode").clicked() {
                            self.prepare_performance();
                            ui.close_menu();
                        }
                        if ui.button("Select unused channels").hint("Channels with no notes in any pattern and no clip on the playlist").clicked() {
                            let n = self.select_unused_channels();
                            self.status = format!("{n} unused channels selected");
                            ui.close_menu();
                        }
                        if ui.button("Delete empty patterns").hint("Patterns with no notes or events that lie nowhere on the playlist; named ones are kept").clicked() {
                            let n = self.purge_patterns();
                            self.status = format!("{n} empty patterns removed");
                            ui.close_menu();
                        }
                        if ui.button("Purge unused audio clips").hint("Audio clip channels that lie nowhere on the playlist are removed").clicked() {
                            let mut n = 0;
                            for ci in (0..self.song.channels.len()).rev() {
                                let unused = self.song.channels[ci].kind == cereal_engine::flp::ChannelKind::AudioClip && !self.song.clips.iter().any(|c| matches!(c.src, cereal_engine::song::ClipSrc::Audio(ch) if ch == ci));
                                if unused && self.song.channels.len() > 1 {
                                    self.delete_channel(ci);
                                    n += 1;
                                }
                            }
                            self.status = format!("{n} unused audio clips removed");
                            ui.close_menu();
                        }
                        if ui.button("Close all plugin windows").clicked() {
                            self.fx_open = None;
                            self.show_chanset = false;
                            for inst in self.song.hosted() {
                                inst.hide_window();
                            }
                            ui.close_menu();
                        }
                        if ui.button("Stop all sound                Ctrl+H").clicked() {
                            self.handle.send(Cmd::Panic);
                            ui.close_menu();
                        }
                    });
                });
                ui.menu_button("HELP", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // the hint bar: what the pointer rests on, or else the last thing that happened
                    let hint = widgets::take_hint();
                    if hint.is_empty() {
                        ui.label(RichText::new(&self.status).color(TEXT_DIM).small());
                    } else {
                        ui.label(RichText::new(hint).color(TEXT).small());
                    }
                });
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let tr = self.handle.transport.clone();
                let song_mode = tr.song_mode();
                if ui.selectable_label(!song_mode, RichText::new("PAT").strong()).hint("Pattern mode (L)").clicked() {
                    tr.set_song_mode(false);
                }
                if ui.selectable_label(song_mode, RichText::new("SONG").strong()).hint("Song mode (L)").clicked() {
                    tr.set_song_mode(true);
                }
                let playing = tr.playing();
                if ui.add_sized([44.0, 24.0], egui::Button::new(RichText::new(if playing { "||" } else { ">" }).strong().color(if playing { GREEN } else { TEXT }))).hint("Play / stop (Space)").clicked() {
                    if playing {
                        tr.set_playing(false);
                    } else {
                        self.play();
                    }
                }
                if ui.add_sized([34.0, 24.0], egui::Button::new(RichText::new("[]").strong())).hint("Stop").clicked() {
                    self.stop();
                }
                let rec = tr.recording();
                if ui.add_sized([34.0, 24.0], egui::Button::new(RichText::new("REC").strong().color(if rec { egui::Color32::from_rgb(235, 70, 60) } else { TEXT_DIM }))).hint("Record what you play into the current pattern (R)").clicked() {
                    tr.set_recording(!rec);
                }
                if ui.toggle_value(&mut self.rec_audio, "MIC").hint("Also record sound from the input device, as an audio clip on the playlist").changed() && !self.rec_audio {
                    self.record_audio();
                }
                ui.separator();
                let live = tr.tempo();
                let mut tempo = if playing && song_mode && live > 0.0 { live } else { self.song.tempo };
                let tempo_box = ui.add(egui::DragValue::new(&mut tempo).range(10.0..=522.0).speed(0.2).fixed_decimals(3)).hint("Tempo (right-click for more)");
                if tempo_box.changed() {
                    self.song.tempo = tempo;
                    self.dirty = true;
                }
                let mut set_tempo = None;
                tempo_box.context_menu(|ui| {
                    if ui.button("Tap").hint("Click in time, four times or more").clicked() {
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
                            set_tempo = Some((60.0 * (self.taps.len() - 1) as f32 / span.max(0.01)).round());
                        }
                    }
                    ui.separator();
                    for (label, t) in [("Half", self.song.tempo / 2.0), ("Double", self.song.tempo * 2.0)] {
                        if ui.button(label).clicked() {
                            set_tempo = Some(t);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    for t in [80.0, 90.0, 100.0, 110.0, 120.0, 128.0, 130.0, 140.0, 150.0, 160.0, 174.0] {
                        if ui.button(format!("{t:.0}")).clicked() {
                            set_tempo = Some(t);
                            ui.close_menu();
                        }
                    }
                });
                if let Some(t) = set_tempo {
                    self.song.tempo = t.clamp(10.0, 522.0);
                    self.dirty = true;
                }
                ui.separator();
                // pattern picker
                let pat = tr.pattern();
                self.song.ensure_pattern(pat);
                let picker = egui::ComboBox::from_id_salt("pattern").width(150.0).selected_text(self.song.pattern_name(pat)).show_ui(ui, |ui| {
                    for i in 0..self.song.patterns.len() {
                        if ui.selectable_label(i == pat, self.song.pattern_name(i)).clicked() {
                            tr.set_pattern(i);
                        }
                    }
                });
                if picker.response.hovered() {
                    // the wheel over the picker steps through the patterns
                    let turn = ui.input(|i| i.raw_scroll_delta.y);
                    if turn > 0.0 && pat > 0 {
                        tr.set_pattern(pat - 1);
                    } else if turn < 0.0 && pat + 1 < self.song.patterns.len() {
                        tr.set_pattern(pat + 1);
                    }
                }
                if ui.button("+").hint("New pattern (F4)").clicked() {
                    self.new_pattern();
                }
                let mut met = tr.metronome();
                if ui.toggle_value(&mut met, "Metro").hint("Metronome (Ctrl+M)").changed() {
                    tr.set_metronome(met);
                }
                let mut wait = tr.waiting();
                if ui.toggle_value(&mut wait, "Wait").hint("Wait for input before the song starts (Ctrl+I)").changed() {
                    tr.set_waiting(wait);
                }
                let mut count = tr.countdown();
                if ui.toggle_value(&mut count, "3 2 1").hint("Count one bar in before recording (Ctrl+P)").changed() {
                    tr.set_countdown(count);
                }
                ui.toggle_value(&mut self.rec_blend, "Blend").hint("Recorded notes are added to the ones already there (Ctrl+B)");
                let mut step_edit = tr.step_edit();
                if ui.toggle_value(&mut step_edit, "Step").hint("Step editing: played notes are written one grid step after another (Ctrl+E)").changed() {
                    tr.set_step_edit(step_edit);
                }
                ui.toggle_value(&mut self.typing_keys, "Keys").hint("Typing keyboard to piano (Ctrl+T)");
                ui.separator();
                // time
                let tick = tr.tick() as u32;
                let step = self.song.step();
                // in the song the bar count follows the time signature changes
                let (from, first, sig) = if tr.song_mode() { self.song.sig_at(tick) } else { (0, 0, self.song.time_sig) };
                let bar = (self.song.ppq * 4 * sig.0.max(1) as u32 / sig.1.max(1) as u32).max(1);
                let tick = tick - from;
                let shown = if self.time_secs {
                    let secs = tr.tick() * 60.0 / (self.song.tempo.max(1.0) as f64 * self.song.ppq as f64);
                    format!("{:03}:{:02}:{:02}", (secs / 60.0) as u32, secs as u32 % 60, (secs.fract() * 100.0) as u32)
                } else {
                    format!("{:03}:{:02}:{:02}", first + tick / bar + 1, (tick % bar) / step + 1, tick % step)
                };
                let clock = ui.add(egui::Label::new(RichText::new(shown).monospace().size(18.0).color(GREEN)).sense(egui::Sense::click()));
                if clock.hint(if self.time_secs { "Minutes : seconds : hundredths (click for bars)" } else { "Bar : step : tick (click for minutes and seconds)" }).clicked() {
                    self.time_secs = !self.time_secs;
                }
                // where the song stands, as a slider to drag
                let total = if tr.song_mode() { self.song.length() } else { self.song.patterns.get(pat).map(|p| p.length(self.song.ppq * 4)).unwrap_or(0) }.max(1);
                let mut at = (tr.tick() as f32 / total as f32).clamp(0.0, 1.0);
                ui.spacing_mut().slider_width = 110.0;
                if ui.add(egui::Slider::new(&mut at, 0.0..=1.0).show_value(false)).hint("Song position").changed() {
                    tr.seek((at * total as f32) as u32);
                }
                ui.separator();
                let mut master = tr.master();
                ui.label(RichText::new("Master").color(TEXT_DIM));
                if knob(ui, &mut master, 0.0, 1.25, 1.0, 22.0).hint("Main volume").changed() {
                    tr.set_master(master);
                }
                let mut pitch = self.song.main_pitch as f32;
                if knob(ui, &mut pitch, -1200.0, 1200.0, 0.0, 22.0).hint(format!("Main pitch: {:+} cents", self.song.main_pitch)).changed() {
                    self.song.main_pitch = pitch.round() as i16;
                    self.dirty = true;
                }
                ui.separator();
                for (flag, label, tip) in [
                    (&mut self.show_playlist, "Playlist", "F5"),
                    (&mut self.show_piano, "Piano roll", "F7"),
                    (&mut self.show_rack, "Channel rack", "F6"),
                    (&mut self.show_mixer, "Mixer", "F9"),
                    (&mut self.show_browser, "Browser", "Alt+F8"),
                ] {
                    if ui.selectable_label(*flag, label).hint(tip).clicked() {
                        *flag = !*flag;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let load = CPU_LOAD.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10.0;
                    ui.label(RichText::new(format!("{} voices", tr.voices())).color(TEXT_DIM).small());
                    ui.label(RichText::new(format!("CPU {load:.0}%")).color(if load > 85.0 { egui::Color32::from_rgb(235, 70, 60) } else { TEXT_DIM }).small()).hint("Share of each block's time the sound engine needs");
                });
            });
            ui.add_space(2.0);
        });
    }
}

impl App {
    /// Project info, the pattern rename box and the about box.
    fn small_windows(&mut self, ctx: &egui::Context) {
        // hosted plug-ins get their turn on this thread: timers, callbacks, and word of settings changed in their own windows
        let hosted = self.song.hosted();
        if !hosted.is_empty() {
            for h in &hosted {
                h.idle();
                if h.take_dirty() {
                    self.unsaved = true;
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        self.settings_window(ctx);
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
                if let (Some(c), Some(p)) = (crate::widgets::walk_colour(ui), self.song.patterns.get_mut(pi)) {
                    p.color = Some(c);
                    self.dirty = true;
                }
                crate::widgets::name_field(ui, &mut name);
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
        if let Some(ask) = self.state_ask.take() {
            self.state_file(ask);
        }
        let mut open = self.show_about;
        egui::Window::new("About").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("Breakfast Cereal DAW");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("A free, open source music workstation for Linux and Windows.");
            ui.label(RichText::new(&self.audio).color(TEXT_DIM).small());
        });
        self.show_about = open;
        self.script_ask(ctx);
    }

    /// A project's note script is somebody's program: it runs only once the user has said this very text may.
    fn script_ask(&mut self, ctx: &egui::Context) {
        let Some(source) = cereal_engine::script::waiting().into_iter().find(|s| !self.script_later.contains(s)) else { return };
        let title = cereal_engine::script::ScriptDef { source: source.clone(), params: String::new() }.title();
        let mut run = false;
        let mut later = false;
        egui::Window::new("Note script").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(if title.is_empty() { "This project carries a note script.".to_string() } else { format!("This project carries a note script: {title}") });
            ui.label("A note script is a small Python program that plays notes. It runs with your own rights on this machine, like any program you start. Let it run only if you trust where the project came from. Until then its notes pass through unchanged.");
            ui.collapsing("Read the script", |ui| {
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&source).monospace().small()).wrap());
                });
            });
            ui.horizontal(|ui| {
                run = ui.button("Let it run").clicked();
                later = ui.button("Not now").clicked();
            });
        });
        if run {
            setup::trust_script(&source);
        } else if later {
            self.script_later.push(source);
        }
    }
}

/// Choices in the rendering window.
struct RenderOpts {
    flac: bool,
    /// 0 for the lossless formats above, 1 MP3, 2 Ogg Vorbis.
    lossy: u8,
    kbps: u32,
    quality: f32,
    bits: u16,
    rate: u32,
    tail: f32,
    song: bool,
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(s) = self.script.as_mut().filter(|s| s.pending()) {
            s.feed(raw);
            ctx.request_repaint();
        }
        self.take_wheel(raw);
        self.wheel_over = None;
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // a stretch of time picked out on a ruler plays round and round: the playlist's in song mode, the piano roll's otherwise
        let tr = &self.handle.transport;
        tr.set_range(if tr.song_mode() { self.playlist.time_sel } else { self.piano.time_sel });
        if !std::mem::replace(&mut self.prefs_win.scaled, true) && self.settings.scale > 0.0 {
            self.apply_scale(ctx);
        }
        if !self.setup.open {
            self.keys(ctx);
        }
        self.midi.chan.store(self.sel_chan, std::sync::atomic::Ordering::Relaxed);
        // what a control's right-click menu asked for last frame
        let ask_id = egui::Id::new("ctl_ask");
        if let Some((target, ask)) = ctx.data_mut(|d| d.remove_temp::<Option<(cereal_engine::song::Target, widgets::CtlAsk)>>(ask_id)).flatten() {
            match ask {
                widgets::CtlAsk::AutoClip => self.make_auto_clip(target),
                widgets::CtlAsk::Events => {
                    self.events.target = Some(self.song.event_address(target));
                    self.events.wanted = self.events.target;
                    self.show_events = true;
                }
                widgets::CtlAsk::Link => self.link_learn = Some(target),
            }
        }
        self.controllers_ui(ctx);
        self.setup_ui(ctx);
        // files dropped on the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            if p.extension().map(|e| e.eq_ignore_ascii_case("flp") || e.eq_ignore_ascii_case("zip")).unwrap_or(false) {
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
        self.keep_inputs();
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
        // a press anywhere inside one of the four big windows gives it the keys
        let press = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None });
        let took = |r: &egui::Response| press.is_some_and(|p| r.rect.contains(p) && ctx.layer_id_at(p) == Some(r.layer_id));
        self.raise_window(ctx);
        let mut open = self.show_playlist;
        let shown = if self.pl_max && open {
            // Enter lets the playlist fill the work area; its own window keeps its place for afterwards
            let size = [(screen.width() - left - 8.0).max(200.0), (screen.height() - top - 44.0).max(120.0)];
            egui::Window::new("Playlist").id(egui::Id::new("playlist_max")).open(&mut open).frame(frame).fixed_pos([left, top - 44.0]).fixed_size(size).show(ctx, |ui| {
                ui.set_min_size(size.into());
                self.playlist_ui(ui)
            })
        } else {
            egui::Window::new("Playlist").open(&mut open).frame(frame).default_pos([left, top]).default_size([wide, upper]).show(ctx, |ui| self.playlist_ui(ui))
        };
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Playlist;
        }
        self.show_playlist = open;
        let mut open = self.show_rack;
        let shown = egui::Window::new("Channel rack").open(&mut open).frame(frame).default_pos([left, low_y]).default_size([rack_w, low_h]).show(ctx, |ui| self.rack_ui(ui));
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Rack;
        }
        self.show_rack = open;
        let mut open = self.show_piano;
        let shown = egui::Window::new("Piano roll").open(&mut open).frame(frame).default_pos([left + 24.0, top + 24.0]).default_size([wide - 24.0, upper + 60.0]).show(ctx, |ui| self.piano_ui(ui));
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Piano;
        }
        self.show_piano = open;
        let mut open = self.show_mixer;
        let shown = egui::Window::new("Mixer").open(&mut open).frame(frame).default_pos([left + rack_w + 28.0, low_y]).default_size([(screen.width() - left - rack_w - 44.0).max(400.0), low_h]).show(ctx, |ui| self.mixer_ui(ui));
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Mixer;
        }
        self.show_mixer = open;
        self.picker_ui(ctx);
        self.project_ui(ctx);
        if let Some((i, mut name)) = self.rename_insert.take() {
            let mut keep = true;
            egui::Window::new("Rename mixer track").collapsible(false).resizable(false).show(ctx, |ui| {
                if let (Some(c), Some(ins)) = (crate::widgets::walk_colour(ui), self.song.inserts.get_mut(i)) {
                    ins.color = Some(c);
                    self.dirty = true;
                }
                crate::widgets::name_field(ui, &mut name);
                if ui.input(|x| x.key_pressed(Key::Enter)) {
                    if let Some(ins) = self.song.inserts.get_mut(i) {
                        ins.name = name.clone();
                    }
                    self.dirty = true;
                    keep = false;
                }
                if ui.input(|x| x.key_pressed(Key::Escape)) {
                    keep = false;
                }
            });
            if keep {
                self.rename_insert = Some((i, name));
            }
        }
        if let Some(at) = self.tool_menu {
            // the tools of the window that has the focus, at the pointer
            let at = if at == egui::Pos2::ZERO { ctx.pointer_latest_pos().unwrap_or(screen.center()) } else { at };
            self.tool_menu = Some(at);
            let piano = self.focus == Win::Piano;
            let names = if piano { App::pr_tool_names() } else { App::pl_tool_names() };
            let mut pick = None;
            let shown = egui::Window::new("Tools").id(egui::Id::new("tool_menu")).title_bar(false).resizable(false).fixed_pos(at).show(ctx, |ui| {
                ui.label(RichText::new(if piano { "Piano roll tool" } else { "Playlist tool" }).color(TEXT_DIM));
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(false, *n).clicked() {
                        pick = Some(i);
                    }
                }
            });
            let outside = shown.map(|r| ctx.input(|x| x.pointer.any_pressed() && x.pointer.interact_pos().is_some_and(|p| !r.response.rect.contains(p)))).unwrap_or(false);
            if let Some(i) = pick {
                if piano {
                    self.pr_pick_tool(i);
                } else {
                    self.pl_pick_tool(i);
                }
            }
            if pick.is_some() || outside || ctx.input(|x| x.key_pressed(Key::Escape)) {
                self.tool_menu = None;
            }
        }
        if self.grid_win {
            let mut open = true;
            egui::Window::new("Grid colour").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                let mut c = self.settings.grid.unwrap_or([GRID_DARK.r(), GRID_DARK.g(), GRID_DARK.b()]);
                ui.horizontal(|ui| {
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        self.settings.grid = Some(c);
                        let _ = self.settings.save();
                    }
                    if ui.button("Default").clicked() {
                        self.settings.grid = None;
                        let _ = self.settings.save();
                    }
                });
            });
            self.grid_win = open;
        }
        if let Some((i, mut name)) = self.rename_marker.take() {
            let mut keep = i < self.song.markers.len();
            egui::Window::new("Time marker").collapsible(false).resizable(false).show(ctx, |ui| {
                crate::widgets::name_field(ui, &mut name);
                if let Some(m) = self.song.markers.get_mut(i) {
                    const ACTIONS: [(u8, &str); 9] = [(0, "None"), (1, "Marker loop"), (2, "Marker skip"), (3, "Pause"), (4, "Loop"), (5, "Start"), (6, "Punch in recording"), (7, "Punch out recording"), (8, "Time signature")];
                    let now = ACTIONS.iter().find(|a| a.0 == m.kind).map(|a| a.1).unwrap_or("None");
                    ui.horizontal(|ui| {
                        ui.label("Action");
                        egui::ComboBox::from_id_salt("marker_action").selected_text(now).show_ui(ui, |ui| {
                            for (k, label) in ACTIONS {
                                if ui.selectable_label(m.kind == k, label).clicked() && m.kind != k {
                                    m.kind = k;
                                    if k == 8 && (m.sig.0 == 0 || m.sig.1 == 0) {
                                        m.sig = (4, 4);
                                    }
                                    self.dirty = true;
                                }
                            }
                        });
                    });
                }
                if let Some(m) = self.song.markers.get_mut(i).filter(|m| m.kind == 8) {
                    // a time signature change: beats in the bar over the note that counts as one beat
                    ui.horizontal(|ui| {
                        ui.label("Time signature");
                        let mut changed = ui.add(egui::DragValue::new(&mut m.sig.0).range(1..=16)).changed();
                        ui.label("/");
                        egui::ComboBox::from_id_salt("sig_beat").width(40.0).selected_text(m.sig.1.to_string()).show_ui(ui, |ui| {
                            for d in [2u8, 4, 8, 16] {
                                changed |= ui.selectable_value(&mut m.sig.1, d, d.to_string()).changed();
                            }
                        });
                        if changed {
                            self.dirty = true;
                        }
                    });
                }
                let enter = ui.input(|x| x.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        if let Some(m) = self.song.markers.get_mut(i) {
                            m.name = name.clone();
                        }
                        self.dirty = true;
                        keep = false;
                    }
                    if ui.button("Delete").clicked() && i < self.song.markers.len() {
                        self.song.markers.remove(i);
                        self.dirty = true;
                        keep = false;
                    }
                });
                if ui.input(|x| x.key_pressed(Key::Escape)) {
                    keep = false;
                }
            });
            if keep {
                self.rename_marker = Some((i, name));
            }
        }
        let mut open = self.show_chanset;
        let title = self.song.channels.get(self.sel_chan).map(|c| c.name.clone()).unwrap_or_default();
        egui::Window::new(format!("Channel settings - {title}")).id(egui::Id::new("chanset")).open(&mut open).frame(frame).default_pos([screen.width() - right_col, top]).default_width(420.0).resizable(false).show(ctx, |ui| self.chanset_ui(ui));
        self.show_chanset = open;
        let mut open = self.show_events;
        let shown = egui::Window::new("Event editor").open(&mut open).frame(frame).default_pos([left + 60.0, top + 80.0]).default_size([760.0, 260.0]).show(ctx, |ui| self.events_ui(ui));
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Events;
        }
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
            midi::PPQ.store(self.song.ppq.max(1) as usize, std::sync::atomic::Ordering::Relaxed);
            self.song.activate_hosted(OUT_RATE.load(std::sync::atomic::Ordering::Relaxed) as f32);
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
