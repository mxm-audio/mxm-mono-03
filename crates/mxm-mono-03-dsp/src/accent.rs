//! Accent — three things at once, and one of them has memory.
//!
//! `research:filters/machines/tb303-diode-ladder.md` §4 measures the diode ladder and
//! concludes that the filter core is **not** where the machine's reputation comes
//! from, and its §5 ranks the accent and envelope structure first, calling it "a
//! voice, not a filter". This module is that ranking taken literally.
//!
//! # What accent does
//!
//! 1. **The note is louder.** The envelope voltage is added to the amplifier's
//!    control current through a short lag, so the boost arrives just after the
//!    attack rather than exactly with it.
//! 2. **The filter envelope's decay is forced to its minimum**, overriding the
//!    Decay control for that note only. So an accented note is a *different
//!    envelope*, not a louder one — see [`Accent::forces_short_decay`].
//! 3. **A separate sweep stage adds to the cutoff, and it holds charge between
//!    notes.**
//!
//! # (3) is the one that matters, and it is why this is a module
//!
//! The sweep capacitor does not fully discharge before the next step arrives, so
//! **consecutive accented notes climb**, each peaking higher than the last. That
//! climb is what people mean by acid.
//!
//! The architectural consequence: **accent is not a per-note gain.** Its output
//! depends on what the previous notes did, so the state has to live in the voice.
//! It cannot be expressed as a parameter offset applied per step, and nothing in
//! `mxm-mono-01` has this shape.
//!
//! # Resonance changes accent's character, not its depth
//!
//! On the hardware the sweep's pot **is the second section of the resonance pot**.
//! Turned down, the accent contribution reads as a bit of extra envelope
//! modulation; turned up, it becomes the slow rising sweep that does the climbing.
//! So "turn the resonance up" and "accents start swooping" are one gesture, and
//! modelling accent without it produces an accent that is merely louder.
//!
//! # Evidence
//!
//! All of it secondary — see the plan's source list and
//! `docs/modulation/`. No hardware was measured, here or in the sources. The
//! *structure* below is well attested across independent descriptions; the time
//! constants are chosen to produce the attested behaviour and are the first thing
//! a listening comparison should correct.

use crate::flush;

/// How fast the sweep capacitor charges while an accented note is sounding.
///
/// **Deliberately comparable to a note's own length, not much shorter than it.**
/// A fast charge saturates inside the first gate, and then every accent peaks at
/// the same place and nothing climbs — which is what an earlier value did, and
/// what `consecutive_accents_climb` caught. Partial charging per note is what
/// leaves room for the residual to accumulate.
pub const SWEEP_CHARGE_S: f32 = 0.150;

/// How fast it leaks when nothing is charging it.
///
/// **Slower than a sixteenth note at any dance tempo**, which is the whole point:
/// that is what leaves residual charge for the next accent to build on. At 140 BPM
/// a sixteenth is about 107 ms, comfortably inside this.
pub const SWEEP_DISCHARGE_S: f32 = 0.500;

/// The lag between the accent and the amplifier hearing about it.
pub const VCA_LAG_S: f32 = 0.0015;

/// Bounds on the sweep time a caller may ask for, as a multiple of the defaults.
///
/// Bounded here because a parameter reaching the DSP is user-generated input, and
/// because the interesting behaviour lives in a narrow band: far below this the
/// sweep saturates inside one note and stops climbing, far above it never
/// discharges and the cutoff simply parks at the ceiling.
pub const SWEEP_SCALE_MIN: f32 = 0.15;
pub const SWEEP_SCALE_MAX: f32 = 4.0;

/// Ceiling on accumulated sweep charge.
///
/// A real capacitor cannot charge past its supply, and without a ceiling a long
/// run of accents would walk the cutoff off the top of its range. Bounding it here
/// rather than at the cutoff keeps the limit where the physics is.
pub const SWEEP_CEILING: f32 = 1.6;

/// One-pole coefficient for an exponential reaching `1/e` in `seconds`.
#[inline]
fn coefficient(seconds: f32, sample_rate: f32) -> f32 {
    if seconds <= 0.0 || sample_rate <= 0.0 {
        0.0
    } else {
        (-1.0 / (seconds * sample_rate)).exp()
    }
}

/// The accent circuit's state.
#[derive(Debug, Clone, Default)]
pub struct Accent {
    /// Charge on the sweep capacitor. **Survives between notes** — this is the
    /// module's reason for existing.
    sweep: f32,
    /// The lagged amplifier boost.
    vca: f32,
    /// Whether the note currently sounding is accented, and how hard.
    gate: f32,
}

impl Accent {
    pub const fn new() -> Self {
        Self {
            sweep: 0.0,
            vca: 0.0,
            gate: 0.0,
        }
    }

    /// Clear everything, including the accumulated charge.
    ///
    /// `reset()` must leave no tail, and accumulated sweep charge is exactly the
    /// kind of tail that would otherwise survive a transport stop and make the
    /// first note of the next run louder than it should be.
    pub fn reset(&mut self) {
        self.sweep = 0.0;
        self.vca = 0.0;
        self.gate = 0.0;
    }

    /// Begin an accented note. `amount` is `0..=1`; zero is an ordinary note.
    ///
    /// **A slid-into note must not call this.** A slide holds the gate open, so
    /// there is no new note to accent — see [`crate::voice`], which is where that
    /// rule lives, and `docs/modulation/04-glide-and-portamento.md` §4.4, which
    /// states it: *"no new attack, no new filter sweep, no accent on the second
    /// one."*
    pub fn engage(&mut self, amount: f32) {
        self.gate = amount.clamp(0.0, 1.0);
    }

    /// End the accent gate. The sweep keeps its charge and leaks from here.
    pub fn release(&mut self) {
        self.gate = 0.0;
    }

    /// Whether the note sounding now overrides the Decay control.
    pub fn forces_short_decay(&self) -> bool {
        self.gate > 0.0
    }

    /// The accumulated sweep charge, for tests and telemetry.
    pub fn sweep(&self) -> f32 {
        self.sweep
    }

    /// Advance one sample.
    ///
    /// Charging happens while the accent gate is held; leaking happens always,
    /// which is what makes the charge outlast the note that put it there.
    ///
    /// `sweep_scale` stretches both time constants **together**, keeping their
    /// ratio. That ratio is what produces the climb — partial charge per note
    /// against a slower leak — so exposing the two separately would offer a
    /// thousand settings in which nothing climbs at all. One control, one
    /// behaviour, faster or slower.
    #[inline]
    pub fn process(&mut self, sweep_scale: f32, sample_rate: f32) {
        let scale = sweep_scale.clamp(SWEEP_SCALE_MIN, SWEEP_SCALE_MAX);
        if self.gate > 0.0 {
            let c = coefficient(SWEEP_CHARGE_S * scale, sample_rate);
            self.sweep += (self.gate - self.sweep) * (1.0 - c);
        }
        self.sweep *= coefficient(SWEEP_DISCHARGE_S * scale, sample_rate);
        self.sweep = flush(self.sweep.min(SWEEP_CEILING));

        let c = coefficient(VCA_LAG_S, sample_rate);
        self.vca = flush(self.gate + (self.vca - self.gate) * c);
    }

    /// How much louder this note is, `0..=1` before depth is applied.
    ///
    /// Follows the filter envelope, because on the hardware it is that envelope's
    /// voltage that reaches the amplifier.
    #[inline]
    pub fn vca_boost(&self, filter_env: f32) -> f32 {
        self.vca * filter_env
    }

    /// How much accent adds to the cutoff, in the same units as the filter
    /// envelope.
    ///
    /// **Resonance chooses between two characters rather than scaling one.** At
    /// low resonance the contribution follows the envelope — extra modulation on
    /// this note only, with no memory. At high resonance it is the accumulated
    /// sweep, which is slow, and climbs across consecutive accents.
    #[inline]
    pub fn cutoff_offset(&self, filter_env: f32, resonance: f32) -> f32 {
        let r = resonance.clamp(0.0, 1.0);
        let direct = self.gate * filter_env;
        direct * (1.0 - r) + self.sweep * r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    /// Play `count` accented notes at a sixteenth-note spacing and report the peak
    /// cutoff offset reached during each.
    fn accented_run(count: usize, bpm: f32, resonance: f32) -> Vec<f32> {
        let step = (60.0 / bpm / 4.0 * FS) as usize;
        let gate = step / 2;
        let mut a = Accent::new();
        let mut peaks = Vec::new();
        for _ in 0..count {
            a.engage(1.0);
            let mut peak = 0.0f32;
            for i in 0..step {
                if i == gate {
                    a.release();
                }
                a.process(1.0, FS);
                // A flat envelope isolates the sweep's own contribution.
                peak = peak.max(a.cutoff_offset(1.0, resonance));
            }
            peaks.push(peak);
        }
        peaks
    }

    #[test]
    fn consecutive_accents_climb() {
        // **The finding this module exists for**, and the oracle was checked by
        // sabotage rather than trusted: clearing `sweep` in `release` — so charge
        // does not survive the note — makes all four peaks identical
        // (0.2857 each, ratio exactly 1.0) and fails this on the second note. An
        // oracle that cannot fail is not an oracle.
        let peaks = accented_run(4, 140.0, 1.0);
        for pair in peaks.windows(2) {
            assert!(
                pair[1] > pair[0] * 1.05,
                "each accent should peak above the last: {peaks:?}"
            );
        }
        assert!(
            peaks[3] > peaks[0] * 1.3,
            "four accents should climb clearly: {peaks:?}"
        );
    }

    #[test]
    fn a_lone_accent_decays_away_before_the_next_note() {
        // The complement of the test above, and what stops the state being a latch:
        // leave a gap and the charge must be essentially gone.
        let mut a = Accent::new();
        a.engage(1.0);
        for _ in 0..(FS * 0.05) as usize {
            a.process(1.0, FS);
        }
        a.release();
        let peak = a.sweep();
        assert!(peak > 0.1, "should have charged, got {peak}");
        for _ in 0..(FS * 3.0) as usize {
            a.process(1.0, FS);
        }
        assert!(
            a.sweep() < peak * 0.01,
            "charge should have leaked away, {} remains",
            a.sweep()
        );
    }

    #[test]
    fn resonance_changes_the_accents_character_not_its_depth() {
        // At low resonance accent is extra envelope modulation on this note; at
        // high resonance it is the slow sweep that accumulates. The measurable
        // difference is that only the second one climbs.
        let low = accented_run(4, 140.0, 0.0);
        let high = accented_run(4, 140.0, 1.0);
        let low_climb = low[3] / low[0];
        let high_climb = high[3] / high[0];
        assert!(
            (low_climb - 1.0).abs() < 0.02,
            "at low resonance accents should not build: {low:?}"
        );
        assert!(
            high_climb > 1.3,
            "at high resonance they must: {high:?} (ratio {high_climb})"
        );
    }

    #[test]
    fn a_slower_sweep_climbs_more_gradually() {
        // The advanced control has to change the behaviour it names. Stretching both
        // constants keeps the climb and slows it, rather than removing it.
        let quick = accented_run(4, 140.0, 1.0);
        let slow = {
            let step = (60.0 / 140.0 / 4.0 * FS) as usize;
            let gate = step / 2;
            let mut a = Accent::new();
            let mut peaks = Vec::new();
            for _ in 0..4 {
                a.engage(1.0);
                let mut peak = 0.0f32;
                for i in 0..step {
                    if i == gate {
                        a.release();
                    }
                    a.process(3.0, FS);
                    peak = peak.max(a.cutoff_offset(1.0, 1.0));
                }
                peaks.push(peak);
            }
            peaks
        };
        assert!(
            slow[0] < quick[0],
            "a slower sweep should be further behind on the first note: {slow:?} vs {quick:?}"
        );
        assert!(slow[3] > slow[0] * 1.3, "and it must still climb: {slow:?}");
    }

    #[test]
    fn accent_forces_the_short_decay_only_while_it_is_engaged() {
        let mut a = Accent::new();
        assert!(!a.forces_short_decay());
        a.engage(1.0);
        assert!(a.forces_short_decay());
        a.release();
        assert!(!a.forces_short_decay());
    }

    #[test]
    fn an_unaccented_note_contributes_nothing() {
        // `engage(0.0)` is an ordinary note, not a quiet accent.
        let mut a = Accent::new();
        a.engage(0.0);
        for _ in 0..(FS * 0.2) as usize {
            a.process(1.0, FS);
        }
        assert_eq!(a.sweep(), 0.0);
        assert_eq!(a.cutoff_offset(1.0, 1.0), 0.0);
        assert_eq!(a.vca_boost(1.0), 0.0);
    }

    #[test]
    fn the_amplifier_boost_lags_the_accent() {
        // It arrives just after the attack rather than exactly with it, which is
        // the RC between the envelope and the amplifier's control current.
        let mut a = Accent::new();
        a.engage(1.0);
        a.process(1.0, FS);
        let immediate = a.vca_boost(1.0);
        for _ in 0..(FS * 0.01) as usize {
            a.process(1.0, FS);
        }
        let settled = a.vca_boost(1.0);
        assert!(
            immediate < 0.2 && settled > 0.9,
            "boost should ramp: {immediate} then {settled}"
        );
    }

    #[test]
    fn reset_clears_accumulated_charge() {
        // Otherwise a transport stop leaves the next run's first note pre-charged.
        let mut a = Accent::new();
        a.engage(1.0);
        for _ in 0..(FS * 0.1) as usize {
            a.process(1.0, FS);
        }
        a.reset();
        assert_eq!(a.sweep(), 0.0);
        assert_eq!(a.vca_boost(1.0), 0.0);
        assert!(!a.forces_short_decay());
    }

    #[test]
    fn a_long_run_of_accents_cannot_walk_off_the_top() {
        let peaks = accented_run(64, 160.0, 1.0);
        assert!(
            peaks.iter().all(|p| *p <= SWEEP_CEILING + 1e-6),
            "sweep exceeded its ceiling: max {:?}",
            peaks.iter().cloned().fold(0.0f32, f32::max)
        );
    }
}
