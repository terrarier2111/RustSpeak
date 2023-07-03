#![feature(new_uninit)]
#![feature(int_roundings)]
#![feature(pointer_is_aligned)]

extern crate core;

use crate::atlas::Atlas;
use crate::config::Config;
use crate::network::{AddressMode, NetworkClient};
use crate::packet::{Channel, ClientPacket};
use crate::profile::Profile;
use crate::protocol::{PROTOCOL_VERSION, RWBytes};
use crate::render::Renderer;
use crate::screen::server_list::ServerList;
use crate::screen_sys::ScreenSystem;
use crate::profile_db::{DbProfile, ProfileDb, uuid_from_pub_key};
use crate::utils::current_time_millis;
use bytes::{Bytes, BytesMut};
use quinn::ClientConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::{fs, thread};
use std::error::Error;
use std::fmt::{Debug, Display};
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use colored::{ColoredString, Colorize};
use cpal::traits::{DeviceTrait, HostTrait};
use wgpu::TextureFormat;
use wgpu_biolerless::{StateBuilder, WindowSize};
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::window::WindowBuilder;
use crate::audio::{Audio, AudioConfig};
use crate::command::cli::{CLIBuilder, CmdParamStrConstraints, CommandBuilder, CommandImpl, CommandLineInterface, CommandParam, CommandParamTy, UsageBuilder};
use crate::command::r#impl::CommandProfiles;
use crate::security_level::generate_token_num;
use crate::server::Server;
use pollster::FutureExt;
use swap_arc::{SwapArc, SwapArcOption};

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
mod audio;
pub mod data_structures;

// FIXME: review all the endianness related shit!

const RELATIVE_PROFILE_DB_PATH: &str = "user_db";
const VOICE_THRESHOLD: i16 = 50/*100*/; // 0-6 even occurs in idle (if nobody is near the input device)

// FIXME: can we even let tokio do this right here? do we have to run our event_loop on the main thread?
#[tokio::main] // FIXME: is it okay to use tokio in the main thread already, don't we need it to do rendering stuff?
async fn main() -> anyhow::Result<()> {
    /*let cfg = cpal::default_host().default_input_device().unwrap().default_input_config().unwrap().config();
    println!("{:?}", cfg);*/
    for cfg in cpal::default_host().default_input_device().unwrap().supported_input_configs().unwrap().into_iter() {
        println!("{:?}", cfg);
    }
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
            name: "action".to_string(),
            ty: CommandParamTy::String(CmdParamStrConstraints::Variants(Box::new(["list".to_string(), "create".to_string(), "delete".to_string(), "rename".to_string(), "bump_sl".to_string()]))),
        }).optional(CommandParam { // FIXME: add ability to make following arguments depend on the value of the previous argument (maybe by integrating the following arguments into the variants list)
            name: "name".to_string(),
            ty: CommandParamTy::String(CmdParamStrConstraints::None),
        })).cmd_impl(Box::new(CommandProfiles()))).build();
    let client = Arc::new(Client {
        config: cfg.clone(),
        profile_db: profile_db.clone(),
        renderer: renderer.clone(),
        screen_sys: screen_sys.clone(),
        atlas: atlas.clone(),
        cli,
        server: SwapArcOption::empty(),
        audio: SwapArc::new(Arc::new(Audio::from_cfg(&AudioConfig::new()?.unwrap())?.unwrap())),
    });

    let tmp = client.clone();
    thread::spawn(move || {
        let client = tmp;
        loop {
            client.cli.await_input(&client).unwrap(); // FIXME: handle errors properly!
        }
    });
    let tmp = client.clone();
    thread::spawn(move || {
        let client = tmp;
        loop {
            let client = client.clone();
            let server = client.server.load();
            if let Some(server) = server.as_ref() { // FIXME: check for channel
                if !server.state.is_connected() {
                    sleep(Duration::from_millis(1));
                    continue;
                }
                    let client = client.clone();
                    println!("in audio loop!");
                    let tmp_client = client.clone();
                    let has_err = Arc::new(AtomicBool::new(false));
                    let server = server.clone();
                    let has_err_rec = has_err.clone();
                    let stream = client.audio.load().start_record(move |data, input| {
                        println!("recorded!");
                        let has_err = &has_err_rec;
                        // FIXME: handle endianness of `data`
                        // println!("sending audio {}", data.len());
                        let client = &tmp_client;
                        let empty = data.iter().all(|x| *x == 0);
                        if !empty {
                            let max = *data.iter().max().unwrap();
                            let min = *data.iter().min().unwrap();
                            let max_diff = max - min;
                            if max_diff > VOICE_THRESHOLD {
                                println!("max diff: {}", max_diff);
                                let data = Bytes::copy_from_slice(bytemuck::cast_slice(data));
                                let tmp = client.server.load();
                                let tmp_conn = tmp.as_ref().unwrap().connection.get();
                                if let Err(err) = pollster::block_on(tmp_conn.unwrap().send_unreliable::<2>(data)) {
                                    pollster::block_on(server.error(err, &client));
                                    has_err.store(true, Ordering::Release);
                                    // stop recording!
                                    return;
                                }
                            }
                        }
                        // println!("send audio!");
                    }).unwrap();

                    while !has_err.load(Ordering::Acquire) {
                        sleep(Duration::from_millis(1));
                    }
            } else {
                sleep(Duration::from_millis(1));
            }
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
    let mut buf = BytesMut::new();
    auth_packet.write(&mut buf)?;

    Ok(client)
}
pub struct Client {
    pub config: Arc<Config>,
    // FIXME: make this somehow mutable (maybe using an ArcSwap or a Mutex)
    pub profile_db: Arc<ProfileDb>,
    pub renderer: Arc<Renderer>,
    pub screen_sys: Arc<ScreenSystem>,
    pub atlas: Arc<Atlas>,
    pub cli: CommandLineInterface,
    pub server: SwapArcOption<Server>, // FIXME: support multiple servers at once!
    pub audio: SwapArc<Audio>,
}

impl Client {

    pub fn handle_err(&self, err: anyhow::Error, err_src: ErrorSource) {

    }

    pub fn println(&self, msg: &str) {
        self.cli.println(msg);
    }

}

#[derive(Copy, Clone, Debug)]
pub enum ErrorSource {
    UI,
    Network,
    Other,
    Unknown,
}
