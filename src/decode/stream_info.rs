//! Stream metadata information extracted from media files.

use crate::core::time::Time;

/// Information about a video or audio stream
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub index: usize,
    pub duration: Time,  // Duration in nanoseconds
    pub codec_name: String,
}

/// Video-specific stream information
#[derive(Debug, Clone)]
pub struct VideoStreamInfo {
    pub stream_info: StreamInfo,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub pixel_format: String,
}

/// Audio-specific stream information
#[derive(Debug, Clone)]
pub struct AudioStreamInfo {
    pub stream_info: StreamInfo,
    pub sample_rate: u32,
    pub channels: u32,
    pub sample_format: String,
}
