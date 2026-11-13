//! Print the path scratcher phrases of one channel, for comparing against the original.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let n: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&p), &mut lib).unwrap();
    let Some(g) = &s.channels[n].gen else { return };
    let set = cereal_engine::travel::Set::parse(&g.raw);
    println!("shown {} phrases {}", set.shown, set.phrases.len());
    if set.phrases.is_empty() {
        println!("block of {} bytes starts {:?}", g.raw.len(), &g.raw[..g.raw.len().min(96)]);
        std::fs::write(std::env::var("DUMP").unwrap_or("/dev/null".into()), &g.raw).ok();
    }
    for (key, ph) in &set.phrases {
        println!("key {key} {:?} beats {} speed {} own {} attack {} release {} start {} end {} mutes {:?}", ph.name, ph.beats, ph.speed, ph.own_speed, ph.attack, ph.release, ph.start, ph.end, ph.mutes);
        for (what, line) in [("wave", &ph.wave), ("level", &ph.level)] {
            for q in line {
                println!("  {what} x {:.4} y {:.4} kind {} tension {:.3} before {:?} after {:?}", q.x, q.y, q.kind, q.tension, q.before, q.after);
            }
        }
    }
}
