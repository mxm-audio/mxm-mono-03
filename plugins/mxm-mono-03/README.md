# mxm-mono-03

A monophonic acid bass voice. Architecture inspired by the Roland TB-303; the interface is not, and
the name is not. Not affiliated with or endorsed by Roland.

**The synth only.** The sequencer half of that machine is [MXM Player](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player)'s.

## What it is

| | |
|---|---|
| Oscillator | Sawtooth, or a square **waveshaped out of it** — so the square's duty cycle moves with pitch, roughly 71% at the bottom of the range and 45% at the top |
| Filter | A **diode ladder**, modelled as its pole set. Four poles spread from 0.121x to 3.532x the nominal cutoff, which is where its sagging corner and its high resonance threshold both come from |
| Envelopes | Two, both attack-decay with **no sustain**. Only the filter envelope's decay is a control; the amplitude envelope has none at all |
| Accent | Louder, a forced-short filter envelope, **and a sweep that holds charge between notes** — so consecutive accents climb |
| Glide | In the voice, not the sequencer. Engages across a legato transition and nowhere else |

## The controls

**Tune · Waveform · Cutoff · Resonance · Decay · Accent · Volume** — the hardware's, and nothing
else on the panel. Being unable to adjust the rest is a large part of what the instrument is, so the
machine's fixed constants stay fixed here and move to Advanced below.

Plus the hardware's two **per-step buttons**, which have to be parameters here because this is a
synth with no sequencer and that is the only route a host has to a per-step decision:

- **Accent note** — is this note accented?
- **Slide** — does this note glide in from the one before?

Both are on/off — the machine's slide time is a fixed RC, so the button is the whole control. The
lag lives in the voice rather than the sequencer, so a MIDI keyboard gets it too.

**Its modulation is routing.** Env Mod is the *Cutoff from Filter envelope* route, and the accent
circuit reaches the filter and the amplifier through two routes of its own — all three present in a
fresh instance, which sounds as the machine does. Removing the filter envelope's route still leaves
its floor on the cutoff, as on the hardware. The key, velocity, mod wheel, pressure, bender, the
accent circuit and the oscillator can each reach the cutoff, the resonance or the amplifier; nothing
the machine lacked is wired until you add it.

**Cutoff shows a position, not a frequency**, exactly as the hardware's does. Calibrating it to a
−3 dB point would need about eight times the headroom, and the ladder's widest pole then caps the
whole control in the high hundreds of hertz. See
`research:filters/machines/tb303-diode-ladder.md` §10.5.

### Advanced

In the labelled Advanced zone below the instrument, the constants the machine fixed:

| | |
|---|---|
| **Slide time** | the RC that sets how long a slide takes |
| **Amp attack** · **Filter attack** | the de-clickers, made into shapes |
| **Amp decay** | why every note is flat-topped |
| **Accent decay** | the forced-short envelope an accent imposes |
| **Accent sweep** | how fast the accent climb builds |
| **Drive** | how hard the oscillator hits the ladder |
| **Filter model** | the one-diode ladder, or the three-diode variant |

**Every one defaults to the hardware's own value**, so opening the panel changes nothing until you
move something. Bend range is disclosed here too.

## Slides

A full hardware slide is two things at once, and they arrive by different routes: the **Slide
switch** engages the pitch lag, and a **tie** in the sequencer holds the gate open so the note does
not re-attack. The voice reads them separately, which is why the switch on its own gives a bend into
a new note rather than silence.

Authoring one in MXM Player is not possible yet; the player plays them correctly but cannot write
them down. See `plans/plan-slide-steps.md` (`plans/plan-slide-steps.md` in the private archive).

## Presets

Fifty factory sounds, compiled into the plugin — copy the `.clap` alone and the sounds travel with
it. **Init is not a file**: it is generated from the parameter defaults, so it cannot be deleted and
cannot drift from them. Your own presets are saved as readable JSON under the platform config
directory, browsed from the same bar, and can be starred to sort first.

## Status

**The editor shipped.** Two cards (Voice and Filter) above a labelled Advanced zone, the filter
response curve and the accent-sweep display, and in the app bar a preset browser, Volume beside the
output meter, and a scale control (75–200%).

**Fidelity is UNVERIFIED.** No hardware was measured, here or in any source this instrument rests
on. The tests prove the model is self-consistent — not that it sounds like the machine. The listening
comparison against reference recordings has not been run.

## Building

```bash
cargo xtask bundle mxm-mono-03 --release
clap-validator validate "target/bundled/mxm-mono-03.clap"
```

GPL-3.0-or-later — see the repository's [`LICENSE`](../../LICENSE). All code is original.
