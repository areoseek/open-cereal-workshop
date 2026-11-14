//! Level and pan curves, all in one place so they can be corrected against the reference in one edit.
//! These are measured approximations until checked value by value (docs/PARITY.md, "Level laws").

use std::f32::consts::FRAC_PI_4;

/// Channel volume knob: `v` is 0..=1 of the knob's travel (file value / 12800). 100 % is unity gain and the
/// default 78 % sits a little over 5 dB down. Measured against renders of two instruments at the default: the
/// same curve the sampler's velocity follows.
pub fn channel_gain(v: f32) -> f32 {
    (11f32.powf(v.clamp(0.0, 1.0)) - 1.0) / 10.0
}

/// Mixer fader: `v` is file value / 12800, so 1.0 is 0 dB and 1.25 the top of the fader (about +5.6 dB).
pub fn fader_gain(v: f32) -> f32 {
    v.clamp(0.0, 1.25).powf(2.9)
}

/// Note velocity: `v` is 0..=1.28 where 1.0 is the default velocity of 100. Measured on an instrument channel at
/// velocities 25, 50, 100 and 128 (0.082, 0.232, 1 and 2.05 times): the channel volume curve again.
pub fn velocity_gain(v: f32) -> f32 {
    (11f32.powf(v.clamp(0.0, 1.28)) - 1.0) / 10.0
}

/// Sampler note level for a velocity (`v` as in [`velocity_gain`]): measured against the reference at thirteen
/// velocities, within 0.1 dB. Velocity 100 at the centre plays the sample 3 dB down, 127 about 6 dB above that.
pub fn sampler_velocity(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.28);
    (11f32.powf(v) - 1.0) / 10.0 * std::f32::consts::FRAC_1_SQRT_2
}

/// The velocity that plays `gain` times as loud as velocity `v` on the curve above.
pub fn scale_velocity(v: f32, gain: f32) -> f32 {
    (1.0 + gain.max(0.0) * (11f32.powf(v.clamp(0.0, 1.28)) - 1.0)).log(11.0)
}

/// The levels adjustment volume of a channel (`v` is file value / 12800, 1.0 leaves the level alone): the
/// channel volume curve carried on past its top. Measured at 0.5 (0.2317 times).
pub fn adjust_gain(v: f32) -> f32 {
    (11f32.powf(v.clamp(0.0, 4.0)) - 1.0) / 10.0
}

/// Audio clip level from its channel volume: the default 78 % plays the recording as it is.
pub fn clip_gain(v: f32) -> f32 {
    channel_gain(v) / channel_gain(10000.0 / 12800.0)
}

/// The project's panning law: 0 circular, 2 triangular. One song plays at a time, so one switch serves.
static TRIANGULAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_pan_law(law: u8) {
    TRIANGULAR.store(law == 2, std::sync::atomic::Ordering::Relaxed);
}

/// Circular pan: centre is unity on both sides, hard left/right is +3 dB on one side and silence on the other.
/// Triangular pan (the older law): the side turned to stays at unity and the other falls in a straight line;
/// that reading of it is an estimate until measured.
pub fn pan(p: f32) -> (f32, f32) {
    if TRIANGULAR.load(std::sync::atomic::Ordering::Relaxed) {
        let p = p.clamp(-1.0, 1.0);
        return ((1.0 - p).min(1.0), (1.0 + p).min(1.0));
    }
    let a = (p.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (a.cos() * std::f32::consts::SQRT_2, a.sin() * std::f32::consts::SQRT_2)
}

/// Envelope time knob to seconds. The file holds 100 (shortest) to 65536. (Measured at nine settings of
/// the attack knob; the same curve gives the period of the channel's wobbles.)
pub fn env_seconds(raw: i32) -> f32 {
    if raw <= 100 {
        return 0.0;
    }
    knob_seconds((raw as f32 / 65536.0).min(1.0))
}

/// The time curve of the envelope and wobble knobs, from 0..=1 of the knob.
pub fn knob_seconds(x: f32) -> f32 {
    (0.063_57 * ((6.92 * x).exp() - 0.985)).max(0.0)
}

/// The knob position (0..=1) that gives a time.
pub fn seconds_knob(s: f32) -> f32 {
    ((s.max(0.0) / 0.063_57 + 0.985).ln() / 6.92).clamp(0.0, 1.0)
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
        // measured: velocity 100 at the centre is 3 dB down, 127 six above that, 64 nearly nine below
        let db = |v: f32| 20.0 * (sampler_velocity(v / 100.0) / sampler_velocity(1.0)).log10();
        assert!((20.0 * sampler_velocity(1.0).log10() + 3.01).abs() < 0.05);
        assert!((db(127.0) - 6.0).abs() < 0.2 && (db(64.0) + 8.8).abs() < 0.2 && (db(8.0) + 33.5).abs() < 0.3, "{} {} {}", db(127.0), db(64.0), db(8.0));
        assert!((clip_gain(10000.0 / 12800.0) - 1.0).abs() < 1e-6);
    }
}
