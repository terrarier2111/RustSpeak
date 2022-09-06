use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;
use crate::ChannelPerms;
use serde_derive::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct ChannelDbEntry<'a> {
    pub id: u128, // channel uuid
    pub name: Cow<'a, str>,
    pub desc: Cow<'a, str>,
    pub password: bool,
    pub user_groups: Vec<(u128, u128)>, // user uuid and channel group uuid
    pub perms: ChannelPerms,
}

pub struct ChannelDb {
    path: String,
}

impl ChannelDb {

    pub fn new(path: String) -> Self {
        Self {
            path,
        }
    }

    pub fn read_or_create<'db, F: FnOnce() -> anyhow::Result<Vec<ChannelDbEntry<'db>>>>(&self, default: F) -> anyhow::Result<Vec<ChannelDbEntry<'db>>> {
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
