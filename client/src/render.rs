use wgpu::{RenderPass, RenderPipeline, TextureViewDescriptor};
use wgpu_biolerless::{PipelineBuilder, State, VertexShaderState};

pub struct Renderer {
    pub state: State,
}

impl Renderer {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn render(&mut self) {
        self.state
            .render(
                |view, encoder, state| encoder,
                &TextureViewDescriptor::default(),
            )
            .unwrap();
    }
}

fn color_render_pipeline(state: &State) -> RenderPipeline {
    PipelineBuilder::new().vertex(VertexShaderState {
        entry_point: "main_vs",
        buffers: &[],
    }).build(state)
}
