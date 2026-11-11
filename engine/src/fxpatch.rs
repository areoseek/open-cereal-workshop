//! The patch bay: a unit that holds other units and the cables between them.
//!
//! Its block is a version number and then a run of chunks, each a 32-bit id, a 64-bit length and that many
//! bytes. A chunk with id 0 is a group of further chunks, and describes one thing on the bay's surface:
//!
//! * a held unit: 5000 (a header), 5004 (where the unit lives on disk), 5008 (its identifier), 5010 (the
//!   name shown), 5005 (its settings, in the unit's own block format), 5007 (its window) and one 5009 per
//!   socket: the kind of signal (0 sound, 1 notes, 2 a setting), an index, the socket's id and its role
//!   (3 takes sound in, 2 gives sound out, 11 a setting that can be driven, 10 a driver)
//! * a socket of the bay itself: 1400 (id, kind of signal) and 1402 (its name)
//! * a cable: 1300 (the socket it ends at, the socket it starts from, and a level)
//! * where a unit sits on the surface: 1600
//!
//! We read the held units into our own, follow the sound cables, and keep everything else as it was.

use crate::fx::{Ctx, Fx, Unit};

/// One held unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The identifier stored for the unit.
    pub ident: String,
    /// The name shown for it on the bay's surface.
    pub name: String,
    /// Our unit for it, when we have one.
    pub fx: Option<Fx>,
}

impl Node {
    fn fx_before(&self, block: &[u8]) -> Option<Fx> {
        let known = if self.ident.eq_ignore_ascii_case("Fruity 7 Band Equalizer") { "Fruity 7 Band EQ" } else { &self.ident };
        Fx::decode(known, block)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum End {
    /// The bay's own sound input (as a start) or output (as an end).
    Bay,
    Node(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cable {
    pub from: End,
    pub to: End,
    pub level: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Patch {
    pub nodes: Vec<Node>,
    /// Sound cables only.
    pub cables: Vec<Cable>,
    /// The order the units run in: every unit after the ones that feed it, where the cables allow.
    pub order: Vec<usize>,
    /// Held instruments, besides the first, whose notes are played outside and handed in as sound.
    pub feeds: Vec<usize>,
}

fn u32at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn u64at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The chunks of `b` from `at` on: id and where the data lies. `None` when a length runs past the end.
fn chunks(b: &[u8], mut at: usize) -> Option<Vec<(u32, std::ops::Range<usize>)>> {
    let mut out = Vec::new();
    while at + 12 <= b.len() {
        let id = u32at(b, at)?;
        let n = usize::try_from(u64at(b, at + 4)?).ok()?;
        let end = (at + 12).checked_add(n)?;
        if end > b.len() {
            return None;
        }
        out.push((id, at + 12..end));
        at = end;
    }
    (at == b.len()).then_some(out)
}

fn put(out: &mut Vec<u8>, id: u32, data: &[u8]) {
    out.extend(id.to_le_bytes());
    out.extend((data.len() as u64).to_le_bytes());
    out.extend(data);
}

/// Names are held as plain bytes in older blocks and two bytes a letter in newer ones.
fn text(b: &[u8]) -> String {
    if b.len() >= 2 && b.len() % 2 == 0 && b.iter().skip(1).step_by(2).all(|&x| x == 0) {
        return String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()).trim_end_matches('\0').to_string();
    }
    String::from_utf8_lossy(b).trim_end_matches('\0').to_string()
}

pub fn decode(state: &[u8]) -> Option<Patch> {
    read(state, false).map(|r| r.0)
}

/// A bay that stands in a channel as an instrument: the bay, with the cables of the instrument the notes
/// reach starting at the bay's input instead, then which held unit that instrument is and its settings.
/// Further instruments the notes reach come back as layers.
pub fn decode_gen(state: &[u8]) -> Option<(Patch, usize, crate::synth::Gen, Layers)> {
    let (p, lead, layers) = read(state, true)?;
    let (k, g) = lead?;
    Some((p, k, g, layers))
}

/// Further held instruments that play the same notes: which held unit each is, and its settings.
pub type Layers = Vec<(usize, crate::synth::Gen)>;

/// The block of a bay standing as an instrument, with that instrument's settings brought up to date.
pub fn encode_gen(base: &[u8], k: usize, gen: &crate::synth::Gen) -> Vec<u8> {
    let Some(top) = chunks(base, 4) else { return base.to_vec() };
    let mut out = base[..4].to_vec();
    let mut at = 0;
    for (id, r) in top {
        let g = &base[r];
        let inner = if id == 0 { chunks(g, 0) } else { None };
        match inner {
            Some(inner) if inner.first().map(|c| c.0) == Some(5000) => {
                let mut body = Vec::with_capacity(g.len());
                for (j, q) in inner {
                    if j == 5005 && at == k {
                        put(&mut body, j, &gen.encode(Some(&g[q])));
                    } else {
                        put(&mut body, j, &g[q]);
                    }
                }
                put(&mut out, 0, &body);
                at += 1;
            }
            _ => put(&mut out, id, g),
        }
    }
    out
}

fn read(state: &[u8], as_gen: bool) -> Option<(Patch, Option<(usize, crate::synth::Gen)>, Layers)> {
    use std::collections::HashMap;
    let top = chunks(state, 4)?;
    let mut nodes = Vec::new();
    // socket id -> (unit, kind of signal, index, role)
    let mut pins: HashMap<u64, (usize, u32, u32, u32)> = HashMap::new();
    // the bay's own sockets -> kind of signal
    let mut own: HashMap<u64, u32> = HashMap::new();
    let mut raw = Vec::new();
    let mut blocks = Vec::new();
    for (id, r) in &top {
        if *id != 0 || r.is_empty() {
            continue;
        }
        let g = &state[r.clone()];
        let inner = chunks(g, 0)?;
        match inner.first().map(|c| c.0) {
            Some(5000) => {
                let (mut ident, mut name, mut block) = (String::new(), String::new(), &g[0..0]);
                for (j, q) in &inner {
                    match j {
                        5008 => ident = text(&g[q.clone()]),
                        5010 => name = text(&g[q.clone()]),
                        5005 => block = &g[q.clone()],
                        5009 => {
                            let d = &g[q.clone()];
                            pins.insert(u64at(d, 8)?, (nodes.len(), u32at(d, 0)?, u32at(d, 4)? & 0xff_ffff, u32at(d, 16)?));
                        }
                        _ => {}
                    }
                }
                // one unit goes by a longer name here than in a project's own slots
                let mut node = Node { ident, name, fx: None };
                node.fx = node.fx_before(block);
                nodes.push(node);
                blocks.push(block);
            }
            Some(1400) => {
                let d = &g[inner[0].1.clone()];
                own.insert(u64at(d, 0)?, u32at(d, 8)?);
            }
            Some(1300) => {
                let d = &g[inner[0].1.clone()];
                raw.push((u64at(d, 0)?, u64at(d, 8)?, f32::from_le_bytes(d.get(16..20)?.try_into().ok()?)));
            }
            _ => {}
        }
    }
    // the bay's main sound sockets: the lowest-numbered one cables start from, and the lowest they end at
    let sound = |id: &u64| own.get(id) == Some(&0);
    let mut main_in = raw.iter().map(|c| c.1).filter(sound).min();
    let mut lead = None;
    let mut layers: Layers = Vec::new();
    if as_gen {
        main_in = None;
        // follow the note cables from the bay's note sockets, through any note-shaping units, to instruments
        let mut reached: Vec<usize> = Vec::new();
        let mut live: Vec<Option<usize>> = vec![None];
        while let Some(at) = live.pop() {
            for &(to, from, _) in &raw {
                let starts = match at {
                    None => own.get(&from) == Some(&1),
                    Some(n) => matches!(pins.get(&from), Some(&(m, 1, _, _)) if m == n),
                };
                if let (true, Some(&(n, 1, _, _))) = (starts, pins.get(&to)) {
                    if !reached.contains(&n) {
                        reached.push(n);
                        live.push(Some(n));
                    }
                }
            }
        }
        reached.sort();
        // with no note cable we can follow, every held instrument is taken to play
        let all: Vec<usize> = if reached.is_empty() { (0..nodes.len()).collect() } else { reached };
        for n in all {
            if nodes[n].fx.as_ref().is_some_and(|f| f.kind != crate::fx::Kind::Thru) {
                continue;
            }
            if let Some(g) = crate::synth::Gen::decode(&nodes[n].ident, blocks[n]) {
                if lead.is_none() {
                    lead = Some((n, g));
                } else {
                    layers.push((n, g));
                }
            }
        }
    }
    let main_out = raw.iter().map(|c| c.0).filter(sound).min();
    let mut cables = Vec::new();
    for (to, from, level) in raw {
        let from = match pins.get(&from) {
            Some(&(n, 0, _, _)) if lead.as_ref().is_some_and(|l| l.0 == n) => End::Bay,
            Some(&(n, 0, _, _)) => End::Node(n),
            Some(_) => continue,
            None if Some(from) == main_in => End::Bay,
            None => continue,
        };
        let to = match pins.get(&to) {
            // only a unit's first sound input; the others are side inputs we do not feed yet
            Some(&(n, 0, 0, _)) => End::Node(n),
            Some(_) => continue,
            None if Some(to) == main_out => End::Bay,
            None => continue,
        };
        cables.push(Cable { from, to, level });
    }
    let order = sort(nodes.len(), &cables);
    let feeds = layers.iter().map(|l| l.0).collect();
    Some((Patch { nodes, cables, order, feeds }, lead, layers))
}

/// Units in an order where each comes after the ones feeding it. Units caught in a loop of cables go last,
/// in the order they were stored; a loop then hears itself one block late.
fn sort(n: usize, cables: &[Cable]) -> Vec<usize> {
    let mut waiting = vec![0usize; n];
    for c in cables {
        if let (End::Node(a), End::Node(b)) = (c.from, c.to) {
            if a != b {
                waiting[b] += 1;
            }
        }
    }
    let mut order: Vec<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
    let mut at = 0;
    while at < order.len() {
        let a = order[at];
        at += 1;
        for c in cables {
            if let (End::Node(x), End::Node(b)) = (c.from, c.to) {
                if x == a && a != b {
                    waiting[b] -= 1;
                    if waiting[b] == 0 {
                        order.push(b);
                    }
                }
            }
        }
    }
    for i in 0..n {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    order
}

/// The block again, with the settings of any held unit that changed written into its own block. Everything
/// else is carried over byte for byte.
pub fn encode(p: &Patch, base: Option<&[u8]>) -> Vec<u8> {
    let Some(base) = base else { return fresh() };
    let Some(top) = chunks(base, 4) else { return base.to_vec() };
    let mut out = base[..4].to_vec();
    let mut k = 0;
    for (id, r) in top {
        let g = &base[r.clone()];
        let inner = if id == 0 { chunks(g, 0) } else { None };
        let held = inner.as_ref().is_some_and(|c| c.first().map(|c| c.0) == Some(5000));
        if !held {
            put(&mut out, id, g);
            continue;
        }
        let node = p.nodes.get(k);
        k += 1;
        let mut body = Vec::with_capacity(g.len());
        for (j, q) in inner.unwrap() {
            let d = &g[q];
            match node.and_then(|n| n.fx.as_ref().map(|f| (n, f))) {
                Some((n, f)) if j == 5005 && n.fx_before(d).as_ref() != Some(f) => put(&mut body, j, &f.encode(Some(d))),
                _ => put(&mut body, j, d),
            }
        }
        put(&mut out, 0, &body);
    }
    out
}

/// An empty bay: sound in cabled straight to sound out.
pub fn fresh() -> Vec<u8> {
    let mut out = 1u32.to_le_bytes().to_vec();
    let mut head = [0u8; 64];
    head[0..4].copy_from_slice(&1001u32.to_le_bytes());
    head[8] = 1;
    put(&mut out, 1000, &head);
    let mut view = [0u8; 44];
    view[17] = 0x0f;
    for (i, v) in [20.0f32, 181.5, 644.5, 181.5].iter().enumerate() {
        view[21 + i * 4..25 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    put(&mut out, 1002, &view);
    for (id, flags, name) in [(1000u64, 7u32, "Audio input"), (1001, 6, "Audio output")] {
        let mut g = Vec::new();
        let mut d = id.to_le_bytes().to_vec();
        for v in [0u32, flags, 2, 0, 0] {
            d.extend(v.to_le_bytes());
        }
        put(&mut g, 1400, &d);
        put(&mut g, 1402, name.as_bytes());
        put(&mut g, 1401, &[]);
        put(&mut out, 0, &g);
    }
    let mut g = Vec::new();
    let mut d = 1001u64.to_le_bytes().to_vec();
    d.extend(1000u64.to_le_bytes());
    d.extend(1.0f32.to_le_bytes());
    d.extend([0u8; 12]);
    put(&mut g, 1300, &d);
    put(&mut g, 1301, &[]);
    put(&mut out, 0, &g);
    let mut d = 1.0f32.to_le_bytes().to_vec();
    d.extend([0u8; 8]);
    d.extend(600u32.to_le_bytes());
    d.extend(471u32.to_le_bytes());
    d.extend([0u8; 20]);
    put(&mut out, 1500, &d);
    put(&mut out, 0, &[]);
    put(&mut out, 0, &[]);
    put(&mut out, 1001, &[]);
    out
}

/// The running bay.
pub struct State {
    units: Vec<Option<Unit>>,
    bufs: Vec<Vec<[f32; 2]>>,
    scratch: Vec<[f32; 2]>,
    /// Sound handed in for a held instrument this block: the unit, the sound, and whether any came.
    feeds: Vec<(usize, Vec<[f32; 2]>, bool)>,
}

impl State {
    pub fn new(p: &Patch, sr: f32) -> Self {
        Self {
            units: p.nodes.iter().map(|n| n.fx.clone().map(|f| Unit::new(f, sr))).collect(),
            bufs: vec![vec![[0.0; 2]; 1024]; p.nodes.len()],
            scratch: vec![[0.0; 2]; 1024],
            feeds: p.feeds.iter().map(|&n| (n, vec![[0.0; 2]; 1024], false)).collect(),
        }
    }

    /// Where to add this block's sound for layer `k` (in the order of the bay's `feeds`).
    pub fn feed(&mut self, k: usize, n: usize) -> Option<&mut [[f32; 2]]> {
        let f = self.feeds.get_mut(k)?;
        if f.1.len() < n {
            f.1.resize(n, [0.0; 2]);
        }
        f.2 = true;
        Some(&mut f.1[..n])
    }

    /// How long the bay rings after its input stops, in frames.
    pub fn tail(&self, cx: &Ctx) -> usize {
        let t: usize = self.units.iter().flatten().map(|u| u.tail(cx)).sum();
        t.min((cx.sr * 20.0) as usize)
    }

    /// Bring the held units' settings in line with `p` (same units, new values).
    pub fn sync(&mut self, p: &Patch) {
        for (u, n) in self.units.iter_mut().zip(&p.nodes) {
            if let (Some(u), Some(f)) = (u, &n.fx) {
                if u.fx != *f && u.fx.kind == f.kind && u.fx.p.len() == f.p.len() {
                    u.fx.p.copy_from_slice(&f.p);
                    if u.fx.sub != f.sub {
                        u.fx.sub = f.sub.clone();
                    }
                }
            }
        }
    }
}

pub fn process(p: &Patch, st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if !p.cables.iter().any(|c| c.to == End::Bay) {
        // nothing reaches the output we know of: better the dry sound than silence
        return;
    }
    let n = io.len();
    if st.scratch.len() < n {
        st.scratch.resize(n, [0.0; 2]);
        for b in &mut st.bufs {
            b.resize(n, [0.0; 2]);
        }
    }
    let State { units, bufs, scratch, feeds } = st;
    let gather = |to: End, scratch: &mut [[f32; 2]], bufs: &[Vec<[f32; 2]>], io: &[[f32; 2]]| {
        scratch.fill([0.0; 2]);
        for c in p.cables.iter().filter(|c| c.to == to) {
            let src: &[[f32; 2]] = match c.from {
                End::Bay => io,
                End::Node(j) => &bufs[j][..io.len()],
            };
            for (o, s) in scratch.iter_mut().zip(src) {
                o[0] += s[0] * c.level;
                o[1] += s[1] * c.level;
            }
        }
    };
    for &i in &p.order {
        gather(End::Node(i), &mut scratch[..n], bufs, io);
        for f in feeds.iter_mut().filter(|f| f.0 == i && f.2) {
            for (o, s) in scratch[..n].iter_mut().zip(f.1.iter_mut()) {
                o[0] += s[0];
                o[1] += s[1];
                *s = [0.0; 2];
            }
            f.2 = false;
        }
        if let Some(u) = &mut units[i] {
            u.process(&mut scratch[..n], cx);
        }
        bufs[i][..n].copy_from_slice(&scratch[..n]);
    }
    gather(End::Bay, &mut scratch[..n], bufs, io);
    io.copy_from_slice(&scratch[..n]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_bay_passes_sound_and_round_trips() {
        let b = fresh();
        let p = decode(&b).expect("reads back");
        assert!(p.nodes.is_empty());
        assert_eq!(p.cables, vec![Cable { from: End::Bay, to: End::Bay, level: 1.0 }]);
        assert_eq!(encode(&p, Some(&b)), b);
        let mut st = State::new(&p, 44100.0);
        let mut io = vec![[0.25, -0.5]; 2000];
        process(&p, &mut st, &mut io, &Ctx { sr: 44100.0, step: 5512.5, clock: Default::default() });
        assert!(io.iter().all(|s| *s == [0.25, -0.5]));
    }

    #[test]
    fn units_run_after_their_feeders() {
        let c = |a, b| Cable { from: a, to: b, level: 1.0 };
        let order = sort(3, &[c(End::Bay, End::Node(2)), c(End::Node(2), End::Node(0)), c(End::Node(0), End::Node(1)), c(End::Node(1), End::Bay)]);
        assert_eq!(order, vec![2, 0, 1]);
        // a loop still lists every unit once
        let order = sort(2, &[c(End::Node(0), End::Node(1)), c(End::Node(1), End::Node(0))]);
        assert_eq!(order, vec![0, 1]);
    }
}
