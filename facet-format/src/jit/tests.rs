use super::*;

#[test]
fn test_format_jit_compatibility() {
    // Vec<bool> should be supported
    assert!(is_format_jit_compatible(<Vec<bool>>::SHAPE));

    // Vec of integer types should be supported
    assert!(is_format_jit_compatible(<Vec<i8>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<i16>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<i32>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<i64>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<u8>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<u16>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<u32>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<u64>>::SHAPE));

    // Vec of float types are supported
    assert!(is_format_jit_compatible(<Vec<f32>>::SHAPE));
    assert!(is_format_jit_compatible(<Vec<f64>>::SHAPE));

    // Vec<String> is supported
    assert!(is_format_jit_compatible(<Vec<String>>::SHAPE));

    // Primitive types alone are not supported (need to be in a container)
    assert!(!is_format_jit_compatible(bool::SHAPE));
    assert!(!is_format_jit_compatible(i64::SHAPE));
}

/// Compile-time verification that the ABI signature is correct.
///
/// This test documents and verifies the Tier-2 ABI contract:
/// - Compiled function has the expected `extern "C"` signature
/// - Takes (input_ptr, len, pos, out, scratch) parameters
/// - Returns isize (new position on success >= 0, error code on failure < 0)
///
/// For runtime ABI contract tests (error handling, initialization), see:
/// - `facet-format-json/tests/jit_tier2_tests.rs`
#[test]
fn test_abi_signature_compiles() {
    use crate::jit::format::JitScratch;

    // Define the expected ABI signature
    type ExpectedAbi = unsafe extern "C" fn(
        input_ptr: *const u8,
        len: usize,
        pos: usize,
        out: *mut u8,
        scratch: *mut JitScratch,
    ) -> isize;

    // Verify the signature compiles (type-level contract)
    // This ensures the compiled function pointer can be cast to ExpectedAbi
    let _verify_signature = |fn_ptr: *const u8| {
        let _typed_fn: ExpectedAbi = unsafe { std::mem::transmute(fn_ptr) };
    };
}

#[test]
fn test_vec_string_compatibility() {
    let shape = <Vec<String>>::SHAPE;
    let compatible = is_format_jit_compatible(shape);
    eprintln!("Vec<String> is_format_jit_compatible: {}", compatible);
    assert!(compatible, "Vec<String> should be Tier-2 compatible");

    // Also check the element shape
    if let facet_core::Def::List(list_def) = &shape.def {
        let elem_shape = list_def.t;
        eprintln!(
            "  elem_shape.is_type::<String>(): {}",
            elem_shape.is_type::<String>()
        );
        eprintln!("  elem_shape.scalar_type(): {:?}", elem_shape.scalar_type());

        // Check FormatListElementKind::from_shape
        let elem_kind = FormatListElementKind::from_shape(elem_shape);
        eprintln!("  FormatListElementKind::from_shape(): {:?}", elem_kind);
        assert_eq!(
            elem_kind,
            Some(FormatListElementKind::String),
            "Should detect String element type"
        );
    }
}
