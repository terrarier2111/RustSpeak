use std::sync::{Arc, RwLock};
use crate::ui::wgpu::render::GlyphBuilder;
use crate::Client;
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Color, Coloring, Container, TextBox};

#[derive(Clone)]
pub struct ServerChannelsScreen {
    container: Arc<Container>,
}

impl ServerChannelsScreen {

    pub fn new() -> Self {
        Self {
            container: Arc::new(Container::new()),
        }
    }

}

const CHANNEL_ENTRY_SIZE: (f32, f32) = (0.2, 0.1);
const SPACING_OFFSET: f32 = CHANNEL_ENTRY_SIZE.1 * 0.1;

impl Screen for ServerChannelsScreen {
    fn on_active(&mut self, client: &Arc<Client>) {
        // self.container.add(Arc::new(RwLock::new(Box::new())));
        for (idx, channel) in client.server.load().as_ref().unwrap().channels.load().iter().enumerate() {
            let off_y = (CHANNEL_ENTRY_SIZE.1 + SPACING_OFFSET) * idx as f32;
            self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
                pos: (0.0, 1.0 - off_y),
                width: CHANNEL_ENTRY_SIZE.0,
                height: CHANNEL_ENTRY_SIZE.1,
                coloring: Coloring::Color([Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 }; 6]),
                texts: vec![GlyphBuilder::new(&channel.1.name, (0.0, 1.0 - off_y), CHANNEL_ENTRY_SIZE).build()],
            }))));
        }
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {
        self.container.clear();
    }

    fn tick(&mut self, _client: &Arc<Client>) {}

    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}
