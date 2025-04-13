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
fn wip_nested() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let v = WipValue::alloc::<Outer>()
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
fn readme_sample() -> eyre::Result<()> {
    facet_testhelpers::setup();

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
        .pop()?
        .field_named("bar")?
        .put(String::from("Hello, World!"))?
        .build()?
        .materialize::<FooBar>()?;

    // Now we can use the constructed value
    println!("{}", foo_bar.bar);

    Ok(())
}

#[test]
fn lifetimes() -> eyre::Result<()> {
    #[derive(Debug, Facet)]
    struct Foo<'a> {
        s: &'a str,
    }

    let wip = WipValue::alloc::<Foo>();
    let v = {
        let s = "abc".to_string();
        let foo = Foo { s: &s };

        wip.put(foo)
            .unwrap()
            .build()
            .unwrap()
            .materialize::<Foo>()
            .unwrap()
    };
    dbg!(v);

    Ok(())
}
