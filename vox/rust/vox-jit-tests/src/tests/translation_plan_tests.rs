//! Translation-plan differential tests: schema-evolution scenarios.
//!
//! These tests drive `build_plan` with mismatched schemas and verify that:
//! - Skip operations correctly consume remote bytes without materializing them.
//! - Field reordering maps correctly to local layout.
//! - Unknown enum variants produce `UnknownVariant` errors at decode time.
//! - Nested type mismatches are caught at plan-build time (not decode time).

use facet::Facet;
use vox_postcard::{
    SchemaSet, build_plan, from_slice_with_plan, plan::PlanInput, serialize::to_vec,
};
use vox_types::schema::extract_schemas;

use crate::differential::ErrorClass;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn schema_set_for<T: Facet<'static>>() -> SchemaSet {
    let extracted = extract_schemas(T::SHAPE).expect("schema extraction failed");
    SchemaSet::from_root_and_schemas(extracted.root.clone(), extracted.schemas.clone())
}

// ---------------------------------------------------------------------------
// Skip: remote has extra field that local doesn't know about
//
// Both sides must share the same nominal type name for build_plan to succeed.
// We define "Item" in two sub-modules.
// ---------------------------------------------------------------------------

mod skip_test_types {
    pub mod remote {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Item {
            pub value: u32,
            pub extra: String,
        }
    }
    pub mod local {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Item {
            pub value: u32,
        }
    }
}

#[test]
fn translation_skip_extra_remote_field() {
    use skip_test_types::{local, remote};

    let remote_set = schema_set_for::<remote::Item>();
    let local_set = schema_set_for::<local::Item>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan should succeed: extra field is skipped");

    let remote_val = remote::Item {
        value: 123,
        extra: "ignored_data".to_string(),
    };
    let bytes = to_vec(&remote_val).expect("encode");

    let result: local::Item =
        from_slice_with_plan(&bytes, &plan, &remote_set.registry).expect("decode");

    assert_eq!(result.value, 123);
}

#[test]
fn translation_skip_large_extra_field() {
    use skip_test_types::{local, remote};

    let remote_set = schema_set_for::<remote::Item>();
    let local_set = schema_set_for::<local::Item>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan");

    let long_extra = "x".repeat(1000);
    let remote_val = remote::Item {
        value: 77,
        extra: long_extra,
    };
    let bytes = to_vec(&remote_val).expect("encode");

    let result: local::Item =
        from_slice_with_plan(&bytes, &plan, &remote_set.registry).expect("decode");
    assert_eq!(result.value, 77);
}

// ---------------------------------------------------------------------------
// Field reorder: remote has fields in different order from local.
// Both sides named "Pair" so build_plan accepts them.
// ---------------------------------------------------------------------------

mod reorder_types {
    pub mod remote {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Pair {
            pub b: u32,
            pub a: u32,
        }
    }
    pub mod local {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Pair {
            pub a: u32,
            pub b: u32,
        }
    }
}

#[test]
fn translation_field_reorder() {
    use reorder_types::{local, remote};

    let remote_set = schema_set_for::<remote::Pair>();
    let local_set = schema_set_for::<local::Pair>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan for field reorder");

    // Remote encodes b=10, a=20 (remote wire order)
    let remote_val = remote::Pair { b: 10, a: 20 };
    let bytes = to_vec(&remote_val).expect("encode");

    let result: local::Pair =
        from_slice_with_plan(&bytes, &plan, &remote_set.registry).expect("decode");

    // Local fields matched by name: a=20, b=10
    assert_eq!(result.a, 20, "field 'a' value");
    assert_eq!(result.b, 10, "field 'b' value");
}

// ---------------------------------------------------------------------------
// Enum: remote has extra variant unknown to local.
// Both sides named "Status" so build_plan accepts them.
// ---------------------------------------------------------------------------

mod enum_compat_types {
    pub mod remote {
        #[derive(facet::Facet, Debug, PartialEq)]
        #[repr(u8)]
        pub enum Status {
            Ok,
            Warn,
            Error, // not in local
        }
    }
    pub mod local {
        #[derive(facet::Facet, Debug, PartialEq)]
        #[repr(u8)]
        pub enum Status {
            Ok,
            Warn,
        }
    }
}

#[test]
fn translation_enum_unknown_remote_variant_is_runtime_error() {
    use enum_compat_types::{local, remote};

    let remote_set = schema_set_for::<remote::Status>();
    let local_set = schema_set_for::<local::Status>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan: unknown remote variants are ok at plan time");

    // Known variant Ok: should succeed
    let bytes_ok = to_vec(&remote::Status::Ok).expect("encode Ok");
    let result_ok: local::Status =
        from_slice_with_plan(&bytes_ok, &plan, &remote_set.registry).expect("decode Ok");
    assert_eq!(result_ok, local::Status::Ok);

    // Known variant Warn: should succeed
    let bytes_warn = to_vec(&remote::Status::Warn).expect("encode Warn");
    let result_warn: local::Status =
        from_slice_with_plan(&bytes_warn, &plan, &remote_set.registry).expect("decode Warn");
    assert_eq!(result_warn, local::Status::Warn);

    // Unknown variant Error (discriminant 2): should fail with UnknownVariant
    let bytes_error = to_vec(&remote::Status::Error).expect("encode Error");
    let err = from_slice_with_plan::<local::Status>(&bytes_error, &plan, &remote_set.registry)
        .expect_err("expected UnknownVariant error");
    assert_eq!(
        ErrorClass::of(&err),
        ErrorClass::UnknownVariant,
        "wrong error class: {err}"
    );
}

// ---------------------------------------------------------------------------
// Nested: struct with nested Vec<String> — translation plan propagates.
// Both sides named "Container" so build_plan accepts them.
// ---------------------------------------------------------------------------

mod nested_types {
    pub mod remote {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Container {
            pub tags: Vec<String>,
            pub extra: u32,
        }
    }
    pub mod local {
        #[derive(facet::Facet, Debug, PartialEq)]
        pub struct Container {
            pub tags: Vec<String>,
        }
    }
}

#[test]
fn translation_nested_vec_string() {
    use nested_types::{local, remote};

    let remote_set = schema_set_for::<remote::Container>();
    let local_set = schema_set_for::<local::Container>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan");

    let remote_val = remote::Container {
        tags: vec!["x".to_string(), "y".to_string()],
        extra: 99,
    };
    let bytes = to_vec(&remote_val).expect("encode");

    let result: local::Container =
        from_slice_with_plan(&bytes, &plan, &remote_set.registry).expect("decode");
    assert_eq!(result.tags, vec!["x", "y"]);
}

// ---------------------------------------------------------------------------
// Fill defaults: local has extra field with #[facet(default)] that remote
// didn't send. Decode must zero/default-initialize that field rather than
// leaving uninitialized memory.
// ---------------------------------------------------------------------------

mod fill_default_types {
    pub mod remote {
        #[derive(facet::Facet, Debug, PartialEq, Clone)]
        pub struct Point {
            pub x: f64,
            pub y: f64,
        }
    }
    pub mod local {
        #[derive(facet::Facet, Debug, PartialEq, Clone)]
        pub struct Point {
            pub x: f64,
            pub y: f64,
            #[facet(default)]
            pub z: f64,
        }
    }
}

#[test]
fn translation_fill_default_missing_field() {
    use fill_default_types::{local, remote};

    let remote_set = schema_set_for::<remote::Point>();
    let local_set = schema_set_for::<local::Point>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan: missing field has default, must accept");

    let remote_val = remote::Point { x: 6.0, y: 8.0 };
    let bytes = to_vec(&remote_val).expect("encode");

    // Oracle path (reflective Partial::build) — must fill z=0.0.
    let result: local::Point =
        from_slice_with_plan(&bytes, &plan, &remote_set.registry).expect("decode");
    assert_eq!(result.x, 6.0);
    assert_eq!(result.y, 8.0);
    assert_eq!(result.z, 0.0, "oracle: missing field must fill default");
}

/// Same scenario, exercised through the JIT compile path + IR interpreter.
/// This is the code path the RPC dispatch uses and where the fill-defaults
/// bug originally surfaced: JIT/IR never emits an op for unmatched local
/// fields, so whatever happened to be at that memory location leaks through.
#[test]
fn translation_fill_default_missing_field_jit() {
    use std::mem::MaybeUninit;

    use fill_default_types::{local, remote};
    use vox_jit::{
        CraneliftBackend,
        abi::{DecodeCtx, DecodeStatus},
    };
    use vox_jit_cal::{BorrowMode, CalibrationRegistry};
    use vox_postcard::ir::{from_slice_ir, lower_with_cal};

    let remote_set = schema_set_for::<remote::Point>();
    let local_set = schema_set_for::<local::Point>();

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan");

    let remote_val = remote::Point { x: 6.0, y: 8.0 };
    let bytes = to_vec(&remote_val).expect("encode");

    // IR interpreter — goes through the same `lower_struct` path the JIT uses.
    let cal = CalibrationRegistry::default();
    let ir_result: local::Point =
        from_slice_ir(&bytes, &plan, &remote_set.registry, Some(&cal)).expect("IR decode");
    assert_eq!(ir_result.x, 6.0);
    assert_eq!(ir_result.y, 8.0);
    assert_eq!(ir_result.z, 0.0, "IR: missing field must fill default");

    // JIT stub — out buffer is pre-poisoned with nonzero bytes so that a
    // decoder which leaves z untouched is caught even if the result happens
    // to be 0.0 on some allocators.
    let program = lower_with_cal(
        &plan,
        <local::Point as facet::Facet>::SHAPE,
        &remote_set.registry,
        Some(&cal),
        BorrowMode::Owned,
    )
    .expect("lower");
    let mut backend = CraneliftBackend::new().expect("backend");
    let owned_fn = backend
        .compile_decode_owned(<local::Point as facet::Facet>::SHAPE, &program, &cal)
        .expect("compile");

    let mut out = MaybeUninit::<local::Point>::uninit();
    unsafe {
        std::ptr::write_bytes(
            out.as_mut_ptr() as *mut u8,
            0xAA,
            std::mem::size_of::<local::Point>(),
        );
    }
    let mut ctx = DecodeCtx::new(&bytes);
    let status = unsafe { owned_fn(&mut ctx, out.as_mut_ptr() as *mut u8) };
    assert_eq!(status, DecodeStatus::Ok, "JIT decode status");
    let jit_result = unsafe { out.assume_init() };
    assert_eq!(jit_result.x, 6.0);
    assert_eq!(jit_result.y, 8.0);
    assert_eq!(jit_result.z, 0.0, "JIT: missing field must fill default");
}

// ---------------------------------------------------------------------------
// Type mismatch: build_plan must reject structurally incompatible schemas
// at plan-build time (not decode time).
// ---------------------------------------------------------------------------

#[test]
fn translation_plan_rejects_kind_mismatch() {
    #[derive(Facet, Debug, PartialEq)]
    struct RemoteStruct {
        pub x: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum LocalEnum {
        Foo,
    }

    let remote_set = schema_set_for::<RemoteStruct>();
    let local_set = schema_set_for::<LocalEnum>();

    let result = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    });

    assert!(
        result.is_err(),
        "build_plan should reject struct-vs-enum mismatch"
    );
}
