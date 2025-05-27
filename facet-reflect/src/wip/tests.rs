use facet::Facet;
use facet_testhelpers::test;

use super::Wip;

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*);
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        /* no-op under miri */
    };
}

#[test]
fn f64_uninit() {
    let wip = Wip::alloc::<f64>()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn f64_init() {
    let mut wip = Wip::alloc::<f64>()?;
    wip.put::<f64>(6.241)?;
    let hv = wip.build()?;
    assert_eq!(*hv, 6.241);
}

#[test]
fn option_uninit() {
    let wip = Wip::alloc::<Option<f64>>()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn option_init() {
    let mut wip = Wip::alloc::<Option<f64>>()?;
    wip.put::<Option<f64>>(Some(6.241))?;
    let hv = wip.build()?;
    assert_eq!(*hv, Some(6.241));
}

#[test]
fn struct_fully_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let wip = Wip::alloc::<FooBar>()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn struct_partially_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let mut wip = Wip::alloc::<FooBar>()?;
    wip.push_field("foo")?;
    wip.put::<u64>(42)?;
    wip.pop()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn struct_fully_init() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let mut wip = Wip::alloc::<FooBar>()?;
    wip.push_field("foo")?;
    wip.put::<u64>(42)?;
    wip.pop()?;
    wip.push_field("bar")?;
    wip.put::<bool>(true)?;
    wip.pop()?;
    let hv = wip.build()?;
    assert_eq!(hv.foo, 42u64);
    assert_eq!(hv.bar, true);
}

#[test]
fn drop_partially_initialized_struct() {
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
        let mut wip = Wip::alloc::<Container>()?;

        // Initialize first field
        wip.push_field("first")?;
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

        wip.put(NoisyDrop { value: 1 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After put, the value should NOT be dropped yet"
        );

        wip.pop()?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "Still no drops after pop"
        );

        // Initialize second field
        wip.push_field("second")?;
        wip.put(NoisyDrop { value: 2 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After second put, still should have no drops"
        );

        wip.pop()?;

        // Don't initialize third field - just drop the wip
        // This should call drop on the two NoisyDrop instances we created
    }

    let final_drops = DROP_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        final_drops, 2,
        "Expected 2 drops total for the two initialized NoisyDrop fields, but got {}",
        final_drops
    );
}

#[test]
fn drop_nested_partially_initialized() {
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
        let mut wip = Wip::alloc::<Outer>()?;

        // Start initializing inner struct
        wip.push_field("inner")?;
        wip.push_field("a")?;
        wip.put(NoisyDrop { id: 1 })?;
        wip.pop()?;

        // Only initialize one field of inner, leave 'b' uninitialized
        // Don't pop from inner

        // Drop without finishing initialization
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop only the one initialized NoisyDrop in the nested struct"
    );
}

#[test]
fn drop_with_copy_types() {
    // Test that Copy types don't cause double-drops or other issues
    #[derive(Facet, Debug)]
    struct MixedTypes {
        copyable: u64,
        droppable: String,
        more_copy: bool,
    }

    let mut wip = Wip::alloc::<MixedTypes>()?;

    wip.push_field("copyable")?;
    wip.put(42u64)?;
    wip.pop()?;

    wip.push_field("droppable")?;
    wip.put("Hello".to_string())?;
    wip.pop()?;

    // Drop without initializing 'more_copy'
    drop(wip);

    // If this doesn't panic or segfault, we're good
}

#[test]
fn drop_fully_uninitialized() {
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
        let wip = Wip::alloc::<Container>()?;
        // Drop immediately without initializing anything
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        0,
        "No drops should occur for completely uninitialized struct"
    );
}

#[test]
fn drop_after_successful_build() {
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

    let mut wip = Wip::alloc::<NoisyDrop>()?;
    wip.put(NoisyDrop { value: 42 })?;
    let hv = wip.build()?;

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
}
