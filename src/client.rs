use std::io;

use tokio::net::{TcpListener, TcpStream};

use crate::cert_from_pem;
use crate::config;
use crate::connection::Connection;

pub async fn run(cfg: config::Client) -> io::Result<()> {
    let cert = cert_from_pem(&cfg.ca_certificate)?;
    let conn = Connection::new(
        cert,
        cfg.domain_name,
        cfg.remote_addr,
        cfg.congestion_controller,
    )?;
    let listener = TcpListener::bind(&cfg.local_addr).await?;
    log::debug!("listening on {:?}", listener.local_addr()?);
    loop {
        let (mut stream, addr) = listener.accept().await?;
        log::debug!("accept tcp from {:?}", addr);
        let conn = conn.clone();
        tokio::spawn(async move {
            if let Err(e) = proxy(&mut stream, conn).await {
                log::error!("proxy error {:?}", e);
            }
        });
    }
}

async fn proxy(socket: &mut TcpStream, conn: Connection) -> io::Result<()> {
    let stream = conn.new_stream().await?;
    log::debug!("proxy to {:?}", stream.send_stream.id());
    crate::proxy(socket, stream).await;
    Ok(())
}
