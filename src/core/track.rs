//! Track data structure for managing clips on video/audio tracks.
//! Per SPEC.md: Track types are Video and Audio.

use crate::core::clip::{Clip, ClipId};
use crate::core::time::Time;
use std::fmt;

/// Error type for track operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackError {
    /// Clip overlaps with existing clips on the track
    Overlap { clip_id: ClipId },
}

impl fmt::Display for TrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackError::Overlap { clip_id } => {
                write!(f, "Clip {} overlaps with existing clips on the track", clip_id)
            }
        }
    }
}

impl std::error::Error for TrackError {}

/// Unique identifier for a track
pub type TrackId = u64;

/// Type of track (video or audio)
/// Per SPEC.md: Track types are Video and Audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackType {
    Video,
    Audio,
}

/// A track contains clips arranged on a timeline.
/// 
/// Clips are stored sorted by `timeline_start` for efficient lookup.
/// Overlapping clips are not allowed on the same track.
#[derive(Debug, Clone)]
pub struct Track {
    pub id: TrackId,
    pub track_type: TrackType,
    pub clips: Vec<Clip>,  // Sorted by timeline_start
    pub muted: bool,
    pub volume: f32,       // 0.0 to 1.0
}

impl Track {
    /// Create a new track.
    /// 
    /// # Arguments
    /// - `id`: Unique identifier for the track
    /// - `track_type`: Video or Audio
    pub fn new(id: TrackId, track_type: TrackType) -> Self {
        Self {
            id,
            track_type,
            clips: Vec::new(),
            muted: false,
            volume: 1.0,
        }
    }

    /// Add a clip to the track with overlap validation.
    /// 
    /// Returns `Ok(())` if successful, `Err(TrackError::Overlap)` if the clip overlaps
    /// with existing clips. Maintains sorted order by `timeline_start`.
    /// 
    /// # Overlap Rules
    /// - Adjacent clips (touching at boundaries) are allowed
    /// - Overlapping clips are not allowed
    pub fn add_clip(&mut self, clip: Clip) -> Result<(), TrackError> {
        // Check for overlaps with existing clips
        for existing_clip in &self.clips {
            if clip.overlaps_with(existing_clip) {
                return Err(TrackError::Overlap { clip_id: clip.id });
            }
        }

        self.clips.push(clip);
        self.clips.sort_by_key(|c| c.timeline_start);
        Ok(())
    }

    /// Remove a clip by ID.
    /// 
    /// Returns the removed clip if found, `None` otherwise.
    pub fn remove_clip(&mut self, clip_id: ClipId) -> Option<Clip> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == clip_id) {
            Some(self.clips.remove(pos))
        } else {
            None
        }
    }

    /// Find the clip at a given timeline position.
    /// 
    /// Returns the first clip that contains the position, or `None` if no clip
    /// contains that position.
    pub fn clip_at(&self, timeline_position: Time) -> Option<&Clip> {
        // Since clips are sorted, we can use binary search for efficiency
        // But for simplicity, we'll use linear search (clips list is typically small)
        self.clips.iter().find(|clip| clip.contains(timeline_position))
    }

    /// Find all clips that overlap with a time range.
    /// 
    /// Returns clips where `timeline_start <= end && timeline_end >= start`.
    pub fn clips_in_range(&self, start: Time, end: Time) -> Vec<&Clip> {
        self.clips
            .iter()
            .filter(|clip| {
                // Check if clip overlaps with range
                clip.timeline_start <= end && clip.timeline_end >= start
            })
            .collect()
    }

    /// Get the duration of the track in nanoseconds.
    /// 
    /// Returns the end time of the last clip, or 0 if the track is empty.
    pub fn duration(&self) -> Time {
        self.clips
            .iter()
            .map(|clip| clip.timeline_end)
            .max()
            .unwrap_or(0)
    }

    /// Set volume (clamped to 0.0-1.0).
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    /// Set muted state.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::time;
    use std::path::PathBuf;

    #[test]
    fn test_track_creation() {
        let track = Track::new(1, TrackType::Video);
        assert_eq!(track.clips.len(), 0);
        assert_eq!(track.volume, 1.0);
        assert!(!track.muted);
    }

    #[test]
    fn test_add_clip() {
        let mut track = Track::new(1, TrackType::Video);
        
        let clip = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(0.0),
            0,
        );

        track.add_clip(clip).unwrap();
        assert_eq!(track.clips.len(), 1);
    }

    #[test]
    fn test_clip_at() {
        let mut track = Track::new(1, TrackType::Video);
        
        let clip = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(1.0),
            0,
        );

        track.add_clip(clip).unwrap();
        
        assert!(track.clip_at(time::from_seconds(1.5)).is_some());
        assert!(track.clip_at(time::from_seconds(0.5)).is_none());
    }

    #[test]
    fn test_duration() {
        let mut track = Track::new(1, TrackType::Video);
        
        let clip1 = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(0.0),
            0,
        );
        
        let clip2 = Clip::new(
            2,
            std::path::PathBuf::from("test2.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(3.0),
            time::from_seconds(5.0),
            0,
        );

        track.add_clip(clip1).unwrap();
        track.add_clip(clip2).unwrap();
        
        // Duration should be end of last clip (5 + 3 = 8 seconds)
        assert_eq!(track.duration(), time::from_seconds(8.0));
    }

    #[test]
    fn test_overlap_validation() {
        let mut track = Track::new(1, TrackType::Video);
        
        let clip1 = Clip::new(
            1,
            std::path::PathBuf::from("test1.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(0.0),
            0,
        );

        let clip2 = Clip::new(
            2,
            std::path::PathBuf::from("test2.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0), // Overlaps with clip1
            0,
        );

        let clip3 = Clip::new(
            3,
            std::path::PathBuf::from("test3.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(15.0), // No overlap
            0,
        );

        // First clip should be added successfully
        assert!(track.add_clip(clip1).is_ok());
        
        // Second clip overlaps, should fail
        assert!(matches!(track.add_clip(clip2), Err(TrackError::Overlap { clip_id: 2 })));
        
        // Third clip doesn't overlap, should succeed
        assert!(track.add_clip(clip3).is_ok());
        
        // Track should have 2 clips (clip1 and clip3)
        assert_eq!(track.clips.len(), 2);
    }

    #[test]
    fn test_overlap_edge_cases() {
        let mut track = Track::new(1, TrackType::Video);
        
        let clip1 = Clip::new(
            1,
            std::path::PathBuf::from("test1.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(0.0),
            0,
        );

        // Adjacent clips (touching but not overlapping) should be allowed
        let clip2 = Clip::new(
            2,
            std::path::PathBuf::from("test2.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(10.0), // Starts exactly where clip1 ends
            0,
        );

        // Clip that starts before but ends during clip1
        let clip3 = Clip::new(
            3,
            std::path::PathBuf::from("test3.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(5.0), // Overlaps
            0,
        );

        track.add_clip(clip1).unwrap();
        
        // Adjacent clips should be allowed
        assert!(track.add_clip(clip2).is_ok());
        
        // Overlapping clip should be rejected
        assert!(track.add_clip(clip3).is_err());
    }

    #[test]
    fn test_clips_in_range() {
        let mut track = Track::new(1, TrackType::Video);
        
        let clip1 = Clip::new(
            1,
            std::path::PathBuf::from("test1.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(5.0),
            time::from_seconds(0.0),
            0,
        );
        
        let clip2 = Clip::new(
            2,
            std::path::PathBuf::from("test2.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(5.0),
            time::from_seconds(10.0),
            0,
        );
        
        let clip3 = Clip::new(
            3,
            std::path::PathBuf::from("test3.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(5.0),
            time::from_seconds(20.0),
            0,
        );

        track.add_clip(clip1).unwrap();
        track.add_clip(clip2).unwrap();
        track.add_clip(clip3).unwrap();
        
        // Range [2s, 12s] should include clip1 and clip2
        let clips = track.clips_in_range(time::from_seconds(2.0), time::from_seconds(12.0));
        assert_eq!(clips.len(), 2);
        assert!(clips.iter().any(|c| c.id == 1));
        assert!(clips.iter().any(|c| c.id == 2));
    }

    #[test]
    fn test_sorted_order() {
        let mut track = Track::new(1, TrackType::Video);
        
        // Add clips in non-sorted order
        let clip1 = Clip::new(1, PathBuf::from("test1.mp4"), time::from_seconds(0.0), time::from_seconds(5.0), time::from_seconds(20.0), 0);
        let clip2 = Clip::new(2, PathBuf::from("test2.mp4"), time::from_seconds(0.0), time::from_seconds(5.0), time::from_seconds(0.0), 0);
        let clip3 = Clip::new(3, PathBuf::from("test3.mp4"), time::from_seconds(0.0), time::from_seconds(5.0), time::from_seconds(10.0), 0);

        track.add_clip(clip1).unwrap();
        track.add_clip(clip2).unwrap();
        track.add_clip(clip3).unwrap();
        
        // Clips should be sorted by timeline_start
        assert_eq!(track.clips[0].id, 2); // timeline_start = 0
        assert_eq!(track.clips[1].id, 3); // timeline_start = 10
        assert_eq!(track.clips[2].id, 1); // timeline_start = 20
    }
}

