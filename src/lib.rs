use std::io;

use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

use stream::Stream;

pub mod args;
pub mod config;
pub mod stream;

pub fn other(desc: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, desc)
}

pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

pub async fn proxy(mut socket: TcpStream, mut stream: Stream) {
    match copy_bidirectional(&mut socket, &mut stream).await {
        Ok((n1, n2)) => {
            log::debug!("proxy local => remote: {}, remote => local: {}", n1, n2);
        }
        Err(e) => {
            log::error!("copy_bidirectional err: {:?}", e);
        }
    }
}
