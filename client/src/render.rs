use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use flume::Sender;
use wgpu::{BindGroupLayoutEntry, BindingType, Color, LoadOp, Operations, RenderPass, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPipeline, Sampler, SamplerBindingType, ShaderSource, ShaderStages, Texture, TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension};
use wgpu_biolerless::{FragmentShaderState, ModuleSrc, PipelineBuilder, ShaderModuleSources, State, VertexShaderState, WindowSize};
use winit::window::Window;
use crate::atlas::{Atlas, AtlasAlloc};

pub struct Renderer {
    pub state: State,
    tex_pipeline: RenderPipeline,
    color_pipeline: RenderPipeline,
    pub dimensions: Dimensions,
}

impl Renderer {
    pub fn new(state: State, window: &Window) -> Self {
        let (width, height) = window.window_size();
        Self { tex_pipeline: Self::atlas_pipeline(&state), color_pipeline: Self::color_pipeline(&state), state, dimensions: Dimensions::new(width, height) }
    }

    pub fn render(&mut self, models: Vec<Model>, atlas: Arc<Atlas>/*atlases: Arc<Mutex<Vec<Arc<Atlas>>>>*/) {
        self.state
            .render(
                |view, mut encoder, state| {
                    /*for atlas in atlases.lock().unwrap().iter() {
                        atlas.update(&mut encoder);
                    }*/
                    atlas.update(&mut encoder);
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
                    // FIXME: render models
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

    fn atlas_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new().vertex(VertexShaderState {
            entry_point: "main_vert",
            buffers: &[],
        }).fragment(FragmentShaderState {
            entry_point: "main_frag",
            targets: &[]
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

struct ColorVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

struct AtlasVertex {
    pos: [f32; 2],
    alpha: f32,
    uv: (u32, u32),
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
