//! List the stored blocks of one instrument kind across a list of projects: genraws list.txt identifier [out folder]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut n = 0;
    for p in std::fs::read_to_string(&a[1]).unwrap().lines() {
        let mut lib = cereal_engine::Library::new();
        let Ok((s, _)) = cereal_engine::load_flp(std::path::Path::new(p), &mut lib) else { continue };
        for c in &s.channels {
            if c.plugin.eq_ignore_ascii_case(&a[2]) {
                if let Some(g) = &c.gen {
                    let w = |i: usize| g.raw.get(i..i + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).unwrap_or(-1);
                    println!("{n} len {} ver {} tail {:?}", g.raw.len(), w(0), &g.raw[g.raw.len().saturating_sub(13).max(424.min(g.raw.len()))..]);
                    if let Some(d) = a.get(3) { std::fs::write(format!("{d}/{n}.bin"), &g.raw).unwrap(); }
                    n += 1;
                }
            }
        }
    }
}
