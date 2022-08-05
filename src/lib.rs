use std::fs;
use std::future::Future;
use std::io;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;
use rustls::{Certificate, PrivateKey};
use tokio::io::{copy_bidirectional, AsyncRead, AsyncWrite};
use tokio::time::{sleep, Duration, Instant, Sleep};

use stream::Stream;

pub mod args;
pub mod client;
pub mod config;
pub mod connection;
pub mod server;
pub mod stream;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_IDLE_TIMEOUT: u32 = 30_000;
pub const DEFAULT_MAX_CONCURRENT_BIDI_STREAMS: u32 = 2048;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DEFAULT_VISITED_GAP: Duration = Duration::from_secs(3);

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

pub async fn proxy<A>(socket: &mut A, mut stream: Stream)
where
    A: AsyncRead + AsyncWrite + Unpin + ?Sized,
{
    match IdleTimeout::new(
        copy_bidirectional(socket, &mut stream),
        DEFAULT_IDLE_TIMEOUT,
    )
    .await
    {
        Ok(v) => match v {
            Ok((n1, n2)) => {
                log::debug!("proxy local => remote: {}, remote => local: {}", n1, n2);
                return;
            }
            Err(e) => {
                log::error!("copy_bidirectional err: {:?}", e);
            }
        },
        Err(_) => {
            log::error!("copy_bidirectional idle timeout");
        }
    }
    stream.reset();
}

pin_project! {
    /// A future with timeout time set
    pub struct IdleTimeout<S: Future> {
        #[pin]
        inner: S,
        #[pin]
        sleep: Sleep,
        idle_timeout: Duration,
        last_visited: Instant,
    }
}

impl<S: Future> IdleTimeout<S> {
    pub fn new(inner: S, idle_timeout: Duration) -> Self {
        let sleep = sleep(idle_timeout);

        Self {
            inner,
            sleep,
            idle_timeout,
            last_visited: Instant::now(),
        }
    }
}

impl<S: Future> Future for IdleTimeout<S> {
    type Output = io::Result<S::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        match this.inner.poll(cx) {
            Poll::Ready(v) => Poll::Ready(Ok(v)),
            Poll::Pending => match Pin::new(&mut this.sleep).poll(cx) {
                Poll::Ready(_) => Poll::Ready(Err(io::ErrorKind::TimedOut.into())),
                Poll::Pending => {
                    let now = Instant::now();
                    if now.sub(*this.last_visited) >= DEFAULT_VISITED_GAP {
                        *this.last_visited = now;
                        this.sleep.reset(now.add(*this.idle_timeout));
                    }
                    Poll::Pending
                }
            },
        }
    }
}
