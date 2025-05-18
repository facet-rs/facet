use facet::Facet;
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct ContravariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ())>,
}

fn main() {
    fn scope<'l, 'a>(token: &'l ContravariantLifetime<'static>) -> &'l ContravariantLifetime<'a> {
        Peek::new(token).get::<ContravariantLifetime<'a>>().unwrap()
    }
    scope(&ContravariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
