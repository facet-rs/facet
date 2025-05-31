use facet_testhelpers::test;
use std::mem::{MaybeUninit, size_of};

use facet::{EnumType, Facet, Field, PtrConst, PtrUninit, StructType, Type, UserType, Variant};
use facet_reflect::{Partial, ReflectError};

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
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_field("name")?;
    partial.set(String::from("Hello, world!"))?;
    partial.end()?;
    partial.begin_field("inner")?;
    partial.begin_field("x")?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_field("b")?;
    partial.set(43)?;
    partial.end()?;
    partial.end()?;
    let v = *partial.build()?;

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

    let mut partial = Partial::alloc::<FooBar>()?;
    partial.begin_field("foo")?;
    partial.set(42u64)?;
    partial.end()?;
    partial.begin_field("bar")?;
    partial.set(String::from("Hello, World!"))?;
    partial.end()?;
    let foo_bar = *partial.build()?;

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
    let mut partial = Partial::alloc::<SimpleEnum>()?;
    partial.select_variant_named("A")?;
    let a = *partial.build()?;
    assert_eq!(a, SimpleEnum::A);

    // Test unit variant B
    let mut partial = Partial::alloc::<SimpleEnum>()?;
    partial.select_variant(1)?; // B is at index 1
    let b = *partial.build()?;
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
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Empty")?;
    let empty = *partial.build()?;
    assert_eq!(empty, EnumWithData::Empty);

    // Test single-field tuple variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_enum_field(0)?; // Access the first field
    partial.set(42)?;
    partial.end()?;
    let single = *partial.build()?;
    assert_eq!(single, EnumWithData::Single(42));

    // Test multi-field tuple variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Tuple")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_nth_enum_field(1)?;
    partial.set(String::from("Hello"))?;
    partial.end()?;
    let tuple = *partial.build()?;
    assert_eq!(tuple, EnumWithData::Tuple(42, String::from("Hello")));

    // Test struct variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Struct")?;
    partial.begin_field("x")?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_field("y")?;
    partial.set(String::from("World"))?;
    partial.end()?;
    let struct_variant = *partial.build()?;
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
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Empty")?;
    let empty = *partial.build()?;
    assert_eq!(empty, EnumWithDataReprC::Empty);

    // Test single-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_enum_field(0)?; // Access the first field
    partial.set(42)?;
    partial.end()?;
    let single = *partial.build()?;
    assert_eq!(single, EnumWithDataReprC::Single(42));

    // Test multi-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Tuple")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_nth_enum_field(1)?;
    partial.set(String::from("Hello"))?;
    partial.end()?;
    let tuple = *partial.build()?;
    assert_eq!(tuple, EnumWithDataReprC::Tuple(42, String::from("Hello")));

    // Test struct variant
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Struct")?;
    partial.begin_field("x")?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_field("y")?;
    partial.set(String::from("World"))?;
    partial.end()?;
    let struct_variant = *partial.build()?;
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
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Empty")?;
    let empty = *partial.build()?;
    assert_eq!(empty, EnumWithDataReprCI16::Empty);

    // Test single-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_enum_field(0)?; // Access the first field
    partial.set(42)?;
    partial.end()?;
    let single = *partial.build()?;
    assert_eq!(single, EnumWithDataReprCI16::Single(42));

    // Test multi-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Tuple")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_nth_enum_field(1)?;
    partial.set(String::from("Hello"))?;
    partial.end()?;
    let tuple = *partial.build()?;
    assert_eq!(
        tuple,
        EnumWithDataReprCI16::Tuple(42, String::from("Hello"))
    );

    // Test struct variant
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Struct")?;
    partial.begin_field("x")?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_field("y")?;
    partial.set(String::from("World"))?;
    partial.end()?;
    let struct_variant = *partial.build()?;
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
        let mut partial = Partial::alloc::<T>()?;
        partial.select_variant(0)?;
        partial.begin_nth_enum_field(0)?;
        partial.set(1u8)?;
        partial.end()?;
        partial.begin_nth_enum_field(1)?;
        partial.set(2u16)?;
        partial.end()?;
        let v = *partial.build()?;
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
    let mut partial = Partial::alloc::<EnumWithData>()?;
    let result = partial.begin_field("x");
    assert!(result.is_err());

    // Test error: trying to select a non-existent variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    let result = partial.select_variant_named("NonExistent");
    assert!(result.is_err());

    // Test error: trying to access a non-existent field in a variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Struct")?;
    let result = partial.begin_field("non_existent");
    assert!(result.is_err());

    // Test error: trying to build without initializing all fields
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Struct")?;
    partial.begin_field("x")?;
    partial.set(42)?;
    partial.end()?;
    let result = partial.build();
    assert!(result.is_err());
}

// We've already tested enum functionality with SimpleEnum and EnumWithData,
// so we'll skip additional representation tests

#[test]
fn wip_switch_enum_variant() {
    // Test switching variants
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.select_variant_named("Tuple")?; // Switch to another variant
    partial.begin_nth_enum_field(0)?;
    partial.set(43)?;
    partial.end()?;
    partial.begin_nth_enum_field(1)?;
    partial.set(String::from("Changed"))?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result, EnumWithData::Tuple(43, String::from("Changed")));
}

// List tests

#[test]
fn wip_empty_list() {
    // Create an empty list by setting an empty vec
    let mut partial = Partial::alloc::<Vec<i32>>()?;
    partial.set(Vec::<i32>::new())?;
    let empty_list = *partial.build()?;

    assert_eq!(empty_list, Vec::<i32>::new());
    assert_eq!(empty_list.len(), 0);
}

#[test]
fn wip_list_push() {
    // Build a vector by pushing elements one by one
    let mut partial = Partial::alloc::<Vec<i32>>()?;
    partial.begin_list()?;
    partial.begin_list_item()?;
    partial.set(10)?;
    partial.end()?;
    partial.begin_list_item()?;
    partial.set(20)?;
    partial.end()?;
    partial.begin_list_item()?;
    partial.set(30)?;
    partial.end()?;
    let list = *partial.build()?;

    assert_eq!(list, vec![10, 20, 30]);
    assert_eq!(list.len(), 3);
}

#[test]
fn wip_list_string() {
    // Build a vector of strings
    let mut partial = Partial::alloc::<Vec<String>>()?;
    partial.begin_list()?;
    partial.begin_list_item()?;
    partial.set("hello".to_string())?;
    partial.end()?;
    partial.begin_list_item()?;
    partial.set("world".to_string())?;
    partial.end()?;
    let list = *partial.build()?;

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
    let mut partial = Partial::alloc::<WithList>()?;
    partial.begin_field("name")?;
    partial.set("test list".to_string())?;
    partial.end()?;
    partial.begin_field("values")?;
    partial.begin_list()?;
    partial.begin_list_item()?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_list_item()?;
    partial.set(43)?;
    partial.end()?;
    partial.begin_list_item()?;
    partial.set(44)?;
    partial.end()?;
    partial.end()?;
    let with_list = *partial.build()?;

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
    // Test error: trying to begin_list_item on a non-list type
    let mut partial = Partial::alloc::<i32>()?;
    let result = partial.begin_list_item();
    assert!(result.is_err());

    // Test error: trying to begin_list on non-list type
    let mut partial = Partial::alloc::<String>()?;
    let result = partial.begin_list();
    assert!(result.is_err());

    // Test error: trying to use list API on non-list type
    let mut partial = Partial::alloc::<i32>()?;
    let result = partial.begin_list();
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

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.set(Handle(std::sync::Arc::new(NotDerivingFacet(35))))?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(*result.inner.0, NotDerivingFacet(35));
}

#[test]
fn wip_put_option_explicit_some() {
    // Test explicit Some
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial.set(Some(42u64))?;
    let result = *partial.build()?;

    assert_eq!(result, Some(42));
}

#[test]
fn wip_put_option_explicit_none() {
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial.set(None::<u64>)?;
    let result = *partial.build()?;

    assert_eq!(result, None);
}

#[test]
fn wip_put_option_implicit_some() {
    // Note: implicit conversion removed in new API, must use explicit Some
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial.set(Some(42u64))?;
    let result = *partial.build()?;

    assert_eq!(result, Some(42));
}

#[test]
fn wip_parse_option() {
    // parse() replaced with set() with parsed value
    let mut partial = Partial::alloc::<Option<f64>>()?;
    partial.set(Some(8.13))?;
    let result = *partial.build()?;

    assert_eq!(result, Some(8.13));
}

#[test]
#[cfg(feature = "fn-ptr")]
fn wip_fn_ptr() {
    #[derive(Facet, Debug, PartialEq, Eq)]
    struct Foo {
        foo: fn() -> i32,
    }

    fn f() -> i32 {
        1113
    }

    let mut partial = Partial::alloc::<Foo>()?;
    partial.begin_field("foo")?;
    partial.set(f as fn() -> i32)?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!((result.foo)(), 1113);

    let mut partial = Partial::alloc::<Foo>()?;
    partial.begin_field("foo")?;
    assert!(partial.set((|| 0.0) as fn() -> f32).is_err());
}

#[test]
fn gh_354_leak_1() {
    #[derive(Debug, Facet)]
    struct Foo {
        a: String,
        b: String,
    }

    fn leak1() -> Result<(), ReflectError<'static>> {
        let mut partial = Partial::alloc::<Foo>()?;
        partial.begin_field("a")?;
        partial.set(String::from("Hello, World!"))?;
        partial.end()?;
        let _ = partial.build()?;
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

    fn leak2() -> Result<(), ReflectError<'static>> {
        let mut partial = Partial::alloc::<Foo>()?;
        partial.begin_field("a")?;
        partial.set(String::from("Hello, World!"))?;
        partial.end()?;
        partial.begin_field("a")?;
        partial.set(String::from("Hello, World!"))?;
        partial.end()?;
        let _ = partial.build()?;
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
    let clone_into = (<Foo as Facet>::SHAPE.vtable.clone_into)().unwrap();
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
    let clone_into = (<Type as Facet>::SHAPE.vtable.clone_into)().unwrap();
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
    let clone_into = (<Type as Facet>::SHAPE.vtable.clone_into)().unwrap();
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
    let clone_into = (<Type as Facet>::SHAPE.vtable.clone_into)().unwrap();
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
    let mut partial = Partial::alloc::<(f64,)>()?;
    partial.begin_nth_field(0)?;
    partial.set(5.4321)?;
    partial.end()?;
    let tuple = *partial.build()?;
    assert_eq!(tuple.0, 5.4321);
}

#[test]
fn wip_build_option_none_through_default() {
    let mut partial = Partial::alloc::<Option<u32>>()?;
    partial.set_default()?;
    let option = *partial.build()?;
    assert_eq!(option, None);
}
