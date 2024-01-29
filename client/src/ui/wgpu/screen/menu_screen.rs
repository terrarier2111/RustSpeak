use std::sync::{Arc, RwLock};
use crate::Client;
use crate::ui::wgpu::{ctx, DARK_GRAY_UI};
use crate::ui::wgpu::render::GlyphBuilder;
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Button, Color, Coloring, Container, TextBox};

use super::server_list;

#[derive(Clone)]
pub struct Menu {
    container: Arc<Container>,
}

impl Menu {
    
    pub fn new() -> Self {
        Self {
            container: Arc::new(Default::default()),
        }
    }
    
}

const BOX_WIDTH: f32 = 0.1;
const BOX_HEIGHT: f32 = 0.05;
const BOX_BORDER: f32 = 0.015;

const BOX_SCREEN_OFFSET_X: f32 = 0.3 + 2.0 * BOX_BORDER;
const BOX_SCREEN_OFFSET_Y: f32 = 0.3;

#[inline]
fn rev_y_uv(uv: (f32, f32)) -> (f32, f32) {
    (uv.0, 1.0 - uv.1)
}

#[inline]
fn rev_y(y: f32) -> f32 {
    1.0 - y
}

impl Screen for Menu {
    fn on_active(&mut self, client: &Arc<Client>) {
        self.container.add(Arc::new(RwLock::new(Box::new(Button {
            inner_box: TextBox {
                pos: (BOX_SCREEN_OFFSET_X, BOX_SCREEN_OFFSET_Y),
                width: BOX_WIDTH,
                height: BOX_HEIGHT,
                coloring: Coloring::Color([DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI]),
                texts: vec![GlyphBuilder::new("Menu", (BOX_SCREEN_OFFSET_X, BOX_SCREEN_OFFSET_Y), (BOX_WIDTH, BOX_HEIGHT)).in_bounds_off((0.03, 0.03)).build()],
            },
            data: None,
            on_click: Arc::new(Box::new(|button, client| {})),
        }))));
        let pos = (BOX_SCREEN_OFFSET_X + BOX_WIDTH + BOX_BORDER, BOX_SCREEN_OFFSET_Y);
        self.container.add(Arc::new(RwLock::new(Box::new(Button {
            inner_box: TextBox {
                pos,
                width: BOX_WIDTH,
                height: BOX_HEIGHT,
                coloring: Coloring::Color([DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI]),
                texts: vec![GlyphBuilder::new("Profiles", pos, (BOX_WIDTH, BOX_HEIGHT)).in_bounds_off((0.03, 0.03)).build()],
            },
            data: None,
            on_click: Arc::new(Box::new(|button, client| {})),
        }))));
        let pos = (pos.0 + BOX_WIDTH + BOX_BORDER, pos.1);
        self.container.add(Arc::new(RwLock::new(Box::new(Button {
            inner_box: TextBox {
                pos,
                width: BOX_WIDTH,
                height: BOX_HEIGHT,
                coloring: Coloring::Color([DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI, DARK_GRAY_UI]),
                texts: vec![GlyphBuilder::new("Servers", pos, (BOX_WIDTH, BOX_HEIGHT)).in_bounds_off((0.03, 0.03)).build()],
            },
            data: None,
            on_click: Arc::new(Box::new(|button, client| {
                ctx().screen_sys.push_screen(Box::new(server_list::ServerList::new()));
                // FIXME: refresh screen, disable glyphs for current screen
            })),
        }))));
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
