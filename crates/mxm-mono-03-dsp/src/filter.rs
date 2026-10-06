//! The diode ladder — 4-pole resonant lowpass, unbuffered rungs.
//!
//! Topology-preserving transform (TPT) one-poles in the style of Zavalishin's
//! *The Art of VA Filter Design*, with a saturating resonance feedback path solved
//! per sample by Newton iteration — the same machinery `mxm-mono-01` uses, with
//! **one change that is the whole of the difference**.
//!
//! # The pole set is the model
//!
//! In a transistor ladder each rung is buffered from the next, so all four poles
//! sit at the same frequency. In a diode ladder they are not: every stage loads
//! the one above it, the poles interact, and they spread out. Stinchcombe derives
//! the positions, and `research:filters/machines/tb303-diode-ladder.md` §2 records
//! them — **that spread is the entire linear character of this filter**, and it is
//! reproduced here by giving each one-pole its own cutoff rather than by modelling
//! a single diode.
//!
//! No diode equations, and no inter-stage coupling constant to tune. An earlier
//! sketch in `docs/filters/02-topologies.md` §2.5 used a hand-tuned coupling
//! constant and was labelled behavioural; the pole set has no free parameters and
//! reproduces two published numbers, so it supersedes it.
//!
//! # Two consequences that are easy to get wrong
//!
//! **It needs `k` around 18, not 4.** The threshold is over four times a
//! transistor ladder's, so a resonance scale shared with `mxm-mono-01` would put
//! the singing point in completely the wrong place.
//!
//! **It does not sing at its cutoff.** Self-oscillation lands at 1.1955x the
//! nominal cutoff — a minor third sharp. Anything treating this filter as a tone
//! source must ask [`DiodeConfig::oscillation_ratio`] rather than assume 1.0.
//!
//! # Why the nominal cutoff is clamped by the widest pole
//!
//! The poles spread up to 3.532x the nominal cutoff, and it is the *widest* one
//! that has to stay below Nyquist. Clamping each stage independently would keep
//! the filter stable while silently changing the ratios between the poles — which
//! is to say, silently turning it into a different filter as the cutoff rises. So
//! the nominal cutoff is clamped instead, by the widest pole's headroom, and the
//! ratios hold exactly at every setting. See [`max_nominal_cutoff_hz`].
//!
//! That costs top end, and `research:filters/machines/tb303-diode-ladder.md` §10.5
//! is where the cost was worked out. It is also why this instrument's cutoff
//! control shows a position rather than a frequency.

use crate::{Rng, flush};
use std::f32::consts::PI;

/// Newton steps used to solve the resonance feedback each sample.
///
/// Three is enough because the equation is strictly monotonic — see
/// [`DiodeLadder::process`].
const NEWTON_ITERATIONS: usize = 3;

/// How far past its own threshold the top of the resonance control reaches.
///
/// The same 1.15x-ish margin `mxm-mono-01` uses over its own much smaller
/// threshold, so the top of the control is meaningfully past self-oscillation
/// without being absurd.
pub const RESONANCE_MARGIN: f32 = 1.145;

/// The largest `k` any configuration can be asked for, which is the TB-303's since
/// it has the highest threshold.
///
/// Used only to state [`OUTPUT_BOUND`]: the bound has to hold for whichever
/// configuration is selected, so it is taken from the worst case rather than from
/// whatever is loaded.
pub const K_MAX: f32 = 21.0;

/// Above this resonance the filter is excited so self-oscillation can start from
/// silence. See [`DiodeLadder::process`].
pub const EXCITATION_THRESHOLD: f32 = 0.9;

/// Amplitude of that excitation. About -120 dB: inaudible against any real signal,
/// but enough to seed oscillation.
pub const EXCITATION_LEVEL: f32 = 1e-6;

/// Lowest nominal cutoff, in Hz.
pub const CUTOFF_MIN_HZ: f32 = 20.0;

/// The highest fraction of the sample rate any single pole may sit at.
pub const NYQUIST_FRACTION: f32 = 0.45;

/// Output bound, and the argument that establishes it.
///
/// The only path back into the loop is through [`tanh_approx`], whose magnitude
/// never exceeds 1, so the injected feedback is bounded by `k` regardless of
/// internal state and the ladder input satisfies `|x| <= 1 + k`. Each TPT one-pole
/// has unity DC gain, so that bounds the steady state; the extra margin covers
/// transient overshoot while coefficients are moving, and is **measured** by
/// `output_stays_within_the_stated_bound_under_overdrive`.
pub const OUTPUT_BOUND: f32 = 1.0 + K_MAX + 4.0;

/// Which diode ladder configuration to model.
///
/// Same code, different constants — the configurations differ only in where their
/// poles sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiodeConfig {
    /// One diode at the top of the ladder and the bottom capacitor halved, which
    /// is Stinchcombe's identification of the TB-303's configuration.
    #[default]
    Tb303,
    /// The three-diode configuration, which oscillates at a much lower `k`.
    ///
    /// **These pole positions are fitted, not derived** — see
    /// `research:filters/machines/tb303-diode-ladder.md` §8. They reproduce the
    /// published threshold and should not be trusted for the corner shape.
    Ems,
}

impl DiodeConfig {
    /// Pole positions in units of the nominal cutoff, at `k = 0`.
    pub const fn poles(self) -> [f32; 4] {
        match self {
            DiodeConfig::Tb303 => [3.532, 2.347, 1.000, 0.121],
            DiodeConfig::Ems => [3.0, 2.1, 1.0, 0.23],
        }
    }

    /// The widest pole, which is what bounds the usable cutoff range.
    pub fn widest_pole(self) -> f32 {
        let p = self.poles();
        let mut w = p[0];
        let mut i = 1;
        while i < 4 {
            if p[i] > w {
                w = p[i];
            }
            i += 1;
        }
        w
    }

    /// Loop gain at which this configuration oscillates, in closed form.
    ///
    /// Setting `s = jw` in the closed-loop denominator and separating real and
    /// imaginary parts gives the oscillation point directly, with `e_n` the
    /// elementary symmetric polynomials of the poles:
    ///
    /// ```text
    ///     w^2 = e3 / e1                    (imaginary part = 0)
    ///     k   = (e2*w^2 - w^4)/e4 - 1      (real part = 0)
    /// ```
    ///
    /// This is arithmetic on Stinchcombe's pole set, and it reproduces his
    /// separately-derived 18.4 and 9.8.
    pub fn threshold(self) -> f32 {
        let p = self.poles();
        let (e1, e2, e3, e4) = symmetric(p);
        let w2 = e3 / e1;
        (e2 * w2 - w2 * w2) / e4 - 1.0
    }

    /// Frequency of self-oscillation, relative to the nominal cutoff.
    ///
    /// 1.1955 for the TB-303 configuration — **+309 cents, a minor third sharp**.
    pub fn oscillation_ratio(self) -> f32 {
        let p = self.poles();
        let (e1, _, e3, _) = symmetric(p);
        (e3 / e1).sqrt()
    }
}

/// Elementary symmetric polynomials of four poles.
fn symmetric(p: [f32; 4]) -> (f32, f32, f32, f32) {
    let e1 = p[0] + p[1] + p[2] + p[3];
    let e2 = p[0] * p[1] + p[0] * p[2] + p[0] * p[3] + p[1] * p[2] + p[1] * p[3] + p[2] * p[3];
    let e3 = p[0] * p[1] * p[2] + p[0] * p[1] * p[3] + p[0] * p[2] * p[3] + p[1] * p[2] * p[3];
    let e4 = p[0] * p[1] * p[2] * p[3];
    (e1, e2, e3, e4)
}

/// The highest nominal cutoff at which every pole stays below the Nyquist limit.
///
/// Clamping here rather than per stage is what keeps the pole *ratios* exact — see
/// the module documentation.
pub fn max_nominal_cutoff_hz(config: DiodeConfig, sample_rate: f32) -> f32 {
    NYQUIST_FRACTION * sample_rate / config.widest_pole()
}

/// `tan` for the prewarp, in `f64` because `tan` near `pi/2` loses significance
/// fast and four prewarps per sample compound it.
#[inline]
fn tan_approx(x: f64) -> f64 {
    x.tan()
}

/// Bounded, monotonic `tanh` approximation — a [7/6] Pade form.
///
/// The input clamp is load-bearing, not defensive: without it the rational form
/// diverges for large `x`, which would break the boundedness argument the whole
/// filter design rests on. `tanh(4) = 0.9993`, so the curve is essentially flat
/// where the clamp takes over.
///
/// The output clamp guards the last ulp: the invariant `|tanh_approx(x)| <= 1` is
/// what bounds the filter, so it should be exactly true in `f32`, not nearly true.
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let num = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let den = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    (num / den).clamp(-1.0, 1.0)
}

/// A 4-pole diode ladder lowpass.
#[derive(Debug, Clone)]
pub struct DiodeLadder {
    config: DiodeConfig,
    /// One integrator state per pole.
    s: [f32; 4],
    /// Previous output, used to start the Newton solve.
    y_prev: f32,
    /// Deterministic excitation source for self-oscillation.
    rng: Rng,
}

impl Default for DiodeLadder {
    fn default() -> Self {
        Self::new(DiodeConfig::Tb303)
    }
}

impl DiodeLadder {
    pub const fn new(config: DiodeConfig) -> Self {
        Self {
            config,
            s: [0.0; 4],
            y_prev: 0.0,
            // Fixed seed: excitation must be bit-repeatable so it can be tested.
            rng: Rng::new(0x5EED_0303),
        }
    }

    pub fn config(&self) -> DiodeConfig {
        self.config
    }

    /// Switch configuration.
    ///
    /// A **control-rate** operation: the pole set changes, so this belongs at a block
    /// boundary and never mid-sample. The integrator states are deliberately kept —
    /// clearing them would silence the filter on every switch, and the states are
    /// bounded whatever the poles are, so carrying them across is safe and sounds
    /// like turning a knob rather than like a glitch.
    pub fn set_config(&mut self, config: DiodeConfig) {
        self.config = config;
    }

    /// Clear all state. Leaves no tail from previous playback.
    pub fn reset(&mut self) {
        self.s = [0.0; 4];
        self.y_prev = 0.0;
        self.rng = Rng::new(0x5EED_0303);
    }

    /// Map a normalised resonance `0..=1` onto **this configuration's own** `k`.
    ///
    /// `docs/filters/09-voicing.md` §9.1: a resonance scale shared across filter
    /// families puts the singing point somewhere different on each one, so the knob
    /// is normalised against the family's own threshold rather than against a
    /// constant.
    ///
    /// **This was a constant once, and the bug is worth keeping written down.** With
    /// only the TB-303 configuration reachable it looked harmless — but the moment
    /// the EMS set could be selected, the same knob position sat at 63% of threshold
    /// on one and comfortably past it on the other, because their thresholds are
    /// 18.34 and 10.08. Switching model would have changed how resonant the filter
    /// was rather than what kind of filter it was.
    #[inline]
    pub fn native_resonance(&self, resonance: f32) -> f32 {
        self.config.threshold() * RESONANCE_MARGIN * resonance.clamp(0.0, 1.0)
    }

    /// Where this filter would sing, for a given nominal cutoff.
    #[inline]
    pub fn oscillation_hz(&self, cutoff_hz: f32) -> f32 {
        cutoff_hz * self.config.oscillation_ratio()
    }

    /// Process one sample.
    ///
    /// `resonance` is `0..=1`. Coefficients are recomputed every sample so that
    /// per-sample envelope and accent modulation of the cutoff actually takes
    /// effect — which for this instrument is most of the point.
    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32, sample_rate: f32) -> f32 {
        let fc = cutoff_hz.clamp(
            CUTOFF_MIN_HZ,
            max_nominal_cutoff_hz(self.config, sample_rate),
        );
        let poles = self.config.poles();

        // Per-stage prewarp. This is the only structural difference from a
        // transistor ladder, and it is the whole of the diode ladder's character.
        let mut big_g = [0.0f32; 4];
        let mut inv = [0.0f32; 4];
        for i in 0..4 {
            let g = tan_approx((PI * fc * poles[i] / sample_rate) as f64) as f32;
            big_g[i] = g / (1.0 + g);
            inv[i] = 1.0 / (1.0 + g);
        }

        let resonance = resonance.clamp(0.0, 1.0);
        let k = self.native_resonance(resonance);

        let mut u = input;

        // A filter fed digital silence stays silent forever, so self-oscillation
        // needs a seed. This is deliberate excitation, not denormal protection
        // (that is `flush`), and it is gated so that "no input, low resonance"
        // stays exactly zero.
        if resonance > EXCITATION_THRESHOLD {
            u += EXCITATION_LEVEL * self.rng.next_bipolar();
        }

        // Each TPT one-pole gives y = G*x + s/(1+g). Cascading four with *different*
        // G gives
        //   y4 = (G0*G1*G2*G3)*x
        //      + G1*G2*G3*s0*inv0 + G2*G3*s1*inv1 + G3*s2*inv2 + s3*inv3
        // and with the resonance feedback x = u - k*tanh(y4) that is a scalar
        // nonlinear equation in y4:
        //   F(y) = A - P*k*tanh(y) - y = 0,   P = G0*G1*G2*G3,  A = P*u + S
        let p = big_g[0] * big_g[1] * big_g[2] * big_g[3];
        let a = p * u
            + big_g[1] * big_g[2] * big_g[3] * (self.s[0] * inv[0])
            + big_g[2] * big_g[3] * (self.s[1] * inv[1])
            + big_g[3] * (self.s[2] * inv[2])
            + (self.s[3] * inv[3]);

        // Solve with a fixed number of Newton steps, starting from the previous
        // output. No convergence check and no fallback is needed: F is strictly
        // decreasing (F' <= -1 everywhere), so it has exactly one root and the
        // derivative can never be zero.
        let mut y_solved = self.y_prev;
        for _ in 0..NEWTON_ITERATIONS {
            let t = tanh_approx(y_solved);
            let f = a - p * k * t - y_solved;
            // d/dy tanh(y) = 1 - tanh(y)^2
            let df = -p * k * (1.0 - t * t) - 1.0;
            y_solved -= f / df;
        }

        let x = u - k * tanh_approx(y_solved);

        let mut y = x;
        for (state, g) in self.s.iter_mut().zip(big_g.iter()) {
            let v = (y - *state) * g;
            y = v + *state;
            *state = flush(y + v);
        }

        self.y_prev = flush(y);
        y
    }
}

/// Measure the resonance `k` at which the filter starts to self-oscillate.
///
/// Exists so the threshold is a **measurement of the running filter**, not an
/// assumption inherited from the closed form. The two agreeing is the point of
/// `the_running_filter_agrees_with_the_closed_form`.
pub fn measure_oscillation_threshold(config: DiodeConfig, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let sustains = |resonance: f32| {
        let mut f = DiodeLadder::new(config);
        f.process(1.0, cutoff_hz, resonance, sample_rate);
        let settle = (sample_rate * 0.20) as usize;
        for _ in 0..settle {
            f.process(0.0, cutoff_hz, resonance, sample_rate);
        }
        let mut early = 0.0f32;
        for _ in 0..(sample_rate * 0.05) as usize {
            early = early.max(f.process(0.0, cutoff_hz, resonance, sample_rate).abs());
        }
        for _ in 0..(sample_rate * 0.30) as usize {
            f.process(0.0, cutoff_hz, resonance, sample_rate);
        }
        let mut late = 0.0f32;
        for _ in 0..(sample_rate * 0.05) as usize {
            late = late.max(f.process(0.0, cutoff_hz, resonance, sample_rate).abs());
        }
        late > early * 0.5 && late > 1e-4
    };

    // Bisect on the normalised control, then report the native k for this config.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if sustains(mid) { hi = mid } else { lo = mid }
    }
    DiodeLadder::new(config).native_resonance(hi)
}

/// Measure the frequency a self-oscillating filter settles at, by counting zero
/// crossings.
pub fn measure_oscillation_hz(config: DiodeConfig, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let mut f = DiodeLadder::new(config);
    // Well past threshold so it reaches a limit cycle quickly.
    let resonance = 1.0;
    for _ in 0..(sample_rate * 1.0) as usize {
        f.process(0.0, cutoff_hz, resonance, sample_rate);
    }
    let window = (sample_rate * 0.5) as usize;
    let mut crossings = 0usize;
    let mut prev = f.process(0.0, cutoff_hz, resonance, sample_rate);
    for _ in 0..window {
        let y = f.process(0.0, cutoff_hz, resonance, sample_rate);
        if prev <= 0.0 && y > 0.0 {
            crossings += 1;
        }
        prev = y;
    }
    crossings as f32 / (window as f32 / sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f32; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

    #[test]
    fn the_pole_set_reproduces_the_published_thresholds() {
        // Stinchcombe reports about 18.4 for the TB-303 configuration and about
        // 9.8 for the EMS type, from a separate derivation. Ours is arithmetic on
        // his pole positions, so agreement is a real check on both.
        let tb = DiodeConfig::Tb303.threshold();
        let ems = DiodeConfig::Ems.threshold();
        assert!(
            (tb - 18.34).abs() < 0.05,
            "TB-303 threshold {tb}, expected 18.34"
        );
        assert!(
            (ems - 10.08).abs() < 0.05,
            "EMS threshold {ems}, expected 10.08"
        );
    }

    #[test]
    fn it_does_not_sing_at_its_own_cutoff() {
        // +309 cents. This is the detail that makes a diode ladder used as a tone
        // source sharp against a conventionally calibrated filter frequency.
        let ratio = DiodeConfig::Tb303.oscillation_ratio();
        assert!(
            (ratio - 1.1955).abs() < 0.001,
            "oscillation ratio {ratio}, expected 1.1955"
        );
        let cents = 1200.0 * ratio.ln() / 2.0f32.ln();
        assert!((cents - 309.0).abs() < 2.0, "{cents} cents, expected ~309");
    }

    #[test]
    fn the_running_filter_agrees_with_the_closed_form() {
        // The model is only worth having if the thing that runs matches the
        // analysis. 15% because the bisection reads a limit cycle, not a pole.
        let analytic = DiodeConfig::Tb303.threshold();
        let measured = measure_oscillation_threshold(DiodeConfig::Tb303, 400.0, 48_000.0);
        let error = (measured - analytic).abs() / analytic;
        assert!(
            error < 0.15,
            "measured k {measured} against analytic {analytic}"
        );
    }

    #[test]
    fn self_oscillation_lands_above_the_nominal_cutoff() {
        // Measured on the running filter, against the analytic ratio.
        let nominal = 400.0;
        let hz = measure_oscillation_hz(DiodeConfig::Tb303, nominal, 48_000.0);
        let ratio = hz / nominal;
        assert!(
            ratio > 1.10 && ratio < 1.30,
            "sang at {hz} Hz, ratio {ratio}, expected about 1.1955"
        );
    }

    #[test]
    fn resonance_means_the_same_thing_on_every_configuration() {
        // The knob is a position relative to *this* family's threshold. Both
        // configurations must therefore start singing at about the same knob
        // setting, despite thresholds of 18.34 and 10.08.
        let sings_at = |config: DiodeConfig| {
            let native = measure_oscillation_threshold(config, 400.0, 48_000.0);
            native / (config.threshold() * RESONANCE_MARGIN)
        };
        let tb = sings_at(DiodeConfig::Tb303);
        let ems = sings_at(DiodeConfig::Ems);
        assert!(
            (tb - ems).abs() < 0.12,
            "the two should sing at a similar knob position: {tb} vs {ems}"
        );
        // And the *native* values must still differ, or nothing was normalised.
        let native_tb = DiodeLadder::new(DiodeConfig::Tb303).native_resonance(1.0);
        let native_ems = DiodeLadder::new(DiodeConfig::Ems).native_resonance(1.0);
        assert!(
            native_tb > native_ems * 1.5,
            "native k should differ a lot: {native_tb} vs {native_ems}"
        );
    }

    #[test]
    fn silence_in_gives_exactly_zero_out() {
        // Also the denormal-flush test: without flushing, the states decay
        // asymptotically and never reach zero.
        for fs in RATES {
            let mut f = DiodeLadder::default();
            f.process(1.0, 800.0, 0.5, fs);
            for _ in 0..(fs as usize) {
                f.process(0.0, 800.0, 0.5, fs);
            }
            let y = f.process(0.0, 800.0, 0.5, fs);
            assert_eq!(y, 0.0, "not exactly silent at {fs} Hz: {y}");
        }
    }

    #[test]
    fn no_nan_or_inf_across_a_rate_cutoff_resonance_sweep() {
        for fs in RATES {
            for cutoff_step in 0..12 {
                let cutoff = 20.0 * 2.0f32.powi(cutoff_step);
                for r_step in 0..=10 {
                    let r = r_step as f32 / 10.0;
                    let mut f = DiodeLadder::default();
                    for n in 0..2000 {
                        let x = if n % 100 == 0 { 1.0 } else { 0.0 };
                        let y = f.process(x, cutoff, r, fs);
                        assert!(
                            y.is_finite(),
                            "non-finite at fs={fs} cutoff={cutoff} r={r}: {y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn output_stays_within_the_stated_bound_under_overdrive() {
        for fs in RATES {
            let mut f = DiodeLadder::default();
            let mut peak = 0.0f32;
            // Past the oscillation threshold, driven hard, with the cutoff moving.
            for n in 0..(fs as usize) {
                let t = n as f32 / fs;
                let cutoff = 200.0 + 1500.0 * (1.0 + (2.0 * PI * 3.0 * t).sin());
                let y = f.process(4.0 * (2.0 * PI * 110.0 * t).sin(), cutoff, 1.0, fs);
                peak = peak.max(y.abs());
            }
            assert!(
                peak < OUTPUT_BOUND,
                "peak {peak} exceeded bound {OUTPUT_BOUND} at {fs} Hz"
            );
        }
    }

    #[test]
    fn reset_leaves_no_tail() {
        let mut f = DiodeLadder::default();
        for _ in 0..1000 {
            f.process(0.9, 500.0, 0.8, 48_000.0);
        }
        f.reset();
        let y = f.process(0.0, 500.0, 0.8, 48_000.0);
        assert_eq!(y, 0.0, "reset left a tail: {y}");
    }

    #[test]
    fn the_nominal_cutoff_is_bounded_by_the_widest_pole() {
        // The ratios must hold at every setting, so it is the nominal cutoff that
        // is clamped, not each stage. At 44.1 kHz the widest pole is what caps it.
        let fs = 44_100.0;
        let max = max_nominal_cutoff_hz(DiodeConfig::Tb303, fs);
        let widest = DiodeConfig::Tb303.widest_pole();
        assert!(
            (max * widest - NYQUIST_FRACTION * fs).abs() < 1.0,
            "widest pole should land exactly on the Nyquist fraction"
        );
        // And asking for more does not detune the model: it is simply clamped.
        let mut a = DiodeLadder::default();
        let mut b = DiodeLadder::default();
        for _ in 0..64 {
            let ya = a.process(0.5, max, 0.3, fs);
            let yb = b.process(0.5, max * 4.0, 0.3, fs);
            assert_eq!(ya, yb, "above the cap the filter must simply stop moving");
        }
    }

    #[test]
    fn the_corner_sags_long_before_the_nominal_cutoff() {
        // The signature of the spread pole set: already well down an octave below
        // the nominal cutoff, where a transistor ladder is barely touched. This is
        // why a 303 cutoff control cannot be labelled where a conventional
        // filter's is.
        let fs = 48_000.0;
        let nominal = 1000.0;
        let response = |hz: f32| {
            let mut f = DiodeLadder::default();
            let n = (fs * 0.5) as usize;
            // Settle, then measure peak over a whole number of slow cycles.
            for i in 0..n {
                let t = i as f32 / fs;
                f.process((2.0 * PI * hz * t).sin(), nominal, 0.0, fs);
            }
            let mut peak = 0.0f32;
            for i in n..(n + (fs * 0.2) as usize) {
                let t = i as f32 / fs;
                peak = peak.max(f.process((2.0 * PI * hz * t).sin(), nominal, 0.0, fs).abs());
            }
            20.0 * peak.log10()
        };
        let at_eighth = response(nominal / 8.0);
        let at_nominal = response(nominal);
        assert!(
            at_eighth < -1.0,
            "an eighth below nominal should already be down, got {at_eighth} dB"
        );
        assert!(
            at_nominal < -15.0,
            "at the nominal cutoff the response should be far down, got {at_nominal} dB"
        );
    }
}
