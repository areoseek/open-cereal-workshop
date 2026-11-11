//! The three note-keeping units: pages of plain text, pages of rich text, and a web address.
//!
//! They make no sound; the text travels in the unit's settings as one number per letter so that it follows the
//! same road as every other setting (undo, copy, save).

/// What a note unit holds. `flavour` 0 is plain pages, 1 rich-text pages, 2 a web address (kept as page 1).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Notes {
    pub flavour: u8,
    pub active: i32,
    pub pages: Vec<(i32, String)>,
}

pub fn flavour_of(ident: &str) -> Option<u8> {
    ["Fruity NoteBook 2", "Fruity NoteBook", "Fruity HTML NoteBook"].iter().position(|i| i.eq_ignore_ascii_case(ident)).map(|i| i as u8)
}

impl Notes {
    pub fn unpack(p: &[f32]) -> Self {
        let mut n = Notes { flavour: p.first().map(|v| *v as u8).unwrap_or(0), active: p.get(1).map(|v| *v as i32).unwrap_or(1), pages: vec![] };
        let count = p.get(2).map(|v| *v as usize).unwrap_or(0);
        let mut at = 3;
        for _ in 0..count {
            let (Some(num), Some(len)) = (p.get(at), p.get(at + 1)) else { break };
            let len = (*len as usize).min(p.len().saturating_sub(at + 2));
            let units: Vec<u16> = p[at + 2..at + 2 + len].iter().map(|v| *v as u16).collect();
            n.pages.push((*num as i32, String::from_utf16_lossy(&units)));
            at += 2 + len;
        }
        if n.pages.is_empty() {
            n.pages.push((1, String::new()));
        }
        n
    }
    pub fn pack(&self) -> Vec<f32> {
        let mut p = vec![self.flavour as f32, self.active as f32, self.pages.len() as f32];
        for (num, text) in &self.pages {
            let units: Vec<u16> = text.encode_utf16().collect();
            p.push(*num as f32);
            p.push(units.len() as f32);
            p.extend(units.iter().map(|u| *u as f32));
        }
        p
    }
    /// The page being shown, as a place in `pages`.
    pub fn shown(&self) -> usize {
        self.pages.iter().position(|(n, _)| *n == self.active).unwrap_or(0)
    }
}

struct R<'a> {
    d: &'a [u8],
    at: usize,
}

impl R<'_> {
    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.d.get(self.at..self.at + 4)?.try_into().ok()?);
        self.at += 4;
        Some(v)
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.d.get(self.at)?;
        self.at += 1;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let v = self.d.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(v)
    }
    /// A length in one byte, or all ones and then four.
    fn len(&mut self) -> Option<usize> {
        match self.u8()? {
            255 => self.u32().map(|v| v as usize),
            v => Some(v as usize),
        }
    }
}

fn put_len(out: &mut Vec<u8>, n: usize) {
    if n < 255 {
        out.push(n as u8);
    } else {
        out.push(255);
        out.extend((n as u32).to_le_bytes());
    }
}

/// Read a block. The second value is where the pages end (after the closing mark), for what follows to be kept.
fn read(flavour: u8, state: &[u8]) -> Option<(Notes, usize, Vec<std::ops::Range<usize>>)> {
    let mut r = R { d: state, at: 0 };
    let mut n = Notes { flavour, active: 1, pages: vec![] };
    let mut spans = vec![];
    r.u32()?;
    if flavour == 2 {
        let len = r.len()?;
        n.pages.push((1, String::from_utf8_lossy(r.take(len)?).into_owned()));
        return Some((n, r.at, spans));
    }
    n.active = r.u32()? as i32;
    loop {
        let num = r.u32()? as i32;
        if num < 0 {
            break;
        }
        if flavour == 0 {
            let len = r.len()?;
            let b = r.take(len.checked_mul(2)?)?;
            let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            n.pages.push((num, String::from_utf16_lossy(&units).replace("\r\n", "\n").replace('\r', "\n")));
        } else {
            let len = r.u32()? as usize;
            let from = r.at;
            let b = r.take(len)?;
            n.pages.push((num, rtf_plain(b)));
            spans.push(from..from + len);
        }
        if n.pages.len() > 4096 {
            return None;
        }
    }
    if n.pages.is_empty() {
        n.pages.push((1, String::new()));
    }
    Some((n, r.at, spans))
}

pub fn decode(ident: &str, state: &[u8]) -> Option<Vec<f32>> {
    let flavour = flavour_of(ident)?;
    read(flavour, state).map(|(n, _, _)| n.pack())
}

pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let n = Notes::unpack(p);
    let old = base.and_then(|b| read(n.flavour, b).map(|(o, end, spans)| (b, o, end, spans)));
    let mut out = vec![];
    match n.flavour {
        2 => {
            out.extend(old.as_ref().map(|(b, ..)| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(1).to_le_bytes());
            let url = n.pages.first().map(|p| p.1.as_bytes()).unwrap_or(&[]);
            put_len(&mut out, url.len());
            out.extend(url);
            if let Some((b, _, end, _)) = &old {
                out.extend(&b[*end..]);
            }
        }
        0 => {
            out.extend(old.as_ref().map(|(b, ..)| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(1).to_le_bytes());
            out.extend((n.active as u32).to_le_bytes());
            for (k, (num, text)) in n.pages.iter().enumerate() {
                out.extend((*num as u32).to_le_bytes());
                // a page left alone keeps its line ends as they were
                let kept = old.as_ref().and_then(|(b, o, ..)| (o.pages.get(k) == Some(&(*num, text.clone()))).then(|| page_bytes(b, k))).flatten();
                match kept {
                    Some(bytes) => out.extend(bytes),
                    None => {
                        let units: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().collect();
                        put_len(&mut out, units.len());
                        out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
                    }
                }
            }
            out.extend(u32::MAX.to_le_bytes());
            match &old {
                Some((b, _, end, _)) => out.extend(&b[*end..]),
                None => out.extend([1, 12, 0, 0, 0]),
            }
        }
        _ => {
            out.extend(1000u32.to_le_bytes());
            out.extend((n.active as u32).to_le_bytes());
            for (k, (num, text)) in n.pages.iter().enumerate() {
                out.extend((*num as u32).to_le_bytes());
                let kept = old.as_ref().and_then(|(b, o, _, spans)| (o.pages.get(k) == Some(&(*num, text.clone()))).then(|| spans.get(k).map(|s| &b[s.clone()])).flatten());
                let fresh;
                let bytes = match kept {
                    Some(b) => b,
                    None => {
                        fresh = plain_rtf(text);
                        &fresh[..]
                    }
                };
                out.extend((bytes.len() as u32).to_le_bytes());
                out.extend(bytes);
            }
            out.extend(u32::MAX.to_le_bytes());
            match &old {
                Some((b, _, end, _)) => out.extend(&b[*end..]),
                None => out.push(1),
            }
        }
    }
    out
}

/// The stored bytes (length and letters) of plain page `k`.
fn page_bytes(b: &[u8], k: usize) -> Option<&[u8]> {
    let mut r = R { d: b, at: 8 };
    for i in 0.. {
        if (r.u32()? as i32) < 0 {
            return None;
        }
        let from = r.at;
        let len = r.len()?;
        r.take(len.checked_mul(2)?)?;
        if i == k {
            return Some(&b[from..r.at]);
        }
    }
    None
}

/// The words of a rich-text page without their dressing.
pub fn rtf_plain(b: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    // depth at which a group that carries no words (fonts, colours, hidden parts) began
    let mut skip_from: Option<usize> = None;
    let mut depth = 0usize;
    let mut skip_after_u = 0;
    while i < b.len() {
        let c = b[i];
        i += 1;
        match c {
            b'{' => {
                depth += 1;
                if skip_from.is_none() {
                    let rest = &b[i..];
                    if [&b"\\fonttbl"[..], b"\\colortbl", b"\\stylesheet", b"\\info", b"\\*", b"\\pict"].iter().any(|w| rest.starts_with(w)) {
                        skip_from = Some(depth);
                    }
                }
            }
            b'}' => {
                if skip_from == Some(depth) {
                    skip_from = None;
                }
                depth = depth.saturating_sub(1);
            }
            b'\\' => {
                let Some(&n) = b.get(i) else { break };
                if n == b'\'' {
                    let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                    i += 3;
                    if let (Some(h), None) = (hex, skip_from) {
                        if skip_after_u > 0 {
                            skip_after_u -= 1;
                        } else {
                            out.push(h as char);
                        }
                    }
                } else if !n.is_ascii_alphabetic() {
                    i += 1;
                    if skip_from.is_none() {
                        match n {
                            b'\\' | b'{' | b'}' => out.push(n as char),
                            b'~' => out.push(' '),
                            _ => {}
                        }
                    }
                } else {
                    let w0 = i;
                    while i < b.len() && b[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = &b[w0..i];
                    let n0 = i;
                    if i < b.len() && (b[i] == b'-' || b[i].is_ascii_digit()) {
                        i += 1;
                        while i < b.len() && b[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let num: Option<i32> = std::str::from_utf8(&b[n0..i]).ok().and_then(|s| s.parse().ok());
                    if i < b.len() && b[i] == b' ' {
                        i += 1;
                    }
                    if skip_from.is_none() {
                        match word {
                            b"par" | b"line" => out.push('\n'),
                            b"tab" => out.push('\t'),
                            b"u" => {
                                if let Some(ch) = num.and_then(|v| char::from_u32(v as u16 as u32)) {
                                    out.push(ch);
                                }
                                skip_after_u = 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            b'\r' | b'\n' | 0 => {}
            _ => {
                if skip_from.is_none() && depth > 0 {
                    if skip_after_u > 0 {
                        skip_after_u -= 1;
                    } else {
                        out.push(c as char);
                    }
                }
            }
        }
    }
    // the closing paragraph mark is not a line of its own
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Plain words dressed as the simplest rich text.
pub fn plain_rtf(text: &str) -> Vec<u8> {
    let mut s = String::from("{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0\\fswiss Arial;}}\r\n\\pard\\f0\\fs18 ");
    for c in text.chars() {
        match c {
            '\\' | '{' | '}' => {
                s.push('\\');
                s.push(c);
            }
            '\n' => s.push_str("\\par\r\n"),
            '\t' => s.push_str("\\tab "),
            c if (c as u32) < 128 => s.push(c),
            c => {
                let mut u = [0u16; 2];
                for unit in c.encode_utf16(&mut u) {
                    s.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    s.push_str("\\par\r\n}\r\n");
    let mut b = s.into_bytes();
    b.push(0);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_block(pages: &[(i32, &str)], tail: &[u8]) -> Vec<u8> {
        let mut b = vec![1, 0, 0, 0, 1, 0, 0, 0];
        for (n, t) in pages {
            b.extend((*n as u32).to_le_bytes());
            let u: Vec<u16> = t.encode_utf16().collect();
            put_len(&mut b, u.len());
            b.extend(u.iter().flat_map(|x| x.to_le_bytes()));
        }
        b.extend(u32::MAX.to_le_bytes());
        b.extend(tail);
        b
    }

    #[test]
    fn plain_pages_go_round() {
        let long = "x".repeat(600);
        let b = plain_block(&[(1, "Hello\r\nthere"), (2, &long)], &[1, 12, 0, 0, 0]);
        let p = decode("Fruity NoteBook 2", &b).unwrap();
        let mut n = Notes::unpack(&p);
        assert_eq!(n.pages[0], (1, "Hello\nthere".to_string()));
        assert_eq!(n.pages[1].1.len(), 600);
        // untouched, the block comes back byte for byte
        assert_eq!(encode(&p, Some(&b)), b);
        n.pages[0].1 = "Changed".into();
        n.active = 2;
        let b2 = encode(&n.pack(), Some(&b));
        let n2 = Notes::unpack(&decode("Fruity NoteBook 2", &b2).unwrap());
        assert_eq!(n2, n);
        assert!(b2.ends_with(&[255, 255, 255, 255, 1, 12, 0, 0, 0]));
        // a new one reads back too
        let fresh = Notes { flavour: 0, active: 1, pages: vec![(1, "a\nb".into())] };
        assert_eq!(Notes::unpack(&decode("Fruity NoteBook 2", &encode(&fresh.pack(), None)).unwrap()), fresh);
    }

    #[test]
    fn rich_pages_are_read_as_words_and_kept_when_left_alone() {
        let rtf = b"{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0\\fswiss Arial;}}\r\n{\\colortbl ;\\red51\\green51\\blue51;}\r\n\\viewkind4\\uc1\\pard\\cf1\\f0\\fs18 Caf\\'e9 \\{one\\}\\par\r\nVol = -9.0 db\\par\r\n}\r\n\0";
        assert_eq!(rtf_plain(rtf), "Caf\u{e9} {one}\nVol = -9.0 db");
        let mut b = vec![];
        b.extend(1000u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend((rtf.len() as u32).to_le_bytes());
        b.extend(rtf);
        b.extend(u32::MAX.to_le_bytes());
        b.push(1);
        let p = decode("Fruity NoteBook", &b).unwrap();
        assert_eq!(encode(&p, Some(&b)), b);
        let mut n = Notes::unpack(&p);
        n.pages[0].1 = "New \u{266b} words\nline {two}".into();
        n.pages.push((2, "second".into()));
        let b2 = encode(&n.pack(), Some(&b));
        assert_eq!(Notes::unpack(&decode("Fruity NoteBook", &b2).unwrap()), n);
    }

    #[test]
    fn a_web_address_goes_round() {
        let b = [&[1u8, 0, 0, 0, 23][..], b"https://www.example.org"].concat();
        let p = decode("Fruity HTML NoteBook", &b).unwrap();
        let mut n = Notes::unpack(&p);
        assert_eq!(n.pages[0].1, "https://www.example.org");
        assert_eq!(encode(&p, Some(&b)), b);
        n.pages[0].1 = "https://example.com/x".into();
        assert_eq!(Notes::unpack(&decode("Fruity HTML NoteBook", &encode(&n.pack(), Some(&b))).unwrap()), n);
    }
}
