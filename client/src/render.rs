use wgpu::TextureViewDescriptor;
use wgpu_biolerless::State;

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
