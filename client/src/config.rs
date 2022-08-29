use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config<'a> {
    entries: Vec<ServerEntry<'a>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerEntry<'a> {
    name: &'a str, // FIXME: should we use Cow?
    ip: IpAddr,
    port: u16,
}
