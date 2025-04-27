use facet::{Facet, Type, UserType};

#[test]
fn vec_wrapper() {
    #[derive(Facet)]
    struct VecWrapper<T> {
        data: Vec<T>,
    }

    let shape = VecWrapper::<u32>::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sd)) => {
            assert_eq!(sd.fields.len(), 1);
            let field = sd.fields[0];
            let shape_name = format!("{}", field.shape());
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
            let shape_name = format!("{}", field.shape());
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
    struct TupleVecWrapper<T>(Vec<T>);

    let shape = TupleVecWrapper::<u32>::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sd)) => {
            assert_eq!(sd.fields.len(), 1);
            let field = sd.fields[0];
            let shape_name = format!("{}", field.shape());
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
    enum EnumVecWrapper<T> {
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
            let field_shape_name = format!("{}", fields[0].shape());
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
    assert_eq!(shape.type_params.len(), 3);
    let k = &shape.type_params[0];
    let v = &shape.type_params[1];
    let s = &shape.type_params[2];
    assert_eq!(k.name, "K");
    assert_eq!(v.name, "V");
    assert_eq!(s.name, "S");
    assert_eq!(k.shape(), String::SHAPE);
    assert_eq!(v.shape(), u8::SHAPE);
    assert_eq!(s.shape().to_string(), "RandomState");
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
