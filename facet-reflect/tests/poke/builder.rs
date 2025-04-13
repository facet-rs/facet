use facet::Facet;
use facet_reflect::{PokeValueUninit, Builder};

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
    let v = Builder::new(v);
    let v = v.field_named("name")?;
    v.put("Hello, world!")?;

    Ok(())
}
