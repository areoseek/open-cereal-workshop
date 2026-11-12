//! What a folder of projects needs that is not found or not played: `survey <folder> [factory root]`.
use cereal_engine::song::Song;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("folder");
    let factory = args.next();
    let mut stack = vec![std::path::PathBuf::from(root)];
    let (mut files, mut chans, mut silent) = (0, 0, 0);
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    let mut quiet: BTreeMap<String, usize> = BTreeMap::new();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp")) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&p) else { continue };
            let Ok(f) = cereal_engine::flp::parse(&bytes) else { continue };
            let mut lib = cereal_engine::Library::new();
            if let Some(r) = &factory {
                lib.factory_roots.push(r.into());
            }
            let song = Song::from_flp_with(&f, Some(&mut lib), p.parent());
            files += 1;
            for m in &lib.missing {
                let ext = m.split(": ").next().unwrap_or("").rsplit('.').next().unwrap_or("").to_lowercase();
                let odd = ext.len() > 6 || !ext.is_ascii();
                *missing.entry(if odd { "(odd name)".into() } else { ext }).or_default() += 1;
                if std::env::var_os("SURVEY_NAMES").is_some() {
                    println!("    missing: {}", m);
                }
            }
            for (ci, c) in song.channels.iter().enumerate() {
                use cereal_engine::flp::ChannelKind as K;
                if !matches!(c.kind, K::Plugin | K::Other(_)) {
                    continue;
                }
                chans += 1;
                let plays = c.gen.is_some() || c.ext.is_some() || c.slices.is_some() && c.sample.is_some() || c.zones.as_ref().is_some_and(|z| z.zones.iter().any(|z| z.sample.is_some()));
                if !plays {
                    silent += 1;
                    let mut what = c.plugin.to_lowercase();
                    if what == "fruity wrapper" {
                        let st = f.channels.get(ci).map(|fc| fc.state.clone()).unwrap_or_default();
                        what = format!("wrapper: {}", inside(&st));
                    }
                    if std::env::var_os("SURVEY_WHY").is_some() && !what.starts_with("wrapper") {
                        let st = f.channels.get(ci).map(|fc| fc.state.clone()).unwrap_or_default();
                        let hex: String = st.iter().take(24).map(|b| format!("{b:02x}")).collect();
                        println!("    quiet: {} | {} | {} | {} bytes {} | sample {:?}", f.version, p.file_name().unwrap().to_string_lossy(), what, st.len(), hex, c.sample_path);
                    }
                    *quiet.entry(what).or_default() += 1;
                }
            }
        }
    }
    println!("{files} projects, {chans} instrument channels, {silent} without their sound");
    let mut rows: Vec<_> = quiet.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (n, c) in rows {
        println!("  {c:5}  {n}");
    }
    println!("files not found, by kind: {missing:?}");
}

/// The name of the plug-in a host wrapper block holds.
fn inside(state: &[u8]) -> String {
    let mut at = 4;
    while at + 12 <= state.len() {
        let id = u32::from_le_bytes(state[at..at + 4].try_into().unwrap());
        let size = u64::from_le_bytes(state[at + 4..at + 12].try_into().unwrap()) as usize;
        at += 12;
        let Some(data) = state.get(at..at.saturating_add(size)) else { break };
        if id == 54 {
            return String::from_utf8_lossy(data).trim_end_matches('\0').to_lowercase();
        }
        at += size;
    }
    format!("(old layout, head {})", state.first().copied().unwrap_or(0))
}
