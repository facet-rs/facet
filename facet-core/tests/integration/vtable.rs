//! Tests for the new VTable API

use facet_core::{Facet, OxRef, VTableErased};

#[test]
fn test_bool_debug() {
    let shape = <bool as Facet>::SHAPE;
    let vtable = shape.vtable;

    // VTableErased should be Direct for bool
    match vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.debug.is_some(), "bool should have debug");
            assert!(vt.display.is_some(), "bool should have display");
            assert!(vt.partial_eq.is_some(), "bool should have partial_eq");
            assert!(vt.cmp.is_some(), "bool should have cmp");
        }
        VTableErased::Indirect(_) => {
            panic!("bool should use Direct vtable, not Indirect");
        }
    }
}

#[test]
fn test_str_indirect() {
    let shape = <str as Facet>::SHAPE;
    let vtable = shape.vtable;

    // VTableErased should be Indirect for str (unsized)
    match vtable {
        VTableErased::Direct(_) => {
            panic!("str should use Indirect vtable, not Direct");
        }
        VTableErased::Indirect(vt) => {
            assert!(vt.debug.is_some(), "str should have debug");
            assert!(vt.display.is_some(), "str should have display");
            assert!(vt.partial_eq.is_some(), "str should have partial_eq");
            assert!(vt.cmp.is_some(), "str should have cmp");
        }
    }
}

#[test]
fn test_integers_have_vtable() {
    // All integers should have Direct vtables with standard traits
    macro_rules! check_integer {
        ($t:ty) => {
            let shape = <$t as Facet>::SHAPE;
            match shape.vtable {
                VTableErased::Direct(vt) => {
                    assert!(
                        vt.debug.is_some(),
                        concat!(stringify!($t), " should have debug")
                    );
                    assert!(
                        vt.display.is_some(),
                        concat!(stringify!($t), " should have display")
                    );
                    assert!(
                        vt.partial_eq.is_some(),
                        concat!(stringify!($t), " should have partial_eq")
                    );
                    assert!(
                        vt.cmp.is_some(),
                        concat!(stringify!($t), " should have cmp")
                    );
                }
                VTableErased::Indirect(_) => {
                    panic!(concat!(stringify!($t), " should use Direct vtable"));
                }
            }
        };
    }

    check_integer!(u8);
    check_integer!(i8);
    check_integer!(u16);
    check_integer!(i16);
    check_integer!(u32);
    check_integer!(i32);
    check_integer!(u64);
    check_integer!(i64);
    check_integer!(u128);
    check_integer!(i128);
    check_integer!(usize);
    check_integer!(isize);
}

#[test]
fn test_hash_present() {
    // Types that implement Hash should have hash in vtable
    let bool_shape = <bool as Facet>::SHAPE;
    match bool_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.hash.is_some(), "bool should have hash");
        }
        _ => panic!("bool should use Direct vtable"),
    }

    let u32_shape = <u32 as Facet>::SHAPE;
    match u32_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.hash.is_some(), "u32 should have hash");
        }
        _ => panic!("u32 should use Direct vtable"),
    }

    let char_shape = <char as Facet>::SHAPE;
    match char_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.hash.is_some(), "char should have hash");
        }
        _ => panic!("char should use Direct vtable"),
    }

    let str_shape = <str as Facet>::SHAPE;
    match str_shape.vtable {
        VTableErased::Indirect(vt) => {
            assert!(vt.hash.is_some(), "str should have hash");
        }
        _ => panic!("str should use Indirect vtable"),
    }
}

#[test]
fn test_floats_no_hash() {
    // Floats don't implement Hash (because of NaN)
    let f32_shape = <f32 as Facet>::SHAPE;
    match f32_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.hash.is_none(), "f32 should NOT have hash (NaN)");
        }
        _ => panic!("f32 should use Direct vtable"),
    }

    let f64_shape = <f64 as Facet>::SHAPE;
    match f64_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.hash.is_none(), "f64 should NOT have hash (NaN)");
        }
        _ => panic!("f64 should use Direct vtable"),
    }
}

#[test]
fn test_floats_no_ord() {
    // Floats should NOT have cmp (because of NaN)
    let f32_shape = <f32 as Facet>::SHAPE;
    match f32_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.debug.is_some(), "f32 should have debug");
            assert!(vt.partial_eq.is_some(), "f32 should have partial_eq");
            assert!(vt.partial_cmp.is_some(), "f32 should have partial_cmp");
            assert!(vt.cmp.is_none(), "f32 should NOT have cmp (NaN)");
        }
        _ => panic!("f32 should use Direct vtable"),
    }

    let f64_shape = <f64 as Facet>::SHAPE;
    match f64_shape.vtable {
        VTableErased::Direct(vt) => {
            assert!(vt.debug.is_some(), "f64 should have debug");
            assert!(vt.partial_eq.is_some(), "f64 should have partial_eq");
            assert!(vt.partial_cmp.is_some(), "f64 should have partial_cmp");
            assert!(vt.cmp.is_none(), "f64 should NOT have cmp (NaN)");
        }
        _ => panic!("f64 should use Direct vtable"),
    }
}

#[test]
fn test_option_vtable_indirect() {
    // Option<T> uses Indirect vtable
    let shape = <Option<i32> as Facet>::SHAPE;
    match shape.vtable {
        VTableErased::Direct(_) => {
            panic!("Option<i32> should use Indirect vtable, not Direct");
        }
        VTableErased::Indirect(vt) => {
            assert!(vt.debug.is_some(), "Option should have debug");
            assert!(vt.hash.is_some(), "Option should have hash");
            assert!(vt.partial_eq.is_some(), "Option should have partial_eq");
            assert!(vt.partial_cmp.is_some(), "Option should have partial_cmp");
            assert!(vt.cmp.is_some(), "Option should have cmp");
        }
    }
    // drop_in_place moved from VTable to TypeOps
    assert!(shape.type_ops.is_some(), "Option should have type_ops");
}

#[test]
fn test_option_debug_some() {
    let value: Option<i32> = Some(42);
    let ox = OxRef::from_ref(&value);

    let debug_str = format!("{ox:?}");
    assert_eq!(debug_str, "Some(42)");
}

#[test]
fn test_option_debug_none() {
    let value: Option<i32> = None;
    let ox = OxRef::from_ref(&value);

    let debug_str = format!("{ox:?}");
    assert_eq!(debug_str, "None");
}

#[test]
fn test_option_partial_eq() {
    let some1: Option<i32> = Some(42);
    let some2: Option<i32> = Some(42);
    let some3: Option<i32> = Some(99);
    let none: Option<i32> = None;

    let ox1 = OxRef::from_ref(&some1);
    let ox2 = OxRef::from_ref(&some2);
    let ox3 = OxRef::from_ref(&some3);
    let ox_none = OxRef::from_ref(&none);

    // Some(42) == Some(42)
    assert!(ox1 == ox2);
    // Some(42) != Some(99)
    assert!(ox1 != ox3);
    // Some(42) != None
    assert!(ox1 != ox_none);
    // None == None
    assert!(ox_none == ox_none);
}

#[test]
fn test_option_partial_cmp() {
    let some1: Option<i32> = Some(42);
    let some2: Option<i32> = Some(99);
    let none: Option<i32> = None;

    let ox1 = OxRef::from_ref(&some1);
    let ox2 = OxRef::from_ref(&some2);
    let ox_none = OxRef::from_ref(&none);

    use std::cmp::Ordering;

    // None < Some(_)
    assert_eq!(ox_none.partial_cmp(&ox1), Some(Ordering::Less));
    // Some(42) < Some(99)
    assert_eq!(ox1.partial_cmp(&ox2), Some(Ordering::Less));
    // Some(99) > Some(42)
    assert_eq!(ox2.partial_cmp(&ox1), Some(Ordering::Greater));
    // None == None
    assert_eq!(ox_none.partial_cmp(&ox_none), Some(Ordering::Equal));
}

#[test]
fn test_option_hash() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let some1: Option<i32> = Some(42);
    let some2: Option<i32> = Some(42);
    let none: Option<i32> = None;

    let ox1 = OxRef::from_ref(&some1);
    let ox2 = OxRef::from_ref(&some2);
    let ox_none = OxRef::from_ref(&none);

    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    let mut h_none = DefaultHasher::new();

    ox1.hash(&mut h1);
    ox2.hash(&mut h2);
    ox_none.hash(&mut h_none);

    // Same values should have same hash
    assert_eq!(h1.finish(), h2.finish());
    // Different values should (likely) have different hashes
    // (not guaranteed, but highly likely for these values)
    assert_ne!(h1.finish(), h_none.finish());
}

#[test]
fn test_option_nested_debug() {
    // Test nested Option
    let value: Option<Option<i32>> = Some(Some(42));
    let ox = OxRef::from_ref(&value);

    let debug_str = format!("{ox:?}");
    assert_eq!(debug_str, "Some(Some(42))");

    let none_inner: Option<Option<i32>> = Some(None);
    let ox_none_inner = OxRef::from_ref(&none_inner);
    assert_eq!(format!("{ox_none_inner:?}"), "Some(None)");

    let none_outer: Option<Option<i32>> = None;
    let ox_none_outer = OxRef::from_ref(&none_outer);
    assert_eq!(format!("{ox_none_outer:?}"), "None");
}

/// Test that Shape hash is consistent with equality.
/// This is a regression test for https://github.com/facet-rs/facet/issues/1574
#[test]
fn test_shape_hash_consistent_with_eq() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Get Shape from multiple places - in the same crate they're definitely
    // the same pointer, but the hash/eq must be consistent for HashSet to work.
    let shape1 = <u8 as Facet>::SHAPE;
    let shape2 = <u8 as Facet>::SHAPE;

    // They should be equal
    assert_eq!(shape1, shape2, "Same type's shapes should be equal");

    // If a == b, then hash(a) == hash(b) (Hash trait requirement)
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    shape1.hash(&mut h1);
    shape2.hash(&mut h2);
    assert_eq!(
        h1.finish(),
        h2.finish(),
        "Equal shapes must have equal hashes"
    );
}

/// Test that ConstTypeId hash is consistent with equality.
#[test]
fn test_const_type_id_hash_consistent_with_eq() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use facet_core::ConstTypeId;

    let id1 = ConstTypeId::of::<u8>();
    let id2 = ConstTypeId::of::<u8>();

    // They should be equal
    assert_eq!(id1, id2, "Same type's ConstTypeIds should be equal");

    // If a == b, then hash(a) == hash(b) (Hash trait requirement)
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    id1.hash(&mut h1);
    id2.hash(&mut h2);
    assert_eq!(
        h1.finish(),
        h2.finish(),
        "Equal ConstTypeIds must have equal hashes"
    );
}

/// Test that Infallible implements Facet correctly.
/// Infallible is a zero-sized type that cannot be instantiated,
/// but should still have Facet implementation for use in Result<T, Infallible>.
#[test]
fn test_infallible_has_facet() {
    use std::convert::Infallible;

    let shape = <Infallible as Facet>::SHAPE;

    // Infallible should use Indirect vtable (like other zero-sized types)
    match shape.vtable {
        VTableErased::Indirect(vt) => {
            assert!(vt.debug.is_some(), "Infallible should have debug");
            assert!(vt.display.is_none(), "Infallible should NOT have display");
            assert!(vt.partial_eq.is_some(), "Infallible should have partial_eq");
            assert!(
                vt.partial_cmp.is_some(),
                "Infallible should have partial_cmp"
            );
            assert!(vt.cmp.is_some(), "Infallible should have cmp");
            assert!(vt.hash.is_some(), "Infallible should have hash");
        }
        VTableErased::Direct(_) => {
            panic!("Infallible should use Indirect vtable, not Direct");
        }
    }

    // Infallible should have type_ops with drop
    assert!(shape.type_ops.is_some(), "Infallible should have type_ops");

    // Verify the type identifier
    assert_eq!(
        shape.type_identifier, "Infallible",
        "Type identifier should be 'Infallible'"
    );
}

/// Test that Result<T, Infallible> can use Facet reflection.
/// This is the primary use case for Infallible implementing Facet.
#[test]
fn test_result_with_infallible() {
    use std::convert::Infallible;

    // This is a common pattern for infallible operations
    let _shape = <Result<i32, Infallible> as Facet>::SHAPE;

    // Just verify it compiles and we can get the shape
    // The actual Ok value can be reflected
    let value: Result<i32, Infallible> = Ok(42);
    let ox = OxRef::from_ref(&value);
    let debug_str = format!("{ox:?}");
    assert_eq!(debug_str, "Ok(42)");
}
