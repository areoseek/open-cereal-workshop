//! Plain ZIP archives, enough for project packages: a project file with the sounds it uses beside it.
//! Written from the public ZIP format notes. Entries are stored or deflated; nothing else is read.

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    !data.iter().fold(!0u32, |c, &b| table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8))
}

/// Pack named files into one archive. Names use forward slashes.
pub fn write(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = vec![];
    let mut dir: Vec<u8> = vec![];
    for (name, data) in entries {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() { (8, &packed) } else { (0, data) };
        let crc = crc32(data);
        let at = out.len() as u32;
        let mut head: Vec<u8> = vec![];
        head.extend(20u16.to_le_bytes()); // version needed
        head.extend(0x0800u16.to_le_bytes()); // names are UTF-8
        head.extend(method.to_le_bytes());
        head.extend(0u16.to_le_bytes()); // time
        head.extend(0x21u16.to_le_bytes()); // date: 1 January 1980
        head.extend(crc.to_le_bytes());
        head.extend((body.len() as u32).to_le_bytes());
        head.extend((data.len() as u32).to_le_bytes());
        head.extend((name.len() as u16).to_le_bytes());
        head.extend(0u16.to_le_bytes()); // extra
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(&head);
        out.extend(name.as_bytes());
        out.extend(body);
        dir.extend(0x0201_4b50u32.to_le_bytes());
        dir.extend(20u16.to_le_bytes()); // made by
        dir.extend(&head);
        dir.extend([0u8; 10]); // comment length, disk, attributes
        dir.extend(at.to_le_bytes());
        dir.extend(name.as_bytes());
    }
    let at = out.len() as u32;
    out.extend(&dir);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// Unpack an archive into (name, bytes) pairs. Folders are skipped; names that would leave the folder they are
/// unpacked into are refused, as is anything that claims to unpack to more than `limit` bytes in all.
pub fn read(z: &[u8], limit: usize) -> Result<Vec<(String, Vec<u8>)>, String> {
    let u16at = |p: usize| z.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or("archive cut short");
    let u32at = |p: usize| z.get(p..p + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or("archive cut short");
    let end = (0..=z.len().saturating_sub(22)).rev().take(66_000).find(|&p| z[p..p + 4] == 0x0605_4b50u32.to_le_bytes()).ok_or("not a ZIP archive")?;
    let count = u16at(end + 10)?;
    let mut p = u32at(end + 16)?;
    let mut out = vec![];
    let mut total = 0usize;
    for _ in 0..count {
        if u32at(p)? != 0x0201_4b50 {
            return Err("archive list damaged".into());
        }
        let (flags, method, crc, packed, size) = (u16at(p + 8)?, u16at(p + 10)?, u32at(p + 16)?, u32at(p + 20)?, u32at(p + 24)?);
        let (nlen, xlen, clen, local) = (u16at(p + 28)?, u16at(p + 30)?, u16at(p + 32)?, u32at(p + 42)?);
        let name = String::from_utf8_lossy(z.get(p + 46..p + 46 + nlen).ok_or("archive cut short")?).replace('\\', "/");
        p += 46 + nlen + xlen + clen;
        if name.ends_with('/') {
            continue;
        }
        if flags & 1 != 0 {
            return Err(format!("{name} is locked with a password"));
        }
        if name.starts_with('/') || name.contains(':') || name.split('/').any(|part| part == ".." || part.is_empty()) {
            return Err(format!("unsafe name in the archive: {name}"));
        }
        total = total.saturating_add(size);
        if total > limit {
            return Err("the archive unpacks to more than is allowed".into());
        }
        if u32at(local)? != 0x0403_4b50 {
            return Err("archive entry damaged".into());
        }
        let start = local + 30 + u16at(local + 26)? + u16at(local + 28)?;
        let body = z.get(start..start + packed).ok_or("archive cut short")?;
        let data = match method {
            0 => body.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(body, size).map_err(|_| format!("{name} does not unpack"))?,
            m => return Err(format!("{name} is packed in a way that is not read here (method {m})")),
        };
        if data.len() != size || crc32(&data) as usize != crc {
            return Err(format!("{name} is damaged"));
        }
        out.push((name, data));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_goes_in_comes_out() {
        let files = vec![("song.flp".to_string(), vec![7u8; 5000]), ("sounds/kick.wav".to_string(), (0..=255u8).collect::<Vec<u8>>()), ("empty".to_string(), vec![])];
        let z = write(&files);
        assert!(z.len() < 5000, "the run of sevens packs down");
        if let Some(to) = std::env::var_os("CEREAL_ZIP_OUT") {
            std::fs::write(to, &z).unwrap();
        }
        assert_eq!(read(&z, 1 << 20).unwrap(), files);
        assert!(read(&z, 100).is_err(), "the size limit holds");
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn a_packed_project_opens_elsewhere_with_its_sound() {
        let dir = std::env::temp_dir().join(format!("cereal-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("far away")).unwrap();
        let wav = dir.join("far away/Tone.wav");
        let data: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        crate::wav::write_pcm16(std::fs::File::create(&wav).unwrap(), &data, 44_100, 1).unwrap();
        let mut song = crate::song::Song::default();
        let mut c = crate::song::Chan::default();
        c.sample_path = wav.to_string_lossy().to_string();
        song.channels.push(c);
        let (z, n) = crate::package(&mut song, None, "packed", &crate::sample::Library::default(), None).unwrap();
        assert_eq!(n, 1);
        std::fs::write(dir.join("packed.zip"), z).unwrap();
        std::fs::remove_file(&wav).unwrap();
        let flp = crate::unpack(&dir.join("packed.zip"), &dir.join("packed")).unwrap();
        let mut lib = crate::sample::Library::default();
        let (back, _) = crate::load_flp(&flp, &mut lib).unwrap();
        assert!(lib.missing.is_empty(), "{:?}", lib.missing);
        assert!(back.channels.iter().any(|c| c.sample.is_some()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_that_climb_out_are_refused() {
        let z = write(&[("../up.txt".to_string(), vec![1])]);
        assert!(read(&z, 1 << 20).is_err());
        let z = write(&[("/abs.txt".to_string(), vec![1])]);
        assert!(read(&z, 1 << 20).is_err());
    }
}
