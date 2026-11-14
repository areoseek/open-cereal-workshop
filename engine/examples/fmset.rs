//! Write a copy of a project with whole numbers of one operator synth block changed:
//! fmset in.flp out.flp channel offset=value ...   (offsets into the unpacked block; F1+8 and O2+4 count from a filter or operator)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let n: usize = a[3].parse().unwrap();
    let g = s.channels[n].gen.as_mut().expect("an instrument");
    let was = cereal_engine::fm::Patch::parse(&g.raw).expect("an operator synth block");
    // F1+8 counts from the first filter's settings, O2+4 from the second operator's
    let place = |o: &str| -> usize {
        // E7 is the on switch of the seventh setting's envelope (counted from 0), L7 of its slow oscillator
        if let Some(k) = o.strip_prefix('E').and_then(|k| k.parse::<usize>().ok()) {
            return was.arts[k].env.span.1 - 16;
        }
        if let Some(k) = o.strip_prefix('L').and_then(|k| k.parse::<usize>().ok()) {
            return was.arts[k].lfo.span.1 - 16;
        }
        // K7.0 is the first of the four length knobs under the seventh setting's envelope
        if let Some((k, j)) = o.strip_prefix('K').and_then(|k| k.split_once('.')) {
            return was.arts[k.parse::<usize>().unwrap()].env.span.1 + 4 * j.parse::<usize>().unwrap();
        }
        match o.split_once('+') {
            Some((b, off)) => (if b.starts_with('F') { &was.filt_at[..] } else { &was.op_at[..] })[b[1..].parse::<usize>().unwrap() - 1] + off.parse::<usize>().unwrap(),
            None => o.parse().unwrap(),
        }
    };
    // P7=0/0,0.5/1,1/0.3 writes the points (x step / height) of the seventh setting's envelope and switches it on;
    // P7t does the same and marks its times as tempo-based
    let mut lines: Vec<cereal_engine::fm::Curve> = vec![];
    let mut plain: Vec<&String> = vec![];
    for x in &a[4..] {
        let (o, v) = x.split_once('=').unwrap();
        // M61.1=0/0,1/1 writes the second mapping (0 key, 1 velocity, ...) of setting 61
        if let Some((k, m)) = o.strip_prefix('M').and_then(|k| k.split_once('.')) {
            let mut c = was.arts[k.parse::<usize>().unwrap()].maps[m.parse::<usize>().unwrap()].clone();
            c.pts = v
                .split(',')
                .map(|p| p.split('/').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>())
                .map(|v| cereal_engine::fm::Pt { x: v[0], y: v[1], tension: v.get(2).copied().unwrap_or(0.0), ..Default::default() })
                .collect();
            c.live = true;
            lines.push(c);
            continue;
        }
        match o.strip_prefix('P') {
            Some(k) => {
                let tempo = k.ends_with('t');
                let mut c = was.arts[k.trim_end_matches('t').parse::<usize>().unwrap()].env.clone();
                // "0/0,1/1,1/0;d1;s2" also puts the decay marker on point 1 and the sustain marker on point 2
                let mut parts = v.split(';');
                let v = parts.next().unwrap();
                let marks: Vec<(char, i32)> = parts.map(|m| (m.chars().next().unwrap(), m[1..].parse().unwrap())).collect();
                c.pts = v
                    .split(',')
                    .map(|p| p.split('/').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>())
                    .map(|v| cereal_engine::fm::Pt { x: v[0], y: v[1], tension: v.get(2).copied().unwrap_or(0.0), ..Default::default() })
                    .collect();
                c.live = true;
                c.flags = (c.flags & !1) | tempo as i32;
                c.decay = -1;
                c.loop_start = -1;
                c.sustain = c.pts.len() as i32 - 1;
                for (m, at) in marks {
                    match m {
                        'd' => c.decay = at,
                        'l' => c.loop_start = at,
                        _ => c.sustain = at,
                    }
                }
                lines.push(c);
            }
            None => plain.push(x),
        }
    }
    lines.sort_by_key(|c| std::cmp::Reverse(c.span.0));
    // b1000=1 writes one byte instead of four
    let sets: Vec<(usize, i32, usize)> = plain
        .iter()
        .map(|x| x.split_once('=').unwrap())
        .map(|(o, v)| match o.strip_prefix('b') {
            Some(o) => (o.parse().unwrap(), v.parse().unwrap(), 1),
            None => (place(o), v.parse().unwrap(), 4),
        })
        .collect();
    let (raw, patch) = cereal_engine::fm::Patch::edited(&g.raw, |d| {
        for (o, v, wide) in &sets {
            d[*o..*o + wide].copy_from_slice(&v.to_le_bytes()[..*wide]);
        }
        for c in &lines {
            assert!(c.store(d), "the line goes back");
        }
    })
    .expect("a block that still reads");
    g.raw = raw;
    g.fm = Some(std::sync::Arc::new(patch));
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
