//! Breakfast Cereal DAW, first slice: transport bar and a channel rack with a 16-step sequencer.

use cereal_engine::{render_project, wav, Engine, Project, Shared, STEPS};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use egui::{Color32, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::sync::Arc;

/// Where projects and renders live until there is a file dialog (docs/ROADMAP.md).
fn projects_dir() -> PathBuf {
    let base = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("breakfast-cereal-projects");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn save_project(sh: &Shared, name: &str) -> Result<PathBuf, String> {
    let name = sanitize(name);
    let path = projects_dir().join(format!("{name}.{}", cereal_engine::project::EXT));
    let text = Project::from_shared(sh, &name).to_text();
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

fn load_project(sh: &Shared, name: &str) -> Result<(), String> {
    let path = projects_dir().join(format!("{}.{}", sanitize(name), cereal_engine::project::EXT));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let p = Project::from_text(&text)?;
    if p.channels.len() != sh.channels.len() {
        return Err("that project has a different channel layout than this build".into());
    }
    p.apply_to(sh);
    Ok(())
}

fn export_wav(sh: &Shared, name: &str, bars: u32) -> Result<PathBuf, String> {
    let name = sanitize(name);
    let project = Project::from_shared(sh, &name);
    let rate = 44_100.0;
    let samples = render_project(&project, rate, bars.max(1));
    let path = projects_dir().join(format!("{name}.wav"));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    wav::write_pcm16(std::io::BufWriter::new(file), &samples, rate as u32, 1).map_err(|e| e.to_string())?;
    Ok(path)
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "untitled".into()
    } else {
        s
    }
}

fn start_audio(shared: Arc<Shared>) -> Result<(cpal::Stream, String), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let channels = config.channels() as usize;
    let rate = config.sample_rate().0 as f32;
    let name = format!("{} ({} Hz, {} ch)", device.name().unwrap_or_default(), rate as u32, channels);
    let mut engine = Engine::new(shared, rate);
    let err = |e| eprintln!("audio stream error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| engine.render(out, channels),
            err,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut scratch = vec![0.0f32; 1 << 16];
            device.build_output_stream(
                &config.into(),
                move |out: &mut [i16], _| {
                    let n = out.len().min(scratch.len());
                    engine.render(&mut scratch[..n], channels);
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

struct App {
    shared: Arc<Shared>,
    _stream: Option<cpal::Stream>,
    audio: String,
    paint: Option<bool>, // while dragging across steps: the value being painted
    proj_name: String,
    export_bars: u32,
    status: String,
}

impl App {
    fn report(&mut self, r: Result<PathBuf, String>, verb: &str) {
        self.status = match r {
            Ok(p) => format!("{verb}: {}", p.display()),
            Err(e) => format!("{verb} failed: {e}"),
        };
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let sh = self.shared.clone();
        let sh = &sh;
        let (space, ctrl_s, ctrl_o, ctrl_r) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.modifiers.command && i.key_pressed(egui::Key::S),
                i.modifiers.command && i.key_pressed(egui::Key::O),
                i.modifiers.command && i.key_pressed(egui::Key::R),
            )
        });
        if space {
            sh.set_playing(!sh.playing());
        }
        if ctrl_s {
            let r = save_project(sh, &self.proj_name);
            self.report(r, "Saved");
        }
        if ctrl_o {
            let r = load_project(sh, &self.proj_name);
            self.status = match r {
                Ok(()) => format!("Loaded {}", self.proj_name),
                Err(e) => format!("Load failed: {e}"),
            };
        }
        if ctrl_r {
            let r = export_wav(sh, &self.proj_name, self.export_bars);
            self.report(r, "Exported");
        }
        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let playing = sh.playing();
                let label = if playing { "Stop" } else { "Play" };
                if ui.add_sized([70.0, 28.0], egui::Button::new(RichText::new(label).strong())).clicked() {
                    sh.set_playing(!playing);
                }
                ui.separator();
                let mut bpm = sh.bpm();
                ui.label("Tempo");
                if ui.add(egui::DragValue::new(&mut bpm).range(10.0..=522.0).speed(0.5).fixed_decimals(1).suffix(" BPM")).changed() {
                    sh.set_bpm(bpm);
                }
                ui.separator();
                let mut master = sh.master();
                ui.label("Master");
                if ui.add(egui::Slider::new(&mut master, 0.0..=1.25).show_value(false)).changed() {
                    sh.set_master(master);
                }
                ui.separator();
                ui.label(RichText::new(&self.audio).weak());
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Project");
                ui.add(egui::TextEdit::singleline(&mut self.proj_name).desired_width(140.0));
                if ui.button("Save").on_hover_text("Ctrl+S").clicked() {
                    let r = save_project(sh, &self.proj_name);
                    self.report(r, "Saved");
                }
                if ui.button("Load").on_hover_text("Ctrl+O").clicked() {
                    match load_project(sh, &self.proj_name) {
                        Ok(()) => self.status = format!("Loaded {}", self.proj_name),
                        Err(e) => self.status = format!("Load failed: {e}"),
                    }
                }
                ui.separator();
                ui.label("Bars");
                ui.add(egui::DragValue::new(&mut self.export_bars).range(1..=64).speed(0.1));
                if ui.button("Export WAV").on_hover_text("Ctrl+R").clicked() {
                    let r = export_wav(sh, &self.proj_name, self.export_bars);
                    self.report(r, "Exported");
                }
            });
            if !self.status.is_empty() {
                ui.label(RichText::new(&self.status).weak());
            }
            ui.add_space(6.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Channel rack");
            ui.add_space(8.0);
            let now = sh.current_step();
            let playing = sh.playing();
            if !ctx.input(|i| i.pointer.primary_down()) {
                self.paint = None;
            }
            for ch in &sh.channels {
                ui.horizontal(|ui| {
                    let mut on = !ch.muted();
                    if ui.checkbox(&mut on, "").on_hover_text("Mute").changed() {
                        ch.set_muted(!on);
                    }
                    ui.add_sized([70.0, 22.0], egui::Label::new(RichText::new(ch.name).strong()));
                    let mut vol = ch.volume();
                    if ui.add_sized([70.0, 18.0], egui::Slider::new(&mut vol, 0.0..=1.5).show_value(false)).on_hover_text("Channel volume").changed() {
                        ch.set_volume(vol);
                    }
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for i in 0..STEPS {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 30.0), Sense::click_and_drag());
                        let lit = ch.step(i);
                        // click sets the paint value; dragging across other steps paints the same value
                        if resp.is_pointer_button_down_on() && self.paint.is_none() {
                            self.paint = Some(!lit);
                        }
                        if let Some(v) = self.paint {
                            if ui.rect_contains_pointer(rect) && ctx.input(|i| i.pointer.primary_down()) {
                                ch.set_step(i, v);
                            }
                        }
                        let group_dark = (i / 4) % 2 == 0;
                        let mut fill = match (ch.step(i), group_dark) {
                            (true, _) => Color32::from_rgb(240, 170, 60),
                            (false, true) => Color32::from_gray(70),
                            (false, false) => Color32::from_rgb(95, 70, 70),
                        };
                        if playing && i == now {
                            fill = fill.gamma_multiply(1.0).lerp_to_gamma(Color32::WHITE, 0.45);
                        }
                        ui.painter().rect_filled(rect, 4.0, fill);
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Space: play / stop.  Click or drag across steps to draw or erase.").weak());
        });
        if sh.playing() {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let shared = Shared::new();
    let (stream, audio) = match start_audio(shared.clone()) {
        Ok((s, name)) => (Some(s), name),
        Err(e) => (None, format!("NO AUDIO: {e}")),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 300.0]).with_title("Breakfast Cereal DAW"),
        ..Default::default()
    };
    eframe::run_native(
        "breakfast-cereal",
        options,
        Box::new(move |_| {
            Ok(Box::new(App {
                shared,
                _stream: stream,
                audio,
                paint: None,
                proj_name: "untitled".into(),
                export_bars: 2,
                status: String::new(),
            }))
        }),
    )
}
