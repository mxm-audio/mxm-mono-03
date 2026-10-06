//! mxm-mono-03 — monophonic acid bass voice.
//!
//! Architecture inspired by the Roland TB-303: one oscillator switching between sawtooth and a
//! square shaped out of it, a diode ladder, two attack-decay envelopes with no sustain, and the
//! accent circuit that is most of what the machine actually sounds like. Not affiliated with or
//! endorsed by Roland.
//!
//! This file is the plugin shell: identity, parameter plumbing, and MIDI. All the signal processing
//! lives in `mxm-mono-03-dsp`, which knows nothing about nice-plug.
//!
//! # Accent and slide are per-note switches, and the gate is separate
//!
//! The hardware gives every sequencer step an accent button and a slide button, so both are
//! parameters here — that is the only route a host has for reaching a per-step decision.
//!
//! What is **not** a parameter is whether the envelopes retrigger. That follows the gate: a tie
//! makes a host emit the new note **before** releasing the old one, so the plugin sees an overlap,
//! and an overlap means the gate never closed. The voice reads the two separately, which is why a
//! slide can never produce a silent note.
//!
//! The consequence for note tracking: a note-off or a choke for a note that is **not** the one
//! sounding is ignored, because it belongs to the note the new one already replaced.

/// The plugin's name, and the **only** place it is written in this crate.
///
/// Everything else that names the instrument derives from here: [`NAME`], which the host shows, and
/// [`CLAP_ID`], which it remembers. A rename is this line.
///
/// A macro rather than a `const` because [`CLAP_ID`] is built with `concat!`, which takes literals.
macro_rules! plugin_name {
    () => {
        "mxm-mono-03"
    };
}

/// What the host displays.
pub const NAME: &str = plugin_name!();

/// The permanent CLAP identifier.
///
/// **Deliberately assembled from [`plugin_name!`] and not from `CARGO_PKG_NAME`.** Deriving it from
/// the package name would mean a future `git mv` of this directory silently changed the plugin's
/// permanent identity — no compile error, no failing test, and every preset and saved project
/// written under the old id orphaned. Renaming the plugin is a deliberate act that edits
/// `plugin_name!` above: one line, one decision.
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

// Public for `apps/mxm-layout-lab` on the `dynamic-layout` branch: the lab draws these real
// cards outside a host. Nothing else about them changes, and the shipped cdylib is unaffected.
pub mod editor;
pub mod params;
pub mod preset;
pub mod routes;
pub mod telemetry;

use mxm_mono_03_dsp::routing::Routing;
use mxm_mono_03_dsp::voice::{Params as VoiceParams, Voice};
use nice_plug::prelude::*;
use params::MxmMono03Params;
use std::sync::Arc;

/// Upper bound on how many samples are rendered between event checks.
///
/// Splitting only on events is not enough: a buffer containing no MIDI at all would otherwise
/// become one arbitrarily long block, and per-sample modulation would be the only thing keeping it
/// honest.
const MAX_BLOCK_SIZE: usize = 64;

/// MIDI channels, for the per-channel bend state.
const NUM_CHANNELS: usize = 16;

/// **The collection's developer channel, off unless asked for** (`plugins/AGENTS.md`). With
/// `MXM_DEV_CC` set in the plugin's process environment when an instance is made, CC 119 selects the
/// category (0–5) or Parameters (127); CC 118 opens (≥ 64) or closes its expander, through telemetry
/// atomics; the DSP reads nothing. It exists so a script, a screenshot run or an AI can put the
/// editor in a state CLAP gives a host no way to ask for — through the player, `mxm-cli cc 119 1`.
const DEV_VIEW_CC: u8 = 119;
const DEV_DISCLOSURE_CC: u8 = 118;
const DEV_BROWSER_CC: u8 = 117;
const DEV_THEME_CC: u8 = 116;
const DEV_CC_ENV: &str = "MXM_DEV_CC";

pub struct MxmMono03 {
    params: Arc<MxmMono03Params>,
    voice: Voice,

    /// The note currently sounding, if any.
    ///
    /// Held so a note-off or a choke can be matched against it: during a slide the host emits the
    /// new note before releasing the old one, and that stale release must not silence the note
    /// that replaced it.
    sounding: Option<u8>,

    /// Pitch bend per channel, in `-1..=1`. CLAP delivers `0..=1` with 0.5 centred.
    bend: [f32; NUM_CHANNELS],
    /// The mod wheel per channel, `0..=1` — a routing source only: the machine had no wheel.
    wheel: [f32; NUM_CHANNELS],
    /// Channel pressure per channel, `0..=1` — a routing source only.
    pressure: [f32; NUM_CHANNELS],
    /// Channel of the note currently sounding, so the right bend applies.
    active_channel: u8,
    /// Per-note pitch expression for the sounding note, in semitones.
    ///
    /// One voice, so one value: a `PolyTuning` is applied only while it names the note actually
    /// sounding, and a new note-on clears it. **A note-off does not** — the offset the note was
    /// bent to holds through its release, where zeroing it would snap the pitch back mid-tail.
    expression_semitones: f32,

    /// The topology the last block ran, and this sample's route amounts. Kept across blocks so a
    /// route that has just become present is known, and its smoother snapped
    /// (`routes::Routes::topology_from`).
    routing: Routing,

    sample_rate: f32,

    /// DSP -> editor, atomics only. Held even with no editor open: `activate` publishes the sample
    /// rate, and a few atomic stores per block are not worth branching on.
    telemetry: Arc<telemetry::Telemetry>,
    /// Whether the developer channel is on: `DEV_CC_ENV` was set when this instance was made.
    dev_cc: bool,
}

impl Default for MxmMono03 {
    fn default() -> Self {
        Self {
            params: Arc::new(MxmMono03Params::default()),
            voice: Voice::new(),
            sounding: None,
            bend: [0.0; NUM_CHANNELS],
            wheel: [0.0; NUM_CHANNELS],
            pressure: [0.0; NUM_CHANNELS],
            routing: Routing::new(),
            active_channel: 0,
            expression_semitones: 0.0,
            sample_rate: 48_000.0,
            telemetry: telemetry::Telemetry::shared(),
            dev_cc: std::env::var_os(DEV_CC_ENV).is_some(),
        }
    }
}

impl MxmMono03 {
    /// Build one sample's worth of plain values from the parameter smoothers.
    ///
    /// Called per sample, which is what makes accent and envelope modulation of the cutoff
    /// sample-accurate. Every smoother must be advanced exactly once per sample, so this is the
    /// only place they are read.
    #[inline]
    fn next_patch(&self) -> VoiceParams {
        let p = &self.params;
        let channel = self.active_channel as usize;

        VoiceParams {
            // Bend rides on top of the tuning control rather than replacing it.
            tune_semitones: p.tune.smoothed.next() / 100.0
                + self.bend[channel] * p.bend_range.smoothed.next(),
            expression_semitones: self.expression_semitones,
            waveform: p.waveform.value().into(),
            cutoff: p.cutoff.smoothed.next(),
            resonance: p.resonance.smoothed.next(),
            decay: p.decay.value(),
            accent: p.accent.smoothed.next(),
            glide_s: p.slide_time.value(),
            volume: p.volume.smoothed.next(),
            // The routing sources a channel owns, reduced to the channel of the latest note-on as the
            // bend always was. They write no parameter.
            wheel: self.wheel[channel],
            pressure: self.pressure[channel],
            bend: self.bend[channel],

            // Advanced. Unsmoothed by design: a constant is not a signal.
            amp_attack_s: p.amp_attack.value(),
            amp_decay_s: p.amp_decay.value(),
            filter_attack_s: p.filter_attack.value(),
            accent_decay_s: p.accent_decay.value(),
            accent_sweep: p.accent_sweep.value(),
            drive: p.drive.value(),
            filter_model: p.filter_model.value().into(),
        }
    }

    /// Which routes are live this interval, **once per callback**: resolved against the last block's
    /// topology so a route that has just become present has its smoother snapped to its stored depth,
    /// then armed on the voice, which clears a source that has just become read. Returns whether
    /// anything is routed.
    fn resolve_topology(&mut self) -> bool {
        self.routing = self.params.routes.topology_from(&self.routing);
        self.voice.set_topology(&self.routing);
        !self.routing.live().is_empty()
    }

    /// One sample through the plugin's own path: the patch from the smoothers, each live route's
    /// amount, then the voice.
    ///
    /// **What `process()` renders per sample, and all the measurement seam renders**, so the two
    /// cannot drift and [`Self::render_block_for_test`] measures the path a host hears.
    #[inline]
    fn render_sample(&mut self, routed: bool) -> f32 {
        let patch = self.next_patch();
        if routed {
            self.params.routes.advance(&mut self.routing);
        }
        self.voice.process(&patch, &self.routing, self.sample_rate)
    }

    /// Renders one block through the plugin's own per-sample path, for measurement.
    ///
    /// **A measurement seam, not a second `process()`.** It calls what `process()` calls —
    /// [`Self::resolve_topology`] once, then [`Self::render_sample`] per sample — and omits the
    /// wrapper's event handling, telemetry and buffer plumbing, which are not where the
    /// routing conversion's work lands (`plans/plan-mxm-mono-03-modulation.md` §11). `mxm-mono-01`,
    /// `mxm-mono-02` and `mxm-poly-06` carry the same seam for the same reason.
    pub fn render_block_for_test(&mut self, out: &mut [f32]) {
        let routed = self.resolve_topology();
        for sample in out.iter_mut() {
            *sample = self.render_sample(routed);
        }
    }

    fn handle_event(&mut self, event: NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn {
                channel,
                note,
                velocity,
                ..
            } => {
                // Velocity zero is a note-off by convention. Otherwise velocity reaches the voice as
                // the Velocity routing source and nothing else (`plan-modulation-routing.md` decision
                // 1.7): the hardware has no velocity sensitivity and accent is its only dynamic, so
                // nothing routes it in the init patch.
                if velocity <= 0.0 {
                    self.note_off(note);
                } else {
                    self.active_channel = channel;
                    self.sounding = Some(note);
                    // A new note carries no expression until the host sends one — including the
                    // note that begins a slide, which is a new note with the gate held open.
                    self.expression_semitones = 0.0;
                    // The two per-step buttons, read at the note-on they belong to. Whether the
                    // envelopes retrigger is *not* read here: that follows the gate, which the
                    // voice tracks itself.
                    let accent = if self.params.accent_note.value() {
                        1.0
                    } else {
                        0.0
                    };
                    self.voice
                        .note_on(note, accent, self.params.slide.value(), velocity);
                }
            }

            NoteEvent::NoteOff { note, .. } => self.note_off(note),

            // Immediate, no release — **for the note that is sounding**, matched exactly as a
            // note-off is. A slide replaces the sounding note before the old one's events have all
            // arrived, and a stale choke for it reset the voice and cut the note it slid into
            // (audit D9).
            NoteEvent::Choke { note, .. } => {
                if self.sounding == Some(note) {
                    self.sounding = None;
                    self.voice.choke();
                }
            }

            // **Per-note pitch, from the host's piano roll.** CLAP's tuning expression, already
            // in semitones, applied only while it names the note that is sounding: an expression
            // for a note this monophonic voice is not playing belongs to no gate here, and
            // applying it would bend a note the host never asked to bend.
            // A non-finite tuning is dropped, and the note keeps the offset it had: a NaN in the
            // pitch sum would reach the oscillator's phase and never leave.
            NoteEvent::PolyTuning { note, tuning, .. } => {
                if self.sounding == Some(note) && tuning.is_finite() {
                    self.expression_semitones = tuning;
                }
            }

            // Channel pressure, per channel: a routing source, which nothing routes in the init
            // patch — the hardware has no aftertouch.
            NoteEvent::MidiChannelPressure {
                channel, pressure, ..
            } => {
                self.pressure[channel as usize % NUM_CHANNELS] = pressure;
            }

            NoteEvent::MidiPitchBend { channel, value, .. } => {
                self.bend[channel as usize % NUM_CHANNELS] = 2.0 * (value - 0.5);
            }

            NoteEvent::MidiCC {
                channel, cc, value, ..
            } => match cc {
                // The collection's developer channel, only when this instance was started with it.
                DEV_VIEW_CC if self.dev_cc => self
                    .telemetry
                    .request_view((value.clamp(0.0, 1.0) * 127.0).round() as u8),
                DEV_DISCLOSURE_CC if self.dev_cc => {
                    self.telemetry.request_disclosure(value >= 0.5);
                }
                DEV_BROWSER_CC if self.dev_cc => {
                    self.telemetry.request_browser(value >= 0.5);
                }
                // A theme by index, 0 light / 1 dark / 2 system, as the app bar's control lists
                // them. Applied to the editor and never saved: a capture run must not rewrite the
                // choice the person at the machine made.
                DEV_THEME_CC if self.dev_cc => {
                    self.telemetry
                        .request_theme((value.clamp(0.0, 1.0) * 127.0).round() as u8);
                }
                // All sound off: immediate, no release.
                control_change::ALL_SOUND_OFF => {
                    self.sounding = None;
                    self.voice.choke();
                }
                // All notes off: deliberately different, the note releases normally.
                control_change::ALL_NOTES_OFF => {
                    self.sounding = None;
                    self.voice.note_off();
                }
                // The mod wheel, per channel: a routing source, which nothing routes in the init
                // patch — the hardware has no wheel.
                control_change::MODULATION_MSB => {
                    self.wheel[channel as usize % NUM_CHANNELS] = value;
                }
                // The sustain pedal is accepted and ignored: the hardware has nothing for it to reach.
                _ => {}
            },

            _ => {}
        }
    }

    /// Release `note`, **if it is the one sounding**.
    ///
    /// A slide replaces the sounding note before the old one's release arrives, so an unmatched
    /// note-off is that stale release and must be dropped. Acting on it would silence a note that
    /// has only just started.
    fn note_off(&mut self, note: u8) {
        if self.sounding == Some(note) {
            self.sounding = None;
            self.voice.note_off();
        }
    }
}

impl Plugin for MxmMono03 {
    const NAME: &'static str = crate::NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// An instrument: no main input. Stereo output is dual mono — the voice is monophonic and there
    /// is no widening.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    /// `MidiCCs` rather than `Basic`: pitch bend, CC 120 and CC 123 are all needed. Declaring MIDI
    /// input is also what makes a host's panic actually clear a stuck note — a CLAP-only note port
    /// gets a wildcard `NoteChoke`, which nice-plug's wrapper does not handle.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;

    /// Smoothers advance per sample inside each event-delimited block, which already removes zipper
    /// noise; splitting a second time buys little here.
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = editor::MxmMono03Editor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    /// The editor is transient: the host opens and closes it at will, and audio renders normally
    /// with none open. Both halves it needs are already `Arc`s, so this hands out clones rather
    /// than borrowing anything the audio thread owns.
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // A rate the DSP's clamps cannot hold is refused before anything changes: a NaN, or one
        // below `MIN_SAMPLE_RATE`, crosses a `clamp` bound and panics on the audio thread.
        if !buffer_config.sample_rate.is_finite()
            || buffer_config.sample_rate < mxm_mono_03_dsp::MIN_SAMPLE_RATE
        {
            return false;
        }
        self.sample_rate = buffer_config.sample_rate;
        self.voice.reset();
        // Once, here: it changes only when the host reconfigures, and the filter curve is plotted
        // against it.
        self.telemetry.publish_sample_rate(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.voice.reset();
        self.sounding = None;
        self.bend = [0.0; NUM_CHANNELS];
        self.wheel = [0.0; NUM_CHANNELS];
        self.pressure = [0.0; NUM_CHANNELS];
        // A fresh topology on the next block, so every present route arrives at its stored depth.
        self.routing = Routing::new();
        self.active_channel = 0;
        self.expression_semitones = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let num_samples = buffer.samples();
        let mut next_event = context.next_event();
        let mut block_start = 0usize;
        // Topology once per callback, never per sub-block (plan §6): a presence changes between
        // buffers, where the host delivers it.
        let routed = self.resolve_topology();

        while block_start < num_samples {
            let mut block_end = (block_start + MAX_BLOCK_SIZE).min(num_samples);

            // Apply everything scheduled at or before this point, then shorten the block so the
            // next event lands exactly where it should.
            loop {
                match next_event {
                    Some(event) if (event.timing() as usize) <= block_start => {
                        self.handle_event(event);
                        next_event = context.next_event();
                    }
                    Some(event) if (event.timing() as usize) < block_end => {
                        block_end = event.timing() as usize;
                        break;
                    }
                    _ => break,
                }
            }

            // `block_end == block_start` can happen when several events share a sample; the loop
            // above consumes them on the next pass, so this cannot spin forever.
            {
                let output = buffer.as_slice();
                // Accumulated across the block and published once, rather than an atomic store per
                // sample: it is the wrong amount of traffic for a value a display reads at frame
                // rate.
                let mut block_peak = 0.0f32;
                for i in block_start..block_end {
                    let sample = self.render_sample(routed);
                    block_peak = block_peak.max(sample.abs());
                    for channel in output.iter_mut() {
                        channel[i] = sample;
                    }
                }
                self.telemetry.publish_peak(block_peak);
                self.telemetry.publish_sweep(self.voice.accent_sweep());
            }

            block_start = block_end;
        }

        if self.voice.is_active() {
            // The amplitude envelope's release is what actually ends a note, and the tail must
            // reflect *audible* output rather than hidden pre-amplifier state.
            ProcessStatus::Tail(
                self.voice
                    .tail_samples(self.params.amp_decay.value(), self.sample_rate),
            )
        } else {
            ProcessStatus::Normal
        }
    }
}

impl ClapPlugin for MxmMono03 {
    /// Permanent. Reverse DNS of a domain the project owns, so it survives moving between forges.
    /// Changing it breaks every saved project using the plugin.
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A monophonic acid bass synthesizer with a diode-ladder filter, slides and accents");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

nice_export_clap!(MxmMono03);

/// The plugin's name, checked where it escapes this crate.
#[cfg(test)]
mod identity {
    use super::{CLAP_ID, NAME};

    /// The id is built from the name, so it cannot drift from it.
    #[test]
    fn the_id_is_the_name_under_the_project_domain() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
    }

    /// `bundler.toml` names the same instrument this crate does.
    ///
    /// **The one place the plugin's name is duplicated outside this crate**, and nothing else would
    /// catch a disagreement: `bundler.toml` is read by `xtask` at bundle time, never by the plugin,
    /// so a stale display name there produces a correctly-built bundle under the wrong filename.
    #[test]
    fn the_bundle_is_named_after_this_plugin() {
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }
}

#[cfg(test)]
mod init_patch {
    use super::params::MxmMono03Params;
    use crate::routes::ROUTE_IDS;
    use mxm_mono_03_dsp::routing::{INIT_AT_FULL, INIT_PRESENT, Routing, SOURCES, TARGETS};
    use nice_plug::prelude::Params;

    /// **Pins the rule, not the taste.** A retune of any control survives this; making a depth
    /// non-zero because it sounded nice does not.
    #[test]
    fn every_amount_starts_at_zero() {
        let p = MxmMono03Params::default();
        assert_eq!(p.accent.value(), 0.0, "accent depth is an amount");
        assert!(!p.accent_note.value(), "no note is accented by default");
        assert!(!p.slide.value(), "no note slides by default");
        assert_eq!(p.resonance.value(), 0.0, "resonance is an amount");
        assert_eq!(p.tune.value(), 0.0, "tuning is centred");
        // Every route's amount too, but the accent circuit's two, which
        // `the_accent_circuits_routes_start_at_full` records.
        for (t, group) in p.routes.each().into_iter().enumerate() {
            for (s, route) in group.routes(t).into_iter().enumerate() {
                if !INIT_AT_FULL.contains(&(t, s)) {
                    assert_eq!(
                        route.amount.normalised(),
                        0.5,
                        "{} starts at zero",
                        ROUTE_IDS[t][s].0
                    );
                }
            }
        }
    }

    /// **Env Mod's deviation is circuit, not an amount** — recorded so it reads as a decision rather
    /// than an oversight. The hardware cannot turn the filter envelope's reach on the cutoff down to
    /// zero; the voice adds that floor whatever is routed, and the (Cutoff ← Filter envelope) route that
    /// replaced the `envmod` knob starts at zero above it.
    #[test]
    fn env_mods_floor_is_circuit_and_its_route_starts_at_zero() {
        use mxm_mono_03_dsp::routing::ENV_MOD_FLOOR_OCTAVES;
        let p = MxmMono03Params::default();
        assert!(
            p.routes.cutoff.env_on.value(),
            "the Env Mod route is present at Init"
        );
        assert_eq!(
            p.routes.cutoff.env.value(),
            0.0,
            "and at zero, which is the floor"
        );
        assert!(
            std::hint::black_box(ENV_MOD_FLOOR_OCTAVES) > 0.0,
            "the whole point of the deviation is that the floor is not zero"
        );
    }

    /// **The accent circuit's two routes start at full — two recorded init deviations.** The Accent
    /// knob is the circuit's one depth, ahead of both outputs, so these routes are fixed-gain wiring,
    /// and a fresh instance, whose knob is at zero, sounds as it always did (plan §5, D1 a).
    #[test]
    fn the_accent_circuits_routes_start_at_full() {
        let p = MxmMono03Params::default();
        for (t, s) in INIT_AT_FULL {
            let routes = p.routes.each()[t].routes(t);
            assert!(routes[s].is_present(), "{} is wired", ROUTE_IDS[t][s].1);
            assert_eq!(
                routes[s].amount.normalised(),
                1.0,
                "{} starts at full",
                ROUTE_IDS[t][s].0
            );
        }
        assert_eq!(
            p.accent.value(),
            0.0,
            "and the knob that is their depth starts at zero"
        );
    }

    /// **The init patch wires exactly the machine's own three paths**, and the topology the plugin
    /// resolves from it is the DSP's init patch.
    #[test]
    fn the_init_patch_wires_exactly_the_machines_own_routes() {
        let p = MxmMono03Params::default();
        for (t, group) in p.routes.each().into_iter().enumerate() {
            for (s, route) in group.routes(t).into_iter().enumerate() {
                assert_eq!(
                    route.is_present(),
                    INIT_PRESENT.contains(&(t, s)),
                    "{}",
                    ROUTE_IDS[t][s].1
                );
            }
        }
        assert_eq!(p.routes.topology().live(), Routing::init().live());
    }

    /// **`envmod` is retired and never comes back** (plan §4), and the instrument has what the plan
    /// counts: eighteen parameters of its own and a presence and an amount per routing pair.
    #[test]
    fn no_retired_id_reappears() {
        let ids: Vec<String> = MxmMono03Params::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert!(
            !ids.iter().any(|id| id == "envmod"),
            "a retired id must never be reused"
        );
        assert_eq!(ids.len(), 18 + TARGETS * SOURCES * 2);
    }

    /// **Slide is a switch, and there is no slide *time* control.**
    ///
    /// The hardware's slide time is a fixed RC and its button is on or off, so the only thing
    /// anyone decides is *whether* a transition slides. A test rather than a comment, because "this
    /// is deliberately a switch" is exactly the decision someone undoes while being helpful — it
    /// was already added as a time knob once, and removed.
    #[test]
    fn slide_is_a_switch_and_its_time_is_advanced() {
        let ids: Vec<String> = MxmMono03Params::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert!(
            ids.iter().any(|id| id == "slide"),
            "the slide switch is missing: {ids:?}"
        );
        // The time is reachable, but behind the disclosure and defaulting to the hardware's.
        assert!(
            ids.iter().any(|id| id == "slidetime"),
            "the advanced slide time is missing: {ids:?}"
        );
    }

    /// Every advanced control starts at the value the machine has.
    ///
    /// **This is what makes the disclosure safe.** Opening it changes nothing until something is
    /// moved, so the instrument is the machine until somebody decides otherwise — which is the
    /// whole argument for exposing fixed constants at all.
    #[test]
    fn every_advanced_control_defaults_to_the_hardware() {
        use mxm_mono_03_dsp::envelope;
        use mxm_mono_03_dsp::voice::{DRIVE, SLIDE_TIME_S};
        let p = MxmMono03Params::default();
        assert_eq!(p.slide_time.value(), SLIDE_TIME_S);
        assert_eq!(p.amp_attack.value(), envelope::ATTACK_S);
        assert_eq!(p.filter_attack.value(), envelope::ATTACK_S);
        assert_eq!(p.amp_decay.value(), envelope::AMP_DECAY_S);
        assert_eq!(p.accent_decay.value(), envelope::DECAY_MIN_S);
        assert_eq!(p.accent_sweep.value(), 1.0);
        assert_eq!(p.drive.value(), DRIVE);
        assert_eq!(p.filter_model.value(), super::params::FilterModel::Tb303);
    }

    /// Configurations start somewhere musically useful.
    #[test]
    fn the_filter_starts_open_enough_to_hear() {
        let p = MxmMono03Params::default();
        assert!(
            p.cutoff.value() >= 0.5,
            "the filter should start open, not shut: {}",
            p.cutoff.value()
        );
    }
}

/// The path from a host's note expression to the pitch.
///
/// **The link nothing else covers.** The DSP crate proves `expression_semitones` moves the pitch;
/// these prove the event reaches that field, and that an expression naming some other note does
/// not. Between them there was a plugin arm no test had ever executed.
#[cfg(test)]
mod pitch_expression {
    use super::MxmMono03;
    use nice_plug::prelude::*;

    fn note_on(plugin: &mut MxmMono03, note: u8) {
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            velocity: 0.8,
        });
    }

    fn tune(plugin: &mut MxmMono03, note: u8, semitones: f32) {
        plugin.handle_event(NoteEvent::PolyTuning {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            tuning: semitones,
        });
    }

    #[test]
    fn a_tuning_expression_for_the_sounding_note_reaches_the_patch() {
        let mut plugin = MxmMono03::default();
        note_on(&mut plugin, 48);
        assert_eq!(plugin.next_patch().expression_semitones, 0.0, "the premise");

        tune(&mut plugin, 48, -3.5);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            -3.5,
            "the host's per-note pitch has to arrive in semitones, unscaled"
        );
    }

    #[test]
    fn an_expression_for_another_note_is_ignored() {
        // One voice plays one note. An expression naming a note this instrument is not playing
        // would bend a note the host never asked to bend.
        let mut plugin = MxmMono03::default();
        note_on(&mut plugin, 48);
        tune(&mut plugin, 55, 7.0);
        assert_eq!(plugin.next_patch().expression_semitones, 0.0);
    }

    #[test]
    fn a_new_note_starts_clean_and_a_release_keeps_what_it_was_bent_to() {
        let mut plugin = MxmMono03::default();
        note_on(&mut plugin, 48);
        tune(&mut plugin, 48, 5.0);

        // A slide is a new note with the gate held open, and it starts clean like any other.
        note_on(&mut plugin, 52);
        assert_eq!(
            plugin.next_patch().expression_semitones,
            0.0,
            "a new note carries no expression until the host sends one"
        );

        tune(&mut plugin, 52, 2.0);
        plugin.handle_event(NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel: 0,
            note: 52,
            velocity: 0.0,
        });
        assert_eq!(
            plugin.next_patch().expression_semitones,
            2.0,
            "the note releases at the pitch it was bent to; zeroing here would snap it mid-tail"
        );
    }

    /// **A non-finite tuning is dropped at the event**, and the note keeps the offset it had. A
    /// NaN in the pitch sum reaches `powf` and the oscillator's frequency, and the phase never
    /// recovers. The reference is the same instance never sent it.
    #[test]
    fn a_non_finite_tuning_expression_is_dropped_and_the_pitch_stays_finite() {
        let plugin = || {
            let mut plugin = MxmMono03::default();
            for (_, ptr, _) in plugin.params.param_map() {
                unsafe { ptr._internal_update_smoother(48_000.0, true) };
            }
            note_on(&mut plugin, 48);
            tune(&mut plugin, 48, 3.0);
            plugin
        };
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (mut actual, mut reference) = (plugin(), plugin());
            tune(&mut actual, 48, bad);
            assert_eq!(actual.next_patch().expression_semitones, 3.0, "{bad}");
            reference.next_patch();
            let (mut heard, mut expected) = ([0.0f32; 2048], [0.0f32; 2048]);
            actual.render_block_for_test(&mut heard);
            reference.render_block_for_test(&mut expected);
            assert!(heard.iter().all(|s| s.is_finite()), "{bad}");
            assert_eq!(heard, expected, "{bad}");
            assert!(heard.iter().any(|&s| s != 0.0), "the note sounds");
        }
    }
}

/// A release or a choke acts only on the note that is sounding.
#[cfg(test)]
mod note_tracking {
    use super::*;
    use nice_plug::params::InternalParamMut;

    fn plugin() -> MxmMono03 {
        let mut plugin = MxmMono03::default();
        for (_, ptr, _) in plugin.params.param_map() {
            unsafe { ptr._internal_update_smoother(48_000.0, true) };
        }
        plugin.voice.reset();
        plugin
    }

    fn note_on(plugin: &mut MxmMono03, note: u8) {
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            velocity: 0.8,
        });
    }

    fn choke(plugin: &mut MxmMono03, note: u8) {
        plugin.handle_event(NoteEvent::Choke {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
        });
    }

    /// A slide from 48 into 50 with the gate held, as a host sends a tie: the new note starts
    /// before the old one's events have all arrived.
    fn slid() -> MxmMono03 {
        let mut plugin = plugin();
        note_on(&mut plugin, 48);
        plugin.render_block_for_test(&mut [0.0f32; 4_800]);
        // SAFETY: the write the wrapper makes for a host's parameter event.
        unsafe { plugin.params.slide._internal_set_plain_value(true) };
        note_on(&mut plugin, 50);
        plugin
    }

    /// **Audit D9.** A choke names a note, exactly as a release does, and a stale one for the note
    /// a slide replaced must not cut the note it slid into. The Choke arm reset the voice whatever
    /// note it named.
    #[test]
    fn a_stale_choke_during_a_slide_leaves_the_new_note_sounding() {
        let mut choked = slid();
        let mut untouched = slid();
        choke(&mut choked, 48);
        let (mut heard, mut expected) = ([0.0f32; 4_800], [0.0f32; 4_800]);
        choked.render_block_for_test(&mut heard);
        untouched.render_block_for_test(&mut expected);
        assert!(expected.iter().any(|s| s.abs() > 1e-3), "the premise");
        let peak = heard.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            heard == expected,
            "a choke for 48 changed the slid-into 50: peak {peak}"
        );
        assert_eq!(choked.sounding, Some(50), "the stale choke took the note");
    }

    /// And a choke for the note that is sounding still cuts it, at once and to exact silence.
    #[test]
    fn a_choke_for_the_sounding_note_still_cuts_it() {
        let mut plugin = slid();
        plugin.render_block_for_test(&mut [0.0f32; 480]);
        choke(&mut plugin, 50);
        let mut after = [1.0f32; 4_800];
        plugin.render_block_for_test(&mut after);
        let peak = after.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            after.iter().all(|&s| s == 0.0),
            "the choke left peak {peak}"
        );
        assert_eq!(plugin.sounding, None);
    }
}

#[cfg(test)]
mod developer_channel_tests {
    use super::*;

    fn cc(plugin: &mut MxmMono03, cc: u8, raw: u8) {
        plugin.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc,
            value: f32::from(raw) / 127.0,
        });
    }

    /// The developer channel reaches the editor only when the instance was started with it; a
    /// host sending the same control change to an ordinary instance changes nothing.
    #[test]
    fn the_developer_channel_is_off_unless_the_environment_asked_for_it() {
        let mut plugin = MxmMono03 {
            dev_cc: false,
            ..Default::default()
        };
        cc(&mut plugin, DEV_VIEW_CC, 1);
        cc(&mut plugin, DEV_DISCLOSURE_CC, 127);
        cc(&mut plugin, DEV_BROWSER_CC, 127);
        cc(&mut plugin, DEV_THEME_CC, 1);
        assert_eq!(plugin.telemetry.take_view_request(), None);
        assert_eq!(plugin.telemetry.take_disclosure_request(), None);
        assert_eq!(plugin.telemetry.take_browser_request(), None);
        assert_eq!(plugin.telemetry.take_theme_request(), None);

        plugin.dev_cc = true;
        cc(&mut plugin, DEV_VIEW_CC, 1);
        cc(&mut plugin, DEV_DISCLOSURE_CC, 127);
        cc(&mut plugin, DEV_BROWSER_CC, 127);
        cc(&mut plugin, DEV_THEME_CC, 1);
        assert_eq!(plugin.telemetry.take_view_request(), Some(1));
        assert_eq!(plugin.telemetry.take_disclosure_request(), Some(true));
        assert_eq!(plugin.telemetry.take_browser_request(), Some(true));
        assert_eq!(
            plugin.telemetry.take_theme_request(),
            Some(1),
            "1 is dark, as mxm_ui::theme::from_index reads it"
        );
        // Light is index 0 — a request like any other, not the absence of one.
        cc(&mut plugin, DEV_THEME_CC, 0);
        assert_eq!(plugin.telemetry.take_theme_request(), Some(0));
        // View zero is a request too, not the absence of one.
        cc(&mut plugin, DEV_VIEW_CC, 0);
        assert_eq!(plugin.telemetry.take_view_request(), Some(0));
    }
}

/// The routing's plugin half: the performance inputs that became sources, and a route that arrives
/// mid-performance rendering the same whatever the host's buffers.
#[cfg(test)]
mod routing_path {
    use super::*;
    use mxm_mono_03_dsp::routing::source;
    use nice_plug::params::InternalParamMut;

    const FS: f32 = 48_000.0;

    fn plugin() -> MxmMono03 {
        let mut plugin = MxmMono03::default();
        for (_, ptr, _) in plugin.params.param_map() {
            unsafe { ptr._internal_update_smoother(FS, true) };
        }
        plugin.sample_rate = FS;
        plugin.voice.reset();
        plugin
    }

    fn key(plugin: &mut MxmMono03, on: bool, note: u8, velocity: f32) {
        plugin.handle_event(if on {
            NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 0,
                note,
                velocity,
            }
        } else {
            NoteEvent::NoteOff {
                timing: 0,
                voice_id: None,
                channel: 0,
                note,
                velocity: 0.0,
            }
        });
    }

    fn gestures(plugin: &mut MxmMono03, wheel: f32, pressure: f32) {
        plugin.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::MODULATION_MSB,
            value: wheel,
        });
        plugin.handle_event(NoteEvent::MidiChannelPressure {
            timing: 0,
            channel: 0,
            pressure,
        });
    }

    /// **Velocity, the wheel and pressure reach the voice as routing sources** — new paths under
    /// `plan-modulation-routing.md` decision 1.7 — once a route reads them.
    #[test]
    fn velocity_wheel_and_pressure_reach_the_voice_as_sources() {
        let mut plugin = plugin();
        unsafe {
            let r = &plugin.params.routes.res;
            r.vel_on._internal_set_plain_value(true);
            r.wheel_on._internal_set_plain_value(true);
            r.press_on._internal_set_plain_value(true);
        }
        gestures(&mut plugin, 0.25, 0.5);
        key(&mut plugin, true, 48, 0.75);
        let mut block = [0.0f32; 64];
        plugin.render_block_for_test(&mut block);
        // Velocity as the modulation standard publishes it: `v − 1`.
        assert_eq!(plugin.voice.published(source::VELOCITY), -0.25);
        assert_eq!(plugin.voice.published(source::WHEEL), 0.25);
        assert_eq!(plugin.voice.published(source::PRESSURE), 0.5);
    }

    /// **The new MIDI paths cost nothing at rest** (plan §10): with no route reading them, a note
    /// renders bit-identically whether or not a wheel, a pressure or another velocity arrives.
    #[test]
    fn the_new_midi_paths_change_nothing_while_no_route_reads_them() {
        let render = |moved: bool| {
            let mut plugin = plugin();
            if moved {
                gestures(&mut plugin, 1.0, 1.0);
            }
            key(&mut plugin, true, 48, if moved { 0.2 } else { 0.9 });
            let mut out = vec![0.0f32; 9_600];
            for block in out.chunks_mut(64) {
                plugin.render_block_for_test(block);
            }
            out
        };
        let still = render(false);
        assert!(
            still.iter().any(|s| s.abs() > 1e-3),
            "the premise: it plays"
        );
        assert!(
            still == render(true),
            "a gesture nothing routes must not reach the sound"
        );
    }

    /// **A route that arrives after an idle span renders the same whatever the block size** — its
    /// depth mid-ramp as it arrives, and a standing route's depth moving too.
    #[test]
    fn a_route_arriving_after_an_idle_span_is_block_partition_invariant() {
        let render = |block: usize| -> Vec<f32> {
            let mut plugin = plugin();
            let mut out = vec![0.0f32; 96_000];
            for chunk in out[..48_000].chunks_mut(block) {
                plugin.render_block_for_test(chunk);
            }
            unsafe {
                let r = &plugin.params.routes;
                r.res.osc_on._internal_set_plain_value(true);
                r.res.osc._internal_set_plain_value(0.3);
                r.res.osc._internal_update_smoother(FS, false);
                r.cutoff.env._internal_set_plain_value(0.6);
                r.cutoff.env._internal_update_smoother(FS, false);
            }
            key(&mut plugin, true, 48, 0.8);
            for chunk in out[48_000..].chunks_mut(block) {
                plugin.render_block_for_test(chunk);
            }
            out
        };
        let whole = render(64);
        assert!(
            whole[48_000..].iter().any(|s| s.abs() > 1e-3),
            "the premise: it plays"
        );
        assert!(whole == render(37), "64 against 37 samples a block");
        assert!(whole == render(1024), "64 against 1024 samples a block");
    }
}

/// **M0 of `plans/plan-mxm-mono-03-modulation.md`: what the routing conversion is measured against.**
///
/// Captured before the conversion and recorded in `BASELINE-M0.md`, because afterwards the old
/// renders cannot be produced. Measurements, not assertions, so every test is `#[ignore]`d:
///
/// ```bash
/// cargo test -p mxm-mono-03 --release --lib baseline -- --ignored --nocapture --test-threads=1
/// ```
///
/// With `MXM_M0_DUMP=<dir>` the bank's renders are also written there as raw little-endian `f32`,
/// and `the_bank_against_the_m0_dump` compares a later build against them sample by sample.
/// **The module uses only what both revisions have** — the seam, the permanent ids of the factory
/// files and the two per-step switches — so this same file runs before the conversion and after it.
#[cfg(test)]
mod baseline {
    use super::*;
    use nice_plug::params::InternalParamMut;
    use std::time::Instant;

    const FS: f32 = 48_000.0;
    const BLOCK: usize = 64;

    /// A plugin with every smoother activated (`docs/adding-an-instrument.md` gotcha 13), at the rate
    /// `activate` would give it.
    fn plugin() -> MxmMono03 {
        let mut plugin = MxmMono03::default();
        for (_, ptr, _) in plugin.params.param_map() {
            unsafe { ptr._internal_update_smoother(FS, true) };
        }
        plugin.sample_rate = FS;
        plugin.voice.reset();
        plugin
    }

    /// One of the machine's per-step buttons, pressed as a host would press it.
    fn press(button: &BoolParam, on: bool) {
        unsafe { button._internal_set_plain_value(on) };
    }

    fn key(plugin: &mut MxmMono03, on: bool, note: u8) {
        plugin.handle_event(if on {
            NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 0,
                note,
                velocity: 0.8,
            }
        } else {
            NoteEvent::NoteOff {
                timing: 0,
                voice_id: None,
                channel: 0,
                note,
                velocity: 0.0,
            }
        });
    }

    /// Renders `blocks` blocks of 64 samples onto the end of `out`.
    fn run(plugin: &mut MxmMono03, out: &mut Vec<f32>, blocks: usize) {
        let mut block = [0.0f32; BLOCK];
        for _ in 0..blocks {
            plugin.render_block_for_test(&mut block);
            out.extend_from_slice(&block);
        }
    }

    /// **The score every sound plays, fixed forever**: a plain note; two accented notes a step apart,
    /// so the forced short decay, the louder amplifier and the sweep's climb are all in it; a tie with
    /// Slide on, an octave up, which glides and retriggers nothing; and two seconds of tail. Accent and
    /// slide are the machine's per-step buttons, so the score presses them — no factory sound sets
    /// either (`no_factory_preset_moves_a_per_step_switch`). A block is 1.33 ms, so 90 is 120 ms: a
    /// sixteenth at 125 BPM.
    fn render_score(plugin: &mut MxmMono03) -> Vec<f32> {
        let mut out = Vec::with_capacity(2_040 * BLOCK);
        key(plugin, true, 36);
        run(plugin, &mut out, 45);
        key(plugin, false, 36);
        run(plugin, &mut out, 45);
        press(&plugin.params.accent_note, true);
        for _ in 0..2 {
            key(plugin, true, 36);
            run(plugin, &mut out, 45);
            key(plugin, false, 36);
            run(plugin, &mut out, 45);
        }
        press(&plugin.params.accent_note, false);
        key(plugin, true, 36);
        run(plugin, &mut out, 90);
        // The tie: the new note before the old one's release, so the gate never closes.
        press(&plugin.params.slide, true);
        key(plugin, true, 48);
        key(plugin, false, 36);
        run(plugin, &mut out, 180);
        key(plugin, false, 48);
        press(&plugin.params.slide, false);
        run(plugin, &mut out, 1_500);
        out
    }

    fn throughput(label: &str, plugin: &mut MxmMono03) {
        press(&plugin.params.accent_note, true);
        key(plugin, true, 48);
        let mut out = [0.0f32; BLOCK];
        // Warm the caches: the first blocks pay for page faults, which is not the question.
        for _ in 0..64 {
            plugin.render_block_for_test(&mut out);
        }
        let blocks = 40_000;
        let start = Instant::now();
        for _ in 0..blocks {
            plugin.render_block_for_test(&mut out);
        }
        let taken = start.elapsed().as_secs_f64();
        let samples = (blocks * BLOCK) as f64;
        println!(
            "  mxm-mono-03, {label}, accented held note: {:.3} ns/sample ({:.0} samples/s)",
            taken * 1e9 / samples,
            samples / taken
        );
    }

    /// Per-sample cost through the plugin's own path, on Init and on one fixed routed patch, three
    /// times each so the spread is visible. **A figure counts only from a quiet machine**
    /// (`docs/code-review-notes.md` §3).
    ///
    /// **The routed patch is `Acid line`**: Env Mod and Accent both well up and resonance at 0.85, so
    /// both of the paths the conversion rewrites carry signal, and the accent's sweep is the one that
    /// climbs.
    #[test]
    #[ignore = "a measurement, not an assertion; release only"]
    fn throughput_of_init_and_a_routed_patch() {
        println!();
        for pass in 1..=3 {
            throughput(&format!("pass {pass}, Init"), &mut plugin());
            let mut acid = plugin();
            apply(&acid, factory("Acid line"));
            throughput(&format!("pass {pass}, Acid line"), &mut acid);
        }
        println!();
    }

    /// FNV-1a over the raw bits, as `plugins/mxm-mono-01/host-tests/tests/golden_audio.rs` computes it.
    fn digest(samples: &[f32]) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for sample in samples {
            for byte in sample.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        format!("{hash:016x}")
    }

    fn factory(name: &str) -> &'static str {
        crate::preset::FACTORY_FILES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, text)| *text)
            .unwrap_or_else(|| panic!("no factory sound {name:?}"))
    }

    /// Applies a factory file's stored values by id — **only `v` is read**, as the preset system
    /// reads it — then snaps every smoother, as a fresh load would settle.
    fn apply(plugin: &MxmMono03, json: &str) -> usize {
        let map = plugin.params.param_map();
        let mut applied = 0;
        for (id, ptr, _) in &map {
            let key = format!("\"{id}\"");
            let Some(at) = json.find(&key) else { continue };
            let rest = &json[at + key.len()..];
            let Some(vpos) = rest.find("\"v\"") else {
                continue;
            };
            let number: String = rest[vpos + 3..]
                .chars()
                .skip_while(|c| *c == ':' || c.is_whitespace())
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e')
                .collect();
            if let Ok(v) = number.parse::<f32>() {
                unsafe { ptr._internal_set_normalized_value(v) };
                applied += 1;
            }
        }
        for (_, ptr, _) in &map {
            unsafe { ptr._internal_update_smoother(FS, true) };
        }
        applied
    }

    fn slug(name: &str) -> String {
        name.to_lowercase().replace(' ', "-")
    }

    /// Init first, then the fifty in shipped order.
    fn bank() -> Vec<(&'static str, Vec<f32>, usize)> {
        let mut out = Vec::new();
        let mut init = plugin();
        out.push(("Init", render_score(&mut init), 0));
        for (name, json) in crate::preset::FACTORY_FILES {
            let mut p = plugin();
            let applied = apply(&p, json);
            out.push((name, render_score(&mut p), applied));
        }
        out
    }

    /// A digest and a peak for Init and every factory sound; with `MXM_M0_DUMP` set, the renders too.
    #[test]
    #[ignore = "a measurement, not an assertion; release only"]
    fn the_bank_digests() {
        let dump = std::env::var_os("MXM_M0_DUMP").map(std::path::PathBuf::from);
        println!();
        for (name, samples, applied) in bank() {
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            println!(
                "  | `{}` | `{}` | {peak:.4} | {applied} |",
                slug(name),
                digest(&samples)
            );
            if let Some(dir) = &dump {
                let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                std::fs::write(dir.join(format!("{}.f32", slug(name))), bytes).expect("dump");
            }
        }
        println!();
    }

    /// Sample-by-sample difference from the M0 renders in `MXM_M0_DUMP`: the worst absolute
    /// difference, and the worst relative to that sound's own peak.
    #[test]
    #[ignore = "a comparison against a local dump, not an assertion"]
    fn the_bank_against_the_m0_dump() {
        let Some(dir) = std::env::var_os("MXM_M0_DUMP").map(std::path::PathBuf::from) else {
            println!("set MXM_M0_DUMP to the directory the_bank_digests wrote");
            return;
        };
        println!();
        let (mut worst_abs, mut worst_rel, mut moved) = (0.0f32, 0.0f32, 0);
        for (name, samples, _) in bank() {
            let bytes = std::fs::read(dir.join(format!("{}.f32", slug(name)))).expect("dump");
            let before: Vec<f32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            assert_eq!(before.len(), samples.len(), "{name}: length changed");
            let diff = before
                .iter()
                .zip(&samples)
                .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
            let peak = before.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let rel = if peak > 0.0 { diff / peak } else { diff };
            if diff > 0.0 {
                moved += 1;
            }
            worst_abs = worst_abs.max(diff);
            worst_rel = worst_rel.max(rel);
            println!(
                "  {:<18} max |diff| {diff:.3e}  relative {rel:.3e}",
                slug(name)
            );
        }
        println!(
            "  moved {moved} of 51; worst |diff| {worst_abs:.3e}, worst relative {worst_rel:.3e}\n"
        );
    }
}

/// The host's sample rate at activation: the floor the DSP's clamps are safe above.
#[cfg(test)]
mod sample_rate_floor {
    use super::*;
    use mxm_mono_03_dsp::MIN_SAMPLE_RATE;

    struct Activation;

    impl ActivateContext<MxmMono03> for Activation {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: ()) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn activate_at(plugin: &mut MxmMono03, sample_rate: f32) -> bool {
        plugin.activate(
            &MxmMono03::AUDIO_IO_LAYOUTS[0],
            &BufferConfig {
                sample_rate,
                min_buffer_size: Some(1),
                max_buffer_size: 4096,
                process_mode: ProcessMode::Realtime,
            },
            &mut Activation,
        )
    }

    fn render(plugin: &mut MxmMono03, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; frames];
        plugin.render_block_for_test(&mut out);
        out
    }

    /// **The floor activates and plays, whatever the parameters say.** Every parameter at its
    /// default, then all at the bottom of their ranges, then all at the top — every route present
    /// at full — with a note held for four seconds at 1 kHz.
    #[test]
    fn the_rate_floor_activates_and_plays_at_every_parameter_extreme() {
        for extreme in [None, Some(0.0), Some(1.0)] {
            let mut plugin = MxmMono03::default();
            for (_, ptr, _) in plugin.params.param_map() {
                if let Some(value) = extreme {
                    let _ = unsafe { ptr._internal_set_normalized_value(value) };
                }
                unsafe { ptr._internal_update_smoother(MIN_SAMPLE_RATE, true) };
            }
            assert!(activate_at(&mut plugin, MIN_SAMPLE_RATE), "{extreme:?}");
            assert_eq!(plugin.sample_rate, MIN_SAMPLE_RATE);
            plugin.handle_event(NoteEvent::NoteOn {
                timing: 0,
                voice_id: None,
                channel: 0,
                note: 48,
                velocity: 0.8,
            });
            let out = render(&mut plugin, 4_000);
            assert!(out.iter().all(|s| s.is_finite()), "{extreme:?}");
        }
    }

    /// **A rate the DSP's clamps cannot hold is refused at activation.** `f32::clamp` panics on
    /// a NaN or inverted bound, so a NaN rate or one low enough to cross a corner's floor over its
    /// Nyquist fraction panicked on the audio thread. A refusal leaves the plugin as it was.
    #[test]
    fn activation_refuses_a_non_finite_rate_and_any_below_the_floor() {
        for unsupported in [
            MIN_SAMPLE_RATE.next_down(),
            100.0,
            20.0,
            1.0,
            0.0,
            -48_000.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let mut refused = MxmMono03::default();
            assert!(
                !activate_at(&mut refused, unsupported),
                "accepted {unsupported} Hz"
            );
            assert_eq!(refused.sample_rate, 48_000.0, "{unsupported} Hz");
        }
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
