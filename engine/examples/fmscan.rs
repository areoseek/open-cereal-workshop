//! Tally the operator synth channels of the projects in a list file: block version, unison, filters, effects.
fn main() {
    let list = std::fs::read_to_string(std::env::args().nth(1).expect("list file")).unwrap();
    for p in list.lines() {
        let mut lib = cereal_engine::Library::new();
        let Ok((s, _)) = cereal_engine::load_flp(std::path::Path::new(p), &mut lib) else { continue };
        for c in &s.channels {
            if let Some(g) = &c.gen {
                if format!("{:?}", g.kind) == "Fm" {
                    let Some(pt) = cereal_engine::fm::Patch::parse(&g.raw) else { println!("ver {} UNREAD", g.raw.first().copied().unwrap_or(0)); continue };
                    let f: Vec<String> = pt.filts.iter().enumerate().filter(|(i, _)| pt.feed[*i].iter().any(|&x| x != 0.0) || pt.out[6 + *i] != 0.0).map(|(_, f)| format!("{}/{}{}", f.slope, f.kind, if f.ws_on { "w" } else { "" })).collect();
                    for a in &pt.arts {
                        for (what, c) in [("env", &a.env), ("lfo", &a.lfo)].into_iter().chain(a.maps.iter().map(|m| ("map", m))) {
                            for q in &c.pts { if q.mode != 0 { println!("M {what} {}", q.mode); } }
                        }
                    }
                    let t = &pt.tail;
                    for (i, o) in pt.ops.iter().enumerate() { if pt.used()[i] && o.noise > 0.0 { println!("noise"); } if pt.used()[i] && o.pluck { println!("pluck"); } }
                    for x in &f { println!("F {x}"); }
                    println!("ver {} uni {} filt {} fx c{} d{} r{} send {} ops {}", g.raw[0], pt.uni, f.join(","), t.chorus.is_some() as u8, t.delay.is_some() as u8, t.reverb.is_some() as u8, pt.send.iter().any(|&x| x != 0.0) as u8, pt.used().iter().filter(|&&u| u).count());
                }
            }
        }
    }
}
