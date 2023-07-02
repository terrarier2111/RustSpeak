use crate::network::AddressMode;
use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use crate::DEFAULT_CHANNEL_UUID;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub address_mode: AddressMode,
    pub port: u16,
    pub req_security_level: u8,
    pub default_channel_id: u128,
}

impl Config {
    pub fn load_or_create(src: PathBuf) -> anyhow::Result<Self> {
        Ok(if let Ok(mut config) = File::open(&src) {
            let mut result = String::new();
            config.read_to_string(&mut result)?;
            let mut result: Config = serde_json::from_str(result.as_str())?;
            if result.req_security_level < 1 {
                result.req_security_level = 1;
                // FIXME: warn about invalid config value
            }
            result
        } else {
            let mut file = File::create(src)?;
            let def = Self::default();
            file.write(serde_json::to_string(&def)?.as_ref())?;
            def
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            address_mode: AddressMode::V4,
            port: 20354,
            req_security_level: 12,
            default_channel_id: DEFAULT_CHANNEL_UUID.as_u128(),
        }
    }
}
