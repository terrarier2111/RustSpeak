use std::borrow::Cow;
use std::cell::RefCell;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::mem::{discriminant, transmute};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use futures::AsyncWriteExt;
use uuid::Uuid;
use crate::protocol::{ErrorEnumVariantNotFound, RWBytes, RWBytesMut};
use ordinalizer::Ordinal;
use serde_derive::{Serialize, Deserialize};

/// packets the server sends to the client
/// size: u64
/// id: u8
#[derive(Ordinal)]
pub enum ServerPacket<'a> {
    AuthResponse(AuthResponse<'a>),
    ChannelUpdate(ChannelUpdate<'a>),
    ClientConnected(RemoteProfile<'a>),
    ClientDisconnected(RemoteProfile<'a>),
    ClientUpdateServerGroups {
        client: Uuid,
        update: ClientUpdateServerGroups,
    },
    KeepAlive {
        id: u64,
        send_time: Duration,
    },
    AuthSecurityRequest(AuthSecurityRequest),
}

/// packets the client sends to the server
/// currently this packet's header is:
/// size: u16
/// id: u8
#[derive(Ordinal)]
pub enum ClientPacket {
    AuthRequest {
        protocol_version: u64,
        uuid: Uuid,
        name: String,
        security_proofs: Vec<u128>,
        auth_id: Uuid, // a uuid that is generated from hashing the `private_ley ^ hash(server_address)`
    },
    Disconnect,
    KeepAlive {
        id: u64,
        send_time: Duration,
    },
    UpdateClientServerGroups {
        client: Uuid,
        update: ClientUpdateServerGroups,
    },
}

impl ClientPacket {
    pub fn encode(&self) -> anyhow::Result<BytesMut> {
        let mut buf = BytesMut::new();
        // FIXME: also write length prefix!
        self.write(&mut buf)?;
        Ok(buf)
    }
}

impl ServerPacket<'_> {
    pub fn encode(&self) -> anyhow::Result<BytesMut> {
        let mut buf = BytesMut::new();
        // FIXME: also write length prefix!
        self.write(&mut buf)?;
        Ok(buf)
    }
}

impl RWBytes for ServerPacket<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let id = src.get_u8();

        match id {
            0 => Ok(Self::AuthResponse(AuthResponse::read(src)?)),
            1 => Ok(Self::ChannelUpdate(ChannelUpdate::read(src)?)),
            2 => Ok(Self::ClientConnected(RemoteProfile::read(src)?)),
            3 => Ok(Self::ClientDisconnected(RemoteProfile::read(src)?)),
            4 => {
                let client = Uuid::read(src)?;
                let update = ClientUpdateServerGroups::read(src)?;
                Ok(Self::ClientUpdateServerGroups { client, update })
            }
            5 => {
                let id = u64::read(src)?;
                let send_time = Duration::read(src)?;
                Ok(Self::KeepAlive { id, send_time })
            }
            6 => {
                let challenge = Uuid::read(src)?;
                Ok(Self::AuthSecurityRequest(AuthSecurityRequest {
                    challenge,
                }))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("ServerPacket", id))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            ServerPacket::AuthResponse(response) => {
                response.write(dst)?;
            }
            ServerPacket::ChannelUpdate(update) => {
                update.write(dst)?;
            }
            ServerPacket::ClientConnected(connected) => {
                connected.write(dst)?;
            }
            ServerPacket::ClientDisconnected(disconnected) => {
                disconnected.write(dst)?;
            }
            ServerPacket::ClientUpdateServerGroups { client, update } => {
                client.write(dst)?;
                update.write(dst)?;
            }
            ServerPacket::KeepAlive { id, send_time } => {
                dst.put_u64_le(*id);
                send_time.write(dst)?;
            }
            ServerPacket::AuthSecurityRequest(request) => {
                request.write(dst)?;
            }
        }
        Ok(())
    }
}

impl RWBytes for ClientPacket {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let id = src.get_u8();
        match id {
            0 => {
                let protocol_version = u64::read(src)?;
                let name = String::read(src)?;
                let uuid = Uuid::read(src)?;
                let security_proofs = Vec::<u128>::read(src)?;
                let auth_id = Uuid::read(src)?;
                Ok(Self::AuthRequest {
                    protocol_version,
                    uuid,
                    name,
                    security_proofs,
                    auth_id,
                })
            }
            1 => Ok(Self::Disconnect),
            2 => {
                let id = src.get_u64_le();
                let send_time = Duration::read(src)?;
                Ok(Self::KeepAlive { id, send_time })
            }
            3 => {
                let client = Uuid::read(src)?;
                let update = ClientUpdateServerGroups::read(src)?;
                Ok(Self::UpdateClientServerGroups { client, update })
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("ClientPacket", id))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            ClientPacket::AuthRequest {
                protocol_version,
                name,
                uuid,
                security_proofs,
                auth_id,
            } => {
                dst.put_u64_le(*protocol_version);
                name.write(dst)?;
                uuid.write(dst)?;
                security_proofs.write(dst)?;
                auth_id.write(dst)?;
            }
            ClientPacket::Disconnect => {}
            ClientPacket::KeepAlive { id, send_time } => {
                dst.put_u64_le(*id);
                send_time.write(dst)?;
            }
            ClientPacket::UpdateClientServerGroups { client, update } => {
                client.write(dst)?;
                update.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal)]
pub enum ChannelUpdate<'a> {
    Create(Channel<'a>),
    SubUpdate {
        channel: Uuid,
        update: ChannelSubUpdate<'a>,
    },
    Delete(Uuid),
}

impl RWBytes for ChannelUpdate<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let channel = Channel::read(src)?;
                Ok(Self::Create(channel))
            }
            1 => {
                let channel = Uuid::read(src)?;
                let update = ChannelSubUpdate::read(src)?;
                Ok(Self::SubUpdate { channel, update })
            }
            2 => {
                let uuid = Uuid::read(src)?;
                Ok(Self::Delete(uuid))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("ChannelUpdate", disc))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);

        match self {
            ChannelUpdate::Create(channel) => {
                channel.write(dst)?;
            }
            ChannelUpdate::SubUpdate { channel, update } => {
                channel.write(dst)?;
                update.write(dst)?;
            }
            ChannelUpdate::Delete(channel_id) => {
                channel_id.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal)]
pub enum ChannelSubUpdate<'a> {
    Name(Cow<'a, String>),
    Desc(Cow<'a, String>),
    Perms(ChannelPerms),
    Client(ChannelSubClientUpdate),
}

impl RWBytes for ChannelSubUpdate<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let name = Cow::<String>::read(src)?;
                Ok(Self::Name(name))
            }
            1 => {
                let desc = Cow::<String>::read(src)?;
                Ok(Self::Desc(desc))
            }
            2 => {
                let channel_perms = ChannelPerms::read(src)?;
                Ok(Self::Perms(channel_perms))
            }
            3 => {
                let update = ChannelSubClientUpdate::read(src)?;
                Ok(Self::Client(update))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "ChannelSubUpdate",
                disc,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);

        match self {
            ChannelSubUpdate::Name(name) => {
                name.write(dst)?;
            }
            ChannelSubUpdate::Desc(desc) => {
                desc.write(dst)?;
            }
            ChannelSubUpdate::Perms(perms) => {
                perms.write(dst)?;
            }
            ChannelSubUpdate::Client(client_update) => {
                client_update.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal, Debug, Clone)]
pub enum ChannelSubClientUpdate {
    Add(Uuid), // FIXME: we have to ensure that all updates get flushed if there is any way the receiving client
    // FIXME: could not have a (up-to-date) client with the passed uuid in their database
    Remove(Uuid),
}

impl RWBytes for ChannelSubClientUpdate {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let uuid = Uuid::read(src)?;
                Ok(Self::Add(uuid))
            }
            1 => {
                let uuid = Uuid::read(src)?;
                Ok(Self::Remove(uuid))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "ChannelSubClientUpdate",
                disc,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);

        match self {
            ChannelSubClientUpdate::Add(uuid) => {
                uuid.write(dst)?;
            }
            ChannelSubClientUpdate::Remove(uuid) => {
                uuid.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal, Debug, Clone)]
pub enum ClientUpdateServerGroups {
    Add(Uuid),
    Remove(Uuid),
}

impl RWBytes for ClientUpdateServerGroups {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();
        match disc {
            0 => {
                let uuid = Uuid::read(src)?;
                Ok(ClientUpdateServerGroups::Add(uuid))
            }
            1 => {
                let uuid = Uuid::read(src)?;
                Ok(ClientUpdateServerGroups::Remove(uuid))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "ClientUpdateServerGroups",
                disc,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            ClientUpdateServerGroups::Add(uuid) => {
                uuid.write(dst)?;
            }
            ClientUpdateServerGroups::Remove(uuid) => {
                uuid.write(dst)?;
            }
        }
        Ok(())
    }
}

pub struct RemoteProfile<'a> {
    pub name: Cow<'a, String>,
    pub uuid: Uuid,
    pub server_groups: Vec<Uuid>,
}

impl RWBytes for RemoteProfile<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let name = Cow::<String>::read(src)?;
        let uuid = Uuid::read(src)?;
        let server_groups = Vec::<Uuid>::read(src)?;

        Ok(Self {
            name,
            uuid,
            server_groups,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.name.write(dst)?;
        self.uuid.write(dst)?;
        self.server_groups.write(dst)?;

        Ok(())
    }
}

pub struct Channel<'a> {
    pub uuid: Uuid,
    pub password: AtomicBool,
    // pub hide_users_if_pw: AtomicBool, // FIXME: add capability to hide users if a password is set
    pub name: Arc<RwLock<Cow<'a, str>>>,
    pub desc: Arc<RwLock<Cow<'a, str>>>,
    pub perms: Arc<RwLock<ChannelPerms>>,
    pub clients: Arc<RwLock<Vec<Uuid>>>,
    pub proto_clients: Arc<RwLock<Vec<RemoteProfile<'a>>>>,
}

// FIXME: use Arc<Channel<'_>> so that we don't need a clone impl for Channel<'_>
impl Clone for Channel<'_> {
    fn clone(&self) -> Self {
        Self {
            uuid: self.uuid,
            password: AtomicBool::new(self.password.load(Ordering::Acquire)),
            name: self.name.clone(),
            desc: self.desc.clone(),
            perms: self.perms.clone(),
            clients: self.clients.clone(),
            proto_clients: self.proto_clients.clone(),
        }
    }
}

impl RWBytes for Channel<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let uuid = Uuid::read(src)?;
        let password = AtomicBool::new(bool::read(src)?);
        let name = Arc::new(RwLock::new(Cow::<str>::read(src)?));
        let desc = Arc::new(RwLock::new(Cow::<str>::read(src)?));
        let perms = Arc::new(RwLock::new(ChannelPerms::read(src)?));
        let clients = Arc::new(RwLock::new(Vec::<RemoteProfile>::read(src)?));

        Ok(Self {
            uuid,
            password,
            name,
            desc,
            perms,
            proto_clients: clients,
            clients: Arc::new(RwLock::new(vec![])),
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.id.write(dst)?;
        self.password.write(dst)?;
        RWBytes::write(&self.name, dst)?;
        RWBytes::write(&self.desc, dst)?;
        RWBytes::write(&self.perms, dst)?;
        RWBytes::write(&self.proto_clients, dst)?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelPerms {
    see: u64, // every channel one can see is automatically subscribed to
    // subscribe: u64,
    join: u64,
    send: u64,
    modify: u64,
    talk: u64,
    assign_talk: u64,
    delete: u64, // this might be useful for regulating bots for example
    // kicking is handled simply as a move into the default channel
}

impl RWBytes for ChannelPerms {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let see = u64::read(src)?;
        let join = u64::read(src)?;
        let send = u64::read(src)?;
        let modify = u64::read(src)?;
        let talk = u64::read(src)?;
        let assign_talk = u64::read(src)?;
        let delete = u64::read(src)?;

        Ok(Self {
            see,
            join,
            send,
            modify,
            talk,
            assign_talk,
            delete,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.see.write(dst)?;
        self.join.write(dst)?;
        self.send.write(dst)?;
        self.modify.write(dst)?;
        self.talk.write(dst)?;
        self.assign_talk.write(dst)?;
        self.delete.write(dst)?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct ServerGroup<'a> {
    pub uuid: Uuid,
    pub name: Cow<'a, String>,
    pub priority: u64,
    pub perms: GroupPerms,
}

impl RWBytes for ServerGroup<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let uuid = Uuid::read(src)?;
        let name = Cow::<String>::read(src)?;
        let priority = u64::read(src)?;
        let perms = GroupPerms::read(src)?;

        Ok(Self {
            uuid,
            name,
            priority,
            perms,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.uuid.write(dst)?;
        self.name.write(dst)?;
        self.priority.write(dst)?;
        self.perms.write(dst)?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct GroupPerms {
    pub server_group_assign: u64,
    pub server_group_unassign: u64,
    pub channel_see: u64,
    pub channel_join: u64,
    pub channel_send: u64,
    pub channel_modify: u64,
    pub channel_talk: u64,
    pub channel_assign_talk: u64,
    pub channel_delete: u64,
    pub channel_kick: u64,
    pub channel_create: ChannelCreatePerms,
}

impl RWBytes for GroupPerms {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let server_group_assign = u64::read(src)?;
        let server_group_unassign = u64::read(src)?;
        let channel_see = u64::read(src)?;
        let channel_join = u64::read(src)?;
        let channel_send = u64::read(src)?;
        let channel_modify = u64::read(src)?;
        let channel_talk = u64::read(src)?;
        let channel_assign_talk = u64::read(src)?;
        let channel_delete = u64::read(src)?;
        let channel_kick = u64::read(src)?;
        let channel_create = ChannelCreatePerms::read(src)?;

        Ok(Self {
            server_group_assign,
            server_group_unassign,
            channel_see,
            channel_join,
            channel_send,
            channel_modify,
            channel_talk,
            channel_assign_talk,
            channel_delete,
            channel_kick,
            channel_create,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.server_group_assign.write(dst)?;
        self.server_group_unassign.write(dst)?;
        self.channel_see.write(dst)?;
        self.channel_join.write(dst)?;
        self.channel_send.write(dst)?;
        self.channel_modify.write(dst)?;
        self.channel_talk.write(dst)?;
        self.channel_assign_talk.write(dst)?;
        self.channel_delete.write(dst)?;
        self.channel_kick.write(dst)?;
        self.channel_create.write(dst)?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct ChannelCreatePerms {
    pub power: u64,
    pub set_desc: bool,
    pub set_password: bool,
    // FIXME: add other features that channels have
}

impl RWBytes for ChannelCreatePerms {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let power = u64::read(src)?;
        let set_desc = bool::read(src)?;
        let set_password = bool::read(src)?;

        Ok(Self {
            power,
            set_desc,
            set_password,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.power.write(dst)?;
        self.set_desc.write(dst)?;
        self.set_password.write(dst)?;

        Ok(())
    }
}

#[derive(Ordinal)]
pub enum AuthResponse<'a> {
    Success {
        // server_groups: Cow<'a, dyn Into<dyn ExactSizeIterator<Item = &'a ServerGroup>>>,
        server_groups: Vec<Arc<ServerGroup<'a>>>,
        own_groups: Vec<Uuid>,
        // channels: RefCell<Box<dyn ExactSizeIterator<Item = &'a Channel<'a>>>>,
        // channels: Cow<'a, dyn Into<dyn ExactSizeIterator<Item = &'a Channel>>>,
        channels: Vec<Channel<'a>>,
    },
    Failure(AuthFailure<'a>),
}

impl RWBytes for AuthResponse<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        /*
        /*let disc = src.get_u8();

        match disc {
            0 => {
                let server_groups = Vec::<Arc<ServerGroup>>::read(src)?;
                let own_groups = Vec::<Uuid>::read(src)?;
                let channels = Box::new(Vec::<Channel>::read(src)?);
                Ok(Self::Success {
                    server_groups,
                    own_groups,
                    channels,
                })
            }
            1 => {
                let failure = AuthFailure::read(src)?;
                Ok(Self::Failure(failure))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("AuthResponse", disc))),
        }*/
        // this packet should only be read from clients
        unreachable!()*/
        let disc = src.get_u8();

        match disc {
            0 => {
                let server_groups = Vec::<Arc<ServerGroup>>::read(src)?;
                let own_groups = Vec::<Uuid>::read(src)?;
                let channels = Vec::<Channel>::read(src)?;
                Ok(Self::Success {
                    server_groups,
                    own_groups,
                    channels,
                })
            }
            1 => {
                let failure = AuthFailure::read(src)?;
                Ok(Self::Failure(failure))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("AuthResponse", disc))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            AuthResponse::Success {
                server_groups,
                own_groups,
                channels,
            } => {
                server_groups.write(dst)?;
                own_groups.write(dst)?;
                // let mut val = channels.borrow_mut();
                // RWBytesMut::write(val.deref_mut(), dst)?;
                channels.write(dst)?;
            }
            AuthResponse::Failure(failure) => {
                failure.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal)]
pub enum AuthFailure<'a> {
    Banned {
        reason: String,
        duration: BanDuration,
    },
    ReqSec(u8),
    OutOfDate(u64), // protocol version
    Invalid(Cow<'a, str>),
}

impl RWBytes for AuthFailure<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let reason = String::read(src)?;
                let duration = BanDuration::read(src)?;
                Ok(Self::Banned { reason, duration })
            }
            1 => {
                let req_ver = src.get_u8();
                Ok(Self::ReqSec(req_ver))
            }
            2 => {
                let req_ver = src.get_u64_le();
                Ok(Self::OutOfDate(req_ver))
            }
            3 => {
                let reason = String::read(src)?;
                Ok(Self::Invalid(Cow::from(reason)))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("AuthFailure", disc))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            AuthFailure::Banned { reason, duration } => {
                reason.write(dst)?;
                duration.write(dst)?;
            }
            AuthFailure::ReqSec(security) => {
                dst.put_u8(*security);
            }
            AuthFailure::OutOfDate(req_protocol) => {
                dst.put_u64_le(*req_protocol);
            }
            AuthFailure::Invalid(err) => {
                err.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal)]
pub enum BanDuration {
    Permanent,
    Temporary(Duration),
}

impl RWBytes for BanDuration {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => Ok(BanDuration::Permanent),
            1 => {
                let dur = Duration::read(src)?;
                Ok(BanDuration::Temporary(dur))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound("BanDuration", disc))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);

        match self {
            BanDuration::Permanent => {}
            BanDuration::Temporary(time) => {
                time.write(dst)?;
            }
        }
        Ok(())
    }
}

pub struct AuthSecurityRequest {
    /// A challenge that's different for each individual client
    pub challenge: Uuid,
}

impl RWBytes for AuthSecurityRequest {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let challenge = Uuid::read(src)?;
        Ok(Self {
            challenge,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.challenge.write(dst)
    }
}
