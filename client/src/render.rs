use std::borrow::Cow;
use std::mem::size_of;
use std::process::abort;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use flume::Sender;
use wgpu::{BindGroupLayoutEntry, BindingType, BlendState, BufferAddress, BufferUsages, Color, ColorTargetState, ColorWrites, LoadOp, Operations, RenderPass, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPipeline, Sampler, SamplerBindingType, ShaderSource, ShaderStages, Texture, TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};
use wgpu_biolerless::{FragmentShaderState, ModuleSrc, PipelineBuilder, ShaderModuleSources, State, VertexShaderState, WindowSize};
use winit::window::Window;
use crate::atlas::{Atlas, AtlasAlloc};
use bytemuck_derive::Pod;
use bytemuck_derive::Zeroable;

pub struct Renderer {
    pub state: Arc<State>,
    tex_pipeline: RenderPipeline,
    color_pipeline: RenderPipeline,
    pub dimensions: Dimensions,
}

impl Renderer {
    pub fn new(state: Arc<State>, window: &Window) -> Self {
        let (width, height) = window.window_size();
        Self { tex_pipeline: Self::atlas_pipeline(&state), color_pipeline: Self::color_pipeline(&state), state, dimensions: Dimensions::new(width, height) }
    }

    pub fn render(&self, models: Vec<Model>, atlas: Arc<Atlas>/*atlases: Arc<Mutex<Vec<Arc<Atlas>>>>*/) {
        self.state
            .render(
                |view, mut encoder, state| {
                    /*for atlas in atlases.lock().unwrap().iter() {
                        atlas.update(&mut encoder);
                    }*/
                    atlas.update(&mut encoder);
                    let mut atlas_models = vec![];
                    let mut color_models = vec![];
                    for model in models {
                        match &model.color_src {
                            ColorSource::PerVert => {
                                color_models.extend(model.vertices.into_iter().map(|vert| match vert {
                                    Vertex::Color { pos, color } => {
                                        ColorVertex {
                                            pos,
                                            color,
                                        }
                                    },
                                    Vertex::Atlas { .. } => abort(), // FIXME: is it really necessary to abort because of perf stuff?
                                }));
                            },
                            ColorSource::Atlas(_) => {
                                // FIXME: make different atlases work!
                                atlas_models.extend(model.vertices);
                            }
                        }
                    }
                    let color_buffer = state.create_buffer(color_models.as_slice(), BufferUsages::VERTEX);
                    {
                        let attachments = [Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color::BLACK),
                                store: true,
                            },
                        })];
                        let mut render_pass = state.create_render_pass(
                            &mut encoder,
                            &attachments,
                            None,
                        );
                        // let buffer = state.create_buffer(atlas_models.as_slice(), BufferUsages::VERTEX);
                        // render_pass.set_vertex_buffer(0, buffer.slice(..));

                        render_pass.set_vertex_buffer(0, color_buffer.slice(..));
                        render_pass.set_pipeline(&self.color_pipeline);
                        render_pass.draw(0..(color_models.len() as u32), 0..1);
                    }
                    encoder
                },
                &TextureViewDescriptor::default(),
            )
            .unwrap();
    }

    fn color_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new().vertex(VertexShaderState {
            entry_point: "main_vert",
            buffers: &[
                ColorVertex::desc(),
            ],
        }).fragment(FragmentShaderState {
            entry_point: "main_frag",
            targets: &[Some(ColorTargetState {
                format: state.format(),
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
        }).shader_src(ShaderModuleSources::Single(ModuleSrc::Source(ShaderSource::Wgsl(include_str!("ui_color.wgsl").into()))))
            .layout(&state.create_pipeline_layout(&[], &[])).build(state)
    }

    fn atlas_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new().vertex(VertexShaderState {
            entry_point: "main_vert",
            buffers: &[
                AtlasVertex::desc(),
            ],
        }).fragment(FragmentShaderState {
            entry_point: "main_frag",
            targets: &[Some(ColorTargetState {
                format: state.format(),
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
        }).shader_src(ShaderModuleSources::Single(ModuleSrc::Source(ShaderSource::Wgsl(include_str!("ui_atlas.wgsl").into()))))
            .layout(&state.create_pipeline_layout(&[&state.create_bind_group_layout(&[BindGroupLayoutEntry {
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
                }, ])], &[])).build(state)
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
    fn render(&self, sender: Sender<Vec<Vertex>>/*, screen_dims: (u32, u32)*/);
}

pub struct TexTriple {
    pub tex: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
}
