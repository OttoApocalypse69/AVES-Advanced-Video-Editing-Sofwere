//! Main application UI structure for the video editor.
//! Per SPEC_v1.0.md.md: UI Thread handles input & egui.
//! 
//! This implementation uses eframe::App for window management and event handling.

use eframe::egui::*;
use eframe::{App, CreationContext};
use crate::core::{Timeline, Clip, time};
use crate::ui::{TimelineViewState, timeline_ui};

/// Main editor application UI
/// Defines the primary layout with placeholder panels for a video editor interface
pub struct EditorApp {
    /// Timeline data structure containing tracks and clips
    pub timeline: Timeline,
    /// UI-specific view state for timeline visualization
    pub view_state: TimelineViewState,
}

impl EditorApp {
    /// Create a new editor application
    /// 
    /// Called by eframe during application initialization.
    /// The CreationContext provides access to egui context and wgpu resources.
    /// 
    /// Initializes a dummy timeline for testing with:
    /// - One video track with one clip
    /// - One audio track with one clip
    /// - Timebase: nanoseconds (1/1,000,000,000)
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        // Create a new timeline (already has video_track and audio_track)
        let mut timeline = Timeline::new();
        
        // Create a dummy video clip
        // Clip: 5 seconds duration, starts at timeline position 0
        // Source: from 0s to 5s in source file
        let video_clip = Clip::new(
            1, // clip id
            std::path::PathBuf::from("dummy_video.mp4"),
            time::from_seconds(0.0),      // in_point: start at 0s in source
            time::from_seconds(5.0),      // out_point: end at 5s in source (5s duration)
            time::from_seconds(0.0),      // timeline_start: place at 0s on timeline
            0,                            // stream_index: first video stream
        );
        
        // Create a dummy audio clip
        // Clip: 5 seconds duration, starts at timeline position 0
        // Source: from 0s to 5s in source file
        let audio_clip = Clip::new(
            2, // clip id
            std::path::PathBuf::from("dummy_audio.mp4"),
            time::from_seconds(0.0),      // in_point: start at 0s in source
            time::from_seconds(5.0),     // out_point: end at 5s in source (5s duration)
            time::from_seconds(0.0),     // timeline_start: place at 0s on timeline
            0,                            // stream_index: first audio stream
        );
        
        // Add clips to timeline
        // Note: These operations can fail if clips overlap, but our dummy clips
        // are at the same position which is valid (different tracks)
        timeline.add_video_clip(video_clip)
            .expect("Failed to add dummy video clip");
        timeline.add_audio_clip(audio_clip)
            .expect("Failed to add dummy audio clip");
        
        // Initialize view state with default values
        let view_state = TimelineViewState::default();
        
        Self {
            timeline,
            view_state,
        }
    }
}

impl App for EditorApp {
    /// Update the UI each frame
    /// This method builds the UI layout with all panels
    /// 
    /// Called by eframe each frame to render the UI.
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Top panel: Menu bar
        TopBottomPanel::top("menu_bar")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Menu Bar");
                    // Menu items will be added here in the future
                });
            });

        // Left panel: Media Pool
        SidePanel::left("media_pool")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.heading("Media Pool");
                    // Media pool content will be added here in the future
                });
            });

        // Bottom panel: Timeline
        // Per SPEC_v1.0.md.md: Timeline → Tracks → Clips hierarchy
        TopBottomPanel::bottom("timeline")
            .resizable(true)
            .default_height(200.0)
            .show(ctx, |ui| {
                // Call the timeline_ui function to render the timeline
                // Pass self.timeline and self.view_state as required
                timeline_ui(ui, &self.timeline, &mut self.view_state);
            });

        // Central panel: Program Viewer
        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Program Viewer");
                // Video preview will be rendered here in the future
            });
        });
    }
}
