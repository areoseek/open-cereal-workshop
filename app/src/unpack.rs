//! Reads content files (sounds, presets, demo projects, templates) out of an installer the user downloaded
//! themselves. The installer is an NSIS self-extracting archive, an open source container: a small program,
//! then a header block and one LZMA stream per file. We read it as data. Nothing in it is run, no program
//! files are taken, and the result stays on the user's disk.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Folders under the install's `Data` folder that hold content rather than program parts.
const WANTED: [&str; 4] = ["patches", "demo projects", "projects", "templates"];
/// What is taken from the folders of the built-in instruments: their sample sets, never program parts.
const SOUND_EXT: [&str; 6] = ["wav", "wti", "dat", "ogg", "flac", "fst"];
const SKIP_EXT: [&str; 6] = ["exe", "dll", "sys", "ocx", "bat", "cmd"];
const OP_SET_DIR: i32 = 11;
const OP_EXTRACT: i32 = 20;

#[derive(Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub cancel: AtomicBool,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn unlzma(raw: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    if raw.len() < 5 {
        return Err("short data block".into());
    }
    // the stream carries the five property bytes but no length; it ends at its end marker
    let opts = lzma_rs::decompress::Options { unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(None), ..Default::default() };
    lzma_rs::lzma_decompress_with_options(&mut std::io::Cursor::new(raw), out, &opts).map_err(|e| format!("could not unpack a block: {e:?}"))
}

/// One length-prefixed block at the reader's position.
fn read_block<R: Read>(r: &mut R, limit: usize) -> Result<Vec<u8>, String> {
    let mut n = [0u8; 4];
    r.read_exact(&mut n).map_err(|e| e.to_string())?;
    let n = u32::from_le_bytes(n);
    let (packed, len) = (n & 0x8000_0000 != 0, (n & 0x7fff_ffff) as usize);
    if len > limit {
        return Err("a block is larger than the file allows".into());
    }
    let mut raw = vec![0u8; len];
    r.read_exact(&mut raw).map_err(|e| e.to_string())?;
    if !packed {
        return Ok(raw);
    }
    let mut out = Vec::with_capacity(len * 3);
    unlzma(&raw, &mut out)?;
    Ok(out)
}

/// A string from the header's table: the text after any leading variable (the install folder).
fn string_at(h: &[u8], strings: usize, off: i32) -> Option<String> {
    if off < 0 {
        return None;
    }
    let mut p = strings + off as usize * 2;
    let mut units = vec![];
    loop {
        let c = u16::from_le_bytes(h.get(p..p + 2)?.try_into().ok()?);
        p += 2;
        match c {
            0 => break,
            1..=4 => p += 2, // a variable, shell folder or language reference: not text
            c => units.push(c),
        }
        if units.len() > 1024 {
            return None;
        }
    }
    Some(String::from_utf16_lossy(&units))
}

/// Path parts that are safe to create under the destination.
fn safe_parts(path: &str) -> Option<Vec<String>> {
    let parts: Vec<String> = path.split(['\\', '/']).filter(|p| !p.is_empty()).map(|p| p.to_string()).collect();
    if parts.iter().any(|p| p == ".." || p == "." || p.contains(':') || p.contains('\0')) {
        return None;
    }
    Some(parts)
}

struct Item {
    dir: Vec<String>,
    name: String,
    offset: u64,
}

struct Index {
    data_start: u64,
    items: Vec<Item>,
}

fn index(f: &mut BufReader<File>, file_len: u64) -> Result<Index, String> {
    let mut head = vec![0u8; (4 << 20).min(file_len as usize)];
    f.read_exact(&mut head).map_err(|e| e.to_string())?;
    let magic = head.windows(16).position(|w| w[..4] == [0xef, 0xbe, 0xad, 0xde] && &w[4..] == b"NullsoftInst").ok_or("This file is not an installer this program can read.")?;
    let header_len = u32_at(&head, magic + 16).ok_or("short file")? as usize;
    // the header block follows the lengths; the large-file variant has eight more bytes before it
    let at = [24usize, 28, 32]
        .into_iter()
        .map(|d| magic + d)
        .find(|&a| matches!(u32_at(&head, a), Some(n) if n & 0x8000_0000 != 0 && (n & 0x7fff_ffff) as u64 <= file_len) && head.get(a + 4) == Some(&0x5d))
        .ok_or("This installer packs its files in a way this program cannot read yet.")?;
    f.seek(SeekFrom::Start(at as u64)).map_err(|e| e.to_string())?;
    let h = read_block(f, file_len as usize)?;
    if h.len() != header_len {
        return Err("The installer's header did not unpack to the expected size.".into());
    }
    let data_start = f.stream_position().map_err(|e| e.to_string())?;
    let (entries, count, strings) = (u32_at(&h, 20).ok_or("short header")? as usize, u32_at(&h, 24).ok_or("short header")? as usize, u32_at(&h, 28).ok_or("short header")? as usize);
    if count == 0 || strings <= entries || strings > h.len() {
        return Err("The installer's header has an unexpected layout.".into());
    }
    let size = (strings - entries) / count;
    if size < 28 || size % 4 != 0 || h.get(strings..strings + 2) != Some(&[0, 0]) {
        return Err("The installer's header has an unexpected layout.".into());
    }
    let int = |e: usize, k: usize| u32_at(&h, entries + e * size + k * 4).unwrap_or(0) as i32;
    let mut dir: Option<Vec<String>> = None;
    let mut items = vec![];
    for e in 0..count {
        match int(e, 0) {
            OP_SET_DIR if int(e, 2) != 0 => {
                // keep only folders under <install>\Data\<a content folder>
                dir = string_at(&h, strings, int(e, 1)).and_then(|s| safe_parts(&s)).filter(|p| p.len() >= 2 && ((p[0].eq_ignore_ascii_case("data") && WANTED.contains(&p[1].to_lowercase().as_str())) || (p.len() >= 4 && p[0].eq_ignore_ascii_case("plugins") && p[1].eq_ignore_ascii_case("fruity"))));
            }
            OP_EXTRACT => {
                let (Some(d), Some(name)) = (&dir, string_at(&h, strings, int(e, 2))) else { continue };
                let Some(mut parts) = safe_parts(&name) else { continue };
                let Some(file) = parts.pop() else { continue };
                let ext = file.rsplit('.').next().unwrap_or("").to_lowercase();
                if SKIP_EXT.contains(&ext.as_str()) || (d[0].eq_ignore_ascii_case("plugins") && !SOUND_EXT.contains(&ext.as_str())) {
                    continue;
                }
                let high = if size >= 36 { int(e, 4) as u32 as u64 } else { 0 };
                let offset = (high << 32) | int(e, 3) as u32 as u64;
                if data_start + offset + 4 > file_len {
                    continue;
                }
                let mut full = d.clone();
                full.extend(parts);
                items.push(Item { dir: full, name: file, offset });
            }
            _ => {}
        }
    }
    if items.is_empty() {
        return Err("No sounds or presets were found in this installer.".into());
    }
    Ok(Index { data_start, items })
}

/// Unpack the content files of `installer` under `dest`. Returns how many files were written.
pub fn unpack(installer: &Path, dest: &Path, progress: &Progress) -> Result<usize, String> {
    let file = File::open(installer).map_err(|e| format!("Could not open {}: {e}", installer.display()))?;
    let file_len = file.metadata().map_err(|e| e.to_string())?.len();
    let mut f = BufReader::with_capacity(1 << 20, file);
    let idx = index(&mut f, file_len)?;
    progress.total.store(idx.items.len(), Ordering::Relaxed);
    let mut made: HashSet<PathBuf> = HashSet::new();
    let mut written = 0;
    for it in &idx.items {
        if progress.cancel.load(Ordering::Relaxed) {
            return Err("Cancelled.".into());
        }
        let mut dir = dest.to_path_buf();
        dir.extend(&it.dir);
        if made.insert(dir.clone()) {
            std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        }
        f.seek(SeekFrom::Start(idx.data_start + it.offset)).map_err(|e| e.to_string())?;
        // one damaged file should not lose the rest
        if let Ok(bytes) = read_block(&mut f, (file_len - idx.data_start) as usize) {
            let path = dir.join(&it.name);
            File::create(&path).and_then(|mut o| o.write_all(&bytes)).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
            written += 1;
        }
        progress.done.fetch_add(1, Ordering::Relaxed);
    }
    if written == 0 {
        return Err("Nothing could be unpacked from this installer.".into());
    }
    Ok(written)
}
