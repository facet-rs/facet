//! Regression tests for Tier-2 JIT field count limits
//!
//! The Tier-2 JIT uses u64 bitsets for tracking required fields and flattened enum variants.
//! This means the total number of tracking bits (required fields + flattened enums) cannot exceed 64.
//! These tests verify that structs exceeding this limit are properly rejected with clear diagnostics.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_format::jit as format_jit;
use facet_format_json::JsonParser;

/// Test that structs with >=64 required fields are rejected
#[test]
fn test_too_many_required_fields() {
    // Define a struct with 64 required (non-Option) fields (max is 63)
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct ManyFields {
        f0: u64,
        f1: u64,
        f2: u64,
        f3: u64,
        f4: u64,
        f5: u64,
        f6: u64,
        f7: u64,
        f8: u64,
        f9: u64,
        f10: u64,
        f11: u64,
        f12: u64,
        f13: u64,
        f14: u64,
        f15: u64,
        f16: u64,
        f17: u64,
        f18: u64,
        f19: u64,
        f20: u64,
        f21: u64,
        f22: u64,
        f23: u64,
        f24: u64,
        f25: u64,
        f26: u64,
        f27: u64,
        f28: u64,
        f29: u64,
        f30: u64,
        f31: u64,
        f32: u64,
        f33: u64,
        f34: u64,
        f35: u64,
        f36: u64,
        f37: u64,
        f38: u64,
        f39: u64,
        f40: u64,
        f41: u64,
        f42: u64,
        f43: u64,
        f44: u64,
        f45: u64,
        f46: u64,
        f47: u64,
        f48: u64,
        f49: u64,
        f50: u64,
        f51: u64,
        f52: u64,
        f53: u64,
        f54: u64,
        f55: u64,
        f56: u64,
        f57: u64,
        f58: u64,
        f59: u64,
        f60: u64,
        f61: u64,
        f62: u64,
        f63: u64, // 64th field - exceeds limit of 63
    }

    // Attempt Tier-2 compilation - should fail
    let result = format_jit::get_format_deserializer::<Vec<ManyFields>, JsonParser>();

    assert!(
        result.is_none(),
        "Tier-2 JIT should reject structs with >=64 required fields (max is 63)"
    );
}

/// Test that structs with exactly 63 required fields are accepted
#[test]
fn test_exactly_63_required_fields() {
    // Define a struct with exactly 63 required fields (the maximum)
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct ExactlyMaxFields {
        f0: u64,
        f1: u64,
        f2: u64,
        f3: u64,
        f4: u64,
        f5: u64,
        f6: u64,
        f7: u64,
        f8: u64,
        f9: u64,
        f10: u64,
        f11: u64,
        f12: u64,
        f13: u64,
        f14: u64,
        f15: u64,
        f16: u64,
        f17: u64,
        f18: u64,
        f19: u64,
        f20: u64,
        f21: u64,
        f22: u64,
        f23: u64,
        f24: u64,
        f25: u64,
        f26: u64,
        f27: u64,
        f28: u64,
        f29: u64,
        f30: u64,
        f31: u64,
        f32: u64,
        f33: u64,
        f34: u64,
        f35: u64,
        f36: u64,
        f37: u64,
        f38: u64,
        f39: u64,
        f40: u64,
        f41: u64,
        f42: u64,
        f43: u64,
        f44: u64,
        f45: u64,
        f46: u64,
        f47: u64,
        f48: u64,
        f49: u64,
        f50: u64,
        f51: u64,
        f52: u64,
        f53: u64,
        f54: u64,
        f55: u64,
        f56: u64,
        f57: u64,
        f58: u64,
        f59: u64,
        f60: u64,
        f61: u64,
        f62: u64, // Exactly 63 fields (the maximum)
    }

    // Attempt Tier-2 compilation - should succeed
    let result = format_jit::get_format_deserializer::<Vec<ExactlyMaxFields>, JsonParser>();

    assert!(
        result.is_some(),
        "Tier-2 JIT should accept structs with exactly 63 required fields (the maximum)"
    );
}

/// Test that flattened struct fields count toward the limit
#[test]
fn test_flattened_struct_field_limit() {
    // Inner struct with 40 required fields
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Inner {
        i0: u64,
        i1: u64,
        i2: u64,
        i3: u64,
        i4: u64,
        i5: u64,
        i6: u64,
        i7: u64,
        i8: u64,
        i9: u64,
        i10: u64,
        i11: u64,
        i12: u64,
        i13: u64,
        i14: u64,
        i15: u64,
        i16: u64,
        i17: u64,
        i18: u64,
        i19: u64,
        i20: u64,
        i21: u64,
        i22: u64,
        i23: u64,
        i24: u64,
        i25: u64,
        i26: u64,
        i27: u64,
        i28: u64,
        i29: u64,
        i30: u64,
        i31: u64,
        i32: u64,
        i33: u64,
        i34: u64,
        i35: u64,
        i36: u64,
        i37: u64,
        i38: u64,
        i39: u64, // 40 fields
    }

    // Outer struct with 24 required fields + flattened Inner (40 fields) = 64 total (exceeds max of 63)
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Outer {
        o0: u64,
        o1: u64,
        o2: u64,
        o3: u64,
        o4: u64,
        o5: u64,
        o6: u64,
        o7: u64,
        o8: u64,
        o9: u64,
        o10: u64,
        o11: u64,
        o12: u64,
        o13: u64,
        o14: u64,
        o15: u64,
        o16: u64,
        o17: u64,
        o18: u64,
        o19: u64,
        o20: u64,
        o21: u64,
        o22: u64,
        o23: u64, // 24 fields
        #[facet(flatten)]
        inner: Inner, // +40 fields = 64 total (exceeds max of 63)
    }

    // Should be rejected due to >=64 tracking bits
    let result = format_jit::get_format_deserializer::<Vec<Outer>, JsonParser>();

    assert!(
        result.is_none(),
        "Tier-2 JIT should reject structs where total fields (including flattened) exceed 63"
    );
}

/// Test that Option fields don't count toward the required field limit
#[test]
fn test_option_fields_dont_count() {
    // 70 Option fields + 10 required fields = 80 total fields, but only 10 tracking bits needed
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct ManyOptionalFields {
        // Required fields (10 - well under limit)
        r0: u64,
        r1: u64,
        r2: u64,
        r3: u64,
        r4: u64,
        r5: u64,
        r6: u64,
        r7: u64,
        r8: u64,
        r9: u64,
        // Optional fields (70 - don't count toward limit)
        o0: Option<u64>,
        o1: Option<u64>,
        o2: Option<u64>,
        o3: Option<u64>,
        o4: Option<u64>,
        o5: Option<u64>,
        o6: Option<u64>,
        o7: Option<u64>,
        o8: Option<u64>,
        o9: Option<u64>,
        o10: Option<u64>,
        o11: Option<u64>,
        o12: Option<u64>,
        o13: Option<u64>,
        o14: Option<u64>,
        o15: Option<u64>,
        o16: Option<u64>,
        o17: Option<u64>,
        o18: Option<u64>,
        o19: Option<u64>,
        o20: Option<u64>,
        o21: Option<u64>,
        o22: Option<u64>,
        o23: Option<u64>,
        o24: Option<u64>,
        o25: Option<u64>,
        o26: Option<u64>,
        o27: Option<u64>,
        o28: Option<u64>,
        o29: Option<u64>,
        o30: Option<u64>,
        o31: Option<u64>,
        o32: Option<u64>,
        o33: Option<u64>,
        o34: Option<u64>,
        o35: Option<u64>,
        o36: Option<u64>,
        o37: Option<u64>,
        o38: Option<u64>,
        o39: Option<u64>,
        o40: Option<u64>,
        o41: Option<u64>,
        o42: Option<u64>,
        o43: Option<u64>,
        o44: Option<u64>,
        o45: Option<u64>,
        o46: Option<u64>,
        o47: Option<u64>,
        o48: Option<u64>,
        o49: Option<u64>,
        o50: Option<u64>,
        o51: Option<u64>,
        o52: Option<u64>,
        o53: Option<u64>,
        o54: Option<u64>,
        o55: Option<u64>,
        o56: Option<u64>,
        o57: Option<u64>,
        o58: Option<u64>,
        o59: Option<u64>,
        o60: Option<u64>,
        o61: Option<u64>,
        o62: Option<u64>,
        o63: Option<u64>,
        o64: Option<u64>,
        o65: Option<u64>,
        o66: Option<u64>,
        o67: Option<u64>,
        o68: Option<u64>,
        o69: Option<u64>,
    }

    // Should succeed - only 10 tracking bits needed (for required fields)
    let result = format_jit::get_format_deserializer::<Vec<ManyOptionalFields>, JsonParser>();

    assert!(
        result.is_some(),
        "Tier-2 JIT should accept structs with many Option fields if required fields <= 63"
    );
}
