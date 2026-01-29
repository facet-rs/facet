use bumpalo::Bump;
use std::{collections::HashMap, sync::Arc};

use facet::Facet;
use facet_reflect::{Partial, ReflectErrorKind};
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

    let bump = Bump::new(); let partial: Partial<'_, '_> = Partial::alloc::<FooBar>(&bump).unwrap();
    let result = partial.build();
    assert!(
        matches!(
            result,
            Err(ref err) if matches!(err.kind, ReflectErrorKind::UninitializedField { .. })
        ),
        "Expected UninitializedField, got {result:?}"
    );
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

    let bump = Bump::new(); let partial: Partial<'_, '_> = Partial::alloc::<FooBar>(&bump).unwrap();
    let result = partial.build();
    assert!(
        matches!(
            result,
            Err(ref err) if matches!(err.kind, ReflectErrorKind::UninitializedValue { .. })
        ),
        "Expected UninitializedValue, got {result:?}"
    );

    let bump = Bump::new(); let mut partial: Partial<'_, '_> = Partial::alloc::<FooBar>(&bump).unwrap();
    partial = partial.select_variant_named("Foo").unwrap();
    assert!(partial.build().map(|_| ()).is_ok());

    let bump = Bump::new(); let mut partial: Partial<'_, '_> = Partial::alloc::<FooBar>(&bump).unwrap();
    partial = partial.select_variant_named("Bar").unwrap();
    let result = partial.build();
    assert!(
        matches!(
            result,
            Err(ref err) if matches!(err.kind, ReflectErrorKind::UninitializedField { .. })
        ),
        "Expected UninitializedField, got {result:?}"
    );
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
    let bump = Bump::new(); let partial: Partial<'_, '_> = Partial::alloc::<[f32; 8]>(&bump).unwrap();
    let res = partial.build();
    assert!(
        matches!(res, Err(ref err) if matches!(err.kind, ReflectErrorKind::UninitializedValue { .. })),
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
    let bump = Bump::new(); let partial: Partial<'_, '_> = Partial::alloc::<T>(&bump).unwrap();
    let res = partial.build();
    assert!(
        matches!(res, Err(ref err) if matches!(err.kind, ReflectErrorKind::UninitializedValue { .. })),
        "Expected UninitializedValue error, got {res:?}"
    );
}
