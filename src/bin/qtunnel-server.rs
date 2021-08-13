use std::fs;
use std::io;
use std::sync::Arc;

use futures_util::StreamExt;
use qtunnel::ALPN_QUIC_HTTP;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_uni_streams(0).unwrap();
    let mut server_config = quinn::ServerConfig::default();
    server_config.transport = Arc::new(transport_config);
    let mut server_config = quinn::ServerConfigBuilder::new(server_config);
    server_config.protocols(ALPN_QUIC_HTTP);

    let key = fs::read("tls_config/mydomain.com.key").unwrap();
    let key = quinn::PrivateKey::from_pem(&key).unwrap();
    let cert_chain = fs::read("tls_config/mydomain.com.crt").unwrap();
    let cert_chain = quinn::CertificateChain::from_pem(&cert_chain).unwrap();
    server_config.certificate(cert_chain, key).unwrap();

    let mut endpoint = quinn::Endpoint::builder();
    endpoint.listen(server_config.build());

    let (endpoint, mut incoming) = endpoint.bind(&"127.0.0.1:8091".parse().unwrap()).unwrap();
    log::debug!("listening on {}", endpoint.local_addr().unwrap());

    while let Some(conn) = incoming.next().await {
        log::info!("connection incoming");
        tokio::spawn(handle_connection(conn));
    }
    Ok(())
}

async fn handle_connection(conn: quinn::Connecting) {
    let quinn::NewConnection { mut bi_streams, .. } = conn.await.unwrap();
    log::info!("established");
    // Each stream initiated by the client constitutes a new request.
    while let Some(stream) = bi_streams.next().await {
        let stream = match stream {
            Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                log::info!("connection closed");
                return;
            }
            Err(e) => {
                log::info!("connection err: {:?}", e);
                return;
            }
            Ok(s) => s,
        };
        tokio::spawn(handle_request(stream));
    }
}

async fn handle_request((mut send, recv): (quinn::SendStream, quinn::RecvStream)) {
    let req = recv.read_to_end(64 * 1024).await.unwrap();
    send.write_all(&req).await.unwrap();
    send.finish().await.unwrap();
    log::info!("complete");
}
