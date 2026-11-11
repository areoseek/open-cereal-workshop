//! Plug-ins from other makers: the settings panel we draw for one, and the menus that add them.

use crate::widgets::*;
use cereal_engine::host::{self, Instance};
use eframe::egui::{self, RichText, Ui};
use std::sync::Arc;

/// The settings of a hosted plug-in as rows of sliders, and a button for its own window. True when a
/// setting was moved here.
pub fn panel(ui: &mut Ui, inst: &Arc<Instance>, title: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(&inst.found.name).strong());
        if !inst.found.vendor.is_empty() {
            ui.label(RichText::new(format!("by {}", inst.found.vendor)).small().color(TEXT_DIM));
        }
    });
    let fail_id = ui.id().with(("host-window", Arc::as_ptr(inst) as usize));
    if inst.has_window() {
        let open = inst.window_open();
        if ui.button(if open { "Close its window" } else { "Open its window" }).on_hover_text("The plug-in's own editor, in a window of its own").clicked() {
            if open {
                inst.hide_window();
            } else {
                let ok = inst.show_window(title);
                ui.data_mut(|d| d.insert_temp(fail_id, !ok));
            }
        }
        if ui.data(|d| d.get_temp::<bool>(fail_id).unwrap_or(false)) {
            ui.label(RichText::new("The plug-in could not open its window here; its settings are below.").small().color(egui::Color32::from_rgb(255, 120, 110)));
        }
    }
    let params = inst.params();
    if params.is_empty() {
        ui.label(RichText::new("This plug-in lists no settings.").small().color(TEXT_DIM));
        return false;
    }
    ui.separator();
    let filter_id = ui.id().with(("host-filter", Arc::as_ptr(inst) as usize));
    let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id).unwrap_or_default());
    if params.len() > 12 {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Find").small().color(TEXT_DIM));
            ui.add(egui::TextEdit::singleline(&mut filter).desired_width(160.0));
        });
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    }
    let needle = filter.to_lowercase();
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        let mut module = String::new();
        egui::Grid::new("host-params").num_columns(3).spacing([8.0, 3.0]).show(ui, |ui| {
            for p in params.iter().filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle) || p.module.to_lowercase().contains(&needle)) {
                if p.module != module {
                    module = p.module.clone();
                    if !module.is_empty() {
                        ui.label(RichText::new(&module).small().color(TEXT_DIM));
                        ui.end_row();
                    }
                }
                let Some(now) = inst.value(p.id) else { continue };
                ui.label(&p.name);
                let mut v = now;
                let mut s = egui::Slider::new(&mut v, p.min..=p.max).show_value(false);
                if p.stepped {
                    s = s.step_by(1.0);
                }
                let r = ui.add_enabled(!p.readonly && p.max > p.min, s);
                if r.double_clicked() {
                    v = p.default;
                }
                if v != now {
                    inst.set(p.id, v);
                    changed = true;
                }
                ui.label(RichText::new(inst.value_text(p.id, v).unwrap_or_else(|| format!("{v:.3}"))).small().color(TEXT_DIM));
                ui.end_row();
            }
        });
    });
    changed
}

/// Menu rows for every installed plug-in of one sort. Gives the one picked, opened and ready.
pub fn menu(ui: &mut Ui, instruments: bool) -> Option<Arc<Instance>> {
    let known = host::known();
    let mut list: Vec<&host::Found> = known.iter().filter(|f| f.instrument == instruments).collect();
    list.sort_by_key(|f| f.name.to_lowercase());
    let mut got = None;
    if list.is_empty() {
        ui.label(RichText::new("None installed. Plug-ins in the CLAP format are\nlooked for in the usual folders and in your search folders.").small().color(TEXT_DIM));
    }
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for f in list {
            let r = ui.button(&f.name);
            let r = if f.vendor.is_empty() { r } else { r.on_hover_text(&f.vendor) };
            if r.clicked() {
                got = Instance::open(&f.path, &f.id);
                ui.close_menu();
            }
        }
    });
    ui.separator();
    if ui.button("Look again").on_hover_text("Scan the plug-in folders again").clicked() {
        host::forget();
    }
    got
}
