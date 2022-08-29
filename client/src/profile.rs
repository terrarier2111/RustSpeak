use uuid::Uuid;

pub struct Profile {
    pub name: String,
    uuid: Uuid,
    private_key: Uuid,
    pub security_proofs: Vec<u128>,
}

impl Profile {

    pub fn new(name: String) -> Self {
        Self {
            name,
            uuid: Uuid::new_v4(),
            private_key: Uuid::new_v4(),
            security_proofs: vec![],
        }
    }

    pub fn from_existing(name: String, uuid: Uuid, private_key: Uuid, security_proofs: Vec<u128>) -> Self {
        Self {
            name,
            uuid,
            private_key,
            security_proofs,
        }
    }

    #[inline(always)]
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    #[inline(always)]
    pub fn private_key(&self) -> &Uuid {
        &self.private_key
    }

}