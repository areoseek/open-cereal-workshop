//! Drum patches kept as text (`.ds`): sections of `key=value` lines describing a swept tone, filtered noise,
//! two overtone oscillators, two bands of noise, a filter and a bit crusher, each with its own envelope of
//! `time,level` points. The layout and the way a patch is turned into sound follow the openly published
//! mda DrumSynth by Paul Kellett (MIT licence, see docs/THIRD-PARTY.md); the wave is made at 44.1 kHz.

use std::collections::HashMap;
use std::f32::consts::TAU;

const FS: f32 = 44100.0;
const BLOCK: usize = 1200;

struct Ini(HashMap<(String, String), String>);

impl Ini {
    fn parse(text: &str) -> Self {
        let mut m = HashMap::new();
        let mut sec = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(s) = line.strip_prefix('[') {
                sec = s.trim_end_matches(']').trim().to_lowercase();
            } else if let Some((k, v)) = line.split_once('=') {
                m.entry((sec.clone(), k.trim().to_lowercase())).or_insert_with(|| v.trim().to_string());
            }
        }
        Ini(m)
    }
    fn text(&self, sec: &str, key: &str) -> Option<&str> {
        self.0.get(&(sec.to_lowercase(), key.to_lowercase())).map(|s| s.as_str())
    }
    fn num(&self, sec: &str, key: &str, def: f32) -> f32 {
        self.text(sec, key).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap_or(def)
    }
    fn whole(&self, sec: &str, key: &str, def: i32) -> i32 {
        self.num(sec, key, def as f32) as i32
    }
}

/// An envelope of straight lines between points, stepped one frame at a time.
#[derive(Default, Clone)]
struct Env {
    pts: Vec<(f32, f32)>,
    at: usize,
    next: f32,
    v: f32,
    dv: f32,
    max: f32,
}

impl Env {
    fn read(ini: &Ini, sec: &str, key: &str, stretch: f32) -> Self {
        let mut pts: Vec<(f32, f32)> = ini
            .text(sec, key)
            .unwrap_or("0,0 100,0")
            .split_whitespace()
            .filter_map(|p| p.split_once(','))
            .filter_map(|(t, l)| Some((t.parse::<f32>().ok()? * stretch, l.parse::<f32>().ok()? * 0.01)))
            .take(32)
            .collect();
        if pts.is_empty() {
            pts = vec![(0.0, 0.0), (100.0 * stretch, 0.0)];
        }
        let max = pts.last().map(|p| p.0).unwrap_or(0.0);
        Env { pts, at: 0, next: 0.0, v: 0.0, dv: 0.0, max }
    }
    fn peak(&self) -> f32 {
        self.pts.iter().fold(0f32, |a, p| a.max(p.1))
    }
    fn step(&mut self, t: usize) -> f32 {
        if (t as f32) < self.next {
            self.v += self.dv;
        } else if self.at + 1 < self.pts.len() {
            let (a, b) = (self.pts[self.at], self.pts[self.at + 1]);
            self.next = b.0;
            self.v = a.1;
            self.dv = (b.1 - a.1) / (b.0 - a.0 + 1.0);
            self.at += 1;
        } else {
            self.v = 0.0;
            self.dv = 0.0;
            self.next = f32::MAX;
        }
        self.v
    }
}

fn wave(phi: f32, kind: i32) -> f32 {
    match kind {
        1 => (2.0 * (0.5 * phi).rem_euclid(TAU).sin()).abs() - 1.0,
        2 => 0.636_619_7 * phi.rem_euclid(TAU).cos().acos() - 1.0,
        3 => 0.318_309_9 * phi.rem_euclid(TAU) - 1.0,
        4 => {
            if phi.rem_euclid(TAU).sin() > 0.0 {
                1.0
            } else {
                -1.0
            }
        }
        _ => phi.rem_euclid(TAU).sin(),
    }
}

/// Whether the bytes look like a drum patch of this kind.
pub fn is_patch(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).to_lowercase();
    head.contains("[general]") && head.contains("drumsynth")
}

/// Turn a patch into its wave: mono frames from -1 to 1 at 44.1 kHz.
pub fn render(bytes: &[u8]) -> Option<Vec<f32>> {
    if !is_patch(bytes) {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let ini = Ini::parse(&text);
    let on = |sec: &str| ini.whole(sec, "On", 0) != 0;
    let stretch = (0.01 * ini.num("General", "Stretch", 100.0)).clamp(0.2, 10.0);
    let mut gain = 10f32.powf(0.05 * ini.num("General", "Level", 0.0));
    let tune = 1.059_463_1f32.powf(ini.num("General", "Tuning", 0.0));
    let mut main_filter = 2 * ini.whole("General", "Filter", 0);
    let res = (0.0101 * ini.num("General", "Resonance", 0.0)).max(0.0).sqrt();
    let high_pass = ini.whole("General", "HighPass", 0) as f32;
    let mut e_filter = Env::read(&ini, "General", "FilterEnv", stretch);

    // noise
    let mut n_on = on("Noise");
    let nl = (ini.whole("Noise", "Level", 0) as f32).powi(2);
    let nt = ini.whole("Noise", "Slope", 0) as f32;
    let mut e_noise = Env::read(&ini, "Noise", "Envelope", stretch);
    let (na, nb, nc, nd, ng) = if nt < 0.0 { (1.0 + nt / 105.0, 0.0, 0.0, -nt / 105.0, (1.0 + 0.0005 * nt * nt) * nl) } else { (1.0, -nt / 50.0, nt.abs() / 100.0, 0.0, nl) };

    // tone
    let mut t_on = on("Tone");
    let tone_was_on = t_on;
    let tl = (ini.whole("Tone", "Level", 128) as f32).powi(2);
    let mut e_tone = Env::read(&ini, "Tone", "Envelope", stretch);
    let mut f1 = tune * TAU * ini.num("Tone", "F1", 200.0) / FS;
    if f1.abs() < 0.001 {
        f1 = 0.001;
    }
    let mut f2 = tune * TAU * ini.num("Tone", "F2", 120.0) / FS;
    let mut droop = ini.num("Tone", "Droop", 0.0);
    let drooping = droop > 0.0;
    let ddf;
    if drooping {
        droop = 10f32.powf((droop - 20.0) / 30.0) * -4.0 / e_tone.max.max(1.0);
        f2 = f1 + (f2 - f1) / (1.0 - (droop * e_tone.max).exp());
        ddf = f1 - f2;
    } else {
        ddf = f2 - f1;
    }
    let mut tphi = ini.num("Tone", "Phase", 90.0) / 57.295_78;

    // overtones
    let o_on = on("Overtones");
    let ol = (ini.whole("Overtones", "Level", 128) as f32).powi(2);
    let mut e_o1 = Env::read(&ini, "Overtones", "Envelope1", stretch);
    let mut e_o2 = Env::read(&ini, "Overtones", "Envelope2", stretch);
    let o_mode = ini.whole("Overtones", "Method", 2);
    let mut of1 = tune * TAU * ini.num("Overtones", "F1", 200.0) / FS;
    let mut of2 = tune * TAU * ini.num("Overtones", "F2", 120.0) / FS;
    let (ow1, ow2) = (ini.whole("Overtones", "Wave1", 0), ini.whole("Overtones", "Wave2", 0));
    let param = ini.whole("Overtones", "Param", 50) as f32;
    let drive = param.powi(3) / 125_000.0;
    let bal2 = param * 0.01;
    let bal1 = 1.0 - bal2;
    let (mut ophi1, mut ophi2) = (tphi, tphi);
    if main_filter == 0 {
        main_filter = ini.whole("Overtones", "Filter", 0);
    }
    let track1 = ini.whole("Overtones", "Track1", 0) == 1 && tone_was_on;
    let track2 = ini.whole("Overtones", "Track2", 0) == 1 && tone_was_on;
    if track1 {
        of1 /= f1;
    }
    if track2 {
        of2 /= f1;
    }
    // the metal mode: six pulse trains through a band pass that the second envelope opens
    let mut oc_a = 0.28 + bal1 * bal1;
    let oc_q = oc_a * oc_a;
    let oc_f = (1.8 - 0.7 * oc_q) * 0.92;
    oc_a *= 1.0 + 4.0 * bal1;
    let mut oc: [[f32; 2]; 6] = [[0.0; 2]; 6];
    for (j, hz) in [205.3f32, 304.4, 369.6, 522.7, 540.0, 800.0].iter().enumerate() {
        oc[j][1] = FS / (hz * tune * (of1 * FS / TAU / 200.0).clamp(0.25, 8.0) * 2.0);
        oc[j][0] = oc[j][1] * (j as f32 * 0.37).fract();
    }
    let (mut oc0, mut oc1, mut oc2) = (0f32, 0f32, 0f32);

    // bands of noise
    let band = |sec: &str| {
        let l = (ini.whole(sec, "Level", 128) as f32).powi(2);
        let f = tune * TAU * ini.num(sec, "F", 1000.0) / FS;
        let df = ini.whole(sec, "dF", 50) as f32;
        let q = df * df / (10000.0 - 6600.0 * (f.max(0.0).sqrt() - 0.19));
        let step = 1 + ((40.0 - df / 2.5) / (q + 1.0 + f)) as i32;
        (on(sec), l, f, q, step.max(1) as usize, Env::read(&ini, sec, "Envelope", stretch))
    };
    let (mut b_on, bl, bf, bq, b_step, mut e_b) = band("NoiseBand");
    let (mut b2_on, bl2, bf2, bq2, b2_step, mut e_b2) = band("NoiseBand2");
    let (mut bphi, mut bphi2, mut bdf, mut bdf2) = (TAU / 8.0, TAU / 8.0, 0f32, 0f32);

    // the longest envelope in use sets the length
    let mut length = 0f32;
    let mut loudest = 0f32;
    for (used, e, l) in [(t_on, &e_tone, tl), (n_on, &e_noise, nl), (o_on, &e_o1, ol), (o_on, &e_o2, 0.0), (b_on, &e_b, bl), (b2_on, &e_b2, bl2)] {
        if used {
            length = length.max(e.max);
            loudest += e.peak() * l;
        }
    }
    let length = (BLOCK * (1 + length as usize / BLOCK)).min(FS as usize * 60);

    // crusher
    let d_on = on("Distortion");
    let mut d_step = 1 + ini.whole("Distortion", "Rate", 0).max(0) as usize;
    d_step = match d_step {
        7 => 20,
        6 => 10,
        5 => 8,
        s => s.min(20),
    };
    let mut clip = 32700f32;
    let mut atten = 1f32;
    if d_on {
        clip = (gain * loudest).clamp(1.0, 32700.0);
        atten = 2f32.powf(2.0 * ini.whole("Distortion", "Bits", 0) as f32);
        gain = atten * gain * 10f32.powf(0.05 * ini.whole("Distortion", "Clipping", 0) as f32);
    }

    let mut seed = 1u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        ((seed >> 16) & 0x7fff) as f32 / 32767.0
    };
    let (mut x0, mut x1, mut tt) = (0f32, 0f32, 0f32);
    let (mut mf_in, mut mf_out) = (0f32, 0f32);
    let mut out = Vec::with_capacity(length);
    let mut df = [0f32; BLOCK];
    let mut phi = [0f32; BLOCK];
    let mut tpos = 0usize;
    while tpos < length {
        if n_on {
            for (k, d) in df.iter_mut().enumerate() {
                let e = e_noise.step(tpos + k);
                let x2 = x1;
                x1 = x0;
                x0 = 2.0 * rnd() - 1.0;
                tt = na * x0 + nb * x1 + nc * x2 + nd * tt;
                *d = tt * ng * e;
            }
            if (tpos + BLOCK) as f32 >= e_noise.max {
                n_on = false;
            }
        } else {
            df = [0.0; BLOCK];
        }
        let mut tphi_start = tphi;
        if t_on {
            for (k, p) in phi.iter_mut().enumerate() {
                let t = (tpos + k) as f32;
                *p = if drooping { f2 + ddf * (t * droop).exp() } else { f1 + t / e_tone.max.max(1.0) * ddf };
            }
            for k in 0..BLOCK {
                let e = e_tone.step(tpos + k);
                tphi = (tphi + phi[k]).rem_euclid(TAU * 4096.0);
                df[k] += tl * e * tphi.rem_euclid(TAU).sin();
            }
            if (tpos + BLOCK) as f32 >= e_tone.max {
                t_on = false;
            }
        } else {
            phi = [f2; BLOCK];
        }
        if b_on {
            for (k, d) in df.iter_mut().enumerate() {
                let e = e_b.step(tpos + k);
                if (tpos + k) % b_step == 0 {
                    bdf = rnd() - 0.5;
                }
                bphi = (bphi + bf + bq * bdf).rem_euclid(TAU);
                *d += bphi.cos() * e * bl;
            }
            if (tpos + BLOCK) as f32 >= e_b.max {
                b_on = false;
            }
        }
        if b2_on {
            for (k, d) in df.iter_mut().enumerate() {
                let e = e_b2.step(tpos + k);
                if (tpos + k) % b2_step == 0 {
                    bdf2 = rnd() - 0.5;
                }
                bphi2 = (bphi2 + bf2 + bq2 * bdf2).rem_euclid(TAU);
                *d += bphi2.cos() * e * bl2;
            }
            if (tpos + BLOCK) as f32 >= e_b2.max {
                b2_on = false;
            }
        }
        for k in 0..BLOCK {
            let t = tpos + k;
            let mut ot = 0f32;
            if o_on {
                let e1 = if t as f32 >= e_o1.max { 0.0 } else { e_o1.step(t) };
                let e2 = if t as f32 >= e_o2.max { 0.0 } else { e_o2.step(t) };
                tphi_start += phi[k];
                ophi1 = if track1 { tphi_start * of1 } else { (ophi1 + of1).rem_euclid(TAU * 4096.0) };
                ophi2 = if track2 { tphi_start * of2 } else { (ophi2 + of2).rem_euclid(TAU * 4096.0) };
                ot = match o_mode {
                    0 => ol * (bal1 * e1 * wave(ophi1, ow1) + bal2 * e2 * wave(ophi2, ow2)),
                    1 => {
                        let m = drive * e2 * wave(ophi2, ow2);
                        ol * e1 * wave(ophi1 + m, ow1)
                    }
                    2 => {
                        let m = (1.0 - drive / 8.0) + (drive / 8.0) * e2 * wave(ophi2, ow2);
                        ol * e1 * wave(ophi1, ow1) * m
                    }
                    _ => {
                        let mut hit = 0f32;
                        for o in oc.iter_mut() {
                            o[0] += 1.0;
                            if o[0] > o[1] {
                                o[0] -= o[1];
                                hit = ol * e1 * oc_a;
                            }
                        }
                        let f = (e2 * oc_f).min(1.9);
                        oc0 += f * oc1;
                        oc1 += f * (hit + oc2 - oc_q * oc1 - oc0);
                        oc2 = hit;
                        oc1
                    }
                };
            }
            if main_filter == 1 || main_filter == 2 {
                let m = e_filter.step(t);
                let fb = if m > 0.2 { 1.001 - 10f32.powf(m - 1.0) } else { 0.999 - 0.7824 * m };
                let x = if main_filter == 1 { ot } else { df[k] + ot };
                let v = x + res * (1.0 + 1.0 / fb) * (mf_in - mf_out);
                mf_in = fb * (mf_in - v) + v;
                mf_out = fb * (mf_out - mf_in) + mf_in;
                df[k] = if main_filter == 1 { df[k] + mf_out - high_pass * ot } else { mf_out - high_pass * x };
            } else {
                df[k] += ot;
            }
        }
        if d_on {
            for d in df.iter_mut() {
                *d = gain * (*d / atten).trunc();
            }
            let mut j = 0;
            while j < BLOCK {
                let end = (j + d_step).min(BLOCK);
                let ave = df[j..end].iter().sum::<f32>() / d_step as f32;
                df[j..end].fill(ave);
                j += d_step;
            }
        } else {
            for d in df.iter_mut() {
                *d *= gain;
            }
        }
        out.extend(df.iter().map(|d| if d.is_finite() { d.clamp(-clip, clip) / 32768.0 } else { 0.0 }));
        tpos += BLOCK;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "[General]\nVersion=DrumSynth v1.0\nTuning=0.00\nStretch=100.0\nFilter=0\nHighPass=0\nFilterEnv=0,100 442000,100 442000,0\n\n[Tone]\nOn=1\nLevel=149\nF1=375\nF2=120\nDroop=11\nPhase=90\nEnvelope=0,100 4410,0\n\n[Noise]\nOn=1\nLevel=100\nSlope=36\nEnvelope=0,100 2205,0\n\n[NoiseBand]\nOn=1\nLevel=80\nF=845\ndF=31\nEnvelope=0,100 1000,0\n";

    #[test]
    fn a_patch_becomes_a_short_decaying_hit() {
        let w = render(PATCH.as_bytes()).unwrap();
        assert_eq!(w.len(), 4800);
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        assert!(rms(&w[..1000]) > 0.1, "{}", rms(&w[..1000]));
        assert!(rms(&w[4400..]) < 0.001);
        assert!(w.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        assert!(render(b"RIFF....WAVE").is_none());
    }

    #[test]
    fn stretch_makes_it_longer_and_every_overtone_mode_stays_finite() {
        let long = render(PATCH.replace("Stretch=100.0", "Stretch=200.0").as_bytes()).unwrap();
        assert_eq!(long.len(), 9600);
        for mode in 0..4 {
            let p = format!("{PATCH}\n[Overtones]\nOn=1\nLevel=120\nF1=630\nWave1=3\nTrack1=1\nF2=1200\nWave2=4\nTrack2=0\nFilter=1\nMethod={mode}\nParam=70\nEnvelope1=0,100 3000,0\nEnvelope2=0,100 2000,0\n\n[Distortion]\nOn=1\nClipping=6\nBits=2\nRate=3\n");
            let w = render(p.as_bytes()).unwrap();
            assert!(w.iter().all(|v| v.is_finite() && v.abs() <= 1.0), "mode {mode}");
            assert!(w.iter().any(|v| v.abs() > 0.01), "mode {mode}");
        }
    }
}
