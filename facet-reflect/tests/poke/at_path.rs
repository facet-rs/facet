use core::num::NonZeroU32;

use facet::Facet;
use facet_path::{Path, PathAccessError, PathStep};
use facet_reflect::Poke;
use facet_testhelpers::test;

// ── Test types ──────────────────────────────────────────────────────

#[derive(Facet, Debug, PartialEq)]
struct Inner {
    value: i32,
}

#[derive(Facet, Debug)]
struct Outer {
    name: String,
    inner: Inner,
    items: Vec<i32>,
}

#[derive(Facet, Debug)]
struct WithMap {
    map: std::collections::HashMap<String, i32>,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum MyEnum {
    Unit,
    Tuple(i32),
    Struct { x: i32, y: String },
}

// ── Success: struct field mutation ───────────────────────────────────

#[test]
fn at_path_mut_struct_field() {
    let mut val = Outer {
        name: "hello".into(),
        inner: Inner { value: 42 },
        items: vec![],
    };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(0)); // name

    let mut field_poke = poke.at_path_mut(&path).unwrap();
    field_poke.set("world".to_string()).unwrap();

    assert_eq!(val.name, "world");
}

#[test]
fn at_path_mut_nested_struct_field() {
    let mut val = Outer {
        name: "".into(),
        inner: Inner { value: 1 },
        items: vec![],
    };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(1)); // inner
    path.push(PathStep::Field(0)); // value

    let mut field_poke = poke.at_path_mut(&path).unwrap();
    field_poke.set(999i32).unwrap();

    assert_eq!(val.inner.value, 999);
}

// ── Success: list element mutation ──────────────────────────────────

#[test]
fn at_path_mut_list_index() {
    let mut val = Outer {
        name: "".into(),
        inner: Inner { value: 0 },
        items: vec![10, 20, 30],
    };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(2)); // items
    path.push(PathStep::Index(1)); // [1]

    let mut elem_poke = poke.at_path_mut(&path).unwrap();
    elem_poke.set(99i32).unwrap();

    assert_eq!(val.items, vec![10, 99, 30]);
}

// ── Success: enum variant field mutation ─────────────────────────────

#[test]
fn at_path_mut_enum_tuple_variant() {
    let mut val = MyEnum::Tuple(5);
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<MyEnum as Facet>::SHAPE);
    path.push(PathStep::Variant(1)); // Tuple
    path.push(PathStep::Field(0)); // the i32

    let mut field_poke = poke.at_path_mut(&path).unwrap();
    field_poke.set(42i32).unwrap();

    assert_eq!(val, MyEnum::Tuple(42));
}

#[test]
fn at_path_mut_enum_struct_variant() {
    let mut val = MyEnum::Struct {
        x: 1,
        y: "old".into(),
    };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<MyEnum as Facet>::SHAPE);
    path.push(PathStep::Variant(2)); // Struct
    path.push(PathStep::Field(1)); // y

    let mut field_poke = poke.at_path_mut(&path).unwrap();
    field_poke.set("new".to_string()).unwrap();

    assert_eq!(
        val,
        MyEnum::Struct {
            x: 1,
            y: "new".into()
        }
    );
}

// ── Success: empty path returns root ────────────────────────────────

#[test]
fn at_path_mut_empty() {
    let mut val = 42i32;
    let poke = Poke::new(&mut val);
    let path = Path::new(<i32 as Facet>::SHAPE);

    let mut result = poke.at_path_mut(&path).unwrap();
    result.set(100i32).unwrap();
    assert_eq!(val, 100);
}

// ── Error: root shape mismatch ──────────────────────────────────────

#[test]
fn at_path_mut_root_mismatch() {
    let mut val = 42i32;
    let poke = Poke::new(&mut val);

    let path = Path::new(<String as Facet>::SHAPE);
    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(err, PathAccessError::RootShapeMismatch { .. }));
}

// ── Error: field out of bounds ──────────────────────────────────────

#[test]
fn at_path_mut_field_oob() {
    let mut val = Inner { value: 1 };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Inner as Facet>::SHAPE);
    path.push(PathStep::Field(99));

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::IndexOutOfBounds {
            index: 99,
            bound: 1,
            ..
        }
    ));
}

// ── Error: list index out of bounds ─────────────────────────────────

#[test]
fn at_path_mut_list_index_oob() {
    let mut val = Outer {
        name: "".into(),
        inner: Inner { value: 0 },
        items: vec![1],
    };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(2)); // items
    path.push(PathStep::Index(5)); // oob

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::IndexOutOfBounds {
            index: 5,
            step_index: 1,
            ..
        }
    ));
}

// ── Error: variant mismatch ─────────────────────────────────────────

#[test]
fn at_path_mut_variant_mismatch() {
    let mut val = MyEnum::Unit;
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<MyEnum as Facet>::SHAPE);
    path.push(PathStep::Variant(1)); // Tuple, but value is Unit

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::VariantMismatch {
            expected_variant: 1,
            actual_variant: 0,
            ..
        }
    ));
}

// ── Error: unsupported step kinds ───────────────────────────────────

#[test]
fn at_path_mut_option_some() {
    let mut val = Some(42i32);
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Option<i32> as Facet>::SHAPE);
    path.push(PathStep::OptionSome);

    let mut inner_poke = poke.at_path_mut(&path).unwrap();
    inner_poke.set(99i32).unwrap();

    assert_eq!(val, Some(99));
}

#[test]
fn at_path_mut_option_none() {
    let mut val: Option<i32> = None;
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Option<i32> as Facet>::SHAPE);
    path.push(PathStep::OptionSome);

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(err, PathAccessError::OptionIsNone { .. }));
}

#[test]
fn at_path_mut_deref_unsupported() {
    let mut val = Box::new(42i32);
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<Box<i32> as Facet>::SHAPE);
    path.push(PathStep::Deref);

    let mut inner_poke = poke.at_path_mut(&path).unwrap();
    inner_poke.set(99i32).unwrap();
    assert_eq!(*val, 99);
}

#[test]
fn at_path_mut_map_value() {
    let mut map = std::collections::HashMap::new();
    map.insert("a".to_string(), 10);
    let mut val = WithMap { map };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<WithMap as Facet>::SHAPE);
    path.push(PathStep::Field(0));
    path.push(PathStep::MapValue(0));

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::MissingTarget { step_index: 1, .. }
    ));
}

#[test]
fn at_path_mut_map_key() {
    let mut map = std::collections::HashMap::new();
    map.insert("a".to_string(), 10);
    let mut val = WithMap { map };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<WithMap as Facet>::SHAPE);
    path.push(PathStep::Field(0));
    path.push(PathStep::MapKey(0));

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::MissingTarget { step_index: 1, .. }
    ));
}

#[test]
fn at_path_mut_map_entry_out_of_bounds() {
    let mut map = std::collections::HashMap::new();
    map.insert("a".to_string(), 10);
    let mut val = WithMap { map };
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<WithMap as Facet>::SHAPE);
    path.push(PathStep::Field(0));
    path.push(PathStep::MapValue(8));

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::MissingTarget { step_index: 1, .. }
    ));
}

#[test]
fn at_path_mut_deref_shared_reference_missing_target() {
    let inner = 42i32;
    let mut val = &inner;
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<&i32 as Facet>::SHAPE);
    path.push(PathStep::Deref);

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::MissingTarget { step_index: 0, .. }
    ));
}

#[test]
fn at_path_mut_inner_supported_for_nonzero() {
    let mut val = NonZeroU32::new(7).unwrap();
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<NonZeroU32 as Facet>::SHAPE);
    path.push(PathStep::Inner);

    let mut inner_poke = poke.at_path_mut(&path).unwrap();
    inner_poke.set(11u32).unwrap();
    assert_eq!(val.get(), 11);
}

#[test]
fn at_path_mut_proxy_missing_target() {
    let mut val = 42i32;
    let poke = Poke::new(&mut val);

    let mut path = Path::new(<i32 as Facet>::SHAPE);
    path.push(PathStep::Proxy);

    let err = poke.at_path_mut(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::MissingTarget { step_index: 0, .. }
    ));
}
