//! # VibeGE Renderer
//!
//! GPU-accelerated 2D renderer using wgpu.
//!
//! Provides sprite batching, texture management, and basic 2D rendering
//! with a simple API. Integrates with the WindowManager's native window.

use std::sync::Arc;
use std::sync::Mutex;

use tracing::{debug, info};
use vibege_core::RuntimeError;
use wgpu::util::DeviceExt;

/// Error types specific to rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Failed to create wgpu adapter: {0}")]
    AdapterFailed(String),
    #[error("Failed to create wgpu device: {0}")]
    DeviceFailed(String),
    #[error("Surface configuration failed: {0}")]
    SurfaceFailed(String),
    #[error("Failed to load texture: {0}")]
    TextureLoadFailed(String),
    #[error("No surface available")]
    NoSurface,
    #[error("Render pass error: {0}")]
    RenderPassError(String),
}

impl From<RenderError> for RuntimeError {
    fn from(err: RenderError) -> Self {
        RuntimeError::new(vibege_core::ErrorCode::INIT_FAILED, err.to_string())
    }
}

/// A 2D sprite vertex with position, texture coordinates, and color.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
}

impl SpriteVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, format: wgpu::VertexFormat::Float32x2, shader_location: 0 },
                wgpu::VertexAttribute { offset: 8, format: wgpu::VertexFormat::Float32x2, shader_location: 1 },
                wgpu::VertexAttribute { offset: 16, format: wgpu::VertexFormat::Float32x4, shader_location: 2 },
            ],
        }
    }
}

/// A loaded GPU texture.
pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// The GPU renderer.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    sampler: wgpu::Sampler,
    default_texture: GpuTexture,
    rect_batch: Mutex<Vec<SpriteVertex>>,
    screen_size: (f32, f32),
}

impl Renderer {
    /// Creates a new renderer from a wgpu-compatible window.
    ///
    /// # Panics
    /// Panics if the shader source is missing.
    pub async fn new(window: Arc<winit::window::Window>, width: u32, height: u32) -> Result<Self, RenderError> {
        let size = (width, height);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window))
            .map_err(|e| RenderError::SurfaceFailed(e.to_string()))?;

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }).await
        .ok_or_else(|| RenderError::AdapterFailed("No suitable GPU adapter found".into()))?;

        info!(
            adapter = %adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU adapter selected"
        );

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("VibeGE Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ).await
        .map_err(|e| RenderError::DeviceFailed(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader_source = include_str!("shaders/shader.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sprite Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sprite Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sprite Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[SpriteVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let (vertices, indices) = create_fullscreen_quad();
        let num_indices = indices.len() as u32;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Default Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let default_texture = create_solid_color_texture(&device, &queue, 1, 1, [255, 255, 255, 255]);

        info!(
            width = size.0,
            height = size.1,
            format = ?config.format,
            "Renderer initialised"
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            sampler,
            default_texture,
            rect_batch: Mutex::new(Vec::new()),
            screen_size: (size.0 as f32, size.1 as f32),
        })
    }

    /// Returns the wgpu device for external resource creation.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Returns the wgpu queue for external command submission.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Returns the surface configuration format.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Resizes the renderer's output surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.size = (width, height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            debug!(width, height, "Surface resized");
        }
    }

    /// Records a colored rectangle for the next frame.
    /// Coordinates are in screen space (0,0 = top-left).
    pub fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) {
        let (sw, sh) = self.screen_size;
        // Convert screen coords to NDC (-1 to 1)
        let x1 = (x / sw) * 2.0 - 1.0;
        let y1 = 1.0 - (y / sh) * 2.0;
        let x2 = ((x + w) / sw) * 2.0 - 1.0;
        let y2 = 1.0 - ((y + h) / sh) * 2.0;

        let mut batch = self.rect_batch.lock().unwrap();
        batch.push(SpriteVertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: [r, g, b, a] });
        batch.push(SpriteVertex { position: [x2, y1], tex_coords: [1.0, 0.0], color: [r, g, b, a] });
        batch.push(SpriteVertex { position: [x2, y2], tex_coords: [1.0, 1.0], color: [r, g, b, a] });
        batch.push(SpriteVertex { position: [x1, y2], tex_coords: [0.0, 1.0], color: [r, g, b, a] });
    }

    /// Renders all queued rectangles and presents the frame.
    /// Clears with the given background color first.
    pub fn present(&self, bg_r: f32, bg_g: f32, bg_b: f32, bg_a: f32) -> Result<(), RenderError> {
        let frame = self.surface.get_current_texture()
            .map_err(|e| RenderError::SurfaceFailed(e.to_string()))?;

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let mut batch = self.rect_batch.lock().unwrap();
        let rect_count = batch.len();

        // Create vertex buffer from batch
        let staging_verts = if rect_count > 0 {
            let verts: Vec<SpriteVertex> = batch.drain(..).collect();
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rect VB"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            }))
        } else { None };

        // Create index buffer for the batch
        let staging_indices: Vec<u16> = if rect_count > 0 {
            (0..rect_count as u16).flat_map(|i| {
                let base = i * 4;
                vec![base, base + 1, base + 2, base, base + 2, base + 3]
            }).collect()
        } else { vec![] };

        let staging_idx_buf = if !staging_indices.is_empty() {
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rect IB"),
                contents: bytemuck::cast_slice(&staging_indices),
                usage: wgpu::BufferUsages::INDEX,
            }))
        } else { None };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Game Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: bg_r as f64, g: bg_g as f64, b: bg_b as f64, a: bg_a as f64 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let (Some(vb), Some(ib)) = (&staging_verts, &staging_idx_buf) {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..(rect_count as u32 * 6), 0, 0..1);
            }
        }

        drop(batch);
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Clears the screen with the given color and presents it (simple path).
    /// For backward compatibility — use present() for game rendering.
    pub fn clear(&self, r: f32, g: f32, b: f32, a: f32) -> Result<(), RenderError> {
        self.present(r, g, b, a)
    }

    /// Begins a new render pass, returning a `FrameRenderer` for drawing.
    pub fn begin_frame(&self) -> Result<FrameRenderer, RenderError> {
        let frame = self.surface.get_current_texture()
            .map_err(|e| RenderError::SurfaceFailed(e.to_string()))?;

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(FrameRenderer {
            frame: Some(frame),
            view,
        })
    }
}

/// A single-frame renderer that manages the GPU command buffer.
pub struct FrameRenderer {
    frame: Option<wgpu::SurfaceTexture>,
    view: wgpu::TextureView,
}

impl FrameRenderer {
    /// Returns a reference to the color attachment view.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Presents the frame to the screen.
    pub fn present(self) {
        if let Some(frame) = self.frame {
            frame.present();
        }
    }
}

fn create_fullscreen_quad() -> (Vec<SpriteVertex>, Vec<u16>) {
    let vertices = vec![
        SpriteVertex { position: [-1.0, -1.0], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        SpriteVertex { position: [ 1.0, -1.0], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        SpriteVertex { position: [ 1.0,  1.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
        SpriteVertex { position: [-1.0,  1.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
    ];
    let indices: Vec<u16> = vec![0, 1, 2, 0, 2, 3];
    (vertices, indices)
}

fn create_solid_color_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    pixel: [u8; 4],
) -> GpuTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Solid Color Texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixel,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuTexture { texture, view, width, height }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_error_conversion() {
        let err = RenderError::AdapterFailed("no GPU".into());
        let runtime_err: RuntimeError = err.into();
        assert_eq!(runtime_err.code, vibege_core::ErrorCode::INIT_FAILED);
    }

    #[test]
    fn test_vertex_buffer_layout() {
        let desc = SpriteVertex::desc();
        assert_eq!(desc.attributes.len(), 3);
    }

    #[test]
    fn test_fullscreen_quad() {
        let (verts, indices) = create_fullscreen_quad();
        assert_eq!(verts.len(), 4);
        assert_eq!(indices.len(), 6);
        assert!((verts[0].position[0] - (-1.0)).abs() < 0.001);
        assert!((verts[2].position[0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vertex_bytemuck() {
        let vertex = SpriteVertex {
            position: [0.0, 0.0],
            tex_coords: [0.5, 0.5],
            color: [1.0, 0.0, 0.0, 1.0],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&vertex);
        assert_eq!(bytes.len(), std::mem::size_of::<SpriteVertex>());
    }

    #[test]
    fn test_sprite_vertex_size() {
        assert_eq!(std::mem::size_of::<SpriteVertex>(), 32);
    }
}
