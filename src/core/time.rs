//! Time representation using nanoseconds for frame-accurate video editing.
//! Per SPEC.md: Time unit is nanoseconds (i64), with audio as master clock.

use std::fmt;
use std::ops::{Add, Sub};

/// Time in nanoseconds since timeline start
/// This is the core time representation throughout the application
pub type Time = i64;

/// Time constants for conversions
pub mod constants {
    use super::Time;

    pub const NANOS_PER_SECOND: Time = 1_000_000_000;
    pub const NANOS_PER_MILLI: Time = 1_000_000;
    pub const NANOS_PER_MICRO: Time = 1_000;
}

/// Convert seconds (f64) to nanoseconds (i64)
#[inline]
pub fn from_seconds(seconds: f64) -> Time {
    (seconds * constants::NANOS_PER_SECOND as f64) as Time
}

/// Convert nanoseconds (i64) to seconds (f64)
#[inline]
pub fn to_seconds(nanos: Time) -> f64 {
    nanos as f64 / constants::NANOS_PER_SECOND as f64
}

/// Convert milliseconds to nanoseconds
#[inline]
pub fn from_millis(millis: i64) -> Time {
    millis * constants::NANOS_PER_MILLI
}

/// Convert nanoseconds to milliseconds
#[inline]
pub fn to_millis(nanos: Time) -> i64 {
    nanos / constants::NANOS_PER_MILLI
}

/// Convert microseconds to nanoseconds
#[inline]
pub fn from_micros(micros: i64) -> Time {
    micros * constants::NANOS_PER_MICRO
}

/// Convert nanoseconds to microseconds
#[inline]
pub fn to_micros(nanos: Time) -> i64 {
    nanos / constants::NANOS_PER_MICRO
}

/// Convert time to frame index given a frame rate
#[inline]
pub fn to_frame_index(nanos: Time, fps: f64) -> usize {
    (to_seconds(nanos) * fps).floor() as usize
}

/// Convert frame index to time given a frame rate
#[inline]
pub fn from_frame_index(frame_index: usize, fps: f64) -> Time {
    from_seconds(frame_index as f64 / fps)
}

/// Time zero constant
pub const ZERO: Time = 0;

/// Type alias for backward compatibility
/// Some modules use Timestamp instead of Time
pub type Timestamp = Time;

/// Alias for from_seconds (for backward compatibility)
#[inline]
pub fn seconds_to_ns(seconds: f64) -> Time {
    from_seconds(seconds)
}

/// Alias for to_seconds (for backward compatibility)
#[inline]
pub fn ns_to_seconds(nanos: Time) -> f64 {
    to_seconds(nanos)
}

/// Format time as HH:MM:SS.mmm
pub fn format_time(nanos: Time) -> String {
    let total_seconds = to_seconds(nanos);
    let hours = (total_seconds / 3600.0).floor() as i64;
    let minutes = ((total_seconds % 3600.0) / 60.0).floor() as i64;
    let seconds = (total_seconds % 60.0).floor() as i64;
    let millis = to_millis(nanos) % 1000;
    
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_conversion() {
        let time = from_seconds(1.5);
        assert_eq!(time, 1_500_000_000);
        assert!((to_seconds(time) - 1.5).abs() < 0.000001);
    }

    #[test]
    fn test_millis_conversion() {
        let time = from_millis(1500);
        assert_eq!(time, 1_500_000_000);
        assert_eq!(to_millis(time), 1500);
    }

    #[test]
    fn test_frame_index() {
        let time = from_seconds(1.0);
        assert_eq!(to_frame_index(time, 30.0), 30);
        
        let time2 = from_frame_index(30, 30.0);
        assert_eq!(time2, from_seconds(1.0));
    }

    #[test]
    fn test_format_time() {
        let time = from_seconds(3661.5); // 1 hour, 1 minute, 1.5 seconds
        let formatted = format_time(time);
        assert_eq!(formatted, "01:01:01.500");
    }

    #[test]
    fn test_zero() {
        assert_eq!(ZERO, 0);
        assert_eq!(to_seconds(ZERO), 0.0);
    }

    #[test]
    fn test_micros_conversion() {
        let time = from_micros(1_500_000);
        assert_eq!(time, 1_500_000_000);
        assert_eq!(to_micros(time), 1_500_000);
    }

    #[test]
    fn test_time_arithmetic() {
        let t1 = from_seconds(1.5);
        let t2 = from_seconds(2.5);
        
        // Addition
        let sum = t1 + t2;
        assert_eq!(sum, from_seconds(4.0));
        
        // Subtraction
        let diff = t2 - t1;
        assert_eq!(diff, from_seconds(1.0));
        
        // Negative time (allowed for offsets)
        let neg = -from_seconds(1.0);
        assert_eq!(neg, -1_000_000_000);
    }

    #[test]
    fn test_precision() {
        // Test nanosecond precision
        let one_nano = 1;
        assert_eq!(one_nano, 1);
        
        // Test frame-accurate timing at 30fps
        let frame_time_30fps = from_seconds(1.0 / 30.0);
        let frame_index = to_frame_index(frame_time_30fps, 30.0);
        assert_eq!(frame_index, 1);
        
        // Test frame-accurate timing at 60fps
        let frame_time_60fps = from_seconds(1.0 / 60.0);
        let frame_index_60 = to_frame_index(frame_time_60fps, 60.0);
        assert_eq!(frame_index_60, 1);
    }

    #[test]
    fn test_large_time_values() {
        // Test handling of large time values (hours)
        let one_hour = from_seconds(3600.0);
        assert_eq!(one_hour, 3_600_000_000_000);
        
        let hours = to_seconds(one_hour) / 3600.0;
        assert!((hours - 1.0).abs() < 0.000001);
    }

    #[test]
    fn test_time_conversion_roundtrip() {
        // Test that conversions are reversible
        let original_seconds = 123.456789;
        let time = from_seconds(original_seconds);
        let converted_back = to_seconds(time);
        
        // Should be very close (within floating point precision)
        assert!((original_seconds - converted_back).abs() < 0.000001);
    }

    #[test]
    fn test_frame_index_edge_cases() {
        // Test frame index at exact frame boundaries
        let frame_0 = from_frame_index(0, 30.0);
        assert_eq!(frame_0, 0);
        
        let frame_30 = from_frame_index(30, 30.0);
        assert_eq!(to_frame_index(frame_30, 30.0), 30);
        
        // Test fractional frame times
        let half_frame = from_seconds(0.5 / 30.0);
        let frame_idx = to_frame_index(half_frame, 30.0);
        assert_eq!(frame_idx, 0); // Should floor to 0
    }
}
