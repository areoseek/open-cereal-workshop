//! What the controller links in a folder of projects point at: `links <folder>`.
use std::collections::BTreeMap;

fn main() {
    let root = std::env::args().nth(1).expect("folder");
    let mut stack = vec![std::path::PathBuf::from(root)];
    let mut seen: BTreeMap<(String, u16, u16), usize> = BTreeMap::new();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp")) {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok(f) = cereal_engine::flp::parse(&bytes) else { continue };
                let base = cereal_engine::flp::mixer_base(f.version_tuple());
                for l in &f.links {
                    let raw = (l.dest >> 6) as usize;
                    let what = if l.dest == cereal_engine::flp::DEST_GLOBAL {
                        "global".to_string()
                    } else if l.dest < 0x2000 {
                        match f.channels.get(l.dest as usize) {
                            Some(c) if l.param & 0x8000 != 0 => format!("chan {}", c.plugin),
                            Some(_) => "chan".to_string(),
                            None => "chan ?".to_string(),
                        }
                    } else if raw >= base && raw - base < f.inserts.len() {
                        let slot = (l.dest & 63) as usize;
                        if l.param & 0x8000 != 0 {
                            match f.inserts[raw - base].slots.iter().find(|s| s.index as usize == slot) {
                                Some(s) => format!("slot {}", s.plugin),
                                None => "slot ?".to_string(),
                            }
                        } else {
                            format!("insert s{}", (slot > 0) as u8)
                        }
                    } else {
                        "other".to_string()
                    };
                    *seen.entry((what, l.kind, l.param)).or_default() += 1;
                }
            }
        }
    }
    let mut rows: Vec<_> = seen.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    println!("{} kinds, {} links", rows.len(), rows.iter().map(|r| r.1).sum::<usize>());
    for ((w, k, p), n) in rows.iter().take(90) {
        println!("{n:5}  kind {k:3}  param {p:#06x}  {w}");
    }
}
