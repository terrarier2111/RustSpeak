use std::sync::{Arc, RwLock};
use crate::render::Renderer;
use crate::screen_sys::Screen;
use crate::ScreenSystem;
use crate::ui::{Color, ColorBox, Coloring, Container};

#[derive(Clone)]
pub struct ServerList {
    container: Arc<Container>,
}

impl ServerList {

    pub fn new() -> Self {
        Self {
            container: Arc::new(Container::new()),
        }
    }

}

impl Screen for ServerList {
    fn on_active(&mut self, screen_sys: Arc<ScreenSystem>, renderer: Arc<Renderer>) {
        self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.75, 0.75),
            width: 0.5,
            height: 0.5,
            coloring: Coloring::Color([Color {
                r: 1.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            }, Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }, Color {
                r: 1.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            }, Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            }, Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            }, Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },]),
        }))));
    }

    fn on_deactive(&mut self, screen_sys: Arc<ScreenSystem>, renderer: Arc<Renderer>) {

    }

    fn tick(&mut self, screen_sys: Arc<ScreenSystem>, renderer: Arc<Renderer>, delta: f64) {
    }

    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }
}
