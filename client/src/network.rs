use crate::packet::ClientPacket;
use bytes::{Bytes, BytesMut};
use quinn::{ClientConfig, ConnectionError, Endpoint, NewConnection, RecvStream, SendStream, VarInt};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use arc_swap::ArcSwapOption;
use futures_util::StreamExt;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use crate::{current_time_millis, RWBytes};

pub struct NetworkClient {
    // FIXME: Add keep alive stream
    endpoint: Endpoint,
    conn: tokio::sync::RwLock<NewConnection>,
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
            conn: tokio::sync::RwLock::new(conn),
            bi_conn: (Mutex::new(send), Mutex::new(recv)),
            keep_alive_handler: ArcSwapOption::empty(),
        })
    }

    pub async fn send_reliable(&self, buf: &BytesMut) -> anyhow::Result<()> {
        self.bi_conn.0.lock().await.write_all(buf).await?;
        Ok(())
    }

    pub async fn read_reliable(&self, size: usize) -> anyhow::Result<Bytes> {
        // SAFETY: This is safe because 0 is a valid value for u8
        let mut buf = unsafe { Box::new_zeroed_slice(size).assume_init() };
        self.bi_conn.1.lock().await.read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }

    pub async fn read_reliable_into(&self, buf: &mut BytesMut) -> anyhow::Result<()> {
        self.bi_conn.1.lock().await.read_exact(buf).await?;
        Ok(())
    }

    pub async fn send_unreliable(&self, mut buf: Bytes) -> anyhow::Result<()> {
        // split up large packets into many smaller sub-packets
        let max_bytes = self.conn.read().await.connection.max_datagram_size().unwrap() - 25; // - 100 works | 25 also works (at least up to 2.3k keep alives)
        let full_frames = buf.len().div_floor(max_bytes);
        for x in 0..full_frames {
            self.conn.write().await.connection.send_datagram(buf.slice((x * max_bytes)..(x * max_bytes + max_bytes)))?;
        }
        self.conn.write().await.connection.send_datagram(buf.slice((full_frames * max_bytes)..buf.len()))?;
        Ok(())
    }

    pub async fn read_unreliable(&self) -> Option<Result<Bytes, ConnectionError>> {
        self.conn.write().await.datagrams.next().await
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        self.close_with(0, &[]).await
    }

    pub async fn close_with(&self, err_code: u32, reason: &[u8]) -> anyhow::Result<()> {
        self.bi_conn.0.lock().await.finish().await?;
        self.conn
            .write()
            .await
            .connection
            .close(VarInt::from_u32(err_code), reason);
        Ok(())
    }

    pub async fn start_do_keep_alive<E: Fn(anyhow::Error) + Send + Sync + 'static>(self: &Arc<NetworkClient>, interval: Duration, err_handler: E) -> anyhow::Result<()> {
        let stream = self.conn.write().await.connection.open_bi().await?;
        let this = self.clone();
        let handle = tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                let maybe_err = this.send_keep_alive(KeepAlive {
                    id: this.keep_alive_handler.load().as_ref().unwrap().counter.fetch_add(1, Ordering::Acquire) as u64,
                    send_time: current_time_millis(),
                }).await;
                match maybe_err {
                    Ok(_) => {}
                    Err(err) => {
                        err_handler(err);
                    }
                }
                interval.tick().await;
            }
        });
        let handler = KeepAliveHandler {
            stream: (Mutex::new(stream.0), Mutex::new(stream.1)),
            timer: handle,
            counter: Default::default(),
        };
        self.keep_alive_handler.store(Some(Arc::new(handler)));
        Ok(())
    }

    async fn send_keep_alive(&self, data: KeepAlive) -> anyhow::Result<()> {
        let mut send_data = BytesMut::with_capacity(8 + 8 + 4);
        data.id.write(&mut send_data)?;
        data.send_time.write(&mut send_data)?;
        self.keep_alive_handler.load().as_ref().unwrap().stream.0.lock().await.write_all(&send_data).await?;
        Ok(())
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
    counter: AtomicUsize,
}
