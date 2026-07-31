//! The HTTP transport for the origin seam.
//!
//! `vix::runtime::OriginAdapter` is a three-line trait — `read(capability,
//! coordinate) -> Vec<u8>` — and this is one implementation of it. The fetch
//! primitive never names a protocol: it asks `EffectCtx::origin_candidate` for
//! bytes at a coordinate, and whatever transport the runtime was assembled with
//! answers. Swapping this for reqwest, a unix socket, or an S3 client is a
//! different `OriginAdapter`, not a different primitive.
//!
//! **The adapter is deliberately not trusted.** Verification, identity, store
//! admission, persistence, memoization and receipts all happen in the primitive
//! (`fetch_primitive::serve`), so bytes that do not hash to the pinned identity
//! fail `admit`, and bytes that contradict the pin fail `verify_upstream`. An
//! origin cannot lie about what it served; it can only fail to serve. That is
//! why so little code is enough.
//!
//! This used to be its own crate, `vix-fetch`. Of its 285 lines, 30 were this
//! adapter and the rest was a dead archive-fetching backend contract (no
//! consumers, and it unpacked archives itself, which
//! `machine.primitive.fetch-returns-a-blob` forbids) plus a flat `Map<Path,
//! Blob>` tree the recursive model replaced. The crate boundary was keeping
//! `ureq` out of this one; that is now an ordinary dependency here.

use sha2::{Digest as _, Sha256};
use vix::runtime::{OriginAdapter, OriginAdapterDecl, OriginReadError, ValueId};
use vix::schema::SchemaPattern;
use vix::vir::{ExternKind, Type};

/// Raw Blob transport over HTTP.
///
/// Archive interpretation is deliberately absent: an archive is a file, and
/// unpacking it is a separate Vix demand over the verified Blob identity
/// (`machine.primitive.fetch-returns-a-blob`).
#[derive(Clone, Copy, Debug, Default)]
pub struct HttpBlobOriginAdapter;

impl HttpBlobOriginAdapter {
    /// This adapter's declaration: `http(s)` coordinates for Registry
    /// capabilities, no tree namespace. The scheme sniff and the capability
    /// check the adapter used to perform are these two data fields now —
    /// routing admits before the adapter is reached
    /// (`machine.primitive.origin-routing`).
    #[must_use]
    pub fn decl() -> OriginAdapterDecl {
        OriginAdapterDecl {
            name: "http-blob".to_owned(),
            schemes: vec!["http".to_owned(), "https".to_owned()],
            capability: SchemaPattern::exact(&Type::Extern(ExternKind::Registry).schema_ref()),
            tree_namespace: None,
        }
    }
}

impl OriginAdapter for HttpBlobOriginAdapter {
    fn read(&self, _capability: &ValueId, coordinate: &str) -> Result<Vec<u8>, OriginReadError> {
        // Pure transport: a failed request or body is a miss — "not served
        // from here" — which a multi-origin fetch may fall through. This
        // adapter cannot detect corruption (verification is the seam's job)
        // and never refuses (its declaration already admitted the request).
        let mut response = ureq::get(coordinate)
            .call()
            .map_err(|error| OriginReadError::Miss {
                detail: format!("HTTP Blob origin {coordinate} failed: {error}"),
            })?;
        response
            .body_mut()
            .read_to_vec()
            .map_err(|error| OriginReadError::Miss {
                detail: format!("HTTP Blob origin {coordinate} body failed: {error}"),
            })
    }
}

/// The SHA-256 of `bytes`, hex-encoded — what a registry or lockfile publishes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The same digest written as a **pin**: `sha256:<hex>`.
///
/// The tag is not decoration. A bare digest is only meaningful to a reader who
/// already knows which algorithm produced it, and that assumption is what makes
/// an ecosystem unable to move off an algorithm once it weakens — the way
/// everyone eventually had to move off md5. Naming the algorithm in the value
/// costs seven characters and removes the assumption
/// (`vixen.pins.self-describing`).
#[must_use]
pub fn sha256_pin(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}
