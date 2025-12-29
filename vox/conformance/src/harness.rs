//! Reference peer implementation.
//!
//! This module provides the stdin/stdout frame I/O for the reference peer.

use std::io::{self, Read, Write};

use crate::protocol::{INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, MsgDescHot};

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
    stdin: io::Stdin,
    stdout: io::Stdout,
}

impl Peer {
    pub fn new() -> Self {
        Self {
            stdin: io::stdin(),
            stdout: io::stdout(),
        }
    }

    /// Send a frame to the implementation.
    pub fn send(&mut self, frame: &Frame) -> io::Result<()> {
        let payload = if frame.desc.payload_slot == INLINE_PAYLOAD_SLOT {
            &[] as &[u8]
        } else {
            &frame.payload
        };

        let total_len = 64 + payload.len();

        // Write length prefix
        self.stdout.write_all(&(total_len as u32).to_le_bytes())?;

        // Write descriptor
        self.stdout.write_all(&frame.desc.to_bytes())?;

        // Write payload (if external)
        if !payload.is_empty() {
            self.stdout.write_all(payload)?;
        }

        self.stdout.flush()?;
        Ok(())
    }

    /// Receive a frame from the implementation.
    pub fn recv(&mut self) -> io::Result<Frame> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        self.stdin.read_exact(&mut len_buf)?;
        let total_len = u32::from_le_bytes(len_buf) as usize;

        if total_len < 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame too short: {} bytes", total_len),
            ));
        }

        // Read descriptor
        let mut desc_buf = [0u8; 64];
        self.stdin.read_exact(&mut desc_buf)?;
        let desc = MsgDescHot::from_bytes(&desc_buf);

        // Read external payload if present
        let payload = if total_len > 64 {
            let mut payload = vec![0u8; total_len - 64];
            self.stdin.read_exact(&mut payload)?;
            payload
        } else {
            Vec::new()
        };

        Ok(Frame { desc, payload })
    }

    /// Try to receive a frame with a timeout indication.
    /// Returns Ok(None) if stdin is closed (EOF).
    pub fn try_recv(&mut self) -> io::Result<Option<Frame>> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        match self.stdin.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        let total_len = u32::from_le_bytes(len_buf) as usize;

        if total_len < 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame too short: {} bytes", total_len),
            ));
        }

        // Read descriptor
        let mut desc_buf = [0u8; 64];
        self.stdin.read_exact(&mut desc_buf)?;
        let desc = MsgDescHot::from_bytes(&desc_buf);

        // Read external payload if present
        let payload = if total_len > 64 {
            let mut payload = vec![0u8; total_len - 64];
            self.stdin.read_exact(&mut payload)?;
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
