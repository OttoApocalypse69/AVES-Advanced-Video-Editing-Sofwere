//! Playback state machine.

use std::time::Instant;
use crate::core::time::Time;

/// Playback state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    /// Stopped - no playback active
    Stopped,
    /// Playing - actively playing from a timeline position
    Playing {
        start_time: Instant,
        timeline_start: Time,  // nanoseconds
    },
    /// Paused - playback paused at a specific position
    Paused {
        timeline_position: Time,  // nanoseconds
    },
    /// Seeking - transitioning to a new position
    Seeking {
        target: Time,  // nanoseconds
    },
}

impl PlaybackState {
    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        matches!(self, PlaybackState::Playing { .. })
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        matches!(self, PlaybackState::Paused { .. })
    }

    /// Check if stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self, PlaybackState::Stopped)
    }

    /// Check if seeking
    pub fn is_seeking(&self) -> bool {
        matches!(self, PlaybackState::Seeking { .. })
    }

    /// Get the current timeline position based on state (nanoseconds)
    pub fn current_position(&self) -> Time {
        match self {
            PlaybackState::Stopped => 0,
            PlaybackState::Playing { start_time, timeline_start } => {
                let elapsed = start_time.elapsed();
                let elapsed_ns = elapsed.as_nanos() as i64;
                timeline_start + elapsed_ns
            }
            PlaybackState::Paused { timeline_position } => *timeline_position,
            PlaybackState::Seeking { target } => *target,
        }
    }
}
