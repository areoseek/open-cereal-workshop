//! The MIDI sender: a channel that makes no sound of its own and turns its notes and its knobs into MIDI
//! messages for a port. The engine only queues the messages; the window's sender thread puts them on the wire.
//!
//! Stored block, little endian: version, channel (0-based), patch, bank coarse, bank fine (-1 = not sent),
//! port, a "notes go out on their colour's channel" word, then a run of switch bytes (6 from version 4,
//! 4 bytes of something else in versions 2 and 3, nothing in version 1; the first is taken as "send pan, volume and pitch"), 72 knob values (8 pages of 9), the list of
//! set-up knobs ended by 0xFF, 8 page names, and 4 closing bytes from version 6.

pub const IDENT: &str = "MIDI Out";
pub const KNOBS: usize = 72;
pub const KINDS: [&str; 5] = ["CC", "RPN", "NRPN", "AFT", "Note"];

/// One message for a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Msg {
    pub port: u8,
    pub len: u8,
    pub data: [u8; 3],
}

impl Msg {
    pub fn new(port: u8, bytes: &[u8]) -> Msg {
        let mut data = [0; 3];
        let len = bytes.len().min(3);
        data[..len].copy_from_slice(&bytes[..len]);
        Msg { port, len: len as u8, data }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
}

/// How long the front of the block is in a version: the first had no switch bytes at all.
fn front(ver: i32) -> usize {
    match ver {
        i32::MIN..=1 => 28,
        2 | 3 => 32,
        _ => 34,
    }
}

fn word(b: &[u8], o: usize) -> Option<i32> {
    b.get(o..o + 4).map(|x| i32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

/// The numbers at the front of the block, read without touching the heap (the audio thread uses this).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Head {
    pub ver: u32,
    pub chan: u8,
    pub patch: i32,
    pub bank: [i32; 2],
    pub port: u8,
    pub by_color: bool,
    pub send_mix: bool,
    /// Where the knob values start.
    pub at: usize,
}

impl Head {
    pub fn fresh() -> Head {
        Head { ver: 7, chan: 0, patch: -1, bank: [-1; 2], port: 0, by_color: false, send_mix: true, at: 34 }
    }
    pub fn read(b: &[u8]) -> Head {
        let Some(ver) = word(b, 0) else { return Head::fresh() };
        let at = front(ver);
        if b.len() < at + KNOBS * 4 + 1 {
            return Head::fresh();
        }
        let w = |o| word(b, o).unwrap_or(0);
        Head { ver: ver as u32, chan: (w(4) & 15) as u8, patch: w(8), bank: [w(12), w(16)], port: w(20) as u8, by_color: ver >= 4 && w(24) != 0, send_mix: at < 34 || b[28] != 0, at }
    }
    pub fn value(&self, b: &[u8], knob: usize) -> i32 {
        word(b, self.at + knob * 4).unwrap_or(0)
    }
    /// Calls `f` with each set-up knob: its place (page * 9 + column), number, whether it is switched on,
    /// kind, low end, high end and channel (-1 = the unit's own).
    pub fn each(&self, b: &[u8], mut f: impl FnMut(usize, i32, bool, i32, i32, i32, i32)) {
        let mut o = self.at + KNOBS * 4;
        let k = if self.ver < 4 { 4 } else { 5 };
        while let Some(&slot) = b.get(o) {
            if slot == 0xff {
                break;
            }
            o += 1;
            let w = |i: usize| word(b, o + i * 4).unwrap_or(0);
            let num = w(0);
            f(slot as usize, num & 0x7fff_ffff, num >= 0, w(1), w(2), w(3), if k == 5 { w(4) } else { -1 });
            o += k * 4;
            for _ in 0..2 {
                let Some(&l) = b.get(o) else { return };
                o += 1 + l as usize;
            }
        }
    }
}

/// The messages that put one knob's value on the wire.
pub fn knob_msgs(port: u8, chan: u8, kind: i32, num: i32, value: i32, mut out: impl FnMut(Msg)) {
    let c = chan & 15;
    let v = value.clamp(0, 127) as u8;
    match kind {
        1 | 2 => {
            let (hi, lo) = if kind == 1 { (101, 100) } else { (99, 98) };
            out(Msg::new(port, &[0xb0 | c, hi, ((num >> 7) & 127) as u8]));
            out(Msg::new(port, &[0xb0 | c, lo, (num & 127) as u8]));
            out(Msg::new(port, &[0xb0 | c, 6, v]));
        }
        3 => out(Msg::new(port, &[0xd0 | c, v])),
        4 => {
            if v == 0 {
                out(Msg::new(port, &[0x80 | c, (num & 127) as u8, 0]))
            } else {
                out(Msg::new(port, &[0x90 | c, (num & 127) as u8, v]))
            }
        }
        _ => out(Msg::new(port, &[0xb0 | c, (num & 127) as u8, v])),
    }
}

/// The messages that choose the patch: bank first, when one is set.
pub fn patch_msgs(h: &Head, mut out: impl FnMut(Msg)) {
    if h.bank[0] >= 0 {
        out(Msg::new(h.port, &[0xb0 | h.chan, 0, (h.bank[0] & 127) as u8]));
    }
    if h.bank[1] >= 0 {
        out(Msg::new(h.port, &[0xb0 | h.chan, 32, (h.bank[1] & 127) as u8]));
    }
    if h.patch >= 0 {
        out(Msg::new(h.port, &[0xc0 | h.chan, (h.patch & 127) as u8]));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ctl {
    pub slot: u8,
    pub num: i32,
    pub on: bool,
    pub kind: i32,
    pub lo: i32,
    pub hi: i32,
    pub chan: i32,
    pub name: String,
    pub short: String,
}

/// The whole block, for the settings page.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub ver: u32,
    pub chan: u8,
    pub patch: i32,
    pub bank: [i32; 2],
    pub port: u8,
    pub by_color: bool,
    pub send_mix: bool,
    /// The word the colour switch sits in, as it came (older versions keep something else there).
    pub w24: i32,
    /// The switch bytes after the first, kept as they came.
    pub rest: Vec<u8>,
    pub values: [i32; KNOBS],
    pub ctls: Vec<Ctl>,
    pub pages: [String; 8],
    pub tail: Vec<u8>,
}

fn text(b: &[u8], o: &mut usize) -> Option<String> {
    let l = *b.get(*o)? as usize;
    let s = b.get(*o + 1..*o + 1 + l)?;
    *o += 1 + l;
    Some(String::from_utf8_lossy(s).into_owned())
}

fn put_text(out: &mut Vec<u8>, s: &str) {
    let mut n = s.len().min(255);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    out.push(n as u8);
    out.extend_from_slice(&s.as_bytes()[..n]);
}

impl Set {
    pub fn fresh() -> Set {
        Set { ver: 7, chan: 0, patch: -1, bank: [-1; 2], port: 0, by_color: false, send_mix: true, w24: 0, rest: vec![0; 5], values: [0; KNOBS], ctls: vec![], pages: std::array::from_fn(|i| format!("Page {}", i + 1)), tail: vec![0; 4] }
    }
    pub fn parse(b: &[u8]) -> Option<Set> {
        let ver = word(b, 0)?;
        let at = front(ver);
        if b.len() < at + KNOBS * 4 + 1 {
            return None;
        }
        let h = Head::read(b);
        let mut values = [0; KNOBS];
        for (i, v) in values.iter_mut().enumerate() {
            *v = h.value(b, i);
        }
        let mut o = at + KNOBS * 4;
        let k = if ver < 4 { 4 } else { 5 };
        let mut ctls = vec![];
        loop {
            let slot = *b.get(o)?;
            o += 1;
            if slot == 0xff {
                break;
            }
            let num = word(b, o)?;
            let f: Vec<i32> = (1..k).map(|i| word(b, o + i * 4)).collect::<Option<_>>()?;
            o += k * 4;
            let name = text(b, &mut o)?;
            let short = text(b, &mut o)?;
            ctls.push(Ctl { slot, num: num & 0x7fff_ffff, on: num >= 0, kind: f[0], lo: f[1], hi: f[2], chan: if k == 5 { f[3] } else { -1 }, name, short });
        }
        let mut pages: [String; 8] = Default::default();
        for p in pages.iter_mut() {
            *p = text(b, &mut o)?;
        }
        Some(Set { ver: ver as u32, chan: h.chan, patch: h.patch, bank: h.bank, port: h.port, by_color: h.by_color, send_mix: h.send_mix, w24: word(b, 24)?, rest: b[if at < 34 { 28 } else { 29 }..at].to_vec(), values, ctls, pages, tail: b[o..].to_vec() })
    }
    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(400);
        for w in [self.ver as i32, self.chan as i32, self.patch, self.bank[0], self.bank[1], self.port as i32, if self.ver >= 4 { self.by_color as i32 } else { self.w24 }] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        if self.ver >= 4 {
            out.push(self.send_mix as u8);
        }
        out.extend_from_slice(&self.rest);
        for v in self.values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for c in &self.ctls {
            out.push(c.slot);
            let num = if c.on { c.num } else { c.num | i32::MIN };
            let chan = [c.chan];
            for w in [num, c.kind, c.lo, c.hi].iter().chain(if self.ver < 4 { [].iter() } else { chan.iter() }) {
                out.extend_from_slice(&w.to_le_bytes());
            }
            put_text(&mut out, &c.name);
            put_text(&mut out, &c.short);
        }
        out.push(0xff);
        for p in &self.pages {
            put_text(&mut out, p);
        }
        out.extend_from_slice(&self.tail);
        out
    }
    pub fn ctl(&self, slot: usize) -> Option<&Ctl> {
        self.ctls.iter().find(|c| c.slot as usize == slot)
    }
    /// The knob at a place, made if it is not set up yet.
    pub fn ctl_mut(&mut self, slot: usize) -> &mut Ctl {
        if let Some(i) = self.ctls.iter().position(|c| c.slot as usize == slot) {
            return &mut self.ctls[i];
        }
        let at = self.ctls.iter().position(|c| c.slot as usize > slot).unwrap_or(self.ctls.len());
        self.ctls.insert(at, Ctl { slot: slot as u8, num: 0, on: true, kind: 0, lo: 0, hi: 127, chan: -1, name: String::new(), short: String::new() });
        &mut self.ctls[at]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_block_reads_back_and_spells_its_messages() {
        let mut s = Set::fresh();
        s.chan = 2;
        s.patch = 5;
        s.bank = [1, -1];
        s.port = 3;
        *s.ctl_mut(10) = Ctl { slot: 10, num: 74, on: true, kind: 0, lo: 0, hi: 127, chan: -1, name: "Cutoff".into(), short: "cut".into() };
        *s.ctl_mut(1) = Ctl { slot: 1, num: 300, on: false, kind: 2, lo: 0, hi: 127, chan: 4, name: "Deep".into(), short: "dp".into() };
        s.values[10] = 99;
        let b = s.write();
        assert_eq!(Set::parse(&b).unwrap(), s);
        let h = Head::read(&b);
        assert_eq!((h.chan, h.patch, h.port, h.value(&b, 10)), (2, 5, 3, 99));
        let mut seen = vec![];
        h.each(&b, |slot, num, on, kind, _, _, chan| seen.push((slot, num, on, kind, chan)));
        assert_eq!(seen, vec![(1, 300, false, 2, 4), (10, 74, true, 0, -1)]);
        let mut m = vec![];
        patch_msgs(&h, |x| m.push(x));
        assert_eq!(m.iter().map(|x| x.bytes().to_vec()).collect::<Vec<_>>(), vec![vec![0xb2, 0, 1], vec![0xc2, 5]]);
        m.clear();
        knob_msgs(3, 4, 2, 300, 64, |x| m.push(x));
        assert_eq!(m.iter().map(|x| x.bytes().to_vec()).collect::<Vec<_>>(), vec![vec![0xb4, 99, 2], vec![0xb4, 98, 44], vec![0xb4, 6, 64]]);
    }

    #[test]
    fn stored_blocks_come_back_byte_for_byte() {
        let Ok(dir) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut seen = 0;
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fst")) && p.to_string_lossy().contains("MIDI Out") {
                    let Ok(f) = crate::preset::read(&p) else { continue };
                    if f.ident.eq_ignore_ascii_case(IDENT) && !f.state.is_empty() {
                        let s = Set::parse(&f.state).unwrap_or_else(|| panic!("{}", p.display()));
                        assert_eq!(s.write(), f.state, "{}", p.display());
                        seen += 1;
                    }
                }
            }
        }
        assert!(seen > 5, "{seen}");
    }
}
