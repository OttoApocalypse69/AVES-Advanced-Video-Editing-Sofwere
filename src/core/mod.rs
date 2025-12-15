//! Core types for the video editor.
//! 
//! This module provides the fundamental time representation used throughout the application.
//! All time values are in nanoseconds (i64) as specified in SPEC.md.

pub mod time;

// Re-export time type and common functions
pub use time::{Time, Timestamp, ZERO};
