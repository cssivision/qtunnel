use std::io;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use quinn::{ClientConfig, Endpoint, NewConnection, OpenBi, TransportConfig, VarInt};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::stream::Stream;
use crate::{
    other, DEFAULT_CONNECT_TIMEOUT, DEFAULT_KEEP_ALIVE_INTERVAL,
    DEFAULT_MAX_CONCURRENT_BIDI_STREAMS, DEFAULT_MAX_IDLE_TIMEOUT,
};

const DELAY_MS: &[u64] = &[50, 75, 100, 250, 500, 750, 1000];

#[derive(Clone)]
pub struct Connection(Arc<Inner>);

struct Inner {
    cert: rustls::Certificate,
    addr: SocketAddr,
    domain_name: String,
    new_conn: Mutex<Option<NewConnection>>,
}

impl Connection {
    pub fn new(cert: rustls::Certificate, domain_name: String, addr: SocketAddr) -> Connection {
        Connection(Arc::new(Inner {
            addr,
            domain_name,
            cert,
            new_conn: Mutex::new(None),
        }))
    }

    async fn open_bi(&self) -> OpenBi {
        let mut lock = self.0.new_conn.lock().await;
        if lock.is_none() {
            let new_conn = self.connect().await;
            let _ = mem::replace(&mut *lock, Some(new_conn));
        }
        lock.as_ref().unwrap().connection.open_bi()
    }

    pub async fn new_stream(&self) -> io::Result<Stream> {
        let open_bi = self.open_bi().await;

        match open_bi.await {
            Ok((send_stream, recv_stream)) => Ok(Stream {
                send_stream,
                recv_stream,
            }),
            Err(e) => {
                log::error!("open bi fail {:?}", e);
                let _ = mem::replace(&mut *self.0.new_conn.lock().await, None);
                Err(other(&format!("open bi stream fail {:?}", e)))
            }
        }
    }

    async fn connect(&self) -> NewConnection {
        let mut sleeps = 0;
        loop {
            let fut = async move {
                let mut certs = rustls::RootCertStore::empty();
                certs
                    .add(&self.0.cert)
                    .map_err(|e| other(&format!("add cert fail {:?}", e)))?;
                let mut client_config = ClientConfig::with_root_certificates(certs);
                let mut transport_config = TransportConfig::default();
                transport_config.keep_alive_interval(Some(DEFAULT_KEEP_ALIVE_INTERVAL));
                transport_config
                    .max_concurrent_bidi_streams(VarInt::from_u32(
                        DEFAULT_MAX_CONCURRENT_BIDI_STREAMS,
                    ))
                    .max_idle_timeout(Some(VarInt::from_u32(DEFAULT_MAX_IDLE_TIMEOUT).into()));
                client_config.transport = Arc::new(transport_config);
                let mut endpoint =
                    Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0))
                        .map_err(|e| other(&format!("bind fail {:?}", e)))?;
                endpoint.set_default_client_config(client_config);
                let new_conn = endpoint
                    .connect(self.0.addr, &self.0.domain_name)
                    .map_err(|e| other(&format!("connect remote fail {:?}", e)))?
                    .await
                    .map_err(|e| other(&format!("new connection fail {:?}", e)));
                new_conn
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
