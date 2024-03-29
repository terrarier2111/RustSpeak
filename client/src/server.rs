use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;
use bytes::Buf;
use dashmap::DashMap;
use opus::{Application, Channels, Decoder, Encoder};
use swap_arc::SwapArc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::{AddressMode, Channel, Client, ClientConfig, ClientPacket, NetworkClient, Profile, PROTOCOL_VERSION, RWBytes};
use crate::audio::{AudioMode, SAMPLE_RATE};
use crate::data_structures::byte_buf_ring::BBRing;
use crate::data_structures::conc_once_cell::ConcurrentOnceCell;
use crate::ui::InterUiMessage;
use crate::packet::{AuthResponse, ChannelSubClientUpdate, ChannelSubUpdate, ChannelUpdate, GroupPerms, RemoteProfile, ServerPacket};
use crate::protocol::UserUuid;

pub struct Server {
    pub profile: Profile,
    pub connection: ConcurrentOnceCell<Arc<NetworkClient>>,
    pub default_channel: ConcurrentOnceCell<Uuid>,
    pub channels: SwapArc<HashMap<Uuid, Channel>>,
    pub channels_by_name: SwapArc<HashMap<String, Uuid>>, // FIXME: maintain this!
    pub groups: DashMap<Uuid, Arc<ServerGroup>>,
    pub clients: DashMap<UserUuid, ConnectedRemoteProfile>,
    pub state: ServerState,
    pub name: String,
    pub audio: Option<Arc<ServerAudio>>,
}

pub struct ServerAudio {
    pub buffer: BBRing<2>,
    encoder: std::sync::Mutex<Encoder>,
    decoder: Mutex<Decoder>,
}

impl ServerAudio {

    pub fn encode(&self, input: &[i16], output: &mut Vec<u8>) -> usize {
        let mut encoder = self.encoder.lock().unwrap();
        loop {
            // println!("input len: {}", input.len());
            // println!("output len: {}", output.len());
            match encoder.encode(input, output) {
                Ok(len) => {
                    return len;
                }
                Err(err) => {
                    println!("err: {}", err);
                    output.resize(output.capacity().max(8) * 2, 0);
                }
            }
        }
    }

}

impl Server {
    pub fn new(client: Arc<Client>, profile: Profile, address_mode: AddressMode,
               config: ClientConfig,
               server_addr: SocketAddr,
               server_name: String) -> Arc<Self> {
        let channels = client.audio.load().as_ref().map(|audio| match audio.config().get().0.unwrap() {
            AudioMode::Mono => Channels::Mono,
            AudioMode::Stereo => Channels::Stereo,
        });
        let server = Arc::new(Self {
            profile: profile.clone(),
            connection: ConcurrentOnceCell::new(),
            default_channel: ConcurrentOnceCell::new(),
            channels: Default::default(),
            channels_by_name: Default::default(),
            groups: DashMap::new(),
            clients: Default::default(),
            state: ServerState::new(),
            name: server_name.clone(),
            audio: channels.map(|channels| Arc::new(ServerAudio {
                buffer: BBRing::new(8096),
                encoder: std::sync::Mutex::new(Encoder::new(SAMPLE_RATE, channels, Application::Voip).unwrap()),
                decoder: Mutex::new(Decoder::new(SAMPLE_RATE, channels).unwrap()),
            })),
        });

        let priv_key = profile.private_key();
        let pub_key = priv_key.public_key_to_der().expect("The profile's cryptographic key is invalid");

        let result = server.clone();
        tokio::spawn(async move {
            match NetworkClient::new(address_mode, config, server_addr, server_name.as_str()).await {
                Ok(network_client) => {
                    server.connection.try_init_silent(Arc::new(network_client)).unwrap();

                    server.state.try_set_auth();

                    // now we have to handle auth stuff
                    let auth_packet = ClientPacket::AuthRequest {
                        protocol_version: PROTOCOL_VERSION,
                        pub_key,
                        name: profile.name,
                        security_proofs: profile.security_proofs,
                        signed_data: vec![], // FIXME: sign current time!
                    };
                    let mut buf = auth_packet.encode().unwrap();
                    let tmp_server = server
                        .connection.get();
                    tmp_server.unwrap()
                        .send_reliable(&mut buf)
                        .await
                        .unwrap();
                    let tmp_server = server.clone();

                    // setup packet reader
                    tokio::spawn(async move {
                        let server = tmp_server;
                        'end: loop {
                            let tmp_conn = server.connection.get();
                            match tmp_conn.unwrap().read_reliable(8).await {
                                Ok(mut size) => {
                                    println!("got packet header!");
                                    let size = size.get_u64_le();
                                    let mut payload = match tmp_conn.as_ref().unwrap().read_reliable(size as usize).await {
                                        Ok(payload) => payload,
                                        Err(err) => {
                                            server.error(err, &client).await;
                                            break 'end;
                                        }
                                    };
                                    let packet = ServerPacket::read(&mut payload);
                                    match packet {
                                        Ok(packet) => {
                                            println!("handle packet: {:?}", packet);
                                            handle_packet(packet, &client, &server).await;
                                        }
                                        Err(err) => {
                                            server.error(err, &client).await;
                                            break 'end;
                                        }
                                    }
                                }
                                Err(err) => {
                                    server.error(err, &client).await;
                                    break 'end;
                                }
                            }
                        }
                    });
                }
                Err(_) => {
                    client.inter_ui_msg_queue.send(InterUiMessage::Error(server.clone(), format!("Failed connecting with \"{}\"", &server_name)));
                }
            }

        });
        result
    }

    pub async fn error(&self, err: anyhow::Error, client: &Arc<Client>) {
        if self.state.try_set_disconnected() {
            // FIXME: somehow give feedback to server
            
            // ignore errors happening on close
            let _ = self.connection.get().unwrap().close().await;
            client.println(format!("An error occurred in the connection with {}: {}", self.name, err).as_str());
        }
    }

    pub async fn finish_auth(self: &Arc<Self>, client: Arc<Client>) {
        self.state.try_set_connected();

        let this = self.clone();
        let tmp_client = client.clone();
        self.connection.get().unwrap().start_do_keep_alive(Duration::from_millis(250), move |err| {
            pollster::block_on(this.error(err, &tmp_client)); // FIXME: make this async!
        }).await.unwrap();

        let tmp_server = self.clone();
        let tmp_client = client.clone();

        // set up receiver task
        tokio::spawn(async move {
            let server = tmp_server;
            let client = tmp_client;
            let mut buffer = [0; 2048];
            let mut buf_len = 0;
            loop {
                let tmp_server = server.connection.get();
                match tmp_server.unwrap().read_unreliable().await {
                    Ok(data) => {
                        println!("received voice traffic {}", data.len());
                        if data.len() + buf_len * 2 > buffer.len() * 2 {
                            panic!("Buffer too small ({}) present but ({}) required", buffer.len(), data.len() / 2 + buf_len);
                        }
                        // FIXME: wait for enough frames to arrive!
                        // let data = unsafe { slice_from_raw_parts_mut::<i16>(data.cast::<i16>(), len / 2).as_mut().unwrap() };
                        unsafe { buffer.as_mut_ptr().add(buf_len).cast::<u8>().copy_from_nonoverlapping(data.as_ptr(), data.len()); }
                        buf_len += data.len() / 2;
                        // let mut data = bytemuck::cast_slice_mut::<u8, i16>(data);

                        /*gith
                        
                        let handler = |buf: &mut [i16], info| {
                            if buf.len() != data.len() {
                                panic!("data length {} doesn't match buf length {}", data.len(), buf.len());
                            }
                            for i in 0..(buf.len()) {
                                buf[i] = data[i];
                            }
                        };*/

                        if let Some(audio) = server.audio.as_ref() {
                            if let Ok(len) = audio.decoder.lock().await.decode(data.as_ref(), &mut buffer, false) {
                                // println!("decoded voice traffic {}", len);
                                client.audio.load().as_ref().unwrap().play_back(move |buf, info| {
                                    if buf.len() != len {
                                        // panic!("data length {} doesn't match buf length {}", len, buf.len());
                                        // FIXME: we should probably skip the waiting period below!
                                        return;
                                    }
                                    for i in 0..(buf.len()) {
                                        buf[i] = buffer[i];
                                    }
                                }).unwrap();
                                buf_len = 0;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(err) => {
                        if server.state.try_set_disconnected() {
                            // FIXME: somehow give feedback to server
                            tmp_server.as_ref().unwrap().close().await;
                            client.println(format!("An error occurred in the connection with {}: {:?}", server.name, err).as_str());
                        }
                    }
                }
            }
        });
    }

}

pub async fn handle_packet(packet: ServerPacket<'_>, client: &Arc<Client>, server: &Arc<Server>) {
    match packet {
        ServerPacket::AuthResponse(response) => {
            match response {
                AuthResponse::Success { channels, default_channel_id, server_groups, own_groups } => {
                    let mut channels_by_uuid = HashMap::new();
                    let mut channels_by_name = HashMap::new();
                    for channel in channels {
                        for user in &channel.clients {
                            server.clients.insert(user.key().clone(), ConnectedRemoteProfile {
                                name: user.value().name.clone(),
                                uuid: user.value().uuid.clone(),
                                server_groups: user.value().server_groups.clone(),
                                channel: channel.id.clone(),
                            });
                        }
                        channels_by_name.insert(channel.name.clone(), channel.id);
                        channels_by_uuid.insert(channel.id, channel);
                    }
                    server.channels.store(Arc::new(channels_by_uuid));
                    server.channels_by_name.store(Arc::new(channels_by_name));
                    server.default_channel.try_init_silent(default_channel_id).unwrap();
                    for group in server_groups {
                        server.groups.insert(group.uuid, Arc::new(ServerGroup {
                            uuid: group.uuid,
                            name: SwapArc::new(Arc::new(group.name.into_owned())),
                            priority: AtomicU64::new(group.priority),
                            perms: RwLock::new(group.perms.clone()),
                        }));
                    }
                    server.finish_auth(client.clone()).await;
                    client.inter_ui_msg_queue.send(InterUiMessage::ServerConnected(server.clone()));
                }
                AuthResponse::Failure(failure) => {
                    client.inter_ui_msg_queue.send(InterUiMessage::Error(server.clone(), match failure {
                        crate::packet::AuthFailure::Banned { reason, duration } => format!("You are banned reason: {} for {}", reason, match duration {
                            crate::packet::BanDuration::Permanent => String::from("permanent"),
                            crate::packet::BanDuration::Temporary(time) => {
                                const MINUTE: u64 = 60;
                                const HOUR: u64 = MINUTE * 60;
                                const DAY: u64 = HOUR * 24;
                                const YEAR: u64 = DAY * 365;

                                let raw_secs = time.as_secs();
                                let secs = raw_secs % MINUTE;
                                let minutes = raw_secs % HOUR / MINUTE;
                                let hours = raw_secs % DAY / HOUR;
                                let days = raw_secs % YEAR / DAY;
                                let years = raw_secs / YEAR;
                                format!("{}{}{}{}{}",
                                if secs > 0 { format!("{} seconds", secs) } else { String::new() },
                                if minutes > 0 { format!("{} minutes", minutes) } else { String::new() },
                                if hours > 0 { format!("{} hours", hours) } else { String::new() },
                                if days > 0 { format!("{} days", days) } else { String::new() },
                                if years > 0 { format!("{} years", years) } else { String::new() })
                            },
}),
                        crate::packet::AuthFailure::ReqSec(level) => format!("This server requires a security level of {}", level),
                        crate::packet::AuthFailure::OutOfDate(_) => todo!(),
                        crate::packet::AuthFailure::AlreadyOnline => String::from("You are already online"),
                        crate::packet::AuthFailure::Invalid(reason) => reason.to_string(),
                    }));
                }
            }
        }
        ServerPacket::ChannelUpdate(update) => {
            match update {
                ChannelUpdate::Create(channel) => {
                    let mut channels = server.channels.load().as_ref().clone();
                    channels.insert(channel.id, channel);
                    server.channels.store(Arc::new(channels));
                    // FIXME: update screen
                }
                ChannelUpdate::SubUpdate { channel, update } => {
                    match update {
                        ChannelSubUpdate::Name(name) => {
                            let mut channels = server.channels.load().as_ref().clone();
                            let mut prev_channel = channels.get(&channel).unwrap().clone();
                            prev_channel.name = name.to_string();
                            channels.insert(channel, prev_channel);
                            server.channels.store(Arc::new(channels));
                        }
                        ChannelSubUpdate::Desc(desc) => {
                            let mut channels = server.channels.load().as_ref().clone();
                            let mut prev_channel = channels.get(&channel).unwrap().clone();
                            prev_channel.desc = desc.to_string();
                            channels.insert(channel, prev_channel);
                            server.channels.store(Arc::new(channels));
                        }
                        ChannelSubUpdate::Perms(perms) => {
                            let mut channels = server.channels.load().as_ref().clone();
                            let mut prev_channel = channels.get(&channel).unwrap().clone();
                            prev_channel.perms = perms;
                            channels.insert(channel, prev_channel);
                            server.channels.store(Arc::new(channels));
                        }
                        ChannelSubUpdate::Client(update) => {
                            match update {
                                ChannelSubClientUpdate::Add(user) => {
                                    let profile = server.clients.get_mut(&user).map(|mut val| {
                                        val.value_mut().channel = channel;
                                        val
                                    }).unwrap().clone();
                                    let profile = RemoteProfile {
                                        name: profile.name,
                                        uuid: profile.uuid,
                                        server_groups: profile.server_groups,
                                    };
                                    server.channels.load().as_ref().get(&channel).unwrap().clients.insert(user, profile.clone());
                                    client.inter_ui_msg_queue.send(InterUiMessage::ChannelAddUser(server.clone(), channel, profile));
                                }
                                ChannelSubClientUpdate::Remove(user) => {
                                    server.channels.load().as_ref().get(&channel).unwrap().clients.remove(&user).unwrap().1;
                                    client.inter_ui_msg_queue.send(InterUiMessage::ChannelRemoveUser(server.clone(), channel, user));
                                }
                            }
                        }
                    }
                }
                ChannelUpdate::Delete(channel) => {
                    let mut channels = server.channels.load().as_ref().clone();
                    channels.remove(&channel);
                    server.channels.store(Arc::new(channels));
                    // FIXME: update screen
                }
            }
        }
        ServerPacket::ClientConnected(profile) => {
            let default_channel = server.default_channel.get().cloned().unwrap();
            server.channels.load().get(&default_channel).unwrap().clients.insert(profile.uuid.clone(), profile.clone());
            server.clients.insert(profile.uuid.clone(), ConnectedRemoteProfile {
                name: profile.name.clone(),
                uuid: profile.uuid.clone(),
                server_groups: profile.server_groups.clone(),
                channel: default_channel.clone(),
            });
            client.inter_ui_msg_queue.send(InterUiMessage::ChannelAddUser(server.clone(), default_channel, profile));
        }
        ServerPacket::ClientDisconnected(profile) => {
            let client_profile = server.clients.remove(&profile.uuid).unwrap().1;
            server.channels.load().get(&client_profile.channel).unwrap().clients.insert(profile.uuid.clone(), profile);
            client.inter_ui_msg_queue.send(InterUiMessage::ChannelRemoveUser(server.clone(), client_profile.channel, client_profile.uuid));
        }
        ServerPacket::ClientUpdateServerGroups { client, update } => {
            // server.clients.get(&client).unwrap().server_groups
        }
        ServerPacket::KeepAlive { .. } => {}
        ServerPacket::ChallengeRequest { .. } => {}
        ServerPacket::ForceDisconnect { reason } => {
            todo!()
        }
        ServerPacket::SwitchChannelResponse(_) => {}, // FIXME: use this!
    }
}

const STATE_PENDING: u8 = 0;
const STATE_AUTH: u8 = 1;
const STATE_CONNECTED: u8 = 2;
const STATE_DISCONNECTED: u8 = 3;

pub struct ServerState(AtomicU8);

impl ServerState {

    const fn new() -> Self {
        Self(AtomicU8::new(STATE_PENDING))
    }

    pub fn try_set_auth(&self) -> bool {
        self.0.compare_exchange(STATE_PENDING, STATE_AUTH, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    pub fn try_set_connected(&self) -> bool {
        self.0.compare_exchange(STATE_AUTH, STATE_CONNECTED, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    pub fn try_set_disconnected(&self) -> bool {
        // this should be able to happen with any previous connection state except disconnected.
        self.0.swap(STATE_DISCONNECTED, Ordering::AcqRel) != STATE_DISCONNECTED
    }

    pub fn is_connected(&self) -> bool {
        self.0.load(Ordering::Acquire) == STATE_CONNECTED
    }

}

#[derive(Debug, Clone)]
pub struct ConnectedRemoteProfile {
    pub name: String,
    pub uuid: UserUuid,
    pub server_groups: Vec<Uuid>,
    pub channel: Uuid,
}

pub struct ServerGroup {
    pub uuid: Uuid,
    pub name: SwapArc<String>,
    pub priority: AtomicU64,
    pub perms: RwLock<GroupPerms>,
}
