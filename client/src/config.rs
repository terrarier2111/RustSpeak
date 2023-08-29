use std::fs;
use serde::*;
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use ruint::aliases::U256;
use crate::protocol::UserUuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub fav_servers: Vec<ServerEntry>,
    pub last_server: Option<SocketAddr>,
    default_account: Option<UserUuidContainer>,
}

impl Config {

    pub fn get_default_account(&self) -> Option<UserUuid> {
        self.default_account.as_ref().map(|container| container.unwrap())
    }

    #[must_use]
    pub fn set_default_account(&self, account: UserUuid) -> Config {
        Config {
            fav_servers: self.fav_servers.clone(),
            last_server: self.last_server.clone(),
            default_account: Some(UserUuidContainer::new(account)),
        }
    }

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
            default_account: None,
        }
    }
}

pub const DATA_DIR_PATH: &str = "RustSpeakClient/";
pub const CONFIG_FILE: &str = "config.json";

pub fn data_path() -> PathBuf {
    dirs::config_dir().unwrap().join(DATA_DIR_PATH)
}

pub fn config_path() -> PathBuf {
    data_path().join(CONFIG_FILE)
}

impl Config {
    pub fn load_or_create() -> anyhow::Result<Self> {
        let data_dir = dirs::config_dir().unwrap().join(DATA_DIR_PATH);
        fs::create_dir_all(data_dir.clone())?;
        let src = config_path();
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

    pub fn save(&self) -> anyhow::Result<()> {
        let mut file = File::create(config_path())?;
        file.write(serde_json::to_string(self)?.as_ref())?;
        Ok(())
    }

}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerEntry {
    pub name: String,
    // FIXME: should we use Cow?
    pub addr: SocketAddr,
    profile: Option<UserUuidContainer>,
    // FIXME: we need a favicon (image) for each server image
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct UserUuidContainer([u8; size_of::<UserUuid>()]);

impl UserUuidContainer {

    fn new(uuid: UserUuid) -> Self {
        Self(uuid.into_u256().to_le_bytes())
    }

    fn unwrap(&self) -> UserUuid {
        UserUuid::from_u256(U256::from_le_bytes(self.0.clone()))
    }

}

impl ServerEntry {
    
    pub fn new(name: String, addr: SocketAddr, profile: Option<UserUuid>) -> Self {
        Self {
            name,
            addr,
            profile: profile.map(|uuid| UserUuidContainer::new(uuid)),
        }
    }

    pub fn profile(&self) -> Option<UserUuid> {
        self.profile.as_ref().map(|container| container.unwrap())
    }

}
