use std::{collections::HashMap, sync::Arc};

use facet::Facet;
use facet_reflect::{ReflectError, Wip};

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

    facet_testhelpers::setup();
    let wip = Wip::alloc::<FooBar>();
    assert!(matches!(
        wip.build(),
        Err(ReflectError::UninitializedField { .. })
    ),);
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

    facet_testhelpers::setup();
    let wip = Wip::alloc::<FooBar>();
    assert!(matches!(
        wip.build(),
        Err(ReflectError::NoVariantSelected { .. })
    ),);

    let wip = Wip::alloc::<FooBar>().variant_named("Foo").unwrap();
    assert!(wip.build().is_ok());

    let wip = Wip::alloc::<FooBar>().variant_named("Bar").unwrap();
    assert!(matches!(
        wip.build(),
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
    test_uninit::<[f32; 8]>();
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

fn test_uninit<'a, T: Facet<'a>>() {
    facet_testhelpers::setup();
    let wip = Wip::alloc::<T>();
    assert!(
        matches!(wip.build(), Err(ReflectError::UninitializedValue { .. })),
        "Expected UninitializedValue error"
    );
}
