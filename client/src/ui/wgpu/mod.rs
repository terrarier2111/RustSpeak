use std::ops::Deref;
use std::sync::Arc;
use wgpu::TextureFormat;
use wgpu_biolerless::StateBuilder;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::window::{Window, WindowBuilder};
use crate::Client;
use crate::data_structures::conc_once_cell::ConcurrentOnceCell;
use crate::ui::wgpu::atlas::Atlas;
use crate::ui::wgpu::render::Renderer;
use crate::ui::wgpu::screen::menu_screen::Menu;
use crate::ui::wgpu::screen::server_list::ServerList;
use crate::ui::wgpu::screen_sys::ScreenSystem;

mod ui;
mod atlas;
mod render;
mod screen_sys;
mod screen;

pub(crate) const LIGHT_GRAY_GPU: wgpu::Color = wgpu::Color {
    r: 0.384,
    g: 0.396,
    b: 0.412,
    a: 1.0,
};

pub(crate) const DARK_GRAY_UI: ui::Color = ui::Color {
    r: 0.224,
    g: 0.239,
    b: 0.278,
    a: 1.0,
};

pub(crate) struct UiCtx {
    pub(crate) screen_sys: Arc<ScreenSystem>,
    pub(crate) renderer: Arc<Renderer>,
    pub(crate) atlas: Arc<Atlas>,
    pub(crate) window: Arc<Window>,
}

static UI_CTX: ConcurrentOnceCell<Arc<UiCtx>> = ConcurrentOnceCell::new();

pub(crate) fn ctx() -> &'static Arc<UiCtx> {
    UI_CTX.get().unwrap()
}

pub fn run(client: Arc<Client>) -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::new().build()?;
    let window = Arc::new(WindowBuilder::new()
        .with_title("RustSpeak")
        .build(&event_loop)
        .unwrap());
    let state = Arc::new(pollster::block_on(
        StateBuilder::new().window(window.clone()).build(),
    )?);
    let atlas = Arc::new(Atlas::new(
        state.clone(),
        (1024, 1024),
        TextureFormat::Rgba8Uint,
    ));
    let renderer = Arc::new(Renderer::new(state.clone(), &window)?);
    let screen_sys = Arc::new(ScreenSystem::new());
    // screen_sys.push_screen(Box::new(Menu::new()));
    screen_sys.push_screen(Box::new(ServerList::new()));

    UI_CTX.try_init_silent(Arc::new(UiCtx {
        screen_sys: screen_sys.clone(),
        renderer: renderer.clone(),
        atlas: atlas.clone(),
        window: window.clone(),
    })).unwrap();

    let mut mouse_pos = (0.0, 0.0);
    event_loop.run(move |event, control_flow| match event {
        Event::WindowEvent { window_id, event } if window_id == window.id() => match event {
            WindowEvent::Resized(size) => {
                if !state.resize(size) {
                    panic!("Couldn't resize!");
                } else {
                    renderer.dimensions.set(size.width, size.height);
                }
            }
            WindowEvent::Moved(_) => {}
            WindowEvent::CloseRequested => {
                control_flow.exit();
            }
            WindowEvent::Destroyed => {}
            WindowEvent::DroppedFile(_) => {}
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::Focused(_) => {}
            WindowEvent::KeyboardInput { event, .. } => {
                screen_sys.press_key(event.physical_key, event.state == ElementState::Pressed);
            }
            WindowEvent::ModifiersChanged(_) => {}
            WindowEvent::CursorMoved { position, .. } => {
                let (width, height) = renderer.dimensions.get();
                mouse_pos = (position.x / width as f64, 1.0 - position.y / height as f64);
            }
            WindowEvent::CursorEntered { .. } => {}
            WindowEvent::CursorLeft { .. } => {}
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::MouseInput { button, state, .. } => {
                if button == MouseButton::Left && state == ElementState::Released {
                    screen_sys.on_mouse_click(&client, mouse_pos);
                }
            }
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::AxisMotion { .. } => {}
            WindowEvent::Touch(_) => {}
            WindowEvent::ScaleFactorChanged { scale_factor, inner_size_writer } => {
                if !state.resize(((state.size().1 as f64 * scale_factor) as u32, state.size().1)) {
                    panic!("Couldn't resize!");
                }
            }
            WindowEvent::ThemeChanged(_) => {}
            WindowEvent::Occluded(_) => {}
            _ => {}
        },
        Event::DeviceEvent { .. } => {},
        Event::UserEvent(_) => {},
        _ => {},
        /*Event::NewEvents(_) => {}
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => match event {
            WindowEvent::Resized(size) => {
                if !state.resize(*size) {
                    panic!("Couldn't resize!");
                } else {
                    renderer.dimensions.set(size.width, size.height);
                }
            }
            WindowEvent::Moved(_) => {}
            WindowEvent::CloseRequested => {
                *control_flow = ControlFlow::Exit;
            }
            WindowEvent::Destroyed => {}
            WindowEvent::DroppedFile(_) => {}
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::Focused(_) => {}
            WindowEvent::KeyboardInput { .. } => {}
            WindowEvent::ModifiersChanged(_) => {}
            WindowEvent::CursorMoved { position, .. } => {
                let (width, height) = renderer.dimensions.get();
                mouse_pos = (position.x / width as f64, 1.0 - position.y / height as f64);
            }
            WindowEvent::CursorEntered { .. } => {}
            WindowEvent::CursorLeft { .. } => {}
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::MouseInput { button, state, .. } => {
                if button == &MouseButton::Left && state == &ElementState::Released {
                    screen_sys.on_mouse_click(&client, mouse_pos);
                }
            }
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::AxisMotion { .. } => {}
            WindowEvent::Touch(_) => {}
            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                if !state.resize(**new_inner_size) {
                    panic!("Couldn't resize!");
                }
            }
            WindowEvent::ThemeChanged(_) => {}
            WindowEvent::Ime(_) => {}
            WindowEvent::Occluded(_) => {}
            WindowEvent::TouchpadMagnify { .. } => {}
            WindowEvent::SmartMagnify { .. } => {}
            WindowEvent::TouchpadRotate { .. } => {}
        },
        Event::DeviceEvent { .. } => {}
        Event::UserEvent(_) => {}
        Event::Suspended => {}
        Event::Resumed => {}
        Event::MainEventsCleared => {
            // RedrawRequested will only trigger once, unless we manually
            // request it.
            window.request_redraw();
        }
        Event::RedrawRequested(_) => {
            // FIXME: perform redraw
            let models = screen_sys.tick(0.0, &client, &window);
            renderer.render(models, atlas.clone());
        }
        Event::RedrawEventsCleared => {}
        Event::LoopDestroyed => {}
        _ => {}*/
    })?;
    Ok(())
}
