//! Clip data structure representing a segment of video/audio on the timeline.

use std::path::PathBuf;
use crate::core::time::Time;

/// Unique identifier for a clip
pub type ClipId = u64;

/// A clip represents a segment of source media placed on the timeline
#[derive(Debug, Clone)]
pub struct Clip {
    pub id: ClipId,
    pub source_path: PathBuf,
    pub in_point: Time,        // Start time in source media (nanoseconds)
    pub out_point: Time,       // End time in source media (nanoseconds)
    pub timeline_start: Time,  // Position on timeline (nanoseconds)
    pub timeline_end: Time,    // End position on timeline (nanoseconds)
    pub stream_index: usize,   // Which stream in source file (0 = first video, 1 = first audio, etc.)
}

impl Clip {
    /// Create a new clip
    pub fn new(
        id: ClipId,
        source_path: PathBuf,
        in_point: Time,
        out_point: Time,
        timeline_start: Time,
        stream_index: usize,
    ) -> Self {
        let duration = out_point - in_point;
        let timeline_end = timeline_start + duration;

        Self {
            id,
            source_path,
            in_point,
            out_point,
            timeline_start,
            timeline_end,
            stream_index,
        }
    }

    /// Get the duration of the clip in nanoseconds
    pub fn duration(&self) -> Time {
        self.out_point - self.in_point
    }

    /// Check if a timeline position is within this clip
    pub fn contains(&self, timeline_position: Time) -> bool {
        timeline_position >= self.timeline_start && timeline_position <= self.timeline_end
    }

    /// Convert a timeline position to a source position
    /// Returns None if the timeline position is not within this clip
    pub fn timeline_to_source(&self, timeline_position: Time) -> Option<Time> {
        if !self.contains(timeline_position) {
            return None;
        }

        let offset = timeline_position - self.timeline_start;
        let source_time = self.in_point + offset;
        Some(source_time)
    }

    /// Convert a source position to a timeline position
    /// Returns None if the source position is not within this clip's range
    pub fn source_to_timeline(&self, source_position: Time) -> Option<Time> {
        if source_position < self.in_point || source_position > self.out_point {
            return None;
        }

        let offset = source_position - self.in_point;
        let timeline_time = self.timeline_start + offset;
        Some(timeline_time)
    }

    /// Trim the start of the clip (move in_point forward)
    /// `new_in_point` must be >= current in_point and < out_point
    /// Returns true if successful, false if invalid
    pub fn trim_in(&mut self, new_in_point: Time) -> bool {
        if new_in_point < self.in_point || new_in_point >= self.out_point {
            return false;
        }

        let trim_amount = new_in_point - self.in_point;
        self.in_point = new_in_point;
        self.timeline_start += trim_amount;
        // timeline_end stays the same (duration decreases)
        true
    }

    /// Trim the end of the clip (move out_point backward)
    /// `new_out_point` must be > in_point and <= current out_point
    /// Returns true if successful, false if invalid
    pub fn trim_out(&mut self, new_out_point: Time) -> bool {
        if new_out_point <= self.in_point || new_out_point > self.out_point {
            return false;
        }

        let old_duration = self.duration();
        self.out_point = new_out_point;
        let new_duration = self.duration();
        self.timeline_end = self.timeline_start + new_duration;
        true
    }

    /// Set the timeline start position (moves the clip)
    /// Updates timeline_end to maintain duration
    pub fn set_timeline_start(&mut self, new_timeline_start: Time) {
        let duration = self.duration();
        self.timeline_start = new_timeline_start;
        self.timeline_end = new_timeline_start + duration;
    }

    /// Move the clip to a new timeline position (alias for set_timeline_start)
    pub fn move_to(&mut self, new_timeline_start: Time) {
        self.set_timeline_start(new_timeline_start);
    }

    /// Check if this clip overlaps with another clip
    pub fn overlaps_with(&self, other: &Clip) -> bool {
        // Two clips overlap if neither is completely before the other
        !(self.timeline_end <= other.timeline_start || other.timeline_end <= self.timeline_start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::time;

    #[test]
    fn test_clip_creation() {
        let in_point = time::from_seconds(1.0);
        let out_point = time::from_seconds(3.0);
        let timeline_start = time::from_seconds(0.0);

        let clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            in_point,
            out_point,
            timeline_start,
            0,
        );

        assert_eq!(clip.duration(), time::from_seconds(2.0));
        assert_eq!(clip.timeline_end, time::from_seconds(2.0));
    }

    #[test]
    fn test_clip_contains() {
        let clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(0.0),
            0,
        );

        assert!(clip.contains(time::from_seconds(1.0)));
        assert!(!clip.contains(time::from_seconds(3.0)));
    }

    #[test]
    fn test_timeline_to_source() {
        let clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(5.0),  // Start at 5s in source
            time::from_seconds(10.0), // End at 10s in source
            time::from_seconds(0.0),  // Start at 0s on timeline
            0,
        );

        // Timeline position 2.0s should map to source position 7.0s
        assert_eq!(
            clip.timeline_to_source(time::from_seconds(2.0)),
            Some(time::from_seconds(7.0))
        );
        
        // Position outside clip should return None
        assert_eq!(clip.timeline_to_source(time::from_seconds(10.0)), None);
    }

    #[test]
    fn test_source_to_timeline() {
        let clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(5.0),
            time::from_seconds(10.0),
            time::from_seconds(2.0),  // Timeline starts at 2s
            0,
        );

        // Source position 7.0s should map to timeline position 4.0s
        assert_eq!(
            clip.source_to_timeline(time::from_seconds(7.0)),
            Some(time::from_seconds(4.0))
        );
        
        // Position outside clip source range should return None
        assert_eq!(clip.source_to_timeline(time::from_seconds(1.0)), None);
    }

    #[test]
    fn test_trim_in() {
        let mut clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0),
            0,
        );

        // Trim 2 seconds from the start
        assert!(clip.trim_in(time::from_seconds(2.0)));
        assert_eq!(clip.in_point, time::from_seconds(2.0));
        assert_eq!(clip.timeline_start, time::from_seconds(7.0)); // Moved forward by 2s
        assert_eq!(clip.timeline_end, time::from_seconds(15.0)); // End unchanged
        assert_eq!(clip.duration(), time::from_seconds(8.0));

        // Invalid: new_in_point before current in_point
        assert!(!clip.trim_in(time::from_seconds(1.0)));

        // Invalid: new_in_point >= out_point
        assert!(!clip.trim_in(time::from_seconds(8.0)));
    }

    #[test]
    fn test_trim_out() {
        let mut clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0),
            0,
        );

        // Trim 2 seconds from the end
        assert!(clip.trim_out(time::from_seconds(8.0)));
        assert_eq!(clip.out_point, time::from_seconds(8.0));
        assert_eq!(clip.timeline_start, time::from_seconds(5.0)); // Start unchanged
        assert_eq!(clip.timeline_end, time::from_seconds(13.0)); // End moved back
        assert_eq!(clip.duration(), time::from_seconds(8.0));

        // Invalid: new_out_point <= in_point
        assert!(!clip.trim_out(time::from_seconds(0.0)));

        // Invalid: new_out_point > current out_point
        assert!(!clip.trim_out(time::from_seconds(10.0)));
    }

    #[test]
    fn test_set_timeline_start() {
        let mut clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0),
            0,
        );

        let duration = clip.duration();
        clip.set_timeline_start(time::from_seconds(20.0));
        
        assert_eq!(clip.timeline_start, time::from_seconds(20.0));
        assert_eq!(clip.timeline_end, time::from_seconds(20.0) + duration);
        assert_eq!(clip.duration(), duration); // Duration unchanged
    }

    #[test]
    fn test_move_to() {
        let mut clip = Clip::new(
            1,
            PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0),
            0,
        );

        clip.move_to(time::from_seconds(100.0));
        assert_eq!(clip.timeline_start, time::from_seconds(100.0));
        assert_eq!(clip.timeline_end, time::from_seconds(110.0));
    }

    #[test]
    fn test_overlaps_with() {
        let clip1 = Clip::new(
            1,
            PathBuf::from("test1.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(0.0),
            0,
        );

        let clip2 = Clip::new(
            2,
            PathBuf::from("test2.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0), // Overlaps with clip1
            0,
        );

        let clip3 = Clip::new(
            3,
            PathBuf::from("test3.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(15.0), // No overlap
            0,
        );

        assert!(clip1.overlaps_with(&clip2));
        assert!(clip2.overlaps_with(&clip1));
        assert!(!clip1.overlaps_with(&clip3));
        assert!(!clip3.overlaps_with(&clip1));
    }
}
