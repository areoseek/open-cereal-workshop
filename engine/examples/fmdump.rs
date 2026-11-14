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
    for (k, a) in pt.arts.iter().enumerate() {
        if a.env.live || a.lfo.live {
            println!("art {k} on: env {} flags {} lfo {} flags {} bipolar {}", a.env.live, a.env.flags, a.lfo.live, a.lfo.flags, a.bipolar);
        }
    }
    println!("x {} y {}", pt.modx, pt.mody);
    if let Some(d) = cereal_engine::fm::Patch::unpack(raw) {
        let at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let row = |c: usize| (0..9).map(|r| at(12 + r * 36 + c * 4)).collect::<Vec<_>>();
        println!("pan {:?}\nsend {:?}\nout {:?}", row(6), row(7), row(8));
        println!("chorus on {} {:?}", d[975], (736..=752).step_by(4).map(at).collect::<Vec<_>>());
        for b in [978usize, 1001, 1024] {
            println!("echo fb {} time {} side {} vol {} mode {} tempo {} on {}", at(b), at(b + 4), at(b + 8), at(b + 12), at(b + 16), d[b + 20], d[b + 22]);
        }
        println!("room on {} vol {} secs {} cuts {} {}", d[1083], at(1079), at(1067), at(1071), at(1051));
    }
    println!("tail {:?}", pt.tail);
    println!("version word {:?}", &raw[..4.min(raw.len())]);
    if std::env::var("ARTS").is_ok() {
        for (k, a) in pt.arts.iter().enumerate() {
            let e = &a.env;
            for (m, c) in a.maps.iter().enumerate() {
                if c.pts.iter().any(|p| p.y != c.pts[0].y) || c.pts.first().is_some_and(|p| p.y != if a.bipolar { 0.5 } else { 1.0 }) {
                    println!("art {k} map {m} live {} flags {} pts {:?}", c.live, c.flags, c.pts.iter().map(|p| (p.x, p.y, p.tension)).collect::<Vec<_>>());
                }
            }
            println!("art {k} env flags {} decay {} loop {} sus {} pts {:?} | lfo flags {} n {}", e.flags, e.decay, e.loop_start, e.sustain, e.pts.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(), a.lfo.flags, a.lfo.pts.len());
        }
    }
    for k in [1usize, 61] {
        let e = &pt.arts[k].env;
        println!("art {k} flags {} decay {} loop {} sustain {} pts {:?}", e.flags, e.decay, e.loop_start, e.sustain, e.pts.iter().map(|p| (p.x, p.y, p.tension, p.mode)).collect::<Vec<_>>());
    }
    for (i, f) in pt.filts.iter().enumerate() {
        println!("filter {i} slope {} kind {} cut {} res {} env {} levels {:?} next {} ws {} feed {:?} out {}", f.slope, f.kind, f.cut, f.res, f.env_amt, f.levels, f.next, f.ws_on, pt.feed[i], pt.out[6 + i]);
    }
    for (i, f) in pt.filts.iter().enumerate() {
        if f.ws_on {
            println!("shaper {i} gain {} mix {} bipolar {} curve {:?}", f.ws_gain, f.ws_mix, f.ws_bipolar, f.ws.iter().step_by(16).map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>());
        }
    }
    for (i, o) in pt.ops.iter().enumerate() {
        println!("op {i} ratio {} offset {} vol {} phase {} noise {} pluck {} fm {:?} rm {:?}", o.ratio, o.offset_hz, o.vol, o.phase, o.noise, o.pluck, pt.fm[i], pt.rm[i]);
        // the wave page of a current block: shape, tension, skew, sine shaper, pre-filter, then the switches, and the harmonics set
        if let Some(d) = cereal_engine::fm::Patch::unpack(raw).filter(|d| pt.op_at[i] + 2128 <= d.len() && pt.op_at[i] > 0) {
            let at = |o: usize| i32::from_le_bytes(d[pt.op_at[i] + o..pt.op_at[i] + o + 4].try_into().unwrap());
            let fl = |o: usize| f32::from_le_bytes(d[pt.op_at[i] + o..pt.op_at[i] + o + 4].try_into().unwrap());
            let mags: Vec<(usize, f32, f32)> = (0..256).filter(|&k| fl(52 + k * 4) != 0.0).map(|k| (k + 1, fl(52 + k * 4), fl(1076 + k * 4))).take(12).collect();
            println!("   wave {:?} switches {:?} harmonics {:?}", [at(2100), at(2104), at(2108), at(2112), at(2116)], &d[pt.op_at[i] + 2120..pt.op_at[i] + 2128], mags);
        }
    }
}
