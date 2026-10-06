//! Renders an acid line to a WAV, because *does it sound like the machine* has no
//! unit test.
//!
//! The pattern is chosen to exercise the three things the tests can only measure
//! one at a time: a run of consecutive accents (which must climb), slides into new
//! pitches (which must not retrigger), and the resonance interaction that turns
//! accent from extra envelope into a sweep.
//!
//! ```bash
//! cargo run -p mxm-mono-03-dsp --release --example mono_03_render_demo
//! ```
//!
//! **This is a listening check, not a measurement**, and it is the only kind of
//! evidence available here: no hardware was measured, and every published constant
//! this instrument rests on is secondary. Comparing this against a recording of the
//! real machine is the fidelity gate, and it has not been run.

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
const BPM: f32 = 130.0;

/// One step of the pattern.
struct Step {
    /// MIDI note, or `None` for a rest.
    note: Option<u8>,
    /// Whether this step is accented.
    accent: bool,
    /// Whether this step slides in from the one before — a tie, in the player's
    /// terms. A slid step does not retrigger and takes no accent.
    slide: bool,
}

const fn s(note: u8, accent: bool, slide: bool) -> Step {
    Step {
        note: Some(note),
        accent,
        slide,
    }
}

const fn rest() -> Step {
    Step {
        note: None,
        accent: false,
        slide: false,
    }
}

fn main() {
    // A minor pentatonic line. Steps 4-7 are four accents in a row, which is what
    // makes the climb audible; steps 9-10 and 13-14 are slid pairs.
    let pattern = [
        s(33, true, false),
        rest(),
        s(45, false, false),
        s(33, false, false),
        s(33, true, false),
        s(36, true, false),
        s(40, true, false),
        s(43, true, false),
        s(33, false, false),
        s(33, false, false),
        s(40, false, true), // slides up from 33
        rest(),
        s(36, true, false),
        s(36, false, false),
        s(31, false, true), // slides down
        rest(),
    ];

    let params = Params {
        cutoff: 0.35,
        resonance: 0.85,
        decay: 0.35,
        accent: 0.8,
        waveform: Waveform::Sawtooth,
        ..Params::default()
    };

    let step_samples = (60.0 / BPM / 4.0 * FS) as usize;
    let mut voice = Voice::new();
    // Env Mod at 0.7 is a route now: the knob's position above the circuit's floor is the
    // (Cutoff ← Filter envelope) route's amount.
    let mut routing = Routing::init();
    routing.amounts[target::CUTOFF][source::FILTER_ENVELOPE] =
        (0.7 - ENV_MOD_FLOOR) / (1.0 - ENV_MOD_FLOOR);
    voice.set_topology(&routing);
    let mut out: Vec<f32> = Vec::new();

    // Four times through, so the accent climb has room to be heard repeating.
    for _ in 0..4 {
        for (index, step) in pattern.iter().enumerate() {
            // A slide takes two steps: the one slid into, and its predecessor, which must hold its
            // gate to the boundary so there is something open to slide from. Same rule as the
            // player's `Pattern::held_past_gate`.
            let next = &pattern[(index + 1) % pattern.len()];
            let gate = if step.slide || next.slide {
                step_samples
            } else {
                step_samples / 2
            };
            // A step that does not slide closes the previous gate first, as the player's runtime
            // does at a step start. Otherwise a slid step's held gate runs into the next note and
            // that note loses its attack and its accent.
            if !step.slide {
                voice.note_off();
            }
            match step.note {
                Some(note) => {
                    voice.note_on(note, if step.accent { 1.0 } else { 0.0 }, step.slide, 0.8)
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
    }

    // Let the tail finish rather than cutting it.
    while voice.is_active() && out.len() < (FS * 60.0) as usize {
        out.push(voice.process(&params, &routing, FS));
    }

    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!("rendered {:.2} s, peak {peak:.3}", out.len() as f32 / FS);
    if peak > 1.0 {
        println!("  NOTE: peak exceeds full scale; the demo is normalised on write");
    }

    let path = "mxm-mono-03-demo.wav";
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
        path.push(format!("render-demo-demo-{}.wav", std::process::id()));
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
            "mxm-mono-03-demo.wav",
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
        std::fs::remove_file("mxm-mono-03-demo.wav").ok();
    }
}
