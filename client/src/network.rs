use crate::packet::ClientPacket;
use quinn::{ClientConfig, Endpoint, NewConnection, RecvStream, SendStream};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};

pub struct NetworkClient {
    endpoint: Endpoint,
    conn: RwLock<NewConnection>,
    bi_conn: (Mutex<SendStream>, Mutex<RecvStream>),
}

impl NetworkClient {
    pub async fn new(
        address_mode: AddressMode,
        config: Option<ClientConfig>,
        server: SocketAddr,
        server_name: &str,
    ) -> anyhow::Result<Self> {
        let endpoint = Endpoint::client(address_mode.local())?;
        let conn = if let Some(config) = config {
            endpoint.connect_with(config, server, server_name)?.await?
        } else {
            endpoint.connect(server, server_name)?.await?
        };
        let (send, recv) = conn.connection.open_bi().await?;

        Ok(Self {
            endpoint,
            conn: RwLock::new(conn),
            bi_conn: (Mutex::new(send), Mutex::new(recv)),
        })
    }

    pub async fn send_reliable(&self, data: &ClientPacket) -> anyhow::Result<()> {
        let mut send = self.bi_conn.0.lock().unwrap();
        send.write_all(data).await?;

        Ok(())
    }

    pub async fn read_reliable(&self, size_limit: usize) -> () {
        let mut recv = self.bi_conn.1.lock().unwrap();
        recv.read_to_end(size_limit).await?
    }
}

#[derive(Copy, Clone, Debug)]
pub enum AddressMode {
    V4,
    V6,
}

impl AddressMode {
    fn local(self) -> SocketAddr {
        match self {
            AddressMode::V4 => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            AddressMode::V6 => SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
        }
    }
}

pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}
