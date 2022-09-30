#![feature(new_uninit)]

extern crate core;

use crate::atlas::Atlas;
use crate::config::Config;
use crate::network::{AddressMode, NetworkClient};
use crate::packet::{Channel, ClientPacket, RemoteProfile};
use crate::profile::Profile;
use crate::protocol::{RWBytes, PROTOCOL_VERSION, UserUuid};
use crate::render::Renderer;
use crate::screen::server_list::ServerList;
use crate::screen_sys::ScreenSystem;
use crate::profile_db::{DbProfile, ProfileDb, uuid_from_pub_key};
use crate::utils::current_time_millis;
use arc_swap::{ArcSwap, ArcSwapOption};
use bytes::BytesMut;
use quinn::ClientConfig;
use ruint::aliases::U256;
use std::fs::File;
use std::mem::transmute;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, io, thread};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use colored::{ColoredString, Colorize};
use openssl::pkey::PKey;
use uuid::Uuid;
use wgpu::TextureFormat;
use wgpu_biolerless::{StateBuilder, WindowSize};
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::window::{Window, WindowBuilder};
use crate::command::cli::{CLIBuilder, CmdParamStrConstraints, CommandBuilder, CommandImpl, CommandLineInterface, CommandParam, CommandParamTy, UsageBuilder};
use crate::command::r#impl::CommandProfiles;
use crate::security_level::generate_token_num;
use crate::server::Server;

mod atlas;
mod certificate;
mod config;
mod network;
mod packet;
mod profile;
mod protocol;
mod render;
mod screen;
mod screen_sys;
mod security_level;
mod ui;
mod profile_db;
mod utils;
mod server;
mod command;

// FIXME: review all the endianness related shit!

const RELATIVE_PROFILE_DB_PATH: &str = "user_db";

// FIXME: can we even let tokio do this right here? do we have to run our event_loop on the main thread?
#[tokio::main] // FIXME: is it okay to use tokio in the main thread already, don't we need it to do rendering stuff?
async fn main() -> anyhow::Result<()> {
    let (cfg, profile_db) = load_data()?;
    let cfg = Arc::new(cfg);
    let profile_db = Arc::new(profile_db);
    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("RustSpeak")
        .build(&event_loop)
        .unwrap();
    let state = Arc::new(pollster::block_on(
        StateBuilder::new().window(&window).build(),
    )?);
    let atlas = Arc::new(Atlas::new(
        state.clone(),
        (1024, 1024),
        TextureFormat::Rgba8Uint,
    ));
    let renderer = Arc::new(Renderer::new(state.clone(), &window)?);
    let screen_sys = Arc::new(ScreenSystem::new());
    screen_sys.push_screen(Box::new(ServerList::new()));
    let cli = CLIBuilder::new()
        .prompt(ColoredString::from("RustSpeak").green())
        .help_msg(ColoredString::from("This command doesn't exist").red())
        .command(CommandBuilder::new().name("profiles").desc("manage profiles via command line")
        .params(UsageBuilder::new().required(CommandParam {
            name: "action",
            ty: CommandParamTy::String(CmdParamStrConstraints::Variants(&["list", "create", "delete", "rename", "bump_sl"])),
        }).optional(CommandParam { // FIXME: add ability to make following arguments depend on the value of the previous argument (maybe by integrating the following arguments into the variants list)
            name: "name",
            ty: CommandParamTy::String(CmdParamStrConstraints::None),
        })).cmd_impl(Box::new(CommandProfiles()))).build();
    let client = Arc::new(Client {
        config: cfg.clone(),
        profile_db: profile_db.clone(),
        renderer: renderer.clone(),
        screen_sys: screen_sys.clone(),
        atlas: atlas.clone(),
        cli,
        server: ArcSwapOption::empty(),
    });

    let tmp = client.clone();
    thread::spawn(move || {
        let client = tmp.clone();
        loop {
            client.cli.await_input(&client).unwrap(); // FIXME: handle errors properly!
        }
    });

    println!(
        "Client started up successfully, waiting for commands..."
    );
    // let client = start_connect_to();

    let mut mouse_pos = (0.0, 0.0);
    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(_) => {}
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => match event {
            WindowEvent::Resized(size) => {
                if !state.resize(*size) {
                    println!("Couldn't resize!");
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
            WindowEvent::ReceivedCharacter(_) => {}
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
                    client.screen_sys.on_mouse_click(&client, mouse_pos);
                }
            }
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
            let models = screen_sys.tick(0.0, &client, &window);
            renderer.render(models, atlas.clone());
        }
        Event::RedrawEventsCleared => {}
        Event::LoopDestroyed => {}
        _ => {}
    })
}

fn load_data() -> anyhow::Result<(Config, ProfileDb)> {
    let data_dir = dirs::config_dir().unwrap().join("RustSpeakClient/");
    fs::create_dir_all(data_dir.clone())?;
    let config = data_dir.join("config.json");
    let config = Config::load_or_create(config.clone())?; // FIXME: move this into some thread safe (semi) global that can be accessed by the UI code
    let profile_db = ProfileDb::new(
        data_dir
            .join(RELATIVE_PROFILE_DB_PATH)
            .to_str()
            .unwrap()
            .to_string(),
    )?;
    Ok((config, profile_db))
}

pub async fn start_connect_to(
    server_addr: SocketAddr,
    server_name: &str,
    profile: &Profile,
) -> anyhow::Result<NetworkClient> {
    let client = NetworkClient::new(
        AddressMode::V4,
        certificate::insecure_local::config(),
        server_addr,
        server_name,
    )
    .await
    .unwrap();
    let ctm = current_time_millis();
    let mut data = vec![];
    data.extend_from_slice(&ctm.as_secs().to_le_bytes());
    data.extend_from_slice(&ctm.subsec_nanos().to_le_bytes());
    let signed_data = profile.sign_data(&data)?;
    let auth_packet = ClientPacket::AuthRequest {
        protocol_version: PROTOCOL_VERSION,
        pub_key: profile.private_key().public_key_to_der()?,
        name: profile.name.clone(),
        security_proofs: vec![],
        signed_data,
    };
    // client.send_reliable((auth_packet as dyn RWBytes).encode());
    let mut buf = BytesMut::new();
    auth_packet.write(&mut buf)?;

    Ok(client)
}
pub struct Client<'a> {
    pub config: Arc<Config>,
    // FIXME: make this somehow mutable (maybe using an ArcSwap or a Mutex)
    pub profile_db: Arc<ProfileDb>,
    pub renderer: Arc<Renderer>,
    pub screen_sys: Arc<ScreenSystem>,
    pub atlas: Arc<Atlas>,
    pub cli: CommandLineInterface<'a>,
    pub server: ArcSwapOption<Server>, // FIXME: support multiple servers at once!
}

impl Client<'_> {

    pub fn handle_err(&self, err: anyhow::Error, err_src: ErrorSource) {

    }

}

#[derive(Copy, Clone, Debug)]
pub enum ErrorSource {
    UI,
    Network,
    Other,
    Unknown,
}
