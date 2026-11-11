//! The loop slicer: a loop cut at stored points, each cut played by its own key. The block holds the loop's
//! length and tempo, the file's stored path, and the cuts; the notes that play them live in the patterns.

#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    /// Frame of the loop where the slice starts.
    pub pos: usize,
    /// The key that plays it, or -1 for "in order from the base key".
    pub key: i32,
    pub reverse: bool,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slices {
    pub beats: f32,
    pub bpm: f32,
    pub path: String,
    pub cuts: Vec<Cut>,
    /// The key of the first slice.
    pub base: i32,
    /// Frames per second the cut points are counted in.
    pub rate: u32,
}

pub fn is_slicer(ident: &str) -> bool {
    ident.eq_ignore_ascii_case("Fruity Slicer")
}

impl Slices {
    pub fn parse(state: &[u8]) -> Option<Self> {
        let int = |at: usize| state.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
        let flt = |at: usize| state.get(at..at + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()));
        let text = |at: usize| {
            let n = *state.get(at)? as usize;
            Some((String::from_utf8_lossy(state.get(at + 1..at + 1 + n)?).into_owned(), at + 1 + n))
        };
        if !(1..=64).contains(&int(0)?) {
            return None;
        }
        let (path, mut at) = text(32)?;
        let count = int(at)?;
        at += 4;
        if !(0..=4096).contains(&count) {
            return None;
        }
        let mut cuts = vec![];
        for _ in 0..count {
            let (name, next) = text(at)?;
            let pos = int(next)?.max(0) as usize;
            let key = int(next + 4)?;
            let reverse = *state.get(next + 12)? != 0;
            at = next + 13;
            cuts.push(Cut { pos, key, reverse, name });
        }
        // after the cuts: a switch, the base key and the rate the cuts are counted in
        let base = int(at + 1).filter(|k| (0..128).contains(k)).unwrap_or(60);
        let rate = int(at + 5).filter(|r| (8000..=384000).contains(r)).unwrap_or(44100) as u32;
        Some(Self { beats: flt(4)?, bpm: flt(8)?, path, cuts, base, rate })
    }

    /// The slice a key plays: (first frame, frames, reversed), in the cut points' own rate.
    pub fn for_key(&self, key: u8, total: usize) -> Option<(usize, usize, bool)> {
        let k = self.cuts.iter().position(|c| c.key == key as i32).or_else(|| {
            let i = key as i32 - self.base;
            (i >= 0 && (i as usize) < self.cuts.len() && self.cuts[i as usize].key < 0).then_some(i as usize)
        })?;
        let start = self.cuts[k].pos;
        // a slice runs to the next cut after it, wherever that one sits in the list
        let end = self.cuts.iter().map(|c| c.pos).filter(|&p| p > start).min().unwrap_or(total);
        (end > start).then_some((start, end - start, self.cuts[k].reverse))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_read_and_keys_find_slices() {
        let mut b = vec![];
        b.extend_from_slice(&15i32.to_le_bytes());
        b.extend_from_slice(&8f32.to_le_bytes());
        b.extend_from_slice(&60f32.to_le_bytes());
        for v in [0i32, 0, 4, 0, 4] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(8);
        b.extend_from_slice(b"loop.wav");
        b.extend_from_slice(&3i32.to_le_bytes());
        for (name, pos) in [("Kick", 0i32), ("", 22050), ("Snare", 44100)] {
            b.push(name.len() as u8);
            b.extend_from_slice(name.as_bytes());
            b.extend_from_slice(&pos.to_le_bytes());
            b.extend_from_slice(&(-1i32).to_le_bytes());
            b.extend_from_slice(&(-1f32).to_le_bytes());
            b.push(0);
        }
        b.push(1);
        b.extend_from_slice(&60i32.to_le_bytes());
        b.extend_from_slice(&44100i32.to_le_bytes());
        b.extend_from_slice(&[1, 1, 1, 0, 0, 0, 0]);
        let s = Slices::parse(&b).unwrap();
        assert_eq!((s.path.as_str(), s.cuts.len(), s.base, s.rate), ("loop.wav", 3, 60, 44100));
        assert_eq!(s.cuts[2].name, "Snare");
        assert_eq!(s.for_key(60, 60000), Some((0, 22050, false)));
        assert_eq!(s.for_key(62, 60000), Some((44100, 15900, false)));
        assert_eq!(s.for_key(63, 60000), None);
        assert_eq!(s.for_key(59, 60000), None);
    }
}
