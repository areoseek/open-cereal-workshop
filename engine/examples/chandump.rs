//! Print one channel's loaded settings, for comparing against the original.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let n: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&p), &mut lib).unwrap();
    let mut c = s.channels[n].clone();
    if let Some(g) = &c.gen {
        println!("gen {:?} {:?}", g.kind, g.p);
    }
    c.gen = None;
    let frames = c.sample.as_ref().map(|x| x.frames.len());
    c.sample = None;
    c.raw = None;
    println!("sample frames {frames:?}");
    let t = format!("{:?}", c);
    println!("{}", &t[..t.len().min(3500)]);
}
