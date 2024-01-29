use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
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
use crate::packet::{AuthResponse, ChannelSubClientUpdate, ChannelSubUpdate, ChannelUpdate, RemoteProfile, ServerPacket};
use crate::protocol::UserUuid;

pub struct Server {
    pub profile: Profile,
    pub connection: ConcurrentOnceCell<Arc<NetworkClient>>,
    pub default_channel: ConcurrentOnceCell<Uuid>,
    pub channels: SwapArc<HashMap<Uuid, Channel>>,
    pub channels_by_name: SwapArc<HashMap<String, Uuid>>, // FIXME: maintain this!
    pub clients: DashMap<UserUuid, ConnectedRemoteProfile>,
    pub state: ServerState,
    pub name: String,
    pub audio: Arc<ServerAudio>,
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
        let channels = match client.audio.load().config().get().0.unwrap() {
            AudioMode::Mono => Channels::Mono,
            AudioMode::Stereo => Channels::Stereo,
        };
        let server = Arc::new(Self {
            profile: profile.clone(),
            connection: ConcurrentOnceCell::new(),
            default_channel: ConcurrentOnceCell::new(),
            channels: Default::default(),
            channels_by_name: Default::default(),
            clients: Default::default(),
            state: ServerState::new(),
            name: server_name.clone(),
            audio: Arc::new(ServerAudio {
                buffer: BBRing::new(8096),
                encoder: std::sync::Mutex::new(Encoder::new(SAMPLE_RATE, channels, Application::Voip).unwrap()),
                decoder: Mutex::new(Decoder::new(SAMPLE_RATE, channels).unwrap()),
            }),
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
                    /*let auth_packet = ClientPacket::AuthRequest {
                        protocol_version: PROTOCOL_VERSION,
                        pub_key: vec![],
                        name: "TESTING".to_string(),
                        security_proofs: vec![],
                        signed_data: vec![], // FIXME: sign current time!
                    };*/
                    let mut buf = auth_packet.encode().unwrap();
                    let tmp_server = server
                        .connection.get();
                    tmp_server.unwrap()
                        .send_reliable(&mut buf)
                        .await
                        .unwrap();
                    let tmp_server = server.clone();
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
                    // client.screen_sys.push_screen(Box::new(ConnectionFailureScreen::new(&client, server_name.to_string())));
                    client.inter_ui_msg_queue.send(InterUiMessage::Error(format!("Failed connecting with \"{}\"", &server_name)));
                }
            }

        });
        result
    }

    pub async fn error(&self, err: anyhow::Error, client: &Arc<Client>) {
        if self.state.try_set_disconnected() {
            // FIXME: somehow give feedback to server
            self.connection.get().unwrap().close().await.unwrap();
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
        /*tokio::spawn(async move { // FIXME: is this okay perf-wise?
            let server = tmp_server;
            let client = tmp_client;
            loop {
                let tmp_server = server.connection.get();
                match tmp_server.unwrap().read_unreliable().await {
                    Ok(data) => {
                        println!("received voice traffic {}", data.len());
                        let mut data_vec = data.to_vec();
                        let len = data_vec.len();
                        let data = if data_vec.as_ptr().is_aligned_to(2) {
                            data_vec.deref_mut()
                        } else {
                            // realloc to make the allocation aligned to 2 bytes
                            let mut new_alloc = unsafe { alloc(Layout::array::<u16>(len).unwrap()) }; // FIXME: dealloc this again later on.
                            if !new_alloc.is_null() {
                                unsafe {
                                    ptr::copy_nonoverlapping(data_vec.as_ptr(), new_alloc, len);
                                }
                                unsafe { slice::from_raw_parts_mut(new_alloc, len) }
                            } else {
                                unreachable!()
                            }
                        };
                        // let data = unsafe { slice_from_raw_parts_mut::<i16>(data.cast::<i16>(), len / 2).as_mut().unwrap() };
                        client.audio.load_full().buffer.push(data);
                        // thread::sleep(Duration::from_millis(60));
                        tokio::time::sleep(Duration::from_millis(60)).await;
                        // let mut data = bytemuck::cast_slice_mut::<u8, i16>(data);

        /*
        let handler = |buf: &mut [i16], info| {
            if buf.len() != data.len() {
                panic!("data length {} doesn't match buf length {}", data.len(), buf.len());
            }
            for i in 0..(buf.len()) {
                buf[i] = data[i];
            }
        };*/
                    }
                    Err(err) => {
                        if server.state.try_set_disconnected() {
                            // FIXME: somehow give feedback to server
                            tmp_server.as_ref().unwrap().close().await;
                            client.println(format!("An error occurred in the connection with {}: {}", server.name, err).as_str());
                        }
                    }
                }
            }
        });
        let client = client.clone();
        thread::spawn(move || {
            let client = client.clone();
            loop {
                let audio = client.audio.load();
                if let Some(raw_data) = audio.buffer.pop_front() {
                    let tmp_client = client.clone();
                    audio.as_ref().play_back(move |buf, info| {
                        let client = &tmp_client;
                        let data = bytemuck::cast_slice::<u8, i16>(raw_data.as_ref());
                        if buf.len() < data.as_ref().len() {
                            client.audio.load().buffer.push(&raw_data.as_ref()[(buf.len() * 2)..]);
                        }
                        for i in 0..(data.as_ref().len().min(buf.len())) {
                            buf[i] = data[i];
                        }
                    }).unwrap();
                }
                thread::sleep(Duration::from_millis(60));
                // tokio::time::sleep(Duration::from_millis(60)).await;
            }
        });*/
        tokio::spawn(async move { // FIXME: is this okay perf-wise?
            let server = tmp_server;
            let client = tmp_client;
            let mut buffer = [0; 2048];
            let mut buf_len = 0;
            loop {
                let tmp_server = server.connection.get();
                match tmp_server.unwrap().read_unreliable().await {
                    Ok(data) => {
                        // println!("received voice traffic {}", data.len());
                        if data.len() + buf_len * 2 > buffer.len() * 2 {
                            panic!("Buffer too small ({}) present but ({}) required", buffer.len(), data.len() / 2 + buf_len);
                        }
                        // FIXME: wait for enough frames to arrive!
                        // let data = unsafe { slice_from_raw_parts_mut::<i16>(data.cast::<i16>(), len / 2).as_mut().unwrap() };
                        unsafe { buffer.as_mut_ptr().add(buf_len).cast::<u8>().copy_from_nonoverlapping(data.as_ptr(), data.len()); }
                        buf_len += data.len() / 2;
                        // let mut data = bytemuck::cast_slice_mut::<u8, i16>(data);

                        /*
                        let handler = |buf: &mut [i16], info| {
                            if buf.len() != data.len() {
                                panic!("data length {} doesn't match buf length {}", data.len(), buf.len());
                            }
                            for i in 0..(buf.len()) {
                                buf[i] = data[i];
                            }
                        };*/

                        if let Ok(len) = server.audio.decoder.lock().await.decode(data.as_ref(), &mut buffer, false) {
                            // println!("decoded voice traffic {}", len);
                            client.audio.load().as_ref().play_back(move |buf, info| {
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
                        // thread::sleep(Duration::from_millis(60));
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
        /*ClientPacket::AuthRequest { .. } => unreachable!(),
        ClientPacket::Disconnect => {
            server.online_users.remove(client.uuid.load().as_ref().unwrap()); // FIXME: verify that this can't be received before AuthRequest is handled!
        }
        ClientPacket::KeepAlive { .. } => {

        }
        ClientPacket::UpdateClientServerGroups { .. } => {}*/
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
                    server.finish_auth(client.clone()).await;
                    client.inter_ui_msg_queue.send(InterUiMessage::ServerConnected);
                }
                AuthResponse::Failure(_) => {
                    // FIXME: do error screen!
                }
            }
        }
        ServerPacket::ChannelUpdate(update) => {
            match update {
                ChannelUpdate::Create(_) => {}
                ChannelUpdate::SubUpdate { channel, update } => {
                    match update {
                        ChannelSubUpdate::Name(_) => {}
                        ChannelSubUpdate::Desc(_) => {}
                        ChannelSubUpdate::Perms(_) => {}
                        ChannelSubUpdate::Client(update) => {
                            match update {
                                ChannelSubClientUpdate::Add(user) => {
                                    let profile = client.server.load().as_ref().unwrap().clients.get_mut(&user).map(|mut val| {
                                        val.value_mut().channel = channel;
                                        val
                                    }).unwrap().clone();
                                    let profile = RemoteProfile {
                                        name: profile.name,
                                        uuid: profile.uuid,
                                        server_groups: profile.server_groups,
                                    };
                                    client.server.load().as_ref().unwrap().channels.load().as_ref().get(&channel).unwrap().clients.insert(user, profile.clone());
                                    client.inter_ui_msg_queue.send(InterUiMessage::ChannelAddUser(channel, profile));
                                }
                                ChannelSubClientUpdate::Remove(user) => {
                                    client.server.load().as_ref().unwrap().channels.load().as_ref().get(&channel).unwrap().clients.remove(&user).unwrap().1;
                                    client.inter_ui_msg_queue.send(InterUiMessage::ChannelRemoveUser(channel, user));
                                }
                            }
                        }
                    }
                }
                ChannelUpdate::Delete(_) => {}
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
            client.inter_ui_msg_queue.send(InterUiMessage::ChannelAddUser(default_channel, profile));
        }
        ServerPacket::ClientDisconnected(profile) => {
            let client_profile = server.clients.remove(&profile.uuid).unwrap().1;
            server.channels.load().get(&client_profile.channel).unwrap().clients.insert(profile.uuid.clone(), profile);
            client.inter_ui_msg_queue.send(InterUiMessage::ChannelRemoveUser(client_profile.channel, client_profile.uuid));
        }
        ServerPacket::ClientUpdateServerGroups { .. } => {}
        ServerPacket::KeepAlive { .. } => {}
        ServerPacket::ChallengeRequest { .. } => {}
        ServerPacket::ForceDisconnect { reason } => {
            todo!()
        }
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
