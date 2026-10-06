//! One tree per card, and the advanced zone.
//!
//! The brief owns what goes where; this file implements it. In particular §5's two zones and §10's
//! section order are the contract, and [`super::SECTIONS`] is written in that order so a
//! reordering is a visible diff rather than a drift.
//!
//! A card's body is described once, as a `mxm_ui::tree` ([`card`]), and that one description is
//! both measured — the card's floor and its height, by the paging renderer — and drawn, leaf by
//! leaf, through the bindings ([`paint`]). Nothing is typed and nothing is drawn to learn a size
//! (`plans/plan-layout-tree.md`).

use std::collections::HashMap;

use egui::{Rect, Ui};
use mxm_ui::control::{Size, Wave};
use mxm_ui::space::SPACE_3;
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{
    self, Height, Kind, Node, Share, leaf, pad, pad_all, row_gap, share, stack, stack_gap,
};
use nice_plug::prelude::ParamSetter;

use super::binding::{Bound, segmented_named, segmented_waves_named, toggle_labelled};
use super::{Section, visuals};
use crate::params::MxmMono03Params;
use crate::telemetry::Telemetry;
use mxm_mono_03_dsp::routing::{FULL_SCALE, TARGET_NAMES, cutoff_octaves, source, target};

/// §7.1's sizing, as used here.
///
/// Brief §2 puts Cutoff and Resonance at **Primary**: unlike `mxm-mono-01`, this instrument has few
/// enough controls per card that the larger tier costs no row. Everything else is Standard, and the
/// advanced zone is Compact — a size difference is the cheapest way to say *these are not the ones
/// you reach for*, and it carries alongside the surface change rather than relying on it.
const PRIMARY: Size = Size::Primary;
const STANDARD: Size = Size::Standard;
const COMPACT: Size = Size::Compact;

/// The paging items' titles, in key order: the two sections, then the advanced zone.
pub const TITLES: [&str; 3] = ["Voice", "Filter", "Advanced"];

/// The oscillator's two waveforms: a switch, not a blend — the hardware offers one at a time.
///
/// **Drawn, not spelled.** mxm-kit's `crates/ui/AGENTS.md` names this exact case: a square wave is
/// the same picture in an LFO, an oscillator and here, and the vocabulary is shared so every
/// instrument says it the same way. The words survive as hover and accessible name.
const WAVEFORMS: &[(Wave, &str)] = &[(Wave::RampUp, "Sawtooth"), (Wave::Square, "Square")];

/// The pole set's cells, **labelled by the parameter itself** — each its option's own text, as the
/// host's automation list reads it — each as wide as the longer and no wider (design system §7.3).
fn filter_models(params: &MxmMono03Params) -> Vec<String> {
    let param = binding_for("filtermodel", params).param;
    let last = param.steps().expect("the pole set is stepped");
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

// --------------------------------------------------------------------------------------------
// The cards, as trees (plans/plan-layout-tree.md). Each card is described once — `card` — and that
// one description is both measured (its floor and its height) and drawn, leaf by leaf, through the
// bindings (`paint`). The gaps are the hand layout's own: a card body's `SPACE_3` rhythm between
// siblings, and whatever `add_space` it put on top of that, as a pad.
// --------------------------------------------------------------------------------------------

/// What a leaf of this editor's cards draws. Hashed by what it names, which is also what keeps its
/// widget ids stable when a route appears above it.
#[derive(Clone, Debug, Hash)]
pub enum Leaf {
    Knob(&'static str, Size),
    Waveform,
    /// A per-note switch.
    Toggle(&'static str),
    FilterModel,
    /// A target's route stack, by target.
    Routes(usize),
    FilterResponse,
    AccentSweep,
}

fn knob(params: &MxmMono03Params, id: &'static str, size: Size) -> Node<Leaf> {
    let param = binding_for(id, params).param;
    leaf(
        Leaf::Knob(id, size),
        Kind::Knob {
            name: binding_for(id, params).painted().to_owned(),
            widest: mxm_ui::control::widest_value(|n| param.format(n as f32)),
            size,
            column: 0.0,
        },
    )
}

/// Knobs in **columns of their own width**, left to right.
///
/// `mxm-mono-01` divides a card's full width between its knobs, which is right there because its
/// cards are full of them. Here it was the whole of the negative-space problem: two knobs in a
/// 380-point card sat 190 points apart with a 48-point knob in each, so most of every card was gap.
///
/// The row is instead **capped at the width its knobs need** — diameter plus a gutter each, floored
/// so names still fit — and the columns divide that rather than the card: `ui.columns` inside
/// `set_max_width(needed.min(available))`, as data. Leftover width goes to the panel instead of
/// between two knobs, and below the cap the columns shrink only as far as the widest knob allows.
fn knobs(ui: &Ui, params: &MxmMono03Params, knobs: &[(&'static str, Size)]) -> Node<Leaf> {
    mxm_ui::tree::knob_row(
        ui,
        knobs
            .iter()
            .map(|(id, size)| (*size, knob(params, id, *size)))
            .collect(),
    )
}

fn toggle(params: &MxmMono03Params, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Toggle(id),
        Kind::Toggle {
            label: binding_for(id, params).painted().to_owned(),
        },
    )
}

/// One of §8's displays: it fills the card's width at [`visuals::HEIGHT`], and has no minimum width
/// of its own.
fn display(key: Leaf) -> Node<Leaf> {
    leaf(
        key,
        Kind::Custom {
            min_width: 0.0,
            height: Height::Fixed(visuals::HEIGHT),
            fills: true,
        },
    )
}

/// One target's route stack, `SPACE_3` below what precedes it. A route stack is a composite with a
/// rule of its own — its floor is every route revealed — so it states its size (`stack_size`).
fn routes_leaf(ui: &Ui, params: &MxmMono03Params, t: usize) -> Node<Leaf> {
    let size = mxm_modulation_params::ui::stack_size(
        ui,
        TARGET_NAMES[t],
        &params.routes.each()[t].routes(t),
    );
    pad(
        SPACE_3,
        leaf(
            Leaf::Routes(t),
            Kind::Custom {
                min_width: size.x,
                height: Height::Fixed(size.y),
                fills: true,
            },
        ),
    )
}

/// Paging item `index`'s body, as a tree — Voice, Filter, then the advanced zone ([`TITLES`]) —
/// from the parameters alone: nothing here is disclosed, and the displays' heights are fixed.
pub fn card(ui: &Ui, index: usize, params: &MxmMono03Params) -> Node<Leaf> {
    match index {
        // Oscillator, accent and amplifier, in that order.
        //
        // **One card, not three.** They are three small groups and giving each its own card left
        // the panel tall and mostly gutter; stacked in one they read as the signal reaching the
        // amplifier, which is what they are. The Filter gets a card to itself because it is where
        // the playing happens. The Volume is the instrument's output, so it is in the app bar beside
        // the meter (design system §3.1).
        0 => stack(vec![
            // **One knob row, in the asked-for order: oscillator, then accent**, with the
            // amplifier's routes beneath it. Stacked as three vertical groups instead — a heading's
            // worth of space each — this card came out 75 points *taller* than the three separate
            // cards it replaced. Left to right is the same order and costs one row.
            knobs(ui, params, &[("tune", STANDARD), ("accent", STANDARD)]),
            // The accent circuit's amplifier output, and anything a player routes to the amplifier.
            routes_leaf(ui, params, target::AMPLITUDE),
            // A row of its own — there is no knob beside it to line up with.
            pad(
                SPACE_3,
                leaf(
                    Leaf::Waveform,
                    Kind::Waves {
                        label: Some(binding_for("waveform", params).param.name().to_owned()),
                        count: WAVEFORMS.len(),
                        marks: Vec::new(),
                        beside: None,
                    },
                ),
            ),
            // The two per-step switches, side by side because they are the same kind of thing: the
            // machine's two sequencer buttons. **Slide is here** rather than with the pitch controls
            // for that reason, and because `mxm-mono-01` puts `glide` in the oscillator on the
            // grounds that portamento is played rather than configured. One width for the pair, the
            // longer label's, so the two read as a set.
            pad(
                SPACE_3,
                share(
                    Share::Toggles,
                    row_gap(
                        ui.spacing().item_spacing.x,
                        vec![
                            toggle(params, "slide"),
                            pad_all(0.0, SPACE_3, 0.0, toggle(params, "accentnote")),
                        ],
                    ),
                ),
            ),
            // Brief §8.2. The display this instrument exists to have: consecutive accents climb
            // because a capacitor has not finished discharging, and no control on the panel can
            // show that.
            pad(SPACE_3, display(Leaf::AccentSweep)),
        ]),
        // Where the playing happens, so it gets a card to itself.
        1 => stack(vec![
            // Brief §8.1: the curve comes first, because it is the only place the cutoff's
            // frequency appears at all — the control shows a position, deliberately.
            display(Leaf::FilterResponse),
            // **The envelope is in here, not in a card of its own.** This instrument has exactly
            // one envelope and it is the filter's, which the hardware says by putting ENV MOD and
            // DECAY in the same knob row as CUT OFF FREQ and RESONANCE. A card called *Envelope*
            // invites the question *which one*, and there is no other.
            pad(
                SPACE_3,
                knobs(
                    ui,
                    params,
                    &[
                        ("cutoff", PRIMARY),
                        ("resonance", PRIMARY),
                        ("decay", STANDARD),
                    ],
                ),
            ),
            // Env Mod and the accent circuit's filter output, then anything a player routes to
            // either.
            routes_leaf(ui, params, target::CUTOFF),
            routes_leaf(ui, params, target::RESONANCE),
        ]),
        // Advanced — shown, not hidden. Brief §5. **Two rows of four**, each the collection's knob
        // row (`plans/plan-editor-standard.md` A1): a card is as wide as its content, and a grid
        // whose line count followed the width has no width of its own to hug — hugged, it fell to
        // one knob a line, the tallest card on the panel.
        _ => stack(vec![
            stack_gap(
                SPACE_3 + SPACE_3,
                ADVANCED
                    .chunks(4)
                    .map(|row| {
                        let row: Vec<(&'static str, Size)> =
                            row.iter().map(|id| (*id, COMPACT)).collect();
                        knobs(ui, params, &row)
                    })
                    .collect(),
            ),
            // The pole set is stepped, so it is a segmented control rather than a knob.
            pad(
                SPACE_3,
                leaf(
                    Leaf::FilterModel,
                    Kind::Segmented {
                        label: binding_for("filtermodel", params).painted().to_owned(),
                        options: filter_models(params),
                        beside: None,
                    },
                ),
            ),
        ]),
    }
}

/// Everything a leaf draws with: the parameters and their host, the telemetry the displays read
/// (none of it destructive), and the text-entry buffers.
pub struct Live<'a, 'b> {
    pub params: &'a MxmMono03Params,
    pub telemetry: &'a Telemetry,
    pub setter: &'a ParamSetter<'b>,
    pub entries: &'a mut HashMap<&'static str, Option<String>>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings — so the controls,
/// their gestures and their names are exactly what they were.
pub fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: Rect, live: &mut Live<'_, '_>) {
    let params = live.params;
    let setter = live.setter;
    match *leaf {
        // Inside its column this **is** the column's width, which is what stops a long name being
        // broken to fit a knob's diameter.
        Leaf::Knob(id, size) => {
            binding_for(id, params).knob(ui, tokens, setter, size, rect.width(), live.entries);
        }
        Leaf::Waveform => {
            let waveform = binding_for("waveform", params);
            segmented_waves_named(
                ui,
                tokens,
                waveform.id,
                waveform.param,
                waveform.panel.as_deref(),
                WAVEFORMS,
                None,
                waveform.details,
                setter,
            );
        }
        // A per-note switch: **one button with two states**, not two cells — a binary read as on or
        // off rather than as a choice between two things. It carries the modulation mark: with a
        // step selected in the player it is being set by something other than itself.
        Leaf::Toggle(id) => {
            let bound = binding_for(id, params);
            toggle_labelled(
                ui,
                tokens,
                id,
                bound.param,
                bound.painted(),
                bound.description,
                setter,
                0.0,
            );
        }
        Leaf::FilterModel => {
            let model = binding_for("filtermodel", params);
            let labels = filter_models(params);
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            segmented_named(
                ui,
                tokens,
                model.id,
                model.param,
                model.panel.as_deref(),
                &options,
                model.details,
                setter,
                0.0,
            );
        }
        Leaf::Routes(t) => routes(ui, tokens, t, params, setter, live.entries),
        Leaf::FilterResponse => {
            let (cutoff_hz, env_hz, accent_hz) = reach(params, live.telemetry.sample_rate());
            visuals::filter_response(
                ui,
                tokens,
                params.filter_model.value().into(),
                cutoff_hz,
                params.resonance.value(),
                env_hz,
                accent_hz,
                visuals::HEIGHT,
            );
        }
        Leaf::AccentSweep => visuals::accent_sweep(
            ui,
            tokens,
            &live.telemetry.sweep_history(),
            mxm_mono_03_dsp::accent::SWEEP_CEILING,
        ),
    }
}

/// Draws one section's body: its tree, shown in `ui`. The layout lab (`apps/mxm-layout-lab`, in the
/// private archive since the split) draws these real cards through this.
///
/// `spare` is not spent: a card is as tall as its tree says, and a row of cards is levelled by the
/// paging renderer, which stretches the shorter card's frame. It stays in the signature so the lab's
/// call is unchanged.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ui: &mut Ui,
    tokens: &Tokens,
    section: Section,
    params: &MxmMono03Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    _spare: f32,
) {
    let index = match section {
        Section::Voice => 0,
        Section::Filter => 1,
    };
    let tree = card(ui, index, params);
    let mut live = Live {
        params,
        telemetry,
        setter,
        entries: text_entry,
    };
    tree::show(ui, tokens, &tree, |ui, leaf, rect| {
        paint(ui, tokens, leaf, rect, &mut live);
    });
}

/// The advanced zone's contents, in the order the brief lists them.
pub const ADVANCED: &[&str] = &[
    "slidetime",
    "ampattack",
    "ampdecay",
    "filterattack",
    "accentdecay",
    "accentsweep",
    "drive",
    "bendrange",
];

// --------------------------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------------------------

/// One target's route stack, drawn by the shared routing widget.
fn routes(
    ui: &mut Ui,
    tokens: &Tokens,
    target: usize,
    params: &MxmMono03Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    let group = params.routes.each()[target];
    let entry = text_entry.entry("routes").or_default();
    mxm_modulation_params::ui::stack(
        ui,
        tokens,
        TARGET_NAMES[target],
        // Nothing to drop: no target's name repeats the card it is drawn on.
        TARGET_NAMES[target],
        &group.routes(target),
        entry,
        setter,
    );
}

/// Where the filter's cutoff sits now, and how far the envelope and accent can push it.
///
/// The arithmetic mirrors `Voice::process`. It is duplicated rather than shared because the DSP
/// crate takes plain values and returns samples — exposing a "where would the cutoff be" accessor
/// to satisfy a display is the kind of outward-facing hole
/// mxm-mono-01's `crates/mxm-mono-01-dsp/AGENTS.md` warns about. Being a **declared
/// approximation** is what makes that acceptable, and brief §8 declares it.
fn reach(params: &MxmMono03Params, sample_rate: f32) -> (f32, f32, f32) {
    use mxm_mono_03_dsp::voice::{CUTOFF_HIGH_HZ, CUTOFF_LOW_HZ};

    let position = params.cutoff.value();
    let base = CUTOFF_LOW_HZ * (CUTOFF_HIGH_HZ / CUTOFF_LOW_HZ).powf(position);
    let base = base.min(mxm_mono_03_dsp::filter::max_nominal_cutoff_hz(
        params.filter_model.value().into(),
        sample_rate,
    ));

    // Through the routes, as the voice sums them: the circuit's floor and the Env Mod route at the
    // envelope's peak, then the accent circuit's filter output at the Accent knob's depth — and none of
    // a route's reach while it is absent, so the curve cannot show a reach the patch has not got.
    let routes = &params.routes.cutoff;
    let depth = |present: bool, amount: f32| if present { amount } else { 0.0 };
    let env_octaves = cutoff_octaves(
        1.0,
        depth(routes.env_on.value(), routes.env.value())
            * FULL_SCALE[target::CUTOFF][source::FILTER_ENVELOPE],
    );
    let accent_octaves = env_octaves
        + depth(routes.accent_on.value(), routes.accent.value())
            * params.accent.value()
            * mxm_mono_03_dsp::voice::ACCENT_OCTAVES;
    (
        base,
        base * 2.0f32.powf(env_octaves),
        base * 2.0f32.powf(accent_octaves),
    )
}

/// One parameter's binding, with the sentence §7.1 requires in its tooltip.
///
/// **The descriptions live here because only the plugin has them.** CLAP carries no such field, so
/// a host cannot supply one — which is the concrete reason this editor belongs to the plugin.
/// How the keyboard steps a parameter: a semitone and an octave for the bend reach,
/// a cent and ten for the tune,
/// the owner's ruling of 2026-09-23. Everything not named keeps its own step. See
/// [`crate::editor::binding::StepLaw`].
fn step_law(id: &str) -> super::binding::StepLaw {
    use super::binding::StepLaw;
    match id {
        "bendrange" => StepLaw::Semitones,
        "tune" => StepLaw::Cents,
        _ => StepLaw::Own,
    }
}

pub fn binding_for<'a>(id: &'static str, p: &'a MxmMono03Params) -> Bound<'a> {
    let (param, description, bipolar): (&'a dyn super::binding::ErasedParam, &'static str, bool) =
        match id {
            "tune" => (&p.tune, "Master tuning, in cents.", true),
            "waveform" => (
                &p.waveform,
                "Sawtooth, or the square shaped out of it \u{2014} whose width follows pitch.",
                false,
            ),
            "slide" => (
                &p.slide,
                "Whether this note glides in from the one before. Sequence it per step.",
                false,
            ),
            "cutoff" => (
                &p.cutoff,
                "Where the filter closes. A position, not a frequency \u{2014} the curve above shows where it lands.",
                false,
            ),
            "resonance" => (
                &p.resonance,
                "Emphasis at the corner \u{2014} and how much accent becomes a rising sweep rather than a kick.",
                false,
            ),
            "decay" => (
                &p.decay,
                "How quickly the filter envelope falls \u{2014} the only envelope, and it always moves \
                 the filter a little, even with its route removed. An accented note overrides this.",
                false,
            ),
            "accent" => (
                &p.accent,
                "How hard an accented note hits: louder, a shorter envelope, and a rising sweep.",
                false,
            ),
            "accentnote" => (
                &p.accent_note,
                "Whether this note is accented. Sequence it per step.",
                false,
            ),
            "volume" => (&p.volume, "Output level.", false),

            // ---- Advanced ----
            "slidetime" => (&p.slide_time, "How long a slide takes.", false),
            "ampattack" => (
                &p.amp_attack,
                "How quickly each note's volume rises.",
                false,
            ),
            "ampdecay" => (
                &p.amp_decay,
                "How long each note's volume takes to fall.",
                false,
            ),
            "filterattack" => (
                &p.filter_attack,
                "How quickly the filter sweep rises.",
                false,
            ),
            "accentdecay" => (
                &p.accent_decay,
                "How long an accented note's filter sweep lasts; accents ignore Decay.",
                false,
            ),
            "accentsweep" => (
                &p.accent_sweep,
                "How fast the accent sweep charges and leaks \u{2014} the rate consecutive accents build.",
                false,
            ),
            "drive" => (
                &p.drive,
                "How hard the oscillator drives the filter: more is louder and grittier.",
                false,
            ),
            "filtermodel" => (&p.filter_model, "Which filter sound.", false),
            "bendrange" => (&p.bend_range, "Pitch-bend range, in semitones.", false),

            other => unreachable!("no binding for {other}"),
        };
    Bound {
        id,
        param,
        description,
        panel: None,
        bipolar,
        law: step_law(id),
        stepped: None,
        details: details_of(id),
    }
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3; the owner, 2026-09-27: the cells of a row do not share one sentence).
/// Empty for everything drawn as a knob or a toggle.
fn details_of(id: &str) -> &'static [&'static str] {
    match id {
        "waveform" => &[
            "Bright and buzzy: every harmonic.",
            "Hollow and reedy; its width changes with pitch.",
        ],
        "filtermodel" => &[
            "The instrument's own filter: the squelchy acid sound.",
            "A different diode filter, with a character of its own.",
        ],
        _ => &[],
    }
}

/// Every parameter, bound, in the instrument's own order.
///
/// What `preset.rs` iterates: capture, Init, resolve and the dirty baseline all walk this list, so
/// a parameter missing from [`ALL_IDS`] would silently fall out of every preset — which is why
/// `every_parameter_is_drawn_exactly_once` checks the list against the `Params` derive.
pub fn all_parameters(params: &MxmMono03Params) -> Vec<Bound<'_>> {
    ALL_IDS.iter().map(|id| binding_for(id, params)).collect()
}

/// Every id this editor draws — on the cards, and `volume` in the app bar. One list, so the
/// coverage test and the lookup cannot disagree.
pub const ALL_IDS: &[&str] = &[
    "tune",
    "waveform",
    "slide",
    "cutoff",
    "resonance",
    "decay",
    "accent",
    "accentnote",
    "volume",
    "slidetime",
    "ampattack",
    "ampdecay",
    "filterattack",
    "accentdecay",
    "accentsweep",
    "drive",
    "bendrange",
    "filtermodel",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every parameter appears exactly once in the panel: on a card, in the advanced zone, or in the
    /// app bar.
    ///
    /// **The check the brief's sign-off asks for**, mechanised: a parameter with no control is a
    /// parameter nobody can reach, and one drawn twice is two controls disagreeing about a value.
    /// `editor::tests::the_volume_is_drawn_in_the_app_bar_and_on_no_card` holds these tables to what
    /// is painted.
    #[test]
    fn every_parameter_is_drawn_exactly_once() {
        use nice_plug::prelude::Params;
        let params = MxmMono03Params::default();
        let declared: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            // A routing parameter is drawn by its target's stack, not by this list.
            .filter(|id| !id.starts_with("mod_"))
            .collect();

        let mut drawn: Vec<&str> = super::super::SECTIONS
            .iter()
            .flat_map(|s| s.parameters().iter().copied())
            .collect();
        drawn.extend_from_slice(ADVANCED);
        drawn.push("filtermodel");
        drawn.extend_from_slice(super::super::APP_BAR);

        for id in &declared {
            let count = drawn.iter().filter(|d| *d == id).count();
            assert_eq!(count, 1, "{id} is drawn {count} times, expected once");
        }
        assert_eq!(
            drawn.len(),
            declared.len(),
            "drawn {drawn:?} against declared {declared:?}"
        );
    }

    /// Every binding resolves, which `binding_for`'s `unreachable!` would otherwise turn into a
    /// panic inside a paint call — and a panic there takes the host down with it.
    #[test]
    fn every_drawn_parameter_has_a_binding() {
        let params = MxmMono03Params::default();
        let mut ids: Vec<&str> = super::super::SECTIONS
            .iter()
            .flat_map(|s| s.parameters().iter().copied())
            .collect();
        ids.extend_from_slice(ADVANCED);
        ids.push("filtermodel");
        ids.extend_from_slice(super::super::APP_BAR);
        for id in ids {
            let bound = binding_for(id, &params);
            assert!(!bound.description.is_empty(), "{id} has no description");
            assert!(
                bound.description.ends_with('.'),
                "{id}'s description is not a sentence: {:?}",
                bound.description
            );
        }
    }
}
