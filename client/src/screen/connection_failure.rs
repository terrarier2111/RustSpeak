use std::any::Any;
use std::sync::{Arc, RwLock};
use crate::Client;
use crate::screen_sys::Screen;
use crate::ui::{Color, ColorBox, Coloring, Component, Container, Tex, TextBox, TextSection};
use wgpu_glyph::{HorizontalAlign, Layout, Text, VerticalAlign};
use crate::render::TexTy;

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

const BOX_WIDTH: f32 = 0.4;
const BOX_HEIGHT: f32 = 0.2;
const CLOSE_WIDTH: f32 = 0.05;
const CLOSE_HEIGHT: f32 = 0.025;

impl Screen for ConnectionFailureScreen {
    fn on_active(&mut self, client: &Arc<Client>) {
        println!("dims: {}", client.renderer.dimensions.get().0);
        self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
            pos: (0.5 - BOX_WIDTH / 2.0, 0.5 - BOX_HEIGHT / 2.0),
            width: BOX_WIDTH,
            height: BOX_HEIGHT,
            coloring: Coloring::Color::<6>([Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }; 6]),
            text: TextSection {
                layout: Layout::default_single_line().v_align(VerticalAlign::Bottom/*Bottom*//*VerticalAlign::Center*/).h_align(HorizontalAlign::Left),
                text: vec![Text::default().with_scale(45.0 * (client.renderer.dimensions.get().0 as f32 / 1920.0))],
                texts: vec![format!("Failed connecting with \"{}\"", &self.server_name)],
            },
        }))));
        /*self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.0, 0.0),
            width: 0.5 - CLOSE_WIDTH,
            height: 0.5 - CLOSE_HEIGHT,
            coloring: Coloring::Tex(Tex {
                ty: TexTy::Atlas(),
            }),
        }))));*/
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>, _delta: f64) {}

    fn on_resize(&mut self, client: &Arc<Client>) {
        self.container.clear();
        self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
            pos: (0.5 - BOX_WIDTH / 2.0, 0.5 - BOX_HEIGHT / 2.0),
            width: BOX_WIDTH,
            height: BOX_HEIGHT,
            coloring: Coloring::Color::<6>([Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }; 6]),
            text: TextSection {
                layout: Layout::default_single_line().v_align(VerticalAlign::Bottom/*Bottom*//*VerticalAlign::Center*/).h_align(HorizontalAlign::Left),
                text: vec![Text::default().with_scale(45.0 * (client.renderer.dimensions.get().0 as f32 / 1920.0))],
                texts: vec![format!("Failed connecting with \"{}\"", &self.server_name)],
            },
        }))));
    }

    #[inline]
    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}