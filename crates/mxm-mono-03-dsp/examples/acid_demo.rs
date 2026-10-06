//! An acid line, in the classic idiom, with the filter played.
//!
//! ```bash
//! cargo run -p mxm-mono-03-dsp --release --example acid_demo
//! ```
//!
//! # Why this exists when `mono_03_render_demo` already renders notes
//!
//! `mono_03_render_demo` checks the mechanisms one at a time. This one checks the thing they add
//! up to,
//! and the difference is **the knobs move**. Acid is a performance on the filter: a static patch
//! playing the right notes does not sound like the genre, however correct the voice is. So the
//! cutoff sweeps, the resonance rises, and the envelope depth opens across the render — which is
//! also what makes the accent circuit's behaviour audible, since resonance is what turns accent
//! from extra envelope into the climbing sweep.
//!
//! # The pattern
//!
//! Written in the idiom rather than transcribed from a record: a one-bar sixteenth-note figure on a
//! minor pentatonic root, with octave jumps, slides into the off-beats, and accents clustered so
//! consecutive ones can build. That shape — one bar, sixteen steps, a handful of accents and
//! slides, repeated while the filter is worked — *is* the form, and it is what the machine was
//! built to play.
//!
//! **This is a listening check, not a measurement.** Nothing here was compared against hardware.

/// Writes a listening demo, applying this collection's demo headroom law **at the call site**.
///
/// `mxm_audio_file` encodes what it is given and applies no gain — normalisation is a judgement
/// about the material and the file crate carries no policy. The law here is the one the
/// six hand-written writers all applied internally: leave 2 % of headroom, and scale down further if
/// the material is over full scale.
fn write_demo(path: &str, interleaved: &[f32], channels: u16, rate: u32) {
    let peak = mxm_measure::level::peak(interleaved)
        .expect("a rendered demo is finite; a NaN here is a DSP defect, not a level");
    let gain = if peak > 1.0 { 0.98 / peak } else { 0.98 };
    let scaled: Vec<f32> = interleaved.iter().map(|s| s * gain).collect();
    mxm_audio_file::write(
        path,
        &scaled,
        channels,
        rate,
        mxm_audio_file::Target::Wav(mxm_audio_file::Bits::Sixteen),
    )
    .expect("the demo is written");
}

/// **Where this demo's channel count and sample rate are decided — once, for `main` and for the
/// test below.** Both call this, so a change to either constant changes both paths and the test's
/// literal expectations catch it. With the two supplied separately at each site, a `main` passing
/// the wrong channel count left the test perfectly green.
const DEMO_CHANNELS: u16 = 1;

fn write_demo_file(path: &str, interleaved: &[f32]) {
    write_demo(path, interleaved, DEMO_CHANNELS, FS as u32);
}

use mxm_mono_03_dsp::oscillator::Waveform;
use mxm_mono_03_dsp::routing::{Routing, source, target};
use mxm_mono_03_dsp::voice::{ENV_MOD_FLOOR, Params, Voice};

const FS: f32 = 48_000.0;
const BPM: f32 = 125.0;
const BARS: usize = 16;

/// One sixteenth.
#[derive(Clone, Copy)]
struct Step {
    /// MIDI note, or `None` for a rest.
    note: Option<u8>,
    accent: bool,
    /// Slides **into** this step: the gate stays open across the boundary and the pitch moves.
    slide: bool,
}

const fn n(note: u8) -> Step {
    Step {
        note: Some(note),
        accent: false,
        slide: false,
    }
}
const fn a(note: u8) -> Step {
    Step {
        note: Some(note),
        accent: true,
        slide: false,
    }
}
const fn sl(note: u8) -> Step {
    Step {
        note: Some(note),
        accent: false,
        slide: true,
    }
}
const fn asl(note: u8) -> Step {
    Step {
        note: Some(note),
        accent: true,
        slide: true,
    }
}
const fn r() -> Step {
    Step {
        note: None,
        accent: false,
        slide: false,
    }
}

/// A minor pentatonic figure on A. The octave jump on the "and" of 2 and the slid answer on 4 are
/// the two gestures that make a line read as acid rather than as a bass part.
const PATTERN: [Step; 16] = [
    a(33), // A1, accented downbeat
    n(33),
    r(),
    n(45),  // octave up
    a(33),  // accent cluster begins
    a(36),  // C2
    sl(40), // slide up to E2 - no retrigger, the gate is still open
    n(33),
    r(),
    a(33),
    n(43),  // G2
    sl(45), // slide to A2
    a(36),
    n(33),
    asl(31), // accented *and* slid: the accent is ignored, because a slide is not a new note
    r(),
];

fn main() {
    let step_samples = (60.0 / BPM / 4.0 * FS) as usize;
    let total_steps = BARS * PATTERN.len();

    let mut voice = Voice::new();
    let mut routing = Routing::init();
    voice.set_topology(&routing);
    let mut out: Vec<f32> = Vec::with_capacity(total_steps * step_samples);

    for step_index in 0..total_steps {
        let step = PATTERN[step_index % PATTERN.len()];

        // **The performance.** One slow pass of the cutoff over the whole render, with resonance
        // and envelope depth following it up. This is the part that is played rather than
        // programmed, and without it the notes below are just a bass line.
        let t = step_index as f32 / total_steps as f32;
        let sweep = 0.5 - 0.5 * (std::f32::consts::TAU * t).cos(); // 0 -> 1 -> 0
        let params = Params {
            waveform: Waveform::Sawtooth,
            cutoff: 0.18 + 0.42 * sweep,
            resonance: 0.55 + 0.40 * sweep,
            decay: 0.45 - 0.25 * sweep, // shorter as it opens, so it spits rather than smears
            accent: 0.85,
            ..Params::default()
        };
        // Env Mod is a route now: the knob's position above the circuit's floor is the
        // (Cutoff ← Filter envelope) route's amount, following the sweep up with the cutoff.
        let env_mod = 0.35 + 0.50 * sweep;
        routing.amounts[target::CUTOFF][source::FILTER_ENVELOPE] =
            (env_mod - ENV_MOD_FLOOR) / (1.0 - ENV_MOD_FLOOR);

        // **A slide is a property of the transition, so it takes two steps to make one.** The step
        // being slid *into* is not the only one that changes: its predecessor must hold its gate
        // to the boundary, or there is nothing left open to slide from.
        //
        // This is the player's own rule — `Pattern::held_past_gate` is `tied(n) || tied(n + 1)`,
        // both clauses needed for exactly this reason. Getting it wrong here rendered every slid
        // step **silent**, because the predecessor had already been released.
        let next = PATTERN[(step_index + 1) % PATTERN.len()];
        let gate = if step.slide || next.slide {
            step_samples
        } else {
            step_samples / 2
        };

        // **A step that does not slide closes the previous gate first**, which is what the
        // player's runtime does at a step start: release, then sound. Without it a slid step's
        // held gate runs on into the *next* step, so that note is read as a legato joint too and
        // silently loses its attack and its accent. Measured: the accent on step 13 fell from
        // 0.95 to 0.46 because step 12 slid.
        if !step.slide {
            voice.note_off();
        }
        match step.note {
            Some(note) => {
                let accent = if step.accent { 1.0 } else { 0.0 };
                voice.note_on(note, accent, step.slide, 0.8);
            }
            None => voice.note_off(),
        }

        for i in 0..step_samples {
            if i == gate && step.note.is_some() {
                voice.note_off();
            }
            out.push(voice.process(&params, &routing, FS));
        }
    }

    // Let the tail finish rather than cutting it off.
    let tail_params = Params::default();
    while voice.is_active() && out.len() < (FS * 90.0) as usize {
        out.push(voice.process(&tail_params, &routing, FS));
    }

    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "{BARS} bars at {BPM} BPM - {:.1} s, peak {peak:.3}",
        out.len() as f32 / FS
    );
    if peak > 1.0 {
        println!("  peak is over full scale, so the file is normalised down");
    }

    let path = "mxm-mono-03-acid.wav";
    write_demo_file(path, &out);
    println!("wrote {path}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's own write path, exercised through the same wrapper `main` uses.
    ///
    /// The shared encoder is proved in `mxm-measure` against fixed header and payload bytes. What
    /// that cannot see is *this* file later writing the wrong channel count or rate, so the
    /// expectations here are **literals** — the facts about this instrument — rather than the
    /// constants under test.
    #[test]
    fn the_demo_write_path_produces_a_playable_file() {
        let frames = 256;
        let samples: Vec<f32> = (0..frames * DEMO_CHANNELS as usize)
            .map(|i| {
                let t = i as f32 / 48_000 as f32;
                // Past full scale, so the headroom branch is taken rather than skipped.
                1.6 * (std::f32::consts::TAU * 220.0 * t).sin()
            })
            .collect();

        let mut path = std::env::temp_dir();
        path.push(format!("acid-demo-demo-{}.wav", std::process::id()));
        write_demo_file(path.to_str().expect("a utf-8 path"), &samples);

        let read = mxm_audio_file_decode::decode_file(
            &path,
            &mxm_audio_file_decode::Limits::new(
                usize::MAX,
                mxm_audio_file_decode::AtLimit::Refuse,
                mxm_audio_file_decode::Keep::AllUpTo(2),
            ),
        )
        .expect("the demo file parses");
        assert_eq!(read.channels, 1, "the demo wrote the wrong channel count");
        assert_eq!(
            read.sample_rate, 48_000,
            "the demo wrote the wrong sample rate"
        );
        assert_eq!(read.frames(), frames, "the demo dropped or invented frames");

        // The headroom law, asserted rather than assumed: a source at 1.6 comes back just under
        // full scale, not clipped to it and not left loud.
        let peak = mxm_measure::level::peak(&read.interleaved).expect("a finite file");
        assert!(
            (0.97..=0.985).contains(&peak),
            "the 0.98 headroom law did not run: peak {peak}"
        );
        std::fs::remove_file(&path).ok();
    }

    /// **The whole production path, `main` included.** This is what a writer test cannot otherwise
    /// reach: the render itself, the buffer `main` chooses, and the channel count and rate it hands
    /// over. An empty or truncated render fails here and nowhere else.
    ///
    /// `#[ignore]`d because it renders the demo in full, which is tens of seconds of audio; run it
    /// with `cargo test --all-targets -- --ignored` when the demo or its write path changes.
    #[test]
    #[ignore = "renders the whole demo; run with --ignored"]
    fn the_whole_demo_renders_and_writes_a_playable_file() {
        main();
        let read = mxm_audio_file_decode::decode_file(
            "mxm-mono-03-acid.wav",
            &mxm_audio_file_decode::Limits::new(
                usize::MAX,
                mxm_audio_file_decode::AtLimit::Refuse,
                mxm_audio_file_decode::Keep::AllUpTo(2),
            ),
        )
        .expect("the demo file parses");
        assert_eq!(read.channels, 1, "the demo wrote the wrong channel count");
        assert_eq!(
            read.sample_rate, 48000,
            "the demo wrote the wrong sample rate"
        );
        assert!(
            read.frames() > 48000,
            "the demo rendered under a second of audio"
        );
        let peak = mxm_measure::level::peak(&read.interleaved).expect("a finite render");
        assert!(peak > 0.1, "the demo rendered near-silence: peak {peak}");

        // `main` writes into the working directory, which under `cargo test` is the crate root.
        // Leaving it there drops an untracked WAV into the tree every time this runs.
        std::fs::remove_file("mxm-mono-03-acid.wav").ok();
    }
}
