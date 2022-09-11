// we want to have helper functions to perform and verify incremental proofs of work
// which have a polynomial/quadratic time complexity and linear space complexity on the prover's side
// and constant space and time complexity on the verifier's side

// a good potential thing we could use is: https://en.wikipedia.org/wiki/Hashcash
// FIXME: the issue with hashcash is that it is in no way incremental

// FIXME: we can fix this by sending a list of following values that added to the previous hash
// FIXME: have one 0 more at the start than the previous hash

use rand::{random, Rng};
use ripemd::{Digest, Ripemd160, Ripemd320};
use sha2::Sha256;
use std::borrow::Cow;
use std::mem::transmute;

// this is a hashcash implementation based on the ripemd-160 hashing algorithm

fn security_level_num(input: u128) -> u8 {
    let mut hasher = Ripemd160::new();
    hasher.update(input.to_le_bytes());
    let result = hasher.finalize();
    let hash = result.as_slice();

    for x in hash.iter().enumerate() {
        if *x.1 != 0x0 {
            return x.1.leading_zeros() as u8 + x.0 as u8 * 8;
        }
    }
    // there are only zeros in this
    hash.len() as u8 * 8
}

/*
pub fn security_level_str(input: &str) -> u8 {
    let mut hasher = Ripemd160::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    security_level(result.as_slice())
}*/

/*
fn security_level(hash: &[u8]) -> u8 {
    for x in hash.iter().enumerate() {
        if *x.1 != 0x0 {
            return x.1.leading_zeros() as u8 + x.0 as u8 * 8;
        }
    }
    // there are no zeros in this at all
    hash.len() as u8 * 8
}*/

pub fn verified_security_level(uuid: u128, hashes: Vec<u128>) -> Option<u8> {
    let initial_hash = hash_sha(uuid) ^ hashes[0];

    if security_level_num(initial_hash) != 1 {
        return None;
    }

    let hashes_len = hashes.len();

    let mut curr = initial_hash;
    for x in hashes.into_iter().skip(1).enumerate() {
        curr = hash_sha(curr ^ x.1);
        // verify that the expected amount of leading zeros is present
        if x.0 + 1 != curr.leading_zeros() as usize {
            return None;
        }
    }

    Some(hashes_len as u8)
}

pub fn generate_token_num(req_level: u8, uuid: u128) -> u128 {
    loop {
        let token = random::<u128>();
        // we first hash the uuid here in order to prevent the possibility to
        // reverse the XOR operation we do
        let uuid_hashed = hash_sha(uuid);
        let security_level = security_level_num(uuid_hashed ^ token);

        if security_level >= req_level {
            return token;
        }
    }
}

fn hash_sha(val: u128) -> u128 {
    let mut hasher = Sha256::new();
    hasher.update(val.to_le_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    // SAFETY: It's safe to reinterpret 32 bytes as two consecutive 16 byte values
    let data: (u128, u128) = unsafe { transmute(bytes) };
    data.0 ^ data.1
}

// wgpu-biolerless
// wgpu-boilerless

fn hash(val: u128) -> [u8; 20] {
    let mut hasher = Ripemd160::new();
    hasher.update(val.to_le_bytes());
    hasher.finalize().into()
}

/*
pub fn generate_token_str<'a>(req_level: u8, prefix: Option<Cow<'a, &'a str>>, additional_len: usize) -> String {
    loop {
        let token = prefix.map_or_else(|| {
            get_random_string(additional_len)
        }, |prefix| {
            let rand = get_random_string(additional_len);
            let mut result = prefix.to_string();
            result.push_str(rand.as_str());
            result
        });
        let security_level = security_level_str(token.as_str());

        if security_level >= req_level {
            return token;
        }
    }
}
*/

/*
fn get_random_string(len: usize) -> String {
    rand::thread_rng()
        .sample_iter::<char, _>(rand::distributions::Standard)
        .take(len)
        .collect()
}*/
