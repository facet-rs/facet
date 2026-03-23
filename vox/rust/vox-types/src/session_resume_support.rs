use crate::{Metadata, MetadataEntry, MetadataFlags, MetadataValue, SessionResumeKey};

pub const SESSION_RESUME_KEY_METADATA_KEY: &str = "vox-session-key";

pub fn append_session_resume_key_metadata<'a>(
    metadata: &mut Metadata<'a>,
    key: &'a SessionResumeKey,
) {
    metadata.push(MetadataEntry {
        key: SESSION_RESUME_KEY_METADATA_KEY,
        value: MetadataValue::Bytes(&key.0),
        flags: MetadataFlags::NONE,
    });
}

pub fn metadata_session_resume_key(
    metadata: &[crate::MetadataEntry<'_>],
) -> Option<SessionResumeKey> {
    metadata.iter().find_map(|entry| {
        if entry.key != SESSION_RESUME_KEY_METADATA_KEY {
            return None;
        }
        match entry.value {
            MetadataValue::Bytes(bytes) if bytes.len() == 16 => {
                let mut key = [0u8; 16];
                key.copy_from_slice(bytes);
                Some(SessionResumeKey(key))
            }
            _ => None,
        }
    })
}
