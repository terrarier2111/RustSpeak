use crate::RWBytes;
use bytes::{Bytes, BytesMut};
use ruint::aliases::U256;
use sled::{Db, Iter, IVec};
use openssl::pkey::PKey;
use openssl::sha::sha256;
use crate::profile::PRIVATE_KEY_LEN_BITS;
use crate::security_level::generate_token_num;

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

    pub fn iter(&self) -> Iter {
        self.db.iter()
    }

    pub fn len(&self) -> usize {
        self.db.len()
    }
}

#[derive(Debug)]
pub struct DbProfile {
    pub priv_key: Vec<u8>,
    pub name: String,
    pub security_proofs: Vec<U256>,
}

pub fn uuid_from_pub_key(pub_key: &[u8]) -> U256 {
    let uuid = sha256(pub_key);
    U256::from_le_bytes(uuid) // FIXME: is this usage of LE stuff correct here?
}

impl DbProfile {
    pub fn new(name: String) -> anyhow::Result<Self> {
        let keys = openssl::rsa::Rsa::generate(PRIVATE_KEY_LEN_BITS)?;
        let priv_key = PKey::from_rsa(keys)?;
        let uuid = uuid_from_pub_key(&*priv_key.public_key_to_der()?); // FIXME: IMPORTANT: (THIS COULD BE SECURITY RELEVANT) could we switch to using the raw public key instead of using the "der" version of it?
        let mut proofs = vec![];
        generate_token_num(1, uuid, &mut proofs);
        Ok(Self {
            name,
            priv_key: priv_key.private_key_to_der()?,
            security_proofs: proofs,
        })
    }

    fn to_bytes(self) -> anyhow::Result<IVec> {
        let mut buf = BytesMut::new();
        self.priv_key.write(&mut buf)?;
        self.name.write(&mut buf)?;
        self.security_proofs.write(&mut buf)?;
        Ok(IVec::from(buf.to_vec()))
    }

    pub(crate) fn from_bytes(bytes: IVec) -> anyhow::Result<Self> {
        let mut buf = Bytes::from(bytes.to_vec());
        Ok(Self {
            priv_key: Vec::<u8>::read(&mut buf)?,
            name: String::read(&mut buf)?,
            security_proofs: Vec::<U256>::read(&mut buf)?,
        })
    }
}
