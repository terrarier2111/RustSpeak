use serde::*;
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use ruint::aliases::U256;
use crate::protocol::UserUuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub fav_servers: Vec<ServerEntry>,
    pub last_server: Option<SocketAddr>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fav_servers: vec![ServerEntry {
                name: "local".to_string(),
                addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 20354)),
                profile: None,
            }],
            last_server: None,
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
    pub name: String,
    // FIXME: should we use Cow?
    pub addr: SocketAddr,
    profile: Option<UserUuidContainer>,
    // FIXME: we need a favicon (image) for each server image
}

#[derive(Serialize, Deserialize, Debug)]
struct UserUuidContainer {
    raw: [u8; size_of::<UserUuid>()],
    _align: [UserUuid; 0],
}

impl ServerEntry {
    
    pub fn new(name: String, addr: SocketAddr, profile: Option<UserUuid>) -> Self {
        Self {
            name,
            addr,
            profile: profile.map(|uuid| UserUuidContainer {
                raw: uuid.into_u256().to_le_bytes(),
                _align: [],
            }),
        }
    }

    pub fn profile(&self) -> Option<UserUuid> {
        self.profile.as_ref().map(|container| UserUuid::from_u256(U256::from_le_bytes(container.raw.clone())))
    }

}
