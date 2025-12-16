//! UI module for egui-based video editor interface.
//! Per SPEC_v1.0.md.md: UI Thread handles input & egui.

pub mod timeline_view;
pub mod app;

pub use timeline_view::{TimelineView, timeline_ui};
pub use app::EditorApp;

/// UI state for the timeline view
/// Manages zoom level and pan position for timeline visualization
#[derive(Debug, Clone)]
pub struct TimelineViewState {
    /// Zoom level (1.0 = normal, 2.0 = 2x zoom, 0.5 = half zoom)
    pub zoom: f32,
    /// Pan position in nanoseconds (offset from timeline start)
    pub pan_nanos: i64,
}

impl Default for TimelineViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_nanos: 0,
        }
    }
}

