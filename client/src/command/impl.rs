use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::sync::Arc;
use openssl::pkey::PKey;
use crate::{Client, CommandImpl, DbProfile, generate_token_num, uuid_from_pub_key};

pub struct CommandProfiles();

impl CommandImpl for CommandProfiles {
    fn execute(&self, client: &Arc<Client>, input: &[&str]) -> anyhow::Result<()> {
        match input[0] {
            "create" => {
                if client.profile_db.get(&input[1].to_string())?.is_some() {
                    return Err(anyhow::Error::from(ProfileAlreadyExistsError(input[1].to_string())));
                }
                client.profile_db.insert(DbProfile::new(input[1].to_string(), input[1].to_string())?)?; // FIXME: support custom alias!
                client.println(format!("A profile with the name {} was created.", input[1]).as_str());
            },
            "list" => {
                client.println(format!("There are {} profiles:", client.profile_db.len()).as_str());
                // println!("Name   UUID   SecLevel"); // FIXME: adjust this and try using it for more graceful profile display
                for profile in client.profile_db.iter() {
                    let profile = DbProfile::from_bytes(profile?.1)?;
                    client.println(format!("{:?}", profile).as_str());
                }
            },
            "bump_sl" => {
                if let Some(mut profile) = client.profile_db.get(&input[1].to_string())? {
                    let req_lvl = input[2].parse::<u8>()?;
                    let priv_key = PKey::private_key_from_der(&*profile.priv_key)?;
                    let pub_key = priv_key.public_key_to_der()?;
                    generate_token_num(req_lvl, uuid_from_pub_key(&*pub_key), &mut profile.security_proofs);
                    client.profile_db.insert(profile)?;
                    client.println(format!("Successfully levelled up security level to {}", req_lvl).as_str());
                } else {
                    println!("Couldn't find profile {}", input[1]);
                }
            }
            _ => {}
        }

        Ok(())
    }
}

struct ProfileAlreadyExistsError(String);

impl Debug for ProfileAlreadyExistsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("a profile with the name ")?;
        f.write_str(&*self.0)?;
        f.write_str(" already exists!")
    }
}

impl Display for ProfileAlreadyExistsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("a profile with the name ")?;
        f.write_str(&*self.0)?;
        f.write_str(" already exists!")
    }
}

impl Error for ProfileAlreadyExistsError {}