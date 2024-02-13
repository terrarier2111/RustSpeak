use std::sync::{Arc, RwLock};
use pollster::FutureExt;

use crate::packet::ClientPacket;
use crate::server::Server;
use crate::ui::wgpu::render::GlyphBuilder;
use crate::Client;
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Button, Color, Coloring, Container, TextBox};

#[derive(Clone)]
pub struct ServerChannelsScreen {
    container: Arc<Container>,
    server: Arc<Server>,
}

impl ServerChannelsScreen {

    pub fn new(server: Arc<Server>) -> Self {
        Self {
            container: Arc::new(Container::new()),
            server,
        }
    }

}

const CHANNEL_ENTRY_SIZE: (f32, f32) = (0.2, 0.1);
const SPACING_OFFSET: f32 = CHANNEL_ENTRY_SIZE.1 * 0.1;

impl Screen for ServerChannelsScreen {
    fn on_active(&mut self, client: &Arc<Client>) {
        // self.container.add(Arc::new(RwLock::new(Box::new())));
        // FIXME: sort by sort id
        for (idx, channel) in self.server.channels.load().iter().enumerate() {
            println!("added channel {idx}");
            let off_y = (CHANNEL_ENTRY_SIZE.1 + SPACING_OFFSET) * idx as f32 + CHANNEL_ENTRY_SIZE.1;
            let uuid = channel.0.clone();
            self.container.add(Arc::new(RwLock::new(Box::new(Button {
                inner_box: TextBox {
                    pos: (0.0, 1.0 - off_y),
                    width: CHANNEL_ENTRY_SIZE.0,
                    height: CHANNEL_ENTRY_SIZE.1,
                    coloring: Coloring::Color([Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 }; 6]),
                    texts: vec![GlyphBuilder::new(&channel.1.name, (0.0, 1.0 - off_y), CHANNEL_ENTRY_SIZE).build()],
                },
                data: (uuid, self.server.clone()),
                on_click: Arc::new(Box::new(|button, client| {
                    let channel_switch = ClientPacket::SwitchChannel { channel: button.data.0.clone() }.encode().unwrap();
                    button.data.1.connection.get().unwrap().send_reliable(&channel_switch).block_on().unwrap();
                })),
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
