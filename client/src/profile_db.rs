use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
use std::sync::Mutex;
use crate::RWBytes;
use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use ruint::aliases::U256;
use sled::{Db, Iter, IVec};
use openssl::pkey::PKey;
use openssl::sha::sha256;
use serde_derive::{Deserialize, Serialize};
use crate::profile::PRIVATE_KEY_LEN_BITS;
use crate::security_level::{DEFAULT_SECURITY_LEVEL, generate_token_num};

pub struct ProfileDb {
    profiles: DashMap<String, DbProfile>,
    internal_cache: Mutex<Vec<RawDbProfile>>,
    path: String,
}

impl ProfileDb {
    pub fn new<F: FnOnce() -> anyhow::Result<Vec<DbProfile>>>(path: String, default: F) -> anyhow::Result<Self> {
        let result = match File::open(&path) {
            Ok(mut db_file) => {
                let mut content = String::new();
                db_file.read_to_string(&mut content)?;
                let profile_db: Vec<RawDbProfile> = serde_json::from_str(&content)?;
                profile_db
            }
            Err(_) => {
                let default = default()?.into_iter().map(|entry| RawDbProfile {
                    name: entry.name,
                    alias: entry.alias,
                    priv_key: entry.priv_key,
                    security_proofs: entry.security_proofs.into_iter().map(|entry| U256Container::new(entry)).collect::<Vec<_>>(),
                }).collect::<Vec<_>>();
                write(&path, &default)?;
                default
            }
        };
        let internal_cache = result.clone();
        let result = result.into_iter().map(|entry| DbProfile {
            name: entry.name,
            alias: entry.alias,
            priv_key: entry.priv_key,
            security_proofs: entry.security_proofs.into_iter().map(|entry| entry.unwrap()).collect::<Vec<_>>(),
        }).collect::<Vec<_>>();
        Ok(Self {
            profiles: {
                let mut profiles = DashMap::new();
                for entry in result {
                    profiles.insert(entry.name.clone(), entry);
                }
                profiles
            },
            internal_cache: Mutex::new(internal_cache),
            path,
        })
    }

    pub fn insert(&self, user: DbProfile) -> anyhow::Result<()> {
        let mut profiles = self.internal_cache.lock().unwrap();
        profiles.push(RawDbProfile {
            name: user.name.clone(),
            alias: user.alias.clone(),
            priv_key: user.priv_key.clone(),
            security_proofs: user.security_proofs.iter().map(|entry| U256Container::new(entry.clone())).collect::<Vec<_>>(),
        });
        write(&self.path, &profiles)?;
        self.profiles.insert(user.name.clone(), user);
        Ok(())
    }

    #[inline(always)]
    pub fn cache_ref(&self) -> &DashMap<String, DbProfile> {
        &self.profiles
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }
}

fn write(path: &String, profiles: &Vec<RawDbProfile>) -> anyhow::Result<()> {
    let val = serde_json::to_string(profiles)?;
    let mut file = File::create(path)?;
    file.write(val.as_bytes())?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct DbProfile {
    pub name: String, // this is the name that is used by the client
    pub alias: String, // this is the alias the server will see
    pub priv_key: Vec<u8>,
    pub security_proofs: Vec<U256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawDbProfile {
    name: String, // this is the name that is used by the client
    alias: String, // this is the alias the server will see
    priv_key: Vec<u8>,
    security_proofs: Vec<U256Container>,
}

pub fn uuid_from_pub_key(pub_key: &[u8]) -> U256 {
    let uuid = sha256(pub_key);
    U256::from_le_bytes(uuid) // FIXME: is this usage of LE stuff correct here?
}

impl DbProfile {
    pub fn new(name: String, alias: String) -> anyhow::Result<Self> {
        let keys = openssl::rsa::Rsa::generate(PRIVATE_KEY_LEN_BITS)?;
        let priv_key = PKey::from_rsa(keys)?;
        let uuid = uuid_from_pub_key(&*priv_key.public_key_to_der()?); // FIXME: IMPORTANT: (THIS COULD BE SECURITY RELEVANT) could we switch to using the raw public key instead of using the "der" version of it?
        let mut proofs = vec![];
        generate_token_num(DEFAULT_SECURITY_LEVEL, uuid, &mut proofs);
        Ok(Self {
            name,
            alias,
            priv_key: priv_key.private_key_to_der()?,
            security_proofs: proofs,
        })
    }

    pub fn pub_key(&self) -> anyhow::Result<Vec<u8>> {
        Ok(PKey::private_key_from_der(&self.priv_key)?.public_key_to_der()?)
    }

    pub fn uuid(&self) -> anyhow::Result<U256> {
        Ok(uuid_from_pub_key(&self.pub_key()?))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct U256Container {
    raw: [u8; size_of::<U256>()],
    #[serde(skip)]
    _align: [U256; 0],
}

impl U256Container {

    pub fn new(val: U256) -> Self {
        Self {
            raw: val.to_le_bytes(),
            _align: [],
        }
    }

    pub fn unwrap(&self) -> U256 {
        U256::from_le_bytes(self.raw.clone())
    }

}
