//! A look inside one project, for working out how a unit is stored: run with
//! `PROBE_FILE=<project> cargo test probe_project -- --ignored --nocapture`.

#[test]
#[ignore]
fn probe_project() {
    let Ok(path) = std::env::var("PROBE_FILE") else { return };
    let bytes = std::fs::read(&path).unwrap();
    let flp = crate::flp::parse(&bytes).unwrap();
    let mut lib = crate::sample::Library::new();
    let song = crate::song::Song::from_flp_with(&flp, Some(&mut lib), std::path::Path::new(&path).parent());
    println!("clips {} patterns {}", song.clips.len(), song.patterns.len());
    for m in &song.markers {
        println!("marker at {} kind {} {:?}", m.pos, m.kind, m.name);
    }
    if song.performance {
        for (i, t) in song.tracks.iter().enumerate().filter(|(_, t)| t.perf != crate::song::Perf::default() || t.grouped) {
            println!("track {} {:?} grouped {}", i, t.perf, t.grouped);
        }
    }
    for c in song.clips.iter().take(std::env::var("PROBE_CLIPS").ok().and_then(|s| s.parse().ok()).unwrap_or(0)) {
        println!("clip at {} len {} track {} {:?} offset {} group {}", c.pos, c.len, c.track, c.src, c.offset, c.group);
    }
    println!("ppq {} tempo {:?}", song.ppq, song.tempo);
    for (i, c) in song.channels.iter().enumerate() {
        let g = c.gen.as_ref();
        println!("channel {i} {:?} plugin {:?} kind {:?} insert {:?} vol {:.3} pan {:.2} settings {:?}", c.name, c.plugin, c.kind, c.insert, c.volume, c.pan, g.map(|g| &g.p[..g.p.len().min(40)]));
        if std::env::var("PROBE_RAW").is_ok() {
            if let Some(g) = g {
                dump(&g.raw);
            }
        }
    }
    for (i, ins) in song.inserts.iter().enumerate() {
        if ins.volume != 1.0 || !ins.slots.is_empty() {
            println!("insert {i} vol {:.3} pan {:.2} sends {:?} levels {:?}", ins.volume, ins.pan, ins.sends, ins.sends.iter().map(|&d| ins.send_level(d)).collect::<Vec<_>>());
        }
        for s in &ins.slots {
            println!("insert {i} slot {} {:?} on {} mix {} sound {:?} settings {:?}", s.index, s.plugin, s.enabled, s.mix, s.fx.as_ref().and_then(|f| f.ir.as_ref()).map(|i| i.path.clone()), s.fx.as_ref().map(|f| &f.p[..f.p.len().min(40)]));
            if std::env::var("PROBE_RAW").is_ok() {
                if let Some(f) = &s.fx {
                    dump(&f.encode(f.raw.as_ref().map(|r| &r[..])));
                }
            }
        }
    }
    let mut seen = std::collections::BTreeMap::<(u16, u16), (usize, i32, i32, u32, u32)>::new();
    for p in &song.patterns {
        for e in &p.events {
            let x = seen.entry((e.param, e.dest)).or_insert((0, i32::MAX, i32::MIN, u32::MAX, 0));
            *x = (x.0 + 1, x.1.min(e.raw), x.2.max(e.raw), x.3.min(e.tick), x.4.max(e.tick));
        }
    }
    for (k, v) in seen {
        println!("events param {:#x} dest {:#x}: {} values {}..{} ticks {}..{}", k.0, k.1, v.0, v.1, v.2, v.3, v.4);
    }
    for (i, p) in song.patterns.iter().enumerate() {
        if !p.notes.is_empty() {
            println!("pattern {i}: {} notes, first {:?}", p.notes.len(), p.notes.iter().take(6).collect::<Vec<_>>());
        }
    }
}

fn dump(d: &[u8]) {
    let n = d.len();
    for (i, c) in d.chunks(32).enumerate() {
        if n > 1024 && i >= 8 && i < n / 32 - 8 {
            continue;
        }
        println!("   {:5} {}", i * 32, c.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }
}

#[test]
#[ignore]
fn probe_events() {
    let Ok(path) = std::env::var("PROBE_FILE") else { return };
    let bytes = std::fs::read(&path).unwrap();
    let f = crate::flp::FlpFile::parse(&bytes).unwrap();
    let from: usize = std::env::var("PROBE_FROM").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let count: usize = std::env::var("PROBE_COUNT").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let only: Option<u8> = std::env::var("PROBE_ID").ok().and_then(|s| s.parse().ok());
    for (i, e) in f.events.iter().enumerate().skip(from).take(count) {
        if only.is_some_and(|id| id != e.id) {
            continue;
        }
        match &e.value {
            crate::flp::Value::Data(d) => {
                let text: String = d.iter().take(60).map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
                println!("{i} id {} data {} {}", e.id, d.len(), text);
            }
            v => println!("{i} id {} {:?}", e.id, v),
        }
    }
}

#[test]
#[ignore]
fn probe_hex() {
    let Ok(path) = std::env::var("PROBE_FILE") else { return };
    let bytes = std::fs::read(&path).unwrap();
    let f = crate::flp::FlpFile::parse(&bytes).unwrap();
    let only: u8 = std::env::var("PROBE_ID").ok().and_then(|s| s.parse().ok()).unwrap_or(212);
    for (i, e) in f.events.iter().enumerate() {
        if let (true, crate::flp::Value::Data(d)) = (e.id == only, &e.value) {
            println!("{i} id {} len {}", e.id, d.len());
            dump(d);
        }
    }
}

/// One line per project in `PROBE_DIR`: what is in it, which units did not read, and whether a short
/// render makes sound.
#[test]
#[ignore]
fn probe_survey() {
    let Ok(dir) = std::env::var("PROBE_DIR") else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("flp"))).collect();
    files.sort();
    let mut lib = crate::sample::Library::new();
    for path in files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(flp) = crate::flp::parse(&bytes) else {
            println!("{name}: DOES NOT PARSE");
            continue;
        };
        let mut song = crate::song::Song::from_flp_with(&flp, Some(&mut lib), path.parent());
        let same = crate::save::save(&mut song, Some(&flp)).map(|b| b.0 == bytes).unwrap_or(false);
        let mut units = vec![];
        for c in &song.channels {
            if !c.plugin.is_empty() {
                units.push(format!("{}{}", c.plugin, if c.gen.is_some() { "" } else { " (UNREAD)" }));
            }
        }
        for ins in &song.inserts {
            for s in &ins.slots {
                units.push(format!("[{}{}]", s.plugin, if s.fx.is_some() { "" } else { " (UNREAD)" }));
            }
        }
        let notes: usize = song.patterns.iter().map(|p| p.notes.len()).sum();
        let song_mode = !song.clips.is_empty();
        let out = crate::render_song(&song, 44100.0, song_mode, 0, 0.5);
        let take = &out[..out.len().min(44100 * 2 * 8)];
        let peak = take.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let bad = take.iter().any(|v| !v.is_finite());
        println!("{name}: v{} same {same} notes {notes} len {:.1}s peak {peak:.3}{} | {}", flp.version, out.len() as f32 / 88200.0, if bad { " NAN" } else { "" }, units.join(", "));
    }
}
