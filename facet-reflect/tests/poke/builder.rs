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
