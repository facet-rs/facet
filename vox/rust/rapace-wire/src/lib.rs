#![deny(unsafe_code)]

use facet::Facet;

/// Placeholder crate for wire types.
///
/// Canonical definitions live in `docs/content/spec/_index.md` and `docs/content/shm-spec/_index.md`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct _Stub;
