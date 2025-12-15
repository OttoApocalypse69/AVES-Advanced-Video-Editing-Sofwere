//! Audio mixing and synchronization.
//! Per SPEC.md: Master clock is audio playback, audio drives timing.

use crate::timeline::Timeline;
use crate::core::time::Time;
use crate::audio::buffer::AudioBuffer;
use crate::decode::decoder::Decoder;

/// Error type for audio mixing operations
#[derive(Debug)]
pub enum MixerError {
    Decode(crate::decode::decoder::DecodeError),
    NoClip,
}

impl std::fmt::Display for MixerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MixerError::Decode(e) => write!(f, "Decode error: {}", e),
            MixerError::NoClip => write!(f, "No audio clip at position"),
        }
    }
}

impl std::error::Error for MixerError {}

impl From<crate::decode::decoder::DecodeError> for MixerError {
    fn from(err: crate::decode::decoder::DecodeError) -> Self {
        MixerError::Decode(err)
    }
}

/// Audio mixer that combines audio from timeline tracks
pub struct AudioMixer {
    pub timeline: Timeline,
    pub sample_rate: u32,
    pub channels: u32,
}

impl AudioMixer {
    /// Create a new audio mixer
    pub fn new(timeline: Timeline, sample_rate: u32, channels: u32) -> Self {
        Self {
            timeline,
            sample_rate,
            channels,
        }
    }

    /// Get audio samples for a specific time range
    /// Returns interleaved PCM f32 samples (per SPEC.md)
    pub fn get_samples(
        &mut self,
        start_time: Time,
        duration_nanos: Time,
        decoders: &mut std::collections::HashMap<std::path::PathBuf, Decoder>,
    ) -> Result<AudioBuffer, MixerError> {
        let duration_seconds = crate::core::time::to_seconds(duration_nanos);
        let num_samples = (duration_seconds * self.sample_rate as f64) as usize;
        let mut buffer = AudioBuffer::with_capacity(
            self.sample_rate,
            self.channels,
            crate::audio::buffer::SampleFormat::F32,
            num_samples * self.channels as usize,
            start_time,
        );

        // Get the audio clip at the start time
        if let Some(clip) = self.timeline.audio_track.clip_at(start_time) {
            // Get decoder for this clip's source
            let _decoder = decoders
                .entry(clip.source_path.clone())
                .or_insert_with(|| {
                    Decoder::new(&clip.source_path)
                        .expect("Failed to create decoder")
                });

            // Convert timeline position to source position
            if let Some(_source_time) = clip.timeline_to_source(start_time) {
                // TODO: Decode audio samples from source
                // This would involve:
                // 1. Seeking decoder to source_time
                // 2. Decoding audio packets (returns AudioFrame with interleaved PCM f32)
                // 3. Resampling if needed
                // 4. Mixing with volume/mute settings
                
                // Placeholder: generate silence
                let silence = vec![0.0f32; num_samples * self.channels as usize];
                buffer.append(&silence);
            }
        } else {
            // No clip at this position - generate silence
            let silence = vec![0.0f32; num_samples * self.channels as usize];
            buffer.append(&silence);
        }

        // Apply track volume and mute
        if self.timeline.audio_track.muted {
            buffer.clear();
            // Re-fill with silence
            let silence = vec![0.0f32; num_samples * self.channels as usize];
            buffer.append(&silence);
        } else {
            for sample in buffer.as_mut_slice() {
                *sample *= self.timeline.audio_track.volume;
            }
        }

        Ok(buffer)
    }

    /// Update the timeline reference
    pub fn update_timeline(&mut self, timeline: Timeline) {
        self.timeline = timeline;
    }
}
