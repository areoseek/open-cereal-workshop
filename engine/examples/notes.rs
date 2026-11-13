//! Print every note of a project's patterns.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&p), &mut lib).unwrap();
    for (i, pat) in s.patterns.iter().enumerate() {
        for n in &pat.notes {
            println!("pat {i} pos {} len {} chan {} key {} vel {:.4} pan {:.3} fine {} rel {:.3} x {:.3} y {:.3} slide {} porta {}", n.pos, n.len, n.chan, n.key, n.vel, n.pan, n.fine_cents, n.release, n.mod_x, n.mod_y, n.slide, n.porta);
        }
    }
}
