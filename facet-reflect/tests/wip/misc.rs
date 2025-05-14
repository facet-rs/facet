use facet_testhelpers::test;
use std::mem::MaybeUninit;

use facet::{EnumType, Facet, Field, PtrConst, PtrUninit, StructType, Type, UserType, Variant};
use facet_reflect::{ReflectError, Wip};

#[derive(Facet, PartialEq, Eq, Debug)]
struct Outer {
    name: String,
    inner: Inner,
}

#[derive(Facet, PartialEq, Eq, Debug)]
struct Inner {
    x: i32,
    b: i32,
}

#[test]
fn wip_nested() {
    let v = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );
}

#[test]
fn wip_nested_out_of_order() {
    let v = Wip::alloc::<Outer>()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .pop()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );
}

#[test]
fn readme_sample() {
    use facet::Facet;

    #[derive(Debug, PartialEq, Eq, Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let foo_bar = Wip::alloc::<FooBar>()?
        .field_named("foo")?
        .put(42u64)?
        .pop()?
        .field_named("bar")?
        .put(String::from("Hello, World!"))?
        .pop()?
        .build()?
        .materialize::<FooBar>()?;

    println!("{}", foo_bar.bar);
}

// Enum tests

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(u8)]
enum SimpleEnum {
    A,
    B,
    #[expect(dead_code)]
    C,
}

#[test]
fn wip_unit_enum() {
    // Test unit variant A
    let a = Wip::alloc::<SimpleEnum>()?
        .variant_named("A")?
        .build()?
        .materialize::<SimpleEnum>()?;
    assert_eq!(a, SimpleEnum::A);

    // Test unit variant B
    let b = Wip::alloc::<SimpleEnum>()?
        .variant(1)? // B is at index 1
        .build()?
        .materialize::<SimpleEnum>()?;
    assert_eq!(b, SimpleEnum::B);
}

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(u8)]
enum EnumWithData {
    Empty,
    Single(i32),
    Tuple(i32, String),
    Struct { x: i32, y: String },
}

#[test]
fn wip_enum_with_data() {
    // Test empty variant
    let empty = Wip::alloc::<EnumWithData>()?
        .variant_named("Empty")?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(empty, EnumWithData::Empty);

    // Test single-field tuple variant
    let single = Wip::alloc::<EnumWithData>()?
        .variant_named("Single")?
        .field(0)? // Access the first field
        .put(42)?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(single, EnumWithData::Single(42));

    // Test multi-field tuple variant
    let tuple = Wip::alloc::<EnumWithData>()?
        .variant_named("Tuple")?
        .field(0)?
        .put(42)?
        .pop()?
        .field(1)?
        .put(String::from("Hello"))?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(tuple, EnumWithData::Tuple(42, String::from("Hello")));

    // Test struct variant
    let struct_variant = Wip::alloc::<EnumWithData>()?
        .variant_named("Struct")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("y")?
        .put(String::from("World"))?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(
        struct_variant,
        EnumWithData::Struct {
            x: 42,
            y: String::from("World")
        }
    );
}

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(C)]
enum EnumWithDataReprC {
    Empty,
    Single(i32),
    Tuple(i32, String),
    Struct { x: i32, y: String },
}

#[test]
fn wip_enum_with_data_repr_c() {
    // Test empty variant
    let empty = Wip::alloc::<EnumWithDataReprC>()?
        .variant_named("Empty")?
        .build()?
        .materialize::<EnumWithDataReprC>()?;
    assert_eq!(empty, EnumWithDataReprC::Empty);

    // Test single-field tuple variant
    let single = Wip::alloc::<EnumWithDataReprC>()?
        .variant_named("Single")?
        .field(0)? // Access the first field
        .put(42)?
        .pop()?
        .build()?
        .materialize::<EnumWithDataReprC>()?;
    assert_eq!(single, EnumWithDataReprC::Single(42));

    // Test multi-field tuple variant
    let tuple = Wip::alloc::<EnumWithDataReprC>()?
        .variant_named("Tuple")?
        .field(0)?
        .put(42)?
        .pop()?
        .field(1)?
        .put(String::from("Hello"))?
        .pop()?
        .build()?
        .materialize::<EnumWithDataReprC>()?;
    assert_eq!(tuple, EnumWithDataReprC::Tuple(42, String::from("Hello")));

    // Test struct variant
    let struct_variant = Wip::alloc::<EnumWithDataReprC>()?
        .variant_named("Struct")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("y")?
        .put(String::from("World"))?
        .pop()?
        .build()?
        .materialize::<EnumWithDataReprC>()?;
    assert_eq!(
        struct_variant,
        EnumWithDataReprC::Struct {
            x: 42,
            y: String::from("World")
        }
    );
}

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(C, i16)]
enum EnumWithDataReprCI16 {
    Empty,
    Single(i32),
    Tuple(i32, String),
    Struct { x: i32, y: String },
}

#[test]
fn wip_enum_with_data_repr_c_i16() {
    // Test empty variant
    let empty = Wip::alloc::<EnumWithDataReprCI16>()?
        .variant_named("Empty")?
        .build()?
        .materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(empty, EnumWithDataReprCI16::Empty);

    // Test single-field tuple variant
    let single = Wip::alloc::<EnumWithDataReprCI16>()?
        .variant_named("Single")?
        .field(0)? // Access the first field
        .put(42)?
        .pop()?
        .build()?
        .materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(single, EnumWithDataReprCI16::Single(42));

    // Test multi-field tuple variant
    let tuple = Wip::alloc::<EnumWithDataReprCI16>()?
        .variant_named("Tuple")?
        .field(0)?
        .put(42)?
        .pop()?
        .field(1)?
        .put(String::from("Hello"))?
        .pop()?
        .build()?
        .materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(
        tuple,
        EnumWithDataReprCI16::Tuple(42, String::from("Hello"))
    );

    // Test struct variant
    let struct_variant = Wip::alloc::<EnumWithDataReprCI16>()?
        .variant_named("Struct")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("y")?
        .put(String::from("World"))?
        .pop()?
        .build()?
        .materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(
        struct_variant,
        EnumWithDataReprCI16::Struct {
            x: 42,
            y: String::from("World")
        }
    );
}

#[test]
fn test_enum_reprs() {
    const fn field_offsets<T: Facet<'static>>() -> [usize; 2] {
        match T::SHAPE.ty {
            Type::User(UserType::Enum(EnumType {
                variants:
                    &[
                        Variant {
                            data:
                                StructType {
                                    fields:
                                        &[
                                            Field {
                                                offset: offset1, ..
                                            },
                                            Field {
                                                offset: offset2, ..
                                            },
                                        ],
                                    ..
                                },
                            ..
                        },
                    ],
                ..
            })) => [offset1, offset2],
            _ => unreachable!(),
        }
    }

    // Layout, 4 bytes: [d] [0] [1] [1]
    // d: discriminant
    // 0: u8 field
    // 1: u16 field
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum ReprU8 {
        A(u8, u16),
    }
    assert_eq!(size_of::<ReprU8>(), 4);
    assert_eq!(field_offsets::<ReprU8>(), [1, 2]);

    // Layout, 6 bytes: [d] [p] [0] [p] [1] [1]
    // d: discriminant
    // p: padding bytes
    // 0: u8 field
    // 1: u16 field
    #[derive(Debug, PartialEq, Facet)]
    #[repr(C, u8)]
    enum ReprCU8 {
        A(u8, u16),
    }
    assert_eq!(size_of::<ReprCU8>(), 6);
    assert_eq!(field_offsets::<ReprCU8>(), [2, 4]);

    fn build<T: Facet<'static>>() -> eyre::Result<T> {
        let v = Wip::alloc::<T>()?
            .variant(0)?
            .field(0)?
            .put(1u8)?
            .pop()?
            .field(1)?
            .put(2u16)?
            .pop()?
            .build()?
            .materialize()?;
        Ok(v)
    }

    let v1: ReprU8 = build()?;
    assert_eq!(v1, ReprU8::A(1, 2));

    let v2: ReprCU8 = build()?;
    assert_eq!(v2, ReprCU8::A(1, 2));
}

#[test]
fn wip_enum_error_cases() {
    // Test error: trying to access a field without selecting a variant
    let result = Wip::alloc::<EnumWithData>()?.field_named("x");
    assert!(result.is_err());

    // Test error: trying to select a non-existent variant
    let result = Wip::alloc::<EnumWithData>()?.variant_named("NonExistent");
    assert!(result.is_err());

    // Test error: trying to access a non-existent field in a variant
    let result = Wip::alloc::<EnumWithData>()?
        .variant_named("Struct")?
        .field_named("non_existent");
    assert!(result.is_err());

    // Test error: trying to build without initializing all fields
    let result = Wip::alloc::<EnumWithData>()?
        .variant_named("Struct")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .build();
    assert!(result.is_err());
}

// We've already tested enum functionality with SimpleEnum and EnumWithData,
// so we'll skip additional representation tests

#[test]
fn wip_switch_enum_variant() {
    // Test switching variants
    let result = Wip::alloc::<EnumWithData>()?
        .variant_named("Single")?
        .field(0)?
        .put(42)?
        .pop()?
        .variant_named("Tuple")? // Switch to another variant
        .field(0)?
        .put(43)?
        .pop()?
        .field(1)?
        .put(String::from("Changed"))?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;

    assert_eq!(result, EnumWithData::Tuple(43, String::from("Changed")));
}

// List tests

#[test]
fn wip_empty_list() {
    // Create an empty list with put_empty_list
    let empty_list = Wip::alloc::<Vec<i32>>()?
        .put_empty_list()?
        .build()?
        .materialize::<Vec<i32>>()?;

    assert_eq!(empty_list, Vec::<i32>::new());
    assert_eq!(empty_list.len(), 0);
}

#[test]
fn wip_list_push() {
    // Build a vector by pushing elements one by one
    let list = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?
        .push()?
        .put(30)?
        .pop()?
        .build()?
        .materialize::<Vec<i32>>()?;

    assert_eq!(list, vec![10, 20, 30]);
    assert_eq!(list.len(), 3);
}

#[test]
fn wip_list_string() {
    // Build a vector of strings
    let list = Wip::alloc::<Vec<String>>()?
        .begin_pushback()?
        .push()?
        .put("hello".to_string())?
        .pop()?
        .push()?
        .put("world".to_string())?
        .pop()?
        .build()?
        .materialize::<Vec<String>>()?;

    assert_eq!(list, vec!["hello".to_string(), "world".to_string()]);
}

#[derive(Facet, Debug, PartialEq)]
struct WithList {
    name: String,
    values: Vec<i32>,
}

#[test]
fn wip_struct_with_list() {
    // Create a struct that contains a list
    let with_list = Wip::alloc::<WithList>()?
        .field_named("name")?
        .put("test list".to_string())?
        .pop()?
        .field_named("values")?
        .begin_pushback()?
        .push()?
        .put(42)?
        .pop()?
        .push()?
        .put(43)?
        .pop()?
        .push()?
        .put(44)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<WithList>()?;

    assert_eq!(
        with_list,
        WithList {
            name: "test list".to_string(),
            values: vec![42, 43, 44]
        }
    );
}

#[test]
fn wip_list_error_cases() {
    // Test error: trying to push to a non-list type
    let result = Wip::alloc::<i32>()?.push();
    assert!(result.is_err());

    // Test error: trying to get element shape from non-list
    let result = Wip::alloc::<String>()?.element_shape();
    assert!(result.is_err());

    // Test error: trying to put_empty_list on non-list type
    let result = Wip::alloc::<i32>()?.put_empty_list();
    assert!(result.is_err());
}

#[test]
fn wip_opaque_arc() {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    #[derive(Facet)]
    pub struct Handle(#[facet(opaque)] std::sync::Arc<NotDerivingFacet>);

    #[derive(Facet)]
    pub struct Container {
        inner: Handle,
    }

    let result = Wip::alloc::<Container>()?
        .field_named("inner")?
        .put(Handle(std::sync::Arc::new(NotDerivingFacet(35))))?
        .pop()?
        .build()?
        .materialize::<Container>()?;

    assert_eq!(*result.inner.0, NotDerivingFacet(35));
}

#[test]
fn wip_put_option_explicit_some() {
    // Test switching variants
    let result = Wip::alloc::<Option<u64>>()?
        .put::<Option<u64>>(Some(42))?
        .build()?
        .materialize::<Option<u64>>()?;

    assert_eq!(result, Some(42));
}

#[test]
fn wip_put_option_explicit_none() {
    let result = Wip::alloc::<Option<u64>>()?
        .put::<Option<u64>>(None)?
        .build()?
        .materialize::<Option<u64>>()?;

    assert_eq!(result, None);
}

#[test]
fn wip_put_option_implicit_some() {
    // Test switching variants
    let result = Wip::alloc::<Option<u64>>()?
        .put::<u64>(42)?
        .build()?
        .materialize::<Option<u64>>()?;

    assert_eq!(result, Some(42));
}

#[test]
fn wip_parse_option() {
    // Test switching variants
    let result = Wip::alloc::<Option<f64>>()?
        .parse("8.13")?
        .build()?
        .materialize::<Option<f64>>()?;

    assert_eq!(result, Some(8.13));
}

#[test]
fn wip_option_explicit_some_through_push_some() {
    #[derive(Facet, Debug, PartialEq, Eq)]
    struct Foo {
        foo: u32,
    }

    // Test switching variants
    let result = Wip::alloc::<Option<Foo>>()?
        .push_some()?
        .field_named("foo")?
        .put::<u32>(42)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<Option<Foo>>()?;

    assert_eq!(result, Some(Foo { foo: 42 }));
}

#[test]
fn wip_fn_ptr() {
    #[derive(Facet, Debug, PartialEq, Eq)]
    struct Foo {
        foo: fn() -> i32,
    }

    fn f() -> i32 {
        1113
    }

    let result = Wip::alloc::<Foo>()?
        .field_named("foo")?
        .put::<fn() -> i32>(f)?
        .pop()?
        .build()?
        .materialize::<Foo>()?;

    assert_eq!((result.foo)(), 1113);

    assert!(
        Wip::alloc::<Foo>()?
            .field_named("foo")?
            .put::<fn() -> f32>(|| 0.0)
            .is_err()
    );
}

#[test]
fn wip_put_u16_into_u64() {
    // put a u16 into an u64 field (should work, coercion up)
    #[derive(Facet, Debug, PartialEq, Eq)]
    struct FooU64 {
        value: u64,
    }
    let result = Wip::alloc::<FooU64>()?
        .field_named("value")?
        .put::<u16>(12345)?
        .pop()?
        .build()?
        .materialize::<FooU64>()?;
    assert_eq!(result.value, 12345u64);
}

#[test]
fn wip_put_u64_into_u16() {
    #[derive(Facet, Debug, PartialEq, Eq)]
    struct FooU16 {
        value: u16,
    }
    // should work when value fits
    let result = Wip::alloc::<FooU16>()?
        .field_named("value")?
        .put::<u64>(54321)?
        .pop()?
        .build()?
        .materialize::<FooU16>()?;
    assert_eq!(result.value, 54321u16);

    // should fail when value does not fit in u16
    let err = Wip::alloc::<FooU16>()?
        .field_named("value")?
        .put::<u64>(70000);
    assert!(
        err.is_err(),
        "Expected error when putting too large u64 into u16"
    );

    // should also fail for negative when putting an i64 into u16
    let err = Wip::alloc::<FooU16>()?.field_named("value")?.put::<i64>(-1);
    assert!(
        err.is_err(),
        "Expected error when putting negative i64 into u16"
    );
}

#[test]
fn gh_354_leak_1() {
    #[derive(Debug, Facet)]
    struct Foo {
        a: String,
        b: String,
    }

    fn leak1() -> Result<(), ReflectError> {
        Wip::alloc::<Foo>()?
            .field_named("a")?
            .put(String::from("Hello, World!"))?
            .pop()?
            .build()?
            .materialize::<Foo>()?;
        Ok(())
    }
    leak1().unwrap_err();
}

#[test]
fn gh_354_leak_2() {
    #[derive(Debug, Facet)]
    struct Foo {
        a: String,
        b: String,
    }

    fn leak2() -> Result<(), ReflectError> {
        Wip::alloc::<Foo>()?
            .field_named("a")?
            .put(String::from("Hello, World!"))?
            .pop()?
            .field_named("a")?
            .put(String::from("Hello, World!"))?
            .pop()?
            .build()?
            .materialize::<Foo>()?;
        Ok(())
    }

    leak2().unwrap_err();
}

#[test]
fn clone_into() {
    use std::sync::atomic::{AtomicU64, Ordering};

    static CLONES: AtomicU64 = AtomicU64::new(0);

    #[derive(Facet)]
    struct Foo;

    impl Clone for Foo {
        fn clone(&self) -> Self {
            eprintln!("Foo is cloning...");
            CLONES.fetch_add(1, Ordering::SeqCst);
            eprintln!("Foo is cloned!");
            Foo
        }
    }

    let f: Foo = Foo;
    assert_eq!(CLONES.load(Ordering::SeqCst), 0);
    let _f2 = f.clone();
    assert_eq!(CLONES.load(Ordering::SeqCst), 1);

    let mut f3: MaybeUninit<Foo> = MaybeUninit::uninit();
    let clone_into = <Foo as Facet>::SHAPE.vtable.clone_into.unwrap();
    unsafe {
        clone_into(PtrConst::new(&f), PtrUninit::from_maybe_uninit(&mut f3));
    }
    assert_eq!(CLONES.load(Ordering::SeqCst), 2);
}

#[test]
fn clone_into_vec() {
    type Type = Vec<String>;
    let mut vec: Type = vec!["hello".to_owned()];
    let mut vec_clone: MaybeUninit<Type> = MaybeUninit::uninit();
    let clone_into = <Type as Facet>::SHAPE.vtable.clone_into.unwrap();
    let clone_vec = unsafe {
        clone_into(
            PtrConst::new(&vec),
            PtrUninit::from_maybe_uninit(&mut vec_clone),
        );
        vec_clone.assume_init()
    };
    vec[0].push_str(" world");
    assert_eq!(clone_vec[0], "hello");
}

#[test]
fn clone_into_hash_map() {
    use std::collections::HashMap;

    type Type = HashMap<String, i32>;
    let mut map: Type = HashMap::new();
    map.insert("key".to_owned(), 42);

    let mut map_clone: MaybeUninit<Type> = MaybeUninit::uninit();
    let clone_into = <Type as Facet>::SHAPE.vtable.clone_into.unwrap();
    let clone_map = unsafe {
        clone_into(
            PtrConst::new(&map),
            PtrUninit::from_maybe_uninit(&mut map_clone),
        );
        map_clone.assume_init()
    };

    map.insert("key".to_owned(), 99);
    map.insert("new_key".to_owned(), 100);

    assert_eq!(clone_map.get("key"), Some(&42));
    assert_eq!(clone_map.get("new_key"), None);
}

#[test]
fn clone_into_btree_map() {
    use std::collections::BTreeMap;

    type Type = BTreeMap<String, i32>;
    let mut map: Type = BTreeMap::new();
    map.insert("key".to_owned(), 42);

    let mut map_clone: MaybeUninit<Type> = MaybeUninit::uninit();
    let clone_into = <Type as Facet>::SHAPE.vtable.clone_into.unwrap();
    let clone_map = unsafe {
        clone_into(
            PtrConst::new(&map),
            PtrUninit::from_maybe_uninit(&mut map_clone),
        );
        map_clone.assume_init()
    };

    map.insert("key".to_owned(), 99);
    map.insert("new_key".to_owned(), 100);

    assert_eq!(clone_map.get("key"), Some(&42));
    assert_eq!(clone_map.get("new_key"), None);
}

#[test]
fn wip_build_tuple_through_listlike_api_exact() {
    let wip = Wip::alloc::<(f64,)>()?;
    let wip = wip.begin_pushback()?;
    let wip = wip.put(5.4321)?;
    let hv = wip.build()?;
    let tuple = hv.materialize::<(f64,)>()?;
    assert_eq!(tuple.0, 5.4321);
}

#[test]
fn wip_build_tuple_through_listlike_api_coerce() {
    let wip = Wip::alloc::<(f32,)>()?;
    let wip = wip.begin_pushback()?;
    let wip = wip.put(5.4321)?;
    let hv = wip.build()?;
    let tuple = hv.materialize::<(f32,)>()?;
    assert_eq!(tuple.0, 5.4321);
}

#[test]
fn wip_build_option_none_through_default() {
    let wip = Wip::alloc::<Option<u32>>()?;
    let wip = wip.put_default()?;
    let hv = wip.build()?;
    let option = hv.materialize::<Option<u32>>()?;
    assert_eq!(option, None);
}
