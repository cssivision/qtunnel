use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde_derive::{Deserialize, Serialize};

use crate::other;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub client: Option<Client>,
    pub server: Option<Server>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub enum CongestionController {
    #[default]
    Bbr,
    Cubic,
    NewReno,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Server {
    pub local_addr: SocketAddr,
    pub remote_addrs: Vec<Addr>,
    pub server_cert: String,
    pub server_key: String,
    #[serde(default)]
    pub congestion_controller: CongestionController,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Addr {
    Socket(SocketAddr),
    Path(PathBuf),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Client {
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub domain_name: String,
    pub ca_certificate: String,
    #[serde(default)]
    pub congestion_controller: CongestionController,
}

impl Config {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Config> {
        if path.as_ref().exists() {
            let contents = fs::read_to_string(path)?;
            let config: Config = match toml::from_str(&contents) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("{}", e);
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
            };
            if config.client.is_none() && config.server.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "client or server required",
                ));
            }
            return Ok(config);
        }
        Err(other(&format!("{:?} not exist", path.as_ref().to_str())))
    }
}
