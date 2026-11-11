//! Print what the reader understands of a .flp file.  cargo run --example flp_info -- song.flp [-v]
use cereal_engine::flp::{self, ClipSource};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "-v");
    for path in args.iter().filter(|a| *a != "-v") {
        let bytes = std::fs::read(path).expect("read file");
        let f = match flp::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let notes: usize = f.patterns.iter().map(|p| p.notes.len()).sum();
        println!("{path}\n  version {}  title {:?}  tempo {}  ppq {}  sig {}/{}", f.version, f.title, f.tempo, f.header.ppq, f.time_sig.0, f.time_sig.1);
        println!("  {} channels, {} patterns ({} notes), {} arrangements, {} inserts", f.channels.len(), f.patterns.len(), notes, f.arrangements.len(), f.inserts.len());
        for a in &f.arrangements {
            let pat = a.items.iter().filter(|i| matches!(i.source, ClipSource::Pattern(_))).count();
            let named = a.tracks.iter().filter(|t| !t.name.is_empty()).count();
            let end = a.items.iter().map(|i| i.position + i.length).max().unwrap_or(0);
            let top = a.items.iter().map(|i| i.track).max().unwrap_or(0);
            println!("  arrangement {:?}: {} clips ({} pattern), {} tracks ({} named), lowest track used {}, ends at tick {}, {} markers", a.name, a.items.len(), pat, a.tracks.len(), named, top + 1, end, a.markers.len());
        }
        if verbose {
            for c in &f.channels {
                println!("  ch {:3} {:?} {:?} plugin {:?} vol {} pan {} insert {} root {} sample {:?}", c.index, c.kind, c.name, c.plugin, c.volume, c.pan, c.insert, c.root_key, c.sample_path);
            }
            for p in &f.patterns {
                println!("  pat {:3} {:?} {} notes", p.index, p.name, p.notes.len());
            }
            for i in f.inserts.iter().filter(|i| !i.name.is_empty() || !i.slots.is_empty() || i.volume != 12800) {
                let slots: Vec<String> = i.slots.iter().map(|s| format!("{}:{}", s.index, s.plugin)).collect();
                let routes: Vec<usize> = i.routes.iter().enumerate().filter(|(_, r)| **r).map(|(k, _)| k).collect();
                println!("  insert {:3} {:?} vol {} pan {} -> {:?} slots {:?}", i.index, i.name, i.volume, i.pan, routes, slots);
            }
        }
    }
}
