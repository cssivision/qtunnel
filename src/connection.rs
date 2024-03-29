use std::io;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use quinn::{congestion, ClientConfig, Endpoint, TransportConfig, VarInt};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::config::CongestionController;
use crate::stream::Stream;
use crate::{
    other, DEFAULT_CONNECT_TIMEOUT, DEFAULT_KEEP_ALIVE_INTERVAL,
    DEFAULT_MAX_CONCURRENT_BIDI_STREAMS, DEFAULT_MAX_IDLE_TIMEOUT,
};

const DELAY_MS: &[u64] = &[50, 75, 100, 250, 500, 750, 1000];
const OPEN_BI_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct Connection(Arc<Inner>);

struct Inner {
    addr: SocketAddr,
    domain_name: String,
    conn: Mutex<Option<quinn::Connection>>,
    client_config: ClientConfig,
}

impl Connection {
    pub fn new(
        cert: rustls::Certificate,
        domain_name: String,
        addr: SocketAddr,
        cc: CongestionController,
    ) -> io::Result<Connection> {
        let mut certs = rustls::RootCertStore::empty();
        certs
            .add(&cert)
            .map_err(|e| other(&format!("add cert fail {e:?}")))?;
        let mut client_config = ClientConfig::with_root_certificates(certs);
        let mut transport_config = TransportConfig::default();
        transport_config.keep_alive_interval(Some(DEFAULT_KEEP_ALIVE_INTERVAL));
        transport_config
            .max_concurrent_bidi_streams(VarInt::from_u32(DEFAULT_MAX_CONCURRENT_BIDI_STREAMS))
            .max_idle_timeout(Some(VarInt::from_u32(DEFAULT_MAX_IDLE_TIMEOUT).into()));
        match cc {
            CongestionController::Bbr => {
                transport_config
                    .congestion_controller_factory(Arc::new(congestion::BbrConfig::default()));
            }
            CongestionController::NewReno => {
                transport_config
                    .congestion_controller_factory(Arc::new(congestion::NewRenoConfig::default()));
            }
            CongestionController::Cubic => {
                transport_config
                    .congestion_controller_factory(Arc::new(congestion::CubicConfig::default()));
            }
        }
        client_config.transport_config(Arc::new(transport_config));
        Ok(Connection(Arc::new(Inner {
            addr,
            domain_name,
            conn: Mutex::new(None),
            client_config,
        })))
    }

    pub async fn new_stream(&self) -> io::Result<Stream> {
        let mut lock = self.0.conn.lock().await;
        if lock.is_none() {
            let new_conn = self.connect().await;
            let _ = mem::replace(&mut *lock, Some(new_conn));
        }
        let open_bi = lock.as_ref().unwrap().open_bi();

        match timeout(OPEN_BI_TIMEOUT, open_bi).await {
            Ok(open_bi) => match open_bi {
                Ok((send_stream, recv_stream)) => Ok(Stream {
                    send_stream,
                    recv_stream,
                }),
                Err(e) => {
                    log::error!("open bi fail {:?}", e);
                    let _ = mem::replace(&mut *self.0.conn.lock().await, None);
                    Err(other(&format!("open bi stream fail {e:?}")))
                }
            },
            Err(e) => {
                log::error!("open bi timeout {:?}", e);
                let _ = mem::replace(&mut *self.0.conn.lock().await, None);
                Err(other(&format!("open bi timeout {e:?}")))
            }
        }
    }

    async fn connect(&self) -> quinn::Connection {
        let mut sleeps = 0;
        loop {
            let fut = async move {
                let mut endpoint =
                    Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0))
                        .map_err(|e| other(&format!("bind fail {e:?}")))?;
                endpoint.set_default_client_config(self.0.client_config.clone());
                endpoint
                    .connect(self.0.addr, &self.0.domain_name)
                    .map_err(|e| other(&format!("connect remote fail {e:?}")))?
                    .await
                    .map_err(|e| other(&format!("new connection fail {e:?}")))
            };

            match timeout(DEFAULT_CONNECT_TIMEOUT, fut).await {
                Ok(v) => match v {
                    Ok(v) => return v,
                    Err(e) => {
                        log::error!("reconnect {:?} fail {:?}", self.0.addr, e);
                    }
                },
                Err(e) => {
                    log::error!("connect remote timeout {}", e);
                }
            }
            let delay = DELAY_MS.get(sleeps as usize).unwrap_or(&1000);
            sleeps += 1;
            sleep(Duration::from_millis(*delay)).await;
        }
    }
}
