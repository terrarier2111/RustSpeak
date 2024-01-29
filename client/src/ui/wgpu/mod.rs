use std::ops::Deref;
use std::sync::Arc;
use wgpu::TextureFormat;
use wgpu_biolerless::StateBuilder;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use winit::window::{Window, WindowBuilder};
use crate::Client;
use crate::data_structures::conc_once_cell::ConcurrentOnceCell;
use crate::ui::wgpu::atlas::Atlas;
use crate::ui::wgpu::render::Renderer;
use crate::ui::wgpu::screen::menu_screen::Menu;
use crate::ui::wgpu::screen::server_list::ServerList;
use crate::ui::wgpu::screen_sys::ScreenSystem;

use self::screen::error_screen::ErrorScreen;

use super::{InterUiMessage, UiQueue, UiQueueSender};

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
    pub(crate) queue: EventLoopProxy<InterUiMessage>,
}

static UI_CTX: ConcurrentOnceCell<Arc<UiCtx>> = ConcurrentOnceCell::new();

pub(crate) fn ctx() -> &'static Arc<UiCtx> {
    UI_CTX.get().unwrap()
}

pub fn queue() -> Box<dyn UiQueue> {
    Box::new(|msg| {
        ctx().queue.send_event(msg).unwrap();
    })
}

pub fn run(client: Arc<Client>) -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
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
    screen_sys.push_screen(Box::new(Menu::new()));
    // screen_sys.push_screen(Box::new(ServerList::new()));

    UI_CTX.try_init_silent(Arc::new(UiCtx {
        screen_sys: screen_sys.clone(),
        renderer: renderer.clone(),
        atlas: atlas.clone(),
        window: window.clone(),
        queue: proxy,
    })).unwrap();

    let mut mouse_pos = (0.0, 0.0);
    // FIXME: redraw once we got a notification from the (flume) channel
    event_loop.run(move |event, control_flow| {
        let redraw = || {
            let models = screen_sys.tick(&client, &window);
            renderer.render(models, atlas.clone());
        };
        match event {
        Event::WindowEvent { window_id, event } if window_id == window.id() => match event {
            WindowEvent::Resized(size) => {
                if !state.resize(size) {
                    panic!("Couldn't resize!");
                } else {
                    renderer.dimensions.set(size.width, size.height);
                }
                renderer.rescale_glyphs();
                redraw();
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
                redraw();
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
                    redraw();
                }
            }
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::AxisMotion { .. } => {}
            WindowEvent::Touch(_) => {}
            WindowEvent::ScaleFactorChanged { scale_factor, inner_size_writer } => {
                if !state.resize(((state.size().1 as f64 * scale_factor) as u32, state.size().1)) {
                    panic!("Couldn't resize!");
                }
                renderer.rescale_glyphs();
                redraw();
            }
            WindowEvent::ThemeChanged(_) => {}
            WindowEvent::Occluded(_) => {}
            WindowEvent::RedrawRequested => {
                // perform redraw
                redraw();
            }
            _ => {}
        },
        Event::DeviceEvent { .. } => {},
        Event::UserEvent(event) => {
            match event {
                InterUiMessage::ChannelRemoveUser(_, _) => todo!(),
                InterUiMessage::ChannelAddUser(_, _) => todo!(),
                InterUiMessage::UpdateProfiles => todo!(),
                InterUiMessage::Error(error) => {
                    screen_sys.push_screen(Box::new(ErrorScreen::new(&client, error)));
                    redraw();
                },
                InterUiMessage::ServerConnected => todo!(),
            }
        },
        _ => {},
    }
})?;
    Ok(())
}
