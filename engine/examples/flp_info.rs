//! Dump what our .flp reader understands from a file: cargo run -p cereal-engine --example flp_info -- file.flp
use cereal_engine::flp;
use std::collections::BTreeMap;

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        match flp::parse(&bytes) {
            Ok(f) => {
                let mut by_id: BTreeMap<u8, usize> = BTreeMap::new();
                for e in &f.events {
                    *by_id.entry(e.id).or_default() += 1;
                }
                let named: Vec<_> = f.channel_names.iter().filter(|s| !s.is_empty()).take(8).cloned().collect();
                println!(
                    "{}\n  version={:?} title={:?} tempo={} chans={} ppq={} events={} notes={}\n  names={:?}\n  distinct event ids={}",
                    path, f.version, f.title, f.tempo, f.header.channels, f.header.ppq, f.events.len(), f.notes.len(), named, by_id.len()
                );
            }
            Err(e) => println!("{path}\n  PARSE ERROR: {e}"),
        }
    }
}
