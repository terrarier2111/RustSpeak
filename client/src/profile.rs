use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sha::sha256;
use openssl::sign::Signer;
use serde_derive::{Deserialize, Serialize};
use std::mem::transmute;
use uuid::Uuid;
use crate::protocol::UserUuid;

const PRIVATE_KEY_LEN_BITS: u32 = 4096;

// #[derive(Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    uuid: UserUuid,
    private_key: Vec<u8>, // this private rsa key gets used to verify the ownership of the profile
    pub security_proofs: Vec<u128>,
}

impl Profile {
    pub fn new(name: String) -> anyhow::Result<Self> {
        let keys = openssl::rsa::Rsa::generate(PRIVATE_KEY_LEN_BITS)?;
        let priv_key = PKey::from_rsa(keys)?;
        let pub_key = priv_key.rsa()?.public_key_to_der()?;
        let pub_hash = sha256(&pub_key);
        // SAFETY: This is safe because UserUuid can represent any 16 byte value
        let uuid = unsafe { transmute(pub_hash) };
        Ok(Self {
            name,
            uuid,
            private_key: priv_key.private_key_to_der()?,
            security_proofs: vec![],
        })
    }

    pub fn from_existing(
        name: String,
        uuid: UserUuid,
        private_key: Vec<u8>,
        security_proofs: Vec<u128>,
    ) -> Self {
        Self {
            name,
            uuid,
            private_key,
            security_proofs,
        }
    }

    #[inline(always)]
    pub fn uuid(&self) -> &UserUuid {
        &self.uuid
    }

    #[inline(always)]
    pub fn private_key(&self) -> &Vec<u8> {
        &self.private_key
    }
}
