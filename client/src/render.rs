use wgpu::{BindGroupLayoutEntry, BindingType, RenderPass, RenderPipeline, SamplerBindingType, ShaderStages, TextureSampleType, TextureViewDescriptor, TextureViewDimension};
use wgpu_biolerless::{PipelineBuilder, State};

pub struct Renderer {
    pub state: State,
    tex_pipeline: RenderPipeline,
    color_pipeline: RenderPipeline,
}

impl Renderer {
    pub fn new(state: State) -> Self {
        Self { tex_pipeline: Self::tex_pipeline(&state), color_pipeline: Self::color_pipeline(&state), state }
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
        PipelineBuilder::new().layout(&state.create_pipeline_layout(&[])).build(state)
    }

    fn tex_pipeline(state: &State) -> RenderPipeline {
        PipelineBuilder::new().layout(&state.create_pipeline_layout(&[&state.create_bind_group_layout(&[BindGroupLayoutEntry {
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

pub struct ColorVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

pub struct TexVertex {
    pub pos: [f32; 2],
    pub alpha: f32,
    // FIXME: somehow add texture (but probably only per model and not per vertex)
}
