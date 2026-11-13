//! The help window (F1): every key and mouse binding, by window, with a find box.
//! The list is read from `docs/KEYBINDINGS.md`, so the window and the document cannot drift apart.

use crate::widgets::TEXT_DIM;
use crate::{App, Win};
use eframe::egui::{self, RichText};

const TEXT: &str = include_str!("../../docs/KEYBINDINGS.md");

/// One window's bindings: its name and rows of (group, binding, action, note).
pub(crate) struct Page {
    name: String,
    rows: Vec<(String, String, String, String)>,
}

#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    page: usize,
    find: String,
    pages: Vec<Page>,
}

fn plain(s: &str) -> String {
    s.replace("**", "").replace('`', "").trim().to_string()
}

fn pages() -> Vec<Page> {
    let mut out: Vec<Page> = vec![];
    let mut group = String::new();
    for l in TEXT.lines() {
        if let Some(h) = l.strip_prefix("## ") {
            out.push(Page { name: plain(h), rows: vec![] });
            group.clear();
        } else if let Some(h) = l.strip_prefix("### ") {
            group = plain(h);
        } else if l.starts_with('|') && !l.starts_with("|---") {
            let c: Vec<&str> = l.trim().trim_matches('|').split('|').collect();
            if c.len() < 3 || c[0].trim() == "Binding" {
                continue;
            }
            let done = plain(c[2]);
            if done == "no" {
                continue;
            }
            // "yes" says nothing; anything longer is a note worth showing
            let note = done.strip_prefix("yes").or_else(|| done.strip_prefix("part")).unwrap_or(&done).trim_matches(|ch: char| ch == ':' || ch == ' ' || ch == '(' || ch == ')').to_string();
            if let Some(p) = out.last_mut() {
                p.rows.push((group.clone(), plain(c[0]), plain(c[1]), note));
            }
        }
    }
    out
}

impl App {
    /// Open the help on the page of the window that has the keys.
    pub(crate) fn help_open(&mut self) {
        if self.help.pages.is_empty() {
            self.help.pages = pages();
        }
        let want = match self.focus {
            Win::Rack => "Channel rack",
            Win::Mixer => "Mixer",
            Win::Piano => "Piano roll",
            Win::Playlist => "Playlist",
            Win::Browser => "Browser",
            _ => "Everywhere",
        };
        if let Some(i) = self.help.pages.iter().position(|p| p.name == want) {
            self.help.page = i;
        }
        self.help.open = true;
    }

    pub(crate) fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Help").open(&mut open).default_size([660.0, 520.0]).default_pos([420.0, 120.0]).show(ctx, |ui| {
            let h = &mut self.help;
            ui.horizontal_wrapped(|ui| {
                for (i, p) in h.pages.iter().enumerate() {
                    ui.selectable_value(&mut h.page, i, &p.name);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Find");
                ui.add(egui::TextEdit::singleline(&mut h.find).desired_width(220.0).hint_text("a key or what it does"));
                if !h.find.is_empty() && ui.small_button("x").clicked() {
                    h.find.clear();
                }
            });
            ui.separator();
            let find = h.find.to_lowercase();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // a search looks through every page, otherwise only the picked one
                for (pi, p) in h.pages.iter().enumerate() {
                    if find.is_empty() && pi != h.page {
                        continue;
                    }
                    let rows: Vec<_> = p.rows.iter().filter(|r| find.is_empty() || r.1.to_lowercase().contains(&find) || r.2.to_lowercase().contains(&find)).collect();
                    if rows.is_empty() {
                        continue;
                    }
                    if !find.is_empty() {
                        ui.heading(&p.name);
                    }
                    let mut group = "";
                    egui::Grid::new(("help", pi)).num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                        for r in rows {
                            if r.0 != group {
                                group = &r.0;
                                ui.label(RichText::new(group).strong());
                                ui.end_row();
                            }
                            ui.scope(|ui| {
                                ui.set_width(220.0);
                                ui.add(egui::Label::new(RichText::new(&r.1).monospace()).wrap());
                            });
                            ui.vertical(|ui| {
                                ui.set_width(380.0);
                                ui.add(egui::Label::new(&r.2).wrap());
                                if !r.3.is_empty() {
                                    ui.add(egui::Label::new(RichText::new(&r.3).color(TEXT_DIM).small()).wrap());
                                }
                            });
                            ui.end_row();
                        }
                    });
                }
            });
        });
        self.help.open = open;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bindings_document_reads_into_pages() {
        let p = super::pages();
        assert!(p.len() >= 6);
        assert!(p.iter().all(|p| !p.rows.is_empty()));
        assert!(p.iter().any(|p| p.name == "Piano roll"));
    }
}
