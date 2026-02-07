use facet::Facet;
use facet_path::{Path, PathAccessError, PathStep};
use facet_reflect::Peek;
use facet_testhelpers::test;

// ── Test types ──────────────────────────────────────────────────────

#[derive(Facet)]
struct Inner {
    value: i32,
}

#[derive(Facet)]
struct Outer {
    name: String,
    inner: Inner,
    items: Vec<i32>,
}

#[derive(Facet)]
struct WithOption {
    maybe: Option<i32>,
}

#[derive(Facet)]
struct WithBox {
    boxed: Box<i32>,
}

#[derive(Facet)]
struct WithMap {
    map: std::collections::HashMap<String, i32>,
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum MyEnum {
    Unit,
    Tuple(i32),
    Struct { x: i32, y: String },
}

// ── Success: struct field ───────────────────────────────────────────

#[test]
fn at_path_struct_field() {
    let val = Outer {
        name: "hello".into(),
        inner: Inner { value: 42 },
        items: vec![1, 2, 3],
    };
    let peek = Peek::new(&val);

    // Navigate to `name` (field 0)
    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(0));

    let result = peek.at_path(&path).unwrap();
    assert_eq!(result.get::<String>().unwrap(), "hello");
}

#[test]
fn at_path_nested_struct_field() {
    let val = Outer {
        name: "hello".into(),
        inner: Inner { value: 99 },
        items: vec![],
    };
    let peek = Peek::new(&val);

    // Navigate to `inner.value` (field 1, then field 0)
    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(1)); // inner
    path.push(PathStep::Field(0)); // value

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 99);
}

// ── Success: list index ─────────────────────────────────────────────

#[test]
fn at_path_list_index() {
    let val = Outer {
        name: "".into(),
        inner: Inner { value: 0 },
        items: vec![10, 20, 30],
    };
    let peek = Peek::new(&val);

    // Navigate to `items[1]` (field 2, then index 1)
    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(2)); // items
    path.push(PathStep::Index(1)); // [1]

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 20);
}

// ── Success: enum variant + field ───────────────────────────────────

#[test]
fn at_path_enum_tuple_variant() {
    let val = MyEnum::Tuple(42);
    let peek = Peek::new(&val);

    // Navigate to Tuple(field 0): Variant(1), Field(0)
    let mut path = Path::new(<MyEnum as Facet>::SHAPE);
    path.push(PathStep::Variant(1)); // Tuple
    path.push(PathStep::Field(0)); // the i32

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 42);
}

#[test]
fn at_path_enum_struct_variant() {
    let val = MyEnum::Struct {
        x: 7,
        y: "world".into(),
    };
    let peek = Peek::new(&val);

    // Navigate to Struct.y: Variant(2), Field(1)
    let mut path = Path::new(<MyEnum as Facet>::SHAPE);
    path.push(PathStep::Variant(2)); // Struct
    path.push(PathStep::Field(1)); // y

    let result = peek.at_path(&path).unwrap();
    assert_eq!(result.get::<String>().unwrap(), "world");
}

// ── Success: Option ─────────────────────────────────────────────────

#[test]
fn at_path_option_some() {
    let val = WithOption { maybe: Some(123) };
    let peek = Peek::new(&val);

    // Navigate to `maybe` then into Some
    let mut path = Path::new(<WithOption as Facet>::SHAPE);
    path.push(PathStep::Field(0)); // maybe
    path.push(PathStep::OptionSome); // into Some

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 123);
}

// ── Success: Box (pointer deref) ────────────────────────────────────

#[test]
fn at_path_box_deref() {
    let val = WithBox {
        boxed: Box::new(77),
    };
    let peek = Peek::new(&val);

    // Navigate to `boxed` then deref
    let mut path = Path::new(<WithBox as Facet>::SHAPE);
    path.push(PathStep::Field(0)); // boxed
    path.push(PathStep::Deref); // through Box

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 77);
}

// ── Success: map ────────────────────────────────────────────────────

#[test]
fn at_path_map_value() {
    let mut map = std::collections::HashMap::new();
    map.insert("a".to_string(), 1);
    let val = WithMap { map };
    let peek = Peek::new(&val);

    // Navigate to `map` then to value at entry 0
    let mut path = Path::new(<WithMap as Facet>::SHAPE);
    path.push(PathStep::Field(0)); // map
    path.push(PathStep::MapValue(0)); // first entry's value

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 1);
}

// ── Success: empty path returns root ────────────────────────────────

#[test]
fn at_path_empty() {
    let val = 42i32;
    let peek = Peek::new(&val);
    let path = Path::new(<i32 as Facet>::SHAPE);

    let result = peek.at_path(&path).unwrap();
    assert_eq!(*result.get::<i32>().unwrap(), 42);
}

// ── Error: root shape mismatch ──────────────────────────────────────

#[test]
fn at_path_root_mismatch() {
    let val = 42i32;
    let peek = Peek::new(&val);

    // Path is for String, but value is i32
    let path = Path::new(<String as Facet>::SHAPE);
    let err = peek.at_path(&path).unwrap_err();
    assert!(matches!(err, PathAccessError::RootShapeMismatch { .. }));
}

// ── Error: wrong step kind ──────────────────────────────────────────

#[test]
fn at_path_field_on_scalar() {
    let val = 42i32;
    let peek = Peek::new(&val);

    let mut path = Path::new(<i32 as Facet>::SHAPE);
    path.push(PathStep::Field(0));

    let err = peek.at_path(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::WrongStepKind { step_index: 0, .. }
    ));
}

#[test]
fn at_path_index_on_struct() {
    let val = Inner { value: 1 };
    let peek = Peek::new(&val);

    let mut path = Path::new(<Inner as Facet>::SHAPE);
    path.push(PathStep::Index(0));

    let err = peek.at_path(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::WrongStepKind { step_index: 0, .. }
    ));
}

// ── Error: field out of bounds ──────────────────────────────────────

#[test]
fn at_path_field_out_of_bounds() {
    let val = Inner { value: 1 };
    let peek = Peek::new(&val);

    let mut path = Path::new(<Inner as Facet>::SHAPE);
    path.push(PathStep::Field(99));

    let err = peek.at_path(&path).unwrap_err();
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
fn at_path_list_index_oob() {
    let val = Outer {
        name: "".into(),
        inner: Inner { value: 0 },
        items: vec![1, 2],
    };
    let peek = Peek::new(&val);

    let mut path = Path::new(<Outer as Facet>::SHAPE);
    path.push(PathStep::Field(2)); // items
    path.push(PathStep::Index(5)); // out of bounds

    let err = peek.at_path(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::IndexOutOfBounds {
            index: 5,
            bound: 2,
            step_index: 1,
            ..
        }
    ));
}

// ── Error: enum variant mismatch ────────────────────────────────────

#[test]
fn at_path_variant_mismatch() {
    let val = MyEnum::Unit;
    let peek = Peek::new(&val);

    // Path says Variant(1) (Tuple) but value is Unit (variant 0)
    let mut path = Path::new(<MyEnum as Facet>::SHAPE);
    path.push(PathStep::Variant(1));

    let err = peek.at_path(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::VariantMismatch {
            expected_variant: 1,
            actual_variant: 0,
            ..
        }
    ));
}

// ── Error: option is None ───────────────────────────────────────────

#[test]
fn at_path_option_none() {
    let val = WithOption { maybe: None };
    let peek = Peek::new(&val);

    let mut path = Path::new(<WithOption as Facet>::SHAPE);
    path.push(PathStep::Field(0)); // maybe
    path.push(PathStep::OptionSome); // but it's None!

    let err = peek.at_path(&path).unwrap_err();
    assert!(matches!(
        err,
        PathAccessError::OptionIsNone { step_index: 1, .. }
    ));
}
