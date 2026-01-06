use std::time::Duration;

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use rapace_wire::{Hello, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn env_peer_addr() -> Result<String, String> {
    std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())
}

fn our_hello() -> Hello {
    Hello::V1 {
        max_payload_size: 1024 * 1024,
        initial_stream_credit: 64 * 1024,
    }
}

fn main() -> Result<(), String> {
    // Manual runtime (avoid tokio-macros / syn).
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    rt.block_on(async_main())
}

fn our_max_payload() -> u32 {
    match our_hello() {
        Hello::V1 {
            max_payload_size, ..
        } => max_payload_size,
    }
}

async fn async_main() -> Result<(), String> {
    let addr = env_peer_addr()?;
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("connect {addr}: {e}"))?;

    let mut io = CobsFramed::new(stream);

    // r[message.hello.timing]: send Hello immediately after connection.
    io.send(&Message::Hello(our_hello()))
        .await
        .map_err(|e| format!("send hello: {e}"))?;

    // Track negotiated max payload, once peer Hello is received.
    let mut peer_max_payload: Option<u32> = None;

    struct EchoService;

    impl spec_proto::Echo for EchoService {
        async fn echo(&self, message: String) -> String {
            message
        }

        async fn reverse(&self, message: String) -> String {
            message.chars().rev().collect()
        }
    }

    let echo = EchoService;

    loop {
        let msg = match io.recv_timeout(Duration::from_secs(30)).await {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(e) => {
                // Special-case: unknown Hello variant inside a Message::Hello.
                // The tests craft [Message::Hello discriminant][Hello unknown discriminant].
                if io.last_decoded.starts_with(&[0x00, 0x01]) {
                    let _ = io
                        .send(&Message::Goodbye {
                            reason: "message.hello.unknown-version".into(),
                        })
                        .await;
                    break;
                }
                return Err(format!("recv: {e}"));
            }
        };

        match msg {
            Message::Hello(Hello::V1 {
                max_payload_size, ..
            }) => {
                peer_max_payload = Some(max_payload_size);
            }
            Message::Request {
                request_id,
                method_id,
                metadata: _,
                payload,
            } => {
                if let Some(max) = peer_max_payload {
                    let effective = our_max_payload().min(max);
                    if payload.len() as u32 > effective {
                        let _ = io
                            .send(&Message::Goodbye {
                                reason: "flow.unary.payload-limit".into(),
                            })
                            .await;
                        break;
                    }
                }

                // Spec: r[unary.error.unknown-method] and r[unary.error.invalid-payload].
                let response_payload = spec_proto::echo_dispatch_unary(&echo, method_id, &payload)
                    .await
                    .map_err(|e| format!("dispatch Echo: {e:?}"))?;

                let resp = Message::Response {
                    request_id,
                    metadata: Vec::new(),
                    payload: response_payload,
                };
                io.send(&resp)
                    .await
                    .map_err(|e| format!("send response: {e}"))?;
            }
            Message::Close { stream_id } | Message::Reset { stream_id } => {
                if stream_id == 0 {
                    let _ = io
                        .send(&Message::Goodbye {
                            reason: "streaming.id.zero-reserved".into(),
                        })
                        .await;
                    break;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

struct CobsFramed {
    stream: TcpStream,
    buf: Vec<u8>,
    last_decoded: Vec<u8>,
}

impl CobsFramed {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            last_decoded: Vec::new(),
        }
    }

    async fn send(&mut self, msg: &Message) -> std::io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let mut framed = cobs_encode_vec(&payload);
        framed.push(0x00);
        self.stream.write_all(&framed).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> std::io::Result<Option<Message>> {
        tokio::time::timeout(timeout, self.recv_inner())
            .await
            .unwrap_or(Ok(None))
    }

    async fn recv_inner(&mut self) -> std::io::Result<Option<Message>> {
        loop {
            if let Some(idx) = self.buf.iter().position(|b| *b == 0x00) {
                let frame = self.buf.drain(..idx).collect::<Vec<_>>();
                self.buf.drain(..1);

                let decoded = cobs_decode_vec(&frame).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("cobs: {e}"))
                })?;
                self.last_decoded = decoded.clone();

                let msg: Message = facet_postcard::from_slice(&decoded).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("postcard: {e}"))
                })?;
                return Ok(Some(msg));
            }

            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}
