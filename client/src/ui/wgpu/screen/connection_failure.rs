use std::sync::{Arc, RwLock};
use crate::Client;
use crate::ui::wgpu::atlas::AtlasAlloc;
use crate::ui::wgpu::ctx;
use crate::ui::wgpu::render::{GlyphBuilder, TexTy};
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Color, ColorBox, Coloring, Container, Tex, TextBox};

#[derive(Clone)]
pub struct ConnectionFailureScreen {
    container: Arc<Container>,
    server_name: String,
    alloc: Arc<AtlasAlloc>,
}

const CLOSE_BOX_PATH: &str = "./resources/cross.jpg";

impl ConnectionFailureScreen {

    pub fn new(client: &Arc<Client>, server_name: String) -> Self {
        let mut buf = image::open(CLOSE_BOX_PATH).unwrap();
        let buf = buf.into_rgba8();
        println!("bytes: {} exp: {}", buf.as_ref().len(), buf.dimensions().0 * 4 * buf.dimensions().1);
        let alloc = ctx().atlas.alloc(CLOSE_BOX_PATH.to_string(), buf.dimensions(), buf.as_ref());

        Self {
            container: Arc::new(Default::default()),
            server_name,
            alloc,
        }
    }

}

const BOX_WIDTH: f32 = 0.4;
const BOX_HEIGHT: f32 = 0.2;
const CLOSE_WIDTH: f32 = 0.05;
const CLOSE_HEIGHT: f32 = 0.025;

impl Screen for ConnectionFailureScreen {
    fn on_active(&mut self, client: &Arc<Client>) {
        println!("dims: {}", ctx().renderer.dimensions.get().0);
        let pos = (0.5 - BOX_WIDTH / 2.0, 0.5 - BOX_HEIGHT / 2.0);
        self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
            pos,
            width: BOX_WIDTH,
            height: BOX_HEIGHT,
            coloring: Coloring::Color::<6>([Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }; 6]),
            text: GlyphBuilder::new(&format!("Failed connecting with \"{}\"", &self.server_name),
            (0.0, 0.0),
            pos, (BOX_WIDTH, BOX_HEIGHT)).build()/*TextSection {
                layout: Layout::default_single_line().v_align(VerticalAlign::Bottom/*Bottom*//*VerticalAlign::Center*/).h_align(HorizontalAlign::Left),
                text: vec![Text::default().with_scale(45.0 * (ctx().renderer.dimensions.get().0 as f32 / 1920.0))],
                texts: vec![format!("Failed connecting with \"{}\"", &self.server_name)],
            }*/,
        }))));
        self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.5 - CLOSE_WIDTH, 0.5 - CLOSE_HEIGHT),
            width: CLOSE_WIDTH,
            height: CLOSE_HEIGHT,
            coloring: Coloring::Tex(Tex {
                ty: TexTy::Atlas(self.alloc.clone()),
            }),
        }))));
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>, _delta: f64) {}

    fn on_resize(&mut self, client: &Arc<Client>) {
        self.container.clear();
        let pos = (0.5 - BOX_WIDTH / 2.0, 0.5 - BOX_HEIGHT / 2.0);
        self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
            pos,
            width: BOX_WIDTH,
            height: BOX_HEIGHT,
            coloring: Coloring::Color::<6>([Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }; 6]),
            text: GlyphBuilder::new(&format!("Failed connecting with \"{}\"", &self.server_name),
            (0.0, 0.0),
            pos, (BOX_WIDTH, BOX_HEIGHT)).build()/*TextSection {
                layout: Layout::default_single_line().v_align(VerticalAlign::Bottom/*Bottom*//*VerticalAlign::Center*/).h_align(HorizontalAlign::Left),
                text: vec![Text::default().with_scale(45.0 * (ctx().renderer.dimensions.get().0 as f32 / 1920.0))],
                texts: vec![format!("Failed connecting with \"{}\"", &self.server_name)],
            }*/,
        }))));
        self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.5 - CLOSE_WIDTH, 0.5 - CLOSE_HEIGHT),
            width: CLOSE_WIDTH,
            height: CLOSE_HEIGHT,
            coloring: Coloring::Tex(Tex {
                ty: TexTy::Atlas(self.alloc.clone()),
            }),
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