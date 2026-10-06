# mxm-mono-03 — UI design brief

Required by `MXM_DESIGN_SYSTEM.md` §14, written before implementation. Answers the ten questions in
order, then records the deliberate deviations.

*Since the split (2026-10-06):* the design system, `MXM_CONTROL_MAP.md`, `docs/AGENTS.md` and
`crates/ui` are mxm-kit's ([`docs/MXM_DESIGN_SYSTEM.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/MXM_DESIGN_SYSTEM.md));
`plugins/mxm-mono-01/src/telemetry.rs` is in mxm-mono-01's repository; `plans/AGENTS.md` is in the
private archive. The no-image rule is mxm-kit's
[`collection-rules.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/collection-rules.md#research-boundary).

**Instrument:** monophonic acid bass voice. Architecture inspired by the Roland TB-303; the
interface is not.

---

## 1. Primary sound-design task

**Performing the filter while a sequence runs.** Not patch programming — this instrument has almost
no patch to program. A line is written in the sequencer and the sound is made by working the cutoff
and resonance against it, in real time, for as long as the pattern loops.

That is a different task from `mxm-mono-01`'s *tweak while playing*, and it is more specific: there
are two controls that are moved constantly and eight more that are set and left. The editor's job
is to make the two obvious and immediate, and to keep the rest from competing with them.

## 2. The three to five parameters users reach for most

1. **Cutoff** — moved continuously. The performance is mostly this.
2. **Resonance** — its partner, and here it does double duty: it also decides whether accent reads
   as extra envelope or as the climbing sweep, so it changes character and not only sharpness.
3. **Env mod** — how much of the note's shape the filter takes. Since the routing conversion it is
   the (Cutoff ← Filter envelope) route, the first row of the Filter card's stack.
4. **Decay** — with one filter envelope, this is the difference between a spit and a smear.
5. **Accent** — how hard an accented note hits.

Cutoff and Resonance take **Primary** sizing and sit adjacent at the visual centre of gravity. The
other three are Standard.

## 3. Signal flow that must be visible

```
Oscillator  →  Filter  →  Amplifier  →  out
                 ↑            ↑
   filter env ───┤            │
   accent ───────┴────────────┘   (accent reaches both, and the filter twice)
```

**Since the routing conversion each of those arrows is a route present in the init patch** — Env
Mod, and the accent circuit's two outputs — drawn as a stack under the controls it moves, so the
flow reads on the cards themselves: *Cutoff from Filter envelope* and *Cutoff from Accent* in the
Filter card, *Amplitude from Accent level* in the Voice card. The forced decay and the held sweep
stay circuit, which is why the sweep display stays.

Two things must read without a manual, and both are things people get wrong about this machine:

- **Accent is not a volume.** It reaches the amplifier *and* the filter, forces a different envelope
  decay, and accumulates across notes. Drawing it as a single line into the amplifier would teach
  the wrong model.
- **There is one filter envelope and no amplifier envelope to speak of.** The amplitude shape is
  fixed; what moves is the filter. Anyone expecting two ADSRs needs to see immediately that there
  are not two.

Accent's routing is drawn with the modulation colours (§8.4) on the controls it reaches.

## 4. Which controls belong in Play view

**Not applicable — no `Play` view.** See §6.

## 5. Advanced controls and their disclosure

**Shown, not hidden. Below a divider, on a different surface.**

This is a deliberate departure from `mxm-mono-01`, which puts its one addition behind a labelled
expander. The reason is the instrument's size: **eighteen parameters of its own, nine of them above
the line**, beside fifty-four routing parameters that draw nothing until a route is present. An
expander earns its place when it protects a person from a wall of controls; here it would hide eight
things behind a click on an instrument whose entire appeal is that everything is in reach. §4.3's
rule that essential controls must not disappear is served better by showing them than by making them
one interaction away.

So the panel is two zones, each of module cards:

| Zone | Cards | Surface |
|---|---|---|
| **The instrument** | **Voice** (Tune and Accent on one row; the Amplitude stack; then Waveform, the two per-step switches, and the accent-sweep display) · **Filter** (the response curve, then Cutoff, Resonance, Decay, and the Cutoff and Resonance stacks) | `surface-1` |
| **Advanced** | Slide time · Amp attack · Amp decay · Filter attack · Accent decay · Accent sweep · Drive · Filter model · Bend range | `surface-2` |

**Volume is in neither zone's cards: it is the instrument's master output, an inline slider in the
app bar beside the level meter** (§3.1 slot 6; the owner's ruling of 2026-09-18 for every
instrument). It is still one of the controls the machine let you touch.

**The divider carries a label**, because a boundary with no name is a boundary someone has to guess
at. `surface-2` is the system's existing *nested groups* token, so the tonal change costs no new
colour and no literal in plugin code.

**Accent note and Slide are in the upper zone**, though neither was on the hardware's panel. They
are the machine's two per-step buttons, and they are performed rather than configured — the same
argument that puts them in the sequencer on the original.

**What the split means, stated so it does not drift**: above the divider is what the machine let you
touch; below it is what the machine fixed. That is a rule about provenance, not about frequency of
use, and it is the same rule `mxm-mono-01`'s brief §5 states. Slide time is below the line despite
being useful, because the machine's slide time was an RC.

## 6. Views

**Space-derived pages**, following design-system §3.2. Voice is an indivisible Generators card,
including its accent/amplifier controls; Filter and the separately labelled Advanced card are
Tone. Advanced stays fully reachable, not disclosed or stranded below another scroll area.
Full category names, no fixed tab count, no bar when all three cards fit.
Parameters remains the separate developer testing surface at CC 119 value 127, without a tab.
Controller roles/pages are unchanged.

## 7. Identity accent

**Acid lime.** Dark `#B6E82B`, light `#4F6B00`.

Measured against the surfaces it is drawn on, rather than judged by eye:

| | vs `surface-1` | vs `surface-2` |
|---|---:|---:|
| Dark `#B6E82B` | **12.09 : 1** | **11.06 : 1** |
| Light `#4F6B00` | **6.11 : 1** | **5.12 : 1** |

Both themes clear 4.5 : 1 for text and 3 : 1 for control boundaries with room to spare, which
matters because the advanced zone puts accent-coloured controls on `surface-2` as well as
`surface-1`.

**Why this hue and not the alternatives.** Orchid and coral also pass, but both sit about 20° of hue
from a reserved colour — orchid from `mod-key-voice` violet, coral from `danger` — and a
control-boundary accent that reads as a warning is a real cost. Acid lime sits ~85° from `success`,
which is the nearest reserved hue, so it is the most separable of the three as well as the most
apt.

**§5.3's trade-dress rule is satisfied**: the hardware's arrangement is silver with orange and red,
and this is none of those.

**It also takes lime off the table for `mxm-mono-01`**, whose accent is still unchosen. That is a
consequence, not an oversight: its brief lists magenta/orchid and warm coral as its other
candidates, and both remain available to it.

## 8. Live visualizations

Three, and the test each had to pass is whether it answers a question the controls cannot.

1. **Filter response curve**, with the envelope's and accent's modulation ranges as secondary
   traces, **computed from the routes** — the floor, the Env Mod route and the Accent route — so the
   curve cannot show a reach the patch has not got. This earns its place twice over here: it shows
   the **sagging corner** that is the whole character of the diode ladder, and it is the only place
   the cutoff's actual frequency appears at all — the control shows a position, deliberately, so the
   curve is where a number lives.
2. **The accent sweep's charge.** The one display unique to this instrument. Consecutive accents
   climb because a capacitor has not finished discharging, and *nothing on the panel can show that*
   — the Accent knob has not moved, the note is simply brighter than the last one. A small trace of
   the accumulated charge is the answer to "why did that happen", and without it the instrument's
   most distinctive behaviour looks like a bug.
3. **Output level with clip indication** in the app bar, per §3.1, with Volume drawn right beside
   it as the bar's master output control.

Deliberately **not** included: an envelope display. Two attack-decay envelopes with one control
between them are fully described by that control, so a picture of them would restate the panel.

### Ownership

A single `Telemetry` struct, `Arc`-shared, **atomics only**, written once per block, stopping when
the editor is closed — the pattern `plugins/mxm-mono-01/src/telemetry.rs` already sets, including
its two rules worth carrying: a **peak is max-combined and reset on read**, and a **clip latches**
until acknowledged.

| Visualization | Writer | Truth model |
|---|---|---|
| Filter response | UI thread, computed from parameters + sample rate | **Declared approximation** — the linear analytic response, ignoring drive and the feedback nonlinearity |
| Accent sweep charge | Audio thread, once per block, from `Accent::sweep()` | **Exact** — the value the DSP used |
| Output level + clip | Audio thread, once per block | **Exact** — peak of the samples produced |

The filter curve's approximation is declared here rather than discovered later: at high resonance or
high drive a measured sweep will not match it. It still shows the sagging corner, which is what it
is for.

## 9. What is removed from the source hardware layout, and why

**Kept:** the control set, and the signal flow it implies.
**Removed:** the panel layout, appearance, geometry, control style, colour arrangement, typography,
trade dress.

The hardware's own panel was consulted for the **control set and its order** — six knobs reading
TUNING · CUT OFF FREQ · RESONANCE · ENV MOD · DECAY · ACCENT, a waveform switch at the left edge, a
volume knob beside the power switch, and ACCENT and SLIDE again as per-step buttons in the sequencer
half. Nothing about how it looks was taken, and per `docs/AGENTS.md` and `plans/AGENTS.md` no image
of it is kept in this repository: **name an influence, do not ship a picture of one.**

| Removed | Why |
|---|---|
| The panel layout | §2 forbids copying the inspiring instrument's panel. Controls are grouped by task; only the sequence survives, because the sequence is the signal flow |
| The sequencer | Out of scope: it is the player's, and this instrument is the synth half |
| The keyboard and its transpose controls | §2 forbids a decorative keyboard; note input is the host's |
| The pitch-mode / write-mode switching | Sequencer surface, and none of it touches the sound |
| Vintage typography and the silver-and-orange arrangement | §2 and §5.3 — trade dress |

What is **kept** is architectural: one oscillator with a switch, a diode ladder, one filter envelope,
an accent with three destinations, and a slide.

## 10. Minimum size and 200% scale

**Resizable: the editor's `REFERENCE` and `MINIMUM`, derived and held by its tests.** Category/card
order is §6's. `every_dynamic_page_fits_and_every_card_is_reachable` checks all pages in both themes
at opening size, the quarter-4K content size and the minimum, at 1×/2× with that simulated physical
budget fixed. Floors are computed with every route revealed.
Advanced wraps knob rows at their existing column floor inside its indivisible card. Component
floor/row checks remain separate. Independent zoom is **75–200%**; only indivisible overflow
scrolls. Keep the physical window fixed for §15's DPI/zoom gate. Native-window, real-DAW and owner
inspection remain open.

**Two primary cards, plus Advanced.** Oscillator, accent and amplifier are three small groups, and a card each left the
panel tall and mostly gutter; in one card, Tune and Accent on a single row in that order with the
amplifier's routes beneath them, they read as the signal reaching the amplifier. The filter is where the playing happens, so it keeps
a card of its own. *Voice* is the collection's own word for this grouping — `MXM_CONTROL_MAP.md` has
had a **Voice** page since before either editor existed.

**And the envelope is in the Filter card, not its own.** *Envelope* was separate in the first build and it was both emptier and less
true: this instrument has exactly one envelope and it is the filter's, which the hardware says by
putting ENV MOD and DECAY in the same knob row as CUT OFF FREQ and RESONANCE. A card called
*Envelope* invites the question *which one*, and there is no other. Decay's tooltip carries what
the separate card was there to say — a caption on the card did, until the owner ruled out help
text on the panel (2026-09-27).

**That order is the hardware's own, and it is also the signal flow**, which is why keeping it is
allowed where copying the panel is not. The machine's knob row reads TUNING · CUT OFF FREQ ·
RESONANCE · ENV MOD · DECAY · ACCENT, with the waveform switch at the left edge and VOLUME on its
own. Split into cards that is exactly the sequence above, and VOLUME is on its own here too, in the
app bar. `mxm-mono-01`'s brief made the same
distinction in the same words — *kept: section sequence and membership, because they are the signal
flow; removed: appearance, geometry, control style, colour arrangement, typography, trade dress.*

---

## 10a. Coherence with `mxm-mono-01`

The collection's consistency is supposed to come from the system rather than from instruments
imitating each other, but two editors built months apart drift unless the borrowing is deliberate.
What this editor takes from the first one, and why:

| Taken | Why |
|---|---|
| **`SECTIONS` as a `const` array, with the order stated as the contract** | It is the information architecture. `mxm-mono-01`'s editor says *"changing it changes what an SH-101 user finds where"*, and the same holds here |
| **`mxm_ui::ModuleCard` per section** | §3.3's grouping, already implemented, already themed |
| **Card names from a shared vocabulary** — Oscillator, Filter, Envelope, Amplifier | Someone who has used one instrument in the collection should not have to learn new words for the same parts. **Accent** is the only new name, and it is new because the thing is |
| **Slide sits in Oscillator** | `mxm-mono-01` puts `glide` there, on the grounds that portamento is played rather than configured. The same argument, the same card |
| **One place brackets gestures** — `binding::Bound::apply` | `begin_set_parameter` / `set` / `end`, once. Load-bearing for the player's step editing, and getting it wrong reads as the knob fighting you |
| **The three-box knob column** — fixed name, knob, value | Already solved in `crates/ui`, including the two defects that a value's length or a name's wrapping must not move the layout |
| **A `Parameters` view, and Init in the utility menu** | Same pair of views, same placement |
| **`Telemetry` as the only DSP → editor channel**, atomics only, once per block | Including its peak-reset-on-read and latching-clip rules |
| **The zoom control, 75–200%** | One size that is always right, absorbed by zoom rather than by reflow |

**Where it deliberately differs**, and both are consequences of the instrument rather than taste:

- **Two zones and a labelled divider, instead of a disclosure expander.** §5 above.
- **A vertical arrangement rather than `mxm-mono-01`'s three balanced columns.** That editor packs
  six roughly equal cards; this one has five small cards and a second zone, and forcing them into
  three equal columns would leave the column-height matching solving a problem this panel does not
  have.

---

## 11. The recognisability trial

§9 makes a recognisable *control set* the requirement — not a recognisable panel, which §2 forbids.
So the trial asks about the controls and the model, not about where things sit.

**The trial patch:** cutoff 35%, resonance 85%, the Env Mod route at 70%, decay 35%, accent 80%,
sawtooth, a 16-step pattern with four consecutive accents and two slides.

Run with **someone who has used a 303 or a clone**, without showing them the hardware.

### Stage 1 — before composition, on a wireframe

1. *"Trace the sound from where it starts to where it leaves."* → oscillator, filter, amplifier.
2. *"What does accent do?"* → **more than one thing**, and specifically that it reaches the filter.
   A single answer of "it makes it louder" is a fail, and it is a fail of the drawing, not of the
   person.
3. *"Which of these did the original let you change?"* → everything above the divider. If the
   divider does not answer this, it is not doing its job.

### Stage 2 — on the finished editor

Six location tasks, each with exactly one correct control:

| Task | Control |
|---|---|
| *Make it squelch.* | `resonance` |
| *Open it up.* | `cutoff` |
| *Make the accented notes hit harder.* | `accent` |
| *Make each note shorter and snappier.* | `decay` |
| *Make this note slide into the next one.* | `slide` |
| *Make it a bit quieter overall.* | `volume` |

**Per task:** found within ten seconds, entering at most one wrong zone. **Gate: five of six.**

*Results: not yet run.* An unrun trial is recorded as unmet, never as passed.

---

## Deliberate deviations from the design system

### Advanced controls are shown rather than disclosed (§3.3, §14.5)

§3.3 offers a card footer as the place for "optional routing or advanced disclosure". This editor
puts advanced controls in a **visible second zone** instead.

**Decision:** the instrument is small enough that hiding a third of it costs more than it saves, and
a visible zone is strictly more reachable than an expander — which §4.3's priority order prefers.
The divider is labelled, so the boundary is explained rather than merely drawn.

Revisit if a future instrument in the collection has enough advanced surface that a zone becomes a
wall. This is not a precedent for the collection; it is a judgement about eighteen parameters of its
own.

### No undo/redo, and no preset browser (§3.1, §15)

Both for `mxm-mono-01`'s reasons, which apply unchanged: the only undoable events are parameter
edits, which hosts already track; and v1 ships no preset system. **Init** exists and lives in the
global utility menu.

---

## Sign-off checklist

- [ ] Signal flow readable without documentation, and **accent's multiple destinations visible**
- [ ] Cutoff and Resonance at Primary sizing, adjacent
- [ ] The divider is labelled and the two zones are tonally distinct in **both** themes
- [ ] Identity accent applied, with the measured ratios above holding against `surface-2` as well
- [ ] Every parameter present — 9 above the divider (8 on the two cards, Volume in the app bar), 9
      below (bend range included), and 54 routing parameters in three stacks
- [ ] Card names match the collection's vocabulary; only **Accent** is new
- [ ] No row of knobs spreads to fill its card: a row is capped at what its knobs need
- [ ] Both views reachable, `Synth` active on open
- [ ] Height measured and pinned by a test rather than assumed
- [ ] Verified at 75%, 100%, 150% and 200%
- [ ] Dark and light both complete, with all control states
- [ ] §11's trial run and recorded — pass or unmet, never skipped
- [ ] §15 QA gate passed in full
