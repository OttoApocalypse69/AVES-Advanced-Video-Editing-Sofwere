//! Main playback engine coordinating audio and video playback.
//! Uses crossbeam channels for thread communication per SPEC.md

use crossbeam::channel;
use std::thread;
use crate::core::timeline::Timeline;
use crate::core::time::Timestamp;
use crate::audio::player::AudioPlayer;
use crate::render::compositor::Compositor;
use crate::decode::decoder::Decoder;
use crate::decode::frame_cache::FrameCache;
use crate::playback::state::PlaybackState;
use crate::playback::sync::SyncController;

/// Command sent to playback engine
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Stop,
    Seek(Timestamp),  // nanoseconds
    UpdateTimeline(Timeline),
}

/// Response from playback engine
#[derive(Debug, Clone)]
pub enum PlaybackResponse {
    StateChanged(PlaybackState),
    Error(String),
}

/// Error type for playback engine
#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("Audio error: {0}")]
    Audio(#[from] crate::audio::player::AudioPlayerError),
    #[error("Render error: {0}")]
    Render(#[from] crate::render::compositor::CompositorError),
    #[error("Decode error: {0}")]
    Decode(#[from] crate::decode::decoder::DecodeError),
    #[error("Thread error: {0}")]
    Thread(String),
}

/// Main playback engine
pub struct PlaybackEngine {
    timeline: Timeline,
    state: PlaybackState,
    audio_player: AudioPlayer,
    compositor: Compositor,
    sync_controller: SyncController,
    frame_cache: FrameCache,
    decoders: std::collections::HashMap<std::path::PathBuf, Decoder>,
    command_tx: Option<channel::Sender<PlaybackCommand>>,
    response_rx: Option<channel::Receiver<PlaybackResponse>>,
    video_thread_handle: Option<thread::JoinHandle<()>>,
}

impl PlaybackEngine {
    /// Create a new playback engine
    pub fn new(
        timeline: Timeline,
        compositor: Compositor,
    ) -> Result<Self, PlaybackError> {
        let audio_player = AudioPlayer::new(timeline.clone())?;
        let sync_controller = SyncController::new();
        let frame_cache = FrameCache::default();

        Ok(Self {
            timeline,
            state: PlaybackState::Stopped,
            audio_player,
            compositor,
            sync_controller,
            frame_cache,
            decoders: std::collections::HashMap::new(),
            command_tx: None,
            response_rx: None,
            video_thread_handle: None,
        })
    }

    /// Start the playback engine
    pub fn start(&mut self) -> Result<(), PlaybackError> {
        let (command_tx, command_rx) = channel::unbounded();
        let (response_tx, response_rx) = channel::unbounded();

        self.command_tx = Some(command_tx);
        self.response_rx = Some(response_rx);

        // Start video thread
        let master_clock = self.sync_controller.master_clock();
        let mut compositor = self.compositor;
        let mut frame_cache = self.frame_cache.clone();
        let mut decoders = std::collections::HashMap::new();

        let video_thread = thread::spawn(move || {
            // Video rendering loop
            loop {
                // Read master clock
                let elapsed_ns = master_clock.load(std::sync::atomic::Ordering::Relaxed);
                
                // TODO: Get current timeline position, find clip, decode frame, render
                // For now, just sleep to prevent busy-waiting
                thread::sleep(std::time::Duration::from_millis(16)); // ~60 FPS
            }
        });

        self.video_thread_handle = Some(video_thread);

        Ok(())
    }

    /// Process a playback command
    pub fn process_command(&mut self, command: PlaybackCommand) -> Result<(), PlaybackError> {
        match command {
            PlaybackCommand::Play => {
                let position = match &self.state {
                    PlaybackState::Paused { timeline_position } => *timeline_position,
                    _ => self.timeline.playhead,
                };
                
                self.sync_controller.start(position);
                self.audio_player.play(position)?;
                
                self.state = PlaybackState::Playing {
                    start_time: std::time::Instant::now(),
                    timeline_start: position,
                };
            }
            PlaybackCommand::Pause => {
                let position = self.sync_controller.current_timeline_position();
                self.audio_player.pause()?;
                self.state = PlaybackState::Paused {
                    timeline_position: position,
                };
            }
            PlaybackCommand::Stop => {
                self.audio_player.stop()?;
                self.sync_controller.stop();
                self.state = PlaybackState::Stopped;
            }
            PlaybackCommand::Seek(position) => {
                self.timeline.set_playhead(position);
                self.sync_controller.seek(position);
                self.audio_player.seek(position)?;
                self.state = PlaybackState::Seeking { target: position };
            }
            PlaybackCommand::UpdateTimeline(timeline) => {
                self.timeline = timeline.clone();
                self.audio_player.update_timeline(timeline);
            }
        }

        Ok(())
    }

    /// Get the current playback state
    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    /// Get the timeline
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// Get mutable timeline reference
    pub fn timeline_mut(&mut self) -> &mut Timeline {
        &mut self.timeline
    }

    /// Update the playhead based on current playback
    pub fn update_playhead(&mut self) {
        if self.state.is_playing() {
            let position = self.sync_controller.current_timeline_position();
            self.timeline.set_playhead(position);
        }
    }
}

impl Drop for PlaybackEngine {
    fn drop(&mut self) {
        // Stop playback and clean up threads
        let _ = self.audio_player.stop();
        if let Some(handle) = self.video_thread_handle.take() {
            let _ = handle.join();
        }
    }
}
