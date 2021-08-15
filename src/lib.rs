use std::io;
use std::time::Duration;

use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

use stream::Stream;

pub mod args;
pub mod config;
pub mod connection;
pub mod stream;

pub fn other(desc: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, desc)
}

pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_CONCURRENT_BIDI_STREAMS: u64 = 65536;

pub async fn proxy(mut socket: TcpStream, mut stream: Stream) {
    match copy_bidirectional(&mut socket, &mut stream).await {
        Ok((n1, n2)) => {
            log::debug!("proxy local => remote: {}, remote => local: {}", n1, n2);
        }
        Err(e) => {
            stream.reset();
            log::error!("copy_bidirectional err: {:?}", e);
        }
    }
}
