use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use quinn::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const ERROR_CODE_RESET: u32 = 100;

pub struct Stream {
    pub send_stream: SendStream,
    pub recv_stream: RecvStream,
}

impl Stream {
    pub fn reset(&mut self) {
        let _ = self.send_stream.reset(ERROR_CODE_RESET.into());
        let _ = self.recv_stream.stop(ERROR_CODE_RESET.into());
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.recv_stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.send_stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send_stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send_stream).poll_shutdown(cx)
    }
}
