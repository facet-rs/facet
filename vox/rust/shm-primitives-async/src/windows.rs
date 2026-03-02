pub mod doorbell;
pub mod mmap_control;

pub use doorbell::{
    Doorbell, DoorbellHandle, SignalResult, close_handle, set_handle_inheritable, validate_handle,
};

pub use mmap_control::{
    MmapControlHandle, MmapControlReceiver, MmapControlSender, create_mmap_control_pair,
    create_mmap_control_pair_connected, create_mmap_control_receiver_server,
};

/// No-op on Windows (equivalent of Unix `clear_cloexec`).
pub fn clear_cloexec(_: ()) -> std::io::Result<()> {
    Ok(())
}

/// 16-byte metadata sent alongside each mmap region attachment.
///
/// Mirrors the Unix `MmapAttachMessage` so that in-process channels and
/// data structures compile on Windows without any Unix-specific socket code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct MmapAttachMessage {
    pub map_id: u32,
    pub map_generation: u32,
    pub mapping_length: u64,
}

impl MmapAttachMessage {
    pub fn to_le_bytes(self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.map_id.to_le_bytes());
        buf[4..8].copy_from_slice(&self.map_generation.to_le_bytes());
        buf[8..16].copy_from_slice(&self.mapping_length.to_le_bytes());
        buf
    }

    pub fn from_le_bytes(buf: [u8; 16]) -> Self {
        Self {
            map_id: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            map_generation: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            mapping_length: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
        }
    }
}
