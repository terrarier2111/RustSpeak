use bytemuck_derive::Pod;
use bytemuck_derive::Zeroable;
use flume::Sender;
use std::collections::HashMap;
use std::mem::size_of;
use std::ops::DerefMut;
use std::process::abort;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use wgpu::{BindGroupLayoutEntry, BindingType, BlendState, BufferAddress, BufferUsages, ColorTargetState, ColorWrites, LoadOp, Operations, RenderPassColorAttachment, RenderPipeline, Sampler, SamplerBindingType, ShaderSource, ShaderStages, Texture, TextureFormat, TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};
use wgpu::util::StagingBelt;
use wgpu_biolerless::{
    FragmentShaderState, ModuleSrc, PipelineBuilder, ShaderModuleSources, State, VertexShaderState,
    WindowSize,
};
use wgpu_glyph::{ab_glyph, GlyphBrush, GlyphBrushBuilder, Section};
use winit::window::Window;
use crate::ui::wgpu::atlas::{Atlas, AtlasAlloc, AtlasId};
use crate::ui::wgpu::LIGHT_GRAY_GPU;

pub struct Renderer {
    pub state: Arc<State>,
    tex_generic_pipeline: RenderPipeline,
    color_generic_pipeline: RenderPipeline,
    tex_circle_pipeline: RenderPipeline,
    color_circle_pipeline: RenderPipeline,
    pub dimensions: Dimensions,
    glyphs: Mutex<Vec<GlyphInfo>>,
}

pub struct GlyphInfo {
    pub brush: Mutex<GlyphBrush<()>>,
    pub format: TextureFormat,
    staging_belt: Mutex<StagingBelt>,
}

impl GlyphInfo {
    pub fn new(brush: GlyphBrush<()>, format: TextureFormat) -> Self {
        Self {
            brush: Mutex::new(brush),
            format,
            staging_belt: Mutex::new(StagingBelt::new(1024)),
        }
    }
}

impl Renderer {
    pub fn new(state: Arc<State>, window: &Window) -> anyhow::Result<Self> {
        let mut glyphs = vec![];
        let font = ab_glyph::FontArc::try_from_slice(include_bytes!(
            "PlayfairDisplayRegular.ttf"
        ))?;

        glyphs.push(GlyphInfo {
            brush: Mutex::new(GlyphBrushBuilder::using_font(font).build(state.device(), state.format())),
            format: state.format(),
            staging_belt: Mutex::new(StagingBelt::new(1024)),
        });
        let (width, height) = window.window_size();
        Ok(Self {
            tex_generic_pipeline: Self::atlas_generic_pipeline(&state),
            color_generic_pipeline: Self::color_generic_pipeline(&state),
            tex_circle_pipeline: Self::atlas_circle_pipeline(&state),
            color_circle_pipeline: Self::color_circle_pipeline(&state),
            state,
            dimensions: Dimensions::new(width, height),
            glyphs: Mutex::new(glyphs),
        })
    }

    pub fn render(
        &self,
        models: Vec<Model>,
        atlas: Arc<Atlas>, /*atlases: Arc<Mutex<Vec<Arc<Atlas>>>>*/
    ) {
        self.state
            .render(
                |view, mut encoder, state| {
                    /*for atlas in atlases.lock().unwrap().iter() {
                        atlas.update(&mut encoder);
                    }*/
                    atlas.update(&mut encoder);
                    let mut generic_atlas_models: HashMap<AtlasId, Vec<GenericAtlasVertex>> = HashMap::new();
                    let mut generic_color_models = vec![];
                    let mut circle_atlas_models: HashMap<AtlasId, Vec<CircleAtlasVertex>> = HashMap::new();
                    let mut circle_color_models = vec![];
                    for model in models {
                        match &model.color_src {
                            ColorSource::PerVert => {
                                model.vertices.into_iter().for_each(
                                    |vert| match vert {
                                        Vertex::GenericColor { pos, color } => generic_color_models.push(GenericColorVertex { pos, color }),
                                        Vertex::GenericAtlas { .. } => abort(), // FIXME: is it really necessary to abort because of perf stuff?
                                        Vertex::CircleColor { pos, color, radius, border_thickness } => circle_color_models.push(CircleColorVertex {
                                            pos,
                                            color,
                                            radius,
                                            border_thickness,
                                        }),
                                        Vertex::CircleAtlas { .. } => abort(),
                                    },
                                );
                            }
                            ColorSource::Atlas(atlas) => {
                                // FIXME: make different atlases work!
                                let mut generic_vertices = vec![];
                                let mut circle_vertices = vec![];
                                model.vertices.into_iter().for_each(|vert| match vert {
                                    Vertex::GenericColor { .. } => abort(),
                                    Vertex::GenericAtlas { pos, alpha, uv } => generic_vertices.push(GenericAtlasVertex {
                                        pos,
                                        alpha,
                                        uv,
                                    }),
                                    Vertex::CircleColor { .. } => abort(),
                                    Vertex::CircleAtlas { pos, alpha, uv, radius, border_thickness } => circle_vertices.push(CircleAtlasVertex {
                                        pos,
                                        alpha,
                                        uv,
                                        radius,
                                        border_thickness,
                                    }),
                                });
                                if !generic_vertices.is_empty() {
                                    if let Some(mut models) = generic_atlas_models.get_mut(&atlas.id()) {
                                        models.extend(generic_vertices);
                                    } else {
                                        generic_atlas_models
                                            .insert(atlas.id(), generic_vertices);
                                    }
                                } else {
                                    if let Some(mut models) = circle_atlas_models.get_mut(&atlas.id()) {
                                        models.extend(circle_vertices);
                                    } else {
                                        circle_atlas_models
                                            .insert(atlas.id(), circle_vertices);
                                    }
                                }
                            }
                        }
                    }
                    let generic_color_buffer =
                        state.create_buffer(generic_color_models.as_slice(), BufferUsages::VERTEX);
                    let circle_color_buffer =
                        state.create_buffer(circle_color_models.as_slice(), BufferUsages::VERTEX);
                    {
                        let attachments = [Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(LIGHT_GRAY_GPU),
                                store: true,
                            },
                        })];
                        let mut render_pass =
                            state.create_render_pass(&mut encoder, &attachments, None);
                        // let buffer = state.create_buffer(atlas_models.as_slice(), BufferUsages::VERTEX);
                        // render_pass.set_vertex_buffer(0, buffer.slice(..));

                        render_pass.set_vertex_buffer(0, generic_color_buffer.slice(..));
                        render_pass.set_pipeline(&self.color_generic_pipeline);
                        render_pass.draw(0..(generic_color_models.len() as u32), 0..1);
                        render_pass.set_vertex_buffer(0, circle_color_buffer.slice(..));
                        render_pass.set_pipeline(&self.color_circle_pipeline);
                        render_pass.draw(0..(circle_color_models.len() as u32), 0..1);
                    }
                    for glyph in self.glyphs.lock().unwrap().iter() {
                        let mut staging_belt = glyph.staging_belt.lock().unwrap();
                        let (width, height) = self.dimensions.get();
                        glyph.brush.lock().unwrap().draw_queued(state.device(), staging_belt.deref_mut(), &mut encoder, view, width, height).unwrap();
                        staging_belt.finish();
                    }
                    encoder
                },
                &TextureViewDescriptor::default(),
            )
            .unwrap();
        for glyph in self.glyphs.lock().unwrap().iter() {
            glyph.staging_belt.lock().unwrap().recall();
        }
    }

    fn color_generic_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new()
            .vertex(VertexShaderState {
                entry_point: "main_vert",
                buffers: &[GenericColorVertex::desc()],
            })
            .fragment(FragmentShaderState {
                entry_point: "main_frag",
                targets: &[Some(ColorTargetState {
                    format: state.format(),
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            })
            .shader_src(ShaderModuleSources::Single(ModuleSrc::Source(
                ShaderSource::Wgsl(include_str!("ui_color_generic.wgsl").into()),
            )))
            .layout(&state.create_pipeline_layout(&[], &[]))
            .build(state)
    }

    fn atlas_generic_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new()
            .vertex(VertexShaderState {
                entry_point: "main_vert",
                buffers: &[GenericAtlasVertex::desc()],
            })
            .fragment(FragmentShaderState {
                entry_point: "main_frag",
                targets: &[Some(ColorTargetState {
                    format: state.format(),
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            })
            .shader_src(ShaderModuleSources::Single(ModuleSrc::Source(
                ShaderSource::Wgsl(include_str!("ui_atlas_generic.wgsl").into()),
            )))
            .layout(&state.create_pipeline_layout(
                &[&state.create_bind_group_layout(&[
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
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        // This should match the filterable field of the
                        // corresponding Texture entry above.
                        ty: BindingType::Sampler(SamplerBindingType::Filtering),
                        count: None,
                    },
                ])],
                &[],
            ))
            .build(state)
    }

    fn color_circle_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new()
            .vertex(VertexShaderState {
                entry_point: "main_vert",
                buffers: &[CircleColorVertex::desc()],
            })
            .fragment(FragmentShaderState {
                entry_point: "main_frag",
                targets: &[Some(ColorTargetState {
                    format: state.format(),
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            })
            .shader_src(ShaderModuleSources::Single(ModuleSrc::Source(
                ShaderSource::Wgsl(include_str!("ui_color_circle.wgsl").into()),
            )))
            .layout(&state.create_pipeline_layout(&[], &[]))
            .build(state)
    }

    fn atlas_circle_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new()
            .vertex(VertexShaderState {
                entry_point: "main_vert",
                buffers: &[CircleAtlasVertex::desc()],
            })
            .fragment(FragmentShaderState {
                entry_point: "main_frag",
                targets: &[Some(ColorTargetState {
                    format: state.format(),
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            })
            .shader_src(ShaderModuleSources::Single(ModuleSrc::Source(
                ShaderSource::Wgsl(include_str!("ui_atlas_circle.wgsl").into()),
            )))
            .layout(&state.create_pipeline_layout(
                &[&state.create_bind_group_layout(&[
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
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        // This should match the filterable field of the
                        // corresponding Texture entry above.
                        ty: BindingType::Sampler(SamplerBindingType::Filtering),
                        count: None,
                    },
                ])],
                &[],
            ))
            .build(state)
    }

    pub fn add_glyph(&self, glyph_info: GlyphInfo) -> usize {
        let mut glyphs = self.glyphs.lock().unwrap();
        let len = glyphs.len();
        glyphs.push(glyph_info);
        len
    }

    pub fn queue_glyph(&self, glyph_id: usize, section: Section) {
        self.glyphs.lock().unwrap()[glyph_id].brush.lock().unwrap().queue(section);
    }
}

#[derive(Clone)]
pub struct Model {
    pub vertices: Vec<Vertex>,
    pub color_src: ColorSource,
}

#[derive(Clone)]
pub enum ColorSource {
    PerVert,
    Atlas(Arc<Atlas>),
    // FIXME: add single tex
}

pub enum TexTy {
    Atlas(Arc<AtlasAlloc>),
    // Simple(TexTriple), // FIXME: implement this!
}

#[derive(Copy, Clone)]
pub enum Vertex {
    GenericColor {
        pos: [f32; 2],
        color: [f32; 4],
    },
    GenericAtlas {
        pos: [f32; 2],
        alpha: f32,
        uv: (u32, u32),
    },
    CircleColor {
        pos: [f32; 2],
        color: [f32; 4],
        radius: f32,
        border_thickness: f32,
    },
    CircleAtlas {
        pos: [f32; 2],
        alpha: f32,
        uv: (u32, u32),
        radius: f32,
        border_thickness: f32,
    },
}

#[derive(Pod, Zeroable, Copy, Clone)]
#[repr(C)]
struct GenericColorVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

impl GenericColorVertex {
    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<GenericColorVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 2]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
            ],
        }
    }
}

struct GenericAtlasVertex {
    pos: [f32; 2],
    alpha: f32,
    uv: (u32, u32),
}

impl GenericAtlasVertex {
    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<GenericAtlasVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 2]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 4]>() as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32,
                },
            ],
        }
    }
}

#[derive(Pod, Zeroable, Copy, Clone)]
#[repr(C)]
struct CircleColorVertex {
    pos: [f32; 2],
    color: [f32; 4],
    radius: f32,
    border_thickness: f32,
}

impl CircleColorVertex {
    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<CircleColorVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 2]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 6]>() as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 7]>() as BufferAddress,
                    shader_location: 3,
                    format: VertexFormat::Float32,
                },
            ],
        }
    }
}

struct CircleAtlasVertex {
    pos: [f32; 2],
    alpha: f32,
    uv: (u32, u32),
    radius: f32,
    border_thickness: f32,
}

impl CircleAtlasVertex {
    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<CircleAtlasVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 2]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 4]>() as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 5]>() as BufferAddress,
                    shader_location: 3,
                    format: VertexFormat::Float32,
                },
                VertexAttribute {
                    offset: size_of::<[f32; 6]>() as BufferAddress,
                    shader_location: 4,
                    format: VertexFormat::Float32,
                },
            ],
        }
    }
}

pub struct Dimensions {
    inner: AtomicU64,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            inner: AtomicU64::new(width as u64 | ((height as u64) << 32)),
        }
    }

    pub fn get(&self) -> (u32, u32) {
        let val = self.inner.load(Ordering::Acquire);
        (val as u32, (val >> 32) as u32)
    }

    pub fn set(&self, width: u32, height: u32) {
        let val = width as u64 | ((height as u64) << 32);
        self.inner.store(val, Ordering::Release);
    }
}

pub trait Renderable {
    fn render(&self, sender: Sender<Vec<Vertex>> /*, screen_dims: (u32, u32)*/);
}

pub struct TexTriple {
    pub tex: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
}
