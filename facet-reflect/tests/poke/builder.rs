use facet::Facet;
use facet_reflect::{Builder, PokeValueUninit};

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
    let v = PokeValueUninit::alloc::<Outer>();
    let v = Builder::new(v)
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
    use facet_reflect::{Builder, PokeValueUninit};

    #[derive(Debug, PartialEq, Eq, Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    // Allocate memory for a struct - returns a PokeValueUninit
    let v = PokeValueUninit::alloc::<FooBar>();

    // Use the Builder API to set field values
    let foo_bar = Builder::new(v)
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
