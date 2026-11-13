//! A score sheet: the notes of one channel written out on a treble and a bass staff, as a PDF file.
//!
//! The notes are brought to a grid of sixteenths. Each staff carries one line of chords: a chord lasts until the
//! next one on its staff starts, longer notes are tied over the bar lines, and silence becomes rests. Everything
//! is drawn from lines and curves of our own, so the file needs no music font.

use std::fmt::Write as _;

/// One note: start and length in ticks, and its key (60 is the C between the staves).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetNote {
    pub pos: u32,
    pub len: u32,
    pub key: u8,
}

/// What stands at one place of a staff: keys sounding together (none for a rest), all of one written length.
#[derive(Debug, Clone, PartialEq)]
pub struct Sign {
    /// 0 the treble staff, 1 the bass staff.
    pub staff: u8,
    pub bar: u32,
    /// Sixteenths from the start of the bar.
    pub at: u32,
    /// Written length in sixteenths: 16, 12, 8, 6, 4, 3, 2 or 1.
    pub value: u32,
    pub keys: Vec<u8>,
    /// The same keys carry on in the next sign.
    pub tied: bool,
    /// A rest that fills its whole bar.
    pub whole_bar: bool,
}

const VALUES: [u32; 8] = [16, 12, 8, 6, 4, 3, 2, 1];

/// Cut a stretch of sixteenths into written lengths, longest first.
fn pieces(mut len: u32) -> Vec<u32> {
    let mut out = vec![];
    while len > 0 {
        let v = VALUES.iter().copied().find(|v| *v <= len).unwrap_or(1);
        out.push(v);
        len -= v;
    }
    out
}

/// The signs of both staves, in order, and the number of bars. `bar16` is the length of a bar in sixteenths.
pub fn layout(notes: &[SheetNote], ppq: u32, bar16: u32) -> (Vec<Sign>, u32) {
    let unit = (ppq / 4).max(1) as f32;
    let bar16 = bar16.max(1);
    let grid: Vec<(u32, u32, u8)> = notes.iter().map(|n| ((n.pos as f32 / unit).round() as u32, ((n.len as f32 / unit).round() as u32).max(1), n.key)).collect();
    let end = grid.iter().map(|n| n.0 + n.1).max().unwrap_or(0);
    let bars = end.div_ceil(bar16).max(1);
    let mut out = vec![];
    for staff in 0..2u8 {
        let mut mine: Vec<(u32, u32, u8)> = grid.iter().copied().filter(|n| (n.2 >= 60) == (staff == 0)).collect();
        mine.sort();
        // chords: every start on this staff, with the keys starting there, lasting until the next start at most
        let mut chords: Vec<(u32, u32, Vec<u8>)> = vec![];
        for n in &mine {
            match chords.last_mut() {
                Some(c) if c.0 == n.0 => {
                    c.1 = c.1.max(n.1);
                    if !c.2.contains(&n.2) {
                        c.2.push(n.2);
                    }
                }
                _ => chords.push((n.0, n.1, vec![n.2])),
            }
        }
        for i in 0..chords.len() {
            if let Some(next) = chords.get(i + 1).map(|c| c.0) {
                chords[i].1 = chords[i].1.min(next - chords[i].0);
            }
            chords[i].2.sort_unstable();
        }
        // sounding and silent stretches, cut at the bar lines
        let put = |from: u32, to: u32, keys: &[u8], out: &mut Vec<Sign>| {
            let mut t = from;
            while t < to {
                let bar = t / bar16;
                let stop = to.min((bar + 1) * bar16);
                if keys.is_empty() && t % bar16 == 0 && stop - t == bar16 {
                    out.push(Sign { staff, bar, at: 0, value: 16, keys: vec![], tied: false, whole_bar: true });
                } else {
                    // rests read best with the short ones first when they lead up to a beat
                    let mut parts = pieces(stop - t);
                    if keys.is_empty() && t % 4 != 0 {
                        parts.reverse();
                    }
                    let mut at = t;
                    let n = parts.len();
                    for (k, v) in parts.into_iter().enumerate() {
                        let last = k + 1 == n && stop == to;
                        out.push(Sign { staff, bar, at: at % bar16, value: v, keys: keys.to_vec(), tied: !keys.is_empty() && !last, whole_bar: false });
                        at += v;
                    }
                }
                t = stop;
            }
        };
        let mut at = 0;
        for c in &chords {
            put(at, c.0, &[], &mut out);
            put(c.0, c.0 + c.1, &c.2, &mut out);
            at = c.0 + c.1;
        }
        put(at, bars * bar16, &[], &mut out);
    }
    (out, bars)
}

/// The place of a key on the staves: steps of the scale above the C five octaves below key 60, and whether it
/// is written with a sharp.
fn step_of(key: u8) -> (i32, bool) {
    const STEP: [(i32, bool); 12] = [(0, false), (0, true), (1, false), (1, true), (2, false), (3, false), (3, true), (4, false), (4, true), (5, false), (5, true), (6, false)];
    let (s, sharp) = STEP[key as usize % 12];
    ((key as i32 / 12) * 7 + s, sharp)
}

/// Half spaces above the bottom line of its staff.
fn place(key: u8, staff: u8) -> (i32, bool) {
    let (d, sharp) = step_of(key);
    // bottom lines: the E above key 60 for the treble staff, the G an octave and a fourth below it for the bass
    let bottom = if staff == 0 { step_of(64).0 } else { step_of(43).0 };
    (d - bottom, sharp)
}

/// The drawing of one page, in points from the top left corner.
struct Page(String);

impl Page {
    fn new() -> Self {
        Page(String::from("1 0 0 -1 0 842 cm\n0 G 0 g\n"))
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), w: f32) {
        let _ = writeln!(self.0, "{w:.2} w {:.2} {:.2} m {:.2} {:.2} l S", a.0, a.1, b.0, b.1);
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let _ = writeln!(self.0, "{x:.2} {y:.2} {w:.2} {h:.2} re f");
    }
    fn oval(&mut self, c: (f32, f32), rx: f32, ry: f32, fill: bool) {
        let k = 0.5523;
        let (x, y) = c;
        let _ = writeln!(
            self.0,
            "1.1 w {:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {}",
            x + rx, y,
            x + rx, y + ry * k, x + rx * k, y + ry, x, y + ry,
            x - rx * k, y + ry, x - rx, y + ry * k, x - rx, y,
            x - rx, y - ry * k, x - rx * k, y - ry, x, y - ry,
            x + rx * k, y - ry, x + rx, y - ry * k, x + rx, y,
            if fill { "f" } else { "S" }
        );
    }
    /// A stroked run of curves: the start, then three points for each curve.
    fn curve(&mut self, pts: &[(f32, f32)], w: f32) {
        let _ = write!(self.0, "{w:.2} w 1 J 1 j {:.2} {:.2} m", pts[0].0, pts[0].1);
        for c in pts[1..].chunks_exact(3) {
            let _ = write!(self.0, " {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c", c[0].0, c[0].1, c[1].0, c[1].1, c[2].0, c[2].1);
        }
        self.0.push_str(" S 0 J 0 j\n");
    }
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, s: &str) {
        let mut t = String::new();
        for ch in s.chars() {
            match ch {
                '(' | ')' | '\\' => {
                    t.push('\\');
                    t.push(ch);
                }
                ' '..='~' => t.push(ch),
                c if (c as u32) < 256 => {
                    let _ = write!(t, "\\{:03o}", c as u32);
                }
                _ => t.push('?'),
            }
        }
        let _ = writeln!(self.0, "BT /F{} {size:.1} Tf 1 0 0 -1 {x:.2} {y:.2} Tm ({t}) Tj ET", if bold { 2 } else { 1 });
    }
}

const SP: f32 = 6.0;
const LEFT: f32 = 44.0;
const RIGHT: f32 = 551.0;
/// From the top line of the treble staff to the top line of the bass staff.
const STAFF_GAP: f32 = 66.0;
const SYSTEM_GAP: f32 = 148.0;

fn treble_clef(p: &mut Page, x: f32, g: f32) {
    let at = |a: f32, b: f32| (x + a * SP, g + b * SP);
    p.curve(
        &[
            at(0.0, 0.5),
            at(-0.9, 0.5), at(-0.9, -0.9), at(0.1, -0.9),
            at(1.3, -0.9), at(1.6, 1.0), at(0.0, 1.05),
            at(-1.7, 1.1), at(-2.0, -0.9), at(-0.6, -2.3),
            at(0.4, -3.3), at(1.0, -4.2), at(0.4, -5.2),
            at(-0.2, -5.0), at(-0.5, -4.0), at(-0.3, -3.0),
            at(-0.1, -1.5), at(0.3, 0.5), at(0.5, 1.9),
            at(0.7, 2.9), at(-0.6, 3.0), at(-0.9, 2.2),
        ],
        1.3,
    );
    p.oval(at(-0.75, 2.2), 0.33 * SP, 0.33 * SP, true);
}

fn bass_clef(p: &mut Page, x: f32, f: f32) {
    let at = |a: f32, b: f32| (x + a * SP, f + b * SP);
    p.oval(at(-1.0, 0.0), 0.36 * SP, 0.36 * SP, true);
    p.curve(&[at(-1.0, 0.0), at(-1.0, -1.1), at(1.2, -1.3), at(1.2, 0.2), at(1.2, 1.6), at(0.0, 2.6), at(-1.4, 3.2)], 1.5);
    p.oval(at(1.9, -0.5), 0.22 * SP, 0.22 * SP, true);
    p.oval(at(1.9, 0.5), 0.22 * SP, 0.22 * SP, true);
}

fn rest(p: &mut Page, x: f32, bottom: f32, value: u32, whole_bar: bool) {
    let y = |pos: f32| bottom - pos * SP / 2.0;
    let base = match value {
        12 => 8,
        6 => 4,
        3 => 2,
        v => v,
    };
    match base {
        16 => p.rect(x - 4.0, y(6.0), 8.0, 3.0),
        _ if whole_bar => p.rect(x - 4.0, y(6.0), 8.0, 3.0),
        8 => p.rect(x - 4.0, y(4.0) - 3.0, 8.0, 3.0),
        4 => {
            for (a, b) in [((-1.8, 7.0), (2.0, 5.3)), ((2.0, 5.3), (-1.8, 4.0)), ((-1.8, 4.0), (2.2, 2.6))] {
                p.line((x + a.0, y(a.1)), (x + b.0, y(b.1)), 1.7);
            }
            p.curve(&[(x + 2.2, y(2.6)), (x - 2.5, y(3.2)), (x - 2.8, y(1.4)), (x + 0.4, y(0.8))], 1.3);
        }
        n => {
            let dots = if n == 2 { 1 } else { 2 };
            for d in 0..dots {
                let pos = 5.0 - 2.0 * d as f32;
                p.oval((x - 2.2 - 0.6 * d as f32, y(pos)), 1.5, 1.5, true);
                p.line((x - 2.2 - 0.6 * d as f32, y(pos) + 1.2), (x + 3.0 - 0.6 * d as f32, y(pos + 0.5)), 1.1);
            }
            p.line((x + 3.2, y(5.7)), (x + 3.2 - 0.3 * (5.7 - (1.2 - 2.0 * (dots - 1) as f32)) * 1.0, y(1.2 - 2.0 * (dots - 1) as f32)), 1.2);
        }
    }
    if matches!(value, 12 | 6 | 3) && !whole_bar {
        p.oval((x + 7.0, y(5.0)), 1.1, 1.1, true);
    }
}

/// The sheet as a PDF file. `sig` is the time signature as written (4, 4); `bar16` the bar length in sixteenths.
pub fn sheet_pdf(title: &str, notes: &[SheetNote], ppq: u32, sig: (u8, u8), bar16: u32, tempo: f32) -> Vec<u8> {
    let bar16 = bar16.max(1);
    let (signs, bars) = layout(notes, ppq, bar16);
    let per_line: u32 = if bar16 > 16 { 3 } else { 4 };
    let lines = bars.div_ceil(per_line);
    let mut pages: Vec<Page> = vec![];
    let mut top = 0.0;
    for line in 0..lines {
        // a new page when the system no longer fits
        if pages.is_empty() || top + STAFF_GAP + 4.0 * SP + 40.0 > 842.0 {
            let mut p = Page::new();
            if pages.is_empty() {
                p.text(LEFT, 52.0, 17.0, true, title);
                p.text(LEFT, 70.0, 10.0, false, &format!("Tempo {}", (tempo * 1000.0).round() / 1000.0));
                top = 118.0;
            } else {
                p.text(RIGHT - 30.0, 30.0, 9.0, false, &format!("{}", pages.len() + 1));
                top = 70.0;
            }
            pages.push(p);
        }
        let p = pages.last_mut().unwrap();
        let tops = [top, top + STAFF_GAP];
        let bottoms = [tops[0] + 4.0 * SP, tops[1] + 4.0 * SP];
        p.line((LEFT, tops[0]), (LEFT, bottoms[1]), 0.8);
        p.rect(LEFT - 4.5, tops[0], 2.2, bottoms[1] - tops[0]);
        treble_clef(p, LEFT + 14.0, bottoms[0] - SP);
        bass_clef(p, LEFT + 14.0, tops[1] + SP);
        let mut x0 = LEFT + 30.0;
        if line == 0 {
            for t in tops {
                p.text(x0, t + 2.0 * SP - 0.5, 15.0, true, &sig.0.to_string());
                p.text(x0, t + 4.0 * SP - 0.5, 15.0, true, &sig.1.to_string());
            }
            x0 += 16.0;
        }
        p.text(LEFT, tops[0] - 8.0, 8.0, false, &format!("{}", line * per_line + 1));
        let bar_w = (RIGHT - x0) / per_line as f32;
        let x_of = |bar: u32, at: u32| x0 + (bar - line * per_line) as f32 * bar_w + 12.0 + at as f32 / bar16 as f32 * (bar_w - 16.0);
        // the staves stop at the closing bar line
        let right = x0 + (bars - line * per_line).min(per_line) as f32 * bar_w + if (line + 1) * per_line >= bars { 4.0 } else { 0.0 };
        for t in tops {
            for l in 0..5 {
                p.line((LEFT, t + l as f32 * SP), (right, t + l as f32 * SP), 0.6);
            }
        }
        for b in 0..per_line {
            let bar = line * per_line + b;
            let x = x0 + (b + 1) as f32 * bar_w;
            if bar < bars {
                p.line((x, tops[0]), (x, bottoms[1]), if bar + 1 == bars { 0.8 } else { 0.7 });
                if bar + 1 == bars {
                    p.rect(x + 1.6, tops[0], 2.4, bottoms[1] - tops[0]);
                }
            }
        }
        for staff in 0..2u8 {
            let bottom = bottoms[staff as usize];
            let y = |pos: i32| bottom - pos as f32 * SP / 2.0;
            // which places carry a sharp so far in the bar
            let mut sharp_in: std::collections::BTreeMap<i32, bool> = Default::default();
            let mut bar_now = u32::MAX;
            let mine: Vec<&Sign> = signs.iter().filter(|s| s.staff == staff && s.bar / per_line == line).collect();
            for (k, s) in mine.iter().enumerate() {
                if s.bar != bar_now {
                    bar_now = s.bar;
                    sharp_in.clear();
                }
                let x = if s.whole_bar { x0 + (s.bar - line * per_line) as f32 * bar_w + bar_w / 2.0 } else { x_of(s.bar, s.at) };
                if s.keys.is_empty() {
                    rest(p, x, bottom, s.value, s.whole_bar);
                    continue;
                }
                let places: Vec<(i32, bool)> = s.keys.iter().map(|k| place(*k, staff)).collect();
                let (lo, hi) = (places[0].0, places[places.len() - 1].0);
                let up = (lo + hi) < 8;
                let (rx, ry) = (3.9, 2.7);
                let solid = !matches!(s.value, 16 | 12 | 8);
                // ledger lines
                let mut l = -2;
                while l >= lo {
                    p.line((x - 6.5, y(l)), (x + 6.5, y(l)), 0.7);
                    l -= 2;
                }
                let mut l = 10;
                while l <= hi {
                    p.line((x - 6.5, y(l)), (x + 6.5, y(l)), 0.7);
                    l += 2;
                }
                let mut shifted = false;
                let mut last_pos = i32::MIN;
                for &(pos, sharp) in &places {
                    // a note right next to the one below it sits on the other side of the stem
                    shifted = pos - last_pos == 1 && !shifted;
                    last_pos = pos;
                    let cx = if shifted { x + if up { 2.0 * rx - 1.0 } else { -(2.0 * rx - 1.0) } } else { x };
                    if solid {
                        p.oval((cx, y(pos)), rx, ry, true);
                    } else {
                        p.oval((cx, y(pos)), rx - 0.5, ry - 0.5, false);
                    }
                    if sharp_in.get(&pos).copied().unwrap_or(false) != sharp {
                        sharp_in.insert(pos, sharp);
                        let ax = x - rx - 6.0;
                        let ay = y(pos);
                        if sharp {
                            p.line((ax - 1.3, ay - 5.0), (ax - 1.3, ay + 5.5), 0.7);
                            p.line((ax + 1.3, ay - 5.5), (ax + 1.3, ay + 5.0), 0.7);
                            p.line((ax - 3.0, ay - 1.2), (ax + 3.0, ay - 2.6), 1.5);
                            p.line((ax - 3.0, ay + 2.6), (ax + 3.0, ay + 1.2), 1.5);
                        } else {
                            p.line((ax - 1.4, ay - 6.0), (ax - 1.4, ay + 2.4), 0.7);
                            p.line((ax + 1.4, ay - 2.4), (ax + 1.4, ay + 6.0), 0.7);
                            p.line((ax - 1.4, ay - 1.4), (ax + 1.4, ay - 2.4), 1.4);
                            p.line((ax - 1.4, ay + 2.4), (ax + 1.4, ay + 1.4), 1.4);
                        }
                    }
                    if matches!(s.value, 12 | 6 | 3) {
                        p.oval((x + rx + 3.6 + if up && places.len() > 1 { rx } else { 0.0 }, y(pos) - if pos % 2 == 0 { SP / 2.0 } else { 0.0 }), 1.1, 1.1, true);
                    }
                    if s.tied {
                        // an arc towards the next sign, or to the end of the line when that is on the next one
                        let to = mine.get(k + 1).filter(|n| !n.keys.is_empty()).map(|n| x_of(n.bar, n.at) - rx - 1.0).unwrap_or(right - 2.0);
                        let (a, bend) = (x + rx + 1.0, if up { 5.0 } else { -5.0 });
                        let yy = y(pos) + if up { 3.5 } else { -3.5 };
                        p.curve(&[(a, yy), (a + (to - a) * 0.3, yy + bend), (a + (to - a) * 0.7, yy + bend), (to, yy)], 0.9);
                    }
                }
                if s.value < 16 {
                    let len = 3.5 * SP;
                    let (sx, from, to) = if up { (x + rx - 0.5, y(lo), y(hi) - len) } else { (x - rx + 0.5, y(hi), y(lo) + len) };
                    p.line((sx, from), (sx, to), 0.9);
                    let flags = match s.value {
                        3 | 2 => 1,
                        1 => 2,
                        _ => 0,
                    };
                    for f in 0..flags {
                        let d = if up { 1.0 } else { -1.0 };
                        let ty = to + d * 5.0 * f as f32;
                        p.curve(&[(sx, ty), (sx + 1.0, ty + d * 5.0), (sx + 7.5, ty + d * 6.5), (sx + 5.0, ty + d * 13.0)], 1.3);
                    }
                }
            }
        }
        top += SYSTEM_GAP;
    }

    // the file: catalogue, page list, two fonts, then a page and its drawing for each sheet
    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = vec![];
    let n = pages.len();
    let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = (0..n).map(|i| format!("{} 0 R ", 5 + i * 2)).collect();
    obj(&mut out, format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>").as_bytes());
    obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
    for (i, p) in pages.iter().enumerate() {
        obj(&mut out, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", 6 + i * 2).as_bytes());
        obj(&mut out, format!("<< /Length {} >>\nstream\n{}\nendstream", p.0.len() + 1, p.0).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_become_chords_ties_and_rests() {
        // a C chord for a beat, a rest, then a low note held over the bar line
        let n = |pos, len, key| SheetNote { pos, len, key };
        let notes = [n(0, 96, 60), n(0, 96, 64), n(0, 96, 67), n(192, 96 * 4, 40)];
        let (signs, bars) = layout(&notes, 96, 16);
        assert_eq!(bars, 2);
        let treble: Vec<&Sign> = signs.iter().filter(|s| s.staff == 0).collect();
        assert_eq!((treble[0].value, treble[0].keys.clone(), treble[0].tied), (4, vec![60, 64, 67], false));
        // the rest of bar one is a dotted half of silence, bar two a whole-bar rest
        assert_eq!((treble[1].value, treble[1].keys.is_empty(), treble[1].at), (12, true, 4));
        assert!(treble[2].whole_bar && treble[2].bar == 1);
        let bass: Vec<&Sign> = signs.iter().filter(|s| s.staff == 1).collect();
        assert_eq!((bass[0].value, bass[0].keys.is_empty()), (8, true));
        assert_eq!((bass[1].at, bass[1].value, bass[1].tied, bass[1].bar), (8, 8, true, 0));
        assert_eq!((bass[2].at, bass[2].value, bass[2].tied, bass[2].bar), (0, 8, false, 1));
        // every bar of every staff is filled exactly
        for staff in 0..2 {
            for bar in 0..bars {
                let total: u32 = signs.iter().filter(|s| s.staff == staff && s.bar == bar).map(|s| if s.whole_bar { 16 } else { s.value }).sum();
                assert_eq!(total, 16);
            }
        }
    }

    #[test]
    fn keys_land_on_the_right_lines() {
        assert_eq!(place(64, 0), (0, false), "the treble staff's bottom line");
        assert_eq!(place(60, 0), (-2, false), "one line below the treble staff");
        assert_eq!(place(77, 0), (8, false), "its top line");
        assert_eq!(place(43, 1), (0, false), "the bass staff's bottom line");
        assert_eq!(place(58, 1), (8, true), "a sharpened A on the bass staff's top line");
    }

    #[test]
    fn the_sheet_is_a_well_formed_file() {
        let notes: Vec<SheetNote> = (0..200).map(|i| SheetNote { pos: i * 48, len: 48, key: 48 + (i * 7 % 30) as u8 }).collect();
        let pdf = sheet_pdf("A (test) sheet", &notes, 96, (4, 4), 16, 140.0);
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4") && s.trim_end().ends_with("%%EOF"));
        // the cross reference table points at every object
        let xref: usize = s[s.rfind("startxref").unwrap() + 10..].lines().next().unwrap().parse().unwrap();
        assert!(s[xref..].starts_with("xref"));
        for (i, l) in s[xref..].lines().skip(3).take_while(|l| l.ends_with("n ")).enumerate() {
            let at: usize = l[..10].parse().unwrap();
            assert!(s[at..].starts_with(&format!("{} 0 obj", i + 1)));
        }
        assert!(s.matches("/Type /Page ").count() >= 2, "two hundred notes need more than one page");
        assert!(s.contains("(A \\(test\\) sheet)"));
    }
}
