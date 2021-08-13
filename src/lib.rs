use std::io;

pub mod config;

pub fn other(desc: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, desc)
}

pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];
