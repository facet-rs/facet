use facet::Facet;
use facet_reflect::{ReflectError, Wip};

#[derive(Debug, Facet)]
struct ContravariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ())>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: ContravariantLifetime<'a>,
    }

    fn scope<'a>(token: ContravariantLifetime<'static>) -> Result<Wrapper<'a>, ReflectError> {
        Wip::<'a>::alloc::<Wrapper<'a>>()
            .field_named("token")?
            .put(token)?
            .pop()?
            .build()?
            .materialize::<Wrapper>()
    }
    scope(ContravariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
