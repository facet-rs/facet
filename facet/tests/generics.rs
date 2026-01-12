use facet::{Facet, Type, UserType};

#[test]
fn vec_wrapper() {
    #[derive(Facet)]
    struct VecWrapper<T: 'static> {
        data: Vec<T>,
    }

    let shape = VecWrapper::<u32>::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sd)) => {
            assert_eq!(sd.fields.len(), 1);
            let field = sd.fields[0];
            let shape_name = format!("{}", field.shape().type_name());
            assert_eq!(shape_name, "Vec<u32>");
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), u32::SHAPE);
}

#[cfg(feature = "std")]
#[test]
fn hash_map_wrapper() {
    use std::collections::HashMap;

    #[derive(Facet)]
    struct HashMapWrapper<K, V>
    where
        K: core::hash::Hash + Eq + 'static,
        V: 'static,
    {
        map: HashMap<K, V>,
    }

    let shape = HashMapWrapper::<u16, String>::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sd)) => {
            assert_eq!(sd.fields.len(), 1);
            let field = sd.fields[0];
            let shape_name = format!("{}", field.shape().type_name());
            assert_eq!(shape_name, "HashMap<u16, String>");
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    assert_eq!(shape.type_params.len(), 2);
    let k = &shape.type_params[0];
    let v = &shape.type_params[1];
    assert_eq!(k.name, "K");
    assert_eq!(v.name, "V");
    assert_eq!(k.shape(), u16::SHAPE);
    assert_eq!(v.shape(), String::SHAPE);
}

#[test]
fn tuple_struct_vec_wrapper() {
    #[derive(Facet)]
    struct TupleVecWrapper<T: 'static>(Vec<T>);

    let shape = TupleVecWrapper::<u32>::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sd)) => {
            assert_eq!(sd.fields.len(), 1);
            let field = sd.fields[0];
            let shape_name = format!("{}", field.shape().type_name());
            assert_eq!(shape_name, "Vec<u32>");
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), u32::SHAPE);
}

#[test]
fn enum_vec_variant_wrapper() {
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum EnumVecWrapper<T: 'static> {
        VecVariant(Vec<T>),
        None,
    }

    let shape = EnumVecWrapper::<u32>::SHAPE;
    match shape.ty {
        Type::User(UserType::Enum(ed)) => {
            // Should have two variants: VecVariant, None
            assert_eq!(ed.variants.len(), 2);

            let v0 = &ed.variants[0];
            assert_eq!(v0.name, "VecVariant");
            let fields = &v0.data.fields;
            assert_eq!(fields.len(), 1);
            let field_shape_name = format!("{}", fields[0].shape().type_name());
            assert_eq!(field_shape_name, "Vec<u32>");

            let v1 = &ed.variants[1];
            assert_eq!(v1.name, "None");
            assert_eq!(v1.data.fields.len(), 0);

            eprintln!("Enum shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), u32::SHAPE);
}

#[test]
fn opaque_struct() {
    #[derive(Debug)]
    struct NonFacet;

    #[derive(Facet, Debug)]
    #[facet(opaque, traits(Debug))]
    struct GenStruct<T: core::fmt::Debug>(T);

    let shape = GenStruct::<NonFacet>::SHAPE;
    match shape.ty {
        Type::User(UserType::Opaque) => {
            eprintln!("Enum shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    assert!(shape.vtable.has_debug());
    assert_eq!(shape.type_params.len(), 0);
}

#[test]
fn opaque_enum() {
    #[derive(Debug)]
    struct NonFacet;

    #[derive(Facet, Debug)]
    #[facet(opaque, traits(Debug))]
    struct GenEnum<T: core::fmt::Debug>(T);

    let shape = GenEnum::<NonFacet>::SHAPE;
    match shape.ty {
        Type::User(UserType::Opaque) => {
            eprintln!("Enum shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    assert!(shape.vtable.has_debug());
    assert_eq!(shape.type_params.len(), 0);
}

#[test]
fn type_params_vec_f64() {
    let shape = Vec::<f64>::SHAPE;
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), f64::SHAPE);
}

#[cfg(feature = "std")]
#[test]
fn type_params_hash_map_string_u8() {
    use std::collections::HashMap;
    let shape = HashMap::<String, u8>::SHAPE;
    assert_eq!(shape.type_params.len(), 2);
    let k = &shape.type_params[0];
    let v = &shape.type_params[1];
    assert_eq!(k.name, "K");
    assert_eq!(v.name, "V");
    assert_eq!(k.shape(), String::SHAPE);
    assert_eq!(v.shape(), u8::SHAPE);
}

#[test]
fn type_params_btreemap_u8_i32() {
    use std::collections::BTreeMap;
    let shape = BTreeMap::<u8, i32>::SHAPE;
    assert_eq!(shape.type_params.len(), 2);
    let k = &shape.type_params[0];
    let v = &shape.type_params[1];
    assert_eq!(k.name, "K");
    assert_eq!(v.name, "V");
    assert_eq!(k.shape(), u8::SHAPE);
    assert_eq!(v.shape(), i32::SHAPE);
}

#[test]
fn type_params_option_bool() {
    let shape = Option::<bool>::SHAPE;
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), bool::SHAPE);
}

#[test]
fn type_params_arc_string() {
    use std::sync::Arc;
    let shape = Arc::<String>::SHAPE;
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), String::SHAPE);
}

#[test]
fn type_params_weak_string() {
    use std::sync::Weak;
    let shape = Weak::<String>::SHAPE;
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), String::SHAPE);
}

#[test]
fn type_params_array_f32_12() {
    let shape = <[f32; 12]>::SHAPE;
    // Arrays have a single type parameter, usually called "T"
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), f32::SHAPE);
}

#[test]
fn type_params_slice_ref_bool() {
    let shape = <&[bool]>::SHAPE;
    // Reference has a type param for referent, named "T"
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(format!("{}", t.shape()), "[bool]");
}

#[test]
fn type_params_slice_bool() {
    let shape = <[bool]>::SHAPE;
    // Reference has a type param for referent, named "T"
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(format!("{}", t.shape()), "bool");
}

#[test]
fn type_params_nonnull_u8() {
    use std::ptr::NonNull;

    let shape = NonNull::<u8>::SHAPE;
    // NonNull has a single type parameter, usually named "T"
    assert_eq!(shape.type_params.len(), 1);
    let t = &shape.type_params[0];
    assert_eq!(t.name, "T");
    assert_eq!(t.shape(), u8::SHAPE);
}

// Tests for #[facet(bound = "...")] custom bounds feature

#[test]
fn custom_bound_single() {
    #![allow(dead_code)]

    // Test that a single custom bound works with opaque types
    #[derive(Facet)]
    #[facet(opaque)]
    #[facet(bound = "I: Clone")]
    struct StateManager<I> {
        internal: I,
    }

    // String implements Clone, so this should compile
    let shape = StateManager::<String>::SHAPE;
    match shape.ty {
        Type::User(UserType::Opaque) => {
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }
}

#[test]
fn custom_bound_multiple_attrs() {
    #![allow(dead_code)]

    // Test multiple #[facet(bound = "...")] attributes
    #[derive(Facet)]
    #[facet(opaque)]
    #[facet(bound = "T: Clone")]
    #[facet(bound = "U: core::fmt::Debug")]
    struct TwoParams<T, U> {
        t: T,
        u: U,
    }

    let shape = TwoParams::<String, u32>::SHAPE;
    match shape.ty {
        Type::User(UserType::Opaque) => {
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }
}

#[test]
fn custom_bound_with_non_opaque() {
    // Test custom bounds on non-opaque types (adds to Facet<'ʄ> bound)
    #[derive(Facet)]
    #[facet(bound = "T: Clone")]
    struct ClonableWrapper<T: 'static> {
        data: T,
    }

    let shape = ClonableWrapper::<String>::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sd)) => {
            assert_eq!(sd.fields.len(), 1);
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }
}

#[test]
fn custom_bound_enum() {
    // Test custom bounds on enums
    #[derive(Facet)]
    #[repr(u8)]
    #[facet(opaque)]
    #[facet(bound = "T: Clone")]
    #[allow(dead_code)]
    enum MaybeClone<T> {
        Some(T),
        None,
    }

    let shape = MaybeClone::<String>::SHAPE;
    match shape.ty {
        Type::User(UserType::Opaque) => {
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }
}

#[test]
fn custom_bound_with_proxy() {
    // This test demonstrates the primary use case for custom bounds:
    // An opaque container with a proxy type where the From impls require
    // additional bounds (Clone) that aren't automatically inferred.

    // A wrapper type that does NOT implement Facet
    struct NonFacetSmartPtr<T>(T);

    impl<T> NonFacetSmartPtr<T> {
        fn new(value: T) -> Self {
            Self(value)
        }

        fn read(&self) -> &T {
            &self.0
        }
    }

    // The proxy type that IS serializable - it just holds the inner value
    #[derive(Facet, Clone)]
    struct SmartPtrProxy<I>(I);

    // The main type - opaque because NonFacetSmartPtr doesn't implement Facet
    // We need custom bounds because:
    // - `I: Clone` is required by the From impls
    // - `I: Facet<'ʄ>` is required because SmartPtrProxy<I> needs it for its Facet impl
    #[derive(Facet)]
    #[facet(opaque, proxy = SmartPtrProxy<I>)]
    #[facet(bound = "I: Clone + Facet<'ʄ>")]
    struct SmartPtr<I> {
        inner: NonFacetSmartPtr<I>,
    }

    impl<I> SmartPtr<I> {
        fn new(value: I) -> Self {
            Self {
                inner: NonFacetSmartPtr::new(value),
            }
        }
    }

    // Serialize: SmartPtr -> SmartPtrProxy (needs Clone to extract value)
    impl<I: Clone> From<&SmartPtr<I>> for SmartPtrProxy<I> {
        fn from(value: &SmartPtr<I>) -> Self {
            SmartPtrProxy(value.inner.read().clone())
        }
    }

    // Deserialize: SmartPtrProxy -> SmartPtr
    impl<I> From<SmartPtrProxy<I>> for SmartPtr<I> {
        fn from(proxy: SmartPtrProxy<I>) -> Self {
            SmartPtr::new(proxy.0)
        }
    }

    // Verify the shape is correct
    let shape = SmartPtr::<String>::SHAPE;
    match shape.ty {
        Type::User(UserType::Opaque) => {
            assert!(shape.proxy.is_some(), "proxy should be set");
            eprintln!("SmartPtr shape {shape} looks correct");
        }
        _ => unreachable!(),
    }

    // Verify the proxy shape is also correct
    let proxy_shape = SmartPtrProxy::<String>::SHAPE;
    match proxy_shape.ty {
        Type::User(UserType::Struct(_)) => {
            eprintln!("SmartPtrProxy shape {proxy_shape} looks correct");
        }
        _ => unreachable!(),
    }
}
