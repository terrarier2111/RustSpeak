use crate::RWBytes;
use bytemuck_derive::Zeroable;
use bytes::{Bytes, BytesMut};
use ruint::aliases::U256;
use sled::{Db, IVec};
use std::borrow::Cow;
use std::io::Read;
use uuid::Uuid;

pub struct ProfileDb {
    db: Db,
}

impl ProfileDb {
    pub fn new(path: String) -> anyhow::Result<Self> {
        Ok(Self {
            db: sled::open(path)?,
        })
    }

    pub fn get(&self, name: &String) -> anyhow::Result<Option<DbProfile>> {
        Ok(match self.db.get(name)? {
            None => None,
            Some(x) => Some(DbProfile::from_bytes(x)?),
        })
    }

    pub fn insert(&self, user: DbProfile) -> anyhow::Result<()> {
        self.db.insert(user.name.clone(), user.to_bytes()?)?;
        Ok(())
    }
}

pub struct DbProfile {
    pub priv_key: Vec<u8>,
    pub name: String,
    pub security_proofs: Vec<U256>,
}

impl DbProfile {
    fn to_bytes(self) -> anyhow::Result<IVec> {
        let mut buf = BytesMut::new();
        self.priv_key.write(&mut buf)?;
        self.name.write(&mut buf)?;
        self.security_proofs.write(&mut buf)?;
        Ok(IVec::from(buf.to_vec()))
    }

    fn from_bytes(bytes: IVec) -> anyhow::Result<Self> {
        let mut buf = Bytes::from(bytes.to_vec());
        Ok(Self {
            priv_key: Vec::<u8>::read(&mut buf)?,
            name: String::read(&mut buf)?,
            security_proofs: Vec::<U256>::read(&mut buf)?,
        })
    }
}
