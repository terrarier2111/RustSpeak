use std::sync::{Arc, RwLock};
use crate::Client;
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Container, TextBox};

#[derive(Clone)]
pub struct ServerChannels {
    container: Arc<Container>,
}

impl Screen for ServerChannels {
    fn on_active(&mut self, client: &Arc<Client>) {
        // self.container.add(Arc::new(RwLock::new(Box::new())));
        /*for channel in client.server.load().unwrap().channels.load().iter() {
            self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
                pos: (0.0, 0.0),
                width: 0.0,
                height: 0.0,
                coloring: (),
                text: (),
            }))));
        }*/
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>) {}

    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}
