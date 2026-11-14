//! Everything the song keeps about one channel, without its sample or plug-in bytes: `chdump song.flp 12`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).expect("load");
    let c = &s.channels[a[2].parse::<usize>().expect("channel")];
    let text = format!("{c:#?}");
    for l in text.lines().filter(|l| l.len() < 160 && !l.trim_start().chars().next().is_some_and(|ch| ch.is_ascii_digit() || ch == '-')) {
        println!("{l}");
    }
}
