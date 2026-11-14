//! Write a copy of a project whose first pattern holds other notes for channel 0:
//! renote in.flp out.flp tempo key:pos:len[:velocity[:x:y]] ...   (ticks; tempo 0 keeps the tempo)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let tempo: f32 = a[3].parse().unwrap();
    if tempo > 0.0 {
        s.tempo = tempo as _;
    }
    let proto = s.patterns[0].notes[0].clone();
    s.patterns[0].notes = a[4..]
        .iter()
        .map(|x| {
            let v: Vec<u32> = x.split(':').map(|n| n.parse().unwrap()).collect();
            // the fifth and sixth fields are the note's two free controls as the file's byte (128 is the middle)
            let free = |k: usize, was: f32| v.get(k).map_or(was, |&x| (x as f32 - 128.0) / 128.0);
            cereal_engine::song::Note {
                key: v[0] as u8,
                pos: v[1],
                len: v[2],
                vel: v.get(3).map_or(proto.vel, |&x| x as f32 / 100.0),
                mod_x: free(4, proto.mod_x),
                mod_y: free(5, proto.mod_y),
                ..proto.clone()
            }
        })
        .collect();
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
