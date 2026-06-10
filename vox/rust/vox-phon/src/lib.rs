//! phon as a vox codec.
//!
//! This is the data-plane adapter for the codec migration: encode/decode a
//! `#[derive(Facet)]` value through phon's typed (schema-driven) path, mirroring
//! the old driver-facing codec surface (`to_vec` / `from_slice`).
//! It derives the schema + descriptor from the facet `Shape`, lowers that to
//! phon IR, then runs the native JIT backend when this target supports it.
//!
//! The wire is **phon-compact** — fixed-width little-endian with `u32` length
//! prefixes and alignment padding — and is deliberately NOT byte-compatible with
//! the wire format it replaces. Swapping codecs breaks the old wire by design.
//!
use std::{
    collections::HashMap,
    mem::MaybeUninit,
    sync::{Arc, LazyLock},
};

use facet::{Facet, PtrConst, Shape};
use moire::sync::SyncMutex;
pub use phon::api::{JitFallbackRecord, JitFallbackReport, MethodJitFallbackReport};
use phon::derive::{Derived, of_shape};
use phon_engine::{Registry, typed};
use phon_ir::Lowered;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use phon_ir::MemOp;

pub mod schema;
pub use schema::{
    AuxiliaryRoot, DecodeProgram, SchemaBundle, build_decode_program, decode_compat,
    decode_owned_with_program, decode_with_program, from_self_describing, parse_schema_bytes,
    recursive_schema_ids_for_shape, schema_bytes, schema_bytes_for_shape,
    schema_bytes_for_shape_with_auxiliary_roots, schema_id_for_shape, to_self_describing,
};

/// Opaque-passthrough sentinel: build an `OpaqueSerialize` that emits already-encoded
/// `bytes` verbatim as the opaque inner content (no re-derive/re-encode). Used by the
/// `Payload` adapter to forward an already-encoded RPC payload (e.g. a proxied call).
pub use phon::derive::{RAW_OPAQUE_BYTES_SHAPE, RawOpaqueBytes, raw_opaque_bytes};

/// A codec error: the type could not be lowered to a phon schema, or the
/// value/bytes did not match it.
#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Whether a shape-erased typed program uses the native JIT backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JitStatus {
    pub encode_native: bool,
    pub decode_native: bool,
}

impl JitStatus {
    pub fn fully_native(self) -> bool {
        self.encode_native && self.decode_native
    }
}

fn lower_derived(type_name: &str, derived: &Derived) -> Result<Lowered, Error> {
    let reg = Registry::new(derived.schemas.clone());
    typed::lower_typed(&derived.descriptor, &derived.descriptor_blocks, &reg)
        .map_err(|e| Error(format!("lower {type_name}: {e:?}")))
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
struct NativeEncodeProgram(phon_jit::native::NativeEncode);

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
struct NativeDecodeProgram(phon_jit::native::NativeDecode);

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe impl Send for NativeEncodeProgram {}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe impl Sync for NativeEncodeProgram {}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe impl Send for NativeDecodeProgram {}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe impl Sync for NativeDecodeProgram {}

struct TypedProgram {
    lowered: Lowered,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    native_encode: Option<NativeEncodeProgram>,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    native_decode: Option<NativeDecodeProgram>,
}

impl TypedProgram {
    fn for_shape(shape: &'static Shape) -> Result<Self, Error> {
        let type_name = shape.type_identifier;
        let derived = of_shape(shape).map_err(|e| Error(format!("derive {type_name}: {e}")))?;
        let lowered = lower_derived(type_name, &derived)?;

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let native_encode = native_encode_supported(&lowered).then(|| {
                NativeEncodeProgram(phon_jit::native::NativeEncode::compile_lowered(&lowered))
            });
            let native_decode = native_decode_supported(&lowered).then(|| {
                NativeDecodeProgram(phon_jit::native::NativeDecode::compile_lowered(&lowered))
            });
            Ok(Self {
                lowered,
                native_encode,
                native_decode,
            })
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            Ok(Self { lowered })
        }
    }

    unsafe fn encode(&self, base: *const u8) -> Vec<u8> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if let Some(native) = &self.native_encode {
                return unsafe { native.0.run(base) };
            }
        }
        unsafe { typed::encode_with(&self.lowered, base) }
    }

    unsafe fn decode_into(
        &self,
        bytes: &[u8],
        base: *mut u8,
        type_name: &str,
    ) -> Result<(), Error> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if let Some(native) = &self.native_decode {
                return unsafe { native.0.run(bytes, base) }
                    .map_err(|e| Error(format!("decode {type_name}: {e:?}")));
            }
        }
        unsafe { typed::decode_with(&self.lowered, bytes, base) }
            .map_err(|e| Error(format!("decode {type_name}: {e:?}")))
    }

    fn jit_status(&self) -> JitStatus {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            JitStatus {
                encode_native: self.native_encode.is_some(),
                decode_native: self.native_decode.is_some(),
            }
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            JitStatus {
                encode_native: false,
                decode_native: false,
            }
        }
    }

    fn jit_fallback_report(&self) -> JitFallbackReport {
        let report = phon::api::jit_fallback_report_for_lowered(&self.lowered);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let mut report = report;
            if self.native_decode.is_none() && report.decode.is_empty() {
                report.decode.push(JitFallbackRecord {
                    path: "$".to_string(),
                    reason: "native decode JIT was not compiled for this program",
                });
            }
            if self.native_encode.is_none() && report.encode.is_empty() {
                report.encode.push(JitFallbackRecord {
                    path: "$".to_string(),
                    reason: "native encode JIT was not compiled for this program",
                });
            }
            report
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            report
        }
    }
}

// The lowered program and native executors are immutable after construction, and
// thunk contexts inside the lowered IR are `'static` descriptor data.
unsafe impl Send for TypedProgram {}
unsafe impl Sync for TypedProgram {}

static TYPED_PROGRAMS: LazyLock<SyncMutex<HashMap<usize, Arc<TypedProgram>>>> =
    LazyLock::new(|| SyncMutex::new("vox-phon.typed_programs", HashMap::new()));

fn typed_program_for_shape(shape: &'static Shape) -> Result<Arc<TypedProgram>, Error> {
    let key = core::ptr::from_ref(shape) as usize;
    if let Some(program) = TYPED_PROGRAMS.lock().get(&key) {
        return Ok(Arc::clone(program));
    }

    let program = Arc::new(TypedProgram::for_shape(shape)?);
    let mut cache = TYPED_PROGRAMS.lock();
    if let Some(existing) = cache.get(&key) {
        return Ok(Arc::clone(existing));
    }
    cache.insert(key, Arc::clone(&program));
    Ok(program)
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
fn native_encode_supported(lowered: &Lowered) -> bool {
    encode_program_supported(&lowered.program)
        && lowered
            .blocks
            .values()
            .all(|block| encode_program_supported(block))
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn decode_program_supported(program: &[MemOp]) -> bool {
    program.iter().all(|op| match op {
        MemOp::Scalar { .. }
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
        MemOp::Pointer(p) => decode_program_supported(&p.pointee),
        MemOp::Opaque(_) | MemOp::Dynamic { .. } | MemOp::CallBlock { .. } => true,
    })
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn encode_program_supported(program: &[MemOp]) -> bool {
    program.iter().all(|op| match op {
        MemOp::Scalar { .. } | MemOp::Bytes(_) | MemOp::Borrow(_) => true,
        MemOp::NativeInt { .. } => false,
        MemOp::Sequence(s) => encode_program_supported(&s.element),
        MemOp::Set(s) => encode_program_supported(&s.element),
        MemOp::Option(o) => encode_program_supported(&o.some),
        MemOp::Enum(e) => e
            .variants
            .iter()
            .all(|variant| encode_program_supported(&variant.payload)),
        MemOp::Map(m) => encode_program_supported(&m.key) && encode_program_supported(&m.value),
        MemOp::Result(r) => encode_program_supported(&r.ok) && encode_program_supported(&r.err),
        MemOp::Pointer(p) => encode_program_supported(&p.pointee),
        MemOp::SkipWire(_) | MemOp::Default(_) => false,
        MemOp::Opaque(_) | MemOp::Dynamic { .. } | MemOp::CallBlock { .. } => true,
    })
}

/// Encode `value` to phon-compact bytes via its facet-derived schema.
///
/// # Errors
/// [`Error`] if `T` cannot be lowered to a phon schema or the value does not
/// match it.
pub fn to_vec<'a, T: Facet<'a>>(value: &T) -> Result<Vec<u8>, Error> {
    let program = typed_program_for_shape(T::SHAPE)?;
    // Safety: `value` is a live `T`; `program` was built from `T`'s descriptor.
    Ok(unsafe { program.encode((value as *const T).cast::<u8>()) })
}

/// Encode a type-erased value `(ptr, shape)` to phon-compact bytes via its
/// facet-derived schema — the shape-driven analog of [`to_vec`], used where the
/// concrete type isn't a generic param (e.g. the `Payload::Value` send path that
/// must pre-encode channel-bearing args out-of-band).
///
/// # Safety
/// `ptr` must point to an initialized value whose layout matches `shape`.
///
/// # Errors
/// [`Error`] if `shape` cannot be lowered to a phon schema or the value does not
/// match it.
pub fn to_vec_for_shape(ptr: PtrConst, shape: &'static Shape) -> Result<Vec<u8>, Error> {
    let program = typed_program_for_shape(shape)?;
    // Safety: `ptr` points to a live value of `shape`; `program` was built from
    // that shape's descriptor.
    Ok(unsafe { program.encode(ptr.as_byte_ptr()) })
}

/// Report whether the shape-erased typed program for `shape` is backed by
/// native JIT programs on this build target.
///
/// # Errors
/// [`Error`] if `shape` cannot be lowered to a phon schema.
pub fn jit_status_for_shape(shape: &'static Shape) -> Result<JitStatus, Error> {
    Ok(typed_program_for_shape(shape)?.jit_status())
}

/// Report native-JIT fallback diagnostics for a shape-erased generated bridge root.
///
/// # Errors
/// [`Error`] if `shape` cannot be lowered to a phon schema.
pub fn jit_fallback_report_for_shape(shape: &'static Shape) -> Result<JitFallbackReport, Error> {
    Ok(typed_program_for_shape(shape)?.jit_fallback_report())
}

/// Report native-JIT fallback diagnostics scoped to a generated Vox method root.
///
/// # Errors
/// [`Error`] if `shape` cannot be lowered to a phon schema.
pub fn method_jit_fallback_report_for_shape(
    shape: &'static Shape,
    method: impl Into<String>,
    phase: impl Into<String>,
) -> Result<MethodJitFallbackReport, Error> {
    Ok(jit_fallback_report_for_shape(shape)?.scoped(method, phase))
}

/// Decode `T` from phon-compact bytes, BORROWING from `bytes`: `&str`,
/// `&[u8]`, `Cow`, and opaque payloads point INTO `bytes`, so the decoded value may
/// not outlive it. The lifetime tie (`bytes: &'a [u8]`, `T: Facet<'a>`) enforces it.
///
/// This is the recv-path decode for the `Message` envelope: the payload field
/// decodes to a borrowed span and metadata strings borrow the backing.
///
/// # Errors
/// [`Error`] if `T` cannot be lowered, or the bytes are malformed for it.
pub fn from_slice_borrowed<'a, T: Facet<'a>>(bytes: &'a [u8]) -> Result<T, Error> {
    let type_name = T::SHAPE.type_identifier;
    let program = typed_program_for_shape(T::SHAPE)?;
    let mut slot = MaybeUninit::<T>::uninit();
    // Safety: `program` was built from `T`'s descriptor; on `Ok`, decode has fully
    // initialized the slot. Borrowed fields point into `bytes`, which outlives the
    // returned `T` by the `'a` tie.
    unsafe {
        program.decode_into(bytes, slot.as_mut_ptr().cast::<u8>(), type_name)?;
        Ok(slot.assume_init())
    }
}

/// Decode an owned `T` from phon-compact bytes via its facet-derived schema,
/// rejecting trailing bytes.
///
/// # Errors
/// [`Error`] if `T` cannot be lowered, or the bytes are malformed for it.
pub fn from_slice<'a, T: Facet<'a>>(bytes: &[u8]) -> Result<T, Error> {
    let type_name = T::SHAPE.type_identifier;
    let program = typed_program_for_shape(T::SHAPE)?;
    let mut slot = MaybeUninit::<T>::uninit();
    // Safety: `program` was built from `T`'s descriptor; on `Ok`, decode has fully
    // initialized the slot.
    unsafe {
        program.decode_into(bytes, slot.as_mut_ptr().cast::<u8>(), type_name)?;
        Ok(slot.assume_init())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use phon::derive::of;
    use phon_engine::typed;
    use spec_proto::DodecaParseResult;
    use vox_types::{
        BindingDirection, ConnectionId, Message, MessagePayload, MethodId, Payload, RequestBody,
        RequestCall, RequestId, RequestMessage, SchemaBytes, SchemaMessage,
    };

    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: u32,
        y: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Shape {
        Circle(f64),
        Rectangle { width: f64, height: f64 },
        Point,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        age: u32,
        email: Option<String>,
        tags: Vec<String>,
        home: Point,
        favorite: Shape,
        big: u64,
    }

    fn round_trip<T>(value: &T) -> T
    where
        T: Facet<'static> + std::fmt::Debug + PartialEq,
    {
        let bytes = to_vec(value).expect("encode");
        from_slice::<T>(&bytes).expect("decode")
    }

    #[test]
    fn typed_program_cache_reuses_shape_program() {
        let first = typed_program_for_shape(Person::SHAPE).expect("first program");
        let second = typed_program_for_shape(Person::SHAPE).expect("second program");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(
            first.jit_status().fully_native(),
            cfg!(all(target_os = "macos", target_arch = "aarch64"))
        );

        let p = Person {
            name: "Ada".to_string(),
            age: 36,
            email: Some("ada@example.com".to_string()),
            tags: vec!["math".to_string(), "engine".to_string()],
            home: Point { x: 10, y: 20 },
            favorite: Shape::Rectangle {
                width: 3.0,
                height: 4.0,
            },
            big: 5_000_000_000,
        };
        let generic = to_vec(&p).expect("generic encode");
        let erased = to_vec_for_shape(
            PtrConst::new((&p as *const Person).cast::<u8>()),
            Person::SHAPE,
        )
        .expect("shape encode");
        assert_eq!(generic, erased);
    }

    #[test]
    fn vox_wire_shapes_report_native_jit_when_available() {
        for (name, shape) in [
            ("Message", Message::SHAPE),
            ("MessagePayload", MessagePayload::SHAPE),
            ("RequestCall", RequestCall::SHAPE),
            ("RequestMessage", RequestMessage::SHAPE),
            ("SchemaMessage", SchemaMessage::SHAPE),
            ("Payload", Payload::SHAPE),
            ("DodecaParseResult", DodecaParseResult::SHAPE),
        ] {
            let status = jit_status_for_shape(shape).unwrap_or_else(|err| {
                panic!("failed to build JIT status for {name}: {err}");
            });
            assert_eq!(
                status.fully_native(),
                cfg!(all(target_os = "macos", target_arch = "aarch64")),
                "{name} JIT status: {status:?}"
            );
        }
    }

    #[test]
    fn vox_wire_shapes_expose_method_scoped_fallback_reports() {
        for (name, shape) in [
            ("Message", Message::SHAPE),
            ("MessagePayload", MessagePayload::SHAPE),
            ("RequestCall", RequestCall::SHAPE),
            ("RequestMessage", RequestMessage::SHAPE),
            ("SchemaMessage", SchemaMessage::SHAPE),
            ("Payload", Payload::SHAPE),
            ("DodecaParseResult", DodecaParseResult::SHAPE),
        ] {
            let report =
                method_jit_fallback_report_for_shape(shape, name, "wire").unwrap_or_else(|err| {
                    panic!("failed to build JIT fallback report for {name}: {err}");
                });
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                assert!(report.is_empty(), "{name} fallback report: {report:?}");
            } else {
                assert!(!report.is_empty(), "{name} should report unavailable JIT");
                for record in &report.records {
                    assert_eq!(record.method, name);
                    assert_eq!(record.phase, "wire");
                    assert!(matches!(record.direction, "decode" | "encode"));
                    assert_eq!(record.path, "$");
                }
            }
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn unsupported_native_int_program_reports_before_native_compile() {
        let program = TypedProgram {
            lowered: Lowered {
                program: vec![
                    MemOp::NativeInt {
                        offset: 0,
                        mem_size: 4,
                        signed: false,
                    },
                    MemOp::NativeInt {
                        offset: 4,
                        mem_size: 4,
                        signed: true,
                    },
                ],
                blocks: Default::default(),
            },
            native_encode: None,
            native_decode: None,
        };

        let report = program.jit_fallback_report().scoped("nativeSized", "args");
        assert_eq!(report.records.len(), 4);
        assert!(report.records.iter().all(|record| {
            record.method == "nativeSized"
                && record.phase == "args"
                && record.reason.contains("native-sized integer casts")
                && matches!(record.direction, "decode" | "encode")
        }));
    }

    #[test]
    fn round_trips_a_rich_struct() {
        let p = Person {
            name: "Ada".to_string(),
            age: 36,
            email: Some("ada@example.com".to_string()),
            tags: vec!["math".to_string(), "engine".to_string()],
            home: Point { x: 10, y: 20 },
            favorite: Shape::Rectangle {
                width: 3.0,
                height: 4.0,
            },
            big: 5_000_000_000,
        };
        assert_eq!(round_trip(&p), p);
    }

    #[test]
    fn round_trips_each_enum_variant() {
        assert_eq!(round_trip(&Shape::Circle(2.5)), Shape::Circle(2.5));
        assert_eq!(round_trip(&Shape::Point), Shape::Point);
        assert_eq!(
            round_trip(&Shape::Rectangle {
                width: 1.0,
                height: 2.0
            }),
            Shape::Rectangle {
                width: 1.0,
                height: 2.0
            },
        );
    }

    #[test]
    fn round_trips_empty_collections_and_none() {
        let p = Person {
            name: String::new(),
            age: 0,
            email: None,
            tags: Vec::new(),
            home: Point { x: 0, y: 0 },
            favorite: Shape::Point,
            big: 0,
        };
        assert_eq!(round_trip(&p), p);
    }

    fn interpreter_to_vec<'a, T: Facet<'a>>(value: &T) -> Vec<u8> {
        let type_name = T::SHAPE.type_identifier;
        let derived = of::<T>().expect("derive");
        let lowered = lower_derived(type_name, &derived).expect("lower");
        // Safety: `value` is a live `T`; `lowered` was built from `T`.
        unsafe { typed::encode_with(&lowered, (value as *const T).cast::<u8>()) }
    }

    #[test]
    fn native_message_schema_message_encode_matches_interpreter() {
        let message = Message {
            connection_id: ConnectionId(7),
            payload: MessagePayload::SchemaMessage(SchemaMessage {
                method_id: MethodId(0x0102_0304_0506_0708),
                direction: BindingDirection::Args,
                schemas: SchemaBytes(vec![0x18, 0x24, 0x42, 0x99]),
            }),
        };

        let native = to_vec(&message).expect("native encode");
        let interpreter = interpreter_to_vec(&message);
        assert_eq!(native, interpreter);
    }

    #[test]
    fn native_message_request_call_encode_matches_interpreter() {
        let args = [0x18, 0x24, 0x42, 0x99];
        let message = Message {
            connection_id: ConnectionId(7),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(9),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(0x0102_0304_0506_0708),
                    channels: Vec::new(),
                    metadata: Default::default(),
                    args: Payload::Encoded(&args),
                    schemas: SchemaBytes(Vec::new()),
                }),
            }),
        };

        let native = to_vec(&message).expect("native encode");
        let interpreter = interpreter_to_vec(&message);
        assert_eq!(native, interpreter);
    }
}
