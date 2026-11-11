//! The playlist: pattern clips and audio clips on tracks.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use cereal_engine::song::{Clip, ClipSrc};

use crate::widgets::*;
use crate::App;

pub struct State {
    scroll_x: f32,
    scroll_y: f32,
    /// Pixels per tick at 96 ppq.
    zoom: f32,
    track_h: f32,
    drag: Option<(usize, f32)>,
}

impl Default for State {
    fn default() -> Self {
        Self { scroll_x: 0.0, scroll_y: 0.0, zoom: 0.09, track_h: 30.0, drag: None }
    }
}

const HEAD_W: f32 = 112.0;
const RULER_H: f32 = 18.0;

impl App {
    fn clip_look(&self, c: &Clip) -> (String, Color32) {
        match c.src {
            ClipSrc::Pattern(p) => (self.song.pattern_name(p), self.song.patterns.get(p).and_then(|p| p.color).map(rgb).unwrap_or(Color32::from_rgb(86, 110, 130))),
            ClipSrc::Audio(ch) | ClipSrc::Automation(ch) => match self.song.channels.get(ch) {
                Some(c) => (c.name.clone(), rgb(c.color)),
                None => ("?".into(), STEP_A),
            },
        }
    }

    pub(crate) fn playlist_ui(&mut self, ui: &mut Ui) {
        let pat = self.handle.transport.pattern();
        let ppq = self.song.ppq as f32;
        let bar = self.song.bar();
        let beat = self.song.ppq.max(1);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Playlist  -  Arrangement  -  {}", self.song.pattern_name(pat))).color(TEXT));
            ui.label(egui::RichText::new("   left-click: place the selected pattern / move, right-click: delete, Ctrl+wheel: zoom, Alt: no snap").color(TEXT_DIM).small());
        });
        let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(Vec2::new(200.0, 120.0)), Sense::click_and_drag());
        let grid = Rect::from_min_max(rect.min + Vec2::new(HEAD_W, RULER_H), rect.max);
        let heads = Rect::from_min_max(Pos2::new(rect.left(), grid.top()), Pos2::new(grid.left(), rect.bottom()));
        let ruler = Rect::from_min_max(Pos2::new(grid.left(), rect.top()), Pos2::new(rect.right(), grid.top()));
        let px = self.playlist.zoom * 96.0 / ppq;
        let th = self.playlist.track_h;
        let ntracks = self.song.tracks.len();

        if resp.hovered() {
            let (scroll, zoom, mods) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
            if zoom != 1.0 {
                if mods.alt {
                    self.playlist.track_h = (self.playlist.track_h * zoom).clamp(14.0, 120.0);
                } else {
                    self.playlist.zoom = (self.playlist.zoom * zoom).clamp(0.005, 4.0);
                }
            } else if mods.shift {
                self.playlist.scroll_x -= scroll.y + scroll.x;
            } else {
                self.playlist.scroll_y -= scroll.y;
                self.playlist.scroll_x -= scroll.x;
            }
        }
        self.playlist.scroll_x = self.playlist.scroll_x.max(0.0);
        self.playlist.scroll_y = self.playlist.scroll_y.clamp(0.0, (ntracks as f32 * th - grid.height()).max(0.0));
        let (sx, sy) = (self.playlist.scroll_x, self.playlist.scroll_y);
        let x_of = |tick: f32| grid.left() + tick * px - sx;
        let tick_of = |x: f32| ((x - grid.left() + sx) / px).max(0.0);
        let y_of = |track: usize| grid.top() + track as f32 * th - sy;
        let track_of = |y: f32| (((y - grid.top() + sy) / th).floor().max(0.0) as usize).min(ntracks.saturating_sub(1));

        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, GRID_DARK);
        let first_track = track_of(grid.top());
        let last_track = track_of(grid.bottom());

        // grid
        let last_tick = tick_of(grid.right()) as u32 + bar;
        let mut t = (tick_of(grid.left()) as u32 / bar) * bar;
        let bar_px = bar as f32 * px;
        let every = if bar_px >= 14.0 { 1 } else { (14.0 / bar_px).ceil() as u32 };
        while t <= last_tick {
            let n = t / bar;
            let x = x_of(t as f32);
            if (n / 4) % 2 == 1 {
                p.rect_filled(Rect::from_min_max(Pos2::new(x.max(grid.left()), grid.top()), Pos2::new((x + bar_px).max(grid.left()), grid.bottom())), 0.0, GRID_LIGHT);
            }
            if n % every == 0 && x >= grid.left() {
                p.line_segment([Pos2::new(x, grid.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, LINE_SOFT));
            }
            t += bar;
        }
        for tr in first_track..=last_track {
            let y = y_of(tr);
            p.line_segment([Pos2::new(grid.left(), y), Pos2::new(grid.right(), y)], Stroke::new(1.0_f32, LINE_SOFT));
        }

        // clips
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut hit: Option<usize> = None;
        let gp = ui.painter_at(grid);
        let font = FontId::proportional(10.5);
        for (i, c) in self.song.clips.iter().enumerate() {
            let tr = c.track as usize;
            if tr < first_track || tr > last_track {
                continue;
            }
            let r = Rect::from_min_max(Pos2::new(x_of(c.pos as f32), y_of(tr) + 1.0), Pos2::new(x_of((c.pos + c.len) as f32) - 1.0, y_of(tr) + th));
            if r.right() < grid.left() || r.left() > grid.right() {
                continue;
            }
            let (name, col) = self.clip_look(c);
            let enabled = !c.muted && self.song.tracks.get(tr).map(|t| t.enabled).unwrap_or(true);
            let col = if enabled { col } else { col.gamma_multiply(0.35) };
            gp.rect_filled(r, 2.0, col.gamma_multiply(0.55));
            let title = Rect::from_min_max(r.min, Pos2::new(r.right(), (r.top() + 12.0).min(r.bottom())));
            gp.rect_filled(title, 2.0, col);
            gp.rect_stroke(r, 2.0, Stroke::new(1.0_f32, LINE), egui::StrokeKind::Inside);
            if r.width() > 16.0 {
                gp.with_clip_rect(r.intersect(grid)).text(title.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, &name, font.clone(), Color32::from_gray(245));
            }
            // a picture of what is inside
            let body = Rect::from_min_max(Pos2::new(r.left(), title.bottom()), r.max);
            if body.height() > 6.0 && r.width() > 6.0 {
                match c.src {
                    ClipSrc::Pattern(pi) => {
                        if let Some(pt) = self.song.patterns.get(pi) {
                            if !pt.notes.is_empty() {
                                let lo = pt.notes.iter().map(|n| n.key).min().unwrap_or(60) as f32;
                                let hi = pt.notes.iter().map(|n| n.key).max().unwrap_or(60) as f32;
                                let span = (hi - lo).max(1.0);
                                let cp = gp.with_clip_rect(body.intersect(grid));
                                for n in &pt.notes {
                                    if n.pos < c.offset || n.pos >= c.offset + c.len {
                                        continue;
                                    }
                                    let x0 = x_of((c.pos + n.pos - c.offset) as f32);
                                    let x1 = (x0 + (n.len.max(beat / 4) as f32 * px)).max(x0 + 1.5);
                                    let y = body.bottom() - 2.0 - (n.key as f32 - lo) / span * (body.height() - 4.0);
                                    cp.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.5_f32, Color32::from_gray(240)));
                                }
                            }
                        }
                    }
                    ClipSrc::Audio(ch) => {
                        if let Some(smp) = self.song.channels.get(ch).and_then(|c| c.sample.as_ref()) {
                            // frames per tick at the song tempo
                            let fpt = smp.rate as f32 * 60.0 / self.song.tempo.max(1.0) / ppq;
                            let cp = gp.with_clip_rect(body.intersect(grid));
                            let x_from = r.left().max(grid.left()) as i32;
                            let x_to = r.right().min(grid.right()) as i32;
                            let mid = body.center().y;
                            for x in x_from..x_to {
                                let t0 = (x as f32 - r.left()) / px + c.offset as f32;
                                let a = (t0 * fpt) as usize;
                                let b = (((t0 + 1.0 / px) * fpt) as usize).min(smp.frames.len());
                                if a >= b {
                                    break;
                                }
                                let stride = ((b - a) / 32).max(1);
                                let peak = smp.frames[a..b].iter().step_by(stride).fold(0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
                                let h = (peak * body.height() * 0.5).max(0.5);
                                cp.line_segment([Pos2::new(x as f32, mid - h), Pos2::new(x as f32, mid + h)], Stroke::new(1.0_f32, Color32::from_gray(240)));
                            }
                        }
                    }
                    ClipSrc::Automation(_) => {}
                }
            }
            if let Some(pp) = pointer {
                if r.contains(pp) && grid.contains(pp) {
                    hit = Some(i);
                }
            }
        }

        // play head
        if self.handle.transport.song_mode() {
            let x = x_of(self.handle.transport.tick() as f32);
            if x >= grid.left() && x <= grid.right() {
                p.line_segment([Pos2::new(x, ruler.top()), Pos2::new(x, grid.bottom())], Stroke::new(1.0_f32, GREEN));
            }
        }

        // ruler
        p.rect_filled(ruler, 0.0, PANEL_DARK);
        let label_every = every.max((40.0 / bar_px).ceil() as u32).max(1);
        let mut b = (tick_of(grid.left()) as u32 / bar) * bar;
        while b <= last_tick {
            let n = b / bar;
            let x = x_of(b as f32);
            if n % label_every == 0 && x >= grid.left() - 1.0 {
                p.text(Pos2::new(x + 3.0, ruler.center().y), Align2::LEFT_CENTER, format!("{}", n + 1), FontId::proportional(10.5), TEXT_DIM);
            }
            b += bar;
        }

        // track heads
        p.rect_filled(Rect::from_min_max(rect.min, Pos2::new(grid.left(), grid.top())), 0.0, PANEL_DARK);
        let hp = ui.painter_at(heads);
        let mut toggle: Option<usize> = None;
        let clicked = resp.clicked();
        for tr in first_track..=last_track {
            let y = y_of(tr);
            let r = Rect::from_min_max(Pos2::new(heads.left(), y + 1.0), Pos2::new(heads.right() - 1.0, y + th));
            let t = &self.song.tracks[tr];
            hp.rect_filled(r, 2.0, rgb(t.color).gamma_multiply(if t.enabled { 1.0 } else { 0.5 }));
            let name = if t.name.is_empty() { format!("Track {}", tr + 1) } else { t.name.clone() };
            hp.text(r.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, fit(ui, &name, r.width() - 24.0, &font), font.clone(), TEXT);
            let dot = Pos2::new(r.right() - 9.0, r.center().y);
            hp.circle_filled(dot, 4.0, if t.enabled { GREEN } else { PANEL_DARK });
            hp.circle_stroke(dot, 4.0, Stroke::new(1.0_f32, LINE));
            if clicked && pointer.map(|pp| r.contains(pp)).unwrap_or(false) {
                toggle = Some(tr);
            }
        }
        if let Some(tr) = toggle {
            self.song.tracks[tr].enabled = !self.song.tracks[tr].enabled;
            self.dirty = true;
        }

        // interaction
        let (pressed, down, released, sec_down, alt) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.secondary_down(), i.modifiers.alt));
        let unit = if bar_px >= 120.0 { beat } else { bar };
        let snap = |t: f32| if alt { t.round().max(0.0) as u32 } else { ((t / unit as f32).floor().max(0.0) as u32) * unit };
        if let Some(pp) = pointer.filter(|_| resp.hovered() || self.playlist.drag.is_some()) {
            if ruler.contains(pp) && down {
                if !self.handle.transport.song_mode() {
                    self.handle.transport.set_song_mode(true);
                }
                self.handle.transport.seek(snap(tick_of(pp.x)));
            } else if grid.contains(pp) || self.playlist.drag.is_some() {
                let track = track_of(pp.y) as u16;
                if sec_down {
                    if let Some(i) = hit {
                        self.song.clips.remove(i);
                        self.playlist.drag = None;
                        self.dirty = true;
                    }
                } else if pressed && grid.contains(pp) {
                    match hit {
                        Some(i) => {
                            self.playlist.drag = Some((i, tick_of(pp.x) - self.song.clips[i].pos as f32));
                            if let ClipSrc::Pattern(pi) = self.song.clips[i].src {
                                self.handle.transport.set_pattern(pi);
                            }
                        }
                        None => {
                            let pos = snap(tick_of(pp.x));
                            let len = self.song.pattern_length(pat);
                            self.song.clips.push(Clip { pos, len, track, src: ClipSrc::Pattern(pat), offset: 0, muted: false });
                            self.playlist.drag = Some((self.song.clips.len() - 1, tick_of(pp.x) - pos as f32));
                            self.dirty = true;
                        }
                    }
                } else if down {
                    if let Some((i, dt)) = self.playlist.drag {
                        if let Some(c) = self.song.clips.get_mut(i) {
                            let pos = snap(tick_of(pp.x) - dt + if alt { 0.0 } else { unit as f32 * 0.5 });
                            if c.pos != pos || c.track != track {
                                c.pos = pos;
                                c.track = track;
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
        }
        if released || !down {
            self.playlist.drag = None;
        }
    }
}
