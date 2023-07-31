#![feature(new_uninit)]
#![feature(int_roundings)]
#![feature(once_cell)]
#![feature(strict_provenance)]
#![feature(adt_const_params)]
#![feature(arbitrary_self_types)]
#![feature(const_option)]
#![feature(lazy_cell)]

use crate::channel_db::{ChannelDb, ChannelDbEntry};
use crate::cli::{CLIBuilder, CommandLineInterface};
use crate::config::Config;
use crate::network::{ClientConnection, handle_packet, NetworkServer};
use crate::packet::{
    AuthFailure, AuthResponse, Channel, ChannelCreatePerms, ChannelPerms, ClientPacket,
    RemoteProfile, ServerGroup, PermsSnapshot, ServerPacket,
};
use crate::protocol::{RWBytes, UserUuid, PROTOCOL_VERSION};
use crate::server_group_db::{ServerGroupDb, ServerGroupEntry};
use crate::user_db::{DbUser, UserDb};
use crate::utils::{LIGHT_GRAY, parse_bool};
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
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::{fs, thread};
use std::future::Future;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::Duration;
use crossbeam_utils::Backoff;
use futures::StreamExt;
use futures::task::noop_waker_ref;
use pollster::FutureExt;
use swap_arc::{SwapArc, SwapArcOption};
use tokio::{join, select};
use uuid::Uuid;
use crate::cli_core::{CmdParamNumConstraints, CmdParamStrConstraints, CommandBuilder, CommandImpl, CommandParam, CommandParamTy, EnumVal, UsageBuilder, UsageSubBuilder};
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
mod cli_core;

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
                sort_id: 0,
                name: Cow::Borrowed("Lobby"),
                desc: Default::default(),
                password: None,
                user_groups: vec![],
                perms: ChannelPerms {
                    see: 0,
                    join: 0,
                    modify: 100,
                    talk: 0,
                    assign_talk: 100,
                    delete: 100,
                },
                slots: -1,
            }])
        })?
        .into_iter()
        .map(|entry| Channel {
            uuid: Uuid::from_u128(entry.id),
            password: AtomicBool::new(entry.password.is_some()),
            name: Arc::new(SwapArc::new(Arc::new(entry.name.to_string()))),
            desc: Arc::new(SwapArc::new(Arc::new(entry.desc.to_string()))),
            perms: Arc::new(SwapArc::new(Arc::new(entry.perms))),
            clients: Arc::new(Default::default()),
            proto_clients: Arc::new(Default::default()),
            slots: AtomicI16::new(entry.slots),
            sort_id: AtomicU16::new(entry.sort_id),
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
                perms: PermsSnapshot {
                    server_group_assign: 0,
                    server_group_unassign: 0,
                    channel_see: 0,
                    channel_join: 0,
                    channel_modify: 0,
                    channel_talk: 0,
                    channel_assign_talk: 0,
                    channel_delete: 0,
                    can_send: false,
                    channel_create: ChannelCreatePerms {
                        power: 0,
                        set_desc: false,
                        set_password: false,
                        resort_channel: false,
                    },
                },
            },
            ServerGroupEntry {
                uuid: DEFAULT_GROUP_UUID.as_u128(),
                name: Cow::Borrowed("default"),
                perms: PermsSnapshot {
                    server_group_assign: 0,
                    server_group_unassign: 0,
                    channel_see: 0,
                    channel_join: 0,
                    channel_modify: 0,
                    channel_talk: 0,
                    channel_assign_talk: 0,
                    channel_delete: 0,
                    can_send: false,
                    channel_create: ChannelCreatePerms {
                        power: 0,
                        set_desc: false,
                        set_password: false,
                        resort_channel: false,
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
            perms: PermsSnapshot {
                server_group_assign: 0,
                server_group_unassign: 0,
                channel_see: 0,
                channel_join: 0,
                channel_modify: 0,
                channel_talk: 0,
                channel_assign_talk: 0,
                channel_delete: 0,
                can_send: false,
                channel_create: ChannelCreatePerms {
                    power: 0,
                    set_desc: false,
                    set_password: false,
                    resort_channel: false,
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
            CommandBuilder::new("help", CommandHelp())
                .desc("returns a list of available commands")
                .aliases(&["?", "h"]),
        )
        .command(
            CommandBuilder::new("stop", CommandShutdown())
                .desc("shuts down the server gracefully")
                .aliases(&["shutdown", "end", "kill", "off"]),
        )
        .command(
            CommandBuilder::new("user", CommandUser())
                .params(UsageBuilder::new().required(CommandParam {
                    name: "name".to_string(),
                    ty: CommandParamTy::String(CmdParamStrConstraints::None),
                }).optional(CommandParam {
                    name: "action".to_string(),
                    ty: CommandParamTy::Enum(vec![("delete", EnumVal::None), ("group", EnumVal::None), ("perms", EnumVal::None)]),
                })),
        )
        .command(
            CommandBuilder::new("onlineusers", CommandOnlineUsers()),
        )
        .command(
            CommandBuilder::new("channels", CommandChannels()),
    )
        .command(
            CommandBuilder::new("channel", CommandChannel())
                .params(UsageBuilder::new().required(CommandParam {
                    name: "name".to_string(),
                    ty: CommandParamTy::String(CmdParamStrConstraints::None),
                }).optional(CommandParam {
                    name: "action".to_string(),
                    ty: CommandParamTy::Enum(vec![("create", EnumVal::Complex(UsageSubBuilder::new().required(CommandParam {
                        name: "slots".to_string(),
                        ty: CommandParamTy::Int(CmdParamNumConstraints::None),
                    }))), // FIXME: expand this!
                                                  ("delete", EnumVal::None), ("edit", EnumVal::Complex(UsageSubBuilder::new().required(CommandParam {
                        name: "property".to_string(),
                        ty: CommandParamTy::Enum(vec![("name", EnumVal::Simple(CommandParamTy::String(CmdParamStrConstraints::None))), ("slots", EnumVal::Simple(CommandParamTy::Int(CmdParamNumConstraints::None)))]), // FIXME: expand this!
                    })))]),
                })),
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

    server.println(
        format!("Server started up successfully, waiting for inbound connections on port {}...",
        server.config.port).as_str()
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
                    // println!("got size {}", size);
                    let mut data = new_conn.read_reliable(size as usize).await?;
                    let packet = ClientPacket::read(&mut data, None)?;
                    // println!("read packet!");
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
                                    .remote_address()
                                    .ip(),
                                uuid,
                            }));
                        }
                        // FIXME: compare auth_id with the auth_id in our data base if this isn't the first login!
                        // FIXME: insert data send the proper data back!
                        new_conn.uuid.try_init(uuid).unwrap();
                        server.println(format!("{} ({:?}) successfully connected", name, uuid).as_str());
                        let server_groups = server.server_groups.read().await;
                        let server_groups = server_groups.values();
                        let user = if let Some(user) = server.user_db.get(&uuid)? {
                            user
                        } else {
                            let user = DbUser {
                                uuid,
                                name: name.clone(),
                                last_security_proof: last_security_proof.unwrap(),
                                last_verified_security_level: security_proof_result,
                                groups: vec![],
                                perms: PermsSnapshot::default(),
                            };
                            server.user_db.insert(user.clone())?;
                            user
                        };
                        server.println(format!("uuid_cmp: {}", uuid == user.uuid).as_str());
                        server.println(format!("uuid: {:?}", uuid).as_str());
                        server.println(format!("user uuid: {:?}", user.uuid).as_str());

                        let profile = RemoteProfile {
                            name,
                            uuid,
                            server_groups: user.groups.clone(),
                        };

                        // broadcast user join to other users
                        let packet = ServerPacket::ClientConnected(profile.clone());
                        let encoded = packet.encode()?;
                        for user in server.online_users.iter() {
                            user.connection.send_reliable(&encoded).await?;
                        }

                        let active_perms = {
                            let mut active = ActivePerms {
                                server_group_assign: user.perms.server_group_assign,
                                server_group_unassign: user.perms.server_group_unassign,
                                channel_see: user.perms.channel_see,
                                channel_join: user.perms.channel_join,
                                channel_modify: user.perms.channel_modify,
                                channel_talk: user.perms.channel_talk,
                                channel_assign_talk: user.perms.channel_assign_talk,
                                channel_delete: user.perms.channel_delete,
                                send: if user.perms.can_send {
                                    user.perms.channel_join
                                } else {
                                    0
                                },
                                channel_create: ActiveChannelCreatePerms {
                                    power: user.perms.channel_create.power,
                                    set_desc: if user.perms.channel_create.set_desc {
                                        user.perms.channel_create.power
                                    } else {
                                        0
                                    },
                                    set_password: if user.perms.channel_create.set_password {
                                        user.perms.channel_create.power
                                    } else {
                                        0
                                    },
                                    resort_channel: if user.perms.channel_create.resort_channel {
                                        user.perms.channel_create.power
                                    } else {
                                        0
                                    },
                                },
                            };

                            for group in user.groups.iter() {
                                let groups = server.server_groups.read().block_on();
                                let group = groups.get(group).unwrap();
                                if group.perms.server_group_assign > active.server_group_assign {
                                    active.server_group_assign = group.perms.server_group_assign;
                                }
                                if group.perms.server_group_unassign > active.server_group_unassign {
                                    active.server_group_unassign = group.perms.server_group_unassign;
                                }
                                if group.perms.channel_join > active.channel_join {
                                    active.channel_join = group.perms.channel_join;
                                }
                                if group.perms.can_send && group.perms.channel_join > active.send {
                                    active.send = group.perms.channel_join;
                                }
                                if group.perms.channel_talk > active.channel_talk {
                                    active.channel_talk = group.perms.channel_talk;
                                }
                                if group.perms.channel_see > active.channel_see {
                                    active.channel_see = group.perms.channel_see;
                                }
                                if group.perms.channel_delete > active.channel_delete {
                                    active.channel_delete = group.perms.channel_delete;
                                }
                                if group.perms.channel_assign_talk > active.channel_assign_talk {
                                    active.channel_assign_talk = group.perms.channel_assign_talk;
                                }
                                if group.perms.channel_modify > active.channel_modify {
                                    active.channel_modify = group.perms.channel_modify;
                                }
                                if group.perms.channel_create.power > active.channel_create.power {
                                    active.channel_create.power = group.perms.channel_create.power;
                                }
                                if group.perms.channel_create.resort_channel && group.perms.channel_create.power > active.channel_create.resort_channel {
                                    active.channel_create.resort_channel = group.perms.channel_create.power;
                                }
                                if group.perms.channel_create.set_password && group.perms.channel_create.power > active.channel_create.set_password {
                                    active.channel_create.set_password = group.perms.channel_create.power;
                                }
                                if group.perms.channel_create.set_desc && group.perms.channel_create.power > active.channel_create.set_desc {
                                    active.channel_create.set_desc = group.perms.channel_create.power;
                                }
                                // FIXME: extend this once there are more perms!
                            }

                            active
                        };

                        server.online_users.insert(uuid, User {
                            uuid,
                            name: SwapArc::new(Arc::new(user.name.into())),
                            last_security_proof: user.last_security_proof,
                            last_verified_security_level: user.last_verified_security_level,
                            groups: RwLock::new(user.groups.clone()),
                            connection: new_conn.clone(),
                            perms: SwapArc::new(Arc::new(user.perms)),
                            active_perms: SwapArc::new(Arc::new(active_perms)),
                        });
                        let channels = server.channels.read().await;
                        let channel = channels.get(&new_conn.channel.load()).unwrap();
                        channel.clients.write().await.push(uuid);
                        RwLock::write(&channel.proto_clients).unwrap().push(profile.clone());
                        // println!("channels: {}", channels.len());
                        let channels = channels.values();
                        let channels = channels.cloned().collect::<Vec<_>>();

                        let auth = ServerPacket::AuthResponse(AuthResponse::Success {
                            default_channel_id: Uuid::from_u128(server.config.default_channel_id),
                            server_groups: server_groups.cloned().collect::<Vec<_>>(), // FIXME: try getting rid of this clone!
                            own_groups: user.groups,
                            channels,
                        });
                        let encoded = auth.encode()?;
                        new_conn.send_reliable(&encoded).await?;
                        let keep_alive_stream = new_conn.conn.accept_bi().await?;
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

    pub fn read_channel_db(&self) -> anyhow::Result<Vec<ChannelDbEntry>> {
        let channels = self.channel_db
            .read_or_create(|| {
                Ok(vec![ChannelDbEntry {
                    id: DEFAULT_CHANNEL_UUID.as_u128(),
                    sort_id: 0,
                    name: Cow::Borrowed("Lobby"),
                    desc: Default::default(),
                    password: None,
                    user_groups: vec![],
                    perms: ChannelPerms {
                        see: 0,
                        join: 0,
                        modify: 100,
                        talk: 0,
                        assign_talk: 100,
                        delete: 100,
                    },
                    slots: 100,
                }])
            })?;
        Ok(channels)
    }

}

pub struct User {
    pub uuid: UserUuid,
    pub name: SwapArc<String>,
    pub last_security_proof: U256,
    pub last_verified_security_level: u8,
    pub groups: RwLock<Vec<Uuid>>,
    pub connection: Arc<ClientConnection>,
    pub perms: SwapArc<PermsSnapshot>,
    pub active_perms: SwapArc<ActivePerms>,
}

pub struct ActivePerms {
    pub server_group_assign: u64,
    pub server_group_unassign: u64,
    pub channel_see: u64,
    pub channel_join: u64,
    pub channel_modify: u64,
    pub channel_talk: u64,
    pub channel_assign_talk: u64,
    pub channel_delete: u64,
    pub send: u64,
    pub channel_create: ActiveChannelCreatePerms,
}

pub struct ActiveChannelCreatePerms {
    pub power: u64,
    pub set_desc: u64,
    pub set_password: u64,
    pub resort_channel: u64,
    // FIXME: add other features that channels have
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
    type CTX = Arc<Server>;

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
                    ty.push_str(param.ty.to_string(2).as_str());
                    ty.push(')');
                    let ty = ColoredString::from(ty.as_str()).italic().color(LIGHT_GRAY);
                    ret_usage.push_str(&*format!("{ty}"));
                    ret_usage.push(']');
                }
                for param in usage.optional() {
                    ret_usage.push(' ');
                    ret_usage.push('<');
                    ret_usage.push_str(&*param.name);
                    let mut ty = String::new();
                    ty.push('(');
                    ty.push_str(param.ty.to_string(2).as_str());
                    ty.push(')');
                    let ty = ColoredString::from(ty.as_str()).italic().color(LIGHT_GRAY);
                    ret_usage.push_str(&*format!("{ty}"));
                    ret_usage.push('>');
                }
                for param in usage.optional_prefixed() {
                    ret_usage.push(' ');
                    ret_usage.push_str(usage.optional_prefixed_prefix().as_ref().unwrap().as_str());
                    ret_usage.push('<');
                    ret_usage.push_str(&*param.name);
                    let mut ty = String::new();
                    ty.push('(');
                    ty.push_str(param.ty.to_string(2).as_str());
                    ty.push(')');
                    let ty = ColoredString::from(ty.as_str()).italic().color(LIGHT_GRAY);
                    ret_usage.push_str(&*format!("{ty}"));
                    ret_usage.push('>');
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
    type CTX = Arc<Server>;

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
    type CTX = Arc<Server>;

    fn execute(&self, server: &Arc<Server>, input: &[&str]) -> anyhow::Result<()> {
        todo!()
    }
}

struct CommandOnlineUsers();

impl CommandImpl for CommandOnlineUsers {
    type CTX = Arc<Server>;

    fn execute(&self, server: &Arc<Server>, _input: &[&str]) -> anyhow::Result<()> {
        // FIXME: add groups printing support!
        if server.online_users.len() == 1 {
            server.println("There is 1 user online:");
        } else {
            server.println(format!("There are {} users online:", server.online_users.len()).as_str());
        }
        server.println("Name   UUID   SecLevel");
        for user in server.online_users.iter() {
            server.println(format!("{} | {:?} | {}", user.name.load(), user.uuid, user.last_verified_security_level).as_str());
        }
        Ok(())
    }
}

struct CommandChannel();

impl CommandImpl for CommandChannel {
    type CTX = Arc<Server>;

    fn execute(&self, server: &Arc<Server>, input: &[&str]) -> anyhow::Result<()> {
        if input.len() == 1 {
            // FIXME: print channel info!
            return Ok(());
        }
        match input[1] {
            "create" => {
                let mut id = rand::random::<u128>();
                let mut db = server.read_channel_db()?;
                while db.iter().any(|channel| channel.id == id) {
                    id = rand::random::<u128>();
                }
                let desc = if input.len() >= 6 {
                    input[5..].join(" ").to_string()
                } else {
                    String::new()
                };
                let pw = input.get(4).map(|raw| Some(Cow::Owned(raw.to_string()))).unwrap_or(None);
                let has_pw = pw.is_some();
                let slots = isize::from_str(input[2]).unwrap() as i16;
                if slots < -1 {
                    panic!("A slot count below -1 is illegal");
                }
                let sort_id = input.get(3).map(|raw| usize::from_str(raw).unwrap() as u16).unwrap_or_else(|| {
                    let mut last_id = 0;
                    for channel in db.iter() {
                        if channel.sort_id > last_id {
                            last_id = channel.sort_id;
                        }
                    }
                    last_id + 1
                });
                let channel = ChannelDbEntry {
                    id,
                    sort_id,
                    name: Cow::Owned(input[0].to_string()),
                    desc: Cow::Owned(desc.clone()),
                    password: pw,
                    user_groups: vec![],
                    perms: ChannelPerms::default(), // FIXME: make this configurable via cmd params!
                    slots,
                };
                db.push(channel);
                server.channel_db.write(&db).expect("An error occurred while writing the new database!");
                server.channels.write().block_on().insert(Uuid::from_u128(id), Channel {
                    uuid: Uuid::from_u128(id),
                    password: AtomicBool::new(has_pw),
                    name: Arc::new(SwapArc::new(Arc::new(input[0].to_string()))),
                    desc: Arc::new(SwapArc::new(Arc::new(desc))),
                    perms: Arc::new(SwapArc::new(Arc::new(ChannelPerms::default()))), // FIXME: make this configurable via cmd params!
                    clients: Arc::new(Default::default()),
                    proto_clients: Arc::new(Default::default()),
                    slots: AtomicI16::new(slots),
                    sort_id: AtomicU16::new(sort_id),
                });

                server.println(format!("Created channel {}", input[0]).as_str());
            },
            "edit" => {},
            "delete" => {
                let channel = server.channels.read().block_on().values().find(|channel| channel.name.load().deref().as_str() == input[0]).map(|channel| channel.uuid.clone());
                if channel.is_none() {
                    return Err(anyhow::Error::from(ChannelInexistentError(input[0].to_string())));
                }
                if server.config.default_channel_id == channel.as_ref().unwrap().as_u128() {
                    return Err(anyhow::Error::from(DefaultChannelNotDeletableError(input[0].to_string())));
                }
                // FIXME: move all clients from the to-be-deleted channel into the default channel!
                let mut channels = server.channels.write().block_on();
                channels.remove(&channel.unwrap());
                let mut db = server.read_channel_db()?;
                let entry = db.iter().enumerate().find(|entry| entry.1.id == channel.as_ref().unwrap().as_u128());
                match entry {
                    None => {
                        return Err(anyhow::Error::from(ChannelInexistentError(input[0].to_string())));
                    }
                    Some(entry) => {
                        let id = entry.0;
                        db.remove(id);
                    }
                }
                server.channel_db.write(&db).expect("An error occurred while writing the new database!");
                server.println(format!("Deleted channel {}", input[0]).as_str());
            },
            _ => unreachable!(),
        }
        Ok(())
    }
}

struct ChannelInexistentError(String);

impl Debug for ChannelInexistentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("There is no channel named ")?;
        f.write_str(self.0.as_str())
    }
}

impl Display for ChannelInexistentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for ChannelInexistentError {}

struct DefaultChannelNotDeletableError(String);

impl Debug for DefaultChannelNotDeletableError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())?;
        f.write_str(" and may not be deleted")
    }
}

impl Display for DefaultChannelNotDeletableError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for DefaultChannelNotDeletableError {}

struct CommandChannels();

impl CommandImpl for CommandChannels {
    type CTX = Arc<Server>;

    fn execute(&self, server: &Arc<Server>, input: &[&str]) -> anyhow::Result<()> {
        let channels = server.channels.read().block_on();
        if channels.len() == 1 {
            server.println("There is 1 channel:");
        } else {
            server.println(format!("There are {} channels:", channels.len()).as_str());
        }
        server.println("Name   UUID   Users/Slots");
        for channel in channels.values() {
            let slots = channel.slots.load(Ordering::Acquire);
            server.println(format!("{} | {:?} | {}/{}", channel.name.load(), channel.uuid, channel.clients.read().block_on().len(), if slots == -1 {
                String::from("unlimited")
            } else {
                slots.to_string()
            }).as_str());
        }
        Ok(())
    }
}
