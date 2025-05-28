use facet::Facet;
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct InvariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ()) -> &'a ()>,
}

fn main() {
    fn scope<'l, 'a>(token: &'l InvariantLifetime<'static>) -> &'l InvariantLifetime<'a> {
        Peek::new(&token).get::<InvariantLifetime<'a>>().unwrap()
    }
    scope(&InvariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
