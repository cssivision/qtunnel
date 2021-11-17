use std::fs;
use std::io;
use std::time::Duration;

use rustls::{Certificate, PrivateKey};
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

use stream::Stream;

pub mod args;
pub mod config;
pub mod connection;
pub mod stream;

pub fn private_key_from_pem(server_key: &str) -> io::Result<PrivateKey> {
    let pem = fs::read(server_key).map_err(|e| other(&format!("read server key fail {:?}", e)))?;
    let pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut &pem[..])
        .map_err(|e| other(&format!("malformed PKCS #8 private key {:?}", e)))?;
    if let Some(x) = pkcs8.into_iter().next() {
        return Ok(PrivateKey(x));
    }
    let rsa = rustls_pemfile::rsa_private_keys(&mut &pem[..])
        .map_err(|e| other(&format!("malformed PKCS #1 private key {:?}", e)))?;
    if let Some(x) = rsa.into_iter().next() {
        return Ok(PrivateKey(x));
    }
    Err(other("no private key found"))
}

pub fn cert_from_pem(cert_path: &str) -> io::Result<Certificate> {
    let cert =
        fs::read(&cert_path).map_err(|e| other(&format!("read server cert fail {:?}", e)))?;
    let certs = rustls_pemfile::certs(&mut &cert[..])
        .map_err(|e| other(&format!("extract cert fail {:?}", e)))?;
    if let Some(pem) = certs.into_iter().next() {
        return Ok(Certificate(pem));
    }
    Err(other("no cert found"))
}

pub fn other(desc: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, desc)
}

pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_IDLE_TIMEOUT: u32 = 30_000;
pub const DEFAULT_MAX_CONCURRENT_BIDI_STREAMS: u32 = 2048;

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
