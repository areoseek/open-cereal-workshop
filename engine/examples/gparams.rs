//! Print a span of one instrument's settings for every use of it across a folder: gparams <folder> <name> <from> <to>
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
    let a: Vec<String> = std::env::args().collect();
    let (from, to): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    let mut files = Vec::new();
    walk(std::path::Path::new(&a[1]), &mut files);
    for f in files {
        let mut lib = cereal_engine::Library::new();
        let Ok((s, _)) = cereal_engine::load_flp(&f, &mut lib) else { continue };
        for c in s.channels.iter().filter(|c| c.plugin == a[2]) {
            if let Some(g) = c.gen.as_ref() {
                println!("{:?}", &g.p[from.min(g.p.len())..to.min(g.p.len())]);
            }
        }
    }
}
