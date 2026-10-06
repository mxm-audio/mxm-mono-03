# AGENTS.md — plugins/mxm-mono-03

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell for **mxm-mono-03**, a monophonic acid bass voice inspired by the Roland TB-303.
Identity, parameters, MIDI, presets, telemetry and the editor. The whole voice is
[`crates/mxm-mono-03-dsp`](../../crates/mxm-mono-03-dsp/AGENTS.md).

Shared conventions — nice-plug's API, the init-patch contract, preset rules, `process()` realtime
rules, the editor contract — live in the parent and are not restated here. This doc holds what is
**local to this plugin**. The rationale, measurements and history behind it are in
[NOTES.md](NOTES.md).

# Ownership

`BASELINE-M0.md`, `Cargo.toml`, `README.md`, `control-map.json`, `presets/`, and `src/` —
`lib.rs`, `params.rs`, `routes.rs`, `preset.rs`, `telemetry.rs`, and `editor.rs` with its
`editor/{binding, sections, visuals}.rs`. Its licence is the repository's root `LICENSE`.

`BASELINE-M0.md` is the routing conversion's reference, produced by `lib.rs`'s `#[ignore]`d
`baseline` module through `render_block_for_test`. That is a measurement seam, not a second
`process()`: both render every sample through the same `render_sample`
([NOTES.md § The routing baseline](NOTES.md#the-routing-baseline)).

# Local Contracts

## Permanent identifiers

| What | Value |
|---|---|
| `CLAP_ID` | `dk.mxm.mxm-mono-03` — assembled from `plugin_name!` in `src/lib.rs`, **not** from `CARGO_PKG_NAME`, so renaming the directory cannot move it |
| Parameter `#[id]`s | **Panel:** `tune` `waveform` `bendrange` `cutoff` `resonance` `decay` `accent` `accentnote` `slide` `volume`<br>**Advanced:** `slidetime` `ampattack` `ampdecay` `filterattack` `accentdecay` `accentsweep` `drive` `filtermodel` |
| Routing `#[id]`s | `mod_<target>_<source>`, the amount, and `mod_<target>_<source>on`, the presence — targets `cutoff` `res` `amp`, sources `key` `vel` `wheel` `press` `bend` `env` `accent` `accentlevel` `osc`. Written out in `routes::ROUTE_IDS`, which `the_id_table_is_what_the_derive_actually_produces` holds to the derive |
| Retired `#[id]`s | `envmod` — never to be reused |

Treat all of them as public interface.

## Modulation is routing; the machine's wiring is the init patch

- `routes.rs` declares a presence and a signed amount for every *(target, source)* pair; the DSP's
  `routing` module evaluates them, and nothing asks whether a route is the machine's own
  ([NOTES.md § The machine's modulation is routing](NOTES.md#the-machines-modulation-is-routing-and-its-wiring-is-the-init-patch)).
- Init wires exactly (Cutoff ← Filter envelope), (Cutoff ← Accent) and (Amplitude ← Accent level)
  (`the_init_patch_wires_exactly_the_machines_own_routes`).
- **A route's amount reads what its pair delivers** (`SOURCE_PEAK`), through the collection's one
  route parameter `mxm_modulation_params::reading::amount_param_at`:
  `a_route_reads_what_its_pair_delivers_and_reads_back`, `every_route_parameter_says_what_the_dsp_does`.
- **A reading never prints a negative zero**: numbers go through `mxm_modulation_params::signed`
  (`every_reading_survives_the_hosts_round_trip_a_rounded_zero_included`).
- **`envmod` is retired and never reused** (`no_retired_id_reappears`); no `filter_state` is built
  (owner, 2026-09-15), so a saved state loses what `envmod` held.
- Per callback, `resolve_topology` → `Routes::topology_from` **snaps each newly present route's
  smoother** to its stored depth
  (`a_re_added_route_arrives_at_its_stored_depth_rather_than_ramping_from_a_stale_one`); per sample,
  `render_sample` → `Routes::advance`. `process()` and `render_block_for_test` both call these.
  `a_route_arriving_after_an_idle_span_is_block_partition_invariant` holds block-size invariance.
- The routing must cost no more than `BASELINE-M0.md` records.

## Parameter text survives the host's round trip

- A formatter must not switch unit or precision at the raw value: `params.rs`'s `v2s_time` chooses
  its unit from what the millisecond text would round to
  ([NOTES.md § Every parameter's text](NOTES.md#every-parameters-text-survives-the-hosts-round-trip)).
- `params::tests::every_parameter_reads_the_same_after_the_hosts_round_trip` holds every parameter.

## Init deviations, recorded and local

- **Env Mod's floor is circuit, not an amount**: the voice adds `routing::ENV_MOD_FLOOR_OCTAVES`
  whatever is routed, and the (Cutoff ← Filter envelope) route starts at zero on top of it
  (`env_mods_floor_is_circuit_and_its_route_starts_at_zero`). **This stays local**, never a
  collection rule
  ([NOTES.md § Env Mod's floor](NOTES.md#env-mods-floor-is-circuit-not-an-amount--a-recorded-deviation-from-the-init-patch-contract)).
- **The accent circuit's two routes start at full** — fixed-gain wiring behind the Accent knob
  (`the_accent_circuits_routes_start_at_full`).
- Everything else starts at zero (`every_amount_starts_at_zero`).

## There is no distortion

- The TB-303 has none (amplifier to output jack), so this plugin has none, under the collection's
  rule that an instrument ships the effects its original had and no others
  ([`../AGENTS.md`](../AGENTS.md)). The distortion is a separate plugin.
- *The clones build one in* and *it is most of the recorded sound* are true and are not arguments:
  neither is about the TB-303. The full call and its evidence:
  [NOTES.md § There is no distortion](NOTES.md#there-is-no-distortion-and-that-is-a-scope-decision-rather-than-an-omission).
- **Do not confuse this with drive**: the drive into the filter is part of the voice and is not a
  user control.

## The advanced disclosure

[NOTES.md § The advanced disclosure](NOTES.md#the-advanced-disclosure-holds-the-machines-fixed-constants)
has the table of what each control frees.

- The eight Advanced ids are **ordinary automatable parameters**, not persisted state, and **every
  one defaults to the hardware** (`every_advanced_control_defaults_to_the_hardware`).
- **None of them is smoothed.** **They are deliberately absent from `control-map.json`.**
- Ranges are bounded in the DSP (`envelope::ATTACK_MIN_S`/`MAX_S`, `accent::SWEEP_SCALE_MIN`/`MAX`).
- Deliberately **not** exposed: makeup gain, the sweep ceiling, the Decay control's range ends, the DC
  blocker, the oscillator's duty-cycle endpoints.

## Slide, pitch expression and the gate

- **`slide` is a switch**; `slidetime` exists only behind the advanced disclosure, defaulting to
  `SLIDE_TIME_S`. No front-panel time knob (`slide_is_a_switch_and_its_time_is_advanced`;
  [NOTES.md § Slide is a switch](NOTES.md#slide-is-a-switch-and-its-time-is-advanced)).
- **`expression_semitones` is added after the glide lag**, never summed into the target before it
  (`a_pitch_expression_rides_on_a_slide_rather_than_replacing_it`;
  [NOTES.md § A drawn pitch bend](NOTES.md#a-drawn-pitch-bend-rides-on-the-slide-and-does-not-become-a-second-slide)).
- Accent and slide are parameters; **whether the envelopes retrigger is not a parameter, and must
  not become one** — it follows the gate
  ([NOTES.md § The two per-step buttons](NOTES.md#the-two-per-step-buttons-are-parameters-the-gate-is-not)).
- **A note-off or a choke is matched against the note actually sounding; an unmatched one is
  dropped** (`note_tracking::a_stale_choke_during_a_slide_leaves_the_new_note_sounding`,
  `a_choke_for_the_sounding_note_still_cuts_it`). All Sound Off still resets whatever is sounding.

## Velocity, wheel, pressure and accent

- Velocity, the wheel and pressure are routing sources only; nothing routes them in Init
  (`the_new_midi_paths_change_nothing_while_no_route_reads_them`). **Velocity is not an accent**
  ([NOTES.md § Velocity, the wheel and pressure](NOTES.md#velocity-the-wheel-and-pressure-are-routing-sources-and-nothing-more)).
- `accent` is the depth and `accentnote` the per-step button; one control cannot be both.
- `control-map.json` **deliberately omits `accent`, `accentnote` and `slide`** until an unknown role
  is inert rather than fatal; `filter.env_amount` names `mod_cutoff_env`
  (`a_control_map_role_never_points_at_a_dead_route`;
  [NOTES.md § Accent is two parameters](NOTES.md#accent-is-two-parameters-and-the-control-map-claims-none-of-the-new-roles-yet)).

## The editor

The brief is [`docs/briefs/mxm-mono-03.md`](../../docs/briefs/mxm-mono-03.md); the layout detail is
[NOTES.md § The editor, and its brief](NOTES.md#the-editor-and-its-brief).

- **Developer channel** (`plugins/AGENTS.md`, *A developer channel in every editor*), with
  `MXM_DEV_CC` set: CC 119 category or Parameters, CC 117 the preset browser, CC 118 taken and
  dropped (nothing is disclosed), CC 116 the theme, unsaved.
- Three stable paging items, `Voice`, `Filter` and `Advanced`, titled by `sections::TITLES`; Advanced
  is always reachable. Each target's routes are a stack drawn by `mxm_modulation_params::ui::stack`.
- **Every card is a `mxm_ui::tree`**; floors are computed, each card exactly as wide as its floor. The
  opening size and the minimum are derived: `the_opening_size_is_the_budget_hugged`,
  `the_minimum_holds_the_widest_floor_and_its_gutters`, `the_app_bar_holds_in_the_minimum_window`,
  `every_dynamic_page_fits_and_every_card_is_reachable`,
  `every_card_passes_the_tree_checks_in_every_state`.
- **No caption on any card** (the owner, 2026-09-27; design system §7.6).
- `sections::draw` keeps its signature for `apps/mxm-layout-lab` (in the private archive).
- **Volume is in the app bar, not the Voice card** (owner, 2026-09-18), under `VOLUME_CARD`;
  `the_volume_is_drawn_in_the_app_bar_and_on_no_card`.
- `src/telemetry.rs` is the **only** DSP → editor channel: `Arc`-shared, atomics only, written once
  per block. The editor sets this crate's MSRV at **1.95**; the DSP crate stays at 1.87.
- `editor`, `params`, `routes` and `telemetry` are **permanently `pub`** for the layout lab (private archive); nothing
  else changes for it
  ([NOTES.md § Public modules](NOTES.md#editor-params-routes-and-telemetry-are-public)).

## Presets

- `preset.rs` is this instrument's `Instrument` impl on mxm-kit's `mxm-preset`; `editor/binding.rs`
  re-exports `mxm_preset::binding`
  ([NOTES.md § preset.rs](NOTES.md#presetrs-is-this-instruments-instrument-impl-and-its-factory-set)).
- The factory files are generated from `FACTORY_DESIGN` (`write_the_factory_presets`, `#[ignore]`d);
  `the_factory_files_match_the_design_they_were_generated_from` catches a stale file. Init has no file.
- Factory presets never set `accentnote` or `slide` (`no_factory_preset_moves_a_per_step_switch`).

## Activation refuses a rate the DSP cannot hold

`activate` returns `false`, before anything changes, for a non-finite rate or one below
`mxm_mono_03_dsp::MIN_SAMPLE_RATE`: `activation_refuses_a_non_finite_rate_and_any_below_the_floor`,
`the_rate_floor_activates_and_plays_at_every_parameter_extreme`
([NOTES.md § Activation](NOTES.md#activation-refuses-a-rate-the-dsp-cannot-hold)).

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
and `plugins/mxm-mono-03/host-tests/tests/golden_audio.rs` (its digest pinned on Windows only). The baseline: `cargo test -p mxm-mono-03 --release --lib baseline --
--ignored --nocapture --test-threads=1`, whose timings count only on a quiet machine.

**Not run:** a real DAW, and any listening comparison against hardware. Fidelity is UNVERIFIED — see
[`crates/mxm-mono-03-dsp/AGENTS.md`](../../crates/mxm-mono-03-dsp/AGENTS.md).

# Child DOX Index

No child AGENTS.md files.
