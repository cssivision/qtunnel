use std::io;
use std::mem;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::sleep;

use crate::stream::Stream;

use crate::{other, ALPN_QUIC_HTTP};

const DELAY_MS: &[u64] = &[50, 75, 100, 250, 500, 750, 1000];

#[derive(Clone)]
pub struct Connection(Arc<Inner>);

struct Inner {
    cert: quinn::Certificate,
    addr: SocketAddr,
    domain_name: String,
    new_conn: Mutex<Option<quinn::NewConnection>>,
}

impl Connection {
    fn new(cert: quinn::Certificate, domain_name: String, addr: SocketAddr) -> Connection {
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

        if let Some(new_conn) = self.0.new_conn.lock().unwrap().as_ref() {
            match new_conn.connection.open_bi().await {
                Ok((send_stream, recv_stream)) => {
                    return Ok(Stream {
                        send_stream,
                        recv_stream,
                    });
                }
                Err(e) => {
                    log::error!("open bi fail {:?}", e);
                    return Err(other(&format!("open bi stream fail {:?}", e)));
                }
            }
        }
        unreachable!()
    }

    async fn connect(&self) -> quinn::NewConnection {
        let mut sleeps = 0;
        loop {
            let fut = async move {
                let mut endpoint = quinn::Endpoint::builder();
                let mut client_config = quinn::ClientConfigBuilder::default();
                client_config.protocols(ALPN_QUIC_HTTP);
                client_config
                    .add_certificate_authority(self.0.cert.clone())
                    .unwrap();
                endpoint.default_client_config(client_config.build());
                let (endpoint, _) = endpoint
                    .bind(
                        &"127.0.0.1:0"
                            .parse()
                            .map_err(|e| other(&format!("invalid bind addr {:?}", e)))?,
                    )
                    .map_err(|e| other(&format!("bind fail {:?}", e)))?;
                let new_conn = endpoint
                    .connect(&self.0.addr, &self.0.domain_name)
                    .map_err(|e| other(&format!("connect remote fail {:?}", e)))?
                    .await
                    .map_err(|e| other(&format!("new connection fail {:?}", e)));
                new_conn
            };

            match fut.await {
                Ok(v) => return v,
                Err(e) => {
                    log::trace!("reconnect err: {:?} fail: {:?}", self.0.addr, e);
                    let delay = DELAY_MS.get(sleeps as usize).unwrap_or(&1000);
                    sleeps += 1;
                    sleep(Duration::from_millis(*delay)).await;
                }
            }
        }
    }
}