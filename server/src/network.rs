use std::collections::HashMap;
use std::future::Future;
use std::mem::MaybeUninit;
use quinn::{ConnectionError, Endpoint, IdleTimeout, Incoming, NewConnection, RecvStream, SendStream, ServerConfig, TransportConfig, VarInt};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use futures::StreamExt;
use crate::packet::ServerPacket;
use serde_derive::{Serialize, Deserialize};

pub struct NetworkServer {
    pub endpoint: Endpoint,
    pub incoming: Mutex<Incoming>,
    pub connections: DashMap<usize, Arc<ClientConnection>>,
}

impl NetworkServer {
    pub fn new(port: u16, idle_timeout_millis: u32, address_mode: AddressMode, mut config: ServerConfig) -> anyhow::Result<Self> {
        let mut transport_cfg = TransportConfig::default();
        transport_cfg
            .max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(idle_timeout_millis))));
        config.transport = Arc::new(transport_cfg);
        let endpoint = Endpoint::server(config, address_mode.local(port))?;

        Ok(Self {
            endpoint: endpoint.0,
            incoming: Mutex::new(endpoint.1),
            connections: Default::default(),
        })
    }

    pub async fn accept_connections<F: Fn(Arc<ClientConnection>) -> B, B: Future<Output = anyhow::Result<()>>, E: Fn(anyhow::Error)>(&self, handler: F, error_handler: E) {
    // pub async fn try_accept_connections<F: Future<Output = anyhow::Result<()>>/*Fn(&mut NewConnection)*/>(&self, handler: F) -> anyhow::Result<usize> {
        // let mut new_cons = 0;
        'server: while let Some(conn) = self.incoming.lock().unwrap().next().await {
            let mut connection = match conn.await {
                Ok(val) => val,
                Err(err) => {
                    error_handler(anyhow::Error::from(err));
                    continue 'server;
                },
            };
            let id = connection.connection.stable_id();
            let client_conn = match ClientConnection::new(connection).await {
                    Ok(val) => Arc::new(val),
                    Err(err) => {
                        error_handler(anyhow::Error::from(err));
                        continue 'server;
                    },
                };
            if let Err(err) = handler(client_conn.clone()).await {
                error_handler(anyhow::Error::from(err));
                continue 'server;
            }

            self.connections.insert(id, client_conn);
            // new_cons += 1;
        }
        // Ok(()/*new_cons*/)
    }
}

pub struct ClientConnection {
    pub conn: RwLock<NewConnection>,
    pub bi_conn: (Mutex<SendStream>, Mutex<RecvStream>),
}

impl ClientConnection {

    async fn new(conn: NewConnection) -> anyhow::Result<Self> {
        let (send, recv) = conn.connection.open_bi().await?;
        Ok(Self {
            conn: RwLock::new(conn),
            bi_conn: (Mutex::new(send), Mutex::new(recv)),
        })
    }

    pub async fn send_reliable(&self, buf: &BytesMut) -> anyhow::Result<()> {
        let mut send = self.bi_conn.0.lock().unwrap();
        send.write_all(buf).await?;

        Ok(())
    }

    pub async fn read_reliable(&self, size: usize) -> anyhow::Result<Bytes> {
        let mut recv = self.bi_conn.1.lock().unwrap();
        // SAFETY: This is safe because 0 is a valid value for u8
        let mut buf = unsafe { Box::new_zeroed_slice(size).assume_init() };
        recv.read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }

    pub async fn read_reliable_into(&self, buf: &mut BytesMut) -> anyhow::Result<()> {
        let mut recv = self.bi_conn.1.lock().unwrap();
        recv.read_exact(buf).await?;
        Ok(())
    }

    pub fn send_unreliable(&self, buf: Bytes) -> anyhow::Result<()> {
        self.conn.write().unwrap().connection.send_datagram(buf)?;
        Ok(())
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        self.bi_conn.0.lock().unwrap().finish().await?;
        self.conn.write().unwrap().connection.close(VarInt::from_u32(0), &[]);
        Ok(())
    }

}

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub enum AddressMode {
    V4,
    V6,
}

impl AddressMode {
    fn local(self, port: u16) -> SocketAddr {
        match self {
            AddressMode::V4 => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            AddressMode::V6 => SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
        }
    }
}
