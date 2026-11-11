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
    ident.eq_ignore_ascii_case("Fruity Slicer") || ident.eq_ignore_ascii_case("Slicex")
}

/// The marker points stored in a wave file, in frames, with the file's rate.
pub fn markers(wav: &[u8]) -> (Vec<usize>, u32) {
    let mut out = vec![];
    let mut rate = 44100;
    if wav.len() < 12 || &wav[..4] != b"RIFF" {
        return (out, rate);
    }
    let int = |at: usize| wav.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let mut at = 12;
    while let (Some(id), Some(n)) = (wav.get(at..at + 4), int(at + 4)) {
        let body = at + 8;
        if id == b"fmt " {
            rate = int(body + 4).unwrap_or(44100) as u32;
        } else if id == b"cue " {
            let count = int(body).unwrap_or(0).min(4096);
            for k in 0..count {
                // each point: its number, its place, the piece it is in, two starts, and the frame
                if let Some(frame) = int(body + 4 + k * 24 + 20) {
                    out.push(frame);
                }
            }
        }
        at = body + n + (n & 1);
    }
    out.sort_unstable();
    out.dedup();
    (out, rate)
}

/// Read a slicing instrument's block and load its loop. The second kind keeps its cuts as markers inside
/// the wave, which is either stored whole in the block or named by its path.
pub fn open(ident: &str, state: &[u8], mut lib: Option<&mut crate::sample::Library>, project_dir: Option<&std::path::Path>) -> Option<(Slices, Option<std::sync::Arc<crate::sample::Sample>>)> {
    if !is_slicer(ident) {
        return None;
    }
    if ident.eq_ignore_ascii_case("Fruity Slicer") {
        let sl = Slices::parse(state)?;
        let smp = lib.as_deref_mut().and_then(|l| l.load_stored(&sl.path, project_dir));
        return Some((sl, smp));
    }
    // a fixed head, the stored path, then the length of a wave kept in the block (0 for none)
    let n = *state.get(96)? as usize;
    let path = String::from_utf8_lossy(state.get(97..97 + n)?).into_owned();
    let at = 97 + n;
    let size = u32::from_le_bytes(state.get(at..at + 4)?.try_into().unwrap()) as usize;
    let (points, rate, smp) = if size > 0 {
        let wav = state.get(at + 4..at + 4 + size)?;
        let (points, rate) = markers(wav);
        (points, rate, crate::sample::Sample::decode(wav.to_vec(), "wav").ok().map(std::sync::Arc::new))
    } else {
        let lib = lib.as_deref_mut()?;
        let file = lib.resolve(&path, project_dir);
        let (points, rate) = file.as_ref().and_then(|f| std::fs::read(f).ok()).map(|b| markers(&b)).unwrap_or((vec![], 44100));
        (points, rate, lib.load_stored(&path, project_dir))
    };
    let mut cuts: Vec<Cut> = points.into_iter().map(|pos| Cut { pos, key: -1, reverse: false, name: String::new() }).collect();
    if cuts.is_empty() {
        // no markers: the whole loop is one slice
        cuts.push(Cut { pos: 0, key: -1, reverse: false, name: String::new() });
    }
    let f64_at = |o: usize| state.get(at + 4 + size + o..at + 12 + size + o).map(|b| f64::from_le_bytes(b.try_into().unwrap()) as f32);
    Some((Slices { beats: f64_at(24).unwrap_or(0.0), bpm: 0.0, path, cuts, base: 60, rate }, smp))
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
    fn a_wave_kept_in_the_block_gives_its_markers_as_slices() {
        let tone: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let mut cur = std::io::Cursor::new(Vec::new());
        crate::wav::write_pcm16(&mut cur, &tone, 48_000, 1).unwrap();
        let mut wav = cur.into_inner();
        let mut cue = 3u32.to_le_bytes().to_vec();
        for (k, f) in [0u32, 1200, 2400].iter().enumerate() {
            for v in [k as u32 + 1, *f, u32::from_le_bytes(*b"data"), 0, 0, *f] {
                cue.extend(v.to_le_bytes());
            }
        }
        wav.extend(b"cue ");
        wav.extend((cue.len() as u32).to_le_bytes());
        wav.extend(&cue);
        let mut block = vec![0u8; 96];
        block.push(5);
        block.extend(b"a.wav");
        block.extend((wav.len() as u32).to_le_bytes());
        block.extend(&wav);
        block.extend([0u8; 40]);
        let (sl, smp) = open("Slicex", &block, None, None).unwrap();
        assert_eq!(sl.cuts.iter().map(|c| c.pos).collect::<Vec<_>>(), [0, 1200, 2400]);
        assert_eq!(sl.rate, 48_000);
        assert_eq!(smp.unwrap().frames.len(), 4800);
        assert_eq!(sl.for_key(61, 4800), Some((1200, 1200, false)));
    }

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
