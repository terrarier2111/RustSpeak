use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use flume::Sender;
use wgpu::{BindGroupLayoutEntry, BindingType, RenderPass, RenderPipeline, Sampler, SamplerBindingType, ShaderSource, ShaderStages, Texture, TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension};
use wgpu_biolerless::{FragmentShaderState, ModuleSrc, PipelineBuilder, ShaderModuleSources, State, VertexShaderState, WindowSize};
use winit::window::Window;
use crate::atlas::AtlasAlloc;

pub struct Renderer {
    pub state: State,
    tex_pipeline: RenderPipeline,
    color_pipeline: RenderPipeline,
    pub dimensions: Dimensions,
}

impl Renderer {
    pub fn new(state: State, window: &Window) -> Self {
        let (width, height) = window.window_size();
        Self { tex_pipeline: Self::tex_pipeline(&state), color_pipeline: Self::color_pipeline(&state), state, dimensions: Dimensions::new(width, height) }
    }

    pub fn render(&mut self) {
        self.state
            .render(
                |view, encoder, state| {

                    encoder
                },
                &TextureViewDescriptor::default(),
            )
            .unwrap();
    }

    fn color_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new().vertex(VertexShaderState {
            entry_point: "main_vert",
            buffers: &[],
        }).fragment(FragmentShaderState {
            entry_point: "main_frag",
            targets: &[]
        }).shader_src(ShaderModuleSources::Single(ModuleSrc::Source(ShaderSource::Wgsl(include_str!("ui_color.wgsl").into()))))
            .layout(&state.create_pipeline_layout(&[], &[])).build(state)
    }

    fn tex_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new().vertex(VertexShaderState {
            entry_point: "main_vert",
            buffers: &[],
        }).fragment(FragmentShaderState {
            entry_point: "main_frag",
            targets: &[]
        }).shader_src(ShaderModuleSources::Single(ModuleSrc::Source(ShaderSource::Wgsl(include_str!("ui_tex.wgsl").into()))))
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

pub enum TexTy {
    Atlas(Arc<AtlasAlloc>),
    Simple(TexTriple),
}

pub enum Vertex {
    Color {
        pos: [f32; 2],
        color: [f32; 4],
    },
    Tex {
        pos: [f32; 2],
        alpha: f32,
        // FIXME: somehow add texture (but probably only per model and not per vertex) - although for now we will probably do it per vertex
    },
}

struct ColorVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

struct TexVertex {
    pub pos: [f32; 2],
    pub alpha: f32,
    // FIXME: somehow add texture (but probably only per model and not per vertex) - although for now we will probably do it per vertex
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
