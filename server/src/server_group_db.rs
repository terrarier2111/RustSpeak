use crate::packet::PermsSnapshot;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Write};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct ServerGroupEntry<'a> {
    pub uuid: u128,
    pub name: Cow<'a, str>,
    pub perms: PermsSnapshot,
}

pub struct ServerGroupDb {
    pub(crate) path: String,
}

impl ServerGroupDb {
    pub fn new(path: String) -> Self {
        Self { path }
    }

    pub fn read_or_create<'db, F: FnOnce() -> anyhow::Result<Vec<ServerGroupEntry<'db>>>>(
        &self,
        default: F,
    ) -> anyhow::Result<Vec<ServerGroupEntry<'db>>> {
        match File::open(self.path.clone()) {
            Ok(mut db_file) => {
                let mut content = String::new();
                db_file.read_to_string(&mut content)?;
                let server_group_db = serde_json::from_str(&content)?;
                Ok(server_group_db)
            }
            Err(_) => {
                let default = default()?;
                self.write(&default)?;
                Ok(default)
            }
        }
    }

    pub fn write(&self, server_groups: &Vec<ServerGroupEntry>) -> anyhow::Result<()> {
        let val = serde_json::to_string(server_groups)?;
        let mut file = File::create(self.path.clone())?;
        file.write(val.as_bytes())?;

        Ok(())
    }
}
