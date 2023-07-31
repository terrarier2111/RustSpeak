use crate::{RWBytes, UserUuid};
use bytemuck_derive::Zeroable;
use bytes::{Bytes, BytesMut};
use sled::{Db, IVec};
use std::borrow::Cow;
use std::io::Read;
use ruint::aliases::U256;
use uuid::Uuid;
use crate::packet::PermsSnapshot;

pub struct UserDb {
    db: Db,
}

impl UserDb {
    pub fn new(path: String) -> anyhow::Result<Self> {
        Ok(Self {
            db: sled::open(path)?,
        })
    }

    pub fn get(&self, uuid: &UserUuid) -> anyhow::Result<Option<DbUser>> {
        Ok(match self.db.get(uuid)? {
            None => None,
            Some(x) => {
                println!("size: {}", x.len());
                if !x.is_empty() {
                    Some(DbUser::from_bytes(x)?)
                } else {
                    None
                }
            },
        })
    }

    pub fn insert(&self, user: DbUser) -> anyhow::Result<()> {
        self.db.insert(user.uuid.clone(), user.to_bytes()?)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct DbUser {
    pub uuid: UserUuid, // FIXME: maybe it's better to store the user's actual public key instead of their uuid
    pub name: String,
    pub last_security_proof: U256,
    pub last_verified_security_level: u8,
    pub groups: Vec<Uuid>,
    pub perms: PermsSnapshot,
}

impl DbUser {
    fn to_bytes(self) -> anyhow::Result<IVec> {
        let mut buf = BytesMut::new();
        self.uuid.write(&mut buf)?;
        self.name.write(&mut buf)?;
        self.last_security_proof.write(&mut buf)?;
        self.last_verified_security_level.write(&mut buf)?;
        self.groups.write(&mut buf)?;
        self.perms.write(&mut buf)?;
        Ok(IVec::from(buf.to_vec()))
    }

    fn from_bytes(bytes: IVec) -> anyhow::Result<Self> {
        let mut buf = Bytes::from(bytes.to_vec());
        Ok(Self {
            uuid: UserUuid::read(&mut buf, None)?,
            name: String::read(&mut buf, None)?,
            last_security_proof: U256::read(&mut buf, None)?,
            last_verified_security_level: u8::read(&mut buf, None)?,
            groups: Vec::<Uuid>::read(&mut buf, None)?,
            perms: PermsSnapshot::read(&mut buf, None)?,
        })
    }
}
