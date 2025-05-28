use facet::Facet;
use facet_reflect::Partial;

#[derive(Debug, Facet)]
struct Foo<'a> {
    s: &'a str,
}

fn main() -> eyre::Result<()> {
    let mut partial = Partial::alloc_shape(Foo::SHAPE)?;
    let partial = {
        let s = "abc".to_string();
        let foo = Foo { s: &s };
        partial.set(foo)?
    };

    let v = partial.build()?.materialize()?;
    dbg!(v);

    Ok(())
}
