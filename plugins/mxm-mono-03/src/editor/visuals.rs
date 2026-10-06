//! The brief's §8 displays.
//!
//! Two here; the third — output level with clip indication — is the app bar's, per §3.1.
//!
//! Both draw with theme tokens and never with a literal colour: mxm-kit's `crates/ui/AGENTS.md` is
//! explicit that a consumer needing a value the theme does not expose adds the token there rather
//! than the literal here.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2};
use mxm_mono_03_dsp::filter::DiodeConfig;
use mxm_ui::theme::Tokens;

/// The height both displays are drawn at, filling their card's width. **This is the size each card's
/// tree states for them** (`sections::card`), so the statement and the drawing are one constant.
///
/// Enough to read a shape, not enough to compete with the controls.
pub const HEIGHT: f32 = 72.0;

/// The plotted frequency span. Wider than the cutoff's own range, so the corner is never against
/// an edge.
const PLOT_LOW_HZ: f32 = 20.0;
const PLOT_HIGH_HZ: f32 = 20_000.0;

/// The bottom of the plotted magnitude span, in dB. Fixed: the stopband is always worth seeing.
const PLOT_BOTTOM_DB: f32 = -48.0;

/// The **least** headroom above the passband, in dB.
///
/// The top is not fixed. A resonant peak grows without bound as the loop gain approaches threshold,
/// so any ceiling is one a high resonance setting walks straight through — which it did, and the
/// peak was drawn as a flat line along the top edge. The axis is scaled to whatever the curves
/// actually reach instead, and this is only the floor of that, so a filter at rest does not get a
/// wastefully tall plot.
const PLOT_MIN_TOP_DB: f32 = 12.0;

/// Breathing room above the tallest peak, so it does not touch the frame.
const PLOT_HEADROOM_DB: f32 = 3.0;

/// The filter's response, with what the envelope and accent do to it.
///
/// # Why this display earns its place twice
///
/// It shows the **sagging corner** that is the diode ladder's whole character — already 3 dB down
/// three octaves below the nominal cutoff, where a transistor ladder is barely touched. And it is
/// the only place the cutoff's frequency appears at all, because the control deliberately shows a
/// position: `research:filters/machines/tb303-diode-ladder.md` §10.5 works out why a calibrated readout
/// would cost three octaves of headroom.
///
/// # Declared approximation
///
/// The **linear analytic response** of the pole set at the current sample rate. It ignores the
/// drive stage and the resonance feedback's nonlinearity, so a measured sweep at high resonance or
/// high drive will not match it exactly. Stated here rather than left to be discovered: an
/// undeclared approximation is a bug report from whoever compares the curve to a sweep.
#[allow(clippy::too_many_arguments)]
pub fn filter_response(
    ui: &mut Ui,
    tokens: &Tokens,
    config: DiodeConfig,
    cutoff_hz: f32,
    resonance: f32,
    env_reach_hz: f32,
    accent_reach_hz: f32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, tokens.surface_2);

    // Every curve that will be drawn, base last so it paints over the others.
    let mut curves: Vec<(f32, Color32, f32)> = Vec::new();
    for (reach, colour) in [
        (accent_reach_hz, tokens.mod_random),
        (env_reach_hz, tokens.mod_envelope),
    ] {
        if reach > cutoff_hz * 1.01 {
            curves.push((reach, faded(colour), 1.0));
        }
    }
    curves.push((cutoff_hz, tokens.accent, 1.5));

    // **The axis is scaled to what is actually drawn**, over every curve at once — a peak that fits
    // one trace and clips another would be worse than clipping both, because only one of them would
    // look wrong.
    let samples: Vec<Vec<f32>> = curves
        .iter()
        .map(|(hz, _, _)| response_db(config, *hz, resonance))
        .collect();
    let peak = samples
        .iter()
        .flat_map(|curve| curve.iter().copied())
        .fold(f32::NEG_INFINITY, f32::max);
    let top_db = (peak + PLOT_HEADROOM_DB).max(PLOT_MIN_TOP_DB);

    for (curve, (_, colour, width)) in samples.iter().zip(&curves) {
        plot(&painter, rect, curve, top_db, *colour, *width);
    }

    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
}

/// One curve's magnitude, in dB, sampled evenly in log frequency across the plot.
fn response_db(config: DiodeConfig, cutoff_hz: f32, resonance: f32) -> Vec<f32> {
    (0..POINTS)
        .map(|i| {
            let t = i as f32 / (POINTS - 1) as f32;
            let hz = PLOT_LOW_HZ * (PLOT_HIGH_HZ / PLOT_LOW_HZ).powf(t);
            magnitude_db(config, cutoff_hz, resonance, hz)
        })
        .collect()
}

/// How finely a curve is sampled.
const POINTS: usize = 160;

/// One curve, mapped onto the rect.
fn plot(
    painter: &egui::Painter,
    rect: Rect,
    curve: &[f32],
    top_db: f32,
    colour: Color32,
    width: f32,
) {
    let span = top_db - PLOT_BOTTOM_DB;
    let points: Vec<Pos2> = curve
        .iter()
        .enumerate()
        .map(|(i, db)| {
            let t = i as f32 / (curve.len() - 1) as f32;
            let y = (top_db - db) / span;
            pos2(
                rect.left() + t * rect.width(),
                rect.top() + y.clamp(0.0, 1.0) * rect.height(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(width, colour)));
}

/// The pole set's magnitude at one frequency, in dB.
///
/// Four real poles with global feedback: `H(s) = 1 / (prod(1 + s/p_i) + k)`, evaluated on the
/// imaginary axis. This is the *analogue* prototype rather than the running digital filter — the
/// difference is prewarping near Nyquist, which is off the right-hand edge of this plot.
fn magnitude_db(config: DiodeConfig, cutoff_hz: f32, resonance: f32, hz: f32) -> f32 {
    let poles = config.poles();
    let k =
        config.threshold() * mxm_mono_03_dsp::filter::RESONANCE_MARGIN * resonance.clamp(0.0, 1.0);

    // Product of (1 + j w / (p_i * wc)), in complex arithmetic done by hand — this crate has no
    // complex type and does not want one for four multiplies.
    let (mut re, mut im) = (1.0f32, 0.0f32);
    for p in poles {
        let ratio = hz / (cutoff_hz * p);
        let (nr, ni) = (re * 1.0 - im * ratio, re * ratio + im * 1.0);
        re = nr;
        im = ni;
    }
    re += k;

    let magnitude = 1.0 / (re * re + im * im).sqrt().max(1e-12);
    // Normalised so the passband sits at 0 dB rather than at the droop's -1/(1+k). The droop is
    // real and audible, but a curve that sank off the bottom of the plot as resonance rose would
    // show it by becoming unreadable.
    let dc = 1.0 / (1.0 + k);
    20.0 * (magnitude / dc).max(1e-6).log10()
}

/// The accent sweep's accumulated charge, over the last few seconds.
///
/// # The display this instrument exists to have
///
/// Consecutive accents climb because the sweep capacitor has not finished discharging between
/// notes. **Nothing on the panel can show that** — the Accent knob has not moved, and the note is
/// simply brighter than the one before it. Brief §8 makes the case: without this, the machine's
/// most distinctive behaviour reads as a fault.
///
/// **Exact**, not approximated: the values the DSP used, published once per block.
pub fn accent_sweep(ui: &mut Ui, tokens: &Tokens, history: &[f32], ceiling: f32) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), HEIGHT), Sense::hover());
    // What the sweep shows, on hover rather than printed (design system §7.6).
    response.on_hover_text(
        "Each accent's filter sweep. Accents close together build on each other, so a run of them \
         climbs higher.",
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, tokens.surface_2);

    if history.len() > 1 {
        let last = history.len() - 1;
        let points: Vec<Pos2> = history
            .iter()
            .enumerate()
            .map(|(i, charge)| {
                let t = i as f32 / last as f32;
                let level = (charge / ceiling).clamp(0.0, 1.0);
                pos2(
                    rect.left() + t * rect.width(),
                    rect.bottom() - level * rect.height(),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.5, tokens.mod_random),
        ));
    }

    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
}

/// A secondary trace's colour: the same hue, quieter, so the base curve reads as the base.
fn faded(colour: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), 110)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The defect, stated as a number.**
    ///
    /// The plot had a fixed +18 dB ceiling and clamped to it, so at high resonance the peak was
    /// drawn as a flat line along the top edge. This asserts the peak really does outgrow that
    /// ceiling — without it the autoscale below would be a fix for nothing, and would pass whether
    /// or not it worked.
    #[test]
    fn a_resonant_peak_outgrows_any_fixed_ceiling() {
        let curve = response_db(DiodeConfig::Tb303, 800.0, 0.9);
        let peak = curve.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            peak > 18.0,
            "the ceiling that was being clipped against is not actually exceeded, so this display \
             had no bug and the autoscale is solving nothing: peak {peak} dB"
        );
    }

    /// Nothing drawn ever leaves the box, at any resonance, on either pole set.
    ///
    /// Checked against the mapping itself rather than against the clamp inside `plot`: a clamp is
    /// what *hid* the defect as a flat line instead of an overflow, so a test that trusted it would
    /// pass on the broken version too.
    #[test]
    fn the_curve_stays_inside_the_plot_at_every_resonance() {
        for config in [DiodeConfig::Tb303, DiodeConfig::Ems] {
            for step in 0..=20 {
                let resonance = step as f32 / 20.0;
                let curve = response_db(config, 800.0, resonance);
                let peak = curve.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let top_db = (peak + PLOT_HEADROOM_DB).max(PLOT_MIN_TOP_DB);
                let span = top_db - PLOT_BOTTOM_DB;

                let highest = (top_db - peak) / span;
                assert!(
                    (0.0..=1.0).contains(&highest),
                    "{config:?} at resonance {resonance} puts its peak at {highest} of the plot"
                );
                assert!(
                    highest > 0.0,
                    "{config:?} at resonance {resonance} draws its peak on the frame"
                );
            }
        }
    }

    /// The passband stays put as resonance rises, so the axis moving does not read as the filter
    /// moving.
    #[test]
    fn the_passband_sits_at_zero_whatever_the_resonance() {
        for step in 0..=10 {
            let resonance = step as f32 / 10.0;
            let low = magnitude_db(DiodeConfig::Tb303, 800.0, resonance, PLOT_LOW_HZ);
            assert!(
                low.abs() < 1.0,
                "at resonance {resonance} the passband sits at {low} dB rather than 0"
            );
        }
    }
}
