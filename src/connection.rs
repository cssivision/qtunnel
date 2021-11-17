use std::io;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use quinn::VarInt;
use tokio::time::sleep;
use tokio::time::timeout;

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
    new_conn: Mutex<Option<quinn::NewConnection>>,
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

    pub async fn new_stream(&self) -> io::Result<Stream> {
        if self.0.new_conn.lock().unwrap().is_none() {
            let new_conn = self.connect().await;
            let _ = mem::replace(&mut *self.0.new_conn.lock().unwrap(), Some(new_conn));
        }

        let open_bi = self
            .0
            .new_conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .connection
            .open_bi();

        match open_bi.await {
            Ok((send_stream, recv_stream)) => Ok(Stream {
                send_stream,
                recv_stream,
            }),
            Err(e) => {
                log::error!("open bi fail {:?}", e);
                let _ = mem::replace(&mut *self.0.new_conn.lock().unwrap(), None);
                Err(other(&format!("open bi stream fail {:?}", e)))
            }
        }
    }

    async fn connect(&self) -> quinn::NewConnection {
        let mut sleeps = 0;
        loop {
            let fut = async move {
                let mut certs = rustls::RootCertStore::empty();
                certs
                    .add(&self.0.cert)
                    .map_err(|e| other(&format!("add cert fail {:?}", e)))?;
                let mut client_config = quinn::ClientConfig::with_root_certificates(certs);
                let mut transport_config = quinn::TransportConfig::default();
                transport_config.keep_alive_interval(Some(DEFAULT_KEEP_ALIVE_INTERVAL));
                transport_config
                    .max_concurrent_bidi_streams(VarInt::from_u32(
                        DEFAULT_MAX_CONCURRENT_BIDI_STREAMS,
                    ))
                    .max_idle_timeout(Some(VarInt::from_u32(DEFAULT_MAX_IDLE_TIMEOUT).into()));
                client_config.transport = Arc::new(transport_config);
                let mut endpoint = quinn::Endpoint::client(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                    0,
                ))
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
