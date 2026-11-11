//! List the effect slots of a project and whether we have a unit for each.
fn main() {
    let path = std::env::args().nth(1).expect("project path");
    let mut lib = cereal_engine::Library::default();
    let (song, _) = cereal_engine::load_flp(std::path::Path::new(&path), &mut lib).expect("load");
    for (i, ins) in song.inserts.iter().enumerate() {
        for s in &ins.slots {
            println!("{i:3} {:2} {:28} on={} mix={:.2} {}", s.index, s.plugin, s.enabled, s.mix, match &s.fx { Some(f) => format!("{:?}", f.p), None => "(pass-through)".into() });
        }
    }
}
