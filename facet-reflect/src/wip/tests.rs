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
    assert_snapshot!(Wip::alloc::<f64>()?.build().unwrap_err());
}

#[test]
fn f64_init() {
    let hv = Wip::alloc::<f64>()?.set::<f64>(6.241)?.build()?;
    assert_eq!(*hv, 6.241);
}

#[test]
fn option_uninit() {
    assert_snapshot!(Wip::alloc::<Option<f64>>()?.build().unwrap_err());
}

#[test]
fn option_init() {
    let hv = Wip::alloc::<Option<f64>>()?
        .set::<Option<f64>>(Some(6.241))?
        .build()?;
    assert_eq!(*hv, Some(6.241));
}

#[test]
fn struct_fully_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    assert_snapshot!(Wip::alloc::<FooBar>()?.build().unwrap_err());
}

#[test]
fn struct_partially_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let mut wip = Wip::alloc::<FooBar>()?;
    assert_snapshot!(
        wip.push_field("foo")?
            .set::<u64>(42)?
            .pop()?
            .build()
            .unwrap_err()
    );
}

#[test]
fn struct_fully_init() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let hv = Wip::alloc::<FooBar>()?
        .push_field("foo")?
        .set::<u64>(42)?
        .pop()?
        .push_field("bar")?
        .set::<bool>(true)?
        .pop()?
        .build()?;
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
        wip.push_field("tracker")?
            .set(DropTracker { id: 1 })?
            .pop()?;

        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

        // Set tracker field second time (should drop the previous value)
        wip.push_field("tracker")?
            .set(DropTracker { id: 2 })?
            .pop()?;

        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            1,
            "First DropTracker should have been dropped"
        );

        // Set value field
        wip.push_field("value")?.set(100u64)?.pop()?;

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
        Wip::alloc::<[DropTracker; 3]>()?
            // Set element 0
            .push_nth_element(0)?
            .set(DropTracker { id: 1 })?
            .pop()?
            // Set element 0 again - drops old value
            .push_nth_element(0)?
            .set(DropTracker { id: 2 })?
            .pop()?
            // Set element 1
            .push_nth_element(1)?
            .set(DropTracker { id: 3 })?
            .pop()?
            // Set element 2
            .push_nth_element(2)?
            .set(DropTracker { id: 4 })?
            .pop()?
            .build()
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

    let sample = Wip::alloc::<Sample>()?.set_default()?.build()?;
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

    let result = Wip::alloc::<NoDefault>()?.set_default().map(|_| ());
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

    let point = Wip::alloc::<Point>()?
        .set_from_function(|ptr| {
            // We need to build the struct using another Wip
            Wip::from_ptr(ptr, <Point as Facet>::SHAPE)
                .push_field("x")?
                .set(56.124)?
                .pop()?
                .push_field("y")?
                .set(2.71)?
                .pop()?;
            Ok(())
        })?
        .build()?;
    assert_eq!(*point, Point { x: 56.124, y: 2.71 });
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

    let hv = Wip::alloc::<NoisyDrop>()?
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
}

#[test]
fn array_init() {
    let hv = Wip::alloc::<[u32; 3]>()?
        // Initialize in order
        .push_nth_element(0)?
        .set(42u32)?
        .pop()?
        .push_nth_element(1)?
        .set(43u32)?
        .pop()?
        .push_nth_element(2)?
        .set(44u32)?
        .pop()?
        .build()?;
    assert_eq!(*hv, [42, 43, 44]);
}

#[test]
fn array_init_out_of_order() {
    let hv = Wip::alloc::<[u32; 3]>()?
        // Initialize out of order
        .push_nth_element(2)?
        .set(44u32)?
        .pop()?
        .push_nth_element(0)?
        .set(42u32)?
        .pop()?
        .push_nth_element(1)?
        .set(43u32)?
        .pop()?
        .build()?;
    assert_eq!(*hv, [42, 43, 44]);
}

#[test]
fn array_partial_init() {
    // Should fail to build
    assert_snapshot!(
        Wip::alloc::<[u32; 3]>()?
            // Initialize only two elements
            .push_nth_element(0)?
            .set(42u32)?
            .pop()?
            .push_nth_element(2)?
            .set(44u32)?
            .pop()?
            .build()
            .unwrap_err()
    );
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
    let hv = Wip::alloc::<Box<u32>>()?
        // Push into the Box to build its inner value
        .push_box()?
        .set(42u32)?
        .pop()?
        .build()?;
    assert_eq!(**hv, 42);
}

#[test]
fn box_partial_init() {
    // Don't initialize the Box at all
    assert_snapshot!(Wip::alloc::<Box<u32>>()?.build().unwrap_err());
}

#[test]
fn box_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let hv = Wip::alloc::<Box<Point>>()?
        // Push into the Box
        .push_box()?
        // Build the Point inside the Box
        .push_field("x")?
        .set(1.0)?
        .pop()?
        .push_field("y")?
        .set(2.0)?
        .pop()?
        // Pop from Box
        .pop()?
        .build()?;
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

    let hv = Wip::alloc::<Arc<u32>>()?
        // Push into the Arc to build its inner value
        .push_smart_ptr()?
        .set(42u32)?
        .pop()?
        .build()?;
    assert_eq!(**hv, 42);
}

#[test]
fn arc_partial_init() {
    use alloc::sync::Arc;

    // Don't initialize the Arc at all
    assert_snapshot!(Wip::alloc::<Arc<u32>>()?.build().unwrap_err());
}

#[test]
fn arc_struct() {
    use alloc::sync::Arc;

    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let hv = Wip::alloc::<Arc<Point>>()?
        // Push into the Arc
        .push_smart_ptr()?
        // Build the Point inside the Arc
        .push_field("x")?
        .set(3.0)?
        .pop()?
        .push_field("y")?
        .set(4.0)?
        .pop()?
        // Pop from Arc
        .pop()?
        .build()?;
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
    #[allow(dead_code)]
    enum Status {
        Active = 0,
        Inactive = 1,
        Pending = 2,
    }

    let hv = Wip::alloc::<Status>()?
        .push_variant(1)? // Inactive
        .build()?;
    assert_eq!(*hv, Status::Inactive);
}

#[test]
fn enum_struct_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text { content: String } = 0,
        Number { value: i32 } = 1,
        Empty = 2,
    }

    let hv = Wip::alloc::<Message>()?
        .push_variant(0)? // Text variant
        .push_field("content")?
        .set("Hello, world!".to_string())?
        .pop()?
        .build()?;
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
    #[allow(dead_code)]
    enum Value {
        Int(i32) = 0,
        Float(f64) = 1,
        Pair(i32, String) = 2,
    }

    let hv = Wip::alloc::<Value>()?
        .push_variant(2)? // Pair variant
        .push_nth_enum_field(0)?
        .set(42)?
        .pop()?
        .push_nth_enum_field(1)?
        .set("test".to_string())?
        .pop()?
        .build()?;
    assert_eq!(*hv, Value::Pair(42, "test".to_string()));
}

#[test]
fn enum_set_field_twice() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u16)]
    enum Data {
        Point { x: f32, y: f32 } = 0,
    }

    let hv = Wip::alloc::<Data>()?
        .push_variant(0)? // Point variant
        // Set x field
        .push_field("x")?
        .set(1.0f32)?
        .pop()?
        // Set x field again (should drop previous value)
        .push_field("x")?
        .set(2.0f32)?
        .pop()?
        // Set y field
        .push_field("y")?
        .set(3.0f32)?
        .pop()?
        .build()?;
    assert_eq!(*hv, Data::Point { x: 2.0, y: 3.0 });
}

#[test]
fn enum_partial_initialization_error() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Config {
        Settings { timeout: u32, retries: u8 } = 0,
    }

    // Should fail to build because retries is not initialized
    let result = Wip::alloc::<Config>()?
        .push_variant(0)? // Settings variant
        // Only initialize timeout, not retries
        .push_field("timeout")?
        .set(5000u32)?
        .pop()?
        .build();
    assert!(result.is_err());
}

#[test]
fn list_vec_basic() {
    let hv = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        // Push first element
        .push()?
        .set(42)?
        .pop()?
        // Push second element
        .push()?
        .set(84)?
        .pop()?
        // Push third element
        .push()?
        .set(126)?
        .pop()?
        .build()?;
    let vec: &Vec<i32> = hv.as_ref();
    assert_eq!(vec, &vec![42, 84, 126]);
}

#[test]
fn list_vec_complex() {
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    let hv = Wip::alloc::<Vec<Person>>()?
        .begin_pushback()?
        // Push first person
        .push()?
        .push_nth_field(0)? // name
        .set("Alice".to_string())?
        .pop()?
        .push_nth_field(1)? // age
        .set(30u32)?
        .pop()?
        .pop()? // Done with first person
        // Push second person
        .push()?
        .push_nth_field(0)? // name
        .set("Bob".to_string())?
        .pop()?
        .push_nth_field(1)? // age
        .set(25u32)?
        .pop()?
        .pop()? // Done with second person
        .build()?;
    let vec: &Vec<Person> = hv.as_ref();
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
    let hv = Wip::alloc::<Vec<String>>()?
        .begin_pushback()?
        // Don't push any elements
        .build()?;
    let vec: &Vec<String> = hv.as_ref();
    assert_eq!(vec, &Vec::<String>::new());
}

#[test]
fn list_vec_nested() {
    let hv = Wip::alloc::<Vec<Vec<i32>>>()?
        .begin_pushback()?
        // Push first inner vec
        .push()?
        .begin_pushback()?
        .push()?
        .set(1)?
        .pop()?
        .push()?
        .set(2)?
        .pop()?
        .pop()? // Done with first inner vec
        // Push second inner vec
        .push()?
        .begin_pushback()?
        .push()?
        .set(3)?
        .pop()?
        .push()?
        .set(4)?
        .pop()?
        .push()?
        .set(5)?
        .pop()?
        .pop()? // Done with second inner vec
        .build()?;
    let vec: &Vec<Vec<i32>> = hv.as_ref();
    assert_eq!(vec, &vec![vec![1, 2], vec![3, 4, 5]]);
}

#[test]
fn map_hashmap_simple() {
    use std::collections::HashMap;

    let hv = Wip::alloc::<HashMap<String, i32>>()?
        .begin_map()?
        // Insert first pair: "foo" -> 42
        .begin_insert()?
        .push_key()?
        .set("foo".to_string())?
        .pop()?
        .push_value()?
        .set(42)?
        .pop()?
        // Insert second pair: "bar" -> 123
        .begin_insert()?
        .push_key()?
        .set("bar".to_string())?
        .pop()?
        .push_value()?
        .set(123)?
        .pop()?
        .build()?;
    let map: &HashMap<String, i32> = hv.as_ref();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("foo"), Some(&42));
    assert_eq!(map.get("bar"), Some(&123));
}

#[test]
fn map_hashmap_empty() {
    use std::collections::HashMap;

    let hv = Wip::alloc::<HashMap<String, String>>()?
        .begin_map()?
        // Don't insert any pairs
        .build()?;
    let map: &HashMap<String, String> = hv.as_ref();
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

    let hv = Wip::alloc::<HashMap<String, Person>>()?
        .begin_map()?
        // Insert "alice" -> Person { name: "Alice", age: 30 }
        .begin_insert()?
        .push_key()?
        .set("alice".to_string())?
        .pop()?
        .push_value()?
        .push_field("name")?
        .set("Alice".to_string())?
        .pop()?
        .push_field("age")?
        .set(30u32)?
        .pop()?
        .pop()? // Done with value
        // Insert "bob" -> Person { name: "Bob", age: 25 }
        .begin_insert()?
        .push_key()?
        .set("bob".to_string())?
        .pop()?
        .push_value()?
        .push_field("name")?
        .set("Bob".to_string())?
        .pop()?
        .push_field("age")?
        .set(25u32)?
        .pop()?
        .pop()? // Done with value
        .build()?;
    let map: &HashMap<String, Person> = hv.as_ref();
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
    let animal = Wip::alloc::<Animal>()?
        .push_variant_named("Dog")?
        .push_field("name")?
        .set("Buddy".to_string())?
        .pop()?
        .push_field("age")?
        .set(5u8)?
        .pop()?
        .build()?;
    assert_eq!(
        *animal,
        Animal::Dog {
            name: "Buddy".to_string(),
            age: 5
        }
    );

    // Test Cat variant
    let animal = Wip::alloc::<Animal>()?
        .push_variant_named("Cat")?
        .push_field("name")?
        .set("Whiskers".to_string())?
        .pop()?
        .push_field("lives")?
        .set(9u8)?
        .pop()?
        .build()?;
    assert_eq!(
        *animal,
        Animal::Cat {
            name: "Whiskers".to_string(),
            lives: 9
        }
    );

    // Test Bird variant
    let animal = Wip::alloc::<Animal>()?
        .push_variant_named("Bird")?
        .push_field("species")?
        .set("Parrot".to_string())?
        .pop()?
        .build()?;
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

    let person = Wip::alloc::<Person>()?
        // Use field names instead of indices
        .push_field("email")?
        .set("john@example.com".to_string())?
        .pop()?
        .push_field("name")?
        .set("John Doe".to_string())?
        .pop()?
        .push_field("age")?
        .set(30u32)?
        .pop()?
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
    let mut wip = Wip::alloc::<Person>()?;
    let result = wip.push_field("invalid_field");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("field not found"));
}

#[test]
fn field_named_on_enum() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Config {
        Server { host: String, port: u16, tls: bool } = 0,
        Client { url: String, timeout: u32 } = 1,
    }

    // Test field access on Server variant
    let config = Wip::alloc::<Config>()?
        .push_variant_named("Server")?
        .push_field("port")?
        .set(8080u16)?
        .pop()?
        .push_field("host")?
        .set("localhost".to_string())?
        .pop()?
        .push_field("tls")?
        .set(true)?
        .pop()?
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

#[test]
fn tuple_basic() {
    // Test building a simple tuple
    let boxed = Wip::alloc::<(i32, String)>()?
        // Tuples are represented as structs, so we use push_nth_field
        .push_nth_field(0)?
        .set(42i32)?
        .pop()?
        .push_nth_field(1)?
        .set("hello".to_string())?
        .pop()?
        .build()?;
    assert_eq!(*boxed, (42, "hello".to_string()));
}

#[test]
fn tuple_mixed_types() {
    // Test building a tuple with more diverse types
    let boxed = Wip::alloc::<(u8, bool, f64, String)>()?
        // Set fields in non-sequential order to test flexibility
        .push_nth_field(2)?
        .set(56.124f64)?
        .pop()?
        .push_nth_field(0)?
        .set(255u8)?
        .pop()?
        .push_nth_field(3)?
        .set("world".to_string())?
        .pop()?
        .push_nth_field(1)?
        .set(true)?
        .pop()?
        .build()?;
    assert_eq!(*boxed, (255u8, true, 56.124f64, "world".to_string()));
}

#[test]
fn tuple_nested() {
    // Test nested tuples
    let boxed = Wip::alloc::<((i32, i32), String)>()?
        // Build the nested tuple first
        .push_nth_field(0)?
        .push_nth_field(0)?
        .set(1i32)?
        .pop()?
        .push_nth_field(1)?
        .set(2i32)?
        .pop()?
        .pop()? // Pop out of the nested tuple
        // Now set the string
        .push_nth_field(1)?
        .set("nested".to_string())?
        .pop()?
        .build()?;
    assert_eq!(*boxed, ((1, 2), "nested".to_string()));
}

#[test]
fn tuple_empty() {
    // Test empty tuple (unit type)
    let boxed = Wip::alloc::<()>()?
        // Empty tuple has no fields to set, but we still need to set it
        .set(())?
        .build()?;
    assert_eq!(*boxed, ());
}
