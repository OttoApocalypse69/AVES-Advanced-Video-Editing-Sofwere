//! Audio/video synchronization controller.
//! Implements audio-driven timing with video sync.
//! Master clock is in nanoseconds per SPEC.md

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crate::core::time::Timestamp;

/// Synchronization controller for audio-driven playback
/// Master clock is maintained by audio thread in nanoseconds
pub struct SyncController {
    /// Master clock: nanoseconds since playback started
    master_clock: Arc<AtomicU64>,
    /// Timeline start position when playback began (nanoseconds)
    timeline_start: Timestamp,
}

impl SyncController {
    /// Create a new sync controller
    pub fn new() -> Self {
        Self {
            master_clock: Arc::new(AtomicU64::new(0)),
            timeline_start: 0,
        }
    }

    /// Get the master clock (for audio thread)
    pub fn master_clock(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.master_clock)
    }

    /// Start playback from a timeline position (nanoseconds)
    pub fn start(&mut self, timeline_position: Timestamp) {
        self.timeline_start = timeline_position;
        self.master_clock.store(0, Ordering::Relaxed);
    }

    /// Stop playback
    pub fn stop(&mut self) {
        self.master_clock.store(0, Ordering::Relaxed);
    }

    /// Get the current timeline position based on master clock (nanoseconds)
    pub fn current_timeline_position(&self) -> Timestamp {
        let elapsed_ns = self.master_clock.load(Ordering::Relaxed) as i64;
        self.timeline_start + elapsed_ns
    }

    /// Update master clock (called by audio thread)
    pub fn update_clock(&self, nanoseconds: u64) {
        self.master_clock.store(nanoseconds, Ordering::Relaxed);
    }

    /// Seek to a new timeline position (nanoseconds)
    pub fn seek(&mut self, position: Timestamp) {
        self.timeline_start = position;
        self.master_clock.store(0, Ordering::Relaxed);
    }

    /// Calculate video frame timestamp for synchronization (nanoseconds)
    /// Returns the target timestamp that video should display
    pub fn video_target_timestamp(&self) -> Timestamp {
        self.current_timeline_position()
    }

    /// Check if video is ahead or behind audio
    /// Returns positive if video is ahead, negative if behind (in nanoseconds)
    pub fn sync_offset(&self, video_timestamp: Timestamp) -> i64 {
        let target = self.video_target_timestamp();
        video_timestamp - target
    }
}

impl Default for SyncController {
    fn default() -> Self {
        Self::new()
    }
}
