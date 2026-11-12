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
}
