#![feature(new_uninit)]
#![feature(int_roundings)]
#![feature(pointer_is_aligned)]

extern crate core;

use crate::command::CommandProfiles;
use crate::config::{Config, DATA_DIR_PATH, CONFIG_FILE, data_path};
use crate::network::{AddressMode, NetworkClient};
use crate::packet::{Channel, ClientPacket};
use crate::profile::Profile;
use crate::protocol::{PROTOCOL_VERSION, RWBytes};
use crate::profile_db::{DbProfile, ProfileDb, uuid_from_pub_key};
use crate::utils::current_time_millis;
use bytes::{Bytes, BytesMut};
use clitty::core::{CLICore, CmdParamStrConstraints, CommandBuilder, CommandParam, CommandParamTy, UsageBuilder};
use clitty::ui::{CLIBuilder, CmdLineInterface, PrintFallback};
use quinn::ClientConfig;
use tokio::sync::RwLock;
use ui::UiQueue;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs, thread};
use std::fmt::{Debug, Display};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use colored::{ColoredString, Colorize};
use cpal::traits::{DeviceTrait, HostTrait};
use flume::{Receiver, Sender};
use crate::audio::{Audio, AudioConfig};
use crate::security_level::generate_token_num;
use crate::server::Server;
use swap_arc::{SwapArc, SwapArcOption};
use crate::ui::{InterUiMessage, UiImpl};

mod certificate;
mod config;
mod network;
mod packet;
mod profile;
mod protocol;
mod security_level;
mod profile_db;
mod utils;
mod server;
mod command;
mod audio;
pub mod data_structures;
mod ui;

// FIXME: review all the endianness related shit!

const RELATIVE_PROFILE_DB_PATH: &str = "user_db";
const VOICE_THRESHOLD: i16 = 50/*100*/; // 0-6 even occurs in idle (if nobody is near the input device)

const MIN_BUF_SIZE: usize = 480;

const UI: UiImpl = UiImpl::Wgpu;

// FIXME: can we even let tokio do this right here? do we have to run our event_loop on the main thread?
#[tokio::main] // FIXME: is it okay to use tokio in the main thread already, don't we need it to do rendering stuff?
async fn main() -> anyhow::Result<()> {
    /*let cfg = cpal::default_host().default_input_device().unwrap().default_input_config().unwrap().config();
    println!("{:?}", cfg);*/
    if let Some(cfgs) = cpal::default_host().default_input_device().map(|inner| inner.supported_input_configs().ok()).flatten() {
        for cfg in cfgs.into_iter() {
            println!("{:?}", cfg);
        }   
    }
    let (cfg, profile_db) = load_data()?;
    println!("loaded config!");
    let cfg = Arc::new(SwapArc::new(Arc::new(cfg)));
    let profile_db = Arc::new(profile_db);
    let cli = CLIBuilder::new()
        .prompt(format!("{}{}", "RustSpeak".green(), ": ".color(utils::LIGHT_GRAY_TERM)))
        .fallback(Box::new(PrintFallback::new(ColoredString::from("This command doesn't exist").red().to_string())))
        .command(CommandBuilder::new("profiles", CommandProfiles()).desc("manage profiles via command line")
        .params(UsageBuilder::new().required(CommandParam {
            name: "action",
            ty: CommandParamTy::String(CmdParamStrConstraints::Variants { variants: &["list", "create", "delete", "rename", "bump_sl"], ignore_case: true }),
        }).optional(CommandParam { // FIXME: add ability to make following arguments depend on the value of the previous argument (maybe by integrating the following arguments into the variants list)
            name: "name",
            ty: CommandParamTy::String(CmdParamStrConstraints::None),
        }))).build();
    let cli = Arc::new(CmdLineInterface::new(cli));
    let client = Arc::new(Client {config:cfg.clone(),profile_db:profile_db.clone(),cli,audio:SwapArcOption::new(AudioConfig::new()?.map(|cfg|Audio::from_cfg(&cfg).unwrap()).flatten().map(|audio|Arc::new(audio))),inter_ui_msg_queue:ui::ui_queue(UI), servers: RwLock::new(vec![]), voice_server: SwapArcOption::empty() });

    let tmp = client.clone();
    thread::spawn(move || {
        let client = tmp;
        loop {
            client.cli.await_input(&client).unwrap(); // FIXME: handle errors properly!
        }
    });
    // start the voice sending thread
    let tmp = client.clone();
    thread::spawn(move || {
        let client = tmp;
        loop {
            let client = client.clone();
            let server = client.voice_server.load();
            if let Some(server) = server.as_ref() { // FIXME: check for channel
                if !server.state.is_connected() {
                    sleep(Duration::from_millis(1));
                    continue;
                }
                    let client = client.clone();
                    // println!("in audio loop!");
                    let tmp_client = client.clone();
                    let has_err = Arc::new(AtomicBool::new(false));
                    let server = server.clone();
                    let has_err_rec = has_err.clone();
                    let glob_buf = Arc::new(Mutex::new(vec![]));
                    if let Some(audio) = client.audio.load().as_ref() {
                        let stream = audio.start_record(move |data, input| {
                            // println!("recorded!");
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
                                    let glob_buf = glob_buf.clone();
                                    let mut glob_buf = glob_buf.lock().unwrap();
                                    glob_buf.extend_from_slice(data);
                                    // server.audio.buffer.push(unsafe { &*slice_from_raw_parts(data as *const [i16] as *const i16 as *const u8, data.len() * 2) });
                                    if glob_buf.len() >= MIN_BUF_SIZE {
                                        let mut buffer = vec![0; 2048];
                                        // println!("max diff: {}", max_diff);
                                        let tmp = client.voice_server.load();
                                        let tmp_conn = tmp.as_ref().unwrap().connection.get();
                                        let data = server.audio.as_ref().unwrap().encode(&glob_buf.as_slice()[0..MIN_BUF_SIZE], &mut buffer);
                                        glob_buf.drain(0..MIN_BUF_SIZE);
                                        if let Err(err) = pollster::block_on(tmp_conn.unwrap().send_unreliable::<2>(Bytes::copy_from_slice(buffer.as_slice()).slice(0..data))) {
                                            pollster::block_on(server.error(err, &client));
                                            has_err.store(true, Ordering::Release);
                                            // stop recording!
                                            return;
                                        }
                                    }
                                }
                            }
                            // println!("send audio!");
                        }).unwrap();
                    }

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

    Ok(ui::start_ui(client, UI)?)
}

fn load_data() -> anyhow::Result<(Config, ProfileDb)> {
    let config = Config::load_or_create()?;
    let data_dir = data_path();
    let profile_db = ProfileDb::new(
        data_dir
            .join(RELATIVE_PROFILE_DB_PATH)
            .to_str()
            .unwrap()
            .to_string(),
        || {
            Ok(vec![DbProfile::new(String::from("default"), String::from("RustSpeakUser")).unwrap()])
        }
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
    pub config: Arc<SwapArc<Config>>,
    // FIXME: make this somehow mutable (maybe using an ArcSwap or a Mutex)
    pub profile_db: Arc<ProfileDb>,
    // pub renderer: Arc<Renderer>,
    // pub screen_sys: Arc<ScreenSystem>,
    // pub atlas: Arc<Atlas>,
    pub cli: Arc<CmdLineInterface<Arc<Client>>>,
    pub servers: RwLock<Vec<Arc<Server>>>,
    pub voice_server: SwapArcOption<Server>, // the currently active voice server
    pub audio: SwapArcOption<Audio>,
    pub inter_ui_msg_queue: Box<dyn UiQueue>,
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
