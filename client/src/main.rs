use std::fs::File;
use std::net::SocketAddr;
use std::{io, thread};
use bytes::BytesMut;
use wgpu_biolerless::StateBuilder;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::window::WindowBuilder;
use crate::config::Config;
use crate::network::{AddressMode, NetworkClient};
use crate::packet::{ClientPacket, PROTOCOL_VERSION, RemoteProfile, RWBytes};
use crate::profile::Profile;

mod certificate;
mod config;
mod network;
mod packet;
mod profile;
mod render;
mod screen_sys;
mod security_level;
mod protocol;

// FIXME: can we even let tokio do this right here? do we have to run our event_loop on the main thread?

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("RustSpeak")
        .build(&event_loop)
        .unwrap();
    let state = pollster::block_on(StateBuilder::new().window(&window).build())?;

    let _ = thread::spawn(|| {
        start_app_backend();
    });

    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(_) => {}
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => match event {
            WindowEvent::Resized(size) => {
                if !state.resize(*size) {
                    println!("Couldn't resize!");
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
            WindowEvent::ReceivedCharacter(_) => {}
            WindowEvent::Focused(_) => {}
            WindowEvent::KeyboardInput { .. } => {}
            WindowEvent::ModifiersChanged(_) => {}
            WindowEvent::CursorMoved { .. } => {}
            WindowEvent::CursorEntered { .. } => {}
            WindowEvent::CursorLeft { .. } => {}
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::MouseInput { .. } => {}
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::AxisMotion { .. } => {}
            WindowEvent::Touch(_) => {}
            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                if !state.resize(**new_inner_size) {
                    println!("Couldn't resize!");
                }
            }
            WindowEvent::ThemeChanged(_) => {}
            WindowEvent::Ime(_) => {}
            WindowEvent::Occluded(_) => {}
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
        }
        Event::RedrawEventsCleared => {}
        Event::LoopDestroyed => {}
        _ => {}
    })
}

#[tokio::main]
async fn start_app_backend() {
    let config = dirs::config_dir().unwrap().join("RustSpeakClient/config.json");
    let config = Config::load_or_create(config).unwrap(); // FIXME: move this into some thread safe (semi) global that can be accessed by the UI code

}

pub async fn start_connect_to(server_addr: SocketAddr, server_name: &str, profile: &Profile) -> anyhow::Result<NetworkClient> {
    let client = NetworkClient::new(AddressMode::V4, None, server_addr, server_name).await.unwrap();
    let auth_packet = ClientPacket::AuthRequest {
        protocol_version: PROTOCOL_VERSION,
        name: profile.name.clone(),
        uuid: profile.uuid().clone(),
        security_proofs: vec![],
        auth_id: Default::default()
    };
    // client.send_reliable((auth_packet as dyn RWBytes).encode());
    let mut buf = BytesMut::new();
    auth_packet.write(&mut buf)?;

    Ok(client)
}
