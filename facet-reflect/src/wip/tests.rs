use facet::Facet;
use facet_testhelpers::test;

use super::Wip;
use crate::ReflectError;

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
fn struct_field_set_twice() {
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

    let result = (|| -> Result<Box<Container>, ReflectError> {
        let mut wip = Wip::alloc::<Container>()?;

        // Set tracker field first time
        wip.push_field("tracker")?;
        wip.set(DropTracker { id: 1 })?;
        wip.pop()?;

        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

        // Set tracker field second time (should drop the previous value)
        wip.push_field("tracker")?;
        wip.set(DropTracker { id: 2 })?;
        wip.pop()?;

        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            1,
            "First DropTracker should have been dropped"
        );

        // Set value field
        wip.push_field("value")?;
        wip.set(100u64)?;
        wip.pop()?;

        wip.build()
    })();

    assert!(result.is_ok());
    let container = result.unwrap();
    assert_eq!(container.tracker.id, 2); // Should have the second value
    assert_eq!(container.value, 100);

    // Drop the container
    drop(container);

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        2,
        "Both DropTrackers should have been dropped"
    );
}

#[test]
fn array_element_set_twice() {
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

    DROP_COUNT.store(0, Ordering::SeqCst);

    let result = (|| -> Result<Box<[DropTracker; 3]>, ReflectError> {
        let mut wip = Wip::alloc::<[DropTracker; 3]>()?;

        // Set element 0
        wip.push_nth_element(0)?;
        wip.set(DropTracker { id: 1 })?;
        wip.pop()?;

        // Set element 0 again - this should now work and drop the old value
        wip.push_nth_element(0)?;
        wip.set(DropTracker { id: 2 })?;
        wip.pop()?;

        // Set other elements
        wip.push_nth_element(1)?;
        wip.set(DropTracker { id: 3 })?;
        wip.pop()?;

        wip.push_nth_element(2)?;
        wip.set(DropTracker { id: 4 })?;
        wip.pop()?;

        wip.build()
    })();

    // Now this should succeed with array element re-initialization support
    assert!(result.is_ok());
    let array = result.unwrap();

    // Verify the final array has the expected values
    assert_eq!(array[0].id, 2); // Re-initialized value
    assert_eq!(array[1].id, 3);
    assert_eq!(array[2].id, 4);

    // The first value (id: 1) should have been dropped when we re-initialized
    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "First array element should have been dropped during re-initialization"
    );
}

#[test]
fn set_default() {
    #[derive(Facet, Debug, PartialEq, Default)]
    struct Sample {
        x: u32,
        y: String,
    }

    let mut wip = Wip::alloc::<Sample>()?;
    wip.set_default()?;
    let sample = wip.build()?;
    assert_eq!(*sample, Sample::default());
    assert_eq!(sample.x, 0);
    assert_eq!(sample.y, "");
}

#[test]
fn set_default_no_default_impl() {
    #[derive(Facet, Debug)]
    struct NoDefault {
        value: u32,
    }

    let mut wip = Wip::alloc::<NoDefault>()?;
    let result = wip.set_default();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("does not implement Default")
    );
}

#[test]
fn set_from_function() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let mut wip = Wip::alloc::<Point>()?;
    wip.set_from_function(|ptr| {
        // We need to build the struct using another Wip
        let mut inner_wip = Wip::from_ptr(ptr, <Point as Facet>::SHAPE);
        inner_wip.push_field("x")?;
        inner_wip.set(3.14)?;
        inner_wip.pop()?;
        inner_wip.push_field("y")?;
        inner_wip.set(2.71)?;
        inner_wip.pop()?;
        Ok(())
    })?;

    let point = wip.build()?;
    assert_eq!(*point, Point { x: 3.14, y: 2.71 });
}

#[test]
fn set_default_drops_previous() {
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

    let mut wip = Wip::alloc::<DropTracker>()?;

    // Set initial value
    wip.set(DropTracker { id: 1 })?;
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);

    // Set default (should drop the previous value)
    wip.set_default()?;
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

    let tracker = wip.build()?;
    assert_eq!(tracker.id, 999); // Default value

    drop(tracker);
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
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

#[test]
fn enum_unit_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Status {
        Active = 0,
        Inactive = 1,
        Pending = 2,
    }

    let mut wip = Wip::alloc::<Status>()?;
    wip.push_variant(1)?; // Inactive

    let hv = wip.build()?;
    assert_eq!(*hv, Status::Inactive);
}

#[test]
fn enum_struct_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Message {
        Text { content: String } = 0,
        Number { value: i32 } = 1,
        Empty = 2,
    }

    let mut wip = Wip::alloc::<Message>()?;
    wip.push_variant(0)?; // Text variant

    wip.push_field("content")?;
    wip.set("Hello, world!".to_string())?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(
        *hv,
        Message::Text {
            content: "Hello, world!".to_string()
        }
    );
}

#[test]
fn enum_tuple_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(i32)]
    enum Value {
        Int(i32) = 0,
        Float(f64) = 1,
        Pair(i32, String) = 2,
    }

    let mut wip = Wip::alloc::<Value>()?;
    wip.push_variant(2)?; // Pair variant

    wip.push_nth_enum_field(0)?;
    wip.set(42)?;
    wip.pop()?;

    wip.push_nth_enum_field(1)?;
    wip.set("test".to_string())?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(*hv, Value::Pair(42, "test".to_string()));
}

#[test]
fn enum_set_field_twice() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u16)]
    enum Data {
        Point { x: f32, y: f32 } = 0,
    }

    let mut wip = Wip::alloc::<Data>()?;
    wip.push_variant(0)?; // Point variant

    // Set x field
    wip.push_field("x")?;
    wip.set(1.0f32)?;
    wip.pop()?;

    // Set x field again (should drop previous value)
    wip.push_field("x")?;
    wip.set(2.0f32)?;
    wip.pop()?;

    // Set y field
    wip.push_field("y")?;
    wip.set(3.0f32)?;
    wip.pop()?;

    let hv = wip.build()?;
    assert_eq!(*hv, Data::Point { x: 2.0, y: 3.0 });
}

#[test]
fn enum_partial_initialization_error() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    enum Config {
        Settings { timeout: u32, retries: u8 } = 0,
    }

    let mut wip = Wip::alloc::<Config>()?;
    wip.push_variant(0)?; // Settings variant

    // Only initialize timeout, not retries
    wip.push_field("timeout")?;
    wip.set(5000u32)?;
    wip.pop()?;

    // Should fail to build because retries is not initialized
    let result = wip.build();
    assert!(result.is_err());
}

#[test]
fn list_vec_basic() {
    let mut wip = Wip::alloc::<Vec<i32>>()?;
    wip.begin_pushback()?;

    // Push first element
    wip.push()?;
    wip.set(42)?;
    wip.pop()?;

    // Push second element
    wip.push()?;
    wip.set(84)?;
    wip.pop()?;

    // Push third element
    wip.push()?;
    wip.set(126)?;
    wip.pop()?;

    let hv = wip.build()?;
    let vec: &Vec<i32> = unsafe { hv.as_ref() };
    assert_eq!(vec, &vec![42, 84, 126]);
}

#[test]
fn list_vec_complex() {
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    let mut wip = Wip::alloc::<Vec<Person>>()?;
    wip.begin_pushback()?;

    // Push first person
    wip.push()?;
    wip.push_nth_field(0)?; // name
    wip.set("Alice".to_string())?;
    wip.pop()?;
    wip.push_nth_field(1)?; // age
    wip.set(30u32)?;
    wip.pop()?;
    wip.pop()?; // Done with first person

    // Push second person
    wip.push()?;
    wip.push_nth_field(0)?; // name
    wip.set("Bob".to_string())?;
    wip.pop()?;
    wip.push_nth_field(1)?; // age
    wip.set(25u32)?;
    wip.pop()?;
    wip.pop()?; // Done with second person

    let hv = wip.build()?;
    let vec: &Vec<Person> = unsafe { hv.as_ref() };
    assert_eq!(
        vec,
        &vec![
            Person {
                name: "Alice".to_string(),
                age: 30
            },
            Person {
                name: "Bob".to_string(),
                age: 25
            }
        ]
    );
}

#[test]
fn list_vec_empty() {
    let mut wip = Wip::alloc::<Vec<String>>()?;
    wip.begin_pushback()?;
    // Don't push any elements

    let hv = wip.build()?;
    let vec: &Vec<String> = unsafe { hv.as_ref() };
    assert_eq!(vec, &Vec::<String>::new());
}

#[test]
fn list_vec_nested() {
    let mut wip = Wip::alloc::<Vec<Vec<i32>>>()?;
    wip.begin_pushback()?;

    // Push first inner vec
    wip.push()?;
    wip.begin_pushback()?;
    wip.push()?;
    wip.set(1)?;
    wip.pop()?;
    wip.push()?;
    wip.set(2)?;
    wip.pop()?;
    wip.pop()?; // Done with first inner vec

    // Push second inner vec
    wip.push()?;
    wip.begin_pushback()?;
    wip.push()?;
    wip.set(3)?;
    wip.pop()?;
    wip.push()?;
    wip.set(4)?;
    wip.pop()?;
    wip.push()?;
    wip.set(5)?;
    wip.pop()?;
    wip.pop()?; // Done with second inner vec

    let hv = wip.build()?;
    let vec: &Vec<Vec<i32>> = unsafe { hv.as_ref() };
    assert_eq!(vec, &vec![vec![1, 2], vec![3, 4, 5]]);
}

#[test]
fn map_hashmap_simple() {
    use std::collections::HashMap;

    let mut wip = Wip::alloc::<HashMap<String, i32>>()?;
    wip.begin_map()?;

    // Insert first pair: "foo" -> 42
    wip.begin_insert()?;
    wip.push_key()?;
    wip.set("foo".to_string())?;
    wip.pop()?;
    wip.push_value()?;
    wip.set(42)?;
    wip.pop()?;

    // Insert second pair: "bar" -> 123
    wip.begin_insert()?;
    wip.push_key()?;
    wip.set("bar".to_string())?;
    wip.pop()?;
    wip.push_value()?;
    wip.set(123)?;
    wip.pop()?;

    let hv = wip.build()?;
    let map: &HashMap<String, i32> = unsafe { hv.as_ref() };
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("foo"), Some(&42));
    assert_eq!(map.get("bar"), Some(&123));
}

#[test]
fn map_hashmap_empty() {
    use std::collections::HashMap;

    let mut wip = Wip::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?;
    // Don't insert any pairs

    let hv = wip.build()?;
    let map: &HashMap<String, String> = unsafe { hv.as_ref() };
    assert_eq!(map.len(), 0);
}

#[test]
fn map_hashmap_complex_values() {
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    let mut wip = Wip::alloc::<HashMap<String, Person>>()?;
    wip.begin_map()?;

    // Insert "alice" -> Person { name: "Alice", age: 30 }
    wip.begin_insert()?;
    wip.push_key()?;
    wip.set("alice".to_string())?;
    wip.pop()?;
    wip.push_value()?;
    wip.push_field("name")?;
    wip.set("Alice".to_string())?;
    wip.pop()?;
    wip.push_field("age")?;
    wip.set(30u32)?;
    wip.pop()?;
    wip.pop()?; // Done with value

    // Insert "bob" -> Person { name: "Bob", age: 25 }
    wip.begin_insert()?;
    wip.push_key()?;
    wip.set("bob".to_string())?;
    wip.pop()?;
    wip.push_value()?;
    wip.push_field("name")?;
    wip.set("Bob".to_string())?;
    wip.pop()?;
    wip.push_field("age")?;
    wip.set(25u32)?;
    wip.pop()?;
    wip.pop()?; // Done with value

    let hv = wip.build()?;
    let map: &HashMap<String, Person> = unsafe { hv.as_ref() };
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get("alice"),
        Some(&Person {
            name: "Alice".to_string(),
            age: 30
        })
    );
    assert_eq!(
        map.get("bob"),
        Some(&Person {
            name: "Bob".to_string(),
            age: 25
        })
    );
}

#[test]
fn variant_named() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Animal {
        Dog { name: String, age: u8 } = 0,
        Cat { name: String, lives: u8 } = 1,
        Bird { species: String } = 2,
    }

    // Test Dog variant
    let mut wip = Wip::alloc::<Animal>()?;
    wip.push_variant_named("Dog")?;
    wip.push_field("name")?;
    wip.set("Buddy".to_string())?;
    wip.pop()?;
    wip.push_field("age")?;
    wip.set(5u8)?;
    wip.pop()?;
    let animal = wip.build()?;
    assert_eq!(
        *animal,
        Animal::Dog {
            name: "Buddy".to_string(),
            age: 5
        }
    );

    // Test Cat variant
    let mut wip = Wip::alloc::<Animal>()?;
    wip.push_variant_named("Cat")?;
    wip.push_field("name")?;
    wip.set("Whiskers".to_string())?;
    wip.pop()?;
    wip.push_field("lives")?;
    wip.set(9u8)?;
    wip.pop()?;
    let animal = wip.build()?;
    assert_eq!(
        *animal,
        Animal::Cat {
            name: "Whiskers".to_string(),
            lives: 9
        }
    );

    // Test Bird variant
    let mut wip = Wip::alloc::<Animal>()?;
    wip.push_variant_named("Bird")?;
    wip.push_field("species")?;
    wip.set("Parrot".to_string())?;
    wip.pop()?;
    let animal = wip.build()?;
    assert_eq!(
        *animal,
        Animal::Bird {
            species: "Parrot".to_string()
        }
    );

    // Test invalid variant name
    let mut wip = Wip::alloc::<Animal>()?;
    let result = wip.push_variant_named("Fish");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No variant found with the given name")
    );
}

#[test]
fn field_named_on_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        age: u32,
        email: String,
    }

    let mut wip = Wip::alloc::<Person>()?;

    // Use field names instead of indices
    wip.push_field("email")?;
    wip.set("john@example.com".to_string())?;
    wip.pop()?;

    wip.push_field("name")?;
    wip.set("John Doe".to_string())?;
    wip.pop()?;

    wip.push_field("age")?;
    wip.set(30u32)?;
    wip.pop()?;

    let person = wip.build()?;
    assert_eq!(
        *person,
        Person {
            name: "John Doe".to_string(),
            age: 30,
            email: "john@example.com".to_string(),
        }
    );

    // Test invalid field name
    let mut wip = Wip::alloc::<Person>()?;
    let result = wip.push_field("invalid_field");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("field not found"));
}

#[test]
fn field_named_on_enum() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Config {
        Server { host: String, port: u16, tls: bool } = 0,
        Client { url: String, timeout: u32 } = 1,
    }

    // Test field access on Server variant
    let mut wip = Wip::alloc::<Config>()?;
    wip.push_variant_named("Server")?;

    wip.push_field("port")?;
    wip.set(8080u16)?;
    wip.pop()?;

    wip.push_field("host")?;
    wip.set("localhost".to_string())?;
    wip.pop()?;

    wip.push_field("tls")?;
    wip.set(true)?;
    wip.pop()?;

    let config = wip.build()?;
    assert_eq!(
        *config,
        Config::Server {
            host: "localhost".to_string(),
            port: 8080,
            tls: true,
        }
    );

    // Test invalid field name on enum variant
    let mut wip = Wip::alloc::<Config>()?;
    wip.push_variant_named("Client")?;
    let result = wip.push_field("port"); // port doesn't exist on Client
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("field not found in current enum variant")
    );
}

#[test]
fn map_partial_initialization_drop() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::collections::HashMap;
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

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let mut wip = Wip::alloc::<HashMap<String, DropTracker>>()?;
        wip.begin_map()?;

        // Insert a complete pair
        wip.begin_insert()?;
        wip.push_key()?;
        wip.set("first".to_string())?;
        wip.pop()?;
        wip.push_value()?;
        wip.set(DropTracker { id: 1 })?;
        wip.pop()?;

        // Start inserting another pair but only complete the key
        wip.begin_insert()?;
        wip.push_key()?;
        wip.set("second".to_string())?;
        wip.pop()?;
        // Don't push value - leave incomplete

        // Drop the wip - should clean up properly
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop the one inserted value"
    );
}
