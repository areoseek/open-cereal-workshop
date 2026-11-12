//! The older control board: sliders, wheels, switches and captions laid out on a panel. It makes no sound;
//! what its controls are set to goes to whatever is linked to the channel, one output per control in the
//! order of the block's list. The layout was read off the stored examples: a head of seven numbers, the
//! values (the first is the picked patch), a list of 81-byte entries naming each sending control with its
//! number and top value, then the panel with its size and every control's kind, name and rectangle. The
//! skins the block names are artwork we do not have, so the controls are drawn in our own look.

pub const IDENT: &str = "Dashboard";

#[derive(Debug, Clone, PartialEq)]
pub struct Ctl {
    /// 1 slider, 2 wheel, 4 switch, 5 caption, 8 patch picker.
    pub kind: u8,
    pub name: String,
    pub rect: [i32; 4],
    /// Which sending entry this control moves.
    pub out: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Board {
    pub w: i32,
    pub h: i32,
    pub controls: Vec<Ctl>,
    /// The top value of each sending entry.
    pub tops: Vec<i32>,
}

fn int(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn text(b: &[u8], o: usize) -> Option<(String, usize)> {
    let n = *b.get(o)? as usize;
    Some((b.get(o + 1..o + 1 + n)?.iter().map(|&c| c as char).collect(), o + 1 + n))
}

/// Where the values start and how many sending entries there are.
fn head(b: &[u8]) -> Option<(usize, usize)> {
    let n = int(b, 28)?;
    if !(1..=4096).contains(&n) {
        return None;
    }
    let recs = int(b, 32 + 4 * n as usize)?;
    (recs >= 0 && recs < n + 1).then_some((32, recs as usize))
}

impl Board {
    pub fn parse(b: &[u8]) -> Option<Self> {
        let (vals, recs) = head(b)?;
        let n = int(b, 28)? as usize;
        let list = vals + 4 * n + 4;
        let mut names = Vec::new();
        let mut tops = Vec::new();
        for r in 0..recs {
            let at = list + r * 81;
            names.push(text(b, at)?.0);
            tops.push(int(b, at + 76)?.max(1));
        }
        let mut o = list + recs * 81;
        let version = int(b, o)?;
        o += 8;
        let (_, p) = text(b, o)?;
        let (_, p) = text(b, p)?;
        let (w, h) = (int(b, p + 6)?, int(b, p + 10)?);
        let (_, p) = text(b, p + 18)?;
        // the older panel keeps a shorter run of settings before its controls
        o = p + 8 + if version >= 4 { 76 } else { 66 };
        let count = int(b, o)?;
        o += 4;
        if !(0..=4096).contains(&count) || !(1..=8192).contains(&w) || !(1..=8192).contains(&h) {
            return None;
        }
        let mut controls = Vec::new();
        for _ in 0..count {
            let kind = *b.get(o)?;
            let (_, p) = text(b, o + 1)?;
            let size = int(b, p)? as usize;
            let body = b.get(p + 4..p + 4 + size)?;
            let (name, q) = text(body, 0)?;
            let (_, q) = text(body, q)?;
            let rect = [int(body, q)?, int(body, q + 4)?, int(body, q + 8)?, int(body, q + 12)?];
            let out = if matches!(kind, 1 | 2 | 4) { names.iter().position(|n| *n == name) } else { None };
            controls.push(Ctl { kind, name, rect, out });
            o = p + 4 + size + 16;
        }
        Some(Self { w, h, controls, tops })
    }
}

/// How many outputs the board has.
pub fn outs(b: &[u8]) -> usize {
    head(b).map_or(0, |h| h.1)
}

/// Output `out` as 0..1.
pub fn value(b: &[u8], out: usize) -> Option<f32> {
    let (vals, recs) = head(b)?;
    if out >= recs {
        return None;
    }
    let n = int(b, 28)? as usize;
    let top = int(b, vals + 4 * n + 4 + out * 81 + 76)?.max(1);
    Some((int(b, vals + 4 * (out + 1))? as f32 / top as f32).clamp(0.0, 1.0))
}

pub fn set_value(b: &mut [u8], out: usize, v: f32) {
    let Some((vals, recs)) = head(b) else { return };
    if out >= recs {
        return;
    }
    let n = int(b, 28).unwrap_or(0) as usize;
    let top = int(b, vals + 4 * n + 4 + out * 81 + 76).unwrap_or(65536).max(1);
    let at = vals + 4 * (out + 1);
    b[at..at + 4].copy_from_slice(&((v.clamp(0.0, 1.0) * top as f32).round() as i32).to_le_bytes());
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn pstr(s: &str) -> Vec<u8> {
        let mut v = vec![s.len() as u8];
        v.extend_from_slice(s.as_bytes());
        v
    }

    /// A board of our own with one slider and a caption, laid out like the stored ones.
    pub fn block() -> Vec<u8> {
        let mut b: Vec<u8> = [2i32, 0, -1, -1, -1, 0, 0, 2, 0, 16384, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut rec = vec![0u8; 81];
        rec[..5].copy_from_slice(&pstr("Fade"));
        rec[48..53].copy_from_slice(&pstr("Fade"));
        rec[76..80].copy_from_slice(&65536i32.to_le_bytes());
        b.extend_from_slice(&rec);
        let mut panel = pstr("Plain");
        panel.extend(pstr("plain.bmp"));
        panel.extend([3, 5, 0x40, 0x40, 0x40, 0]);
        panel.extend([300i32, 120, 23].iter().flat_map(|v| v.to_le_bytes()));
        panel.extend(pstr("A font"));
        panel.extend([0u8; 8 + 76]);
        panel.extend(2i32.to_le_bytes());
        for (kind, name, rect) in [(1u8, "Fade", [10, 10, 22, 100]), (5, "Title", [50, 10, 40, 12])] {
            panel.push(kind);
            panel.extend(pstr("skin"));
            let mut body = pstr(name);
            body.extend(pstr(name));
            body.extend(rect.iter().flat_map(|v: &i32| v.to_le_bytes()));
            body.extend([0u8; 14]);
            panel.extend((body.len() as i32).to_le_bytes());
            panel.extend(body);
            panel.extend([0u8; 16]);
        }
        b.extend(4i32.to_le_bytes());
        b.extend((panel.len() as i32).to_le_bytes());
        b.extend(panel);
        b.extend([0u8; 4]);
        b
    }

    #[test]
    fn a_board_is_read_and_its_values_set() {
        let mut b = block();
        let board = Board::parse(&b).unwrap();
        assert_eq!((board.w, board.h, board.controls.len()), (300, 120, 2));
        assert_eq!(board.controls[0], Ctl { kind: 1, name: "Fade".into(), rect: [10, 10, 22, 100], out: Some(0) });
        assert_eq!(board.controls[1].out, None);
        assert_eq!((outs(&b), value(&b, 0), value(&b, 1)), (1, Some(0.25), None));
        let len = b.len();
        set_value(&mut b, 0, 1.0);
        assert_eq!((value(&b, 0), b.len()), (Some(1.0), len));
        assert!(Board::parse(&[0; 20]).is_none());
    }

    #[test]
    fn stored_boards_open() {
        let Some(root) = std::env::var_os("CEREAL_FLP_CORPUS") else { return };
        let dir = std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Dashboard");
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut seen = 0;
        for f in rd.flatten() {
            let Ok(pre) = crate::preset::read(&f.path()) else { continue };
            let Some(crate::preset::Unit::Gen(g)) = pre.unit() else { continue };
            let raw = &g.raw;
            let board = Board::parse(raw).unwrap_or_else(|| panic!("{:?}", f.path()));
            assert!(board.controls.iter().filter(|c| matches!(c.kind, 1 | 2 | 4)).all(|c| c.out.is_some()) , "{:?}", f.path());
            seen += 1;
        }
        assert!(seen == 0 || seen >= 5);
    }
}
