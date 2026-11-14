//! Print a filter's shaper line of an operator synth patch.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let n: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let k: usize = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&p), &mut lib).unwrap();
    let Some(g) = s.channels[n].gen.as_ref() else { return };
    let Some(pt) = cereal_engine::fm::Patch::parse(&g.raw[..]) else { return };
    println!("{:?}", pt.filts[k].ws);
}
