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
    /// The stored block of a unit that works on notes, as it stands now; the note cables read from it.
    pub notes: Option<Vec<u8>>,
}

impl Patch {
    /// Put an edited block in place for the note unit `i` and let every cable through it follow.
    pub fn set_note_block(&mut self, i: usize, block: Vec<u8>) -> bool {
        let Some(node) = self.nodes.get_mut(i) else { return false };
        let Some(shaper) = crate::noteshape::Shaper::decode(&node.ident, &block) else { return false };
        node.notes = Some(block);
        for (_, chain) in &mut self.shape {
            for l in chain.iter_mut().filter(|l| l.node == i) {
                l.shaper = shaper.clone();
            }
        }
        true
    }

    /// The same units cabled the same way, whatever their settings are.
    pub fn same_wiring(&self, o: &Patch) -> bool {
        self.nodes.len() == o.nodes.len()
            && self.nodes.iter().zip(&o.nodes).all(|(a, b)| a.ident == b.ident && a.fx.as_ref().map(|f| (f.kind, f.p.len())) == b.fx.as_ref().map(|f| (f.kind, f.p.len())))
            && self.cables == o.cables
            && self.order == o.order
            && self.feeds == o.feeds
            && self.shape == o.shape
            && self.sets == o.sets
    }
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

#[derive(Debug, Clone, Copy)]
pub struct Cable {
    pub from: End,
    pub to: End,
    pub level: f32,
    /// The sockets it was stored between (from, to), or zeros for a cable laid here: a unit with several
    /// outputs keeps the one its cable left from when the wiring is written again.
    pub via: (u64, u64),
}

impl PartialEq for Cable {
    fn eq(&self, o: &Cable) -> bool {
        (self.from, self.to, self.level) == (o.from, o.to, o.level)
    }
}

impl Cable {
    pub fn new(from: End, to: End, level: f32) -> Self {
        Cable { from, to, level, via: (0, 0) }
    }
}

/// A setting cable: an output of one unit moves a setting of another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetCable {
    /// The sending unit and which of its outputs.
    pub from: (usize, u16),
    /// The unit moved and the setting, as the unit numbers its settings.
    pub to: (usize, u16),
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
    /// For a held instrument (by its number), the note units its notes pass on the way in.
    pub shape: Vec<(usize, Vec<crate::noteshape::Link>)>,
    /// Setting cables between held units.
    pub sets: Vec<SetCable>,
    /// The sound cables were changed by hand and are to be written anew.
    pub rewired: bool,
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
                let mut node = Node { ident, name, fx: None, notes: None };
                node.fx = node.fx_before(block);
                if node.fx.as_ref().is_none_or(|f| f.kind == crate::fx::Kind::Thru) && crate::noteshape::Shaper::decode(&node.ident, block).is_some() {
                    node.notes = Some(block.to_vec());
                }
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
    let mut shape: Vec<(usize, Vec<crate::noteshape::Link>)> = Vec::new();
    if as_gen {
        main_in = None;
        // follow the note cables from the bay's note sockets, through any note-shaping units, to instruments
        let mut reached: Vec<usize> = Vec::new();
        let mut live: Vec<(Option<usize>, Vec<crate::noteshape::Link>)> = vec![(None, vec![])];
        while let Some((at, path)) = live.pop() {
            for &(to, from, _) in &raw {
                let (starts, port) = match (at, pins.get(&from)) {
                    (None, _) => (own.get(&from) == Some(&1), 0),
                    // a note unit's first socket is its input; its outputs count from there
                    (Some(n), Some(&(m, 1, k, _))) => (m == n, k.saturating_sub(1)),
                    _ => (false, 0),
                };
                if let (true, Some(&(n, 1, _, _))) = (starts, pins.get(&to)) {
                    if !reached.contains(&n) {
                        reached.push(n);
                        let mut path = path.clone();
                        if let Some(shaper) = at.and_then(|m| crate::noteshape::Shaper::decode(&nodes[m].ident, blocks[m])) {
                            path.push(crate::noteshape::Link { shaper, port, node: at.unwrap_or(0) });
                        }
                        if !path.is_empty() {
                            shape.push((n, path.clone()));
                        }
                        live.push((Some(n), path));
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
                // the channel's own instrument is the first that sounds; a held sender is only that when nothing does
                if lead.is_none() && g.kind != crate::synth::Kind::Quiet {
                    lead = Some((n, g));
                } else {
                    layers.push((n, g));
                }
            }
        }
        if lead.is_none() && !layers.is_empty() {
            lead = Some(layers.remove(0));
        }
    }
    let main_out = raw.iter().map(|c| c.0).filter(sound).min();
    let mut cables = Vec::new();
    let mut sets = Vec::new();
    for (to, from, level) in raw {
        let via = (from, to);
        // a setting cable runs from a unit's output socket (role 10) to a setting's socket (role 11)
        if let (Some(&(a, 2, out, 10)), Some(&(b, 2, k, 11))) = (pins.get(&from), pins.get(&to)) {
            if a != b && out < 4096 && k < 65536 {
                sets.push(SetCable { from: (a, out as u16), to: (b, k as u16) });
            }
            continue;
        }
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
        cables.push(Cable { from, to, level, via });
    }
    // a unit runs after the ones that send it settings, too
    let mut deps = cables.clone();
    deps.extend(sets.iter().map(|s| Cable::new(End::Node(s.from.0), End::Node(s.to.0), 1.0)));
    let order = sort(nodes.len(), &deps);
    let feeds: Vec<usize> = layers.iter().map(|l| l.0).collect();
    shape.retain(|s| lead.as_ref().is_some_and(|l| l.0 == s.0) || feeds.contains(&s.0));
    Some((Patch { nodes, cables, order, feeds, shape, sets, rewired: false }, lead, layers))
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
    let rewired;
    let base = match Some(()).filter(|_| p.rewired).and_then(|_| recable(base, &p.cables)) {
        Some(b) => {
            rewired = b;
            &rewired[..]
        }
        None => base,
    };
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
                _ => match node.and_then(|n| n.notes.as_deref()).filter(|_| j == 5005) {
                    Some(mine) => put(&mut body, j, mine),
                    None => put(&mut body, j, d),
                },
            }
        }
        put(&mut out, 0, &body);
    }
    out
}

/// The block with its sound cables taken out and `cables` written in their place. Note and setting cables,
/// and sound cables to sockets we do not use, stay as they are. A cable whose end has no socket is left out.
fn recable(base: &[u8], cables: &[Cable]) -> Option<Vec<u8>> {
    use std::collections::HashMap;
    let top = chunks(base, 4)?;
    let mut pins: HashMap<u64, (usize, u32, u32, u32)> = HashMap::new();
    let mut own: HashMap<u64, (u32, u32)> = HashMap::new();
    // where each cable group lies among the top chunks, with its ends and the two words after its level
    let mut raw: Vec<(usize, u64, u64, u32)> = Vec::new();
    let mut units = 0;
    for (at, (id, r)) in top.iter().enumerate() {
        if *id != 0 || r.is_empty() {
            continue;
        }
        let g = &base[r.clone()];
        let inner = chunks(g, 0)?;
        match inner.first().map(|c| c.0) {
            Some(5000) => {
                for (j, q) in &inner {
                    if *j == 5009 {
                        let d = &g[q.clone()];
                        pins.insert(u64at(d, 8)?, (units, u32at(d, 0)?, u32at(d, 4)? & 0xff_ffff, u32at(d, 16)?));
                    }
                }
                units += 1;
            }
            Some(1400) => {
                let d = &g[inner[0].1.clone()];
                own.insert(u64at(d, 0)?, (u32at(d, 8)?, u32at(d, 12)?));
            }
            Some(1300) => {
                let d = &g[inner[0].1.clone()];
                raw.push((at, u64at(d, 0)?, u64at(d, 8)?, u32at(d, 20).unwrap_or(0)));
            }
            _ => {}
        }
    }
    let sound = |id: &u64| own.get(id).is_some_and(|o| o.0 == 0);
    // the bay's own sockets as the reader picks them, or by their direction when no cable touches them yet
    let side = |flag: u32| own.iter().filter(|(_, o)| o.0 == 0 && o.1 == flag).map(|(id, _)| *id).min();
    let main_in = raw.iter().map(|c| c.2).filter(sound).min().or_else(|| side(7));
    let main_out = raw.iter().map(|c| c.1).filter(sound).min().or_else(|| side(6));
    // the cable groups the reader takes for sound cables: these are the ones replaced
    let ours = |to: u64, from: u64| {
        let a = match pins.get(&from) {
            Some(&(_, 0, _, _)) => true,
            Some(_) => false,
            None => Some(from) == main_in,
        };
        let b = match pins.get(&to) {
            Some(&(_, 0, 0, _)) => true,
            Some(_) => false,
            None => Some(to) == main_out,
        };
        a && b
    };
    let gone: Vec<usize> = raw.iter().filter(|c| ours(c.1, c.2)).map(|c| c.0).collect();
    let word = raw.first().map(|c| c.3).unwrap_or(2);
    // a unit's sockets carry a word whose lowest bit says input and the next says in use
    let socket = |n: usize, input: u32| pins.iter().filter(|(_, v)| v.0 == n && v.1 == 0 && v.3 & 1 == input).map(|(id, v)| (v.2, *id)).min().map(|x| x.1);
    let fits = |id: u64, end: End, input: u32| match (end, pins.get(&id)) {
        (End::Node(n), Some(&(m, 0, _, role))) => m == n && (role & 1 == input || input == 0),
        (End::Bay, None) => own.get(&id).is_some_and(|o| o.0 == 0),
        _ => false,
    };
    let mut fresh = Vec::new();
    let mut used: Vec<u64> = Vec::new();
    for c in cables {
        let from = Some(c.via.0).filter(|&id| fits(id, c.from, 0)).or(match c.from {
            End::Bay => main_in,
            End::Node(n) => socket(n, 0),
        });
        let to = Some(c.via.1).filter(|&id| fits(id, c.to, 1)).or(match c.to {
            End::Bay => main_out,
            End::Node(n) => socket(n, 1),
        });
        let (Some(from), Some(to)) = (from, to) else { continue };
        used.extend([from, to]);
        let mut d = to.to_le_bytes().to_vec();
        d.extend(from.to_le_bytes());
        d.extend(c.level.to_le_bytes());
        d.extend(word.to_le_bytes());
        d.extend([0u8; 8]);
        let mut g = Vec::new();
        put(&mut g, 1300, &d);
        put(&mut g, 1301, &[]);
        put(&mut fresh, 0, &g);
    }
    // the new cables go where the first cable lay, or ahead of the view chunk that follows the cables
    let spot = raw.first().map(|c| c.0).or_else(|| top.iter().position(|c| c.0 == 1500)).unwrap_or(top.len());
    let mut out = base[..4].to_vec();
    for (at, (id, r)) in top.iter().enumerate() {
        if at == spot {
            out.extend(&fresh);
        }
        if gone.contains(&at) {
            continue;
        }
        let g = &base[r.clone()];
        let inner = if *id == 0 { chunks(g, 0).filter(|c| c.first().map(|c| c.0) == Some(5000)) } else { None };
        let Some(inner) = inner else {
            put(&mut out, *id, g);
            continue;
        };
        // a socket a cable now ends at is marked as in use
        let mut body = Vec::with_capacity(g.len());
        for (j, q) in inner {
            let mut d = g[q].to_vec();
            if j == 5009 && d.len() >= 20 && u32at(&d, 0) == Some(0) && u64at(&d, 8).is_some_and(|s| used.contains(&s)) {
                d[16] |= 2;
            }
            put(&mut body, j, &d);
        }
        put(&mut out, 0, &body);
    }
    if spot == top.len() {
        out.extend(&fresh);
    }
    Some(out)
}

impl Patch {
    /// After cables were changed by hand: put the units back in an order that follows them.
    pub fn rewire(&mut self) {
        let mut deps = self.cables.clone();
        deps.extend(self.sets.iter().map(|s| Cable::new(End::Node(s.from.0), End::Node(s.to.0), 1.0)));
        self.order = sort(self.nodes.len(), &deps);
        self.rewired = true;
    }

    /// Whether a unit takes sound in and gives sound out (what a hand-laid cable can reach).
    pub fn can_cable(&self, n: usize) -> bool {
        self.nodes.get(n).is_some_and(|u| u.fx.is_some())
    }
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
    /// What each setting cable last carried; not a number until its sender has been heard once.
    last: Vec<f32>,
    /// The patch the units' settings were last brought in line with.
    seen: Option<std::sync::Arc<Patch>>,
    /// Where the song stands, in beats, and its tempo: for held units that keep time with it.
    pub song_at: (f64, f32),
    /// Settings sent to held instruments, which are played outside: the unit, the setting, the value.
    pub gen_sets: Vec<(usize, u16, f32)>,
}

impl State {
    pub fn new(p: &Patch, sr: f32) -> Self {
        Self {
            units: p.nodes.iter().map(|n| n.fx.clone().map(|f| Unit::new(f, sr))).collect(),
            bufs: vec![vec![[0.0; 2]; 1024]; p.nodes.len()],
            scratch: vec![[0.0; 2]; 1024],
            feeds: p.feeds.iter().map(|&n| (n, vec![[0.0; 2]; 1024], false)).collect(),
            last: vec![f32::NAN; p.sets.len()],
            seen: None,
            song_at: (0.0, 130.0),
            gen_sets: Vec::new(),
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
    pub fn sync(&mut self, p: &std::sync::Arc<Patch>) {
        // only when the patch itself was changed: in between, cables move the units' settings and they stay moved
        if self.seen.as_ref().is_some_and(|s| std::sync::Arc::ptr_eq(s, p)) {
            return;
        }
        let before = self.seen.replace(p.clone());
        self.last.resize(p.sets.len(), f32::NAN);
        // the first time round the units are as they were made, from this very patch
        let Some(before) = before else { return };
        for (i, (u, n)) in self.units.iter_mut().zip(&p.nodes).enumerate() {
            // a unit nobody edited keeps what the cables made of it
            if before.nodes.get(i).is_some_and(|o| o.fx == n.fx) {
                continue;
            }
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

/// Move a setting of a held unit, as a cable or an outside sender asks.
fn set(p: &Patch, units: &mut [Option<Unit>], gen_sets: &mut Vec<(usize, u16, f32)>, to: (usize, u16), v: f32) {
    match units.get_mut(to.0) {
        Some(Some(u)) if u.fx.kind != crate::fx::Kind::Thru => u.fx.set_travel(to.1 as usize, v),
        // an instrument, or a unit we have no sound for
        _ if to.0 < p.nodes.len() && gen_sets.len() < 256 => gen_sets.push((to.0, to.1, v)),
        _ => {}
    }
}

impl State {
    /// An output of a held unit that is run outside the bay (a note script's, say) has moved: everything cabled to it follows.
    pub fn drive(&mut self, p: &Patch, from: (usize, u16), v: f32) {
        for c in p.sets.iter().filter(|c| c.from == from) {
            set(p, &mut self.units, &mut self.gen_sets, c.to, v.clamp(0.0, 1.0));
        }
    }
}

pub fn process(p: &Patch, st: &mut State, io: &mut [[f32; 2]], cx: &Ctx) {
    if !p.cables.iter().any(|c| c.to == End::Bay) {
        // nothing reaches the output we know of: better the dry sound than silence
        if p.sets.is_empty() {
            return;
        }
    }
    let n = io.len();
    if st.scratch.len() < n {
        st.scratch.resize(n, [0.0; 2]);
        for b in &mut st.bufs {
            b.resize(n, [0.0; 2]);
        }
    }
    let song_at = st.song_at;
    let State { units, bufs, scratch, feeds, last, gen_sets, .. } = st;
    let sounds = p.cables.iter().any(|c| c.to == End::Bay);
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
            u.song_at = song_at;
            u.process(&mut scratch[..n], cx);
        }
        bufs[i][..n].copy_from_slice(&scratch[..n]);
        // what the unit sends goes down its setting cables, each time it has changed; when two senders move
        // one setting, the one that moved last has it
        if units[i].is_some() {
            for (k, c) in p.sets.iter().enumerate().filter(|(_, c)| c.from.0 == i) {
                let Some(v) = units[i].as_ref().and_then(|u| u.sends(c.from.1 as usize)) else { continue };
                let Some(l) = last.get_mut(k) else { continue };
                if l.is_nan() {
                    *l = v;
                } else if (v - *l).abs() > 1e-6 {
                    *l = v;
                    set(p, units, gen_sets, c.to, v.clamp(0.0, 1.0));
                }
            }
        }
    }
    if !sounds {
        return;
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
        assert_eq!(p.cables, vec![Cable::new(End::Bay, End::Bay, 1.0)]);
        assert_eq!(encode(&p, Some(&b)), b);
        let mut st = State::new(&p, 44100.0);
        let mut io = vec![[0.25, -0.5]; 2000];
        process(&p, &mut st, &mut io, &Ctx { sr: 44100.0, step: 5512.5, clock: Default::default() });
        assert!(io.iter().all(|s| *s == [0.25, -0.5]));
    }

    fn bay(senders: usize) -> Patch {
        use crate::fx::Kind;
        let mut nodes: Vec<Node> = (0..senders).map(|_| Node { ident: "Fruity X-Y Controller".into(), name: "s".into(), fx: Some(Fx::new(Kind::Xy)), notes: None }).collect();
        nodes.push(Node { ident: "Fruity Balance".into(), name: "b".into(), fx: Some(Fx::new(Kind::Balance)), notes: None });
        let c = |a, b| Cable::new(a, b, 1.0);
        Patch {
            cables: vec![c(End::Bay, End::Node(senders)), c(End::Node(senders), End::Bay)],
            order: (0..=senders).collect(),
            sets: (0..senders).map(|i| SetCable { from: (i, 0), to: (senders, 1) }).collect(),
            nodes,
            ..Default::default()
        }
    }

    fn level(p: &std::sync::Arc<Patch>, st: &mut State) -> f32 {
        let cx = Ctx { sr: 44100.0, step: 5512.5, clock: Default::default() };
        let mut io = vec![[0.5, 0.5]; 512];
        for _ in 0..40 {
            io.fill([0.5, 0.5]);
            st.sync(p);
            process(p, st, &mut io, &cx);
        }
        io[511][0].abs()
    }

    #[test]
    fn a_setting_cable_sends_when_its_sender_moves() {
        let mut p = bay(1);
        p.nodes[0].fx.as_mut().unwrap().p[0] = 65536.0;
        let a = std::sync::Arc::new(p.clone());
        let mut st = State::new(&a, 44100.0);
        // a sender that sits still leaves the setting where it was stored
        let dry = level(&a, &mut st);
        assert!((dry - 0.5).abs() < 0.02, "{dry}");
        p.nodes[0].fx.as_mut().unwrap().p[0] = 0.0;
        let b = std::sync::Arc::new(p.clone());
        assert!(level(&b, &mut st) < 0.005);
        // and the moved setting stays moved while the sender rests
        assert!(level(&b, &mut st) < 0.005);
        p.nodes[0].fx.as_mut().unwrap().p[0] = 65536.0;
        let c = std::sync::Arc::new(p);
        assert!(level(&c, &mut st) > dry * 1.1);
    }

    #[test]
    fn of_two_senders_the_one_that_moved_last_has_the_setting() {
        let mut p = bay(2);
        let a = std::sync::Arc::new(p.clone());
        let mut st = State::new(&a, 44100.0);
        level(&a, &mut st);
        p.nodes[0].fx.as_mut().unwrap().p[0] = 65536.0;
        let b = std::sync::Arc::new(p.clone());
        let loud = level(&b, &mut st);
        p.nodes[1].fx.as_mut().unwrap().p[0] = 100.0;
        let c = std::sync::Arc::new(p.clone());
        let quiet = level(&c, &mut st);
        assert!(quiet < loud * 0.1, "{quiet} {loud}");
        // the first sender has not moved since: it does not take the setting back
        assert!(level(&c, &mut st) < loud * 0.1);
    }

    #[test]
    fn an_outside_sender_moves_what_it_is_cabled_to() {
        let p = std::sync::Arc::new(bay(1));
        let mut st = State::new(&p, 44100.0);
        level(&p, &mut st);
        st.drive(&p, (0, 0), 0.0);
        // the unit inside has not moved, so what came from outside holds
        assert!(level(&p, &mut st) < 0.005);
        st.drive(&p, (0, 1), 1.0);
        assert!(level(&p, &mut st) < 0.005);
        // an instrument's settings are handed out instead
        let mut q = bay(1);
        q.nodes[1].fx = None;
        let q = std::sync::Arc::new(q);
        let mut st = State::new(&q, 44100.0);
        st.drive(&q, (0, 0), 0.25);
        assert_eq!(st.gen_sets, vec![(1, 1, 0.25)]);
    }

    #[test]
    fn a_board_sends_each_of_its_controls() {
        let mut p = bay(1);
        let mut board = Fx::new(crate::fx::Kind::Surface);
        board.p = vec![0.0, 0.0, 0.0, 1.0];
        p.nodes[0].fx = Some(board);
        p.sets[0].from = (0, 3);
        let a = std::sync::Arc::new(p.clone());
        let mut st = State::new(&a, 44100.0);
        level(&a, &mut st);
        p.nodes[0].fx.as_mut().unwrap().p[3] = 0.0;
        let b = std::sync::Arc::new(p.clone());
        assert!(level(&b, &mut st) < 0.005);
        // a cable into the board turns its control, and the board passes that on
        let mut q = bay(2);
        let mut board = Fx::new(crate::fx::Kind::Surface);
        board.p = vec![1.0];
        q.nodes[1].fx = Some(board);
        q.sets = vec![SetCable { from: (0, 0), to: (1, 0) }, SetCable { from: (1, 0), to: (2, 1) }];
        let a = std::sync::Arc::new(q.clone());
        let mut st = State::new(&a, 44100.0);
        let dry = level(&a, &mut st);
        q.nodes[0].fx.as_mut().unwrap().p[0] = 0.0;
        let b = std::sync::Arc::new(q);
        assert!(dry > 0.4 && level(&b, &mut st) < 0.005);
    }

    /// With CEREAL_FLP_CORPUS set: the boards in the stock bays have the outputs their cables start from.
    #[test]
    fn a_fresh_bay_can_be_uncabled_and_cabled_again() {
        let base = fresh();
        let mut p = decode(&base).unwrap();
        assert_eq!(p.cables.len(), 1);
        p.cables.clear();
        p.rewire();
        let bare = encode(&p, Some(&base));
        assert!(decode(&bare).unwrap().cables.is_empty());
        p.cables.push(Cable::new(End::Bay, End::Bay, 0.5));
        let again = encode(&p, Some(&bare));
        assert_eq!(decode(&again).unwrap().cables, p.cables);
        assert_eq!(again.len(), base.len());
    }

    #[test]
    fn stock_bays_take_new_wiring() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets/Effects/Patcher")];
        let (mut bays, mut kept) = (0, 0);
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(f) = crate::preset::read(&path).ok() else { continue };
                let Some(mut p) = decode(&f.state) else { continue };
                if p.cables.is_empty() {
                    continue;
                }
                bays += 1;
                // the wiring written out again as it stands reads back the same, whatever the sockets used
                let same = recable(&f.state, &p.cables).and_then(|b| decode(&b)).unwrap();
                let key = |c: &Cable| format!("{:?}", c);
                let (mut a, mut b): (Vec<String>, Vec<String>) = (p.cables.iter().map(key).collect(), same.cables.iter().map(key).collect());
                a.sort();
                a.dedup();
                b.sort();
                b.dedup();
                kept += (a == b) as usize;
                assert_eq!(same.cables.len(), p.cables.len(), "{}", path.display());
                assert_eq!(same.sets, p.sets, "{}", path.display());
                assert_eq!(same.nodes.len(), p.nodes.len());
                // one cable less, and a level turned
                p.cables.pop();
                if let Some(c) = p.cables.first_mut() {
                    c.level = 0.25;
                }
                p.rewire();
                let back = decode(&encode(&p, Some(&f.state))).unwrap();
                assert_eq!(back.cables.len(), p.cables.len(), "{}", path.display());
                if let Some(c) = back.cables.iter().find(|c| c.level == 0.25) {
                    assert_eq!((c.from, c.to), (p.cables[0].from, p.cables[0].to));
                } else {
                    assert!(p.cables.is_empty(), "{}", path.display());
                }
            }
        }
        if bays > 0 {
            eprintln!("{kept} of {bays} bays keep their wiring when it is written anew");
            assert!(kept * 20 >= bays * 19);
        }
    }

    #[test]
    fn stock_bays_cable_their_boards() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets/Effects/Patcher")];
        let (mut bays, mut from_board, mut short) = (0, 0, 0);
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(p) = crate::preset::read(&path).ok().and_then(|f| decode(&f.state)) else { continue };
                bays += 1;
                for c in &p.sets {
                    if let Some(f) = p.nodes[c.from.0].fx.as_ref().filter(|f| f.kind == crate::fx::Kind::Surface) {
                        from_board += 1;
                        if c.from.1 as usize >= f.p.len() {
                            short += 1;
                        }
                    }
                }
            }
        }
        eprintln!("{bays} bays, {from_board} cables from boards, {short} from outputs the board does not hold");
        assert!(bays > 50 && from_board > 500 && short * 50 < from_board);
    }

    /// With CEREAL_FLP_CORPUS set: the note units held in the stock instrument bays go back into their
    /// blocks as they came, and an edited one is saved inside the bay and read back.
    #[test]
    fn stock_note_units_survive_editing() {
        use crate::noteshape::Shaper;
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let mut stack = vec![std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Patcher")];
        let (mut units, mut edited, mut lines, mut bays) = (0, 0, 0, 0);
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(f) = crate::preset::read(&path).ok() else { continue };
                let Some((mut p, _, _)) = read(&f.state, true) else { continue };
                bays += 1;
                assert_eq!(encode(&p, Some(&f.state)), f.state, "{}", path.display());
                for i in 0..p.nodes.len() {
                    let Some(block) = p.nodes[i].notes.clone() else { continue };
                    let ident = p.nodes[i].ident.clone();
                    let Some(mut s) = Shaper::decode(&ident, &block) else { continue };
                    units += 1;
                    if !matches!(s, Shaper::Motion(_) | Shaper::Script(_)) {
                        assert_eq!(s.encode(&ident, &block).as_deref(), Some(&block[..]), "{} unit {i}", path.display());
                    }
                    for &h in &Shaper::line_heads(&ident, &block) {
                        if let Some(c) = crate::fm::Curve::read_at(&block, h) {
                            let mut b = block.clone();
                            assert!(crate::fm::Curve::replace_at(&mut b, h, &c));
                            assert_eq!(Shaper::decode(&ident, &b), Shaper::decode(&ident, &block), "{} unit {i} line at {h}", path.display());
                            lines += 1;
                        }
                    }
                    match &mut s {
                        Shaper::Keys { mode, .. } => *mode = (*mode + 1) % 5,
                        Shaper::Zones(z) => z[0].transpose += 1,
                        Shaper::Colours { colour, .. } => colour[0] = (colour[0] + 1) % 16,
                        Shaper::Levels(l) => l[0].offset += 0.125,
                        _ => continue,
                    }
                    let new = s.encode(&ident, &block).unwrap();
                    assert!(p.set_note_block(i, new));
                    let saved = encode(&p, Some(&f.state));
                    let back = read(&saved, true).unwrap().0;
                    assert_eq!(Shaper::decode(&ident, back.nodes[i].notes.as_ref().unwrap()), Some(s), "{} unit {i}", path.display());
                    edited += 1;
                }
            }
        }
        eprintln!("{bays} bays, {units} note units, {edited} edited and read back, {lines} lines put back");
        assert!(edited > 3);
    }

    #[test]
    fn units_run_after_their_feeders() {
        let c = |a, b| Cable::new(a, b, 1.0);
        let order = sort(3, &[c(End::Bay, End::Node(2)), c(End::Node(2), End::Node(0)), c(End::Node(0), End::Node(1)), c(End::Node(1), End::Bay)]);
        assert_eq!(order, vec![2, 0, 1]);
        // a loop still lists every unit once
        let order = sort(2, &[c(End::Node(0), End::Node(1)), c(End::Node(1), End::Node(0))]);
        assert_eq!(order, vec![0, 1]);
    }
}
