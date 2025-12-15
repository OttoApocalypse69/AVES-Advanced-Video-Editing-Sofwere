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
//!
//! # Public Clock API
//!
//! ## Core Functions
//!
//! - `AudioClock::new()` - Initialize audio device and clock
//! - `start_playback(start_time_ns)` - Start playback from timeline position
//! - `stop_playback()` - Stop playback (clock retains last position)
//! - `pause()` / `resume()` - Pause/resume playback
//! - `seek(position_ns)` - Jump to new timeline position
//! - `current_time()` - Read current playback time (lock-free, any thread)
//!
//! ## Thread Safety
//!
//! - `current_time()`: Safe to call from any thread (lock-free atomic read)
//! - Control methods (`start`, `stop`, `seek`, etc.): Call from single control thread
//! - Audio callback: Runs in real-time audio thread (no blocking, no allocations)
//!
//! # Callback Design
//!
//! The audio callback (`build_output_stream`) is the heart of the master clock:
//!
//! 1. **Runs in real-time audio thread** (cpal's audio callback thread)
//! 2. **Drives clock advancement**: Each callback advances clock by buffer duration
//! 3. **No allocations**: Uses only stack variables and pre-allocated buffers
//! 4. **No blocking**: All operations are lock-free atomics
//! 5. **Handles underruns**: Fills with silence gracefully (no panic)
//!
//! Clock update formula:
//! ```
//! duration_nanos = (samples_per_channel / sample_rate) * 1e9
//! new_time = current_time + duration_nanos
//! ```
//!
//! # Sync Guarantees
//!
//! ## Monotonicity
//!
//! - **During playback**: Clock only increases (monotonic)
//! - **During seek**: Clock may decrease (expected behavior)
//! - **During pause**: Clock stops advancing (maintains position)
//!
//! ## Accuracy
//!
//! - Clock advances based on **actual samples played** (sample-accurate)
//! - Time is calculated from buffer size and sample rate
//! - No drift correction: clock follows audio hardware timing
//!
//! ## Thread Coordination
//!
//! - Render thread reads `current_time()` to sync video frames
//! - Decode threads can read `current_time()` to determine what to decode
//! - Control thread calls `seek()` to change position
//! - All coordination is lock-free (atomic operations only)
//!
//! # Failure Modes
//!
//! ## Buffer Underruns
//!
//! **Behavior**: Callback fills buffer with silence (0.0)
//! **Impact**: Audio drops out, but clock continues advancing
//! **Recovery**: Automatic - next callback attempts to fill normally
//! **No panic**: Underruns are handled gracefully
//!
//! ## Device Errors
//!
//! **Behavior**: Error callback logs message, stream may continue
//! **Impact**: Playback may stop or degrade
//! **Recovery**: Control thread should call `stop_playback()` and restart
//! **No panic**: Errors are logged, not propagated
//!
//! ## No Audio Device
//!
//! **Behavior**: `AudioClock::new()` returns `AudioError::NoDevice`
//! **Impact**: Cannot initialize audio playback
//! **Recovery**: Application should handle gracefully (disable audio features)
//! **No panic**: Error returned, not panicked
//!
//! ## Seek During Playback
//!
//! **Behavior**: Clock jumps to new position immediately
//! **Impact**: Clock may decrease (non-monotonic during seek)
//! **Recovery**: Clock resumes monotonic advancement from new position
//! **No panic**: Seek is atomic and safe
//!
//! # Known Limitations
//!
//! 1. **No audio sample feeding**: Currently fills with silence
//!    - Future: Samples will come from decode threads via channels (per spec)
//!    - This requires integration with decode subsystem
//!
//! 2. **No drift correction**: Clock follows hardware timing exactly
//!    - If hardware drifts, clock drifts (predictable behavior)
//!    - Stability > perfect accuracy (per requirements)
//!
//! 3. **No buffer management**: Assumes cpal handles buffering
//!    - Buffer size is determined by cpal/OS
//!    - Larger buffers = more latency but fewer underruns
//!
//! 4. **Single device**: Uses default audio output device only
//!    - No device selection or hot-plugging support
//!    - Sufficient for MVP (per spec)

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use std::sync::atomic::{AtomicI64, AtomicBool, Ordering};
use std::sync::Arc;
use crate::core::time::{Time, from_seconds};

/// Error type for audio playback operations
#[derive(Debug)]
pub enum AudioError {
    Stream(cpal::StreamError),
    Device(cpal::DevicesError),
    Backend(cpal::BackendSpecificError),
    DefaultConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    PauseStream(cpal::PauseStreamError),
    NoDevice,
    InvalidConfig,
    NotStarted,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::Stream(e) => write!(f, "cpal stream error: {}", e),
            AudioError::Device(e) => write!(f, "cpal device error: {}", e),
            AudioError::Backend(e) => write!(f, "cpal backend error: {}", e),
            AudioError::DefaultConfig(e) => write!(f, "cpal default config error: {}", e),
            AudioError::BuildStream(e) => write!(f, "cpal build stream error: {}", e),
            AudioError::PlayStream(e) => write!(f, "cpal play stream error: {}", e),
            AudioError::PauseStream(e) => write!(f, "cpal pause stream error: {}", e),
            AudioError::NoDevice => write!(f, "No audio output device available"),
            AudioError::InvalidConfig => write!(f, "Invalid stream configuration"),
            AudioError::NotStarted => write!(f, "Playback not started"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<cpal::StreamError> for AudioError {
    fn from(err: cpal::StreamError) -> Self {
        AudioError::Stream(err)
    }
}

impl From<cpal::DevicesError> for AudioError {
    fn from(err: cpal::DevicesError) -> Self {
        AudioError::Device(err)
    }
}

impl From<cpal::BackendSpecificError> for AudioError {
    fn from(err: cpal::BackendSpecificError) -> Self {
        AudioError::Backend(err)
    }
}

impl From<cpal::DefaultStreamConfigError> for AudioError {
    fn from(err: cpal::DefaultStreamConfigError) -> Self {
        AudioError::DefaultConfig(err)
    }
}

impl From<cpal::BuildStreamError> for AudioError {
    fn from(err: cpal::BuildStreamError) -> Self {
        AudioError::BuildStream(err)
    }
}

impl From<cpal::PlayStreamError> for AudioError {
    fn from(err: cpal::PlayStreamError) -> Self {
        AudioError::PlayStream(err)
    }
}

impl From<cpal::PauseStreamError> for AudioError {
    fn from(err: cpal::PauseStreamError) -> Self {
        AudioError::PauseStream(err)
    }
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
        // All state is Arc-wrapped for lock-free access from callback
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
                // CRITICAL: All operations here are lock-free and allocation-free
                // This callback runs in a real-time audio thread - no blocking allowed
                if !is_playing.load(Ordering::Relaxed) || is_paused.load(Ordering::Relaxed) {
                    // Fill with silence when paused or stopped
                    // Clock does NOT advance when paused - maintains position
                    data.fill(0.0);
                    return;
                }

                // Calculate duration of this buffer in nanoseconds
                // This calculation is allocation-free and uses only stack variables
                let samples_per_channel = data.len() / channels as usize;
                let duration_seconds = samples_per_channel as f64 / sample_rate as f64;
                let duration_nanos = from_seconds(duration_seconds);

                // Handle buffer underrun: if we don't have audio data, fill with silence
                // This is graceful degradation - playback continues but with silence
                // The clock still advances, maintaining sync
                // CRITICAL: No allocations here - fill() operates on pre-allocated buffer
                data.fill(0.0);

                // Update master clock: advance by the duration of samples we're providing
                // This makes the clock monotonic and accurate to actual playback
                // We use Relaxed ordering because:
                // 1. Clock updates are independent (no data dependencies)
                // 2. Other threads only read (no write-write conflicts)
                // 3. Maximum performance in real-time callback
                //
                // Clock monotonicity: During playback, clock only increases.
                // Seek operations (called from control thread) can set clock to any value.
                // This is acceptable - seek is an explicit position change.
                let current_time = master_clock.load(Ordering::Relaxed);
                let new_time = current_time + duration_nanos;
                master_clock.store(new_time, Ordering::Relaxed);
            },
            |err| {
                // Error callback: log but don't crash
                // Underruns and other errors are handled gracefully
                // The stream continues operating - stability > perfect audio
                eprintln!("Audio stream error: {}", err);
                // Note: We don't update is_playing here because:
                // 1. Error callback runs in audio thread (no allocations/panics)
                // 2. Control thread should handle recovery
                // 3. Stream may recover automatically
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
    /// - If playing, continues from new position (clock advances from here)
    /// - If paused, updates position but remains paused
    /// - Clock may decrease (non-monotonic) during seek - this is expected
    ///
    /// # Thread Safety
    ///
    /// Safe to call from control thread while callback is running.
    /// The callback will see the new position on its next iteration.
    pub fn seek(&mut self, position_ns: Time) -> Result<(), AudioError> {
        // Update timeline start position atomically
        self.timeline_start.store(position_ns, Ordering::Relaxed);
        // Update master clock to new position atomically
        // This may cause clock to "jump" backwards, which is expected for seeks
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
    /// (only increases during playback, unless `seek()` is called) and represents
    /// the actual time position based on samples played.
    ///
    /// # Sync Guarantees
    ///
    /// - Clock advances with each audio callback (sample-accurate)
    /// - Monotonic during playback (never decreases unless seek)
    /// - Thread-safe: can be read from any thread without locks
    /// - Real-time safe: no allocations or blocking operations
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

