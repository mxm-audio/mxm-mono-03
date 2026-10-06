//! Parameter definitions.
//!
//! Every `#[id]` here is **permanent**. Changing one breaks every saved project that used the
//! plugin, so ids are part of the public interface.
//!
//! # The panel is the hardware's controls, and nothing else
//!
//! Tuning, Waveform, Cutoff, Resonance, Decay, Accent, Volume. The fixed things — the
//! amplitude envelope's times, the filter envelope's attack, the accent sweep's time constant — stay
//! internal, because being unable to adjust them is a large part of what this instrument *is*.
//! Anyone wanting to reach them wants a different instrument, and the collection is where those go.
//!
//! **Env Mod is a route now, not a knob**: the (Cutoff ← Filter envelope) route in
//! [`routes`](crate::routes), above the circuit's floor, which is not an amount at all
//! (`plans/plan-mxm-mono-03-modulation.md` §3).
//!
//! # The two per-note switches
//!
//! The hardware's sequencer gives every step two buttons — **accent** and **slide** — and this
//! instrument has no sequencer, so both have to be parameters here for a host's per-step modulation
//! to reach them. They are the only controls on this panel the hardware's *panel* did not have, and
//! they are not additions to the machine: they are the machine's own per-step flags, arriving by the
//! only route a plugin has.
//!
//! **Slide has no time control**, because the hardware has none: its slide time is a fixed RC in the
//! pitch path and the button is simply on or off. The owner asks for glide in the synth rather than
//! the sequencer, and that is about where the *mechanism* lives — the lag is in the voice, so a
//! keyboard player gets it too — not about a time knob the machine never had.
//!
//! # Two collection contracts, and where this instrument sits against them
//!
//! **Every amount starts at zero.** Accent depth, accent-note, slide and resonance all do, and so does
//! the Env Mod route; the circuit's floor under it is not an amount. **The accent circuit's two
//! routes start at full** — two recorded init deviations, because the Accent knob is their depth.
//! See the plugin's own AGENTS.md.
//!
//! # Smooth signals, not coefficients
//!
//! Cutoff, resonance, accent depth, volume and every route's amount are added to or multiplied into
//! the audio and are smoothed. Decay, accent-note and slide are not: they set state-machine behaviour, and
//! smoothing them would make the timing impossible to reason about — and would turn a per-note
//! decision into a ramp between two notes.

use mxm_mono_03_dsp::accent;
use mxm_mono_03_dsp::envelope;
use mxm_mono_03_dsp::filter::DiodeConfig;
use mxm_mono_03_dsp::oscillator::Waveform;
use mxm_mono_03_dsp::voice::{DRIVE, SLIDE_TIME_S};
use nice_plug::prelude::*;
use std::sync::{Arc, RwLock};

/// The waveform switch: one at a time, as the hardware has it.
///
/// **Not a blend and not a mixer.** A continuous morph is a strict superset that reaches timbres the
/// machine cannot, which is the argument against it here.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveKind {
    #[id = "sawtooth"]
    #[name = "Sawtooth"]
    Sawtooth,
    #[id = "square"]
    #[name = "Square"]
    Square,
}

impl From<WaveKind> for Waveform {
    fn from(w: WaveKind) -> Self {
        match w {
            WaveKind::Sawtooth => Waveform::Sawtooth,
            WaveKind::Square => Waveform::Square,
        }
    }
}

/// Which diode ladder configuration the filter models.
///
/// **Advanced.** The TB-303 configuration is the instrument; the EMS type is the
/// three-diode variant, and the pole sets differ enough to be a different filter rather than a
/// detuning. Its pole positions are **fitted, not derived** — see
/// `research:filters/machines/tb303-diode-ladder.md` §8 — so it should not be trusted for corner shape.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterModel {
    #[id = "tb303"]
    #[name = "Diode (1)"]
    Tb303,
    #[id = "ems"]
    #[name = "Diode (3)"]
    Ems,
}

impl From<FilterModel> for DiodeConfig {
    fn from(m: FilterModel) -> Self {
        match m {
            FilterModel::Tb303 => DiodeConfig::Tb303,
            FilterModel::Ems => DiodeConfig::Ems,
        }
    }
}

/// Formats a parameter value for display.
type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
/// Parses a typed-in value, returning `None` if it cannot be understood.
type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

/// Format seconds as milliseconds below a second, seconds above.
///
/// **The unit is chosen from what the millisecond text would round to, not from the raw value.**
/// A host parses the text and normalises it before printing it again, so the value it prints lands a
/// hair either side of where it started: switching at the raw second printed `0.9996 s` as
/// `1000 ms`, which parses to one second and prints `1.00 s`, and `clap-validator`'s
/// `param-conversions` fails whenever its values land there. Anything that would print `1000 ms`
/// prints seconds instead.
fn v2s_time() -> ValueToString {
    Arc::new(|s| {
        if (s * 1_000.0).round() >= 1_000.0 {
            format!("{s:.2} s")
        } else {
            format!("{:.0} ms", s * 1000.0)
        }
    })
}

fn s2v_time() -> StringToValue {
    Arc::new(|text| {
        let t = text.trim().to_lowercase();
        let (number, scale) = if let Some(rest) = t.strip_suffix("ms") {
            (rest, 0.001)
        } else if let Some(rest) = t.strip_suffix('s') {
            (rest, 1.0)
        } else {
            (t.as_str(), 0.001)
        };
        number.trim().parse::<f32>().ok().map(|v| v * scale)
    })
}

/// Show a `0..=1` control as a percentage.
fn v2s_percent() -> ValueToString {
    Arc::new(|v| format!("{:.0} %", v * 100.0))
}

fn s2v_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
    })
}

#[derive(Params)]
pub struct MxmMono03Params {
    // ---- Oscillator ----
    #[id = "tune"]
    pub tune: FloatParam,
    #[id = "waveform"]
    pub waveform: EnumParam<WaveKind>,
    /// **An addition**, and the MIDI-era equivalent of a control the hardware had its own way of
    /// offering. Set once per setup.
    #[id = "bendrange"]
    pub bend_range: FloatParam,

    // ---- Filter ----
    /// A **position**, not a frequency. See `mxm_mono_03_dsp::voice::CUTOFF_HIGH_HZ` for why a
    /// calibrated readout is not on offer.
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,
    /// Filter envelope decay. Overridden to its minimum on an accented note.
    #[id = "decay"]
    pub decay: FloatParam,

    // ---- Accent ----
    /// How hard an accent hits — the hardware's knob.
    #[id = "accent"]
    pub accent: FloatParam,
    /// Whether *this note* is accented — the hardware's per-step button.
    ///
    /// **Two controls, not one**, because a single parameter cannot be both: turning a depth knob up
    /// would accent every note, which the machine does not do.
    ///
    /// **A `BoolParam`, because the button is on or off.** It was a `FloatParam` in the first build,
    /// on the reasoning that a host modulates continuously — but a host modulating a bool works
    /// exactly the same way, `(unmodulated + offset).clamp(0, 1)`, and the wrong type cost the
    /// segmented control its step count. `mxm-ui`'s debug assertion caught it on the first frame.
    #[id = "accentnote"]
    pub accent_note: BoolParam,

    // ---- Slide ----
    /// Whether *this note* slides in from the one before — the hardware's per-step button.
    ///
    /// On or off, with no time control: the slide time is fixed in hardware. What it decides is
    /// only whether the pitch lag engages; whether the envelopes retrigger follows the gate, which
    /// is the host's business. See `mxm_mono_03_dsp::voice::Voice::note_on`.
    #[id = "slide"]
    pub slide: BoolParam,

    // ---- Output ----
    #[id = "volume"]
    pub volume: FloatParam,

    // ================= Advanced =================
    //
    // **Behind a disclosure, and every one defaults to the hardware's own value**, so Init is the
    // machine and opening the panel changes nothing until something is moved. They are the
    // constants the machine fixed; the brief's rule is that a disclosure holds what the original
    // did not have, and an adjustable version of a fixed constant is exactly that.
    //
    // Ordinary automatable parameters rather than persisted state: this is a fixed set the plugin
    // ships, not a model anybody can extend at runtime. See `plugins/AGENTS.md`.
    //
    // **None of them is smoothed.** A constant is not a signal.
    /// Slide time. Fixed at [`SLIDE_TIME_S`] on the hardware.
    #[id = "slidetime"]
    pub slide_time: FloatParam,
    /// The amplitude envelope's attack — the hardware's de-clicker, made a shape.
    #[id = "ampattack"]
    pub amp_attack: FloatParam,
    /// The amplitude envelope's decay. The big one: it is why every note is flat-topped.
    #[id = "ampdecay"]
    pub amp_decay: FloatParam,
    /// The filter envelope's attack. Lets the filter swell instead of only snapping.
    #[id = "filterattack"]
    pub filter_attack: FloatParam,
    /// The decay an accented note is forced to, overriding Decay.
    #[id = "accentdecay"]
    pub accent_decay: FloatParam,
    /// Stretches the accent sweep's time constants together. The climb's rate.
    #[id = "accentsweep"]
    pub accent_sweep: FloatParam,
    /// How hard the oscillator drives the filter.
    #[id = "drive"]
    pub drive: FloatParam,
    /// Which diode ladder configuration the filter models.
    #[id = "filtermodel"]
    pub filter_model: EnumParam<FilterModel>,

    /// **The routing**: a presence and a signed amount for every *(target, source)* pair. Env Mod
    /// and the accent circuit's two outputs are routes present in the init patch
    /// (`plans/plan-mxm-mono-03-modulation.md` §5).
    #[nested(group = "Modulation")]
    pub routes: crate::routes::Routes,

    /// Which preset is loaded, and what it looked like when it was.
    ///
    /// **Persisted with the patch, not beside it.** nice-plug carries non-parameter state through
    /// the `Params` derive's `#[persist]`, so it belongs here rather than as a field on the plugin
    /// struct — and it has its own version number, because nice-plug's state version is
    /// `Plugin::VERSION`, which moves for unrelated reasons.
    #[persist = "preset"]
    pub preset: RwLock<mxm_preset::PresetIdentity>,
}

impl Default for MxmMono03Params {
    /// The init patch, which is also the set of CLAP `default_value`s.
    ///
    /// They are allowed to be equal and must not be two concepts: a host shows `default_value` as a
    /// control's detent and uses it for "reset this parameter", so a divergence means the Init
    /// button and the host disagree about the same sound.
    fn default() -> Self {
        Self {
            tune: FloatParam::new(
                "Tune",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" cents")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            waveform: EnumParam::new("Waveform", WaveKind::Sawtooth),

            bend_range: FloatParam::new(
                "Bend range",
                2.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 24.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            cutoff: FloatParam::new("Cutoff", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_value_to_string(v2s_percent())
                .with_string_to_value(s2v_percent()),

            resonance: FloatParam::new("Resonance", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_value_to_string(v2s_percent())
                .with_string_to_value(s2v_percent()),

            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                // Not smoothed: it sets an envelope's timing, not a signal.
                .with_value_to_string(v2s_percent())
                .with_string_to_value(s2v_percent()),

            accent: FloatParam::new("Accent", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_value_to_string(v2s_percent())
                .with_string_to_value(s2v_percent()),

            // Read once, at a note-on. There is nothing to smooth: a per-note decision ramped
            // between two notes would be neither.
            accent_note: BoolParam::new("Accent note", false),
            slide: BoolParam::new("Slide", false),

            volume: FloatParam::new(
                "Volume",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(0.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 0.0),
                },
            )
            // Stored as linear gain, formatted as dB: `SmoothingStyle::Logarithmic` over a dB range
            // spanning zero is mathematically invalid and trips a debug assertion.
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // ---- Advanced ----
            slide_time: FloatParam::new(
                "Slide time",
                SLIDE_TIME_S,
                FloatRange::Skewed {
                    min: 0.005,
                    max: 0.500,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(v2s_time())
            .with_string_to_value(s2v_time()),

            amp_attack: FloatParam::new(
                "Amp attack",
                envelope::ATTACK_S,
                FloatRange::Skewed {
                    min: envelope::ATTACK_MIN_S,
                    max: envelope::ATTACK_MAX_S,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(v2s_time())
            .with_string_to_value(s2v_time()),

            amp_decay: FloatParam::new(
                "Amp decay",
                envelope::AMP_DECAY_S,
                FloatRange::Skewed {
                    min: 0.030,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(v2s_time())
            .with_string_to_value(s2v_time()),

            filter_attack: FloatParam::new(
                "Filter attack",
                envelope::ATTACK_S,
                FloatRange::Skewed {
                    min: envelope::ATTACK_MIN_S,
                    max: envelope::ATTACK_MAX_S,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(v2s_time())
            .with_string_to_value(s2v_time()),

            accent_decay: FloatParam::new(
                "Accent decay",
                envelope::DECAY_MIN_S,
                FloatRange::Skewed {
                    min: 0.020,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(v2s_time())
            .with_string_to_value(s2v_time()),

            accent_sweep: FloatParam::new(
                "Accent sweep",
                1.0,
                FloatRange::Skewed {
                    min: accent::SWEEP_SCALE_MIN,
                    max: accent::SWEEP_SCALE_MAX,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            drive: FloatParam::new(
                "Drive",
                DRIVE,
                FloatRange::Skewed {
                    min: 0.5,
                    max: 24.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            filter_model: EnumParam::new("Filter model", FilterModel::Tb303),

            routes: crate::routes::Routes::new(),

            preset: RwLock::new(mxm_preset::PresetIdentity::none()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every parameter reads the same after the host's own round trip**: printed with its unit,
    /// parsed, and printed again, it is the same text (mxm-kit's `docs/code-review-notes.md` §6).
    ///
    /// The host never hands a formatter a plain value. The CLAP wrapper's `value_to_text` and
    /// `text_to_value` carry a normalised value in `f64`, scaled by the step count, so a parsed number
    /// goes through the range's normalisation and back before it is printed again — and a formatter
    /// that picks its unit or its precision from the *raw* value flips branch when that trip lands a
    /// hair the other side of the switch: `0.9996 s` printed `1000 ms`, which parses to one second
    /// and prints `1.00 s`. `clap-validator`'s `param-conversions` fails only when its values land in
    /// that sliver, so one clean run proves nothing.
    ///
    /// So this walks the whole parameter map, as the wrapper converts, at the validator's grids, at
    /// plain values either side of every branch point this file's and the routes' formatters have — a
    /// second, zero where a range crosses it, 0 dB — and at every representable normalised value
    /// near each of those points.
    #[test]
    fn every_parameter_reads_the_same_after_the_hosts_round_trip() {
        let params = MxmMono03Params::default();
        let map = params.param_map();
        // `clap-validator` 0.4.1 spends 4000 conversions across the parameters, 5 to 100 each.
        let installed = 4000usize.div_ceil(map.len()).clamp(5, 100);

        let mut probes: Vec<f32> = vec![
            // Seconds, printed `{:.0} ms` below one second: the millisecond text reaches `1000` at
            // 0.9995 s.
            0.9994, 0.999_49, 0.9995, 0.999_51, 0.9996, 0.9999, 1.0, 1.000_01, 1.004, 1.005, 1.006,
        ];
        // Either side of zero, where a plain `{:.N}` prints a negative zero.
        probes.push(0.0);
        for decade in [1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1] {
            for multiple in [1.0, 4.0, 5.0, 6.0] {
                probes.extend([decade * multiple, -decade * multiple]);
            }
        }
        // Either side of 0 dB, for a gain shown in decibels.
        for db in [1e-4, 1e-3, 0.04, 0.05, 0.06] {
            probes.extend([util::db_to_gain(db), util::db_to_gain(-db)]);
        }
        // The branch points themselves, where every nearby normalised value is tried too.
        let edges = [0.0, 0.9995, 1.0];

        let mut failures = Vec::new();
        for (id, param, _) in &map {
            // SAFETY: `params` owns every parameter these pointers name and outlives the loop; this
            // is the access the wrapper makes.
            unsafe {
                let steps = param.step_count();
                let scale = steps.unwrap_or(1) as f64;
                let mut values: Vec<f64> = (0..=19)
                    .map(|i| scale * f64::from(i) / 19.0)
                    .chain((0..installed).map(|i| scale * i as f64 / (installed - 1) as f64))
                    .collect();
                if steps.is_none() {
                    let (low, high) = (param.preview_plain(0.0), param.preview_plain(1.0));
                    values.extend(
                        probes
                            .iter()
                            .map(|&plain| f64::from(param.preview_normalized(plain))),
                    );
                    for &edge in edges.iter().filter(|&&edge| low <= edge && edge <= high) {
                        let mut up = param.preview_normalized(edge);
                        let mut down = up;
                        for _ in 0..=64 {
                            values.extend([f64::from(up), f64::from(down)]);
                            up = up.next_up().min(1.0);
                            down = down.next_down().max(0.0);
                        }
                    }
                }
                // `ext_params_value_to_text` and `ext_params_text_to_value`, as the wrapper has them.
                let text_of = |value: f64| {
                    param.normalized_value_to_string(value as f32 / scale as f32, true)
                };
                for value in values {
                    let first = text_of(value);
                    let Some(back) = param.string_to_normalized_value(&first) else {
                        failures.push(format!("{id}: {first:?} does not parse"));
                        continue;
                    };
                    let second = text_of(f64::from(back) * scale);
                    if second != first {
                        failures.push(format!("{id}: {first:?} parses and reads {second:?}"));
                    }
                }
            }
        }
        failures.sort();
        failures.dedup();
        assert!(
            failures.is_empty(),
            "{} texts changed through the host's conversion:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
