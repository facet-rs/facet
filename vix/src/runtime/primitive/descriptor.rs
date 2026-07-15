use std::fmt;

use crate::runtime::identity::{Digest, SchemaId, hash_framed};
use taxon::SchemaId as TaxonSchemaId;

pub const RESERVED_NAMES: &[&str] = &[
    "None",
    "Some",
    "by_key",
    "range",
    "expect",
    "expect_eq",
    "expect_ne",
    "expect_some",
    "expect_none",
    "expect_snapshot",
    "demanded",
    "json_decode",
    "toml_decode",
    "scheduler_requests_at_most",
    "memo_entries_at_most",
    "store_interns_at_most",
];

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PrimitiveName(String);

impl PrimitiveName {
    pub fn new(name: &str) -> Result<Self, RegistrationError> {
        if RESERVED_NAMES.contains(&name) {
            return Err(RegistrationError::ReservedName {
                name: name.to_owned(),
            });
        }
        if !is_valid_primitive_name(name) {
            return Err(RegistrationError::InvalidName {
                name: name.to_owned(),
            });
        }
        Ok(Self(name.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrimitiveName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_valid_primitive_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_lowercase()) {
        return false;
    }
    chars.all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PrimitiveId(pub Digest);

impl PrimitiveId {
    #[must_use]
    pub fn derive(
        name: &PrimitiveName,
        version: u32,
        protocol: u32,
        request: TaxonSchemaId,
        response: TaxonSchemaId,
    ) -> Self {
        Self(hash_framed(
            b"vix.primitive.v1",
            &[
                name.as_str().as_bytes(),
                &version.to_le_bytes(),
                &protocol.to_le_bytes(),
                &request.as_u64().to_le_bytes(),
                &response.as_u64().to_le_bytes(),
            ],
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoPolicy {
    Hermetic,
    Pinned,
    Observed,
    Volatile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredSchema {
    pub taxon_root: TaxonSchemaId,
    pub taxon_schemas: Vec<taxon::Schema>,
    pub vix_type: crate::vir::Type,
    pub store_schema: SchemaId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    pub id: PrimitiveId,
    pub name: PrimitiveName,
    pub version: u32,
    pub protocol: u32,
    pub request: RegisteredSchema,
    pub response: RegisteredSchema,
    pub policy: MemoPolicy,
    pub capabilities: Vec<CapabilityRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityRequirement {
    pub identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationError {
    InvalidName { name: String },
    ReservedName { name: String },
    DuplicateName { name: String },
    UnsupportedShape { path: String, kind: String },
    Derive { message: String },
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { name } => write!(f, "invalid primitive name: {name}"),
            Self::ReservedName { name } => write!(f, "reserved primitive name: {name}"),
            Self::DuplicateName { name } => write!(f, "duplicate primitive name: {name}"),
            Self::UnsupportedShape { path, kind } => {
                write!(f, "unsupported primitive schema shape at {path}: {kind}")
            }
            Self::Derive { message } => write!(f, "primitive schema derivation failed: {message}"),
        }
    }
}

impl std::error::Error for RegistrationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_id_rekeys_on_every_descriptor_axis() {
        let name = PrimitiveName::new("probe_version").unwrap();
        let other = PrimitiveName::new("probe_other").unwrap();
        let req = taxon::SchemaId::from_raw(11);
        let resp = taxon::SchemaId::from_raw(22);
        let base = PrimitiveId::derive(&name, 1, 1, req, resp);
        assert_eq!(base, PrimitiveId::derive(&name, 1, 1, req, resp));
        assert_ne!(base, PrimitiveId::derive(&other, 1, 1, req, resp));
        assert_ne!(base, PrimitiveId::derive(&name, 2, 1, req, resp));
        assert_ne!(base, PrimitiveId::derive(&name, 1, 2, req, resp));
        assert_ne!(
            base,
            PrimitiveId::derive(&name, 1, 1, taxon::SchemaId::from_raw(12), resp)
        );
        assert_ne!(
            base,
            PrimitiveId::derive(&name, 1, 1, req, taxon::SchemaId::from_raw(23))
        );
    }

    #[test]
    fn names_are_validated() {
        assert!(PrimitiveName::new("probe_version").is_ok());
        assert!(matches!(
            PrimitiveName::new(""),
            Err(RegistrationError::InvalidName { .. })
        ));
        assert!(matches!(
            PrimitiveName::new("9lives"),
            Err(RegistrationError::InvalidName { .. })
        ));
        assert!(matches!(
            PrimitiveName::new("Probe"),
            Err(RegistrationError::InvalidName { .. })
        ));
        assert!(matches!(
            PrimitiveName::new("range"),
            Err(RegistrationError::ReservedName { .. })
        ));
    }

    #[test]
    fn semantic_schema_id_matches_scheduler_format() {
        // Pins the format the scheduler/lowering used before the hoist. If this
        // breaks, value identities across the runtime change — do not "fix" the
        // test; investigate.
        let ty = crate::vir::Type::Int;
        assert_eq!(
            crate::runtime::identity::semantic_schema_id(&ty),
            crate::runtime::identity::SchemaId::named(&format!("vix.semantic.v1:{}", ty.name())),
        );
    }
}
