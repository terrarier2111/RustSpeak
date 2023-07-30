use crate::ChannelPerms;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
use std::path::Path;
use ruint::aliases::U256;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct ChannelDbEntry<'a> {
    pub id: u128, // channel uuid
    pub sort_id: u16,
    pub name: Cow<'a, str>,
    pub desc: Cow<'a, str>,
    pub password: Option<Cow<'a, str>>,
    pub user_groups: Vec<(U256Container, u128)>, // user uuid and channel group uuid
    pub perms: ChannelPerms,
    pub slots: u16, // FIXME: add option for unlimited slots via `-1` value!
}

pub struct ChannelDb {
    path: String,
}

impl ChannelDb {
    pub fn new(path: String) -> Self {
        Self { path }
    }

    pub fn read_or_create<'db, F: FnOnce() -> anyhow::Result<Vec<ChannelDbEntry<'db>>>>(
        &self,
        default: F,
    ) -> anyhow::Result<Vec<ChannelDbEntry<'db>>> {
        match File::open(self.path.clone()) {
            Ok(mut db_file) => {
                let mut content = String::new();
                db_file.read_to_string(&mut content)?;
                let channel_db = serde_json::from_str(&content)?;
                Ok(channel_db)
            }
            Err(_) => {
                let default = default()?;
                self.write(&default)?;
                Ok(default)
            }
        }
    }

    pub fn write(&self, channels: &Vec<ChannelDbEntry>) -> anyhow::Result<()> {
        let val = serde_json::to_string(channels)?;
        let mut file = File::create(self.path.clone())?;
        file.write(val.as_bytes())?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct U256Container {
    raw: [u8; size_of::<U256>()],
    #[serde(skip)]
    _align: [U256; 0],
}

impl U256Container {

    fn new(val: U256) -> Self {
        Self {
            raw: val.to_le_bytes(),
            _align: [],
        }
    }

    fn unwrap(&self) -> U256 {
        U256::from_le_bytes(self.raw.clone())
    }

}
