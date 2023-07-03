use std::alloc::{alloc, Layout};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::ptr;
use std::ptr::slice_from_raw_parts_mut;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;
use bytes::Buf;
use swap_arc::SwapArc;
use uuid::Uuid;
use crate::{AddressMode, Channel, Client, ClientConfig, ClientPacket, NetworkClient, Profile, PROTOCOL_VERSION, RWBytes};
use crate::conc_once_cell::ConcurrentOnceCell;
use crate::packet::ServerPacket;
use crate::screen::connection_failure::ConnectionFailureScreen;

pub struct Server {
    pub profile: Profile,
    pub connection: ConcurrentOnceCell<Arc<NetworkClient>>,
    pub channels: SwapArc<HashMap<Uuid, Channel>>,
    pub state: ServerState,
    pub name: String,
}

impl Server {
    pub async fn new(client: Arc<Client>, profile: Profile, address_mode: AddressMode,
               config: ClientConfig,
               server_addr: SocketAddr,
               server_name: String) -> anyhow::Result<Arc<Self>> {
        let server = Arc::new(Self {
            profile: profile.clone(),
            connection: ConcurrentOnceCell::new(),
            channels: Default::default(),
            state: ServerState::new(),
            name: server_name.clone(),
        });

        let priv_key = profile.private_key();
        let pub_key = priv_key.public_key_to_der()?;

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
                                            server.error(err, &client);
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
                                            server.error(err, &client);
                                            break 'end;
                                        }
                                    }
                                }
                                Err(err) => {
                                    server.error(err, &client);
                                    break 'end;
                                }
                            }
                        }
                    });
                }
                Err(_) => {
                    client.screen_sys.push_screen(Box::new(ConnectionFailureScreen::new(&client, server_name.to_string())));
                }
            }

        });
        Ok(result)
    }

    pub async fn error(&self, err: anyhow::Error, client: &Arc<Client>) {
        if self.state.try_set_disconnected() {
            // FIXME: somehow give feedback to server
            self.connection.get().unwrap().close().await;
            client.println(format!("An error occurred in the connection with {}: {}", self.name, err).as_str());
        }
    }

    pub async fn finish_auth(self: &Arc<Self>, client: Arc<Client>) {
        self.state.try_set_connected();

        let this = self.clone();
        let tmp_client = client.clone();
        self.connection.get().unwrap().start_do_keep_alive(Duration::from_millis(250), move |err| {
            this.error(err, &tmp_client);
        }).await.unwrap();

        let tmp_server = self.clone();
        let tmp_client = client;
        tokio::spawn(async move { // FIXME: is this okay perf-wise?
            let server = tmp_server;
            let client = tmp_client;
            loop {
                let tmp_server = server.connection.get();
                match tmp_server.unwrap().read_unreliable().await {
                    Ok(data) => {
                        println!("received voice traffic {}", data.len());
                        let mut data_vec = data.to_vec();
                        let len = data_vec.len();
                        let mut data = if data_vec.as_ptr().is_aligned_to(2) {
                            data_vec.as_mut_ptr()
                        } else {
                            // realloc to make the allocation aligned to 2 bytes
                            let mut new_alloc = unsafe { alloc(Layout::array::<u16>(len).unwrap()) };
                            if !new_alloc.is_null() {
                                unsafe {
                                    ptr::copy_nonoverlapping(data_vec.as_ptr(), new_alloc, len);
                                }
                                new_alloc
                            } else {
                                unreachable!()
                            }
                        };
                        let data = unsafe { slice_from_raw_parts_mut::<i16>(data.cast::<i16>(), len / 2).as_mut().unwrap() };
                        // let mut data = bytemuck::cast_slice_mut::<u8, i16>(data);
                        client.audio.load().as_ref().play_back(data).unwrap();
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
        ServerPacket::AuthResponse(_response) => {
            server.finish_auth(client.clone()).await;
        }
        ServerPacket::ChannelUpdate(_) => {}
        ServerPacket::ClientConnected(_) => {}
        ServerPacket::ClientDisconnected(_) => {}
        ServerPacket::ClientUpdateServerGroups { .. } => {}
        ServerPacket::KeepAlive { .. } => {}
        ServerPacket::ChallengeRequest { .. } => {}
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
