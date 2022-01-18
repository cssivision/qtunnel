use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::Path;

use serde_derive::{Deserialize, Serialize};

use crate::other;

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub local_addr: String,
    pub remote_addr: String,
    pub domain_name: String,
    pub ca_certificate: String,
    pub server_cert: String,
    pub server_key: String,
}

impl Config {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Config> {
        if path.as_ref().exists() {
            let contents = fs::read_to_string(path)?;
            let config = match serde_json::from_str(&contents) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("{}", e);
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
            };

            return Ok(config);
        }
        Err(other(&format!("{:?} not exist", path.as_ref().to_str())))
    }

    pub fn remote_socket_addrs(&self) -> Vec<Addr> {
        self.remote_addr
            .split(',')
            .filter_map(|v| {
                if let Ok(addr) = v.parse() {
                    Some(Addr::Socket(addr))
                } else {
                    Some(Addr::Unix(v.to_string()))
                }
            })
            .collect()
    }
}

#[derive(Clone)]
pub enum Addr {
    Socket(SocketAddr),
    Unix(String),
}
