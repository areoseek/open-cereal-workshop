//! Tally the values the operator synth patches under a folder hold in the effects stretch of their block:
//! `fmfx folder [from to]` prints, for each four-byte place 736..=764 and each byte 965..=977, how often
//! each value turns up (patches that send nothing to the effects are left out).
use std::collections::BTreeMap;
fn main() {
    let root = std::env::args().nth(1).expect("folder");
    let mut lib = cereal_engine::Library::new();
    let mut ints: BTreeMap<usize, BTreeMap<i32, usize>> = BTreeMap::new();
    let mut bytes: BTreeMap<usize, BTreeMap<u8, usize>> = BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    let mut n = 0;
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
            let Ok((s, _)) = cereal_engine::load_flp(&p, &mut lib) else { continue };
            for c in &s.channels {
                let Some(g) = c.gen.as_ref() else { continue };
                let Some(pt) = cereal_engine::fm::Patch::parse(&g.raw) else { continue };
                if !pt.send.iter().any(|&s| s != 0.0) {
                    continue;
                }
                let mut raw = Vec::new();
                cereal_engine::fm::Patch::edited(&g.raw, |d| raw = d.to_vec());
                if raw.len() < 1137 {
                    continue;
                }
                n += 1;
                for o in (736..=764).step_by(4) {
                    *ints.entry(o).or_default().entry(i32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]])).or_default() += 1;
                }
                for o in 965..=977 {
                    *bytes.entry(o).or_default().entry(raw[o]).or_default() += 1;
                }
            }
        }
    }
    println!("{n} patches");
    for (o, m) in ints {
        println!("{o}: {}", m.iter().map(|(v, k)| format!("{v}x{k}")).collect::<Vec<_>>().join(" "));
    }
    for (o, m) in bytes {
        println!("b{o}: {}", m.iter().map(|(v, k)| format!("{v}x{k}")).collect::<Vec<_>>().join(" "));
    }
}
