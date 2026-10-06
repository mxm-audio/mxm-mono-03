# NOTES.md — crates/mxm-mono-03-dsp

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples. AGENTS.md is the contract; this file is the reference it links to.

*Since the split (2026-10-06):* the `plans/` cited below are the design history, in the private
archive.

## The accent circuit is the instrument, and it has memory

`research:filters/machines/tb303-diode-ladder.md` §4 measures the diode ladder and concludes **the
filter core is not the sound** — at equal output it is only 1.2x dirtier than a transistor ladder.
Its §5 ranks "the envelope, accent and drive structure" first and calls it *"a voice, not a
filter"*. `accent.rs` is that ranking taken literally, and it is where changes should be aimed
before the filter.

Three things happen on an accented note, and the third is the one emulations miss:

1. the note is louder, through a short lag;
2. **the filter envelope's decay is forced to its minimum**, overriding the Decay control for that
   note only — so an accented note is a *different envelope*, not a louder one;
3. **a sweep stage adds to the cutoff and holds charge between notes**, so consecutive accents
   climb.

**(3) is why accent is a module rather than a multiplier.** Its output depends on what previous notes
did, so it cannot be expressed as a per-note gain or as a parameter offset applied per step. Nothing
in `mxm-mono-01` has this shape.

**Resonance chooses accent's character, not its depth.** On the hardware the sweep's pot *is* the
second section of the resonance pot: turned down, accent reads as extra envelope modulation; turned
up, it becomes the slow sweep that climbs. Modelling accent without this gives an accent that is
merely louder.

**The sweep must charge slowly relative to a note.** A fast charge saturates inside the first gate and
every accent then peaks in the same place — nothing climbs. That was a real defect here, caught by
`consecutive_accents_climb`.

## Slide is a switch; the gate is separate, and that separation is load-bearing

**`slide` says the pitch glides. The gate says whether anything retriggers.** The hardware couples
them in one button; here they arrive by different routes — the switch from the patch, the gate from
the host — so `note_on` reads them separately. A slide with the gate held is the hardware's slide; a
slide with the gate closed is a bend into a new note, which is the only reading that is not silent.

When the gate *is* held across the boundary, the second note:

- **retriggers neither envelope** — no new attack, no new filter sweep;
- **takes no accent**, because there is no note-on to accent.

That is [`docs/modulation/04-glide-and-portamento.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/modulation/04-glide-and-portamento.md)
§4.4 followed directly. Getting it wrong gives two notes with a pitch bend between them, which is a
different and much more ordinary sound.

**A slide needs an open gate, and the *gate* is the test — not whether the amplifier is still
ringing.** Those differ by about a tenth of a second, which is how long the release takes to fall
below the idle threshold. Ask `is_active()` and a note released halfway through its step still counts
as sounding, so the next note is treated as slid, retriggers nothing, and inherits a dying release —
**it never speaks**. Two tests hold the pair: `the_first_note_of_a_phrase_does_not_glide` and
`a_slide_from_a_released_note_still_speaks`. The acid demo is what found the second, and every slid
step in it was silent.

**A slide takes two steps to make, and only one of them is the slid one.** Its predecessor must hold
its gate to the boundary or there is nothing open to slide from — the player's own rule,
`Pattern::held_past_gate` = `tied(n) || tied(n + 1)`, both clauses needed for exactly this reason.
Anything driving this voice from a pattern has to do the same; both examples do.

**The slide time is fixed and is not a control.** The hardware's is an RC in the pitch path and its
per-step button is on or off, so the only decision is *whether* a transition slides.
`Params::glide_s` exists so tests can vary it and a measurement can correct it; the plugin always
passes `SLIDE_TIME_S`.

## The pitch has three inputs, and only one of them lags

The lagged note number is `target_note + glide_offset` — the slide carried as its **remaining
distance**, which lands exactly, where the note-number form stalled a cent flat at the circuit's
60 ms (`a_slide_lands_exactly_on_its_note`; mxm-mono-01's
[`crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md), *Numeric
contracts*). `Params` carries two offsets on top of it:
`tune_semitones` (the tuning control plus the channel bend) and `expression_semitones` (the host's
per-note pitch expression). **Both are added after the lag**, so neither is smeared by the slide's
RC — a curve an author drew against one note arrives at the oscillator as drawn.

They are separate fields rather than one sum because the patch has to be able to say which moved:
one is the performer's and the patch's, the other is the host's and belongs to a single note. The
plugin sums the first two before handing them over; the third stays its own field all the way down,
which is also the shape a **polyphonic** sibling needs, where it becomes per-voice state.

## The filter is the pole set, and its cutoff is clamped by the widest pole

Four one-poles at spread positions with global feedback — no diode equations and no coupling
constant to tune. Two consequences bind:

- **`k` around 18, not 4**, and **normalised against each configuration's own threshold**.
  `native_resonance` is the seam, and it scales by `config.threshold()` rather than by a constant.
  That was a constant while only one configuration was reachable, and it looked harmless — but the
  moment the EMS set could be selected, the same knob position sat at 63% of threshold on one and
  past it on the other, because the thresholds are 18.34 and 10.08. Switching model would have
  changed how resonant the filter was rather than what kind of filter it was.
  `resonance_means_the_same_thing_on_every_configuration` holds it.
- **It does not sing at its cutoff** — 1.1955x, a minor third sharp. Ask `oscillation_ratio`; never
  assume 1.0.

**The *nominal* cutoff is clamped, not each stage.** Clamping stages independently would keep the
filter stable while silently changing the ratios between the poles — which is silently turning it
into a different filter as the cutoff rises. Clamping the nominal cutoff by the widest pole's
headroom keeps the ratios exact at every setting, and costs top end. That cost is why the
instrument's cutoff control shows a **position, not a frequency**; the trap is worked out in
`tb303-diode-ladder.md` §10.5, and range, calibration and clamp have to be chosen together.

## The hardware's constants are inputs, and every one defaults to the hardware

The envelope attacks, the amplitude decay, the accent decay and sweep rate, the drive and the pole
set all arrive through `voice::Params` rather than being read from a constant. The constants remain
as the **defaults**, so `Params::default()` is the machine — `the_init_patch_is_the_hardware` pins
that.

**Ranges are bounded here, not only in the plugin**, because a parameter reaching this crate is
user-generated input: `envelope::ATTACK_MIN_S`/`MAX_S`, `accent::SWEEP_SCALE_MIN`/`MAX`.

**The accent sweep is one control over two time constants.** Stretching them together preserves the
ratio, and that ratio is what produces the climb — partial charge per note against a slower leak.
Exposing them separately would offer a thousand settings in which nothing climbs at all.

**Switching the filter model is control-rate.** `set_config` changes the pole set, so it happens at
a block boundary, never mid-sample; the integrator states are kept deliberately, since clearing them
would silence the filter on every switch.

## The droop is the topology — compensate outside the filter, never inside

With resonance up, 8.75 units of input buy 0.5 of output. This is a filter you **must** drive hard to
hear at all, so drive and makeup are part of the voice — and neither is a user control, because the
hardware has none. Do not add Q compensation inside the filter: that is the same rule
`mxm-mono-01`'s doc states about its own ladder, for the same reason.

**`MIN_SAMPLE_RATE` (1 kHz) is the lowest rate the plugin activates at.** `f32::clamp` panics on a
NaN or crossed bound, and the nominal cutoff's `20 Hz ..= 0.45 × rate ÷ widest pole` crosses below
157 Hz.

## Modulation is routing: nine sources, three targets, a floor that is circuit

`routing.rs` is this instrument's declaration on the shared
[`mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md):
its sources, its targets, each pair's full scale, what each source reaches, and which routes the
init patch holds (`plans/plan-mxm-mono-03-modulation.md` §2 to §5). **Nothing in the voice asks
whether a route is the machine's own**; `INIT_PRESENT` and `INIT_AT_FULL` are the whole of that.
The routing travels **beside** `Params`, never inside it.

| # | Source | Value |
|---|---|---|
| 0 | Key | The key **after the slide's lag**, from middle C over `KEY_UNIT_SEMITONES`, 60 — so a slide moves a key-tracked target with the pitch |
| 1 | Velocity | The note-on's velocity as `v − 1`; a slid-into note keeps it, as it keeps its envelopes |
| 2–4 | Wheel, Pressure, Bend | The channel the plugin reduces to; bend ±1 |
| 5 | Filter envelope | 0…1 |
| 6 | Accent | **The accent circuit's filter output** at the Accent knob's depth, reading the resonance *control*; published at `ACCENT_UNIT`, a half, because the sweep peaks at 1.6 |
| 7 | Accent level | **The circuit's amplifier output** at the knob's depth, 0…1 |
| 8 | Oscillator | The selected waveform before the drive |

Key, Velocity, Wheel, Pressure and Bend are published through `mxm_modulation::standard`, so each is
zero at its rest and means what it means on every instrument (`plans/plan-modulation-standard.md`).

| Target | Law | Full scale at amount one — the machine's paths, then every added one |
|---|---|---|
| Cutoff | `ENV_MOD_FLOOR_OCTAVES × fenv + Σ`, in octaves | Filter envelope `5 · (1 − F)`; Accent 4 per unit of signal, doubled to undo the half unit; added 4, Key one octave per octave |
| Resonance | The control plus `Σ`, clamped to 0…1 | Added 1, Key a fifth per octave; Accent 2 |
| Amplitude | `aenv × standard::amplitude_factor(Σ)`: silence to double | Accent level 1; added 1, Key a fifth per octave; Accent 2 |

**Every path the TB-303 did not have takes the collection's standard reach**: an added cutoff route
used to take Env Mod's full five octaves. **The amplitude sum is bounded at the standard's one**,
where it was 64 — the accent at full reaches exactly double, as the circuit does, and several routes
together reach no further (`plans/plan-collection-sync.md` D8, closed). No factory design used an
added route and no amplitude sum passed one, so the bank capture is identical before and after.
`conformance.rs`'s `Declared` runs the standard's checks over the real tables, each falsified once;
the plugin's tests reuse it through the `conformance` feature.

**Every source is published before any target reads**, so no route this instrument can make is
backward; `a_route_from_the_oscillator_lands_this_sample_in_every_target` reads that off real
samples. **The Env Mod floor is circuit**: the voice adds it whatever is routed, so removing the
route leaves it (`the_floor_survives_removal_zero_and_every_positive_amount`). **The accent
circuit's two routes are bit-identical to the expressions they replaced**, because the half unit and
its doubling are exact; the Env Mod route agrees with the retired knob to rounding, because
splitting the floor off re-associates one multiply
(`the_machines_own_routes_reach_what_the_old_expressions_did`). What each source reaches, for a
reading, is `SOURCE_PEAK` — `mxm-mono-00`'s rule — and nothing evaluates with it.

## The ladder's loop is bounded by its saturator, and that was measured rather than assumed

`mxm-mono-00`'s phaser ran away to infinity when a summing input drove its coefficient coherently
near Nyquist (mxm-kit's `docs/code-review-notes.md` §7), and this instrument's Oscillator can now drive the
diode ladder's cutoff and resonance the same way. **It does not pump**:
`the_ladder_driven_at_audio_rate_from_silence_does_not_pump_itself` alternates the cutoff between
its clamp ends and the resonance between none and full, at every period from two samples, with no
input, from 1 kHz to 768 kHz, and the ladder stays at or below its own self-oscillation. The
structure is why: the feedback passes through `tanh_approx`, and each TPT stage is non-expansive for
any `g` in 0…1. **So no bandwidth sits on the routed cutoff or resonance**, unlike the phaser's
MANUAL IN.

**It and `audio_rate_routes_into_cutoff_and_resonance_stay_bounded_through_the_voice` are guards
with no fix to remove.** Neither went red under an energy-injecting coefficient change, a fully
linear loop or the cutoff clamp removed — the ladder is that well damped — so they stand for the
property rather than for a repair. **The cutoff clamp is load-bearing in a way worth knowing**: with
it removed, an unclamped coefficient produced a NaN, and *no key down is exact silence* went red
too, because `NaN × 0` is `NaN` behind a closed amplifier.

## Dependencies: one at runtime

**One runtime dependency,
[`mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md)**,
which has none of its own and holds this same 1.87 floor — the routing conversion added it, and the
MSRV override still rests on the shipped graph staying that small. `cargo tree -p mxm-mono-03-dsp -e
normal` shows that crate and nothing else.

`[dev-dependencies]` holds **`mxm-measure`**, the collection's measurement rulers — zero dependencies
at this same floor, reaching only tests and `examples/`, never a shipped `.clap`.
mxm-kit's [`crates/mxm-measure/AGENTS.md`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-measure/AGENTS.md)'s
verification section checks that rather than asserting it.

It also holds **`mxm-audio-file`**, which writes the listening demo, and **`mxm-audio-file-decode`**,
which its test reads the file back through — test-only edges on the same terms. The decoder's
MPL-2.0 symphonia therefore reaches this crate's tests and never its shipped graph.

**`mono_03_render_demo` and `acid_demo` no longer write their WAV by hand**; they encode through
`mxm_audio_file` and apply their own headroom at the call site.

**The rulers are shared, the thresholds are not.** `mxm-measure` is a `[dev-dependencies]` entry —
zero dependencies at this same 1.87 floor, and **not in the shipped graph**, which is what the
manifest's *no runtime dependencies* comment means. Measurements come from there; every bound and
its headroom stays in the test that argues for it.

## Why `acid_demo` is kept

`acid_demo` earns its place beside `mono_03_render_demo` rather than duplicating it: acid is a
performance on the filter, so a static patch playing the right notes does not sound like the genre
however correct the voice is. Sweeping the cutoff is also the only way the accent circuit's
behaviour becomes audible, since resonance is what turns accent from extra envelope into the
climbing sweep. **It found a defect the unit tests did not**, which is the argument for keeping it.

## An oracle that cannot fail is not an oracle

`consecutive_accents_climb` was checked by **sabotage**, not trusted: clearing the sweep on release
— so charge does not survive the note — makes all four peaks identical (0.2857 each, ratio exactly
1.0) and fails it on the second note. That check was run. Do the same to any test guarding
accumulated state.

## What is not verified, and must not be claimed

**No hardware was measured, here or in any source this instrument rests on.** Every time constant in
`accent.rs`, the duty-cycle figures in `oscillator.rs` and the slide time in `voice.rs` are
**published or chosen, not measured** — the modules say so individually. The tests above prove the
model is self-consistent; **they do not establish that it sounds like the machine.**

The gate that would is a **listening comparison against reference recordings**, and it has **not been
run**. Until it has, fidelity is UNVERIFIED and should be recorded that way rather than implied by a
passing suite.

**The golden-audio test is `plugins/mxm-mono-03/host-tests/tests/golden_audio.rs`**, added at M0 of
`plans/plan-mxm-mono-03-modulation.md` (its N5): a fixed score through the real player hosting the
real bundle — a plain note, two accents whose sweep climbs, a tied slide and the tail — and
`the_reference_is_sensitive_to_the_envelope_depth` shows the digest can move. **It is recent**: until
it existed a DSP change here could alter the sound with a full suite green, which is why it was added
before the routing conversion rather than after. `plugins/mxm-mono-03/host-tests/tests/behaviour.rs`
measures the same bundle's pitch, cutoff, accent and slide on rendered audio.
