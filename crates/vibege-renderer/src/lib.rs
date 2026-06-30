#![allow(clippy::too_many_arguments)]

use std::sync::Arc;
use std::sync::Mutex;

use tracing::{debug, info};
use vibege_core::RuntimeError;
use wgpu::util::DeviceExt;

mod font;

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
                wgpu::VertexAttribute {
                    offset: 0,
                    format: wgpu::VertexFormat::Float32x2,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    format: wgpu::VertexFormat::Float32x2,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    format: wgpu::VertexFormat::Float32x4,
                    shader_location: 2,
                },
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
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    },
    Sprite {
        tex_idx: usize,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    /// A single glyph from the font atlas with explicit UV sub-rect.
    Glyph {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u1: f32,
        v1: f32,
        u2: f32,
        v2: f32,
        r: f32,
        g: f32,
        b: f32,
    },
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

    font_bind_group: wgpu::BindGroup, // bitmap font atlas
    font_tex_w: u32,                  // font atlas width in pixels
    font_tex_h: u32,                  // font atlas height in pixels
    font_chars_per_row: u32,          // glyphs per row in atlas

    draw_list: Mutex<Vec<DrawCmd>>,
    clear_color: Mutex<(f32, f32, f32, f32)>,
    screen_size: (f32, f32),
}

impl Renderer {
    pub async fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        let size = (width, height);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| RenderError::SurfaceFailed(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| RenderError::AdapterFailed("No suitable GPU adapter found".into()))?;

        info!(adapter = %adapter.get_info().name, backend = ?adapter.get_info().backend, "GPU adapter selected");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("VibeGE Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| RenderError::DeviceFailed(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(
                surface_caps
                    .formats
                    .first()
                    .copied()
                    .unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb),
            );

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
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Default sampler — Nearest filtering for pixel-art crispness
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Default Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
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
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&default_tex.view),
                },
            ],
        });

        // Create font atlas texture from embedded bitmap font
        let font_rgba = font::font_atlas_rgba();
        let font_w = 128u32;
        let font_h = 48u32;
        let font_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font Atlas"),
            size: wgpu::Extent3d {
                width: font_w,
                height: font_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &font_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &font_rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * font_w),
                rows_per_image: Some(font_h),
            },
            wgpu::Extent3d {
                width: font_w,
                height: font_h,
                depth_or_array_layers: 1,
            },
        );
        let font_view = font_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let font_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Font BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&font_view),
                },
            ],
        });

        info!(width = size.0, height = size.1, format = ?config.format, "Renderer initialised");

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            bind_group_layout,
            sampler,
            default_bind_group,
            font_bind_group,
            font_tex_w: font_w,
            font_tex_h: font_h,
            font_chars_per_row: 16,
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
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
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
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("user_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });

        let tex_idx = {
            let mut groups = self.texture_bind_groups.lock().expect("lock");
            let idx = groups.len();
            groups.push(bind_group);
            idx
        };
        debug!(idx = tex_idx, width, height, "Texture loaded");
        Ok(tex_idx)
    }

    /// Queue a colored rectangle for the next frame.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) {
        self.draw_list.lock().expect("lock").push(DrawCmd::Rect {
            x,
            y,
            w,
            h,
            r,
            g,
            b,
            a,
        });
    }

    /// Queue a textured sprite for the next frame.
    pub fn draw_sprite(&self, tex_idx: usize, x: f32, y: f32, w: f32, h: f32) {
        self.draw_list.lock().expect("lock").push(DrawCmd::Sprite {
            tex_idx,
            x,
            y,
            w,
            h,
        });
    }

    /// Draw text using the embedded 8×8 monospace bitmap font.
    /// `char_w` = width in pixels of one character (e.g. 8.0 for 1:1 scale, 16.0 for 2x).
    /// Character height = `char_w * 1.0` (square glyphs).
    /// Only ASCII 32–126 is supported; out-of-range chars render as space.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(&self, x: f32, y: f32, text: &str, char_w: f32, r: f32, g: f32, b: f32) {
        let char_h = char_w; // square glyphs
        let atlas_w = self.font_tex_w as f32;
        let atlas_h = self.font_tex_h as f32;
        let glyph_uv_w = 8.0 / atlas_w; // each glyph is 8×8 pixels in the atlas
        let glyph_uv_h = 8.0 / atlas_h;

        let mut list = self.draw_list.lock().expect("lock");
        for (i, ch) in text.chars().enumerate() {
            let mut code = ch as u8;
            #[allow(clippy::manual_range_contains)]
            if code < b' ' || code > b'~' {
                code = b' ';
            }
            let local_idx = (code - b' ') as u32;
            let col = local_idx % self.font_chars_per_row;
            let row = local_idx / self.font_chars_per_row;
            let u1 = col as f32 * glyph_uv_w;
            let v1 = row as f32 * glyph_uv_h;
            let u2 = u1 + glyph_uv_w;
            let v2 = v1 + glyph_uv_h;
            let gx = x + i as f32 * char_w;
            list.push(DrawCmd::Glyph {
                x: gx,
                y,
                w: char_w,
                h: char_h,
                u1,
                v1,
                u2,
                v2,
                r,
                g,
                b,
            });
        }
    }

    /// Set the background clear color.
    pub fn set_clear(&self, r: f32, g: f32, b: f32, a: f32) {
        *self.clear_color.lock().expect("lock") = (r, g, b, a);
    }

    /// Render all queued commands and present the frame.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&self) -> Result<(), RenderError> {
        #[derive(PartialEq)]
        enum BgKind {
            Font,
            Texture(usize),
        }

        let clear = *self.clear_color.lock().expect("lock");
        let frame = self
            .surface
            .get_current_texture()
            .map_err(|e| RenderError::SurfaceFailed(e.to_string()))?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Always begin a render pass — clears the screen even with zero draw calls
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Game Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear.0 as f64,
                        g: clear.1 as f64,
                        b: clear.2 as f64,
                        a: clear.3 as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        #[allow(clippy::too_many_arguments)]
        fn add_rect(
            x: f32,
            y: f32,
            w: f32,
            h: f32,
            r: f32,
            g: f32,
            b: f32,
            a: f32,
            sw: f32,
            sh: f32,
            verts: &mut Vec<SpriteVertex>,
            idxs: &mut Vec<u16>,
        ) {
            let x1 = (x / sw) * 2.0 - 1.0;
            let y1 = 1.0 - (y / sh) * 2.0;
            let x2 = ((x + w) / sw) * 2.0 - 1.0;
            let y2 = 1.0 - ((y + h) / sh) * 2.0;
            let base = verts.len() as u16;
            verts.push(SpriteVertex {
                position: [x1, y1],
                tex_coords: [0.0, 0.0],
                color: [r, g, b, a],
            });
            verts.push(SpriteVertex {
                position: [x2, y1],
                tex_coords: [1.0, 0.0],
                color: [r, g, b, a],
            });
            verts.push(SpriteVertex {
                position: [x2, y2],
                tex_coords: [1.0, 1.0],
                color: [r, g, b, a],
            });
            verts.push(SpriteVertex {
                position: [x1, y2],
                tex_coords: [0.0, 1.0],
                color: [r, g, b, a],
            });
            idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        #[allow(clippy::too_many_arguments)]
        fn add_sprite(
            _tex_idx: usize,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
            sw: f32,
            sh: f32,
            verts: &mut Vec<SpriteVertex>,
            idxs: &mut Vec<u16>,
        ) {
            let x1 = (x / sw) * 2.0 - 1.0;
            let y1 = 1.0 - (y / sh) * 2.0;
            let x2 = ((x + w) / sw) * 2.0 - 1.0;
            let y2 = 1.0 - ((y + h) / sh) * 2.0;
            let base = verts.len() as u16;
            verts.push(SpriteVertex {
                position: [x1, y1],
                tex_coords: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
            verts.push(SpriteVertex {
                position: [x2, y1],
                tex_coords: [1.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
            verts.push(SpriteVertex {
                position: [x2, y2],
                tex_coords: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
            verts.push(SpriteVertex {
                position: [x1, y2],
                tex_coords: [0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
            idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        #[allow(clippy::too_many_arguments)]
        fn add_glyph(
            x: f32,
            y: f32,
            w: f32,
            h: f32,
            u1: f32,
            v1: f32,
            u2: f32,
            v2: f32,
            r: f32,
            g: f32,
            b: f32,
            sw: f32,
            sh: f32,
            verts: &mut Vec<SpriteVertex>,
            idxs: &mut Vec<u16>,
        ) {
            let x1 = (x / sw) * 2.0 - 1.0;
            let y1 = 1.0 - (y / sh) * 2.0;
            let x2 = ((x + w) / sw) * 2.0 - 1.0;
            let y2 = 1.0 - ((y + h) / sh) * 2.0;
            let base = verts.len() as u16;
            verts.push(SpriteVertex {
                position: [x1, y1],
                tex_coords: [u1, v1],
                color: [r, g, b, 1.0],
            });
            verts.push(SpriteVertex {
                position: [x2, y1],
                tex_coords: [u2, v1],
                color: [r, g, b, 1.0],
            });
            verts.push(SpriteVertex {
                position: [x2, y2],
                tex_coords: [u2, v2],
                color: [r, g, b, 1.0],
            });
            verts.push(SpriteVertex {
                position: [x1, y2],
                tex_coords: [u1, v2],
                color: [r, g, b, 1.0],
            });
            idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        fn set_bind_group(pass: &mut wgpu::RenderPass, renderer: &Renderer, bg: &Option<BgKind>) {
            match bg {
                Some(BgKind::Font) => pass.set_bind_group(0, &renderer.font_bind_group, &[]),
                Some(BgKind::Texture(idx)) => {
                    let groups = renderer.texture_bind_groups.lock().expect("lock");
                    if *idx < groups.len() {
                        pass.set_bind_group(0, &groups[*idx], &[]);
                    } else {
                        pass.set_bind_group(0, &renderer.default_bind_group, &[]);
                    }
                }
                _ => pass.set_bind_group(0, &renderer.default_bind_group, &[]),
            }
        }

        fn draw_batch(
            pass: &mut wgpu::RenderPass,
            renderer: &Renderer,
            verts: &mut Vec<SpriteVertex>,
            idxs: &mut Vec<u16>,
            bg: &Option<BgKind>,
        ) {
            if verts.is_empty() {
                return;
            }
            let vb = renderer
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Batch VB"),
                    contents: bytemuck::cast_slice(verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let ib = renderer
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Batch IB"),
                    contents: bytemuck::cast_slice(idxs),
                    usage: wgpu::BufferUsages::INDEX,
                });
            pass.set_pipeline(&renderer.pipeline);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint16);
            set_bind_group(pass, renderer, bg);
            pass.draw_indexed(0..idxs.len() as u32, 0, 0..1);
            verts.clear();
            idxs.clear();
        }

        let (sw, sh) = self.screen_size;
        let draw_cmds = self
            .draw_list
            .lock()
            .expect("lock")
            .drain(..)
            .collect::<Vec<_>>();
        let mut vertices: Vec<SpriteVertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut current_bg: Option<BgKind> = None;

        for cmd in &draw_cmds {
            let need_bg: Option<BgKind> = match cmd {
                DrawCmd::Rect { .. } => None,
                DrawCmd::Sprite { tex_idx, .. } => Some(BgKind::Texture(*tex_idx)),
                DrawCmd::Glyph { .. } => Some(BgKind::Font),
            };

            // Flush on bind group change
            if need_bg != current_bg && !vertices.is_empty() {
                draw_batch(&mut pass, self, &mut vertices, &mut indices, &current_bg);
            }
            current_bg = need_bg;

            match cmd {
                DrawCmd::Rect {
                    x,
                    y,
                    w,
                    h,
                    r,
                    g,
                    b,
                    a,
                } => {
                    add_rect(
                        *x,
                        *y,
                        *w,
                        *h,
                        *r,
                        *g,
                        *b,
                        *a,
                        sw,
                        sh,
                        &mut vertices,
                        &mut indices,
                    );
                }
                DrawCmd::Sprite {
                    tex_idx,
                    x,
                    y,
                    w,
                    h,
                } => {
                    add_sprite(
                        *tex_idx,
                        *x,
                        *y,
                        *w,
                        *h,
                        sw,
                        sh,
                        &mut vertices,
                        &mut indices,
                    );
                }
                DrawCmd::Glyph {
                    x,
                    y,
                    w,
                    h,
                    u1,
                    v1,
                    u2,
                    v2,
                    r,
                    g,
                    b,
                } => {
                    add_glyph(
                        *x,
                        *y,
                        *w,
                        *h,
                        *u1,
                        *v1,
                        *u2,
                        *v2,
                        *r,
                        *g,
                        *b,
                        sw,
                        sh,
                        &mut vertices,
                        &mut indices,
                    );
                }
            }
        }

        // Flush final batch
        draw_batch(&mut pass, self, &mut vertices, &mut indices, &current_bg);

        drop(pass);
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

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }
}

fn create_solid_color_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    pixel: [u8; 4],
) -> RawTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Solid Color Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
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
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    RawTexture {
        _texture: texture,
        view,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_list_queue_and_drain() {
        let list = Mutex::new(Vec::new());
        list.lock().unwrap().push(DrawCmd::Rect {
            x: 10.0, y: 20.0, w: 100.0, h: 50.0,
            r: 1.0, g: 0.0, b: 0.0, a: 1.0,
        });
        list.lock().unwrap().push(DrawCmd::Sprite { tex_idx: 0, x: 0.0, y: 0.0, w: 32.0, h: 32.0 });
        list.lock().unwrap().push(DrawCmd::Glyph {
            x: 50.0, y: 50.0, w: 8.0, h: 8.0,
            u1: 0.0, v1: 0.0, u2: 0.0625, v2: 0.1667,
            r: 1.0, g: 1.0, b: 1.0,
        });
        let cmds = list.lock().unwrap().drain(..).collect::<Vec<_>>();
        assert_eq!(cmds.len(), 3);
        match &cmds[0] {
            DrawCmd::Rect { x, y, .. } => { assert_eq!(*x, 10.0); assert_eq!(*y, 20.0); }
            _ => panic!("Expected Rect"),
        }
    }

    #[test]
    fn test_clear_color_storage() {
        let clear_color = Mutex::new((0.0f32, 0.0f32, 0.0f32, 1.0f32));
        *clear_color.lock().unwrap() = (0.1, 0.2, 0.3, 0.5);
        let (r, g, b, a) = *clear_color.lock().unwrap();
        assert!((r - 0.1).abs() < 1e-6f32);
        assert!((g - 0.2).abs() < 1e-6f32);
        assert!((b - 0.3).abs() < 1e-6f32);
        assert!((a - 0.5).abs() < 1e-6f32);
    }

    #[test]
    fn test_rect_ndc_conversion() {
        let sw: f32 = 800.0;
        let sh: f32 = 600.0;
        let x1 = (0.0 / sw) * 2.0 - 1.0;
        let y1 = 1.0 - (0.0 / sh) * 2.0;
        let x2 = (800.0 / sw) * 2.0 - 1.0;
        let y2 = 1.0 - (600.0 / sh) * 2.0;
        assert!((x1 - (-1.0)).abs() < 1e-6f32);
        assert!((y1 - 1.0).abs() < 1e-6f32);
        assert!((x2 - 1.0).abs() < 1e-6f32);
        assert!((y2 - (-1.0)).abs() < 1e-6f32);
    }
}
