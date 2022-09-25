use crate::packet::ClientPacket;
use bytes::{Bytes, BytesMut};
use quinn::{ClientConfig, Endpoint, NewConnection, RecvStream, SendStream, VarInt};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use arc_swap::ArcSwapOption;
use tokio::task::JoinHandle;
use crate::{current_time_millis, RWBytes};

pub struct NetworkClient {
    // FIXME: Add keep alive stream
    endpoint: Endpoint,
    conn: RwLock<NewConnection>,
    bi_conn: (Mutex<SendStream>, Mutex<RecvStream>),
    keep_alive_handler: ArcSwapOption<KeepAliveHandler>,
}

impl NetworkClient {
    pub async fn new(
        address_mode: AddressMode,
        config: ClientConfig,
        server: SocketAddr,
        server_name: &str,
    ) -> anyhow::Result<Self> {
        let endpoint = Endpoint::client(address_mode.local())?;
        let conn = endpoint.connect_with(config, server, server_name)?.await?;
        let (send, recv) = conn.connection.open_bi().await?;

        // panic!("we got pretty far!")

        Ok(Self {
            endpoint,
            conn: RwLock::new(conn),
            bi_conn: (Mutex::new(send), Mutex::new(recv)),
            keep_alive_handler: ArcSwapOption::empty(),
        })
    }

    /*
    pub(crate) async fn flush(&self) -> anyhow::Result<()> {
        self.bi_conn.0.lock().unwrap().flush().await?;
        Ok(())
    }*/

    pub async fn send_reliable(&self, buf: &BytesMut) -> anyhow::Result<()> {
        self.bi_conn.0.lock().unwrap().write_all(buf).await?;
        Ok(())
    }

    pub async fn read_reliable(&self, size: usize) -> anyhow::Result<Bytes> {
        // SAFETY: This is safe because 0 is a valid value for u8
        let mut buf = unsafe { Box::new_zeroed_slice(size).assume_init() };
        self.bi_conn.1.lock().unwrap().read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }

    pub async fn read_reliable_into(&self, buf: &mut BytesMut) -> anyhow::Result<()> {
        self.bi_conn.1.lock().unwrap().read_exact(buf).await?;
        Ok(())
    }

    pub fn send_unreliable(&self, buf: Bytes) -> anyhow::Result<()> {
        self.conn.write().unwrap().connection.send_datagram(buf)?;
        Ok(())
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        self.close_with(0, &[]).await
    }

    pub async fn close_with(&self, err_code: u32, reason: &[u8]) -> anyhow::Result<()> {
        self.bi_conn.0.lock().unwrap().finish().await?;
        self.conn
            .write()
            .unwrap()
            .connection
            .close(VarInt::from_u32(err_code), reason);
        Ok(())
    }

    pub async fn start_do_keep_alive(&self, interval: Duration) -> anyhow::Result<()> {
        let stream = self.conn.write().unwrap().connection.open_bi().await?;
        let handle = tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                self.send_keep_alive(KeepAlive {
                    id: 0, // FIXME: choose an id that makes sense
                    send_time: current_time_millis(),
                });
                interval.tick().await;
            }
        });
        let handler = KeepAliveHandler {
            stream: (Mutex::new(stream.0), Mutex::new(stream.1)),
            timer: handle,
        };
        self.keep_alive_handler.store(Some(Arc::new(handler)));
        Ok(())
    }

    async fn send_keep_alive(&self, data: KeepAlive) -> anyhow::Result<()> {
        let mut send_data = BytesMut::with_capacity(8 + 8 + 4);
        data.id.write(&mut send_data)?;
        data.send_time.write(&mut send_data)?;
        self.keep_alive_handler.load().as_ref().unwrap().stream.0.lock().unwrap().write_all(&send_data).await?;
        Ok(())
    }

    pub async fn read_unreliable(&self) -> anyhow::Result<Bytes> {
        // self.conn.write().unwrap().connection.
        todo!()
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

pub struct KeepAlive {
    pub id: u64,
    pub send_time: Duration,
}

struct KeepAliveHandler {
    stream: (Mutex<SendStream>, Mutex<RecvStream>),
    timer: JoinHandle<()>,
}
