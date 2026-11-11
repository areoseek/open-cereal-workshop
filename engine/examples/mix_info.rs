//! Print the mixer of a project: each used insert with its level, where it sends and what sits in its slots.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&p), &mut lib).unwrap();
    for (i, c) in s.channels.iter().enumerate() {
        println!("channel {i} \"{}\" -> insert {}", c.name, c.insert);
    }
    for (i, ins) in s.inserts.iter().enumerate() {
        if ins.slots.is_empty() && ins.name.is_empty() && !s.channels.iter().any(|c| c.insert as usize == i) {
            continue;
        }
        let sends: Vec<String> = ins.sends.iter().map(|&d| format!("{d}@{:.2}", ins.send_level(d))).collect();
        println!("insert {i} \"{}\" vol {:.2} on {} sends [{}]", ins.name, ins.volume, ins.enabled, sends.join(" "));
        for sl in &ins.slots {
            println!("    slot {} {} on {} mix {:.2} {}", sl.index, sl.plugin, sl.enabled, sl.mix, if sl.fx.is_some() { "" } else { "(passes through)" });
        }
    }
}
