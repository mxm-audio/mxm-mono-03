//! mxm-mono-03's editor.
//!
//! Built to `docs/briefs/mxm-mono-03.md`, which is the gating document — this module implements it
//! and does not re-decide it. In particular the brief owns:
//!
//! - **§10's section sequence**, Oscillator · Filter · Envelope · Accent · Amplifier. That sequence
//!   is the hardware's own knob order *and* the signal flow, which is why keeping it is allowed
//!   where copying the panel is not. [`SECTIONS`] is written in that order so a reordering is a
//!   visible diff rather than a drift.
//! - **§5's two zones**: the instrument above a labelled divider, the machine's fixed constants
//!   below it on `surface-2`. **Shown, not disclosed** — this is the deliberate departure from
//!   `mxm-mono-01`, and its reason is that hiding a third of a small instrument costs more than it
//!   saves.
//! - **§6's category/card inventory**, with derived musician pages and developer-only Parameters.
//! - **§7's identity accent** and **§8's two displays**.
//!
//! # It is a panel, not a window
//!
//! [`panel`] takes a `Ui` and draws into it. It does not create a window, run an event loop, or own
//! a swapchain. That is what lets the same code be the plugin's CLAP editor and, later, a
//! standalone harness's contents.
//!
//! # Gestures
//!
//! Every edit is bracketed: `begin_set_parameter`, `set_parameter_normalized`, `end_set_parameter`,
//! in exactly one place — [`binding::Bound::apply`]. An unclosed gesture leaves a host's automation
//! lane latched, which is silent until someone records over it, and it breaks the player's step
//! editing outright.

pub mod binding;
pub mod sections;
mod visuals;

use std::collections::HashMap;
use std::sync::Arc;

use egui::Ui;
use mxm_ui::space::SPACE_5;
use mxm_ui::theme::Tokens;
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use crate::params::MxmMono03Params;
use crate::telemetry::Telemetry;

/// Opening size; every derived page is checked by the real-panel paging proof. Re-derived at the
/// routing conversion, where the route stacks made the budget hug taller.
const REFERENCE: (u32, u32) = (797, 558);

// The two primary cards remain indivisible; Advanced is a third paging item.
/// The five sections of brief §10, **in sequence**.
///
/// This is the instrument's information architecture and the order is the contract. It is also the
/// hardware's knob order — TUNING, CUT OFF FREQ, RESONANCE, ENV MOD, DECAY, ACCENT — which is the
/// signal flow, and the reason §9 keeps the sequence while removing everything about the panel's
/// appearance.
pub const SECTIONS: &[Section] = &[Section::Voice, Section::Filter];

/// The narrowest the window may be: the wider of one card — the widest floor — with the panel's
/// gutters, and the app bar at its last compact step. `the_minimum_holds_the_widest_floor_and_its_gutters`
/// holds it against the computed floors, so a floor that grows past it fails there rather than as a
/// card wider than the window; the bar decides it today, and `the_app_bar_holds_in_the_minimum_window`
/// measures that.
const MINIMUM: (u32, u32) = (446, 320);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Voice,
    Filter,
}

impl Section {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Voice => "Voice",
            Self::Filter => "Filter",
        }
    }

    /// Which parameters this card draws, so a coverage test can ask rather than assume.
    ///
    /// Only tests read it, and that is the point: the alternative is a test that repeats the
    /// layout's own list, which then agrees with itself while disagreeing with the panel.
    #[cfg(test)]
    pub(crate) const fn parameters(self) -> &'static [&'static str] {
        match self {
            Self::Voice => &["tune", "waveform", "slide", "accent", "accentnote"],
            Self::Filter => &["cutoff", "resonance", "decay"],
        }
    }
}

/// Which parameters the app bar draws, beside the level meter (design system §3.1), for the same
/// coverage tests [`Section::parameters`] serves.
#[cfg(test)]
pub(crate) const APP_BAR: &[&str] = &["volume"];

/// The keyboard cursor's card for the app bar's Volume, outside the paging keys 0…2.
const VOLUME_CARD: u64 = 64;

/// Builds the editor. Called from `Plugin::editor`.
pub fn create(params: Arc<MxmMono03Params>, telemetry: Arc<Telemetry>) -> Option<MxmMono03Editor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );

    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: "mxm-mono-03".to_owned(),
            // **Resizable, because the layout reflows** (§3.4, §4.3). The floor is one card wide
            // plus the panel's gutters: below it a card would be drawn narrower than its own
            // controls, which no arrangement can fix. Zoom is still chosen from the app bar, never
            // derived from the window — deriving it feeds back and the window jitters.
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmMono03App::new(params, telemetry),
    )
}

/// The editor type the plugin exposes.
pub type MxmMono03Editor = nice_plug_egui::EguiEditor<MxmMono03App>;

/// The editor's own state: what the plugin does not own and the host does not need.
pub struct MxmMono03App {
    params: Arc<MxmMono03Params>,
    telemetry: Arc<Telemetry>,
    /// Set in `build`, because that is where nice-plug hands it over.
    gui_context: Option<GuiContext>,
    /// Which of [`VIEWS`] is showing. `0` is `Synth`, so the brief's default costs nothing.
    view: usize,
    /// Open text-entry buffers, keyed by parameter id. §7.1 requires direct text entry on every
    /// continuous control.
    text_entry: HashMap<&'static str, Option<String>>,
    /// The preset library and everything the browser needs across frames.
    presets: PresetUi,
    /// Where the keyboard is: a card, and a parameter inside it. Transient, like the text
    /// buffers — it is not a parameter and nothing durable reads it.
    nav: mxm_ui::navigation::State,
}

/// The app bar's preset controls and what they need between frames — `mxm-preset`'s, one for
/// every instrument.
pub use mxm_preset::PresetUi;

impl MxmMono03App {
    pub fn new(params: Arc<MxmMono03Params>, telemetry: Arc<Telemetry>) -> Self {
        let params_for_presets = Arc::clone(&params);
        Self {
            params,
            telemetry,
            gui_context: None,
            view: 0,
            text_entry: HashMap::new(),
            presets: PresetUi::new(params_for_presets.as_ref()),
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmMono03App {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        // Once, on open. A `Ui` reads a clone of the style it was built with, so applying mid-frame
        // would show as an unstyled flash on the first one.
        mxm_ui::theme::apply(&egui_ctx);
        mxm_ui::typography::apply(&egui_ctx);

        // Light by default, overridable with `MXM_EDITOR_THEME`. The reasoning, and why the
        // default is not `System`, lives on `mxm_ui::theme::preference`.
        egui_ctx.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = self.gui_context.clone() else {
            // No context means `build` did not run, which should be impossible. Draw nothing rather
            // than panicking: a panic in a plugin's paint call takes the host with it.
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &gui_context.param_setter(),
            &mut self.view,
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }

    fn editor_closed(&mut self) {
        // Brief §8: the displays stop when the editor closes. Dropping the context is also what
        // releases the host's callbacks, which nice-plug asks for explicitly.
        self.gui_context = None;
    }
}

/// The whole editor, as a panel.
///
/// Free function over borrowed state rather than a method, so a harness can draw it without an
/// `App` — the same shape `mxm-mono-01` uses for the same reason.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ui: &mut Ui,
    params: &MxmMono03Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    view: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    let tokens = &tokens_for(ui);

    // A frame every 50 ms while the editor is open: the level meter changes between input events,
    // and so does a developer-channel request, which a frame that waited for the pointer would
    // strand on a view where nothing animates.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));

    // The collection's developer channel (`plugins/AGENTS.md`): the view. This editor discloses
    // nothing — Advanced is shown, not hidden — so the expander request has nothing to reach.
    // One question, and both layers suspend on it: the paging renderer's `hold` and the cursor's
    // `inert` both ask whether another surface owns this frame's keyboard.
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    mxm_ui::paging::editor::developer_request(ui.ctx(), view, telemetry.take_view_request());

    // **The keyboard cursor moves before anything is drawn**, so a navigation arrow is consumed
    // here rather than also walking egui's own focus ring. It reads the registry and the exact
    // card rectangles the previous frame built, and it resolves the developer-view request first,
    // because which surface this frame is deciding who owns its keyboard.
    if *view == mxm_ui::paging::PARAMETERS {
        // This surface has no cards. Stop rather than merely hiding the outline, or its controls
        // lose their legacy bare-arrow editing to an invisible stale musician cursor.
        mxm_ui::navigation::stop(ui.ctx());
    } else {
        // The app bar's Volume is above the paging renderer, which cannot report it, so it is a
        // bar card the cursor reaches first.
        mxm_ui::navigation::paged_with_bar(ui.ctx(), nav, busy, &[VOLUME_CARD]);
    }
    if let Some(open) = telemetry.take_browser_request() {
        presets.set_browser_open(open);
    }
    // A theme, by index. Applied and not stored: this channel is how a screenshot run and a test
    // reach a state, and neither should overwrite the choice made in the control.
    if let Some(index) = telemetry.take_theme_request()
        && let Some(preference) = mxm_ui::theme::from_index(index)
    {
        ui.ctx().set_theme(preference);
    }
    let _ = telemetry.take_disclosure_request();

    // §3.1: slots 2-4 are the patch — the preset browser — and the right edge carries the output
    // state, the output control and the scale control.
    let peak = telemetry.take_peak();
    let clipped = telemetry.clipped();
    mxm_ui::AppBar::new("mxm-mono-03").show_with(
        ui,
        tokens,
        |ui| mxm_preset::ui::preset_row(ui, tokens, params, setter, presets),
        |ui| {
            if mxm_ui::shell::level_meter(ui, tokens, peak, clipped) {
                telemetry.clear_clip();
            }
            // Design system §3.1 slot 6: the instrument's output control sits beside its meter,
            // not in the Voice card among the controls that shape a note.
            mxm_ui::navigation::bar_card(ui, VOLUME_CARD, |ui| {
                ui.scope(|ui| {
                    sections::binding_for("volume", params)
                        .slider_inline(ui, tokens, setter, text_entry, 96.0);
                })
                .response
                .rect
            });
            mxm_ui::shell::zoom_control(ui);

            // §3.1 slot 5, and the same place the player keeps it: at the left end of the bar's
            // right-hand group. What the person picks is remembered for every MXM editor, so the
            // next one to open agrees with this one.
            mxm_ui::shell::editor_theme_control(ui);
        },
    );

    // A name being typed, or the last thing that went wrong. Below the bar rather than in it: §3.1
    // says *do not turn the app bar into a second parameter panel*, and a text field that appears
    // in a bar moves everything beside it.
    mxm_preset::ui::overlays(ui, tokens, params, setter, presets);

    // **The Parameters view has no tab.** It is the complete generated list, and an editor whose
    // own interface reaches every control does not need a second way to the same parameters in
    // front of a musician every day. It stays reachable: the developer channel still requests it
    // by index, which is what the CLI and a host's automation list use it for.
    // Navigation is derived by the paging renderer.

    // **The canvas, and the gutter.** Without this the region below the view bar is never painted
    // at all — the window's own clear colour shows through, which is black, and the cards sit hard
    // against the frame with no margin. Both were visible in the first build and neither is
    // subtle; `mxm-mono-01` has had this panel from the start and it is why its background is grey.
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            if *view == mxm_ui::paging::PARAMETERS {
                parameters_view(ui, tokens, params, setter, text_entry);
            } else {
                paged_view(ui, tokens, params, telemetry, setter, text_entry);
            }
        });
}

/// Every paging item — Voice, Filter and the advanced zone, `sections::TITLES` — each floor computed
/// from its card's tree in `ui`'s fonts every frame, and each card exactly as wide as that floor:
/// its ceiling is its floor (`plans/plan-editor-standard.md` A1), with no usability minimum (A2).
pub fn page_items(ui: &Ui, params: &MxmMono03Params) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::{
        flow::Card,
        paging::{Category as C, Item, Key},
    };
    sections::TITLES
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let tree = sections::card(ui, i, params);
            let floor = mxm_ui::tree::card_floor(ui, title, &tree);
            Item {
                key: Key(i as u64),
                card: Card::new(title, floor).capped(floor),
                category: if i == 0 { C::Generators } else { C::Tone },
                kind: title,
            }
        })
        .collect()
}

fn paged_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMono03Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let items = page_items(ui, params);
    let text_editing = entries.values().any(Option::is_some);
    // Nothing a card draws reads telemetry destructively: the peak is the app bar's, read above.
    let mut live = sections::Live {
        params,
        telemetry,
        setter,
        entries,
    };
    mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        &[],
        text_editing,
        &mut |ui, i| sections::card(ui, i, params),
        &mut |ui, _, leaf, rect| sections::paint(ui, tokens, leaf, rect, &mut live),
    );
}

/// The paging items as the editor computes them, from a context set up as an editor's is — three
/// passes in, so the weighted font cuts are bound — for tests, which have no editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let params = MxmMono03Params::default();
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, &params);
        });
        output.textures_delta.clear();
    }
    items
}

/// The cards' floors in paging order, as [`test_items`] computes them.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

/// §6's `Parameters` view: every parameter as a slider, which is the testing surface.
///
/// **Editable, not a readout.** `mxm-mono-01`'s is the pattern and the reason is that this is where
/// a parameter gets driven when the panel is not the thing under test; a list of text would be a
/// worse version of what a host already shows.
///
/// This diagnostic list scrolls independently of musician paging.
fn parameters_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmMono03Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    mxm_ui::shell::scroll_list(ui).show(ui, |ui| {
        let columns = if ui.available_width() >= 1000.0 { 3 } else { 2 };
        // **In the instrument's order**, not the declaration's: a flat list in a different order
        // would be a second, contradictory information architecture for the same parameters.
        // Then every route, drawn bipolar where it is an amount: a route at zero is no modulation,
        // and a half-filled track would read as half on.
        let entries: Vec<_> = sections::ALL_IDS
            .iter()
            .map(|id| sections::binding_for(id, params))
            .chain(
                params
                    .routes
                    .parameters()
                    .into_iter()
                    .map(|(id, param)| binding::Bound {
                        id,
                        param,
                        description: "A modulation route's presence or depth.",
                        bipolar: !id.ends_with("on"),
                        law: binding::StepLaw::Own,
                        panel: None,
                        stepped: None,
                        details: &[],
                    }),
            )
            .collect();
        let per_column = entries.len().div_ceil(columns);

        ui.columns(columns, |uis| {
            for (index, chunk) in entries.chunks(per_column).enumerate() {
                let Some(column) = uis.get_mut(index) else {
                    continue;
                };
                for entry in chunk {
                    entry.slider(column, tokens, setter, text_entry);
                }
            }
        });
    });
}

/// The collection's tokens, with this instrument's identity accent (brief §7).
///
/// The hue itself lives in `mxm_ui::theme` — a colour literal here would be a token that had
/// escaped the shared crate, which mxm-kit's `crates/ui/AGENTS.md` forbids outright.
fn tokens_for(ui: &Ui) -> Tokens {
    let dark = ui.visuals().dark_mode;
    let base = if dark { mxm_ui::DARK } else { mxm_ui::LIGHT };
    base.with_identity(mxm_ui::theme::ACID_LIME, dark)
}

#[cfg(test)]
mod tests {
    use mxm_plugin_test::{opening_size, paging_checks};

    /// **The editor opens at the quarter-4K budget, hugged** — the owner's rule, 2026-09-09. The budget
    /// is the most room an editor may ask for, so laying the panel out there shows as many modules as
    /// it ever will; taking the slack away is the whole of the size.
    #[test]
    fn the_opening_size_is_the_budget_hugged() {
        let params = MxmMono03Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::is_the_budget_hugged(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &REVEAL,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
    /// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
    #[test]
    fn the_app_bar_holds_in_the_minimum_window() {
        let params = MxmMono03Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::bar_holds_from_the_minimum(
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    use mxm_plugin_test::keyboard_checks;

    /// What this editor keeps behind a disclosure, opened so the reachability check sees it.
    /// Nothing here: every control is on a card.
    const REVEAL: fn(&egui::Context) = |_| {};

    /// Every route present, as the `‹ modulate ›` menu would add them one at a time. An absent route
    /// draws nothing at all, so a check at the init patch reaches three of fifty-four.
    fn reveal_every_route(params: &MxmMono03Params) {
        use nice_plug::params::InternalParamMut;
        for group in params.routes.each() {
            for presence in [
                &group.key_on,
                &group.vel_on,
                &group.wheel_on,
                &group.press_on,
                &group.bend_on,
                &group.env_on,
                &group.accent_on,
                &group.accentlevel_on,
                &group.osc_on,
            ] {
                // SAFETY: a test owns these parameters outright; nothing else holds them.
                unsafe { presence._internal_set_plain_value(true) };
            }
        }
    }

    /// Every parameter the panel should draw: the cards' own, and a presence and an amount for every
    /// route that is present.
    fn drawn_ids(params: &MxmMono03Params) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = sections::all_parameters(params)
            .iter()
            .map(|bound| bound.id)
            .collect();
        for (t, group) in params.routes.each().into_iter().enumerate() {
            for (s, present) in group.presences(t).into_iter().enumerate() {
                if present {
                    let (amount, presence) = crate::routes::ROUTE_IDS[t][s];
                    ids.push(amount);
                    ids.push(presence);
                }
            }
        }
        ids
    }

    /// The rollout's own failure mode: a control whose `navigation::at` scope was forgotten paints
    /// exactly as before and is simply unreachable from the keyboard. Nothing else would say so.
    /// **Two frames** (`plan-modulation-routing.md` §8a): the init patch, and every route present.
    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        reach_and_operate(false);
    }

    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_route_revealed() {
        reach_and_operate(true);
    }

    fn reach_and_operate(revealed: bool) {
        let params = MxmMono03Params::default();
        if revealed {
            reveal_every_route(&params);
        }
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let ids = drawn_ids(&params);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &test_items(),
            keyboard_checks::Coverage::Exactly(&ids),
            &REVEAL,
            &host,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// **The Volume is in the app bar, and every card draws what its table says** — read off the
    /// painted panel rather than off the tables. Every card is requested in turn with every route
    /// revealed, and each parameter the keyboard registry recorded is filed under the card it was
    /// painted in; the routes, which their targets' stacks draw, are set aside. The app bar's card
    /// must hold [`APP_BAR`] and nothing else, each paging card exactly its own list, and no
    /// parameter may be painted in two cards.
    #[test]
    fn the_volume_is_drawn_in_the_app_bar_and_on_no_card() {
        use std::collections::{BTreeMap, BTreeSet};

        let params = MxmMono03Params::default();
        reveal_every_route(&params);
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        let session =
            keyboard_checks::Session::new(egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32));
        let items = test_items();
        assert!(
            items.iter().all(|item| item.key.0 != VOLUME_CARD),
            "the app bar's card key collides with a paging key"
        );

        let mut on_card: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
        let mut cards_of: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        for item in &items {
            mxm_ui::paging::editor::request_card(session.context(), item.key);
            session.settle(&mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            for spot in mxm_ui::navigation::spots(session.context()) {
                if spot.key.starts_with("mod_") {
                    continue;
                }
                on_card
                    .entry(spot.card)
                    .or_default()
                    .insert(spot.key.clone());
                cards_of.entry(spot.key).or_default().insert(spot.card);
            }
        }

        let set = |ids: &[&str]| {
            ids.iter()
                .map(|id| (*id).to_owned())
                .collect::<BTreeSet<_>>()
        };
        let mut advanced = sections::ADVANCED.to_vec();
        advanced.push("filtermodel");
        assert_eq!(
            on_card.get(&VOLUME_CARD),
            Some(&set(APP_BAR)),
            "the app bar draws Volume and nothing else"
        );
        assert_eq!(on_card.get(&0), Some(&set(Section::Voice.parameters())));
        assert_eq!(on_card.get(&1), Some(&set(Section::Filter.parameters())));
        assert_eq!(on_card.get(&2), Some(&set(&advanced)));
        for (id, cards) in &cards_of {
            assert_eq!(cards.len(), 1, "{id} is painted in cards {cards:?}");
        }
    }

    #[test]
    fn every_dynamic_page_fits_and_every_card_is_reachable() {
        let params = MxmMono03Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0;
        let mut entries = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        paging_checks::verify(
            &test_items(),
            &[
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                egui::vec2(1880.0, 1040.0),
                egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            ],
            |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut entries,
                    &mut presets,
                    &mut nav,
                )
            },
        );
    }
    use super::*;
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::prelude::{PluginApi, PluginState};

    /// The editor reports edits through a `ParamSetter`; laying it out makes none, so every method
    /// here is unreachable and exists only to satisfy the trait.
    struct NoHost;

    impl nice_plug::context::gui::GuiContextInner for NoHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// Lays the whole editor out and returns the context, so a test can read what it recorded.
    fn lay_out(view: usize, width: f32, height: f32) -> egui::Context {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);

        let params = MxmMono03Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = view;
        let mut text_entry = HashMap::new();
        // A library rooted nowhere: a layout test must never touch the real config directory.
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };
        for _ in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            output.textures_delta.clear();
        }
        ctx
    }

    /// Lays the whole editor out at a given size and reports the height it actually needed.
    fn measure(view: usize, width: f32, height: f32) -> f32 {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);

        let params = MxmMono03Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = view;
        let mut text_entry = HashMap::new();
        // A library rooted nowhere: a layout test must never touch the real config directory.
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };

        // Three passes: the first sizes text galleys, and the column-height matching needs one more
        // frame after that to settle, because it enforces this frame what it measured last.
        let mut used = 0.0;
        for _ in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            // epaint asserts on a dropped `TexturesDelta` that was never applied, and there is no
            // renderer here to apply one. Clearing it is the documented way out.
            output.textures_delta.clear();
            used = ctx.globally_used_rect().height();
        }
        used
    }

    /// **The scar this editor inherited, turned into a number.**
    ///
    /// `mxm-mono-01`'s height was specified before its panel existed, the cards needed more, and
    /// with a fixed window the difference is not a scrollbar — it is controls cut off the bottom
    /// with no way to reach them. This editor shipped its first build with exactly that defect: the
    /// whole advanced zone was below the fold and the owner's first question was *where are the
    /// advanced parameters?*
    #[test]
    fn the_synth_view_fits_the_editor() {
        let used = measure(0, REFERENCE.0 as f32, REFERENCE.1 as f32);
        assert!(
            used <= REFERENCE.1 as f32,
            "the Synth view needs {used} points of height in a {} point window; it will be clipped, \
             and the window cannot be resized to reveal it",
            REFERENCE.1
        );
    }

    /// **The two cards end level, and neither is taller than the row needs.**
    ///
    /// This replaced *the cards end level by growing the curve*. The curve absorbed a column's
    /// levelling deficit; a row hands out none, because every card in it is already the row's
    /// height. The property that matters is unchanged and is now checked where a person sees it —
    /// on the drawn rectangles.
    #[test]
    fn the_two_cards_share_one_bottom_edge() {
        // Component geometry: paging may separate Voice and Tone at the opening height.
        let ctx = lay_out(0, REFERENCE.0 as f32, 20000.0);
        let placed = paging_checks::all_rects(&ctx, 2);

        // Side by side at the reference width, so one row and one bottom edge.
        assert!(
            (placed[0].top() - placed[1].top()).abs() < 1.0,
            "the cards start at {:?} and {:?}",
            placed[0].top(),
            placed[1].top()
        );
        assert!(
            (placed[0].bottom() - placed[1].bottom()).abs() < 1.0,
            "the cards end at {:?} and {:?}",
            placed[0].bottom(),
            placed[1].bottom()
        );
    }

    /// Narrow enough and they stack, each at its own height — the wrap, and the proof that the
    /// levelling above is a row's doing and not two cards that happen to match.
    #[test]
    fn the_cards_stack_when_the_window_is_too_narrow_for_both() {
        let widest = test_floors()[..2].iter().copied().fold(0.0_f32, f32::max);
        let ctx = lay_out(0, widest + 40.0, 20000.0);
        let placed = paging_checks::all_rects(&ctx, 2);
        assert!(
            placed[1].top() >= placed[0].bottom() - 1.0,
            "the Filter should have wrapped under the Voice: {placed:?}"
        );
    }

    /// **The window's minimum is one card wide**, the widest computed floor, plus the panel's two
    /// gutters (design system §4.3).
    #[test]
    fn the_minimum_holds_the_widest_floor_and_its_gutters() {
        let widest = test_floors().into_iter().fold(0.0_f32, f32::max);
        assert!(
            widest + 2.0 * SPACE_5 <= MINIMUM.0 as f32,
            "the minimum {} does not hold the widest floor {widest:.1} and its gutters",
            MINIMUM.0
        );
    }

    /// `Parameters` is allowed to be taller: it is the one view that scrolls.
    #[test]
    fn the_parameters_view_lays_out_without_panicking() {
        assert!(
            measure(
                mxm_ui::paging::PARAMETERS,
                REFERENCE.0 as f32,
                REFERENCE.1 as f32
            ) > 0.0
        );
    }

    /// The section order is the contract, so it is asserted rather than assumed.
    ///
    /// It is the hardware's knob order and the signal flow at once, and brief §10 states it. A
    /// reordering that was not meant is a failing test rather than a thing somebody notices later.
    #[test]
    fn the_sections_are_in_the_briefs_order() {
        let titles: Vec<&str> = SECTIONS.iter().map(|s| s.title()).collect();
        assert_eq!(titles, ["Voice", "Filter"]);
    }

    /// Card names come from the collection's shared vocabulary; only **Accent** is new.
    ///
    /// Brief §10a: somebody who has used one instrument in the collection should not have to learn
    /// new words for the same parts.
    ///
    /// *Voice* is not one of `mxm-mono-01`'s card names, but it is the collection's word all the
    /// same — mxm-kit's `docs/MXM_CONTROL_MAP.md` has carried a **Voice** page since before either
    /// editor existed, holding glide, bend range and output. The same parts, under the same name.
    #[test]
    fn card_names_come_from_the_collections_vocabulary() {
        const KNOWN: &[&str] = &[
            "LFO",
            "Oscillator",
            "Mixer",
            "Filter",
            "Amplifier",
            "Envelope",
            "Voice",
        ];
        for title in SECTIONS.iter().map(|s| s.title()) {
            assert!(
                KNOWN.contains(&title),
                "{title} is a card name the collection does not use anywhere else"
            );
        }
    }

    use mxm_plugin_test::tree_checks;

    /// A host that **applies** what a `ParamSetter` reports, which `NoHost` deliberately does not —
    /// for a test that has to move a parameter through the setter.
    struct ApplyingHost;

    impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
            // SAFETY: the parameter outlives the setter, which borrows this host for the call.
            unsafe { param._internal_set_normalized_value(normalized) };
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// Every card, in every state that changes what it holds, passes the layout tree's checks
    /// (plans/plan-layout-tree.md §4.3, `tree_checks::card`): its floor holds its content with
    /// nothing painted outside the card, the content floor is exact, the height its tree states is
    /// the height it draws, and every leaf stays in the room it was given — the Advanced grid at its
    /// floor and sixty points wider, so at two line counts.
    ///
    /// The states are this editor's structural-state matrix: the init patch, where the machine's own
    /// three routes are the only rows; every route revealed at full negative depth — where a reading
    /// carries its sign and every digit, the widest text a row can show; and a note sounding, a run
    /// of accents climbing in the sweep display. Nothing here is disclosed, and both displays are a
    /// fixed height, so telemetry changes only what they paint.
    #[test]
    fn every_card_passes_the_tree_checks_in_every_state() {
        let floors = test_floors();
        for state in ["init", "every route revealed", "a note sounding"] {
            let params = MxmMono03Params::default();
            let host = ApplyingHost;
            let setter = ParamSetter::new(&host);
            if state == "every route revealed" {
                reveal_every_route(&params);
                for (t, group) in params.routes.each().into_iter().enumerate() {
                    for route in &group.routes(t) {
                        route.amount.set(&setter, 0.0);
                    }
                }
            }
            let telemetry = Telemetry::default();
            if state == "a note sounding" {
                for step in 0..crate::telemetry::SWEEP_HISTORY {
                    telemetry.publish_sweep(
                        (step % 32) as f32 / 32.0 * mxm_mono_03_dsp::accent::SWEEP_CEILING,
                    );
                }
            }
            for (index, title) in sections::TITLES.iter().enumerate() {
                let mut entries = HashMap::new();
                let mut live = sections::Live {
                    params: &params,
                    telemetry: &telemetry,
                    setter: &setter,
                    entries: &mut entries,
                };
                tree_checks::card(
                    &|_| {},
                    state,
                    title,
                    floors[index],
                    &|ui| sections::card(ui, index, &params),
                    &mut |ui, leaf, rect| {
                        sections::paint(ui, &tokens_for(ui), leaf, rect, &mut live);
                    },
                );
            }
        }
    }

    /// **The layout lab's entry draws each card as its tree.** `apps/mxm-layout-lab` (private
    /// archive) calls `sections::draw` with its own state and a `spare` of zero; the wrapper builds
    /// the card's tree and shows it, so the card the lab draws is as tall as that tree says, at the
    /// floor the paging renderer is given.
    #[test]
    fn the_layout_labs_entry_draws_each_card_as_its_tree() {
        let floors = test_floors();
        for (index, section) in SECTIONS.iter().enumerate() {
            let params = MxmMono03Params::default();
            let telemetry = Telemetry::default();
            let host = NoHost;
            let setter = ParamSetter::new(&host);
            let mut text_entry = HashMap::new();
            let ctx = tree_checks::context(&|_| {});
            let mut result = (0.0, 0.0);
            for _ in 0..3 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let mut column =
                        ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(floors[index], 4000.0),
                        )));
                    let tokens = tokens_for(ui);
                    result =
                        mxm_ui::ModuleCard::new(section.title()).show(&mut column, &tokens, |ui| {
                            let tree = sections::card(ui, index, &params);
                            let stated =
                                tree.height(ui, tree.drawn_width(ui, ui.available_width()));
                            let top = ui.min_rect().top();
                            sections::draw(
                                ui,
                                &tokens,
                                *section,
                                &params,
                                &telemetry,
                                &setter,
                                &mut text_entry,
                                0.0,
                            );
                            (stated, ui.min_rect().bottom() - top)
                        });
                });
                output.textures_delta.clear();
            }
            let (stated, drawn) = result;
            assert!(
                (stated - drawn).abs() < 0.5,
                "{}: the tree says {stated:.1}, the lab's entry drew {drawn:.1}",
                section.title()
            );
        }
    }

    /// Every page at the opening size, light and dark, for the owner's review of the layout-tree
    /// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-mono-03/<tag>/`, where
    /// `MXM_PICTURES` names the tag — `before` on the unconverted editor, `after` on the tree.
    ///
    /// `MXM_PICTURES=after cargo test -p mxm-mono-03 --lib tree_pictures -- --ignored`
    #[test]
    #[ignore = "renders through wgpu; run by hand"]
    fn tree_pictures() {
        let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/layout-tree/mxm-mono-03")
            .join(tag);
        let params = MxmMono03Params::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut view = 0usize;
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        tree_checks::pictures(
            &|_| {},
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &dir,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut view,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }
}
