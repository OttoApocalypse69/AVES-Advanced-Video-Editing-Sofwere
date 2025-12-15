//! Main application entry point.
//! Per SPEC_v1.0.md.md: UI Thread handles input & egui.
//! 
//! This module uses eframe to bootstrap the application with winit/wgpu/egui integration.
//! The EditorApp from src/ui/app.rs is used as the main application UI.

use eframe::egui;
use aves::ui::EditorApp;

fn main() -> eframe::Result<()> {
    // Configure native options for the window
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AVES - Advanced Video Editing Software")
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    // Run the application with EditorApp
    eframe::run_native(
        "AVES",
        native_options,
        Box::new(|cc| Box::new(EditorApp::new(cc))),
    )
}
