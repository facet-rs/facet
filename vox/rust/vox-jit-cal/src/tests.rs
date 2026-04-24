//! Calibration self-tests — these run as plain Rust tests.
//!
//! The qa-engineer owns the Miri + fuzz coverage layer (task #15).
//! These tests gate whether a concrete T may use the fast path.

use crate::{
    CalibrationResult, ContainerKind, OFFSET_ABSENT, calibrate_box_slice, calibrate_box_t,
    calibrate_string, calibrate_vec,
};

/// Validate a Vec<T> or String descriptor (three-word layout, all slots present).
fn assert_three_word_desc_sane(result: CalibrationResult, label: &str) -> crate::OpaqueDescriptor {
    match result {
        CalibrationResult::Ok(d) => {
            // Offsets must be distinct.
            assert_ne!(
                d.ptr_offset, d.len_offset,
                "{label}: ptr and len share offset"
            );
            assert_ne!(
                d.ptr_offset, d.cap_offset,
                "{label}: ptr and cap share offset"
            );
            assert_ne!(
                d.len_offset, d.cap_offset,
                "{label}: len and cap share offset"
            );

            // Neither len nor cap may be OFFSET_ABSENT.
            assert_ne!(d.len_offset, OFFSET_ABSENT, "{label}: len_offset is ABSENT");
            assert_ne!(d.cap_offset, OFFSET_ABSENT, "{label}: cap_offset is ABSENT");

            let pw = std::mem::size_of::<usize>() as u8;
            for &off in &[d.ptr_offset, d.len_offset, d.cap_offset] {
                assert_eq!(off % pw, 0, "{label}: offset {off} not word-aligned");
                assert!(
                    (off as usize) < d.size,
                    "{label}: offset {off} >= size {}",
                    d.size
                );
            }

            assert_eq!(
                d.empty_bytes.len(),
                d.size,
                "{label}: empty_bytes len != size"
            );
            d
        }
        CalibrationResult::Unsupported { reason } => {
            panic!("{label}: calibration failed: {reason}");
        }
    }
}

/// Validate a Box<T> descriptor (one-word layout, no len, no cap).
fn assert_box_t_desc_sane(result: CalibrationResult, label: &str) -> crate::OpaqueDescriptor {
    match result {
        CalibrationResult::Ok(d) => {
            assert_eq!(d.kind, ContainerKind::BoxOwned, "{label}: wrong kind");
            assert_eq!(
                d.size,
                std::mem::size_of::<usize>(),
                "{label}: Box<T> size != ptr_width"
            );
            assert_eq!(d.ptr_offset, 0, "{label}: Box<T> ptr_offset != 0");
            assert_eq!(
                d.len_offset, OFFSET_ABSENT,
                "{label}: len_offset should be ABSENT"
            );
            assert_eq!(
                d.cap_offset, OFFSET_ABSENT,
                "{label}: cap_offset should be ABSENT"
            );
            assert_eq!(
                d.empty_bytes.len(),
                d.size,
                "{label}: empty_bytes len != size"
            );
            d
        }
        CalibrationResult::Unsupported { reason } => {
            panic!("{label}: calibration failed: {reason}");
        }
    }
}

/// Validate a Box<[T]> descriptor (two-word fat pointer, len present, no cap).
fn assert_box_slice_desc_sane(result: CalibrationResult, label: &str) -> crate::OpaqueDescriptor {
    match result {
        CalibrationResult::Ok(d) => {
            assert_eq!(d.kind, ContainerKind::BoxSlice, "{label}: wrong kind");
            assert_eq!(
                d.size,
                2 * std::mem::size_of::<usize>(),
                "{label}: Box<[T]> size != 2*ptr_width"
            );
            assert_ne!(d.ptr_offset, OFFSET_ABSENT, "{label}: ptr_offset is ABSENT");
            assert_ne!(d.len_offset, OFFSET_ABSENT, "{label}: len_offset is ABSENT");
            assert_eq!(
                d.cap_offset, OFFSET_ABSENT,
                "{label}: cap_offset should be ABSENT"
            );
            assert_ne!(
                d.ptr_offset, d.len_offset,
                "{label}: ptr and len share offset"
            );

            let pw = std::mem::size_of::<usize>() as u8;
            for &off in &[d.ptr_offset, d.len_offset] {
                assert_eq!(off % pw, 0, "{label}: offset {off} not word-aligned");
                assert!(
                    (off as usize) < d.size,
                    "{label}: offset {off} >= size {}",
                    d.size
                );
            }

            assert_eq!(
                d.empty_bytes.len(),
                d.size,
                "{label}: empty_bytes len != size"
            );
            d
        }
        CalibrationResult::Unsupported { reason } => {
            panic!("{label}: calibration failed: {reason}");
        }
    }
}

// -----------------------------------------------------------------------
// Vec<T> tests (unchanged from before, using renamed helper)
// -----------------------------------------------------------------------

#[test]
fn vec_u8_calibration() {
    let d = assert_three_word_desc_sane(calibrate_vec::<u8>(), "Vec<u8>");
    assert_eq!(d.kind, ContainerKind::Vec);
    assert_eq!(d.elem_size, 1);
    assert_eq!(d.elem_align, 1);
}

#[test]
fn vec_u32_calibration() {
    let d = assert_three_word_desc_sane(calibrate_vec::<u32>(), "Vec<u32>");
    assert_eq!(d.elem_size, 4);
    assert_eq!(d.elem_align, 4);
}

#[test]
fn vec_u64_calibration() {
    let d = assert_three_word_desc_sane(calibrate_vec::<u64>(), "Vec<u64>");
    assert_eq!(d.elem_size, 8);
}

#[test]
fn vec_usize_calibration() {
    let d = assert_three_word_desc_sane(calibrate_vec::<usize>(), "Vec<usize>");
    assert_eq!(d.elem_size, std::mem::size_of::<usize>());
}

#[test]
fn vec_zst_calibration() {
    #[derive(Clone, Copy)]
    struct Zst;
    let d = assert_three_word_desc_sane(calibrate_vec::<Zst>(), "Vec<Zst>");
    assert_eq!(d.elem_size, 0);
}

#[test]
fn string_calibration() {
    let d = assert_three_word_desc_sane(calibrate_string(), "String");
    assert_eq!(d.kind, ContainerKind::String);
    assert_eq!(d.elem_size, 1);
    assert_eq!(d.elem_align, 1);
}

#[test]
fn vec_and_string_offsets_independent() {
    // Ensure String is calibrated separately from Vec<u8>.
    let vec_u8 = assert_three_word_desc_sane(calibrate_vec::<u8>(), "Vec<u8>");
    let string = assert_three_word_desc_sane(calibrate_string(), "String");
    // On the current stable compiler these agree, but they come from
    // independent probes — the point is they are separate calibrations.
    let _ = (vec_u8, string);
}

#[test]
fn empty_bytes_len_matches_size() {
    let r = calibrate_vec::<u32>();
    if let CalibrationResult::Ok(d) = r {
        assert_eq!(d.empty_bytes.len(), d.size);
    }
}

#[test]
fn registry_roundtrip() {
    use crate::CalibrationRegistry;
    let mut reg = CalibrationRegistry::new();

    let h_vec = reg.calibrate_vec::<u8>().expect("Vec<u8> should calibrate");
    let h_str = reg.calibrate_string().expect("String should calibrate");

    let d_vec = reg.get(h_vec).expect("handle valid");
    let d_str = reg.get(h_str).expect("handle valid");

    assert_eq!(d_vec.elem_size, 1);
    assert_eq!(d_str.elem_size, 1);
    assert_ne!(h_vec, h_str);
}

// -----------------------------------------------------------------------
// Box<T> tests  (task #19)
// -----------------------------------------------------------------------

#[test]
fn box_t_u8_calibration() {
    let d = assert_box_t_desc_sane(calibrate_box_t::<u8>(), "Box<u8>");
    assert_eq!(d.elem_size, 1);
    assert_eq!(d.elem_align, 1);
}

#[test]
fn box_t_u64_calibration() {
    let d = assert_box_t_desc_sane(calibrate_box_t::<u64>(), "Box<u64>");
    assert_eq!(d.elem_size, 8);
}

#[test]
fn box_t_zst_calibration() {
    #[derive(Clone, Copy)]
    struct Zst;
    let d = assert_box_t_desc_sane(calibrate_box_t::<Zst>(), "Box<Zst>");
    assert_eq!(d.elem_size, 0);
}

#[test]
fn box_t_empty_bytes_is_dangling_sentinel() {
    // The empty_bytes for Box<T> must be nonzero (it's a dangling sentinel,
    // not a null pointer).
    let r = calibrate_box_t::<u32>();
    if let CalibrationResult::Ok(d) = r {
        let val = usize::from_ne_bytes(d.empty_bytes.try_into().unwrap());
        assert_ne!(val, 0, "Box<T> dangling sentinel must be non-zero");
    }
}

// -----------------------------------------------------------------------
// Box<[T]> tests  (task #19)
// -----------------------------------------------------------------------

#[test]
fn box_slice_u8_calibration() {
    let d = assert_box_slice_desc_sane(calibrate_box_slice::<u8>(), "Box<[u8]>");
    assert_eq!(d.elem_size, 1);
    assert_eq!(d.elem_align, 1);
}

#[test]
fn box_slice_u32_calibration() {
    let d = assert_box_slice_desc_sane(calibrate_box_slice::<u32>(), "Box<[u32]>");
    assert_eq!(d.elem_size, 4);
}

#[test]
fn box_slice_u64_calibration() {
    let d = assert_box_slice_desc_sane(calibrate_box_slice::<u64>(), "Box<[u64]>");
    assert_eq!(d.elem_size, 8);
}

#[test]
fn box_slice_zst_calibration() {
    #[derive(Clone, Copy)]
    struct Zst;
    let d = assert_box_slice_desc_sane(calibrate_box_slice::<Zst>(), "Box<[Zst]>");
    assert_eq!(d.elem_size, 0);
}

#[test]
fn box_slice_empty_bytes_len_matches_size() {
    let r = calibrate_box_slice::<u32>();
    if let CalibrationResult::Ok(d) = r {
        assert_eq!(d.empty_bytes.len(), d.size);
    }
}

#[test]
fn box_and_vec_are_distinct_kinds() {
    // Box<[u8]> and Vec<u8> must not have the same ContainerKind —
    // they are calibrated separately and must never be folded.
    let box_slice = calibrate_box_slice::<u8>();
    let vec_u8 = calibrate_vec::<u8>();
    if let (CalibrationResult::Ok(b), CalibrationResult::Ok(v)) = (box_slice, vec_u8) {
        assert_ne!(
            b.kind, v.kind,
            "Box<[u8]> and Vec<u8> must have distinct kinds"
        );
    }
}

#[test]
fn box_registry_roundtrip() {
    use crate::CalibrationRegistry;
    let mut reg = CalibrationRegistry::new();

    let h_box_t = reg
        .calibrate_box_t::<u32>()
        .expect("Box<u32> should calibrate");
    let h_box_sl = reg
        .calibrate_box_slice::<u32>()
        .expect("Box<[u32]> should calibrate");

    let d_box_t = reg.get(h_box_t).expect("handle valid");
    let d_box_sl = reg.get(h_box_sl).expect("handle valid");

    assert_eq!(d_box_t.kind, ContainerKind::BoxOwned);
    assert_eq!(d_box_sl.kind, ContainerKind::BoxSlice);
    assert_ne!(h_box_t, h_box_sl);
}

#[test]
fn with_common_populates_registry() {
    use crate::{CalibrationRegistry, ContainerKind};
    let mut reg = CalibrationRegistry::new();
    reg.with_common();

    // `with_common` now only calibrates `String`. Vec<T>, Box<T>, and
    // Box<[T]> are handled on-demand via `get_or_calibrate_by_shape`.
    for (i, (_, d)) in reg.iter().enumerate() {
        assert!(d.size > 0, "descriptor {i}: size must be > 0");
        assert!(d.align > 0, "descriptor {i}: align must be > 0");
        assert_eq!(
            d.empty_bytes.len(),
            d.size,
            "descriptor {i}: empty_bytes length must equal size"
        );
    }
    assert!(
        reg.iter().any(|(_, d)| d.kind == ContainerKind::String),
        "with_common must include a String descriptor"
    );
}

// -----------------------------------------------------------------------
// get_or_calibrate_by_shape tests  (task #29)
// -----------------------------------------------------------------------

#[test]
fn on_demand_vec_u32_by_shape() {
    use crate::{CalibrationRegistry, ContainerKind};
    use facet::Facet;
    let mut reg = CalibrationRegistry::new();
    let shape = <Vec<u32> as Facet>::SHAPE;
    let handle = reg
        .get_or_calibrate_by_shape(shape)
        .expect("Vec<u32> should calibrate on demand");
    let desc = reg.get(handle).expect("handle valid");
    assert_eq!(desc.kind, ContainerKind::Vec);
    assert_eq!(desc.elem_size, 4);
    assert_eq!(desc.empty_bytes.len(), desc.size);
}

#[test]
fn on_demand_vec_u8_by_shape() {
    use crate::{CalibrationRegistry, ContainerKind};
    use facet::Facet;
    let mut reg = CalibrationRegistry::new();
    let handle = reg
        .get_or_calibrate_by_shape(<Vec<u8> as Facet>::SHAPE)
        .expect("Vec<u8> should calibrate on demand");
    let desc = reg.get(handle).unwrap();
    assert_eq!(desc.kind, ContainerKind::Vec);
    assert_eq!(desc.elem_size, 1);
}

#[test]
fn on_demand_vec_cached_on_second_call() {
    use crate::CalibrationRegistry;
    use facet::Facet;
    let mut reg = CalibrationRegistry::new();
    let shape = <Vec<u64> as Facet>::SHAPE;
    let h1 = reg.get_or_calibrate_by_shape(shape).expect("first call");
    let h2 = reg.get_or_calibrate_by_shape(shape).expect("second call");
    assert_eq!(h1, h2, "second call must return the cached handle");
    assert_eq!(reg.len(), 1, "must not register a duplicate descriptor");
}

#[test]
fn on_demand_box_t_by_shape() {
    use crate::{CalibrationRegistry, ContainerKind, OFFSET_ABSENT};
    use facet::Facet;
    let mut reg = CalibrationRegistry::new();
    let shape = <Box<u32> as Facet>::SHAPE;
    let handle = reg
        .get_or_calibrate_by_shape(shape)
        .expect("Box<u32> should calibrate on demand");
    let desc = reg.get(handle).unwrap();
    assert_eq!(desc.kind, ContainerKind::BoxOwned);
    assert_eq!(desc.len_offset, OFFSET_ABSENT);
    assert_eq!(desc.cap_offset, OFFSET_ABSENT);
    assert_eq!(desc.elem_size, 4);
}

#[test]
fn on_demand_unsupported_shape_returns_none() {
    use crate::CalibrationRegistry;
    use facet::Facet;
    // A plain scalar (u32) is not a container; on-demand calibration must return None.
    let mut reg = CalibrationRegistry::new();
    let result = reg.get_or_calibrate_by_shape(<u32 as Facet>::SHAPE);
    assert!(
        result.is_none(),
        "u32 shape must not calibrate as a container"
    );
}

// -----------------------------------------------------------------------
// Regression test: structural caching must not rely on pointer address  (#30)
// -----------------------------------------------------------------------

// Miri: Box::leak is intentional here (creates a second &'static Shape at a
// distinct address to test structural equality). Skip under Miri to avoid
// the spurious leak report — this is a test artifact, not a production bug.
#[cfg_attr(miri, ignore)]
#[test]
fn shape_cache_is_structural_not_pointer_based() {
    // Simulate the cross-crate-duplicate scenario: two &'static Shape references
    // that are structurally identical (same ConstTypeId) but at different pointer
    // addresses. Both must resolve to the same descriptor handle.
    //
    // We construct this by building a second Shape value in heap memory whose
    // `id` field equals `Vec<u32>::SHAPE.id`. Since Shape::Hash and Shape::PartialEq
    // both delegate to `self.id` (a ConstTypeId derived from TypeId), the two shapes
    // are `==` and hash identically — and our cache must reflect that.
    use crate::{CalibrationRegistry, DescriptorHandle};
    use facet::Facet;
    use facet_core::Shape;

    let canonical: &'static Shape = <Vec<u32> as Facet>::SHAPE;

    // Build a heap-allocated copy of the Shape header. We need to put it in a
    // 'static reference without leaking memory in tests — we use Box::leak here
    // deliberately (test-only, process exits after). We only copy the `id` field
    // because that's the sole field used by Hash/PartialEq; other fields don't
    // affect cache key equality.
    //
    // This is the exact scenario that occurs with cross-crate generic statics:
    // `Vec<u32>::SHAPE` can appear at two different addresses in the final binary.
    let duplicate: &'static Shape = Box::leak(Box::new(*canonical));

    // The two shapes must be at different addresses to make the test meaningful.
    assert_ne!(
        canonical as *const Shape as usize, duplicate as *const Shape as usize,
        "test setup: the two shapes must be at different addresses"
    );

    // Both must compare equal (structural identity via ConstTypeId).
    assert_eq!(
        canonical, duplicate,
        "shapes with the same type must be equal (PartialEq keys by ConstTypeId)"
    );

    // Calibrate using the canonical pointer.
    let mut reg = CalibrationRegistry::new();
    let h1: DescriptorHandle = reg
        .get_or_calibrate_by_shape(canonical)
        .expect("canonical Vec<u32> shape must calibrate");

    // Retrieve using the duplicate pointer — must hit the same cached entry.
    let h2: DescriptorHandle = reg
        .get_or_calibrate_by_shape(duplicate)
        .expect("duplicate Vec<u32> shape must find cached descriptor");

    assert_eq!(
        h1, h2,
        "both shape pointers must resolve to the same descriptor handle (structural cache)"
    );
    assert_eq!(
        reg.len(),
        1,
        "only one descriptor must be registered (no duplicate from second pointer)"
    );
}

// Miri: same Box::leak rationale as shape_cache_is_structural_not_pointer_based.
#[cfg_attr(miri, ignore)]
#[test]
fn lookup_by_shape_is_structural_not_pointer_based() {
    // Same regression as above but for `register_for_shape` + `lookup_by_shape`.
    use crate::{CalibrationRegistry, calibrate_vec};
    use facet::Facet;
    use facet_core::Shape;

    let canonical: &'static Shape = <Vec<u32> as Facet>::SHAPE;
    let duplicate: &'static Shape = Box::leak(Box::new(*canonical));

    assert_ne!(
        canonical as *const Shape as usize,
        duplicate as *const Shape as usize,
    );

    if let crate::CalibrationResult::Ok(desc) = calibrate_vec::<u32>() {
        let mut reg = CalibrationRegistry::new();
        reg.register_for_shape(canonical, desc);

        // Lookup via duplicate pointer must succeed.
        let found = reg.lookup_by_shape(duplicate);
        assert!(
            found.is_some(),
            "lookup_by_shape with a structurally equal shape at a different address must find the descriptor"
        );
    }
}
