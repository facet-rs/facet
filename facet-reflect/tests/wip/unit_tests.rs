use facet::Facet;
use facet_reflect::Wip;

#[derive(Facet, PartialEq, Eq, Debug)]
struct Outer {
    name: String,
    inner: Inner,
}

#[derive(Facet, PartialEq, Eq, Debug)]
struct Inner {
    x: i32,
    b: i32,
}

#[test]
fn wip_nested() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let v = Wip::alloc::<Outer>()
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );

    Ok(())
}

#[test]
fn wip_nested_out_of_order() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let v = Wip::alloc::<Outer>()
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .pop()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );

    Ok(())
}

#[test]
fn readme_sample() -> eyre::Result<()> {
    facet_testhelpers::setup();

    use facet::Facet;

    #[derive(Debug, PartialEq, Eq, Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let foo_bar = Wip::alloc::<FooBar>()
        .field_named("foo")?
        .put(42u64)?
        .pop()?
        .field_named("bar")?
        .put(String::from("Hello, World!"))?
        .pop()?
        .build()?
        .materialize::<FooBar>()?;

    // Now we can use the constructed value
    println!("{}", foo_bar.bar);

    Ok(())
}

#[test]
fn build_with_invariants() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    #[facet(invariants = "invariants")]
    struct MyNonZeroU8(u8);

    impl MyNonZeroU8 {
        fn invariants(&self) -> bool {
            self.0 != 0
        }
    }

    let wip: MyNonZeroU8 = Wip::alloc::<MyNonZeroU8>()
        .put(MyNonZeroU8(42))?
        .build()?
        .materialize()?;
    assert_eq!(wip, MyNonZeroU8(42));

    let result = Wip::alloc::<MyNonZeroU8>().put(MyNonZeroU8(0))?.build();
    assert!(result.is_err());

    Ok(())
}

#[test]
fn put_vec_no_leak_1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let w = Wip::alloc::<Vec<String>>();
    let w = w.put(vec!["a".to_string()])?;
    // let it drop: the fields should be deinitialized, and the memory for the Wip should be freed
    drop(w);
    Ok(())
}

#[test]
fn put_vec_no_leak_2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let w = Wip::alloc::<Vec<String>>();
    let w = w.put(vec!["a".to_string()])?;
    let w = w.build()?;
    // let it drop: the entire value should be deinitialized, and the memory for the Wip should be freed
    drop(w);
    Ok(())
}

#[test]
fn put_vec_no_leak_3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let w = Wip::alloc::<Vec<String>>();
    let w = w.put(vec!["a".to_string()])?;
    let w = w.build()?;
    let v = w.materialize::<Vec<String>>()?;
    assert_eq!(v, vec!["a".to_string()]);
    Ok(())
}

// Enum tests

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(u8)]
enum SimpleEnum {
    A,
    B,
    #[expect(dead_code)]
    C,
}

#[test]
fn wip_unit_enum() -> eyre::Result<()> {
    facet_testhelpers::setup();

    // Test unit variant A
    let a = Wip::alloc::<SimpleEnum>()
        .variant_named("A")?
        .build()?
        .materialize::<SimpleEnum>()?;
    assert_eq!(a, SimpleEnum::A);

    // Test unit variant B
    let b = Wip::alloc::<SimpleEnum>()
        .variant(1)? // B is at index 1
        .build()?
        .materialize::<SimpleEnum>()?;
    assert_eq!(b, SimpleEnum::B);

    Ok(())
}

#[derive(Facet, PartialEq, Eq, Debug)]
#[repr(u8)]
enum EnumWithData {
    Empty,
    Single(i32),
    Tuple(i32, String),
    Struct { x: i32, y: String },
}

#[test]
fn wip_enum_with_data() -> eyre::Result<()> {
    facet_testhelpers::setup();

    // Test empty variant
    let empty = Wip::alloc::<EnumWithData>()
        .variant_named("Empty")?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(empty, EnumWithData::Empty);

    // Test single-field tuple variant
    let single = Wip::alloc::<EnumWithData>()
        .variant_named("Single")?
        .field(0)? // Access the first field
        .put(42)?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(single, EnumWithData::Single(42));

    // Test multi-field tuple variant
    let tuple = Wip::alloc::<EnumWithData>()
        .variant_named("Tuple")?
        .field(0)?
        .put(42)?
        .pop()?
        .field(1)?
        .put(String::from("Hello"))?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(tuple, EnumWithData::Tuple(42, String::from("Hello")));

    // Test struct variant
    let struct_variant = Wip::alloc::<EnumWithData>()
        .variant_named("Struct")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("y")?
        .put(String::from("World"))?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;
    assert_eq!(
        struct_variant,
        EnumWithData::Struct {
            x: 42,
            y: String::from("World")
        }
    );

    Ok(())
}

#[test]
fn wip_enum_error_cases() -> eyre::Result<()> {
    facet_testhelpers::setup();

    // Test error: trying to access a field without selecting a variant
    let result = Wip::alloc::<EnumWithData>().field_named("x");
    assert!(result.is_err());

    // Test error: trying to select a non-existent variant
    let result = Wip::alloc::<EnumWithData>().variant_named("NonExistent");
    assert!(result.is_err());

    // Test error: trying to access a non-existent field in a variant
    let result = Wip::alloc::<EnumWithData>()
        .variant_named("Struct")?
        .field_named("non_existent");
    assert!(result.is_err());

    // Test error: trying to build without initializing all fields
    let result = Wip::alloc::<EnumWithData>()
        .variant_named("Struct")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .build();
    assert!(result.is_err());

    Ok(())
}

// We've already tested enum functionality with SimpleEnum and EnumWithData,
// so we'll skip additional representation tests

#[test]
fn wip_switch_enum_variant() -> eyre::Result<()> {
    facet_testhelpers::setup();

    // Test switching variants
    let result = Wip::alloc::<EnumWithData>()
        .variant_named("Single")?
        .field(0)?
        .put(42)?
        .pop()?
        .variant_named("Tuple")? // Switch to another variant
        .field(0)?
        .put(43)?
        .pop()?
        .field(1)?
        .put(String::from("Changed"))?
        .pop()?
        .build()?
        .materialize::<EnumWithData>()?;

    assert_eq!(result, EnumWithData::Tuple(43, String::from("Changed")));

    Ok(())
}
