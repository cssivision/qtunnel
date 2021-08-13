use std::fs;
use std::io::{self, Write};

use qtunnel::ALPN_QUIC_HTTP;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let mut endpoint = quinn::Endpoint::builder();
    let mut client_config = quinn::ClientConfigBuilder::default();
    client_config.protocols(ALPN_QUIC_HTTP);
    client_config
        .add_certificate_authority(
            quinn::Certificate::from_pem(&fs::read("tls_config/rootCA.crt")?).unwrap(),
        )
        .unwrap();

    endpoint.default_client_config(client_config.build());
    let (endpoint, _) = endpoint.bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let new_conn = endpoint
        .connect(&"127.0.0.1:8091".parse().unwrap(), "mydomain.com")
        .unwrap()
        .await
        .unwrap();

    let quinn::NewConnection {
        connection: conn, ..
    } = new_conn;
    let (mut send, recv) = conn.open_bi().await.unwrap();

    send.write_all(b"helloworld").await.unwrap();
    send.finish().await.unwrap();
    let resp = recv.read_to_end(usize::max_value()).await.unwrap();
    io::stdout().write_all(&resp).unwrap();
    io::stdout().flush().unwrap();
    conn.close(0u32.into(), b"done");

    // Give the server a fair chance to receive the close packet
    endpoint.wait_idle().await;

    Ok(())
}
