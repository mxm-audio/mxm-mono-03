# AGENTS.md — plugins/mxm-mono-03

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell for **mxm-mono-03**, a monophonic acid bass voice inspired by the Roland TB-303.
Identity, parameters, MIDI, presets, telemetry and the editor. The whole voice is
[`crates/mxm-mono-03-dsp`](../../crates/mxm-mono-03-dsp/AGENTS.md).

Shared conventions — nice-plug's API, the init-patch contract, preset rules, `process()` realtime
rules, the editor contract — live in the parent and are not restated here. This doc holds what is
**local to this plugin**.

# Ownership

`BASELINE-M0.md`, `Cargo.toml`, `LICENSE`, `README.md`, `control-map.json`, `presets/`, and `src/` —
`lib.rs`, `params.rs`, `routes.rs`, `preset.rs`, `telemetry.rs`, and `editor.rs` with its
`editor/{binding, sections, visuals}.rs`.

**`BASELINE-M0.md` is the routing conversion's reference**, captured before any of it
(`plans/plan-mxm-mono-03-modulation.md` M0): the factory bank's digests through
`render_block_for_test`, the plugin's own per-sample path, and the throughput the cost gate compares.
`lib.rs`'s `#[ignore]`d `baseline` module produces both and runs unchanged on the converted tree. The
seam is a measurement seam, not a second `process()`: `process()` renders every sample through the
same `render_sample`.

# Local Contracts

## Permanent identifiers

| What | Value |
|---|---|
| `CLAP_ID` | `dk.mxm.mxm-mono-03` — assembled from `plugin_name!` in `src/lib.rs`, **not** from `CARGO_PKG_NAME`, so renaming the directory cannot move it |
| Parameter `#[id]`s | **Panel:** `tune` `waveform` `bendrange` `cutoff` `resonance` `decay` `accent` `accentnote` `slide` `volume`<br>**Advanced:** `slidetime` `ampattack` `ampdecay` `filterattack` `accentdecay` `accentsweep` `drive` `filtermodel` |
| Routing `#[id]`s | `mod_<target>_<source>`, the amount, and `mod_<target>_<source>on`, the presence — targets `cutoff` `res` `amp`, sources `key` `vel` `wheel` `press` `bend` `env` `accent` `accentlevel` `osc`. Written out in `routes::ROUTE_IDS`, which `the_id_table_is_what_the_derive_actually_produces` holds to the derive |
| Retired `#[id]`s | `envmod` — never to be reused |

Treat all of them as public interface.

## The machine's modulation is routing, and its wiring is the init patch

`routes.rs` declares a presence and a signed amount for every *(target, source)* pair — three
targets, Cutoff, Resonance and Amplitude, and nine sources — and `crates/mxm-mono-03-dsp`'s
`routing` module evaluates them. Nothing asks whether a route is the machine's own. **The machine's
three modulation paths are present in the init patch**: (Cutoff ← Filter envelope), which replaced
the `envmod` knob, and the accent circuit's two outputs, (Cutoff ← Accent) and (Amplitude ← Accent
level) — `the_init_patch_wires_exactly_the_machines_own_routes`.

**A route's amount reads what its pair delivers**, in the target's own unit and against its source's
peak — `mxm-mono-00`'s `SOURCE_PEAK` rule — so (Cutoff ← Filter envelope) at full reads +4.60 oct
above the floor, (Cutoff ← Accent) +6.40 oct, the most the accent sweep can open, and (Amplitude ←
Accent level) +100 %; a route the TB-303 never had reads the collection's standard reach, +4.00 oct
or +100 %. `a_route_reads_what_its_pair_delivers_and_reads_back` holds each, and that a
typed reading lands back on its amount. Every amount is the collection's one route parameter —
`mxm_modulation_params::reading::amount_param_at`, because the accent circuit's two routes start at
full — and `every_route_parameter_says_what_the_dsp_does` holds every pair's travel and reading to
`mxm_mono_03_dsp::conformance` (`mxm_plugin_test::routing_checks`). **A reading never prints a negative zero**: its number goes
through `mxm_modulation_params::signed`, because `-0 %/oct` parses to zero and prints `+0 %/oct`,
text that is not idempotent through the host's conversion — `clap-validator`'s `param-conversions`
found it on this conversion's first bundle
(`every_reading_survives_the_hosts_round_trip_a_rounded_zero_included`).

**`envmod` is retired**, an owner decision under the governing plan's decision 1.13
(`plans/plan-mxm-mono-03-modulation.md` §4), and never reused (`no_retired_id_reappears`). What it
could reach is reachable exactly: an Env Mod at normalised `n` is the route at amount `n`, above the
floor. The factory designs were translated so — `(n + 1) / 2` on the route's signed range — and
regenerated. **Decision, X1 — the owner, 2026-09-15: no `filter_state` is built**, so a saved state
keeps every surviving parameter and loses what `envmod` held.

**Once per callback, then once per sample**, through `resolve_topology` and `render_sample` in
`lib.rs`, which `process()` and the measurement seam `render_block_for_test` both call:

- `Routes::topology_from` builds the topology against the last callback's and **snaps each newly
  present route's smoother** to its stored depth —
  `a_re_added_route_arrives_at_its_stored_depth_rather_than_ramping_from_a_stale_one`.
- The voice clears a source that has just become read, which is the DSP's contract.
- `Routes::advance`, per sample, advances each live route's smoother. There is no legacy push: the
  machine had no wheel.
- `a_route_arriving_after_an_idle_span_is_block_partition_invariant` renders that path at 64, 37 and
  1024 samples a block.

**What the routing cost is measured**: `BASELINE-M0.md` — no slower than before the conversion.

## Every parameter's text survives the host's round trip

A host parses a parameter's text and **normalises the number before printing it again**, so the
value it prints lands a hair either side of where it started. A formatter that switches unit or
precision at the raw value is not idempotent there, and `clap-validator`'s `param-conversions` fails
only when its values land in that sliver. So `src/params.rs`'s `v2s_time` chooses its unit from what
the millisecond text would round to: anything that would read `1000 ms` prints seconds. The route
readings never print a negative zero (above).
`params::tests::every_parameter_reads_the_same_after_the_hosts_round_trip` holds all 72, converting
as the wrapper does, with the unit: at the validator's grids, either side of each branch point, and
at every representable normalised value near each. Amp decay and Accent decay, the two advanced
times that cross a second, failed it before the fix.

## Env Mod's floor is circuit, not an amount — a recorded deviation from the init-patch contract

The parent says **every amount starts at zero**. The hardware's circuit cannot turn the filter
envelope's reach on the cutoff down to zero, and this instrument's standard is fidelity to it. So
the voice adds that floor — `routing::ENV_MOD_FLOOR_OCTAVES` of the envelope — **whatever is
routed**, and the (Cutoff ← Filter envelope) route sums on top of it, starting at zero like every
other amount. Removing the route leaves the floor, which is the hardware: the ✕ cannot do what no
303 can. A negative amount inverts the envelope and, at `−F / (1 − F)`, cancels the floor — a new
option, labelled rather than hidden. `env_mods_floor_is_circuit_and_its_route_starts_at_zero` pins
it.

**This is local, and must stay local.** It is a fact about one machine, not a collection policy —
putting it in the parent would hand every later instrument a mxm-mono-03 exception as though it were
a rule.

**Two more init deviations came with the routing** (plan §5, D1 a): **the accent circuit's two
routes start at full.** The Accent knob is the circuit's one depth, ahead of both outputs, so they
are fixed-gain wiring — `mxm-mono-00`'s *a route whose depth was not a control before the conversion
has no zero to inherit* — and a fresh instance, whose knob is at zero, sounds exactly as before.
`the_accent_circuits_routes_start_at_full` pins them.

Everything else obeys the contract — accent depth, accent-note, resonance, tune and every other
route start at zero — and `every_amount_starts_at_zero` pins that as a *rule* rather than a taste.

## There is no distortion, and that is a scope decision rather than an omission

The hardware has none — it goes amplifier to output jack. Distortion is nonetheless most of the
*recorded* acid sound, because players ran a 303 into a pedal, the Boss DS-1 being the canonical
one, and the clones that build one in are adding what people did anyway.

**The machine had none, so this plugin has none** — the whole of the argument, under the collection's
rule that an instrument ships the effects its original had and no others
([`../AGENTS.md`](../AGENTS.md)). The distortion is a separate box and will be a separate plugin;
the owner plans an effects collection.

*The clones build one in* is not a counter-argument, and it is the one that will be offered. Nor is
*it is most of the recorded sound*. Both are true and neither is about the TB-303: putting a
waveshaper after the amplifier here would bake one pedal's character into an instrument, sit outside
the voice while looking like part of it, and foreclose the choice for anyone who wanted a different
pedal. **An instrument whose original did have an effect is the opposite case and takes it inside**
— that is the same rule, not an exception to it.

Note the constraint that makes the separate plugin harder than it sounds: `apps/mxm-player` refuses
any plugin declaring an audio input, so an effect cannot run in this project's own host until the
player learns to host one. That is host work, recorded in the player's README, and it is a blocker
on the effects collection only — a built-in effect is inside an instrument that has no audio input.

**Do not confuse this with drive.** The drive into the filter *is* part of the voice — the ladder's
droop means it must be driven hard to be heard at all — and it is not a user control because the
hardware has none.

## The advanced disclosure holds the machine's fixed constants

Eight controls, all **ordinary automatable parameters** rather than persisted state. The parent's
*Editable models* rule does not apply: this is a fixed set the plugin ships, not a model anybody can
extend at runtime, and the line is *who can add to the set*.

**Every one defaults to the value the machine has.** That is what makes the disclosure safe —
opening it changes nothing until something is moved, so the instrument is the machine until somebody
decides otherwise. `every_advanced_control_defaults_to_the_hardware` pins it, and it is the whole
argument for exposing fixed constants at all.

| id | What it frees | Was |
|---|---|---|
| `slidetime` | the slide's RC | fixed 60 ms |
| `ampattack` `filterattack` | the de-clicker becomes a shape | fixed, shared |
| `ampdecay` | why every note is flat-topped | fixed, long |
| `accentdecay` | the forced-short envelope on an accent | fixed at the Decay floor |
| `accentsweep` | the climb's rate | fixed |
| `drive` | how hard the oscillator hits the ladder | fixed |
| `filtermodel` | the EMS pole set, already built and tested | unreachable |

**None of them is smoothed.** A constant is not a signal, and that part of the parent's rule carries
whether or not the rest of it applies.

**They are deliberately absent from `control-map.json`.** They are set-once controls, not things to
reach for on a knob, and leaving them out sidesteps the unknown-role problem below entirely.

**Ranges are bounded in the DSP, not only here.** A parameter arriving from a host is
user-generated input — `envelope::ATTACK_MIN_S`/`MAX_S` and `accent::SWEEP_SCALE_MIN`/`MAX` are
where that is enforced.

Deliberately **not** exposed, because every id is permanent and worth resisting: makeup gain (it
follows drive, so two controls for one perceptual thing), the sweep ceiling (a safety bound, not a
voicing), the Decay control's range ends (the control already spans them), the DC blocker, and the
oscillator's duty-cycle endpoints — that pitch dependence *is* the oscillator.

## Slide is a switch, and its time is advanced

The hardware's slide is a per-step **on/off button**; its slide time is a fixed RC, and there is no
time control anywhere on the machine. So `slide` is a switch, and `slidetime` exists only behind the
advanced disclosure, defaulting to `SLIDE_TIME_S`.

The lag lives in the voice rather than the sequencer, so MIDI-keyboard performance receives the
same slide. That scope decision does not authorize a front-panel time knob the machine lacks.

`slide_is_a_switch_and_its_time_is_advanced` pins both halves, because "this is deliberately a
switch" is exactly the decision someone undoes while being helpful.

## A drawn pitch bend rides on the slide, and does not become a second slide

`expression_semitones` is added **after** the glide lag, not before it, so a host's per-note pitch
curve bends the pitch the slide is already moving instead of fighting it — mid-slide the expressed
voice sits a fixed interval above the plain one and both are still travelling
(`a_pitch_expression_rides_on_a_slide_rather_than_replacing_it`).

That ordering is the whole decision, and it is the one worth not undoing. Summing the expression
into the *target* before the lag would make a drawn bend slide too — the RC would smear a curve the
author drew exactly, and two mechanisms would be competing to say what the pitch is. The slide
belongs to the transition between notes; the expression belongs to the note. See
[`../AGENTS.md`](../AGENTS.md), *Per-note pitch expression is accepted*.

## The two per-step buttons are parameters; the gate is not

Accent and slide are the hardware's two per-step buttons, so both are parameters — the only route a
host has to a per-step decision.

**Whether the envelopes retrigger is not a parameter, and must not become one.** It follows the
gate: a tie makes a host emit the new note **before** releasing the old one, so the plugin sees an
overlap, and an overlap means the gate never closed. Reading the switch and the gate separately is
what makes a slide unable to produce a silent note — see the DSP crate's doc for the failure that
taught this.

The consequence, which is easy to get wrong: **a note-off is matched against the note actually
sounding, and an unmatched one is dropped.** During a slide the stale release for the replaced note
arrives after the new note has started, and acting on it would silence a note that has only just
begun. **A choke is matched the same way**, by note: the arm once reset the voice for a choke naming
any note, so a stale one cut the note a slide had moved to (audit D9;
`note_tracking::a_stale_choke_during_a_slide_leaves_the_new_note_sounding`, and
`a_choke_for_the_sounding_note_still_cuts_it` to exact silence). All Sound Off still resets whatever
is sounding.

## Velocity, the wheel and pressure are routing sources, and nothing more

The hardware has no velocity sensitivity, no wheel and no aftertouch, and accent is its only
dynamic. **All three are routing sources anyway** — the governing plan's decision 1.7 — held per
note for velocity and per channel for the wheel and pressure, and **nothing routes them in the init
patch**, so a fresh instance still ignores them
(`the_new_midi_paths_change_nothing_while_no_route_reads_them`). They are published by the
collection's modulation standard: Velocity as `v − 1`, so a route from it does nothing at the hardest
note. **Velocity is still not an
accent**: the player's sequencer sends a fixed velocity for every step, so a velocity-driven accent
would work from a keyboard and nowhere else, which is the worst of both.

## Accent is two parameters, and the control map claims none of the new roles yet

`accent` is the depth — the hardware's knob. `accentnote` is whether *this* note is accented — the
hardware's per-step button, which has to be a parameter for a host's per-step modulation to reach it.
One control cannot be both: a depth knob turned up would accent every note.

`control-map.json` **deliberately omits both, and `slide` with them** — while `filter.env_amount`
names the Env Mod route's amount, `mod_cutoff_env`, which Init wires, so its knob is live
(`a_control_map_role_never_points_at_a_dead_route`). The collection standard is compiled into the
player, and an instrument map naming a role the player's standard does not declare is refused
**whole** — so claiming an accent role would cost this instrument every other mapping on any player
built before the role existed. The three roles now exist in the standard; this file claims them once
an unknown role is inert rather than fatal. See
[`docs/MXM_CONTROL_MAP.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/MXM_CONTROL_MAP.md) §9.

## The editor, and its brief

Carries the collection's **developer channel** (`plugins/AGENTS.md`, *A developer channel in every
editor*): with `MXM_DEV_CC` in the process environment, CC 119 selects a category (0–5) or Parameters (127), as defined by the parent, CC 117 opens and closes the preset browser. This editor discloses nothing — Advanced is shown, not hidden — so CC 118 has nothing to reach and is taken and dropped. CC 116 sets the theme by index — 0 light, 1 dark, 2 system — without saving it.

Three stable paging items: `Voice` (key 0, Generators), `Filter` (1, Tone) and `Advanced` (2, Tone),
titled by `sections::TITLES`. Voice's oscillator/accent/amplifier body stays indivisible; Advanced
is explicitly labelled and always reachable, never hidden behind disclosure or left below a separate
scroll area. The two-card row test uses a tall component canvas because category paging may
separate Voice and Tone at the opening height. The filter response and accent-sweep displays remain,
and the curve's traces read the routes. **Each target's routes are a stack** — Amplitude under the
Voice card's knob row, Cutoff and Resonance at the foot of the Filter card — drawn by
`mxm_modulation_params::ui::stack`. The opening size is the quarter-4K budget hugged
(`REFERENCE`), derived: `the_opening_size_is_the_budget_hugged` prints the number to take when a
card changes. The minimum holds one card — the widest computed floor plus both gutters
(`the_minimum_holds_the_widest_floor_and_its_gutters`) — and the app bar at its last compact step is wider and sets it (`the_app_bar_holds_in_the_minimum_window`). The brief is
[`docs/briefs/mxm-mono-03.md`](../../docs/briefs/mxm-mono-03.md).
`every_dynamic_page_fits_and_every_card_is_reachable` checks all three cards in both themes at
opening, quarter-4K content and minimum sizes at 1× and 2× with the simulated physical budget fixed.

**Every card is a `mxm_ui::tree`** (`crates/ui/AGENTS.md`, *A card body as data*).
`sections::card` describes each paging item's body once and `sections::paint` draws each leaf
through the same bindings; the paged view is `paging::editor::show`. The gaps are the hand
layout's: the body's `SPACE_3` rhythm, with the `add_space` it put on top as pads.

- **Floors are computed.** `page_items` takes each card's floor from its tree every frame, and each
  card is exactly as wide as that floor: its ceiling is its floor (`plans/plan-editor-standard.md`
  A1), with no usability minimum (A2). A route stack's floor is its widest row at its widest reading,
  present or not (`mxm_modulation_params::ui::stack_size`): the source and its reading side by side
  over a `TRACK_MIN` track, which is what sets Voice's and the Filter's.
- **A knob row is the collection's `mxm_ui::tree::knob_row`**, at `mxm_ui::control::knob_column`:
  its columns take what they are offered up to the cap and shrink below it only as far as the widest
  knob allows.
- **Advanced's knobs are two rows of four**, each the collection's knob row. They were a grid
  whose line count followed the card's width, which has no width of its own to hug: hugged, it fell
  to one knob a line.
- **Slide and Accent note share one width**, the longer label's (`Share::Toggles`).
- **The displays state their size**: `visuals::HEIGHT`, filling the card with no minimum width
  of their own. **No caption on any card** (the owner, 2026-09-27; design system §7.6): the accent
  sweep's hover text says consecutive accents build on each other, and Decay's tooltip that it is
  the only envelope and always moves the filter a little. The filter model's cells are the parameter's own option text, and every control draws through the shared
  named controls.
- `sections::draw` stays for `apps/mxm-layout-lab`, with its signature: it builds the section's tree
  and shows it. Its `spare` is unused: a row of cards is levelled by the paging renderer.
- `every_card_passes_the_tree_checks_in_every_state` runs `mxm_plugin_test::tree_checks` over every
  card at Init, with every route revealed at full negative depth, and with a run of accents climbing
  in the sweep display.

**Volume is in the app bar, not the Voice card** (owner, 2026-09-18: every instrument's master
output is in the app bar; design system §3.1 slot 6). It is an inline slider between the level meter
and the zoom control, drawn through `mxm_ui::navigation::bar_card` under `VOLUME_CARD` (64, outside
the paging keys 0–2), and the cursor runs through `navigation::paged_with_bar`. The Voice card's knob
row is Tune and Accent, with the Amplitude stack beneath. Its id, range, default and smoothing are
unchanged. `editor::APP_BAR` is the bar's entry beside `Section::parameters` for the table tests, and
`the_volume_is_drawn_in_the_app_bar_and_on_no_card` holds both tables to what is painted.
`src/telemetry.rs` is the **only** DSP → editor channel: `Arc`-shared, atomics only, written once
per block. The editor is what raised this crate's MSRV to the workspace's **1.95** — the DSP crate
stays at 1.87.

## `preset.rs` is this instrument's `Instrument` impl and its factory set

The system is `crates/mxm-preset` (since 2026-09-04): this file was the second copy, and the copies
were the evidence the extraction was made from. `editor/binding.rs` re-exports
`mxm_preset::binding`, the collection's one binding, since 2026-09-24. What is local: the **fifty factory sounds** in `presets/`, each with its category, generated from `FACTORY_DESIGN` in
`preset.rs`'s test module (`write_the_factory_presets`, `#[ignore]`d;
`the_factory_files_match_the_design_they_were_generated_from` catches a stale file). Init has no
file — it is generated from the parameter defaults, so it cannot be deleted and cannot drift.
Factory presets never set `accentnote` or `slide`: those are the sequencer's per-step buttons, and
`no_factory_preset_moves_a_per_step_switch` pins it.

## `editor`, `params`, `routes` and `telemetry` are public

They are `pub`, with the `Section` enum, its `SECTIONS`, `title()` and the card grouping the flow
reads, so [`apps/mxm-layout-lab`](https://github.com/mxm-audio/newdawn-workspace/blob/main/apps/mxm-layout-lab/AGENTS.md) can draw **these real cards**
on its bench instead of copying the section code, which would then drift.

It began as a branch-only change for that lab and **is now permanent**, because the reflowing layout
the lab was built to judge shipped on 2026-09-04: the same section data that feeds
the paging renderer in this editor is what the bench re-draws. Nothing else changes — no item's own
behaviour moves, and the shipped `cdylib` and its CLAP entry point are untouched.

## Activation refuses a rate the DSP cannot hold

`activate` returns `false`, before anything changes, for a non-finite host rate or one below
`mxm_mono_03_dsp::MIN_SAMPLE_RATE`, 1 kHz: a NaN rate, or one low enough for a corner's floor to
cross 0.45 of it, panicked on the audio thread.
`activation_refuses_a_non_finite_rate_and_any_below_the_floor` holds the refusal, and
`the_rate_floor_activates_and_plays_at_every_parameter_extreme` a held note at the floor with every
parameter at its default and at either end.

# Work Guidance

# Verification

```bash
cargo test -p mxm-mono-03
cargo test -p mxm-mono-03 --lib every_card_passes_the_tree_checks_in_every_state
# Every page, light and dark, for review -> target/layout-tree/mxm-mono-03/<MXM_PICTURES tag>/
MXM_PICTURES=after cargo test -p mxm-mono-03 --lib tree_pictures -- --ignored
cargo clippy -p mxm-mono-03 --all-targets
cargo xtask bundle mxm-mono-03 --release
clap-validator validate "target/bundled/mxm-mono-03.clap"
```

Run a debug bundle as well as release because `assert_process_allocs` is debug-only. The keyboard
coverage check runs in two frames — the init patch and every route present
(`the_keyboard_cursor_reaches_and_operates_every_route_revealed`).

Through the player, against the release bundle: `plugins/mxm-mono-03/host-tests/tests/behaviour.rs`
and `plugins/mxm-mono-03/host-tests/tests/golden_audio.rs`. The baseline: `cargo test -p mxm-mono-03 --release --lib baseline --
--ignored --nocapture --test-threads=1`, whose timings count only on a quiet machine.

**Not run:** a real DAW, and any listening comparison against hardware. Fidelity is UNVERIFIED — see
[`crates/mxm-mono-03-dsp/AGENTS.md`](../../crates/mxm-mono-03-dsp/AGENTS.md).

# Child DOX Index

No child AGENTS.md files.
