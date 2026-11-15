//! Which parts of the filter bank unit a set of projects really uses: one line per bank that is switched on.
use cereal_engine::fxbank::{BANKS, HEAD, LISTED, PER};
fn main() {
    let mut lib = cereal_engine::Library::default();
    for path in std::env::args().skip(1) {
        let Ok((song, _)) = cereal_engine::load_flp(std::path::Path::new(&path), &mut lib) else { continue };
        for (i, ins) in song.inserts.iter().enumerate() {
            for s in &ins.slots {
                let Some(f) = &s.fx else { continue };
                if f.kind != cereal_engine::fx::Kind::Bank || f.p.len() <= LISTED {
                    continue;
                }
                let tables = (f.p.len() - LISTED) / BANKS;
                let art = (tables - 257) / 8;
                for b in 0..BANKS {
                    let q = &f.p[HEAD + b * PER..HEAD + (b + 1) * PER];
                    if q[14] < 0.5 {
                        continue;
                    }
                    let t = &f.p[LISTED + b * tables..LISTED + (b + 1) * tables];
                    let ws_line = t[..257].iter().enumerate().all(|(k, v)| (v - k as f32 / 256.0).abs() < 0.01);
                    let mut used = String::new();
                    for a in 0..8 {
                        let r = &t[257 + a * art..257 + (a + 1) * art];
                        let flat = |s: &[f32]| s.iter().all(|v| (v - s[0]).abs() < 1e-3);
                        let maps = &r[4 + 257..];
                        let mut u = String::new();
                        if r[0] > 0.0 { u += &format!("pat{} ", r[0]); }
                        let block = f.encode(None);
                        for pt in 0..11 {
                            let k = b * 113 + 1 + a * 14 + pt;
                            if let (Some(c), Some(t)) = (f.line(k), cereal_engine::fxbank::after(&block, k)) {
                                if c.pts.len() > 1 || c.flags != 1 || (pt < 10 && t != [128, 128, 0, 128]) || (pt == 10 && (t != [40000, 0, 0, 0] || c.pts[0].y < 0.999)) {
                                    if std::env::var("BANK_HEX").is_ok() && c.pts.len() < 4 {
                                        let raw = cereal_engine::fxbank::raw_line(&block, k).unwrap();
                                        u += &format!("[{}] ", raw.iter().map(|b| format!("{b:02x}")).collect::<String>());
                                    }
                                    u += &format!("<{pt}: n{} len{:.3} flags{} live{} tail{:?} {:?} y0 {:.3}> ", c.pts.len(), c.pts.iter().map(|p| p.x).sum::<f32>(), c.flags, c.live, [c.decay, c.loop_start, c.sustain], t, c.pts[0].y);
                                }
                            }
                        }
                        if r[1].abs() > 1e-4 { u += &format!("wave(d{:.2} s{} t{}) ", r[1], r[2], r[3]); }
                        for (m, nm) in ["x", "y", "f"].iter().enumerate() {
                            let s = &maps[m * 65..(m + 1) * 65];
                            if !flat(s) { u += &format!("{nm}[{:.2}..{:.2}] ", s[0], s[64]); } else if (s[0] - if matches!(a, 0 | 2 | 3) { 0.5 } else { 1.0 }).abs() > 1e-3 { u += &format!("{nm}={:.2} ", s[0]); }
                        }
                        if !u.is_empty() { used += &format!("| {}: {}", ["pan", "vol", "cut", "res", "low", "band", "high", "wsmix"][a], u); }
                    }
                    println!("{} ins{i} bank{} head {:?} q {:?} ws_line={} {}", std::path::Path::new(&path).file_name().unwrap().to_string_lossy(), b + 1, &f.p[..4], q, ws_line, used);
                }
            }
        }
    }
}
