//! The oscillator: a sawtooth core, and a square shaped out of it.
//!
//! # The square is not a second oscillator
//!
//! On the hardware the square is not generated independently — it is waveshaped
//! from the sawtooth core by two capacitors, four resistors and a transistor,
//! which is about as cheap as a waveform gets. The consequence is that it is
//! **not 50% duty, and its duty cycle moves with pitch**: roughly 45% at high
//! pitches and roughly 71% at the lowest.
//!
//! mxm-kit's `docs/oscillators/04-analog-character.md` §4.5 is blunt about the priority
//! here: if you are modelling this machine, that pitch dependence "is a much
//! larger effect than anything in chapter 2" — that is, larger than the choice of
//! antialiasing method. It costs one lookup.
//!
//! **The figures are secondary and unverified** (`research:oscillators/05-machines.md`
//! §5.4 says so), so they are named here as constants rather than buried.
//!
//! # Antialiasing
//!
//! PolyBLEP — a 2-point polynomial approximation to a band-limited step, added at
//! each discontinuity. The pulse is generated as the difference of two sawtooths a
//! duty cycle apart, which is the same construction as the waveshaper and gets
//! both of its edges corrected for free.
//!
//! Because the duty cycle moves with pitch, the *second* edge moves too. That is a
//! property the correction has to survive rather than a new technique: the BLEP is
//! evaluated against each edge's own phase distance, so a moving edge is handled
//! by construction.

/// Duty cycle at and below [`DUTY_LOW_HZ`]. Secondary, unverified.
pub const DUTY_AT_LOW: f32 = 0.71;
/// Duty cycle at and above [`DUTY_HIGH_HZ`]. Secondary, unverified.
pub const DUTY_AT_HIGH: f32 = 0.45;
/// Fundamental at which the duty cycle stops widening.
pub const DUTY_LOW_HZ: f32 = 32.7;
/// Fundamental at which the duty cycle stops narrowing.
pub const DUTY_HIGH_HZ: f32 = 523.3;

/// Which waveform the switch has selected.
///
/// **A switch, not a blend.** The hardware offers one at a time, and this
/// instrument does not offer a mixture: reaching timbres the machine cannot is
/// what the rest of the collection is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    #[default]
    Sawtooth,
    Square,
}

/// The pulse width for a given fundamental, interpolated in log frequency.
///
/// Log rather than linear because pitch is logarithmic: an interpolation linear in
/// Hz would spend almost all of its travel in the top octave and leave the bass
/// end — where this instrument lives — effectively constant.
pub fn duty_for(f0: f32) -> f32 {
    let f0 = f0.clamp(DUTY_LOW_HZ, DUTY_HIGH_HZ);
    let t = (f0 / DUTY_LOW_HZ).ln() / (DUTY_HIGH_HZ / DUTY_LOW_HZ).ln();
    DUTY_AT_LOW + (DUTY_AT_HIGH - DUTY_AT_LOW) * t
}

/// PolyBLEP correction at a discontinuity.
///
/// `t` is the phase distance past the edge, `dt` the phase increment per sample.
/// Returns the correction to add to a naive waveform whose step is +2 (a falling
/// sawtooth reset is -2, so the caller negates).
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// A free-running phase accumulator.
#[derive(Debug, Clone, Copy, Default)]
pub struct Phasor {
    phase: f32,
    inc: f32,
}

impl Phasor {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            inc: 0.0,
        }
    }

    pub fn set_frequency(&mut self, hz: f32, sample_rate: f32) {
        self.inc = (hz / sample_rate).clamp(0.0, 0.49);
    }

    pub fn increment(&self) -> f32 {
        self.inc
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    #[inline]
    pub fn advance(&mut self) {
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
    }
}

/// The oscillator: one phasor, two waveforms taken from it.
#[derive(Debug, Clone, Default)]
pub struct Oscillator {
    phasor: Phasor,
}

impl Oscillator {
    pub const fn new() -> Self {
        Self {
            phasor: Phasor::new(),
        }
    }

    pub fn reset(&mut self) {
        self.phasor.reset();
    }

    /// One sample at `hz`, band-limited.
    #[inline]
    pub fn process(&mut self, hz: f32, wave: Waveform, sample_rate: f32) -> f32 {
        self.phasor.set_frequency(hz, sample_rate);
        let t = self.phasor.phase();
        let dt = self.phasor.increment();

        let y = match wave {
            Waveform::Sawtooth => saw(t, dt),
            Waveform::Square => {
                // The waveshaper: a pulse is the difference of two sawtooths a
                // duty cycle apart. Both edges get their own BLEP, which is what
                // lets the edge move with pitch without special handling.
                // `duty_for` is the fraction of the period spent *high*, so the
                // two sawtooths are offset by its complement.
                let d = 1.0 - duty_for(hz);
                let t2 = if t >= d { t - d } else { t + 1.0 - d };
                // Difference of two saws gives 2d and 2d-2; this offset makes the
                // swing a symmetric +/-1. It does **not** remove the mean, and must
                // not: a pulse whose duty is not 50% carries DC by definition, and
                // that is exactly what the pitch-dependent duty produces here. The
                // DC blocker on the voice's *output* is where it comes off, per
                // mxm-kit's `docs/filters/03-nonlinearity.md` §3.4.
                saw(t, dt) - saw(t2, dt) - (2.0 * d - 1.0)
            }
        };

        self.phasor.advance();
        y
    }
}

/// A band-limited sawtooth rising from -1 to 1 over the period.
#[inline]
fn saw(t: f32, dt: f32) -> f32 {
    2.0 * t - 1.0 - poly_blep(t, dt)
}

/// Peak of the ideal (unfiltered) waveform, for tests and for the voice's makeup.
pub const OSC_PEAK: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn duty_widens_toward_the_bass_and_matches_the_published_ends() {
        // The published figures, such as they are. Named here so that if someone
        // ever measures a real machine, the disagreement is visible.
        assert!((duty_for(20.0) - DUTY_AT_LOW).abs() < 1e-6, "clamped low");
        assert!(
            (duty_for(2000.0) - DUTY_AT_HIGH).abs() < 1e-6,
            "clamped high"
        );
        let low = duty_for(40.0);
        let high = duty_for(400.0);
        assert!(
            low > high,
            "duty must narrow as pitch rises: {low} at 40 Hz vs {high} at 400 Hz"
        );
        assert!((0.44..=0.72).contains(&low) && (0.44..=0.72).contains(&high));
    }

    #[test]
    fn the_square_is_not_fifty_percent() {
        // The whole point of §5.4. Measure the fraction of the period spent above
        // zero at a bass pitch and confirm it is nowhere near half.
        let fs = 48_000.0;
        let hz = 55.0;
        let mut o = Oscillator::new();
        let n = (fs / hz) as usize * 8;
        let mut above = 0usize;
        for _ in 0..n {
            if o.process(hz, Waveform::Square, fs) > 0.0 {
                above += 1;
            }
        }
        let fraction = above as f32 / n as f32;
        let expected = duty_for(hz);
        assert!(
            fraction > 0.6,
            "at 55 Hz the duty should be well over half, measured {fraction}"
        );
        assert!(
            (fraction - expected).abs() < 0.05,
            "measured duty {fraction} should match the model's {expected}"
        );
    }

    #[test]
    fn the_squares_dc_tracks_its_duty_and_is_left_for_the_output_to_block() {
        // A pulse whose duty is not 50% carries DC, and this one's duty moves with
        // pitch — so its offset moves with pitch too. That is the waveform being
        // right, not a defect: the mean must equal `2*duty - 1`, and it comes off
        // at the voice's output, never inside the filter loop.
        let fs = 48_000.0;
        let mut o = Oscillator::new();
        for hz in [40.0f32, 110.0, 330.0] {
            let n = (fs / hz) as usize * 20;
            let mut sum = 0.0f64;
            for _ in 0..n {
                sum += o.process(hz, Waveform::Square, fs) as f64;
            }
            let mean = (sum / n as f64) as f32;
            let expected = 2.0 * duty_for(hz) - 1.0;
            assert!(
                (mean - expected).abs() < 0.05,
                "at {hz} Hz mean {mean}, expected {expected} from a duty of {}",
                duty_for(hz)
            );
        }
    }

    #[test]
    fn aliasing_stays_far_below_a_naive_sawtooth() {
        // Measured against a naive modulo counter computed in the same test, so
        // the measurement validates itself rather than trusting a constant.
        let fs = 48_000.0;
        let hz = 2093.0; // high enough that a naive saw folds badly
        let n = 8192;

        let mut o = Oscillator::new();
        let blep: Vec<f32> = (0..n)
            .map(|_| o.process(hz, Waveform::Sawtooth, fs))
            .collect();

        let mut phase = 0.0f32;
        let inc = hz / fs;
        let naive: Vec<f32> = (0..n)
            .map(|_| {
                let y = 2.0 * phase - 1.0;
                phase += inc;
                if phase >= 1.0 {
                    phase -= 1.0;
                }
                y
            })
            .collect();

        // Energy at frequencies that are not harmonics of the fundamental.
        let inharmonic = |x: &[f32]| {
            let mut total = 0.0f64;
            for bin in 1..(n / 2) {
                let f = bin as f32 * fs / n as f32;
                let ratio = f / hz;
                if (ratio - ratio.round()).abs() < 0.05 {
                    continue; // a harmonic, or close enough to one
                }
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, s) in x.iter().enumerate() {
                    let w = 2.0 * PI as f64 * bin as f64 * i as f64 / n as f64;
                    re += *s as f64 * w.cos();
                    im -= *s as f64 * w.sin();
                }
                total += re * re + im * im;
            }
            total
        };

        let ours = inharmonic(&blep);
        let theirs = inharmonic(&naive);
        assert!(
            ours < theirs * 0.25,
            "aliasing {ours} should be far below the naive counter's {theirs}"
        );
    }
}
