//! FFmpeg decoder wrapper with safe API.
//! All unsafe FFmpeg code is isolated in this module.
//! Per SPEC.md: Video decode output is RGBA8, Audio decode output is interleaved PCM f32

use std::path::{Path, PathBuf};
use crate::core::time::Time;
use crate::decode::stream_info::{VideoStreamInfo, AudioStreamInfo};

/// Error type for decoding operations
#[derive(Debug)]
pub enum DecodeError {
    FFmpeg(String),
    FileNotFound(PathBuf),
    NoVideoStream,
    NoAudioStream,
    InvalidStreamIndex(usize),
    SeekFailed,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::FFmpeg(msg) => write!(f, "FFmpeg error: {}", msg),
            DecodeError::FileNotFound(path) => write!(f, "File not found: {:?}", path),
            DecodeError::NoVideoStream => write!(f, "No video stream found"),
            DecodeError::NoAudioStream => write!(f, "No audio stream found"),
            DecodeError::InvalidStreamIndex(idx) => write!(f, "Invalid stream index: {}", idx),
            DecodeError::SeekFailed => write!(f, "Seek failed"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decoded video frame (RGBA8 as per SPEC.md)
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,      // Raw pixel data (RGBA8)
    pub width: u32,
    pub height: u32,
    pub timestamp: Time,    // Timestamp in nanoseconds
}

/// Decoded audio frame (interleaved PCM f32 as per SPEC.md)
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub data: Vec<f32>,     // Interleaved PCM samples (L, R, L, R, ...)
    pub sample_rate: u32,
    pub channels: u32,
    pub timestamp: Time,    // Timestamp in nanoseconds
}

/// Safe wrapper around FFmpeg decoder
/// All unsafe FFmpeg operations are contained within this struct
pub struct Decoder {
    _path: PathBuf,
    // FFmpeg context would be stored here as an opaque pointer
    // For now, we'll use a placeholder structure
    // In real implementation, this would be: inner: *mut FFmpegContext
    _inner: (),  // Placeholder - would be FFmpeg context
}

impl Decoder {
    /// Create a new decoder for a media file
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, DecodeError> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(DecodeError::FileNotFound(path.to_path_buf()));
        }

        // TODO: Initialize FFmpeg context
        // This would involve unsafe FFmpeg API calls:
        // - avformat_open_input
        // - avformat_find_stream_info
        // - avcodec_find_decoder
        // - avcodec_open2
        
        Ok(Self {
            _path: path.to_path_buf(),
            _inner: (),
        })
    }

    /// Get video stream information
    pub fn get_video_stream_info(&self, _stream_index: usize) -> Result<VideoStreamInfo, DecodeError> {
        // TODO: Extract video stream info from FFmpeg context
        // This would involve unsafe FFmpeg API calls to read codec parameters
        
        // Placeholder implementation
        Err(DecodeError::NoVideoStream)
    }

    /// Get audio stream information
    pub fn get_audio_stream_info(&self, _stream_index: usize) -> Result<AudioStreamInfo, DecodeError> {
        // TODO: Extract audio stream info from FFmpeg context
        
        // Placeholder implementation
        Err(DecodeError::NoAudioStream)
    }

    /// Find the first video stream index
    pub fn find_video_stream(&self) -> Result<usize, DecodeError> {
        // TODO: Iterate through streams and find video stream
        // This would involve unsafe FFmpeg API calls
        
        // Placeholder implementation
        Err(DecodeError::NoVideoStream)
    }

    /// Find the first audio stream index
    pub fn find_audio_stream(&self) -> Result<usize, DecodeError> {
        // TODO: Iterate through streams and find audio stream
        
        // Placeholder implementation
        Err(DecodeError::NoAudioStream)
    }

    /// Seek to a specific timestamp in the source (nanoseconds)
    pub fn seek(&mut self, _timestamp: Time, _stream_index: usize) -> Result<(), DecodeError> {
        // TODO: Implement FFmpeg seeking
        // This would involve unsafe FFmpeg API calls:
        // - av_seek_frame or avformat_seek_file
        // Need to convert nanoseconds to FFmpeg timebase units
        
        // Placeholder implementation
        Ok(())
    }

    /// Decode the next video frame from the specified stream
    /// Returns RGBA8 format as per SPEC.md
    pub fn decode_next_video_frame(&mut self, _stream_index: usize) -> Result<Option<VideoFrame>, DecodeError> {
        // TODO: Implement video frame decoding
        // This would involve unsafe FFmpeg API calls:
        // - av_read_frame
        // - avcodec_send_packet
        // - avcodec_receive_frame
        // - sws_scale (for format conversion to RGBA8)
        // Convert FFmpeg timestamp to nanoseconds
        
        // Placeholder implementation
        Ok(None)
    }

    /// Decode the next audio frame from the specified stream
    /// Returns interleaved PCM f32 as per SPEC.md
    pub fn decode_next_audio_frame(&mut self, _stream_index: usize) -> Result<Option<AudioFrame>, DecodeError> {
        // TODO: Implement audio frame decoding
        // This would involve unsafe FFmpeg API calls:
        // - av_read_frame
        // - avcodec_send_packet
        // - avcodec_receive_frame
        // - swr_convert (for format conversion to f32 PCM)
        // Convert FFmpeg timestamp to nanoseconds
        
        // Placeholder implementation
        Ok(None)
    }

    /// Decode a video frame at a specific timestamp (nanoseconds)
    /// This will seek to the timestamp and decode the frame
    pub fn decode_video_frame_at(&mut self, timestamp: Time, stream_index: usize) -> Result<VideoFrame, DecodeError> {
        // Seek to the timestamp
        self.seek(timestamp, stream_index)?;
        
        // Decode the frame
        match self.decode_next_video_frame(stream_index)? {
            Some(frame) => Ok(frame),
            None => Err(DecodeError::FFmpeg("No frame found at timestamp".to_string())),
        }
    }

    /// Decode an audio frame at a specific timestamp (nanoseconds)
    pub fn decode_audio_frame_at(&mut self, timestamp: Time, stream_index: usize) -> Result<AudioFrame, DecodeError> {
        // Seek to the timestamp
        self.seek(timestamp, stream_index)?;
        
        // Decode the frame
        match self.decode_next_audio_frame(stream_index)? {
            Some(frame) => Ok(frame),
            None => Err(DecodeError::FFmpeg("No audio frame found at timestamp".to_string())),
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // TODO: Clean up FFmpeg resources
        // This would involve unsafe FFmpeg API calls:
        // - avcodec_free_context
        // - avformat_close_input
    }
}

// Note: In a real implementation, we would have:
// 
// struct FFmpegContext {
//     format_ctx: *mut AVFormatContext,
//     codec_ctxs: Vec<*mut AVCodecContext>,
//     sws_ctx: *mut SwsContext,  // For video scaling/conversion
//     swr_ctx: *mut SwrContext,  // For audio resampling/conversion
//     // ... other FFmpeg structures
// }
//
// All FFmpeg operations would be wrapped in unsafe blocks within this module.
// The public API (Decoder) would remain safe.
