//! Timeline data structure managing video and audio tracks.
//! Per SPEC.md: Timeline → Tracks → Clips hierarchy.

use crate::timeline::track::{Track, TrackType, TrackError};
use crate::timeline::clip::{Clip, ClipId};
use crate::core::time::Time;

/// Main timeline structure.
/// 
/// Per SPEC.md: Timeline → Tracks → Clips hierarchy.
/// Contains one video track and one audio track.
/// All time values are in nanoseconds.
#[derive(Debug, Clone)]
pub struct Timeline {
    pub video_track: Track,
    pub audio_track: Track,
    pub duration: Time,       // Total timeline duration in nanoseconds
    pub playhead: Time,       // Current playhead position in nanoseconds
}

impl Timeline {
    /// Create a new timeline with empty video and audio tracks.
    pub fn new() -> Self {
        let video_track = Track::new(1, TrackType::Video);
        let audio_track = Track::new(2, TrackType::Audio);

        Self {
            video_track,
            audio_track,
            duration: 0,
            playhead: 0,
        }
    }

    /// Add a clip to the video track with overlap validation.
    /// 
    /// Returns `Ok(())` if successful, `Err(TrackError)` if the clip overlaps
    /// with existing clips. Updates timeline duration automatically.
    pub fn add_video_clip(&mut self, clip: Clip) -> Result<(), TrackError> {
        self.video_track.add_clip(clip)?;
        self.update_duration();
        Ok(())
    }

    /// Add a clip to the audio track with overlap validation.
    /// 
    /// Returns `Ok(())` if successful, `Err(TrackError)` if the clip overlaps
    /// with existing clips. Updates timeline duration automatically.
    pub fn add_audio_clip(&mut self, clip: Clip) -> Result<(), TrackError> {
        self.audio_track.add_clip(clip)?;
        self.update_duration();
        Ok(())
    }

    /// Remove a clip from the video track.
    /// 
    /// Returns the removed clip if found, `None` otherwise.
    /// Updates timeline duration automatically.
    pub fn remove_video_clip(&mut self, clip_id: ClipId) -> Option<Clip> {
        let result = self.video_track.remove_clip(clip_id);
        if result.is_some() {
            self.update_duration();
        }
        result
    }

    /// Remove a clip from the audio track.
    /// 
    /// Returns the removed clip if found, `None` otherwise.
    /// Updates timeline duration automatically.
    pub fn remove_audio_clip(&mut self, clip_id: ClipId) -> Option<Clip> {
        let result = self.audio_track.remove_clip(clip_id);
        if result.is_some() {
            self.update_duration();
        }
        result
    }

    /// Update the timeline duration based on track durations.
    /// 
    /// Duration is the maximum of video and audio track durations.
    fn update_duration(&mut self) {
        let video_duration = self.video_track.duration();
        let audio_duration = self.audio_track.duration();
        
        self.duration = video_duration.max(audio_duration);
    }

    /// Set the playhead position.
    /// 
    /// Playhead is clamped to [0, duration].
    pub fn set_playhead(&mut self, position: Time) {
        self.playhead = position.max(0).min(self.duration);
    }

    /// Get the video clip at the current playhead position.
    pub fn video_clip_at_playhead(&self) -> Option<&Clip> {
        self.video_track.clip_at(self.playhead)
    }

    /// Get the audio clip at the current playhead position.
    pub fn audio_clip_at_playhead(&self) -> Option<&Clip> {
        self.audio_track.clip_at(self.playhead)
    }

    /// Get all clips (video and audio) that overlap with a time range.
    /// 
    /// Returns a tuple of (video_clips, audio_clips).
    pub fn clips_in_range(&self, start: Time, end: Time) -> (Vec<&Clip>, Vec<&Clip>) {
        (
            self.video_track.clips_in_range(start, end),
            self.audio_track.clips_in_range(start, end),
        )
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::time;

    #[test]
    fn test_timeline_creation() {
        let timeline = Timeline::new();
        assert_eq!(timeline.playhead, 0);
        assert_eq!(timeline.duration, 0);
    }

    #[test]
    fn test_add_clip() {
        let mut timeline = Timeline::new();
        
        let clip = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(0.0),
            0,
        );

        timeline.add_video_clip(clip).unwrap();
        assert_eq!(timeline.video_track.clips.len(), 1);
        assert!(timeline.duration > 0);
    }

    #[test]
    fn test_playhead_clamping() {
        let mut timeline = Timeline::new();
        
        let clip = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(0.0),
            0,
        );

        timeline.add_video_clip(clip).unwrap();
        timeline.set_playhead(time::from_seconds(10.0));
        
        // Playhead should be clamped to duration
        assert!(timeline.playhead <= timeline.duration);
        assert_eq!(timeline.playhead, timeline.duration);
    }

    #[test]
    fn test_duration_updates() {
        let mut timeline = Timeline::new();
        
        let clip1 = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(2.0),
            time::from_seconds(0.0),
            0,
        );
        
        timeline.add_video_clip(clip1).unwrap();
        let duration_after_first = timeline.duration;
        
        let clip2 = Clip::new(
            2,
            std::path::PathBuf::from("test2.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(3.0),
            time::from_seconds(5.0),
            0,
        );
        
        timeline.add_audio_clip(clip2).unwrap();
        
        // Duration should be updated to include longer audio clip
        assert!(timeline.duration > duration_after_first);
        assert_eq!(timeline.duration, time::from_seconds(8.0));
    }

    #[test]
    fn test_overlap_validation() {
        let mut timeline = Timeline::new();
        
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

        // First clip should be added successfully
        assert!(timeline.add_video_clip(clip1).is_ok());
        
        // Second clip overlaps, should fail
        assert!(timeline.add_video_clip(clip2).is_err());
        
        // Video track should still have only 1 clip
        assert_eq!(timeline.video_track.clips.len(), 1);
    }

    #[test]
    fn test_clips_in_range() {
        let mut timeline = Timeline::new();
        
        let video_clip = Clip::new(
            1,
            std::path::PathBuf::from("video.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(5.0),
            time::from_seconds(0.0),
            0,
        );
        
        let audio_clip = Clip::new(
            2,
            std::path::PathBuf::from("audio.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(5.0),
            time::from_seconds(10.0),
            0,
        );

        timeline.add_video_clip(video_clip).unwrap();
        timeline.add_audio_clip(audio_clip).unwrap();
        
        // Range [2s, 12s] should include video clip and audio clip
        let (video_clips, audio_clips) = timeline.clips_in_range(
            time::from_seconds(2.0),
            time::from_seconds(12.0),
        );
        
        assert_eq!(video_clips.len(), 1);
        assert_eq!(audio_clips.len(), 1);
    }

    #[test]
    fn test_remove_clip() {
        let mut timeline = Timeline::new();
        
        let clip = Clip::new(
            1,
            std::path::PathBuf::from("test.mp4"),
            time::from_seconds(0.0),
            time::from_seconds(10.0),
            time::from_seconds(0.0),
            0,
        );

        timeline.add_video_clip(clip).unwrap();
        assert_eq!(timeline.video_track.clips.len(), 1);
        assert!(timeline.duration > 0);
        
        let removed = timeline.remove_video_clip(1);
        assert!(removed.is_some());
        assert_eq!(timeline.video_track.clips.len(), 0);
        assert_eq!(timeline.duration, 0);
    }

    #[test]
    fn test_playhead_negative() {
        let mut timeline = Timeline::new();
        timeline.set_playhead(time::from_seconds(-5.0));
        
        // Playhead should be clamped to 0
        assert_eq!(timeline.playhead, 0);
    }
}

