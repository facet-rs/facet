use bytes::Bytes;

use crate::{Metadata, MetadataEntry, MetadataFlags, MetadataValue};

pub const RETRY_SUPPORT_METADATA_KEY: &str = "vox-retry-support";
pub const OPERATION_ID_METADATA_KEY: &str = "vox-operation-id";
pub const CHANNEL_RETRY_MODE_METADATA_KEY: &str = "vox-channel-retry-mode";
pub const RETRY_SUPPORT_VERSION: u64 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChannelRetryMode {
    None = 0,
    NonIdem = 1,
    Idem = 2,
}

/// A unique operation identifier for exactly-once delivery.
///
/// Operation IDs are assigned by the client and carried in request metadata.
/// They survive across disconnects — the operation store uses them to
/// deduplicate and replay sealed responses after session resumption.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct OperationId(pub u64);

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Postcard-encoded bytes for a response payload (without schemas).
///
/// This is the format stored in the operation store. Schemas are stored
/// separately, deduplicated by SchemaHash. Backed by `Bytes` so the buffer
/// can be shared (cheap arc-clone) between the wire-send path and the
/// retry store without copying.
#[derive(Clone, Debug)]
pub struct PostcardPayload(pub Bytes);

impl PostcardPayload {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for PostcardPayload {
    fn from(v: Vec<u8>) -> Self {
        Self(Bytes::from(v))
    }
}

impl From<Bytes> for PostcardPayload {
    fn from(b: Bytes) -> Self {
        Self(b)
    }
}

pub fn append_retry_support_metadata(metadata: &mut Metadata<'_>) {
    if metadata_supports_retry(metadata) {
        return;
    }
    metadata.push(MetadataEntry {
        key: RETRY_SUPPORT_METADATA_KEY.into(),
        value: MetadataValue::U64(RETRY_SUPPORT_VERSION),
        flags: MetadataFlags::NONE,
    });
}

pub fn metadata_supports_retry(metadata: &[MetadataEntry<'_>]) -> bool {
    metadata.iter().any(|entry| {
        entry.key == RETRY_SUPPORT_METADATA_KEY
            && matches!(&entry.value, MetadataValue::U64(v) if *v == RETRY_SUPPORT_VERSION)
    })
}

pub fn metadata_operation_id(metadata: &[MetadataEntry<'_>]) -> Option<OperationId> {
    metadata.iter().find_map(|entry| {
        if entry.key != OPERATION_ID_METADATA_KEY {
            return None;
        }
        match &entry.value {
            MetadataValue::U64(value) => Some(OperationId(*value)),
            _ => None,
        }
    })
}

pub fn ensure_operation_id(metadata: &mut Metadata<'_>, operation_id: OperationId) {
    if metadata_operation_id(metadata).is_some() {
        return;
    }
    metadata.push(MetadataEntry {
        key: OPERATION_ID_METADATA_KEY.into(),
        value: MetadataValue::U64(operation_id.0),
        flags: MetadataFlags::NONE,
    });
}

pub fn metadata_channel_retry_mode(metadata: &[MetadataEntry<'_>]) -> ChannelRetryMode {
    metadata
        .iter()
        .find_map(|entry| {
            if entry.key != CHANNEL_RETRY_MODE_METADATA_KEY {
                return None;
            }
            match &entry.value {
                MetadataValue::U64(1) => Some(ChannelRetryMode::NonIdem),
                MetadataValue::U64(2) => Some(ChannelRetryMode::Idem),
                _ => Some(ChannelRetryMode::None),
            }
        })
        .unwrap_or(ChannelRetryMode::None)
}

pub fn ensure_channel_retry_mode(metadata: &mut Metadata<'_>, mode: ChannelRetryMode) {
    if matches!(mode, ChannelRetryMode::None) {
        return;
    }
    if metadata
        .iter()
        .any(|entry| entry.key == CHANNEL_RETRY_MODE_METADATA_KEY)
    {
        return;
    }
    metadata.push(MetadataEntry {
        key: CHANNEL_RETRY_MODE_METADATA_KEY.into(),
        value: MetadataValue::U64(mode as u64),
        flags: MetadataFlags::NONE,
    });
}
