use std::io;

use qtunnel::args::parse_args;
use qtunnel::cert_from_pem;
use qtunnel::connection::Connection;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let config = parse_args("qtunnel-client").expect("invalid config");
    log::info!("{}", serde_json::to_string_pretty(&config).unwrap());

    let remote_addr = config.remote_addr.parse().expect("invalid remote addr");
    let cert = cert_from_pem(&config.ca_certificate)?;
    let conn = Connection::new(cert, config.domain_name, remote_addr);
    let listener = TcpListener::bind(&config.local_addr).await?;
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept tcp from {:?}", addr);
                let conn = conn.clone();
                tokio::spawn(async move {
                    if let Err(e) = proxy(stream, conn).await {
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

async fn proxy(socket: TcpStream, conn: Connection) -> io::Result<()> {
    let stream = conn.new_stream().await?;
    log::debug!("proxy to {:?}", stream.send_stream.id());
    qtunnel::proxy(socket, stream).await;
    Ok(())
}
