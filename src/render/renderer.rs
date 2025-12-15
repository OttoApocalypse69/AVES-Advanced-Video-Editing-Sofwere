//! Public GPU rendering interface for layered video frames.
//! Per SPEC.md: Accepts RGBA8 frames, supports basic transforms (scale, position, opacity).

use wgpu::*;
use winit::window::Window;
use crate::decode::decoder::VideoFrame;
use crate::render::compositor::Compositor;

/// Error type for rendering operations
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("wgpu error: {0}")]
    Wgpu(String),
    #[error("Surface error: {0}")]
    Surface(String),
    #[error("Invalid layer: {0}")]
    InvalidLayer(String),
}

/// Transform parameters for a video layer
/// All coordinates are normalized (0.0-1.0) relative to output dimensions
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    /// Position in normalized coordinates (0.0, 0.0) = top-left, (1.0, 1.0) = bottom-right
    /// Position represents the center of the layer
    pub position: (f32, f32),
    /// Scale factors (1.0 = original size, 2.0 = double size)
    pub scale: (f32, f32),
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: (0.5, 0.5),  // Center
            scale: (1.0, 1.0),      // Original size
            opacity: 1.0,           // Fully opaque
        }
    }
}

/// A single layer to render
/// Layers are composited in order (first = back, last = front)
#[derive(Debug, Clone)]
pub struct Layer {
    /// RGBA8 video frame to render
    pub frame: VideoFrame,
    /// Transform to apply to this layer
    pub transform: Transform,
}

/// GPU renderer for layered video frames
/// Handles wgpu initialization and provides a clean API for rendering
pub struct Renderer {
    compositor: Compositor,
}

impl Renderer {
    /// Create a new renderer with a wgpu surface from a window
    pub fn new(window: &Window) -> Result<Self, RenderError> {
        let compositor = Compositor::new(window)
            .map_err(|e| RenderError::Wgpu(e.to_string()))?;
        
        Ok(Self { compositor })
    }

    /// Resize the render surface
    /// Should be called when the window is resized
    pub fn resize(&mut self, width: u32, height: u32) {
        self.compositor.resize(width, height);
    }

    /// Render multiple layers to the surface
    /// Layers are composited in order (first layer = back, last layer = front)
    /// Each layer can have independent transforms (position, scale, opacity)
    pub fn render_layers(&mut self, layers: &[Layer]) -> Result<(), RenderError> {
        // Validate layers
        for (i, layer) in layers.iter().enumerate() {
            if layer.frame.width == 0 || layer.frame.height == 0 {
                return Err(RenderError::InvalidLayer(
                    format!("Layer {} has zero dimensions", i)
                ));
            }
            if layer.frame.data.len() != (layer.frame.width * layer.frame.height * 4) as usize {
                return Err(RenderError::InvalidLayer(
                    format!("Layer {} has invalid data size", i)
                ));
            }
            if layer.transform.opacity < 0.0 || layer.transform.opacity > 1.0 {
                return Err(RenderError::InvalidLayer(
                    format!("Layer {} has invalid opacity (must be 0.0-1.0)", i)
                ));
            }
        }

        self.compositor.render_layers(layers)
            .map_err(|e| RenderError::Wgpu(e.to_string()))
    }

    /// Get the wgpu device (for external use, e.g., creating textures)
    pub fn device(&self) -> &Device {
        self.compositor.device()
    }

    /// Get the wgpu queue (for external use, e.g., uploading data)
    pub fn queue(&self) -> &Queue {
        self.compositor.queue()
    }
}

