#![feature(new_uninit)]
#![feature(int_roundings)]
#![feature(once_cell)]
#![feature(strict_provenance)]
#![feature(adt_const_params)]
#![feature(arbitrary_self_types)]
#![feature(const_option)]
#![feature(lazy_cell)]

use crate::channel_db::{ChannelDb, ChannelDbEntry};
use crate::cli::{
    CLIBuilder, CmdParamStrConstraints, CommandBuilder, CommandImpl, CommandLineInterface,
    CommandParam, CommandParamTy, UsageBuilder,
};
use crate::config::Config;
use crate::network::{ClientConnection, handle_packet, NetworkServer};
use crate::packet::{
    AuthFailure, AuthResponse, Channel, ChannelCreatePerms, ChannelPerms, ClientPacket,
    RemoteProfile, ServerGroup, ServerGroupPerms, ServerPacket,
};
use crate::protocol::{RWBytes, UserUuid, PROTOCOL_VERSION};
use crate::server_group_db::{ServerGroupDb, ServerGroupEntry};
use crate::user_db::{DbUser, UserDb};
use crate::utils::LIGHT_GRAY;
use bytes::Buf;
use colored::{Color, ColoredString, Colorize};
use dashmap::DashMap;
use openssl::sha::sha256;
use ruint::aliases::U256;
use sled::Db;
use std::borrow::Cow;
use std::cell::{LazyCell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::fs::File;
use std::net::IpAddr;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::{fs, thread};
use std::future::Future;
use std::task::{Context, Poll};
use std::time::Duration;
use crossbeam_utils::Backoff;
use futures::{FutureExt, StreamExt};
use futures::task::noop_waker_ref;
use swap_arc::{SwapArc, SwapArcOption};
use tokio::{join, select};
use uuid::Uuid;
use crate::conc_once_cell::ConcurrentOnceCell;

mod certificate;
mod channel_db;
mod cli;
mod config;
mod network;
mod packet;
mod protocol;
mod security_level;
mod server_group_db;
mod user_db;
mod utils;
mod conc_once_cell;
mod sized_box;
mod conc_vec;

// FIXME: review all the endianness related shit!

const RELATIVE_USER_DB_PATH: &str = "user_db";
const RELATIVE_CHANNEL_DB_PATH: &str = "channel_db.json";
const RELATIVE_SERVER_GROUP_DB_PATH: &str = "server_group_db.json";
const ADMIN_GROUP_UUID: Uuid = Uuid::from_u128(0x1);
const DEFAULT_GROUP_UUID: Uuid = Uuid::from_u128(0x0);
const DEFAULT_CHANNEL_UUID: Uuid = Uuid::from_u128(0x0);

// FIXME: take a look at: https://www.nist.gov/news-events/news/2022/07/nist-announces-first-four-quantum-resistant-cryptographic-algorithms

fn main() -> anyhow::Result<()> {
    // FIXME: use logger!
    println!("Starting up server...");
    let data_dir = dirs::config_dir().unwrap().join("RustSpeakServer/");
    fs::create_dir_all(data_dir.clone())?;
    let user_db = UserDb::new(
        data_dir
            .clone()
            .join(RELATIVE_USER_DB_PATH)
            .to_str()
            .unwrap()
            .to_string(),
    )?;
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
            name: Arc::new(SwapArc::new(Arc::new(entry.name.to_string()))),
            desc: Arc::new(SwapArc::new(Arc::new(entry.desc.to_string()))),
            perms: Arc::new(SwapArc::new(Arc::new(entry.perms))),
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
            name: server_group.name.to_string(),
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

    let cli = CLIBuilder::new()
        .prompt(ColoredString::from("RustSpeak").red())
        .help_msg(ColoredString::from("This command doesn't exist, try using help to get a full list of all available commands").red()) // FIXME: color "help" in yellow
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
                    name: "user".to_string(),
                    ty: CommandParamTy::String(CmdParamStrConstraints::None),
                }).optional(CommandParam {
                    name: "action".to_string(),
                    ty: CommandParamTy::String(CmdParamStrConstraints::Variants(&["delete", "group", "perms"])),
                }))
                .cmd_impl(Box::new(CommandUser())),
        )
        .command(
            CommandBuilder::new()
                .name("onlineusers")
                .cmd_impl(Box::new(CommandOnlineUsers())),
        );

    let main_server = Arc::new(ConcurrentOnceCell::new());

    let main_server_ref = main_server.clone();
    thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let network_server = setup_network_server(&config).unwrap(); // FIXME: handle panics by gracefully shutting down using a panic hook!
                let server = Arc::new(Server {
                    server_groups: tokio::sync::RwLock::new(server_groups),
                    channels: tokio::sync::RwLock::new(channels),
                    online_users: Default::default(),
                    network_server,
                    config,
                    user_db,
                    channel_db,
                    server_group_db,
                    cli: cli.build(),
                    shutting_down: Default::default(),
                    shut_down: Default::default(),
                });
                main_server_ref.try_init(server.clone()).expect("server already init, this can't happen!");
                let tmp = server.clone();
                thread::spawn(move || {
                    let server = tmp.clone();
                    loop {
                        server.cli.await_input(&server).unwrap(); // FIXME: handle errors properly!
                    }
                });
                start_server(server.clone(), |err| {
                    server.cli.println(
                        format!("An error occurred while establishing a client connection: {}", err).as_str());
                }).await;
            });
    });

    let backoff = Backoff::new();
    let server = loop {
        if let Some(server) = main_server.get() {
            break server.clone();
        }
        backoff.snooze();
    };

    println!(
        "Server started up successfully, waiting for inbound connections on port {}...",
        server.config.port
    );

    while !server.shut_down.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(250));
    }

    Ok(())
}

fn setup_network_server(config: &Config) -> anyhow::Result<NetworkServer> {
    let (local_cert, private_key) = certificate::insecure_local::generate_self_signed_cert()?;
    NetworkServer::new(
        config.port,
        /*1000*/ u32::MAX,
        config.address_mode,
        certificate::create_config(local_cert, private_key)?,
    )
}

async fn start_server<F: Fn(anyhow::Error)>(server: Arc<Server>, error_handler: F) {
    let tmp_srv = server.clone();
    server
        .network_server
        .accept_connections(
            move |new_conn| {
                let new_conn = new_conn.clone();
                let server = tmp_srv.clone();
                async move {
                    let new_conn = new_conn.clone();
                    // FIXME: use more sophisticated packet header like the one commented out below!
                    // let mut header = new_conn.read_reliable(2).await?;
                    // let size = header.get_u16_le();
                    // let id = header.get_u8(); // FIXME: try to somehow get this data here already
                    // let mut data = new_conn.read_reliable(size as usize).await?;
                    let size = new_conn.read_reliable(8).await?.get_u64_le();
                    println!("got size {}", size);
                    let mut data = new_conn.read_reliable(size as usize).await?;
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
                        server.println(format!("{} tried to connect!", name).as_str());
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
                                    .await
                                    .remote_address()
                                    .ip(),
                                uuid: UserUuid::from_u256(U256::from_le_bytes(sha256(
                                    &signed_data,
                                ))),
                                recv_proto_ver: protocol_version,
                            }));
                        }
                        let uuid = UserUuid::from_u256(U256::from_le_bytes(sha256(&pub_key)));
                        let last_security_proof = security_proofs.last().copied();
                        let security_proof_result = if let Some(level) =
                        security_level::verified_security_level(
                            uuid.into_u256(),
                            security_proofs,
                        ) {
                            level
                        } else {
                            let failure = ServerPacket::AuthResponse(AuthResponse::Failure(
                                AuthFailure::Invalid(Cow::from("Invalid security proofs!")),
                            ));
                            let encoded = failure.encode()?;
                            new_conn.send_reliable(&encoded).await?;
                            new_conn.close().await?;
                            return Err(anyhow::Error::from(ErrorAuthInvSecProof {
                                ip: new_conn
                                    .conn
                                    .read()
                                    .await
                                    .remote_address()
                                    .ip(),
                                uuid,
                            }));
                        };
                        if server.config.req_security_level > security_proof_result {
                            let failure = ServerPacket::AuthResponse(AuthResponse::Failure(
                                AuthFailure::ReqSec(server.config.req_security_level),
                            ));
                            let encoded = failure.encode()?;
                            new_conn.send_reliable(&encoded).await?;
                            new_conn.close().await?;
                            return Err(anyhow::Error::from(ErrorAuthLowSecProof {
                                ip: new_conn
                                    .conn
                                    .read()
                                    .await
                                    .remote_address()
                                    .ip(),
                                uuid,
                                provided_lvl: security_proof_result,
                            }));
                        }
                        if server.online_users.contains_key(&uuid) {
                            let failure = ServerPacket::AuthResponse(AuthResponse::Failure(
                                AuthFailure::AlreadyOnline,
                            ));
                            let encoded = failure.encode()?;
                            new_conn.send_reliable(&encoded).await?;
                            new_conn.close().await?;
                            return Err(anyhow::Error::from(ErrorAlreadyOnline {
                                ip: new_conn
                                    .conn
                                    .read()
                                    .await
                                    .remote_address()
                                    .ip(),
                                uuid,
                            }));
                        }
                        // FIXME: compare auth_id with the auth_id in our data base if this isn't the first login!
                        // FIXME: insert data send the proper data back!
                        new_conn.uuid.store(Some(Arc::new(uuid)));
                        server.println(format!("{} ({:?}) successfully connected", name, uuid).as_str());
                        let channels = server.channels.read().await;
                        let channels = channels.values();
                        let server_groups = server.server_groups.read().await;
                        let server_groups = server_groups.values();
                        let channels = channels.cloned().collect::<Vec<_>>();
                        let user = if let Some(user) = server.user_db.get(&uuid)? {
                            user
                        } else {
                            let user = DbUser {
                                uuid,
                                name,
                                last_security_proof: last_security_proof.unwrap(),
                                last_verified_security_level: security_proof_result,
                                groups: vec![],
                            };
                            server.user_db.insert(user.clone())?;
                            user
                        };
                        server.println(format!("uuid_cmp: {}", uuid == user.uuid).as_str());
                        server.println(format!("uuid: {:?}", uuid).as_str());
                        server.println(format!("user uuid: {:?}", user.uuid).as_str());
                        server.online_users.insert(uuid, User {
                            uuid,
                            name: SwapArc::new(Arc::new(user.name.into())),
                            last_security_proof: user.last_security_proof,
                            last_verified_security_level: user.last_verified_security_level,
                            groups: RwLock::new(user.groups.clone()),
                            connection: new_conn.clone(),
                        });
                        server.channels.read().await.get(&new_conn.channel).unwrap().clients.write().await.push(uuid); // FIXME: remove the user from the channel again later on!
                        let auth = ServerPacket::AuthResponse(AuthResponse::Success {
                            default_channel_id: Uuid::from_u128(server.config.default_channel_id),
                            server_groups: server_groups.cloned().collect::<Vec<_>>(), // FIXME: try getting rid of this clone!
                            own_groups: user.groups,
                            channels,
                        });
                        let encoded = auth.encode()?;
                        new_conn.send_reliable(&encoded).await?;
                        let keep_alive_stream = new_conn.conn.write().await.accept_bi().await?;
                        let _ = new_conn.keep_alive_stream.try_init((tokio::sync::Mutex::new(keep_alive_stream.0), tokio::sync::Mutex::new(keep_alive_stream.1)));
                        new_conn.start_read().await;
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
            error_handler, server.clone(),
        ).await // FIXME: is this okay perf-wise?
}

pub struct Server {
    pub server_groups: tokio::sync::RwLock<HashMap<Uuid, Arc<ServerGroup>>>,
    pub channels: tokio::sync::RwLock<HashMap<Uuid, Channel>>,
    pub online_users: DashMap<UserUuid, User>, // FIXME: add a timed cache for offline users
    pub network_server: NetworkServer,
    pub config: Config, // FIXME: make this mutable somehow
    pub user_db: UserDb,
    pub channel_db: ChannelDb,
    pub server_group_db: ServerGroupDb,
    pub cli: CommandLineInterface,
    pub shutting_down: AtomicBool,
    pub shut_down: AtomicBool,
}

// A pseudo debug impl
impl Debug for Server {
    #[inline]
    fn fmt(&self, _: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl Server {

    pub fn println(&self, msg: &str) {
        self.cli.println(msg);
    }

}

pub struct User {
    pub uuid: UserUuid,
    pub name: SwapArc<String>,
    pub last_security_proof: U256,
    pub last_verified_security_level: u8,
    pub groups: RwLock<Vec<Uuid>>,
    pub connection: Arc<ClientConnection>,
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
        f.write_str(" tried to login with an incompatible protocol version ")?;
        f.write_str(self.recv_proto_ver.to_string().as_str())
    }
}

impl Display for ErrorAuthProtoVer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with an incompatible protocol version ")?;
        f.write_str(self.recv_proto_ver.to_string().as_str())
    }
}

impl Error for ErrorAuthProtoVer {}

struct ErrorAuthInvSecProof {
    ip: IpAddr,
    uuid: UserUuid,
}

impl Debug for ErrorAuthInvSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with invalid security proofs")
    }
}

impl Display for ErrorAuthInvSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with invalid security proofs")
    }
}

impl Error for ErrorAuthInvSecProof {}

struct ErrorAuthLowSecProof {
    ip: IpAddr,
    uuid: UserUuid,
    provided_lvl: u8,
}

impl Debug for ErrorAuthLowSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with a too weak security level ")?;
        f.write_str(&*format!("{}", self.provided_lvl))
    }
}

impl Display for ErrorAuthLowSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login with a too weak security level ")?;
        f.write_str(&*format!("{}", self.provided_lvl))
    }
}

impl Error for ErrorAuthLowSecProof {}

struct ErrorAlreadyOnline {
    ip: IpAddr,
    uuid: UserUuid,
}

impl Debug for ErrorAlreadyOnline {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login although they were already online")
    }
}

impl Display for ErrorAlreadyOnline {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(&*format!("{:?}", self.uuid))?;
        f.write_str(" tried to login although they were already online")
    }
}

impl Error for ErrorAlreadyOnline {}

struct CommandHelp();

impl CommandImpl for CommandHelp {
    fn execute(&self, server: &Arc<Server>, _input: &[&str]) -> anyhow::Result<()> {
        let cmds = server.cli.cmds();
        server.println(format!("Commands ({}):", cmds.len()).as_str());
        for cmd in cmds {
            let usage = if let Some(usage) = cmd.1.params() {
                let mut ret_usage = String::new();
                for param in usage.required() {
                    ret_usage.push(' ');
                    ret_usage.push('[');
                    ret_usage.push_str(&*param.name);
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
                server.println(format!("{}{}: {}", cmd.1.name(), usage, desc).as_str());
            } else {
                server.println(format!("{}{}", cmd.1.name(), usage).as_str());
            }
        }

        Ok(())
    }
}

struct CommandShutdown();

impl CommandImpl for CommandShutdown {
    fn execute(&self, server: &Arc<Server>, _input: &[&str]) -> anyhow::Result<()> {
        server.println("Shutting down...");
        server.shutting_down.store(true, Ordering::Release);
        for user in server.online_users.iter() {
            user.value().connection.close();
        }
        server.online_users.clear();
        server.println("Shutdown successfully!");
        server.shut_down.store(true, Ordering::Release);
        Ok(())
    }
}

struct CommandUser();

impl CommandImpl for CommandUser {
    fn execute(&self, server: &Arc<Server>, input: &[&str]) -> anyhow::Result<()> {
        todo!()
    }
}

struct CommandOnlineUsers();

impl CommandImpl for CommandOnlineUsers {
    fn execute(&self, server: &Arc<Server>, _input: &[&str]) -> anyhow::Result<()> {
        // FIXME: add groups printing support!
        server.println(format!("There are {} users online:", server.online_users.len()).as_str());
        server.println("Name   UUID   SecLevel");
        for user in server.online_users.iter() {
            server.println(format!("{} | {:?} | {}", user.name.load(), user.uuid, user.last_verified_security_level).as_str());
        }
        Ok(())
    }
}
