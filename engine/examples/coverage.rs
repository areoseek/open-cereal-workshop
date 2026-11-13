//! How much of a folder of projects our units cover: `coverage <folder>`.
use std::collections::BTreeMap;

fn main() {
    let root = std::env::args().nth(1).expect("folder");
    let mut stack = vec![std::path::PathBuf::from(root)];
    // (plugin, size) -> (slots or channels, ones we run)
    let mut fx: BTreeMap<(String, usize), (usize, usize)> = BTreeMap::new();
    let mut gen: BTreeMap<(String, usize), (usize, usize)> = BTreeMap::new();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp")) {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok(f) = cereal_engine::flp::parse(&bytes) else { continue };
                for ins in &f.inserts {
                    for s in &ins.slots {
                        let e = fx.entry((s.plugin.clone(), s.state.len())).or_default();
                        e.0 += 1;
                        e.1 += cereal_engine::fx::Fx::decode(&s.plugin, &s.state).is_some() as usize;
                    }
                }
                for c in &f.channels {
                    if c.kind == cereal_engine::flp::ChannelKind::Plugin {
                        let e = gen.entry((c.plugin.clone(), c.state.len())).or_default();
                        e.0 += 1;
                        e.1 += (cereal_engine::synth::Gen::decode(&c.plugin, &c.state).is_some() || cereal_engine::fxpatch::decode_gen(&c.state).is_some_and(|_| c.plugin == "Patcher") || cereal_engine::slicer::open(&c.plugin, &c.state, None, None).is_some() || cereal_engine::zones::Map::parse(&c.plugin, &c.state).is_some() || cereal_engine::sf2::is_ident(&c.plugin)) as usize;
                    }
                }
            }
        }
    }
    for (what, m) in [("effects", &fx), ("instruments", &gen)] {
        let (all, ours) = m.values().fold((0, 0), |a, v| (a.0 + v.0, a.1 + v.1));
        println!("{what}: {ours} of {all} run");
        let mut by: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for ((n, _), v) in m {
            let e = by.entry(n).or_default();
            e.0 += v.0;
            e.1 += v.1;
        }
        let mut rows: Vec<_> = by.into_iter().collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.1 .0 - r.1 .1));
        for (n, (a, o)) in rows.iter().take(40) {
            println!("  {a:5} {o:5}  {n}");
        }
    }
}
