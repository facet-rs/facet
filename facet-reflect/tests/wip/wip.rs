use facet::Facet;
use facet_reflect::WipValue;

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
fn test_tree() -> eyre::Result<()> {
    let v = WipValue::alloc::<Outer>()
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .field_named("b")?
        .put(43)?
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
    use facet::Facet;
    use facet_reflect::WipValue;

    #[derive(Debug, PartialEq, Eq, Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let foo_bar = WipValue::alloc::<FooBar>()
        .field_named("foo")?
        .put(42u64)?
        .field_named("bar")?
        .put(String::from("Hello, World!"))?
        .build()?
        .materialize::<FooBar>()?;

    // Now we can use the constructed value
    println!("{}", foo_bar.bar);

    Ok(())
}
