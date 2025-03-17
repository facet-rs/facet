use crate::partial::Partial;
use crate::{Shape, Shapely, mini_typeid};

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
                layout: std::alloc::Layout::new::<StructWithDrop>(),
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
                repr: crate::EnumRepr::Default,
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
                repr: crate::EnumRepr::Default,
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
