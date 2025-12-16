//! Offline export pipeline for rendering timeline to MP4 file.
//! Per SPEC.md: No real-time preview, no UI, offline render only.
//! Uses nanosecond time units (i64) throughout.

use std::path::Path;
use std::collections::{HashMap, HashSet};
use crate::core::Timeline;
use crate::core::time::{Time, from_seconds, to_seconds};
use crate::export::encoder::Encoder;
use crate::decode::decoder::{Decoder, DecodeError, VideoFrame};
use crate::export::pipeline::{ExportSettings, ExportError};

/// Exporter for offline rendering of timeline to MP4
/// 
/// This exporter performs frame-by-frame rendering of the timeline:
/// 1. Iterates through timeline time in nanoseconds at the target frame rate
/// 2. Decodes video frames from clips at each frame time
/// 3. Decodes and accumulates audio samples for each frame duration
/// 4. Encodes frames and samples to MP4 (H.264 + AAC)
/// 
/// Frame pacing: Each frame represents a fixed duration (1/fps seconds).
/// Timeline time advances by frame_duration_ns for each frame.
/// 
/// Sync behavior:
/// - Video frames are decoded at exact timeline timestamps
/// - Audio samples are accumulated per frame duration
/// - Audio/video sync is maintained by encoding audio samples that correspond
///   to each video frame's time range
/// - Frame-perfect output: every frame at the target FPS is encoded
/// 
/// Error handling:
/// - Decode errors for individual frames are logged and result in black frames
/// - Audio decode errors result in silence for that time range
/// - Encoder errors propagate and abort the export
/// - Timeline errors (missing decoders, invalid mappings) abort the export
/// 
/// Known limitations:
/// - Frame scaling is not implemented (relies on encoder)
/// - Audio resampling is not implemented (assumes source matches export settings)
/// - No support for multiple overlapping clips (takes first clip found)
/// - Audio mixing for overlapping clips is simplified (volume only)
pub struct Exporter {
    timeline: Timeline,
    settings: ExportSettings,
}

impl Exporter {
    /// Create a new exporter with timeline and export settings
    pub fn new(timeline: Timeline, settings: ExportSettings) -> Self {
        Self {
            timeline,
            settings,
        }
    }

    /// Export the timeline to an MP4 file
    /// 
    /// This performs offline rendering:
    /// - Iterates through timeline time at target frame rate
    /// - Decodes video frames from clips
    /// - Decodes and encodes audio samples
    /// - Writes to MP4 file via FFmpeg encoder
    /// 
    /// Frame pacing strategy:
    /// - Calculate frame duration: 1/fps seconds in nanoseconds
    /// - For each frame, advance timeline_time_ns by frame_duration_ns
    /// - Decode video frame at timeline_time_ns
    /// - Accumulate audio samples for frame_duration_ns duration
    /// - Encode when enough samples accumulated
    /// 
    /// Returns Ok(()) on success, Err(ExportError) on failure.
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

        // Calculate frame timing
        let duration_ns = self.timeline.duration;
        let frame_duration_seconds = 1.0 / self.settings.fps;
        let frame_duration_ns = from_seconds(frame_duration_seconds);
        
        // Calculate audio samples per frame
        let samples_per_frame = (self.settings.sample_rate as f64 * frame_duration_seconds) as usize;

        // Collect all unique source paths
        let mut source_paths = HashSet::new();
        for clip in &self.timeline.video_track.clips {
            source_paths.insert(clip.source_path.clone());
        }
        for clip in &self.timeline.audio_track.clips {
            source_paths.insert(clip.source_path.clone());
        }

        // Initialize decoders for all source files
        let mut decoders: HashMap<std::path::PathBuf, Decoder> = HashMap::new();
        for path in &source_paths {
            decoders.insert(
                path.clone(),
                Decoder::new(path)
                    .map_err(ExportError::Decode)?,
            );
        }

        // Audio sample accumulation buffer
        let mut audio_buffer: Vec<f32> = Vec::new();

        // Export frame by frame (using nanosecond timestamps, not frame numbers)
        let mut timeline_time_ns: Time = 0;
        let mut frame_num = 0;
        let total_frames = ((to_seconds(duration_ns) * self.settings.fps).ceil() as usize).max(1);
        
        while timeline_time_ns < duration_ns {
            // === VIDEO FRAME PROCESSING ===
            // Find video clip at current timeline time
            if let Some(video_clip) = self.timeline.video_track.clip_at(timeline_time_ns) {
                // Convert timeline time to source time
                if let Some(source_time_ns) = video_clip.timeline_to_source(timeline_time_ns) {
                    let decoder = decoders.get_mut(&video_clip.source_path)
                        .ok_or_else(|| ExportError::Timeline(
                            format!("Decoder not found for source: {:?}", video_clip.source_path)
                        ))?;
                    
                    // Decode video frame at source time
                    match decoder.decode_video_frame_at(source_time_ns, video_clip.stream_index) {
                        Ok(frame) => {
                            // Scale frame to export resolution if needed
                            let scaled_frame = self.scale_frame_if_needed(&frame)?;
                            
                            // Encode video frame
                            encoder.encode_video_frame(&scaled_frame)?;
                        }
                        Err(e) => {
                            // Log warning but continue - frame dropping allowed during export
                            eprintln!("Warning: Failed to decode video frame at {}: {}", 
                                     to_seconds(timeline_time_ns), e);
                            // Encode black frame as fallback
                            self.encode_black_frame(&mut encoder)?;
                        }
                    }
                } else {
                    // No valid source time mapping - encode black frame
                    self.encode_black_frame(&mut encoder)?;
                }
            } else {
                // No video clip at this time - encode black frame
                self.encode_black_frame(&mut encoder)?;
            }

            // === AUDIO SAMPLE PROCESSING ===
            // Accumulate audio samples for this frame duration
            let frame_end_time_ns = (timeline_time_ns + frame_duration_ns).min(duration_ns);
            
            // Find audio clips that overlap with this frame duration
            let audio_clips = self.timeline.audio_track.clips_in_range(
                timeline_time_ns,
                frame_end_time_ns
            );

            // Decode audio samples from overlapping clips
            for audio_clip in &audio_clips {
                // Calculate overlap range in timeline time
                let clip_start = audio_clip.timeline_start.max(timeline_time_ns);
                let clip_end = audio_clip.timeline_end.min(frame_end_time_ns);
                
                if clip_start < clip_end {
                    // Convert timeline times to source times
                    if let Some(source_start_ns) = audio_clip.timeline_to_source(clip_start) {
                        if let Some(source_end_ns) = audio_clip.timeline_to_source(clip_end) {
                            let decoder = decoders.get_mut(&audio_clip.source_path)
                                .ok_or_else(|| ExportError::Timeline(
                                    format!("Decoder not found for audio source: {:?}", audio_clip.source_path)
                                ))?;
                            
                            // Decode audio samples for this range
                            match self.decode_audio_range(
                                decoder,
                                source_start_ns,
                                source_end_ns,
                                audio_clip.stream_index,
                            ) {
                                Ok(samples) => {
                                    // Apply track volume if not muted
                                    let volume = if self.timeline.audio_track.muted {
                                        0.0
                                    } else {
                                        self.timeline.audio_track.volume
                                    };
                                    
                                    // Mix samples into buffer (apply volume)
                                    let mixed_samples: Vec<f32> = samples
                                        .iter()
                                        .map(|s| s * volume)
                                        .collect();
                                    
                                    audio_buffer.extend_from_slice(&mixed_samples);
                                }
                                Err(e) => {
                                    eprintln!("Warning: Failed to decode audio at {}: {}", 
                                             to_seconds(timeline_time_ns), e);
                                }
                            }
                        }
                    }
                }
            }

            // If no audio clips, add silence for this frame duration
            if audio_clips.is_empty() {
                let silence_samples = vec![0.0f32; samples_per_frame];
                audio_buffer.extend_from_slice(&silence_samples);
            }

            // Encode audio when we have enough samples
            // We encode in chunks matching the frame rate
            while audio_buffer.len() >= samples_per_frame {
                let samples_to_encode: Vec<f32> = audio_buffer
                    .drain(..samples_per_frame)
                    .collect();
                
                // Resample if needed (simplified - assumes decoder outputs correct sample rate)
                encoder.encode_audio_samples(&samples_to_encode)?;
            }

            // Progress reporting
            if frame_num % 30 == 0 {
                let progress = (to_seconds(timeline_time_ns) / to_seconds(duration_ns)) * 100.0;
                eprintln!("Export progress: {:.1}% (frame {}/{}), timeline: {:.3}s", 
                         progress, frame_num, total_frames, to_seconds(timeline_time_ns));
            }

            // Advance to next frame
            timeline_time_ns += frame_duration_ns;
            frame_num += 1;
        }

        // Flush remaining audio samples
        if !audio_buffer.is_empty() {
            // Pad with silence to match expected frame size
            while audio_buffer.len() < samples_per_frame {
                audio_buffer.push(0.0);
            }
            encoder.encode_audio_samples(&audio_buffer)?;
        }

        // Finalize encoding
        encoder.finish()?;

        eprintln!("Export complete: {} frames exported", frame_num);
        Ok(())
    }

    /// Scale frame to export resolution if dimensions don't match
    /// 
    /// Currently returns frame as-is. In a full implementation, this would
    /// use FFmpeg's sws_scale to resize RGBA8 frames.
    /// 
    /// Known limitation: Frame scaling is not implemented.
    fn scale_frame_if_needed(&self, frame: &VideoFrame) -> Result<VideoFrame, ExportError> {
        if frame.width == self.settings.width && frame.height == self.settings.height {
            return Ok(frame.clone());
        }

        // TODO: Implement frame scaling using FFmpeg sws_scale
        // This would convert RGBA8 frame to target resolution
        // For now, return frame as-is (encoder should handle scaling)
        // In production, this should scale RGBA8 frame to target resolution
        Ok(frame.clone())
    }

    /// Encode a black frame (used when no video clip is present)
    fn encode_black_frame(&self, encoder: &mut Encoder) -> Result<(), ExportError> {
        // Create black RGBA8 frame
        let black_video_frame = VideoFrame {
            data: vec![0u8; (self.settings.width * self.settings.height * 4) as usize],
            width: self.settings.width,
            height: self.settings.height,
            timestamp: 0, // Not used for encoding
        };
        
        encoder.encode_video_frame(&black_video_frame)
            .map_err(ExportError::Encode)
    }

    /// Decode audio samples for a time range
    /// Returns interleaved PCM f32 samples
    /// 
    /// Known limitation: Audio resampling is not implemented.
    /// Assumes source sample rate matches export settings.
    fn decode_audio_range(
        &self,
        decoder: &mut Decoder,
        start_time_ns: Time,
        end_time_ns: Time,
        stream_index: usize,
    ) -> Result<Vec<f32>, DecodeError> {
        // Seek to start time
        decoder.seek(start_time_ns, stream_index)?;
        
        let duration_seconds = to_seconds(end_time_ns - start_time_ns);
        let expected_samples = (self.settings.sample_rate as f64 * duration_seconds) as usize;
        let mut samples = Vec::with_capacity(expected_samples);
        
        // Decode audio frames until we have enough samples
        let mut current_time_ns = start_time_ns;
        while current_time_ns < end_time_ns {
            match decoder.decode_next_audio_frame(stream_index)? {
                Some(audio_frame) => {
                    // Check if frame is within our range
                    if audio_frame.timestamp >= end_time_ns {
                        break;
                    }
                    
                    // Calculate how many samples to take from this frame
                    let frame_start = audio_frame.timestamp.max(start_time_ns);
                    let frame_end = (audio_frame.timestamp + 
                        from_seconds(audio_frame.data.len() as f64 / 
                        (audio_frame.sample_rate * audio_frame.channels) as f64))
                        .min(end_time_ns);
                    
                    if frame_start < frame_end {
                        let frame_duration = to_seconds(frame_end - frame_start);
                        let samples_to_take = (audio_frame.sample_rate as f64 * 
                                              audio_frame.channels as f64 * 
                                              frame_duration) as usize;
                        
                        // Take samples from frame (simplified - assumes frame data matches)
                        let start_idx = ((to_seconds(frame_start - audio_frame.timestamp) * 
                                        audio_frame.sample_rate as f64 * 
                                        audio_frame.channels as f64) as usize)
                                        .min(audio_frame.data.len());
                        let end_idx = (start_idx + samples_to_take).min(audio_frame.data.len());
                        
                        // Resample if needed (simplified - assumes same sample rate)
                        if audio_frame.sample_rate == self.settings.sample_rate &&
                           audio_frame.channels == self.settings.channels {
                            samples.extend_from_slice(&audio_frame.data[start_idx..end_idx]);
                        } else {
                            // TODO: Implement resampling using FFmpeg swr_convert
                            // For now, just take samples as-is (will cause issues if rates differ)
                            eprintln!("Warning: Sample rate/channel mismatch - resampling not implemented");
                            samples.extend_from_slice(&audio_frame.data[start_idx..end_idx]);
                        }
                    }
                    
                    current_time_ns = audio_frame.timestamp + 
                        from_seconds(audio_frame.data.len() as f64 / 
                        (audio_frame.sample_rate * audio_frame.channels) as f64);
                }
                None => {
                    // No more frames - pad with silence
                    let remaining_samples = expected_samples.saturating_sub(samples.len());
                    samples.extend(vec![0.0f32; remaining_samples]);
                    break;
                }
            }
        }
        
        // Ensure we have the expected number of samples
        while samples.len() < expected_samples {
            samples.push(0.0);
        }
        samples.truncate(expected_samples);
        
        Ok(samples)
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
