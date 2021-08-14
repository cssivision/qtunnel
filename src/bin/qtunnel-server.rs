use std::fs;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::StreamExt;
use qtunnel::args::parse_args;
use qtunnel::stream::Stream;
use qtunnel::{
    other, ALPN_QUIC_HTTP, DEFAULT_CONNECT_TIMEOUT, DEFAULT_KEEP_ALIVE_INTERVAL,
    DEFAULT_MAX_IDLE_TIMEOUT,
};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let cfg = parse_args("qtunnel-server").expect("invalid config");
    log::info!("{}", serde_json::to_string_pretty(&cfg).unwrap());

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.keep_alive_interval(Some(DEFAULT_KEEP_ALIVE_INTERVAL));
    transport_config
        .max_idle_timeout(Some(DEFAULT_MAX_IDLE_TIMEOUT))
        .map_err(|e| other(&format!("transport set max_idle_timeout fail {:?}", e)))?;
    let mut server_config = quinn::ServerConfig::default();
    server_config.transport = Arc::new(transport_config);
    let mut server_config = quinn::ServerConfigBuilder::new(server_config);
    server_config.protocols(ALPN_QUIC_HTTP);

    let key =
        fs::read(&cfg.server_key).map_err(|e| other(&format!("read server key fail {:?}", e)))?;
    let key = quinn::PrivateKey::from_pem(&key)
        .map_err(|e| other(&format!("parse server key fail {:?}", e)))?;
    let cert =
        fs::read(&cfg.server_cert).map_err(|e| other(&format!("read server cert fail {:?}", e)))?;
    let cert = quinn::CertificateChain::from_pem(&cert)
        .map_err(|e| other(&format!("parse server cert fail {:?}", e)))?;
    server_config.certificate(cert, key).unwrap();

    let mut endpoint = quinn::Endpoint::builder();
    endpoint.listen(server_config.build());
    let local_addr = cfg
        .local_addr
        .parse()
        .map_err(|e| other(&format!("parse local addr fail {:?}", e)))?;
    let (endpoint, mut incoming) = endpoint
        .bind(&local_addr)
        .map_err(|e| other(&format!("bind local addr fail {:?}", e)))?;
    log::debug!("listening on {:?}", endpoint.local_addr());

    let remote_addrs = cfg.remote_socket_addrs();
    while let Some(conn) = incoming.next().await {
        log::info!("connection incoming");
        let remote_addrs = remote_addrs.clone();
        tokio::spawn(async move {
            if let Err(e) = proxy(conn, remote_addrs).await {
                log::error!("proxy connection fail: {:?}", e);
            }
        });
    }
    Ok(())
}

async fn proxy(conn: quinn::Connecting, addrs: Vec<SocketAddr>) -> io::Result<()> {
    let quinn::NewConnection { mut bi_streams, .. } = conn
        .await
        .map_err(|e| other(&format!("bind local addr fail {:?}", e)))?;
    log::debug!("established");

    let mut next: usize = 0;

    // Each stream initiated by the client constitutes a new request.
    while let Some(stream) = bi_streams.next().await {
        match stream {
            Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                return Err(other("connection closed"));
            }
            Err(e) => {
                return Err(other(&format!("connection err: {:?}", e)));
            }
            Ok((send_stream, recv_stream)) => {
                log::debug!("new stream incoming {}", send_stream.id());
                next = next.wrapping_add(1);
                let current = next % addrs.len();
                let addr = addrs[current];
                tokio::spawn(async move {
                    let stream = Stream {
                        send_stream,
                        recv_stream,
                    };
                    proxy_stream(stream, addr).await
                });
            }
        };
    }
    Ok(())
}

async fn proxy_stream(mut stream: Stream, addr: SocketAddr) {
    match timeout(DEFAULT_CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(conn) => {
            match conn {
                Ok(conn) => {
                    qtunnel::proxy(conn, stream).await;
                }
                Err(e) => {
                    log::error!("connect to {} err {:?}", &addr, e);
                }
            };
        }
        Err(e) => {
            stream.reset();
            log::error!("connect to {} err {:?}", &addr, e);
        }
    }
}
