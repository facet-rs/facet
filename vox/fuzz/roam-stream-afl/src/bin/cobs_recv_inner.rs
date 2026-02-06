use afl::fuzz;
use cobs::encode_vec as cobs_encode_vec;
use roam_stream::CobsFramed;
use roam_wire::{ConnectionId, Message};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::runtime::Builder;
use tokio::time::{Duration, timeout};

struct ChunkedStream {
    input: Vec<u8>,
    pos: usize,
    chunk_pattern: [usize; 2],
    chunk_ix: usize,
}

impl ChunkedStream {
    fn new(input: Vec<u8>, first_chunk: usize, second_chunk: usize) -> Self {
        Self {
            input,
            pos: 0,
            chunk_pattern: [first_chunk.max(1), second_chunk.max(1)],
            chunk_ix: 0,
        }
    }
}

impl AsyncRead for ChunkedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pos >= self.input.len() {
            return Poll::Ready(Ok(()));
        }

        let chunk = self.chunk_pattern[self.chunk_ix % self.chunk_pattern.len()];
        self.chunk_ix += 1;

        let remaining = self.input.len() - self.pos;
        let n = chunk.min(remaining).min(buf.remaining());
        let start = self.pos;
        let end = start + n;
        buf.put_slice(&self.input[start..end]);
        self.pos = end;
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for ChunkedStream {
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

fn fuzz_recv_chunked(data: &[u8]) {
    if data.len() < 3 {
        return;
    }

    let Ok(runtime) = Builder::new_current_thread().enable_all().build() else {
        return;
    };

    runtime.block_on(async {
        let first_chunk = (data[0] as usize % 256) + 1;
        let second_chunk = (data[1] as usize % 256) + 1;
        let stream_data = data[2..].to_vec();
        let stream = ChunkedStream::new(stream_data, first_chunk, second_chunk);
        let mut framed = CobsFramed::new(stream);

        let _ = timeout(Duration::from_millis(20), framed.recv()).await;
    });
}

fn fuzz_large_valid_message(payload: &[u8]) {
    let Ok(runtime) = Builder::new_current_thread().enable_all().build() else {
        return;
    };

    runtime.block_on(async {
        let message = Message::Response {
            conn_id: ConnectionId::ROOT,
            request_id: 1,
            metadata: vec![],
            channels: vec![],
            payload: payload.to_vec(),
        };

        let Ok(postcard) = facet_postcard::to_vec(&message) else {
            return;
        };
        let mut framed_bytes = cobs_encode_vec(&postcard);
        framed_bytes.push(0x00);

        let stream = ChunkedStream::new(framed_bytes, 4096, 1536);
        let mut receiver = CobsFramed::new(stream);
        let _ = timeout(Duration::from_millis(20), receiver.recv()).await;
    });
}

fn main() {
    fuzz!(|data: &[u8]| {
        fuzz_recv_chunked(data);
        fuzz_large_valid_message(data);
    });
}
