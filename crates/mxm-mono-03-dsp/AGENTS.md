# AGENTS.md — crates/mxm-mono-03-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The complete mxm-mono-03 voice as plain Rust: oscillator, diode-ladder filter, two
attack-decay envelopes, the accent circuit, and the monophonic voice that wires them together.
Framework-free, so the whole signal path is testable with `cargo test` and no host involved.

**The instrument is an acid bass voice**, with architecture inspired by the Roland TB-303. The
completed implementation record remains in
`plans/plan-mxm-mono-03.md` (`plans/plan-mxm-mono-03.md` in the private archive).

The rationale, measurements and history behind the rules below are in [NOTES.md](NOTES.md).

# Ownership

Owns `src/` (`lib.rs`, `oscillator.rs`, `filter.rs`, `envelope.rs`, `accent.rs`, `voice.rs`,
`routing.rs`), `tests/routing.rs`, `examples/` (`mono_03_render_demo.rs`, `acid_demo.rs`; the
`common/wav.rs` they used to share retired to `mxm-measure`'s encoder and then to
`mxm-audio-file`), and `Cargo.toml`.

Does **not** own parameter definitions, ranges, smoothing or the editor — those belong to
[`plugins/AGENTS.md`](../../plugins/AGENTS.md). This crate takes plain values and a sample rate.

# Local Contracts

## The accent circuit

- `accent.rs` is the sound (`research:filters/machines/tb303-diode-ladder.md` §4–§5); aim changes
  there before the filter
  ([NOTES.md § The accent circuit](NOTES.md#the-accent-circuit-is-the-instrument-and-it-has-memory)).
- An accented note is louder through a short lag, **forces the filter envelope's decay to its
  minimum** for that note only, and **adds a sweep to the cutoff that holds charge between notes**,
  so consecutive accents climb. Accent is therefore a module, never a per-note gain or offset.
- **Resonance chooses accent's character, not its depth**: the sweep reads the resonance control.
- **The sweep must charge slowly relative to a note**, or nothing climbs (`consecutive_accents_climb`).

## Slide and the gate

- `note_on` reads the `slide` switch (the patch) and the gate (the host) separately. A slide with
  the gate held retriggers neither envelope and takes no accent; with the gate closed it is a bend
  into a new note
  ([NOTES.md § Slide is a switch](NOTES.md#slide-is-a-switch-the-gate-is-separate-and-that-separation-is-load-bearing)).
- **The gate is the test, never `is_active()`**, or a slide from a released note never speaks:
  `the_first_note_of_a_phrase_does_not_glide`, `a_slide_from_a_released_note_still_speaks`.
- Anything driving the voice from a pattern holds the predecessor's gate to the boundary
  (`Pattern::held_past_gate` = `tied(n) || tied(n + 1)`); both examples do.
- **The slide time is fixed, not a control.** `Params::glide_s` exists for tests and measurement;
  the plugin always passes `SLIDE_TIME_S`.

## Pitch

- The lag is `target_note + glide_offset`, the slide carried as its remaining distance
  (`a_slide_lands_exactly_on_its_note`;
  [NOTES.md § The pitch has three inputs](NOTES.md#the-pitch-has-three-inputs-and-only-one-of-them-lags)).
- `tune_semitones` and `expression_semitones` are **added after the lag** and stay separate fields;
  the expression stays its own field all the way down.

## The filter

- Four one-poles at spread positions with global feedback. `native_resonance` scales by
  `config.threshold()`, never a constant (`resonance_means_the_same_thing_on_every_configuration`;
  [NOTES.md § The filter is the pole set](NOTES.md#the-filter-is-the-pole-set-and-its-cutoff-is-clamped-by-the-widest-pole)).
- It does not sing at its cutoff: ask `oscillation_ratio`, never assume 1.0.
- **Clamp the nominal cutoff by the widest pole, never each stage**, so the pole ratios stay exact.
  The cutoff control therefore shows a position, not a frequency.
- **Never add Q compensation inside the filter**; drive and makeup are part of the voice and are not
  user controls ([NOTES.md § The droop](NOTES.md#the-droop-is-the-topology--compensate-outside-the-filter-never-inside)).
- `MIN_SAMPLE_RATE` is the lowest rate the plugin activates at: below it the nominal cutoff's clamp
  bounds cross and `f32::clamp` panics.
- No bandwidth sits on a routed cutoff or resonance: the ladder's loop is bounded by `tanh_approx`.
  `the_ladder_driven_at_audio_rate_from_silence_does_not_pump_itself` and
  `audio_rate_routes_into_cutoff_and_resonance_stay_bounded_through_the_voice` guard the property;
  the cutoff clamp is load-bearing, since without it a NaN reaches the output
  ([NOTES.md § The ladder's loop](NOTES.md#the-ladders-loop-is-bounded-by-its-saturator-and-that-was-measured-rather-than-assumed)).

## The hardware's constants are inputs

- Envelope attacks, amplitude decay, accent decay and sweep rate, drive and the pole set arrive
  through `voice::Params`; the constants are the defaults, so `Params::default()` is the machine
  (`the_init_patch_is_the_hardware`;
  [NOTES.md § The hardware's constants](NOTES.md#the-hardwares-constants-are-inputs-and-every-one-defaults-to-the-hardware)).
- **Ranges are bounded here, not only in the plugin**: `envelope::ATTACK_MIN_S`/`MAX_S`,
  `accent::SWEEP_SCALE_MIN`/`MAX`.
- The accent sweep is **one control over two time constants**; never expose them separately.
- `set_config` (the filter model) is control-rate, at a block boundary, and keeps the integrator
  states.

## Modulation is routing

- `routing.rs` declares nine sources and three targets (Cutoff, Resonance, Amplitude) on the shared
  [`mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md);
  the tables are in
  [NOTES.md § Modulation is routing](NOTES.md#modulation-is-routing-nine-sources-three-targets-a-floor-that-is-circuit).
  **Nothing in the voice asks whether a route is the machine's own**: `INIT_PRESENT` and
  `INIT_AT_FULL` are the whole of that. The routing travels **beside** `Params`, never inside it.
- Key, Velocity, Wheel, Pressure and Bend publish through `mxm_modulation::standard`. Every path the
  TB-303 did not have takes the collection's standard reach; the amplitude sum is bounded at the
  standard's one. `conformance.rs`'s `Declared` runs the standard's checks over the real tables.
- **Every source is published before any target reads**: no route is backward
  (`a_route_from_the_oscillator_lands_this_sample_in_every_target`).
- **The Env Mod floor is circuit**: the voice adds `ENV_MOD_FLOOR_OCTAVES` whatever is routed
  (`the_floor_survives_removal_zero_and_every_positive_amount`).
- The machine's own routes reach what the retired expressions did
  (`the_machines_own_routes_reach_what_the_old_expressions_did`). `SOURCE_PEAK` is for readings
  only; nothing evaluates with it.

**What the routing owes, each a test run against the defect it names** (`tests/routing.rs`):

- a source that becomes read starts from silence;
- an accented note at Accent depth zero still gets the short decay — the forced decay reads the
  gate;
- consecutive accents climb through the routes, which needs the Accent source to read the resonance;
- no key down is exact silence whatever is routed; `reset` clears the frame; velocity follows the
  note and a slid note keeps it; the Key source follows the slide;
- an Amplitude sum below zero closes the amplifier and never inverts it — N7's `max(0, …)`;
- the accent sources are the circuit's two outputs at the knob's depth, checked against the retired
  expression on the voice's own samples, so the machine's routes are guarded end to end;
- every pair at extreme amounts stays finite and bounded, at 8, 44.1 and 192 kHz.

## Dependencies

- **One runtime dependency, `mxm-modulation`**, dependency-free at this crate's 1.87 floor; the MSRV
  override rests on that ([NOTES.md § Dependencies](NOTES.md#dependencies-one-at-runtime)).
- `[dev-dependencies]` (`mxm-measure`, `mxm-audio-file`, `mxm-audio-file-decode`) reach only tests and
  `examples/`, never a shipped `.clap`; MPL-2.0 symphonia never reaches the shipped graph.

## Everything else the collection's DSP already requires

No framework types; realtime rules on every per-sample path; denormals flushed in the DSP itself on
every recursive state; `f32` in the audio path and `f64` for prewarping; every saturator bounded
exactly and monotonic; a stated `pub const` output bound with the argument that establishes it;
deterministic seeded randomness. These are the monorepo root's and mxm-mono-01's
[`crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md)'s, not restated here.

# Work Guidance

- Do not extract DSP from this crate without a shared-DSP proposal backed by identical needs in
  another shipped implementation. Candidates for comparison are denormal flushing, the ladder’s
  scalar Newton feedback solve, bounded monotonic saturation, PolyBLEP and
  [`docs/filters/09-voicing.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/filters/09-voicing.md)’s normalized-resonance model;
  resemblance is not an API.
- Filter theory is [`docs/filters/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/filters/README.md); oscillators
  [`docs/oscillators/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/README.md); envelopes and glide
  [`docs/modulation/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/modulation/README.md). Read the relevant chapter before changing a module.
- Prefer a clear implementation to a clever one. This is reference-quality open source.
- **Check a test guarding accumulated state by sabotage**: break the state and watch it fail
  ([NOTES.md § An oracle that cannot fail](NOTES.md#an-oracle-that-cannot-fail-is-not-an-oracle)).

# Verification

**The rulers are shared, the thresholds are not**: measurements come from `mxm-measure`; every bound
and its headroom stays in the test that argues for it.

```bash
cargo test -p mxm-mono-03-dsp
cargo +1.87.0 test -p mxm-mono-03-dsp       # the MSRV this crate claims
cargo tree -p mxm-mono-03-dsp -e normal      # mxm-modulation, and nothing else
cargo clippy -p mxm-mono-03-dsp --all-targets
cargo run -p mxm-mono-03-dsp --release --example mono_03_render_demo   # the mechanisms, one at a time
cargo run -p mxm-mono-03-dsp --release --example acid_demo     # what they add up to, filter played
```

Both demos render audio for inspection. Keep both
([NOTES.md § Why `acid_demo` is kept](NOTES.md#why-acid_demo-is-kept)).

Properties the tests must keep asserting, because each regresses silently:

- silence in gives **exactly** zero out, after decay — the denormal-flush test too
- no NaN or inf across a sample-rate x cutoff x resonance sweep, and across a parameter sweep
- output within the stated bound under overdrive past the oscillation threshold
- `reset()` leaves no tail — **including the accent sweep's accumulated charge**, which would
  otherwise survive a transport stop and make the next run's first note wrong
- the filter's threshold and oscillation ratio, **measured on the running filter**, against the
  closed form the docs already record
- **consecutive accents climb, and a lone accent decays away** — the pair, because either alone is
  satisfiable by a latch
- a tie slides and an un-tied step does not, and a slid destination retriggers nothing

Through the player: `plugins/mxm-mono-03/host-tests/tests/golden_audio.rs` pins the sound by digest
on Windows (`the_reference_is_sensitive_to_the_envelope_depth`) and `host-tests/tests/behaviour.rs` measures
pitch, cutoff, accent and slide on rendered audio.

**Not verified, and must not be claimed:** no hardware was measured; the time constants in
`accent.rs`, the duty-cycle figures in `oscillator.rs` and the slide time are published or chosen.
Fidelity is UNVERIFIED until a listening comparison against reference recordings is run
([NOTES.md § What is not verified](NOTES.md#what-is-not-verified-and-must-not-be-claimed)).

The development machine is Windows; Linux and macOS are checked later, together, and by CI on
a `v*` tag (root *Windows, Linux and macOS*).

# Child DOX Index

No child AGENTS.md files. `src/` and `examples/` are covered by this doc.
