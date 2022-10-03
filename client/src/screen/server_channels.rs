use std::sync::{Arc, RwLock};
use crate::Client;
use crate::screen_sys::Screen;
use crate::ui::Container;

#[derive(Clone)]
pub struct ServerChannels {
    container: Arc<Container>,
}

impl Screen for ServerChannels {
    fn on_active(&mut self, _client: &Arc<Client>) {
        // self.container.add(Arc::new(RwLock::new(Box::new())));
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>, delta: f64) {}

    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}
