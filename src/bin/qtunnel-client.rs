use std::fs;
use std::io;
use std::sync::Arc;

use qtunnel::args::parse_args;
use qtunnel::stream::Stream;
use qtunnel::{other, ALPN_QUIC_HTTP};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();
    let config = parse_args("qtunnel-client").expect("invalid config");
    log::info!("{}", serde_json::to_string_pretty(&config).unwrap());
    let mut endpoint = quinn::Endpoint::builder();
    let mut client_config = quinn::ClientConfigBuilder::default();
    client_config.protocols(ALPN_QUIC_HTTP);
    client_config
        .add_certificate_authority(
            quinn::Certificate::from_pem(&fs::read(config.ca_certificate).expect("read ca fail"))
                .unwrap(),
        )
        .unwrap();
    endpoint.default_client_config(client_config.build());
    let (endpoint, _) = endpoint
        .bind(
            &"127.0.0.1:0"
                .parse()
                .map_err(|e| other(&format!("invalid bind addr {:?}", e)))?,
        )
        .unwrap();
    let local_addr = config.local_addr.parse().expect("invalid remote addr");
    let new_conn = endpoint
        .connect(&local_addr, &config.domain_name)
        .map_err(|e| other(&format!("connect remote fail {:?}", e)))?
        .await
        .map_err(|e| other(&format!("new connection fail {:?}", e)))?;
    let new_conn = Arc::new(new_conn);
    let listener = TcpListener::bind(&config.local_addr).await?;
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept tcp from {:?}", addr);
                let new_conn = new_conn.clone();
                tokio::spawn(async move {
                    if let Err(e) = proxy(stream, new_conn).await {
                        log::error!("proxy error {:?}", e);
                    }
                });
            }
            Err(e) => {
                log::error!("accept fail: {:?}", e);
            }
        }
    }
}

async fn proxy(socket: TcpStream, new_conn: Arc<quinn::NewConnection>) -> io::Result<()> {
    log::debug!("new stream");
    let (send_stream, recv_stream) = new_conn
        .connection
        .open_bi()
        .await
        .map_err(|e| other(&e.to_string()))?;
    log::debug!("proxy to {:?}", send_stream.id());
    let stream = Stream {
        send_stream,
        recv_stream,
    };
    qtunnel::proxy(socket, stream).await;
    Ok(())
}
