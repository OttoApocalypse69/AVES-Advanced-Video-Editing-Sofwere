//! wgpu-based compositor for rendering layered video frames with transforms.
//! Internal implementation - use Renderer for public API.

use wgpu::*;
use winit::window::Window;
use crate::render::texture::Texture;
use crate::render::renderer::Layer;
use crate::render::shader::{compile_shader, VERTEX_SHADER, FRAGMENT_SHADER};

/// Error type for compositor operations
#[derive(Debug, thiserror::Error)]
pub enum CompositorError {
    #[error("wgpu error: {0}")]
    Wgpu(String),
    #[error("Surface error: {0}")]
    Surface(String),
}

/// Uniform buffer data for layer transforms
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct TransformUniform {
    position: [f32; 2],      // Normalized position (0.0-1.0)
    scale: [f32; 2],          // Scale factors
    opacity: f32,             // Opacity (0.0-1.0)
    _padding: f32,            // Padding for alignment
    output_size: [f32; 2],   // Output dimensions (width, height)
    frame_size: [f32; 2],    // Frame dimensions (width, height)
    _padding2: [f32; 2],     // Additional padding
}

/// Compositor for rendering layered video frames to a surface
pub struct Compositor {
    device: Device,
    queue: Queue,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    render_pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    uniform_buffer: Buffer,
    texture_cache: Vec<Texture>,  // Cache textures for layers
}

impl Compositor {
    /// Create a new compositor with a wgpu surface
    pub fn new(window: &Window) -> Result<Self, CompositorError> {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        // SAFETY: Surface doesn't actually hold a reference to the window after creation.
        // It's an owned value. The 'static lifetime is safe because the surface is owned by Compositor.
        let surface_raw = instance
            .create_surface(window)
            .map_err(|e| CompositorError::Surface(e.to_string()))?;
        let surface: Surface<'static> = unsafe { std::mem::transmute(surface_raw) };

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| CompositorError::Wgpu("No adapter found".to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                required_limits: Limits::default(),
            },
            None,
        ))
        .map_err(|e| CompositorError::Wgpu(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &surface_config);

        // Create bind group layout for texture + sampler + uniform buffer
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Layer Bind Group Layout"),
            entries: &[
                // Texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // Sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // Transform uniform buffer
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Transform Uniform Buffer"),
            size: std::mem::size_of::<TransformUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create render pipeline
        let shader = compile_shader(&device, FRAGMENT_SHADER);
        let vertex_shader = compile_shader(&device, VERTEX_SHADER);

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &vertex_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: surface_config.format,
                    // Alpha blending for opacity support
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::One,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            render_pipeline,
            bind_group_layout,
            uniform_buffer,
            texture_cache: Vec::new(),
        })
    }

    /// Resize the surface
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    /// Render multiple layers to the surface
    /// Layers are composited in order (first = back, last = front)
    pub fn render_layers(&mut self, layers: &[Layer]) -> Result<(), CompositorError> {
        if layers.is_empty() {
            // Clear to black if no layers
            let output = self
                .surface
                .get_current_texture()
                .map_err(|e| CompositorError::Surface(e.to_string()))?;
            let view = output.texture.create_view(&TextureViewDescriptor::default());
            
            let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Clear Encoder"),
            });
            
            {
                let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("Clear Pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color::BLACK),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
            }
            
            self.queue.submit(std::iter::once(encoder.finish()));
            output.present();
            return Ok(());
        }

        // Ensure texture cache has enough capacity
        self.texture_cache.resize_with(layers.len(), || {
            // Placeholder - will be replaced
            Texture::from_rgba(&self.device, &self.queue, 1, 1, &[0, 0, 0, 0])
        });

        // Update textures for all layers
        for (i, layer) in layers.iter().enumerate() {
            let texture = &mut self.texture_cache[i];
            
            // Check if we need to recreate the texture
            if texture.width != layer.frame.width || texture.height != layer.frame.height {
                self.texture_cache[i] = Texture::from_rgba(
                    &self.device,
                    &self.queue,
                    layer.frame.width,
                    layer.frame.height,
                    &layer.frame.data,
                );
            } else {
                texture.update_rgba(&self.queue, &layer.frame.data);
            }
        }

        // Get surface texture
        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| CompositorError::Surface(e.to_string()))?;

        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

        // Pre-create all bind groups so they live long enough (before render_pass)
        let mut bind_groups = Vec::with_capacity(layers.len());
        for (i, layer) in layers.iter().enumerate() {
            let texture = &self.texture_cache[i];
            
            // Prepare transform uniform
            let transform_uniform = TransformUniform {
                position: [layer.transform.position.0, layer.transform.position.1],
                scale: [layer.transform.scale.0, layer.transform.scale.1],
                opacity: layer.transform.opacity,
                _padding: 0.0,
                output_size: [self.surface_config.width as f32, self.surface_config.height as f32],
                frame_size: [layer.frame.width as f32, layer.frame.height as f32],
                _padding2: [0.0, 0.0],
            };

            // Update uniform buffer
            // Safe conversion: TransformUniform is repr(C) and contains only f32
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    &transform_uniform as *const TransformUniform as *const u8,
                    std::mem::size_of::<TransformUniform>(),
                )
            };
            self.queue.write_buffer(&self.uniform_buffer, 0, bytes);

            // Create bind group for this layer
            let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("Layer Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&texture.view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&texture.sampler),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                ],
            });
            bind_groups.push(bind_group);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Begin render pass
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);

        // Render each layer using the pre-created bind groups
        for bind_group in &bind_groups {
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
        
        // Explicitly drop render_pass to release borrow on encoder
        drop(render_pass);

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Get the device (for external use)
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Get the queue (for external use)
    pub fn queue(&self) -> &Queue {
        &self.queue
    }
}
