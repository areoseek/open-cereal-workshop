//! Count, per block version, how the grid's mute switches are used in the operator synth patches under a folder.
use std::collections::BTreeMap;
fn main() {
    let root = std::env::args().nth(1).expect("folder");
    let mut lib = cereal_engine::Library::new();
    let mut seen: BTreeMap<(u32, u8, bool), usize> = BTreeMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
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
                if cereal_engine::fm::Patch::parse(&g.raw).is_none() {
                    continue;
                }
                let raw = cereal_engine::fm::Patch::unpack(&g.raw).unwrap();
                let ver = u32::from_le_bytes([g.raw[0], g.raw[1], g.raw[2], g.raw[3]]);
                let m = &raw[800..881];
                *seen.entry((ver, *m.iter().max().unwrap(), m.iter().any(|&b| b != 0))).or_default() += 1;
            }
        }
    }
    for ((ver, max, any), n) in seen {
        println!("version {ver}: highest byte {max}, any set {any}: {n}");
    }
}
