//! The **only** channel from the audio thread to the editor.
//!
//! Atomics, written once per block, read whenever the editor happens to look. No locks, no
//! allocation, and the UI may drop as many frames as it likes — a display that made the audio
//! thread wait would be a display that could cause a dropout.
//!
//! **The meter and the developer channel's requests are `mxm_telemetry`'s**, held as `meter` and
//! `dev`, with one policy for every plugin: a peak is max-combined and reset when the UI reads it,
//! a clip latches until acknowledged (design system §5.4), and a non-finite peak latches the clip
//! and is never the level. What follows here is this plugin's own.
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
use std::sync::atomic::{AtomicU32, Ordering};

/// How many past blocks of sweep charge the editor can draw.
///
/// A ring rather than a single value, because the point of the display is the *shape* of a climb
/// across several notes, which one number cannot show. Sized for a few seconds of blocks at a
/// typical buffer size; it is a display, so a stale tail is preferable to a lock.
pub const SWEEP_HISTORY: usize = 256;

#[derive(Debug)]
pub struct Telemetry {
    /// The output level and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`).
    pub meter: mxm_telemetry::PeakMeter,
    /// The developer channel's requests of the editor, each taken once
    /// (`mxm_telemetry::DevRequests`).
    pub dev: mxm_telemetry::DevRequests,
    /// The accent sweep's charge, one entry per block, oldest overwritten.
    sweep: [AtomicU32; SWEEP_HISTORY],
    /// Where the next block's charge goes. Wraps.
    sweep_head: AtomicU32,
    /// Published once in `activate`, because the filter curve is plotted against it and it changes
    /// only when the host reconfigures.
    sample_rate: AtomicU32,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            meter: mxm_telemetry::PeakMeter::new(),
            dev: mxm_telemetry::DevRequests::new(),
            sweep: std::array::from_fn(|_| AtomicU32::new(0)),
            sweep_head: AtomicU32::new(0),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    // ---- audio thread ----

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
