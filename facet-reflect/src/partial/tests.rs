use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::{IPanic, test};

use super::Partial;

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
fn option_uninit() -> Result<(), IPanic> {
    assert_snapshot!(Partial::alloc::<Option<f64>>()?.build().unwrap_err());
    Ok(())
}

#[test]
fn option_init() -> Result<(), IPanic> {
    let hv = Partial::alloc::<Option<f64>>()?
        .set::<Option<f64>>(Some(6.241))?
        .build()?;
    assert_eq!(*hv, Some(6.241));
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
fn array_element_set_twice() -> Result<(), IPanic> {
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

    let array = Partial::alloc::<[DropTracker; 3]>()?
        // Set element 0
        .set_nth_field(0, DropTracker { id: 1 })?
        // Set element 0 again - drops old value
        .set_nth_field(0, DropTracker { id: 2 })?
        // Set element 1
        .set_nth_field(1, DropTracker { id: 3 })?
        // Set element 2
        .set_nth_field(2, DropTracker { id: 4 })?
        .build()?;

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
fn array_init() -> Result<(), IPanic> {
    let hv = Partial::alloc::<[u32; 3]>()?
        // Initialize in order
        .set_nth_field(0, 42u32)?
        .set_nth_field(1, 43u32)?
        .set_nth_field(2, 44u32)?
        .build()?;
    assert_eq!(*hv, [42, 43, 44]);
    Ok(())
}

#[test]
fn array_init_out_of_order() -> Result<(), IPanic> {
    let hv = Partial::alloc::<[u32; 3]>()?
        // Initialize out of order
        .set_nth_field(2, 44u32)?
        .set_nth_field(0, 42u32)?
        .set_nth_field(1, 43u32)?
        .build()?;
    assert_eq!(*hv, [42, 43, 44]);
    Ok(())
}

#[test]
fn array_partial_init() -> Result<(), IPanic> {
    // Should fail to build
    assert_snapshot!(
        Partial::alloc::<[u32; 3]>()?
            // Initialize only two elements
            .set_nth_field(0, 42u32)?
            .set_nth_field(2, 44u32)?
            .build()
            .unwrap_err()
    );
    Ok(())
}

#[test]
fn drop_array_partially_initialized() -> Result<(), IPanic> {
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
        let mut partial = Partial::alloc::<[NoisyDrop; 4]>()?;

        // Initialize elements 0 and 2
        partial.set_nth_field(0, NoisyDrop { value: 10 })?;
        partial.set_nth_field(2, NoisyDrop { value: 30 })?;

        // Drop without initializing elements 1 and 3
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        2,
        "Should drop only the two initialized array elements"
    );
    Ok(())
}

#[test]
fn box_init() -> Result<(), IPanic> {
    let hv = Partial::alloc::<Box<u32>>()?
        // Push into the Box to build its inner value
        .begin_smart_ptr()?
        .set(42u32)?
        .end()?
        .build()?;
    assert_eq!(**hv, 42);
    Ok(())
}

#[test]
fn box_partial_init() -> Result<(), IPanic> {
    // Don't initialize the Box at all
    assert_snapshot!(Partial::alloc::<Box<u32>>()?.build().unwrap_err());
    Ok(())
}

#[test]
fn box_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let hv = Partial::alloc::<Box<Point>>()?
        // Push into the Box
        .begin_smart_ptr()?
        // Build the Point inside the Box using set_field shorthand
        .set_field("x", 1.0)?
        .set_field("y", 2.0)?
        // end from Box
        .end()?
        .build()?;
    assert_eq!(**hv, Point { x: 1.0, y: 2.0 });
    Ok(())
}

#[test]
fn drop_box_partially_initialized() -> Result<(), IPanic> {
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
        let mut partial = Partial::alloc::<Box<DropCounter>>()?;

        // Initialize the Box's inner value using set
        partial.begin_smart_ptr()?;
        partial.set(DropCounter { value: 99 })?;
        partial.end()?;

        // Drop the partial - should drop the Box which drops the inner value
    }

    assert_eq!(
        INNER_DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop the inner value through Box's drop"
    );
    Ok(())
}

#[test]
fn arc_init() -> Result<(), IPanic> {
    use alloc::sync::Arc;

    let hv = Partial::alloc::<Arc<u32>>()?
        // Push into the Arc to build its inner value
        .begin_smart_ptr()?
        .set(42u32)?
        .end()?
        .build()?;
    assert_eq!(**hv, 42);
    Ok(())
}

#[test]
fn arc_partial_init() -> Result<(), IPanic> {
    use alloc::sync::Arc;

    // Don't initialize the Arc at all
    assert_snapshot!(Partial::alloc::<Arc<u32>>()?.build().unwrap_err());
    Ok(())
}

#[test]
fn arc_struct() -> Result<(), IPanic> {
    use alloc::sync::Arc;

    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }

    let hv = Partial::alloc::<Arc<Point>>()?
        // Push into the Arc
        .begin_smart_ptr()?
        // Build the Point inside the Arc using set_field shorthand
        .set_field("x", 3.0)?
        .set_field("y", 4.0)?
        // end from Arc
        .end()?
        .build()?;
    assert_eq!(**hv, Point { x: 3.0, y: 4.0 });
    Ok(())
}

#[test]
fn drop_arc_partially_initialized() -> Result<(), IPanic> {
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
        let mut partial = Partial::alloc::<Arc<DropCounter>>()?;

        // Initialize the Arc's inner value
        partial.begin_smart_ptr()?;
        partial.set(DropCounter { value: 123 })?;
        partial.end()?;

        // Drop the partial - should drop the Arc which drops the inner value
    }

    assert_eq!(
        INNER_DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop the inner value through Arc's drop"
    );
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

    let hv = Partial::alloc::<Status>()?
        .select_variant(1)?
        // Inactive
        .build()?;
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
        // Text variant
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
        // Pair variant
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
        // Point variant
        // Set x field
        .set_field("x", 1.0f32)?
        // Set x field again (should drop previous value)
        .set_field("x", 2.0f32)?
        // Set y field
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

    // Should fail to build because retries is not initialized
    let result = Partial::alloc::<Config>()?
        .select_variant(0)?
        // Settings variant
        // Only initialize timeout, not retries
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

    // Test selecting variant by index (0-based)
    let hv = Partial::alloc::<Status>()?
        .select_nth_variant(1)?
        // Inactive (index 1)
        .build()?;
    assert_eq!(*hv, Status::Inactive);

    // Test selecting different variant by index
    let hv2 = Partial::alloc::<Status>()?
        .select_nth_variant(2)?
        // Pending (index 2)
        .build()?;
    assert_eq!(*hv2, Status::Pending);
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
fn list_vec_basic() -> Result<(), IPanic> {
    let hv = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .push(42)?
        .push(84)?
        .push(126)?
        .build()?;
    let vec: &Vec<i32> = hv.as_ref();
    assert_eq!(vec, &vec![42, 84, 126]);
    Ok(())
}

#[test]
fn list_vec_complex() -> Result<(), IPanic> {
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    let hv = Partial::alloc::<Vec<Person>>()?
        .begin_list()?
        // Push first person
        .begin_list_item()?
        .set_field("name", "Alice".to_string())?
        .set_field("age", 30u32)?
        .end()?
        // Done with first person
        // Push second person
        .begin_list_item()?
        .set_field("name", "Bob".to_string())?
        .set_field("age", 25u32)?
        .end()?
        // Done with second person
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
    Ok(())
}

#[test]
fn list_vec_empty() -> Result<(), IPanic> {
    let hv = Partial::alloc::<Vec<String>>()?
        .begin_list()?
        // Don't push any elements
        .build()?;
    let vec: &Vec<String> = hv.as_ref();
    assert_eq!(vec, &Vec::<String>::new());
    Ok(())
}

#[test]
fn list_vec_nested() -> Result<(), IPanic> {
    let hv = Partial::alloc::<Vec<Vec<i32>>>()?
        .begin_list()?
        // Push first inner vec
        .begin_list_item()?
        .begin_list()?
        .push(1)?
        .push(2)?
        .end()?
        // Done with first inner vec
        // Push second inner vec
        .begin_list_item()?
        .begin_list()?
        .push(3)?
        .push(4)?
        .push(5)?
        .end()?
        // Done with second inner vec
        .build()?;
    let vec: &Vec<Vec<i32>> = hv.as_ref();
    assert_eq!(vec, &vec![vec![1, 2], vec![3, 4, 5]]);
    Ok(())
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
fn map_hashmap_simple() -> Result<(), IPanic> {
    use std::collections::HashMap;

    let hv = Partial::alloc::<HashMap<String, i32>>()?
        .begin_map()?
        // Insert first pair: "foo" -> 42
        .begin_key()?
        .set("foo".to_string())?
        .end()?
        .begin_value()?
        .set(42)?
        .end()?
        // Insert second pair: "bar" -> 123
        .begin_key()?
        .set("bar".to_string())?
        .end()?
        .begin_value()?
        .set(123)?
        .end()?
        .build()?;
    let map: &HashMap<String, i32> = hv.as_ref();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("foo"), Some(&42));
    assert_eq!(map.get("bar"), Some(&123));
    Ok(())
}

#[test]
fn map_hashmap_empty() -> Result<(), IPanic> {
    use std::collections::HashMap;

    let hv = Partial::alloc::<HashMap<String, String>>()?
        .begin_map()?
        // Don't insert any pairs
        .build()?;
    let map: &HashMap<String, String> = hv.as_ref();
    assert_eq!(map.len(), 0);
    Ok(())
}

#[test]
fn map_hashmap_complex_values() -> Result<(), IPanic> {
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    let hv = Partial::alloc::<HashMap<String, Person>>()?
        .begin_map()?
        // Insert "alice" -> Person { name: "Alice", age: 30 }
        .set_key("alice".to_string())?
        .begin_value()?
        .set_field("name", "Alice".to_string())?
        .set_field("age", 30u32)?
        .end()?
        // Done with value
        // Insert "bob" -> Person { name: "Bob", age: 25 }
        .set_key("bob".to_string())?
        .begin_value()?
        .set_field("name", "Bob".to_string())?
        .set_field("age", 25u32)?
        .end()?
        // Done with value
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

    // Test Dog variant
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

    // Test Cat variant
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

    // Test Bird variant
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

    // Test invalid variant name
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
fn map_partial_initialization_drop() -> Result<(), IPanic> {
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
        let mut partial = Partial::alloc::<HashMap<String, DropTracker>>()?;
        partial
            .begin_map()?
            // Insert a complete pair
            .begin_key()?
            .set("first".to_string())?
            .end()?
            .begin_value()?
            .set(DropTracker { id: 1 })?
            .end()?
            // Start inserting another pair but only complete the key
            .begin_key()?
            .set("second".to_string())?
            .end()?;
        // Don't set_value - leave incomplete

        // Drop the partial - should clean up properly
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "Should drop the one inserted value"
    );
    Ok(())
}

#[test]
fn tuple_basic() -> Result<(), IPanic> {
    // Test building a simple tuple
    let boxed = Partial::alloc::<(i32, String)>()?
        .set_nth_field(0, 42i32)?
        .set_nth_field(1, "hello".to_string())?
        .build()?;
    assert_eq!(*boxed, (42, "hello".to_string()));
    Ok(())
}

#[test]
fn tuple_mixed_types() -> Result<(), IPanic> {
    // Test building a tuple with more diverse types
    let boxed = Partial::alloc::<(u8, bool, f64, String)>()?
        // Set fields in non-sequential order to test flexibility
        .set_nth_field(2, 56.124f64)?
        .set_nth_field(0, 255u8)?
        .set_nth_field(3, "world".to_string())?
        .set_nth_field(1, true)?
        .build()?;
    assert_eq!(*boxed, (255u8, true, 56.124f64, "world".to_string()));
    Ok(())
}

#[test]
fn tuple_nested() -> Result<(), IPanic> {
    // Test nested tuples
    let boxed = Partial::alloc::<((i32, i32), String)>()?
        // Build the nested tuple first
        .begin_nth_field(0)?
        .set_nth_field(0, 1i32)?
        .set_nth_field(1, 2i32)?
        .end()?
        // Pop out of the nested tuple
        // Now set the string
        .set_nth_field(1, "nested".to_string())?
        .build()?;
    assert_eq!(*boxed, ((1, 2), "nested".to_string()));
    Ok(())
}

#[test]
fn tuple_empty() -> Result<(), IPanic> {
    // Test empty tuple (unit type)
    let boxed = Partial::alloc::<()>()?.set(())?.build()?;
    assert_eq!(*boxed, ());
    Ok(())
}
