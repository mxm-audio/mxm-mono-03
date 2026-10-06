//! The two envelopes, and why only one of them has a control.
//!
//! Both are attack-decay with **no sustain**. That is not a simplification to be
//! improved on later: it is why every note from this instrument has the same
//! shape, and it is most of what makes a line sound like one continuous
//! instrument rather than like a synthesizer being played.
//!
//! | | Attack | Decay | Controlled by |
//! |---|---|---|---|
//! | [`FilterEnvelope`] | fixed, fast | the Decay control — **or forced short by accent** | Decay, and Env Mod for depth |
//! | [`AmpEnvelope`] | fixed, fast | fixed, and long | **nothing** |
//!
//! # The filter envelope decays whether or not the gate is held
//!
//! It is attack-decay, not attack-sustain-release: once it has peaked it falls,
//! and holding a note longer does not hold the filter open. A long note is a note
//! whose filter has already closed.
//!
//! # The amplitude envelope's decay is long enough to be invisible
//!
//! Long compared with a sixteenth note at any dance tempo, so within a step the
//! level is essentially flat. Note-off closes it quickly — that fast release, not
//! the decay, is what ends a note.
//!
//! # Accent shortens the filter envelope rather than raising it
//!
//! On an accented note the Decay control is bypassed and the envelope runs at its
//! minimum time. So **an accented note is a different envelope, not a louder
//! one** — see [`crate::accent`], which owns the rest of that behaviour.

use crate::flush;

/// Attack time for both envelopes. Fast enough to read as instant, slow enough
/// not to click.
///
/// **A de-clicker rather than a shaped attack**, which is what the hardware's is:
/// secondary sources put it at a few milliseconds. It is the *default* — the two
/// attacks are reachable behind the plugin's advanced disclosure, so that this
/// stays the machine's behaviour without being the only behaviour.
pub const ATTACK_S: f32 = 0.003;

/// Bounds on any attack the caller may ask for.
///
/// Bounded **here** rather than only in the editor: a parameter reaching the DSP
/// is user-generated input, and a zero attack would click while an unbounded one
/// would swallow a sixteenth note whole.
pub const ATTACK_MIN_S: f32 = 0.0005;
pub const ATTACK_MAX_S: f32 = 0.500;

/// The shortest filter-envelope decay, which is also what accent forces.
pub const DECAY_MIN_S: f32 = 0.200;

/// The longest filter-envelope decay the Decay control reaches.
pub const DECAY_MAX_S: f32 = 2.000;

/// The amplitude envelope's decay. Fixed, long, and deliberately not a control.
pub const AMP_DECAY_S: f32 = 3.0;

/// The amplitude envelope's release once the gate closes.
pub const AMP_RELEASE_S: f32 = 0.010;

/// Level below which an envelope is treated as finished.
///
/// About -100 dB. Exponentials never reach zero, and a voice that never reports
/// itself idle would keep a host's tail open forever.
pub const EPSILON: f32 = 1e-5;

/// One-pole coefficient for an exponential reaching `1/e` in `seconds`.
#[inline]
fn coefficient(seconds: f32, sample_rate: f32) -> f32 {
    if seconds <= 0.0 || sample_rate <= 0.0 {
        0.0
    } else {
        (-1.0 / (seconds * sample_rate)).exp()
    }
}

/// Which part of its life an envelope is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Attack,
    Decay,
    Release,
}

/// An attack-decay envelope with no sustain.
#[derive(Debug, Clone)]
pub struct AttackDecay {
    level: f32,
    stage: Stage,
}

impl Default for AttackDecay {
    fn default() -> Self {
        Self::new()
    }
}

impl AttackDecay {
    pub const fn new() -> Self {
        Self {
            level: 0.0,
            stage: Stage::Idle,
        }
    }

    pub fn reset(&mut self) {
        self.level = 0.0;
        self.stage = Stage::Idle;
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Start a new note.
    ///
    /// **The level is not reset to zero.** An envelope retriggered before it has
    /// decayed away starts its attack from wherever it is, which is what the
    /// analog circuit does — and it is the reason a fast run of notes builds
    /// rather than restarting cleanly.
    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    /// Close the gate. Only the amplitude envelope uses this.
    pub fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// One sample.
    ///
    /// `attack_s` shapes the rise; `decay_s` the fall; `release_s` the release, for
    /// the envelope that has one. A filter envelope passes the same value for decay
    /// and release, because it has no gate behaviour of its own.
    #[inline]
    pub fn process(
        &mut self,
        attack_s: f32,
        decay_s: f32,
        release_s: f32,
        sample_rate: f32,
    ) -> f32 {
        match self.stage {
            Stage::Idle => return 0.0,
            Stage::Attack => {
                let c = coefficient(attack_s.clamp(ATTACK_MIN_S, ATTACK_MAX_S), sample_rate);
                // Approach 1 from below; switch to decay once essentially there.
                self.level = 1.0 + (self.level - 1.0) * c;
                if self.level >= 1.0 - 1e-3 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level *= coefficient(decay_s, sample_rate);
                if self.level < EPSILON {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
            Stage::Release => {
                self.level *= coefficient(release_s, sample_rate);
                if self.level < EPSILON {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level = flush(self.level);
        self.level
    }
}

/// Map the Decay control `0..=1` onto a time.
///
/// Exponential, so the control's travel is even in perceived time rather than
/// spending most of itself at the long end.
pub fn decay_time_s(control: f32) -> f32 {
    let control = control.clamp(0.0, 1.0);
    DECAY_MIN_S * (DECAY_MAX_S / DECAY_MIN_S).powf(control)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn an_attack_decay_envelope_falls_while_the_gate_is_still_held() {
        // The defining difference from an ADSR: there is no sustain to hold. A
        // long note is a note whose filter has already closed.
        let mut e = AttackDecay::new();
        e.trigger();
        for _ in 0..(FS * 0.05) as usize {
            e.process(ATTACK_S, DECAY_MIN_S, DECAY_MIN_S, FS);
        }
        let early = e.level();
        for _ in 0..(FS * 0.30) as usize {
            e.process(ATTACK_S, DECAY_MIN_S, DECAY_MIN_S, FS);
        }
        let late = e.level();
        assert!(
            late < early * 0.5,
            "decayed from {early} to {late} with the gate never released"
        );
    }

    #[test]
    fn the_decay_control_spans_the_published_range() {
        assert!((decay_time_s(0.0) - DECAY_MIN_S).abs() < 1e-6);
        assert!((decay_time_s(1.0) - DECAY_MAX_S).abs() < 1e-6);
        let mid = decay_time_s(0.5);
        assert!(
            mid > DECAY_MIN_S && mid < DECAY_MAX_S,
            "midpoint {mid} out of range"
        );
    }

    #[test]
    fn retriggering_before_decay_starts_from_where_it_was() {
        // The analog behaviour: a fast run builds instead of restarting cleanly.
        let mut e = AttackDecay::new();
        e.trigger();
        for _ in 0..(FS * 0.10) as usize {
            e.process(ATTACK_S, DECAY_MAX_S, DECAY_MAX_S, FS);
        }
        let before = e.level();
        assert!(before > 0.1, "should still be sounding, got {before}");
        e.trigger();
        // One sample after retrigger it must be climbing from `before`, not from 0.
        let after = e.process(ATTACK_S, DECAY_MAX_S, DECAY_MAX_S, FS);
        assert!(
            after > before,
            "retrigger should climb from {before}, got {after}"
        );
    }

    #[test]
    fn an_envelope_reaches_exactly_zero_and_reports_idle() {
        // Exponentials never arrive; a voice that never goes idle keeps a host's
        // tail open forever.
        let mut e = AttackDecay::new();
        e.trigger();
        for _ in 0..(FS * 20.0) as usize {
            e.process(ATTACK_S, DECAY_MIN_S, AMP_RELEASE_S, FS);
        }
        assert_eq!(e.level(), 0.0);
        assert_eq!(e.stage(), Stage::Idle);
        assert!(!e.is_active());
    }

    #[test]
    fn release_ends_a_note_far_faster_than_the_amp_decay() {
        // The amp envelope's decay is long enough to be invisible within a step,
        // so it is the release that actually ends a note.
        let mut held = AttackDecay::new();
        let mut released = AttackDecay::new();
        held.trigger();
        released.trigger();
        for _ in 0..(FS * 0.02) as usize {
            held.process(ATTACK_S, AMP_DECAY_S, AMP_RELEASE_S, FS);
            released.process(ATTACK_S, AMP_DECAY_S, AMP_RELEASE_S, FS);
        }
        released.release();
        for _ in 0..(FS * 0.05) as usize {
            held.process(ATTACK_S, AMP_DECAY_S, AMP_RELEASE_S, FS);
            released.process(ATTACK_S, AMP_DECAY_S, AMP_RELEASE_S, FS);
        }
        assert!(
            held.level() > 0.9,
            "a held note should still be near full, got {}",
            held.level()
        );
        assert!(
            released.level() < 0.01,
            "a released note should be gone, got {}",
            released.level()
        );
    }

    #[test]
    fn a_longer_attack_actually_takes_longer() {
        // The advanced control has to do something, and "it compiles" is not that.
        let mut fast = AttackDecay::new();
        let mut slow = AttackDecay::new();
        fast.trigger();
        slow.trigger();
        for _ in 0..(FS * 0.01) as usize {
            fast.process(ATTACK_S, AMP_DECAY_S, AMP_RELEASE_S, FS);
            slow.process(0.2, AMP_DECAY_S, AMP_RELEASE_S, FS);
        }
        assert!(
            fast.level() > slow.level() * 2.0,
            "a 3 ms attack should be far ahead of a 200 ms one: {} vs {}",
            fast.level(),
            slow.level()
        );
    }

    #[test]
    fn an_attack_is_bounded_in_the_dsp() {
        // A parameter reaching here is user-generated input. Zero would click and
        // something enormous would swallow the note.
        let mut e = AttackDecay::new();
        e.trigger();
        for _ in 0..(FS * 0.05) as usize {
            let y = e.process(0.0, AMP_DECAY_S, AMP_RELEASE_S, FS);
            assert!(
                y.is_finite() && (0.0..=1.0).contains(&y),
                "out of bounds: {y}"
            );
        }
        assert!(e.level() > 0.9, "a zero attack must still arrive");
    }

    #[test]
    fn reset_leaves_no_tail() {
        let mut e = AttackDecay::new();
        e.trigger();
        for _ in 0..1000 {
            e.process(ATTACK_S, DECAY_MAX_S, AMP_RELEASE_S, FS);
        }
        e.reset();
        assert_eq!(e.process(ATTACK_S, DECAY_MAX_S, AMP_RELEASE_S, FS), 0.0);
        assert_eq!(e.stage(), Stage::Idle);
    }
}
