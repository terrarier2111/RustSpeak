use crate::protocol::UserUuid;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::sha::sha256;
use openssl::sign::Signer;
use std::mem::transmute;
use ruint::aliases::U256;

pub const PRIVATE_KEY_LEN_BITS: u32 = 4096;

// #[derive(Serialize, Deserialize)]
#[derive(Clone)]
pub struct Profile {
    pub name: String,
    private_key: Vec<u8>, // this private rsa key gets used to verify the ownership of the profile
    pub security_proofs: Vec<U256>,
}

impl Profile {
    #[inline]
    pub fn from_existing(name: String, private_key: Vec<u8>, security_proofs: Vec<U256>) -> Self {
        Self {
            name,
            private_key,
            security_proofs,
        }
    }

    pub fn uuid(&self) -> UserUuid {
        let pub_key = Rsa::private_key_from_der(&self.private_key)
            .unwrap()
            .public_key_to_der()
            .unwrap();
        let pub_hash = sha256(&pub_key);
        // SAFETY: This is safe because UserUuid can represent any 16 byte value
        unsafe { transmute(pub_hash) }
    }

    pub fn private_key(&self) -> Rsa<Private> {
        Rsa::private_key_from_der(&self.private_key).unwrap()
    }

    pub fn sign_data(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let pkey = PKey::private_key_from_der(&self.private_key)?;
        let mut signer = Signer::new(MessageDigest::sha256(), &pkey)?;
        Ok(signer.sign_oneshot_to_vec(data)?)
    }
}
