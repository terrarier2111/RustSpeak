#![feature(new_uninit)]

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::fs;
use std::fs::File;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicBool;
use arc_swap::ArcSwap;
use bytes::Buf;
use dashmap::DashMap;
use sled::Db;
use uuid::Uuid;
use crate::channel_db::{ChannelDb, ChannelDbEntry};
use crate::config::Config;
use crate::network::NetworkServer;
use crate::packet::{AuthFailure, AuthResponse, Channel, ChannelCreatePerms, ChannelPerms, ClientPacket, RemoteProfile, ServerGroup, ServerGroupPerms, ServerPacket};
use crate::protocol::{PROTOCOL_VERSION, RWBytes};
use crate::server_group_db::{ServerGroupDb, ServerGroupEntry};

mod certificate;
mod network;
mod packet;
mod config;
mod security_level;
mod protocol;
mod channel_db;
mod server_group_db;
mod cli;

const RELATIVE_USER_DB_PATH: &str = "user_db";
const RELATIVE_CHANNEL_DB_PATH: &str = "channel_db.json";
const RELATIVE_SERVER_GROUP_DB_PATH: &str = "server_group_db.json";
const ADMIN_GROUP_UUID: Uuid = Uuid::from_u128(0x1);
const DEFAULT_GROUP_UUID: Uuid = Uuid::from_u128(0x0);
const DEFAULT_CHANNEL_UUID: Uuid = Uuid::from_u128(0x0);

// FIXME: add CLI

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // FIXME: use logger!
    println!("Starting up server...");
    let data_dir = dirs::config_dir().unwrap().join("RustSpeakServer/");
    fs::create_dir_all(data_dir.clone())?;
    let user_db = sled::open(data_dir.clone().join(RELATIVE_USER_DB_PATH))?;
    let channel_db = ChannelDb::new(data_dir.clone().join(RELATIVE_CHANNEL_DB_PATH).to_string_lossy().to_string());
    let channels = channel_db.read_or_create(|| Ok(vec![ChannelDbEntry {
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
        }
    }]))?.into_iter().map(|entry| Channel {
        uuid: Uuid::from_u128(entry.id),
        password: AtomicBool::new(entry.password),
        name: Arc::new(RwLock::new(entry.name)),
        desc: Arc::new(RwLock::new(entry.desc)),
        perms: Arc::new(RwLock::new(entry.perms)),
        clients: Arc::new(Default::default()),
        proto_clients: Arc::new(Default::default()),
    }).collect::<Vec<_>>();
    let channels = {
        let mut result = HashMap::new();

        for channel in channels {
            result.insert(channel.uuid.clone(), channel);
        }

        result
    };
    let server_group_db = ServerGroupDb::new(data_dir.clone().join(RELATIVE_SERVER_GROUP_DB_PATH).to_string_lossy().to_string());
    let server_groups = server_group_db.read_or_create(|| Ok(vec![
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
                    set_password: false
                }
            }
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
                    set_password: false
                }
            }
        }
    ]))?;
    let server_groups = server_groups.into_iter().map(|server_group| ServerGroup {
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
                set_password: false
            }
        }
    }).collect::<Vec<_>>();
    let server_groups = {
        let mut result = HashMap::new();

        for server_group in server_groups.into_iter() {
            result.insert(server_group.uuid.clone(), Arc::new(server_group));
        }

        result
    };
    let config = Config::load_or_create(data_dir.join("config.json"))?;
    let network_server = setup_network_server(&config)?;

    let server = Arc::new(Server {
        server_groups: ArcSwap::new(Arc::new(server_groups)), // FIXME: load groups from database
        channels: ArcSwap::new(Arc::new(channels)),
        online_users: Default::default(),
        network_server: Arc::new(network_server),
        config: Arc::new(config),
        user_db: Arc::new(user_db),
        channel_db: Arc::new(channel_db),
        server_group_db: Arc::new(server_group_db),
    });

    println!("Server started up successfully, waiting for inbound connections...");
    start_server(server, |err| {
        println!("An error occurred while establishing a client connection: {}", err);
    }).await;
    Ok(())
}

fn setup_network_server(config: &Config) -> anyhow::Result<NetworkServer> {
    let (local_cert, private_key) = certificate::insecure_local::generate_self_signed_cert()?;
    NetworkServer::new(config.port, 1000, config.address_mode, certificate::create_config(local_cert, private_key)?)
}

async fn start_server<F: Fn(anyhow::Error)>(server: Arc<Server<'_>>, error_handler: F) {
    let tmp_srv = server.clone();
    server.network_server.accept_connections(move |new_conn| {
        let new_conn = new_conn.clone();
        let server = tmp_srv.clone();
        async move {
            let new_conn = new_conn.clone();
            let mut header = new_conn.read_reliable(2).await?;
            let size = header.get_u16_le();
            // let id = header.get_u8(); // FIXME: try to somehow get this data here already
            let mut data = new_conn.read_reliable(size as usize).await?;
            let packet = ClientPacket::read(&mut data)?;
            let server = server.clone();
            if let ClientPacket::AuthRequest { protocol_version, uuid, name, security_proofs, auth_id } = packet {
                if protocol_version != PROTOCOL_VERSION {
                    let failure = ServerPacket::AuthResponse(AuthResponse::Failure(AuthFailure::OutOfDate(PROTOCOL_VERSION)));
                    let encoded = failure.encode()?;
                    new_conn.send_reliable(&encoded).await?;
                    new_conn.close().await?;
                    return Err(anyhow::Error::from(ErrorAuthProtoVer {
                        ip: new_conn.conn.read().unwrap().connection.remote_address().ip(),
                        uuid,
                        recv_proto_ver: protocol_version,
                    }));
                }
                let security_proof_result = if let Some(level) = security_level::verified_security_level(uuid.as_u128(), security_proofs) {
                    level
                } else {
                    let failure = ServerPacket::AuthResponse(AuthResponse::Failure(AuthFailure::Invalid(Cow::from("Invalid security proofs!"))));
                    let encoded = failure.encode()?;
                    new_conn.send_reliable(&encoded).await?;
                    new_conn.close().await?;
                    return Err(anyhow::Error::from(ErrorAuthSecProof {
                        ip: new_conn.conn.read().unwrap().connection.remote_address().ip(),
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
                let failure = ServerPacket::AuthResponse(AuthResponse::Failure(AuthFailure::Invalid(Cow::from("The first packet sent has to be a `AuthRequest` packet!"))));
                let encoded = failure.encode()?;
                new_conn.send_reliable(&encoded).await?;
                new_conn.close().await?;
            }

            Ok(())
        }
    }, error_handler).await;
}

pub struct Server<'a> {
    // pub server_groups: DashMap<Uuid, ServerGroup>,
    pub server_groups: ArcSwap<HashMap<Uuid, Arc<ServerGroup<'a>>>>,
    pub channels: ArcSwap<HashMap<Uuid, Channel<'a>>>,
    pub online_users: DashMap<Uuid, User<'a>>,
    pub network_server: Arc<NetworkServer>,
    pub config: Arc<Config>, // FIXME: make this mutable somehow
    pub user_db: Arc<Db>,
    pub channel_db: Arc<ChannelDb>,
    pub server_group_db: Arc<ServerGroupDb>,
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
    uuid: Uuid,
    recv_proto_ver: u64,
}

impl Debug for ErrorAuthProtoVer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(self.uuid.to_string().as_str())?;
        f.write_str(" tried to login with incompatible protocol version ")?;
        f.write_str(self.recv_proto_ver.to_string().as_str())
    }
}

impl Display for ErrorAuthProtoVer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(self.uuid.to_string().as_str())?;
        f.write_str(" tried to login with incompatible protocol version ")?;
        f.write_str(self.recv_proto_ver.to_string().as_str())
    }
}

impl Error for ErrorAuthProtoVer {}

struct ErrorAuthSecProof {
    ip: IpAddr,
    uuid: Uuid,
}

impl Debug for ErrorAuthSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(self.uuid.to_string().as_str())?;
        f.write_str(" tried to login with invalid security proofs")
    }
}

impl Display for ErrorAuthSecProof {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("client from ")?;
        f.write_str(self.ip.to_string().as_str())?;
        f.write_str(" with uuid ")?;
        f.write_str(self.uuid.to_string().as_str())?;
        f.write_str(" tried to login with invalid security proofs")
    }
}

impl Error for ErrorAuthSecProof {}
