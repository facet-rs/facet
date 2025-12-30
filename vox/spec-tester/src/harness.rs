//! Reference peer implementation.
//!
//! This module provides async stdin/stdout frame I/O for the reference peer.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::protocol::{INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, MsgDescHot};

/// Default timeout for receiving frames.
pub const DEFAULT_RECV_TIMEOUT: Duration = Duration::from_secs(5);

/// A frame for transmission.
#[derive(Debug, Clone)]
pub struct Frame {
    pub desc: MsgDescHot,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame with inline payload.
    pub fn inline(mut desc: MsgDescHot, payload: &[u8]) -> Self {
        assert!(
            payload.len() <= INLINE_PAYLOAD_SIZE,
            "payload too large for inline"
        );
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
        Self {
            desc,
            payload: Vec::new(),
        }
    }

    /// Create a new frame with external payload.
    pub fn with_payload(mut desc: MsgDescHot, payload: Vec<u8>) -> Self {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Self { desc, payload }
    }

    /// Get payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        if self.desc.payload_slot == INLINE_PAYLOAD_SLOT {
            &self.desc.inline_payload[..self.desc.payload_len as usize]
        } else {
            &self.payload
        }
    }
}

/// Reference peer that communicates via stdin/stdout.
pub struct Peer {
    stdin: tokio::io::Stdin,
    stdout: tokio::io::Stdout,
}

impl Peer {
    pub fn new() -> Self {
        Self {
            stdin: tokio::io::stdin(),
            stdout: tokio::io::stdout(),
        }
    }

    /// Send a frame to the implementation.
    ///
    /// Note: StreamTransport sends inline payloads as separate bytes after the descriptor,
    /// even though they're also in desc.inline_payload. We match this behavior for compatibility.
    pub async fn send(&mut self, frame: &Frame) -> std::io::Result<()> {
        let payload = if frame.desc.payload_slot == INLINE_PAYLOAD_SLOT {
            &frame.desc.inline_payload[..frame.desc.payload_len as usize]
        } else {
            &frame.payload
        };

        let total_len = 64 + payload.len();

        // Write length prefix
        self.stdout
            .write_all(&(total_len as u32).to_le_bytes())
            .await?;

        // Write descriptor
        self.stdout.write_all(&frame.desc.to_bytes()).await?;

        // Write payload bytes (StreamTransport sends inline payloads here too)
        if !payload.is_empty() {
            self.stdout.write_all(payload).await?;
        }

        self.stdout.flush().await?;
        Ok(())
    }

    /// Receive a frame from the implementation with default timeout.
    pub async fn recv(&mut self) -> std::io::Result<Frame> {
        self.recv_timeout(DEFAULT_RECV_TIMEOUT).await
    }

    /// Receive a frame from the implementation with specified timeout.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> std::io::Result<Frame> {
        match tokio::time::timeout(timeout, self.recv_inner()).await {
            Ok(result) => result,
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("recv timed out after {:?}", timeout),
            )),
        }
    }

    async fn recv_inner(&mut self) -> std::io::Result<Frame> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        self.stdin.read_exact(&mut len_buf).await?;
        let total_len = u32::from_le_bytes(len_buf) as usize;

        if total_len < 64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too short: {} bytes", total_len),
            ));
        }

        // Read descriptor
        let mut desc_buf = [0u8; 64];
        self.stdin.read_exact(&mut desc_buf).await?;
        let desc = MsgDescHot::from_bytes(&desc_buf);

        // Read external payload if present
        let payload = if total_len > 64 {
            let mut payload = vec![0u8; total_len - 64];
            self.stdin.read_exact(&mut payload).await?;
            payload
        } else {
            Vec::new()
        };

        Ok(Frame { desc, payload })
    }

    /// Try to receive a frame with timeout.
    /// Returns Ok(None) if stdin is closed (EOF) or timeout.
    pub async fn try_recv(&mut self) -> std::io::Result<Option<Frame>> {
        self.try_recv_timeout(DEFAULT_RECV_TIMEOUT).await
    }

    /// Try to receive a frame with specified timeout.
    /// Returns Ok(None) if stdin is closed (EOF) or timeout.
    pub async fn try_recv_timeout(&mut self, timeout: Duration) -> std::io::Result<Option<Frame>> {
        match tokio::time::timeout(timeout, self.try_recv_inner()).await {
            Ok(result) => result,
            Err(_) => Ok(None), // Timeout returns None, not error
        }
    }

    async fn try_recv_inner(&mut self) -> std::io::Result<Option<Frame>> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        match self.stdin.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        let total_len = u32::from_le_bytes(len_buf) as usize;

        if total_len < 64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too short: {} bytes", total_len),
            ));
        }

        // Read descriptor
        let mut desc_buf = [0u8; 64];
        self.stdin.read_exact(&mut desc_buf).await?;
        let desc = MsgDescHot::from_bytes(&desc_buf);

        // Read external payload if present
        let payload = if total_len > 64 {
            let mut payload = vec![0u8; total_len - 64];
            self.stdin.read_exact(&mut payload).await?;
            payload
        } else {
            Vec::new()
        };

        Ok(Some(Frame { desc, payload }))
    }
}

impl Default for Peer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::flags;

    #[test]
    fn test_frame_inline() {
        let mut desc = MsgDescHot::new();
        desc.msg_id = 1;
        desc.flags = flags::DATA;

        let frame = Frame::inline(desc, b"hello");
        assert_eq!(frame.payload_bytes(), b"hello");
        assert!(frame.payload.is_empty()); // External payload empty for inline
    }

    #[test]
    fn test_frame_external() {
        let mut desc = MsgDescHot::new();
        desc.msg_id = 1;

        let payload = vec![0u8; 100];
        let frame = Frame::with_payload(desc, payload.clone());
        assert_eq!(frame.payload_bytes(), &payload);
    }
}
