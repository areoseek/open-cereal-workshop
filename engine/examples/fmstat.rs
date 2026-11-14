//! Count which filter slopes, kinds and level settings the operator synth patches under a folder use.
use std::collections::BTreeMap;
fn main() {
    let root = std::env::args().nth(1).expect("folder");
    // (FIELD picks another setting of the filter than its first level to count by)
    let field: usize = std::env::var("FIELD").ok().and_then(|v| v.parse().ok()).unwrap_or(16);
    let mut lib = cereal_engine::Library::new();
    let mut seen: BTreeMap<(i32, i32, i32), usize> = BTreeMap::new();
    let mut wave: BTreeMap<&str, usize> = BTreeMap::new();
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
                let Some(pt) = cereal_engine::fm::Patch::parse(&g.raw) else { continue };
                let mut raw = Vec::new();
                cereal_engine::fm::Patch::edited(&g.raw, |d| raw = d.to_vec());
                for k in 0..3 {
                    if (pt.out[6 + k] != 0.0 || pt.filts[k].next != 0.0) && raw.len() >= pt.filt_at[k] + field + 4 {
                        let a = pt.filt_at[k] + field;
                        let lv = i32::from_le_bytes([raw[a], raw[a + 1], raw[a + 2], raw[a + 3]]);
                        *seen.entry((pt.filts[k].slope, pt.filts[k].kind, lv)).or_default() += 1;
                    }
                }
                for k in 0..6 {
                    let o = pt.op_at[k];
                    if !pt.used()[k] || raw.len() < o + 2128 {
                        continue;
                    }
                    let i = |a: usize| i32::from_le_bytes([raw[o + a], raw[o + a + 1], raw[o + a + 2], raw[o + a + 3]]);
                    *wave.entry("operators").or_default() += 1;
                    for (name, on) in [("tension", i(2104) != 0), ("skew", i(2108) != 0), ("sine shaper", i(2112) != 0), ("pre-filter", i(2116) != 128), ("sw 2120", raw[o + 2120] != 0), ("sw 2121", raw[o + 2121] != 0), ("sw 2122", raw[o + 2122] != 0), ("sw 2123", raw[o + 2123] != 0), ("sw 2124", raw[o + 2124] != 0), ("sw 2125", raw[o + 2125] != 0), ("sw 2126", raw[o + 2126] != 0), ("sw 2127", raw[o + 2127] != 0), ("harmonics", (1..256).any(|h| i(52 + h * 4) != 0)), ("shape", i(2100) != 0)] {
                        if on {
                            *wave.entry(name).or_default() += 1;
                        }
                    }
                }
            }
        }
    }
    for (name, n) in &wave {
        println!("{name}: {n}");
    }
    for ((slope, kind, lv), n) in seen {
        println!("slope {slope} kind {kind} level {lv}: {n}");
    }
}
