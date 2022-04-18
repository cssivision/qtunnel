use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde_derive::{Deserialize, Serialize};

use crate::other;

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub client: Option<Client>,
    pub server: Option<Server>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CongestionController {
    Bbr,
    Cubic,
    NewReno,
}

impl Default for CongestionController {
    fn default() -> Self {
        CongestionController::Bbr
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Server {
    pub local_addr: String,
    pub remote_addr: String,
    pub server_cert: String,
    pub server_key: String,
    pub congestion_controller: CongestionController,
}

impl Server {
    pub fn remote_socket_addrs(&self) -> Vec<Addr> {
        self.remote_addr
            .split(',')
            .map(|v| {
                if let Ok(addr) = v.parse() {
                    Addr::Socket(addr)
                } else {
                    Addr::Path(Path::new(v).to_path_buf())
                }
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Client {
    pub local_addr: String,
    pub remote_addr: String,
    pub domain_name: String,
    pub ca_certificate: String,
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

#[derive(Clone)]
pub enum Addr {
    Socket(SocketAddr),
    Path(PathBuf),
}
