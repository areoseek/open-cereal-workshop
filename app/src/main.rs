//! Breakfast Cereal DAW, first slice: transport bar and a channel rack with a 16-step sequencer.

use cereal_engine::{Engine, Shared, STEPS};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use egui::{Color32, RichText, Sense, Vec2};
use std::sync::Arc;

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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let sh = &self.shared;
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            sh.set_playing(!sh.playing());
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
        Box::new(move |_| Ok(Box::new(App { shared, _stream: stream, audio, paint: None }))),
    )
}
