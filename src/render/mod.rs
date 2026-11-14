//! Renderer 3D (wgpu): gwiazdy, ciała niebieskie z teksturami, atmosfery, pierścienie, linie orbit
//! i trajektorii oraz punkty satelitów – do bufora poza ekranem, który egui wkleja w okno
//! (wywołanie zwrotne malowania). Pod szkłem interfejsu obraz jest rozmyty.

pub mod camera;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};

use bytemuck::{Pod, Zeroable};
use eframe::egui;
use eframe::egui_wgpu;
use glam::{Mat3, Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::astro::bodies::{self, BodyId};

const MSAA: u32 = 4;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const BODY_STRIDE: u64 = 256;
const MAX_BODIES: u64 = 96;
pub const GLASS_MAX: usize = 12;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    sky_proj: [[f32; 4]; 4],
    viewport: [f32; 4],
    sun_rel: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BodyUniform {
    model: [[f32; 4]; 4],
    rot: [[f32; 4]; 4],
    color: [f32; 4],
    atmo: [f32; 4],
    center: [f32; 4],
    extra: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlassUniform {
    rects: [[f32; 4]; GLASS_MAX],
    amount: [[f32; 4]; GLASS_MAX / 4],
    info: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StarInst {
    dir: [f32; 3],
    mag: f32,
    color: [f32; 3],
}

/// Odcinek linii (względem kamery), barwa liniowa RGBA, grubość w pikselach.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct LineInst {
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    pub color: [f32; 4],
    pub width: f32,
}

/// Punkt (względem kamery), średnica w pikselach, barwa liniowa RGBA.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct PointInst {
    pub p: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
}

/// Ciało do narysowania w tej klatce.
#[derive(Clone, Copy, Debug)]
pub struct BodyDraw {
    pub id: BodyId,
    /// Środek względem kamery [m].
    pub center: Vec3,
    pub radius: f32,
    /// Orientacja: układ ciała → świat.
    pub rot: Mat3,
    pub color: [f32; 3],
    pub atmo: Option<[f32; 4]>,
    pub sun: bool,
    pub cloud_u: f32,
    pub rings: bool,
}

pub struct Frame<'a> {
    pub view_proj: Mat4,
    pub sky_proj: Mat4,
    pub sun_rel: Vec3,
    pub star_brightness: f32,
    pub exposure: f32,
    pub near: f32,
    pub bodies: &'a [BodyDraw],
    pub lines: &'a [LineInst],
    pub points: &'a [PointInst],
    pub glows: &'a [PointInst],
}

struct Targets {
    size: [u32; 2],
    color_ms: wgpu::TextureView,
    resolved: wgpu::TextureView,
    depth: wgpu::TextureView,
    blit_bind: wgpu::BindGroup,
}

/// Mipmapy tekstury przygotowane w tle.
struct Decoded {
    body: BodyId,
    surface: Vec<image::RgbaImage>,
    clouds: Option<Vec<image::RgbaImage>>,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals_buf: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    body_buf: wgpu::Buffer,
    body_bind: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    default_tex: wgpu::BindGroup,
    textures: HashMap<BodyId, wgpu::BindGroup>,
    requested: HashSet<BodyId>,
    tx: Sender<Decoded>,
    rx: Receiver<Decoded>,
    ctx: egui::Context,
    stars: (wgpu::Buffer, u32),
    sphere: (wgpu::Buffer, wgpu::Buffer, u32),
    ring: (wgpu::Buffer, wgpu::Buffer, u32),
    lines_buf: wgpu::Buffer,
    points_buf: wgpu::Buffer,
    glows_buf: wgpu::Buffer,
    p_stars: wgpu::RenderPipeline,
    p_body: wgpu::RenderPipeline,
    p_atmo: wgpu::RenderPipeline,
    p_ring: wgpu::RenderPipeline,
    p_lines: wgpu::RenderPipeline,
    p_points: wgpu::RenderPipeline,
    p_glow: wgpu::RenderPipeline,
    p_blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    clamp_sampler: wgpu::Sampler,
    glass_buf: wgpu::Buffer,
    targets: Option<Targets>,
    t0: std::time::Instant,
}

fn stars_data() -> Vec<StarInst> {
    let b: &[u8] = include_bytes!("../../assets/stars.bin");
    let rot = glam::DMat3::from_rotation_x(-crate::astro::OBLIQUITY);
    b.chunks_exact(16)
        .map(|c| {
            let f = |o: usize| f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
            let (ra, dec, mag) = (f(0) as f64, f(4) as f64, f(8));
            let eq = glam::DVec3::new(dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin());
            let d = rot * eq;
            // barwy klas widmowych O B A F G K M (lekko nasycone)
            let color = match c[12] {
                0 => [0.62, 0.7, 1.0],
                1 => [0.7, 0.78, 1.0],
                2 => [0.86, 0.9, 1.0],
                3 => [1.0, 0.98, 0.94],
                4 => [1.0, 0.93, 0.8],
                5 => [1.0, 0.82, 0.62],
                _ => [1.0, 0.7, 0.52],
            };
            StarInst { dir: [d.x as f32, d.y as f32, d.z as f32], mag, color }
        })
        .collect()
}

/// Kula jednostkowa: biegun w osi Z, południk 0 w osi X; u = (λ + 180°)/360°, v = (90° − φ)/180°.
fn sphere_mesh(nlon: u32, nlat: u32) -> (Vec<f32>, Vec<u32>) {
    let mut v = Vec::new();
    for j in 0..=nlat {
        let vv = j as f32 / nlat as f32;
        let lat = (0.5 - vv) * std::f32::consts::PI;
        for i in 0..=nlon {
            let u = i as f32 / nlon as f32;
            let lon = (u - 0.5) * std::f32::consts::TAU;
            v.extend_from_slice(&[lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin(), u, vv]);
        }
    }
    let mut idx = Vec::new();
    let row = nlon + 1;
    for j in 0..nlat {
        for i in 0..nlon {
            let a = j * row + i;
            let (b, c, d) = (a + 1, a + row, a + row + 1);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (v, idx)
}

/// Pierścień w płaszczyźnie XY: promienie w metrach.
fn ring_mesh(inner: f32, outer: f32, n: u32) -> (Vec<f32>, Vec<u32>) {
    let mut v = Vec::new();
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        for (k, r) in [inner, outer].iter().enumerate() {
            v.extend_from_slice(&[r * a.cos(), r * a.sin(), 0.0, i as f32 / n as f32, k as f32]);
        }
    }
    let mut idx = Vec::new();
    for i in 0..n {
        let a = i * 2;
        idx.extend_from_slice(&[a, a + 1, a + 2, a + 2, a + 1, a + 3]);
    }
    (v, idx)
}

fn mips(img: image::RgbaImage) -> Vec<image::RgbaImage> {
    let mut out = vec![img];
    loop {
        let last = out.last().expect("co najmniej jeden poziom");
        let (w, h) = (last.width(), last.height());
        if w <= 1 && h <= 1 {
            break;
        }
        let next = image::imageops::resize(last, (w / 2).max(1), (h / 2).max(1), image::imageops::FilterType::Triangle);
        out.push(next);
    }
    out
}

impl Renderer {
    pub fn new(rs: &egui_wgpu::RenderState, ctx: egui::Context) -> Self {
        let device = rs.device.clone();
        let queue = rs.queue.clone();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("kosmos"), source: wgpu::ShaderSource::Wgsl(include_str!("space.wgsl").into()) });
        let blit_sh = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("blit"), source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()) });
        let uniform_entry = |binding: u32, dynamic: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: None },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("globals"), entries: &[uniform_entry(0, false)] });
        let body_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("ciało"), entries: &[uniform_entry(0, true)] });
        let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tekstury"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                tex_entry(0),
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                uniform_entry(2, false),
            ],
        });
        let buf = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let globals_buf = buf("globals", std::mem::size_of::<Globals>() as u64, wgpu::BufferUsages::UNIFORM);
        let body_buf = buf("ciała", BODY_STRIDE * MAX_BODIES, wgpu::BufferUsages::UNIFORM);
        let glass_buf = buf("szkło", std::mem::size_of::<GlassUniform>() as u64, wgpu::BufferUsages::UNIFORM);
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("globals"), layout: &globals_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }] });
        let body_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ciało"),
            layout: &body_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &body_buf, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<BodyUniform>() as u64) }),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tekstury"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let clamp_sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("blit"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let white = Self::upload(&device, &queue, &[image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))], COLOR);
        let black = Self::upload(&device, &queue, &[image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]))], wgpu::TextureFormat::Rgba8Unorm);
        let default_tex = Self::tex_bind(&device, &tex_layout, &sampler, &white, &black);

        let layout = |label: &str, groups: &[&wgpu::BindGroupLayout]| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: groups, push_constant_ranges: &[] });
        let l_glob = layout("globals", &[&globals_layout]);
        let l_body = layout("ciało", &[&globals_layout, &body_layout, &tex_layout]);
        let l_ring = layout("pierścień", &[&globals_layout, &body_layout]);
        let ms = wgpu::MultisampleState { count: MSAA, ..Default::default() };
        let depth = |write: bool, cmp: wgpu::CompareFunction| Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: write, depth_compare: cmp, stencil: Default::default(), bias: Default::default() });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let premul = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        let attrs_star = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x3];
        let attrs_mesh = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
        let attrs_line = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32];
        let attrs_point = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4];
        #[allow(clippy::too_many_arguments)]
        let pipe = |label: &str,
                    layout: &wgpu::PipelineLayout,
                    vs: &str,
                    fs: &str,
                    vb: wgpu::VertexBufferLayout,
                    blend: Option<wgpu::BlendState>,
                    cull: Option<wgpu::Face>,
                    depth: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[vb] },
                primitive: wgpu::PrimitiveState { cull_mode: cull, front_face: wgpu::FrontFace::Ccw, ..Default::default() },
                depth_stencil: depth,
                multisample: ms,
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: COLOR, blend, write_mask: wgpu::ColorWrites::ALL })] }),
                multiview: None,
                cache: None,
            })
        };
        let inst = |stride: usize, attrs: &'static [wgpu::VertexAttribute]| wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: attrs };
        let vert = |stride: usize, attrs: &'static [wgpu::VertexAttribute]| wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: attrs };
        // tablice atrybutów muszą żyć statycznie
        let a_star: &'static [wgpu::VertexAttribute] = Box::leak(Box::new(attrs_star));
        let a_mesh: &'static [wgpu::VertexAttribute] = Box::leak(Box::new(attrs_mesh));
        let a_line: &'static [wgpu::VertexAttribute] = Box::leak(Box::new(attrs_line));
        let a_point: &'static [wgpu::VertexAttribute] = Box::leak(Box::new(attrs_point));
        let p_stars = pipe("gwiazdy", &l_glob, "vs_star", "fs_star", inst(28, a_star), Some(additive), None, depth(false, wgpu::CompareFunction::Always));
        let p_body = pipe("ciała", &l_body, "vs_body", "fs_body", vert(20, a_mesh), None, Some(wgpu::Face::Back), depth(true, wgpu::CompareFunction::GreaterEqual));
        let p_atmo = pipe("atmosfera", &l_body, "vs_atmo", "fs_atmo", vert(20, a_mesh), Some(additive), Some(wgpu::Face::Back), depth(false, wgpu::CompareFunction::GreaterEqual));
        let p_ring = pipe("pierścienie", &l_ring, "vs_ring", "fs_ring", vert(20, a_mesh), Some(premul), None, depth(false, wgpu::CompareFunction::GreaterEqual));
        let p_lines = pipe("linie", &l_glob, "vs_line", "fs_line", inst(44, a_line), Some(premul), None, depth(false, wgpu::CompareFunction::GreaterEqual));
        let p_points = pipe("punkty", &l_glob, "vs_point", "fs_point", inst(32, a_point), Some(premul), None, depth(false, wgpu::CompareFunction::GreaterEqual));
        let p_glow = pipe("poświata", &l_glob, "vs_point", "fs_glow", inst(32, a_point), Some(additive), None, depth(false, wgpu::CompareFunction::GreaterEqual));
        let target = rs.target_format;
        let gamma = !target.is_srgb();
        let p_blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&layout("blit", &[&blit_layout])),
            vertex: wgpu::VertexState { module: &blit_sh, entry_point: Some("vs_blit"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blit_sh,
                entry_point: Some(if gamma { "fs_blit_gamma" } else { "fs_blit_linear" }),
                compilation_options: Default::default(),
                targets: &[Some(target.into())],
            }),
            multiview: None,
            cache: None,
        });
        let stars = stars_data();
        let stars_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gwiazdy"), contents: bytemuck::cast_slice(&stars), usage: wgpu::BufferUsages::VERTEX });
        let (sv, si) = sphere_mesh(160, 80);
        let sphere = (
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kula"), contents: bytemuck::cast_slice(&sv), usage: wgpu::BufferUsages::VERTEX }),
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kula-i"), contents: bytemuck::cast_slice(&si), usage: wgpu::BufferUsages::INDEX }),
            si.len() as u32,
        );
        let (rv, ri) = ring_mesh(66.0e6, 141.0e6, 360);
        let ring = (
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("pierścień"), contents: bytemuck::cast_slice(&rv), usage: wgpu::BufferUsages::VERTEX }),
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("pierścień-i"), contents: bytemuck::cast_slice(&ri), usage: wgpu::BufferUsages::INDEX }),
            ri.len() as u32,
        );
        let vb = |label: &str| buf(label, 64 * 1024, wgpu::BufferUsages::VERTEX);
        let (tx, rx) = channel();
        Self {
            lines_buf: vb("linie"),
            points_buf: vb("punkty"),
            glows_buf: vb("poświaty"),
            device,
            queue,
            globals_buf,
            globals_bind,
            body_buf,
            body_bind,
            tex_layout,
            sampler,
            default_tex,
            textures: HashMap::new(),
            requested: HashSet::new(),
            tx,
            rx,
            ctx,
            stars: (stars_buf, stars.len() as u32),
            sphere,
            ring,
            p_stars,
            p_body,
            p_atmo,
            p_ring,
            p_lines,
            p_points,
            p_glow,
            p_blit,
            blit_layout,
            clamp_sampler,
            glass_buf,
            targets: None,
            t0: std::time::Instant::now(),
        }
    }

    fn upload(device: &wgpu::Device, queue: &wgpu::Queue, levels: &[image::RgbaImage], format: wgpu::TextureFormat) -> wgpu::TextureView {
        let (w, h) = (levels[0].width(), levels[0].height());
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tekstura"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, l) in levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: i as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                l.as_raw(),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * l.width()), rows_per_image: Some(l.height()) },
                wgpu::Extent3d { width: l.width(), height: l.height(), depth_or_array_layers: 1 },
            );
        }
        tex.create_view(&Default::default())
    }

    fn tex_bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, a: &wgpu::TextureView, b: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tekstury"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(a) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(b) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    }

    /// Tekstury dekodowane w tle przy pierwszym użyciu ciała.
    fn request_texture(&mut self, id: BodyId) {
        if !self.requested.insert(id) {
            return;
        }
        let b = bodies::get(id);
        let (Some(bytes), clouds) = (b.texture, b.clouds) else { return };
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let decode = |data: &[u8]| image::load_from_memory(data).ok().map(|i| mips(i.to_rgba8()));
            let Some(surface) = decode(bytes) else { return };
            let clouds = clouds.and_then(decode);
            let _ = tx.send(Decoded { body: id, surface, clouds });
            ctx.request_repaint();
        });
    }

    fn receive_textures(&mut self) {
        while let Ok(d) = self.rx.try_recv() {
            let a = Self::upload(&self.device, &self.queue, &d.surface, COLOR);
            let b = match &d.clouds {
                Some(c) => Self::upload(&self.device, &self.queue, c, wgpu::TextureFormat::Rgba8Unorm),
                None => Self::upload(&self.device, &self.queue, &[image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]))], wgpu::TextureFormat::Rgba8Unorm),
            };
            let bind = Self::tex_bind(&self.device, &self.tex_layout, &self.sampler, &a, &b);
            self.textures.insert(d.body, bind);
        }
    }

    pub fn texture_ready(&self, id: BodyId) -> bool {
        self.textures.contains_key(&id)
    }

    fn ensure_targets(&mut self, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let mk = |format, samples, usage| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("cel 3D"),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color_ms = mk(COLOR, MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolved = mk(COLOR, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let depth = mk(DEPTH, MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let blit_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&resolved) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.clamp_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: self.glass_buf.as_entire_binding() },
            ],
        });
        self.targets = Some(Targets { size, color_ms, resolved, depth, blit_bind });
    }

    fn upload_instances<T: Pod>(device: &wgpu::Device, queue: &wgpu::Queue, slot: &mut wgpu::Buffer, data: &[T]) {
        let need = (data.len().max(1) * std::mem::size_of::<T>()) as u64;
        if slot.size() < need {
            *slot = device.create_buffer(&wgpu::BufferDescriptor { label: Some("instancje"), size: need.next_power_of_two(), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        }
        if !data.is_empty() {
            queue.write_buffer(slot, 0, bytemuck::cast_slice(data));
        }
    }

    /// Klatka do bufora poza ekranem; zwraca wywołanie zwrotne do wklejenia w egui.
    pub fn render(&mut self, f: &Frame, size_pt: [f32; 2], ppp: f32, scale: f32, glass: &[([f32; 4], f32)]) -> Blit {
        self.receive_textures();
        let size = [((size_pt[0] * ppp * scale).round() as u32).max(1), ((size_pt[1] * ppp * scale).round() as u32).max(1)];
        let mut gu = GlassUniform { rects: [[0.0; 4]; GLASS_MAX], amount: [[0.0; 4]; GLASS_MAX / 4], info: [0.0; 4] };
        let n = glass.len().min(GLASS_MAX);
        for (i, (r, a)) in glass.iter().take(n).enumerate() {
            gu.rects[i] = *r;
            gu.amount[i / 4][i % 4] = a.clamp(0.0, 1.0);
        }
        gu.info = [n as f32, 1.0 / size[0] as f32, 1.0 / size[1] as f32, 14.0 * ppp * scale];
        self.queue.write_buffer(&self.glass_buf, 0, bytemuck::bytes_of(&gu));
        self.ensure_targets(size);
        // rozmiar punktów i linii w pikselach bufora: przeliczenie z punktów ekranu
        let px_scale = ppp * scale;
        let globals = Globals {
            view_proj: f.view_proj.to_cols_array_2d(),
            sky_proj: f.sky_proj.to_cols_array_2d(),
            viewport: [size[0] as f32 / px_scale, size[1] as f32 / px_scale, px_scale / size[0] as f32, px_scale / size[1] as f32],
            sun_rel: [f.sun_rel.x, f.sun_rel.y, f.sun_rel.z, f.star_brightness],
            params: [self.t0.elapsed().as_secs_f32(), f.exposure, f.near, 0.0],
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        let n_bodies = f.bodies.len().min(MAX_BODIES as usize);
        let mut staging = vec![0u8; BODY_STRIDE as usize * n_bodies.max(1)];
        for (i, b) in f.bodies.iter().take(n_bodies).enumerate() {
            if bodies::get(b.id).texture.is_some() {
                self.request_texture(b.id);
            }
            let model = Mat4::from_translation(b.center) * Mat4::from_mat3(b.rot) * Mat4::from_scale(Vec3::splat(b.radius));
            let atmo = b.atmo.unwrap_or([0.0; 4]);
            let u = BodyUniform {
                model: model.to_cols_array_2d(),
                rot: Mat4::from_mat3(b.rot).to_cols_array_2d(),
                color: [b.color[0], b.color[1], b.color[2], if self.textures.contains_key(&b.id) { 1.0 } else { 0.0 }],
                atmo,
                center: [b.center.x, b.center.y, b.center.z, b.radius],
                extra: [if b.sun { 1.0 } else { 0.0 }, b.cloud_u, if bodies::get(b.id).clouds.is_some() && self.textures.contains_key(&b.id) { 1.0 } else { 0.0 }, 1.0 + 0.025 * (atmo[3] > 0.0) as u8 as f32],
            };
            let off = i * BODY_STRIDE as usize;
            staging[off..off + std::mem::size_of::<BodyUniform>()].copy_from_slice(bytemuck::bytes_of(&u));
        }
        self.queue.write_buffer(&self.body_buf, 0, &staging);
        // pierścienie: osobny wpis (bez skali promienia)
        let ring_slot = f.bodies.iter().take(n_bodies).position(|b| b.rings).map(|i| {
            let b = &f.bodies[i];
            let model = Mat4::from_translation(b.center) * Mat4::from_mat3(b.rot);
            let u = BodyUniform {
                model: model.to_cols_array_2d(),
                rot: Mat4::from_mat3(b.rot).to_cols_array_2d(),
                color: [1.0; 4],
                atmo: [0.0; 4],
                center: [b.center.x, b.center.y, b.center.z, b.radius],
                extra: [0.0; 4],
            };
            let slot = n_bodies.min(MAX_BODIES as usize - 1);
            self.queue.write_buffer(&self.body_buf, slot as u64 * BODY_STRIDE, bytemuck::bytes_of(&u));
            slot
        });
        Self::upload_instances(&self.device, &self.queue, &mut self.lines_buf, f.lines);
        Self::upload_instances(&self.device, &self.queue, &mut self.points_buf, f.points);
        Self::upload_instances(&self.device, &self.queue, &mut self.glows_buf, f.glows);
        let Some(t) = &self.targets else { unreachable!("cele utworzone wyżej") };
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("klatka") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kosmos"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.color_ms,
                    depth_slice: None,
                    resolve_target: Some(&t.resolved),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.002, a: 1.0 }), store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            if f.star_brightness > 0.0 {
                pass.set_pipeline(&self.p_stars);
                pass.set_vertex_buffer(0, self.stars.0.slice(..));
                pass.draw(0..6, 0..self.stars.1);
            }
            pass.set_pipeline(&self.p_body);
            pass.set_vertex_buffer(0, self.sphere.0.slice(..));
            pass.set_index_buffer(self.sphere.1.slice(..), wgpu::IndexFormat::Uint32);
            for (i, b) in f.bodies.iter().take(n_bodies).enumerate() {
                pass.set_bind_group(1, &self.body_bind, &[(i as u64 * BODY_STRIDE) as u32]);
                pass.set_bind_group(2, self.textures.get(&b.id).unwrap_or(&self.default_tex), &[]);
                pass.draw_indexed(0..self.sphere.2, 0, 0..1);
            }
            if let Some(slot) = ring_slot {
                pass.set_pipeline(&self.p_ring);
                pass.set_bind_group(1, &self.body_bind, &[(slot as u64 * BODY_STRIDE) as u32]);
                pass.set_vertex_buffer(0, self.ring.0.slice(..));
                pass.set_index_buffer(self.ring.1.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.ring.2, 0, 0..1);
            }
            pass.set_pipeline(&self.p_atmo);
            pass.set_vertex_buffer(0, self.sphere.0.slice(..));
            pass.set_index_buffer(self.sphere.1.slice(..), wgpu::IndexFormat::Uint32);
            for (i, b) in f.bodies.iter().take(n_bodies).enumerate() {
                if b.atmo.is_some_and(|a| a[3] > 0.0) {
                    pass.set_bind_group(1, &self.body_bind, &[(i as u64 * BODY_STRIDE) as u32]);
                    pass.set_bind_group(2, self.textures.get(&b.id).unwrap_or(&self.default_tex), &[]);
                    pass.draw_indexed(0..self.sphere.2, 0, 0..1);
                }
            }
            if !f.lines.is_empty() {
                pass.set_pipeline(&self.p_lines);
                pass.set_vertex_buffer(0, self.lines_buf.slice(..));
                pass.draw(0..6, 0..f.lines.len() as u32);
            }
            if !f.points.is_empty() {
                pass.set_pipeline(&self.p_points);
                pass.set_vertex_buffer(0, self.points_buf.slice(..));
                pass.draw(0..6, 0..f.points.len() as u32);
            }
            if !f.glows.is_empty() {
                pass.set_pipeline(&self.p_glow);
                pass.set_vertex_buffer(0, self.glows_buf.slice(..));
                pass.draw(0..6, 0..f.glows.len() as u32);
            }
        }
        self.queue.submit(Some(enc.finish()));
        Blit { pipeline: self.p_blit.clone(), bind: t.blit_bind.clone() }
    }
}

pub struct Blit {
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
}

impl egui_wgpu::CallbackTrait for Blit {
    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, _res: &egui_wgpu::CallbackResources) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    fn check(src: &str) {
        let m = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&m)
            .unwrap_or_else(|e| panic!("{e:?}"));
    }

    #[test]
    fn shaders_validate() {
        check(include_str!("space.wgsl"));
        check(include_str!("blit.wgsl"));
    }

    #[test]
    fn star_catalog() {
        let s = super::stars_data();
        assert!(s.len() > 9000);
        // Syriusz (najjaśniejsza)
        let min = s.iter().map(|x| x.mag).fold(f32::INFINITY, f32::min);
        assert!((min + 1.46).abs() < 0.05, "{min}");
    }
}
