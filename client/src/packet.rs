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

    pub fn to_bytes(&self) -> anhow::Result<BytesMut> {
        let mut output = BytesMut::new();
        output.put_u8(discriminant(&self).into());
        match self {
            ClientPacket::AuthRequest { protocol_version, profile, security_proofs, auth_id } => {
                output.put_u64_le(*protocol_version);
                profile.write(&mut output)?;
                security_proofs.write(&mut output)?;
                auth_id.write(&mut output)?;
            }
            ClientPacket::Disconnect => {}
            ClientPacket::KeepAlive { id, send_time } => {
                output.put_u64_le(*id);
                send_time.write(&mut output)?;
            }
            ClientPacket::UpdateClientServerGroups { client, update } => {
                client.write(&mut output)?;
                update.write(&mut output)?;
            }
        }
        Ok(output)
    }

}

impl ServerPacket {

    pub fn from_bytes(src: &mut Bytes) -> anhow::Result<Self> {
        let id = src.get_u8();
        match id {

            _ => Err(()),
        }
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

pub enum ChannelSubUpdate {
    Name(String),
    Desc(String),
    Perms(ChannelPerms),
    Client(ChannelSubClientUpdate),
}

pub enum ChannelSubClientUpdate {
    Add(Uuid), // FIXME: we have to ensure that all updates get flushed if there is any way the receiving client
               // FIXME: could not have a (up-to-date) client with the passed uuid in their database
    Remove(Uuid),
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
    uuid: Uuid,
    pub server_groups: Vec<Uuid>,
}

impl RemoteProfile {

    pub fn from_existing(name: String, uuid: Uuid, server_groups: Vec<Uuid>) -> Self {
        Self {
            name,
            uuid,
            server_groups,
        }
    }

    #[inline(always)]
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
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

pub struct ServerGroup {
    pub uuid: Uuid,
    pub name: String,
    pub priority: u64,
    pub perms: GroupPerms,
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

pub struct ChannelCreatePerms {
    power: u64,
    set_desc: bool,
    set_password: bool,
    // FIXME: add other features that channels have
}

pub enum AuthResponse {
    Success {
        server_groups: Vec<ServerGroup>,
        own_groups: Vec<Uuid>,
        channels: Vec<Channel>,
    },
    Failure(AuthFailure),
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

pub enum BanDuration {
    Permanent,
    Temporary(Duration),
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
        self.server_groups.write(dst)
    }
}
