//! Timeline view UI component with zoom and pan functionality.
//! Per SPEC_v1.0.md.md: Timeline → Tracks → Clips hierarchy.

use eframe::egui::*;
use crate::core::Timeline;
use crate::ui::TimelineViewState;
use crate::core::time::{to_seconds, from_seconds};

/// Timeline view component
/// Per SPEC_v1.0.md.md: Timeline time ≠ source time. Clips have in/out points (source time).
pub struct TimelineView {
    // UI state will be added here
}

impl TimelineView {
    /// Create a new timeline view
    pub fn new() -> Self {
        Self {}
    }

    /// Render the timeline view
    /// This will use egui in the future.
    /// NOTE: When implemented, this should take an egui::Ui context parameter
    /// to properly integrate with the egui rendering system.
    pub fn render(&mut self) {
        // Placeholder - will be implemented with egui
        // Future signature: pub fn render(&mut self, ui: &mut egui::Ui)
    }
}

impl Default for TimelineView {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the timeline UI with zoom and pan functionality
/// 
/// This function handles:
/// - Zoom: Ctrl + Scroll Wheel (keeps point under cursor stationary)
/// - Pan: Middle mouse button drag (pan speed relative to zoom level)
/// 
/// All interaction logic is contained within this function.
pub fn timeline_ui(ui: &mut Ui, timeline: &Timeline, view_state: &mut TimelineViewState) {
    // Define timeline area dimensions
    let available_size = ui.available_size();
    let timeline_height = 200.0;
    let timeline_rect = Rect::from_min_size(
        ui.cursor().left_top(),
        vec2(available_size.x, timeline_height),
    );
    
    // Reserve space for timeline
    let timeline_response = ui.allocate_response(
        vec2(available_size.x, timeline_height),
        Sense::click_and_drag(),
    );
    
    // Draw timeline background
    let painter = ui.painter();
    painter.rect_filled(timeline_rect, 0.0, Color32::from_gray(30));
    
    // Handle input interactions
    ui.input(|i| {
        // Check if pointer is over timeline area
        let pointer_pos = i.pointer.hover_pos();
        let is_over_timeline = pointer_pos
            .map(|pos| timeline_rect.contains(pos))
            .unwrap_or(false);
        
        if is_over_timeline {
            // === ZOOM: Ctrl + Scroll Wheel ===
            if i.modifiers.ctrl && i.raw_scroll_delta.y != 0.0 {
                // Calculate the time position under the cursor before zoom
                let cursor_x = pointer_pos.unwrap().x - timeline_rect.left();
                let time_at_cursor = if timeline_rect.width() > 0.0 {
                    // Convert pixel position to time (nanoseconds)
                    let normalized_x = cursor_x / timeline_rect.width();
                    let visible_time_range = if timeline.duration > 0 {
                        timeline.duration as f64 / view_state.zoom as f64
                    } else {
                        10.0 // Default 10 seconds if timeline is empty
                    };
                    (view_state.pan_nanos as f64) + 
                        (normalized_x as f64 * visible_time_range)
                } else {
                    view_state.pan_nanos as f64
                };
                
                // Apply zoom (scroll up = zoom in, scroll down = zoom out)
                let zoom_factor = 1.0 + (i.raw_scroll_delta.y * 0.001);
                let new_zoom = (view_state.zoom * zoom_factor).clamp(0.1, 100.0);
                
                // Calculate new pan to keep the point under cursor stationary
                if timeline_rect.width() > 0.0 {
                    let new_visible_time_range = if timeline.duration > 0 {
                        timeline.duration as f64 / new_zoom as f64
                    } else {
                        10.0
                    };
                    let normalized_x = cursor_x / timeline_rect.width();
                    let new_pan_nanos = time_at_cursor - (normalized_x as f64 * new_visible_time_range);
                    view_state.pan_nanos = new_pan_nanos as i64;
                }
                
                view_state.zoom = new_zoom;
            }
        }
    });
    
    // Handle panning via middle mouse button drag
    if timeline_response.dragged_by(PointerButton::Middle) {
        let drag_delta = timeline_response.drag_delta();
        
        // Pan speed is relative to zoom level (higher zoom = slower pan)
        // At zoom 1.0, 1 pixel = some base time unit
        // At zoom 10.0, 1 pixel = 1/10th of that time unit
        let base_pixels_per_second = 100.0; // Base scale: 100 pixels per second at zoom 1.0
        let pixels_per_second = base_pixels_per_second * view_state.zoom;
        let seconds_per_pixel = 1.0 / pixels_per_second;
        
        // Convert drag delta to time delta (nanoseconds)
        let time_delta_seconds = drag_delta.x * seconds_per_pixel;
        let time_delta_nanos = from_seconds(time_delta_seconds as f64);
        
        // Update pan position
        view_state.pan_nanos -= time_delta_nanos;
        
        // Clamp pan to valid range
        let visible_time_range = if timeline.duration > 0 {
            timeline.duration as f64 / view_state.zoom as f64
        } else {
            10.0
        };
        let max_pan = visible_time_range as i64;
        view_state.pan_nanos = view_state.pan_nanos.max(0).min(max_pan);
    }
    
    // Draw timeline content (simplified visualization)
    // Calculate visible time range in nanoseconds
    let visible_time_range_ns = if timeline.duration > 0 {
        (timeline.duration as f64 / view_state.zoom as f64) as i64
    } else {
        from_seconds(10.0) // Default 10 seconds if timeline is empty
    };

    let start_time_ns = view_state.pan_nanos;
    let end_time_ns = start_time_ns + visible_time_range_ns;

    // Draw time markers
    let time_marker_spacing_ns = from_seconds(1.0); // 1 second intervals in nanoseconds

    // Calculate first marker time (aligned to spacing)
    let mut current_time_ns = (start_time_ns / time_marker_spacing_ns) * time_marker_spacing_ns;

    while current_time_ns <= end_time_ns {
        let x = timeline_rect.left() +
                (((current_time_ns - start_time_ns) as f64 / visible_time_range_ns as f64) * timeline_rect.width() as f64) as f32;

        if x >= timeline_rect.left() && x <= timeline_rect.right() {
            // Draw vertical line for time marker
            painter.line_segment(
                [pos2(x, timeline_rect.top()), pos2(x, timeline_rect.bottom())],
                Stroke::new(1.0, Color32::from_gray(100)),
            );

            // Draw time label
            let time_seconds = to_seconds(current_time_ns);
            painter.text(
                pos2(x + 2.0, timeline_rect.top() + 15.0),
                Align2::LEFT_TOP,
                format!("{:.1}s", time_seconds),
                FontId::monospace(10.0),
                Color32::from_gray(200),
            );
        }

        current_time_ns += time_marker_spacing_ns;

        // Safety break to prevent infinite loops in case of logic error
        if time_marker_spacing_ns <= 0 { break; }
    }

    // Draw clips (simplified)
    for clip in &timeline.video_track.clips {
        let clip_start_x = timeline_rect.left() +
            (((clip.timeline_start - start_time_ns) as f64 / visible_time_range_ns as f64) * timeline_rect.width() as f64) as f32;
        let clip_end_x = timeline_rect.left() +
            (((clip.timeline_end - start_time_ns) as f64 / visible_time_range_ns as f64) * timeline_rect.width() as f64) as f32;

        if clip_end_x >= timeline_rect.left() && clip_start_x <= timeline_rect.right() {
            let clip_rect = Rect::from_min_max(
                pos2(clip_start_x, timeline_rect.top() + 20.0),
                pos2(clip_end_x, timeline_rect.top() + 60.0),
            );
            painter.rect_filled(clip_rect, 2.0, Color32::from_rgb(100, 150, 255));
        }
    }

    for clip in &timeline.audio_track.clips {
        let clip_start_x = timeline_rect.left() +
            (((clip.timeline_start - start_time_ns) as f64 / visible_time_range_ns as f64) * timeline_rect.width() as f64) as f32;
        let clip_end_x = timeline_rect.left() +
            (((clip.timeline_end - start_time_ns) as f64 / visible_time_range_ns as f64) * timeline_rect.width() as f64) as f32;

        if clip_end_x >= timeline_rect.left() && clip_start_x <= timeline_rect.right() {
            let clip_rect = Rect::from_min_max(
                pos2(clip_start_x, timeline_rect.top() + 70.0),
                pos2(clip_end_x, timeline_rect.top() + 110.0),
            );
            painter.rect_filled(clip_rect, 2.0, Color32::from_rgb(255, 150, 100));
        }
    }

    // Draw playhead
    let playhead_x = timeline_rect.left() +
        (((timeline.playhead - start_time_ns) as f64 / visible_time_range_ns as f64) * timeline_rect.width() as f64) as f32;

    
    if playhead_x >= timeline_rect.left() && playhead_x <= timeline_rect.right() {
        painter.line_segment(
            [pos2(playhead_x, timeline_rect.top()), pos2(playhead_x, timeline_rect.bottom())],
            Stroke::new(2.0, Color32::from_rgb(255, 0, 0)),
        );
    }
}
