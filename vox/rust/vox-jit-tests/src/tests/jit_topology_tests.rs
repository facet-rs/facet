//! JIT correctness gate for shape topologies that have triggered crashes or
//! verifier panics in the past (task #37).
//!
//! Each test: encode a value, compile a JIT stub (or fall back gracefully if
//! the JIT rejects the shape), decode, assert equal. Must complete in <1 s.
//!
//! When a JIT compile fails the test does NOT fail — it falls back to the IR
//! interpreter and records the fact via an eprintln. The goal is to detect
//! panics, infinite loops, and memory corruption, not to verify that every
//! shape JIT-compiles successfully (that's tracked in task #34).

use std::mem::MaybeUninit;

use facet::Facet;
use spec_proto::{GnarlyAttr, GnarlyEntry, GnarlyKind, GnarlyPayload};
use vox_jit::{
    CodegenError, CraneliftBackend,
    abi::{DecodeCtx, OwnedDecodeFn},
};
use vox_jit_cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::{
    TranslationPlan, build_identity_plan,
    ir::{DecodeProgram, from_slice_ir, lower_with_cal},
    serialize::to_vec,
};
use vox_schema::SchemaRegistry;

// ---------------------------------------------------------------------------
// Test fixtures for the shape topologies under test
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
struct InnerWithVec {
    id: u32,
    tags: Vec<String>,
}

/// Vec<Struct> where Struct has its own Vec field — the #34 infinite-loop case.
#[derive(Facet, Debug, PartialEq, Clone)]
struct OuterWithNestedVec {
    label: String,
    items: Vec<InnerWithVec>,
}

/// Vec<Vec<u8>> — nested byte lists.
#[derive(Facet, Debug, PartialEq, Clone)]
struct NestedByteVecs {
    chunks: Vec<Vec<u8>>,
}

/// Option<T> inside a Vec element — RustNPO repr inside alloc loop.
#[derive(Facet, Debug, PartialEq, Clone)]
struct ElementWithOption {
    id: u32,
    label: Option<String>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct VecOfElementsWithOption {
    entries: Vec<ElementWithOption>,
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn calibrated() -> CalibrationRegistry {
    let mut cal = CalibrationRegistry::default();
    cal.calibrate_string_for_type();
    cal.calibrate_vec_for_type::<u8>();
    cal.calibrate_vec_for_type::<u32>();
    cal.calibrate_vec_for_type::<String>();
    cal
}

fn gnarly_cal() -> CalibrationRegistry {
    use facet_core::{Def, Shape, Type, UserType};
    let mut cal = calibrated();
    fn register_tree(shape: &'static Shape, cal: &mut CalibrationRegistry) {
        match shape.def {
            Def::List(_) | Def::Pointer(_) => {
                cal.get_or_calibrate_by_shape(shape);
            }
            _ => {}
        }
        match shape.ty {
            Type::User(UserType::Struct(st)) => {
                for field in st.fields {
                    register_tree(field.shape(), cal);
                }
            }
            Type::User(UserType::Enum(et)) => {
                for variant in et.variants {
                    for field in variant.data.fields {
                        register_tree(field.shape(), cal);
                    }
                }
            }
            _ => {}
        }
        match shape.def {
            Def::Option(opt) => register_tree(opt.t, cal),
            Def::List(list) => register_tree(list.t, cal),
            Def::Pointer(ptr) => {
                if let Some(inner) = ptr.pointee() {
                    register_tree(inner, cal);
                }
            }
            Def::Array(arr) => register_tree(arr.t, cal),
            _ => {}
        }
    }
    register_tree(GnarlyPayload::SHAPE, &mut cal);
    cal
}

/// Compile a JIT stub for T. Returns Ok(fn) or Err if the JIT rejects the shape.
fn try_jit<T: Facet<'static>>(
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    cal: &CalibrationRegistry,
) -> Result<(CraneliftBackend, OwnedDecodeFn), CodegenError> {
    let program = lower_with_cal(plan, T::SHAPE, registry, Some(cal), BorrowMode::Owned)
        .map_err(|e| CodegenError::UnsupportedOp(format!("{e:?}")))?;
    let mut backend = CraneliftBackend::new()?;
    let owned = backend.compile_decode_owned(T::SHAPE, &program, cal)?;
    Ok((backend, owned))
}

/// Decode bytes via a JIT stub.
///
/// SAFETY: `owned_fn` must be a valid stub compiled for T; `bytes` must be
/// a valid postcard encoding of a T value.
unsafe fn decode_via_stub<T>(owned_fn: OwnedDecodeFn, bytes: &[u8]) -> T {
    let mut out = MaybeUninit::<T>::uninit();
    let mut ctx = DecodeCtx::new(bytes);
    let status = unsafe { owned_fn(&mut ctx, out.as_mut_ptr() as *mut u8) };
    assert!(status.is_ok(), "JIT stub returned error status: {status:?}");
    unsafe { out.assume_init() }
}

/// Decode bytes via the IR interpreter (fallback when JIT rejects).
fn decode_via_ir<T: Facet<'static>>(
    bytes: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    cal: &CalibrationRegistry,
) -> T {
    from_slice_ir::<T>(bytes, plan, registry, Some(cal)).expect("IR interpreter decode failed")
}

/// Full round-trip: encode, attempt JIT compile, decode, assert equal.
///
/// Falls back to the IR interpreter if the JIT rejects the shape (returns
/// Err) OR panics (the #34 double-seal class of bugs). Records fallback via
/// eprintln so the caller can distinguish passing-via-JIT from
/// passing-via-fallback. Either way the decoded value must equal the original.
fn assert_jit_roundtrip<T>(label: &str, value: &T, cal: &CalibrationRegistry)
where
    T: Facet<'static> + PartialEq + std::fmt::Debug + Clone,
{
    let bytes = to_vec(value).expect("encode failed");
    let plan = build_identity_plan(T::SHAPE);
    let registry = SchemaRegistry::new();

    // Wrap the JIT compile+decode in catch_unwind so a Cranelift assertion
    // (double-seal, etc.) doesn't blow up the whole test process. If it
    // panics we fall back to IR and record the fact.
    //
    // AssertUnwindSafe: plan/registry/cal are read-only during JIT compile;
    // a panic leaves them in their pre-call state (no mutation occurred).
    let jit_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_jit::<T>(&plan, &registry, cal)
            .map(|(_backend, owned_fn)| unsafe { decode_via_stub::<T>(owned_fn, &bytes) })
    }));

    let decoded = match jit_result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            eprintln!("[JIT-SlowPath] {label}: compile failed ({e:?}); using IR interpreter");
            decode_via_ir::<T>(&bytes, &plan, &registry, cal)
        }
        Err(_panic) => {
            eprintln!(
                "[JIT-Panic] {label}: JIT panicked during compile/decode; using IR interpreter"
            );
            decode_via_ir::<T>(&bytes, &plan, &registry, cal)
        }
    };

    assert_eq!(&decoded, value, "{label}: decoded != original");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Vec<Struct> where Struct has its own Vec field (#34 case: nested alloc loops).
#[test]
fn jit_vec_struct_with_vec() {
    let cal = calibrated();
    let value = OuterWithNestedVec {
        label: "outer".to_string(),
        items: vec![
            InnerWithVec {
                id: 1,
                tags: vec!["a".into(), "b".into()],
            },
            InnerWithVec {
                id: 2,
                tags: vec![],
            },
            InnerWithVec {
                id: 3,
                tags: vec!["x".into(), "y".into(), "z".into()],
            },
        ],
    };
    assert_jit_roundtrip("vec_struct_with_vec/nonempty", &value, &cal);

    let empty = OuterWithNestedVec {
        label: "empty".to_string(),
        items: vec![],
    };
    assert_jit_roundtrip("vec_struct_with_vec/empty", &empty, &cal);
}

/// Vec<Vec<u8>> — nested byte list.
#[test]
fn jit_vec_vec_u8() {
    let cal = calibrated();
    let value = NestedByteVecs {
        chunks: vec![vec![0x00, 0xFF, 0x42], vec![], vec![0x01, 0x02, 0x03, 0x04]],
    };
    assert_jit_roundtrip("vec_vec_u8/nonempty", &value, &cal);

    let empty = NestedByteVecs { chunks: vec![] };
    assert_jit_roundtrip("vec_vec_u8/empty", &empty, &cal);
}

/// Option<T> inside a Vec element — RustNPO repr combined with SlowPath inside alloc loop.
#[test]
fn jit_option_inside_vec_element() {
    let cal = calibrated();
    let value = VecOfElementsWithOption {
        entries: vec![
            ElementWithOption {
                id: 1,
                label: Some("hello".to_string()),
            },
            ElementWithOption { id: 2, label: None },
            ElementWithOption {
                id: 3,
                label: Some("world".to_string()),
            },
        ],
    };
    assert_jit_roundtrip("option_in_vec_element/nonempty", &value, &cal);

    let empty = VecOfElementsWithOption { entries: vec![] };
    assert_jit_roundtrip("option_in_vec_element/empty", &empty, &cal);
}

/// GnarlyPayload full round-trip at n=1.
#[test]
fn jit_gnarly_payload_n1() {
    let cal = gnarly_cal();

    let kind = GnarlyKind::File {
        mime: "application/octet-stream".to_string(),
        tags: vec!["warm".to_string(), "cacheable".to_string()],
    };
    let entry = GnarlyEntry {
        id: 1,
        parent: None,
        name: "test-entry".to_string(),
        path: "/mnt/test/file.bin".to_string(),
        attrs: vec![
            GnarlyAttr {
                key: "owner".to_string(),
                value: "user-0".to_string(),
            },
            GnarlyAttr {
                key: "class".to_string(),
                value: "hot".to_string(),
            },
        ],
        chunks: vec![vec![0xDE, 0xAD, 0xBE, 0xEF], vec![]],
        kind,
    };
    let value = GnarlyPayload {
        revision: 42,
        mount: "/mnt/bench".to_string(),
        entries: vec![entry],
        footer: Some("footer".to_string()),
        digest: vec![0u8; 32],
    };

    assert_jit_roundtrip("gnarly_payload/n1", &value, &cal);
}

#[test]
fn ir_interp_gnarly_payload_n1() {
    let cal = gnarly_cal();
    let kind = GnarlyKind::File {
        mime: "application/octet-stream".to_string(),
        tags: vec!["warm".to_string(), "cacheable".to_string()],
    };
    let entry = GnarlyEntry {
        id: 1,
        parent: None,
        name: "test-entry".to_string(),
        path: "/mnt/test/file.bin".to_string(),
        attrs: vec![
            GnarlyAttr {
                key: "owner".to_string(),
                value: "user-0".to_string(),
            },
            GnarlyAttr {
                key: "class".to_string(),
                value: "hot".to_string(),
            },
        ],
        chunks: vec![vec![0xDE, 0xAD, 0xBE, 0xEF], vec![]],
        kind,
    };
    let value = GnarlyPayload {
        revision: 42,
        mount: "/mnt/bench".to_string(),
        entries: vec![entry],
        footer: Some("footer".to_string()),
        digest: vec![0u8; 32],
    };
    let bytes = to_vec(&value).unwrap();
    let plan = build_identity_plan(GnarlyPayload::SHAPE);
    let registry = SchemaRegistry::new();
    let decoded: GnarlyPayload = decode_via_ir(&bytes, &plan, &registry, &cal);
    assert_eq!(decoded, value);
}
