//! Golden audio for mxm-mono-03 through the real MXM Player host path.
//!
//! **What the score exercises**, and why each is in it: a resonant squelch with the filter envelope
//! doing the talking; two **accented** notes a step apart, so the forced short decay, the louder
//! amplifier and the sweep's climb are all in the digest; a **tie with Slide on**, an octave up,
//! which glides and retriggers nothing; and the release tail to silence. Accent and slide are the
//! machine's per-step buttons, so the score presses them itself.
//!
//! **Regenerating.** A deliberate DSP or shell change should fail this test. Then: rebuild the
//! bundle (`cargo xtask bundle mxm-mono-03 --release`), run this test, **listen to the WAV it
//! names**, review the DSP diff that moved it, and only then pin the new digest here with a line
//! saying why it moved. Never update the digest to make the test pass.
//!
//! **Pinned at M0 of `plans/plan-mxm-mono-03-modulation.md`** as `969baba189f92fe5`, against the
//! bundle built before the routing conversion — the change that most needs a host-path digest from
//! before it (plan N5) — and **provisionally repinned at its B4** as `555a7888da50f235`. It moved by
//! at most 6.0e-7 of full scale, 5.3e-7 of its peak, against the M0 render: the Env Mod route
//! re-associates one multiply (plan N6), which is rounding in size. No human listening is claimed:
//! the owner's pass at the plan's B5 confirms this pin or replaces it.
//!
//! **Repinned 2026-09-26 as `908a97387412e7c9`: the slide lands on its note.** The lag used to be
//! kept as the pitch, `target + (glide − target) × c`, which rounds every step's decrement to the
//! pitch's ulp: during the slide it wandered up to about a cent off its exponential, and at rest it
//! stalled a cent flat. It is kept as the remaining distance now (`crates/mxm-mono-03-dsp`). Measured
//! against the render before the change: identical up to the slide at 0.53 s, then at most 3.2e-3 of
//! full scale apart, −46 to −68 dB under the signal and fading through the release. Asked for by the
//! owner; not yet listened to.
//!
//! **The score's gestures are fixed; one parameter name was translated**, at that conversion: Env
//! Mod became the (Cutoff ← Filter envelope) route, and a depth `d` of the retired knob is the
//! fraction `(d + 1) / 2` of the route's signed range (plan §4).

use mxm_player::events::input::Payload;
use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use mxm_player_harness::app_harness;
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-mono-03";
const GOLDEN_DIGEST: &str = "908a97387412e7c9";

/// Whether this platform's render can match the pinned digests. They are Windows': each platform's
/// maths library rounds in its own way, so the same score renders different bits on Linux and macOS.
/// The owner pinned them on Windows only, where the sound was recorded and approved (2026-10-06);
/// elsewhere every other check in these tests still runs.
const DIGESTS_PINNED_HERE: bool = cfg!(target_os = "windows");
const GOLDEN_SAMPLES: usize = 101 * FRAMES_PER_BLOCK * 2;

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::any_bundled_dir()?;
    let file = dir.join("mxm-mono-03.clap");
    file.exists().then_some((dir, file))
}

/// Sets a parameter by display name, as a fraction of its plain range.
fn set(s: &mut Session, name: &str, f: f64) {
    let p = s
        .state()
        .param(name)
        .unwrap_or_else(|| panic!("`{name}` is not a parameter"))
        .clone();
    s.app().engine_mut().push_gui_event(Payload::ParamValue {
        param_id: p.id,
        value: p.min + f * (p.max - p.min),
    });
}

/// The patch the score plays. Fixed forever.
fn patch(s: &mut Session) {
    set(s, "Cutoff", 0.3);
    set(s, "Resonance", 0.75);
    // Env Mod at 0.6 of its range, which since the routing conversion is the (Cutoff ← Filter
    // envelope) route at amount 0.6 — the fraction (0.6 + 1) / 2 of its signed range.
    set(s, "Cutoff from Filter envelope", 0.8);
    set(s, "Decay", 0.35);
    set(s, "Accent", 0.8);
}

/// Fixed forever. `extra` runs after the patch, so a sensitivity check plays the same score.
fn score(s: &mut Session) -> Result<(), String> {
    score_with(s, |_| {})
}

fn score_with(s: &mut Session, extra: impl FnOnce(&mut Session)) -> Result<(), String> {
    patch(s);
    extra(s);
    s.advance_blocks(4)?;
    // A plain note.
    s.app().note_on(36, 0.8);
    s.advance_blocks(6)?;
    s.app().note_off(36);
    s.advance_blocks(6)?;
    // Two accented notes: the sweep holds its charge between them, so the second climbs.
    set(s, "Accent note", 1.0);
    s.advance_blocks(1)?;
    for _ in 0..2 {
        s.app().note_on(36, 0.8);
        s.advance_blocks(6)?;
        s.app().note_off(36);
        s.advance_blocks(6)?;
    }
    set(s, "Accent note", 0.0);
    s.advance_blocks(1)?;
    // A tie: the new note before the old one's release, with Slide on, so the pitch glides and
    // nothing retriggers.
    s.app().note_on(36, 0.8);
    s.advance_blocks(6)?;
    set(s, "Slide", 1.0);
    s.advance_blocks(1)?;
    s.app().note_on(48, 0.8);
    s.app().note_off(36);
    s.advance_blocks(12)?;
    s.app().note_off(48);
    s.advance_blocks(40)
}

fn render(name: &str) -> Option<(Vec<f32>, PathBuf)> {
    let (dir, file) = bundle()?;
    let mut s = Session::scratch(name, vec![dir]);
    s.load(&file, PLUGIN);
    score(&mut s).unwrap();
    let samples = s.captured();
    let (wav, _) = s.write_artifacts(name).unwrap();
    Some((samples, wav))
}

#[test]
fn the_fixed_real_host_score_has_not_moved() {
    let Some((samples, wav)) = render("golden-mono-03") else {
        eprintln!("skipping: run `cargo xtask bundle mxm-mono-03 --release`");
        return;
    };
    assert_eq!(samples.len(), GOLDEN_SAMPLES);
    assert!(
        samples.iter().any(|x| x.abs() > 1e-4),
        "the score is silent"
    );
    let actual = digest(&samples);
    if DIGESTS_PINNED_HERE {
        assert_eq!(
            actual,
            GOLDEN_DIGEST,
            "render moved; listen to {} and, if intended, pin {actual}",
            wav.display()
        );
    }
}

/// **The digest must move when the envelope depth the score sets moves**, or it proves nothing about
/// the modulation it claims to exercise.
///
/// **Both renders come from the same bundle.** Compared against the pinned constant instead, any
/// change that moves the base render passes trivially — which is how this test's first version stayed
/// green against a bundle that ignored Env Mod altogether.
#[test]
fn the_reference_is_sensitive_to_the_envelope_depth() {
    let Some((dir, file)) = bundle() else {
        eprintln!("skipping: run `cargo xtask bundle mxm-mono-03 --release`");
        return;
    };
    let render_with = |name: &str, env_mod: Option<f64>| {
        let mut s = Session::scratch(name, vec![dir.clone()]);
        s.load(&file, PLUGIN);
        score_with(&mut s, |s| {
            if let Some(fraction) = env_mod {
                set(s, "Cutoff from Filter envelope", fraction);
            }
        })
        .unwrap();
        s.captured()
    };
    let base = render_with("golden-mono-03-base", None);
    let deeper = render_with("golden-mono-03-sensitive", Some(0.975));
    assert_eq!(base.len(), GOLDEN_SAMPLES);
    assert_eq!(
        deeper.len(),
        GOLDEN_SAMPLES,
        "the same score, so the same length"
    );
    assert_ne!(
        digest(&base),
        digest(&deeper),
        "moving the Env Mod route must move the render"
    );
}

fn digest(samples: &[f32]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}
