//! Metadata: a self-describing key→value map carried on the wire as a dynamic
//! [`Value`] (`r[rpc.metadata]`). Values are strings, byte runs, or `u64`s.
//!
//! There are no duplicate keys (a later write for a key replaces the earlier one).
//! Per-key handling conventions are encoded directly in the key string: a leading
//! `#` marks the value sensitive, `-` marks it no-propagate, and `-#` does both.
//!
//! Build metadata with the fluent [`metadata`] builder; read it through the
//! [`MetadataExt`] accessors. Construction leans on `Default` — an absent metadata
//! field is just `Value::default()` (null), which reads as empty.

use facet_value::{VBytes, VObject, VString, Value};

/// Metadata is a self-describing [`Value`] — an object of string keys to values
/// (string / bytes / `u64`), or null when empty.
// r[impl rpc.metadata]
// r[impl rpc.metadata.keys]
// r[impl rpc.metadata.unknown]
// r[impl schema.interaction.metadata]
pub type Metadata = Value;

/// Insert (or replace) `key`→`value` into a metadata [`Value`], creating the object
/// if needed. The construction primitive the builder and the middleware
/// push-helpers share.
// r[impl rpc.metadata.value]
// r[impl rpc.metadata.duplicates]
pub fn meta_set(metadata: &mut Metadata, key: &str, value: impl Into<Value>) {
    if metadata.as_object().is_none() {
        *metadata = Value::from(VObject::new());
    }
    let obj = metadata
        .as_object_mut()
        .expect("metadata was just made an object");
    obj.insert(VString::new(key), value.into());
}

/// Whether `key` uses the metadata sensitive sigil (`#key` or `-#key`).
// r[impl rpc.metadata.sigils]
#[must_use]
pub fn metadata_key_is_redacted(key: &str) -> bool {
    key.strip_prefix('-').unwrap_or(key).starts_with('#')
}

/// Whether `key` uses the metadata no-propagate sigil (`-key` or `-#key`).
// r[impl rpc.metadata.sigils]
#[must_use]
pub fn metadata_key_is_no_propagate(key: &str) -> bool {
    key.starts_with('-')
}

/// Start building metadata fluently: `metadata().str("trace", "abc").u64("n", 5).build()`.
#[must_use]
pub fn metadata() -> MetadataBuilder {
    MetadataBuilder {
        obj: VObject::new(),
    }
}

/// Fluent builder producing a metadata [`Value`] object.
pub struct MetadataBuilder {
    obj: VObject,
}

impl MetadataBuilder {
    /// Add (or replace) a string entry.
    #[must_use]
    pub fn str(mut self, key: impl Into<VString>, value: impl Into<VString>) -> Self {
        self.obj.insert(key, Value::from(value.into()));
        self
    }

    /// Add (or replace) a `u64` entry.
    #[must_use]
    pub fn u64(mut self, key: impl Into<VString>, value: u64) -> Self {
        self.obj.insert(key, Value::from(value));
        self
    }

    /// Add (or replace) a byte-run entry.
    #[must_use]
    pub fn bytes(mut self, key: impl Into<VString>, value: impl Into<VBytes>) -> Self {
        self.obj.insert(key, Value::from(value.into()));
        self
    }

    /// Finish building, returning the metadata [`Value`].
    #[must_use]
    pub fn build(self) -> Metadata {
        Value::from(self.obj)
    }
}

/// Read accessors for a metadata [`Value`]. Implemented for [`Value`]; a null
/// value reads as empty.
pub trait MetadataExt {
    /// The string value at `key`, if present and a string.
    fn meta_str(&self, key: &str) -> Option<&str>;
    /// The `u64` value at `key`, if present and a number.
    fn meta_u64(&self, key: &str) -> Option<u64>;
    /// The byte-run value at `key`, if present and bytes.
    fn meta_bytes(&self, key: &str) -> Option<&[u8]>;
    /// Whether there are no metadata entries.
    fn meta_is_empty(&self) -> bool;
    /// The number of entries (0 when null).
    fn meta_len(&self) -> usize;
    /// Iterate the `(key, value)` entries.
    fn meta_entries(&self) -> Vec<(&str, &Value)>;
}

impl MetadataExt for Value {
    fn meta_str(&self, key: &str) -> Option<&str> {
        self.as_object()?.get(key)?.as_string().map(VString::as_str)
    }

    fn meta_u64(&self, key: &str) -> Option<u64> {
        self.as_object()?.get(key)?.as_number()?.to_u64()
    }

    fn meta_bytes(&self, key: &str) -> Option<&[u8]> {
        self.as_object()?.get(key)?.as_bytes().map(VBytes::as_slice)
    }

    fn meta_is_empty(&self) -> bool {
        self.meta_len() == 0
    }

    fn meta_len(&self) -> usize {
        self.as_object().map_or(0, VObject::len)
    }

    fn meta_entries(&self) -> Vec<(&str, &Value)> {
        match self.as_object() {
            Some(obj) => obj.iter().map(|(k, v)| (k.as_str(), v)).collect(),
            None => Vec::new(),
        }
    }
}

// ----------------------------------------------------------------------------
// Compatibility shims (delegate to the builder / accessors)
// ----------------------------------------------------------------------------

/// Look up a string metadata value by key.
pub fn metadata_get_str<'a>(metadata: &'a Metadata, key: &str) -> Option<&'a str> {
    metadata.meta_str(key)
}

/// Look up a `u64` metadata value by key.
pub fn metadata_get_u64(metadata: &Metadata, key: &str) -> Option<u64> {
    metadata.meta_u64(key)
}

/// Metadata is already an owned [`Value`]; conversion is the identity.
#[must_use]
pub fn metadata_into_owned(metadata: Metadata) -> Metadata {
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify rpc.metadata]
    // r[verify rpc.metadata.value]
    // r[verify rpc.metadata.keys]
    // r[verify rpc.metadata.duplicates]
    // r[verify rpc.metadata.unknown]
    // r[verify schema.interaction.metadata]
    #[test]
    fn builder_and_accessors_round_trip() {
        let m = metadata()
            .str("trace", "abc")
            .u64("n", 99)
            .bytes("blob", &[1u8, 2, 3][..])
            .str("Trace", "case-sensitive")
            .str("unknown-key", "ignored unless read explicitly")
            .str("trace", "replacement")
            .build();

        assert_eq!(m.meta_str("trace"), Some("replacement"));
        assert_eq!(m.meta_str("Trace"), Some("case-sensitive"));
        assert_eq!(m.meta_str("TRACE"), None);
        assert_eq!(m.meta_u64("n"), Some(99));
        assert_eq!(m.meta_bytes("blob"), Some(&[1u8, 2, 3][..]));
        let entries: Vec<&str> = m.meta_entries().into_iter().map(|(k, _)| k).collect();
        assert_eq!(entries.len(), 5);
        assert!(entries.contains(&"trace"));
        assert!(entries.contains(&"Trace"));
        assert!(entries.contains(&"unknown-key"));
        assert!(entries.contains(&"n"));
        assert!(entries.contains(&"blob"));
    }

    // r[verify rpc.metadata.sigils]
    #[test]
    fn key_sigils_are_conventions_on_the_key_string() {
        assert!(!metadata_key_is_redacted("regular.metadata"));
        assert!(!metadata_key_is_no_propagate("regular.metadata"));

        assert!(metadata_key_is_redacted("#sensitive.metadata"));
        assert!(!metadata_key_is_no_propagate("#sensitive.metadata"));

        assert!(!metadata_key_is_redacted("-no-propagate-metadata"));
        assert!(metadata_key_is_no_propagate("-no-propagate-metadata"));

        assert!(metadata_key_is_redacted(
            "-#sensitive-and-no-propagate-metadata"
        ));
        assert!(metadata_key_is_no_propagate(
            "-#sensitive-and-no-propagate-metadata"
        ));
    }

    #[test]
    fn default_is_empty() {
        let m = Metadata::default();
        assert!(m.meta_is_empty());
        assert_eq!(m.meta_len(), 0);
        assert_eq!(m.meta_str("x"), None);
    }
}
