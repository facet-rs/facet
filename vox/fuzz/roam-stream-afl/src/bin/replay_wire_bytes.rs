use std::fs;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use roam_stream::CobsFramed;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Duration, timeout};

struct ReplayStream {
    input: Vec<u8>,
    pos: usize,
}

impl ReplayStream {
    fn new(input: Vec<u8>) -> Self {
        Self { input, pos: 0 }
    }
}

impl AsyncRead for ReplayStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pos >= self.input.len() {
            return Poll::Ready(Ok(()));
        }
        let n = (self.input.len() - self.pos).min(buf.remaining());
        let start = self.pos;
        let end = start + n;
        buf.put_slice(&self.input[start..end]);
        self.pos = end;
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for ReplayStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/roam-wire-read-bytes.bin".to_string());
    let input = fs::read(path)?;
    eprintln!("replaying {} bytes", input.len());

    let stream = ReplayStream::new(input);
    let mut framed = CobsFramed::new(stream);

    for i in 0..1000usize {
        match timeout(Duration::from_millis(250), framed.recv()).await {
            Ok(Ok(Some(_msg))) => {}
            Ok(Ok(None)) => {
                eprintln!("eof after {i} iterations");
                return Ok(());
            }
            Ok(Err(e)) => {
                eprintln!("recv error at iter {i}: {e}");
                return Ok(());
            }
            Err(_) => {
                eprintln!("timeout/spin at iter {i}");
                return Ok(());
            }
        }
    }

    eprintln!("completed 1000 recv calls without timeout");
    Ok(())
}
