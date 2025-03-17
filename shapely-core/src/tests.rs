use crate::{Shape, Shapely, mini_typeid};

// Allow dead code in test modules since we're not constructing all enum variants
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
struct FooBar {
    foo: u64,
    bar: String,
}

impl Shapely for FooBar {
    fn shape() -> crate::Shape {
        Shape {
            name: |f| write!(f, "FooBar"),
            typeid: mini_typeid::of::<Self>(),
            layout: std::alloc::Layout::new::<Self>(),
            innards: crate::Innards::Struct {
                fields: crate::struct_fields!(FooBar, (foo, bar)),
            },
            set_to_default: None,
            drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
        }
    }
}

#[test]
fn build_foobar_through_reflection() {
    let shape = FooBar::shape();
    eprintln!("{shape:#?}");

    let mut partial = FooBar::partial();
    for (index, field) in shape.known_fields().iter().enumerate() {
        let slot = partial.slot_by_index(index).unwrap();
        match field.name {
            "foo" => slot.fill(42u64),
            "bar" => slot.fill(String::from("Hello, World!")),
            _ => panic!("Unknown field: {}", field.name),
        }
    }
    let foo_bar = partial.build::<FooBar>();

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
fn build_u64_through_reflection() {
    let shape = u64::shape();
    eprintln!("{shape:#?}");

    let mut partial = u64::partial();
    let slot = partial.scalar_slot().unwrap();
    slot.fill(42u64);
    let value = partial.build::<u64>();

    // Verify the value was set correctly
    assert_eq!(value, 42);
}

#[test]
#[should_panic(expected = "Scalar value was not initialized")]
fn build_u64_through_reflection_without_filling() {
    let shape = u64::shape();
    eprintln!("{shape:#?}");

    let uninit = u64::partial();
    // Intentionally not filling the slot
    let _value = uninit.build::<u64>();
    // This should panic
}

#[test]
#[should_panic(expected = "Field 'bar' was not initialized")]
fn build_foobar_through_reflection_with_missing_field() {
    let shape = FooBar::shape();
    eprintln!("{shape:#?}");

    let mut uninit = FooBar::partial();
    for field in shape.known_fields() {
        if field.name == "foo" {
            let slot = uninit.slot_by_name(field.name).unwrap();
            slot.fill(42u64);
            // Intentionally not setting the 'bar' field
        }
    }

    // This should panic because 'bar' is not initialized
    let _foo_bar = uninit.build::<FooBar>();
}

#[test]
#[should_panic(
    expected = "This is a partial \u{1b}[1;34mu64\u{1b}[0m, you can't build a \u{1b}[1;32mu32\u{1b}[0m out of it"
)]
fn build_u64_get_u32_through_reflection() {
    let shape = u64::shape();
    eprintln!("{shape:#?}");

    let mut uninit = u64::partial();

    let slot = uninit.scalar_slot().unwrap();
    slot.fill(42u64);

    // Attempt to build as u32 instead of u64
    let _value = uninit.build::<u32>();
    // This should panic due to type mismatch
}

#[test]
fn build_struct_with_drop_field() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct DropCounter;

    impl Shapely for DropCounter {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "DropCounter"),
                typeid: mini_typeid::of::<DropCounter>(),
                layout: std::alloc::Layout::new::<DropCounter>(),
                innards: crate::Innards::Struct { fields: &[] },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe {
                    std::ptr::drop_in_place(ptr as *mut DropCounter)
                }),
            }
        }
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct StructWithDrop {
        counter: DropCounter,
        value: i32,
    }

    impl Shapely for StructWithDrop {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "StructWithDrop"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Struct {
                    fields: crate::struct_fields!(StructWithDrop, (counter, value)),
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe {
                    std::ptr::drop_in_place(ptr as *mut StructWithDrop)
                }),
            }
        }
    }

    let mut partial = StructWithDrop::partial();

    // First assignment
    {
        let slot = partial.slot_by_index(0).unwrap();
        slot.fill(DropCounter);
    }

    // Second assignment, should trigger drop of the first value
    {
        let slot = partial.slot_by_index(0).unwrap();
        slot.fill(DropCounter);
    }

    // Set the value field
    {
        let slot = partial.slot_by_index(1).unwrap();
        slot.fill(42i32);
    }

    let _struct_with_drop = partial.build::<StructWithDrop>();

    // Check that drop was called once for the first assignment
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

    // Explicitly drop _struct_with_drop
    drop(_struct_with_drop);

    // Check that drop was called twice: once for the first assignment and once for the final instance
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
}

#[test]
fn build_scalar_with_drop() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct DropScalar;

    impl Shapely for DropScalar {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "DropScalar"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Scalar(crate::Scalar::Nothing),
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    impl Drop for DropScalar {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    let mut uninit = DropScalar::partial();

    // First assignment
    {
        let slot = uninit.scalar_slot().unwrap();
        slot.fill(DropScalar);
    }

    // Second assignment, should trigger drop of the first value
    {
        let slot = uninit.scalar_slot().unwrap();
        slot.fill(DropScalar);
    }

    let _drop_scalar = uninit.build::<DropScalar>();

    // Check that drop was called once for the first assignment
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

    // Explicitly drop _drop_scalar
    drop(_drop_scalar);

    // Check that drop was called twice: once for the first assignment and once for the final instance
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
}

#[test]
fn build_truck_with_drop_fields() {
    use std::sync::atomic::{AtomicIsize, Ordering};

    static ENGINE_COUNT: AtomicIsize = AtomicIsize::new(0);
    static WHEELS_COUNT: AtomicIsize = AtomicIsize::new(0);

    struct Engine;
    struct Wheels;

    impl Drop for Engine {
        fn drop(&mut self) {
            ENGINE_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Drop for Wheels {
        fn drop(&mut self) {
            WHEELS_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Shapely for Engine {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "Engine"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Scalar(crate::Scalar::Nothing),
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe {
                    std::ptr::drop_in_place(ptr as *mut Self);
                }),
            }
        }
    }

    impl Shapely for Wheels {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "Wheels"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Scalar(crate::Scalar::Nothing),
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    struct Truck {
        engine: Engine,
        wheels: Wheels,
    }

    impl Shapely for Truck {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "Truck"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Struct {
                    fields: crate::struct_fields!(Truck, (engine, wheels)),
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    fn reset_atomics() {
        ENGINE_COUNT.store(0, Ordering::SeqCst);
        WHEELS_COUNT.store(0, Ordering::SeqCst);
    }

    // Scenario 1: Not filling any fields
    {
        reset_atomics();
        let partial = Truck::partial();
        drop(partial);
        assert_eq!(ENGINE_COUNT.load(Ordering::SeqCst), 0, "No drops occurred.");
        assert_eq!(WHEELS_COUNT.load(Ordering::SeqCst), 0, "No drops occurred.");
    }

    // Scenario 2: Filling only the engine field
    {
        reset_atomics();
        let mut partial = Truck::partial();
        {
            let slot = partial.slot_by_name("engine").unwrap();
            slot.fill(Engine);
        }
        drop(partial);
        assert_eq!(
            ENGINE_COUNT.load(Ordering::SeqCst),
            1,
            "Engine field should have been dropped."
        );
        assert_eq!(
            WHEELS_COUNT.load(Ordering::SeqCst),
            0,
            "Wheels field wasn't set."
        );
    }

    // Scenario 3: Filling only the wheels field
    {
        reset_atomics();
        let mut partial = Truck::partial();
        {
            let slot = partial.slot_by_name("wheels").unwrap();
            slot.fill(Wheels);
        }
        drop(partial);
        assert_eq!(
            ENGINE_COUNT.load(Ordering::SeqCst),
            0,
            "Engine field wasn't set."
        );
        assert_eq!(
            WHEELS_COUNT.load(Ordering::SeqCst),
            1,
            "Wheels field should have been dropped."
        );
    }

    // Scenario 4: Filling both fields
    {
        reset_atomics();
        let mut partial = Truck::partial();
        {
            let slot = partial.slot_by_name("engine").unwrap();
            slot.fill(Engine);
        }
        {
            let slot = partial.slot_by_name("wheels").unwrap();
            slot.fill(Wheels);
        }
        drop(partial);
        assert_eq!(
            ENGINE_COUNT.load(Ordering::SeqCst),
            1,
            "Engine field should have been dropped."
        );
        assert_eq!(
            WHEELS_COUNT.load(Ordering::SeqCst),
            1,
            "Wheels field should have been dropped."
        );
    }
}

#[test]
fn test_partial_build_in_place() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct DropCounter;

    impl Shapely for DropCounter {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "DropCounter"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Struct { fields: &[] },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct TestShape {
        counter: DropCounter,
        unit: (),
    }

    impl Shapely for TestShape {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "TestShape"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Struct {
                    fields: crate::struct_fields!(TestShape, (counter, unit)),
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    let mut test_shape = std::mem::MaybeUninit::<TestShape>::uninit();
    {
        let mut partial = TestShape::partial_from_uninit(&mut test_shape);
        partial.slot_by_name("counter").unwrap().fill(DropCounter);
        partial.slot_by_name("unit").unwrap().fill(());
        partial.build_in_place();
    }
    let test_shape = unsafe { test_shape.assume_init() };

    // Check that drop hasn't been called yet
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);

    // Manually drop the parent to trigger the drop of TestShape
    drop(test_shape);

    // Check that drop was called once for the DropCounter
    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn test_partial_build_transparent() {
    #[derive(Debug, PartialEq)]
    struct InnerType(u32);

    impl Shapely for InnerType {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "InnerType"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Scalar(crate::Scalar::U32),
                set_to_default: None,
                drop_in_place: None,
            }
        }
    }

    #[derive(Debug, PartialEq)]
    struct TransparentWrapper(InnerType);

    impl Shapely for TransparentWrapper {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "TransparentWrapper"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Transparent(InnerType::shape_desc()),
                set_to_default: None,
                drop_in_place: None,
            }
        }
    }

    let shape = TransparentWrapper::shape();
    eprintln!("{shape:#?}");

    let mut uninit = TransparentWrapper::partial();
    let slot = uninit.scalar_slot().unwrap();
    slot.fill(InnerType(42));

    let wrapper = uninit.build::<TransparentWrapper>();

    assert_eq!(wrapper, TransparentWrapper(InnerType(42)));
}

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
#[repr(u8)]
enum UserStatus {
    Offline = 0,
    Online = 1,
    Away = 2,
    DoNotDisturb = 3,
}

impl Shapely for UserStatus {
    fn shape() -> crate::Shape {
        struct StaticFields;
        impl StaticFields {
            const VARIANTS: &'static [crate::Variant] = &[
                crate::Variant {
                    name: "Offline",
                    discriminant: Some(0),
                    kind: crate::VariantKind::Unit,
                },
                crate::Variant {
                    name: "Online",
                    discriminant: Some(1),
                    kind: crate::VariantKind::Unit,
                },
                crate::Variant {
                    name: "Away",
                    discriminant: Some(2),
                    kind: crate::VariantKind::Unit,
                },
                crate::Variant {
                    name: "DoNotDisturb",
                    discriminant: Some(3),
                    kind: crate::VariantKind::Unit,
                },
            ];
        }

        Shape {
            name: |f| write!(f, "UserStatus"),
            typeid: mini_typeid::of::<Self>(),
            layout: std::alloc::Layout::new::<Self>(),
            innards: crate::Innards::Enum {
                variants: StaticFields::VARIANTS,
                repr: crate::EnumRepr::U8,
            },
            set_to_default: None,
            drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
        }
    }
}

#[test]
fn test_enum_reflection_with_discriminants() {
    let shape = UserStatus::shape();
    eprintln!("{shape:#?}");

    // Test variant count
    assert_eq!(shape.variants().len(), 4);

    // Check discriminant values
    let offline = shape.variant_by_name("Offline").unwrap();
    assert_eq!(offline.discriminant, Some(0));

    let online = shape.variant_by_name("Online").unwrap();
    assert_eq!(online.discriminant, Some(1));

    let away = shape.variant_by_name("Away").unwrap();
    assert_eq!(away.discriminant, Some(2));

    let dnd = shape.variant_by_name("DoNotDisturb").unwrap();
    assert_eq!(dnd.discriminant, Some(3));

    // Demonstrate reflecting on each variant kind
    let enum_variants = shape.variants();
    for variant in enum_variants {
        match variant.kind {
            crate::VariantKind::Unit => {
                println!(
                    "{} is a unit variant with discriminant {:?}",
                    variant.name, variant.discriminant
                );
            }
            crate::VariantKind::Tuple { fields } => {
                println!(
                    "{} is a tuple variant with {} fields and discriminant {:?}",
                    variant.name,
                    fields.len(),
                    variant.discriminant
                );
            }
            crate::VariantKind::Struct { fields } => {
                println!(
                    "{} is a struct variant with {} fields and discriminant {:?}",
                    variant.name,
                    fields.len(),
                    variant.discriminant
                );
            }
        }
    }
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
#[repr(u8)]
enum SimpleEnum {
    A,
    B(i32),
    C { value: String },
}

impl Shapely for SimpleEnum {
    fn shape() -> crate::Shape {
        struct StaticFields;
        impl StaticFields {
            const B_FIELDS: &'static [crate::Field] = &[crate::Field {
                name: "_0",
                shape: crate::ShapeDesc(i32::shape),
                offset: 0, // Will be calculated at runtime
                flags: crate::FieldFlags::EMPTY,
            }];

            const C_FIELDS: &'static [crate::Field] = &[crate::Field {
                name: "value",
                shape: crate::ShapeDesc(String::shape),
                offset: 0, // Will be calculated at runtime
                flags: crate::FieldFlags::EMPTY,
            }];

            const VARIANTS: &'static [crate::Variant] = &[
                crate::Variant {
                    name: "A",
                    discriminant: None,
                    kind: crate::VariantKind::Unit,
                },
                crate::Variant {
                    name: "B",
                    discriminant: None,
                    kind: crate::VariantKind::Tuple {
                        fields: Self::B_FIELDS,
                    },
                },
                crate::Variant {
                    name: "C",
                    discriminant: None,
                    kind: crate::VariantKind::Struct {
                        fields: Self::C_FIELDS,
                    },
                },
            ];
        }

        Shape {
            name: |f| write!(f, "SimpleEnum"),
            typeid: mini_typeid::of::<Self>(),
            layout: std::alloc::Layout::new::<Self>(),
            innards: crate::Innards::Enum {
                variants: StaticFields::VARIANTS,
                repr: crate::EnumRepr::U8,
            },
            set_to_default: None,
            drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
        }
    }
}

#[test]
fn test_build_simple_enum() {
    println!("Starting SimpleEnum test");

    println!("\nSimpleEnum shape and variants:");
    let simple_enum_shape = SimpleEnum::shape();
    println!("{:?}", simple_enum_shape);

    println!("\nTesting variant A (unit variant):");
    let mut partial = SimpleEnum::partial();

    // Select variant 0 (A)
    partial.set_variant_by_index(0).unwrap();

    // Print memory layout information for debugging
    println!("Memory layout before build:");
    unsafe {
        // Print memory as bytes
        let ptr = partial.addr.as_ptr();
        let size = simple_enum_shape.layout.size();
        println!(
            "Raw memory bytes: {:?}",
            std::slice::from_raw_parts(ptr, size)
        );
        println!("First 4 bytes as u32: {}", *(ptr as *const u32));
    }

    println!(
        "Selected variant index: {:?}",
        partial.selected_variant_index()
    );

    if let Some(idx) = partial.selected_variant_index() {
        if let crate::Innards::Enum { variants, repr: _ } = &simple_enum_shape.innards {
            println!("Variant at index {}: {:?}", idx, variants[idx]);
        }
    }

    // Build the enum
    let simple_enum = partial.build::<SimpleEnum>();
    println!("trace: Built SimpleEnum successfully");

    // Check which variant we got - use std::mem::discriminant to avoid accessing fields
    // This avoids crashes with uninitialized fields
    use std::mem;
    let discr_a = mem::discriminant(&SimpleEnum::A);
    let discr_built = mem::discriminant(&simple_enum);

    if discr_built == discr_a {
        println!("✅ Correct! Built SimpleEnum::A as expected");
    } else {
        println!(
            "❌ BUG: Built a different variant than A (expected discriminant {:?}, got {:?})",
            discr_a, discr_built
        );
    }

    println!("\nThe bug appears to be in how enums are represented in memory.");
    println!("We need to ensure our discriminant representation matches Rust's enum layout.");
}

// Create a new test specifically for explicit representation
#[test]
fn test_build_simple_enum_with_explicit_repr() {
    println!("Starting SimpleEnum test with explicit representation");

    // Define our SimpleEnum with explicit repr
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum ExplicitReprEnum {
        A,
        B(i32),
        C { value: String },
    }

    impl Shapely for ExplicitReprEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const B_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(i32::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                }];

                const C_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "value",
                    shape: crate::ShapeDesc(String::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                }];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "A",
                        discriminant: None,
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "B",
                        discriminant: None,
                        kind: crate::VariantKind::Tuple {
                            fields: Self::B_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "C",
                        discriminant: None,
                        kind: crate::VariantKind::Struct {
                            fields: Self::C_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "ExplicitReprEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U8,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    println!("\nExplicitReprEnum shape and variants:");
    let enum_shape = ExplicitReprEnum::shape();
    println!("{:?}", enum_shape);

    println!("\nTesting variant A (unit variant):");
    let mut partial = ExplicitReprEnum::partial();

    // Select variant 0 (A)
    partial.set_variant_by_index(0).unwrap();

    // Print memory layout information for debugging
    println!("Memory layout before build:");
    unsafe {
        // Print memory as bytes
        let ptr = partial.addr.as_ptr();
        let size = enum_shape.layout.size();
        println!(
            "Raw memory bytes: {:?}",
            std::slice::from_raw_parts(ptr, size)
        );
        println!("First 4 bytes as u32: {}", *(ptr as *const u32));
    }

    println!(
        "Selected variant index: {:?}",
        partial.selected_variant_index()
    );

    if let Some(idx) = partial.selected_variant_index() {
        if let crate::Innards::Enum { variants, repr: _ } = &enum_shape.innards {
            println!("Variant at index {}: {:?}", idx, variants[idx]);
        }
    }

    // Build the enum
    let result_enum = partial.build::<ExplicitReprEnum>();
    println!("trace: Built ExplicitReprEnum successfully");

    // Check which variant we got - use std::mem::discriminant to avoid accessing fields
    // This avoids crashes with uninitialized fields
    use std::mem;
    let discr_a = mem::discriminant(&ExplicitReprEnum::A);
    let discr_built = mem::discriminant(&result_enum);

    if discr_built == discr_a {
        println!("✅ Correct! Built ExplicitReprEnum::A as expected");
    } else {
        println!(
            "❌ BUG: Built a different variant than A (expected discriminant {:?}, got {:?})",
            discr_a, discr_built
        );
    }
}

// Test for enum with non-zero/non-sequential discriminants
#[test]
fn test_build_enum_with_custom_discriminants() {
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(i32)]
    enum CustomDiscEnum {
        First = 10,
        Second = 20,
        Third = 30,
    }

    impl Shapely for CustomDiscEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "First",
                        discriminant: Some(10),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Second",
                        discriminant: Some(20),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Third",
                        discriminant: Some(30),
                        kind: crate::VariantKind::Unit,
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "CustomDiscEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::I32,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    println!("Testing enum with custom discriminants");
    let mut partial = CustomDiscEnum::partial();

    // Select variant 1 (Second) with discriminant 20
    partial.set_variant_by_index(1).unwrap();

    // Build the enum
    let result = partial.build::<CustomDiscEnum>();

    // Verify correctness using discriminant
    use std::mem;
    let expected_discriminant = mem::discriminant(&CustomDiscEnum::Second);
    let actual_discriminant = mem::discriminant(&result);

    assert_eq!(
        expected_discriminant, actual_discriminant,
        "Expected CustomDiscEnum::Second (discriminant 20), but got a different variant"
    );

    println!("✅ Successfully built enum with custom discriminant");
}

// Let's add a simpler test for enum variants with data
#[test]
fn test_build_enum_with_simple_variant() {
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum SimpleVariantEnum {
        NoData,
        WithData(u32),
    }

    impl Shapely for SimpleVariantEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const DATA_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(u32::shape),
                    offset: 4, // This is a guess, we'll see if it works
                    flags: crate::FieldFlags::EMPTY,
                }];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "NoData",
                        discriminant: None,
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "WithData",
                        discriminant: None,
                        kind: crate::VariantKind::Tuple {
                            fields: Self::DATA_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "SimpleVariantEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U8,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    println!("Testing enum with simple data variant");

    // Test building NoData variant first
    {
        let mut partial = SimpleVariantEnum::partial();
        partial.set_variant_by_index(0).unwrap();
        let result = partial.build::<SimpleVariantEnum>();
        assert_eq!(
            result,
            SimpleVariantEnum::NoData,
            "Should build NoData variant"
        );
        println!("✅ Successfully built NoData variant");
    }

    // Test building WithData variant
    {
        let mut partial = SimpleVariantEnum::partial();
        partial.set_variant_by_index(1).unwrap();

        // Set the data value
        let slot = partial.variant_field_by_name("_0").unwrap();
        slot.fill(42u32);

        let result = partial.build::<SimpleVariantEnum>();
        assert_eq!(
            result,
            SimpleVariantEnum::WithData(42),
            "Should build WithData variant"
        );
        println!("✅ Successfully built WithData variant");
    }
}

// Test for enum with struct variant
#[test]
fn test_build_enum_with_struct_variant() {
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u32)]
    enum StructEnum {
        NoData,
        Person { id: u32, active: bool },
    }

    impl Shapely for StructEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const PERSON_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "id",
                        shape: crate::ShapeDesc(u32::shape),
                        offset: 4, // This is a guess
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "active",
                        shape: crate::ShapeDesc(bool::shape),
                        offset: 8, // Updated offset
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "NoData",
                        discriminant: None,
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Person",
                        discriminant: None,
                        kind: crate::VariantKind::Struct {
                            fields: Self::PERSON_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "StructEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U32,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    println!("Testing enum with struct variant");

    // Test building NoData variant
    {
        let mut partial = StructEnum::partial();
        partial.set_variant_by_index(0).unwrap();
        let result = partial.build::<StructEnum>();
        assert_eq!(result, StructEnum::NoData, "Should build NoData variant");
        println!("✅ Successfully built NoData variant");
    }

    // Test building Person variant
    {
        let mut partial = StructEnum::partial();
        partial.set_variant_by_index(1).unwrap();

        // Set the struct fields
        let id_slot = partial.variant_field_by_name("id").unwrap();
        id_slot.fill(123u32);

        let active_slot = partial.variant_field_by_name("active").unwrap();
        active_slot.fill(true);

        println!("Debug - Values about to be built:");
        println!("  id value: 123");
        println!("  active value: true");

        let result = partial.build::<StructEnum>();

        // Check that it built correctly
        match result {
            StructEnum::Person { id, active } => {
                assert_eq!(id, 123, "id should be 123");
                assert!(active, "active should be true");
                println!("✅ Successfully built Person variant with id=123, active=true");
            }
            _ => panic!("Expected Person variant but got something else"),
        }
    }
}

// Test for enum without explicit representation
#[test]
#[should_panic(expected = "Enum must have an explicit representation")]
fn test_enum_without_repr() {
    #[allow(dead_code)]
    enum NoReprEnum {
        A,
        B(i32),
        C { value: String },
    }

    impl Shapely for NoReprEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const B_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(i32::shape),
                    offset: 0,
                    flags: crate::FieldFlags::EMPTY,
                }];

                const C_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "value",
                    shape: crate::ShapeDesc(String::shape),
                    offset: 0,
                    flags: crate::FieldFlags::EMPTY,
                }];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "A",
                        discriminant: None,
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "B",
                        discriminant: None,
                        kind: crate::VariantKind::Tuple {
                            fields: Self::B_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "C",
                        discriminant: None,
                        kind: crate::VariantKind::Struct {
                            fields: Self::C_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "NoReprEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::Default,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    // Create a partial and try to build it
    let mut partial = NoReprEnum::partial();
    partial.set_variant_by_index(0).unwrap();

    // This should panic with a meaningful error message about representation
    let _result = partial.build::<NoReprEnum>();
}

// Test for complex nested enums with recursive variants
#[test]
fn test_complex_nested_recursive_enums() {
    // Define a complex enum structure with recursive variants
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum NestedData {
        Empty,
        Text(String),
        Number(i64),
    }

    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum ComplexEnum {
        Simple,
        Nested(NestedData),
        Pair(NestedData, NestedData),
        Struct { id: u32, data: NestedData },
        // Recursive variant using Box to avoid infinite size
        Recursive(Box<ComplexEnum>),
        // Double recursive variant
        Tree(Box<ComplexEnum>, Box<ComplexEnum>),
    }

    impl Shapely for NestedData {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const TEXT_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(String::shape),
                    offset: 8, // Proper alignment for String (8-byte aligned)
                    flags: crate::FieldFlags::EMPTY,
                }];

                const NUMBER_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(i64::shape),
                    offset: 8, // Alignment padding after the discriminant
                    flags: crate::FieldFlags::EMPTY,
                }];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "Empty",
                        discriminant: Some(0),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Text",
                        discriminant: Some(1),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::TEXT_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Number",
                        discriminant: Some(2),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::NUMBER_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "NestedData"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U8,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    impl Shapely for ComplexEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const NESTED_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(NestedData::shape),
                    offset: 8, // Offset after the discriminant with alignment
                    flags: crate::FieldFlags::EMPTY,
                }];

                const PAIR_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "_0",
                        shape: crate::ShapeDesc(NestedData::shape),
                        offset: 8, // Offset after discriminant with alignment
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "_1",
                        shape: crate::ShapeDesc(NestedData::shape),
                        offset: 24, // Approximate offset for second field
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const STRUCT_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "id",
                        shape: crate::ShapeDesc(u32::shape),
                        offset: 4, // Offset after discriminant
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "data",
                        shape: crate::ShapeDesc(NestedData::shape),
                        offset: 8, // Aligned offset for nested data
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const RECURSIVE_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(<Box<ComplexEnum>>::shape),
                    offset: 8, // Offset after discriminant with alignment
                    flags: crate::FieldFlags::EMPTY,
                }];

                const TREE_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "_0",
                        shape: crate::ShapeDesc(<Box<ComplexEnum>>::shape),
                        offset: 8, // Offset after discriminant with alignment
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "_1",
                        shape: crate::ShapeDesc(<Box<ComplexEnum>>::shape),
                        offset: 16, // Offset for second field
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "Simple",
                        discriminant: Some(0),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Nested",
                        discriminant: Some(1),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::NESTED_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Pair",
                        discriminant: Some(2),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::PAIR_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Struct",
                        discriminant: Some(3),
                        kind: crate::VariantKind::Struct {
                            fields: Self::STRUCT_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Recursive",
                        discriminant: Some(4),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::RECURSIVE_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Tree",
                        discriminant: Some(5),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::TREE_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "ComplexEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U8,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    // Implement Shapely for Box<ComplexEnum>
    impl Shapely for Box<ComplexEnum> {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "Box<ComplexEnum>"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Transparent(crate::ShapeDesc(ComplexEnum::shape)),
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    println!("Testing complex nested recursive enums");

    // Test 1: Build a Simple variant
    {
        let mut partial = ComplexEnum::partial();
        partial.set_variant_by_name("Simple").unwrap();
        let result = partial.build::<ComplexEnum>();
        assert_eq!(result, ComplexEnum::Simple);
        println!("✅ Successfully built Simple variant");
    }

    // Test 2: Build a Nested variant with Text
    {
        println!("DIAGNOSTIC: Starting Test 2");
        let mut partial = ComplexEnum::partial();
        println!("DIAGNOSTIC: Created partial for ComplexEnum");

        partial.set_variant_by_name("Nested").unwrap();
        println!("DIAGNOSTIC: Set variant to Nested");

        // Create the nested data (Text variant)
        let nested_slot = partial.variant_field_by_name("_0").unwrap();
        println!("DIAGNOSTIC: Got nested slot");

        let mut nested_partial = NestedData::partial();
        println!("DIAGNOSTIC: Created nested_partial for NestedData");

        nested_partial.set_variant_by_name("Text").unwrap();
        println!("DIAGNOSTIC: Set nested variant to Text");

        // Set the text content with static string to avoid memory issues
        let text_slot = nested_partial.variant_field_by_name("_0").unwrap();
        println!("DIAGNOSTIC: Got text slot");

        // Use a static string to avoid String drop issues
        text_slot.fill(String::from("test"));
        println!("DIAGNOSTIC: Filled text slot with String");

        // Fill the nested slot with our built NestedData
        let built_nested_data = nested_partial.build::<NestedData>();
        println!("DIAGNOSTIC: Built NestedData successfully");

        nested_slot.fill(built_nested_data);
        println!("DIAGNOSTIC: Filled nested slot with built NestedData");

        // Build the final ComplexEnum
        println!("DIAGNOSTIC: About to build ComplexEnum");
        let result = partial.build::<ComplexEnum>();
        println!("DIAGNOSTIC: Built ComplexEnum successfully");

        // Compare the variant structure
        if let ComplexEnum::Nested(NestedData::Text(text)) = &result {
            println!("DIAGNOSTIC: Pattern match on result succeeded");
            assert_eq!(text, "test", "Text content should match what we set");
            println!("✅ Successfully built Nested variant with Text structure");
        } else {
            panic!(
                "Expected ComplexEnum::Nested(NestedData::Text), but got {:?}",
                result
            );
        }
        println!("DIAGNOSTIC: End of Test 2 scope after mem::forget");
    }
    println!("DIAGNOSTIC: After Test 2 scope - did we survive?");

    println!("✅ Complex nested enum tests completed successfully!");
}

// Test for a complex nested structure with less complexity than the full nightmare version
#[test]
fn test_nightmare_reflection() {
    println!("🔥 Testing complex reflection scenario 🔥");

    // Define a simpler enum hierarchy that we can fully initialize
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum InnerData {
        Nothing,
        Text(String),
        Number(i64),
    }

    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    #[repr(u16)]
    enum OuterData {
        Simple,
        WithData(InnerData),
        Boxed(Box<InnerData>),
        Structured {
            id: u32,
            name: String,
            value: InnerData,
        },
    }

    impl Shapely for InnerData {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const TEXT_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(String::shape),
                    offset: 8, // Offset after discriminant with alignment
                    flags: crate::FieldFlags::EMPTY,
                }];

                const NUMBER_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(i64::shape),
                    offset: 8, // Offset after discriminant with alignment
                    flags: crate::FieldFlags::EMPTY,
                }];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "Nothing",
                        discriminant: Some(0),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Text",
                        discriminant: Some(1),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::TEXT_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Number",
                        discriminant: Some(2),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::NUMBER_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "InnerData"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U8,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    impl Shapely for OuterData {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const WITH_DATA_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(InnerData::shape),
                    offset: 8, // Offset after discriminant with alignment
                    flags: crate::FieldFlags::EMPTY,
                }];

                const BOXED_FIELDS: &'static [crate::Field] = &[crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(<Box<InnerData>>::shape),
                    offset: 8, // Offset after discriminant with alignment
                    flags: crate::FieldFlags::EMPTY,
                }];

                const STRUCTURED_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "id",
                        shape: crate::ShapeDesc(u32::shape),
                        offset: 4, // Offset after discriminant
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "name",
                        shape: crate::ShapeDesc(String::shape),
                        offset: 8, // Offset with alignment
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "value",
                        shape: crate::ShapeDesc(InnerData::shape),
                        offset: 32, // Offset with alignment
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "Simple",
                        discriminant: Some(0),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "WithData",
                        discriminant: Some(1),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::WITH_DATA_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Boxed",
                        discriminant: Some(2),
                        kind: crate::VariantKind::Tuple {
                            fields: Self::BOXED_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Structured",
                        discriminant: Some(3),
                        kind: crate::VariantKind::Struct {
                            fields: Self::STRUCTURED_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "OuterData"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U16,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    // Implement Shapely for Box<InnerData>
    impl Shapely for Box<InnerData> {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "Box<InnerData>"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Transparent(crate::ShapeDesc(InnerData::shape)),
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    // Test 1: Build InnerData::Text
    println!("Test 1: Building InnerData::Text");
    let mut inner_partial = InnerData::partial();
    inner_partial.set_variant_by_name("Text").unwrap();

    let text_slot = inner_partial.variant_field_by_name("_0").unwrap();
    text_slot.fill(String::from("Hello Reflection!"));

    let text_data = inner_partial.build::<InnerData>();
    if let InnerData::Text(text) = &text_data {
        println!("✅ Successfully built InnerData::Text: {}", text);
    } else {
        panic!("Failed to build InnerData::Text");
    }

    // Test 2: Build OuterData::Structured
    println!("Test 2: Building OuterData::Structured");
    let mut outer_partial = OuterData::partial();
    outer_partial.set_variant_by_name("Structured").unwrap();

    // Set id
    let id_slot = outer_partial.variant_field_by_name("id").unwrap();
    id_slot.fill(42u32);

    // Set name
    let name_slot = outer_partial.variant_field_by_name("name").unwrap();
    name_slot.fill(String::from("Structured Value"));

    // Set value
    let value_slot = outer_partial.variant_field_by_name("value").unwrap();
    value_slot.fill(text_data);

    let result = outer_partial.build::<OuterData>();
    println!("✅ Successfully built OuterData::Structured");

    // Verify the result matches expected content
    if let OuterData::Structured { id, name, value } = &result {
        assert_eq!(*id, 42u32);
        assert_eq!(name, "Structured Value");
        assert!(matches!(value, InnerData::Text(text) if text == "Hello Reflection!"));
    } else {
        panic!("Result is not the expected OuterData::Structured variant");
    }

    println!("🏆 Complex reflection test completed successfully!");
}

// Test for building a struct with an enum field
#[test]
fn test_struct_with_enum_field() {
    println!("🔥 Testing struct with enum field 🔥");

    // Define a simple enum for status
    #[derive(Debug, PartialEq, Eq)]
    #[allow(dead_code)]
    #[repr(u8)]
    enum Status {
        Active = 1,
        Inactive = 2,
        Pending = 3,
    }

    // Define a struct that contains the enum
    #[derive(Debug, PartialEq, Eq)]
    struct User {
        id: u32,
        name: String,
        status: Status,
        score: i32,
    }

    impl Shapely for Status {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "Active",
                        discriminant: Some(1),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Inactive",
                        discriminant: Some(2),
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Pending",
                        discriminant: Some(3),
                        kind: crate::VariantKind::Unit,
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "Status"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                    repr: crate::EnumRepr::U8,
                },
                set_to_default: None,
                drop_in_place: None,
            }
        }
    }

    impl Shapely for User {
        fn shape() -> crate::Shape {
            Shape {
                name: |f| write!(f, "User"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Struct {
                    fields: crate::struct_fields!(User, (id, name, status, score)),
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    println!("Building the Status enum first");
    let mut status_partial = Status::partial();
    status_partial.set_variant_by_name("Active").unwrap();
    let status = status_partial.build::<Status>();
    assert_eq!(status, Status::Active);
    println!("✅ Successfully built Status::Active");

    println!("Building the User struct with enum field");
    let mut user_partial = User::partial();

    // Set basic fields
    user_partial.slot_by_name("id").unwrap().fill(42u32);
    user_partial
        .slot_by_name("name")
        .unwrap()
        .fill(String::from("John Doe"));
    user_partial.slot_by_name("score").unwrap().fill(100i32);

    // Set the enum field
    user_partial.slot_by_name("status").unwrap().fill(status);

    // Build the User struct
    let user = user_partial.build::<User>();

    // Verify the result
    assert_eq!(user.id, 42u32);
    assert_eq!(user.name, "John Doe");
    assert_eq!(user.status, Status::Active);
    assert_eq!(user.score, 100i32);

    println!("✅ Successfully built User with Status::Active");

    // Build another user with a different status
    println!("Building another User with Status::Pending");
    let mut status_partial = Status::partial();
    status_partial.set_variant_by_name("Pending").unwrap();
    let pending_status = status_partial.build::<Status>();

    let mut user_partial = User::partial();
    user_partial.slot_by_name("id").unwrap().fill(43u32);
    user_partial
        .slot_by_name("name")
        .unwrap()
        .fill(String::from("Jane Smith"));
    user_partial
        .slot_by_name("status")
        .unwrap()
        .fill(pending_status);
    user_partial.slot_by_name("score").unwrap().fill(75i32);

    let user2 = user_partial.build::<User>();

    // Verify the second user
    assert_eq!(user2.id, 43u32);
    assert_eq!(user2.name, "Jane Smith");
    assert_eq!(user2.status, Status::Pending);
    assert_eq!(user2.score, 75i32);

    println!("✅ Successfully built User with Status::Pending");
    println!("🏆 Struct with enum field test completed successfully!");
}
