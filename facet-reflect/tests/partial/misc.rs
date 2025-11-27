use facet_testhelpers::{IPanic, test};
use std::{
    mem::{MaybeUninit, size_of},
    ptr::NonNull,
    sync::atomic::AtomicU64,
};

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

    let mut partial = Partial::alloc::<FooBar>()?;
    partial.begin_field("foo")?;
    partial.set(42u64)?;
    partial.end()?;
    partial.begin_field("bar")?;
    partial.set(String::from("Hello, World!"))?;
    partial.end()?;
    let foo_bar = *partial.build()?;

    println!("{}", foo_bar.bar);
    Ok(())
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
fn wip_unit_enum() -> Result<(), IPanic> {
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
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Empty")?;
    let empty = *partial.build()?;
    assert_eq!(empty, EnumWithData::Empty);

    // Test single-field tuple variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_field(0)?; // Access the first field
    partial.set(42)?;
    partial.end()?;
    let single = *partial.build()?;
    assert_eq!(single, EnumWithData::Single(42));

    // Test multi-field tuple variant
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Tuple")?;
    partial.begin_nth_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_nth_field(1)?;
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
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Empty")?;
    let empty = *partial.build()?;
    assert_eq!(empty, EnumWithDataReprC::Empty);

    // Test single-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_field(0)?; // Access the first field
    partial.set(42)?;
    partial.end()?;
    let single = *partial.build()?;
    assert_eq!(single, EnumWithDataReprC::Single(42));

    // Test multi-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprC>()?;
    partial.select_variant_named("Tuple")?;
    partial.begin_nth_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_nth_field(1)?;
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
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Empty")?;
    let empty = *partial.build()?;
    assert_eq!(empty, EnumWithDataReprCI16::Empty);

    // Test single-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_field(0)?; // Access the first field
    partial.set(42)?;
    partial.end()?;
    let single = *partial.build()?;
    assert_eq!(single, EnumWithDataReprCI16::Single(42));

    // Test multi-field tuple variant
    let mut partial = Partial::alloc::<EnumWithDataReprCI16>()?;
    partial.select_variant_named("Tuple")?;
    partial.begin_nth_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.begin_nth_field(1)?;
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
        let mut partial = Partial::alloc::<T>()?;
        partial.select_variant(0)?;
        partial.begin_nth_field(0)?;
        partial.set(1u8)?;
        partial.end()?;
        partial.begin_nth_field(1)?;
        partial.set(2u16)?;
        partial.end()?;
        let v = *partial.build()?;
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

    Ok(())
}

// We've already tested enum functionality with SimpleEnum and EnumWithData,
// so we'll skip additional representation tests

#[test]
fn wip_switch_enum_variant() -> Result<(), IPanic> {
    // Test switching variants
    let mut partial = Partial::alloc::<EnumWithData>()?;
    partial.select_variant_named("Single")?;
    partial.begin_nth_field(0)?;
    partial.set(42)?;
    partial.end()?;
    partial.select_variant_named("Tuple")?; // Switch to another variant
    partial.begin_nth_field(0)?;
    partial.set(43)?;
    partial.end()?;
    partial.begin_nth_field(1)?;
    partial.set(String::from("Changed"))?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result, EnumWithData::Tuple(43, String::from("Changed")));

    Ok(())
}

// List tests

#[test]
fn wip_empty_list() -> Result<(), IPanic> {
    // Create an empty list by setting an empty vec
    let mut partial = Partial::alloc::<Vec<i32>>()?;
    partial.set(Vec::<i32>::new())?;
    let empty_list = *partial.build()?;

    assert_eq!(empty_list, Vec::<i32>::new());
    assert_eq!(empty_list.len(), 0);

    Ok(())
}

#[test]
fn wip_list_push() -> Result<(), IPanic> {
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

    Ok(())
}

#[test]
fn wip_list_string() -> Result<(), IPanic> {
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

    Ok(())
}

#[test]
fn wip_list_error_cases() -> Result<(), IPanic> {
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

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.set(Handle(std::sync::Arc::new(NotDerivingFacet(35))))?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(*result.inner.0, NotDerivingFacet(35));

    Ok(())
}

#[test]
fn wip_put_option_explicit_some() -> Result<(), IPanic> {
    // Test explicit Some
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial.set(Some(42u64))?;
    let result = *partial.build()?;

    assert_eq!(result, Some(42));

    Ok(())
}

#[test]
fn wip_put_option_explicit_none() -> Result<(), IPanic> {
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial.set(None::<u64>)?;
    let result = *partial.build()?;

    assert_eq!(result, None);

    Ok(())
}

#[test]
fn wip_put_option_implicit_some() -> Result<(), IPanic> {
    // Note: implicit conversion removed in new API, must use explicit Some
    let mut partial = Partial::alloc::<Option<u64>>()?;
    partial.set(Some(42u64))?;
    let result = *partial.build()?;

    assert_eq!(result, Some(42));

    Ok(())
}

#[test]
fn wip_parse_option() -> Result<(), IPanic> {
    // parse() replaced with set() with parsed value
    let mut partial = Partial::alloc::<Option<f64>>()?;
    partial.set(Some(8.13))?;
    let result = *partial.build()?;

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

    let mut partial = Partial::alloc::<Foo>()?;
    partial.begin_field("foo")?;
    partial.set(f as fn() -> i32)?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!((result.foo)(), 1113);

    let mut partial = Partial::alloc::<Foo>()?;
    partial.begin_field("foo")?;
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
        let mut partial = Partial::alloc::<Foo>()?;
        partial.begin_field("a")?;
        partial.set(String::from("Hello, World!"))?;
        partial.end()?;
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
    let clone_into = <Foo as Facet>::SHAPE
        .vtable
        .clone_into
        .expect("Foo should have clone_into");
    unsafe {
        clone_into(
            PtrConst::new(NonNull::from(&f)),
            PtrUninit::from_maybe_uninit(&mut f3),
        );
    }
    assert_eq!(CLONES.load(Ordering::SeqCst), 2);

    Ok(())
}

#[test]
fn wip_build_tuple_through_listlike_api_exact() -> Result<(), IPanic> {
    let mut partial = Partial::alloc::<(f64,)>()?;
    partial.begin_nth_field(0)?;
    partial.set(5.4321)?;
    partial.end()?;
    let tuple = *partial.build()?;
    assert_eq!(tuple.0, 5.4321);

    Ok(())
}

#[test]
fn wip_build_option_none_through_default() -> Result<(), IPanic> {
    let mut partial = Partial::alloc::<Option<u32>>()?;
    partial.set_default()?;
    let option = *partial.build()?;
    assert_eq!(option, None);

    Ok(())
}

#[test]
fn steal_from_default() -> Result<(), IPanic> {
    use std::sync::atomic::Ordering;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[derive(Facet)]
    struct AddOnDrop {
        value: u64,
    }

    impl Drop for AddOnDrop {
        fn drop(&mut self) {
            COUNTER.fetch_add(self.value, Ordering::SeqCst);
        }
    }

    #[derive(Facet)]
    struct S {
        foo: AddOnDrop,
        bar: AddOnDrop,
    }

    impl Default for S {
        fn default() -> Self {
            Self {
                foo: AddOnDrop { value: 1 },
                bar: AddOnDrop { value: 2 },
            }
        }
    }

    let mut dst = Partial::alloc::<S>()?;
    assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

    {
        let mut src = Partial::alloc::<S>()?;
        src.set_default()?;
        assert_eq!(COUNTER.load(Ordering::SeqCst), 0);

        assert!(!dst.is_field_set(0).unwrap());
        assert!(src.is_field_set(0).unwrap());
        dst.steal_nth_field(&mut src, 0)?;
        assert_eq!(COUNTER.load(Ordering::SeqCst), 0);
        assert!(dst.is_field_set(0).unwrap());
        assert!(!src.is_field_set(0).unwrap());

        assert!(!dst.is_field_set(1).unwrap());
        assert!(src.is_field_set(1).unwrap());
        dst.steal_nth_field(&mut src, 1)?;
        assert_eq!(COUNTER.load(Ordering::SeqCst), 0);
        assert!(dst.is_field_set(1).unwrap());
        assert!(!src.is_field_set(1).unwrap());
    }

    drop(dst);
    assert_eq!(COUNTER.load(Ordering::SeqCst), 3);

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
    ($($tt:tt)*) => {{}};
}

#[test]
fn f64_uninit() -> Result<(), IPanic> {
    assert_snapshot!(Partial::alloc::<f64>()?.build().unwrap_err());
    Ok(())
}

#[test]
fn partial_after_build() -> Result<(), IPanic> {
    let mut p = Partial::alloc::<f64>()?;
    p.set(3.24_f64)?;
    let _hv = p.build()?;
    let err = p.build().unwrap_err();
    assert_snapshot!(err);
    Ok(())
}

#[test]
fn frame_count() -> Result<(), IPanic> {
    #[derive(Facet)]
    struct S {
        s: f64,
    }

    let mut p = Partial::alloc::<S>()?;
    assert_eq!(p.frame_count(), 1);
    p.begin_field("s")?;
    assert_eq!(p.frame_count(), 2);
    p.set(4.121_f64)?;
    assert_eq!(p.frame_count(), 2);
    p.end()?;
    assert_eq!(p.frame_count(), 1);
    let hv = *p.build()?;
    assert_eq!(hv.s, 4.121_f64);

    Ok(())
}

#[test]
fn too_many_end() -> Result<(), IPanic> {
    let mut p = Partial::alloc::<u32>()?;
    let err = p.end().unwrap_err();
    assert_snapshot!(err);

    Ok(())
}

#[test]
fn set_shape_wrong_shape() -> Result<(), IPanic> {
    let s = String::from("I am a String");

    let mut p = Partial::alloc::<u32>()?;
    let err = p.set(s).unwrap_err();
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
    let hv = Partial::alloc::<f64>()?.set::<f64>(6.241)?.build()?;
    assert_eq!(*hv, 6.241);
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

    let mut partial = Partial::alloc::<FooBar>()?;
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
        .build()?;
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
    p.set(DropTracker::default())?;
    p.set(DropTracker::default())?;
    p.set(DropTracker::default())?;

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

    let mut partial = Partial::alloc::<Container>()?;

    // Set tracker field first time
    partial.set_field("tracker", DropTracker { id: 1 })?;

    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

    // Set tracker field second time (should drop the previous value)
    partial.set_field("tracker", DropTracker { id: 2 })?;

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "First DropTracker should have been dropped"
    );

    // Set value field
    partial.set_field("value", 100u64)?;

    let container = partial.build()?;

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

    let sample = Partial::alloc::<Sample>()?.set_default()?.build()?;
    assert_eq!(*sample, Sample::default());
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

    let mut partial = Partial::alloc::<DropTracker>()?;

    // Set initial value
    partial.set(DropTracker { id: 1 })?;
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);

    // Set default (should drop the previous value)
    partial.set_default()?;
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

    let tracker = partial.build()?;
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
        let mut partial = Partial::alloc::<Container>()?;

        // Initialize first field
        partial.begin_field("first")?;
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

        partial.set(NoisyDrop { value: 1 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After set, the value should NOT be dropped yet"
        );

        partial.end()?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "Still no drops after end"
        );

        // Initialize second field
        partial.begin_field("second")?;
        partial.set(NoisyDrop { value: 2 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After second set, still should have no drops"
        );

        partial.end()?;

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
        let mut partial = Partial::alloc::<Outer>()?;

        // Start initializing inner struct
        partial.begin_field("inner")?;
        partial.set_field("a", NoisyDrop { id: 1 })?;

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

    let mut partial = Partial::alloc::<MixedTypes>()?;

    partial.set_field("copyable", 42u64)?;

    partial.set_field("droppable", "Hello".to_string())?;

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
        .build()?;

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
    let hv = Partial::alloc::<EmptyStruct>()?.build()?;
    assert_eq!(*hv, EmptyStruct {});
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
        .build()?;
    assert_eq!(
        *person,
        Person {
            name: "John Doe".to_string(),
            age: 30,
            email: "john@example.com".to_string(),
        }
    );

    // Test invalid field name
    let mut partial = Partial::alloc::<Person>()?;
    let result = partial.begin_field("invalid_field");
    assert_snapshot!(result.unwrap_err());
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
        .build()?;
    assert_eq!(
        *config,
        Config::Server {
            host: "localhost".to_string(),
            port: 8080,
            tls: true,
        }
    );

    // Test invalid field name on enum variant

    let mut partial = Partial::alloc::<Config>()?;
    partial.select_variant_named("Client")?;
    let result = partial.begin_field("port"); // port doesn't exist on Client
    assert!(result.is_err());
    assert_snapshot!(result.unwrap_err());
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

    let hv = Partial::alloc::<Status>()?.select_variant(1)?.build()?;
    assert_eq!(*hv, Status::Inactive);
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

    let hv = Partial::alloc::<Message>()?
        .select_variant(0)?
        .set_field("content", "Hello, world!".to_string())?
        .build()?;
    assert_eq!(
        *hv,
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

    let hv = Partial::alloc::<Value>()?
        .select_variant(2)?
        .set_nth_field(0, 42)?
        .set_nth_field(1, "test".to_string())?
        .build()?;
    assert_eq!(*hv, Value::Pair(42, "test".to_string()));
    Ok(())
}

#[test]
fn enum_set_field_twice() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u16)]
    enum Data {
        Point { x: f32, y: f32 } = 0,
    }

    let hv = Partial::alloc::<Data>()?
        .select_variant(0)?
        .set_field("x", 1.0f32)?
        .set_field("x", 2.0f32)?
        .set_field("y", 3.0f32)?
        .build()?;
    assert_eq!(*hv, Data::Point { x: 2.0, y: 3.0 });
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

    let result = Partial::alloc::<Config>()?
        .select_variant(0)?
        .set_field("timeout", 5000u32)?
        .build();
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

    let hv = Partial::alloc::<Status>()?.select_nth_variant(1)?.build()?;
    assert_eq!(*hv, Status::Inactive);

    let hv2 = Partial::alloc::<Status>()?.select_nth_variant(2)?.build()?;
    assert_eq!(*hv2, Status::Pending);
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
        .build()?;
    assert_eq!(
        *animal,
        Animal::Dog {
            name: "Buddy".to_string(),
            age: 5
        }
    );

    let animal = Partial::alloc::<Animal>()?
        .select_variant_named("Cat")?
        .set_field("name", "Whiskers".to_string())?
        .set_field("lives", 9u8)?
        .build()?;
    assert_eq!(
        *animal,
        Animal::Cat {
            name: "Whiskers".to_string(),
            lives: 9
        }
    );

    let animal = Partial::alloc::<Animal>()?
        .select_variant_named("Bird")?
        .set_field("species", "Parrot".to_string())?
        .build()?;
    assert_eq!(
        *animal,
        Animal::Bird {
            species: "Parrot".to_string()
        }
    );

    let mut partial = Partial::alloc::<Animal>()?;
    let result = partial.select_variant_named("Fish");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No variant found with the given name")
    );
    Ok(())
}
