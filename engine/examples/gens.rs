//! Count the instrument kinds used across a folder of projects.
fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp")) {
            out.push(p);
        }
    }
}
fn main() {
    let dir = std::env::args().nth(1).expect("folder");
    let mut files = Vec::new();
    walk(std::path::Path::new(&dir), &mut files);
    let mut n = std::collections::BTreeMap::<String, usize>::new();
    for f in files {
        let mut lib = cereal_engine::Library::new();
        let Ok((s, _)) = cereal_engine::load_flp(&f, &mut lib) else { continue };
        for c in &s.channels {
            let k = if c.plugin.is_empty() { format!("{:?}", c.kind) } else { c.plugin.clone() };
            *n.entry(k).or_default() += 1;
        }
    }
    let mut v: Vec<_> = n.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, c) in v {
        println!("{c:6}  {k}");
    }
}
