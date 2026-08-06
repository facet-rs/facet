//! Schema exchange and **compatibility** decode through phon.
//!
//! A peer describes its types to the other side as phon **self-describing** schema
//! bytes. The receiver parses that closure into a
//! [`SchemaBundle`], then builds a compatibility decode program from the
//! *writer's* schema to the *reader's* derived descriptor — phon's
//! `lower_decode`. Every decode goes through this; there is no same-version
//! shortcut (the schema-identical case is just the degenerate output of the one
//! program).
//!
//! Wire framing of a closure: `u64` root id, `u32` schema count, then each schema as
//! `u32` length + its [`schema_to_bytes`] self-describing bytes. Bindings that need
//! payload-adjacent roots append `u32` auxiliary-root count, then each role as
//! `u32` UTF-8 byte length + bytes, followed by the auxiliary `u64` root.

use std::collections::BTreeSet;
use std::mem::{ManuallyDrop, MaybeUninit};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::sync::Arc;

use facet::{Facet, Shape};
use phon::derive::{of, of_shape};
use phon_engine::{Registry, typed};
use phon_ir::Lowered;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use phon_ir::MemOp;
use phon_schema::bytes::Reader;
use phon_schema::{Schema, SchemaId, schema_from_bytes, schema_to_bytes};

use crate::{Error, derive_error, shape_name};

/// A decoded schema closure: the root type's id and every reachable composite
/// schema. The writer's view of a type, used to build a compat decode program.
#[derive(Clone, Debug)]
pub struct SchemaBundle {
    pub root: SchemaId,
    pub schemas: Vec<Schema>,
    pub auxiliary_roots: Vec<AuxiliaryRoot>,
}

/// An additional writer root carried by the same schema binding, keyed by role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuxiliaryRoot {
    pub role: String,
    pub root: SchemaId,
}

/// The phon schema closure of `T` (root id + every reachable composite schema),
/// encoded as self-describing bytes — what a peer sends so the receiver can build a
/// compatibility decode program for `T`.
///
/// # Errors
/// [`Error`] if `T` cannot be lowered to a phon schema.
// r[impl schema.format.delivery]
// r[impl schema.format.self-contained]
// r[impl schema.principles.once-per-type]
pub fn schema_bytes<'a, T: Facet<'a>>() -> Result<Vec<u8>, Error> {
    let d = of::<T>().map_err(|e| derive_error(T::SHAPE, e))?;
    Ok(encode_bundle(d.root, &d.schemas))
}

/// Like [`schema_bytes`] but from a reflected `Shape` directly (the send tracker
/// works with `&'static Shape`, not a generic `T`).
///
/// # Errors
/// [`Error`] if the shape cannot be lowered to a phon schema.
// r[impl schema.format.delivery]
// r[impl schema.format.self-contained]
// r[impl schema.principles.once-per-type]
pub fn schema_bytes_for_shape(shape: &'static Shape) -> Result<Vec<u8>, Error> {
    let d = of_shape(shape).map_err(|e| derive_error(shape, e))?;
    Ok(encode_bundle(d.root, &d.schemas))
}

/// Build schema-binding bytes for a primary root plus payload-adjacent auxiliary roots.
///
/// # Errors
/// [`Error`] if any shape cannot be lowered to a phon schema.
// r[impl schema.format.binding-roots]
// r[impl schema.principles.once-per-type]
pub fn schema_bytes_for_shape_with_auxiliary_roots(
    shape: &'static Shape,
    auxiliary_roots: &[(&str, &'static Shape)],
) -> Result<Vec<u8>, Error> {
    let primary = of_shape(shape).map_err(|e| derive_error(shape, e))?;
    let mut schemas = primary.schemas;
    let mut seen: BTreeSet<u64> = schemas.iter().map(|s| s.id.as_u64()).collect();
    let mut roots = Vec::with_capacity(auxiliary_roots.len());

    for &(role, aux_shape) in auxiliary_roots {
        if role.is_empty() {
            return Err(Error("auxiliary schema root role must not be empty".into()));
        }
        let derived = of_shape(aux_shape).map_err(|e| derive_error(aux_shape, e))?;
        for schema in derived.schemas {
            if seen.insert(schema.id.as_u64()) {
                schemas.push(schema);
            }
        }
        roots.push(AuxiliaryRoot {
            role: role.to_string(),
            root: derived.root,
        });
    }

    Ok(encode_bundle_with_auxiliary_roots(
        primary.root,
        &schemas,
        &roots,
    ))
}

/// The phon **content-derived** schema id of a shape's root — the canonical id
/// peers agree on (the first 8 bytes of the closure from [`schema_bytes_for_shape`]).
/// This is NOT the vox-types `extract_schemas` `SchemaHash` (a different scheme).
///
/// # Errors
/// [`Error`] if the shape cannot be lowered to a phon schema.
pub fn schema_id_for_shape(shape: &'static Shape) -> Result<SchemaId, Error> {
    let d = of_shape(shape).map_err(|e| derive_error(shape, e))?;
    Ok(d.root)
}

/// The phon schema ids reachable from `shape`'s root that are part of a reference
/// cycle — the schemas a typed-path codegen must emit as recursion blocks
/// (`Access::Recurse` / `CallBlock`) rather than inlining. Exactly the keys of the
/// derive's `descriptor_blocks` (which the Rust derive already collected via the
/// SCC pass). Returns raw `u64` ids to match a codegen working in content ids.
///
/// # Errors
/// [`Error`] if the shape cannot be lowered to a phon schema.
pub fn recursive_schema_ids_for_shape(
    shape: &'static Shape,
) -> Result<std::collections::BTreeSet<u64>, Error> {
    let d = of_shape(shape).map_err(|e| derive_error(shape, e))?;
    Ok(d.descriptor_blocks.keys().map(|id| id.as_u64()).collect())
}

/// Encode a `(root, schemas)` closure to self-describing bytes.
fn encode_bundle(root: SchemaId, schemas: &[Schema]) -> Vec<u8> {
    encode_bundle_with_auxiliary_roots(root, schemas, &[])
}

fn encode_bundle_with_auxiliary_roots(
    root: SchemaId,
    schemas: &[Schema],
    auxiliary_roots: &[AuxiliaryRoot],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&root.as_u64().to_le_bytes());
    out.extend_from_slice(&(schemas.len() as u32).to_le_bytes());
    for s in schemas {
        let b = schema_to_bytes(s);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&b);
    }
    if !auxiliary_roots.is_empty() {
        out.extend_from_slice(&(auxiliary_roots.len() as u32).to_le_bytes());
        for root in auxiliary_roots {
            let role = root.role.as_bytes();
            out.extend_from_slice(&(role.len() as u32).to_le_bytes());
            out.extend_from_slice(role);
            out.extend_from_slice(&root.root.as_u64().to_le_bytes());
        }
    }
    out
}

/// Parse a schema closure produced by [`schema_bytes`].
///
/// # Errors
/// [`Error`] for malformed or truncated input.
pub fn parse_schema_bytes(bytes: &[u8]) -> Result<SchemaBundle, Error> {
    let mut r = Reader::new(bytes);
    let root = SchemaId::from_raw(
        r.read_u64()
            .map_err(|e| Error(format!("schema bundle root: {e:?}")))?,
    );
    let count = r
        .read_u32()
        .map_err(|e| Error(format!("schema bundle count: {e:?}")))? as usize;
    let mut schemas = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let len = r
            .read_u32()
            .map_err(|e| Error(format!("schema bundle entry length: {e:?}")))?
            as usize;
        let slice = r
            .read_slice(len)
            .map_err(|e| Error(format!("schema bundle entry body: {e:?}")))?;
        schemas.push(schema_from_bytes(slice).map_err(|e| Error(format!("schema decode: {e:?}")))?);
    }

    let auxiliary_roots = if r.remaining() == 0 {
        Vec::new()
    } else {
        let count = r
            .read_u32()
            .map_err(|e| Error(format!("schema bundle auxiliary root count: {e:?}")))?
            as usize;
        let mut roots = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let role_len = r
                .read_u32()
                .map_err(|e| Error(format!("schema bundle auxiliary role length: {e:?}")))?
                as usize;
            let role = r
                .read_slice(role_len)
                .map_err(|e| Error(format!("schema bundle auxiliary role body: {e:?}")))?;
            let role = std::str::from_utf8(role)
                .map_err(|e| Error(format!("schema bundle auxiliary role utf8: {e}")))?
                .to_string();
            let root = SchemaId::from_raw(
                r.read_u64()
                    .map_err(|e| Error(format!("schema bundle auxiliary root: {e:?}")))?,
            );
            roots.push(AuxiliaryRoot { role, root });
        }
        roots
    };
    if r.remaining() != 0 {
        return Err(Error(format!(
            "schema bundle has {} trailing bytes",
            r.remaining()
        )));
    }
    Ok(SchemaBundle {
        root,
        schemas,
        auxiliary_roots,
    })
}

/// A prebuilt compatibility decode program: the writer schema matched against the
/// reader type `T`'s descriptor, lowered once. Build it per `(writer root, T)` and
/// reuse it for every message — the compatibility-plan cost is paid here, not per decode.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
struct NativeDecodeProgram(phon_jit::native::NativeDecode);

// Safety: the compiled decode program is immutable after construction; all
// pointers it carries either point at executable code/prog storage owned by the
// program or at `'static` descriptor/thunk contexts.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe impl Send for NativeDecodeProgram {}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe impl Sync for NativeDecodeProgram {}

#[derive(Clone)]
pub struct DecodeProgram {
    lowered: Lowered,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    native: Option<Arc<NativeDecodeProgram>>,
}

pub struct Decoded<T> {
    value: ManuallyDrop<T>,
    retention: ManuallyDrop<DecodeRetention>,
}

unsafe impl<T: Send> Send for Decoded<T> {}

pub struct DecodeRetention {
    inner: typed::DecodeRetention,
}

unsafe impl Send for DecodeRetention {}

impl DecodeRetention {
    pub fn new(inner: typed::DecodeRetention) -> Self {
        Self { inner }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T> Decoded<T> {
    pub fn new(value: T, retention: typed::DecodeRetention) -> Self {
        Self {
            value: ManuallyDrop::new(value),
            retention: ManuallyDrop::new(DecodeRetention::new(retention)),
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn into_retention_and_value(self) -> (DecodeRetention, T) {
        let mut this = ManuallyDrop::new(self);
        unsafe {
            let retention = ManuallyDrop::take(&mut this.retention);
            let value = ManuallyDrop::take(&mut this.value);
            (retention, value)
        }
    }
}

impl<T> Drop for Decoded<T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.value);
            ManuallyDrop::drop(&mut self.retention);
        }
    }
}

impl DecodeProgram {
    fn new(lowered: Lowered) -> Self {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let native = native_decode_supported(&lowered).then(|| {
                Arc::new(NativeDecodeProgram(
                    phon_jit::native::NativeDecode::compile_lowered(&lowered),
                ))
            });
            Self { lowered, native }
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            Self { lowered }
        }
    }

    unsafe fn decode_into(
        &self,
        bytes: &[u8],
        base: *mut u8,
        type_name: &str,
    ) -> Result<(), Error> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if let Some(native) = &self.native {
                return unsafe { native.0.run(bytes, base) }
                    .map_err(|e| Error(format!("decode {type_name}: {e:?}")));
            }
        }

        unsafe { typed::decode_with(&self.lowered, bytes, base) }
            .map_err(|e| Error(format!("decode {type_name}: {e:?}")))
    }

    unsafe fn decode_into_retaining(
        &self,
        bytes: &[u8],
        base: *mut u8,
        type_name: &str,
    ) -> Result<typed::DecodeRetention, Error> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if let Some(native) = &self.native {
                return unsafe { native.0.run(bytes, base) }
                    .map(|()| typed::DecodeRetention::default())
                    .map_err(|e| Error(format!("decode {type_name}: {e:?}")));
            }
        }

        unsafe { typed::decode_with_retention(&self.lowered, bytes, base) }
            .map_err(|e| Error(format!("decode {type_name}: {e:?}")))
    }

    /// Whether this compatibility decode program uses the native JIT backend on
    /// this build target.
    pub fn uses_native_jit(&self) -> bool {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            self.native.is_some()
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            false
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_decode_supported(lowered: &Lowered) -> bool {
    decode_program_supported(&lowered.program)
        && lowered
            .blocks
            .values()
            .all(|block| decode_program_supported(block))
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn decode_program_supported(program: &[MemOp]) -> bool {
    program.iter().all(|op| match op {
        MemOp::Scalar { .. }
        | MemOp::ScalarRun(_)
        | MemOp::Bytes(_)
        | MemOp::Borrow(_)
        | MemOp::Default(_)
        | MemOp::SkipWire(_) => true,
        MemOp::NativeInt { .. } => false,
        MemOp::Sequence(s) => decode_program_supported(&s.element),
        MemOp::Set(s) => decode_program_supported(&s.element),
        MemOp::Option(o) => decode_program_supported(&o.some),
        MemOp::Enum(e) => e
            .variants
            .iter()
            .all(|variant| decode_program_supported(&variant.payload)),
        MemOp::Map(m) => decode_program_supported(&m.key) && decode_program_supported(&m.value),
        MemOp::Result(r) => decode_program_supported(&r.ok) && decode_program_supported(&r.err),
        MemOp::Pointer(p) => {
            !p.thunks.retain_decode_pointee && decode_program_supported(&p.pointee)
        }
        MemOp::Opaque(_) | MemOp::Dynamic { .. } | MemOp::CallBlock { .. } => true,
    })
}

// A built program is immutable, and its thunk `ctx` pointers are all `&'static`
// references (facet defs / adapter defs) cast to `*const ()` — morally `Send + Sync`,
// but the raw-pointer representation loses the auto-trait. Re-assert it so a program
// can be cached on the shared `SchemaRecvTracker` and run from any thread.
// Safety: immutable after build; thunk pointers are `&'static` / stateless.
unsafe impl Send for DecodeProgram {}
unsafe impl Sync for DecodeProgram {}

/// Build the compat decode program from `writer`'s schema against `T`'s
/// derived descriptor. Fails if the schemas are incompatible — before any bytes
/// are touched.
///
/// # Errors
/// [`Error`] if `T` cannot be derived, the writer root is unknown, or the schemas
/// cannot produce a compatibility decode program.
pub fn build_decode_program<'a, T: Facet<'a>>(
    writer: &SchemaBundle,
) -> Result<DecodeProgram, Error> {
    let reader = of::<T>().map_err(|e| derive_error(T::SHAPE, e))?;
    // The registry must resolve both the writer's refs and the reader's refs.
    let mut schemas = writer.schemas.clone();
    for s in &reader.schemas {
        if !schemas.iter().any(|x| x.id == s.id) {
            schemas.push(s.clone());
        }
    }
    let reg = Registry::new(schemas);
    let program = typed::lower_decode(
        writer.root,
        &reader.descriptor,
        &reader.descriptor_blocks,
        &reg,
    )
    .map_err(|e| Error(format!("lower_decode {}: {e:?}", shape_name(T::SHAPE))))?;
    Ok(DecodeProgram::new(program))
}

/// Decode `bytes` into `T` through a prebuilt compat [`DecodeProgram`], BORROWING
/// from `bytes`. The program and `T` must match.
///
/// # Errors
/// [`Error`] for malformed or trailing input.
pub fn decode_with_program<'a, T: Facet<'a>>(
    program: &DecodeProgram,
    bytes: &'a [u8],
) -> Result<T, Error> {
    let mut slot = MaybeUninit::<T>::uninit();
    // Safety: `program` was lowered for `T`'s descriptor; on `Ok`, `decode_with`
    // fully initializes the slot. Borrowed fields point into `bytes` (the `'a` tie).
    unsafe {
        program.decode_into(bytes, slot.as_mut_ptr().cast::<u8>(), &shape_name(T::SHAPE))?;
        Ok(slot.assume_init())
    }
}

pub fn decode_with_program_retained<'a, T: Facet<'a>>(
    program: &DecodeProgram,
    bytes: &'a [u8],
) -> Result<Decoded<T>, Error> {
    let mut slot = MaybeUninit::<T>::uninit();
    unsafe {
        let retention = program.decode_into_retaining(
            bytes,
            slot.as_mut_ptr().cast::<u8>(),
            &shape_name(T::SHAPE),
        )?;
        Ok(Decoded::new(slot.assume_init(), retention))
    }
}

/// Decode an OWNED `T` (`T: Facet<'static>`) through a prebuilt compat
/// [`DecodeProgram`], independent of the input's lifetime. An owned wire type's
/// descriptor uses owned vtables (allocating `String`/`Vec`, never `&str`/`Cow`), so
/// the result borrows nothing from `bytes` and `bytes` may be short-lived.
///
/// # Errors
/// [`Error`] for malformed or trailing input.
pub fn decode_owned_with_program<T: Facet<'static>>(
    program: &DecodeProgram,
    bytes: &[u8],
) -> Result<T, Error> {
    let mut slot = MaybeUninit::<T>::uninit();
    // Safety: `program` was lowered for `T`'s descriptor; `T: Facet<'static>` means
    // the descriptor is fully owned (no borrowed leaves), so the decoded value owns
    // its data and does not reference `bytes`.
    unsafe {
        program.decode_into(bytes, slot.as_mut_ptr().cast::<u8>(), &shape_name(T::SHAPE))?;
        Ok(slot.assume_init())
    }
}

/// Convenience: build a one-shot compat program and decode in one step. Prefer
/// caching a [`DecodeProgram`] across messages where the writer schema is stable.
///
/// # Errors
/// As [`build_decode_program`] and [`decode_with_program`].
pub fn decode_compat<'a, T: Facet<'a>>(bytes: &'a [u8], writer: &SchemaBundle) -> Result<T, Error> {
    let program = build_decode_program::<T>(writer)?;
    decode_with_program::<T>(&program, bytes)
}

/// Encode `value` as a SELF-CONTAINED message: its phon schema closure (`u32` length
/// then [`schema_bytes`]) followed by its compact value. Used where no schema was
/// pre-exchanged — the handshake — so the message carries the schema needed to decode
/// it.
///
/// # Errors
/// [`Error`] if `T` cannot be derived or encoded.
pub fn to_self_describing<'a, T: Facet<'a>>(value: &T) -> Result<Vec<u8>, Error> {
    let schema = schema_bytes::<T>()?;
    let value_bytes = crate::to_vec(value)?;
    let mut out = Vec::with_capacity(4 + schema.len() + value_bytes.len());
    out.extend_from_slice(&(schema.len() as u32).to_le_bytes());
    out.extend_from_slice(&schema);
    out.extend_from_slice(&value_bytes);
    Ok(out)
}

/// Length sentinel marking a **compact schema reference**: the message names its
/// schema by `u64` root id where [`to_self_describing`] would carry the whole closure.
///
/// The field it replaces is a real `u32` byte count, and no closure is four
/// gigabytes — `schema_bytes` of the entire vox `Message` graph is ten kilobytes — so
/// the sentinel can never collide with a length. A reader that predates the compact
/// form reads it as a length and fails on the very next `read_slice`: a clean decode
/// error, never a misparse. That is a *detectable* failure and not a survivable one,
/// which is why a peer may only send this form to an end that has already named the
/// same id (`Hello::compact_handshake_root`).
pub const COMPACT_SCHEMA_REF: u32 = u32::MAX;

/// The content-derived root id of `T`'s schema closure — the id a peer names in
/// [`to_self_describing_by_root`] instead of shipping the closure.
///
/// # Errors
/// [`Error`] if `T` cannot be lowered to a phon schema.
pub fn schema_root_id<'a, T: Facet<'a>>() -> Result<u64, Error> {
    let d = of::<T>().map_err(|e| derive_error(T::SHAPE, e))?;
    Ok(d.root.as_u64())
}

/// Encode `value` naming its schema by **root id** instead of carrying the closure:
/// [`COMPACT_SCHEMA_REF`], the `u64` root, then the compact value.
///
/// Sound only because phon schema ids are content-derived: a reader whose own root id
/// for `T` equals the one on the wire has, by construction, the identical closure, so
/// it can supply from its own derive the bytes the writer did not send. A reader that
/// does not recognise the id cannot decode the message at all, so the sender carries
/// the whole obligation — use this only against a peer that has named the same id.
///
/// # Errors
/// [`Error`] if `T` cannot be derived or encoded.
pub fn to_self_describing_by_root<'a, T: Facet<'a>>(value: &T) -> Result<Vec<u8>, Error> {
    let root = schema_root_id::<T>()?;
    let value_bytes = crate::to_vec(value)?;
    let mut out = Vec::with_capacity(12 + value_bytes.len());
    out.extend_from_slice(&COMPACT_SCHEMA_REF.to_le_bytes());
    out.extend_from_slice(&root.to_le_bytes());
    out.extend_from_slice(&value_bytes);
    Ok(out)
}

/// Decode a self-contained message produced by [`to_self_describing`] into an OWNED
/// `T`: parse the embedded writer schema closure, build a compatibility decode
/// program against `T`, and decode the value. The handshake decode — so even the
/// bootstrap message uses writer→reader planning rather than assuming same-version.
///
/// Also accepts the compact form from [`to_self_describing_by_root`], where the
/// closure is replaced by the writer's root id. Equal content-derived ids imply equal
/// closures, so the reader stands in its own; an id it does not recognise is refused
/// rather than guessed at.
///
/// # Errors
/// [`Error`] for malformed framing, an undecodable schema, an unrecognised compact
/// root, or incompatible schemas.
pub fn from_self_describing<T: Facet<'static>>(bytes: &[u8]) -> Result<T, Error> {
    let mut r = Reader::new(bytes);
    let schema_len = r
        .read_u32()
        .map_err(|e| Error(format!("self-describing schema length: {e:?}")))?;

    if schema_len == COMPACT_SCHEMA_REF {
        let writer_root = r
            .read_u64()
            .map_err(|e| Error(format!("compact schema reference root: {e:?}")))?;
        let reader = of::<T>().map_err(|e| derive_error(T::SHAPE, e))?;
        if writer_root != reader.root.as_u64() {
            return Err(Error(format!(
                "compact schema reference names {} root {writer_root:#x}, not this reader's \
                 {:#x} — a writer may only name an id the reader has published",
                shape_name(T::SHAPE),
                reader.root.as_u64()
            )));
        }
        // Equal ids, so the writer's closure IS the reader's. Still routed through
        // `lower_decode` against a bundle rebuilt from the reader's own derive: the
        // same-version case stays the degenerate output of the one program rather
        // than becoming a second decode path that could drift from it.
        let writer = SchemaBundle {
            root: reader.root,
            schemas: reader.schemas.clone(),
            auxiliary_roots: Vec::new(),
        };
        let program = build_decode_program::<T>(&writer)?;
        return decode_owned_with_program::<T>(&program, &bytes[12..]);
    }

    let schema_len = schema_len as usize;
    let schema = r
        .read_slice(schema_len)
        .map_err(|e| Error(format!("self-describing schema body: {e:?}")))?;
    let value = &bytes[4 + schema_len..];
    let writer = parse_schema_bytes(schema)?;
    let program = build_decode_program::<T>(&writer)?;
    decode_owned_with_program::<T>(&program, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    use spec_proto::DodecaParseResult;

    // Writer and reader compatibility: the writer struct has an extra field the reader
    // lacks (skipped), and the reader has a defaulted field the writer lacks
    // (defaulted). The compatibility decode handles both — the compat path, exercised end to end
    // over a real schema exchange.
    #[derive(Facet)]
    struct Writer {
        a: u32,
        gone: String,
        b: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Reader2 {
        a: u32,
        b: u32,
        #[facet(default)]
        added: u32,
    }

    #[derive(Facet)]
    struct ChannelElement {
        value: String,
    }

    #[test]
    fn compat_decode_bridges_writer_and_reader_changes() {
        // The writer sends its schema closure.
        let writer_bytes = schema_bytes::<Writer>().expect("writer schema bytes");
        let bundle = parse_schema_bytes(&writer_bytes).expect("parse bundle");

        // The writer encodes a value with ITS schema.
        let value = Writer {
            a: 11,
            gone: "discard".to_string(),
            b: 22,
        };
        let wire = crate::to_vec(&value).expect("encode writer value");

        // The reader builds a compatibility decode program from the writer schema and decodes.
        let decoded: Reader2 = decode_compat(&wire, &bundle).expect("compat decode");
        assert_eq!(
            decoded,
            Reader2 {
                a: 11,
                b: 22,
                added: 0
            }
        );
    }

    #[test]
    fn compat_decode_program_compiles_native_jit_when_available() {
        let writer_bytes = schema_bytes::<Writer>().expect("writer schema bytes");
        let bundle = parse_schema_bytes(&writer_bytes).expect("parse bundle");
        let program = build_decode_program::<Reader2>(&bundle).expect("build decode program");

        assert_eq!(
            program.uses_native_jit(),
            cfg!(all(target_os = "macos", target_arch = "aarch64"))
        );
    }

    #[test]
    fn dodeca_parse_result_compat_program_compiles_native_jit_when_available() {
        let writer_bytes = schema_bytes::<DodecaParseResult>().expect("dodeca writer schema bytes");
        let bundle = parse_schema_bytes(&writer_bytes).expect("parse dodeca bundle");
        let program = build_decode_program::<DodecaParseResult>(&bundle)
            .expect("build dodeca decode program");

        assert_eq!(
            program.uses_native_jit(),
            cfg!(all(target_os = "macos", target_arch = "aarch64"))
        );
    }

    #[test]
    // r[verify schema.format.delivery]
    fn a_named_schema_decodes_to_the_same_value_as_a_carried_one() {
        let value = Reader2 {
            a: 5,
            b: 7,
            added: 9,
        };
        let carried = to_self_describing(&value).expect("carry the closure");
        let named = to_self_describing_by_root(&value).expect("name the closure");

        assert_eq!(
            &named[..4],
            &COMPACT_SCHEMA_REF.to_le_bytes(),
            "the named form must be tagged with the sentinel"
        );
        assert_eq!(
            named.len(),
            carried.len() - schema_bytes::<Reader2>().expect("closure").len() + 8,
            "the named form is the carried form with the closure replaced by eight bytes"
        );

        let from_carried: Reader2 = from_self_describing(&carried).expect("decode carried");
        let from_named: Reader2 = from_self_describing(&named).expect("decode named");
        assert_eq!(from_carried, value);
        assert_eq!(from_named, value);
    }

    #[test]
    // r[verify schema.principles.self-describing]
    fn a_named_schema_the_reader_does_not_know_is_refused() {
        let value = Reader2 {
            a: 1,
            b: 2,
            added: 3,
        };
        let mut named = to_self_describing_by_root(&value).expect("name the closure");
        // Flip one bit of the root: a different type, as far as content-derived ids go.
        named[4] ^= 1;

        let error = from_self_describing::<Reader2>(&named)
            .expect_err("a root the reader has not published must not resolve");
        assert!(
            error.0.contains("compact schema reference names"),
            "unhelpful error: {}",
            error.0
        );
    }

    #[test]
    /// The sentinel can never be mistaken for a length by a reader that predates it:
    /// it is `u32::MAX` and the closure it stands in for is five orders of magnitude
    /// smaller. So an old reader fails loudly on the next read rather than misparsing.
    fn the_compact_sentinel_cannot_collide_with_a_real_closure_length() {
        let closure = schema_bytes::<Writer>().expect("writer closure");
        assert!(
            (closure.len() as u64) < u64::from(COMPACT_SCHEMA_REF),
            "a closure long enough to collide with the sentinel would be 4 GiB"
        );
    }

    #[test]
    // r[verify schema.format.delivery]
    // r[verify schema.format.self-contained]
    // r[verify schema.principles.self-describing]
    fn schema_bundle_round_trips() {
        let bytes = schema_bytes::<Writer>().expect("schema bytes");
        let bundle = parse_schema_bytes(&bytes).expect("parse");
        let d = of::<Writer>().expect("derive");
        assert_eq!(bundle.root, d.root);
        assert_eq!(bundle.schemas.len(), d.schemas.len());
        assert!(bundle.auxiliary_roots.is_empty());
    }

    #[test]
    // r[verify schema.format.binding-roots]
    fn schema_bundle_carries_auxiliary_roots() {
        let bytes = schema_bytes_for_shape_with_auxiliary_roots(
            Writer::SHAPE,
            &[("channel.arg.0.tx.element", ChannelElement::SHAPE)],
        )
        .expect("schema bytes");
        let bundle = parse_schema_bytes(&bytes).expect("parse");
        let writer = of::<Writer>().expect("derive writer");
        let element = of::<ChannelElement>().expect("derive element");

        assert_eq!(bundle.root, writer.root);
        assert_eq!(
            bundle.auxiliary_roots,
            vec![AuxiliaryRoot {
                role: "channel.arg.0.tx.element".to_string(),
                root: element.root,
            }]
        );
        assert!(
            bundle
                .schemas
                .iter()
                .any(|schema| schema.id == element.root),
            "auxiliary element schema must be carried in the binding"
        );
    }
}
