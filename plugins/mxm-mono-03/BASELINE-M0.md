# mxm-mono-03 — pre-conversion reference, captured at M0

`plans/plan-mxm-mono-03-modulation.md` (in the private archive) M0. **These figures stop existing once the routing conversion
starts**, which is why they are captured first and committed rather than re-derived.

Produced by `plugins/mxm-mono-03/src/lib.rs`'s `baseline` module, on the tree at `b0fa24c` with only
the measurement seam, that module and the player's two new mono-03 suites added:

```bash
cargo test -p mxm-mono-03 --release --lib baseline -- --ignored --nocapture --test-threads=1
```

Release only. **The module uses only what both revisions have** — `render_block_for_test`, the
factory files' permanent ids and the two per-step switches — so the same file runs on the converted
tree. The seam is the loop `process()` runs, factored out so the two cannot drift:

```rust
fn render_sample(&mut self) -> f32 {
    let patch = self.next_patch();
    self.voice.process(&patch, self.sample_rate)
}
```

`process()` calls it per sample and `render_block_for_test` calls it for a whole block, omitting the
wrapper's event handling, telemetry and buffer plumbing. Factoring it out moved nothing: the plugin's
40 tests and the DSP crate's 50 pass unchanged.

## Throughput

Taken with nothing else building — this machine had run a conversion's builds all day, and
mxm-kit's `docs/code-review-notes.md` §3 is plain that a timing taken during a build is not a measurement — at
48 000 Hz in 64-sample blocks, note 48 held with its accent button pressed, three passes each:

| Case | Pass 1 | Pass 2 | Pass 3 |
|---|---|---|---|
| Init | 115.3 ns/sample | 118.2 | 117.3 |
| `Acid line` | 117.0 | 115.9 | 117.9 |

**Read the spread before the numbers**: about 3 ns between passes of one case, which is as large as
the difference between the two cases. The gate compares Init against the converted tree's Init
**measured the same way on the same day**, best of several passes each; a difference inside this
spread is not a finding. `Acid line` is the routed patch held fixed across the conversion: Env Mod and
Accent both well up and resonance at 0.85, so both paths the conversion rewrites carry signal. Other
programs on the machine were not controlled. **Not portable across machines.**

```bash
cargo test -p mxm-mono-03 --release --lib baseline::throughput -- --ignored --nocapture
```

## Factory bank — fifty-one reference digests

FNV-1a over the raw sample bits, the digest the player's golden tests use. **Every sound plays one
score**, fixed forever, in 64-sample blocks: a plain note 36; two notes 36 with the accent button
pressed, a step apart, so the forced short decay, the louder amplifier and the sweep's climb are all
in it; a note 36 then a tie to 48 with Slide on; and two seconds of tail — 130 560 samples. Accent and
slide are the machine's per-step buttons, so the score presses them itself: no factory sound sets
either. The last column is how many parameters the file applied: all nineteen, every one.

**Verified reproducible**: two consecutive runs produced identical digests for all 51 renders. With
`MXM_M0_DUMP=<dir>` the module also writes each render as raw `f32`, and
`the_bank_against_the_m0_dump` compares a later build against them sample by sample; the M0 dumps are
local to the machine that took them, not committed.

| Sound | Digest | Peak | Applied |
|---|---|---|---|
| *(Init)* | `3ffa15251904aff5` | 2.9038 | 0 |
| `acid-line` | `20ccbc34148844a1` | 1.0327 | 19 |
| `squelch` | `e7a843335196819e` | 1.0183 | 19 |
| `deep-acid` | `e7f7ec9cba216508` | 1.1897 | 19 |
| `square-acid` | `a47c73ab452d7174` | 1.1802 | 19 |
| `rubber-bass` | `4ca3d1fd1eca0e53` | 1.2967 | 19 |
| `round-bass` | `9e868e48eb0c2c1e` | 1.4012 | 19 |
| `hollow-knock` | `63a0ddc2b04aac2f` | 1.1701 | 19 |
| `dark-drone` | `9fb5abf676ff34f8` | 1.3326 | 19 |
| `bright-saw` | `c95416e9c3ff01a8` | 1.1531 | 19 |
| `buzz-lead` | `81c4011b8b04df69` | 0.7099 | 19 |
| `snap-pluck` | `aa341de658f00495` | 0.8733 | 19 |
| `chirp` | `7738138e39921da8` | 0.7605 | 19 |
| `slow-sweep` | `1e1085e467720b31` | 0.6849 | 19 |
| `overdriven` | `804c30d8f494ea90` | 4.8125 | 19 |
| `three-diode` | `730f488f1ee77aac` | 1.9321 | 19 |
| `lazy-slide` | `3c822870b09fbe50` | 0.8379 | 19 |
| `soft-square` | `77293c8ff6835d34` | 2.5159 | 19 |
| `hard-accent` | `ec5609f82521ce00` | 1.0956 | 19 |
| `climbing-accents` | `fa0993143141d278` | 1.1853 | 19 |
| `low-rumble` | `9e3e0d554d5e7770` | 0.9276 | 19 |
| `classic-acid` | `7f638dae31d62246` | 1.0334 | 19 |
| `rolling-bass` | `698782fef15d2829` | 1.2979 | 19 |
| `detroit-stab` | `69c4a11e2abcc49a` | 1.1268 | 19 |
| `wet-squelch` | `53f2e42251d9b3fb` | 1.0294 | 19 |
| `dry-thump` | `0793d9d4259ad0e3` | 1.1935 | 19 |
| `warm-saw-bass` | `4226c4b8cd810c40` | 2.0828 | 19 |
| `woody-square` | `a105fdc5d04dc847` | 0.8629 | 19 |
| `long-acid` | `8bb4d2649cea2079` | 1.0160 | 19 |
| `short-acid` | `0438f274aaf3940e` | 0.9794 | 19 |
| `hoover-accent` | `79b2f6830301ffa1` | 1.2258 | 19 |
| `fizz` | `c740bec6e7cb0c66` | 2.0992 | 19 |
| `sub-thud` | `7b05324ee8c7cc1a` | 0.9067 | 19 |
| `drive-lead` | `69bd453172829be7` | 6.3719 | 19 |
| `whistle-lead` | `135dd05591518f8c` | 0.6917 | 19 |
| `nasal-lead` | `9d75bd148ee6c643` | 0.9599 | 19 |
| `bright-pluck` | `2ec195c019bcb0f9` | 0.9381 | 19 |
| `muted-pluck` | `c3f12d7e80919e96` | 0.4342 | 19 |
| `ping` | `aaa379df3bc466ec` | 0.7936 | 19 |
| `bongo` | `eeacda2056544cc7` | 0.9541 | 19 |
| `kick-tick` | `d3ef89647783d584` | 0.8245 | 19 |
| `wood-block` | `1e2a37f5f87692e0` | 0.6492 | 19 |
| `laser` | `1b26569d80656fbd` | 1.1282 | 19 |
| `filter-howl` | `464fbb6f059bcb72` | 1.9267 | 19 |
| `slow-open` | `ebaa88a2ffcda168` | 0.5977 | 19 |
| `hum-drone` | `3efbd1644f884c37` | 2.0271 | 19 |
| `accent-run` | `4f3fe5ca7a41fcb7` | 1.0740 | 19 |
| `slide-run` | `257138d61bf10cd8` | 1.1781 | 19 |
| `gritty-saw` | `ae013860e073a733` | 13.0999 | 19 |
| `clean-square` | `fc114de9646dd4fc` | 0.4493 | 19 |
| `mid-acid` | `7e11ef5c350e103c` | 1.8249 | 19 |

**Most peaks are above unity, and that is this instrument at M0, not the measurement**: the voice's
output is not clipped, and Init reaches 2.9 on the accented notes. `gritty-saw` reaches 13.1. Recorded
so a later reader does not mistake it for something the conversion did. Whether the output level is
right is a question for the owner's listening pass, not for this plan.

**What the bank covers for plan §10's comparison**: Init is Env Mod at its floor; the fifty span Env
Mod from 0.1 to 0.95 and resonance from 0.05 to 0.95, so the accent's direct character at low
resonance and its climbing sweep at high are both in it; and every render has the accent climb and
the tied slide.

## The player golden

`apps/mxm-player/tests/t4_golden_audio_mono_03.rs`, added at this M0 (plan N5): a resonant squelch,
a plain note, two accented notes, a tie with Slide on and the tail, through the real player hosting the
release bundle built from this tree. **Pinned at `969baba189f92fe5`**, and
`the_reference_is_sensitive_to_the_envelope_depth` holds that the digest moves when Env Mod does. No
human listening is claimed.

## After the conversion — 2026-09-15

The same `baseline` module on the converted tree, the factory designs translated by
`plans/plan-mxm-mono-03-modulation.md` §4 and regenerated, every preset applying all **72**
parameters.

### Factory bank

**51 renders against the M0 dumps, sample by sample: Init is bit-identical; the other 50 moved, the
worst by 4.2e-4 of full scale, 4.1e-4 of that sound's peak (−68 dB).** Init staying bit-identical is
the structural claim holding — the circuit's floor, the accent circuit's two routes, the new
evaluation order and the seam are all exact. Every factory design sets Env Mod, and its route
re-associates one multiply (plan N6), which is rounding in size; where it grows is where the ladder
sits at its self-oscillation edge and magnifies any ulp:

| Sound | Resonance | Worst difference | Relative to its peak |
|---|---|---|---|
| `wet-squelch` | 1.00 | 4.2e-4 | 4.1e-4 |
| `squelch` | 0.95 | 5.9e-5 | 5.8e-5 |
| `laser` | — | 1.0e-5 | 8.9e-6 |
| every other sound | — | at most 5.0e-6 | — |

No peak changed by more than those differences. The factory digests are re-pinned at the owner's
listening pass (plan B5) and not before.

### The player golden

It moved by at most 6.0e-7 of full scale, 5.3e-7 of its peak against the M0 render, which is
rounding; provisionally repinned at `555a7888da50f235`, as the test's own doc comment records.

*Since the split (2026-10-06):* the player golden is this repository's
`plugins/mxm-mono-03/host-tests/tests/golden_audio.rs` (repinned since, as its doc comment records),
and its digest is compared on Windows only.

### Throughput

The same cases, measured the same way on the same day with nothing else building, three passes each:

| Case | M0 | Converted |
|---|---|---|
| Init | 115.3, 118.2, 117.3 ns/sample | 116.0, 114.0, 112.6 |
| `Acid line` | 117.0, 115.9, 117.9 | 114.1, 115.7, 114.5 |

**The conversion is not slower**: best of three, Init 115.3 → 112.6 and `Acid line` 115.9 → 114.1 —
inside the spread between passes, so read it as *no cost*, not as a saving.

## What this does not establish

- **Nothing about how it sounds.** A digest proves *unchanged*; it cannot prove *good*.
- **Nothing about the host path, except the golden.** The bank is the plugin library, in process.
