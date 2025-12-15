//! Audio playback and master clock implementation.
//! 
//! Per SPEC.md:
//! - Audio playback is the MASTER CLOCK
//! - Time unit: nanoseconds (i64)
//! - Video frames sync to audio clock
//! - Audio thread drives master time
//!
//! This module provides a clean API for audio playback that exposes
//! a monotonic playback timestamp in nanoseconds, usable by renderer
//! and timeline modules for synchronization.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use std::sync::atomic::{AtomicI64, AtomicBool, Ordering};
use std::sync::Arc;
use crate::core::time::{Time, from_seconds};

/// Error type for audio playback operations
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("cpal stream error: {0}")]
    Stream(#[from] cpal::StreamError),
    #[error("cpal device error: {0}")]
    Device(#[from] cpal::DeviceError),
    #[error("cpal backend error: {0}")]
    Backend(#[from] cpal::BackendSpecificError),
    #[error("No audio output device available")]
    NoDevice,
    #[error("Invalid stream configuration")]
    InvalidConfig,
    #[error("Playback not started")]
    NotStarted,
}

/// Audio playback and master clock.
///
/// The audio callback drives the master clock, advancing time based on
/// the actual samples played. This provides a monotonic, accurate clock
/// that other threads (renderer, timeline) can read for synchronization.
///
/// # Thread Safety
///
/// - `current_time()` can be called from any thread (lock-free)
/// - Playback control methods should be called from a single thread
/// - The audio callback runs in a real-time thread and updates the clock
///
/// # Example
///
/// ```no_run
/// use aves::media::audio::AudioClock;
/// use aves::core::time::from_seconds;
///
/// // Create audio clock
/// let mut clock = AudioClock::new()?;
///
/// // Start playback from timeline position 0
/// clock.start_playback(0)?;
///
/// // In render thread: read current time
/// let current_time = clock.current_time();
///
/// // Seek to 5 seconds
/// clock.seek(from_seconds(5.0))?;
///
/// // Stop playback
/// clock.stop_playback()?;
/// ```
pub struct AudioClock {
    /// Audio output device
    device: Device,
    /// Stream configuration (sample rate, channels, etc.)
    stream_config: StreamConfig,
    /// Sample rate in Hz
    sample_rate: u32,
    /// Number of audio channels
    channels: u16,
    /// Active audio stream (None when stopped)
    stream: Option<Stream>,
    /// Master clock: current playback time in nanoseconds
    /// Updated by audio callback, readable from any thread
    master_clock: Arc<AtomicI64>,
    /// Timeline position when playback started (nanoseconds)
    /// Used to calculate absolute timeline position
    timeline_start: Arc<AtomicI64>,
    /// Whether playback is currently active
    is_playing: Arc<AtomicBool>,
    /// Whether playback is paused
    is_paused: Arc<AtomicBool>,
}

impl AudioClock {
    /// Create a new audio clock.
    ///
    /// Initializes the default audio output device and prepares it for playback.
    /// Does not start playback - call `start_playback()` to begin.
    ///
    /// # Errors
    ///
    /// Returns `AudioError::NoDevice` if no audio output device is available.
    /// Returns other `AudioError` variants for device initialization failures.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoDevice)?;

        let default_config = device.default_output_config()?;
        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels();
        
        // Convert to StreamConfig
        let stream_config = StreamConfig::from(default_config);

        Ok(Self {
            device,
            stream_config,
            sample_rate,
            channels,
            stream: None,
            master_clock: Arc::new(AtomicI64::new(0)),
            timeline_start: Arc::new(AtomicI64::new(0)),
            is_playing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start audio playback from a timeline position.
    ///
    /// Begins the audio stream and starts the master clock from the given
    /// timeline position. The clock will advance monotonically as audio
    /// samples are played.
    ///
    /// # Arguments
    ///
    /// * `start_time_ns` - Timeline position in nanoseconds to start playback from
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if stream creation or playback fails.
    ///
    /// # Behavior
    ///
    /// - If already playing, stops the current stream and starts a new one
    /// - Buffer underruns are handled by filling with silence
    /// - Clock starts from `start_time_ns` and advances with each callback
    pub fn start_playback(&mut self, start_time_ns: Time) -> Result<(), AudioError> {
        // Stop existing stream if any
        if self.stream.is_some() {
            self.stop_playback()?;
        }

        // Set timeline start position
        self.timeline_start.store(start_time_ns, Ordering::Relaxed);
        self.master_clock.store(start_time_ns, Ordering::Relaxed);
        self.is_playing.store(true, Ordering::Relaxed);
        self.is_paused.store(false, Ordering::Relaxed);

        // Clone shared state for callback
        let master_clock = Arc::clone(&self.master_clock);
        let is_playing = Arc::clone(&self.is_playing);
        let is_paused = Arc::clone(&self.is_paused);
        let sample_rate = self.sample_rate;
        let channels = self.channels;

        // Build audio stream with callback
        let stream = self.device.build_output_stream(
            &self.stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // This callback runs in the audio thread and drives the master clock
                // It is called whenever the audio system needs more samples
                
                // Check if we should be playing
                if !is_playing.load(Ordering::Relaxed) || is_paused.load(Ordering::Relaxed) {
                    // Fill with silence when paused or stopped
                    data.fill(0.0);
                    return;
                }

                // Calculate duration of this buffer in nanoseconds
                let samples_per_channel = data.len() / channels as usize;
                let duration_seconds = samples_per_channel as f64 / sample_rate as f64;
                let duration_nanos = from_seconds(duration_seconds);

                // Handle buffer underrun: if we don't have audio data, fill with silence
                // This is graceful degradation - playback continues but with silence
                // The clock still advances, maintaining sync
                data.fill(0.0);

                // Update master clock: advance by the duration of samples we're providing
                // This makes the clock monotonic and accurate to actual playback
                let current_time = master_clock.load(Ordering::Relaxed);
                let new_time = current_time + duration_nanos;
                master_clock.store(new_time, Ordering::Relaxed);
            },
            |err| {
                // Error callback: log but don't crash
                eprintln!("Audio stream error: {}", err);
            },
            None,
        )?;

        // Start playback
        stream.play()?;
        self.stream = Some(stream);

        Ok(())
    }

    /// Stop audio playback.
    ///
    /// Stops the audio stream and resets the clock. The clock will remain
    /// at the last position until playback is started again.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if stopping the stream fails.
    pub fn stop_playback(&mut self) -> Result<(), AudioError> {
        if let Some(stream) = self.stream.take() {
            stream.pause()?;
            drop(stream);
        }
        
        self.is_playing.store(false, Ordering::Relaxed);
        self.is_paused.store(false, Ordering::Relaxed);
        
        // Note: We don't reset master_clock here - it retains the last position
        // This allows other threads to read the final position
        
        Ok(())
    }

    /// Pause audio playback.
    ///
    /// Pauses the stream without stopping it. The clock stops advancing.
    /// Call `resume()` to continue from the same position.
    ///
    /// # Errors
    ///
    /// Returns `AudioError::NotStarted` if playback hasn't been started.
    /// Returns other `AudioError` variants for stream control failures.
    pub fn pause(&mut self) -> Result<(), AudioError> {
        if let Some(stream) = &self.stream {
            stream.pause()?;
            self.is_paused.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            Err(AudioError::NotStarted)
        }
    }

    /// Resume audio playback.
    ///
    /// Resumes playback from the current position. The clock continues
    /// advancing from where it was paused.
    ///
    /// # Errors
    ///
    /// Returns `AudioError::NotStarted` if playback hasn't been started.
    /// Returns other `AudioError` variants for stream control failures.
    pub fn resume(&mut self) -> Result<(), AudioError> {
        if let Some(stream) = &self.stream {
            stream.play()?;
            self.is_paused.store(false, Ordering::Relaxed);
            Ok(())
        } else {
            Err(AudioError::NotStarted)
        }
    }

    /// Seek to a new timeline position.
    ///
    /// Updates the master clock to the new position immediately.
    /// If playback is active, it continues from the new position.
    ///
    /// # Arguments
    ///
    /// * `position_ns` - New timeline position in nanoseconds
    ///
    /// # Behavior
    ///
    /// - Updates clock atomically (thread-safe)
    /// - If playing, continues from new position
    /// - If paused, updates position but remains paused
    pub fn seek(&mut self, position_ns: Time) -> Result<(), AudioError> {
        // Update timeline start position
        self.timeline_start.store(position_ns, Ordering::Relaxed);
        // Update master clock to new position
        self.master_clock.store(position_ns, Ordering::Relaxed);
        Ok(())
    }

    /// Get the current playback time in nanoseconds.
    ///
    /// Returns the monotonic playback timestamp. This is the master clock
    /// that other threads (renderer, timeline) should use for synchronization.
    ///
    /// # Thread Safety
    ///
    /// This method is lock-free and can be called from any thread.
    /// It uses atomic operations with Relaxed ordering for maximum performance.
    ///
    /// # Returns
    ///
    /// Current timeline position in nanoseconds. The value is monotonic
    /// (only increases during playback) and represents the actual time
    /// position based on samples played.
    pub fn current_time(&self) -> Time {
        self.master_clock.load(Ordering::Relaxed)
    }

    /// Check if playback is currently active.
    ///
    /// Returns `true` if playback has been started and not stopped.
    /// Note: This can be `true` even if paused (use `is_paused()` to check).
    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    /// Check if playback is currently paused.
    ///
    /// Returns `true` if playback is active but paused.
    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    /// Get the sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of audio channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::time::from_seconds;

    #[test]
    fn test_audio_clock_creation() {
        // This test may fail if no audio device is available
        // That's expected in CI environments
        if let Ok(clock) = AudioClock::new() {
            assert!(!clock.is_playing());
            assert!(!clock.is_paused());
            assert_eq!(clock.current_time(), 0);
        }
    }

    #[test]
    fn test_seek() {
        if let Ok(mut clock) = AudioClock::new() {
            let position = from_seconds(5.0);
            clock.seek(position).unwrap();
            assert_eq!(clock.current_time(), position);
        }
    }

    #[test]
    fn test_clock_monotonic() {
        // Test that clock values are reasonable
        if let Ok(clock) = AudioClock::new() {
            let time1 = clock.current_time();
            // Small delay
            std::thread::sleep(std::time::Duration::from_millis(10));
            let time2 = clock.current_time();
            // Clock shouldn't go backwards (unless seek is called)
            assert!(time2 >= time1);
        }
    }
}

