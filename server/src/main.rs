#![feature(new_uninit)]
#![feature(int_roundings)]

use crate::channel_db::{ChannelDb, ChannelDbEntry};
use crate::cli::{
    CLIBuilder, CmdParamStrConstraints, CommandBuilder, CommandImpl, CommandLineInterface,
    CommandParam, CommandParamTy, UsageBuilder,
};
use crate::config::Config;
use crate::network::NetworkServer;
use crate::packet::{
    AuthFailure, AuthResponse, Channel, ChannelCreatePerms, ChannelPerms, ClientPacket,
    RemoteProfile, ServerGroup, ServerGroupPerms, ServerPacket,
};
use crate::protocol::{RWBytes, PROTOCOL_VERSION, UserUuid};
use crate::server_group_db::{ServerGroupDb, ServerGroupEntry};
use crate::utils::LIGHT_GRAY;
use arc_swap::ArcSwap;
use bytes::Buf;
use colored::{Color, ColoredString, Colorize};
use dashmap::DashMap;
use sled::Db;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::fs::File;
use std::net::IpAddr;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::{fs, thread};
use openssl::sha::sha256;
use ruint::aliases::U256;
use uuid::Uuid;
use crate::user_db::UserDb;

mod certificate;
mod channel_db;
mod cli;
mod config;
mod network;
mod packet;
mod protocol;
mod security_level;
mod server_group_db;
mod utils;
mod user_db;

const RELATIVE_USER_DB_PATH: &str = "user_db";
const RELATIVE_CHANNEL_DB_PATH: &str = "channel_db.json";
const RELATIVE_SERVER_GROUP_DB_PATH: &str = "server_group_db.json";
const ADMIN_GROUP_UUID: Uuid = Uuid::from_u128(0x1);
const DEFAULT_GROUP_UUID: Uuid = Uuid::from_u128(0x0);
const DEFAULT_CHANNEL_UUID: Uuid = Uuid::from_u128(0x0);

// FIXME: take a look at: https://www.nist.gov/news-events/news/2022/07/nist-announces-first-four-quantum-resistant-cryptographic-algorithms

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // FIXME: use logger!
    println!("Starting up server...");
    let data_dir = dirs::config_dir().unwrap().join("RustSpeakServer/");
    fs::create_dir_all(data_dir.clone())?;
    let user_db = UserDb::new(data_dir.clone().join(RELATIVE_USER_DB_PATH).to_str().unwrap().to_string())?;
    let channel_db = ChannelDb::new(
        data_dir
            .clone()
            .join(RELATIVE_CHANNEL_DB_PATH)
            .to_string_lossy()
            .to_string(),
    );
    let channels = channel_db
        .read_or_create(|| {
            Ok(vec![ChannelDbEntry {
                id: DEFAULT_CHANNEL_UUID.as_u128(),
                name: Cow::Borrowed("Lobby"),
                desc: Default::default(),
                password: false,
                user_groups: vec![],
                perms: ChannelPerms {
                    see: 0,
                    join: 0,
                    send: 0,
                    modify: 100,
                    talk: 0,
                    assign_talk: 100,
                    delete: 100,
                },
            }])
        })?
        .into_iter()
        .map(|entry| Channel {
            uuid: Uuid::from_u128(entry.id),
            password: AtomicBool::new(entry.password),
            name: Arc::new(RwLock::new(entry.name)),
            desc: Arc::new(RwLock::new(entry.desc)),
            perms: Arc::new(RwLock::new(entry.perms)),
            clients: Arc::new(Default::default()),
            proto_clients: Arc::new(Default::default()),
        })
        .collect::<Vec<_>>();
    let channels = {
        let mut result = HashMap::new();

        for channel in channels {
            result.insert(channel.uuid.clone(), channel);
        }

        result
    };
    let server_group_db = ServerGroupDb::new(
        data_dir
            .clone()
            .join(RELATIVE_SERVER_GROUP_DB_PATH)
            .to_string_lossy()
            .to_string(),
    );
    let server_groups = server_group_db.read_or_create(|| {
        Ok(vec![
            ServerGroupEntry {
                uuid: ADMIN_GROUP_UUID.as_u128(),
                name: Cow::Borrowed("admin"),
                perms: ServerGroupPerms {
                    server_group_assign: 0,
                    server_group_unassign: 0,
                    channel_see: 0,
                    channel_join: 0,
                    channel_send: 0,
                    channel_modify: 0,
                    channel_talk: 0,
                    channel_assign_talk: 0,
                    channel_delete: 0,
                    channel_kick: 0,
                    channel_create: ChannelCreatePerms {
                        power: 0,
                        set_desc: false,
                        set_password: false,
                    },
                },
            },
            ServerGroupEntry {
                uuid: DEFAULT_GROUP_UUID.as_u128(),
                name: Cow::Borrowed("default"),
                perms: ServerGroupPerms {
                    server_group_assign: 0,
                    server_group_unassign: 0,
                    channel_see: 0,
                    channel_join: 0,
                    channel_send: 0,
                    channel_modify: 0,
                    channel_talk: 0,
                    channel_assign_talk: 0,
                    channel_delete: 0,
                    channel_kick: 0,
                    channel_create: ChannelCreatePerms {
                        power: 0,
                        set_desc: false,
                        set_password: false,
                    },
                },
            },
        ])
    })?;
    let server_groups = server_groups
        .into_iter()
        .map(|server_group| ServerGroup {
            uuid: Uuid::from_u128(server_group.uuid),
            name: Cow::Owned(server_group.name.to_string()),
            priority: 0,
            perms: ServerGroupPerms {
                server_group_assign: 0,
                server_group_unassign: 0,
                channel_see: 0,
                channel_join: 0,
                channel_send: 0,
                channel_modify: 0,
                channel_talk: 0,
                channel_assign_talk: 0,
                channel_delete: 0,
                channel_kick: 0,
                channel_create: ChannelCreatePerms {
                    power: 0,
                    set_desc: false,
                    set_password: false,
                },
            },
        })
        .collect::<Vec<_>>();
    let server_groups = {
        let mut result = HashMap::new();

        for server_group in server_groups.into_iter() {
            result.insert(server_group.uuid.clone(), Arc::new(server_group));
        }

        result
    };
    let config = Config::load_or_create(data_dir.join("config.json"))?;
    let network_server = setup_network_server(&config)?;

    let cli = CLIBuilder::new()
        .prompt(ColoredString::from("RustSpeak").red())
        .command(
            CommandBuilder::new()
                .name("help")
                .desc("returns a list of available commands")
                .add_aliases(&["?", "h"])
                .cmd_impl(Box::new(CommandHelp())),
        )
        .command(
            CommandBuilder::new()
                .name("stop")
                .desc("shuts down the server gracefully")
                .add_aliases(&["shutdown", "end", "kill", "off"])
                .cmd_impl(Box::new(CommandShutdown())),
        )
        .command(
            CommandBuilder::new()
                .name("user")
                .params(UsageBuilder::new().required(CommandParam {
                    name: "user",
                    ty: CommandParamTy::String(CmdParamStrConstraints::None),
                }))
                .cmd_impl(Box::new(CommandUser())),
        );

    let server = Arc::new(Server {
        server_groups: ArcSwap::new(Arc::new(server_groups)), // FIXME: load groups from database
        channels: ArcSwap::new(Arc::new(channels)),
        online_users: Default::default(),
        network_server,
        config,
        user_db,
        channel_db,
        server_group_db,
        cli: cli.build(),
    });
    let tmp = server.clone();
    thread::spawn(move || {
        let server = tmp.clone();
        loop {
            server.clone().cli.await_input().unwrap(); // FIXME: handle errors properly!
        }
    });

    println!("Server started up successfully, waiting for inbound connections on port {}...", server.config.port);
    start_server(server, |err| {
        println!(
            "An error occurred while establishing a client connection: {}",
            err
        );
    })
    .await;
    Ok(())
}

fn setup_network_server(config: &Config) -> anyhow::Result<NetworkServer> {
    let (local_cert, private_key) = certificate::insecure_local::generate_self_signed_cert()?;
    NetworkServer::new(
        config.port,
        /*1000*/u32::MAX,
        config.address_mode,
        certificate::create_config(local_cert, private_key)?,
    )
}

async fn start_server<F: Fn(anyhow::Error)>(server: Arc<Server<'_>>, error_handler: F) {
    let tmp_srv = server.clone();
    server
        .network_server
        .accept_connections(
            move |new_conn| {
                let new_conn = new_conn.clone();
                let server = tmp_srv.clone();
                async move {
                    println!("initial connection attempt!");
                    let new_conn = new_conn.clone();
                    // FIXME: use more sophisticated packet header like the one commented out below!
                    // let mut header = new_conn.read_reliable(2).await?;
                    // let size = header.get_u16_le();
                    // let id = header.get_u8(); // FIXME: try to somehow get this data here already
                    // let mut data = new_conn.read_reliable(size as usize).await?;
                    let size = new_conn.read_reliable(8).await?.get_u64_le();
                    println!("got size {}", size);
                    let mut data = new_conn.read_reliable(size as usize).await?;
                    println!("read data!");
                    let packet = ClientPacket::read(&mut data, None)?;
                    println!("read packet!");
                    let server = server.clone();
                    if let ClientPacket::AuthRequest {
                        protocol_version,
                        pub_key,
                        name,
                        security_proofs,
                        signed_data,
                    } = packet
                    {
                        println!("{} tried to connect!", name);
                        if protocol_version != PROTOCOL_VERSION {
                            let failure = ServerPacket::AuthResponse(AuthResponse::Failure(
                                AuthFailure::OutOfDate(PROTOCOL_VERSION),
                            ));
                            let encoded = failure.encode()?;
                            new_conn.send_reliable(&encoded).await?;
                            new_conn.close().await?;
                            return Err(anyhow::Error::from(ErrorAuthProtoVer {
                                ip: new_conn
                                    .conn
                                    .read()
                                    .unwrap()
                                    .connection
                                    .remote_address()
                                    .ip(),
                                uuid: UserUuid::from_u256(U256::from_le_bytes(sha256(&signed_data))),
                                recv_proto_ver: protocol_version,
                            }));
                        }
                        let uuid = UserUuid::from_u256(U256::from_le_bytes(sha256(&pub_key)));
                        let security_proof_result = if let Some(level) =
                            security_level::verified_security_level(uuid.as_u256(), security_proofs)
                        {
                            level
                        } else {
                            let failure = ServerPacket::AuthResponse(AuthResponse::Failure(
                                AuthFailure::Invalid(Cow::from("Invalid security proofs!")),
                            ));
                            let encoded = failure.encode()?;
                            new_conn.send_reliable(&encoded).await?;
                            new_conn.close().await?;
                            return Err(anyhow::Error::from(ErrorAuthSecProof {
                                ip: new_conn
                                    .conn
                                    .read()
                                    .unwrap()
                                    .connection
                                    .remote_address()
                                    .ip(),
                                uuid,
                            }));
                        };
                        // FIXME: compare auth_id with the auth_id in our data base if this isn't the first login!
                        // FIXME: insert data send the proper data back!
                        let channels = server.channels.load();
                        let channels = channels.values();
                        let server_groups = server.server_groups.load();
                        let server_groups = server_groups.values();
                        let channels = channels.cloned().collect::<Vec<_>>();
                        let auth = ServerPacket::AuthResponse(AuthResponse::Success {
                            server_groups: server_groups.cloned().collect::<Vec<_>>(), // FIXME: try getting rid of this clone!
                            own_groups: vec![],
                            // channels: RefCell::new(Box::new(channels)),
                            channels,
                        });
                        let encoded = auth.encode()?;
                        new_conn.send_reliable(&encoded).await?;
                    } else {
                        let failure = ServerPacket::AuthResponse(AuthResponse::Failure(
                            AuthFailure::Invalid(Cow::from(
                                "The first packet sent has to be a `AuthRequest` packet!",
                            )),
                        ));
                        let encoded = failure.encode()?;
                        new_conn.send_reliable(&encoded).await?;
                        new_conn.close().await?;
                    }

                    Ok(())
                }
            },
            error_handler,
        )
        .await;
}

pub struct Server<'a> {
    // pub server_groups: DashMap<Uuid, ServerGroup>,
    pub server_groups: ArcSwap<HashMap<Uuid, Arc<ServerGroup<'a>>>>,
    pub channels: ArcSwap<HashMap<Uuid, Channel<'a>>>,
    pub online_users: DashMap<Uuid, User<'a>>, // FIXME: add a timed cache for offline users
    pub network_server: NetworkServer,
    pub config: Config, // FIXME: make this mutable somehow
    pub user_db: UserDb,
    pub channel_db: ChannelDb,
    pub server_group_db: ServerGroupDb,
    pub cli: CommandLineInterface<'a>,
}

pub struct User<'a> {
    pub uuid: Uuid,
    pub name: RwLock<Cow<'a, str>>,
    pub last_security_proof: u128,
    pub last_verified_security_level: u8,
    pub groups: RwLock<Vec<Uuid>>,
    // FIXME: add individual user perms
}

struct ErrorAuthProtoVer {
    ip: IpAddr,
    uuid: UserUuid,
    recv_proto_ver: u64,
}

impl Debug for ErrorAuthProtoVer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with incompatible protocol version ")?;
        f.write_str(self.recv_proto_ver.to_string().as_str())
    }
}

impl Display for ErrorAuthProtoVer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with incompatible protocol version ")?;
        f.write_str(self.recv_proto_ver.to_string().as_str())
    }
}

impl Error for ErrorAuthProtoVer {}

struct ErrorAuthSecProof {
    ip: IpAddr,
    uuid: UserUuid,
}

impl Debug for ErrorAuthSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with invalid security proofs")
    }
}

impl Display for ErrorAuthSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with invalid security proofs")
    }
}

impl Error for ErrorAuthSecProof {}

struct CommandHelp();

impl CommandImpl for CommandHelp {
    fn execute(&self, cli: &CommandLineInterface, _input: &[&str]) -> anyhow::Result<()> {
        let cmds = cli.cmds();
        println!("Commands ({}):", cmds.len());
        for cmd in cmds {
            let usage = if let Some(usage) = cmd.1.params() {
                let mut ret_usage = String::new();
                for param in usage.required() {
                    ret_usage.push(' ');
                    ret_usage.push('[');
                    ret_usage.push_str(param.name);
                    let mut ty = String::new();
                    ty.push('(');
                    ty.push_str(param.ty.as_str());
                    ty.push(')');
                    let ty = ColoredString::from(ty.as_str()).italic().color(LIGHT_GRAY);
                    ret_usage.push_str(&*format!("{ty}"));
                    ret_usage.push(']');
                }
                ret_usage
            } else {
                String::new()
            };
            if let Some(desc) = cmd.1.desc() {
                println!("{}{}: {}", cmd.1.name(), usage, desc);
            } else {
                println!("{}{}", cmd.1.name(), usage);
            }
        }

        Ok(())
    }
}

struct CommandShutdown();

impl CommandImpl for CommandShutdown {
    fn execute(&self, cli: &CommandLineInterface, input: &[&str]) -> anyhow::Result<()> {
        todo!()
    }
}

struct CommandUser();

impl CommandImpl for CommandUser {
    fn execute(&self, cli: &CommandLineInterface, input: &[&str]) -> anyhow::Result<()> {
        todo!()
    }
}
