use crate::{Metadata, MetadataEntry, MetadataFlags, MetadataValue};

pub const RETRY_SUPPORT_METADATA_KEY: &str = "roam-retry-support";
pub const OPERATION_ID_METADATA_KEY: &str = "roam-operation-id";
pub const RETRY_SUPPORT_VERSION: u64 = 1;

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

pub fn metadata_operation_id(metadata: &[MetadataEntry<'_>]) -> Option<u64> {
    metadata.iter().find_map(|entry| {
        if entry.key != OPERATION_ID_METADATA_KEY {
            return None;
        }
        match entry.value {
            MetadataValue::U64(value) => Some(value),
            _ => None,
        }
    })
}

pub fn ensure_operation_id(metadata: &mut Metadata<'_>, operation_id: u64) {
    if metadata_operation_id(metadata).is_some() {
        return;
    }
    metadata.push(MetadataEntry {
        key: OPERATION_ID_METADATA_KEY,
        value: MetadataValue::U64(operation_id),
        flags: MetadataFlags::NONE,
    });
}
