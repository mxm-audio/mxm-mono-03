//! mxm-mono-03 on rendered audio, through the player's own hosting path.
//!
//! What no unit test in either crate can see: that the shipped `.clap` loads and a note played
//! through the player comes out at its pitch, that a cutoff the host writes reaches the sound, that
//! **the per-step accent button, pressed by the host, makes a note louder**, that **Slide glides into
//! the next note** through the host's event path, that a release ends in exact silence — and, since
//! the routing conversion, that **the routes are reached through the real callback by permanent
//! name**: the Env Mod route opening a closed filter, and a route the machine never had, the mod
//! wheel into the resonance (`docs/code-review-notes.md` §7, *arming a topology is not exercising a
//! route*).
//!
//! Skips, with the reason, when the bundle is not built: run
//! `cargo xtask bundle mxm-mono-03 --release` first.

use mxm_player_harness::app_harness;

use mxm_player::events::input::Payload;
use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-mono-03";
const SAMPLE_RATE: f64 = 48_000.0;
const SKIP: &str = "skipping: run `cargo xtask bundle mxm-mono-03 --release`";

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::bundled_dir_with("mxm-mono-03")?;
    let file = dir.join("mxm-mono-03.clap");
    file.exists().then_some((dir, file))
}

fn session(name: &str) -> Option<Session> {
    let (dir, file) = bundle()?;
    let mut s = Session::scratch(name, vec![dir]);
    s.load(&file, PLUGIN);
    Some(s)
}

// --- measuring ---------------------------------------------------------------------------------

use mxm_measure::channels::left;
use mxm_measure::convert::note_hz;

/// Peak magnitude of a capture. **The `expect` is the point**: the shared ruler reports absence for
/// a non-finite buffer, and folding that into a number would let a NaN render pass a silence check.
fn peak(samples: &[f32]) -> f32 {
    mxm_measure::level::peak(samples).expect("the capture is finite")
}

/// How much of one frequency is in a captured window — **a relative figure, not an amplitude**: the
/// windows are whole blocks rather than whole cycles, so compare pitches within one window only.
/// Absence panics rather than reading as zero, for the same reason as [`peak`].
fn magnitude_at(samples: &[f32], hz: f64) -> f64 {
    mxm_measure::spectrum::component_amplitude(samples, hz, SAMPLE_RATE)
        .expect("the capture is non-empty and finite")
}

/// A crude high-frequency measure: mean absolute sample-to-sample difference. Relative only.
fn brightness(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    samples.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (samples.len() - 1) as f32
}

// --- driving -----------------------------------------------------------------------------------

/// Sets a parameter by display name, as a position in its normalised range, and lets it settle.
fn set_param(session: &mut Session, name: &str, fraction: f64) -> String {
    let param = session
        .state()
        .param(name)
        .unwrap_or_else(|| panic!("`{name}` is not a parameter"))
        .clone();
    let value = param.min + fraction * (param.max - param.min);
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ParamValue {
            param_id: param.id,
            value,
        });
    session.advance_blocks(4).expect("the session advances");
    session
        .state()
        .param(name)
        .map(|p| p.text.clone())
        .unwrap_or_default()
}

/// Renders `blocks` of whatever is sounding and returns the left channel after the first few
/// blocks, so an attack is not in the measurement.
fn capture(session: &mut Session, blocks: u64) -> Vec<f32> {
    session.clear_capture();
    session.advance_blocks(blocks).expect("advances");
    let audio = session.captured();
    let skip = (FRAMES_PER_BLOCK * 2 * 3).min(audio.len());
    left(&audio[skip..])
}

/// Holds `note`, renders `blocks`, releases it, and returns the held part after the attack.
fn note(session: &mut Session, note: u8, blocks: u64) -> Vec<f32> {
    session.app().note_on(note, 100.0 / 127.0);
    let held = capture(session, blocks);
    session.app().note_off(note);
    session.advance_blocks(80).expect("advances");
    held
}

/// Moves the mod wheel, CC 1, on channel 0, and lets it land.
fn wheel(session: &mut Session, value: u8) {
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ControlChange {
            channel: 0,
            controller: 1,
            value,
        });
    session.advance_blocks(1).expect("advances");
}

/// RMS of a capture — **phase-insensitive**, which a spectral fingerprint over a whole-block window is
/// not: the oscillator is free-running, so each note starts at another phase against the window.
/// Absence panics rather than reading as zero, for the same reason as [`peak`].
fn rms(samples: &[f32]) -> f64 {
    mxm_measure::level::rms(samples).expect("the capture is non-empty and finite")
}

// --- the tests ---------------------------------------------------------------------------------

#[test]
fn at_rest_it_is_exactly_silent() {
    let Some(mut s) = session("mono03-rest") else {
        eprintln!("{SKIP}");
        return;
    };
    s.advance_blocks(20).expect("advances");
    assert_eq!(
        peak(&s.captured()),
        0.0,
        "an idle synth must render exact zeros, not merely something quiet"
    );
}

#[test]
fn a_note_sounds_at_its_pitch() {
    let Some(mut s) = session("mono03-pitch") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Cutoff", 0.9);
    let held = note(&mut s, 45, 20);
    let at = magnitude_at(&held, note_hz(45.0));
    let above = magnitude_at(&held, note_hz(46.0));
    let below = magnitude_at(&held, note_hz(44.0));
    assert!(
        at > 3.0 * above && at > 3.0 * below,
        "note 45 must sound at its own pitch: {at} there, {below} a semitone below, {above} above"
    );
}

#[test]
fn closing_the_cutoff_darkens_the_note() {
    let Some(mut s) = session("mono03-cutoff") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Cutoff", 0.9);
    let open = brightness(&note(&mut s, 45, 16));
    set_param(&mut s, "Cutoff", 0.1);
    let closed = brightness(&note(&mut s, 45, 16));
    assert!(
        closed < open * 0.5,
        "a host-written cutoff must reach the sound: closed {closed} against open {open}"
    );
}

/// **The machine's per-step accent button, pressed by the host**: the same note, louder, with the
/// Accent knob up. The button is read at the note-on it belongs to.
#[test]
fn the_accent_button_makes_a_note_louder() {
    let Some(mut s) = session("mono03-accent") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Accent", 1.0);
    let plain = peak(&note(&mut s, 45, 10));
    set_param(&mut s, "Accent note", 1.0);
    let accented = peak(&note(&mut s, 45, 10));
    assert!(
        accented > plain * 1.3,
        "an accented note must hit harder: accented {accented} against plain {plain}"
    );
}

/// **Slide glides into the next note.** Measured in the two blocks after a tie: without Slide the new
/// note is already at its pitch, and with it the pitch is still travelling up from the old one, so the
/// new note's fundamental is far weaker there against the old one's.
#[test]
fn slide_glides_into_the_next_note() {
    let Some(mut s) = session("mono03-slide") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Cutoff", 0.9);
    let landing = |s: &mut Session, slide: bool| -> f64 {
        s.app().note_on(36, 100.0 / 127.0);
        s.advance_blocks(12).expect("advances");
        set_param(s, "Slide", if slide { 1.0 } else { 0.0 });
        s.clear_capture();
        s.app().note_on(48, 100.0 / 127.0);
        s.app().note_off(36);
        s.advance_blocks(2).expect("advances");
        let window = left(&s.captured());
        s.app().note_off(48);
        s.advance_blocks(80).expect("advances");
        magnitude_at(&window, note_hz(48.0)) / magnitude_at(&window, note_hz(36.0))
    };
    let plain = landing(&mut s, false);
    let slid = landing(&mut s, true);
    assert!(
        plain > 2.0,
        "the premise: without Slide the new note arrives at its pitch, ratio {plain}"
    );
    assert!(
        slid < plain * 0.5,
        "with Slide on the pitch is still gliding up: slid {slid} against plain {plain}"
    );
}

#[test]
fn a_release_ends_in_exact_silence() {
    let Some(mut s) = session("mono03-tail") else {
        eprintln!("{SKIP}");
        return;
    };
    note(&mut s, 45, 10);
    s.advance_blocks(500).expect("advances");
    s.clear_capture();
    s.advance_blocks(10).expect("advances");
    assert_eq!(peak(&s.captured()), 0.0, "the tail must reach exact zero");
}

/// **The headline gesture, through the real callback**: the envelope's sweep of the filter — Env Mod on
/// the machine, the route `mod_cutoff_env` now — raised by the host over a note with the filter nearly
/// shut. A route wired to the wrong target, or a parameter that reaches nothing, leaves it as dark as
/// it was.
#[test]
fn raising_the_env_mod_route_opens_a_closed_filter() {
    let Some(mut s) = session("mono03-env-route") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Cutoff", 0.15);
    set_param(&mut s, "Decay", 1.0);
    let closed = brightness(&note(&mut s, 45, 16));
    let text = set_param(&mut s, "Cutoff from Filter envelope", 1.0);
    assert!(
        text.contains("+4.60 oct"),
        "the route reads Env Mod's reach above the floor: {text}"
    );
    let opened = brightness(&note(&mut s, 45, 16));
    assert!(
        opened > closed * 2.0,
        "the route must open the filter: {opened} against {closed}"
    );
}

/// **A route the machine never had, added and played through the real callback**: the mod wheel into
/// the resonance. The note's level is the same with the wheel at rest as before the route existed, and
/// moves once the wheel is up — this ladder's droop is its topology, so resonance changes level. The
/// player sends no channel pressure, which is why this is the wheel; pressure reaching the voice is
/// proven at the plugin (`routing_path::velocity_wheel_and_pressure_reach_the_voice_as_sources`).
#[test]
fn the_wheel_routed_to_resonance_changes_the_sound_only_once_it_moves() {
    let Some(mut s) = session("mono03-wheel-route") else {
        eprintln!("{SKIP}");
        return;
    };
    set_param(&mut s, "Cutoff", 0.5);
    // The shortest decay, so the filter envelope is back at rest before each note — it is not
    // released on a note-off, so a note starts from wherever the last one left it — and a warm-up note
    // first, because the session's first note starts from a state no later one does.
    set_param(&mut s, "Decay", 0.0);
    note(&mut s, 33, 16);
    let before = rms(&note(&mut s, 33, 16));
    set_param(&mut s, "Resonance from Wheel on", 1.0);
    let text = set_param(&mut s, "Resonance from Wheel", 1.0);
    assert!(
        text.contains("+100 %"),
        "the route reads the whole resonance control: {text}"
    );
    wheel(&mut s, 0);
    let at_rest = rms(&note(&mut s, 33, 16));
    wheel(&mut s, 127);
    let pushed = rms(&note(&mut s, 33, 16));
    assert!(
        (at_rest - before).abs() < before * 0.02,
        "the premise: a route whose wheel is at rest changes nothing — {before} before the route, {at_rest} with it"
    );
    assert!(
        (pushed - at_rest).abs() > at_rest * 0.25,
        "the wheel must move the resonance: {at_rest} at rest, {pushed} pushed"
    );
}
