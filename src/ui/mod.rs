//! UI module for egui-based video editor interface.
//! Per SPEC_v1.0.md.md: UI Thread handles input & egui.

pub mod timeline_view;
pub mod app;

pub use timeline_view::TimelineView;
pub use app::EditorApp;

