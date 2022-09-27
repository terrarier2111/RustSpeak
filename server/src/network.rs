use crate::packet::ServerPacket;
use bytes::{Buf, Bytes, BytesMut};
use dashmap::DashMap;
use futures::StreamExt;
use quinn::{ConnectionError, Endpoint, IdleTimeout, Incoming, NewConnection, ReadExact, ReadExactError, RecvStream, SendStream, ServerConfig, TransportConfig, VarInt};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::future::Future;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;
use arc_swap::ArcSwapOption;
use tokio::sync::Mutex;
use crate::{ClientPacket, RWBytes, Server, UserUuid};
use crate::utils::current_time_millis;

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
        server: Arc<Server>,
    ) {
        'server: while let Some(conn) = self.incoming.lock().await.next().await {
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
            let client_conn = match ClientConnection::new(server.clone(), connection, initial_stream).await {
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
    pub uuid: ArcSwapOption<UserUuid>,
    pub conn: RwLock<NewConnection>,
    pub default_stream: (Mutex<SendStream>, Mutex<RecvStream>),
    pub keep_alive_stream: ArcSwapOption<(Mutex<SendStream>, Mutex<RecvStream>)>,
    server: Arc<Server>,
    stable_id: usize,
    // FIXME: cache buffers in order to avoid performance penalty for allocating new ones
}

impl ClientConnection {
    async fn new(server: Arc<Server>, conn: NewConnection, bi_conn: (SendStream, RecvStream)) -> anyhow::Result<ClientConnection> {
        let (send, recv) = bi_conn;
        let stable_id = conn.connection.stable_id();
        Ok(Self {
            uuid: ArcSwapOption::empty(),
            conn: RwLock::new(conn),
            default_stream: (Mutex::new(send), Mutex::new(recv)),
            keep_alive_stream: ArcSwapOption::empty(),
            server,
            stable_id,
        })
    }

    pub async fn start_read(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move { // FIXME: is this okay perf-wise?
            let this = this.clone();
            'end: loop {
                match this.read_keep_alive().await {
                    Ok(keep_alive) => {
                        // FIXME: use the keep alive!
                        println!("got keep alive: {}", keep_alive.id);
                    }
                    Err(_err) => {
                        // FIXME: somehow give feedback to server console and to client
                        this.close().await;
                        break 'end;
                    }
                }
            }
        });

        let this = self.clone();
        let server = self.server.clone();
        tokio::spawn(async move { // FIXME: is this okay perf-wise?
            let server = server.clone();
            let this = this.clone();
            'end: loop {
                match this.read_reliable(8).await {
                    Ok(mut size) => {
                        println!("got packet header!");
                        let size = size.get_u64_le();
                        let mut payload = match this.read_reliable(size as usize).await {
                            Ok(payload) => payload,
                            Err(_err) => {
                                // FIXME: somehow give feedback to server console and to client
                                this.close().await;
                                break 'end;
                            }
                        };
                        let packet = ClientPacket::read(&mut payload, None); // FIXME: provide client key!
                        match packet {
                            Ok(packet) => {
                                handle_packet(packet, &server, &this);
                            }
                            Err(_err) => {
                                // FIXME: somehow give feedback to server console and to client
                                this.close().await;
                                break 'end;
                            }
                        }
                    }
                    Err(_err) => {
                        // FIXME: somehow give feedback to server console and to client
                        this.close().await;
                        break 'end;
                    }
                }
            }
        });


        /*

        */
    }

    pub async fn send_reliable(&self, buf: &BytesMut) -> anyhow::Result<()> {
        self.default_stream.0.lock().await.write_all(buf).await?;
        Ok(())
    }

    pub async fn read_reliable(&self, size: usize) -> anyhow::Result<Bytes> {
        // SAFETY: This is safe because 0 is a valid value for u8
        let mut buf = unsafe { Box::new_zeroed_slice(size).assume_init() };
        self.default_stream.1.lock().await.read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }

    pub async fn read_reliable_into(&self, buf: &mut BytesMut) -> anyhow::Result<()> {
        self.default_stream.1.lock().await.read_exact(buf).await?;
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
        self.keep_alive_stream.load().as_ref().unwrap().0.lock().await.write_all(&send_data).await?;
        Ok(())
    }

    pub async fn read_keep_alive(&self) -> anyhow::Result<KeepAlive> {
        let mut buf = [0; 8 + 8 + 4]; // FIXME: use a cached buffer in order to avoid reallocation
        self.keep_alive_stream.load().as_ref().unwrap().1.lock().await.read_exact(&mut buf).await?;
        let mut buf = Bytes::from(buf.to_vec());

        let ret = KeepAlive {
            id: u64::read(&mut buf, None)?,
            send_time: Duration::read(&mut buf, None)?,
        };

        // FIXME: add abuse prevention (by checking frequency and time diff checking)

        let curr = current_time_millis();
        self.send_keep_alive(KeepAlive {
            id: ret.id,
            send_time: curr,
        }).await?;

        Ok(ret)
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        self.finish_up();
        self.close_with(0, &[]).await
    }

    pub async fn close_with(&self, err_code: u32, reason: &[u8]) -> anyhow::Result<()> {
        self.finish_up();
        self.default_stream.0.lock().await.finish().await?;
        self.conn
            .write()
            .unwrap()
            .connection
            .close(VarInt::from_u32(err_code), reason);
        Ok(())
    }

    fn finish_up(&self) {
        self.server.network_server.connections.remove(&self.stable_id);
        self.server.online_users.remove(self.uuid.load().as_ref().unwrap());
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

pub fn handle_packet(packet: ClientPacket, server: &Arc<Server>, client: &Arc<ClientConnection>) {
    match packet {
        ClientPacket::AuthRequest { .. } => unreachable!(),
        ClientPacket::Disconnect => {
            server.online_users.remove(client.uuid.load().as_ref().unwrap()); // FIXME: verify that this can't be received before AuthRequest is handled!
        }
        ClientPacket::KeepAlive { .. } => {

        }
        ClientPacket::UpdateClientServerGroups { .. } => {}
    }
}
