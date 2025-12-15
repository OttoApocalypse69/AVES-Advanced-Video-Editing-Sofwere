//! Main playback engine coordinating audio and video playback.
//! Uses crossbeam channels for thread communication per SPEC.md

use crossbeam::channel;
use std::thread;
use crate::timeline::Timeline;
use crate::core::time::Time;
use crate::audio::player::AudioPlayer;
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
    Seek(Time),  // nanoseconds
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
    ) -> Result<Self, PlaybackError> {
        let audio_player = AudioPlayer::new(timeline.clone())?;
        let sync_controller = SyncController::new();
        let frame_cache = FrameCache::default();

        Ok(Self {
            timeline,
            state: PlaybackState::Stopped,
            audio_player,
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
        let (command_tx, _command_rx) = channel::unbounded::<PlaybackCommand>();
        let (_response_tx, response_rx) = channel::unbounded::<PlaybackResponse>();

        self.command_tx = Some(command_tx);
        self.response_rx = Some(response_rx);

        // Start video thread
        let master_clock = self.sync_controller.master_clock();
        let _frame_cache = self.frame_cache.clone();

        let video_thread = thread::spawn(move || {
            // Video rendering loop
            loop {
                // Read master clock (use Acquire for proper synchronization)
                let _elapsed_ns = master_clock.load(std::sync::atomic::Ordering::Acquire);
                
                // TODO: Get current timeline position, find clip, decode frame, render
                // Rendering is done via the Renderer which is owned by the main thread
                // Video frames are sent via channel to the render thread
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
