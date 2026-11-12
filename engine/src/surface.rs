//! The control panel: a board of knobs, sliders, buttons and pads that make no sound and only send their
//! positions out, for cables and links to carry to other units' settings.
//!
//! The stored block is a version number and then tagged pieces, each with a 64-bit length. A control is
//! the run of pieces from 2100 to 2101: its name (2103), its place on the board as four numbers (2104),
//! one piece per output with the position, the resting position and the output's number (2102), and its
//! looks as lines of `Key = value` text (2105 in full, 2106 the colours, 2107 the plain settings).
//! Whatever the range a control shows, what it sends always runs from nought to one.

/// What a control is, told by the class named in its looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    Knob,
    Slider,
    Button,
    Check,
    Digits,
    Pad,
    Label,
    Bevel,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub what: What,
    pub name: String,
    /// Left, top, width and height on the board.
    pub rect: [f32; 4],
    /// The numbers of its outputs (a pad has two), which are also its places in the unit's settings.
    pub outs: Vec<u16>,
    /// Where each output rests.
    pub rest: Vec<f32>,
    pub caption: String,
    /// Red, green, blue of its main colour, when one is stored.
    pub color: Option<[u8; 3]>,
    pub text_color: Option<[u8; 3]>,
    pub font: f32,
    pub upright: bool,
    /// A button that stays down when pressed.
    pub stays: bool,
    /// The range a digit wheel shows.
    pub range: (f32, f32),
}

fn pieces(b: &[u8]) -> Vec<(u32, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut at = 4;
    while at + 12 <= b.len() {
        let id = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let len = u64::from_le_bytes(b[at + 4..at + 12].try_into().unwrap()) as usize;
        at += 12;
        if len > b.len() - at {
            break;
        }
        out.push((id, at..at + len));
        at += len;
    }
    out
}

fn wide(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

fn f32at(b: &[u8], at: usize) -> f32 {
    b.get(at..at + 4).map_or(0.0, |c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
}

/// A stored colour: blue in the high byte, red in the low.
fn rgb(v: &str) -> Option<[u8; 3]> {
    let n = v.trim().parse::<i64>().ok()? as u32;
    Some([n as u8, (n >> 8) as u8, (n >> 16) as u8])
}

/// The board's size, where one is stored.
pub fn size(block: &[u8]) -> (f32, f32) {
    for (id, r) in pieces(block) {
        if id == 2002 && r.len() >= 8 {
            let w = u32::from_le_bytes(block[r.start..r.start + 4].try_into().unwrap());
            let h = u32::from_le_bytes(block[r.start + 4..r.start + 8].try_into().unwrap());
            if (16..=8192).contains(&w) && (16..=8192).contains(&h) {
                return (w as f32, h as f32);
            }
        }
    }
    (540.0, 454.0)
}

/// Every control on the board, in stored order (which is also back to front).
pub fn controls(block: &[u8]) -> Vec<Control> {
    let mut out = Vec::new();
    let mut cur: Option<Control> = None;
    for (id, r) in pieces(block) {
        let d = &block[r];
        match (id, &mut cur) {
            (2100, _) => {
                cur = Some(Control { what: What::Other, name: String::new(), rect: [0.0; 4], outs: vec![], rest: vec![], caption: String::new(), color: None, text_color: None, font: 12.0, upright: true, stays: false, range: (0.0, 1.0) })
            }
            (2101, _) => out.extend(cur.take()),
            (2103, Some(c)) => c.name = wide(d),
            (2104, Some(c)) if d.len() >= 16 => c.rect = [f32at(d, 0), f32at(d, 4), f32at(d, 8), f32at(d, 12)],
            (2102, Some(c)) if d.len() >= 12 => {
                c.outs.push(u32::from_le_bytes([d[8], d[9], d[10], d[11]]).min(u16::MAX as u32) as u16);
                c.rest.push(f32at(d, 4));
            }
            (2105 | 2106 | 2107, Some(c)) => {
                let text = wide(d);
                let mut slider = None;
                for line in text.lines() {
                    let line = line.trim();
                    if let Some(class) = line.strip_prefix("control ") {
                        c.what = match class.trim() {
                            "TVectorWheel" => What::Knob,
                            "TVectorSlider" => What::Slider,
                            "TQuickBtn" => What::Button,
                            "TVectorCheckBox" => What::Check,
                            "TVectorDigiWheel" => What::Digits,
                            "TQuickXYZ" | "TDashboardXY" => What::Pad,
                            "TQuickLabel" => What::Label,
                            "TVectorBevel" => What::Bevel,
                            _ => What::Other,
                        };
                        continue;
                    }
                    let Some((k, v)) = line.split_once(" = ") else {
                        if let Some(k) = line.strip_suffix(" =") {
                            if k == "Caption" {
                                c.caption.clear();
                            }
                        }
                        continue;
                    };
                    match k {
                        "Caption" => c.caption = v.trim().to_string(),
                        "Color" | "BackgroundColor" => c.color = rgb(v).or(c.color),
                        "FillColor" | "CheckOnColor" => slider = rgb(v),
                        "QuickFont.Color" => c.text_color = rgb(v).or(c.text_color),
                        "QuickFont.Height" => c.font = v.trim().parse().unwrap_or(c.font),
                        "Orientation" => c.upright = v.trim() != "trHorizontal",
                        "StayDown" => c.stays = v.trim() == "True",
                        "Min" => c.range.0 = v.trim().parse().unwrap_or(c.range.0),
                        "Max" => c.range.1 = v.trim().parse().unwrap_or(c.range.1),
                        _ => {}
                    }
                }
                if matches!(c.what, What::Knob | What::Slider | What::Check) {
                    c.color = slider.or(c.color);
                }
            }
            _ => {}
        }
    }
    out
}

/// The positions of the outputs, by output number.
pub fn decode(block: &[u8]) -> Option<Vec<f32>> {
    if block.len() < 4 || u32::from_le_bytes([block[0], block[1], block[2], block[3]]) > 64 {
        return None;
    }
    let ps = pieces(block);
    if !ps.iter().any(|(id, _)| *id == 2000) {
        return None;
    }
    let mut p = Vec::new();
    for (id, r) in ps {
        if id == 2102 && r.len() >= 12 {
            let d = &block[r];
            let k = u32::from_le_bytes([d[8], d[9], d[10], d[11]]) as usize;
            if k < 4096 {
                if p.len() <= k {
                    p.resize(k + 1, 0.0);
                }
                p[k] = f32at(d, 0).clamp(0.0, 1.0);
            }
        }
    }
    Some(p)
}

/// The block with the outputs' positions written back in.
pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let mut out = base.map_or_else(fresh, <[u8]>::to_vec);
    for (id, r) in pieces(&out) {
        if id == 2102 && r.len() >= 12 {
            let k = u32::from_le_bytes(out[r.start + 8..r.start + 12].try_into().unwrap()) as usize;
            if let Some(v) = p.get(k) {
                // an untouched position keeps its stored bytes
                if (f32at(&out, r.start).clamp(0.0, 1.0) - v).abs() > 1e-7 {
                    out[r.start..r.start + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
    }
    out
}

/// An empty board.
pub fn fresh() -> Vec<u8> {
    let mut v = 1u32.to_le_bytes().to_vec();
    let mut head = [0u8; 64];
    head[0] = 1;
    for (id, d) in [(2000u32, &head[..]), (2002, &[0x1c, 2, 0, 0, 0xc6, 1, 0, 0][..])] {
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(&(d.len() as u64).to_le_bytes());
        v.extend_from_slice(d);
    }
    v
}

/// The block taken apart for editing: its pieces, and whatever bytes follow the last whole one.
struct Parts {
    version: [u8; 4],
    list: Vec<(u32, Vec<u8>)>,
    tail: Vec<u8>,
}

impl Parts {
    fn of(block: &[u8]) -> Parts {
        let block = if block.len() < 4 { fresh() } else { block.to_vec() };
        let ps = pieces(&block);
        let end = ps.last().map_or(4, |p| p.1.end);
        Parts { version: [block[0], block[1], block[2], block[3]], list: ps.into_iter().map(|(id, r)| (id, block[r].to_vec())).collect(), tail: block[end..].to_vec() }
    }

    fn block(&self) -> Vec<u8> {
        let mut v = self.version.to_vec();
        for (id, d) in &self.list {
            v.extend_from_slice(&id.to_le_bytes());
            v.extend_from_slice(&(d.len() as u64).to_le_bytes());
            v.extend_from_slice(d);
        }
        v.extend_from_slice(&self.tail);
        v
    }

    /// The pieces of control `i`, from its opening piece to its closing one.
    fn span(&self, i: usize) -> Option<std::ops::Range<usize>> {
        let mut seen = 0;
        let mut open = None;
        for (k, (id, _)) in self.list.iter().enumerate() {
            match *id {
                2100 => open = Some(k),
                2101 => {
                    if let Some(a) = open.take() {
                        if seen == i {
                            return Some(a..k + 1);
                        }
                        seen += 1;
                    }
                }
                _ => {}
            }
        }
        None
    }
}

fn narrow(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The stored number of a colour.
fn stored(c: [u8; 3]) -> String {
    (c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16).to_string()
}

/// `Key = value` set in a looks text: replaced where the key is, added before the closing line where asked.
fn with_line(text: &str, key: &str, value: &str, add: bool) -> String {
    let mut lines: Vec<String> = text.split("\r\n").map(str::to_string).collect();
    let fresh = if value.is_empty() { format!("  {key} =") } else { format!("  {key} = {value}") };
    let is = |l: &String| {
        let l = l.trim();
        l.strip_prefix(key).is_some_and(|r| r == " =" || r.starts_with(" = "))
    };
    if let Some(l) = lines.iter_mut().find(|l| is(l)) {
        *l = fresh;
    } else if add {
        match lines.iter().rposition(|l| l.trim() == "end") {
            Some(at) => lines.insert(at, fresh),
            None => return text.to_string(),
        }
    }
    lines.join("\r\n")
}

/// The key a kind of control keeps its main colour under.
pub fn color_key(what: What) -> &'static str {
    match what {
        What::Knob | What::Slider => "FillColor",
        What::Check => "CheckOnColor",
        What::Label => "QuickFont.Color",
        _ => "Color",
    }
}

/// Control `i` moved or resized.
pub fn set_rect(block: &[u8], i: usize, rect: [f32; 4]) -> Vec<u8> {
    let mut p = Parts::of(block);
    if let Some(s) = p.span(i) {
        let d: Vec<u8> = rect.iter().flat_map(|f| f.to_le_bytes()).collect();
        match p.list[s.clone()].iter_mut().find(|q| q.0 == 2104) {
            Some(q) => q.1 = d,
            None => p.list.insert(s.start + 1, (2104, d)),
        }
    }
    p.block()
}

/// Control `i` renamed.
pub fn set_name(block: &[u8], i: usize, name: &str) -> Vec<u8> {
    let mut p = Parts::of(block);
    if let Some(s) = p.span(i) {
        match p.list[s.clone()].iter_mut().find(|q| q.0 == 2103) {
            Some(q) => q.1 = narrow(name),
            None => p.list.insert(s.start + 1, (2103, narrow(name))),
        }
    }
    p.block()
}

/// One line of control `i`'s looks set, in every copy of them that is stored.
pub fn set_look(block: &[u8], i: usize, key: &str, value: &str) -> Vec<u8> {
    let mut p = Parts::of(block);
    if let Some(s) = p.span(i) {
        // colours live in the colour text and the full one, everything else in the plain text and the full one
        let colour = key.ends_with("Color");
        let mut done = false;
        for q in p.list[s.clone()].iter_mut().filter(|q| matches!(q.0, 2105..=2107)) {
            let add = q.0 == 2105 || (q.0 == 2106) == colour;
            let (was, now) = (wide(&q.1), with_line(&wide(&q.1), key, value, add));
            done |= add || was != now;
            q.1 = narrow(&now);
        }
        if !done {
            // a control stored with only some of its texts keeps the line in the first it has
            if let Some(q) = p.list[s].iter_mut().find(|q| matches!(q.0, 2105..=2107)) {
                q.1 = narrow(&with_line(&wide(&q.1), key, value, true));
            }
        }
    }
    p.block()
}

/// Control `i` taken off the board.
pub fn remove(block: &[u8], i: usize) -> Vec<u8> {
    let mut p = Parts::of(block);
    if let Some(s) = p.span(i) {
        p.list.drain(s);
    }
    p.block()
}

/// Control `i` brought in front of the others.
pub fn raise(block: &[u8], i: usize) -> Vec<u8> {
    let mut p = Parts::of(block);
    if let Some(s) = p.span(i) {
        let moved: Vec<_> = p.list.drain(s).collect();
        // after the last control, before anything that follows the controls
        let at = p.list.iter().rposition(|q| q.0 == 2101).map_or(p.list.len(), |k| k + 1);
        p.list.splice(at..at, moved);
    }
    p.block()
}

/// The board resized.
pub fn set_size(block: &[u8], w: f32, h: f32) -> Vec<u8> {
    let mut p = Parts::of(block);
    let mut d = (w.clamp(16.0, 8192.0) as u32).to_le_bytes().to_vec();
    d.extend_from_slice(&(h.clamp(16.0, 8192.0) as u32).to_le_bytes());
    match p.list.iter_mut().find(|q| q.0 == 2002) {
        Some(q) if q.1.len() >= 8 => q.1[..8].copy_from_slice(&d),
        Some(q) => q.1 = d,
        None => {
            let at = p.list.iter().position(|q| q.0 == 2100).unwrap_or(p.list.len());
            p.list.insert(at, (2002, d));
        }
    }
    p.block()
}

/// A new control of a kind put on the board at a place, in front of the others. Its outputs take the next free numbers.
pub fn add(block: &[u8], what: What, at: (f32, f32)) -> Vec<u8> {
    let mut p = Parts::of(block);
    let existing = controls(block);
    let next = existing.iter().flat_map(|c| c.outs.iter()).map(|&o| o as u32 + 1).max().unwrap_or(0);
    let n = existing.iter().filter(|c| c.what == what).count() + 1;
    let (code, class, word, size, outs): (u32, &str, &str, [f32; 2], u32) = match what {
        What::Knob => (0, "TVectorWheel", "Knob", [40.0, 40.0], 1),
        What::Pad => (1, "TQuickXYZ", "Pad", [120.0, 120.0], 2),
        What::Slider => (2, "TVectorSlider", "Slider", [24.0, 110.0], 1),
        What::Button => (3, "TQuickBtn", "Button", [64.0, 24.0], 1),
        What::Check => (4, "TVectorCheckBox", "Switch", [72.0, 20.0], 1),
        What::Bevel => (5, "TVectorBevel", "Frame", [140.0, 100.0], 0),
        What::Digits => (7, "TVectorDigiWheel", "Number", [48.0, 22.0], 1),
        What::Label | What::Other => (6, "TQuickLabel", "Label", [80.0, 20.0], 0),
    };
    let name = format!("{word} {n}");
    let (plain, colours): (String, String) = match what {
        What::Knob => ("  Min = 0\r\n  Max = 65536\r\n  DefaultValue = 32768\r\n".into(), format!("  FillColor = {}\r\n", stored([255, 160, 60]))),
        What::Slider => ("  Min = 0\r\n  Max = 65536\r\n  DefaultValue = 32768\r\n  Orientation = trVertical\r\n".into(), format!("  FillColor = {}\r\n", stored([255, 160, 60]))),
        What::Button => (format!("  Caption = {name}\r\n  StayDown = True\r\n  QuickFont.Height = 12\r\n"), String::new()),
        What::Check => (format!("  Caption = {name}\r\n  Checked = False\r\n  QuickFont.Height = 12\r\n"), format!("  CheckOnColor = {}\r\n", stored([255, 160, 60]))),
        What::Digits => ("  Min = 0\r\n  Max = 100\r\n  DefaultValue = 0\r\n".into(), String::new()),
        What::Label | What::Other => (format!("  Caption = {name}\r\n  QuickFont.Height = 14\r\n"), format!("  QuickFont.Color = {}\r\n", stored([220, 220, 220]))),
        What::Pad | What::Bevel => (String::new(), String::new()),
    };
    let wrap = |body: &str| narrow(&format!("control {class}\r\n{body}end\r\n"));
    let mut head = [0u8; 32];
    head[..4].copy_from_slice(&code.to_le_bytes());
    let mut new: Vec<(u32, Vec<u8>)> = vec![(2100, head.to_vec()), (2103, narrow(&name)), (2104, [at.0, at.1, size[0], size[1]].iter().flat_map(|f| f.to_le_bytes()).collect())];
    for k in 0..outs {
        let rest: f32 = if matches!(what, What::Knob | What::Slider | What::Pad) { 0.5 } else { 0.0 };
        let mut d = rest.to_le_bytes().to_vec();
        d.extend_from_slice(&rest.to_le_bytes());
        d.extend_from_slice(&(next + k).to_le_bytes());
        new.push((2102, d));
    }
    new.push((2105, wrap(&format!("{plain}{colours}"))));
    new.push((2106, wrap(&colours)));
    new.push((2107, wrap(&plain)));
    new.push((2101, Vec::new()));
    let at = p.list.iter().rposition(|q| q.0 == 2101).map_or(p.list.len(), |k| k + 1);
    p.list.splice(at..at, new);
    p.block()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(v: &mut Vec<u8>, id: u32, d: &[u8]) {
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(&(d.len() as u64).to_le_bytes());
        v.extend_from_slice(d);
    }

    fn text(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    fn board() -> Vec<u8> {
        let mut v = fresh();
        piece(&mut v, 2100, &[0; 32]);
        piece(&mut v, 2103, &text("Cut"));
        piece(&mut v, 2104, &[10.0f32, 20.0, 40.0, 40.0].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>());
        let mut o = 0.25f32.to_le_bytes().to_vec();
        o.extend_from_slice(&0.5f32.to_le_bytes());
        o.extend_from_slice(&3u32.to_le_bytes());
        piece(&mut v, 2102, &o);
        piece(&mut v, 2105, &text("control TVectorWheel\r\n  FillColor = 255\r\nend\r\n"));
        piece(&mut v, 2101, &[]);
        piece(&mut v, 2100, &[6; 32]);
        piece(&mut v, 2103, &text("L"));
        piece(&mut v, 2107, &text("control TQuickLabel\r\n  Caption = Tone\r\n  QuickFont.Height = 24\r\nend\r\n"));
        piece(&mut v, 2101, &[]);
        v
    }

    #[test]
    fn a_board_reads_and_writes_back() {
        let b = board();
        let p = decode(&b).unwrap();
        assert_eq!(p, vec![0.0, 0.0, 0.0, 0.25]);
        assert_eq!(encode(&p, Some(&b)), b);
        let c = controls(&b);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].what, c[0].name.as_str(), c[0].rect, c[0].outs.as_slice(), c[0].color), (What::Knob, "Cut", [10.0, 20.0, 40.0, 40.0], &[3u16][..], Some([255, 0, 0])));
        assert_eq!((c[1].what, c[1].caption.as_str(), c[1].font), (What::Label, "Tone", 24.0));
        let moved = encode(&[0.0, 0.0, 0.0, 0.75], Some(&b));
        assert_eq!(decode(&moved).unwrap()[3], 0.75);
        assert_eq!(decode(&fresh()).unwrap(), Vec::<f32>::new());
        assert_eq!(size(&fresh()), (540.0, 454.0));
    }

    #[test]
    fn a_board_is_edited_piece_by_piece() {
        let b = board();
        // taking a block apart and putting it together changes nothing
        assert_eq!(Parts::of(&b).block(), b);
        let m = set_rect(&b, 0, [1.0, 2.0, 30.0, 31.0]);
        assert_eq!(controls(&m)[0].rect, [1.0, 2.0, 30.0, 31.0]);
        assert_eq!(controls(&m)[1], controls(&b)[1]);
        let m = set_name(&m, 1, "Title");
        assert_eq!(controls(&m)[1].name, "Title");
        let m = set_look(&m, 1, "Caption", "Bright");
        let m = set_look(&m, 0, color_key(What::Knob), &stored([1, 2, 3]));
        let m = set_look(&m, 1, color_key(What::Label), &stored([9, 8, 7]));
        let c = controls(&m);
        assert_eq!((c[1].caption.as_str(), c[0].color, c[1].text_color), ("Bright", Some([1, 2, 3]), Some([9, 8, 7])));
        // every kind can be added, each with outputs of its own
        let mut m = m;
        for w in [What::Knob, What::Slider, What::Button, What::Check, What::Digits, What::Pad, What::Label, What::Bevel] {
            m = add(&m, w, (5.0, 6.0));
            let c = controls(&m);
            assert_eq!((c.last().unwrap().what, c.last().unwrap().rect[0]), (w, 5.0));
        }
        let c = controls(&m);
        assert_eq!(c.len(), 10);
        let mut outs: Vec<u16> = c.iter().flat_map(|c| c.outs.clone()).collect();
        assert_eq!(outs.len(), 8);
        outs.dedup();
        assert_eq!(outs, vec![3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(decode(&m).unwrap().len(), 11);
        assert_eq!(c[2].rest, vec![0.5]);
        let m = raise(&m, 0);
        let c = controls(&m);
        assert_eq!((c.len(), c[9].name.as_str(), c[0].name.as_str()), (10, "Cut", "Title"));
        let m = remove(&m, 0);
        assert_eq!(controls(&m).len(), 9);
        assert_eq!(size(&set_size(&m, 300.0, 200.0)), (300.0, 200.0));
    }
}
