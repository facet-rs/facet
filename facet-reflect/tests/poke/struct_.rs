use facet::{Facet, OpaqueConst, OpaqueUninit};
use facet_reflect::PokeUninit;

use std::fmt::Debug;

#[derive(Debug, PartialEq, Eq, Facet)]
struct FooBar {
    foo: u64,
    bar: String,
}

impl Default for FooBar {
    fn default() -> Self {
        FooBar {
            foo: 69,
            bar: String::new(),
        }
    }
}

#[test]
fn build_foobar_through_reflection() {
    facet_testhelpers::setup();

    let (poke, guard) = PokeUninit::alloc::<FooBar>();
    let mut poke = poke.into_struct();
    unsafe {
        poke.unchecked_set_by_name("foo", OpaqueConst::new(&42u64))
            .unwrap();
    }

    {
        let bar = String::from("Hello, World!");
        unsafe {
            poke.unchecked_set_by_name("bar", OpaqueConst::new(&bar))
                .unwrap();
        }
        // bar has been moved out of
        core::mem::forget(bar);
    }

    let foo_bar = poke.build::<FooBar>(Some(guard));

    // Verify the fields were set correctly
    assert_eq!(foo_bar.foo, 42);
    assert_eq!(foo_bar.bar, "Hello, World!");

    assert_eq!(
        FooBar {
            foo: 42,
            bar: "Hello, World!".to_string()
        },
        foo_bar
    )
}

#[test]
fn set_by_name_type_mismatch() {
    facet_testhelpers::setup();

    let (poke, _guard) = PokeUninit::alloc::<FooBar>();
    let mut poke = poke.into_struct();
    assert!(matches!(
        poke.set_by_name("foo", 42u16),
        Err(facet_core::FieldError::TypeMismatch)
    ));
}

#[test]
#[should_panic(expected = "Field 'bar' was not initialized")]
fn build_foobar_incomplete() {
    facet_testhelpers::setup();

    let (poke, guard) = PokeUninit::alloc::<FooBar>();
    let mut poke = poke.into_struct();
    unsafe {
        poke.unchecked_set_by_name("foo", OpaqueConst::new(&42u64))
            .unwrap();
    }

    let foo_bar = poke.build::<FooBar>(Some(guard));

    // Verify the fields were set correctly
    assert_eq!(foo_bar.foo, 42);
    assert_eq!(foo_bar.bar, "Hello, World!");

    assert_eq!(
        FooBar {
            foo: 42,
            bar: "Hello, World!".to_string()
        },
        foo_bar
    )
}

#[test]
fn build_foobar_after_default() {
    facet_testhelpers::setup();

    let mut foo_bar: FooBar = Default::default();

    let mut poke = unsafe {
        PokeUninit::unchecked_new(OpaqueUninit::new(&mut foo_bar as *mut _), FooBar::SHAPE)
    }
    .into_struct();
    unsafe {
        poke.mark_initialized(0);
        poke.mark_initialized(1);
    }

    {
        let bar = String::from("Hello, World!");
        unsafe {
            poke.unchecked_set_by_name("bar", OpaqueConst::new(&bar))
                .unwrap();
        }
        // bar has been moved out of
        core::mem::forget(bar);
    }
    poke.build_in_place();

    // Verify the fields were set correctly
    assert_eq!(foo_bar.foo, 69);
    assert_eq!(foo_bar.bar, "Hello, World!");
}

#[test]
fn build_u64_properly() {
    facet_testhelpers::setup();

    let shape = u64::SHAPE;
    eprintln!("{:#?}", shape);

    let (poke, _guard) = PokeUninit::alloc::<u64>();
    let poke = poke.into_scalar();
    let data = poke.put(42u64);
    let value = unsafe { data.read::<u64>() };

    // Verify the value was set correctly
    assert_eq!(value, 42);
}
