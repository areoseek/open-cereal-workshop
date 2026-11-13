//! Print the main numbers of an operator synth patch held by a project's channel.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let n: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&p), &mut lib).unwrap();
    let c = &s.channels[n];
    let Some(g) = c.gen.as_ref() else { return println!("no state") };
    let raw = &g.raw[..];
    let Some(pt) = cereal_engine::fm::Patch::parse(raw) else { return println!("not read, {} bytes", raw.len()) };
    println!("volume {} pitch {} out {:?} uni {}", pt.volume, pt.pitch, pt.out, pt.uni);
    for k in [1usize, 61] {
        let e = &pt.arts[k].env;
        println!("art {k} flags {} decay {} loop {} sustain {} pts {:?}", e.flags, e.decay, e.loop_start, e.sustain, e.pts.iter().map(|p| (p.x, p.y, p.tension, p.mode)).collect::<Vec<_>>());
    }
    for (i, o) in pt.ops.iter().enumerate() {
        println!("op {i} ratio {} offset {} vol {} phase {} fm {:?}", o.ratio, o.offset_hz, o.vol, o.phase, pt.fm[i]);
    }
}
