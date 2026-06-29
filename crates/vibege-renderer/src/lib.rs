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
    #[error("Texture not found: {0}")]
    TextureNotFound(String),
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

/// A simple texture without a bind group (for internal use).
struct RawTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// A draw command stored per frame — either a colored rect or a textured sprite.
enum DrawCmd {
    Rect { x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32 },
    Sprite { tex_idx: usize, x: f32, y: f32, w: f32, h: f32 },
}

/// The GPU renderer.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    default_bind_group: wgpu::BindGroup,
    texture_bind_groups: Mutex<Vec<wgpu::BindGroup>>,

    draw_list: Mutex<Vec<DrawCmd>>,
    clear_color: Mutex<(f32, f32, f32, f32)>,
    screen_size: (f32, f32),
}

impl Renderer {
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
        }).await.ok_or_else(|| RenderError::AdapterFailed("No suitable GPU adapter found".into()))?;

        info!(adapter = %adapter.get_info().name, backend = ?adapter.get_info().backend, "GPU adapter selected");

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("VibeGE Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            }, None,
        ).await.map_err(|e| RenderError::DeviceFailed(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb()).copied().unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.0.max(1), height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Shader with texture sampling
        let shader_source = include_str!("shaders/shader.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sprite Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // Bind group layout: sampler + texture
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sprite Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sprite Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: "vs_main",
                buffers: &[SpriteVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None, cache: None,
        });

        // Default sampler
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

        // Create default white texture bind group (for untextured rects)
        let default_tex = create_solid_color_texture(&device, &queue, 1, 1, [255, 255, 255, 255]);
        let default_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Default White BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&default_tex.view) },
            ],
        });

        info!(width = size.0, height = size.1, format = ?config.format, "Renderer initialised");

        Ok(Self {
            surface, device, queue, config, size, pipeline, bind_group_layout, sampler,
            default_bind_group,
            texture_bind_groups: Mutex::new(Vec::new()),
            draw_list: Mutex::new(Vec::new()),
            clear_color: Mutex::new((0.0, 0.0, 0.0, 1.0)),
            screen_size: (size.0 as f32, size.1 as f32),
        })
    }

    /// Load a PNG texture from file bytes. Returns a texture index for drawing.
    pub fn load_texture(&self, data: &[u8]) -> Result<usize, RenderError> {
        let img = image::load_from_memory(data)
            .map_err(|e| RenderError::TextureLoadFailed(e.to_string()))?
            .to_rgba8();
        let (width, height) = img.dimensions();

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("user_tex"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("user_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
            ],
        });

        let tex_idx = {
            let mut groups = self.texture_bind_groups.lock().unwrap();
            let idx = groups.len();
            groups.push(bind_group);
            idx
        };
        debug!(idx = tex_idx, width, height, "Texture loaded");
        Ok(tex_idx)
    }

    /// Queue a colored rectangle for the next frame.
    pub fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) {
        self.draw_list.lock().unwrap().push(DrawCmd::Rect { x, y, w, h, r, g, b, a });
    }

    /// Queue a textured sprite for the next frame.
    pub fn draw_sprite(&self, tex_idx: usize, x: f32, y: f32, w: f32, h: f32) {
        self.draw_list.lock().unwrap().push(DrawCmd::Sprite { tex_idx, x, y, w, h });
    }

    /// Set the background clear color.
    pub fn set_clear(&self, r: f32, g: f32, b: f32, a: f32) {
        *self.clear_color.lock().unwrap() = (r, g, b, a);
    }

    /// Render all queued commands and present the frame.
    pub fn render(&self) -> Result<(), RenderError> {
        let clear = *self.clear_color.lock().unwrap();
        let frame = self.surface.get_current_texture()
            .map_err(|e| RenderError::SurfaceFailed(e.to_string()))?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let mut vertices: Vec<SpriteVertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut tex_id_for_batch: Option<usize> = None;
        let (sw, sh) = self.screen_size;

        // Convert draw commands to vertices
        let draw_cmds = self.draw_list.lock().unwrap().drain(..).collect::<Vec<_>>();
        for cmd in &draw_cmds {
            match cmd {
                DrawCmd::Rect { x, y, w, h, r, g, b, a } => {
                    let x1 = (x / sw) * 2.0 - 1.0;
                    let y1 = 1.0 - (y / sh) * 2.0;
                    let x2 = ((x + w) / sw) * 2.0 - 1.0;
                    let y2 = 1.0 - ((y + h) / sh) * 2.0;
                    let base = vertices.len() as u16;
                    vertices.push(SpriteVertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: [*r, *g, *b, *a] });
                    vertices.push(SpriteVertex { position: [x2, y1], tex_coords: [1.0, 0.0], color: [*r, *g, *b, *a] });
                    vertices.push(SpriteVertex { position: [x2, y2], tex_coords: [1.0, 1.0], color: [*r, *g, *b, *a] });
                    vertices.push(SpriteVertex { position: [x1, y2], tex_coords: [0.0, 1.0], color: [*r, *g, *b, *a] });
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
                DrawCmd::Sprite { tex_idx, x, y, w, h } => {
                    let x1 = (x / sw) * 2.0 - 1.0;
                    let y1 = 1.0 - (y / sh) * 2.0;
                    let x2 = ((x + w) / sw) * 2.0 - 1.0;
                    let y2 = 1.0 - ((y + h) / sh) * 2.0;
                    let base = vertices.len() as u16;
                    vertices.push(SpriteVertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] });
                    vertices.push(SpriteVertex { position: [x2, y1], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] });
                    vertices.push(SpriteVertex { position: [x2, y2], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] });
                    vertices.push(SpriteVertex { position: [x1, y2], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] });
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                    tex_id_for_batch = Some(*tex_idx);
                }
            }
        }

        // Get the bind group for the texture (or default white)
        // Get bind group by index. Store index to look up later.
        let tex_idx = tex_id_for_batch;
        let _bg: &wgpu::BindGroup = &self.default_bind_group; // default fallback

        if !vertices.is_empty() {
            let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sprite VB"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sprite IB"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            // Look up bind group inside the pass scope
            let bg_for_pass = tex_idx.and_then(|idx| {
                let groups = self.texture_bind_groups.lock().unwrap();
                if idx < groups.len() { Some(&groups[idx] as *const wgpu::BindGroup) } else { None }
            });

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Game Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear.0 as f64, g: clear.1 as f64, b: clear.2 as f64, a: clear.3 as f64 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint16);
            if let Some(ptr) = bg_for_pass {
                unsafe { pass.set_bind_group(0, &*ptr, &[]); }
            } else {
                pass.set_bind_group(0, &self.default_bind_group, &[]);
            }
            pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Resize the output surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.size = (width, height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.screen_size = (width as f32, height as f32);
        }
    }

    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }
    pub fn surface_format(&self) -> wgpu::TextureFormat { self.config.format }
}

fn create_solid_color_texture(
    device: &wgpu::Device, queue: &wgpu::Queue,
    width: u32, height: u32, pixel: [u8; 4],
) -> RawTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Solid Color Texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &pixel,
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    RawTexture { _texture: texture, view }
}
