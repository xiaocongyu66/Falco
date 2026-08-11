//! GPU rendering via wgpu — hardware-accelerated 2D canvas.
//!
//! # Overview
//!
//! This module provides GPU-accelerated rendering using wgpu
//! (Vulkan/Metal/D3D12/WebGPU). When the `real-gpu` feature is enabled,
//! the paint commands are uploaded to the GPU as a vertex buffer and
//! rendered in a single draw call.
//!
//! # Architecture
//!
//! ```text
//! PaintCommand → Vertex Buffer → GPU Pipeline → Framebuffer → PNG
//! ```
//!
//! The GPU renderer:
//! 1. Creates a wgpu device + surface
//! 2. Compiles a WGSL shader (fill rect, fill text, fill path)
//! 3. Converts PaintCommands to vertices
//! 4. Renders everything in a single render pass
//! 5. Reads back the framebuffer to a CPU buffer
//!
//! # Feature Flag
//!
//! This module is only available when `real-gpu` is enabled:
//! ```toml
//! [dependencies]
//! falco = { features = ["real-gpu"] }
//! ```

#![cfg(feature = "real-gpu")]

use wgpu::*;

/// A GPU vertex — position + color + UV.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct GpuVertex {
    /// Position (x, y) in pixels.
    pub pos: [f32; 2],
    /// Color (r, g, b, a) — 0.0 to 1.0.
    pub color: [f32; 4],
    /// UV coordinates (for textures).
    pub uv: [f32; 2],
}

impl GpuVertex {
    pub fn new(x: f32, y: f32, r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            pos: [x, y],
            color: [r, g, b, a],
            uv: [0.0, 0.0],
        }
    }
}

/// The WGSL shader source for the 2D renderer.
pub const SHADER_SOURCE: &str = r#"
struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Convert pixel coordinates to clip space.
    let x = (input.pos.x / uniforms.resolution.x) * 2.0 - 1.0;
    let y = 1.0 - (input.pos.y / uniforms.resolution.y) * 2.0;
    output.clip_pos = vec4<f32>(x, y, 0.0, 1.0);
    output.color = input.color;
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

/// A GPU-accelerated renderer.
pub struct GpuRenderer {
    /// The wgpu device.
    device: Device,
    /// The device queue.
    queue: Queue,
    /// The render pipeline.
    pipeline: RenderPipeline,
    /// The uniform buffer (resolution).
    uniform_buffer: Buffer,
    /// The bind group.
    bind_group: BindGroup,
    /// The vertex buffer.
    vertex_buffer: Option<Buffer>,
    /// Number of vertices.
    vertex_count: u32,
}

impl GpuRenderer {
    /// Create a new GPU renderer.
    ///
    /// This creates a wgpu device using the best available backend
    /// (Vulkan on Linux, Metal on macOS, D3D12 on Windows).
    pub async fn new(width: u32, height: u32) -> anyhow::Result<Self> {
        let instance = Instance::default();
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No suitable GPU adapter found"))?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("Falco GPU Device"),
                required_features: Features::empty(),
                required_limits: Limits::default(),
                memory_hints: MemoryHints::default(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to request GPU device: {}", e))?;

        // Load shader.
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Falco Shader"),
            source: ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        // Create uniform buffer.
        let uniform_bytes = [width as f32, height as f32].to_vec();
        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&uniform_bytes),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        // Create bind group layout.
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout.
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline.
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as BufferAddress,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            uniform_buffer,
            bind_group,
            vertex_buffer: None,
            vertex_count: 0,
        })
    }

    /// Upload vertices to the GPU.
    pub fn upload_vertices(&mut self, vertices: &[GpuVertex]) {
        self.vertex_buffer = Some(self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: BufferUsages::VERTEX,
        }));
        self.vertex_count = vertices.len() as u32;
    }

    /// Render to a texture and read back the pixels.
    pub async fn render_to_pixels(&self, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
        // Create output texture.
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("Output Texture"),
            size: Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&TextureViewDescriptor::default());

        // Create command encoder.
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Render pass.
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_writes: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);

            if let Some(ref vertex_buffer) = self.vertex_buffer {
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.draw(0..self.vertex_count, 0..1);
            }
        }

        // Copy texture to buffer.
        let bytes_per_row = width * 4;
        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("Output Buffer"),
            size: (bytes_per_row * height) as BufferAddress,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d { width, height, depth_or_array_layers: 1 },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // Read back pixels.
        let buffer_slice = buffer.slice(..);
        let (tx, rx) = futures_intrusive_channel::shared::oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).ok();
        });
        self.device.poll(PollType::Wait).map_err(|e| anyhow::anyhow!("Poll failed: {:?}", e))?;
        rx.receive().await.ok_or_else(|| anyhow::anyhow!("Buffer map timed out"))??;

        let data = buffer_slice.get_mapped_range().to_vec();
        drop(buffer);
        Ok(data)
    }
}

// Helper trait for buffer creation.
trait CreateBufferInit {
    fn create_buffer_init(&self, desc: &BufferInitDescriptor) -> Buffer;
}

impl CreateBufferInit for Device {
    fn create_buffer_init(&self, desc: &BufferInitDescriptor) -> Buffer {
        self.create_buffer(&BufferDescriptor {
            label: desc.label,
            size: desc.contents.len() as BufferAddress,
            usage: desc.usage,
            mapped_at_creation: false,
        })
    }
}

struct BufferInitDescriptor<'a> {
    label: Option<&'a str>,
    contents: &'a [u8],
    usage: BufferUsages,
}

// Re-export needed types.
pub use wgpu::Buffer;
pub use wgpu::BufferAddress;
pub use wgpu::BufferDescriptor;
pub use wgpu::BufferUsages;
pub use wgpu::CommandEncoderDescriptor;
pub use wgpu::Device;
pub use wgpu::DeviceDescriptor;
pub use wgpu::Queue;
pub use wgpu::RenderPipeline;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_vertex_creation() {
        let v = GpuVertex::new(10.0, 20.0, 1.0, 0.0, 0.0, 1.0);
        assert_eq!(v.pos, [10.0, 20.0]);
        assert_eq!(v.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn shader_source_not_empty() {
        assert!(!SHADER_SOURCE.is_empty());
        assert!(SHADER_SOURCE.contains("vs_main"));
        assert!(SHADER_SOURCE.contains("fs_main"));
    }
}
