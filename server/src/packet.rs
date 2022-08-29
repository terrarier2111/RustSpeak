use std::fmt::Write;
use std::mem::{discriminant, transmute};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use uuid::Uuid;
use crate::profile::Profile;

/// packets the server sends to the client
pub enum ServerPacket {
    AuthResponse(AuthResponse),
    ChannelUpdate(ChannelUpdate),
    ClientConnected(RemoteProfile),
    ClientDisconnected(RemoteProfile),
    ClientUpdateServerGroups {
        client: Uuid,
        update: ClientUpdateServerGroups,
    },
    KeepAlive {
        id: u64,
        send_time: Duration,
    },
}

/// packets the client sends to the server
pub enum ClientPacket {
    AuthRequest {
        protocol_version: u64,
        profile: RemoteProfile,
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

    pub fn from_bytes(src: &mut Bytes) -> anhow::Result<Self> {
        let id = src.get_u8();
        match id {
            
            _ => Err(()),
        }
    }

}

impl ServerPacket {

    pub fn to_bytes(&self) -> anhow::Result<BytesMut> {
        let mut output = BytesMut::new();
        output.put_u8(discriminant(&self).into());
        match self {
            ServerPacket::AuthResponse(response) => {
                response.write(&mut output)?;
            }
            ServerPacket::ChannelUpdate(update) => {
                update.write(&mut output)?;
            }
            ServerPacket::ClientConnected(connected) => {
                connected.write(&mut output)?;
            }
            ServerPacket::ClientDisconnected(disconnected) => {
                disconnected.write(&mut output)?;
            }
            ServerPacket::ClientUpdateServerGroups { client, update } => {
                client.write(&mut output)?;
                update.write(&mut output)?;
            }
            ServerPacket::KeepAlive { id, send_time } => {
                output.put_u64_le(*id);
                send_time.write(&mut output)?;
            }
        }
        Ok(output)
    }

}

pub enum ChannelUpdate {
    Create(Channel),
    SubUpdate {
        channel: Uuid,
        update: ChannelSubUpdate,
    },
    Delete(Uuid),
}

impl RWBytes for ChannelUpdate {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let channel = Channel::read(src)?;
                Ok(Self::Create(channel))
            },
            1 => {
                let channel = Uuid::read(src)?;
                let update = ChannelSubUpdate::read(src)?;
                Ok(Self::SubUpdate { channel, update })
            },
            2 => {
                let uuid = Uuid::read(src)?;
                Ok(Self::Delete(uuid))
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(self).into());

        match self {
            ChannelUpdate::Create(channel) => {
                channel.write(dst)?;
            }
            ChannelUpdate::SubUpdate { channel, update } => {
                channel.write(dst)?;

            }
            ChannelUpdate::Delete(channel_id) => {
                channel_id.write(dst)?;
            }
        }
        Ok(())
    }
}

pub enum ChannelSubUpdate {
    Name(String),
    Desc(String),
    Perms(ChannelPerms),
    Client(ChannelSubClientUpdate),
}

impl RWBytes for ChannelSubUpdate {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let name = String::read(src)?;
                Ok(Self::Name(name))
            },
            1 => {
                let desc = String::read(src)?;
                Ok(Self::Desc(desc))
            },
            2 => {
                let channel_perms = ChannelPerms::read(src)?;
                Ok(Self::Perms(channel_perms))
            },
            3 => {
                let update = ChannelSubClientUpdate::read(src)?;
                Ok(Self::Client(update))
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(self).into());

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

pub enum ChannelSubClientUpdate {
    Add(Uuid), // FIXME: we have to ensure that all updates get flushed if there is any way the receiving client
    // FIXME: could not have a (up-to-date) client with the passed uuid in their database
    Remove(Uuid),
}

impl RWBytes for ChannelSubClientUpdate {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let uuid = Uuid::read(src)?;
                Ok(Self::Add(uuid))
            },
            1 => {
                let uuid = Uuid::read(src)?;
                Ok(Self::Remove(uuid))
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(self).into());

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

pub enum ClientUpdateServerGroups {
    Add(Uuid),
    Remove(Uuid),
}

impl RWBytes for ClientUpdateServerGroups {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();
        match disc {
            0 => {
                let uuid = Uuid::read(src)?;
                ClientUpdateServerGroups::Add(uuid)
            },
            1 => {
                let uuid = Uuid::read(src)?;
                ClientUpdateServerGroups::Remove(uuid)
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(response).into())?;
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

pub struct RemoteProfile {
    pub name: String,
    pub uuid: Uuid,
    pub server_groups: Vec<Uuid>,
}

impl RWBytes for RemoteProfile {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let name = String::read(src)?;
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

pub struct Channel {
    id: u64,
    password: bool, // FIXME: add capability to hide users if a password is set
    name: String,
    desc: String,
    perms: ChannelPerms,
    clients: Vec<RemoteProfile>,
}

impl RWBytes for Channel {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let id = u64::read(src)?;
        let password = bool::read(src)?;
        let name = String::read(src)?;
        let desc = String::read(src)?;
        let perms = ChannelPerms::read(src)?;
        let clients = Vec::<RemoteProfile>::read(src)?;

        Ok(Self {
            id,
            password,
            name,
            desc,
            perms,
            clients,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.id.write(dst)?;
        self.password.write(dst)?;
        self.name.write(dst)?;
        self.desc.write(dst)?;
        self.perms.write(dst)?;
        self.clients.write(dst)?;

        Ok(())
    }
}

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
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
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

pub struct ServerGroup {
    pub uuid: Uuid,
    pub name: String,
    pub priority: u64,
    pub perms: GroupPerms,
}

impl RWBytes for ServerGroup {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let uuid = Uuid::read(src)?;
        let name = String::read(src)?;
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

pub struct GroupPerms {
    server_group_assign: u64,
    server_group_unassign: u64,
    channel_see: u64,
    channel_join: u64,
    channel_send: u64,
    channel_modify: u64,
    channel_talk: u64,
    channel_assign_talk: u64,
    channel_delete: u64,
    channel_kick: u64,
    channel_create: ChannelCreatePerms,
}

impl RWBytes for GroupPerms {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
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

pub struct ChannelCreatePerms {
    power: u64,
    set_desc: bool,
    set_password: bool,
    // FIXME: add other features that channels have
}

impl RWBytes for ChannelCreatePerms {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
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

pub enum AuthResponse {
    Success {
        server_groups: Vec<ServerGroup>,
        own_groups: Vec<Uuid>,
        channels: Vec<Channel>,
    },
    Failure(AuthFailure),
}

impl RWBytes for AuthResponse {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let server_groups = Vec::<ServerGroup>::read(src)?;
                let own_groups = Vec::<Uuid>::read(src)?;
                let channels = Vec::<Channel>::read(src)?;
                Self::Success {
                    server_groups,
                    own_groups,
                    channels,
                }
            },
            1 => {
                let failure = AuthFailure::read(src)?;
                Self::Failure(failure)
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(self).into());
        match self {
            AuthResponse::Success { server_groups, own_groups, channels } => {
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

pub enum AuthFailure {
    Banned {
        reason: String,
        duration: BanDuration,
    },
    ReqSec(u8),
    OutOfDate(u64), // protocol version
    Invalid(String),
}

impl RWBytes for AuthFailure {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let reason = String::read(src)?;
                let duration = BanDuration::read(src)?;
                Self::Banned { reason, duration }
            },
            1 => {
                let req_ver = src.get_u8();
                Ok(Self::ReqSec(req_ver))
            },
            2 => {
                let req_ver = src.get_u64_le();
                Ok(Self::OutOfDate(req_ver))
            },
            3 => {
                let reason = String::read(src)?;
                Ok(Self::Invalid(reason))
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(self).into());
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

pub enum BanDuration {
    Permanent,
    Temporary(Duration),
}

impl RWBytes for BanDuration {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let disc = src.get_u8();

        match disc {
            0 => Ok(BanDuration::Permanent),
            1 => {
                let dur = Duration::read(src)?;
                Ok(BanDuration::Temporary(dur))
            },
            _ => Err(()),
        }
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(discriminant(self).into());

        match self {
            BanDuration::Permanent => {}
            BanDuration::Temporary(time) => {
                time.write(dst)?;
            }
        }
        Ok(())
    }
}

trait RWBytes: Sized {
    fn read(src: &mut Bytes) -> anyhow::Result<Self>;

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()>;
}

impl RWBytes for u128 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(src.get_u128_le())
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u128_le(*self);
        Ok(())
    }
}

impl RWBytes for u64 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(src.get_u64_le())
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u64_le(*self);
        Ok(())
    }
}

impl RWBytes for u32 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(src.get_u32_le())
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u32_le(*self);
        Ok(())
    }
}

impl RWBytes for u16 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(src.get_u16_le())
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u16_le(*self);
        Ok(())
    }
}

impl RWBytes for u8 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(src.get_u8())
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(*self);
        Ok(())
    }
}

impl RWBytes for bool {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(bool::try_from(src.get_u8())?)
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u8(*self as u8);
        Ok(())
    }
}

impl RWBytes for f32 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        // SAFETY: this is safe because all possible bit patterns are valid for f32
        Ok(unsafe { transmute(src.get_u32_le()) })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        // SAFETY: this is safe because all possible bit patterns are valid for u32
        dst.put_u32_le(unsafe { transmute(*self) });
        Ok(())
    }
}

impl RWBytes for f64 {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        // SAFETY: this is safe because all possible bit patterns are valid for f64
        Ok(unsafe { transmute(src.get_u64_le()) })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        // SAFETY: this is safe because all possible bit patterns are valid for u64
        dst.put_u64_le(unsafe { transmute(*self) });
        Ok(())
    }
}

impl RWBytes for Duration {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let secs = src.get_u64_le();
        let subsec_nanos = src.get_u32_le();
        Ok(Duration::new(secs, subsec_nanos))
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u64_le(self.as_secs());
        dst.put_u32_le(self.subsec_nanos());
        Ok(())
    }
}

impl RWBytes for String {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let len = src.get_u64_le();
        let result = src.read_bytes(&mut src.len() - src.remaining(), len)?;
        Ok(String::from(String::from_utf8_lossy(result)))
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u64_le(self.len() as u64);
        dst.write_str(self.as_str())?;
        Ok(())
    }
}

impl RWBytes for Uuid {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        Ok(Uuid::from_u128(src.get_u128_le()))
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u128_le(self.as_u128());
        Ok(())
    }
}

impl<T: RWBytes> RWBytes for Vec<T> {
    fn read(src: &mut Bytes) -> anyhow::Result<Self> {
        let len = src.get_u64_le() as usize;
        let mut result = Vec::with_capacity(len);
        for _ in 0..len {
            result.push(T::read(src)?);
        }
        Ok(result)
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        dst.put_u64_le(self.len() as u64);
        for val in self.iter() {
            val.write(dst)?;
        }
        Ok(())
    }
}
