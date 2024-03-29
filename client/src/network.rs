use bytes::{Bytes, BytesMut};
use quinn::{ClientConfig, Connection, ConnectionError, Endpoint, RecvStream, SendStream, VarInt};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use arc_swap::ArcSwapOption;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use crate::{current_time_millis, RWBytes};

pub struct NetworkClient {
    endpoint: Endpoint,
    connection: Connection,
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
        let (send, recv) = conn.open_bi().await?;

        Ok(Self {
            endpoint,
            connection: conn,
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

    pub fn max_size<const ALIGN: usize>(&self) -> usize {
        // split up large packets into many smaller sub-packets, this is the max bytes per packet
        let max_bytes = self.connection.max_datagram_size().unwrap() - 25;
        // align max_bytes
        let max_bytes = if ALIGN < 2 || max_bytes % ALIGN == 0 {
            max_bytes
        } else {
            max_bytes - (max_bytes % ALIGN)
        };
        max_bytes
    }

    pub async fn send_unreliable<const ALIGN: usize>(&self, mut buf: Bytes) -> anyhow::Result<()> {
        let max_bytes = self.max_size::<ALIGN>();
        // split up large packets into many smaller sub-packets
        let full_frames = buf.len().div_floor(max_bytes);
        for x in 0..full_frames {
            self.connection.send_datagram(buf.slice((x * max_bytes)..(x * max_bytes + max_bytes)))?;
        }
        self.connection.send_datagram(buf.slice((full_frames * max_bytes)..buf.len()))?;
        Ok(())
    }

    /// Sends a chunk of data as a single packet
    pub async fn send_unreliable_force<const ALIGN: usize>(&self, buf: Bytes) -> anyhow::Result<()> {
        self.connection.send_datagram(buf)?;
        Ok(())
    }

    pub async fn read_unreliable(&self) -> Result<Bytes, ConnectionError> {
        self.connection.read_datagram().await
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        self.close_with(0, &[]).await
    }

    pub async fn close_with(&self, err_code: u32, reason: &[u8]) -> anyhow::Result<()> {
        self.bi_conn.0.lock().await.finish().await?;
        self.connection
            .close(VarInt::from_u32(err_code), reason);
        Ok(())
    }

    pub async fn start_do_keep_alive<E: Fn(anyhow::Error) + Send + Sync + 'static>(self: &Arc<NetworkClient>, interval: Duration, err_handler: E) -> anyhow::Result<()> {
        let stream = self.connection.open_bi().await?;
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

        let this = self.clone();
        tokio::spawn(async move {
            let this = this.clone();
            'end: loop {
                match this.read_keep_alive().await {
                    Ok(keep_alive) => {
                        // FIXME: use the keep alive!
                        // println!("got keep alive: {}", keep_alive.id);
                    }
                    Err(_err) => {
                        // FIXME: somehow give feedback to server console and to client
                        this.close().await;
                        break 'end;
                    }
                }
            }
        });

        Ok(())
    }

    async fn send_keep_alive(&self, data: KeepAlive) -> anyhow::Result<()> {
        let mut send_data = BytesMut::with_capacity(8 + 8 + 4);
        data.id.write(&mut send_data)?;
        data.send_time.write(&mut send_data)?;
        self.keep_alive_handler.load().as_ref().unwrap().stream.0.lock().await.write_all(&send_data).await?;
        Ok(())
    }

    async fn read_keep_alive(&self) -> anyhow::Result<KeepAlive> {
        let mut buf = [0; 8 + 8 + 4];
        let handler = self.keep_alive_handler.load();
        handler.deref().as_ref().unwrap().stream.1.lock().await.read_exact(&mut buf).await?;
        let mut buf = Bytes::from(buf.to_vec());

        let ret = KeepAlive {
            id: u64::read(&mut buf)?,
            send_time: Duration::read(&mut buf)?,
        };

        Ok(ret)
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
