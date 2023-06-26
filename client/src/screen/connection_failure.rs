use std::sync::{Arc, RwLock};
use crate::Client;
use crate::screen_sys::Screen;
use crate::ui::{Color, Coloring, Container, TextBox, TextSection};
use wgpu_glyph::{HorizontalAlign, Layout, Text, VerticalAlign};

#[derive(Clone)]
pub struct ConnectionFailureScreen {
    container: Arc<Container>,
    server_name: String,
}

impl ConnectionFailureScreen {
    
    pub fn new(server_name: String) -> Self {
        Self {
            container: Arc::new(Default::default()),
            server_name,
        }
    }
    
}

impl Screen for ConnectionFailureScreen {
    fn on_active(&mut self, _client: &Arc<Client>) {
        self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
            pos: (0.5, 0.5),
            width: 0.2,
            height: 0.1,
            coloring: Coloring::Color::<6>([Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }; 6]),
            text: TextSection {
                layout: Layout::default_single_line().v_align(VerticalAlign::Bottom/*Bottom*//*VerticalAlign::Center*/).h_align(HorizontalAlign::Left),
                text: vec![Text::default().with_scale(30.0)],
                texts: vec![format!("Failed connecting with \"{}\"", &self.server_name)],
            },
        }))));
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>, _delta: f64) {}

    #[inline]
    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}