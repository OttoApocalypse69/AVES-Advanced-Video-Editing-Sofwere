//! Core types for the video editor.
//!
//! This module provides the fundamental data structures for the timeline,
//! tracks, clips, and time representation, as defined in the project architecture.
//! All time values are in nanoseconds (i64) as specified in SPEC.md.

pub mod clip;
pub mod time;
pub mod timeline;
pub mod track;

// Re-export core data structures for easier access.
pub use clip::Clip;
pub use time::{Time, Timestamp, ZERO};
pub use timeline::Timeline;
pub use track::Track;
