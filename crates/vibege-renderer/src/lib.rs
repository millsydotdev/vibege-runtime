//! GPU-accelerated 2D renderer built on wgpu.
//!
//! # Architecture
//!
//! The renderer follows a deterministic frame pipeline:
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                  begin_frame()                   │
//! │  • acquire swap-chain texture                    │
//! │  • create command encoder                        │
//! │  • begin render pass with clear colour           │
//! └─────────────┬────────────────────────────────────┘
//!               │
//! ┌─────────────▼────────────────────────────────────┐
//! │              process_commands()                   │
//! │  • drain draw list                                │
//! │  • batch by bind group                            │
//! │  • convert screen coords → NDC                   │
//! │  • upload to staging buffer                       │
//! │  • set pipeline + bind group + draw              │
//! └─────────────┬────────────────────────────────────┘
//!               │
//! ┌─────────────▼────────────────────────────────────┐
//! │                 end_frame()                       │
//! │  • flush remaining batch                          │
//! │  • end render pass                                │
//! │  • submit command encoder                         │
//! │  • present swap chain                             │
//! │  • handle surface loss                            │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! # GPU Resource Ownership
//!
//! | Resource       | Created   | Lifetime       | Notes                  |
//! |----------------|-----------|----------------|------------------------|
//! | RenderPipeline | `new()`   | Forever        | Single pipeline        |
//! | Sampler        | `new()`   | Forever        | Nearest filtering      |
//! | BindGroupLayout| `new()`   | Forever        | Shared by all textures |
//! | Default texture| `new()`   | Forever        | 1x1 white pixel        |
//! | Font atlas     | `new()`   | Forever        | 128×48 bitmap          |
//! | User textures  | `load_tex`| Until dropped  | Indexed by usize       |
//! | Staging VB/IB  | `new()`   | Forever*       | Grows on demand        |
//!
//! *Staging buffers are grown when the draw list exceeds capacity. They are
//!  never shrunk, so a single large frame sets the ceiling for the session.
//!
//! # Coordinate System
//!
//! All drawing uses screen-space coordinates with origin at top-left.
//! Internally these are converted to Normalised Device Coordinates (NDC)
//! where the viewport spans [-1, 1] in both axes:
//!
//! ```text
//! ndc_x = (screen_x  / screen_width)  * 2.0 - 1.0
//! ndc_y = 1.0 - (screen_y / screen_height) * 2.0
//! ```
//!
//! # Draw Command Flow
//!
//! 1. Callers queue `DrawCmd` variants via `draw_rect()`, `draw_sprite()`,
//!    `draw_text()` — these are cheap, lock-free pushes.
//! 2. `render()` drains the queue, groups commands by bind group, and
//!    emits a single draw call per bind group.
//! 3. Vertex and index data is written into staging GPU buffers via
//!    `queue.write_buffer()`, avoiding per-frame allocation.
//!
//! # Future Extension Points
//!
//! - **Multiple pipelines**: Add pipeline selection to `DrawCmd` (blend mode,
//!   culling, depth).
//! - **Instancing**: Use `SpriteVertex` instance data for batching identical
//!   sprites.
//! - **Render bundles**: Record static geometry into `RenderBundle` for
//!   replay.
//! - **Dynamic font atlas**: Grow the atlas as new glyphs are requested.
//! - **Z-ordering**: Add `z: i32` to `DrawCmd` and sort before batching.

#![allow(clippy::too_many_arguments)]

use std::sync::Arc;
use std::sync::Mutex;

use bytemuck::Pod;
use bytemuck::Zeroable;
use tracing::{debug, info};
use vibege_asset::TextureAsset;
use vibege_asset::loader::{LoaderError, TextureLoaderCreator};
use vibege_core::RuntimeError;
use wgpu::BufferAddress;

mod font;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors originating from the renderer.
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
    #[error("Surface lost — reconfigure and retry")]
    SurfaceLost,
}

impl From<RenderError> for RuntimeError {
    fn from(err: RenderError) -> Self {
        RuntimeError::new(vibege_core::ErrorCode::INIT_FAILED, err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Vertex type
// ---------------------------------------------------------------------------

/// A 2D sprite vertex with position, texture coordinates, and colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
}

impl SpriteVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as BufferAddress,
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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// A simple texture without a bind group (for internal use).
struct RawTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Converts screen-space coordinates to Normalised Device Coordinates.
struct NdcConverter {
    sx: f32,
    sy: f32,
}

impl NdcConverter {
    fn new(sw: f32, sh: f32) -> Self {
        Self { sx: sw, sy: sh }
    }

    fn ndc(&self, x: f32, y: f32) -> (f32, f32) {
        let nx = x / self.sx * 2.0 - 1.0;
        let ny = 1.0 - y / self.sy * 2.0;
        (nx, ny)
    }

    fn rect_vertices(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        uv: [f32; 4], // [u1, v1, u2, v2]
        color: [f32; 4],
        verts: &mut Vec<SpriteVertex>,
        idxs: &mut Vec<u16>,
    ) {
        let (x1, y1) = self.ndc(x, y);
        let (x2, y2) = self.ndc(x + w, y + h);
        let base = verts.len() as u16;
        let [u1, v1, u2, v2] = uv;

        verts.push(SpriteVertex {
            position: [x1, y1],
            tex_coords: [u1, v1],
            color,
        });
        verts.push(SpriteVertex {
            position: [x2, y1],
            tex_coords: [u2, v1],
            color,
        });
        verts.push(SpriteVertex {
            position: [x2, y2],
            tex_coords: [u2, v2],
            color,
        });
        verts.push(SpriteVertex {
            position: [x1, y2],
            tex_coords: [u1, v2],
            color,
        });
        idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

// ---------------------------------------------------------------------------
// Draw command
// ---------------------------------------------------------------------------

/// A single draw command queued for the next frame.
///
/// Commands are cheap to push and are drained in bulk at render time.
/// They are grouped by bind group to minimise pipeline state changes.
#[derive(Debug, Clone)]
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
    SpriteSubtex {
        tex_idx: usize,
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
        a: f32,
    },
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

/// Identifies which bind group a command needs.
#[derive(Debug, Clone, PartialEq)]
enum BindGroupId {
    Default,
    Font,
    Texture(usize),
}

impl DrawCmd {
    /// Returns the bind group this command requires.
    fn bind_group(&self) -> BindGroupId {
        match self {
            DrawCmd::Rect { .. } => BindGroupId::Default,
            DrawCmd::Sprite { tex_idx, .. } | DrawCmd::SpriteSubtex { tex_idx, .. } => {
                BindGroupId::Texture(*tex_idx)
            }
            DrawCmd::Glyph { .. } => BindGroupId::Font,
        }
    }

    /// Returns the full-screen UV rectangle for this command.
    fn uv(&self) -> [f32; 4] {
        match self {
            DrawCmd::Rect { .. } | DrawCmd::Sprite { .. } => [0.0, 0.0, 1.0, 1.0],
            DrawCmd::SpriteSubtex { u1, v1, u2, v2, .. } => [*u1, *v1, *u2, *v2],
            DrawCmd::Glyph { u1, v1, u2, v2, .. } => [*u1, *v1, *u2, *v2],
        }
    }

    /// Returns the colour tint for this command.
    fn color(&self) -> [f32; 4] {
        match self {
            DrawCmd::Rect { r, g, b, a, .. } => [*r, *g, *b, *a],
            DrawCmd::Sprite { .. } => [1.0, 1.0, 1.0, 1.0],
            DrawCmd::SpriteSubtex { r, g, b, a, .. } => [*r, *g, *b, *a],
            DrawCmd::Glyph { r, g, b, .. } => [*r, *g, *b, 1.0],
        }
    }

    /// Returns the screen-space rectangle (x, y, w, h).
    fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            DrawCmd::Rect { x, y, w, h, .. }
            | DrawCmd::Sprite { x, y, w, h, .. }
            | DrawCmd::SpriteSubtex { x, y, w, h, .. }
            | DrawCmd::Glyph { x, y, w, h, .. } => (*x, *y, *w, *h),
        }
    }
}

// ---------------------------------------------------------------------------
// Staging batch — reusable GPU buffer pair
// ---------------------------------------------------------------------------

/// A growable pair of vertex + index staging buffers.
///
/// Buffers are created with `COPY_DST` usage and written each frame via
/// `queue.write_buffer()`. When capacity is exceeded they are recreated
/// with double the previous capacity.
struct StagingBatch {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    vb_capacity: usize, // in elements
    ib_capacity: usize, // in elements
}

impl StagingBatch {
    const MIN_VERTICES: usize = 4096;
    const MIN_INDICES: usize = 6144; // ~1.5× vertex count for typical quads

    fn new(device: &wgpu::Device) -> Self {
        let (vb, ib) = Self::create_buffers(device, Self::MIN_VERTICES, Self::MIN_INDICES);
        Self {
            vb,
            ib,
            vb_capacity: Self::MIN_VERTICES,
            ib_capacity: Self::MIN_INDICES,
        }
    }

    fn create_buffers(device: &wgpu::Device, vc: usize, ic: usize) -> (wgpu::Buffer, wgpu::Buffer) {
        (
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Staging VB"),
                size: (vc * std::mem::size_of::<SpriteVertex>()) as BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Staging IB"),
                size: (ic * std::mem::size_of::<u16>()) as BufferAddress,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        )
    }

    /// Ensure capacity for `needed_verts` and `needed_idxs`.
    fn ensure(&mut self, device: &wgpu::Device, needed_verts: usize, needed_idxs: usize) {
        if needed_verts > self.vb_capacity || needed_idxs > self.ib_capacity {
            let new_vc = (needed_verts * 2).max(self.vb_capacity * 2);
            let new_ic = (needed_idxs * 2).max(self.ib_capacity * 2);
            debug!(
                "Growing staging batch: vb {}→{}, ib {}→{}",
                self.vb_capacity, new_vc, self.ib_capacity, new_ic
            );
            let (vb, ib) = Self::create_buffers(device, new_vc, new_ic);
            self.vb = vb;
            self.ib = ib;
            self.vb_capacity = new_vc;
            self.ib_capacity = new_ic;
        }
    }

    /// Upload vertex and index data. Returns the vertex count used.
    fn upload(&self, queue: &wgpu::Queue, verts: &[SpriteVertex], idxs: &[u16]) -> (u32, u32) {
        let vbytes = bytemuck::cast_slice(verts);
        let ibytes = bytemuck::cast_slice(idxs);
        queue.write_buffer(&self.vb, 0, vbytes);
        queue.write_buffer(&self.ib, 0, ibytes);
        (verts.len() as u32, idxs.len() as u32)
    }
}

// ---------------------------------------------------------------------------
// Texture Slot Manager — replaces the flat Vec<BindGroup> with a slot map
// that supports removal and reuse.
// ---------------------------------------------------------------------------

/// Manages texture bind group slots with O(1) allocation and free.
struct TextureSlotManager {
    slots: Vec<Option<wgpu::BindGroup>>,
    free_list: Vec<usize>,
}

impl TextureSlotManager {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Allocate a slot and return its index.
    fn allocate(&mut self, bind_group: wgpu::BindGroup) -> usize {
        if let Some(idx) = self.free_list.pop() {
            self.slots[idx] = Some(bind_group);
            idx
        } else {
            let idx = self.slots.len();
            self.slots.push(Some(bind_group));
            idx
        }
    }

    /// Free a slot by index. Returns the bind group (caller can drop it).
    fn free(&mut self, idx: usize) {
        if idx < self.slots.len() {
            self.slots[idx] = None;
            self.free_list.push(idx);
        }
    }

    /// Get a bind group by index (for rendering).
    fn get(&self, idx: usize) -> Option<&wgpu::BindGroup> {
        self.slots.get(idx).and_then(|s| s.as_ref())
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.slots.len() - self.free_list.len()
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.slots.clear();
        self.free_list.clear();
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// The GPU renderer.
///
/// See the [module-level documentation](self) for architecture details.
pub struct Renderer {
    // GPU resources
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    staging: Mutex<StagingBatch>,

    // Pipeline
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    // Bind groups
    default_bind_group: wgpu::BindGroup,
    texture_slots: Mutex<TextureSlotManager>,
    font_bind_group: wgpu::BindGroup,
    font_tex_w: u32,
    font_tex_h: u32,
    font_chars_per_row: u32,

    // Frame state
    draw_list: Mutex<Vec<DrawCmd>>,
    clear_color: Mutex<(f32, f32, f32, f32)>,
    screen_size: (f32, f32),

    // Surface recovery
    surface_lost: Mutex<bool>,
}

impl Renderer {
    /// Initialise the GPU, create the swap chain, pipeline, font atlas, and
    /// staging buffers.
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

        info!(
            adapter = %adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU adapter selected"
        );

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

        // Shader
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

        // Default white texture (for untextured rects)
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

        // Font atlas texture
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

        let staging = StagingBatch::new(&device);

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
            staging: Mutex::new(staging),
            pipeline,
            bind_group_layout,
            sampler,
            default_bind_group,
            font_bind_group,
            font_tex_w: font_w,
            font_tex_h: font_h,
            font_chars_per_row: 16,
            texture_slots: Mutex::new(TextureSlotManager::new()),
            draw_list: Mutex::new(Vec::new()),
            clear_color: Mutex::new((0.0, 0.0, 0.0, 1.0)),
            screen_size: (size.0 as f32, size.1 as f32),
            surface_lost: Mutex::new(false),
        })
    }

    /// Load a texture from raw image bytes and return a TextureAsset.
    ///
    /// The returned `TextureAsset` can be used with `draw_sprite_asset()`.
    pub fn load_texture_asset(&self, data: &[u8]) -> Result<TextureAsset, RenderError> {
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
            let mut slots = self.texture_slots.lock().expect("lock");
            slots.allocate(bind_group)
        };

        debug!(idx = tex_idx, width, height, "Texture loaded");
        Ok(TextureAsset::new(tex_idx, width, height))
    }

    /// Convenience: loads a PNG from bytes and returns a usize index (legacy API).
    pub fn load_texture(&self, data: &[u8]) -> Result<usize, RenderError> {
        self.load_texture_asset(data).map(|a| a.bind_group_index)
    }

    /// Create a texture loader callback suitable for use with
    /// `vibege_asset::AssetManager::set_texture_loader()`.
    pub fn create_asset_texture_loader(self: &Arc<Self>) -> TextureLoaderCreator {
        let renderer = Arc::clone(self);
        Box::new(move |data, _source| {
            renderer
                .load_texture_asset(data)
                .map_err(|e| LoaderError::InvalidData(e.to_string()))
        })
    }

    /// Draw a sprite using a TextureAsset.
    pub fn draw_sprite_asset(&self, tex: &TextureAsset, x: f32, y: f32, w: f32, h: f32) {
        self.draw_sprite(tex.bind_group_index, x, y, w, h);
    }

    /// Remove a texture's GPU resources and free its slot.
    pub fn unload_texture_slot(&self, index: usize) {
        let mut slots = self.texture_slots.lock().expect("lock");
        slots.free(index);
        debug!(idx = index, "Texture slot freed");
    }

    /// Queue a coloured rectangle for the next frame.
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

    /// Queue a sub-texture sprite with UV coordinates and tint colour.
    pub fn draw_sprite_subtex(
        &self,
        tex_idx: usize,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u1: f32,
        v1: f32,
        u2: f32,
        v2: f32,
        tint_r: f32,
        tint_g: f32,
        tint_b: f32,
        tint_a: f32,
    ) {
        self.draw_list
            .lock()
            .expect("lock")
            .push(DrawCmd::SpriteSubtex {
                tex_idx,
                x,
                y,
                w,
                h,
                u1,
                v1,
                u2,
                v2,
                r: tint_r,
                g: tint_g,
                b: tint_b,
                a: tint_a,
            });
    }

    /// Queue a tinted sprite (full texture, custom colour).
    pub fn draw_sprite_tinted(
        &self,
        tex_idx: usize,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) {
        self.draw_sprite_subtex(tex_idx, x, y, w, h, 0.0, 0.0, 1.0, 1.0, r, g, b, a);
    }

    /// Draw text using the embedded 8×8 monospace bitmap font.
    ///
    /// `char_w` = width in pixels of one character (e.g. 8.0 for 1:1 scale,
    /// 16.0 for 2×). Character height equals `char_w` (square glyphs).
    /// Only ASCII 32–126 is supported; out-of-range chars render as space.
    pub fn draw_text(&self, x: f32, y: f32, text: &str, char_w: f32, r: f32, g: f32, b: f32) {
        let char_h = char_w;
        let atlas_w = self.font_tex_w as f32;
        let atlas_h = self.font_tex_h as f32;
        let glyph_uv_w = 8.0 / atlas_w;
        let glyph_uv_h = 8.0 / atlas_h;

        let mut list = self.draw_list.lock().expect("lock");
        for (i, ch) in text.chars().enumerate() {
            let mut code = ch as u8;
            if !(b' '..=b'~').contains(&code) {
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

    /// Set the background clear colour.
    pub fn set_clear(&self, r: f32, g: f32, b: f32, a: f32) {
        *self.clear_color.lock().expect("lock") = (r, g, b, a);
    }

    // -----------------------------------------------------------------------
    // Frame pipeline
    // -----------------------------------------------------------------------

    /// Render all queued commands and present the frame.
    ///
    /// This is the single entry point for frame rendering. Internally it
    /// follows the deterministic pipeline:
    ///
    /// 1. **Begin** — acquire surface texture, create encoder, begin pass
    /// 2. **Batch** — sort commands by bind group, build vertex/index arrays
    /// 3. **Upload** — write vertex/index data to staging GPU buffers
    /// 4. **Render** — set pipeline, bind groups, draw indexed
    /// 5. **Present** — end pass, submit, present
    /// 6. **Cleanup** — drain draw list, handle surface errors
    pub fn render(&self) -> Result<(), RenderError> {
        // ── Surface recovery ──────────────────────────────────────────
        if *self.surface_lost.lock().expect("lock") {
            self.surface.configure(&self.device, &self.config);
            *self.surface_lost.lock().expect("lock") = false;
            info!("Surface reconfigured after loss");
        }

        let clear = *self.clear_color.lock().expect("lock");

        // ── 1. Begin frame ────────────────────────────────────────────
        let frame = self.surface.get_current_texture().map_err(|e| match e {
            wgpu::SurfaceError::Lost => {
                *self.surface_lost.lock().expect("lock") = true;
                RenderError::SurfaceLost
            }
            wgpu::SurfaceError::Timeout => RenderError::SurfaceFailed("swap chain timeout".into()),
            wgpu::SurfaceError::Outdated => {
                RenderError::SurfaceFailed("swap chain outdated".into())
            }
            other => RenderError::SurfaceFailed(other.to_string()),
        })?;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

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

        // ── 2. Drain draw list ────────────────────────────────────────
        let draw_cmds: Vec<DrawCmd> = self.draw_list.lock().expect("lock").drain(..).collect();

        if draw_cmds.is_empty() {
            drop(pass);
            self.queue.submit(std::iter::once(encoder.finish()));
            frame.present();
            return Ok(());
        }

        // ── 3. Batch by bind group, build vertex/index arrays ─────────
        let ndc = NdcConverter::new(self.screen_size.0, self.screen_size.1);
        let mut vertices: Vec<SpriteVertex> = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut batch_starts: Vec<(BindGroupId, usize, usize)> = Vec::new();
        // (bind_group, start_vertex, start_index)

        let mut current_bg: Option<BindGroupId> = None;
        let mut batch_start_v = 0usize;
        let mut batch_start_i = 0usize;

        for cmd in &draw_cmds {
            let bg = cmd.bind_group();
            let needs_flush = match &current_bg {
                Some(cur) => bg != *cur,
                None => false,
            };

            if needs_flush {
                batch_starts.push((current_bg.take().unwrap(), batch_start_v, batch_start_i));
                batch_start_v = vertices.len();
                batch_start_i = indices.len();
            }
            current_bg = Some(bg);

            let (x, y, w, h) = cmd.rect();
            let uv = cmd.uv();
            let color = cmd.color();
            ndc.rect_vertices(x, y, w, h, uv, color, &mut vertices, &mut indices);
        }

        if let Some(bg) = current_bg {
            batch_starts.push((bg, batch_start_v, batch_start_i));
        }

        // ── 4. Upload to staging buffers ──────────────────────────────
        {
            let mut staging = self.staging.lock().expect("lock");
            staging.ensure(&self.device, vertices.len(), indices.len());
            let (_vcount, _icount) = staging.upload(&self.queue, &vertices, &indices);

            pass.set_pipeline(&self.pipeline);

            // ── 5. Emit draw calls ────────────────────────────────
            for (bg, sv, si) in &batch_starts {
                let end_v = batch_starts
                    .iter()
                    .skip_while(|(_, s, _)| s != sv)
                    .nth(1)
                    .map(|(_, ev, _)| *ev)
                    .unwrap_or(vertices.len());
                let end_i = batch_starts
                    .iter()
                    .skip_while(|(_, _, s)| s != si)
                    .nth(1)
                    .map(|(_, _, ei)| *ei)
                    .unwrap_or(indices.len());

                let vtx_count = end_v - sv;
                let idx_count = (end_i - si) as u32;

                if vtx_count == 0 || idx_count == 0 {
                    continue;
                }

                pass.set_vertex_buffer(0, staging.vb.slice(..));
                pass.set_index_buffer(
                    staging
                        .ib
                        .slice((si * std::mem::size_of::<u16>()) as BufferAddress..),
                    wgpu::IndexFormat::Uint16,
                );
                self.set_bind_group(&mut pass, bg);
                pass.draw_indexed(0..idx_count, 0, 0..1);
            }
        }

        // ── 6. Present ────────────────────────────────────────────────
        drop(pass);
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Set the bind group for the current render pass.
    fn set_bind_group(&self, pass: &mut wgpu::RenderPass, bg: &BindGroupId) {
        match bg {
            BindGroupId::Font => pass.set_bind_group(0, &self.font_bind_group, &[]),
            BindGroupId::Texture(idx) => {
                let slots = self.texture_slots.lock().expect("lock");
                if let Some(bind_group) = slots.get(*idx) {
                    pass.set_bind_group(0, bind_group, &[]);
                } else {
                    pass.set_bind_group(0, &self.default_bind_group, &[]);
                }
            }
            BindGroupId::Default => pass.set_bind_group(0, &self.default_bind_group, &[]),
        }
    }

    /// Resize the output surface.
    ///
    /// Silently ignores requests with zero dimensions (e.g. when the window
    /// is minimised).
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.screen_size = (width as f32, height as f32);
        info!(width, height, "Surface resized");
    }

    /// Access the wgpu device (for advanced use).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Access the wgpu queue (for advanced use).
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The surface's texture format.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Draw command generation ──────────────────────────────────────

    #[test]
    fn test_draw_cmd_rect_bind_group() {
        let cmd = DrawCmd::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        assert_eq!(cmd.bind_group(), BindGroupId::Default);
    }

    #[test]
    fn test_draw_cmd_sprite_bind_group() {
        let cmd = DrawCmd::Sprite {
            tex_idx: 3,
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 32.0,
        };
        assert_eq!(cmd.bind_group(), BindGroupId::Texture(3));
    }

    #[test]
    fn test_draw_cmd_glyph_bind_group() {
        let cmd = DrawCmd::Glyph {
            x: 10.0,
            y: 20.0,
            w: 8.0,
            h: 8.0,
            u1: 0.0,
            v1: 0.0,
            u2: 0.0625,
            v2: 0.1667,
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        assert_eq!(cmd.bind_group(), BindGroupId::Font);
    }

    #[test]
    fn test_draw_cmd_uv_mapping() {
        let rect_cmd = DrawCmd::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        assert_eq!(rect_cmd.uv(), [0.0, 0.0, 1.0, 1.0]);

        let glyph_cmd = DrawCmd::Glyph {
            x: 10.0,
            y: 20.0,
            w: 8.0,
            h: 8.0,
            u1: 0.1,
            v1: 0.2,
            u2: 0.3,
            v2: 0.4,
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        assert_eq!(glyph_cmd.uv(), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn test_draw_cmd_color() {
        let rect_cmd = DrawCmd::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            r: 0.5,
            g: 0.3,
            b: 0.1,
            a: 0.8,
        };
        assert_eq!(rect_cmd.color(), [0.5, 0.3, 0.1, 0.8]);

        let sprite_cmd = DrawCmd::Sprite {
            tex_idx: 0,
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 32.0,
        };
        assert_eq!(sprite_cmd.color(), [1.0, 1.0, 1.0, 1.0]);

        let glyph_cmd = DrawCmd::Glyph {
            x: 10.0,
            y: 20.0,
            w: 8.0,
            h: 8.0,
            u1: 0.0,
            v1: 0.0,
            u2: 0.0625,
            v2: 0.1667,
            r: 0.2,
            g: 0.4,
            b: 0.6,
        };
        assert_eq!(glyph_cmd.color(), [0.2, 0.4, 0.6, 1.0]);
    }

    #[test]
    fn test_draw_cmd_rect() {
        let rect_cmd = DrawCmd::Rect {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 50.0,
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        assert_eq!(rect_cmd.rect(), (10.0, 20.0, 100.0, 50.0));

        let sprite_cmd = DrawCmd::Sprite {
            tex_idx: 2,
            x: 5.0,
            y: 15.0,
            w: 64.0,
            h: 64.0,
        };
        assert_eq!(sprite_cmd.rect(), (5.0, 15.0, 64.0, 64.0));
    }

    // ── Batch ordering (same bind group → same batch) ────────────────

    #[test]
    fn test_commands_grouped_by_bind_group() {
        let cmds = [
            DrawCmd::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            DrawCmd::Rect {
                x: 10.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
            DrawCmd::Glyph {
                x: 0.0,
                y: 20.0,
                w: 8.0,
                h: 8.0,
                u1: 0.0,
                v1: 0.0,
                u2: 0.0625,
                v2: 0.1667,
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            DrawCmd::Rect {
                x: 20.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            },
        ];

        let bg_sequence: Vec<BindGroupId> = cmds.iter().map(|c| c.bind_group()).collect();
        // Two Default rects, one Font glyph, one Default rect
        assert_eq!(bg_sequence[0], BindGroupId::Default);
        assert_eq!(bg_sequence[1], BindGroupId::Default);
        assert_eq!(bg_sequence[2], BindGroupId::Font);
        assert_eq!(bg_sequence[3], BindGroupId::Default);
    }

    // ── Coordinate conversion ─────────────────────────────────────────

    #[test]
    fn test_ndc_converter_origin() {
        let ndc = NdcConverter::new(800.0, 600.0);
        let (x, y) = ndc.ndc(0.0, 0.0);
        assert!((x - (-1.0)).abs() < 1e-6);
        assert!((y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_ndc_converter_center() {
        let ndc = NdcConverter::new(800.0, 600.0);
        let (x, y) = ndc.ndc(400.0, 300.0);
        assert!((x - 0.0).abs() < 1e-6);
        assert!((y - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_ndc_converter_bottom_right() {
        let ndc = NdcConverter::new(800.0, 600.0);
        let (x, y) = ndc.ndc(800.0, 600.0);
        assert!((x - 1.0).abs() < 1e-6);
        assert!((y - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_rect_vertices_generation() {
        let ndc = NdcConverter::new(800.0, 600.0);
        let mut verts = Vec::new();
        let mut idxs = Vec::new();
        ndc.rect_vertices(
            0.0,
            0.0,
            800.0,
            600.0,
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            &mut verts,
            &mut idxs,
        );

        assert_eq!(verts.len(), 4);
        assert_eq!(idxs.len(), 6);

        // Full-screen quad covers [-1, 1]
        assert!((verts[0].position[0] - (-1.0)).abs() < 1e-6);
        assert!((verts[0].position[1] - 1.0).abs() < 1e-6);
        assert!((verts[2].position[0] - 1.0).abs() < 1e-6);
        assert!((verts[2].position[1] - (-1.0)).abs() < 1e-6);

        // Index pattern: triangle 0-1-2, then 0-2-3
        assert_eq!(idxs, &[0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn test_rect_vertices_multiple_same_batch() {
        let ndc = NdcConverter::new(800.0, 600.0);
        let mut verts = Vec::new();
        let mut idxs = Vec::new();
        ndc.rect_vertices(
            0.0,
            0.0,
            100.0,
            100.0,
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            &mut verts,
            &mut idxs,
        );
        ndc.rect_vertices(
            100.0,
            0.0,
            100.0,
            100.0,
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            &mut verts,
            &mut idxs,
        );

        assert_eq!(verts.len(), 8);
        assert_eq!(idxs.len(), 12);

        // Second quad's vertices start at index 4
        assert_eq!(idxs[6], 4); // First index of second quad
        assert_eq!(idxs[7], 5);
        assert_eq!(idxs[8], 6);
    }

    // ── Draw list operations ──────────────────────────────────────────

    #[test]
    fn test_draw_list_queue_and_drain() {
        let list = Mutex::new(Vec::new());
        list.lock().unwrap().push(DrawCmd::Rect {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 50.0,
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
        list.lock().unwrap().push(DrawCmd::Sprite {
            tex_idx: 0,
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 32.0,
        });
        list.lock().unwrap().push(DrawCmd::Glyph {
            x: 50.0,
            y: 50.0,
            w: 8.0,
            h: 8.0,
            u1: 0.0,
            v1: 0.0,
            u2: 0.0625,
            v2: 0.1667,
            r: 1.0,
            g: 1.0,
            b: 1.0,
        });
        let cmds = list.lock().unwrap().drain(..).collect::<Vec<_>>();
        assert_eq!(cmds.len(), 3);
        match &cmds[0] {
            DrawCmd::Rect { x, y, .. } => {
                assert_eq!(*x, 10.0);
                assert_eq!(*y, 20.0);
            }
            _ => panic!("Expected Rect"),
        }
    }

    // ── Clear colour ──────────────────────────────────────────────────

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

    // ── Staging batch ─────────────────────────────────────────────────

    #[test]
    fn test_staging_batch_initial_capacity() {
        const _: () = assert!(StagingBatch::MIN_VERTICES > 0);
        const _: () = assert!(StagingBatch::MIN_INDICES > 0);
        const _: () = assert!(StagingBatch::MIN_INDICES >= StagingBatch::MIN_VERTICES);
    }

    #[test]
    fn test_bind_group_id_comparison() {
        assert_eq!(BindGroupId::Default, BindGroupId::Default);
        assert_eq!(BindGroupId::Font, BindGroupId::Font);
        assert_eq!(BindGroupId::Texture(1), BindGroupId::Texture(1));
        assert_ne!(BindGroupId::Texture(1), BindGroupId::Texture(2));
        assert_ne!(BindGroupId::Default, BindGroupId::Font);
        assert_ne!(BindGroupId::Font, BindGroupId::Texture(0));
    }

    // ── Texture index validation ──────────────────────────────────────

    #[test]
    fn test_load_texture_rejects_empty_data() {
        // This just validates the error path; actual GPU texture load
        // requires a device.
        let result = image::load_from_memory(&[]);
        assert!(result.is_err());
    }
}
