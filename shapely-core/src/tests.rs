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
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(u8, u8, u8),
}

impl Shapely for Message {
    fn shape() -> crate::Shape {
        struct StaticFields;
        impl StaticFields {
            const MOVE_FIELDS: &'static [crate::Field] = &[
                crate::Field {
                    name: "x",
                    shape: crate::ShapeDesc(i32::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                },
                crate::Field {
                    name: "y",
                    shape: crate::ShapeDesc(i32::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                },
            ];

            const WRITE_FIELDS: &'static [crate::Field] = &[crate::Field {
                name: "_0",
                shape: crate::ShapeDesc(String::shape),
                offset: 0, // Will be calculated at runtime
                flags: crate::FieldFlags::EMPTY,
            }];

            const CHANGE_COLOR_FIELDS: &'static [crate::Field] = &[
                crate::Field {
                    name: "_0",
                    shape: crate::ShapeDesc(u8::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                },
                crate::Field {
                    name: "_1",
                    shape: crate::ShapeDesc(u8::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                },
                crate::Field {
                    name: "_2",
                    shape: crate::ShapeDesc(u8::shape),
                    offset: 0, // Will be calculated at runtime
                    flags: crate::FieldFlags::EMPTY,
                },
            ];

            const VARIANTS: &'static [crate::Variant] = &[
                crate::Variant {
                    name: "Quit",
                    discriminant: None,
                    kind: crate::VariantKind::Unit,
                },
                crate::Variant {
                    name: "Move",
                    discriminant: None,
                    kind: crate::VariantKind::Struct {
                        fields: Self::MOVE_FIELDS,
                    },
                },
                crate::Variant {
                    name: "Write",
                    discriminant: None,
                    kind: crate::VariantKind::Tuple {
                        fields: Self::WRITE_FIELDS,
                    },
                },
                crate::Variant {
                    name: "ChangeColor",
                    discriminant: None,
                    kind: crate::VariantKind::Tuple {
                        fields: Self::CHANGE_COLOR_FIELDS,
                    },
                },
            ];
        }

        Shape {
            name: |f| write!(f, "Message"),
            typeid: mini_typeid::of::<Self>(),
            layout: std::alloc::Layout::new::<Self>(),
            innards: crate::Innards::Enum {
                variants: StaticFields::VARIANTS,
            },
            set_to_default: None,
            drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
        }
    }
}

#[test]
fn test_complex_enum_reflection() {
    let shape = Message::shape();
    eprintln!("{shape:#?}");

    // Test basic variant info
    assert_eq!(shape.variants().len(), 4);

    // Check different variant kinds
    let quit = shape.variant_by_name("Quit").unwrap();
    match quit.kind {
        crate::VariantKind::Unit => {}
        _ => panic!("Expected 'Quit' to be a Unit variant"),
    }

    let move_variant = shape.variant_by_name("Move").unwrap();
    match &move_variant.kind {
        crate::VariantKind::Struct { fields } => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "x");
            assert_eq!(fields[1].name, "y");
        }
        _ => panic!("Expected 'Move' to be a Struct variant"),
    }

    let write = shape.variant_by_name("Write").unwrap();
    match &write.kind {
        crate::VariantKind::Tuple { fields } => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "_0");
        }
        _ => panic!("Expected 'Write' to be a Tuple variant"),
    }

    let change_color = shape.variant_by_name("ChangeColor").unwrap();
    match &change_color.kind {
        crate::VariantKind::Tuple { fields } => {
            assert_eq!(fields.len(), 3);
            assert_eq!(fields[0].name, "_0");
            assert_eq!(fields[1].name, "_1");
            assert_eq!(fields[2].name, "_2");
        }
        _ => panic!("Expected 'ChangeColor' to be a Tuple variant"),
    }
}

/// This test demonstrates how to access enum variant information programmatically
/// through reflection, which could be used for tasks like serialization or UI generation.
#[test]
fn test_enum_metadata_access() {
    // Get the shape for both of our test enums
    let user_status_shape = UserStatus::shape();
    let message_shape = Message::shape();

    // Create a format string function that works with any enum shape
    fn format_variant_info(variant: &crate::Variant) -> String {
        let kind_str = match &variant.kind {
            crate::VariantKind::Unit => "unit variant".to_string(),
            crate::VariantKind::Tuple { fields } => {
                format!("tuple variant with {} fields", fields.len())
            }
            crate::VariantKind::Struct { fields } => {
                format!("struct variant with {} fields", fields.len())
            }
        };

        let disc_str = if let Some(disc) = variant.discriminant {
            format!(" (discriminant: {})", disc)
        } else {
            "".to_string()
        };

        format!("{} is a {}{}", variant.name, kind_str, disc_str)
    }

    // Print info for UserStatus enum
    println!("UserStatus variants:");
    for variant in user_status_shape.variants() {
        println!("  {}", format_variant_info(variant));
    }

    // Print info for Message enum
    println!("Message variants:");
    for variant in message_shape.variants() {
        println!("  {}", format_variant_info(variant));

        // For variants with fields, print field info
        match &variant.kind {
            crate::VariantKind::Tuple { fields } | crate::VariantKind::Struct { fields } => {
                for field in *fields {
                    println!("    Field: {} (type: {})", field.name, field.shape.get());
                }
            }
            _ => {}
        }
    }

    // Demonstrate programmatic field access with a struct variant
    if let Some(move_variant) = message_shape.variant_by_name("Move") {
        if let crate::VariantKind::Struct { fields } = &move_variant.kind {
            for field in *fields {
                println!("Move.{} is of type {}", field.name, field.shape.get());

                // Access the field shape for further information
                let field_shape = field.shape.get();
                match field_shape.innards {
                    crate::Innards::Scalar(_) => println!("    This is a scalar field"),
                    _ => println!("    This is a non-scalar field"),
                }
            }
        }
    }
}

#[test]
fn test_partial_enum_variant_field_access() {
    // Test that we can access fields of enum variants using the new methods

    // Create a partial for the Message enum (which is already defined in the tests)
    let _shape = Message::shape();
    let mut partial = Message::partial();

    // Set the variant to Write and initialize its field
    partial.set_variant_by_name("Write").unwrap();

    // Get the field slot for the message field (named "_0" for tuple variants)
    let field_slot = partial.variant_field_by_name("_0").unwrap();

    // Verify that we can get the field's shape
    let shape_desc = field_slot.shape();
    let shape = shape_desc.get();
    assert_eq!(shape.to_string(), "String");

    // Test that we can't access fields of a variant that hasn't been selected
    let mut partial = Message::partial();
    assert!(partial.variant_field_by_name("_0").is_err());

    // Test that we can't access fields that don't exist
    partial.set_variant_by_name("Write").unwrap();
    assert!(partial.variant_field_by_name("nonexistent").is_err());

    // Test that we can't access fields of a unit variant
    let mut partial = Message::partial();
    partial.set_variant_by_name("Quit").unwrap();
    assert!(partial.variant_field_by_name("any_field").is_err());
}

#[test]
fn test_enum_reflection() {
    #[derive(Debug, PartialEq)]
    #[allow(dead_code)]
    pub enum TestEnum {
        Unit,
        Tuple(u32, String),
        Struct { field1: u64, field2: bool },
    }

    impl Shapely for TestEnum {
        fn shape() -> crate::Shape {
            struct StaticFields;
            impl StaticFields {
                const TUPLE_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "_0",
                        shape: crate::ShapeDesc(u32::shape),
                        offset: 0,
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "_1",
                        shape: crate::ShapeDesc(String::shape),
                        offset: 0,
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const STRUCT_FIELDS: &'static [crate::Field] = &[
                    crate::Field {
                        name: "field1",
                        shape: crate::ShapeDesc(u64::shape),
                        offset: 0,
                        flags: crate::FieldFlags::EMPTY,
                    },
                    crate::Field {
                        name: "field2",
                        shape: crate::ShapeDesc(bool::shape),
                        offset: 0,
                        flags: crate::FieldFlags::EMPTY,
                    },
                ];

                const VARIANTS: &'static [crate::Variant] = &[
                    crate::Variant {
                        name: "Unit",
                        discriminant: None,
                        kind: crate::VariantKind::Unit,
                    },
                    crate::Variant {
                        name: "Tuple",
                        discriminant: None,
                        kind: crate::VariantKind::Tuple {
                            fields: Self::TUPLE_FIELDS,
                        },
                    },
                    crate::Variant {
                        name: "Struct",
                        discriminant: None,
                        kind: crate::VariantKind::Struct {
                            fields: Self::STRUCT_FIELDS,
                        },
                    },
                ];
            }

            Shape {
                name: |f| write!(f, "TestEnum"),
                typeid: mini_typeid::of::<Self>(),
                layout: std::alloc::Layout::new::<Self>(),
                innards: crate::Innards::Enum {
                    variants: StaticFields::VARIANTS,
                },
                set_to_default: None,
                drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
            }
        }
    }

    let shape = TestEnum::shape();
    eprintln!("{shape:#?}");

    // Test variant count
    assert_eq!(shape.variants().len(), 3);

    // Test variant by name
    let unit_variant = shape.variant_by_name("Unit").unwrap();
    assert_eq!(unit_variant.name, "Unit");
    match unit_variant.kind {
        crate::VariantKind::Unit => {}
        _ => panic!("Expected Unit variant kind"),
    }

    let tuple_variant = shape.variant_by_name("Tuple").unwrap();
    assert_eq!(tuple_variant.name, "Tuple");
    match &tuple_variant.kind {
        crate::VariantKind::Tuple { fields } => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "_0");
            assert_eq!(fields[1].name, "_1");
        }
        _ => panic!("Expected Tuple variant kind"),
    }

    let struct_variant = shape.variant_by_name("Struct").unwrap();
    assert_eq!(struct_variant.name, "Struct");
    match &struct_variant.kind {
        crate::VariantKind::Struct { fields } => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "field1");
            assert_eq!(fields[1].name, "field2");
        }
        _ => panic!("Expected Struct variant kind"),
    }

    // Test variant by index
    let variant0 = shape.variant_by_index(0).unwrap();
    assert_eq!(variant0.name, "Unit");

    let variant1 = shape.variant_by_index(1).unwrap();
    assert_eq!(variant1.name, "Tuple");

    let variant2 = shape.variant_by_index(2).unwrap();
    assert_eq!(variant2.name, "Struct");

    // Test errors
    assert!(shape.variant_by_index(3).is_err());
    assert!(shape.variant_by_name("NonExistent").is_none());
}

/// A custom type to be used in enum variants
#[derive(Debug, PartialEq)]
struct Something {
    value: String,
    count: u64,
}

impl Shapely for Something {
    fn shape() -> crate::Shape {
        Shape {
            name: |f| write!(f, "Something"),
            typeid: mini_typeid::of::<Self>(),
            layout: std::alloc::Layout::new::<Self>(),
            innards: crate::Innards::Struct {
                fields: crate::struct_fields!(Something, (value, count)),
            },
            set_to_default: None,
            drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
        }
    }
}

/// Complex enum with different variant types: struct, tuple and unit
#[derive(Debug, PartialEq)]
enum Foo {
    A { b: u32 },
    B(Something),
    C,
}

impl Shapely for Foo {
    fn shape() -> crate::Shape {
        struct StaticFields;
        impl StaticFields {
            const A_FIELDS: &'static [crate::Field] = &[crate::Field {
                name: "b",
                shape: crate::ShapeDesc(u32::shape),
                offset: 0, // Will be calculated at runtime
                flags: crate::FieldFlags::EMPTY,
            }];

            const B_FIELDS: &'static [crate::Field] = &[crate::Field {
                name: "_0",
                shape: crate::ShapeDesc(Something::shape),
                offset: 0, // Will be calculated at runtime
                flags: crate::FieldFlags::EMPTY,
            }];

            const VARIANTS: &'static [crate::Variant] = &[
                crate::Variant {
                    name: "A",
                    discriminant: None,
                    kind: crate::VariantKind::Struct {
                        fields: Self::A_FIELDS,
                    },
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
                    kind: crate::VariantKind::Unit,
                },
            ];
        }

        Shape {
            name: |f| write!(f, "Foo"),
            typeid: mini_typeid::of::<Self>(),
            layout: std::alloc::Layout::new::<Self>(),
            innards: crate::Innards::Enum {
                variants: StaticFields::VARIANTS,
            },
            set_to_default: None,
            drop_in_place: Some(|ptr| unsafe { std::ptr::drop_in_place(ptr as *mut Self) }),
        }
    }
}

/// Test that demonstrates how Shapely handles complex enums with nested custom types
#[test]
fn test_enum_with_custom_type() {
    // Create values of each variant for testing
    let foo_a = Foo::A { b: 42 };
    let foo_b = Foo::B(Something {
        value: "test".to_string(),
        count: 123,
    });
    let foo_c = Foo::C;

    // Verify debug output works as expected
    println!(
        "Debug output:\nA: {:?}\nB: {:?}\nC: {:?}",
        foo_a, foo_b, foo_c
    );

    // Get the shape for reflection
    let shape = Foo::shape();
    println!("Shape: {:#?}", shape);

    // Verify variant count
    assert_eq!(shape.variants().len(), 3);

    // Check variant types
    let a_variant = shape.variant_by_name("A").unwrap();
    let b_variant = shape.variant_by_name("B").unwrap();
    let c_variant = shape.variant_by_name("C").unwrap();

    // Test A variant (struct with named field)
    match &a_variant.kind {
        crate::VariantKind::Struct { fields } => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "b");

            // Get field type information
            let field_shape = fields[0].shape.get();
            println!("A.b field type: {}", field_shape);

            // Check that the field type is u32
            match field_shape.innards {
                crate::Innards::Scalar(crate::Scalar::U32) => {}
                _ => panic!("Expected u32 field type"),
            }
        }
        _ => panic!("Expected A to be a Struct variant"),
    }

    // Test B variant (tuple with custom type)
    match &b_variant.kind {
        crate::VariantKind::Tuple { fields } => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "_0");

            // Get field type information for the Something type
            let field_shape = fields[0].shape.get();
            println!("B._0 field type: {}", field_shape);

            // Check that it's a struct type
            match &field_shape.innards {
                crate::Innards::Struct { fields } => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "value");
                    assert_eq!(fields[1].name, "count");

                    // Check that the struct fields have the expected types
                    match fields[0].shape.get().innards {
                        crate::Innards::Scalar(crate::Scalar::String) => {}
                        _ => panic!("Expected String field type for value"),
                    }

                    match fields[1].shape.get().innards {
                        crate::Innards::Scalar(scalar) => {
                            // Allow either usize or u64 depending on platform
                            match scalar {
                                crate::Scalar::U64 => {}
                                crate::Scalar::U32 => {}
                                _ => panic!("Expected numeric field type for count"),
                            }
                        }
                        _ => panic!("Expected numeric field type for count"),
                    }

                    // Check that the count field is U64
                    match fields[1].shape.get().innards {
                        crate::Innards::Scalar(crate::Scalar::U64) => {}
                        _ => panic!("Expected U64 field type for count"),
                    }
                }
                _ => panic!("Expected B to contain a struct"),
            }
        }
        _ => panic!("Expected B to be a Tuple variant"),
    }

    // Test C variant (unit)
    match c_variant.kind {
        crate::VariantKind::Unit => {}
        _ => panic!("Expected C to be a Unit variant"),
    }

    // Demonstrate traversing the entire enum shape
    println!("\nEnum Foo structure:");
    print_enum_structure(shape);
}

// Helper function to print out the complete structure of an enum
fn print_enum_structure(shape: Shape) {
    println!("enum {} {{", shape);

    for variant in shape.variants() {
        match &variant.kind {
            crate::VariantKind::Unit => {
                println!("    {},", variant.name);
            }
            crate::VariantKind::Tuple { fields } => {
                let field_types = fields
                    .iter()
                    .map(|f| format!("{}", f.shape.get()))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("    {}({}),", variant.name, field_types);

                // Print nested field types
                for field in *fields {
                    let field_shape = field.shape.get();
                    if let crate::Innards::Struct {
                        fields: nested_fields,
                    } = &field_shape.innards
                    {
                        println!("        struct {} {{", field_shape);
                        for nested_field in *nested_fields {
                            println!(
                                "            {}: {},",
                                nested_field.name,
                                nested_field.shape.get()
                            );
                        }
                        println!("        }}");
                    }
                }
            }
            crate::VariantKind::Struct { fields } => {
                println!("    {} {{", variant.name);
                for field in *fields {
                    println!("        {}: {},", field.name, field.shape.get());
                }
                println!("    }},");
            }
        }
    }

    println!("}}");
}
