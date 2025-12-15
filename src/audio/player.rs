//! cpal audio playback integration.
//! Per SPEC.md: Audio playback is the MASTER CLOCK.
//! Video frames sync to audio clock.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, StreamConfig};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use crate::timeline::Timeline;
use crate::core::time::Time;
use crate::audio::mixer::{AudioMixer, MixerError};
use crate::decode::decoder::Decoder;

/// Error type for audio playback
#[derive(Debug)]
pub enum AudioPlayerError {
    Cpal(cpal::StreamError),
    DefaultConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    PauseStream(cpal::PauseStreamError),
    Mixer(MixerError),
    NoDevice,
}

impl std::fmt::Display for AudioPlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioPlayerError::Cpal(e) => write!(f, "cpal error: {}", e),
            AudioPlayerError::DefaultConfig(e) => write!(f, "cpal default config error: {}", e),
            AudioPlayerError::BuildStream(e) => write!(f, "cpal build stream error: {}", e),
            AudioPlayerError::PlayStream(e) => write!(f, "cpal play stream error: {}", e),
            AudioPlayerError::PauseStream(e) => write!(f, "cpal pause stream error: {}", e),
            AudioPlayerError::Mixer(e) => write!(f, "Mixer error: {}", e),
            AudioPlayerError::NoDevice => write!(f, "No audio device available"),
        }
    }
}

impl std::error::Error for AudioPlayerError {}

impl From<cpal::StreamError> for AudioPlayerError {
    fn from(err: cpal::StreamError) -> Self {
        AudioPlayerError::Cpal(err)
    }
}

impl From<cpal::DefaultStreamConfigError> for AudioPlayerError {
    fn from(err: cpal::DefaultStreamConfigError) -> Self {
        AudioPlayerError::DefaultConfig(err)
    }
}

impl From<cpal::BuildStreamError> for AudioPlayerError {
    fn from(err: cpal::BuildStreamError) -> Self {
        AudioPlayerError::BuildStream(err)
    }
}

impl From<cpal::PlayStreamError> for AudioPlayerError {
    fn from(err: cpal::PlayStreamError) -> Self {
        AudioPlayerError::PlayStream(err)
    }
}

impl From<cpal::PauseStreamError> for AudioPlayerError {
    fn from(err: cpal::PauseStreamError) -> Self {
        AudioPlayerError::PauseStream(err)
    }
}

impl From<MixerError> for AudioPlayerError {
    fn from(err: MixerError) -> Self {
        AudioPlayerError::Mixer(err)
    }
}

/// Audio player using cpal
/// This is the MASTER CLOCK for the entire application (per SPEC.md)
pub struct AudioPlayer {
    _host: Host,
    device: Device,
    stream_config: StreamConfig,
    mixer: AudioMixer,
    stream: Option<cpal::Stream>,
    // Master clock: current playback time in nanoseconds (per SPEC.md)
    // This drives video synchronization
    master_clock: Arc<AtomicI64>,
    playback_start: Option<Instant>,
    timeline_start_position: Time,
    _decoders: std::collections::HashMap<std::path::PathBuf, Decoder>,
}

impl AudioPlayer {
    /// Create a new audio player
    pub fn new(timeline: Timeline) -> Result<Self, AudioPlayerError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioPlayerError::NoDevice)?;

        let default_config = device.default_output_config()?;
        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels() as u32;
        let stream_config = StreamConfig::from(default_config);

        let mixer = AudioMixer::new(timeline, sample_rate, channels);

        Ok(Self {
            _host: host,
            device,
            stream_config,
            mixer,
            stream: None,
            master_clock: Arc::new(AtomicI64::new(0)),
            playback_start: None,
            timeline_start_position: 0,
            _decoders: std::collections::HashMap::new(),
        })
    }

    /// Start audio playback
    /// This starts the MASTER CLOCK (per SPEC.md)
    pub fn play(&mut self, timeline_position: Time) -> Result<(), AudioPlayerError> {
        if self.stream.is_some() {
            self.stop()?;
        }

        self.timeline_start_position = timeline_position;
        self.playback_start = Some(Instant::now());
        self.master_clock.store(timeline_position, Ordering::Relaxed);

        let master_clock = Arc::clone(&self.master_clock);
        let sample_rate = self.stream_config.sample_rate.0;
        let stream_config_clone = self.stream_config.clone();

        let stream = self.device.build_output_stream(
            &self.stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // This callback is the MASTER CLOCK (per SPEC.md)
                // Calculate current playback time in nanoseconds
                let current_time = master_clock.load(Ordering::Relaxed);
                
                // Request samples from mixer
                let samples_needed = data.len();
                let duration_seconds = samples_needed as f64 / (sample_rate * stream_config_clone.channels as u32) as f64;
                let duration_nanos = crate::core::time::from_seconds(duration_seconds);
                
                // TODO: Get actual samples from mixer
                // For now, generate silence
                data.fill(0.0);
                
                // Update master clock (advance time by duration of this buffer)
                let new_time = current_time + duration_nanos;
                master_clock.store(new_time, Ordering::Relaxed);
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);

        Ok(())
    }

    /// Stop audio playback
    pub fn stop(&mut self) -> Result<(), AudioPlayerError> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        self.playback_start = None;
        self.master_clock.store(0, Ordering::Relaxed);
        Ok(())
    }

    /// Pause audio playback
    pub fn pause(&mut self) -> Result<(), AudioPlayerError> {
        if let Some(stream) = &self.stream {
            stream.pause()?;
        }
        Ok(())
    }

    /// Resume audio playback
    pub fn resume(&mut self) -> Result<(), AudioPlayerError> {
        if let Some(stream) = &self.stream {
            stream.play()?;
        }
        Ok(())
    }

    /// Get the current master clock value (nanoseconds)
    /// VIDEO THREADS SYNC TO THIS VALUE (per SPEC.md)
    pub fn master_clock(&self) -> Time {
        self.master_clock.load(Ordering::Relaxed)
    }

    /// Get the current timeline position based on playback
    pub fn current_timeline_position(&self) -> Time {
        self.master_clock.load(Ordering::Relaxed)
    }

    /// Update the timeline
    pub fn update_timeline(&mut self, timeline: Timeline) {
        self.mixer.update_timeline(timeline);
    }

    /// Seek to a new timeline position
    pub fn seek(&mut self, position: Time) -> Result<(), AudioPlayerError> {
        self.timeline_start_position = position;
        self.master_clock.store(position, Ordering::Relaxed);
        if self.playback_start.is_some() {
            self.playback_start = Some(Instant::now());
        }
        Ok(())
    }
}
