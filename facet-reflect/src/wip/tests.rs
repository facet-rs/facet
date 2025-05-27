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
    wip.set::<f64>(6.241)?;
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
    wip.set::<Option<f64>>(Some(6.241))?;
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
    wip.set::<u64>(42)?;
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
    wip.set::<u64>(42)?;
    wip.pop()?;
    wip.push_field("bar")?;
    wip.set::<bool>(true)?;
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

        wip.set(NoisyDrop { value: 1 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After set, the value should NOT be dropped yet"
        );

        wip.pop()?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "Still no drops after pop"
        );

        // Initialize second field
        wip.push_field("second")?;
        wip.set(NoisyDrop { value: 2 })?;
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After second set, still should have no drops"
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
        wip.set(NoisyDrop { id: 1 })?;
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
    wip.set(42u64)?;
    wip.pop()?;

    wip.push_field("droppable")?;
    wip.set("Hello".to_string())?;
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
        let _wip = Wip::alloc::<Container>()?;
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
    wip.set(NoisyDrop { value: 42 })?;
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

#[test]
fn array_init() {
    let mut wip = Wip::alloc::<[u32; 3]>()?;

    // Initialize in order
    wip.push_nth_element(0)?;
    wip.set(42u32)?;
    wip.pop()?;

    wip.push_nth_element(1)?;
    wip.set(43u32)?;
    wip.pop()?;

    wip.push_nth_element(2)?;
    wip.set(44u32)?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(*hv, [42, 43, 44]);
}

#[test]
fn array_init_out_of_order() {
    let mut wip = Wip::alloc::<[u32; 3]>()?;

    // Initialize out of order
    wip.push_nth_element(2)?;
    wip.set(44u32)?;
    wip.pop()?;

    wip.push_nth_element(0)?;
    wip.set(42u32)?;
    wip.pop()?;

    wip.push_nth_element(1)?;
    wip.set(43u32)?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(*hv, [42, 43, 44]);
}

#[test]
fn array_partial_init() {
    let mut wip = Wip::alloc::<[u32; 3]>()?;

    // Initialize only two elements
    wip.push_nth_element(0)?;
    wip.set(42u32)?;
    wip.pop()?;

    wip.push_nth_element(2)?;
    wip.set(44u32)?;
    wip.pop()?;

    // Should fail to build
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn drop_array_partially_initialized() {
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

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let mut wip = Wip::alloc::<[NoisyDrop; 4]>()?;

        // Initialize elements 0 and 2
        wip.push_nth_element(0)?;
        wip.set(NoisyDrop { value: 10 })?;
        wip.pop()?;

        wip.push_nth_element(2)?;
        wip.set(NoisyDrop { value: 30 })?;
        wip.pop()?;

        // Drop without initializing elements 1 and 3
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        2,
        "Should drop only the two initialized array elements"
    );
}

#[test]
fn box_init() {
    let mut wip = Wip::alloc::<Box<u32>>()?;

    // Push into the Box to build its inner value
    wip.push_box()?;
    wip.set(42u32)?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(**hv, 42);
}

#[test]
fn box_partial_init() {
    let wip = Wip::alloc::<Box<u32>>()?;
    // Don't initialize the Box at all
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn box_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let mut wip = Wip::alloc::<Box<Point>>()?;

    // Push into the Box
    wip.push_box()?;

    // Build the Point inside the Box
    wip.push_field("x")?;
    wip.set(1.0)?;
    wip.pop()?;

    wip.push_field("y")?;
    wip.set(2.0)?;
    wip.pop()?;

    // Pop from Box
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(**hv, Point { x: 1.0, y: 2.0 });
}

#[test]
fn drop_box_partially_initialized() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static BOX_DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    static INNER_DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropCounter {
        value: u32,
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            INNER_DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping DropCounter with value: {}", self.value);
        }
    }

    BOX_DROP_COUNT.store(0, Ordering::SeqCst);
    INNER_DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let mut wip = Wip::alloc::<Box<DropCounter>>()?;

        // Initialize the Box's inner value
        wip.push_box()?;
        wip.set(DropCounter { value: 99 })?;
        wip.pop()?;

        // Drop the wip - should drop the Box which drops the inner value
    }

    assert_eq!(
        INNER_DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop the inner value through Box's drop"
    );
}

#[test]
fn arc_init() {
    use alloc::sync::Arc;

    let mut wip = Wip::alloc::<Arc<u32>>()?;

    // Push into the Arc to build its inner value
    wip.push_smart_ptr()?;
    wip.set(42u32)?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(**hv, 42);
}

#[test]
fn arc_partial_init() {
    use alloc::sync::Arc;

    let wip = Wip::alloc::<Arc<u32>>()?;
    // Don't initialize the Arc at all
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn arc_struct() {
    use alloc::sync::Arc;

    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let mut wip = Wip::alloc::<Arc<Point>>()?;

    // Push into the Arc
    wip.push_smart_ptr()?;

    // Build the Point inside the Arc
    wip.push_field("x")?;
    wip.set(3.0)?;
    wip.pop()?;

    wip.push_field("y")?;
    wip.set(4.0)?;
    wip.pop()?;

    // Pop from Arc
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(**hv, Point { x: 3.0, y: 4.0 });
}

#[test]
fn drop_arc_partially_initialized() {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    static INNER_DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropCounter {
        value: u32,
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            INNER_DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping DropCounter with value: {}", self.value);
        }
    }

    INNER_DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let mut wip = Wip::alloc::<Arc<DropCounter>>()?;

        // Initialize the Arc's inner value
        wip.push_smart_ptr()?;
        wip.set(DropCounter { value: 123 })?;
        wip.pop()?;

        // Drop the wip - should drop the Arc which drops the inner value
    }

    assert_eq!(
        INNER_DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop the inner value through Arc's drop"
    );
}
