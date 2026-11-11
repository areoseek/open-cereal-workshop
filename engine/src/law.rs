//! Level and pan curves, all in one place so they can be corrected against the reference in one edit.
//! These are measured approximations until checked value by value (docs/PARITY.md, "Level laws").

use std::f32::consts::FRAC_PI_4;

/// Channel volume knob: `v` is 0..=1 of the knob's travel (file value / 12800). 100 % is unity gain and the
/// default 78 % sits a little over 5 dB down.
pub fn channel_gain(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(2.4)
}

/// Mixer fader: `v` is file value / 12800, so 1.0 is 0 dB and 1.25 the top of the fader (about +5.6 dB).
pub fn fader_gain(v: f32) -> f32 {
    v.clamp(0.0, 1.25).powf(2.9)
}

/// Note velocity: `v` is 0..=1.28 where 1.0 is the default velocity of 100.
pub fn velocity_gain(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.28);
    v * v
}

/// Circular pan: centre is unity on both sides, hard left/right is +3 dB on one side and silence on the other.
pub fn pan(p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (a.cos() * std::f32::consts::SQRT_2, a.sin() * std::f32::consts::SQRT_2)
}

/// Envelope time knob to seconds. The file holds 100 (shortest) to 65536. (Approximate: see docs/PARITY.md.)
pub fn env_seconds(raw: i32) -> f32 {
    if raw <= 100 {
        return 0.0;
    }
    let x = (raw as f32 / 65536.0).min(1.0);
    16.0 * x * x * x
}

pub fn semitones(st: f32) -> f32 {
    (st / 12.0).exp2()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anchors() {
        assert!((channel_gain(1.0) - 1.0).abs() < 1e-6);
        let db = 20.0 * channel_gain(0.78).log10();
        assert!((-5.6..=-4.8).contains(&db), "{db}");
        assert!((fader_gain(1.0) - 1.0).abs() < 1e-6);
        let (l, r) = pan(0.0);
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let (l, r) = pan(-1.0);
        assert!(l > 1.4 && r.abs() < 1e-5);
        assert!((semitones(12.0) - 2.0).abs() < 1e-6);
    }
}
