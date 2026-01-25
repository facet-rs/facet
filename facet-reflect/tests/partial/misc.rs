// In ownership-based APIs, the last assignment to `partial` is often unused
// because the value is consumed by `.build()` - this is expected behavior
#![allow(unused_assignments)]

use facet_testhelpers::{IPanic, test};
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
fn wip_nested() -> Result<(), IPanic> {
    let mut partial: Partial<'_> = Partial::alloc::<Outer>()?;
    partial = partial.begin_field("name")?;
    partial = partial.set(String::from("Hello, world!"))?;
    partial = partial.end()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("x")?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_field("b")?;
    partial = partial.set(43)?;
    partial = partial.end()?;
    partial = partial.end()?;
    let v = partial.build()?.materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );
    Ok(())
}

#[test]
fn readme_sample() -> Result<(), IPanic> {
    use facet::Facet;

    #[derive(Debug, PartialEq, Eq, Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let mut partial: Partial<'_> = Partial::alloc::<FooBar>()?;
    partial = partial.begin_field("foo")?;
    partial = partial.set(42u64)?;
    partial = partial.end()?;
    partial = partial.begin_field("bar")?;
    partial = partial.set(String::from("Hello, World!"))?;
    partial = partial.end()?;
    let foo_bar = partial.build()?.materialize::<FooBar>()?;

    println!("{}", foo_bar.bar);
    Ok(())
}

// Enum tests

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(u8)]
enum SimpleEnum {
    A,
    B,
    C,
}

#[test]
fn wip_unit_enum() -> Result<(), IPanic> {
    // Test unit variant A
    let mut partial: Partial<'_> = Partial::alloc::<SimpleEnum>()?;
    partial = partial.select_variant_named("A")?;
    let a = partial.build()?.materialize::<SimpleEnum>()?;
    assert_eq!(a, SimpleEnum::A);

    // Test unit variant B
    let mut partial: Partial<'_> = Partial::alloc::<SimpleEnum>()?;
    partial = partial.select_variant(1)?; // B is at index 1
    let b = partial.build()?.materialize::<SimpleEnum>()?;
    assert_eq!(b, SimpleEnum::B);

    Ok(())
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
fn wip_enum_with_data() -> Result<(), IPanic> {
    // Test empty variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Empty")?;
    let empty = partial.build()?.materialize::<EnumWithData>()?;
    assert_eq!(empty, EnumWithData::Empty);

    // Test single-field tuple variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Single")?;
    partial = partial.begin_nth_field(0)?; // Access the first field
    partial = partial.set(42)?;
    partial = partial.end()?;
    let single = partial.build()?.materialize::<EnumWithData>()?;
    assert_eq!(single, EnumWithData::Single(42));

    // Test multi-field tuple variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Tuple")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(String::from("Hello"))?;
    partial = partial.end()?;
    let tuple = partial.build()?.materialize::<EnumWithData>()?;
    assert_eq!(tuple, EnumWithData::Tuple(42, String::from("Hello")));

    // Test struct variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Struct")?;
    partial = partial.begin_field("x")?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_field("y")?;
    partial = partial.set(String::from("World"))?;
    partial = partial.end()?;
    let struct_variant = partial.build()?.materialize::<EnumWithData>()?;
    assert_eq!(
        struct_variant,
        EnumWithData::Struct {
            x: 42,
            y: String::from("World")
        }
    );

    Ok(())
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
fn wip_enum_with_data_repr_c() -> Result<(), IPanic> {
    // Test empty variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprC>()?;
    partial = partial.select_variant_named("Empty")?;
    let empty = partial.build()?.materialize::<EnumWithDataReprC>()?;
    assert_eq!(empty, EnumWithDataReprC::Empty);

    // Test single-field tuple variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprC>()?;
    partial = partial.select_variant_named("Single")?;
    partial = partial.begin_nth_field(0)?; // Access the first field
    partial = partial.set(42)?;
    partial = partial.end()?;
    let single = partial.build()?.materialize::<EnumWithDataReprC>()?;
    assert_eq!(single, EnumWithDataReprC::Single(42));

    // Test multi-field tuple variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprC>()?;
    partial = partial.select_variant_named("Tuple")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(String::from("Hello"))?;
    partial = partial.end()?;
    let tuple = partial.build()?.materialize::<EnumWithDataReprC>()?;
    assert_eq!(tuple, EnumWithDataReprC::Tuple(42, String::from("Hello")));

    // Test struct variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprC>()?;
    partial = partial.select_variant_named("Struct")?;
    partial = partial.begin_field("x")?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_field("y")?;
    partial = partial.set(String::from("World"))?;
    partial = partial.end()?;
    let struct_variant = partial.build()?.materialize::<EnumWithDataReprC>()?;
    assert_eq!(
        struct_variant,
        EnumWithDataReprC::Struct {
            x: 42,
            y: String::from("World")
        }
    );

    Ok(())
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
fn wip_enum_with_data_repr_c_i16() -> Result<(), IPanic> {
    // Test empty variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial = partial.select_variant_named("Empty")?;
    let empty = partial.build()?.materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(empty, EnumWithDataReprCI16::Empty);

    // Test single-field tuple variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial = partial.select_variant_named("Single")?;
    partial = partial.begin_nth_field(0)?; // Access the first field
    partial = partial.set(42)?;
    partial = partial.end()?;
    let single = partial.build()?.materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(single, EnumWithDataReprCI16::Single(42));

    // Test multi-field tuple variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial = partial.select_variant_named("Tuple")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(String::from("Hello"))?;
    partial = partial.end()?;
    let tuple = partial.build()?.materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(
        tuple,
        EnumWithDataReprCI16::Tuple(42, String::from("Hello"))
    );

    // Test struct variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial = partial.select_variant_named("Struct")?;
    partial = partial.begin_field("x")?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_field("y")?;
    partial = partial.set(String::from("World"))?;
    partial = partial.end()?;
    let struct_variant = partial.build()?.materialize::<EnumWithDataReprCI16>()?;
    assert_eq!(
        struct_variant,
        EnumWithDataReprCI16::Struct {
            x: 42,
            y: String::from("World")
        }
    );

    Ok(())
}

#[test]
fn test_enum_reprs() -> Result<(), IPanic> {
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

    fn build<T: Facet<'static>>() -> Result<T, IPanic> {
        let mut partial: Partial<'_> = Partial::alloc::<T>()?;
        partial = partial.select_variant(0)?;
        partial = partial.begin_nth_field(0)?;
        partial = partial.set(1u8)?;
        partial = partial.end()?;
        partial = partial.begin_nth_field(1)?;
        partial = partial.set(2u16)?;
        partial = partial.end()?;
        let v = partial.build()?.materialize::<T>()?;
        Ok(v)
    }

    let v1: ReprU8 = build()?;
    assert_eq!(v1, ReprU8::A(1, 2));

    let v2: ReprCU8 = build()?;
    assert_eq!(v2, ReprCU8::A(1, 2));

    Ok(())
}

#[test]
fn wip_enum_error_cases() -> Result<(), IPanic> {
    // Test error: trying to access a field without selecting a variant
    let partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    let result = partial.begin_field("x");
    assert!(result.is_err());

    // Test error: trying to select a non-existent variant
    let partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    let result = partial.select_variant_named("NonExistent");
    assert!(result.is_err());

    // Test error: trying to access a non-existent field in a variant
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Struct")?;
    let result = partial.begin_field("non_existent");
    assert!(result.is_err());

    // Test error: trying to build without initializing all fields
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Struct")?;
    partial = partial.begin_field("x")?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    let result = partial.build();
    assert!(result.is_err());

    Ok(())
}

// We've already tested enum functionality with SimpleEnum and EnumWithData,
// so we'll skip additional representation tests

#[test]
fn wip_switch_enum_variant() -> Result<(), IPanic> {
    // Test switching variants
    let mut partial: Partial<'_> = Partial::alloc::<EnumWithData>()?;
    partial = partial.select_variant_named("Single")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.select_variant_named("Tuple")?; // Switch to another variant
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(43)?;
    partial = partial.end()?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(String::from("Changed"))?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<EnumWithData>()?;

    assert_eq!(result, EnumWithData::Tuple(43, String::from("Changed")));

    Ok(())
}

// List tests

#[test]
fn wip_empty_list() -> Result<(), IPanic> {
    // Create an empty list by setting an empty vec
    let mut partial = Partial::alloc::<Vec<i32>>()?;
    partial = partial.set(Vec::<i32>::new())?;
    let empty_list = partial.build()?.materialize::<Vec<i32>>()?;

    assert_eq!(empty_list, Vec::<i32>::new());
    assert_eq!(empty_list.len(), 0);

    Ok(())
}

#[test]
fn wip_list_push() -> Result<(), IPanic> {
    // Build a vector by pushing elements one by one
    let mut partial = Partial::alloc::<Vec<i32>>()?;
    partial = partial.init_list()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(10)?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(20)?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(30)?;
    partial = partial.end()?;
    let list = partial.build()?.materialize::<Vec<i32>>()?;

    assert_eq!(list, vec![10, 20, 30]);
    assert_eq!(list.len(), 3);

    Ok(())
}

#[test]
fn wip_list_string() -> Result<(), IPanic> {
    // Build a vector of strings
    let mut partial = Partial::alloc::<Vec<String>>()?;
    partial = partial.init_list()?;
    partial = partial.begin_list_item()?;
    partial = partial.set("hello".to_string())?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set("world".to_string())?;
    partial = partial.end()?;
    let list = partial.build()?.materialize::<Vec<String>>()?;

    assert_eq!(list, vec!["hello".to_string(), "world".to_string()]);

    Ok(())
}

#[derive(Facet, Debug, PartialEq)]
struct WithList {
    name: String,
    values: Vec<i32>,
}

#[test]
fn wip_struct_with_list() -> Result<(), IPanic> {
    // Create a struct that contains a list
    let mut partial: Partial<'_> = Partial::alloc::<WithList>()?;
    partial = partial.begin_field("name")?;
    partial = partial.set("test list".to_string())?;
    partial = partial.end()?;
    partial = partial.begin_field("values")?;
    partial = partial.init_list()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(42)?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(43)?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(44)?;
    partial = partial.end()?;
    partial = partial.end()?;
    let with_list = partial.build()?.materialize::<WithList>()?;

    assert_eq!(
        with_list,
        WithList {
            name: "test list".to_string(),
            values: vec![42, 43, 44]
        }
    );

    Ok(())
}

#[test]
fn wip_list_error_cases() -> Result<(), IPanic> {
    // Test error: trying to init_list_item on a non-list type
    let partial: Partial<'_> = Partial::alloc::<i32>()?;
    let result = partial.begin_list_item();
    assert!(result.is_err());

    // Test error: trying to init_list on non-list type
    let partial: Partial<'_> = Partial::alloc::<String>()?;
    let result = partial.init_list();
    assert!(result.is_err());

    // Test error: trying to use list API on non-list type
    let partial: Partial<'_> = Partial::alloc::<i32>()?;
    let result = partial.init_list();
    assert!(result.is_err());

    Ok(())
}

#[test]
fn wip_opaque_arc() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    #[derive(Facet)]
    pub struct Handle(#[facet(opaque)] std::sync::Arc<NotDerivingFacet>);

    #[derive(Facet)]
    pub struct Container {
        inner: Handle,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.set(Handle(std::sync::Arc::new(NotDerivingFacet(35))))?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(*result.inner.0, NotDerivingFacet(35));

    Ok(())
}

#[test]
fn wip_put_option_explicit_some() -> Result<(), IPanic> {
    // Test explicit Some
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial = partial.set(Some(42u64))?;
    let result = partial.build()?.materialize::<Option<u64>>()?;

    assert_eq!(result, Some(42));

    Ok(())
}

#[test]
fn wip_put_option_explicit_none() -> Result<(), IPanic> {
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial = partial.set(None::<u64>)?;
    let result = partial.build()?.materialize::<Option<u64>>()?;

    assert_eq!(result, None);

    Ok(())
}

#[test]
fn wip_put_option_implicit_some() -> Result<(), IPanic> {
    // Note: implicit conversion removed in new API, must use explicit Some
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial = partial.set(Some(42u64))?;
    let result = partial.build()?.materialize::<Option<u64>>()?;

    assert_eq!(result, Some(42));

    Ok(())
}

#[test]
fn wip_parse_option() -> Result<(), IPanic> {
    // parse() replaced with set() with parsed value
    let mut partial = Partial::alloc::<Option<f64>>()?;
    partial = partial.set(Some(8.13))?;
    let result = partial.build()?.materialize::<Option<f64>>()?;

    assert_eq!(result, Some(8.13));

    Ok(())
}

#[test]
#[cfg(feature = "fn-ptr")]
fn wip_fn_ptr() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Foo {
        foo: fn() -> i32,
    }

    fn f() -> i32 {
        1113
    }

    let mut partial: Partial<'_> = Partial::alloc::<Foo>()?;
    partial = partial.begin_field("foo")?;
    partial = partial.set(f as fn() -> i32)?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Foo>()?;

    assert_eq!((result.foo)(), 1113);

    let mut partial: Partial<'_> = Partial::alloc::<Foo>()?;
    partial = partial.begin_field("foo")?;
    assert!(partial.set((|| 0.0) as fn() -> f32).is_err());

    Ok(())
}

#[test]
fn gh_354_leak_1() -> Result<(), IPanic> {
    #[derive(Debug, Facet)]
    struct Foo {
        a: String,
        b: String,
    }

    fn leak1() -> Result<(), ReflectError> {
        let mut partial: Partial<'_> = Partial::alloc::<Foo>()?;
        partial = partial.begin_field("a")?;
        partial = partial.set(String::from("Hello, World!"))?;
        partial = partial.end()?;
        let _ = partial.build()?;
        Ok(())
    }
    leak1().unwrap_err();

    Ok(())
}

#[test]
fn gh_354_leak_2() -> Result<(), IPanic> {
    #[derive(Debug, Facet)]
    struct Foo {
        a: String,
        b: String,
    }

    fn leak2() -> Result<(), ReflectError> {
        let mut partial: Partial<'_> = Partial::alloc::<Foo>()?;
        partial = partial.begin_field("a")?;
        partial = partial.set(String::from("Hello, World!"))?;
        partial = partial.end()?;
        partial = partial.begin_field("a")?;
        partial = partial.set(String::from("Hello, World!"))?;
        partial = partial.end()?;
        let _ = partial.build()?;
        Ok(())
    }

    leak2().unwrap_err();

    Ok(())
}

#[test]
fn clone_into() -> Result<(), IPanic> {
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
    let shape = <Foo as Facet>::SHAPE;
    let src = PtrConst::new(&f as *const Foo);
    let dst = PtrUninit::from_maybe_uninit(&mut f3);
    unsafe {
        shape
            .call_clone_into(src, dst.assume_init())
            .expect("Foo should have clone_into");
    }
    assert_eq!(CLONES.load(Ordering::SeqCst), 2);

    Ok(())
}

#[test]
fn wip_build_tuple_through_listlike_api_exact() -> Result<(), IPanic> {
    let mut partial: Partial<'_> = Partial::alloc::<(f64,)>()?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(5.4321)?;
    partial = partial.end()?;
    let tuple = partial.build()?.materialize::<(f64,)>()?;
    assert_eq!(tuple.0, 5.4321);

    Ok(())
}

#[test]
fn wip_build_option_none_through_default() -> Result<(), IPanic> {
    let mut partial = Partial::alloc::<Option<u32>>()?;
    partial = partial.set_default()?;
    let option = partial.build()?.materialize::<Option<u32>>()?;
    assert_eq!(option, None);

    Ok(())
}

// =============================================================================
// Tests migrated from src/partial/tests.rs
// =============================================================================

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*)
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {{ let _ = $($tt)*; }};
}

#[test]
fn f64_uninit() -> Result<(), IPanic> {
    assert_snapshot!(Partial::alloc::<f64>()?.build().unwrap_err());
    Ok(())
}

// This test is no longer relevant with the ownership-based API.
// The compiler prevents calling build() twice since build() consumes self.
// #[test]
// fn partial_after_build() -> Result<(), IPanic> {
//     let mut p = Partial::alloc::<f64>()?;
//     p = p.set(3.24_f64)?;
//     let _hv = p.build()?;
//     let err = p.build().unwrap_err();
//     assert_snapshot!(err);
//     Ok(())
// }

#[test]
fn frame_count() -> Result<(), IPanic> {
    #[derive(Facet)]
    struct S {
        s: f64,
    }

    let mut p = Partial::alloc::<S>()?;
    assert_eq!(p.frame_count(), 1);
    p = p.begin_field("s")?;
    assert_eq!(p.frame_count(), 2);
    p = p.set(4.121_f64)?;
    assert_eq!(p.frame_count(), 2);
    p = p.end()?;
    assert_eq!(p.frame_count(), 1);
    let hv = p.build()?.materialize::<S>()?;
    assert_eq!(hv.s, 4.121_f64);

    Ok(())
}

#[test]
fn too_many_end() -> Result<(), IPanic> {
    let p = Partial::alloc::<u32>()?;
    let err = match p.end() {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert_snapshot!(err);

    Ok(())
}

#[test]
fn set_shape_wrong_shape() -> Result<(), IPanic> {
    let s = String::from("I am a String");

    let p = Partial::alloc::<u32>()?;
    let err = match p.set(s) {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert_snapshot!(err);

    Ok(())
}

#[test]
fn alloc_shape_unsized() -> Result<(), IPanic> {
    match Partial::alloc::<str>() {
        Ok(_) => unreachable!(),
        Err(err) => assert_snapshot!(err),
    }
    Ok(())
}

#[test]
fn f64_init() -> Result<(), IPanic> {
    let hv = Partial::alloc::<f64>()?
        .set::<f64>(6.241)?
        .build()?
        .materialize::<f64>()?;
    assert_eq!(hv, 6.241);
    Ok(())
}

#[test]
fn struct_fully_uninit() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    assert_snapshot!(Partial::alloc::<FooBar>()?.build().unwrap_err());
    Ok(())
}

#[test]
fn struct_partially_uninit() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let partial: Partial<'_> = Partial::alloc::<FooBar>()?;
    assert_snapshot!(partial.set_field("foo", 42_u64)?.build().unwrap_err());
    Ok(())
}

#[test]
fn struct_fully_init() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let hv = Partial::alloc::<FooBar>()?
        .set_field("foo", 42u64)?
        .set_field("bar", true)?
        .build()?
        .materialize::<FooBar>()?;
    assert_eq!(hv.foo, 42u64);
    assert_eq!(hv.bar, true);
    Ok(())
}

#[test]
fn set_should_drop_when_replacing() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug, Default)]
    struct DropTracker {
        uninteresting: i32,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::AcqRel);
        }
    }

    let mut p = Partial::alloc::<DropTracker>()?;
    p = p.set(DropTracker::default())?;
    p = p.set(DropTracker::default())?;
    p = p.set(DropTracker::default())?;

    assert_eq!(DROP_COUNT.load(Ordering::Acquire), 2);

    let _p = p;

    Ok(())
}

#[test]
fn struct_field_set_twice() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker {
        id: u64,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping DropTracker with id: {}", self.id);
        }
    }

    #[derive(Facet, Debug)]
    struct Container {
        tracker: DropTracker,
        value: u64,
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;

    // Set tracker field first time
    partial = partial.set_field("tracker", DropTracker { id: 1 })?;

    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

    // Set tracker field second time (should drop the previous value)
    partial = partial.set_field("tracker", DropTracker { id: 2 })?;

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "First DropTracker should have been dropped"
    );

    // Set value field
    partial = partial.set_field("value", 100u64)?;

    let container = partial.build()?.materialize::<Container>()?;

    assert_eq!(container.tracker.id, 2); // Should have the second value
    assert_eq!(container.value, 100);

    // Drop the container
    drop(container);

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        2,
        "Both DropTrackers should have been dropped"
    );
    Ok(())
}

#[test]
fn set_default() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq, Default)]
    struct Sample {
        x: u32,
        y: String,
    }

    let sample = Partial::alloc::<Sample>()?
        .set_default()?
        .build()?
        .materialize::<Sample>()?;
    assert_eq!(sample, Sample::default());
    assert_eq!(sample.x, 0);
    assert_eq!(sample.y, "");
    Ok(())
}

#[test]
fn set_default_no_default_impl() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct NoDefault {
        value: u32,
    }

    let result = Partial::alloc::<NoDefault>()?.set_default().map(|_| ());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("does not implement Default")
    );
    Ok(())
}

#[test]
fn set_default_drops_previous() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker {
        id: u64,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Default for DropTracker {
        fn default() -> Self {
            Self { id: 999 }
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    let mut partial: Partial<'_> = Partial::alloc::<DropTracker>()?;

    // Set initial value
    partial = partial.set(DropTracker { id: 1 })?;
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);

    // Set default (should drop the previous value)
    partial = partial.set_default()?;
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

    let tracker = partial.build()?.materialize::<DropTracker>()?;
    assert_eq!(tracker.id, 999); // Default value

    drop(tracker);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn drop_partially_initialized_struct() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct NoisyDrop {
        value: u64,
    }

    impl Drop for NoisyDrop {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping NoisyDrop with value: {}", self.value);
        }
    }

    #[derive(Facet, Debug)]
    struct Container {
        first: NoisyDrop,
        second: NoisyDrop,
        third: bool,
    }

    // Reset counter
    DROP_COUNT.store(0, Ordering::SeqCst);

    // Create a partially initialized struct and drop it
    {
        let mut partial: Partial<'_> = Partial::alloc::<Container>()?;

        // Initialize first field
        partial = partial.begin_field("first")?;
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

        partial = partial.set(NoisyDrop { value: 1 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After set, the value should NOT be dropped yet"
        );

        partial = partial.end()?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "Still no drops after end"
        );

        // Initialize second field
        partial = partial.begin_field("second")?;
        partial = partial.set(NoisyDrop { value: 2 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After second set, still should have no drops"
        );

        partial = partial.end()?;

        // Don't initialize third field - just drop the partial
        // This should call drop on the two NoisyDrop instances we created
    }

    let final_drops = DROP_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        final_drops, 2,
        "Expected 2 drops total for the two initialized NoisyDrop fields, but got {}",
        final_drops
    );
    Ok(())
}

#[test]
fn drop_nested_partially_initialized() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct NoisyDrop {
        id: u64,
    }

    impl Drop for NoisyDrop {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping NoisyDrop with id: {}", self.id);
        }
    }

    #[derive(Facet, Debug)]
    struct Inner {
        a: NoisyDrop,
        b: NoisyDrop,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Inner,
        extra: NoisyDrop,
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let mut partial: Partial<'_> = Partial::alloc::<Outer>()?;

        // Start initializing inner struct
        partial = partial.begin_field("inner")?;
        partial = partial.set_field("a", NoisyDrop { id: 1 })?;

        // Only initialize one field of inner, leave 'b' uninitialized
        // Don't end from inner

        // Drop without finishing initialization
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop only the one initialized NoisyDrop in the nested struct"
    );
    Ok(())
}

#[test]
fn drop_with_copy_types() -> Result<(), IPanic> {
    // Test that Copy types don't cause double-drops or other issues
    #[derive(Facet, Debug)]
    struct MixedTypes {
        copyable: u64,
        droppable: String,
        more_copy: bool,
    }

    let mut partial: Partial<'_> = Partial::alloc::<MixedTypes>()?;

    partial = partial.set_field("copyable", 42u64)?;

    partial = partial.set_field("droppable", "Hello".to_string())?;

    // Drop without initializing 'more_copy'
    drop(partial);

    // If this doesn't panic or segfault, we're good
    Ok(())
}

#[test]
fn drop_fully_uninitialized() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct NoisyDrop {
        value: u64,
    }

    impl Drop for NoisyDrop {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[derive(Facet, Debug)]
    struct Container {
        a: NoisyDrop,
        b: NoisyDrop,
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let _partial = Partial::alloc::<Container>()?;
        // Drop immediately without initializing anything
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        0,
        "No drops should occur for completely uninitialized struct"
    );
    Ok(())
}

#[test]
fn drop_after_successful_build() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct NoisyDrop {
        value: u64,
    }

    impl Drop for NoisyDrop {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    let hv = Partial::alloc::<NoisyDrop>()?
        .set(NoisyDrop { value: 42 })?
        .build()?
        .materialize::<NoisyDrop>()?;

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        0,
        "No drops yet after build"
    );

    drop(hv);

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "One drop after dropping HeapValue"
    );
    Ok(())
}

#[test]
fn empty_struct_init() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct EmptyStruct {}

    // Test that we can build an empty struct without setting any fields
    let hv = Partial::alloc::<EmptyStruct>()?
        .build()?
        .materialize::<EmptyStruct>()?;
    assert_eq!(hv, EmptyStruct {});
    Ok(())
}

#[test]
fn field_named_on_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        age: u32,
        email: String,
    }

    let person = Partial::alloc::<Person>()?
        // Use field names instead of indices
        .begin_field("email")?
        .set("john@example.com".to_string())?
        .end()?
        .begin_field("name")?
        .set("John Doe".to_string())?
        .end()?
        .begin_field("age")?
        .set(30u32)?
        .end()?
        .build()?
        .materialize::<Person>()?;
    assert_eq!(
        person,
        Person {
            name: "John Doe".to_string(),
            age: 30,
            email: "john@example.com".to_string(),
        }
    );

    // Test invalid field name
    let partial: Partial<'_> = Partial::alloc::<Person>()?;
    let err = match partial.begin_field("invalid_field") {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert_snapshot!(err);
    Ok(())
}

#[test]
fn field_named_on_enum() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Config {
        Server { host: String, port: u16, tls: bool } = 0,
        Client { url: String, timeout: u32 } = 1,
    }

    // Test field access on Server variant
    let config = Partial::alloc::<Config>()?
        .select_variant_named("Server")?
        .set_field("port", 8080u16)?
        .set_field("host", "localhost".to_string())?
        .set_field("tls", true)?
        .build()?
        .materialize::<Config>()?;
    assert_eq!(
        config,
        Config::Server {
            host: "localhost".to_string(),
            port: 8080,
            tls: true,
        }
    );

    // Test invalid field name on enum variant

    let mut partial: Partial<'_> = Partial::alloc::<Config>()?;
    partial = partial.select_variant_named("Client")?;
    let result = partial.begin_field("port"); // port doesn't exist on Client
    let err = match result {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert_snapshot!(err);
    Ok(())
}

#[test]
fn enum_unit_variant() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active = 0,
        Inactive = 1,
        Pending = 2,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Status>()?;
    partial = partial.select_variant(1)?;
    let hv = partial.build()?.materialize::<Status>()?;
    assert_eq!(hv, Status::Inactive);
    Ok(())
}

#[test]
fn enum_struct_variant() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text { content: String } = 0,
        Number { value: i32 } = 1,
        Empty = 2,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Message>()?;
    partial = partial.select_variant(0)?;
    partial = partial.set_field("content", "Hello, world!".to_string())?;
    let hv = partial.build()?.materialize::<Message>()?;
    assert_eq!(
        hv,
        Message::Text {
            content: "Hello, world!".to_string()
        }
    );
    Ok(())
}

#[test]
fn enum_tuple_variant() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(i32)]
    #[allow(dead_code)]
    enum Value {
        Int(i32) = 0,
        Float(f64) = 1,
        Pair(i32, String) = 2,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Value>()?;
    partial = partial.select_variant(2)?;
    partial = partial.set_nth_field(0, 42)?;
    partial = partial.set_nth_field(1, "test".to_string())?;
    let hv = partial.build()?.materialize::<Value>()?;
    assert_eq!(hv, Value::Pair(42, "test".to_string()));
    Ok(())
}

#[test]
fn enum_set_field_twice() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u16)]
    enum Data {
        Point { x: f32, y: f32 } = 0,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Data>()?;
    partial = partial.select_variant(0)?;
    partial = partial.set_field("x", 1.0f32)?;
    partial = partial.set_field("x", 2.0f32)?;
    partial = partial.set_field("y", 3.0f32)?;
    let hv = partial.build()?.materialize::<Data>()?;
    assert_eq!(hv, Data::Point { x: 2.0, y: 3.0 });
    Ok(())
}

#[test]
fn enum_partial_initialization_error() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Config {
        Settings { timeout: u32, retries: u8 } = 0,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Config>()?;
    partial = partial.select_variant(0)?;
    partial = partial.set_field("timeout", 5000u32)?;
    let result = partial.build();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn enum_select_nth_variant() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active = 0,
        Inactive = 1,
        Pending = 2,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Status>()?;
    partial = partial.select_nth_variant(1)?;
    let hv = partial.build()?.materialize::<Status>()?;
    assert_eq!(hv, Status::Inactive);

    let mut partial2 = Partial::alloc::<Status>()?;
    partial2 = partial2.select_nth_variant(2)?;
    let hv2 = partial2.build()?.materialize::<Status>()?;
    assert_eq!(hv2, Status::Pending);
    Ok(())
}

#[test]
fn variant_named() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Animal {
        Dog { name: String, age: u8 } = 0,
        Cat { name: String, lives: u8 } = 1,
        Bird { species: String } = 2,
    }

    let animal = Partial::alloc::<Animal>()?
        .select_variant_named("Dog")?
        .set_field("name", "Buddy".to_string())?
        .set_field("age", 5u8)?
        .build()?
        .materialize::<Animal>()?;
    assert_eq!(
        animal,
        Animal::Dog {
            name: "Buddy".to_string(),
            age: 5
        }
    );

    let animal = Partial::alloc::<Animal>()?
        .select_variant_named("Cat")?
        .set_field("name", "Whiskers".to_string())?
        .set_field("lives", 9u8)?
        .build()?
        .materialize::<Animal>()?;
    assert_eq!(
        animal,
        Animal::Cat {
            name: "Whiskers".to_string(),
            lives: 9
        }
    );

    let animal = Partial::alloc::<Animal>()?
        .select_variant_named("Bird")?
        .set_field("species", "Parrot".to_string())?
        .build()?
        .materialize::<Animal>()?;
    assert_eq!(
        animal,
        Animal::Bird {
            species: "Parrot".to_string()
        }
    );

    let partial: Partial<'_> = Partial::alloc::<Animal>()?;
    let result = partial.select_variant_named("Fish");
    let err = match result {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert!(
        err.to_string()
            .contains("No variant found with the given name")
    );
    Ok(())
}

// ============================================================================
// Tests for from_raw + finish_in_place (stack-friendly deserialization)
// ============================================================================

#[test]
fn from_raw_simple_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut slot = MaybeUninit::<Point>::uninit();
    let ptr = PtrUninit::new(slot.as_mut_ptr().cast::<u8>());

    let mut partial: Partial<'_> = unsafe { Partial::from_raw(ptr, Point::SHAPE)? };
    partial = partial.set_field("x", 10i32)?;
    partial = partial.set_field("y", 20i32)?;
    partial.finish_in_place()?;

    let point = unsafe { slot.assume_init() };
    assert_eq!(point, Point { x: 10, y: 20 });
    Ok(())
}

#[test]
fn from_raw_nested_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    let mut slot = MaybeUninit::<Outer>::uninit();
    let ptr = PtrUninit::new(slot.as_mut_ptr().cast::<u8>());

    let mut partial: Partial<'_> = unsafe { Partial::from_raw(ptr, Outer::SHAPE)? };
    partial = partial.set_field("name", "test".to_string())?;
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("value", "nested".to_string())?;
    partial = partial.end()?;
    partial.finish_in_place()?;

    let outer = unsafe { slot.assume_init() };
    assert_eq!(outer.name, "test");
    assert_eq!(outer.inner.value, "nested");
    Ok(())
}

#[test]
fn from_raw_drop_on_error() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker(String);

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[derive(Facet, Debug)]
    struct TwoFields {
        first: DropTracker,
        second: DropTracker,
    }

    DROP_COUNT.store(0, Ordering::Release);

    let mut slot = MaybeUninit::<TwoFields>::uninit();
    let ptr = PtrUninit::new(slot.as_mut_ptr().cast::<u8>());

    let mut partial: Partial<'_> = unsafe { Partial::from_raw(ptr, TwoFields::SHAPE)? };
    partial = partial.set_field("first", DropTracker("first".to_string()))?;
    // Don't set second field - this should cause finish_in_place to fail

    let result = partial.finish_in_place();
    assert!(result.is_err());

    // The first field should have been dropped during cleanup
    assert_eq!(DROP_COUNT.load(Ordering::Acquire), 1);

    Ok(())
}

#[test]
fn from_raw_drop_on_partial_drop() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker(String);

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[derive(Facet, Debug)]
    struct TwoFields {
        first: DropTracker,
        second: DropTracker,
    }

    DROP_COUNT.store(0, Ordering::Release);

    let mut slot = MaybeUninit::<TwoFields>::uninit();
    let ptr = PtrUninit::new(slot.as_mut_ptr().cast::<u8>());

    let mut partial: Partial<'_> = unsafe { Partial::from_raw(ptr, TwoFields::SHAPE)? };
    partial = partial.set_field("first", DropTracker("first".to_string()))?;
    // Drop the partial without calling finish_in_place
    drop(partial);

    // The first field should have been dropped during Partial::drop
    assert_eq!(DROP_COUNT.load(Ordering::Acquire), 1);

    Ok(())
}

#[test]
fn from_raw_with_vec() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithVec {
        items: Vec<i32>,
    }

    let mut slot = MaybeUninit::<WithVec>::uninit();
    let ptr = PtrUninit::new(slot.as_mut_ptr().cast::<u8>());

    let mut partial: Partial<'_> = unsafe { Partial::from_raw(ptr, WithVec::SHAPE)? };
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(1i32)?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(2i32)?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set(3i32)?;
    partial = partial.end()?;
    partial = partial.end()?;
    partial.finish_in_place()?;

    let with_vec = unsafe { slot.assume_init() };
    assert_eq!(with_vec.items, vec![1, 2, 3]);
    Ok(())
}
