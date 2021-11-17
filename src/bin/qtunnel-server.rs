use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::StreamExt;
use qtunnel::args::parse_args;
use qtunnel::stream::Stream;
use qtunnel::{
    cert_from_pem, other, private_key_from_pem, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_KEEP_ALIVE_INTERVAL, DEFAULT_MAX_CONCURRENT_BIDI_STREAMS, DEFAULT_MAX_IDLE_TIMEOUT,
};
use quinn::{
    Connecting, ConnectionError, Endpoint, NewConnection, ServerConfig, TransportConfig, VarInt,
};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let cfg = parse_args("qtunnel-server").expect("invalid config");
    log::info!("{}", serde_json::to_string_pretty(&cfg).unwrap());

    let key = private_key_from_pem(&cfg.server_key)?;
    let cert = cert_from_pem(&cfg.server_cert)?;
    let cert_chain = vec![cert];

    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(DEFAULT_KEEP_ALIVE_INTERVAL));
    transport_config
        .max_concurrent_bidi_streams(VarInt::from_u32(DEFAULT_MAX_CONCURRENT_BIDI_STREAMS))
        .max_idle_timeout(Some(VarInt::from_u32(DEFAULT_MAX_IDLE_TIMEOUT).into()));
    let mut server_config = ServerConfig::with_single_cert(cert_chain, key)
        .map_err(|e| other(&format!("new server config fail {:?}", e)))?;
    server_config.transport = Arc::new(transport_config);

    let local_addr = cfg
        .local_addr
        .parse()
        .map_err(|e| other(&format!("parse local addr fail {:?}", e)))?;
    let (endpoint, mut incoming) = Endpoint::server(server_config, local_addr)?;
    log::debug!("listening on {:?}", endpoint.local_addr()?);

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

async fn proxy(conn: Connecting, addrs: Vec<SocketAddr>) -> io::Result<()> {
    let NewConnection { mut bi_streams, .. } = conn
        .await
        .map_err(|e| other(&format!("bind local addr fail {:?}", e)))?;
    log::debug!("established");

    let mut next: usize = 0;

    // Each stream initiated by the client constitutes a new request.
    while let Some(stream) = bi_streams.next().await {
        match stream {
            Err(ConnectionError::ApplicationClosed { .. }) => {
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
