use crate::atlas::UV;
use crate::render::{ColorSource, Model, TexTriple, TexTy, Vertex};
use crate::screen_sys::ScreenSystem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use fontdue::{Font, FontSettings};
use wgpu::{Sampler, Texture, TextureView};
use wgpu_text::section::{BuiltInLineBreaker, Layout, Section, Text};
use crate::Renderer;

pub trait Component: Send + Sync {
    fn build_model(&self) -> Model;

    // fn is_inbounds(&self, pos: (f32, f32)) -> bool; // FIXME: is this one better?

    fn do_render(&self, _renderer: &Arc<Renderer>) {}

    fn pos(&self) -> (f32, f32);

    fn dims(&self) -> (f32, f32);

    fn on_click(&mut self, screen_sys: &Arc<ScreenSystem>);

    fn on_scroll(&mut self, screen_sys: &Arc<ScreenSystem>);

    fn on_hover(&mut self, screen_sys: &Arc<ScreenSystem>, mode: HoverMode);
}

#[derive(Copy, Clone)]
pub enum HoverMode {
    Enter,
    Exit,
}

pub struct UIComponent {
    inner: Arc<InnerUIComponent>,
}

impl UIComponent {
    pub fn build_model(&self) -> Model {
        self.inner.build_model()
    }

    pub fn is_inbounds(&self, pos: (f32, f32)) -> bool {
        let inner = self.inner.inner.read().unwrap();
        let dims = inner.dims();
        let inner_pos = inner.pos();
        let bounds = (inner_pos.0 + dims.0, inner_pos.1 + dims.1);
        (pos.0 >= inner_pos.0 && pos.1 >= inner_pos.1) && (pos.0 <= bounds.0 && pos.1 <= bounds.1)
    }
}

pub struct InnerUIComponent {
    inner: Arc<RwLock<Box<dyn Component>>>, // FIXME: should we prefer a Mutex over a Rwlock?
    precomputed_model: Mutex<Model>,
    dirty: AtomicBool,
}

impl InnerUIComponent {
    fn build_model(&self) -> Model {
        if self.dirty.fetch_and(false, Ordering::Acquire) {
            let model = self.inner.write().unwrap().build_model();
            *self.precomputed_model.lock().unwrap() = model.clone();
            model
        } else {
            self.precomputed_model.lock().unwrap().clone()
        }
    }

    pub fn make_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }
}

#[derive(Copy, Clone)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn into_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

pub struct Tex {
    // pub alpha: f32, // FIXME: try readding this!
    pub ty: TexTy,
}

pub enum Coloring<const VERTICES: usize> {
    Color([Color; VERTICES]),
    Tex(Tex),
}

pub struct Container {
    components: RwLock<Vec<UIComponent>>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            components: Default::default(),
        }
    }

    pub fn add(self: &Arc<Self>, component: Arc<RwLock<Box<dyn Component>>>) {
        let model = component.read().unwrap().build_model();
        self.components.write().unwrap().push(UIComponent {
            inner: Arc::new(InnerUIComponent {
                inner: component,
                precomputed_model: Mutex::new(model),
                dirty: AtomicBool::new(false),
            }),
        });
    }

    pub fn build_models(&self, renderer: &Arc<Renderer>) -> Vec<Model> {
        let mut models = vec![];
        for component in self.components.read().unwrap().iter() {
            models.push(component.build_model());
            component.inner.inner.read().unwrap().do_render(renderer);
        }
        models
    }
}

pub struct Button {
    pub inner_box: ColorBox,
    pub on_click: Arc<Box<dyn Fn(&mut Button, &Arc<ScreenSystem>) + Send + Sync>>,
}

impl Component for Button {
    fn build_model(&self) -> Model {
        self.inner_box.build_model()
    }

    fn pos(&self) -> (f32, f32) {
        self.inner_box.pos()
    }

    fn dims(&self) -> (f32, f32) {
        self.inner_box.dims()
    }

    fn on_click(&mut self, screen_sys: &Arc<ScreenSystem>) {
        let func = self.on_click.clone();
        func(self, screen_sys);
    }

    fn on_scroll(&mut self, _screen_sys: &Arc<ScreenSystem>) {}

    fn on_hover(&mut self, _screen_sys: &Arc<ScreenSystem>, _mode: HoverMode) {}
}

pub struct ColorBox {
    pub pos: (f32, f32),
    pub width: f32,
    pub height: f32,
    pub coloring: Coloring<6>,
}

impl Component for ColorBox {
    fn build_model(&self) -> Model {
        let (x_off, y_off) = ((2.0 * self.pos.0), (2.0 * self.pos.1));
        let vertices = [
            [-1.0 + x_off, -1.0 + y_off],
            [2.0 * self.width - 1.0 + x_off, -1.0 + y_off],
            [
                2.0 * self.width - 1.0 + x_off,
                2.0 * self.height - 1.0 + y_off,
            ],
            [-1.0 + x_off, -1.0 + y_off],
            [-1.0 + x_off, 2.0 * self.height - 1.0 + y_off],
            [
                2.0 * self.width - 1.0 + x_off,
                2.0 * self.height - 1.0 + y_off,
            ],
        ];
        let vertices = match &self.coloring {
            Coloring::Color(colors) => {
                let mut ret = Vec::with_capacity(6);
                for (i, pos) in vertices.into_iter().enumerate() {
                    ret.push(Vertex::Color {
                        pos,
                        color: colors[i].into_array(),
                    });
                }
                ret
            }
            Coloring::Tex(tex) => {
                let mut ret = Vec::with_capacity(6);
                for pos in vertices {
                    ret.push(Vertex::Atlas {
                        pos,
                        alpha: 1.0, // FIXME: make this actually parameterized!
                        uv: match &tex.ty {
                            TexTy::Atlas(atlas) => atlas.uv().into_tuple(),
                        },
                    });
                }
                ret
            }
        };
        Model {
            vertices,
            color_src: match &self.coloring {
                Coloring::Color(_) => ColorSource::PerVert,
                Coloring::Tex(tex) => match &tex.ty {
                    TexTy::Atlas(atlas) => ColorSource::Atlas(atlas.atlas().clone()),
                },
            },
        }
    }

    fn pos(&self) -> (f32, f32) {
        self.pos
    }

    fn dims(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn on_click(&mut self, _screen_sys: &Arc<ScreenSystem>) {}

    fn on_scroll(&mut self, _screen_sys: &Arc<ScreenSystem>) {}

    fn on_hover(&mut self, _screen_sys: &Arc<ScreenSystem>, _mode: HoverMode) {}
}

pub struct TextBox<'a> {
    pub pos: (f32, f32),
    pub width: f32,
    pub height: f32,
    pub coloring: Coloring<6>,
    pub text: TextSection<'a>,
}

impl Component for TextBox<'_> {
    fn build_model(&self) -> Model {
        let (x_off, y_off) = ((2.0 * self.pos.0), (2.0 * self.pos.1));
        let vertices = [
            [-1.0 + x_off, -1.0 + y_off],
            [2.0 * self.width - 1.0 + x_off, -1.0 + y_off],
            [
                2.0 * self.width - 1.0 + x_off,
                2.0 * self.height - 1.0 + y_off,
            ],
            [-1.0 + x_off, -1.0 + y_off],
            [-1.0 + x_off, 2.0 * self.height - 1.0 + y_off],
            [
                2.0 * self.width - 1.0 + x_off,
                2.0 * self.height - 1.0 + y_off,
            ],
        ];
        let vertices = match &self.coloring {
            Coloring::Color(colors) => {
                let mut ret = Vec::with_capacity(6);
                for (i, pos) in vertices.into_iter().enumerate() {
                    ret.push(Vertex::Color {
                        pos,
                        color: colors[i].into_array(),
                    });
                }
                ret
            }
            Coloring::Tex(tex) => {
                let mut ret = Vec::with_capacity(6);
                for pos in vertices {
                    ret.push(Vertex::Atlas {
                        pos,
                        alpha: 1.0, // FIXME: make this actually parameterized!
                        uv: match &tex.ty {
                            TexTy::Atlas(atlas) => atlas.uv().into_tuple(),
                        },
                    });
                }
                ret
            }
        };
        Model {
            vertices,
            color_src: match &self.coloring {
                Coloring::Color(_) => ColorSource::PerVert,
                Coloring::Tex(tex) => match &tex.ty {
                    TexTy::Atlas(atlas) => ColorSource::Atlas(atlas.atlas().clone()),
                },
            },
        }
    }

    fn do_render(&self, renderer: &Arc<Renderer>) {
        let (width, height) = renderer.dimensions.get();
        renderer.queue_glyph(0, Section {
            screen_position: (self.pos.0 * width as f32/*(self.pos.0 - 1.0) / 2.0*/, /*0.0*/(1.0 - self.pos.1 - self.height) * height as f32/*(self.pos.1 - 1.0) / 2.0*/),
            bounds: (self.width * width as f32, self.height * height as f32),
            layout: self.text.layout,
            text: self.text.text.iter().enumerate().map(|txt| {
                txt.1.with_text(&*self.text.texts[txt.0])
            }).collect::<Vec<_>>(),
        });
    }

    fn pos(&self) -> (f32, f32) {
        self.pos
    }

    fn dims(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn on_click(&mut self, _screen_sys: &Arc<ScreenSystem>) {}

    fn on_scroll(&mut self, _screen_sys: &Arc<ScreenSystem>) {}

    fn on_hover(&mut self, _screen_sys: &Arc<ScreenSystem>, _mode: HoverMode) {}
}

pub struct TextSection<'a/*, X = Extra*/> {
    /// Built in layout, can be overridden with custom layout logic see queue_custom_layout
    pub layout: Layout<BuiltInLineBreaker>,
    /// Text to render, rendered next to one another according the layout.
    pub text: Vec<Text<'a/*, X*/>>,
    pub texts: Vec</*Arc<*/String/*>*/>,
}
