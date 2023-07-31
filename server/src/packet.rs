use crate::protocol::{ErrorEnumVariantNotFound, RWBytes, RWBytesMut, UserUuid};
use bytemuck::Pod;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use openssl::hash::MessageDigest;
use openssl::pkey::{PKeyRef, Public};
use openssl::rsa::{Padding, Rsa};
use openssl::sign::Verifier;
use ordinalizer::Ordinal;
use ruint::aliases::U256;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::{Debug, Display};
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use swap_arc::SwapArc;
use uuid::Uuid;

const PRIVATE_KEY_LEN_BITS: u32 = 4096;

/// packets the server sends to the client
/// size: u64
/// id: u8
#[repr(u8)]
#[derive(Ordinal)]
pub enum ServerPacket<'a> {
    AuthResponse(AuthResponse<'a>) = 0,
    ChannelUpdate(ChannelUpdate<'a>) = 1,
    ClientConnected(RemoteProfile) = 2,
    ClientDisconnected(RemoteProfile) = 3,
    ClientUpdateServerGroups {
        client: UserUuid,
        update: ClientUpdateServerGroups,
    } = 4,
    KeepAlive {
        id: u64,
        send_time: Duration,
    } = 5,
    ChallengeRequest {
        signed_data: Vec<u8>, // contains the public server key and a random challenge and all of that encrypted with the client's public key
    } = 6,
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
        // auth_kind: ,
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
    pub fn decode(buf: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self> {
        let _len = buf.get_u64_le(); // FIXME: use this and don't trust the client in any way shape or form!
        Self::read(buf, client_key)
    }
}

impl ServerPacket<'_> {
    pub fn encode(&self) -> anyhow::Result<BytesMut> {
        let mut tmp_buf = BytesMut::new();
        self.write(&mut tmp_buf)?;
        let mut result_buf = BytesMut::with_capacity(8 + tmp_buf.len());
        result_buf.put_u64_le(tmp_buf.len() as u64);
        result_buf.put(tmp_buf);

        Ok(result_buf)
    }
}

impl RWBytes for ServerPacket<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let id = src.get_u8();

        match id {
            0 => Ok(Self::AuthResponse(AuthResponse::read(src, client_key)?)),
            1 => Ok(Self::ChannelUpdate(ChannelUpdate::read(src, client_key)?)),
            2 => Ok(Self::ClientConnected(RemoteProfile::read(src, client_key)?)),
            3 => Ok(Self::ClientDisconnected(RemoteProfile::read(
                src, client_key,
            )?)),
            4 => {
                let client = UserUuid::read(src, client_key)?;
                let update = ClientUpdateServerGroups::read(src, client_key)?;
                Ok(Self::ClientUpdateServerGroups { client, update })
            }
            5 => {
                let id = u64::read(src, client_key)?;
                let send_time = Duration::read(src, client_key)?;
                Ok(Self::KeepAlive { id, send_time })
            }
            6 => {
                todo!()
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
        }
        Ok(())
    }
}

impl RWBytes for ClientPacket {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let id = src.get_u8();
        match id {
            0 => {
                let protocol_version = u64::read(src, client_key)?;
                let pub_key = Vec::<u8>::read(src, client_key)?;
                let name = String::read(src, client_key)?;
                println!("got name: {}", name);
                let security_proofs = Vec::<U256>::read(src, client_key)?;
                let signed_data = Vec::<u8>::read(src, client_key)?;
                println!("got signed data: {:?}", signed_data);
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
                let send_time = Duration::read(src, client_key)?;
                Ok(Self::KeepAlive { id, send_time })
            }
            3 => {
                let client = UserUuid::read(src, client_key)?;
                let update = ClientUpdateServerGroups::read(src, client_key)?;
                Ok(Self::UpdateClientServerGroups { client, update })
            }
            4 => {
                todo!()
            }
            5 => {
                let channel = Uuid::read(src, client_key)?;
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
                name,
                pub_key,
                security_proofs,
                signed_data,
            } => {
                dst.put_u64_le(*protocol_version);
                name.write(dst)?;
                pub_key.write(dst)?;
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
            ClientPacket::ChallengeResponse { .. } => {
                todo!()
            }
            ClientPacket::SwitchChannel { channel } => {
                channel.write(dst)?;
            }
        }
        Ok(())
    }
}

pub(crate) struct Encrypted<T: RWBytes + Pod> {
    pub data: T,
    pub pub_key: Rsa<Public>,
}

impl<T: RWBytes + Pod> RWBytes for Encrypted<T> {
    type Ty = T;

    fn read(_src: &mut Bytes, _client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        todo!()
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        // SAFETY: This is safe because all bit patterns are valid for u8
        let mut buf = unsafe {
            Box::new_zeroed_slice(PRIVATE_KEY_LEN_BITS.div_ceil(8) as usize).assume_init()
        };
        self.pub_key.public_encrypt(
            bytemuck::bytes_of(&self.data),
            &mut buf,
            Padding::PKCS1_OAEP,
        )?;
        buf.into_vec().write(dst)?; // FIXME: optimize writing u8 arrays to the network
        Ok(())
    }
}

pub(crate) struct Signed<T: RWBytes>
where
    T::Ty: Pod,
{
    pub data: T,
    pub pub_key: PKeyRef<Public>,
}

impl<T: RWBytes> RWBytes for Signed<T>
where
    T::Ty: Pod,
{
    type Ty = T::Ty;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let signature = Vec::<u8>::read(src, client_key)?;
        let data = T::read(src, client_key)?;
        let mut verifier = Verifier::new(MessageDigest::sha256(), client_key.unwrap())?;
        verifier.verify_oneshot(&signature, bytemuck::bytes_of(&data))?;
        Ok(data)
    }

    fn write(&self, _dst: &mut BytesMut) -> anyhow::Result<()> {
        // we don't ever need to write Signed data, we only need to read signed data on the server side
        unimplemented!()
    }
}

#[derive(Ordinal, Copy, Clone)]
pub enum AuthKind {
    Text, // Text connection
    Full, // Voice and text connection
}

#[derive(Ordinal)]
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

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let channel = Channel::read(src, client_key)?;
                Ok(Self::Create(channel))
            }
            1 => {
                let channel = Uuid::read(src, client_key)?;
                let update = ChannelSubUpdate::read(src, client_key)?;
                Ok(Self::SubUpdate { channel, update })
            }
            2 => {
                let uuid = Uuid::read(src, client_key)?;
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

#[derive(Ordinal)]
pub enum ChannelSubUpdate<'a> {
    Name(Cow<'a, String>),
    Desc(Cow<'a, String>),
    Perms(ChannelPerms),
    Client(ChannelSubClientUpdate),
}

impl RWBytes for ChannelSubUpdate<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let name = Cow::<String>::read(src, client_key)?;
                Ok(Self::Name(name))
            }
            1 => {
                let desc = Cow::<String>::read(src, client_key)?;
                Ok(Self::Desc(desc))
            }
            2 => {
                let channel_perms = ChannelPerms::read(src, client_key)?;
                Ok(Self::Perms(channel_perms))
            }
            3 => {
                let update = ChannelSubClientUpdate::read(src, client_key)?;
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
    Add(UserUuid), // FIXME: we have to ensure that all updates get flushed if there is any way the receiving client
    // FIXME: could not have a (up-to-date) client with the passed uuid in their database
    Remove(UserUuid),
}

impl RWBytes for ChannelSubClientUpdate {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let uuid = UserUuid::read(src, client_key)?;
                Ok(Self::Add(uuid))
            }
            1 => {
                let uuid = UserUuid::read(src, client_key)?;
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

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();
        match disc {
            0 => {
                let uuid = Uuid::read(src, client_key)?;
                Ok(ClientUpdateServerGroups::Add(uuid))
            }
            1 => {
                let uuid = Uuid::read(src, client_key)?;
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

#[derive(Clone)]
pub struct RemoteProfile {
    pub name: String,
    pub uuid: UserUuid,
    pub server_groups: Vec<Uuid>,
}

impl RWBytes for RemoteProfile {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let name = String::read(src, client_key)?;
        let uuid = UserUuid::read(src, client_key)?;
        let server_groups = Vec::<Uuid>::read(src, client_key)?;

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
    pub uuid: Uuid,
    pub password: AtomicBool,
    // pub hide_users_if_pw: AtomicBool, // FIXME: add capability to hide users if a password is set
    pub name: Arc<SwapArc<String>>,
    pub desc: Arc<SwapArc<String>>,
    pub perms: Arc<SwapArc<ChannelPerms>>,
    pub clients: Arc<tokio::sync::RwLock<Vec<UserUuid>>>, // FIXME: try making this lock free!
    pub proto_clients: Arc<RwLock<Vec<RemoteProfile>>>, // FIXME: is it worth making RemoteProfiles ref-counted?
    pub slots: AtomicI16,
    pub sort_id: AtomicU16,
}

// FIXME: use Arc<Channel<'_>> so that we don't need a clone impl for Channel<'_>
impl Clone for Channel {
    fn clone(&self) -> Self {
        Self {
            uuid: self.uuid,
            password: AtomicBool::new(self.password.load(Ordering::Acquire)),
            name: self.name.clone(),
            desc: self.desc.clone(),
            perms: self.perms.clone(),
            clients: self.clients.clone(),
            proto_clients: self.proto_clients.clone(),
            slots: AtomicI16::new(self.slots.load(Ordering::Acquire)),
            sort_id: AtomicU16::new(self.sort_id.load(Ordering::Acquire)),
        }
    }
}

impl<T: RWBytes<Ty = T> + Send + Sync> RWBytes for SwapArc<T> {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        Ok(SwapArc::new(Arc::new(T::read(src, client_key)?)))
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.load().write(dst)
    }
}

impl RWBytes for Channel {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let uuid = Uuid::read(src, client_key)?;
        let password = AtomicBool::new(bool::read(src, client_key)?);
        let name = Arc::new(SwapArc::new(Arc::new(String::read(src, client_key)?)));
        let desc = Arc::new(SwapArc::new(Arc::new(String::read(src, client_key)?)));
        let perms = Arc::new(SwapArc::new(Arc::new(ChannelPerms::read(src, client_key)?)));
        let clients = Arc::new(RwLock::new(Vec::<RemoteProfile>::read(src, client_key)?));
        let slots = AtomicI16::new(i16::read(src, client_key)?);
        let sort_id = AtomicU16::new(u16::read(src, client_key)?);

        Ok(Self {
            uuid,
            password,
            name,
            desc,
            perms,
            proto_clients: clients,
            clients: Arc::new(tokio::sync::RwLock::new(vec![])),
            slots,
            sort_id,
        })
    }

    fn write(&self, dst: &mut BytesMut) -> anyhow::Result<()> {
        self.uuid.write(dst)?;
        self.password.write(dst)?;
        RWBytes::write(&self.name, dst)?;
        RWBytes::write(&self.desc, dst)?;
        RWBytes::write(&self.perms, dst)?;
        RWBytes::write(&self.proto_clients, dst)?;
        self.slots.write(dst)?;
        self.sort_id.write(dst)?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelPerms {
    pub(crate) see: u64, // every channel one can see is automatically subscribed to
    // pub(crate) subscribe: u64,
    pub(crate) join: u64,
    pub(crate) modify: u64,
    pub(crate) talk: u64,
    pub(crate) assign_talk: u64,
    pub(crate) delete: u64, // this might be useful for regulating bots for example
                            // kicking is handled simply as a move into the default channel
}

impl Default for ChannelPerms {
    fn default() -> Self {
        Self {
            see: 0,
            join: 0,
            modify: 100,
            talk: 0,
            assign_talk: 100,
            delete: 100,
        }
    }
}

impl RWBytes for ChannelPerms {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let see = u64::read(src, client_key)?;
        let join = u64::read(src, client_key)?;
        let modify = u64::read(src, client_key)?;
        let talk = u64::read(src, client_key)?;
        let assign_talk = u64::read(src, client_key)?;
        let delete = u64::read(src, client_key)?;

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

#[derive(Clone)]
pub struct ServerGroup {
    pub uuid: Uuid,
    pub name: String,
    pub priority: u64,
    pub perms: PermsSnapshot,
}

impl RWBytes for ServerGroup {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let uuid = Uuid::read(src, client_key)?;
        let name = String::read(src, client_key)?;
        let priority = u64::read(src, client_key)?;
        let perms = PermsSnapshot::read(src, client_key)?;

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

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PermsSnapshot { // FIXME: snapshot isn't a good name!
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

impl RWBytes for PermsSnapshot {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let server_group_assign = u64::read(src, client_key)?;
        let server_group_unassign = u64::read(src, client_key)?;
        let channel_see = u64::read(src, client_key)?;
        let channel_join = u64::read(src, client_key)?;
        let channel_modify = u64::read(src, client_key)?;
        let channel_talk = u64::read(src, client_key)?;
        let channel_assign_talk = u64::read(src, client_key)?;
        let channel_delete = u64::read(src, client_key)?;
        let can_send = bool::read(src, client_key)?;
        let channel_create = ChannelCreatePerms::read(src, client_key)?;

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

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ChannelCreatePerms {
    pub power: u64,
    pub set_desc: bool,
    pub set_password: bool,
    pub resort_channel: bool,
    // FIXME: add other features that channels have
}

impl RWBytes for ChannelCreatePerms {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let power = u64::read(src, client_key)?;
        let set_desc = bool::read(src, client_key)?;
        let set_password = bool::read(src, client_key)?;
        let resort_channel = bool::read(src, client_key)?;

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

#[derive(Ordinal)]
pub enum AuthResponse<'a> {
    Success {
        default_channel_id: Uuid,
        server_groups: Vec<Arc<ServerGroup>>,
        own_groups: Vec<Uuid>,
        channels: Vec<Channel>,
    },
    Failure(AuthFailure<'a>),
}

impl RWBytes for AuthResponse<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
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
                let default_channel_id = Uuid::read(src, client_key)?;
                let server_groups = Vec::<Arc<ServerGroup>>::read(src, client_key)?;
                let own_groups = Vec::<Uuid>::read(src, client_key)?;
                let channels = Vec::<Channel>::read(src, client_key)?;
                Ok(Self::Success {
                    default_channel_id,
                    server_groups,
                    own_groups,
                    channels,
                })
            }
            1 => {
                let failure = AuthFailure::read(src, client_key)?;
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
    AlreadyOnline,
    Invalid(Cow<'a, str>),
}

impl RWBytes for AuthFailure<'_> {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => {
                let reason = String::read(src, client_key)?;
                let duration = BanDuration::read(src, client_key)?;
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
                let reason = String::read(src, client_key)?;
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

#[derive(Ordinal)]
pub enum BanDuration {
    Permanent,
    Temporary(Duration),
}

impl RWBytes for BanDuration {
    type Ty = Self;

    fn read(src: &mut Bytes, client_key: Option<&PKeyRef<Public>>) -> anyhow::Result<Self::Ty> {
        let disc = src.get_u8();

        match disc {
            0 => Ok(BanDuration::Permanent),
            1 => {
                let dur = Duration::read(src, client_key)?;
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
