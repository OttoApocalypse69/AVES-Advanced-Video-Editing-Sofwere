# Shader Interface Description

This document describes the GPU shader interface for layered video rendering with transforms.

## Overview

The rendering system uses two WGSL shaders (vertex and fragment) to render multiple video layers with independent transforms (position, scale, opacity). Layers are composited in order (first = back, last = front).

## Vertex Shader (`vs_main`)

### Inputs
- `@builtin(vertex_index) in_vertex_index: u32` - Vertex index (0-2) used to generate a fullscreen quad

### Uniform Buffer (Binding 2)
```wgsl
struct TransformUniform {
    position: vec2<f32>,      // Normalized position (0.0-1.0), center of layer
    scale: vec2<f32>,          // Scale factors (1.0 = original size)
    opacity: f32,              // Opacity (0.0-1.0)
    _padding: f32,             // Padding for alignment
    output_size: vec2<f32>,   // Output dimensions (width, height)
    frame_size: vec2<f32>,    // Frame dimensions (width, height)
    _padding2: vec2<f32>,     // Additional padding
}
```

### Outputs
- `@location(0) tex_coords: vec2<f32>` - Texture coordinates (0.0-1.0)
- `@builtin(position) clip_position: vec4<f32>` - Clip space position (NDC)

### Transform Logic
1. **Quad Generation**: Creates a fullscreen quad from vertex indices (-1 to 1 in NDC)
2. **Scale Application**: Multiplies quad dimensions by scale factors
3. **Position Offset**: Translates quad center to the specified position
4. **Texture Coordinates**: Maps clip space to texture space (0.0-1.0)

## Fragment Shader (`fs_main`)

### Inputs
- `@location(0) tex_coords: vec2<f32>` - Texture coordinates from vertex shader
- `@builtin(position) clip_position: vec4<f32>` - Fragment position

### Resources
- `@group(0) @binding(0) t_texture: texture_2d<f32>` - Video frame texture (RGBA8)
- `@group(0) @binding(1) s_sampler: sampler` - Texture sampler (linear filtering)
- `@group(0) @binding(2) transform: TransformUniform` - Transform uniform buffer

### Outputs
- `@location(0) vec4<f32>` - RGBA color with opacity applied

### Transform Logic
1. **Texture Sampling**: Samples the video frame texture at the computed coordinates
2. **Opacity Application**: Multiplies the alpha channel by the opacity value
3. **Blending**: The GPU blend state handles alpha compositing (src_alpha, one_minus_src_alpha)

## Bind Group Layout

Each layer requires a bind group with three bindings:

- **Binding 0**: Texture view (2D texture, RGBA8 format)
- **Binding 1**: Sampler (linear filtering, clamp to edge)
- **Binding 2**: Uniform buffer (TransformUniform, 48 bytes)

## Rendering Pipeline

1. **Clear**: Render pass clears to black
2. **Layer Rendering**: For each layer (back to front):
   - Update uniform buffer with layer transform
   - Create bind group with layer texture, sampler, and uniform
   - Draw fullscreen quad (3 vertices, 1 instance)
3. **Blending**: Alpha blending composites layers (first = back, last = front)

## Coordinate Systems

- **Position**: Normalized (0.0, 0.0) = top-left, (1.0, 1.0) = bottom-right
- **Scale**: Multiplicative factors (1.0 = original size, 2.0 = double size)
- **Texture Coordinates**: (0.0, 0.0) = top-left, (1.0, 1.0) = bottom-right
- **Clip Space**: (-1.0, -1.0) = bottom-left, (1.0, 1.0) = top-right

## Performance Considerations

- Uniform buffer is updated per layer (small overhead)
- Texture cache minimizes texture recreation
- Single render pass for all layers (efficient)
- Alpha blending handled by GPU hardware

