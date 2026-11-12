//! The settings window (F10): MIDI devices, the sound card, general and file settings. What is chosen is
//! kept in the settings file and felt at once.

use crate::widgets::Hint as _;
use eframe::egui::{self, RichText};

use crate::{midi, record, App, TEXT_DIM};

#[derive(Default, Clone, Copy, PartialEq)]
pub enum Page {
    #[default]
    Midi,
    Audio,
    General,
    File,
}

#[derive(Default)]
pub struct State {
    pub open: bool,
    pub page: Page,
    /// The sound cards, asked for when the audio page is first shown and on Rescan.
    outs: Option<Vec<String>>,
    ins: Vec<String>,
    /// The interface size from the settings file has been applied.
    pub scaled: bool,
}

const RATES: [u32; 7] = [0, 22_050, 44_100, 48_000, 88_200, 96_000, 192_000];
const BUFFERS: [u32; 8] = [0, 64, 128, 256, 512, 1024, 2048, 4096];

impl App {
    fn midi_prefs_changed(&mut self) {
        let s = &self.settings;
        midi::set_prefs(midi::Prefs { off: s.midi_off.clone(), out: s.midi_out.clone(), sync: s.midi_sync });
        midi::PERF_CHAN.store(s.midi_perf as usize, std::sync::atomic::Ordering::Relaxed);
        let _ = self.settings.save();
    }

    /// Close the sound card and open it again as the settings now say. The song, the transport and whatever
    /// is waiting to be played carry over.
    pub(crate) fn restart_audio(&mut self) {
        // the old stream goes first: two players must never pull the same song
        self._stream = None;
        self.live_in = None;
        self.live_in_failed = false;
        record::set_input(&self.settings.audio_in);
        let handle = &self.handle;
        match crate::start_audio(&self.settings, |rate| handle.player(rate)) {
            Ok((s, name)) => {
                self._stream = Some(s);
                self.audio = name;
            }
            Err(e) => self.audio = format!("NO AUDIO: {e}"),
        }
        self.song.activate_hosted(crate::OUT_RATE.load(std::sync::atomic::Ordering::Relaxed).max(1) as f32);
    }

    pub(crate) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_win.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings").open(&mut open).collapsible(false).default_width(460.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (p, label) in [(Page::Midi, "MIDI"), (Page::Audio, "Audio"), (Page::General, "General"), (Page::File, "File")] {
                    ui.selectable_value(&mut self.prefs_win.page, p, label);
                }
                if ui.selectable_label(false, "Project").hint("Title, artist, comments and time signature of this song (F11)").clicked() {
                    self.show_info = true;
                }
            });
            ui.separator();
            match self.prefs_win.page {
                Page::Midi => self.midi_page(ui),
                Page::Audio => self.audio_page(ui),
                Page::General => self.general_page(ui),
                Page::File => self.file_page(ui),
            }
        });
        self.prefs_win.open = open;
    }

    fn midi_page(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.label(RichText::new("Output").strong());
        ui.label(RichText::new("A sender channel set to a port number plays through the device given that number.").color(TEXT_DIM).small());
        let mut outs = midi::outputs();
        // a device that is not plugged in keeps its line, so its number is not lost
        for (name, _) in &self.settings.midi_out {
            if !outs.contains(name) {
                outs.push(name.clone());
            }
        }
        if outs.is_empty() {
            ui.label(RichText::new("(no MIDI output devices)").color(TEXT_DIM));
        }
        egui::Grid::new("midi_outs").num_columns(2).striped(true).show(ui, |ui| {
            for name in &outs {
                let at = self.settings.midi_out.iter().position(|o| &o.0 == name);
                ui.label(name);
                let text = at.map(|i| format!("Port {}", self.settings.midi_out[i].1)).unwrap_or_else(|| "---".into());
                egui::ComboBox::from_id_salt(("midi_out_port", name)).width(80.0).selected_text(text).show_ui(ui, |ui| {
                    if ui.selectable_label(at.is_none(), "---").clicked() {
                        self.settings.midi_out.retain(|o| &o.0 != name);
                        changed = true;
                    }
                    for port in 0..=255u8 {
                        if ui.selectable_label(at.map(|i| self.settings.midi_out[i].1) == Some(port), format!("Port {port}")).clicked() {
                            self.settings.midi_out.retain(|o| &o.0 != name);
                            self.settings.midi_out.push((name.clone(), port));
                            changed = true;
                        }
                    }
                });
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Performance mode MIDI channel");
            let name = |c: u8| if c == 0 { "---".to_string() } else { format!("{c}") };
            egui::ComboBox::from_id_salt("midi_perf").width(60.0).selected_text(name(self.settings.midi_perf)).show_ui(ui, |ui| {
                for c in 0..=16u8 {
                    changed |= ui.selectable_value(&mut self.settings.midi_perf, c, name(c)).changed();
                }
            });
        })
        .response
        .hint("Notes on this channel play the clips of the performance zone: each octave is a track, each key a clip");
        changed |= ui.checkbox(&mut self.settings.midi_sync, "Send master sync").hint("The clock, start and stop go to every output that has a port number").changed();
        if cfg!(target_os = "linux") {
            ui.label(RichText::new("Each port in use is also a port of \"Breakfast Cereal Out\" that other programs can connect to.").color(TEXT_DIM).small());
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Input").strong());
        let mut ins = midi::inputs();
        for name in &self.settings.midi_off {
            if !ins.contains(name) {
                ins.push(name.clone());
            }
        }
        if ins.is_empty() {
            ui.label(RichText::new("(no MIDI input devices)").color(TEXT_DIM));
        }
        for name in &ins {
            let mut on = !self.settings.midi_off.contains(name);
            if ui.checkbox(&mut on, name).hint("Enable: notes, the wheel, the pedal and knobs of this device are listened to").changed() {
                self.settings.midi_off.retain(|n| n != name);
                if !on {
                    self.settings.midi_off.push(name.clone());
                }
                changed = true;
            }
        }
        if changed {
            self.midi_prefs_changed();
        }
    }

    fn audio_page(&mut self, ui: &mut egui::Ui) {
        use cpal::traits::{DeviceTrait, HostTrait};
        if self.prefs_win.outs.is_none() {
            let host = cpal::default_host();
            let names = |l: Option<Vec<cpal::Device>>| l.unwrap_or_default().iter().filter_map(|d| d.name().ok()).collect::<Vec<_>>();
            self.prefs_win.outs = Some(names(host.output_devices().ok().map(|l| l.collect())));
            self.prefs_win.ins = names(host.input_devices().ok().map(|l| l.collect()));
        }
        let outs = self.prefs_win.outs.clone().unwrap_or_default();
        let ins = self.prefs_win.ins.clone();
        let mut restart = false;
        let pick = |ui: &mut egui::Ui, id: &str, list: &[String], cur: &mut String| {
            let mut hit = false;
            let text = if cur.is_empty() { "(system default)".to_string() } else { cur.clone() };
            egui::ComboBox::from_id_salt(id).width(300.0).selected_text(text).show_ui(ui, |ui| {
                hit |= ui.selectable_value(cur, String::new(), "(system default)").changed();
                for n in list {
                    hit |= ui.selectable_value(cur, n.clone(), n).changed();
                }
            });
            hit
        };
        egui::Grid::new("audio").num_columns(2).show(ui, |ui| {
            ui.label("Device");
            restart |= pick(ui, "audio_out", &outs, &mut self.settings.audio_out);
            ui.end_row();
            ui.label("Input device");
            restart |= pick(ui, "audio_in", &ins, &mut self.settings.audio_in);
            ui.end_row();
            ui.label("Sample rate");
            let name = |r: u32| if r == 0 { "(device default)".to_string() } else { format!("{r} Hz") };
            egui::ComboBox::from_id_salt("audio_rate").width(140.0).selected_text(name(self.settings.rate)).show_ui(ui, |ui| {
                for r in RATES {
                    restart |= ui.selectable_value(&mut self.settings.rate, r, name(r)).changed();
                }
            });
            ui.end_row();
            ui.label("Buffer length");
            let rate = crate::OUT_RATE.load(std::sync::atomic::Ordering::Relaxed).max(1);
            let name = |b: u32| if b == 0 { "(device default)".to_string() } else { format!("{b} smp ({:.0} ms)", b as f32 * 1000.0 / rate as f32) };
            egui::ComboBox::from_id_salt("audio_buf").width(140.0).selected_text(name(self.settings.buffer)).show_ui(ui, |ui| {
                for b in BUFFERS {
                    restart |= ui.selectable_value(&mut self.settings.buffer, b, name(b)).changed();
                }
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("Rescan devices").clicked() {
                self.prefs_win.outs = None;
            }
            if ui.button("Restart audio").clicked() {
                restart = true;
            }
        });
        ui.separator();
        ui.label(RichText::new(format!("Status: {}", self.audio)).color(TEXT_DIM));
        if restart {
            let _ = self.settings.save();
            self.restart_audio();
        }
    }

    fn general_page(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Main interface scaling");
            let cur = self.settings.scale;
            let name = |s: f32| if s <= 0.0 { "(system)".to_string() } else { format!("{:.0}%", s * 100.0) };
            egui::ComboBox::from_id_salt("scale").width(100.0).selected_text(name(cur)).show_ui(ui, |ui| {
                for s in [0.0f32, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
                    changed |= ui.selectable_value(&mut self.settings.scale, s, name(s)).changed();
                }
            });
        });
        ui.checkbox(&mut self.typing_keys, "Typing keyboard to piano");
        ui.checkbox(&mut self.rec_quantise, "Input quantise (snap recorded notes)");
        let mut ask = !self.settings.never_ask;
        if ui.checkbox(&mut ask, "Offer to bring in sounds from your own copy when none are found").changed() {
            self.settings.never_ask = !ask;
            changed = true;
        }
        if changed {
            let _ = self.settings.save();
            self.apply_scale(ui.ctx());
        }
    }

    /// The interface size the settings ask for; none asked = as the system says.
    pub(crate) fn apply_scale(&self, ctx: &egui::Context) {
        ctx.set_zoom_factor(if self.settings.scale > 0.0 { self.settings.scale } else { 1.0 });
    }

    fn file_page(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Browser extra search folders").strong());
        ui.label(RichText::new("Shown in the browser, and searched by file name when a song's sound is not where it was stored.").color(TEXT_DIM).small());
        let mut drop = None;
        for (i, f) in self.settings.folders.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("x").hint("Take this folder off the list").clicked() {
                    drop = Some(i);
                }
                ui.label(f.display().to_string());
            });
        }
        if self.settings.folders.is_empty() {
            ui.label(RichText::new("(none)").color(TEXT_DIM));
        }
        if let Some(i) = drop {
            let gone = self.settings.folders.remove(i);
            self.lib.search_dirs.retain(|d| d != &gone);
            self.lib.rescan();
            self.browser.remove_root(&gone);
            let _ = self.settings.save();
        }
        if ui.button("Add folder...").clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.add_search_folder(dir);
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Sounds and presets from your own copy").strong());
        for c in &self.settings.content {
            ui.label(RichText::new(c.display().to_string()).color(TEXT_DIM));
        }
        if ui.button("Bring in from an installer file...").clicked() {
            self.setup = Default::default();
            self.setup.open = true;
        }
    }

    /// A folder for the browser and the sound search, remembered for next time.
    pub(crate) fn add_search_folder(&mut self, dir: std::path::PathBuf) {
        let label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string());
        cereal_engine::host::add_folder(&dir);
        if !self.lib.search_dirs.contains(&dir) {
            self.lib.search_dirs.push(dir.clone());
            self.lib.rescan();
        }
        self.browser.add_root(&label, dir.clone());
        if !self.settings.folders.contains(&dir) {
            self.settings.folders.push(dir);
            let _ = self.settings.save();
        }
    }
}
