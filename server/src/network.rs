use quinn::{Endpoint, Incoming, ServerConfig};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub struct NetworkServer {
    endpoint: Endpoint,
    incoming: Incoming,
}

impl NetworkServer {
    pub fn new(port: u16, address_mode: AddressMode, config: ServerConfig) -> anyhow::Result<Self> {
        let endpoint = Endpoint::server(config, address_mode.local(port))?;

        Ok(Self {
            endpoint: endpoint.0,
            incoming: endpoint.1,
        })
    }
}

#[derive(Copy, Clone, Debug)]
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
