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
