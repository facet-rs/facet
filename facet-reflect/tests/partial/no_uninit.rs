use std::{collections::HashMap, sync::Arc};

use facet::Facet;
use facet_reflect::{Partial, ReflectError};
use facet_testhelpers::test;

// The order of these tests mirrors the Def enum

#[test]
fn scalar_uninit() {
    test_uninit::<u32>();
}

#[test]
fn struct_uninit() {
    #[derive(Facet)]
    struct FooBar {
        foo: u32,
    }

    let partial: Partial<'_> = Partial::alloc::<FooBar>().unwrap();
    assert!(matches!(
        partial.build(),
        Err(ReflectError::UninitializedField { .. })
    ));
}

#[test]
fn enum_uninit() {
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum FooBar {
        Foo,
        Bar { x: u32 },
    }

    let partial: Partial<'_> = Partial::alloc::<FooBar>().unwrap();
    assert!(matches!(
        partial.build(),
        Err(ReflectError::UninitializedValue { .. })
    ));

    let mut partial: Partial<'_> = Partial::alloc::<FooBar>().unwrap();
    partial = partial.select_variant_named("Foo").unwrap();
    assert!(partial.build().map(|_| ()).is_ok());

    let mut partial: Partial<'_> = Partial::alloc::<FooBar>().unwrap();
    partial = partial.select_variant_named("Bar").unwrap();
    assert!(matches!(
        partial.build(),
        Err(ReflectError::UninitializedEnumField { .. })
    ));
}

#[test]
fn map_uninit() {
    test_uninit::<HashMap<String, String>>();
}

#[test]
fn list_uninit() {
    test_uninit::<Vec<u8>>();
}

#[test]
fn array_uninit() {
    let partial: Partial<'_> = Partial::alloc::<[f32; 8]>().unwrap();
    let res = partial.build();
    assert!(
        matches!(res, Err(ReflectError::UninitializedValue { .. })),
        "Expected UninitializedValue error, got {res:?}"
    );
}

#[test]
fn slice_uninit() {
    test_uninit::<&[f32]>();
}

#[test]
fn option_uninit() {
    test_uninit::<Option<u32>>();
}

#[test]
fn smart_pointer_uninit() {
    test_uninit::<Arc<u8>>();
}

fn test_uninit<T: Facet<'static>>() {
    let partial: Partial<'_> = Partial::alloc::<T>().unwrap();
    let res = partial.build().map(|_| ());
    assert!(
        matches!(res, Err(ReflectError::UninitializedValue { .. })),
        "Expected UninitializedValue error, got {res:?}"
    );
}
