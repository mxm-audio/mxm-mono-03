//! DSP for mxm-mono-03.
//!
//! Deliberately free of any plugin-framework types: everything here takes plain
//! values and a sample rate, so the whole voice is testable with `cargo test` and
//! no host involved.
//!
//! # What this instrument is, and where its sound comes from
//!
//! An acid bass voice. `research:filters/machines/tb303-diode-ladder.md` §4 measures
//! the diode ladder and concludes that **the filter core is not the sound** — at
//! equal output it is only 1.2x dirtier than a transistor ladder. Its §5 ranks
//! "the envelope, accent and drive structure around the filter" first.
//!
//! So the interesting module here is [`accent`], not [`filter`]. Accent is not a
//! per-note gain: its sweep stage holds charge between notes, so consecutive
//! accented notes climb. That state is why it is a module rather than a
//! multiplier.

pub mod accent;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod envelope;
pub mod filter;
pub mod oscillator;
pub mod routing;
pub mod voice;

/// The lowest host rate the plugin activates at; a non-finite rate is refused with it.
///
/// `f32::clamp` panics when its lower bound is above its upper one or either is NaN, and the
/// ladder's nominal cutoff is clamped to `20 Hz ..= 0.45 × rate ÷ 3.532`, its widest pole, which
/// crosses below 157 Hz. 1 kHz is far clear of that, and no higher than the lowest rate
/// clap-validator (1234.57 Hz) or the player's robustness sweeps (1 kHz) ask for.
pub const MIN_SAMPLE_RATE: f32 = 1_000.0;

/// Flush a recursive state toward zero before it can become denormal.
///
/// Denormal arithmetic can cost orders of magnitude more than normal arithmetic,
/// which in a feedback filter shows up as a CPU spike exactly when a note decays
/// into silence. We do this in the DSP rather than relying on a framework FTZ
/// guard: the guard may be a no-op unless an opt-in feature is enabled, and
/// flushing here is also what keeps digital silence *exactly* zero.
///
/// `1e-20` is far above the f32 denormal threshold (~1.18e-38) and about -400 dB,
/// so nothing audible is lost.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_preserves_audible_values_and_kills_tiny_ones() {
        assert_eq!(flush(0.0), 0.0);
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(0.5), 0.5);
        assert_eq!(flush(-1e-6), -1e-6);
    }
}
