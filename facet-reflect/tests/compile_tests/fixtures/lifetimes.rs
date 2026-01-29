use facet::Facet;
use facet_reflect::Partial;

#[derive(Debug, Facet)]
struct Foo<'a> {
    s: &'a str,
}

fn main() {
    // SAFETY: Foo::SHAPE comes from the derived Facet implementation
    let mut partial = unsafe { Partial::alloc_shape(Foo::SHAPE) }.unwrap();
    let partial = {
        let s = "abc".to_string();
        let foo = Foo { s: &s };
        partial.set(foo).unwrap()
    };

    let v: Foo = partial.build().unwrap().materialize().unwrap();
    dbg!(v);
}
