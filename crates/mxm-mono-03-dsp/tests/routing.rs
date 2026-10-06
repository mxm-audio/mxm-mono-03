//! The routing conversion's obligations, each a test that fails without the thing it names.
//!
//! `plans/plan-mxm-mono-03-modulation.md` §10, which is `docs/code-review-notes.md` §7 made specific
//! to this machine. Every assertion was run against the defect it names; the plan's revision table
//! records each mutation and what went red.

// `let mut p = Params::default(); p.x = …` reads as the patch it is, as in the voice's own tests.
#![allow(clippy::field_reassign_with_default)]

use mxm_mono_03_dsp::filter::{self, DiodeConfig, DiodeLadder};
use mxm_mono_03_dsp::routing::{
    ACCENT_UNIT, BOUND, ENV_MOD_FLOOR_OCTAVES, FULL_SCALE, Graph, Routing, SOURCES, TARGET_NAMES,
    TARGETS, cutoff_octaves, source, target,
};
use mxm_mono_03_dsp::voice::{
    ACCENT_OCTAVES, ENV_MOD_FLOOR, ENV_MOD_OCTAVES, MAKEUP, Params, Voice,
};

const FS: f32 = 48_000.0;

/// A routing with exactly these pairs present, at these amounts.
fn wired(pairs: &[(usize, usize, f32)]) -> Routing {
    let mut r = Routing::new();
    for &(t, s, a) in pairs {
        r.present[t][s] = true;
        r.amounts[t][s] = a;
    }
    r.compact();
    r
}

/// Every pair present, at `amount`.
fn everything(amount: f32) -> Routing {
    let mut r = Routing::new();
    for t in 0..TARGETS {
        for s in 0..SOURCES {
            r.present[t][s] = true;
            r.amounts[t][s] = amount;
        }
    }
    r.compact();
    r
}

/// A graph armed with `routing`, with this sample's sources published.
fn graph_with(routing: &Routing, sources: &[(usize, f32)]) -> Graph {
    let mut g = Graph::new();
    g.set_topology(routing);
    g.begin_sample();
    for &(s, v) in sources {
        g.write(s, v);
    }
    g
}

fn render(v: &mut Voice, p: &Params, r: &Routing, samples: usize) -> Vec<f32> {
    v.set_topology(r);
    (0..samples).map(|_| v.process(p, r, FS)).collect()
}

/// **The Env Mod floor survives removal, zero and every positive amount** (plan §3, §10). The
/// circuit adds its floor whatever is routed, and the route at amount `n` reaches what the retired
/// knob at normalised `n` did — `5·(F + (1 − F)·n)` octaves per unit of envelope — to rounding.
#[test]
fn the_floor_survives_removal_zero_and_every_positive_amount() {
    for fenv in [0.0f32, 0.25, 0.7, 1.0] {
        assert_eq!(
            cutoff_octaves(fenv, 0.0),
            ENV_MOD_FLOOR_OCTAVES * fenv,
            "nothing routed is the floor"
        );
        for n in [0.0f32, 0.1, 0.33, 0.5, 0.9, 1.0] {
            let r = wired(&[(target::CUTOFF, source::FILTER_ENVELOPE, n)]);
            let g = graph_with(&r, &[(source::FILTER_ENVELOPE, fenv)]);
            let routed = cutoff_octaves(fenv, g.sum(target::CUTOFF, &r));
            let old = ENV_MOD_OCTAVES * (ENV_MOD_FLOOR + (1.0 - ENV_MOD_FLOOR) * n) * fenv;
            assert!(
                (routed - old).abs() < 1e-5,
                "fenv {fenv}, Env Mod {n}: {routed} against {old}"
            );
        }
    }

    // Through a real voice: removing every route leaves the floor, which is what the route at zero
    // renders too. Accent is at zero, so its two routes carry nothing either way.
    let p = Params::default();
    let note = |routing: &Routing| {
        let mut v = Voice::new();
        v.note_on(36, 0.0, false, 0.8);
        render(&mut v, &p, routing, 4_800)
    };
    let at_zero = note(&Routing::init());
    assert!(
        at_zero == note(&Routing::new()),
        "removing the Env Mod route must leave the floor, not take it away"
    );
    assert!(
        at_zero != note(&wired(&[(target::CUTOFF, source::FILTER_ENVELOPE, 1.0)])),
        "the premise: the route reaches the filter"
    );
}

/// **The machine's own routes reach what the old expressions did.** At Env Mod's floor the cutoff is
/// the old expression to the bit, accent circuit and all; the amplifier's boost is bit-identical at
/// every setting; and Env Mod above its floor agrees to rounding, because splitting the floor off
/// re-associates one multiply. The Accent source's half unit and its doubled scale are both exact —
/// which is why the unit is a power of two (plan N9).
#[test]
fn the_machines_own_routes_reach_what_the_old_expressions_did() {
    for accent in [0.0f32, 0.3, 0.8, 1.0] {
        for offset in [0.0f32, 0.2, 0.77, 1.0, 1.6] {
            for boost in [0.0f32, 0.5, 1.0] {
                for n in [0.0f32, 0.4, 1.0] {
                    for fenv in [0.0f32, 0.6, 1.0] {
                        let mut r = Routing::init();
                        r.amounts[target::CUTOFF][source::FILTER_ENVELOPE] = n;
                        let g = graph_with(
                            &r,
                            &[
                                (source::FILTER_ENVELOPE, fenv),
                                (source::ACCENT, accent * offset * ACCENT_UNIT),
                                (source::ACCENT_LEVEL, accent * boost),
                            ],
                        );
                        assert_eq!(
                            (1.0 + g.sum(target::AMPLITUDE, &r)).max(0.0).to_bits(),
                            (1.0 + accent * boost).to_bits(),
                            "the amplifier's boost at accent {accent}, boost {boost}"
                        );
                        let env_mod = ENV_MOD_FLOOR + (1.0 - ENV_MOD_FLOOR) * n;
                        let old = 5.0 * env_mod * fenv + 4.0 * accent * offset;
                        let new = cutoff_octaves(fenv, g.sum(target::CUTOFF, &r));
                        if n == 0.0 {
                            assert_eq!(
                                new.to_bits(),
                                old.to_bits(),
                                "at the floor, accent {accent}, offset {offset}, fenv {fenv}: {new} against {old}"
                            );
                        } else {
                            assert!(
                                (new - old).abs() < 1e-5,
                                "Env Mod {n}, accent {accent}, offset {offset}, fenv {fenv}: {new} against {old}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// **An accented note at Accent depth zero still gets the short decay** (plan §1, §10). The forced
/// decay reads the per-note accent gate, not the Accent knob, and routing must not change that.
#[test]
fn an_accented_note_at_accent_depth_zero_still_gets_the_short_decay() {
    let mut p = Params::default();
    p.decay = 1.0;
    p.accent = 0.0;
    let r = Routing::init();
    let envelope_after = |accented: bool| {
        let mut v = Voice::new();
        v.note_on(48, if accented { 1.0 } else { 0.0 }, false, 0.8);
        render(&mut v, &p, &r, (FS * 0.4) as usize);
        v.published(source::FILTER_ENVELOPE)
    };
    let (plain, accented) = (envelope_after(false), envelope_after(true));
    assert!(
        accented < plain * 0.5,
        "the accent gate forces the short decay whatever the knob: accented {accented} against plain {plain}"
    );
}

/// **Consecutive accents climb through the routes** (plan §10): the accent circuit's filter output,
/// as the Accent source publishes it on a real voice at full resonance, peaks higher on each of four
/// accented notes a sixteenth apart — which only happens if the source reads the resonance control.
#[test]
fn consecutive_accents_climb_through_the_routes() {
    let mut p = Params::default();
    p.resonance = 1.0;
    p.accent = 1.0;
    let r = Routing::init();
    let step = (60.0 / 140.0 / 4.0 * FS) as usize;
    let mut v = Voice::new();
    v.set_topology(&r);
    let mut peaks = Vec::new();
    for _ in 0..4 {
        v.note_on(36, 1.0, false, 0.8);
        let mut peak = 0.0f32;
        for i in 0..step {
            if i == step / 2 {
                v.note_off();
            }
            v.process(&p, &r, FS);
            peak = peak.max(v.published(source::ACCENT) / ACCENT_UNIT);
        }
        peaks.push(peak);
    }
    for pair in peaks.windows(2) {
        assert!(
            pair[1] > pair[0] * 1.05,
            "each accent should peak above the last: {peaks:?}"
        );
    }
    assert!(
        peaks[3] > peaks[0] * 1.3,
        "four accents should climb clearly: {peaks:?}"
    );
}

/// **A source that becomes needed starts from silence, not from an old phrase**
/// (`docs/code-review-notes.md` §7). The oscillator is read, goes unread for a second while another
/// route keeps the frame running, and is read again before this sample publishes it.
#[test]
fn a_source_that_becomes_needed_starts_from_silence_not_from_an_old_phrase() {
    let reads_oscillator = wired(&[
        (target::RESONANCE, source::OSCILLATOR, 1.0),
        (target::CUTOFF, source::FILTER_ENVELOPE, 0.0),
    ]);
    let without = wired(&[(target::CUTOFF, source::FILTER_ENVELOPE, 0.0)]);
    let mut g = Graph::new();
    g.set_topology(&reads_oscillator);
    g.begin_sample();
    g.write(source::OSCILLATOR, 0.7);
    g.set_topology(&without);
    for _ in 0..48_000 {
        g.begin_sample();
        g.write(source::FILTER_ENVELOPE, 0.3);
    }
    g.set_topology(&reads_oscillator);
    g.begin_sample();
    assert_eq!(
        g.read(source::OSCILLATOR),
        0.0,
        "a newly read source must not return the value from before it went unread"
    );
}

/// **The declared order, rendered: a route from the oscillator lands on the sample it was published
/// in, in every target.** The route is armed mid-note on one of two voices with identical histories,
/// which clears its slot — so a route that read last sample's value would read zero and change
/// nothing on that first sample.
#[test]
fn a_route_from_the_oscillator_lands_this_sample_in_every_target() {
    let mut p = Params::default();
    p.resonance = 0.5;
    let unrouted = Routing::new();
    for (t, name) in TARGET_NAMES.iter().enumerate() {
        let armed = wired(&[(t, source::OSCILLATOR, 1.0)]);
        let (mut a, mut b) = (Voice::new(), Voice::new());
        a.note_on(45, 0.0, false, 0.8);
        b.note_on(45, 0.0, false, 0.8);
        for _ in 0..2_000 {
            a.process(&p, &unrouted, FS);
            b.process(&p, &unrouted, FS);
        }
        a.set_topology(&armed);
        let ya = a.process(&p, &armed, FS);
        let yb = b.process(&p, &unrouted, FS);
        assert_ne!(
            a.published(source::OSCILLATOR),
            0.0,
            "the premise: the oscillator is not at a zero crossing"
        );
        assert_ne!(
            ya, yb,
            "{}: a route from the oscillator must land on the sample it was published in",
            name
        );
    }
}

/// **Every pair at extreme amounts is finite and within the stated bound**, at three sample rates,
/// with a slide across the middle. The bound is loose — the amplifier's sum may reach
/// `BOUND[AMPLITUDE]`, the owner's open decision D8 — but it is a bound.
#[test]
fn every_pair_at_extreme_amounts_stays_finite_and_bounded() {
    let bound = 2.0 * filter::OUTPUT_BOUND * MAKEUP * (1.0 + BOUND[target::AMPLITUDE]);
    for fs in [8_000.0f32, 44_100.0, 192_000.0] {
        for amount in [-1.0f32, 1.0] {
            let r = everything(amount);
            let mut p = Params::default();
            p.accent = 1.0;
            p.resonance = 1.0;
            p.wheel = 1.0;
            p.pressure = 1.0;
            p.bend = 1.0;
            p.cutoff = 1.0;
            p.volume = 1.0;
            let mut v = Voice::new();
            v.set_topology(&r);
            v.note_on(24, 1.0, false, 1.0);
            for i in 0..(fs * 0.5) as usize {
                if i == (fs * 0.25) as usize {
                    v.note_on(96, 1.0, true, 1.0);
                }
                let y = v.process(&p, &r, fs);
                assert!(
                    y.is_finite() && y.abs() <= bound,
                    "fs {fs}, amount {amount}, sample {i}: {y}"
                );
            }
        }
    }
}

/// **The ladder's feedback loop under a coefficient driven at audio rate stays near its own level**
/// — `mxm-mono-00`'s phaser lesson (`docs/code-review-notes.md` §7), where coherent coefficient
/// speed, not size, pumped a loop to infinity from silence. A summing Cutoff or Resonance input can
/// now toggle the ladder's coefficients between their ends every sample, which one knob never could.
///
/// So the cutoff alternates between its clamp ends and the resonance between none and full, at every
/// alternation period from two samples, **with no input at all**, at the rates a validator tries.
/// What it must not do is run away, or pump itself far past the self-oscillation the same filter
/// reaches with its coefficients still.
#[test]
fn the_ladder_driven_at_audio_rate_from_silence_does_not_pump_itself() {
    for fs in [
        1_000.0f32, 8_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 768_000.0,
    ] {
        let top = filter::max_nominal_cutoff_hz(DiodeConfig::Tb303, fs);
        // The loudest the filter gets on its own: resonance at full, coefficients still, from the
        // same excitation.
        let still = {
            let mut f = DiodeLadder::new(DiodeConfig::Tb303);
            (0..40_000)
                .map(|_| f.process(0.0, top * 0.25, 1.0, fs).abs())
                .fold(0.0f32, f32::max)
        };
        for period in [2usize, 4, 8, 16, 64, 256, 2_048] {
            let mut f = DiodeLadder::new(DiodeConfig::Tb303);
            let mut worst = 0.0f32;
            for i in 0..40_000 {
                let high = (i / (period / 2)) % 2 == 0;
                let (cutoff, resonance) = if high {
                    (top, 1.0)
                } else {
                    (filter::CUTOFF_MIN_HZ, 0.0)
                };
                let y = f.process(0.0, cutoff, resonance, fs);
                assert!(y.is_finite(), "fs {fs}, period {period}: non-finite at {i}");
                worst = worst.max(y.abs());
            }
            assert!(
                worst <= 2.0 * still + 1e-3,
                "fs {fs}, period {period}: {worst} against {still} with the coefficients still"
            );
        }
    }
}

/// **Audio-rate routes into the cutoff and the resonance at full, through the voice, stay bounded and
/// do not grow** (plan §10), at three rates and both signs, with the amplitude envelope held long so
/// the late window is not simply quieter.
#[test]
fn audio_rate_routes_into_cutoff_and_resonance_stay_bounded_through_the_voice() {
    let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
    for fs in [8_000.0f32, 44_100.0, 192_000.0] {
        for sign in [-1.0f32, 1.0] {
            let r = wired(&[
                (target::CUTOFF, source::OSCILLATOR, sign),
                (target::RESONANCE, source::OSCILLATOR, -sign),
            ]);
            let mut p = Params::default();
            p.resonance = 1.0;
            p.cutoff = 0.4;
            p.amp_decay_s = 10.0;
            let mut v = Voice::new();
            v.set_topology(&r);
            v.note_on(40, 0.0, false, 0.8);
            let n = (fs * 2.0) as usize;
            let out: Vec<f32> = (0..n).map(|_| v.process(&p, &r, fs)).collect();
            assert!(
                out.iter().all(|y| y.is_finite()),
                "fs {fs}, sign {sign}: non-finite"
            );
            let early = rms(&out[n / 4..n / 2]);
            let late = rms(&out[3 * n / 4..]);
            assert!(
                late <= early * 1.5 + 1e-3,
                "fs {fs}, sign {sign}: late {late} against early {early}"
            );
        }
    }
}

/// **No key down is exact silence whatever is routed** (plan §10): every source into every target at
/// full, the wheel, pressure and bender parked up, the accent knob and the resonance at their tops.
#[test]
fn every_source_into_every_target_with_no_key_down_is_exact_silence() {
    let r = everything(1.0);
    let mut p = Params::default();
    p.accent = 1.0;
    p.wheel = 1.0;
    p.pressure = 1.0;
    p.bend = 1.0;
    p.resonance = 1.0;
    let mut v = Voice::new();
    let out = render(&mut v, &p, &r, 48_000);
    assert!(
        out.iter().all(|&y| y == 0.0),
        "no key down must render exact zeros whatever is routed"
    );
    assert!(!v.is_active());
}

/// **`reset` clears the routing frame**, so nothing a route read before a transport stop survives it.
#[test]
fn reset_leaves_no_routed_tail() {
    let r = wired(&[(target::RESONANCE, source::OSCILLATOR, 1.0)]);
    let p = Params::default();
    let mut v = Voice::new();
    v.note_on(45, 0.0, false, 0.8);
    render(&mut v, &p, &r, 2_000);
    assert_ne!(v.published(source::OSCILLATOR), 0.0, "the premise");
    v.reset();
    assert_eq!(
        v.published(source::OSCILLATOR),
        0.0,
        "reset must clear the routing frame"
    );
}

/// **Velocity follows the note, and a slid-into note keeps it**, as it keeps its envelopes and takes
/// no accent.
#[test]
fn velocity_follows_the_note_and_a_slid_note_keeps_it() {
    let r = wired(&[(target::CUTOFF, source::VELOCITY, 1.0)]);
    let p = Params::default();
    let mut v = Voice::new();
    // Published as the standard's `v − 1`.
    v.note_on(36, 0.0, false, 0.25);
    render(&mut v, &p, &r, 16);
    assert_eq!(v.published(source::VELOCITY), -0.75);
    v.note_on(48, 0.0, true, 0.875);
    render(&mut v, &p, &r, 16);
    assert_eq!(
        v.published(source::VELOCITY),
        -0.75,
        "a slid-into note keeps the velocity it slid from"
    );
    v.note_off();
    v.note_on(40, 0.0, false, 0.5);
    render(&mut v, &p, &r, 16);
    assert_eq!(
        v.published(source::VELOCITY),
        -0.5,
        "a new note brings its own"
    );
}

/// **The Key source follows the slide rather than jumping** (plan N2), so a slide moves a key-tracked
/// target with the pitch.
#[test]
fn the_key_source_follows_the_slide_rather_than_jumping() {
    let r = wired(&[(target::CUTOFF, source::KEY, 1.0)]);
    let p = Params::default();
    let mut v = Voice::new();
    v.note_on(36, 0.0, false, 0.8);
    render(&mut v, &p, &r, 4_800);
    v.note_on(60, 0.0, true, 0.8);
    render(&mut v, &p, &r, 48);
    let key = v.published(source::KEY);
    assert!(
        (-0.4..-0.35).contains(&key),
        "a millisecond into a 60 ms slide from 36 to 60 the key is still near 36: {key}"
    );
}

/// **An Amplitude sum below zero closes the amplifier; it never inverts it** (plan N7). Velocity and
/// the wheel at full, each routed at −100 %, sum to −2, so the factor is `max(0, 1 − 2)` and the note
/// is exact silence. Without the clamp it is the same note at full level, phase-inverted.
#[test]
fn an_amplitude_sum_below_zero_closes_the_amplifier_rather_than_inverting_it() {
    let mut p = Params::default();
    p.wheel = 1.0;
    let samples = (FS * 0.2) as usize;
    let mut open = Voice::new();
    open.note_on(45, 0.0, false, 1.0);
    let sounding = render(&mut open, &p, &Routing::new(), samples);
    let peak = sounding.iter().fold(0.0f32, |m, y| m.max(y.abs()));
    assert!(peak > 1e-3, "the premise: the note sounds, peak {peak}");

    let r = wired(&[
        (target::AMPLITUDE, source::VELOCITY, -1.0),
        (target::AMPLITUDE, source::WHEEL, -1.0),
    ]);
    let mut v = Voice::new();
    v.note_on(45, 0.0, false, 1.0);
    let closed = render(&mut v, &p, &r, samples);
    let leak = closed.iter().fold(0.0f32, |m, y| m.max(y.abs()));
    assert_eq!(
        leak, 0.0,
        "a negative sum must close the amplifier, not invert it"
    );
}

/// **The accent sources are the circuit's two outputs at the Accent knob's depth**, so the machine's
/// routes at full reach what the retired expressions did from the voice's own samples, not only from
/// a source value a test wrote (plan D1 a, N9). At resonance zero the filter output is the accent gate
/// times the filter envelope, which the voice also publishes, so `4 · accent · offset` octaves can be
/// computed sample by sample; the amplifier output follows the gate up from zero and never passes it.
#[test]
fn the_accent_sources_are_the_circuits_outputs_at_the_knobs_depth() {
    let mut p = Params::default();
    p.accent = 0.7;
    p.resonance = 0.0;
    let r = Routing::init();
    for gate in [1.0f32, 0.6, 0.0] {
        let mut v = Voice::new();
        v.note_on(45, gate, false, 1.0);
        v.set_topology(&r);
        let mut widest = 0.0f32;
        for _ in 0..(FS * 0.3) as usize {
            v.process(&p, &r, FS);
            let fenv = v.published(source::FILTER_ENVELOPE);
            let reached = v.published(source::ACCENT) * FULL_SCALE[target::CUTOFF][source::ACCENT];
            let old = ACCENT_OCTAVES * p.accent * gate * fenv;
            assert!(
                (reached - old).abs() <= 1e-6,
                "gate {gate}: the route reaches {reached} oct where the circuit gave {old}"
            );
            let level = v.published(source::ACCENT_LEVEL);
            assert!(
                (0.0..=p.accent * gate * fenv + 1e-6).contains(&level),
                "gate {gate}: accent level {level} beyond the knob's depth of the gated envelope"
            );
            widest = widest.max(fenv);
        }
        assert!(
            widest > 0.5,
            "the premise: the filter envelope opened, peak {widest}"
        );
    }
}
