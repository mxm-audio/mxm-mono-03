//! The monophonic voice: everything wired together.
//!
//! ```text
//! note + gate ─► pitch lag ─► oscillator ─► drive ─► diode ladder ─► VCA ─► out
//!                (legato only)                          ▲             ▲
//!                          filter envelope ─────────────┤             │
//!                          accent sweep ────────────────┘             │
//!                          amp envelope ───────────────────────────────┘
//!                                             ▲
//!                                 accent ─────┘
//! ```
//!
//! # The modulation is routing, and the machine's wiring is the init patch
//!
//! Every arrow above that carries modulation — the filter envelope and the accent circuit's two
//! outputs — is a route in [`crate::routing`], present in the init patch. What stays circuit is what
//! is state rather than a level: the envelopes, the accent circuit itself with its forced decay and
//! its held sweep, the slide's lag, and the Env Mod floor, which the voice adds whatever is routed.
//!
//! # Slide is a switch, and the lag lives here rather than in a sequencer
//!
//! The hardware's slide is a per-step **on/off button**, and its slide *time* is a
//! fixed RC — there is no time control, so [`SLIDE_TIME_S`] is a constant.
//!
//! The owner asks for glide in the synth rather than the sequencer, and
//! `docs/modulation/04-glide-and-portamento.md` §4.7.2 gives the reason: the lag is
//! a property of the voice, downstream of whatever generates notes, so putting it in
//! a sequencer would deny it to anyone playing a keyboard. What a sequencer keeps is
//! the per-transition *decision*, which is that switch.
//!
//! # What a slid-into note does not do
//!
//! A slide holds the gate open, so the second note is not a new note:
//!
//! - it **retriggers neither envelope** — no new attack, no new filter sweep;
//! - it **takes no accent**, because there is no note-on to accent.
//!
//! That is `docs/modulation/04-glide-and-portamento.md` §4.4 followed directly.
//! Getting it wrong gives two notes with a pitch bend between them, which is a
//! different and much more ordinary sound. **The contract is stated, not
//! measured** — that chapter says so, and a listening comparison is what would
//! confirm it.
//!
//! # Why the filter is driven hard
//!
//! `research:filters/machines/tb303-diode-ladder.md` §4 measures the droop: with
//! resonance up, 8.75 units of input buy 0.5 of output. This is a filter you must
//! drive hard to hear at all, and the drive is therefore part of the instrument
//! rather than an effect in front of it. It is **not a user control** — the
//! hardware has none.

use crate::accent::Accent;
use crate::envelope::{self, AttackDecay};
use crate::filter::{self, DiodeConfig, DiodeLadder};
use crate::flush;
use crate::oscillator::{Oscillator, Waveform};
use crate::routing::{
    ACCENT_UNIT, Graph, KEY_UNIT_SEMITONES, Routing, cutoff_octaves, source, target,
};
use mxm_modulation::standard;

/// How hard the oscillator drives the filter, on the hardware. See the module
/// documentation — and note this is the *default*, reachable behind the plugin's
/// advanced disclosure, not a hard-coded constant.
pub const DRIVE: f32 = 6.0;

/// Makeup gain after the filter, compensating the ladder's droop.
///
/// The droop is the topology and must not be "fixed" inside the filter — see
/// `crates/mxm-mono-01-dsp/AGENTS.md`, which makes the same point about its own
/// ladder. Compensating *outside* it keeps the filter honest.
pub const MAKEUP: f32 = 1.4;

/// Cutoff at the bottom of the control.
pub const CUTOFF_LOW_HZ: f32 = 60.0;

/// Cutoff at the top of the control, before the sample-rate clamp.
///
/// **The control shows a position, not a frequency.** Labelling it in Hz would
/// mean running the model at roughly eight times the displayed value so the
/// reading matched the -3 dB point, and the widest pole then caps the whole
/// control in the high hundreds of hertz — the trap worked out in
/// `research:filters/machines/tb303-diode-ladder.md` §10.5. The hardware's own control
/// carries no markings either.
pub const CUTOFF_HIGH_HZ: f32 = 4_000.0;

/// A DC blocker on the output only.
///
/// `docs/filters/03-nonlinearity.md` §3.4: asymmetric saturation inside a feedback
/// loop biases the operating point, and that wobble is part of what makes these
/// circuits feel alive. Block DC at the **output**, never inside the loop, or you
/// remove the effect you were modelling.
pub const DC_BLOCK_HZ: f32 = 5.0;

/// Everything the voice is told, per block. Plain values only.
///
/// **The routing is not in here.** `Params` is `Copy` and rebuilt every sample, so the route grid
/// travels beside it as a [`Routing`] (`docs/code-review-notes.md` §7).
#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Master tuning, in semitones.
    pub tune_semitones: f32,
    /// Per-note pitch expression, in semitones.
    ///
    /// **Separate from `tune_semitones`, and deliberately.** That field carries the tuning control
    /// and the channel bend — things the patch and the performer own. This is the host's *per-note*
    /// offset, CLAP's `CLAP_NOTE_EXPRESSION_TUNING`: the curve a DAW draws against one note in its
    /// piano roll. Both are semitones on the same pitch and they sum, but one field could not say
    /// which of the two moved.
    ///
    /// **It rides on top of the slide rather than replacing it.** The slide lags the pitch
    /// towards the note that was played; this offset is added after that lag, so a drawn bend
    /// bends the sliding pitch instead of fighting it. Unsmoothed, as the bend is — the host
    /// delivers timed events and `process` splits its block on each one.
    pub expression_semitones: f32,
    pub waveform: Waveform,
    /// Cutoff **position**, `0..=1`. Not a frequency.
    pub cutoff: f32,
    /// Resonance, `0..=1`, normalised against this filter's own threshold.
    pub resonance: f32,
    /// Filter envelope decay, `0..=1`, mapped by [`envelope::decay_time_s`].
    pub decay: f32,
    /// Accent depth, `0..=1` — the panel knob, and **the level of both the accent circuit's routing
    /// sources**, which is how one knob still plays accent exactly as before (plan D1 a).
    pub accent: f32,
    /// Slide time constant, in seconds.
    ///
    /// Fixed on the hardware; a control only behind the plugin's advanced
    /// disclosure. [`SLIDE_TIME_S`] is the machine's value and the default.
    pub glide_s: f32,
    /// Output level, linear.
    pub volume: f32,
    /// The mod wheel on the sounding note's channel, `0..=1` — a routing source and nothing else.
    pub wheel: f32,
    /// Channel pressure on the sounding note's channel, `0..=1` — a routing source and nothing else.
    pub pressure: f32,
    /// The bender's lever position, `-1..=1` — a routing source. Its reach in semitones is already
    /// in `tune_semitones`, the pitch path it always had.
    pub bend: f32,

    // ---- Advanced: the hardware's fixed constants, made reachable ----
    //
    // Every one of these defaults to the value the machine has, so the init patch is
    // the machine. They are **configurations**, not amounts, so they start somewhere
    // useful rather than at zero — and none of them is smoothed, because a constant
    // is not a signal.
    /// The amplitude envelope's attack. [`envelope::ATTACK_S`] on the hardware.
    pub amp_attack_s: f32,
    /// The amplitude envelope's decay. [`envelope::AMP_DECAY_S`] on the hardware,
    /// long enough that a note is flat within a step.
    pub amp_decay_s: f32,
    /// The filter envelope's attack. [`envelope::ATTACK_S`] on the hardware.
    pub filter_attack_s: f32,
    /// The decay an accented note is forced to. [`envelope::DECAY_MIN_S`] on the
    /// hardware, where accent shorts out the Decay control.
    pub accent_decay_s: f32,
    /// Stretches the accent sweep's time constants together, `1.0` being the
    /// hardware. See [`Accent::process`].
    pub accent_sweep: f32,
    /// How hard the oscillator drives the filter. [`DRIVE`] on the hardware.
    pub drive: f32,
    /// Which diode ladder configuration the filter models.
    pub filter_model: DiodeConfig,
}

/// The lowest filter-envelope depth the machine's Env Mod can reach, as a fraction of
/// [`ENV_MOD_OCTAVES`].
///
/// **A deliberate deviation from the collection's init-patch contract**, which
/// says every amount starts at zero. The hardware's circuit cannot turn Env Mod
/// down to zero, and this instrument's standard is fidelity. **It is circuit, not an
/// amount**: the voice adds `routing::ENV_MOD_FLOOR_OCTAVES` of the envelope whatever is
/// routed, and the (Cutoff ← Filter envelope) route sums on top of it. Recorded here and in
/// `plugins/mxm-mono-03/AGENTS.md` so it reads as a decision rather than an oversight.
pub const ENV_MOD_FLOOR: f32 = 0.08;

/// How far the filter envelope reaches on the cutoff at full Env Mod, in octaves — the floor and the
/// route together.
pub const ENV_MOD_OCTAVES: f32 = 5.0;

/// How far the accent circuit's filter output reaches on the cutoff per unit of its signal, in
/// octaves.
pub const ACCENT_OCTAVES: f32 = 4.0;

impl Default for Params {
    /// The init patch.
    fn default() -> Self {
        Self {
            tune_semitones: 0.0,
            expression_semitones: 0.0,
            waveform: Waveform::Sawtooth,
            cutoff: 0.5,
            resonance: 0.0,
            decay: 0.5,
            accent: 0.0,
            glide_s: SLIDE_TIME_S,
            volume: 0.8,
            wheel: 0.0,
            pressure: 0.0,
            bend: 0.0,

            amp_attack_s: envelope::ATTACK_S,
            amp_decay_s: envelope::AMP_DECAY_S,
            filter_attack_s: envelope::ATTACK_S,
            accent_decay_s: envelope::DECAY_MIN_S,
            accent_sweep: 1.0,
            drive: DRIVE,
            filter_model: DiodeConfig::Tb303,
        }
    }
}

/// The hardware's slide time constant.
///
/// **Published, not measured** — `docs/modulation/04-glide-and-portamento.md` §4.6
/// says so twice and warns against building a test around it. It is a starting
/// point for a listening comparison, which is why no test here asserts it.
///
/// **Fixed, and not a control.** The hardware's slide is an RC in the pitch path
/// and its per-step slide button is on or off, so the only decision anyone makes
/// is *whether* a transition slides. `Params::glide_s` exists so this crate's
/// tests can vary it and a future measurement can correct it — the plugin always
/// passes this constant.
pub const SLIDE_TIME_S: f32 = 0.060;

/// The monophonic voice.
#[derive(Debug, Clone)]
pub struct Voice {
    osc: Oscillator,
    filter: DiodeLadder,
    filter_env: AttackDecay,
    amp_env: AttackDecay,
    accent: Accent,

    /// Target pitch, in MIDI note numbers.
    target_note: f32,
    /// The slide's **remaining distance** from `target_note`, in semitones: zero unless a slide is
    /// in progress, and the lagged pitch is `target_note + glide_offset`.
    ///
    /// Kept as the distance rather than the pitch because the pitch form,
    /// `target + (glide − target) × c`, stops moving in `f32` once a step is under half an ulp of
    /// the target — at the circuit's 60 ms that was a cent short, well above the 1e-4 snap meant to
    /// end it, so a slid note rested flat until the next one. The distance keeps full precision down
    /// to the snap.
    glide_offset: f32,
    /// Whether the gate is open — a note is being held.
    ///
    /// **This, not the amplitude envelope, is what decides retriggering.** See
    /// [`Voice::note_on`].
    gate: bool,
    /// Whether anything has sounded yet, so the first note of a phrase cannot slide
    /// from a stale pitch.
    started: bool,
    /// The velocity of the note-on that started the sounding note. A routing source.
    velocity: f32,

    /// The routing frame and the live routes into each target.
    graph: Graph,

    /// DC blocker state.
    dc_x: f32,
    dc_y: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

impl Voice {
    pub fn new() -> Self {
        Self {
            osc: Oscillator::new(),
            filter: DiodeLadder::new(DiodeConfig::Tb303),
            filter_env: AttackDecay::new(),
            amp_env: AttackDecay::new(),
            accent: Accent::new(),
            target_note: 60.0,
            glide_offset: 0.0,
            gate: false,
            started: false,
            // Full before any note, so the standard Velocity rests at zero.
            velocity: 1.0,
            graph: Graph::new(),
            dc_x: 0.0,
            dc_y: 0.0,
        }
    }

    /// Clear everything. Must leave no tail from previous playback — **the routing frame included**,
    /// so a route that reads a source before this sample publishes it reads zero rather than the
    /// last phrase.
    pub fn reset(&mut self) {
        self.osc.reset();
        self.filter.reset();
        self.filter_env.reset();
        self.amp_env.reset();
        self.accent.reset();
        self.gate = false;
        self.started = false;
        self.velocity = 1.0;
        self.graph.reset();
        self.dc_x = 0.0;
        self.dc_y = 0.0;
    }

    /// Which routes are live, for every sample until the next call. **Once per processing interval,
    /// never per sample**; a source that has just become read is cleared first.
    pub fn set_topology(&mut self, routing: &Routing) {
        self.graph.set_topology(routing);
    }

    pub fn is_active(&self) -> bool {
        self.amp_env.is_active()
    }

    /// The accent sweep's accumulated charge, for a display.
    ///
    /// **Read-only, additive, and it earns its place the way the crate's contract requires**: the
    /// editor's brief §8 asks for a display of the climb across consecutive accents, and there is
    /// no other source — `accent` is private and `process` returns only a sample. See
    /// `crates/mxm-mono-01-dsp/AGENTS.md`'s rule about telemetry accessors, which this follows: no
    /// new state, no branch in the audio path, and the DSP does not know anyone is looking.
    pub fn accent_sweep(&self) -> f32 {
        self.accent.sweep()
    }

    /// What the routing frame holds for a source — this sample's value if a live route made it
    /// publish, otherwise the last one. **Read-only**, under the same rule as
    /// [`Voice::accent_sweep`]: no state, no branch in the audio path.
    pub fn published(&self, source: usize) -> f32 {
        self.graph.read(source)
    }

    /// How many samples of **audible** output are still to come.
    ///
    /// A host uses this to decide how long to keep processing after the last event, so it has to
    /// describe what can be heard rather than what is still ticking internally. It is the time for
    /// the amplitude envelope's current exponential to fall from where it is to
    /// [`envelope::EPSILON`] — the release when a note has been let go, the long decay while one is
    /// still held.
    ///
    /// Reporting the filter envelope's state here would be the mistake worth naming: it is behind
    /// the amplifier, so it can be busy while nothing is audible.
    pub fn tail_samples(&self, amp_decay_s: f32, sample_rate: f32) -> u32 {
        let level = self.amp_env.level();
        if level <= envelope::EPSILON {
            return 0;
        }
        let tau = match self.amp_env.stage() {
            envelope::Stage::Release => envelope::AMP_RELEASE_S,
            // The *longest* the decay can be set to, because the tail is reported
            // before the next patch is read and must not under-promise.
            _ => amp_decay_s,
        };
        (tau * (level / envelope::EPSILON).ln() * sample_rate).max(0.0) as u32
    }

    /// Start a note.
    ///
    /// # Two independent things, and keeping them apart is the whole design
    ///
    /// **`slide` is the hardware's slide button** — a per-note switch saying this
    /// transition glides. It is the patch's decision, and on the hardware it is a
    /// per-step flag in the sequencer.
    ///
    /// **Whether the envelopes retrigger is not that switch's business.** It follows
    /// the *gate*: a note arriving while one is held is a legato joint, so nothing
    /// retriggers and nothing is accented, there being no new note-on. That is the
    /// host's doing — a tie holds the gate open — and the plugin only observes it.
    ///
    /// The machine couples the two, because one button does both jobs. Here they
    /// arrive by different routes, so they are read separately:
    ///
    /// | gate held | `slide` | what happens |
    /// |---|---|---|
    /// | no | no | an ordinary note |
    /// | no | yes | glides in, but attacks — a bend into a new note |
    /// | yes | no | a legato pitch change, no glide |
    /// | yes | yes | **the hardware's slide**: glides in, no attack, no accent |
    ///
    /// `accent` is `0..=1` and is ignored while the gate is held. **So is `velocity`**, the Velocity
    /// source's value: a joint changes it no more than it retriggers the envelopes.
    ///
    /// # A slide can never make a note silent
    ///
    /// Retrigger is decided by the gate rather than by "is the amplifier still
    /// ringing", and the difference between those is a silent note: the release takes
    /// about a tenth of a second to fall below the idle threshold, so a note released
    /// halfway through its step is *still audible* when the next begins. Suppressing
    /// the attack there hands the new note a release already most of the way to zero,
    /// and it never speaks. `a_slide_from_a_released_note_still_speaks` holds it, and
    /// the acid demo is what found it.
    pub fn note_on(&mut self, note: u8, accent: f32, slide: bool, velocity: f32) {
        // The host is holding a note, so this is a legato joint whatever the switch says.
        let held = self.gate;

        // A slide needs a previous pitch to leave. The first note of a phrase has none,
        // so it is placed rather than lagged and cannot swoop up from whatever the last
        // phrase left behind.
        let slide = slide && self.started;

        let from = self.target_note + self.glide_offset;
        self.target_note = note as f32;
        self.glide_offset = if slide { from - self.target_note } else { 0.0 };
        self.started = true;
        self.gate = true;

        if !held {
            self.filter_env.trigger();
            self.amp_env.trigger();
            self.accent.engage(accent);
            self.velocity = velocity;
        }
    }

    /// Release the note.
    pub fn note_off(&mut self) {
        self.gate = false;
        self.amp_env.release();
        self.accent.release();
    }

    /// Stop immediately, leaving no tail. For a host's choke or panic.
    pub fn choke(&mut self) {
        self.reset();
    }

    /// The nominal cutoff a control position means, at this sample rate.
    pub fn cutoff_hz(&self, position: f32, sample_rate: f32) -> f32 {
        let position = position.clamp(0.0, 1.0);
        let hz = CUTOFF_LOW_HZ * (CUTOFF_HIGH_HZ / CUTOFF_LOW_HZ).powf(position);
        hz.min(filter::max_nominal_cutoff_hz(
            self.filter.config(),
            sample_rate,
        ))
    }

    /// One sample. `routing` carries the live routes' amounts, into the topology
    /// [`Voice::set_topology`] set.
    #[inline]
    pub fn process(&mut self, p: &Params, routing: &Routing, sample_rate: f32) -> f32 {
        // --- pitch -------------------------------------------------------
        // A one-pole lag in the pitch path. `glide_s` is a *time constant*, not a
        // duration: after it, the pitch has covered 63.2% of the interval.
        if self.glide_offset != 0.0 {
            if p.glide_s <= 0.0 {
                self.glide_offset = 0.0;
            } else {
                let c = (-1.0 / (p.glide_s * sample_rate)).exp();
                self.glide_offset *= c;
                // Snap once the remaining error is far below a cent, so the target
                // is actually reached rather than approached forever.
                if self.glide_offset.abs() < 1e-4 {
                    self.glide_offset = 0.0;
                }
            }
        }
        let glide_note = self.target_note + self.glide_offset;
        let hz = 440.0
            * 2.0f32.powf((glide_note + p.tune_semitones + p.expression_semitones - 69.0) / 12.0);

        // --- envelopes ---------------------------------------------------
        // Accent overrides the Decay control for this note only, which is what
        // makes an accented note a different envelope rather than a louder one.
        let decay_s = if self.accent.forces_short_decay() {
            p.accent_decay_s
        } else {
            envelope::decay_time_s(p.decay)
        };
        let fenv = self
            .filter_env
            .process(p.filter_attack_s, decay_s, decay_s, sample_rate);
        let aenv = self.amp_env.process(
            p.amp_attack_s,
            p.amp_decay_s,
            envelope::AMP_RELEASE_S,
            sample_rate,
        );

        self.accent.process(p.accent_sweep, sample_rate);

        // --- routing sources ---------------------------------------------
        // **Nothing routed costs nothing**: with no live route the frame is not opened and no sum is
        // taken, and the cutoff is the circuit's floor alone. Otherwise every source is published
        // before any target reads, which is `routing`'s declared order — the performance sources
        // through the collection's standard, so each is zero at its rest.
        let routed = self.graph.any_live();
        if routed {
            self.graph.begin_sample();
            self.graph
                .write(source::KEY, standard::key(glide_note, KEY_UNIT_SEMITONES));
            self.graph
                .write(source::VELOCITY, standard::velocity(self.velocity));
            self.graph.write(source::WHEEL, standard::wheel(p.wheel));
            self.graph
                .write(source::PRESSURE, standard::pressure(p.pressure));
            self.graph.write(source::BEND, standard::bend(p.bend));
            self.graph.write(source::FILTER_ENVELOPE, fenv);
            // The accent circuit's two outputs, at the Accent knob's depth (plan D1 a). The filter
            // output reads the resonance **control**, unmodulated — the pot's second section — so
            // nothing routed into Resonance makes a cycle through it (N3).
            self.graph.write(
                source::ACCENT,
                p.accent * self.accent.cutoff_offset(fenv, p.resonance) * ACCENT_UNIT,
            );
            self.graph
                .write(source::ACCENT_LEVEL, p.accent * self.accent.vca_boost(fenv));
        }

        // Control-rate: the pole set changes, so it is set before the sample rather
        // than inside it.
        if self.filter.config() != p.filter_model {
            self.filter.set_config(p.filter_model);
        }

        let osc = self.osc.process(hz, p.waveform, sample_rate);
        if routed {
            self.graph.write(source::OSCILLATOR, osc);
        }

        // --- filter ------------------------------------------------------
        let base = self.cutoff_hz(p.cutoff, sample_rate);
        // Envelope and accent open the filter multiplicatively, in octaves, so a
        // given depth means the same musical distance wherever the control sits. The circuit's
        // floor is added whatever is routed; the routes sum on top of it.
        let routed_octaves = if routed {
            self.graph.sum(target::CUTOFF, routing)
        } else {
            0.0
        };
        let cutoff = base * 2.0f32.powf(cutoff_octaves(fenv, routed_octaves));
        let resonance = if routed && !self.graph.is_empty(target::RESONANCE) {
            (p.resonance + self.graph.sum(target::RESONANCE, routing)).clamp(0.0, 1.0)
        } else {
            p.resonance
        };

        let filtered = self
            .filter
            .process(p.drive * osc, cutoff, resonance, sample_rate);

        // --- amplifier ---------------------------------------------------
        // **Amplitude routes scale the envelope and never add to it**, so a note's life stays the
        // amplitude envelope's: no route can make a finished note sound (plan N7). The collection's
        // one amplitude law, silence to double.
        let boost = if routed {
            standard::amplitude_factor(self.graph.sum(target::AMPLITUDE, routing))
        } else {
            1.0
        };
        let mut y = filtered * MAKEUP * aenv * boost * p.volume;

        // DC blocker, on the output only.
        let r = 1.0 - (std::f32::consts::TAU * DC_BLOCK_HZ / sample_rate);
        let out = y - self.dc_x + r * self.dc_y;
        self.dc_x = flush(y);
        self.dc_y = flush(out);
        y = out;

        flush(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    /// The init patch's routing — the machine's own three paths — with the Env Mod route at
    /// `env_mod`, which is the amount the retired knob's normalised position maps to (plan §3).
    fn machine(env_mod: f32) -> Routing {
        let mut r = Routing::init();
        r.amounts[target::CUTOFF][source::FILTER_ENVELOPE] = env_mod;
        r
    }

    fn render(v: &mut Voice, p: &Params, r: &Routing, seconds: f32) -> Vec<f32> {
        v.set_topology(r);
        (0..(FS * seconds) as usize)
            .map(|_| v.process(p, r, FS))
            .collect()
    }

    /// Dominant frequency by counting zero crossings of a heavily smoothed signal.
    fn rough_hz(x: &[f32]) -> f32 {
        let mut crossings = 0usize;
        let mut prev = 0.0f32;
        for &s in x {
            if prev <= 0.0 && s > 0.0 {
                crossings += 1;
            }
            prev = s;
        }
        crossings as f32 / (x.len() as f32 / FS)
    }

    #[test]
    fn a_pitch_expression_moves_the_pitch_exactly_as_the_tuning_control_does() {
        // **The whole contract, stated as an equivalence.** A per-note expression is semitones on
        // the same pitch as everything else, added at the same point — so seven semitones of
        // expression must render bit-for-bit what seven semitones of tuning renders. Anything else
        // means it reached the oscillator by a different route, or reached something else too.
        let tuned = Params {
            tune_semitones: 7.0,
            ..Params::default()
        };
        let expressed = Params {
            expression_semitones: 7.0,
            ..Params::default()
        };
        let r = Routing::init();

        let mut a = Voice::new();
        let mut b = Voice::new();
        a.note_on(48, 0.0, false, 0.8);
        b.note_on(48, 0.0, false, 0.8);

        assert_eq!(
            render(&mut a, &tuned, &r, 0.25),
            render(&mut b, &expressed, &r, 0.25),
            "expression and tuning must be the same semitones on the same pitch"
        );
    }

    #[test]
    fn a_pitch_expression_rides_on_a_slide_rather_than_replacing_it() {
        // The offset is added *after* the glide lag, so a drawn bend bends the sliding pitch
        // instead of fighting it: mid-slide, the expressed voice sits a fixed interval above the
        // plain one and both are still moving. Measured as a frequency ratio, which is what
        // "a fixed interval" means once the pitch is exponential.
        let plain = Params::default();
        let expressed = Params {
            expression_semitones: 12.0,
            ..Params::default()
        };
        let r = Routing::init();

        let mut a = Voice::new();
        let mut b = Voice::new();
        a.note_on(36, 0.0, false, 0.8);
        b.note_on(36, 0.0, false, 0.8);
        render(&mut a, &plain, &r, 0.2);
        render(&mut b, &expressed, &r, 0.2);

        // A slide to a note an octave up: both voices are lagging towards it.
        a.note_on(48, 0.0, true, 0.8);
        b.note_on(48, 0.0, true, 0.8);
        let mid_a = rough_hz(&render(&mut a, &plain, &r, 0.02));
        let mid_b = rough_hz(&render(&mut b, &expressed, &r, 0.02));

        let ratio = mid_b / mid_a;
        assert!(
            (ratio - 2.0).abs() < 0.06,
            "an octave of expression stays an octave mid-slide, measured {ratio}"
        );
        assert!(
            (70.0..125.0).contains(&mid_a),
            "the premise: mid-slide, between the note left (65 Hz) and the note aimed at              (131 Hz), so the interval above is being measured on a pitch that is still              moving — measured {mid_a} Hz"
        );
    }

    #[test]
    fn silence_in_gives_exactly_zero_out() {
        let p = Params::default();
        let r = Routing::init();
        let mut v = Voice::new();
        v.set_topology(&r);
        v.note_on(48, 0.0, false, 0.8);
        for _ in 0..(FS * 0.1) as usize {
            v.process(&p, &r, FS);
        }
        v.note_off();
        for _ in 0..(FS * 5.0) as usize {
            v.process(&p, &r, FS);
        }
        let y = v.process(&p, &r, FS);
        assert_eq!(y, 0.0, "not exactly silent: {y}");
        assert!(!v.is_active());
    }

    #[test]
    fn reset_leaves_no_tail() {
        let p = Params::default();
        let r = Routing::init();
        let mut v = Voice::new();
        v.set_topology(&r);
        v.note_on(48, 1.0, false, 0.8);
        for _ in 0..1000 {
            v.process(&p, &r, FS);
        }
        v.reset();
        assert_eq!(v.process(&p, &r, FS), 0.0);
    }

    #[test]
    fn no_nan_or_inf_across_a_parameter_sweep() {
        let r = machine(1.0);
        for fs in [44_100.0f32, 48_000.0, 96_000.0, 192_000.0] {
            for cut in 0..=4 {
                for res in 0..=4 {
                    for acc in [0.0f32, 1.0] {
                        let p = Params {
                            cutoff: cut as f32 / 4.0,
                            resonance: res as f32 / 4.0,
                            accent: acc,
                            ..Params::default()
                        };
                        let mut v = Voice::new();
                        v.set_topology(&r);
                        v.note_on(36, acc, false, 0.8);
                        for _ in 0..(fs * 0.05) as usize {
                            let y = v.process(&p, &r, fs);
                            assert!(y.is_finite(), "non-finite at fs={fs} p={p:?}: {y}");
                        }
                    }
                }
            }
        }
    }

    /// **A slide lands exactly on its note.** The pitch form, `target + (glide − target) × c`,
    /// stopped moving once a step fell under half an ulp of the target, a cent short at the
    /// circuit's 60 ms and 48 kHz and further at longer times and higher rates — above the 1e-4
    /// snap meant to end it, so a slid note rested flat until the next one.
    ///
    /// Falsified before trusted: with the pitch form back, it rests at 71.99 at 48 kHz.
    #[test]
    fn a_slide_lands_exactly_on_its_note() {
        for rate in [48_000.0f32, 96_000.0] {
            for glide_s in [SLIDE_TIME_S, 0.5] {
                let p = Params {
                    glide_s,
                    ..Params::default()
                };
                let r = Routing::init();
                let mut v = Voice::new();
                v.note_on(48, 0.0, false, 0.8);
                v.process(&p, &r, rate);
                v.note_on(72, 0.0, true, 0.8);
                for _ in 0..(30.0 * glide_s * rate) as usize {
                    v.process(&p, &r, rate);
                }
                assert_eq!(
                    v.target_note + v.glide_offset,
                    72.0,
                    "a {glide_s} s slide at {rate} Hz rests short"
                );
            }
        }
    }

    #[test]
    fn the_first_note_of_a_phrase_does_not_glide() {
        // Otherwise a patch with a long glide opens with a swoop from wherever the
        // previous phrase left the lag.
        let p = Params {
            glide_s: 0.5,
            ..Params::default()
        };
        let r = Routing::init();
        let mut v = Voice::new();
        v.note_on(72, 0.0, true, 0.8); // slide switch on, but nothing has sounded yet
        let early = render(&mut v, &p, &r, 0.02);
        let hz = rough_hz(&early);
        let expected = 440.0 * 2.0f32.powf((72.0 - 69.0) / 12.0);
        assert!(
            (hz - expected).abs() / expected < 0.15,
            "first note started at {hz} Hz, expected about {expected}"
        );
    }

    #[test]
    fn the_slide_switch_decides_whether_a_note_glides() {
        // The switch, not the gate. Measured on rendered audio rather than read back from the
        // pitch state.
        let p = Params::default();
        let r = Routing::init();
        let low = 440.0 * 2.0f32.powf((48.0 - 69.0) / 12.0);
        let high = 440.0 * 2.0f32.powf((60.0 - 69.0) / 12.0);

        // Switch on: the second note must *begin* near the first note's pitch.
        let mut slid = Voice::new();
        slid.note_on(48, 0.0, false, 0.8);
        render(&mut slid, &p, &r, 0.2);
        slid.note_on(60, 0.0, true, 0.8);
        let slid_hz = rough_hz(&render(&mut slid, &p, &r, 0.01));

        // Switch off: it must begin at its own pitch, gate held or not.
        let mut plain = Voice::new();
        plain.note_on(48, 0.0, false, 0.8);
        render(&mut plain, &p, &r, 0.2);
        plain.note_on(60, 0.0, false, 0.8);
        let plain_hz = rough_hz(&render(&mut plain, &p, &r, 0.01));

        assert!(
            (plain_hz - high).abs() < (plain_hz - low).abs(),
            "with the switch off a note must arrive in tune: {plain_hz} Hz, target {high}"
        );
        assert!(
            slid_hz < plain_hz * 0.85,
            "with it on the note must start below its target: slid {slid_hz} vs plain {plain_hz}"
        );
    }

    #[test]
    fn the_gate_decides_retriggering_and_the_switch_does_not() {
        // The pair the hardware couples and this voice reads separately. A held gate suppresses
        // the attack whatever the switch says; a closed one triggers, likewise.
        let p = Params {
            decay: 1.0,
            ..Params::default()
        };
        let r = machine(1.0);

        // Gate held, switch off: still a legato joint, so no new filter sweep.
        let mut held = Voice::new();
        held.note_on(48, 0.0, false, 0.8);
        render(&mut held, &p, &r, 0.4);
        let before = held.filter_env.level();
        held.note_on(60, 0.0, false, 0.8);
        held.process(&p, &r, FS);
        assert!(
            held.filter_env.level() <= before,
            "a held gate must not retrigger, switch or no switch"
        );

        // Gate closed, switch on: a glide, but it attacks. Anything else would be silent.
        let mut fresh = Voice::new();
        fresh.note_on(48, 0.0, false, 0.8);
        render(&mut fresh, &p, &r, 0.2);
        fresh.note_off();
        render(&mut fresh, &p, &r, 0.3);
        fresh.note_on(60, 0.0, true, 0.8);
        render(&mut fresh, &p, &r, 0.02);
        assert!(
            fresh.filter_env.level() > 0.5,
            "a closed gate must trigger even with the switch on, got {}",
            fresh.filter_env.level()
        );
    }

    #[test]
    fn a_slide_from_a_released_note_still_speaks() {
        // **A note that vanishes is the worst possible outcome**, and it is what asking the
        // amplitude envelope rather than the gate produced: the release is slow enough that a note
        // let go halfway through its step is still ringing when the next one starts, so the next
        // note was treated as slid, retriggered nothing, and inherited a dying release.
        //
        // Found by the acid demo, where every slid step rendered silent. Change the condition in
        // `note_on` back to `amp_env.is_active()` and this goes red.
        let p = Params::default();
        let r = Routing::init();
        let mut v = Voice::new();
        v.note_on(48, 0.0, false, 0.8);
        render(&mut v, &p, &r, 0.06);
        v.note_off();
        render(&mut v, &p, &r, 0.06); // still ringing, but the gate is shut

        v.note_on(60, 0.0, true, 0.8); // switch on, but the gate is shut
        let y = render(&mut v, &p, &r, 0.05);
        let peak = y.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.01, "the note never spoke: peak {peak}");
    }

    #[test]
    fn a_slid_destination_retriggers_nothing_and_takes_no_accent() {
        // `docs/modulation/04-glide-and-portamento.md` §4.4: "no new attack, no new
        // filter sweep, no accent on the second one." Getting this wrong gives two
        // notes with a bend between them, which is an ordinary sound.
        let p = Params {
            decay: 1.0,
            accent: 1.0,
            ..Params::default()
        };
        let r = machine(1.0);
        let mut v = Voice::new();
        v.note_on(48, 0.0, false, 0.8);
        render(&mut v, &p, &r, 0.5); // gate still open, which is what makes this a joint
        let before = v.filter_env.level();
        v.note_on(60, 1.0, true, 0.8);
        v.process(&p, &r, FS);
        let after = v.filter_env.level();
        assert!(
            after <= before,
            "a slid note must not retrigger the filter envelope: {before} then {after}"
        );
        assert!(
            !v.accent.forces_short_decay(),
            "a slid note must not take accent"
        );
    }

    #[test]
    fn an_accented_note_is_a_different_envelope_not_a_louder_one() {
        // Accent forces the filter envelope's decay to its minimum, overriding the
        // Decay control. With Decay at maximum the difference is unmissable.
        let p = Params {
            decay: 1.0,
            accent: 1.0,
            ..Params::default()
        };
        let r = machine(1.0);
        let mut plain = Voice::new();
        plain.note_on(48, 0.0, false, 0.8);
        render(&mut plain, &p, &r, 0.4);

        let mut accented = Voice::new();
        accented.note_on(48, 1.0, false, 0.8);
        render(&mut accented, &p, &r, 0.4);

        assert!(
            accented.filter_env.level() < plain.filter_env.level() * 0.5,
            "accent should have collapsed the filter envelope: {} vs {}",
            accented.filter_env.level(),
            plain.filter_env.level()
        );
    }

    #[test]
    fn the_cutoff_control_is_a_position_and_never_exceeds_the_poles_headroom() {
        // The widest pole is what must stay below Nyquist, so the control's top
        // moves with the sample rate rather than the model detuning.
        let v = Voice::new();
        for fs in [44_100.0f32, 48_000.0, 96_000.0] {
            let top = v.cutoff_hz(1.0, fs);
            let widest = DiodeConfig::Tb303.widest_pole();
            assert!(
                top * widest <= filter::NYQUIST_FRACTION * fs + 1.0,
                "at {fs} Hz the top of the control puts the widest pole past Nyquist"
            );
            assert!(v.cutoff_hz(0.0, fs) < v.cutoff_hz(1.0, fs));
        }
    }

    #[test]
    fn the_init_patch_has_no_accent_and_no_resonance() {
        // `plugins/AGENTS.md`: every *amount* starts at zero. Env Mod's floor is the circuit's and
        // its route starts at zero; the accent circuit's two routes start at full, but carry
        // nothing while the Accent knob does.
        let p = Params::default();
        assert_eq!(p.accent, 0.0);
        assert_eq!(p.resonance, 0.0);
        assert_eq!(p.glide_s, SLIDE_TIME_S, "glide is a configuration here");
        assert_eq!(
            Routing::init().amounts[target::CUTOFF][source::FILTER_ENVELOPE],
            0.0,
            "Env Mod starts at the circuit's floor: its route at zero"
        );
    }

    #[test]
    fn the_advanced_controls_change_what_they_name() {
        // Eight permanent ids is a real cost, so each one has to earn it by moving the
        // sound. "It compiles" is not evidence.
        let base = Params {
            decay: 1.0,
            ..Params::default()
        };
        let r = machine(1.0);

        // A long amp decay holds the level up where the hardware's would have fallen.
        let level_after = |amp_decay_s: f32| {
            let p = Params {
                amp_decay_s,
                ..base
            };
            let mut v = Voice::new();
            v.note_on(48, 0.0, false, 0.8);
            let y = render(&mut v, &p, &r, 1.0);
            let tail = &y[y.len() - (FS * 0.05) as usize..];
            tail.iter().fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert!(
            level_after(0.25) < level_after(3.0) * 0.5,
            "amp decay must shape the note's fall"
        );

        // A slow attack is behind a fast one shortly after the note starts.
        let early = |amp_attack_s: f32| {
            let p = Params {
                amp_attack_s,
                ..base
            };
            let mut v = Voice::new();
            v.note_on(48, 0.0, false, 0.8);
            let y = render(&mut v, &p, &r, 0.01);
            y.iter().fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert!(
            early(0.3) < early(0.003) * 0.6,
            "amp attack must shape the rise"
        );

        // The EMS pole set is a different filter. Compared **signal against signal**
        // rather than by total energy: resonance is normalised against each family's
        // own threshold (as it must be), so the two sit at the same musical position
        // and differ in corner shape, which a loudness comparison would miss.
        let render_model = |filter_model: DiodeConfig| {
            let p = Params {
                filter_model,
                resonance: 0.55,
                ..base
            };
            let mut v = Voice::new();
            v.note_on(36, 0.0, false, 0.8);
            render(&mut v, &p, &r, 0.5)
        };
        let tb = render_model(DiodeConfig::Tb303);
        let ems = render_model(DiodeConfig::Ems);
        let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
        let diff: Vec<f32> = tb.iter().zip(&ems).map(|(a, b)| a - b).collect();
        assert!(
            rms(&diff) > rms(&tb) * 0.01,
            "the two pole sets must not sound the same: difference {} against signal {}",
            rms(&diff),
            rms(&tb)
        );

        // Drive is what gets a signal through the ladder's droop at all.
        let driven = |drive: f32| {
            let p = Params {
                drive,
                resonance: 0.7,
                ..base
            };
            let mut v = Voice::new();
            v.note_on(36, 0.0, false, 0.8);
            let y = render(&mut v, &p, &r, 0.3);
            y.iter().fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert!(driven(1.0) < driven(12.0), "drive must reach the output");
    }

    #[test]
    fn the_init_patch_is_the_hardware() {
        // Every advanced control defaults to the machine's own value, so Init sounds
        // like the machine and the disclosure changes nothing until it is opened.
        let p = Params::default();
        assert_eq!(p.amp_attack_s, envelope::ATTACK_S);
        assert_eq!(p.filter_attack_s, envelope::ATTACK_S);
        assert_eq!(p.amp_decay_s, envelope::AMP_DECAY_S);
        assert_eq!(p.accent_decay_s, envelope::DECAY_MIN_S);
        assert_eq!(p.accent_sweep, 1.0);
        assert_eq!(p.drive, DRIVE);
        assert_eq!(p.filter_model, DiodeConfig::Tb303);
        assert_eq!(p.glide_s, SLIDE_TIME_S);
    }

    #[test]
    fn the_output_is_free_of_dc() {
        let p = Params {
            resonance: 0.8,
            ..Params::default()
        };
        let r = machine(1.0);
        let mut v = Voice::new();
        v.note_on(36, 1.0, false, 0.8);
        let y = render(&mut v, &p, &r, 0.5);
        let mean = y.iter().map(|s| *s as f64).sum::<f64>() / y.len() as f64;
        assert!(mean.abs() < 0.01, "DC offset {mean}");
    }
}
