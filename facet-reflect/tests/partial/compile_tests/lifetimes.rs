use facet::Facet;
use facet_reflect::Partial;

#[derive(Debug, Facet)]
struct Foo<'a> {
    s: &'a str,
}

fn main() {
    let mut partial = Partial::alloc_shape(Foo::SHAPE).unwrap();
    let partial = {
        let s = "abc".to_string();
        let foo = Foo { s: &s };
        partial.set(foo).unwrap()
    };

    let v = partial.build().unwrap().materialize().unwrap();
    dbg!(v);

    Ok(())
}
