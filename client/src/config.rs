use std::fs::File;
use std::io;
use std::io::{Read, Write};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use serde::*;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config<'a> {
    pub fav_servers: Vec<ServerEntry<'a>>,
    pub last_server: SocketAddr,
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
pub struct ServerEntry<'a> {
    pub name: &'a str, // FIXME: should we use Cow?
    pub addr: SocketAddr,
}
