//! Export pipeline for rendering timeline to MP4 file.

use std::path::Path;
use crate::timeline::Timeline;
use crate::core::time::{Time, ns_to_seconds, seconds_to_ns};
use crate::export::encoder::{Encoder, EncodeError};
use crate::decode::decoder::{Decoder, DecodeError};

/// Error type for export operations
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("Encode error: {0}")]
    Encode(#[from] EncodeError),
    #[error("Decode error: {0}")]
    Decode(#[from] DecodeError),
    #[error("Timeline error: {0}")]
    Timeline(String),
}

/// Export settings
#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_bitrate: u64,  // bits per second
    pub audio_bitrate: u64,  // bits per second
    pub sample_rate: u32,
    pub channels: u32,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30.0,
            video_bitrate: 5_000_000,  // 5 Mbps
            audio_bitrate: 192_000,     // 192 kbps
            sample_rate: 48000,
            channels: 2,
        }
    }
}

/// Export pipeline for rendering timeline to MP4
pub struct ExportPipeline {
    timeline: Timeline,
    settings: ExportSettings,
}

impl ExportPipeline {
    /// Create a new export pipeline
    pub fn new(timeline: Timeline, settings: ExportSettings) -> Self {
        Self {
            timeline,
            settings,
        }
    }

    /// Export the timeline to an MP4 file
    pub fn export<P: AsRef<Path>>(&self, output_path: P) -> Result<(), ExportError> {
        let output_path = output_path.as_ref();

        // Create encoder
        let mut encoder = Encoder::new(
            output_path,
            self.settings.width,
            self.settings.height,
            self.settings.fps,
            self.settings.video_bitrate,
            self.settings.audio_bitrate,
            self.settings.sample_rate,
            self.settings.channels,
        )?;

        // Get timeline duration in nanoseconds
        let duration_ns = self.timeline.duration;
        let duration_seconds = ns_to_seconds(duration_ns);
        let frame_duration_seconds = 1.0 / self.settings.fps;
        let frame_duration_ns = seconds_to_ns(frame_duration_seconds);
        let total_frames = (duration_seconds * self.settings.fps).ceil() as usize;

        // Create decoders for all source files
        let mut decoders: std::collections::HashMap<std::path::PathBuf, Decoder> = 
            std::collections::HashMap::new();

        // Collect all unique source paths
        let mut source_paths = std::collections::HashSet::new();
        for clip in &self.timeline.video_track.clips {
            source_paths.insert(clip.source_path.clone());
        }
        for clip in &self.timeline.audio_track.clips {
            source_paths.insert(clip.source_path.clone());
        }

        // Initialize decoders
        for path in &source_paths {
            decoders.insert(
                path.clone(),
                Decoder::new(path)
                    .map_err(|e| ExportError::Decode(e))?,
            );
        }

        // Export frame by frame (using nanosecond timestamps, not frame numbers)
        let mut timeline_time_ns: Time = 0;
        let mut frame_num = 0;
        
        while timeline_time_ns < duration_ns {
            // Get video frame
            if let Some(video_clip) = self.timeline.video_track.clip_at(timeline_time_ns) {
                if let Some(source_time_ns) = video_clip.timeline_to_source(timeline_time_ns) {
                    let decoder = decoders.get_mut(&video_clip.source_path)
                        .ok_or_else(|| ExportError::Timeline("Decoder not found".to_string()))?;
                    
                    // Decode frame
                    match decoder.decode_video_frame_at(source_time_ns, video_clip.stream_index) {
                        Ok(frame) => {
                            // TODO: Scale frame to export resolution if needed
                            encoder.encode_video_frame(&frame)?;
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to decode frame {}: {}", frame_num, e);
                            // Continue with next frame
                        }
                    }
                }
            } else {
                // No video clip at this time - encode black frame
                // TODO: Create black frame and encode
            }

            // Get audio samples for this frame duration
            // TODO: Decode and encode audio samples
            // This would involve:
            // 1. Finding audio clip at timeline_time_ns
            // 2. Decoding audio samples for frame_duration_seconds
            // 3. Resampling if needed
            // 4. Encoding audio samples

            // Progress reporting
            if frame_num % 30 == 0 {
                let progress = (ns_to_seconds(timeline_time_ns) / duration_seconds) * 100.0;
                eprintln!("Export progress: {:.1}%", progress);
            }

            // Advance to next frame
            timeline_time_ns += frame_duration_ns;
            frame_num += 1;
        }

        // Finalize encoding
        encoder.finish()?;

        Ok(())
    }

    /// Get export settings
    pub fn settings(&self) -> &ExportSettings {
        &self.settings
    }

    /// Get mutable export settings
    pub fn settings_mut(&mut self) -> &mut ExportSettings {
        &mut self.settings
    }
}
