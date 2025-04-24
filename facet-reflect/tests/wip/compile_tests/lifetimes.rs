use facet::Facet;
use facet_reflect::Wip;

#[derive(Debug, Facet)]
struct Foo<'a> {
    s: &'a str,
}

fn main() -> eyre::Result<()> {
    let mut wip = Wip::alloc::<Foo>()?;
    let wip = {
        let s = "abc".to_string();
        let foo = Foo { s: &s };
        wip.put(foo)?
    };

    let v = wip.build()?.materialize::<Foo>()?;
    dbg!(v);

    Ok(())
}
