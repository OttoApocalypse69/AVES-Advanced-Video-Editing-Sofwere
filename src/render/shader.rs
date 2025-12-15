//! Shader compilation and management for layered video rendering with transforms.

use wgpu::*;

/// Transform uniform buffer structure (must match TransformUniform in compositor.rs)
/// Layout: position[2], scale[2], opacity, padding, output_size[2], frame_size[2], padding2[2]
#[allow(dead_code)]
#[repr(C)]
struct TransformUniform {
    position: [f32; 2],
    scale: [f32; 2],
    opacity: f32,
    _padding: f32,
    output_size: [f32; 2],
    frame_size: [f32; 2],
    _padding2: [f32; 2],
}

/// Vertex shader for rendering video frames with position and scale transforms
/// 
/// Shader Interface:
/// - Input: vertex_index (builtin) - generates fullscreen quad
/// - Uniform: Transform buffer (binding 2) - contains position, scale, output_size, frame_size
/// - Output: clip_position (NDC coordinates), tex_coords (texture coordinates)
/// 
/// Transform logic:
/// - Position: Normalized (0.0-1.0) center position, converted to NDC
/// - Scale: Applied to frame dimensions before positioning
/// - Texture coordinates: Calculated from clip position and frame/output aspect ratios
pub const VERTEX_SHADER: &str = r#"
    struct TransformUniform {
        position: vec2<f32>,
        scale: vec2<f32>,
        opacity: f32,
        _padding: f32,
        output_size: vec2<f32>,
        frame_size: vec2<f32>,
        _padding2: vec2<f32>,
    };

    @group(0) @binding(2) var<uniform> transform: TransformUniform;

    struct VertexOutput {
        @location(0) tex_coords: vec2<f32>,
        @builtin(position) clip_position: vec4<f32>,
    };

    @vertex
    fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
        var out: VertexOutput;
        
        // Generate fullscreen quad vertices (-1 to 1 in NDC)
        var x = f32((in_vertex_index << 1u) & 2u) * 2.0 - 1.0;
        var y = f32(in_vertex_index & 2u) * 2.0 - 1.0;
        
        // Calculate aspect ratios
        let output_aspect = transform.output_size.x / transform.output_size.y;
        let frame_aspect = transform.frame_size.x / transform.frame_size.y;
        
        // Calculate scaled frame size in normalized coordinates
        let scaled_width = transform.scale.x * (transform.frame_size.x / transform.output_size.x);
        let scaled_height = transform.scale.y * (transform.frame_size.y / transform.output_size.y);
        
        // Apply scale to quad
        x *= scaled_width;
        y *= scaled_height;
        
        // Apply position offset (convert from 0.0-1.0 to -1.0 to 1.0)
        x += (transform.position.x - 0.5) * 2.0;
        y -= (transform.position.y - 0.5) * 2.0;  // Flip Y for screen coordinates
        
        out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
        
        // Calculate texture coordinates
        // Map from clip space (-1 to 1) to texture space (0 to 1)
        // Account for frame aspect ratio
        let tex_x = (x + 1.0) * 0.5;
        let tex_y = 1.0 - (y + 1.0) * 0.5;  // Flip Y for texture coordinates
        
        out.tex_coords = vec2<f32>(tex_x, tex_y);
        
        return out;
    }
"#;

/// Fragment shader for sampling texture with opacity support
/// 
/// Shader Interface:
/// - Input: tex_coords (from vertex shader)
/// - Uniform: Transform buffer (binding 2) - contains opacity
/// - Texture: Video frame texture (binding 0)
/// - Sampler: Texture sampler (binding 1)
/// - Output: RGBA color with opacity applied
/// 
/// Transform logic:
/// - Opacity: Multiplies alpha channel of sampled color
pub const FRAGMENT_SHADER: &str = r#"
    struct TransformUniform {
        position: vec2<f32>,
        scale: vec2<f32>,
        opacity: f32,
        _padding: f32,
        output_size: vec2<f32>,
        frame_size: vec2<f32>,
        _padding2: vec2<f32>,
    };

    @group(0) @binding(0) var t_texture: texture_2d<f32>;
    @group(0) @binding(1) var s_sampler: sampler;
    @group(0) @binding(2) var<uniform> transform: TransformUniform;

    struct VertexOutput {
        @location(0) tex_coords: vec2<f32>,
        @builtin(position) clip_position: vec4<f32>,
    };

    @fragment
    fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
        var color = textureSample(t_texture, s_sampler, in.tex_coords);
        // Apply opacity to alpha channel
        color.a *= transform.opacity;
        return color;
    }
"#;

/// Compile a shader module from WGSL source
pub fn compile_shader(device: &Device, source: &str) -> ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}
