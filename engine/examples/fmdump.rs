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
            println!("art {k} on: env {} flags {} lfo {} flags {} bipolar {} switches at {} {}", a.env.live, a.env.flags, a.lfo.live, a.lfo.flags, a.bipolar, a.env.span.1.saturating_sub(16), a.lfo.span.1.saturating_sub(16));
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
    println!("velocity to volume {} mono set {}", pt.vel_vol, pt.uni_mono);
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
            if a.lfo.on() {
                println!("   lfo hz {} shape {:?} pts {:?}", a.lfo_hz, a.lfo_shape, a.lfo.pts.iter().map(|p| (p.x, p.y, p.tension)).collect::<Vec<_>>());
            }
        }
    }
    for k in [1usize, 61] {
        let e = &pt.arts[k].env;
        println!("art {k} flags {} decay {} loop {} sustain {} pts {:?}", e.flags, e.decay, e.loop_start, e.sustain, e.pts.iter().map(|p| (p.x, p.y, p.tension, p.mode)).collect::<Vec<_>>());
    }
    for (i, f) in pt.filts.iter().enumerate() {
        println!("filter {i} slope {} kind {} cut {} res {} env {} levels {:?} next {} ws {} feed {:?} out {}", f.slope, f.kind, f.cut, f.res, f.env_amt, f.levels, f.next, f.ws_on, pt.feed[i], pt.out[6 + i]);
        println!("   filter at {}", pt.filt_at[i]);
        if pt.filts[i].ws_on { let w = &pt.filts[i].ws; println!("   ws bipolar {} gain {} mix {} curve {:?}", pt.filts[i].ws_bipolar, pt.filts[i].ws_gain, pt.filts[i].ws_mix, [0usize, 4, 8, 16, 32, 64, 128, 192, 256].map(|j| w[j])); }
    }
    for (i, f) in pt.filts.iter().enumerate() {
        if f.ws_on {
            println!("shaper {i} gain {} mix {} bipolar {} curve {:?}", f.ws_gain, f.ws_mix, f.ws_bipolar, f.ws.iter().step_by(16).map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>());
        }
    }
    for (k, a) in pt.arts.iter().enumerate() {
        for (j, m) in a.maps.iter().enumerate() {
            let y0 = if a.bipolar { 0.5 } else { 1.0 };
            if m.pts.iter().any(|p| (p.y - y0).abs() > 1e-4) {
                println!("map art {k} axis {j} pts {:?}", m.pts.iter().map(|p| (p.x, p.y, p.tension)).collect::<Vec<_>>());
            }
        }
    }
    for (i, o) in pt.ops.iter().enumerate() {
        println!("op {i} ratio {} offset {} vol {} phase {} noise {} pluck {} fm {:?} rm {:?}", o.ratio, o.offset_hz, o.vol, o.phase, o.noise, o.pluck, pt.fm[i], pt.rm[i]);
        // the wave page of a current block: shape, tension, skew, sine shaper, pre-filter, then the switches, and the harmonics set
        if let Some(d) = cereal_engine::fm::Patch::unpack(raw).filter(|d| pt.op_at[i] + pt.lay.len <= d.len() && pt.op_at[i] > 0) {
            let l = pt.lay;
            let at = |o: usize| i32::from_le_bytes(d[pt.op_at[i] + o..pt.op_at[i] + o + 4].try_into().unwrap());
            let fl = |o: usize| f32::from_le_bytes(d[pt.op_at[i] + o..pt.op_at[i] + o + 4].try_into().unwrap());
            let mags: Vec<(usize, f32, f32)> = (0..l.harmonics).filter(|&k| fl(l.mags + k * 4) != 0.0).map(|k| (k + 1, fl(l.mags + k * 4), if l.len == 2128 { fl(1076 + k * 4) } else { 0.0 })).take(12).collect();
            println!("   wave {:?} switches {:?} harmonics {:?}", [at(l.tail), at(l.tail + 4), at(l.tail + 8), at(l.tail + 12), at(l.tail + 16)], &d[pt.op_at[i] + l.flags..pt.op_at[i] + l.len], mags);
            println!("   at {} len {}", pt.op_at[i], l.len);
        }
    }
}
