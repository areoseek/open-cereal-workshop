//! Extract stems from sample: the question window, fetching the weight files the first time,
//! the work itself on its own thread, and laying the parts on the playlist when it is done.

use crate::App;
use cereal_engine::flp::ChannelKind;
use cereal_engine::sample::Sample;
use cereal_engine::song::ClipSrc;
use cereal_engine::stems::{self, Net};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

/// The order the window lists the parts in, as places in `stems::PARTS`.
const SHOWN: [usize; 4] = [1, 2, 3, 0];

#[derive(Default)]
pub struct State {
    ask: Option<Ask>,
    job: Option<Job>,
}

struct Ask {
    chan: usize,
    want: [bool; 4],
}

#[derive(Default)]
struct Shared {
    fetched: AtomicU64,
    to_fetch: AtomicU64,
    done: AtomicUsize,
    total: AtomicUsize,
    stop: AtomicBool,
}

struct Job {
    chan: usize,
    shared: Arc<Shared>,
    rx: mpsc::Receiver<Result<Vec<PathBuf>, String>>,
}

/// Where the weight files are kept once fetched.
pub fn weights_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CEREAL_STEM_MODELS") {
        return Some(PathBuf::from(d));
    }
    let home = || std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let base = std::env::var_os("XDG_DATA_HOME").or_else(|| std::env::var_os("LOCALAPPDATA")).map(PathBuf::from).or_else(|| home().map(|h| h.join(".local/share")))?;
    Some(base.join("open-cereal-workshop").join("stems"))
}

fn have(dir: &Path, file: &str) -> bool {
    std::fs::metadata(dir.join(file)).is_ok_and(|m| m.len() == stems::FILE_LEN)
}

/// The weight files still to be fetched.
fn missing(dir: &Path) -> Vec<&'static str> {
    stems::FILES.iter().copied().filter(|f| !have(dir, f)).collect()
}

/// Where the parts are written: beside the project when it has a home, else a folder of the user's own.
pub fn folder(project: Option<&Path>) -> PathBuf {
    crate::record::folder(project).with_file_name("Stems")
}

/// Fetches one weight file with the system's own download tool, counting the bytes as they land.
fn fetch(dir: &Path, file: &str, shared: &Shared) -> Result<(), String> {
    let part = dir.join(format!("{file}.part"));
    let _ = std::fs::remove_file(&part);
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-L", "--fail", "-s", "-o"]).arg(&part).arg(format!("{}/{file}/content", stems::SOURCE));
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("The download tool (curl) could not be started: {e}"))?;
    let before = shared.fetched.load(Ordering::Relaxed);
    let status = loop {
        let got = std::fs::metadata(&part).map_or(0, |m| m.len());
        shared.fetched.store(before + got, Ordering::Relaxed);
        if shared.stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err("Stopped".into());
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(150)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let len = std::fs::metadata(&part).map_or(0, |m| m.len());
    if !status.success() || len != stems::FILE_LEN {
        let _ = std::fs::remove_file(&part);
        return Err("The separation model could not be downloaded. Check the internet connection and try again.".into());
    }
    shared.fetched.store(before + len, Ordering::Relaxed);
    std::fs::rename(&part, dir.join(file)).map_err(|e| e.to_string())
}

fn work(sample: &Sample, want: [bool; 4], out: &Path, shared: &Shared) -> Result<Vec<PathBuf>, String> {
    let dir = weights_dir().ok_or("No home folder to keep the separation model in.")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let need = missing(&dir);
    shared.to_fetch.store(need.len() as u64 * stems::FILE_LEN, Ordering::Relaxed);
    for f in need {
        fetch(&dir, f, shared)?;
    }
    let mut nets = Vec::new();
    for f in stems::FILES {
        let bytes = std::fs::read(dir.join(f)).map_err(|e| format!("{f}: {e}"))?;
        nets.push(Net::read(&bytes)?);
    }
    let nets: [Net; 4] = nets.try_into().map_err(|_| "bad weights")?;
    let parts = stems::split(sample, &nets, &shared.done, &shared.total, &shared.stop).ok_or("Stopped")?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut paths = Vec::new();
    for i in SHOWN {
        if !want[i] {
            continue;
        }
        let p = &parts[i];
        let mut path = out.join(&p.name);
        let mut n = 2;
        while path.exists() {
            path = out.join(format!("{} ({n}).wav", p.name.trim_end_matches(".wav")));
            n += 1;
        }
        let flat: Vec<f32> = p.frames.iter().flatten().copied().collect();
        let f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        cereal_engine::wav::write(std::io::BufWriter::new(f), &flat, p.rate, 2, 24).map_err(|e| e.to_string())?;
        paths.push(path);
    }
    Ok(paths)
}

impl App {
    /// Open the question window for a channel's sample.
    pub(crate) fn ask_stems(&mut self, chan: usize) {
        if self.stems.job.is_some() {
            self.status = "Stems are already being extracted".into();
            return;
        }
        if self.song.channels.get(chan).is_some_and(|c| c.sample.is_some()) {
            self.stems.ask = Some(Ask { chan, want: [true; 4] });
        } else {
            self.status = "This channel has no sample to extract stems from".into();
        }
    }

    /// The audio clip under a playlist clip, if that is what it shows.
    pub(crate) fn clip_sample_chan(&self, clip: usize) -> Option<usize> {
        match self.song.clips.get(clip)?.src {
            ClipSrc::Audio(ci) if self.song.channels.get(ci).is_some_and(|c| c.kind == ChannelKind::AudioClip && c.sample.is_some()) => Some(ci),
            _ => None,
        }
    }

    fn start_stems(&mut self, chan: usize, want: [bool; 4]) {
        let Some(sample) = self.song.channels.get(chan).and_then(|c| c.sample.clone()) else { return };
        let out = folder(self.path.as_deref());
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel();
        let sh = shared.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work(&sample, want, &out, &sh));
        });
        self.stems.job = Some(Job { chan, shared, rx });
    }

    /// The parts land on free tracks under the clip they came from, which goes quiet.
    fn place_stems(&mut self, chan: usize, paths: &[PathBuf]) {
        let from = self.song.clips.iter().filter(|c| c.src == ClipSrc::Audio(chan)).map(|c| (c.pos, c.track)).min();
        let (pos, track) = from.unwrap_or((0, 0));
        let color = self.song.channels.get(chan).map(|c| c.color);
        let mut next = track + 1;
        for p in paths {
            let before = self.song.clips.len();
            self.add_audio_clip(p, pos, Some(next));
            if self.song.clips.len() > before {
                let ci = self.song.channels.len() - 1;
                if let Some(col) = color {
                    self.song.channels[ci].color = col;
                }
                if let Some(c) = self.song.clips.iter().find(|c| c.src == ClipSrc::Audio(ci)) {
                    next = c.track + 1;
                }
            }
        }
        if from.is_some() && !paths.is_empty() {
            for c in self.song.clips.iter_mut().filter(|c| c.src == ClipSrc::Audio(chan)) {
                c.muted = true;
            }
        }
        self.dirty = true;
    }

    /// Called every frame: the question window, the progress window, and the finish.
    pub(crate) fn stems_ui(&mut self, ctx: &egui::Context) {
        if let Some(mut ask) = self.stems.ask.take() {
            let name = self.song.channels.get(ask.chan).map(|c| c.name.clone()).unwrap_or_default();
            let (mut go, mut close) = (false, false);
            egui::Window::new("Extract stems from sample").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(name);
                ui.add_space(4.0);
                for i in SHOWN {
                    ui.checkbox(&mut ask.want[i], stems::PARTS[i]);
                }
                let lacking = weights_dir().map_or(4, |d| missing(&d).len());
                if lacking > 0 {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "The separation model is fetched once from zenodo.org ({} MB, Open-Unmix, MIT licence).",
                            lacking as u64 * stems::FILE_LEN / 1_000_000
                        ))
                        .small(),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    go = ui.add_enabled(ask.want.contains(&true), egui::Button::new("Extract")).clicked();
                    close = ui.button("Cancel").clicked();
                });
            });
            close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
            if go {
                self.start_stems(ask.chan, ask.want);
            } else if !close {
                self.stems.ask = Some(ask);
            }
        }
        let Some(job) = &self.stems.job else { return };
        let sh = &job.shared;
        let (to_fetch, fetched) = (sh.to_fetch.load(Ordering::Relaxed), sh.fetched.load(Ordering::Relaxed));
        let (total, done) = (sh.total.load(Ordering::Relaxed), sh.done.load(Ordering::Relaxed));
        let (text, part) = if total > 0 {
            ("Extracting stems".to_string(), done as f32 / total as f32)
        } else if to_fetch > 0 && fetched < to_fetch {
            (format!("Downloading the separation model ({} of {} MB)", fetched / 1_000_000, to_fetch / 1_000_000), fetched as f32 / to_fetch as f32)
        } else {
            ("Getting ready".to_string(), 0.0)
        };
        let mut stop = false;
        egui::Window::new("Stems").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(text);
            ui.add(egui::ProgressBar::new(part).desired_width(280.0).show_percentage());
            stop = ui.button("Cancel").clicked();
        });
        if stop {
            sh.stop.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        match job.rx.try_recv() {
            Ok(Ok(paths)) => {
                let chan = job.chan;
                self.stems.job = None;
                self.place_stems(chan, &paths);
                self.status = format!("{} stems laid on the playlist", paths.len());
            }
            Ok(Err(e)) => {
                self.stems.job = None;
                self.status = if e == "Stopped" { "Stem extraction stopped".into() } else { e };
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.stems.job = None;
                self.status = "Stem extraction failed".into();
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_go_beside_the_project() {
        assert_eq!(folder(Some(Path::new("/a/b/song.flp"))), PathBuf::from("/a/b/Stems"));
    }
}
