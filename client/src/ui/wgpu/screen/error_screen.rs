use std::sync::{Arc, RwLock};
use crate::Client;
use crate::ui::wgpu::atlas::AtlasAlloc;
use crate::ui::wgpu::ctx;
use crate::ui::wgpu::render::{GlyphBuilder, TexTy};
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Color, ColorBox, Coloring, Container, Tex, TextBox};

#[derive(Clone)]
pub struct ErrorScreen {
    container: Arc<Container>,
    text: String,
    // alloc: Arc<AtlasAlloc>,
}

const CLOSE_BOX_PATH: &str = "./resources/cross.jpg";

impl ErrorScreen {

    pub fn new(client: &Arc<Client>, text: String) -> Self {
        /*let mut buf = image::open(CLOSE_BOX_PATH).unwrap();
        let buf = buf.into_rgba8();
        println!("bytes: {} exp: {}", buf.as_ref().len(), buf.dimensions().0 * 4 * buf.dimensions().1);
        let alloc = ctx().atlas.alloc(CLOSE_BOX_PATH.to_string(), buf.dimensions(), buf.as_ref());*/

        Self {
            container: Arc::new(Default::default()),
            text,
            // alloc,
        }
    }

}

const BOX_WIDTH: f32 = 0.4;
const BOX_HEIGHT: f32 = 0.2;
const CLOSE_WIDTH: f32 = 0.05;
const CLOSE_HEIGHT: f32 = 0.025;

impl Screen for ErrorScreen {
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
            texts: vec![GlyphBuilder::new(self.text.as_str(),
            pos, (BOX_WIDTH / 2.0, BOX_HEIGHT / 2.0)).build()],
        }))));
        /*self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.5 - CLOSE_WIDTH, 0.5 - CLOSE_HEIGHT),
            width: CLOSE_WIDTH,
            height: CLOSE_HEIGHT,
            coloring: Coloring::Tex(Tex {
                ty: TexTy::Atlas(self.alloc.clone()),
            }),
        }))));*/
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>) {}

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
            texts: vec![GlyphBuilder::new(self.text.as_str(),
            pos, (BOX_WIDTH / 4.0, BOX_HEIGHT / 4.0)).build()],
        }))));
        /*self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.5 - CLOSE_WIDTH, 0.5 - CLOSE_HEIGHT),
            width: CLOSE_WIDTH,
            height: CLOSE_HEIGHT,
            coloring: Coloring::Tex(Tex {
                ty: TexTy::Atlas(self.alloc.clone()),
            }),
        }))));*/
    }

    #[inline]
    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    #[inline(always)]
    fn is_closable(&self) -> bool {
        true
    }

    #[inline(always)]
    fn is_tick_always(&self) -> bool {
        false
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}