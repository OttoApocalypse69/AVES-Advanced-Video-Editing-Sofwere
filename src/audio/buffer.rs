//! Audio buffer management for playback.
//! Per SPEC.md: Audio decode output is interleaved PCM f32

use crate::core::time::Time;

/// Audio sample format (per SPEC.md: interleaved PCM f32)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    F32,  // Primary format per SPEC
    I16,  // Optional for compatibility
    I32,  // Optional for compatibility
}

/// Audio buffer containing samples
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub data: Vec<f32>,  // Interleaved samples (L, R, L, R, ...) - per SPEC.md
    pub sample_rate: u32,
    pub channels: u32,
    pub format: SampleFormat,
    pub timestamp: Time,  // Timestamp of first sample in nanoseconds
}

impl AudioBuffer {
    /// Create a new audio buffer
    pub fn new(sample_rate: u32, channels: u32, format: SampleFormat, timestamp: Time) -> Self {
        Self {
            data: Vec::new(),
            sample_rate,
            channels,
            format,
            timestamp,
        }
    }

    /// Create a buffer with a specific capacity
    pub fn with_capacity(
        sample_rate: u32,
        channels: u32,
        format: SampleFormat,
        capacity: usize,
        timestamp: Time,
    ) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            sample_rate,
            channels,
            format,
            timestamp,
        }
    }

    /// Get the number of samples (per channel)
    pub fn sample_count(&self) -> usize {
        self.data.len() / self.channels as usize
    }

    /// Get the duration in nanoseconds
    pub fn duration(&self) -> Time {
        let samples = self.sample_count() as f64;
        let duration_seconds = samples / self.sample_rate as f64;
        crate::core::time::from_seconds(duration_seconds)
    }

    /// Append samples to the buffer
    pub fn append(&mut self, samples: &[f32]) {
        self.data.extend_from_slice(samples);
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Get a slice of the audio data
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Get a mutable slice of the audio data
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }
}
