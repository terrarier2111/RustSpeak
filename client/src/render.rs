use crate::atlas::{Atlas, AtlasAlloc, AtlasId};
use bytemuck_derive::Pod;
use bytemuck_derive::Zeroable;
use flume::Sender;
use std::borrow::Cow;
use std::collections::HashMap;
use std::mem::size_of;
use std::process::abort;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use dashmap::DashMap;
use wgpu::{BindGroupLayoutEntry, BindingType, BlendState, BufferAddress, BufferUsages, Color, ColorTargetState, ColorWrites, LoadOp, Operations, RenderPass, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPipeline, Sampler, SamplerBindingType, ShaderSource, ShaderStages, Texture, TextureFormat, TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};
use wgpu::util::StagingBelt;
use wgpu_biolerless::{
    FragmentShaderState, ModuleSrc, PipelineBuilder, ShaderModuleSources, State, VertexShaderState,
    WindowSize,
};
use wgpu_glyph::{ab_glyph, GlyphBrush, GlyphBrushBuilder, Section};
use winit::window::Window;

pub struct Renderer {
    pub state: Arc<State>,
    tex_pipeline: RenderPipeline,
    color_pipeline: RenderPipeline,
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
            tex_pipeline: Self::atlas_pipeline(&state),
            color_pipeline: Self::color_pipeline(&state),
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
                    let mut atlas_models: HashMap<AtlasId, Vec<AtlasVertex>> = HashMap::new();
                    let mut color_models = vec![];
                    for model in models {
                        match &model.color_src {
                            ColorSource::PerVert => {
                                color_models.extend(model.vertices.into_iter().map(
                                    |vert| match vert {
                                        Vertex::Color { pos, color } => ColorVertex { pos, color },
                                        Vertex::Atlas { .. } => abort(), // FIXME: is it really necessary to abort because of perf stuff?
                                    },
                                ));
                            }
                            ColorSource::Atlas(atlas) => {
                                // FIXME: make different atlases work!
                                let vertices = model.vertices.into_iter().map(|vert| match vert {
                                    Vertex::Color { .. } => abort(),
                                    Vertex::Atlas { pos, alpha, uv } => {
                                        AtlasVertex { pos, alpha, uv }
                                    }
                                });
                                if let Some(mut models) = atlas_models.get_mut(&atlas.id()) {
                                    models.extend(vertices);
                                } else {
                                    atlas_models
                                        .insert(atlas.id(), vertices.collect::<Vec<AtlasVertex>>());
                                }
                            }
                        }
                    }
                    let color_buffer =
                        state.create_buffer(color_models.as_slice(), BufferUsages::VERTEX);
                    {
                        let attachments = [Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color::RED),
                                store: true,
                            },
                        })];
                        let mut render_pass =
                            state.create_render_pass(&mut encoder, &attachments, None);
                        // let buffer = state.create_buffer(atlas_models.as_slice(), BufferUsages::VERTEX);
                        // render_pass.set_vertex_buffer(0, buffer.slice(..));

                        render_pass.set_vertex_buffer(0, color_buffer.slice(..));
                        render_pass.set_pipeline(&self.color_pipeline);
                        render_pass.draw(0..(color_models.len() as u32), 0..1);
                    }
                    for glyph in self.glyphs.lock().unwrap().iter() {
                        let mut staging_belt = glyph.staging_belt.lock().unwrap();
                        let (width, height) = self.dimensions.get();
                        glyph.brush.lock().unwrap().draw_queued(state.device(), &mut staging_belt, &mut encoder, view, width, height);
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

    fn color_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new()
            .vertex(VertexShaderState {
                entry_point: "main_vert",
                buffers: &[ColorVertex::desc()],
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
                ShaderSource::Wgsl(include_str!("ui_color.wgsl").into()),
            )))
            .layout(&state.create_pipeline_layout(&[], &[]))
            .build(state)
    }

    fn atlas_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new()
            .vertex(VertexShaderState {
                entry_point: "main_vert",
                buffers: &[AtlasVertex::desc()],
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
                ShaderSource::Wgsl(include_str!("ui_atlas.wgsl").into()),
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
    Color {
        pos: [f32; 2],
        color: [f32; 4],
    },
    Atlas {
        pos: [f32; 2],
        alpha: f32,
        uv: (u32, u32),
    },
}

#[derive(Pod, Zeroable, Copy, Clone)]
#[repr(C)]
struct ColorVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

impl ColorVertex {
    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<ColorVertex>() as BufferAddress,
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

struct AtlasVertex {
    pos: [f32; 2],
    alpha: f32,
    uv: (u32, u32),
}

impl AtlasVertex {
    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<AtlasVertex>() as BufferAddress,
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
