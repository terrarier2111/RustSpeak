use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use arc_swap::ArcSwap;
use openssl::pkey::PKey;
use uuid::Uuid;
use crate::{AddressMode, Channel, ClientConfig, ClientPacket, NetworkClient, Profile, PROTOCOL_VERSION};

pub struct Server {
    pub profile: Profile, // FIXME: make this an ArcSwap instead!
    pub connection: Arc<NetworkClient>, // FIXME: support connecting to multiple servers at once
    pub channels: ArcSwap<HashMap<Uuid, Channel>>,
}

impl Server {
    pub async fn new(profile: Profile, address_mode: AddressMode,
               config: ClientConfig,
               server: SocketAddr,
               server_name: &str,) -> anyhow::Result<Self> {
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
        Ok(server)
    }

}