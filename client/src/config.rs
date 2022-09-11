use serde::*;
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub fav_servers: Vec<ServerEntry>,
    pub last_server: SocketAddr,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fav_servers: vec![],
            last_server: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)), // FIXME: define a default port for client and server
        }
    }
}

impl Config {
    pub fn load_or_create(src: PathBuf) -> anyhow::Result<Self> {
        Ok(if let Ok(mut config) = File::open(&src) {
            let mut result = String::new();
            config.read_to_string(&mut result)?;
            serde_json::from_str(result.as_str())?
        } else {
            let mut file = File::create(src)?;
            let def = Self::default();
            file.write(serde_json::to_string(&def)?.as_ref())?;
            def
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerEntry {
    pub name: String, // FIXME: should we use Cow?
    pub addr: SocketAddr,
}
