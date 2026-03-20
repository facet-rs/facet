use crate::{Metadata, MetadataEntry, MetadataFlags, MetadataValue};

pub const RETRY_SUPPORT_METADATA_KEY: &str = "roam-retry-support";
pub const OPERATION_ID_METADATA_KEY: &str = "roam-operation-id";
pub const RETRY_SUPPORT_VERSION: u64 = 1;

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
/// separately, deduplicated by SchemaHash.
#[derive(Clone, Debug)]
pub struct PostcardPayload(pub Vec<u8>);

impl PostcardPayload {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

pub fn append_retry_support_metadata(metadata: &mut Metadata<'_>) {
    if metadata_supports_retry(metadata) {
        return;
    }
    metadata.push(MetadataEntry {
        key: RETRY_SUPPORT_METADATA_KEY,
        value: MetadataValue::U64(RETRY_SUPPORT_VERSION),
        flags: MetadataFlags::NONE,
    });
}

pub fn metadata_supports_retry(metadata: &[MetadataEntry<'_>]) -> bool {
    metadata.iter().any(|entry| {
        entry.key == RETRY_SUPPORT_METADATA_KEY
            && matches!(entry.value, MetadataValue::U64(RETRY_SUPPORT_VERSION))
    })
}

pub fn metadata_operation_id(metadata: &[MetadataEntry<'_>]) -> Option<OperationId> {
    metadata.iter().find_map(|entry| {
        if entry.key != OPERATION_ID_METADATA_KEY {
            return None;
        }
        match entry.value {
            MetadataValue::U64(value) => Some(OperationId(value)),
            _ => None,
        }
    })
}

pub fn ensure_operation_id(metadata: &mut Metadata<'_>, operation_id: OperationId) {
    if metadata_operation_id(metadata).is_some() {
        return;
    }
    metadata.push(MetadataEntry {
        key: OPERATION_ID_METADATA_KEY,
        value: MetadataValue::U64(operation_id.0),
        flags: MetadataFlags::NONE,
    });
}
