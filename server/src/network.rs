use crate::packet::ServerPacket;
use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use futures::StreamExt;
use quinn::{
    ConnectionError, Endpoint, IdleTimeout, Incoming, NewConnection, RecvStream, SendStream,
    ServerConfig, TransportConfig, VarInt,
};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::future::Future;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use arc_swap::ArcSwapOption;
use crate::RWBytes;

pub struct NetworkServer {
    pub endpoint: Endpoint,
    pub incoming: Mutex<Incoming>,
    pub connections: DashMap<usize, Arc<ClientConnection>>,
}

impl NetworkServer {
    pub fn new(
        port: u16,
        idle_timeout_millis: u32,
        address_mode: AddressMode,
        mut config: ServerConfig,
    ) -> anyhow::Result<Self> {
        let mut transport_cfg = TransportConfig::default();
        transport_cfg.max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(
            idle_timeout_millis,
        ))));
        config.transport = Arc::new(transport_cfg);
        let endpoint = Endpoint::server(config, address_mode.local(port))?;

        Ok(Self {
            endpoint: endpoint.0,
            incoming: Mutex::new(endpoint.1),
            connections: Default::default(),
        })
    }

    pub async fn accept_connections<
        F: Fn(Arc<ClientConnection>) -> B,
        B: Future<Output = anyhow::Result<()>>,
        E: Fn(anyhow::Error),
    >(
        &self,
        handler: F,
        error_handler: E,
    ) {
        'server: while let Some(conn) = self.incoming.lock().unwrap().next().await {
            // FIXME: here we are holding on a mutex across await boundaries
            let mut connection = match conn.await {
                Ok(val) => val,
                Err(err) => {
                    error_handler(anyhow::Error::from(err));
                    continue 'server;
                }
            };
            let initial_stream = {
                match connection.bi_streams.next().await {
                    None => unreachable!(),
                    Some(stream) => match stream {
                        Ok(stream) => stream,
                        Err(err) => {
                            error_handler(anyhow::Error::from(err));
                            continue 'server;
                        }
                    },
                }
            };
            let id = connection.connection.stable_id();
            let client_conn = match ClientConnection::new(connection, initial_stream).await {
                Ok(val) => Arc::new(val),
                Err(err) => {
                    error_handler(anyhow::Error::from(err));
                    continue 'server;
                }
            };
            if let Err(err) = handler(client_conn.clone()).await {
                error_handler(anyhow::Error::from(err));
                continue 'server;
            }

            self.connections.insert(id, client_conn);
        }
    }
}

pub struct ClientConnection {
    pub conn: RwLock<NewConnection>,
    pub default_stream: (Mutex<SendStream>, Mutex<RecvStream>),
    pub keep_alive_stream: ArcSwapOption<(Mutex<SendStream>, Mutex<RecvStream>)>,
    // FIXME: save read future in here and complete it further and further (but only if we already got a size)
}

impl ClientConnection {
    async fn new(conn: NewConnection, bi_conn: (SendStream, RecvStream)) -> anyhow::Result<Self> {
        let (send, recv) = bi_conn;
        Ok(Self {
            conn: RwLock::new(conn),
            default_stream: (Mutex::new(send), Mutex::new(recv)),
            keep_alive_stream: ArcSwapOption::empty(),
        })
    }

    pub async fn send_reliable(&self, buf: &BytesMut) -> anyhow::Result<()> {
        self.default_stream.0.lock().unwrap().write_all(buf).await?;
        Ok(())
    }

    pub async fn read_reliable(&self, size: usize) -> anyhow::Result<Bytes> {
        // SAFETY: This is safe because 0 is a valid value for u8
        let mut buf = unsafe { Box::new_zeroed_slice(size).assume_init() };
        self.default_stream.1.lock().unwrap().read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }

    pub fn try_read_reliable(&self, size: usize) -> anyhow::Result<Option<Bytes>> {
        // SAFETY: This is safe because 0 is a valid value for u8
        let mut buf = unsafe { Box::new_zeroed_slice(size).assume_init() };
        self.default_stream.1.lock().unwrap().read_exact(&mut buf).no?;
        Ok(Some(Bytes::from(buf)))
    }

    pub async fn read_reliable_into(&self, buf: &mut BytesMut) -> anyhow::Result<()> {
        self.default_stream.1.lock().unwrap().read_exact(buf).await?;
        Ok(())
    }

    pub fn send_unreliable(&self, buf: Bytes) -> anyhow::Result<()> {
        self.conn.write().unwrap().connection.send_datagram(buf)?;
        Ok(())
    }

    async fn send_keep_alive(&self, data: KeepAlive) -> anyhow::Result<()> {
        let mut send_data = BytesMut::with_capacity(8 + 8 + 4);
        data.id.write(&mut send_data)?;
        data.send_time.write(&mut send_data)?;
        self.keep_alive_stream.load().as_ref().unwrap().0.lock().unwrap().write_all(&send_data).await?;
        Ok(())
    }

    pub async fn read_keep_alive(&self) -> anyhow::Result<Option<KeepAlive>> {
        let mut buf = [0; 8 + 8 + 4];
        self.keep_alive_stream.load().as_ref().unwrap().1.lock().unwrap().read_exact(&mut buf).await?;
        let mut buf = Bytes::from(buf);

        let ret = KeepAlive {
            id: u64::read(&mut buf)?,
            send_time: Duration::read(&mut buf)?,
        };

        // FIXME: add abuse prevention (by checking frequency and time diff checking)

        self.send_keep_alive(KeepAlive {
            id: ret.id,
            send_time: curr,
        });

        Ok(Some(ret))
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        self.close_with(0, &[]).await
    }

    pub async fn close_with(&self, err_code: u32, reason: &[u8]) -> anyhow::Result<()> {
        self.default_stream.0.lock().unwrap().finish().await?;
        self.conn
            .write()
            .unwrap()
            .connection
            .close(VarInt::from_u32(err_code), reason);
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

pub struct KeepAlive {
    pub id: u64,
    pub send_time: Duration,
}
