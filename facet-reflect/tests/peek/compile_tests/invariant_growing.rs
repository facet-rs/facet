use facet::Facet;
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct InvariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ()) -> &'a ()>,
}

fn main() {
    fn scope<'l, 'a>(token: &'l InvariantLifetime<'a>) -> &'l InvariantLifetime<'static> {
        Peek::new(token)
            .get::<InvariantLifetime<'static>>()
            .unwrap()
    }
    scope(&InvariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
