use crate::{RWBytes, UserUuid};
use bytemuck_derive::Zeroable;
use bytes::{Bytes, BytesMut};
use sled::{Db, IVec};
use std::borrow::Cow;
use std::io::Read;
use uuid::Uuid;

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
            Some(x) => Some(DbUser::from_bytes(x)?),
        })
    }

    pub fn insert(&self, user: DbUser) -> anyhow::Result<()> {
        self.db.insert(user.uuid.clone(), user.to_bytes()?)?;
        Ok(())
    }
}

pub struct DbUser {
    pub uuid: UserUuid,
    pub name: String,
    pub last_security_proof: u128,
    pub last_verified_security_level: u8,
    pub groups: Vec<Uuid>,
    // FIXME: add individual user perms
}

impl DbUser {
    fn to_bytes(self) -> anyhow::Result<IVec> {
        let mut buf = BytesMut::new();
        self.uuid.write(&mut buf)?;
        self.name.write(&mut buf)?;
        self.last_security_proof.write(&mut buf)?;
        self.groups.write(&mut buf)?;
        Ok(IVec::from(buf.to_vec()))
    }

    fn from_bytes(bytes: IVec) -> anyhow::Result<Self> {
        let mut buf = Bytes::from(bytes.to_vec());
        Ok(Self {
            uuid: UserUuid::read(&mut buf, None)?,
            name: String::read(&mut buf, None)?,
            last_security_proof: u128::read(&mut buf, None)?,
            last_verified_security_level: u8::read(&mut buf, None)?,
            groups: Vec::<Uuid>::read(&mut buf, None)?,
        })
    }
}
