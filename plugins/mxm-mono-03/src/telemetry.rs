//! The **only** channel from the audio thread to the editor.
//!
//! Atomics, written once per block, read whenever the editor happens to look. No locks, no
//! allocation, and the UI may drop as many frames as it likes — a display that made the audio
//! thread wait would be a display that could cause a dropout.
//!
//! Two rules carried from mxm-mono-01's `plugins/mxm-mono-01/src/telemetry.rs`, both of which
//! exist because the obvious implementation loses information:
//!
//! - **A peak is max-combined and reset when the UI reads it.** Overwriting each block means a
//!   transient that landed between two frames is simply gone; combining means the value is always
//!   *loudest since you last looked*.
//! - **A clip latches until acknowledged.** A meter that quietly forgets it clipped is worse than
//!   no meter, and design system §5.4 requires the indication to persist.
//!
//! # The accent sweep is this instrument's own
//!
//! `mxm-mono-01` publishes an envelope level because its brief asks for an envelope display. This
//! one publishes the **accent sweep's accumulated charge**, for the reason the brief §8 gives: when
//! consecutive accents climb, nothing on the panel can show it. The Accent knob has not moved and
//! the note is simply brighter than the last one, so without this the instrument's most distinctive
//! behaviour looks like a fault.
//!
//! It is **exact** rather than approximated — the value the DSP actually used, read back through
//! `Accent::sweep()` once per block.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// How many past blocks of sweep charge the editor can draw.
///
/// A ring rather than a single value, because the point of the display is the *shape* of a climb
/// across several notes, which one number cannot show. Sized for a few seconds of blocks at a
/// typical buffer size; it is a display, so a stale tail is preferable to a lock.
pub const SWEEP_HISTORY: usize = 256;

#[derive(Debug)]
pub struct Telemetry {
    /// Peak of the samples produced, max-combined, reset on read.
    peak: AtomicU32,
    /// Sticky: set when a sample reaches full scale, cleared only by the user.
    clipped: AtomicBool,
    /// The accent sweep's charge, one entry per block, oldest overwritten.
    sweep: [AtomicU32; SWEEP_HISTORY],
    /// Where the next block's charge goes. Wraps.
    sweep_head: AtomicU32,
    /// Published once in `activate`, because the filter curve is plotted against it and it changes
    /// only when the host reconfigures.
    sample_rate: AtomicU32,
    /// The developer channel's requests of the editor: a view to show, and whether the expander
    /// is open. `NO_REQUEST` when nothing is asked. See `plugins/AGENTS.md`.
    dev_view: AtomicU8,
    dev_disclosure: AtomicU8,
    /// The developer channel's request to open or close the preset browser, or `NO_REQUEST`.
    dev_browser: AtomicU8,
    /// The developer channel's request to show a theme, by index, or `NO_REQUEST`. Theme is
    /// interface state, so this reaches the editor and nothing else; the DSP never sees it.
    dev_theme: AtomicU8,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            peak: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
            sweep: std::array::from_fn(|_| AtomicU32::new(0)),
            sweep_head: AtomicU32::new(0),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            dev_view: AtomicU8::new(u8::MAX),
            dev_disclosure: AtomicU8::new(u8::MAX),
            dev_browser: AtomicU8::new(u8::MAX),
            dev_theme: AtomicU8::new(u8::MAX),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    // ---- audio thread ----

    /// Publish a block's peak. **Combined, not overwritten**: see the module doc.
    pub fn publish_peak(&self, peak: f32) {
        let mut current = self.peak.load(Ordering::Relaxed);
        loop {
            let combined = f32::from_bits(current).max(peak);
            match self.peak.compare_exchange_weak(
                current,
                combined.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
        if peak >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
    }

    /// Publish a block's accent sweep charge.
    pub fn publish_sweep(&self, charge: f32) {
        let head = self.sweep_head.load(Ordering::Relaxed) as usize % SWEEP_HISTORY;
        self.sweep[head].store(charge.to_bits(), Ordering::Relaxed);
        self.sweep_head
            .store(((head + 1) % SWEEP_HISTORY) as u32, Ordering::Relaxed);
    }

    pub fn publish_sample_rate(&self, rate: f32) {
        self.sample_rate.store(rate.to_bits(), Ordering::Relaxed);
    }

    // ---- editor thread ----

    /// The loudest sample since this was last called, **and resets**.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    /// Acknowledge the clip indication. The user's act, never a timeout.
    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
    }

    /// The sweep history, oldest first.
    pub fn sweep_history(&self) -> Vec<f32> {
        let head = self.sweep_head.load(Ordering::Relaxed) as usize % SWEEP_HISTORY;
        (0..SWEEP_HISTORY)
            .map(|i| f32::from_bits(self.sweep[(head + i) % SWEEP_HISTORY].load(Ordering::Relaxed)))
            .collect()
    }

    pub fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peak_is_combined_and_reset_on_read() {
        // A transient landing between two frames must not be lost, which is what overwriting does.
        let t = Telemetry::new();
        t.publish_peak(0.4);
        t.publish_peak(0.9);
        t.publish_peak(0.2);
        assert_eq!(t.take_peak(), 0.9, "the loudest of the three, not the last");
        assert_eq!(t.take_peak(), 0.0, "and reading resets it");
    }

    #[test]
    fn a_clip_latches_until_acknowledged() {
        let t = Telemetry::new();
        assert!(!t.clipped());
        t.publish_peak(1.0);
        assert!(t.clipped());
        // Quiet blocks must not clear it: the point is that it happened.
        for _ in 0..100 {
            t.publish_peak(0.1);
        }
        assert!(t.clipped(), "a meter that forgets is worse than no meter");
        t.clear_clip();
        assert!(!t.clipped());
    }

    #[test]
    fn the_sweep_history_reads_oldest_first_and_wraps() {
        let t = Telemetry::new();
        for i in 0..SWEEP_HISTORY + 10 {
            t.publish_sweep(i as f32);
        }
        let history = t.sweep_history();
        assert_eq!(history.len(), SWEEP_HISTORY);
        // The most recent value is last, and the oldest survivor is first.
        assert_eq!(*history.last().unwrap(), (SWEEP_HISTORY + 9) as f32);
        assert_eq!(history[0], 10.0);
    }

    #[test]
    fn the_sample_rate_survives_a_round_trip() {
        let t = Telemetry::new();
        t.publish_sample_rate(96_000.0);
        assert_eq!(t.sample_rate(), 96_000.0);
    }
}

/// Nothing requested on a developer-channel slot.
const NO_REQUEST: u8 = u8::MAX;

/// The developer channel's requests of the editor — `plugins/AGENTS.md`, *A developer channel in
/// every editor*. Each is taken once; the DSP reads nothing.
impl Telemetry {
    /// Developer category address (0–5), or Parameters (127); never a derived tab index.
    pub fn request_view(&self, view: u8) {
        self.dev_view
            .store(view.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    /// The developer channel asks the editor to open or close the preset browser.
    pub fn request_browser(&self, open: bool) {
        self.dev_browser.store(u8::from(open), Ordering::Relaxed);
    }

    /// Whether the developer channel asked the browser open or closed since the editor last
    /// looked, if it did.
    pub fn take_browser_request(&self) -> Option<bool> {
        match self.dev_browser.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            open => Some(open != 0),
        }
    }

    /// The developer channel asks the editor for a theme, by index — 0 light, 1 dark, 2 system,
    /// as `mxm_ui::theme::from_index` reads it.
    pub fn request_theme(&self, theme: u8) {
        self.dev_theme
            .store(theme.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    /// The theme the developer channel asked for since the editor last looked, if any.
    pub fn take_theme_request(&self) -> Option<u8> {
        match self.dev_theme.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            theme => Some(theme),
        }
    }

    /// The developer channel asks the editor to open or close its expander.
    pub fn request_disclosure(&self, open: bool) {
        self.dev_disclosure.store(u8::from(open), Ordering::Relaxed);
    }

    /// The view the developer channel asked for since the editor last looked, if any.
    pub fn take_view_request(&self) -> Option<usize> {
        match self.dev_view.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            view => Some(usize::from(view)),
        }
    }

    /// Whether the developer channel asked the expander open or closed since the editor last
    /// looked, if it did.
    pub fn take_disclosure_request(&self) -> Option<bool> {
        match self.dev_disclosure.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            open => Some(open != 0),
        }
    }
}

#[cfg(test)]
mod developer_channel_tests {
    use super::*;

    #[test]
    fn a_developer_request_is_taken_once() {
        let t = Telemetry::new();
        assert_eq!(
            t.take_view_request(),
            None,
            "nothing asked on a fresh instance"
        );
        t.request_view(1);
        assert_eq!(t.take_view_request(), Some(1));
        assert_eq!(t.take_view_request(), None, "and taking it clears it");
        t.request_disclosure(true);
        assert_eq!(t.take_disclosure_request(), Some(true));
        t.request_browser(true);
        assert_eq!(t.take_browser_request(), Some(true));
        assert_eq!(t.take_browser_request(), None, "taken once");
        assert_eq!(t.take_disclosure_request(), None);
    }
}
