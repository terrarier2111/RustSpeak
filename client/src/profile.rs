use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private, Public};
use openssl::sha::sha256;
use openssl::sign::Signer;
use serde_derive::{Deserialize, Serialize};
use std::mem::transmute;
use openssl::rsa::Rsa;
use uuid::Uuid;
use crate::protocol::UserUuid;

const PRIVATE_KEY_LEN_BITS: u32 = 4096;

// #[derive(Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    private_key: Vec<u8>, // this private rsa key gets used to verify the ownership of the profile
    pub security_proofs: Vec<u128>,
}

impl Profile {
    pub fn new(name: String) -> anyhow::Result<Self> {
        let keys = openssl::rsa::Rsa::generate(PRIVATE_KEY_LEN_BITS)?;
        let priv_key = PKey::from_rsa(keys)?;
        Ok(Self {
            name,
            private_key: priv_key.private_key_to_der()?,
            security_proofs: vec![],
        })
    }

    pub fn from_existing(
        name: String,
        private_key: Vec<u8>,
        security_proofs: Vec<u128>,
    ) -> Self {
        Self {
            name,
            private_key,
            security_proofs,
        }
    }

    pub fn uuid(&self) -> UserUuid {
        let pub_key = Rsa::private_key_from_der(&self.private_key)?.public_key_to_der()?;
        let pub_hash = sha256(&pub_key);
        // SAFETY: This is safe because UserUuid can represent any 16 byte value
        unsafe { transmute(pub_hash) }
    }

    pub fn private_key(&self) -> anyhow::Result<Rsa<Private>> {
        Ok(Rsa::private_key_from_der(&self.private_key)??)
    }

    pub fn sign_data(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let pkey = PKey::private_key_from_der(&self.private_key)?;
        let mut signer = Signer::new(MessageDigest::sha256(), &pkey)?;
        Ok(signer.sign_oneshot_to_vec(data)?)
    }
}
