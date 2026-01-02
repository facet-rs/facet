use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn wip_option_testleak1() {
    let wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    let _ = wip.build().unwrap();
}

#[test]
fn wip_option_testleak2() {
    let wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    let _wip = wip.build().unwrap();
}

#[test]
fn wip_option_testleak3() {
    let _wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak4() {
    let _wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak5() {
    let _ = Partial::alloc::<Option<String>>().unwrap();
    // Just allocate without setting a value
}

#[test]
fn wip_option_testleak6() {
    let _ = Partial::alloc::<Option<String>>().unwrap();
}

/// Reproduces issue #1568 - use-after-free in Option cleanup
///
/// The bug: when begin_some() allocates memory for the inner value,
/// and then cleanup happens (drop without finishing), the child frame's
/// memory is freed via dealloc(), but the parent frame's deinit() still
/// tries to drop the Option contents that point to the freed memory.
#[test]
fn wip_option_use_after_free_issue_1568() {
    #[derive(Facet, Debug)]
    struct WithOption {
        name: String,
        opt: Option<String>,
    }

    // Allocate and start building a struct
    let mut partial = Partial::alloc::<WithOption>().unwrap();

    // Set the first field
    partial = partial.set_field("name", String::from("test")).unwrap();

    // Begin the option field
    partial = partial.begin_field("opt").unwrap();

    // Call begin_some - this allocates separate memory for the Option's inner
    partial = partial.begin_some().unwrap();

    // Set the inner string value
    partial = partial.set(String::from("hello")).unwrap();

    // Don't call end() - just drop the partial
    // This triggers the use-after-free: the child frame (Some's inner) gets
    // deallocated, but then the parent frame's deinit tries to drop the Option
    // which still thinks it contains Some pointing to the freed memory
    drop(partial);
}

/// Test deeply nested Option where inner struct deserialization fails
/// This mimics what happens in the HTML deserializer when parsing fails mid-structure
#[test]
fn wip_option_nested_struct_partial_init_issue_1568() {
    #[derive(Facet, Debug)]
    struct Inner {
        x: u32,
        y: String,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Option<Inner>,
    }

    // Allocate and start building a struct
    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Begin the option field
    partial = partial.begin_field("inner").unwrap();

    // Call begin_some - this allocates separate memory for Option's inner
    partial = partial.begin_some().unwrap();

    // Partially initialize Inner - set one field but not the other
    partial = partial.set_field("x", 42u32).unwrap();

    // Drop without finishing - simulates what happens when deserialization errors out
    drop(partial);
}

/// Test Option<String> directly with begin_some + partial state
#[test]
fn wip_option_string_begin_some_drop_issue_1568() {
    // Allocate Option<String> directly
    let mut partial = Partial::alloc::<Option<String>>().unwrap();

    // Call begin_some - this allocates separate memory for the inner String
    partial = partial.begin_some().unwrap();

    // Set a string value
    partial = partial.set(String::from("hello world")).unwrap();

    // Drop without calling end() - should not crash
    drop(partial);
}

/// Reproduces issue #1568 with deferred mode + flattened struct with Option fields
/// This mimics what facet-html's deserializer does with GlobalAttrs
#[test]
fn wip_option_deferred_flatten_issue_1568() {
    use facet_reflect::Resolution;
    use std::collections::HashMap;

    #[derive(Facet, Debug, Default)]
    struct Html {
        #[facet(flatten)]
        attrs: GlobalAttrs,
    }

    #[derive(Facet, Debug, Default)]
    struct GlobalAttrs {
        id: Option<String>,
        class: Option<String>,
        #[facet(flatten, default)]
        extra: HashMap<String, String>,
    }

    // Allocate the struct
    let mut partial = Partial::alloc::<Html>().unwrap();

    // Enter deferred mode (like FormatDeserializer does for structs with flatten)
    let resolution = Resolution::new();
    partial = partial.begin_deferred(resolution).unwrap();

    // Navigate to the flattened attrs field
    partial = partial.begin_field("attrs").unwrap();

    // Navigate to an Option<String> field inside the flattened struct
    partial = partial.begin_field("id").unwrap();

    // Start a Some value - this allocates separate memory
    partial = partial.begin_some().unwrap();

    // Set the string value
    partial = partial.set(String::from("my-id")).unwrap();

    // Drop without finishing - should trigger the bug
    drop(partial);
}

/// Found by fuzzer: enum begin_nth_field then drop
#[test]
fn fuzz_enum_begin_field_drop() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)] // Variants are constructed via reflection, not accessed directly
    enum FuzzEnum {
        Unit,
        Tuple(String, u32),
        Struct { name: String, value: Option<i32> },
    }

    // This sequence was found by the fuzzer
    let partial = Partial::alloc::<FuzzEnum>().unwrap();
    let partial = partial.begin_nth_field(1);
    drop(partial);
}

/// Found by fuzzer: map key leak when parse_from_str is called on key
/// Minimized case: BeginField(Mapping) -> BeginMap -> BeginKey -> ParseFromStr -> ParseFromStr
#[test]
fn fuzz_map_key_leak_minimized() {
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct FuzzTarget {
        mapping: HashMap<String, u32>,
    }

    // This EXACT sequence was found by the fuzzer - TWO parse_from_str calls!
    let partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = partial.begin_field("mapping").unwrap();
    let partial = partial.begin_map().unwrap();
    let partial = partial.begin_key().unwrap();
    // First parse - initializes the key to "appppvpejv"
    let partial = partial.parse_from_str("appppvpejv").unwrap();
    // Second parse - this drops the old string and parses "pv"
    // The leak happens here!
    let _ = partial.parse_from_str("pv");
}
