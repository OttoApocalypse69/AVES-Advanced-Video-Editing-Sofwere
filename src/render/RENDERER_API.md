# GPU Rendering Interface - API Documentation

## Task Restatement

Design and implement a GPU rendering interface using wgpu that accepts pre-decoded RGBA8 video frames and renders them as composited layers with basic transforms (position, scale, opacity). The interface must be decoupled from decoding and timeline logic, providing a clean API for the render thread to composite multiple video layers efficiently on the GPU.

---

## 1. Renderer API

### Public Structs

```rust
/// GPU renderer for layered video frames
pub struct Renderer {
    compositor: Compositor,  // Internal implementation
}

/// A single layer to render
/// Layers are composited in order (first = back, last = front)
pub struct Layer {
    pub frame: VideoFrame,      // RGBA8 frame from decode module
    pub transform: Transform,   // Transform parameters
}

/// Transform parameters for a video layer
/// All coordinates are normalized (0.0-1.0) relative to output dimensions
pub struct Transform {
    pub position: (f32, f32),  // Center position (0.0,0.0 = top-left, 1.0,1.0 = bottom-right)
    pub scale: (f32, f32),     // Scale factors (1.0 = original size)
    pub opacity: f32,          // Opacity (0.0 = transparent, 1.0 = opaque)
}

/// Error type for rendering operations
pub enum RenderError {
    Wgpu(String),        // wgpu/GPU errors
    Surface(String),    // Surface/presentation errors
    InvalidLayer(String), // Invalid layer data
}
```

### Public Function Signatures

```rust
impl Renderer {
    /// Create a new renderer with a wgpu surface from a window
    pub fn new(window: &Window) -> Result<Self, RenderError>
    
    /// Resize the render surface (call on window resize)
    pub fn resize(&mut self, width: u32, height: u32)
    
    /// Render multiple layers to the surface
    /// Layers composited in order (first = back, last = front)
    pub fn render_layers(&mut self, layers: &[Layer]) -> Result<(), RenderError>
    
    /// Get the wgpu device (for external use)
    pub fn device(&self) -> &Device
    
    /// Get the wgpu queue (for external use)
    pub fn queue(&self) -> &Queue
}

impl Transform {
    /// Default transform (centered, 1.0 scale, fully opaque)
    pub fn default() -> Self
}
```

### Usage Example

```rust
use crate::render::{Renderer, Layer, Transform};
use crate::decode::decoder::VideoFrame;

// Initialize renderer
let mut renderer = Renderer::new(&window)?;

// Prepare layers
let layers = vec![
    Layer {
        frame: video_frame_1,  // RGBA8 VideoFrame
        transform: Transform {
            position: (0.5, 0.5),  // Center
            scale: (1.0, 1.0),     // Original size
            opacity: 1.0,          // Fully opaque
        },
    },
    Layer {
        frame: video_frame_2,  // Overlay layer
        transform: Transform {
            position: (0.25, 0.25),  // Top-left quadrant
            scale: (0.5, 0.5),        // Half size
            opacity: 0.8,             // 80% opaque
        },
    },
];

// Render
renderer.render_layers(&layers)?;
```

---

## 2. Resource Lifecycle

### Initialization (`Renderer::new`)
1. Create wgpu `Instance` with all backends
2. Create `Surface` from window
3. Request `Adapter` compatible with surface
4. Request `Device` and `Queue`
5. Configure surface with format and size
6. Create render pipeline (vertex + fragment shaders)
7. Create uniform buffer for transform data
8. Initialize texture cache (empty)

**Resources created:**
- `Surface` (lifetime tied to window)
- `Device` and `Queue` (lifetime tied to renderer)
- `RenderPipeline` (reused for all frames)
- `BindGroupLayout` (reused for all layers)
- `Buffer` (uniform buffer, reused per frame)

### Per-Frame Rendering (`render_layers`)
1. **Validation**: Check layer dimensions, data size, opacity range
2. **Texture Management**:
   - Resize texture cache to match layer count
   - For each layer:
     - If texture exists with matching dimensions: update in-place
     - Otherwise: create new texture
3. **Surface Acquisition**: Get current surface texture
4. **Render Pass**:
   - Clear to black
   - For each layer (back to front):
     - Update uniform buffer with transform
     - Create bind group (texture, sampler, uniform)
     - Draw fullscreen quad
5. **Submission**: Submit command buffer, present surface

**Resource reuse:**
- Textures are cached and reused when dimensions match
- Uniform buffer is reused (updated per layer)
- Render pipeline is reused
- Bind groups are created per layer (wgpu requirement)

### Cleanup
- All resources automatically cleaned up via `Drop` traits
- No manual cleanup required
- Surface is dropped when renderer is dropped

### Memory Management
- **Texture cache**: Grows to maximum layer count, never shrinks
- **Frame data**: Borrowed during `render_layers()` call, not stored
- **Uniform buffer**: Fixed size (48 bytes), allocated once

---

## 3. Sync Assumptions

### Threading Model
- **Single-threaded rendering**: `Renderer` is NOT `Send`/`Sync`
- **Render thread only**: All rendering operations must occur on the render thread
- **No cross-thread access**: Renderer cannot be shared between threads without external synchronization

### Frame Data Synchronization
- **Borrowed data**: `render_layers()` takes `&[Layer]`, caller must ensure data validity
- **No internal storage**: Frame data is not copied or stored internally
- **Upload timing**: Frame data is uploaded to GPU during `render_layers()` call
- **Completion**: GPU operations are submitted synchronously, but execution is asynchronous

### GPU Synchronization
- **wgpu queue**: All GPU operations submitted via queue (handles GPU-side sync)
- **Command buffer**: Single command buffer per frame, submitted atomically
- **Surface presentation**: `present()` blocks until next vsync (platform-dependent)
- **No explicit fences**: wgpu handles GPU synchronization internally

### External Synchronization Requirements
If renderer is accessed from multiple threads:
- External mutex/synchronization required
- Frame data must remain valid during `render_layers()` call
- Window resize events must be synchronized

### Known Sync Behaviors
- **Frame upload**: `queue.write_texture()` is asynchronous (returns immediately)
- **Buffer update**: `queue.write_buffer()` is asynchronous
- **Command submission**: `queue.submit()` is asynchronous
- **Surface present**: May block on vsync (platform-dependent)

---

## 4. Shader Interface Description

### Vertex Shader (`vs_main`)

**Input:**
- `@builtin(vertex_index) in_vertex_index: u32` - Vertex index (0-2) for fullscreen quad

**Uniform Buffer (Binding 2):**
```wgsl
struct TransformUniform {
    position: vec2<f32>,      // Normalized position (0.0-1.0)
    scale: vec2<f32>,          // Scale factors
    opacity: f32,              // Opacity (0.0-1.0)
    _padding: f32,              // Alignment padding
    output_size: vec2<f32>,    // Output dimensions (width, height)
    frame_size: vec2<f32>,     // Frame dimensions (width, height)
    _padding2: vec2<f32>,      // Additional padding
}
```

**Output:**
- `@location(0) tex_coords: vec2<f32>` - Texture coordinates (0.0-1.0)
- `@builtin(position) clip_position: vec4<f32>` - Clip space position (NDC)

**Transform Logic:**
1. Generate fullscreen quad vertices (-1 to 1 in NDC)
2. Apply scale to quad dimensions
3. Translate quad center to specified position
4. Map clip space to texture coordinates

### Fragment Shader (`fs_main`)

**Input:**
- `@location(0) tex_coords: vec2<f32>` - Texture coordinates from vertex shader

**Resources:**
- `@group(0) @binding(0) t_texture: texture_2d<f32>` - Video frame texture (RGBA8)
- `@group(0) @binding(1) s_sampler: sampler` - Linear filtering sampler
- `@group(0) @binding(2) transform: TransformUniform` - Transform uniform

**Output:**
- `@location(0) vec4<f32>` - RGBA color with opacity applied

**Transform Logic:**
1. Sample texture at computed coordinates
2. Multiply alpha channel by opacity value
3. GPU blend state handles alpha compositing

### Bind Group Layout

Each layer requires a bind group with:
- **Binding 0**: Texture view (2D, RGBA8)
- **Binding 1**: Sampler (linear, clamp to edge)
- **Binding 2**: Uniform buffer (48 bytes, TransformUniform)

### Pipeline State

- **Blend**: Alpha blending enabled (src_alpha, one_minus_src_alpha)
- **Topology**: Triangle list (fullscreen quad = 3 vertices)
- **Culling**: Back face culling enabled
- **Depth**: No depth testing (2D compositing)

---

## 5. Unsafe Blocks

### Location: `src/render/compositor.rs:333-338`

```rust
let bytes = unsafe {
    std::slice::from_raw_parts(
        &transform_uniform as *const TransformUniform as *const u8,
        std::mem::size_of::<TransformUniform>(),
    )
};
```

**Justification:**
- **Purpose**: Convert `TransformUniform` struct to byte slice for GPU buffer upload
- **Safety**: 
  - `TransformUniform` is `#[repr(C)]` with explicit padding
  - Contains only `f32` values (no padding issues)
  - Size is known at compile time
  - Lifetime is scoped to the function call
- **Isolation**: Contained within compositor module (internal implementation)
- **Spec compliance**: Per SPEC v1.0, unsafe is allowed for "GPU buffer mapping" - this is buffer serialization for GPU upload

**Alternative considered:** Using `bytemuck` crate, but SPEC prohibits adding dependencies.

---

## 6. Error Handling

### Validation Errors
- **Zero dimensions**: Layer frame has zero width or height
- **Invalid data size**: Frame data length doesn't match width × height × 4
- **Invalid opacity**: Opacity outside [0.0, 1.0] range

### GPU Errors
- **wgpu errors**: Device/adapter creation, buffer/texture creation, pipeline creation
- **Surface errors**: Surface creation, surface acquisition, presentation

### Error Propagation
- All errors returned as `Result<T, RenderError>`
- No panics in public API
- wgpu errors converted to strings (detailed error info may be lost)

### Error Recovery
- **Surface lost**: Caller must recreate renderer
- **Device lost**: Caller must recreate renderer
- **Invalid input**: Validation errors returned immediately, no GPU operations performed

---

## 7. Known Limitations

### Performance
- **Texture cache growth**: Cache never shrinks, may grow large with many layers
- **Bind group creation**: New bind group created per layer per frame (wgpu requirement)
- **Uniform buffer update**: Updated per layer (small overhead)

### Functionality
- **No rotation**: Only position, scale, opacity supported (per spec)
- **No effects**: No color correction, filters, or other effects (per spec)
- **Fixed blend mode**: Alpha blending only, no custom blend modes
- **No depth sorting**: Layers rendered in provided order only

### Constraints
- **Single surface**: One output surface per renderer
- **Fixed format**: Surface format determined by platform (sRGB preferred)
- **No MSAA**: Multisampling not implemented (may cause aliasing)

### Thread Safety
- **Not thread-safe**: Renderer is not `Send`/`Sync`
- **External sync required**: If used from multiple threads, caller must synchronize

### Resource Limits
- **Texture count**: Limited by GPU memory and wgpu limits
- **Layer count**: No hard limit, but performance degrades with many layers
- **Frame size**: Limited by GPU texture size limits (typically 16384×16384)

---

## Implementation Notes

### Frame Upload Efficiency
- **In-place updates**: Textures are updated in-place when dimensions match
- **Minimal allocations**: Texture cache pre-allocated, reused across frames
- **Batch operations**: All layers rendered in single render pass

### GPU Overhead
- **Single pipeline**: One render pipeline for all layers
- **Efficient blending**: Hardware-accelerated alpha blending
- **Minimal state changes**: Only uniform buffer and bind group change per layer

### API Clarity
- **Simple interface**: Three main functions (new, resize, render_layers)
- **Clear ownership**: Frame data borrowed, not owned
- **Explicit transforms**: Transform struct makes parameters clear

