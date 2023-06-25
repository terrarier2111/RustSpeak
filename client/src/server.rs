use std::alloc::{alloc, Layout};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ptr;
use std::ptr::slice_from_raw_parts_mut;
use std::sync::Arc;
use std::time::Duration;
use arc_swap::ArcSwap;
use bytes::{Buf, buf};
use openssl::pkey::PKey;
use uuid::Uuid;
use crate::{AddressMode, Channel, Client, ClientConfig, ClientPacket, NetworkClient, Profile, PROTOCOL_VERSION, RWBytes};
use crate::packet::ServerPacket;

pub struct Server {
    pub profile: Profile, // FIXME: make this an ArcSwap instead!
    pub connection: Arc<NetworkClient>, // FIXME: support connecting to multiple servers at once
    pub channels: ArcSwap<HashMap<Uuid, Channel>>,
}

impl Server {
    pub async fn new(client: Arc<Client>, profile: Profile, address_mode: AddressMode,
               config: ClientConfig,
               server: SocketAddr,
               server_name: &str,) -> anyhow::Result<Arc<Self>> {
        let server = Self {
            profile: profile.clone(),
            connection: Arc::new(NetworkClient::new(address_mode, config, server, server_name).await?),
            channels: Default::default(),
        };

        let priv_key = profile.private_key();
        let pub_key = priv_key.public_key_to_der()?;
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
        server
            .connection
            .send_reliable(&mut buf)
            .await
            .unwrap();
        server
            .connection.start_do_keep_alive(Duration::from_millis(250), |err| {
            panic!("{}", err)
        }).await.unwrap();
        let server = Arc::new(server);
        let tmp_server = server.clone();
        let tmp_client = client.clone();
        tokio::spawn(async move {
            let client = tmp_client;
            let server = tmp_server;
            'end: loop {
                match server.connection.read_reliable(8).await {
                    Ok(mut size) => {
                        println!("got packet header!");
                        let size = size.get_u64_le();
                        let mut payload = match server.connection.read_reliable(size as usize).await {
                            Ok(payload) => payload,
                            Err(_err) => {
                                // FIXME: somehow give feedback to console and to server
                                server.connection.close().await;
                                break 'end;
                            }
                        };
                        let packet = ServerPacket::read(&mut payload);
                        match packet {
                            Ok(packet) => {
                                handle_packet(packet, &client, &server);
                            }
                            Err(_err) => {
                                // FIXME: somehow give feedback to console and to server
                                server.connection.close().await;
                                break 'end;
                            }
                        }
                    }
                    Err(_err) => {
                        // FIXME: somehow give feedback to console and to server
                        server.connection.close().await;
                        break 'end;
                    }
                }
            }
        });
        let tmp_server = server.clone(); // FIXME: when we run this, this will block all other threads!
        let tmp_client = client.clone();
        tokio::spawn(async move { // FIXME: is this okay perf-wise?
            let server = tmp_server;
            let client = tmp_client;
            'end: loop {
                let mut data = server.connection.read_unreliable().await.unwrap().unwrap(); // FIXME: do error handling!
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
        });
        Ok(server.clone())
    }

}

pub fn handle_packet(packet: ServerPacket, client: &Arc<Client>, server: &Arc<Server>) {
    match packet {
        /*ClientPacket::AuthRequest { .. } => unreachable!(),
        ClientPacket::Disconnect => {
            server.online_users.remove(client.uuid.load().as_ref().unwrap()); // FIXME: verify that this can't be received before AuthRequest is handled!
        }
        ClientPacket::KeepAlive { .. } => {

        }
        ClientPacket::UpdateClientServerGroups { .. } => {}*/
        ServerPacket::AuthResponse(_) => {}
        ServerPacket::ChannelUpdate(_) => {}
        ServerPacket::ClientConnected(_) => {}
        ServerPacket::ClientDisconnected(_) => {}
        ServerPacket::ClientUpdateServerGroups { .. } => {}
        ServerPacket::KeepAlive { .. } => {}
        ServerPacket::ChallengeRequest { .. } => {}
    }
}