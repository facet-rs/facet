use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::test;

use super::Partial;

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
    assert_snapshot!(Partial::alloc::<f64>().unwrap().build().unwrap_err());
}

#[test]
fn f64_init() {
    let hv = Partial::alloc::<f64>()
        .unwrap()
        .set::<f64>(6.241)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*hv, 6.241);
}

#[test]
fn option_uninit() {
    assert_snapshot!(
        Partial::alloc::<Option<f64>>()
            .unwrap()
            .build()
            .unwrap_err()
    );
}

#[test]
fn option_init() {
    let hv = Partial::alloc::<Option<f64>>()
        .unwrap()
        .set::<Option<f64>>(Some(6.241))
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*hv, Some(6.241));
}

#[test]
fn struct_fully_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    assert_snapshot!(Partial::alloc::<FooBar>().unwrap().build().unwrap_err());
}

#[test]
fn struct_partially_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let mut partial = Partial::alloc::<FooBar>().unwrap();
    assert_snapshot!(
        partial
            .set_field("foo", 42_u64)
            .unwrap()
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

    let hv = Partial::alloc::<FooBar>()
        .unwrap()
        .set_field("foo", 42u64)
        .unwrap()
        .set_field("bar", true)
        .unwrap()
        .build()
        .unwrap();
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

    let mut partial = Partial::alloc::<Container>().unwrap();

    // Set tracker field first time
    partial.set_field("tracker", DropTracker { id: 1 }).unwrap();

    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

    // Set tracker field second time (should drop the previous value)
    partial.set_field("tracker", DropTracker { id: 2 }).unwrap();

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "First DropTracker should have been dropped"
    );

    // Set value field
    partial.set_field("value", 100u64).unwrap();

    let container = partial.build().unwrap();

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

    let array = Partial::alloc::<[DropTracker; 3]>()
        .unwrap()
        // Set element 0
        .set_nth_element(0, DropTracker { id: 1 })
        .unwrap()
        // Set element 0 again - drops old value
        .set_nth_element(0, DropTracker { id: 2 })
        .unwrap()
        // Set element 1
        .set_nth_element(1, DropTracker { id: 3 })
        .unwrap()
        // Set element 2
        .set_nth_element(2, DropTracker { id: 4 })
        .unwrap()
        .build()
        .unwrap();

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

    let sample = Partial::alloc::<Sample>()
        .unwrap()
        .set_default()
        .unwrap()
        .build()
        .unwrap();
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

    let result = Partial::alloc::<NoDefault>()
        .unwrap()
        .set_default()
        .map(|_| ());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("does not implement Default")
    );
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

    let mut partial = Partial::alloc::<DropTracker>().unwrap();

    // Set initial value
    partial.set(DropTracker { id: 1 }).unwrap();
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);

    // Set default (should drop the previous value)
    partial.set_default().unwrap();
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

    let tracker = partial.build().unwrap();
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
        let mut partial = Partial::alloc::<Container>().unwrap();

        // Initialize first field
        partial.begin_field("first").unwrap();
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0, "No drops yet");

        partial.set(NoisyDrop { value: 1 }).unwrap();
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After set, the value should NOT be dropped yet"
        );

        partial.end().unwrap();
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "Still no drops after end"
        );

        // Initialize second field
        partial.begin_field("second").unwrap();
        partial.set(NoisyDrop { value: 2 }).unwrap();
        assert_eq!(
            DROP_COUNT.load(Ordering::SeqCst),
            0,
            "After second set, still should have no drops"
        );

        partial.end().unwrap();

        // Don't initialize third field - just drop the partial
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
        let mut partial = Partial::alloc::<Outer>().unwrap();

        // Start initializing inner struct
        partial.begin_field("inner").unwrap();
        partial.set_field("a", NoisyDrop { id: 1 }).unwrap();

        // Only initialize one field of inner, leave 'b' uninitialized
        // Don't end from inner

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

    let mut partial = Partial::alloc::<MixedTypes>().unwrap();

    partial.set_field("copyable", 42u64).unwrap();

    partial.set_field("droppable", "Hello".to_string()).unwrap();

    // Drop without initializing 'more_copy'
    drop(partial);

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
        let _partial = Partial::alloc::<Container>().unwrap();
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

    let hv = Partial::alloc::<NoisyDrop>()
        .unwrap()
        .set(NoisyDrop { value: 42 })
        .unwrap()
        .build()
        .unwrap();

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
    let hv = Partial::alloc::<[u32; 3]>()
        .unwrap()
        // Initialize in order
        .set_nth_element(0, 42u32)
        .unwrap()
        .set_nth_element(1, 43u32)
        .unwrap()
        .set_nth_element(2, 44u32)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*hv, [42, 43, 44]);
}

#[test]
fn array_init_out_of_order() {
    let hv = Partial::alloc::<[u32; 3]>()
        .unwrap()
        // Initialize out of order
        .set_nth_element(2, 44u32)
        .unwrap()
        .set_nth_element(0, 42u32)
        .unwrap()
        .set_nth_element(1, 43u32)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*hv, [42, 43, 44]);
}

#[test]
fn array_partial_init() {
    // Should fail to build
    assert_snapshot!(
        Partial::alloc::<[u32; 3]>()
            .unwrap()
            // Initialize only two elements
            .set_nth_element(0, 42u32)
            .unwrap()
            .set_nth_element(2, 44u32)
            .unwrap()
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
        let mut partial = Partial::alloc::<[NoisyDrop; 4]>().unwrap();

        // Initialize elements 0 and 2
        partial.set_nth_element(0, NoisyDrop { value: 10 }).unwrap();
        partial.set_nth_element(2, NoisyDrop { value: 30 }).unwrap();

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
    let hv = Partial::alloc::<Box<u32>>()
        .unwrap()
        // Push into the Box to build its inner value
        .begin_smart_ptr()
        .unwrap()
        .set(42u32)
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(**hv, 42);
}

#[test]
fn box_partial_init() {
    // Don't initialize the Box at all
    assert_snapshot!(Partial::alloc::<Box<u32>>().unwrap().build().unwrap_err());
}

#[test]
fn box_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let hv = Partial::alloc::<Box<Point>>()
        .unwrap()
        // Push into the Box
        .begin_smart_ptr()
        .unwrap()
        // Build the Point inside the Box using set_field shorthand
        .set_field("x", 1.0)
        .unwrap()
        .set_field("y", 2.0)
        .unwrap()
        // end from Box
        .end()
        .unwrap()
        .build()
        .unwrap();
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
        let mut partial = Partial::alloc::<Box<DropCounter>>().unwrap();

        // Initialize the Box's inner value using set
        partial.begin_smart_ptr().unwrap();
        partial.set(DropCounter { value: 99 }).unwrap();
        partial.end().unwrap();

        // Drop the partial - should drop the Box which drops the inner value
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

    let hv = Partial::alloc::<Arc<u32>>()
        .unwrap()
        // Push into the Arc to build its inner value
        .begin_smart_ptr()
        .unwrap()
        .set(42u32)
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(**hv, 42);
}

#[test]
fn arc_partial_init() {
    use alloc::sync::Arc;

    // Don't initialize the Arc at all
    assert_snapshot!(Partial::alloc::<Arc<u32>>().unwrap().build().unwrap_err());
}

#[test]
fn arc_struct() {
    use alloc::sync::Arc;

    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let hv = Partial::alloc::<Arc<Point>>()
        .unwrap()
        // Push into the Arc
        .begin_smart_ptr()
        .unwrap()
        // Build the Point inside the Arc using set_field shorthand
        .set_field("x", 3.0)
        .unwrap()
        .set_field("y", 4.0)
        .unwrap()
        // end from Arc
        .end()
        .unwrap()
        .build()
        .unwrap();
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
        let mut partial = Partial::alloc::<Arc<DropCounter>>().unwrap();

        // Initialize the Arc's inner value
        partial.begin_smart_ptr().unwrap();
        partial.set(DropCounter { value: 123 }).unwrap();
        partial.end().unwrap();

        // Drop the partial - should drop the Arc which drops the inner value
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

    let hv = Partial::alloc::<Status>()
        .unwrap()
        .select_variant(1)
        .unwrap() // Inactive
        .build()
        .unwrap();
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

    let hv = Partial::alloc::<Message>()
        .unwrap()
        .select_variant(0)
        .unwrap() // Text variant
        .set_field("content", "Hello, world!".to_string())
        .unwrap()
        .build()
        .unwrap();
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

    let hv = Partial::alloc::<Value>()
        .unwrap()
        .select_variant(2)
        .unwrap() // Pair variant
        .set_nth_enum_field(0, 42)
        .unwrap()
        .set_nth_enum_field(1, "test".to_string())
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*hv, Value::Pair(42, "test".to_string()));
}

#[test]
fn enum_set_field_twice() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u16)]
    enum Data {
        Point { x: f32, y: f32 } = 0,
    }

    let hv = Partial::alloc::<Data>()
        .unwrap()
        .select_variant(0)
        .unwrap() // Point variant
        // Set x field
        .set_field("x", 1.0f32)
        .unwrap()
        // Set x field again (should drop previous value)
        .set_field("x", 2.0f32)
        .unwrap()
        // Set y field
        .set_field("y", 3.0f32)
        .unwrap()
        .build()
        .unwrap();
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
    let result = Partial::alloc::<Config>()
        .unwrap()
        .select_variant(0)
        .unwrap() // Settings variant
        // Only initialize timeout, not retries
        .set_field("timeout", 5000u32)
        .unwrap()
        .build();
    assert!(result.is_err());
}

#[test]
fn enum_select_nth_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active = 0,
        Inactive = 1,
        Pending = 2,
    }

    // Test selecting variant by index (0-based)
    let hv = Partial::alloc::<Status>()
        .unwrap()
        .select_nth_variant(1)
        .unwrap() // Inactive (index 1)
        .build()
        .unwrap();
    assert_eq!(*hv, Status::Inactive);

    // Test selecting different variant by index
    let hv2 = Partial::alloc::<Status>()
        .unwrap()
        .select_nth_variant(2)
        .unwrap() // Pending (index 2)
        .build()
        .unwrap();
    assert_eq!(*hv2, Status::Pending);
}

#[test]
fn empty_struct_init() {
    #[derive(Facet, Debug, PartialEq)]
    struct EmptyStruct {}

    // Test that we can build an empty struct without setting any fields
    let hv = Partial::alloc::<EmptyStruct>().unwrap().build().unwrap();
    assert_eq!(*hv, EmptyStruct {});
}

#[test]
fn list_vec_basic() {
    let hv = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .begin_list()
        .unwrap()
        .push(42)
        .unwrap()
        .push(84)
        .unwrap()
        .push(126)
        .unwrap()
        .build()
        .unwrap();
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

    let hv = Partial::alloc::<Vec<Person>>()
        .unwrap()
        .begin_list()
        .unwrap()
        // Push first person
        .begin_list_item()
        .unwrap()
        .set_field("name", "Alice".to_string())
        .unwrap()
        .set_field("age", 30u32)
        .unwrap()
        .end()
        .unwrap() // Done with first person
        // Push second person
        .begin_list_item()
        .unwrap()
        .set_field("name", "Bob".to_string())
        .unwrap()
        .set_field("age", 25u32)
        .unwrap()
        .end()
        .unwrap() // Done with second person
        .build()
        .unwrap();
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
    let hv = Partial::alloc::<Vec<String>>()
        .unwrap()
        .begin_list()
        .unwrap()
        // Don't push any elements
        .build()
        .unwrap();
    let vec: &Vec<String> = hv.as_ref();
    assert_eq!(vec, &Vec::<String>::new());
}

#[test]
fn list_vec_nested() {
    let hv = Partial::alloc::<Vec<Vec<i32>>>()
        .unwrap()
        .begin_list()
        .unwrap()
        // Push first inner vec
        .begin_list_item()
        .unwrap()
        .begin_list()
        .unwrap()
        .push(1)
        .unwrap()
        .push(2)
        .unwrap()
        .end()
        .unwrap() // Done with first inner vec
        // Push second inner vec
        .begin_list_item()
        .unwrap()
        .begin_list()
        .unwrap()
        .push(3)
        .unwrap()
        .push(4)
        .unwrap()
        .push(5)
        .unwrap()
        .end()
        .unwrap() // Done with second inner vec
        .build()
        .unwrap();
    let vec: &Vec<Vec<i32>> = hv.as_ref();
    assert_eq!(vec, &vec![vec![1, 2], vec![3, 4, 5]]);
}

#[derive(Debug)]
struct IPanic;

impl<E> From<E> for IPanic
where
    E: core::error::Error + Send + Sync,
{
    #[track_caller]
    fn from(value: E) -> Self {
        panic!("from: {}: {value}", core::panic::Location::caller())
    }
}

#[test]
fn list_vec_reinit() -> Result<(), IPanic> {
    let mut p = Partial::alloc::<Vec<i32>>()?;
    p.begin_list()?;
    p.push(1)?;
    p.push(2)?;
    p.begin_list()?;
    p.push(3)?;
    p.push(4)?;
    let hv = p.build()?;
    let vec: &Vec<i32> = hv.as_ref();
    assert_eq!(vec, &vec![1, 2, 3, 4]);

    Ok(())
}

#[test]
fn list_vec_field_reinit() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        s: Vec<i32>,
    }

    let mut p = Partial::alloc::<S>()?;
    p.begin_field("s")?;
    p.begin_list()?;
    p.push(1)?;
    p.push(2)?;
    p.end()?; // the field
    p.begin_field("s")?;
    p.begin_list()?;
    p.push(3)?;
    p.push(4)?;
    p.end()?; // the field

    let hv = p.build()?;
    let s = hv.as_ref();
    assert_eq!(
        s,
        &S {
            s: vec![1, 2, 3, 4]
        }
    );

    Ok(())
}

#[test]
fn list_wrong_begin_list() -> Result<(), IPanic> {
    let mut hv = Partial::alloc::<HashMap<String, i32>>()?;
    assert!(
        hv.begin_list()
            .unwrap_err()
            .to_string()
            .contains("begin_list can only be called on List types")
    );
    Ok(())
}

#[test]
fn map_hashmap_simple() {
    use std::collections::HashMap;

    let hv = Partial::alloc::<HashMap<String, i32>>()
        .unwrap()
        .begin_map()
        .unwrap()
        // Insert first pair: "foo" -> 42
        .begin_key()
        .unwrap()
        .set("foo".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_value()
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        // Insert second pair: "bar" -> 123
        .begin_key()
        .unwrap()
        .set("bar".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_value()
        .unwrap()
        .set(123)
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap();
    let map: &HashMap<String, i32> = hv.as_ref();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("foo"), Some(&42));
    assert_eq!(map.get("bar"), Some(&123));
}

#[test]
fn map_hashmap_empty() {
    use std::collections::HashMap;

    let hv = Partial::alloc::<HashMap<String, String>>()
        .unwrap()
        .begin_map()
        .unwrap()
        // Don't insert any pairs
        .build()
        .unwrap();
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

    let hv = Partial::alloc::<HashMap<String, Person>>()
        .unwrap()
        .begin_map()
        .unwrap()
        // Insert "alice" -> Person { name: "Alice", age: 30 }
        .set_key("alice".to_string())
        .unwrap()
        .begin_value()
        .unwrap()
        .set_field("name", "Alice".to_string())
        .unwrap()
        .set_field("age", 30u32)
        .unwrap()
        .end()
        .unwrap() // Done with value
        // Insert "bob" -> Person { name: "Bob", age: 25 }
        .set_key("bob".to_string())
        .unwrap()
        .begin_value()
        .unwrap()
        .set_field("name", "Bob".to_string())
        .unwrap()
        .set_field("age", 25u32)
        .unwrap()
        .end()
        .unwrap() // Done with value
        .build()
        .unwrap();
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
    let animal = Partial::alloc::<Animal>()
        .unwrap()
        .select_variant_named("Dog")
        .unwrap()
        .set_field("name", "Buddy".to_string())
        .unwrap()
        .set_field("age", 5u8)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        *animal,
        Animal::Dog {
            name: "Buddy".to_string(),
            age: 5
        }
    );

    // Test Cat variant
    let animal = Partial::alloc::<Animal>()
        .unwrap()
        .select_variant_named("Cat")
        .unwrap()
        .set_field("name", "Whiskers".to_string())
        .unwrap()
        .set_field("lives", 9u8)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        *animal,
        Animal::Cat {
            name: "Whiskers".to_string(),
            lives: 9
        }
    );

    // Test Bird variant
    let animal = Partial::alloc::<Animal>()
        .unwrap()
        .select_variant_named("Bird")
        .unwrap()
        .set_field("species", "Parrot".to_string())
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        *animal,
        Animal::Bird {
            species: "Parrot".to_string()
        }
    );

    // Test invalid variant name
    let mut partial = Partial::alloc::<Animal>().unwrap();
    let result = partial.select_variant_named("Fish");
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

    let person = Partial::alloc::<Person>()
        .unwrap()
        // Use field names instead of indices
        .begin_field("email")
        .unwrap()
        .set("john@example.com".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set("John Doe".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_field("age")
        .unwrap()
        .set(30u32)
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        *person,
        Person {
            name: "John Doe".to_string(),
            age: 30,
            email: "john@example.com".to_string(),
        }
    );

    // Test invalid field name
    let mut partial = Partial::alloc::<Person>().unwrap();
    let result = partial.begin_field("invalid_field");
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
    let config = Partial::alloc::<Config>()
        .unwrap()
        .select_variant_named("Server")
        .unwrap()
        .set_field("port", 8080u16)
        .unwrap()
        .set_field("host", "localhost".to_string())
        .unwrap()
        .set_field("tls", true)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        *config,
        Config::Server {
            host: "localhost".to_string(),
            port: 8080,
            tls: true,
        }
    );

    // Test invalid field name on enum variant

    let mut partial = Partial::alloc::<Config>().unwrap();
    partial.select_variant_named("Client").unwrap();
    let result = partial.begin_field("port"); // port doesn't exist on Client
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
        let mut partial = Partial::alloc::<HashMap<String, DropTracker>>().unwrap();
        partial
            .begin_map()
            .unwrap()
            // Insert a complete pair
            .begin_key()
            .unwrap()
            .set("first".to_string())
            .unwrap()
            .end()
            .unwrap()
            .begin_value()
            .unwrap()
            .set(DropTracker { id: 1 })
            .unwrap()
            .end()
            .unwrap()
            // Start inserting another pair but only complete the key
            .begin_key()
            .unwrap()
            .set("second".to_string())
            .unwrap()
            .end()
            .unwrap();
        // Don't set_value - leave incomplete

        // Drop the partial - should clean up properly
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
    let boxed = Partial::alloc::<(i32, String)>()
        .unwrap()
        .set_nth_field(0, 42i32)
        .unwrap()
        .set_nth_field(1, "hello".to_string())
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*boxed, (42, "hello".to_string()));
}

#[test]
fn tuple_mixed_types() {
    // Test building a tuple with more diverse types
    let boxed = Partial::alloc::<(u8, bool, f64, String)>()
        .unwrap()
        // Set fields in non-sequential order to test flexibility
        .set_nth_field(2, 56.124f64)
        .unwrap()
        .set_nth_field(0, 255u8)
        .unwrap()
        .set_nth_field(3, "world".to_string())
        .unwrap()
        .set_nth_field(1, true)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*boxed, (255u8, true, 56.124f64, "world".to_string()));
}

#[test]
fn tuple_nested() {
    // Test nested tuples
    let boxed = Partial::alloc::<((i32, i32), String)>()
        .unwrap()
        // Build the nested tuple first
        .begin_nth_field(0)
        .unwrap()
        .set_nth_field(0, 1i32)
        .unwrap()
        .set_nth_field(1, 2i32)
        .unwrap()
        .end()
        .unwrap() // Pop out of the nested tuple
        // Now set the string
        .set_nth_field(1, "nested".to_string())
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*boxed, ((1, 2), "nested".to_string()));
}

#[test]
fn tuple_empty() {
    // Test empty tuple (unit type)
    let boxed = Partial::alloc::<()>()
        .unwrap()
        .set(())
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(*boxed, ());
}
