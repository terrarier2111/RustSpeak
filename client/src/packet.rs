use crate::protocol::{ErrorEnumVariantNotFound, RWBytes, UserUuid};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use ordinalizer::Ordinal;
use ruint::aliases::U256;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use uuid::Uuid;

/// packets the server sends to the client
/// size: u64
/// id: u8
#[derive(Ordinal, Debug)]
#[repr(u8)]
pub enum ServerPacket<'a> {
    AuthResponse(AuthResponse<'a>) = 0,
    ChannelUpdate(ChannelUpdate<'a>) = 1,
    ClientConnected(RemoteProfile) = 2,
    ClientDisconnected(RemoteProfile) = 3,
    ClientUpdateServerGroups {
        client: UserUuid,
        update: ClientUpdateServerGroups,
    } = 4,
    KeepAlive { // FIXME: get rid of this!
        id: u64,
        send_time: Duration,
    } = 5,
    ChallengeRequest {
        signed_data: Vec<u8>, // contains the public server key and a random challenge and all of that encrypted with the client's public key
    } = 6,
    ForceDisconnect {
        reason: DisconnectReason,
    } = 7,
    SwitchChannelResponse(SwitchChannelResponse) = 8,
}

/// packets the client sends to the server
/// currently this packet's header is:
/// size: u16
/// id: u8
#[repr(u8)]
#[derive(Ordinal)]
pub enum ClientPacket {
    AuthRequest {
        protocol_version: u64,
        pub_key: Vec<u8>, // the public key of the client which gets later hashed to get it's id
        name: String,
        security_proofs: Vec<U256>, // TODO: add comment
        signed_data: Vec<u8>,       // contains a signed send time
    } = 0,
    Disconnect = 1,
    KeepAlive {
        id: u64,
        send_time: Duration,
    } = 2,
    UpdateClientServerGroups {
        client: UserUuid,
        update: ClientUpdateServerGroups,
    } = 3,
    ChallengeResponse {
        signed_data: Vec<u8>, // contains a signed copy of server's public key
    } = 4,
    SwitchChannel {
        channel: Uuid,
    } = 5,
}

impl ClientPacket {
    pub fn encode(&self) -> anyhow::Result<BytesMut> {
        let mut tmp_buf = BytesMut::new();
        self.write(&mut tmp_buf)?;
        let mut result_buf = BytesMut::with_capacity(8 + tmp_buf.len());
        result_buf.put_u64_le(tmp_buf.len() as u64);
        result_buf.put(tmp_buf);

        Ok(result_buf)
    }
}

impl ServerPacket<'_> {
    pub fn decode(buf: &mut Bytes) -> anyhow::Result<Self> {
        let _len = buf.get_u64_le(); // FIXME: use this!
        Self::read(buf)
    }
}

impl ServerPacket<'_> {
    pub fn encode(&self) -> anyhow::Result<BytesMut> {
        let mut buf = BytesMut::new();
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
                let client = UserUuid::read(src)?;
                let update = ClientUpdateServerGroups::read(src)?;
                Ok(Self::ClientUpdateServerGroups { client, update })
            }
            5 => {
                let id = u64::read(src)?;
                let send_time = Duration::read(src)?;
                Ok(Self::KeepAlive { id, send_time })
            }
            6 => {
                let signed_data = Vec::<u8>::read(src)?;
                Ok(Self::ChallengeRequest { signed_data })
            }
            7 => {
                let reason = DisconnectReason::read(src)?;
                Ok(Self::ForceDisconnect { reason })
            }
            8 => {
                let response = SwitchChannelResponse::read(src)?;
                Ok(Self::SwitchChannelResponse(response))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "ServerPacket",
                id,
            ))),
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
            ServerPacket::ChallengeRequest { signed_data } => {
                signed_data.write(dst)?;
            }
            ServerPacket::ForceDisconnect { reason } => {
                reason.write(dst)?;
            }
            ServerPacket::SwitchChannelResponse(response) => {
                response.write(dst)?;
            },
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
                let pub_key = Vec::<u8>::read(src)?;
                let name = String::read(src)?;
                let security_proofs = Vec::<U256>::read(src)?;
                let signed_data = Vec::<u8>::read(src)?;
                Ok(Self::AuthRequest {
                    protocol_version,
                    pub_key,
                    name,
                    security_proofs,
                    signed_data,
                })
            }
            1 => Ok(Self::Disconnect),
            2 => {
                let id = src.get_u64_le();
                let send_time = Duration::read(src)?;
                Ok(Self::KeepAlive { id, send_time })
            }
            3 => {
                let client = UserUuid::read(src)?;
                let update = ClientUpdateServerGroups::read(src)?;
                Ok(Self::UpdateClientServerGroups { client, update })
            }
            4 => {
                todo!()
            }
            5 => {
                let channel = Uuid::read(src)?;
                Ok(Self::SwitchChannel { channel })
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "ClientPacket",
                id,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            ClientPacket::AuthRequest {
                protocol_version,
                pub_key,
                name,
                security_proofs,
                signed_data,
            } => {
                dst.put_u64_le(*protocol_version);
                pub_key.write(dst)?;
                name.write(dst)?;
                security_proofs.write(dst)?;
                signed_data.write(dst)?;
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
            ClientPacket::ChallengeResponse { signed_data } => {
                signed_data.write(dst)?;
            }
            ClientPacket::SwitchChannel { channel } => {
                channel.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal, Debug)]
pub enum ChannelUpdate<'a> {
    Create(Channel),
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
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "ChannelUpdate",
                disc,
            ))),
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

#[derive(Ordinal, Debug)]
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

#[derive(Debug, Clone, Ordinal)]
pub enum ChannelSubClientUpdate {
    Add(UserUuid), // FIXME: we have to ensure that all updates get flushed if there is any way the receiving client
    // FIXME: could not have a (up-to-date) client with the passed uuid in their database
    Remove(UserUuid),
}

impl RWBytes for ChannelSubClientUpdate {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let uuid = UserUuid::read(src)?;
                Ok(Self::Add(uuid))
            }
            1 => {
                let uuid = UserUuid::read(src)?;
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

#[derive(Debug, Clone, Ordinal)]
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

#[derive(Debug, Clone)]
pub struct RemoteProfile {
    pub name: String,
    pub uuid: UserUuid,
    pub server_groups: Vec<Uuid>,
}

impl RWBytes for RemoteProfile {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let name = String::read(src)?;
        let uuid = UserUuid::read(src)?;
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

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: Uuid,
    pub password: bool, // FIXME: add capability to hide users if a password is set
    pub name: String,
    pub desc: String,
    pub perms: ChannelPerms,
    pub clients: DashMap<UserUuid, RemoteProfile>,
    pub slots: i16,
    pub sort_id: u16,
}

impl RWBytes for Channel {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let id = Uuid::read(src)?;
        let password = bool::read(src)?;
        let name = String::read(src)?;
        let desc = String::read(src)?;
        let perms = ChannelPerms::read(src)?;
        let clients = {
            let raw = Vec::<RemoteProfile>::read(src)?;
            let mut result = DashMap::new();
            for profile in raw {
                result.insert(profile.uuid, profile);
            }
            result
        };
        let slots = i16::read(src)?;
        let sort_id = u16::read(src)?;

        Ok(Self {
            id,
            password,
            name,
            desc,
            perms,
            clients,
            slots,
            sort_id,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.id.write(dst)?;
        self.password.write(dst)?;
        self.name.write(dst)?;
        self.desc.write(dst)?;
        self.perms.write(dst)?;
        self.clients.iter().map(|x| x.value().clone()).collect::<Vec<_>>().write(dst)?;
        self.slots.write(dst)?;
        self.sort_id.write(dst)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ChannelPerms {
    see: u64, // every channel one can see is automatically subscribed to
    // subscribe: u64,
    join: u64,
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
        let modify = u64::read(src)?;
        let talk = u64::read(src)?;
        let assign_talk = u64::read(src)?;
        let delete = u64::read(src)?;

        Ok(Self {
            see,
            join,
            modify,
            talk,
            assign_talk,
            delete,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.see.write(dst)?;
        self.join.write(dst)?;
        self.modify.write(dst)?;
        self.talk.write(dst)?;
        self.assign_talk.write(dst)?;
        self.delete.write(dst)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct GroupPerms {
    pub server_group_assign: u64,
    pub server_group_unassign: u64,
    pub channel_see: u64,
    pub channel_join: u64,
    pub channel_modify: u64,
    pub channel_talk: u64,
    pub channel_assign_talk: u64,
    pub channel_delete: u64,
    pub can_send: bool,
    pub channel_create: ChannelCreatePerms,
}

impl RWBytes for GroupPerms {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let server_group_assign = u64::read(src)?;
        let server_group_unassign = u64::read(src)?;
        let channel_see = u64::read(src)?;
        let channel_join = u64::read(src)?;
        let channel_modify = u64::read(src)?;
        let channel_talk = u64::read(src)?;
        let channel_assign_talk = u64::read(src)?;
        let channel_delete = u64::read(src)?;
        let can_send = bool::read(src)?;
        let channel_create = ChannelCreatePerms::read(src)?;

        Ok(Self {
            server_group_assign,
            server_group_unassign,
            channel_see,
            channel_join,
            channel_modify,
            channel_talk,
            channel_assign_talk,
            channel_delete,
            can_send,
            channel_create,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.server_group_assign.write(dst)?;
        self.server_group_unassign.write(dst)?;
        self.channel_see.write(dst)?;
        self.channel_join.write(dst)?;
        self.channel_modify.write(dst)?;
        self.channel_talk.write(dst)?;
        self.channel_assign_talk.write(dst)?;
        self.channel_delete.write(dst)?;
        self.can_send.write(dst)?;
        self.channel_create.write(dst)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ChannelCreatePerms {
    pub power: u64,
    pub set_desc: bool,
    pub set_password: bool,
    pub resort_channel: bool,
    // FIXME: add other features that channels have
}

impl RWBytes for ChannelCreatePerms {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let power = u64::read(src)?;
        let set_desc = bool::read(src)?;
        let set_password = bool::read(src)?;
        let resort_channel = bool::read(src)?;

        Ok(Self {
            power,
            set_desc,
            set_password,
            resort_channel,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.power.write(dst)?;
        self.set_desc.write(dst)?;
        self.set_password.write(dst)?;
        self.resort_channel.write(dst)?;

        Ok(())
    }
}

#[derive(Ordinal, Debug)]
pub enum AuthResponse<'a> {
    Success {
        default_channel_id: Uuid,
        server_groups: Vec<ServerGroup<'a>>,
        own_groups: Vec<Uuid>,
        channels: Vec<Channel>,
    },
    Failure(AuthFailure<'a>),
}

impl RWBytes for AuthResponse<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let default_channel_id = Uuid::read(src)?;
                let server_groups = Vec::<ServerGroup>::read(src)?;
                let own_groups = Vec::<Uuid>::read(src)?;
                let channels = Vec::<Channel>::read(src)?;
                Ok(Self::Success {
                    default_channel_id,
                    server_groups,
                    own_groups,
                    channels,
                })
            }
            1 => {
                let failure = AuthFailure::read(src)?;
                Ok(Self::Failure(failure))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "AuthResponse",
                disc,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        match self {
            AuthResponse::Success {
                default_channel_id,
                server_groups,
                own_groups,
                channels,
            } => {
                default_channel_id.write(dst)?;
                server_groups.write(dst)?;
                own_groups.write(dst)?;
                channels.write(dst)?;
            }
            AuthResponse::Failure(failure) => {
                failure.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal, Debug)]
pub enum AuthFailure<'a> {
    Banned {
        reason: String,
        duration: BanDuration,
    },
    ReqSec(u8),
    OutOfDate(u64), // protocol version
    AlreadyOnline,
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
                Ok(Self::AlreadyOnline)
            }
            4 => {
                let reason = String::read(src)?;
                Ok(Self::Invalid(Cow::from(reason)))
            }
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "AuthFailure",
                disc,
            ))),
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
            AuthFailure::AlreadyOnline => {}
            AuthFailure::Invalid(err) => {
                err.write(dst)?;
            }
        }
        Ok(())
    }
}

#[derive(Ordinal, Debug)]
#[repr(u8)]
pub enum SwitchChannelResponse {
    Success = 0,
    InvalidChannel = 1, // the client tried to join a channel that doesn't exist
    NoPermissions = 2, // the client has no permissions to join the desired channel
    SameChannel = 3, // the client tried to join the channel its already in
}

impl RWBytes for SwitchChannelResponse {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => Ok(Self::Success),
            1 => Ok(Self::InvalidChannel),
            2 => Ok(Self::NoPermissions),
            3 => Ok(Self::SameChannel),
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "SwitchChannelResponse",
                disc,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);
        Ok(())
    }
}

#[derive(Ordinal, Debug)]
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
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "BanDuration",
                disc,
            ))),
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

#[derive(Debug, Ordinal)]
#[repr(u8)]
pub enum DisconnectReason {
    Kicked {
        message: String,
    } = 0,
    ReceivedInvalidPacketSize {
        allowed: u64,
        received: u64,
    } = 1,
    DecodeError(String) = 2,
    Timeout = 3,
}

impl RWBytes for DisconnectReason {
    type Ty = Self;

    fn read(src: &mut Bytes) -> anyhow::Result<Self::Ty> {
        let ord = src.get_u8();

        match ord {
            0 => Ok(Self::Kicked { message: String::read(src)? }),
            1 => Ok(Self::ReceivedInvalidPacketSize { allowed: u64::read(src)?, received: u64::read(src)? }),
            2 => Ok(Self::DecodeError(String::read(src)?)),
            3 => Ok(Self::Timeout),
            _ => Err(anyhow::Error::from(ErrorEnumVariantNotFound(
                "DisconnectReason",
                ord,
            ))),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(self.ordinal() as u8);

        match self {
            DisconnectReason::Kicked { message } => {
                message.write(dst)
            }
            DisconnectReason::ReceivedInvalidPacketSize { allowed, received } => {
                allowed.write(dst)?;
                received.write(dst)
            }
            DisconnectReason::DecodeError(error) => error.write(dst),
            DisconnectReason::Timeout => Ok(()),
        }
    }
}
