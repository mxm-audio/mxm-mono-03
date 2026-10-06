//! What mxm-mono-03 can modulate, and with what.
//!
//! `plans/plan-mxm-mono-03-modulation.md` §2–§5, under `plans/plan-modulation-routing.md` §9 M5. The
//! shared machinery is [`mxm_modulation`]; this module is the instrument's own declaration — its
//! **source list**, its **target list**, each pair's **full scale**, the circuit's **Env Mod floor**
//! and **which routes the init patch holds**. Nothing in the voice asks whether a route is the
//! machine's own: [`INIT_PRESENT`] and [`INIT_AT_FULL`] are the whole of that.
//!
//! # The machine has almost no modulation, and all of it is here
//!
//! No LFO, one filter envelope, an accent circuit and a slide. Three paths reach anything: the filter
//! envelope on the cutoff through Env Mod, and the accent circuit's two outputs — one on the cutoff,
//! one on the amplifier. Each is a route present at Init.
//!
//! # Evaluation order
//!
//! **Every source is published before any target reads**, so no route this instrument can make is
//! backward: the key after the slide's lag, velocity, the wheel, pressure and the bender, the filter
//! envelope, the accent circuit's two outputs, then the oscillator — and only then do the cutoff, the
//! resonance and the amplitude take their sums. The envelopes and the accent circuit do not depend on
//! the pitch, so computing them before the oscillator changes no arithmetic.
//!
//! # The Env Mod floor is circuit, not an amount
//!
//! The machine cannot turn its envelope's reach on the cutoff down to zero. `5·(F + (1 − F)·a)·fenv`
//! splits exactly into a term the circuit always adds, [`ENV_MOD_FLOOR_OCTAVES`] `× fenv`, and the
//! (Cutoff ← Filter envelope) route at amount `a` with the rest of the reach as its scale. So Init —
//! that route present at zero — is the floor, removing the route leaves the floor, and a stored Env Mod
//! at normalised `n` is the route at `a = n` (plan §3). A negative amount inverts the envelope and, at
//! `a = −F / (1 − F)`, cancels the floor: a new option, labelled rather than hidden.
//!
//! # Everything the machine did not have is the collection's standard
//!
//! Key, Velocity, Wheel, Pressure and Bend mean what they mean on every instrument, and a route the
//! TB-303 never had reaches what it reaches on every instrument ([`mxm_modulation::standard`];
//! `plans/plan-modulation-standard.md`). Velocity is `v − 1`, so a route from it does nothing at the
//! hardest note; Amplitude is the standard factor, silence to double — which is exactly the accent's
//! reach, so the machine's accent is the law's top.

use mxm_modulation::standard::{self, AMPLITUDE_SUM_BOUND, Law, Offer, Performance, reach};
use mxm_modulation::{Compacted, SourceFrame};

use crate::accent::SWEEP_CEILING;
use crate::voice::{ACCENT_OCTAVES, ENV_MOD_FLOOR, ENV_MOD_OCTAVES};

/// Every source this instrument can route, in **declared publication order** (module doc).
pub mod source {
    /// The key **after the slide's lag**, as a signed distance from middle C over
    /// [`super::KEY_UNIT_SEMITONES`] — so a slide moves a key-tracked target with the pitch (plan N2).
    pub const KEY: usize = 0;
    /// The velocity of the note-on that started the sounding note, published as `v − 1`
    /// (`standard::velocity`): zero at the hardest note. A slid-into note keeps it, as it keeps its
    /// envelopes and takes no accent — the press that last triggered the envelope.
    pub const VELOCITY: usize = 1;
    /// The mod wheel on the sounding note's channel, `0..=1`.
    pub const WHEEL: usize = 2;
    /// Channel pressure on the sounding note's channel, `0..=1`.
    pub const PRESSURE: usize = 3;
    /// The bender's lever position, signed — not its reach in semitones.
    pub const BEND: usize = 4;
    /// The filter envelope, `0..=1`.
    pub const FILTER_ENVELOPE: usize = 5;
    /// **The accent circuit's filter output**, at the Accent knob's depth: extra envelope at low
    /// resonance, the climbing sweep at high. Published at [`super::ACCENT_UNIT`] of its value,
    /// because the sweep peaks over unit magnitude (plan N9).
    pub const ACCENT: usize = 6;
    /// **The accent circuit's amplifier output**, at the Accent knob's depth, `0..=1`. A second
    /// source because the circuit's two outputs carry different signals.
    pub const ACCENT_LEVEL: usize = 7;
    /// The oscillator's selected waveform before the drive — the machine's only audio.
    pub const OSCILLATOR: usize = 8;
}

/// How many sources the instrument declares.
pub const SOURCES: usize = 9;

/// Their names, in source order, for the interface and for accessibility.
pub const SOURCE_NAMES: [&str; SOURCES] = [
    "Key",
    "Velocity",
    "Wheel",
    "Pressure",
    "Bend",
    "Filter envelope",
    "Accent",
    "Accent level",
    "Oscillator",
];

/// Every target this instrument declares — **the three a 303 is played through** (plan D3). Pitch is
/// the first append candidate; appending moves no id.
pub mod target {
    /// The cutoff, summed in octaves **on top of the circuit's Env Mod floor**.
    pub const CUTOFF: usize = 0;
    /// The resonance control, summed in its own `0..=1` and clamped to it. The accent circuit reads
    /// the control as set, not as modulated (plan N3), so no route makes a cycle through it.
    pub const RESONANCE: usize = 1;
    /// The amplifier: **multiplies** the amplitude envelope, `aenv × max(0, 1 + Σ)`, so a route
    /// scales a note and never outlives it (plan N7).
    pub const AMPLITUDE: usize = 2;
}

/// How many targets the instrument declares.
pub const TARGETS: usize = 3;

/// Their names, in target order.
pub const TARGET_NAMES: [&str; TARGETS] = ["Cutoff", "Resonance", "Amplitude"];

/// The Key source's unit: **five octaves either side of middle C**, the pilot's.
pub const KEY_UNIT_SEMITONES: f32 = 60.0;

/// The note the Key source reads zero at.
pub const KEY_CENTRE: f32 = 60.0;

/// Which of the standard's performance sources each source is — `None` for the machine's own
/// generators and its audio, which keep their own meaning.
pub const PERFORMANCE: [Option<Performance>; SOURCES] = [
    Some(Performance::Key),
    Some(Performance::Velocity),
    Some(Performance::Wheel),
    Some(Performance::Pressure),
    Some(Performance::Bend),
    None,
    None,
    None,
    None,
];

/// Each target's law, for the standard's offer: two sums and the amplitude factor.
pub const LAW: [Law; TARGETS] = [Law::Sum, Law::Sum, Law::Factor];

/// Whether **the TB-303 itself** has this path — one of [`INIT_PRESENT`] — so its reach is the
/// machine's rather than the standard's.
#[must_use]
pub const fn machine(target: usize, source: usize) -> bool {
    let mut i = 0;
    while i < INIT_PRESENT.len() {
        if INIT_PRESENT[i].0 == target && INIT_PRESENT[i].1 == source {
            return true;
        }
        i += 1;
    }
    false
}

/// Whether and how a pair is offered — `standard::offer`. Every target here sums or scales, so
/// every pair is offered on both halves.
#[must_use]
pub const fn offer(target: usize, source: usize) -> Offer {
    standard::offer(LAW[target], PERFORMANCE[source], machine(target, source))
}

/// How much of the accent circuit's filter output the Accent source publishes (plan N9).
///
/// The sweep peaks at [`SWEEP_CEILING`], 1.6, and a frame bounds every value to unit magnitude, so
/// the source goes out at a half and its column in [`FULL_SCALE`] is doubled. **A power of two**, so
/// the halving and the doubling are both exact and the machine's own route stays bit-identical to the
/// expression it replaced.
pub const ACCENT_UNIT: f32 = 0.5;

const _: () = assert!(SWEEP_CEILING * ACCENT_UNIT <= 1.0);

/// **The circuit's floor on the cutoff**, in octaves per unit of filter envelope: the reach no Env
/// Mod setting removes. Added by the voice whatever is routed.
pub const ENV_MOD_FLOOR_OCTAVES: f32 = ENV_MOD_OCTAVES * ENV_MOD_FLOOR;

/// The rest of Env Mod's reach: what the (Cutoff ← Filter envelope) route's amount spans.
pub const ENV_MOD_ROUTE_OCTAVES: f32 = ENV_MOD_OCTAVES * (1.0 - ENV_MOD_FLOOR);

/// Each pair's scale at an amount of one, **per route**: a route the machine wires keeps the reach it
/// always had, and **a route a player adds takes the collection's standard reach**
/// (`standard::reach`).
///
/// | Target | Added | The machine's own |
/// |---|---|---|
/// | Cutoff, octaves | 4; Key 5 over the key's unit, one octave per octave | Filter envelope `5·(1 − F)` above the floor; Accent 4 per unit of its signal, doubled to undo the half unit |
/// | Resonance, the control's `0..=1` | 1: a full route spans the control; Key a fifth of it per octave | — the machine never modulated it |
/// | Amplitude, a factor on the envelope | 1: a route can close the amplifier or double it; Key a fifth per octave | Accent level 1 |
///
/// Before the standard, an added cutoff route took Env Mod's full five octaves. The Accent column is
/// doubled on every target, so a route a player adds from it reads the signal's own unit.
pub const FULL_SCALE: [[f32; SOURCES]; TARGETS] = {
    let key_linear = reach::KEY_LINEAR_FRACTION_PER_OCTAVE;
    let mut cutoff = [reach::OCTAVES; SOURCES];
    cutoff[source::FILTER_ENVELOPE] = ENV_MOD_ROUTE_OCTAVES;
    cutoff[source::KEY] = standard::key_scale(reach::KEY_OCTAVES_PER_OCTAVE, KEY_UNIT_SEMITONES);
    cutoff[source::ACCENT] = ACCENT_OCTAVES / ACCENT_UNIT;
    let mut resonance = [reach::CONTROL; SOURCES];
    resonance[source::KEY] = standard::key_scale(reach::CONTROL * key_linear, KEY_UNIT_SEMITONES);
    resonance[source::ACCENT] = 1.0 / ACCENT_UNIT;
    let mut amplitude = [reach::AMPLITUDE; SOURCES];
    amplitude[source::KEY] = standard::key_scale(reach::AMPLITUDE * key_linear, KEY_UNIT_SEMITONES);
    amplitude[source::ACCENT] = 1.0 / ACCENT_UNIT;
    [cutoff, resonance, amplitude]
};

/// **What each source actually reaches, in frame units** — for reading an amount, never for
/// evaluating one (`mxm-mono-00`'s rule, in its repository's
/// `crates/mxm-mono-00-dsp/src/routing.rs`).
///
/// A scale is per unit of source, and not every source fills the unit: the accent circuit's filter
/// output peaks at [`SWEEP_CEILING`] and is published at [`ACCENT_UNIT`] of it. Every other source
/// reaches one — the oscillator at `OSC_PEAK`, the envelope, the accent level, velocity, the wheel,
/// pressure and the bender at their tops, and the key at C9, where the frame's unit bound meets it.
pub const SOURCE_PEAK: [f32; SOURCES] = {
    let mut table = [1.0; SOURCES];
    table[source::ACCENT] = SWEEP_CEILING * ACCENT_UNIT;
    table
};

/// What a route delivers **at full amount with its source at its peak**, in the target's own unit —
/// what the interface reads, so the number a player sees is the number the pair moves.
/// (Cutoff ← Filter envelope) at full reads `5·(1 − F)` octaves above the floor, and (Cutoff ←
/// Accent) the 6.4 octaves the circuit's climbing sweep can open (plan §7).
#[inline]
#[must_use]
pub fn reach(target: usize, source: usize, amount: f32) -> f32 {
    amount * FULL_SCALE[target][source] * SOURCE_PEAK[source]
}

/// Each target's bound on its sum. **Generous where the target clamps its own result** — the filter
/// its cutoff, the voice the resonance to its control — and **the standard's one on the amplitude**
/// (`plans/plan-collection-sync.md` D8, closed 2026-09-26), so the accent at full reaches exactly
/// double and several amplitude routes together reach no further.
pub const BOUND: [f32; TARGETS] = [64.0, 64.0, AMPLITUDE_SUM_BOUND];

/// The cutoff's modulation, in octaves: the circuit's floor, then whatever the routes sum on top.
///
/// **The floor is added whatever is routed** — removing every route leaves it, as no Env Mod setting
/// on the machine can take it away.
#[inline]
#[must_use]
pub fn cutoff_octaves(filter_envelope: f32, routed: f32) -> f32 {
    ENV_MOD_FLOOR_OCTAVES * filter_envelope + routed
}

/// The routes **the machine itself wires**, present in the init patch.
pub const INIT_PRESENT: [(usize, usize); 3] = [
    (target::CUTOFF, source::FILTER_ENVELOPE),
    (target::CUTOFF, source::ACCENT),
    (target::AMPLITUDE, source::ACCENT_LEVEL),
];

/// The Init routes that start **at full rather than at zero** — the accent circuit's two outputs.
///
/// **Two recorded init deviations** (plan §5, D1 a), under `mxm-mono-00`'s rule for its own
/// `INIT_AT_FULL`: *a route whose depth was not a control before the conversion has no zero to
/// inherit*. The Accent knob is the circuit's one depth, ahead of both outputs, so these routes are
/// fixed-gain wiring; with the knob at zero, as Init has it, both sources are zero and neither route
/// changes a sound. The Env Mod route starts at zero, which is the floor.
pub const INIT_AT_FULL: [(usize, usize); 2] = [
    (target::CUTOFF, source::ACCENT),
    (target::AMPLITUDE, source::ACCENT_LEVEL),
];

/// A route's amount in the init patch: one for [`INIT_AT_FULL`], zero for everything else.
#[must_use]
pub const fn init_amount(target: usize, source: usize) -> f32 {
    let mut i = 0;
    while i < INIT_AT_FULL.len() {
        if INIT_AT_FULL[i].0 == target && INIT_AT_FULL[i].1 == source {
            return 1.0;
        }
        i += 1;
    }
    0.0
}

/// Which sources are live into which targets, and how much of each.
///
/// **Presence is what the DSP reads.** An absent route contributes nothing whatever its amount
/// holds. **It travels beside the patch, not inside it**: `voice::Params` is `Copy` and rebuilt every
/// sample, and a grid carried there is a memcpy per sample for values that change on a parameter event
/// (mxm-kit's `docs/code-review-notes.md` §7).
#[derive(Debug, Clone, Copy)]
pub struct Routing {
    /// Per target, per source: whether that route exists.
    pub present: [[bool; SOURCES]; TARGETS],
    /// Per target, per source: how much, signed, as a fraction of that route's scale. Only a live
    /// route's amount is filled each sample; an absent one is never read.
    pub amounts: [[f32; SOURCES]; TARGETS],
    live: [(u8, u8); TARGETS * SOURCES],
    live_len: usize,
}

impl Default for Routing {
    fn default() -> Self {
        Self::new()
    }
}

impl Routing {
    /// Nothing routed anywhere.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            present: [[false; SOURCES]; TARGETS],
            amounts: [[0.0; SOURCES]; TARGETS],
            live: [(0, 0); TARGETS * SOURCES],
            live_len: 0,
        }
    }

    /// Rebuilds the live list from [`Routing::present`]. Once per interval, never per sample, and
    /// every caller that changes `present` owes it before the next render.
    pub fn compact(&mut self) {
        self.live_len = 0;
        for (t, target) in self.present.iter().enumerate() {
            for (s, &on) in target.iter().enumerate() {
                if on {
                    self.live[self.live_len] = (t as u8, s as u8);
                    self.live_len += 1;
                }
            }
        }
    }

    /// The live pairs, `(target, source)`.
    #[inline]
    #[must_use]
    pub fn live(&self) -> &[(u8, u8)] {
        &self.live[..self.live_len]
    }

    /// The init patch: [`INIT_PRESENT`], at [`init_amount`].
    #[must_use]
    pub const fn init() -> Self {
        let mut routing = Self::new();
        let mut i = 0;
        while i < INIT_PRESENT.len() {
            let (t, s) = INIT_PRESENT[i];
            routing.present[t][s] = true;
            routing.amounts[t][s] = init_amount(t, s);
            i += 1;
        }
        // In declared order, as `compact` builds it.
        let mut len = 0;
        let mut t = 0;
        while t < TARGETS {
            let mut s = 0;
            while s < SOURCES {
                if routing.present[t][s] {
                    routing.live[len] = (t as u8, s as u8);
                    len += 1;
                }
                s += 1;
            }
            t += 1;
        }
        routing.live_len = len;
        routing
    }
}

/// The voice's routing state: one frame, and one compacted list per target.
#[derive(Debug, Clone)]
pub struct Graph {
    frame: SourceFrame<SOURCES>,
    live: [Compacted<SOURCES>; TARGETS],
    /// Which sources a live route reads. **A source nothing reads is not published**, which is the
    /// cheap half of the cost model; the generators still run.
    needed: [bool; SOURCES],
    /// Whether any route at all is live: with none, the voice opens no frame and takes no sum.
    any: bool,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: SourceFrame::new(),
            live: [const { Compacted::new() }; TARGETS],
            needed: [false; SOURCES],
            any: false,
        }
    }

    /// Rebuilds which routes are live. Once per interval, never per sample.
    ///
    /// **A source that becomes needed starts from silence.** While nothing read it, nothing
    /// published it, so its slot still holds whatever it held the last time something did — which a
    /// route added to a running voice would read before this sample publishes it, from a different
    /// phrase and a span the host's buffers decided. Clearing the slot makes that read a deterministic
    /// zero (mxm-kit's `crates/mxm-modulation/AGENTS.md`, *A gated publication owes a clear*).
    pub fn set_topology(&mut self, routing: &Routing) {
        for (live, present) in self.live.iter_mut().zip(routing.present.iter()) {
            live.build(present);
        }
        self.any = self.live.iter().any(|l| !l.is_empty());
        let was_needed = self.needed;
        self.needed = [false; SOURCES];
        for present in &routing.present {
            for (needed, &on) in self.needed.iter_mut().zip(present.iter()) {
                *needed |= on;
            }
        }
        for (source, (&needed, &before)) in self.needed.iter().zip(was_needed.iter()).enumerate() {
            if needed && !before {
                self.frame.clear(source);
            }
        }
    }

    /// Whether a live route reads this source.
    #[inline]
    #[must_use]
    pub fn needs(&self, source: usize) -> bool {
        self.needed[source]
    }

    /// Whether anything is routed at all.
    #[inline]
    #[must_use]
    pub fn any_live(&self) -> bool {
        self.any
    }

    /// Whether no route is live into that target.
    #[inline]
    #[must_use]
    pub fn is_empty(&self, target: usize) -> bool {
        self.live[target].is_empty()
    }

    /// Opens a sample.
    #[inline]
    pub fn begin_sample(&mut self) {
        self.frame.begin_sample();
    }

    /// Publishes a source's value for this sample, **if a live route reads it**.
    #[inline]
    pub fn write(&mut self, source: usize, value: f32) {
        if self.needed[source] {
            self.frame.write(source, value);
        }
    }

    /// What the frame holds for a source: this sample's if published, otherwise last sample's.
    #[inline]
    #[must_use]
    pub fn read(&self, source: usize) -> f32 {
        self.frame.read(source)
    }

    /// This target's summed modulation, in its own unit: each route `(amount × source) × scale`, in
    /// declared source order, bounded by [`BOUND`].
    #[inline]
    #[must_use]
    pub fn sum(&self, target: usize, routing: &Routing) -> f32 {
        mxm_modulation::sum_scaled(
            &self.frame,
            &self.live[target],
            &routing.amounts[target],
            &FULL_SCALE[target],
            BOUND[target],
        )
    }

    /// Clears both halves of the frame, leaving no tail between renders.
    pub fn reset(&mut self) {
        self.frame.reset();
    }
}
