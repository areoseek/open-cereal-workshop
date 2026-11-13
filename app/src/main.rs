//! Open Cereal Workshop: the application window. Toolbar and transport on top, browser on the left, and the
//! channel rack, piano roll, playlist and mixer as windows on the workspace.

// no console window behind the program on Windows
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
mod help;
mod stems;
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
    /// The toolbar's output picture: the wave, or the level of each frequency.
    scope_bars: bool,
    /// Toolbar parts taken off by the VIEW menu: the window buttons, the output monitor.
    /// The toolbar panels show their move and hide buttons.
    bar_edit: bool,
    /// Minutes of hand-played keys the score log hands over.
    score_minutes: u32,
    scope_buf: Vec<f32>,
    typing_keys: bool,
    /// Snap hand-played notes to the piano roll's grid as they are recorded.
    rec_quantise: bool,
    /// The window key commands go to.
    pub(crate) focus: Win,
    /// A window to bring in front of the others on the next frame.
    raise: Option<Win>,
    /// Counts the layouts put on show; see `win_id`.
    layout_gen: u32,
    layout: Option<[f32; 16]>,
    /// Where the four big windows are now: left, top, inner width and height each.
    win_now: [f32; 16],
    layout_name: String,
    backdrop_tex: Option<(std::path::PathBuf, Option<egui::TextureHandle>)>,
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
    /// When unsaved work was last put in the backup folder (or there was none to put).
    auto_at: std::time::Instant,
    /// Something that would throw unsaved work away is waiting for an answer.
    ask_leave: Option<Leave>,
    quit_ok: bool,
    /// The notes this recording pass wrote, which a pass that does not blend must not wipe again.
    rec_new: Vec<(u32, u16, u8)>,
    /// Where step editing writes next, and how many keys are down.
    step_at: Option<u32>,
    step_down: usize,
    rename_insert: Option<(usize, String)>,
    rename_marker: Option<(usize, String)>,
    /// The tool list that F3 opens at the pointer (a zero position is filled in when it is first shown).
    tool_menu: Option<egui::Pos2>,
    pub(crate) stems: stems::State,
    pub(crate) grid_win: bool,
    pub(crate) setup: setup::State,
    pub(crate) settings: setup::Settings,
    sel_chan: usize,
    /// MIDI keyboards and pads.
    midi: midi::MidiIn,
    sel_insert: usize,
    /// The channel that was picked a frame ago.
    sel_chan_was: usize,
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
    peak_top: Vec<f32>,
    route_only: bool,
    route_side: bool,
    held: HashMap<Key, (u16, u8)>,
    actions: Vec<Action>,
    /// Undo history: snapshots of the song, oldest first; `hist_at` is the one on screen.
    history: Vec<Song>,
    hist_at: usize,
    /// An edit is under way (mouse still down); it becomes one history step when it ends.
    hist_pending: bool,
    show_info: bool,
    /// The load monitor: its window, the rows shown (name, share of real time) and when they were last counted.
    show_load: bool,
    show_touch: bool,
    show_script_out: bool,
    script_out: String,
    /// Inputs tied to one channel whatever is selected: the device name ("" = the typing keyboard), the channel.
    recv: Vec<(String, usize)>,
    touch_held: Option<(u16, u8)>,
    speech_dlg: Option<cereal_engine::speech::Voice>,
    load_rows: Vec<(String, f32)>,
    load_at: Option<std::time::Instant>,
    pub(crate) prefs_win: settings::State,
    show_about: bool,
    help: help::State,
    /// What did not come across when the project was opened, shown once.
    problems: Vec<String>,
    /// The regions found in one audio channel's file, kept while its clip menu is used.
    regions: Option<(usize, String, Vec<cereal_engine::slicer::Region>)>,
    /// Note scripts the user said "not now" to since the program started.
    script_later: Vec<String>,
    render: Option<RenderOpts>,
    rec_held: Vec<(u16, u8, f64, f32)>,
    /// Record sound from the input device too while recording runs.
    rec_audio: bool,
    /// The recording filter: whether hand-turned controls and played notes are taken down.
    rec_automation: bool,
    rec_score: bool,
    /// Where each control being recorded was written last: (pattern, tick), keyed by its address.
    rec_ctl: std::collections::HashMap<(u16, u16), (usize, u32)>,
    audio_rec: Option<record::AudioRec>,
    /// Loop recording: every pass of a looping song becomes a take of its own, the earlier ones muted.
    rec_loop: bool,
    rec_last_tick: f64,
    /// The sound card inputs, open while a mixer track listens to one; the flag stops retries after a failure.
    live_in: Option<record::LiveIn>,
    live_in_failed: bool,
    save_preset: Option<(String, bool)>,
    /// A pick from a plug-in window's options menu: what, the unit, instrument or effect.
    plug_ask: Option<(u8, String, bool)>,
    plug_clip: Option<(String, bool, Vec<u8>)>,
    /// The parameter list window: instrument or effect.
    param_win: Option<bool>,
    pub(crate) state_ask: Option<StateAsk>,
    show_chanset: bool,
    show_events: bool,
    events: events::State,
    /// The control waiting for a knob of the MIDI device to be moved, while its link window is open.
    link_learn: Option<cereal_engine::song::Target>,
    /// Multilink to controllers: the controls turned since it was switched on, in order, each waiting for
    /// the next knob of the MIDI device that has not been given one yet.
    multilink: bool,
    /// The big windows that live in an operating system window of their own (playlist, rack, piano roll, mixer).
    pub(crate) detached: [bool; 4],
    multi_queue: Vec<cereal_engine::song::Target>,
    multi_seen: Vec<u8>,
    /// A control's place copied from its menu, 0 to 1.
    value_clip: Option<f32>,
    /// The control a value is being typed for, the text so far, and whether the box still wants the focus.
    type_in: Option<(cereal_engine::song::Target, String, bool)>,
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
        let detached_bits = settings.detached;
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
        if let Some(d) = setup::backup_dir() {
            browser.add_root("Backup", d);
        }
        if let Some(d) = setup::starter_dir() {
            browser.add_root("Starter sounds", d);
        }
        let midi = midi::MidiIn::start(handle.sender());
        midi::start_out(handle.transport.clone());
        handle.transport.set_click_kind(settings.click_kind);
        handle.transport.set_count_bars(settings.count_bars.max(1));
        cereal_engine::sample::set_quality(settings.resample.unwrap_or(1));
        handle.transport.set_late_notes(settings.late_notes.unwrap_or(true));
        handle.transport.set_reset_fx(settings.opt(setup::OPT_RESET_FX));
        handle.transport.set_preview_insert(settings.preview_track as usize);
        if settings.gm_bank.is_some() {
            cereal_engine::gm::set_bank(settings.gm_bank.clone());
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
            scope_bars: false,
            bar_edit: false,
            score_minutes: 2,
            scope_buf: Vec::new(),
            typing_keys: true,
            rec_quantise: false,
            focus: Win::None,
            raise: None,
            layout_gen: 0,
            layout: None,
            win_now: [0.0; 16],
            layout_name: String::new(),
            backdrop_tex: None,
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
            auto_at: std::time::Instant::now(),
            ask_leave: None,
            quit_ok: false,
            rec_new: Vec::new(),
            step_at: None,
            step_down: 0,
            rename_insert: None,
            rename_marker: None,
            tool_menu: None,
            stems: Default::default(),
            grid_win: false,
            setup: Default::default(),
            settings,
            sel_chan: 0,
            midi,
            sel_insert: 0,
            sel_chan_was: 0,
            mixer_sel: Default::default(),
            rack: Default::default(),
            piano: Default::default(),
            playlist: Default::default(),
            browser,
            peaks: vec![],
            mixer_wave: false,
            peak_hist: vec![],
            peak_top: vec![],
            route_only: false,
            route_side: false,
            held: HashMap::new(),
            actions: vec![],
            history: vec![],
            hist_at: 0,
            hist_pending: false,
            show_info: false,
            show_load: false,
            show_touch: false,
            show_script_out: false,
            script_out: String::new(),
            recv: Vec::new(),
            touch_held: None,
            speech_dlg: None,
            load_rows: vec![],
            load_at: None,
            prefs_win: Default::default(),
            show_about: false,
            help: Default::default(),
            problems: Vec::new(),
            regions: None,
            script_later: Vec::new(),
            render: None,
            rec_held: Vec::new(),
            rec_audio: false,
            rec_automation: true,
            rec_score: true,
            rec_ctl: Default::default(),
            audio_rec: None,
            rec_loop: false,
            rec_last_tick: 0.0,
            live_in: None,
            live_in_failed: false,
            save_preset: None,
            plug_ask: None,
            plug_clip: None,
            param_win: None,
            state_ask: None,
            show_chanset: false,
            show_events: false,
            events: events::State::default(),
            link_learn: None,
            multilink: false,
            detached: std::array::from_fn(|k| detached_bits >> k & 1 == 1),
            multi_queue: vec![],
            multi_seen: vec![],
            value_clip: None,
            type_in: None,
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
        app.starter_kit();
        app.unsaved = false;
        app.setup.open = roots.is_empty() && !app.settings.never_ask && std::env::var_os("CEREAL_NO_SETUP").is_none();
        if let Some(p) = open {
            // a sound handed over on its own lands on the playlist of a new project
            let sound = p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["wav", "flac", "ogg", "mp3", "aif", "aiff", "wv"].iter().any(|k| e.eq_ignore_ascii_case(k)));
            if sound {
                app.add_audio_clip(&p, 0, None);
                app.dirty = false;
            } else {
                app.open(&p);
            }
        }
        // presets to load at start, for screenshots and tests
        if let Ok(v) = std::env::var("CEREAL_PRESET") {
            for p in std::env::split_paths(&v) {
                app.actions.push(Action::LoadPreset(p, PresetTo::Auto));
            }
        }
        // windows to show at start, for screenshots and tests: CEREAL_SHOW=mixer,piano,channel
        if let Ok(p) = std::env::var("CEREAL_WAVE") {
            // a sound straight into the wave recorder of the master track's first slot, for trying it out
            let _ = app.wave_eds.entry((0, 0)).or_default().open(std::path::Path::new(&p));
        }
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
                    w if w.starts_with("chan=") => {
                        // "chan=slicer" picks the first loop slicer; a number picks that channel
                        let n = w[5..].parse().ok().or_else(|| app.song.channels.iter().position(|c| c.slices.is_some()));
                        app.sel_chan = n.unwrap_or(0).min(app.song.channels.len().saturating_sub(1));
                        app.show_chanset = true;
                    }
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
                self.finish_take(r, false);
            }
        } else if self.rec_loop && tr.song_mode() && tr.tick() + (self.song.ppq as f64) < self.rec_last_tick {
            // the song went round: this pass is a take, kept silent while the next one is played
            if let Some(r) = self.audio_rec.take() {
                self.finish_take(r, true);
            }
            if let Ok(r) = record::AudioRec::start(tr.tick().max(0.0) as u32, tr.clone()) {
                self.audio_rec = Some(r);
            }
        }
        self.rec_last_tick = tr.tick();
    }

    fn finish_take(&mut self, r: record::AudioRec, muted: bool) {
        let tr = self.handle.transport.clone();
        let tps = (if tr.tempo() > 0.0 { tr.tempo() } else { self.song.tempo }) as f64 * self.song.ppq as f64 / 60.0;
        let (at, cut) = r.placed(tr.song_mode(), OUT_LAG_US.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6, tps);
        match r.finish(&record::folder(self.path.as_deref()), cut) {
            Ok(path) => {
                let before = self.song.channels.len();
                self.add_audio_clip(&path, at, None);
                if muted && self.song.channels.len() > before {
                    let src = cereal_engine::song::ClipSrc::Audio(before);
                    for c in self.song.clips.iter_mut().filter(|c| c.src == src) {
                        c.muted = true;
                    }
                }
                self.status = format!("Recorded {}", path.display());
            }
            Err(e) => self.status = e,
        }
    }

    /// Controls turned by hand while recording runs are written into the pattern as events, over whatever
    /// the stretch they pass had before. In song mode they go into the clip of the current pattern under the
    /// play position, or into a pattern of their own that spans the song when there is none.
    fn record_controls(&mut self, moved: Vec<cereal_engine::song::Target>) {
        use cereal_engine::song::{Clip, ClipSrc, PatEvent};
        let tr = self.handle.transport.clone();
        if !tr.playing() || !tr.recording() || !self.rec_automation {
            self.rec_ctl.clear();
            return;
        }
        if moved.is_empty() {
            return;
        }
        let now = tr.tick().max(0.0);
        let mut pat = tr.pattern();
        let mut tick = now as u32;
        if tr.song_mode() {
            let under = self.song.clips.iter().find(|c| c.src == ClipSrc::Pattern(pat) && now >= c.pos as f64 && now < (c.pos + c.len) as f64).map(|c| (c.pos, c.offset));
            match under {
                Some((pos, offset)) => tick = tick - pos + offset,
                None => {
                    const NAME: &str = "Main automation";
                    pat = match self.song.patterns.iter().position(|p| p.name == NAME) {
                        Some(p) => p,
                        None => {
                            let p = self.song.next_empty_pattern();
                            self.song.ensure_pattern(p);
                            self.song.patterns[p].name = NAME.into();
                            p
                        }
                    };
                    let end = (self.song.length().max(tick) / self.song.bar().max(1) + 1) * self.song.bar().max(1);
                    match self.song.clips.iter_mut().find(|c| c.src == ClipSrc::Pattern(pat) && c.pos == 0) {
                        Some(c) => c.len = c.len.max(end),
                        None => {
                            let used: std::collections::BTreeSet<u16> = self.song.clips.iter().map(|c| c.track).collect();
                            let track = (0..self.song.tracks.len() as u16).find(|t| !used.contains(t)).unwrap_or(0);
                            self.song.clips.push(Clip { pos: 0, len: end, track, src: ClipSrc::Pattern(pat), offset: 0, muted: false, group: 0 });
                        }
                    }
                }
            }
        }
        self.song.ensure_pattern(pat);
        for t in moved {
            let (param, dest) = self.song.event_address(t);
            let Some(raw) = self.song.event_raw(t) else { continue };
            let from = match self.rec_ctl.insert((param, dest), (pat, tick)) {
                Some((p, last)) if p == pat && last <= tick => last,
                _ => tick,
            };
            let ev = &mut self.song.patterns[pat].events;
            ev.retain(|e| (e.param, e.dest) != (param, dest) || e.tick < from || e.tick > tick || (e.tick == from && from != tick));
            let at = ev.partition_point(|e| e.tick <= tick);
            ev.insert(at, PatEvent { tick, param, dest, raw });
            self.dirty = true;
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
        if !self.rec_score {
            evs.clear();
        }
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
        self.starter_kit();
        self.dirty = true;
        self.fresh = true;
        self.unsaved = false;
        self.status = "New project".into();
    }

    /// Put our own drum sounds on the four channels an empty project starts with, so it makes sound at once.
    fn starter_kit(&mut self) {
        let Some(dir) = setup::starter_dir() else { return };
        for (i, name) in ["Kick", "Clap", "Hat", "Snare"].iter().enumerate() {
            let empty = self.song.channels.get(i).is_some_and(|c| c.kind == ChannelKind::Sampler && c.sample_path.is_empty());
            let path = dir.join(format!("{name}.wav"));
            if empty && path.is_file() {
                self.load_sample(&path, Some(i));
            }
        }
        self.sel_chan = 0;
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
                self.problems = cereal_engine::problems(&song, &f, &self.lib);
                self.handle.transport.set_pattern((f.current_pattern as usize).saturating_sub(1).min(song.patterns.len().saturating_sub(1)));
                self.handle.transport.set_song_mode(song.song_mode);
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
        // the file about to be written over is kept in the backup folder first
        if self.settings.autosave.unwrap_or(4) > 0 && path.is_file() {
            if let Some(dir) = setup::backup_dir() {
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(&path, dir.join(format!("{stem} (overwritten at {}).flp", Self::clock_stamp()))));
                Self::thin_backups(&dir);
                self.browser.add_root("Backup", dir);
            }
        }
        self.auto_at = std::time::Instant::now();
        self.song.song_mode = self.handle.transport.song_mode();
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

    /// Do something that replaces the project, asking first when there is unsaved work.
    fn leave(&mut self, what: Leave) {
        if self.unsaved {
            self.ask_leave = Some(what);
        } else {
            self.leave_now(what);
        }
    }

    fn leave_now(&mut self, what: Leave) {
        match what {
            Leave::Close => self.quit_ok = true,
            Leave::New => self.new_project(),
            Leave::OpenDialog => self.open_dialog(),
            Leave::Open(p) => self.open(&p),
            Leave::Template(p) => {
                self.open(&p);
                // a template is a start, not the file being worked on
                self.path = None;
                self.status = "New project from a template".into();
            }
        }
    }

    /// The question asked before unsaved work is thrown away, and the window's close button.
    fn leave_window(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_ok && self.unsaved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ask_leave = Some(Leave::Close);
        }
        if self.quit_ok {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let Some(what) = self.ask_leave.take() else { return };
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "this project".into());
        let mut answer = None;
        egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(format!("Save changes to {name}?"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Yes").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    answer = Some(0);
                }
                if ui.button("No").clicked() {
                    answer = Some(1);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(2);
                }
            });
        });
        match answer {
            Some(0) => {
                self.save(false);
                // a save that did not go through (the file box was closed) calls the whole thing off
                if !self.unsaved {
                    self.leave_now(what);
                }
            }
            Some(1) => {
                self.unsaved = false;
                self.leave_now(what);
            }
            Some(_) => {}
            None => self.ask_leave = Some(what),
        }
    }

    /// The date and time as text for a file name (UTC).
    fn clock_stamp() -> String {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        let (days, rem) = (s.div_euclid(86_400), s.rem_euclid(86_400));
        // days since 1970 to a calendar date
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        format!("{y:04}-{m:02}-{d:02} {:02}h{:02}", rem / 3600, rem % 3600 / 60)
    }

    /// Keep the newest forty project copies in the backup folder; older ones of ours go.
    fn thin_backups(dir: &Path) {
        let mut ours: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let p = e.path();
                        let name = p.file_name()?.to_string_lossy().to_string();
                        let is_ours = name.ends_with(".flp") && (name.contains(" (overwritten at ") || name.contains(" (autosaved"));
                        is_ours.then(|| e.metadata().and_then(|m| m.modified()).ok().map(|t| (t, p))).flatten()
                    })
                    .collect()
            })
            .unwrap_or_default();
        ours.sort();
        while ours.len() > 40 {
            let _ = std::fs::remove_file(ours.remove(0).1);
        }
    }

    /// Put unsaved work in the backup folder when its time has come. Nothing is written while the song plays.
    fn autosave_tick(&mut self, ctx: &egui::Context) {
        let mins = match self.settings.autosave.unwrap_or(4) {
            2 => 15,
            3 => 10,
            4 => 5,
            _ => return,
        };
        if !self.unsaved {
            self.auto_at = std::time::Instant::now();
            return;
        }
        let due = std::time::Duration::from_secs(std::env::var("CEREAL_AUTOSAVE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(mins * 60));
        let waited = self.auto_at.elapsed();
        if waited < due {
            ctx.request_repaint_after(due - waited);
            return;
        }
        if self.handle.transport.playing() {
            return;
        }
        self.auto_at = std::time::Instant::now();
        let Some(dir) = setup::backup_dir() else { return };
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        let stem = stem.split(" (autosaved").next().unwrap_or(&stem).split(" (overwritten at ").next().unwrap_or(&stem).to_string();
        let mut copy = self.song.clone();
        let made = cereal_engine::save::save(&mut copy, self.flp.as_ref()).and_then(|(bytes, _)| {
            std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(format!("{stem} (autosaved {}).flp", Self::clock_stamp())), bytes)).map_err(|e| e.to_string())
        });
        match made {
            Ok(()) => {
                Self::thin_backups(&dir);
                self.browser.add_root("Backup", dir);
            }
            Err(e) => self.status = format!("Autosave failed: {e}"),
        }
    }

    /// The newest project copy in the backup folder.
    fn last_backup() -> Option<PathBuf> {
        let dir = setup::backup_dir()?;
        std::fs::read_dir(dir).ok()?.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "flp")).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).max().map(|t| t.1)
    }

    /// Ctrl+R opens the rendering window; its Start button asks where to save and writes the file.
    fn export(&mut self) {
        if self.render.is_none() {
            self.render = Some(RenderOpts { flac: false, lossy: 0, kbps: 192, quality: 6.0, bits: 16, rate: 44_100, tail: 6.0, song: self.handle.transport.song_mode(), tail_mode: 1, split: false, mono: false, range: true, marks: false, dither: false, tempo_info: false, no_inserts: false, no_master: false, resample: 3 });
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
        let range = if !o.range {
            None
        } else if o.song {
            self.playlist.time_sel
        } else {
            self.piano.time_sel
        };
        let job = cereal_engine::RenderJob { rate: o.rate as f32, song_mode: o.song, pattern: self.handle.transport.pattern(), tail: if o.tail_mode == 0 { 0.0 } else { o.tail }, range, stems: o.split, stem: None, no_inserts: o.no_inserts, no_master: o.no_master };
        let live = cereal_engine::sample::quality();
        cereal_engine::sample::set_quality(o.resample);
        let mut done = cereal_engine::render_job(&self.song, &job);
        cereal_engine::sample::set_quality(live);
        if o.tail_mode == 2 {
            done.wrap();
        }
        let chans: u16 = if o.mono { 1 } else { 2 };
        let shape = |v: &mut Vec<f32>| {
            if o.mono && o.lossy != 0 {
                // the lossy writers take two channels: both carry the merged sound
                for f in v.chunks_exact_mut(2) {
                    let m = (f[0] + f[1]) * 0.5;
                    f[0] = m;
                    f[1] = m;
                }
            } else if o.mono {
                *v = v.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
            }
            if o.dither && o.lossy == 0 && o.bits == 16 {
                // triangular noise one step wide hides the staircase of 16 bits
                let mut s = 0x2545_f491u32;
                let mut rnd = || {
                    s ^= s << 13;
                    s ^= s >> 17;
                    s ^= s << 5;
                    s as f32 / u32::MAX as f32
                };
                for x in v.iter_mut() {
                    *x += (rnd() - rnd()) / 32768.0;
                }
            }
        };
        // every sounding mixer track gets a file of its own beside the main one
        if o.split && o.lossy == 0 {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            for (i, mut v) in std::mem::take(&mut done.stems) {
                if i == 0 {
                    continue;
                }
                shape(&mut v);
                let name: String = self.song.insert_name(i).chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
                let p = path.with_file_name(format!("{stem}_{name}.{ext}"));
                let r = std::fs::File::create(&p).and_then(|f| {
                    let w = std::io::BufWriter::new(f);
                    if o.flac {
                        cereal_engine::flac::write(w, &v, o.rate, chans, o.bits.min(24) as u32)
                    } else {
                        wav::write(w, &v, o.rate, chans, o.bits)
                    }
                });
                if let Err(e) = r {
                    self.status = format!("Export failed: {e}");
                    return;
                }
            }
        }
        let mut buf = done.mix;
        shape(&mut buf);
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
                cereal_engine::flac::write(w, &buf, o.rate, chans, o.bits.min(24) as u32)
            } else {
                wav::write(w, &buf, o.rate, chans, o.bits)
            }
        });
        if r.is_ok() && o.marks && o.song && !o.flac {
            // the time markers, as cue points a sound editor shows (placed by the song's tempo at its start)
            let from = range.map(|r| r.0).unwrap_or(0);
            let per_tick = 60.0 / self.song.tempo.max(1.0) as f64 / self.song.ppq.max(1) as f64 * o.rate as f64;
            let marks: Vec<cereal_engine::waveed::Mark> = self.song.markers.iter().filter(|m| m.pos >= from && range.is_none_or(|r| m.pos < r.1)).map(|m| cereal_engine::waveed::Mark { at: ((m.pos - from) as f64 * per_tick) as usize, len: 0, name: m.name.clone() }).collect();
            if let Ok(mut bytes) = std::fs::read(&path) {
                cereal_engine::waveed::write_marks(&mut bytes, &marks);
                let _ = std::fs::write(&path, bytes);
            }
        }
        if r.is_ok() && o.tempo_info && !o.flac {
            let frames = done.main;
            let tempo = self.song.tempo.max(1.0);
            let beats = (frames as f64 / o.rate as f64 * tempo as f64 / 60.0).round().max(1.0) as u32;
            if let Ok(mut bytes) = std::fs::read(&path) {
                cereal_engine::waveed::write_loop_info(&mut bytes, frames, o.rate, tempo, beats, (4, 4), o.tail_mode == 2);
                let _ = std::fs::write(&path, bytes);
            }
        }
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
                    if ch.plugin.eq_ignore_ascii_case(cereal_engine::dash::IDENT) {
                        if let Some(b) = ch.gen.as_ref().and_then(|g| cereal_engine::dash::Board::parse(&g.raw)) {
                            for ctl in b.controls.iter() {
                                if let Some(k) = ctl.out {
                                    senders.push((cereal_engine::song::CtlLink::CHAN, c as u16, k as u16, format!("{} (channel {}): {}", ch.name, c + 1, ctl.name)));
                                }
                            }
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
        if self.multilink {
            for &(cc, _) in &moves {
                if self.multi_seen.contains(&cc) || self.multi_queue.is_empty() {
                    continue;
                }
                let t = self.multi_queue.remove(0);
                self.multi_seen.push(cc);
                self.song.midi_links.retain(|l| l.target != t);
                self.song.midi_links.push(cereal_engine::song::MidiLink { ctrl: cc as u16, port: 0, target: t });
                self.status = format!("Controller {cc} linked to {}", self.song.target_name(t));
                self.dirty = true;
            }
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
                ui.label("Resampling");
                egui::ComboBox::from_id_salt("render_resample").width(140.0).selected_text(cereal_engine::sample::QUALITIES[o.resample.min(3) as usize]).show_ui(ui, |ui| {
                    for (k, n) in cereal_engine::sample::QUALITIES.iter().enumerate() {
                        ui.selectable_value(&mut o.resample, k as u8, *n);
                    }
                });
                ui.end_row();
                ui.label("Tail");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut o.tail_mode, 0, "Cut remainder").hint("Stop dead at the end");
                    ui.selectable_value(&mut o.tail_mode, 1, "Leave remainder").hint("Let echoes and releases ring out after the end");
                    ui.selectable_value(&mut o.tail_mode, 2, "Wrap remainder").hint("Lay what rings past the end over the start, for a loop without a gap");
                });
                ui.end_row();
                if o.tail_mode != 0 {
                    ui.label("Longest tail");
                    ui.add(egui::Slider::new(&mut o.tail, 0.0..=20.0).suffix(" s"));
                    ui.end_row();
                }
                ui.label("Channels");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut o.mono, false, "Stereo");
                    ui.selectable_value(&mut o.mono, true, "Mono").hint("Left and right merged");
                });
                ui.end_row();
                ui.label("Options");
                ui.vertical(|ui| {
                    let picked = if o.song { self.playlist.time_sel.is_some() } else { self.piano.time_sel.is_some() };
                    ui.add_enabled(picked, egui::Checkbox::new(&mut o.range, "Only the time selection")).hint("Render just the stretch picked on the ruler");
                    ui.add_enabled(o.lossy == 0, egui::Checkbox::new(&mut o.split, "Split mixer tracks")).hint("Also write one file for every mixer track that sounds, named after the track");
                    ui.add_enabled(o.lossy == 0 && !o.flac && o.song, egui::Checkbox::new(&mut o.marks, "Save playlist markers")).hint("Time markers go into the WAV file as cue points");
                    ui.add_enabled(o.lossy == 0 && o.bits == 16, egui::Checkbox::new(&mut o.dither, "Dithering")).hint("A whisper of noise that hides the steps of 16 bits");
                    ui.add_enabled(o.lossy == 0 && !o.flac, egui::Checkbox::new(&mut o.tempo_info, "Save tempo information")).hint("Tempo and beat count go into the WAV file; with the tail wrapped it is marked as a loop");
                    let mut ins = !o.no_inserts;
                    ui.checkbox(&mut ins, "Enable insert effects").hint("Off: the mixer tracks' effect slots are left out");
                    o.no_inserts = !ins;
                    let mut mas = !o.no_master;
                    ui.checkbox(&mut mas, "Enable master effects").hint("Off: the master track's effect slots are left out");
                    o.no_master = !mas;
                });
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
    /// The audio logger: the last two minutes of output, silence at its ends taken off, saved beside the
    /// project and laid on the playlist.
    fn dump_audio_log(&mut self) {
        let (mut log, rate) = self.handle.transport.audio_log(120.0);
        let loud = |f: &[f32]| f.iter().any(|v| v.abs() > 0.0005);
        let last = log.chunks_exact(2).rposition(loud).map(|i| i + 1).unwrap_or(0);
        log.truncate(last * 2);
        let first = log.chunks_exact(2).position(loud).unwrap_or(0);
        log.drain(..first * 2);
        if log.is_empty() {
            self.status = "Nothing has been heard yet".into();
            return;
        }
        let dir = record::folder(self.path.as_deref());
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = dir.join(format!("Audio log {stamp}.wav"));
        let done = std::fs::create_dir_all(&dir).and_then(|_| std::fs::File::create(&path)).and_then(|f| cereal_engine::wav::write_pcm16(std::io::BufWriter::new(f), &log, rate, 2));
        match done {
            Ok(()) => {
                self.add_audio_clip(&path, 0, None);
                self.status = format!("{:.1} s of sound laid on the playlist", log.len() as f32 / 2.0 / rate as f32);
            }
            Err(e) => self.status = format!("The audio log could not be saved: {e}"),
        }
    }

    pub(crate) fn add_audio_clip(&mut self, path: &Path, pos: u32, track: Option<u16>) {
        let smp = match self.lib.load_path(path) {
            Ok(s) => s,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let mut len = ((smp.seconds() * self.song.tempo / 60.0 * self.song.ppq as f32).ceil() as u32).max(1);
        let mut c = Chan::default();
        c.name = smp.name.clone();
        c.kind = ChannelKind::AudioClip;
        let told = self.settings.opt(setup::OPT_READ_TEMPO).then(|| std::fs::read(path).ok()).flatten();
        if let Some(beats) = told.and_then(|b| cereal_engine::waveed::tempo_beats(&b, &path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), smp.seconds())) {
            c.smp.stretch_beats = beats;
            c.smp.stretch_mul = 1.0;
            c.smp.keep_pitch = true;
            len = ((beats * self.song.ppq as f32).round() as u32).max(1);
        }
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

    pub(crate) fn export_midi(&mut self) {
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
                let keep = into.is_some() && !self.settings.opt(setup::OPT_AUTO_NAME);
                let c = &mut self.song.channels[ci];
                if !keep || c.name.is_empty() {
                    c.name = s.name.clone();
                }
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
                // the path scratcher names the sample its phrases move through
                let named = cereal_engine::synth::sample_named_in(ident, &pre.state).and_then(|n| self.lib.load_stored(&n, path.parent()));
                let c = &mut self.song.channels[ci];
                c.gen = Some(g);
                if kind == cereal_engine::synth::Kind::Travel {
                    c.sample = named.clone();
                    if let Some(g) = c.gen.as_mut() {
                        g.smp = named;
                    }
                }
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
            ui.menu_button("v", |ui| {
                let mut ask = None;
                if ui.button("Save preset as...").clicked() {
                    self.save_preset = Some((ident.to_string(), generator));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Copy preset").clicked() {
                    ask = Some(1);
                }
                let can = self.plug_clip.as_ref().is_some_and(|(i, g, _)| i.eq_ignore_ascii_case(ident) && *g == generator);
                if ui.add_enabled(can, egui::Button::new("Paste preset")).clicked() {
                    ask = Some(2);
                }
                ui.separator();
                if ui.button("Randomize").clicked() {
                    ask = Some(3);
                }
                if ui.button("Reset to defaults").clicked() {
                    ask = Some(6);
                }
                ui.separator();
                if ui.button("Browse parameters").clicked() {
                    ask = Some(4);
                }
                if ui.button("Link all parameters...").clicked() {
                    ask = Some(5);
                }
                if let Some(a) = ask {
                    self.plug_ask = Some((a, ident.to_string(), generator));
                    ui.close_menu();
                }
            })
            .response
            .hint("Plug-in options");
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
    /// Parameter names and ranges of the selected instrument or the open effect, with the targets they answer to.
    fn plug_params(&self, generator: bool) -> Vec<(&'static str, i32, i32, i32, cereal_engine::song::Target)> {
        use cereal_engine::song::Target;
        if generator {
            let ci = self.sel_chan;
            let Some(g) = self.song.channels.get(ci).and_then(|c| c.gen.as_ref()) else { return vec![] };
            cereal_engine::synth::def(g.kind).params.iter().enumerate().filter(|(i, _)| *i < g.p.len()).map(|(i, p)| (p.name, p.min, p.max, p.def, Target::ChanParam(ci, i as u16))).collect()
        } else {
            let Some((ins, s)) = self.fx_open else { return vec![] };
            let Some(f) = self.song.inserts.get(ins).and_then(|t| t.slots.iter().find(|x| x.index == s)).and_then(|x| x.fx.as_ref()) else { return vec![] };
            cereal_engine::fx::def(f.kind).params.iter().enumerate().filter(|(i, _)| *i < f.p.len()).map(|(i, p)| (p.name, p.min, p.max, p.def, Target::SlotParam(ins, s, i as u16))).collect()
        }
    }

    fn plug_values(&mut self, generator: bool) -> Option<&mut Vec<f32>> {
        if generator {
            self.song.channels.get_mut(self.sel_chan)?.gen.as_mut().map(|g| &mut g.p)
        } else {
            let (ins, s) = self.fx_open?;
            self.song.inserts.get_mut(ins)?.slots.iter_mut().find(|x| x.index == s)?.fx.as_mut().map(|f| &mut f.p)
        }
    }

    fn plug_option(&mut self, what: u8, ident: &str, generator: bool) {
        let state = if generator {
            self.song.channels.get(self.sel_chan).and_then(|c| c.gen.as_ref()).filter(|g| cereal_engine::synth::def(g.kind).ident.eq_ignore_ascii_case(ident)).map(|g| g.encode(None))
        } else {
            self.fx_open.and_then(|(i, s)| self.song.inserts.get(i)?.slots.iter().find(|x| x.index == s)?.fx.as_ref()).map(|f| f.encode(None))
        };
        match what {
            1 => {
                if let Some(state) = state {
                    self.plug_clip = Some((ident.to_string(), generator, cereal_engine::preset::write(ident, &state, generator)));
                    self.status = "Preset copied".into();
                }
            }
            2 => {
                let Some((_, _, bytes)) = self.plug_clip.clone() else { return };
                let path = std::env::temp_dir().join(format!("ocw-paste-{}.fst", std::process::id()));
                if std::fs::write(&path, bytes).is_ok() {
                    let to = if generator { PresetTo::Chan(self.sel_chan) } else { self.fx_open.map(|(i, s)| PresetTo::Slot(i, s)).unwrap_or(PresetTo::Auto) };
                    let was = self.preset_name.get(ident).cloned();
                    let name = self.song.channels.get(self.sel_chan).map(|c| c.name.clone());
                    self.load_preset(&path, to);
                    let _ = std::fs::remove_file(&path);
                    match was {
                        Some(w) => self.preset_name.insert(ident.to_string(), w),
                        None => self.preset_name.remove(ident),
                    };
                    if let (true, Some(n), Some(c)) = (generator, name, self.song.channels.get_mut(self.sel_chan)) {
                        c.name = n;
                    }
                    self.status = "Preset pasted".into();
                }
            }
            3 | 6 => {
                let ps = self.plug_params(generator);
                let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
                if let Some(v) = self.plug_values(generator) {
                    for (i, (_, lo, hi, def, _)) in ps.iter().enumerate() {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        let r = (seed >> 11) as f64 / (1u64 << 53) as f64;
                        v[i] = if what == 6 { *def as f32 } else { (*lo as f64 + r * (*hi as f64 - *lo as f64)).round() as f32 };
                    }
                    self.dirty = true;
                    self.status = if what == 6 { "Settings back to their defaults".into() } else { "Settings randomized".into() };
                }
            }
            4 => self.param_win = Some(generator),
            5 => {
                self.multilink = true;
                self.multi_seen.clear();
                self.multi_queue = self.plug_params(generator).into_iter().map(|p| p.4).collect();
                self.status = format!("Linking {} parameters: turn the knobs of the MIDI device one after another", self.multi_queue.len());
            }
            _ => {}
        }
    }

    /// The list of every parameter of the unit, each one a slider that can be linked or automated.
    fn param_window(&mut self, ctx: &egui::Context) {
        let Some(generator) = self.param_win else { return };
        let ps = self.plug_params(generator);
        if ps.is_empty() {
            self.param_win = None;
            return;
        }
        let mut open = true;
        let mut changed = false;
        egui::Window::new("Parameters").id(egui::Id::new("paramwin")).open(&mut open).default_pos([700.0, 180.0]).default_width(320.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                for (i, (name, lo, hi, _, target)) in ps.iter().enumerate() {
                    let Some(v) = self.plug_values(generator).and_then(|v| v.get_mut(i)) else { break };
                    let r = ui.add(egui::Slider::new(v, *lo as f32..=*hi as f32).step_by(1.0).text(*name));
                    widgets::control_menu(&r, *target);
                    changed |= r.changed();
                }
            });
        });
        if changed {
            self.dirty = true;
        }
        if !open {
            self.param_win = None;
        }
    }

    /// How much of the processor each instrument and effect takes, heaviest first.
    /// Words typed here are spoken by our own voice. Listen plays them; Accept writes a speech preset beside
    /// the recordings and opens it in a new channel.
    fn speech_window(&mut self, ctx: &egui::Context) {
        use cereal_engine::speech;
        let Some(mut v) = self.speech_dlg.take() else { return };
        let (mut open, mut listen, mut accept) = (true, false, false);
        egui::Window::new("Text to speech").open(&mut open).collapsible(false).resizable(false).default_width(380.0).show(ctx, |ui| {
            ui.add(egui::TextEdit::multiline(&mut v.text).desired_rows(3).desired_width(f32::INFINITY));
            egui::Grid::new("speech_grid").num_columns(2).show(ui, |ui| {
                ui.label("Personality");
                egui::ComboBox::from_id_salt("speech_person").selected_text(speech::PERSONALITIES[(v.personality as usize).min(19)]).show_ui(ui, |ui| {
                    for (i, n) in speech::PERSONALITIES.iter().enumerate() {
                        ui.selectable_value(&mut v.personality, i as u8, *n);
                    }
                });
                ui.end_row();
                ui.label("Style");
                egui::ComboBox::from_id_salt("speech_style").selected_text(speech::STYLES[(v.style as usize).min(4)]).show_ui(ui, |ui| {
                    for (i, n) in speech::STYLES.iter().enumerate() {
                        ui.selectable_value(&mut v.style, i as u8, *n);
                    }
                });
                ui.end_row();
                ui.label("Mode");
                egui::ComboBox::from_id_salt("speech_mode").selected_text(speech::MODES[(v.mode as usize).min(2)]).show_ui(ui, |ui| {
                    for (i, n) in speech::MODES.iter().enumerate() {
                        ui.selectable_value(&mut v.mode, i as u8, *n);
                    }
                });
                ui.end_row();
                ui.label("Pitch");
                ui.add(egui::Slider::new(&mut v.pitch, 40.0..=400.0).suffix(" Hz").fixed_decimals(0));
                ui.end_row();
                ui.label("Rate");
                ui.add(egui::Slider::new(&mut v.rate, 60.0..=300.0).fixed_decimals(0));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                listen = ui.button("Listen").clicked();
                accept = ui.button("Accept").clicked();
            });
        });
        if listen {
            let wave = speech::render(&v, 44_100);
            self.handle.send(Cmd::Preview(std::sync::Arc::new(cereal_engine::sample::Sample::from_mono("Speech", 44_100, &wave))));
        }
        if accept {
            let dir = record::folder(self.path.as_deref());
            let stem: String = v.text.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ').take(24).collect();
            let stem = if stem.trim().is_empty() { "Speech".to_string() } else { stem.trim().to_string() };
            let mut path = dir.join(format!("{stem}.speech"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{stem} {n}.speech"));
                n += 1;
            }
            match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, speech::write(&v))) {
                Ok(()) => self.actions.push(Action::LoadSample(path, None)),
                Err(e) => self.status = format!("{}: {e}", path.display()),
            }
            return;
        }
        if open {
            self.speech_dlg = Some(v);
        }
    }

    /// Keys and pads to play with the pointer or a finger: the keys play the selected channel, harder the
    /// lower they are struck, and each pad plays one channel at its own pitch. Sliding moves from key to key.
    fn touch_window(&mut self, ctx: &egui::Context) {
        if !self.show_touch {
            if let Some((chan, key)) = self.touch_held.take() {
                self.handle.send(Cmd::NoteOff { chan, key });
            }
            return;
        }
        let mut open = true;
        let mut want: Option<(u16, u8, f32)> = None;
        egui::Window::new("Touch controllers").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            let down = ui.input(|i| i.pointer.primary_down());
            let at = ui.input(|i| i.pointer.interact_pos());
            // pads, one a channel
            let names: Vec<(String, egui::Color32)> = self.song.channels.iter().take(32).map(|c| (c.name.clone(), egui::Color32::from_rgb(c.color[0], c.color[1], c.color[2]))).collect();
            egui::Grid::new("touch_pads").spacing([4.0, 4.0]).show(ui, |ui| {
                for (i, (name, color)) in names.iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(84.0, 44.0), egui::Sense::click_and_drag());
                    let on = self.touch_held == Some((i as u16, 60));
                    let fill = if on { ACCENT } else { color.gamma_multiply(0.55) };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, name.chars().take(12).collect::<String>(), egui::FontId::proportional(11.0), TEXT);
                    if down && at.is_some_and(|p| rect.contains(p)) {
                        let p = at.unwrap();
                        want = Some((i as u16, 60, 0.4 + 0.8 * ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0)));
                    }
                    if i % 8 == 7 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(6.0);
            // four octaves of keys from C3
            let (white_w, h) = (22.0, 110.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(white_w * 29.0, h), egui::Sense::click_and_drag());
            let p = ui.painter_at(rect);
            const WHITE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
            let white_key = |i: usize| 36 + 12 * (i / 7) as u8 + WHITE[i % 7];
            let white_rect = |i: usize| egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * white_w, 0.0), egui::vec2(white_w - 1.0, h));
            // black keys sit over the gap after these white keys of an octave
            let blacks: Vec<(u8, egui::Rect)> = (0..28).filter(|i| matches!(i % 7, 0 | 1 | 3 | 4 | 5)).map(|i| (white_key(i) + 1, egui::Rect::from_min_size(rect.min + egui::vec2((i + 1) as f32 * white_w - 7.0, 0.0), egui::vec2(14.0, h * 0.62)))).collect();
            let chan = self.sel_chan as u16;
            let mut hit: Option<(u8, egui::Rect)> = None;
            if let Some(pos) = at.filter(|p| down && rect.contains(*p)) {
                hit = blacks.iter().find(|b| b.1.contains(pos)).copied().or_else(|| (0..29).map(|i| (white_key(i), white_rect(i))).find(|w| w.1.contains(pos)));
                if let Some((key, r)) = hit {
                    want = Some((chan, key, 0.4 + 0.8 * ((pos.y - r.top()) / r.height()).clamp(0.0, 1.0)));
                }
            }
            let held = self.touch_held.filter(|h| h.0 == chan).map(|h| h.1).or(hit.map(|h| h.0));
            for i in 0..29 {
                let key = white_key(i);
                p.rect_filled(white_rect(i), 2.0, if held == Some(key) { ACCENT } else { egui::Color32::from_gray(225) });
                if i % 7 == 0 {
                    p.text(white_rect(i).center_bottom() - egui::vec2(0.0, 3.0), egui::Align2::CENTER_BOTTOM, format!("C{}", 3 + i / 7), egui::FontId::proportional(9.0), egui::Color32::from_gray(90));
                }
            }
            for (key, r) in &blacks {
                p.rect_filled(*r, 2.0, if held == Some(*key) { ACCENT } else { egui::Color32::from_gray(25) });
            }
        });
        let now = want.map(|w| (w.0, w.1));
        if now != self.touch_held {
            if let Some((chan, key)) = self.touch_held.take() {
                self.handle.send(Cmd::NoteOff { chan, key });
            }
            if let Some((chan, key, vel)) = want {
                self.handle.send(Cmd::NoteOn { chan, key, vel });
                self.touch_held = Some((chan, key));
            }
        }
        if self.touch_held.is_some() {
            ctx.request_repaint();
        }
        self.show_touch = open;
    }

    /// What note scripts printed and the errors they stopped on.
    fn script_out_window(&mut self, ctx: &egui::Context) {
        let new = cereal_engine::script::take_output();
        if !new.is_empty() {
            self.script_out.push_str(&new);
            // the oldest lines go when it grows long
            if self.script_out.len() > 200_000 {
                let cut = self.script_out.len() - 150_000;
                let cut = self.script_out[cut..].find('\n').map_or(cut, |i| cut + i + 1);
                self.script_out.drain(..cut);
            }
        }
        if !self.show_script_out {
            return;
        }
        let mut open = true;
        egui::Window::new("Script output").open(&mut open).default_size([520.0, 260.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    self.script_out.clear();
                }
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(self.script_out.clone());
                }
            });
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(if self.script_out.is_empty() { "Nothing printed yet" } else { &self.script_out }).monospace().size(11.0)).selectable(true));
            });
        });
        self.show_script_out = open;
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn load_window(&mut self, ctx: &egui::Context) {
        let tr = self.handle.transport.clone();
        if !self.show_load {
            if self.load_at.take().is_some() {
                tr.set_load(false);
            }
            return;
        }
        if self.load_at.is_none() {
            tr.set_load(true);
            tr.take_load();
            self.load_at = Some(std::time::Instant::now());
        }
        if self.load_at.is_some_and(|t| t.elapsed().as_secs_f32() >= 0.5) {
            self.load_at = Some(std::time::Instant::now());
            let (frames, chans, slots) = tr.take_load();
            let played = frames as f64 / OUT_RATE.load(std::sync::atomic::Ordering::Relaxed).max(1) as f64;
            if played > 0.0 {
                let mut rows: Vec<(String, f32)> = vec![];
                for (c, secs) in chans {
                    let name = self.song.channels.get(c).map(|ch| if ch.name.is_empty() { format!("Channel {}", c + 1) } else { ch.name.clone() }).unwrap_or_else(|| format!("Channel {}", c + 1));
                    rows.push((name, (secs / played * 100.0) as f32));
                }
                for ((i, k), secs) in slots {
                    let unit = self.song.inserts.get(i).and_then(|t| t.slots.iter().find(|x| x.index as usize == k)).and_then(|x| x.fx.as_ref()).map(|f| cereal_engine::fx::def(f.kind).title).unwrap_or("Effect");
                    rows.push((format!("{} > {} (slot {})", self.song.insert_name(i), unit, k + 1), (secs / played * 100.0) as f32));
                }
                rows.sort_by(|a, b| b.1.total_cmp(&a.1));
                self.load_rows = rows;
            }
        }
        let mut open = true;
        egui::Window::new("Plug-in load monitor").open(&mut open).default_pos([520.0, 140.0]).default_width(360.0).show(ctx, |ui| {
            let total: f32 = self.load_rows.iter().map(|r| r.1).sum();
            ui.label(RichText::new(format!("Instruments and effects together: {total:.1}% of one processor core")).small().color(widgets::TEXT_DIM));
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("loadgrid").striped(true).num_columns(3).show(ui, |ui| {
                    for (name, pct) in &self.load_rows {
                        ui.label(name);
                        ui.label(format!("{pct:.1}%"));
                        ui.add(egui::ProgressBar::new((pct / 25.0).clamp(0.0, 1.0)).desired_width(110.0));
                        ui.end_row();
                    }
                });
                if self.load_rows.is_empty() {
                    ui.label("Nothing is sounding.");
                }
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        self.show_load = open;
    }

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
                    Ok(s) => {
                        self.browser.heard(&p, &s);
                        self.handle.send(Cmd::Preview(s))
                    }
                    Err(e) => self.status = e,
                },
                Action::Open(p) => self.leave(Leave::Open(p)),
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

    /// Write the last minutes of hand-played keys into the pattern on show. Gives the number of notes.
    fn dump_score_log(&mut self) -> usize {
        let tr = self.handle.transport.clone();
        let evs = tr.score_log(self.score_minutes as f64 * 60.0);
        let per_sec = self.song.tempo as f64 / 60.0 * self.song.ppq.max(1) as f64;
        let notes = cereal_engine::player::score_notes(&evs, tr.score_now(), per_sec);
        if notes.is_empty() {
            return 0;
        }
        let pat = tr.pattern();
        self.song.ensure_pattern(pat);
        let n = notes.len();
        self.song.patterns[pat].notes.extend(notes);
        self.song.patterns[pat].sort();
        self.dirty = true;
        n
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

    /// One of the big windows shown as a window of the operating system, so it can sit on another screen.
    /// Closing it closes the window, as it does when it lives inside the main one.
    fn detached_window(&mut self, ctx: &egui::Context, k: usize, title: &str, size: [f32; 2], open: &mut bool, win: Win, body: fn(&mut App, &mut egui::Ui)) {
        if !*open {
            return;
        }
        let id = egui::ViewportId::from_hash_of(("detached", k));
        let builder = egui::ViewportBuilder::default().with_title(format!("{title} - Open Cereal Workshop")).with_inner_size(size).with_min_inner_size([240.0, 140.0]);
        ctx.show_viewport_immediate(id, builder, |vctx, class| {
            let frame = egui::Frame::NONE.fill(PANEL).inner_margin(6.0);
            if class == egui::ViewportClass::Embedded {
                // no second window to be had here: it stays inside after all
                self.detached[k] = false;
                return;
            }
            egui::CentralPanel::default().frame(frame).show(vctx, |ui| body(self, ui));
            let (closed, focused, pressed) = vctx.input(|i| (i.viewport().close_requested(), i.viewport().focused == Some(true), i.pointer.any_pressed()));
            if focused || pressed {
                self.focus = win;
            }
            if focused && !self.setup.open {
                // the keys pressed over there are ours as well
                self.keys(vctx);
            }
            if closed {
                *open = false;
            }
        });
    }

    fn toggle_multilink(&mut self) {
        self.multilink = !self.multilink;
        self.multi_queue.clear();
        self.multi_seen.clear();
        self.status = if self.multilink { "Multilink on: turn the controls, then the knobs of the MIDI device in the same order".into() } else { "Multilink off".into() };
    }

    /// The first empty pattern after the current one.
    fn next_empty_after(&mut self) {
        let from = self.handle.transport.pattern() + 1;
        let used: std::collections::HashSet<usize> = self.song.clips.iter().filter_map(|c| if let cereal_engine::song::ClipSrc::Pattern(p) = c.src { Some(p) } else { None }).collect();
        let p = (from..self.song.patterns.len()).find(|i| self.song.patterns[*i].notes.is_empty() && self.song.patterns[*i].events.is_empty() && !used.contains(i)).unwrap_or(self.song.patterns.len().max(from));
        self.song.ensure_pattern(p);
        self.handle.transport.set_pattern(p);
        self.dirty = true;
    }

    /// An empty pattern at the current place; the ones from here on move down one.
    fn insert_pattern(&mut self) {
        let p = self.handle.transport.pattern();
        self.song.ensure_pattern(p);
        self.song.patterns.push(Default::default());
        let mut at = self.song.patterns.len() - 1;
        while at > p {
            at = self.song.move_pattern(at, true);
        }
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
        let pad = egui_winit::key_side::take_pad();
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
            if modifiers.is_none() && pad.contains(&key) {
                // the number pad picks patterns and steps through the bars, whichever window has the keys and
                // whether or not the typing keyboard is on
                let mut taken = true;
                match key {
                    Key::Slash => self.jump_bar(-1),
                    Key::Num0 => self.jump_bar(1),
                    Key::Plus => self.step_pattern(1),
                    Key::Minus => self.step_pattern(-1),
                    k if DIGITS[1..].contains(&k) => {
                        if !repeat {
                            let p = DIGITS.iter().position(|d| *d == k).unwrap_or(1) - 1;
                            self.song.ensure_pattern(p);
                            self.handle.transport.set_pattern(p);
                            self.dirty = true;
                        }
                    }
                    _ => taken = false,
                }
                if taken {
                    continue;
                }
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
                    Key::O => self.leave(Leave::OpenDialog),
                    Key::S => self.save(modifiers.shift),
                    Key::R if modifiers.shift => {
                        self.export();
                        if let Some(r) = self.render.as_mut() {
                            r.lossy = 1;
                        }
                    }
                    Key::R => self.export(),
                    Key::F4 if modifiers.shift => self.next_empty_after(),
                    Key::F4 => self.new_pattern(),
                    Key::Insert if modifiers.shift => self.insert_pattern(),
                    Key::ArrowUp | Key::ArrowDown if modifiers.shift && !matches!(self.focus, Win::Piano | Win::Playlist) => {
                        let p = self.handle.transport.pattern();
                        self.song.ensure_pattern(p);
                        let to = self.song.move_pattern(p, key == Key::ArrowUp);
                        self.handle.transport.set_pattern(to);
                        self.dirty = true;
                    }
                    Key::H if modifiers.shift => {
                        self.layout = None;
                        ctx.memory_mut(|m| m.reset_areas());
                    }
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
                    Key::J => self.toggle_multilink(),
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
                Key::F1 if !repeat => {
                    if self.help.open {
                        self.help.open = false;
                    } else {
                        self.help_open();
                    }
                }
                Key::F8 if !repeat => self.show_picker = !self.show_picker,
                Key::Slash => self.jump_bar(-1),
                Key::Num0 if !self.typing_keys => self.jump_bar(1),
                k if !self.typing_keys && !repeat && !modifiers.shift && DIGITS[1..].contains(&k) => {
                    let p = DIGITS.iter().position(|d| *d == k).unwrap_or(1) - 1;
                    self.song.ensure_pattern(p);
                    self.handle.transport.set_pattern(p);
                    self.dirty = true;
                }
                Key::F5 if !repeat => {
                    self.show_playlist = !self.show_playlist;
                    if self.show_playlist {
                        self.focus = Win::Playlist;
                    }
                }
                Key::F6 if !repeat => {
                    self.show_rack = !self.show_rack;
                    if self.show_rack {
                        self.focus = Win::Rack;
                    }
                }
                Key::F7 if !repeat => {
                    self.show_piano = !self.show_piano;
                    if self.show_piano {
                        self.focus = Win::Piano;
                    }
                }
                Key::F9 if !repeat => {
                    self.show_mixer = !self.show_mixer;
                    if self.show_mixer {
                        self.focus = Win::Mixer;
                    }
                }
                Key::F3 if !repeat => self.tool_menu = if self.tool_menu.is_some() { None } else { Some(egui::Pos2::ZERO) },
                Key::F4 if !repeat => {
                    if modifiers.shift {
                        self.next_empty_after();
                    } else {
                        self.new_pattern();
                    }
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
                            let chan = self.recv.iter().find(|r| r.0.is_empty() && r.1 < self.song.channels.len()).map_or(self.sel_chan, |r| r.1) as u16;
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
                                    self.leave(Leave::Template(path));
                                }
                            }
                        });
                    });
                    if ui.button("Open...               Ctrl+O").clicked() {
                        ui.close_menu();
                        self.leave(Leave::OpenDialog);
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
                    let last = Self::last_backup();
                    if ui.add_enabled(last.is_some(), egui::Button::new("Revert to last backup")).hint("Open the newest copy in the backup folder: unsaved work put there on a timer, or a project as it was before it was saved over").clicked() {
                        ui.close_menu();
                        if let Some(p) = last {
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
                    if ui.button("Find next empty                Shift+F4").clicked() {
                        self.next_empty_after();
                        ui.close_menu();
                    }
                    if ui.button("Insert one                        Shift+Ctrl+Ins").hint("An empty pattern here; this one and the ones after it move down").clicked() {
                        self.insert_pattern();
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
                    if ui.button("Render as audio clip").hint("The pattern is played into a sound file that becomes an audio clip channel").clicked() {
                        self.render_pattern(false);
                        ui.close_menu();
                    }
                    if ui.button("Render and replace").hint("As above, and every clip of the pattern on the playlist becomes the sound").clicked() {
                        self.render_pattern(true);
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
                    ui.checkbox(&mut self.show_picker, "Plug-in picker     F8");
                    ui.checkbox(&mut self.show_load, "Plug-in load monitor");
                    ui.checkbox(&mut self.show_touch, "Touch controllers");
                    ui.checkbox(&mut self.show_script_out, "Script output");
                    ui.checkbox(&mut self.show_project, "Project picker    Ctrl+F8");
                    ui.separator();
                    ui.menu_button("Layout", |ui| {
                        if ui.button("Arrange windows: default     Ctrl+Shift+H").clicked() {
                            self.layout = None;
                            ui.ctx().memory_mut(|m| m.reset_areas());
                            ui.close_menu();
                        }
                        let mut gone = None;
                        for k in 0..self.settings.layouts.len() {
                            ui.horizontal(|ui| {
                                if ui.button(format!("Arrange windows: {}", self.settings.layouts[k].0)).clicked() {
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
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.layout_name).desired_width(120.0).hint_text("Name"));
                            if ui.add_enabled(!self.layout_name.trim().is_empty(), egui::Button::new("Save this layout")).hint("Remember where the windows are and which are open, under this name").clicked() {
                                let name = std::mem::take(&mut self.layout_name);
                                self.save_layout(&name);
                                ui.close_menu();
                            }
                        });
                        ui.separator();
                        if ui.button("Close all windows     F12").clicked() {
                            self.close_all();
                            ui.close_menu();
                        }
                        if ui.button("Show the main windows").hint("Playlist, channel rack, mixer and browser").clicked() {
                            (self.show_playlist, self.show_rack, self.show_mixer, self.show_browser) = (true, true, true, true);
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Background", |ui| {
                        if ui.button("Choose a picture...").clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("Picture", &["png", "jpg", "jpeg", "bmp"]).pick_file() {
                                self.settings.backdrop = Some(p);
                                let _ = self.settings.save();
                            }
                            ui.close_menu();
                        }
                        if ui.add_enabled(self.settings.backdrop.is_some(), egui::Button::new("No picture")).clicked() {
                            self.settings.backdrop = None;
                            let _ = self.settings.save();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Toolbars", |ui| {
                        self.toolbar_menu(ui);
                    });
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
                    ui.checkbox(&mut self.rec_blend, "Blend recorded notes               Ctrl+B");
                    ui.checkbox(&mut self.rec_loop, "Loop recording");
                    let (mut w, mut st) = (self.handle.transport.waiting(), self.handle.transport.step_edit());
                    if ui.checkbox(&mut w, "Wait for input to start playing   Ctrl+I").changed() {
                        self.handle.transport.set_waiting(w);
                    }
                    if ui.checkbox(&mut st, "Step editing                           Ctrl+E").changed() {
                        self.handle.transport.set_step_edit(st);
                    }
                    let mut ml = self.multilink;
                    if ui.checkbox(&mut ml, "Multilink to controllers           Ctrl+J").on_hover_text("Turn the controls you want, then move a knob of the MIDI device for each, in the same order").changed() {
                        self.toggle_multilink();
                    }
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
                    let tweaked: widgets::Tweaked = ui.ctx().data(|d| d.get_temp(egui::Id::new("tweaked"))).unwrap_or_default();
                    for (k, label) in ["Last tweaked", "Before last tweaked"].into_iter().enumerate() {
                        ui.add_enabled_ui(tweaked[k].is_some(), |ui| {
                            ui.menu_button(label, |ui| {
                                let Some(target) = tweaked[k] else { return };
                                ui.label(RichText::new(self.song.target_name(target)).color(TEXT_DIM).small());
                                if let Some(a) = widgets::control_items(ui) {
                                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ctl_ask"), Some((target, a))));
                                    ui.close_menu();
                                }
                            });
                        });
                    }
                    if ui.button("Text to speech").hint("Type words and have them spoken into a new channel").clicked() {
                        self.speech_dlg.get_or_insert_with(Default::default);
                        ui.close_menu();
                    }
                    ui.menu_button("One-click audio recording", |ui| {
                        if ui.button("Into the audio editor").hint("A Wave Recorder on the selected mixer track starts taking down what arrives there").clicked() {
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
                        if ui.button("Into the playlist as an audio clip").hint("The song starts and the input device is recorded onto the playlist until you stop").clicked() {
                            self.rec_audio = true;
                            self.handle.transport.set_recording(true);
                            if !self.handle.transport.playing() {
                                self.play();
                            }
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Audio logger", |ui| {
                        if ui.button("Dump audio log to the playlist").hint("What came out of the program in the last two minutes becomes an audio clip").clicked() {
                            self.dump_audio_log();
                            ui.close_menu();
                        }
                        if ui.button("Clear log").clicked() {
                            self.handle.transport.clear_audio_log();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Score logger", |ui| {
                        if ui.button("Dump score log to selected pattern").hint("Every key played by hand is remembered, recording or not; this writes the last minutes of it into the pattern on show").clicked() {
                            let n = self.dump_score_log();
                            self.status = if n == 0 { "Nothing was played in that time".into() } else { format!("{n} played notes written to the pattern") };
                            ui.close_menu();
                        }
                        ui.menu_button("Time", |ui| {
                            for m in [2u32, 5, 10, 20, 30] {
                                if ui.radio(self.score_minutes == m, format!("{m} minutes")).clicked() {
                                    self.score_minutes = m;
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button("Clear log").clicked() {
                            self.handle.transport.clear_score_log();
                            ui.close_menu();
                        }
                    });
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
                        if ui.button("Unlock all playlist tracks from content").clicked() {
                            let n = self.song.tracks.iter_mut().filter(|t| t.lock_content).map(|t| t.lock_content = false).count();
                            self.dirty |= n > 0;
                            self.status = format!("{n} tracks unlocked");
                            ui.close_menu();
                        }
                        if ui.button("Reset empty playlist tracks").hint("Tracks with no clip on them get their plain name and colour back").clicked() {
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
                        if ui.button("Prepare for MIDI export").hint("Every sampler and instrument channel becomes a MIDI Sender on its own MIDI channel; the sounds themselves are given up").clicked() {
                            let n = self.song.prepare_midi_export();
                            if n > 0 {
                                self.dirty = true;
                            }
                            self.status = format!("{n} channels turned into MIDI Senders");
                            ui.close_menu();
                        }
                        for (keep, label) in [(true, "Switch all audio clips to stretch"), (false, "Switch all audio clips to resample")] {
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
                        for (on, label) in [(true, "Lock all playlist tracks to content"), (false, "Unlock all playlist tracks from content")] {
                            if ui.button(label).clicked() {
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
                        if ui.button("Remove all Wave Recorder instances").hint("Takes every Wave Recorder out of the mixer's effect slots").clicked() {
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
                    if ui.button("Keys and mouse      F1").clicked() {
                        self.help_open();
                        ui.close_menu();
                    }
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
            // on a narrow screen the window buttons and the monitor take a second row
            let order = self.settings.toolbar_order();
            let bg = ui.interact(ui.max_rect(), ui.id().with("toolbar_bg"), egui::Sense::click());
            bg.context_menu(|ui| self.toolbar_menu(ui));
            ui.horizontal_wrapped(|ui| {
                let mut act = None;
                for (i, &k) in order.iter().enumerate() {
                    let hidden = self.settings.toolbar_hidden & (1 << k) != 0;
                    if self.bar_edit {
                        if ui.small_button("<").hint("Move this panel to the left").clicked() {
                            act = Some((i, 0));
                        }
                        if ui.small_button(if hidden { "show" } else { "hide" }).hint(setup::TOOLBAR_PANELS[k as usize]).clicked() {
                            act = Some((i, 2));
                        }
                        if ui.small_button(">").hint("Move this panel to the right").clicked() {
                            act = Some((i, 1));
                        }
                        if hidden {
                            ui.label(RichText::new(setup::TOOLBAR_PANELS[k as usize]).color(TEXT_DIM).small());
                            ui.separator();
                            continue;
                        }
                    } else if hidden {
                        continue;
                    }
                    self.tb_panel(ui, k);
                    ui.separator();
                }
                if self.bar_edit {
                    if ui.button("Done").hint("Leave the toolbar as it is now").clicked() {
                        self.bar_edit = false;
                    }
                    if ui.button("Reset").hint("The usual order, everything shown").clicked() {
                        self.settings.toolbar.clear();
                        self.settings.toolbar_hidden = 0;
                        let _ = self.settings.save();
                    }
                }
                if let Some((i, what)) = act {
                    let mut o = order.clone();
                    match what {
                        0 if i > 0 => o.swap(i, i - 1),
                        1 if i + 1 < o.len() => o.swap(i, i + 1),
                        2 => self.settings.toolbar_hidden ^= 1 << o[i],
                        _ => {}
                    }
                    self.settings.toolbar = o;
                    let _ = self.settings.save();
                }
            });
            ui.add_space(2.0);
        });
    }

    /// The toolbar's own menu: which panels show, and the switch that lets them be moved.
    fn toolbar_menu(&mut self, ui: &mut egui::Ui) {
        for (k, label) in setup::TOOLBAR_PANELS.iter().enumerate() {
            let mut on = self.settings.toolbar_hidden & (1 << k) == 0;
            if ui.checkbox(&mut on, *label).changed() {
                self.settings.toolbar_hidden ^= 1 << k;
                let _ = self.settings.save();
            }
        }
        ui.separator();
        if ui.checkbox(&mut self.bar_edit, "Edit toolbars").hint("Move the panels left and right, hide and show them").changed() {
            ui.close_menu();
        }
    }

    /// One panel of the toolbar row; the settings hold their order and which are hidden.
    fn tb_panel(&mut self, ui: &mut egui::Ui, k: u8) {
        let tr = self.handle.transport.clone();
        let (song_mode, playing, pat) = (tr.song_mode(), tr.playing(), tr.pattern());
        match k {
            0 => {
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
                let rec_btn = ui.add_sized([34.0, 24.0], egui::Button::new(RichText::new("REC").strong().color(if rec { egui::Color32::from_rgb(235, 70, 60) } else { TEXT_DIM }))).hint("Record what you play into the current pattern (R)");
                rec_btn.context_menu(|ui| {
                    ui.label(RichText::new("Recording filter").color(TEXT_DIM).small());
                    ui.checkbox(&mut self.rec_automation, "Automation").hint("Controls turned while recording are written into the pattern");
                    ui.checkbox(&mut self.rec_score, "Notes");
                    ui.checkbox(&mut self.rec_audio, "Audio").hint("Sound from the input device, as an audio clip on the playlist");
                    ui.separator();
                    let (mut count, mut wait, mut met) = (tr.countdown(), tr.waiting(), tr.metronome());
                    if ui.checkbox(&mut count, "Recording countdown   Ctrl+P").changed() {
                        tr.set_countdown(count);
                    }
                    if ui.checkbox(&mut wait, "Wait for input to start playing   Ctrl+I").changed() {
                        tr.set_waiting(wait);
                    }
                    if ui.checkbox(&mut met, "Metronome   Ctrl+M").changed() {
                        tr.set_metronome(met);
                    }
                    ui.checkbox(&mut self.rec_blend, "Blend recorded notes   Ctrl+B");
                    ui.checkbox(&mut self.rec_loop, "Loop recording");
                    ui.checkbox(&mut self.rec_quantise, "Snap recorded notes to the grid");
                });
                if rec_btn.clicked() {
                    tr.set_recording(!rec);
                }
                if ui.toggle_value(&mut self.rec_audio, "MIC").hint("Also record sound from the input device, as an audio clip on the playlist").changed() && !self.rec_audio {
                    self.record_audio();
                }
            }
            1 => {
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
            }
            2 => {
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
            }
            3 => {
                let mut met = tr.metronome();
                let r = ui.toggle_value(&mut met, "Metro").hint("Metronome (Ctrl+M); right-click picks its sound");
                if r.changed() {
                    tr.set_metronome(met);
                }
                r.context_menu(|ui| {
                    for (k, name) in cereal_engine::player::CLICK_KINDS.iter().enumerate() {
                        if ui.radio(tr.click_kind() as usize == k, *name).clicked() {
                            tr.set_click_kind(k as u8);
                            self.settings.click_kind = k as u8;
                            let _ = self.settings.save();
                            ui.close_menu();
                        }
                    }
                });
                let mut wait = tr.waiting();
                if ui.toggle_value(&mut wait, "Wait").hint("Wait for input before the song starts (Ctrl+I)").changed() {
                    tr.set_waiting(wait);
                }
                let mut count = tr.countdown();
                let r = ui.toggle_value(&mut count, "3 2 1").hint("Count in before recording (Ctrl+P); right-click picks how many bars");
                if r.changed() {
                    tr.set_countdown(count);
                }
                r.context_menu(|ui| {
                    for n in [1u8, 2, 4] {
                        if ui.radio(tr.count_bars() == n, if n == 1 { "1 bar".to_string() } else { format!("{n} bars") }).clicked() {
                            tr.set_count_bars(n);
                            self.settings.count_bars = n;
                            let _ = self.settings.save();
                            ui.close_menu();
                        }
                    }
                });
                ui.toggle_value(&mut self.rec_blend, "Blend").hint("Recorded notes are added to the ones already there (Ctrl+B)");
                let mut step_edit = tr.step_edit();
                if ui.toggle_value(&mut step_edit, "Step").hint("Step editing: played notes are written one grid step after another (Ctrl+E)").changed() {
                    tr.set_step_edit(step_edit);
                }
                ui.toggle_value(&mut self.typing_keys, "Keys").hint("Typing keyboard to piano (Ctrl+T)");
            }
            4 => {
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
            }
            5 => {
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
            }
            6 => {
                let buttons: &mut [(&mut bool, &str, &str)] = &mut [
                    (&mut self.show_playlist, "Playlist", "F5"),
                    (&mut self.show_piano, "Piano roll", "F7"),
                    (&mut self.show_rack, "Channel rack", "F6"),
                    (&mut self.show_mixer, "Mixer", "F9"),
                    (&mut self.show_browser, "Browser", "Alt+F8"),
                ];
                for (flag, label, tip) in buttons.iter_mut() {
                    if ui.selectable_label(**flag, *label).hint(*tip).clicked() {
                        **flag = !**flag;
                    }
                }
            }
            7 => {
                    let load = CPU_LOAD.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10.0;
                    // what is going out: the wave, or after a click the level of each frequency
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(96.0, 20.0), egui::Sense::click());
                    if resp.hint("Output: click to switch between the wave and the frequencies").clicked() {
                        self.scope_bars = !self.scope_bars;
                    }
                    let paint = ui.painter_at(rect);
                    paint.rect_filled(rect, 2.0, egui::Color32::from_rgb(22, 26, 30));
                    tr.output_scope(&mut self.scope_buf);
                    let line = egui::Color32::from_rgb(130, 220, 170);
                    if self.scope_bars {
                        let spec = cereal_engine::scope::spectrum(&self.scope_buf);
                        let half = spec.len() / 2;
                        let bars = 32usize;
                        for b in 0..bars {
                            // evenly spaced by pitch from the second bin to the top
                            let lo = (2.0 * (half as f32 / 2.0).powf(b as f32 / bars as f32)) as usize;
                            let hi = ((2.0 * (half as f32 / 2.0).powf((b + 1) as f32 / bars as f32)) as usize).max(lo + 1).min(half);
                            let v = spec.get(lo..hi).map_or(0.0, |s| s.iter().fold(0.0f32, |m, x| m.max(*x)));
                            let h = ((20.0 * v.max(1e-6).log10() + 72.0) / 72.0).clamp(0.0, 1.0) * rect.height();
                            if h > 0.5 {
                                let x = rect.left() + b as f32 * rect.width() / bars as f32;
                                paint.rect_filled(egui::Rect::from_min_max(egui::pos2(x, rect.bottom() - h), egui::pos2(x + rect.width() / bars as f32 - 1.0, rect.bottom())), 0.0, line);
                            }
                        }
                    } else {
                        let n = self.scope_buf.len();
                        let take = 1024.min(n);
                        let pts: Vec<egui::Pos2> = (0..rect.width() as usize)
                            .map(|x| {
                                let v = self.scope_buf[n - take + (x * take / rect.width() as usize).min(take - 1)];
                                egui::pos2(rect.left() + x as f32, rect.center().y - v.clamp(-1.0, 1.0) * rect.height() * 0.5)
                            })
                            .collect();
                        paint.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, line)));
                    }
                    ui.label(RichText::new(format!("CPU {load:.0}%")).color(if load > 85.0 { egui::Color32::from_rgb(235, 70, 60) } else { TEXT_DIM }).small()).hint("Share of each block's time the sound engine needs");
                    ui.label(RichText::new(format!("{} voices", tr.voices())).color(TEXT_DIM).small());
            }
            _ => {}
        }
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
            ui.horizontal(|ui| {
                ui.label("Timebase (PPQ)").hint("How many ticks a beat is cut into; everything in the project moves onto the new grid");
                let was = self.song.ppq;
                let mut ppq = was;
                egui::ComboBox::from_id_salt("timebase").width(70.0).selected_text(ppq.to_string()).show_ui(ui, |ui| {
                    for v in [24u32, 48, 72, 96, 120, 144, 168, 192, 384, 768, 960] {
                        ui.selectable_value(&mut ppq, v, v.to_string());
                    }
                });
                if ppq != was {
                    let at = self.handle.transport.tick().max(0.0) as f64 * ppq as f64 / was.max(1) as f64;
                    self.song.set_ppq(ppq);
                    self.handle.transport.seek(at as u32);
                    changed = true;
                }
            });
            changed |= ui.checkbox(&mut self.song.truncated, "Play truncated notes in clips").changed();
            ui.horizontal(|ui| {
                ui.label("Panning law");
                let name = |l: u8| if l == 2 { "Triangular" } else { "Circular" };
                egui::ComboBox::from_id_salt("panlaw").width(110.0).selected_text(name(self.song.pan_law)).show_ui(ui, |ui| {
                    for l in [0u8, 2] {
                        changed |= ui.selectable_value(&mut self.song.pan_law, l, name(l)).changed();
                    }
                });
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
        self.load_window(ctx);
        self.speech_window(ctx);
        self.touch_window(ctx);
        self.script_out_window(ctx);
        if let Some((ident, generator)) = self.save_preset.take() {
            self.save_preset_file(&ident, generator);
        }
        let det = (0..4).fold(0u8, |a, k| a | (self.detached[k] as u8) << k);
        if det != self.settings.detached {
            self.settings.detached = det;
            let _ = self.settings.save();
        }
        if let Some((what, ident, generator)) = self.plug_ask.take() {
            self.plug_option(what, &ident, generator);
        }
        self.param_window(ctx);
        if let Some(ask) = self.state_ask.take() {
            self.state_file(ask);
        }
        if !self.problems.is_empty() {
            let mut close = false;
            egui::Window::new("There were problems opening the project").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("The project is open, but these parts of it will not sound:");
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for p in &self.problems {
                        ui.label(p);
                    }
                });
                ui.separator();
                close = ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
            });
            if close {
                self.problems.clear();
            }
        }
        let mut open = self.show_about;
        egui::Window::new("About").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("Open Cereal Workshop");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("A free, open source music workstation for Linux and Windows.");
            ui.label(RichText::new(&self.audio).color(TEXT_DIM).small());
        });
        self.show_about = open;
        self.help_window(ctx);
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
/// Things that replace the project or end the program.
enum Leave {
    Close,
    New,
    OpenDialog,
    Open(PathBuf),
    Template(PathBuf),
}

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
    /// What becomes of sound ringing past the end: 0 cut, 1 left on the end, 2 wrapped over the start.
    tail_mode: u8,
    /// One file for every mixer track that sounds.
    split: bool,
    mono: bool,
    /// Only the stretch picked on the ruler.
    range: bool,
    /// Time markers go into the WAV file as cue points.
    marks: bool,
    dither: bool,
    /// Tempo and beat count (and a loop, when the tail is wrapped) go into the WAV file.
    tempo_info: bool,
    no_inserts: bool,
    no_master: bool,
    /// How samples are read between their frames for this render.
    resample: u8,
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
        let moved: Vec<cereal_engine::song::Target> = ctx.data_mut(|d| d.remove_temp(egui::Id::new("moved"))).unwrap_or_default();
        if self.multilink {
            for t in &moved {
                if !self.multi_queue.contains(t) {
                    self.multi_queue.push(*t);
                }
            }
        }
        self.record_controls(moved);
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
                widgets::CtlAsk::Init => {
                    let pat = self.handle.transport.pattern();
                    self.song.ensure_pattern(pat);
                    let (param, dest) = self.song.event_address(target);
                    if let Some(raw) = self.song.event_raw(target) {
                        let ev = &mut self.song.patterns[pat].events;
                        ev.retain(|e| (e.param, e.dest, e.tick) != (param, dest, 0));
                        ev.insert(0, cereal_engine::song::PatEvent { tick: 0, param, dest, raw });
                        self.dirty = true;
                        self.status = format!("{} starts from here", self.song.target_name(target));
                    }
                }
                widgets::CtlAsk::Copy => self.value_clip = Some(self.target_travel(target)),
                widgets::CtlAsk::Paste => {
                    if let Some(v) = self.value_clip {
                        self.song.set_travel(target, v);
                        self.dirty = true;
                    }
                }
                widgets::CtlAsk::TypeIn => self.type_in = Some((target, String::new(), true)),
            }
        }
        if let Some((target, mut text, first)) = self.type_in.take() {
            let mut keep = true;
            egui::Window::new("Type in value").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(RichText::new(self.song.target_name(target)).color(TEXT_DIM).small());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0).hint_text(format!("{:.1}", self.target_travel(target) * 100.0)));
                    ui.label("%");
                    if first {
                        r.request_focus();
                    }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Accept").clicked() || enter {
                        if let Ok(v) = text.trim().trim_end_matches('%').replace(',', ".").parse::<f32>() {
                            self.song.set_travel(target, v / 100.0);
                            self.dirty = true;
                        }
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep = false;
                    }
                });
            });
            if keep {
                self.type_in = Some((target, text, false));
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
        // the picture behind the windows is read once, and again when another is chosen
        if self.backdrop_tex.as_ref().map(|t| &t.0) != self.settings.backdrop.as_ref() {
            self.backdrop_tex = self.settings.backdrop.clone().map(|p| {
                let tex = image::open(&p).ok().map(|pic| {
                    let pic = if pic.width() > 2560 || pic.height() > 1600 { pic.resize(2560, 1600, image::imageops::FilterType::Triangle) } else { pic };
                    let rgba = pic.to_rgba8();
                    let img = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba);
                    ctx.load_texture("backdrop", img, egui::TextureOptions::LINEAR)
                });
                (p, tex)
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |ui| {
            if let Some(tex) = self.backdrop_tex.as_ref().and_then(|t| t.1.as_ref()) {
                // fills the space, cut at the sides or top and bottom rather than squeezed
                let area = ui.max_rect();
                let [w, h] = tex.size().map(|v| v as f32);
                let (fit_w, fit_h) = ((area.width() / area.height() * h / w).min(1.0), (area.height() / area.width() * w / h).min(1.0));
                let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), egui::vec2(fit_w, fit_h));
                ui.painter().image(tex.id(), area, uv, egui::Color32::WHITE);
            }
        });

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
        let shown = if self.detached[0] {
            self.detached_window(ctx, 0, "Playlist", [wide, upper], &mut open, Win::Playlist, |a, ui| {
                a.playlist_ui(ui);
            });
            None
        } else if self.pl_max && open {
            // Enter lets the playlist fill the work area; its own window keeps its place for afterwards
            let size = [(screen.width() - left - 8.0).max(200.0), (screen.height() - top - 44.0).max(120.0)];
            egui::Window::new("Playlist").id(egui::Id::new("playlist_max")).open(&mut open).frame(frame).fixed_pos([left, top - 44.0]).fixed_size(size).show(ctx, |ui| {
                ui.set_min_size(size.into());
                self.playlist_ui(ui)
            })
        } else {
            let (pos, size) = self.win_place(0, [left, top], [wide, upper]);
            let shown = egui::Window::new("Playlist").id(self.win_id("Playlist")).open(&mut open).frame(frame).default_pos(pos).default_size(size).show(ctx, |ui| {
                let inner = ui.max_rect().size();
                self.playlist_ui(ui);
                inner
            });
            if let Some(r) = &shown {
                self.win_now[0..4].copy_from_slice(&[r.response.rect.left(), r.response.rect.top(), r.inner.map_or(wide, |s| s.x), r.inner.map_or(upper, |s| s.y)]);
            }
            shown.map(|r| egui::InnerResponse::new(Some(()), r.response))
        };
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Playlist;
        }
        self.show_playlist = open;
        let mut open = self.show_rack;
        let (pos, size) = self.win_place(1, [left, low_y], [rack_w, low_h]);
        let shown = if self.detached[1] {
            self.detached_window(ctx, 1, "Channel rack", [rack_w, low_h], &mut open, Win::Rack, |a, ui| {
                a.rack_ui(ui);
            });
            None
        } else {
    egui::Window::new("Channel rack").id(self.win_id("Channel rack")).open(&mut open).frame(frame).default_pos(pos).default_size(size).show(ctx, |ui| {
                let inner = ui.max_rect().size();
                self.rack_ui(ui);
                inner
                    })
        };
        if let Some(r) = &shown {
            self.win_now[4..8].copy_from_slice(&[r.response.rect.left(), r.response.rect.top(), r.inner.map_or(rack_w, |s| s.x), r.inner.map_or(low_h, |s| s.y)]);
        }
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Rack;
        }
        self.show_rack = open;
        let mut open = self.show_piano;
        let (pos, size) = self.win_place(2, [left + 24.0, top + 24.0], [wide - 24.0, upper + 60.0]);
        let shown = if self.detached[2] {
            self.detached_window(ctx, 2, "Piano roll", [wide, upper + 60.0], &mut open, Win::Piano, |a, ui| {
                a.piano_ui(ui);
            });
            None
        } else {
    egui::Window::new("Piano roll").id(self.win_id("Piano roll")).open(&mut open).frame(frame).default_pos(pos).default_size(size).show(ctx, |ui| {
                let inner = ui.max_rect().size();
                self.piano_ui(ui);
                inner
                    })
        };
        if let Some(r) = &shown {
            self.win_now[8..12].copy_from_slice(&[r.response.rect.left(), r.response.rect.top(), r.inner.map_or(wide, |s| s.x), r.inner.map_or(upper, |s| s.y)]);
        }
        if shown.is_some_and(|r| took(&r.response)) {
            self.focus = Win::Piano;
        }
        self.show_piano = open;
        let mut open = self.show_mixer;
        let (pos, size) = self.win_place(3, [left + rack_w + 28.0, low_y], [(screen.width() - left - rack_w - 44.0).max(400.0), low_h]);
        let shown = if self.detached[3] {
            self.detached_window(ctx, 3, "Mixer", [900.0, low_h], &mut open, Win::Mixer, |a, ui| {
                a.mixer_ui(ui);
            });
            None
        } else {
    egui::Window::new("Mixer").id(self.win_id("Mixer")).open(&mut open).frame(frame).default_pos(pos).default_size(size).show(ctx, |ui| {
                let inner = ui.max_rect().size();
                self.mixer_ui(ui);
                inner
                    })
        };
        if let Some(r) = &shown {
            self.win_now[12..16].copy_from_slice(&[r.response.rect.left(), r.response.rect.top(), r.inner.map_or(400.0, |s| s.x), r.inner.map_or(low_h, |s| s.y)]);
        }
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
        self.stems_ui(ctx);
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
        self.autosave_tick(ctx);
        let bank = cereal_engine::gm::picked();
        if bank != self.settings.gm_bank {
            self.settings.gm_bank = bank;
            let _ = self.settings.save();
        }
        self.leave_window(ctx);
        if ctx.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ctx);
        }
        if self.sel_chan != self.sel_chan_was {
            self.sel_chan_was = self.sel_chan;
            if self.settings.opt(setup::OPT_AUTO_SELECT) && !self.fresh {
                if let Some(c) = self.song.channels.get(self.sel_chan) {
                    if c.insert < self.song.inserts.len() {
                        self.sel_insert = c.insert;
                    }
                }
            }
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
            let keep = if self.settings.undo_levels > 0 { self.settings.undo_levels as usize } else { HISTORY_MAX };
            while self.history.len() > keep + 1 {
                self.history.remove(0);
            }
            self.hist_at = self.history.len() - 1;
        }
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{name}{} - Open Cereal Workshop", if self.unsaved { " *" } else { "" })));
        if self.handle.transport.playing() || self.handle.transport.voices() > 0 {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

fn main() -> eframe::Result {
    // asked to look inside one plug-in file for a running copy of ourselves: answer and leave
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--scan-plugin") {
        if let Some(file) = args.next() {
            print!("{}", cereal_engine::host::scan_report(Path::new(&file)));
        }
        std::process::exit(0);
    }
    if let Ok(me) = std::env::current_exe() {
        let memory = setup::settings_path().and_then(|p| p.parent().map(|d| d.join("plugins.txt")));
        cereal_engine::host::scan_with(&me, memory.as_deref());
    }
    let open = std::env::args().nth(1).map(PathBuf::from);
    // our own picture for the window and the task bar
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1500.0, 900.0]).with_maximized(true).with_min_inner_size(Vec2::new(800.0, 500.0)).with_title("Open Cereal Workshop").with_app_id("open-cereal-workshop");
    if let Some(i) = icon {
        viewport = viewport.with_icon(i);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "open-cereal-workshop",
        options,
        Box::new(move |cc| {
            // our own colours whatever the desktop's light or dark setting says
            cc.egui_ctx.set_theme(egui::Theme::Dark);
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
