use facet::Facet;
use facet_reflect::{ReflectError, Wip};

#[derive(Debug, Facet)]
struct ContravariantLifetime<'facet> {
    _pd: std::marker::PhantomData<fn(&'facet ())>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'facet> {
        token: ContravariantLifetime<'facet>,
    }

    fn scope<'facet>(
        token: ContravariantLifetime<'static>,
    ) -> Result<Wrapper<'facet>, ReflectError<'static>> {
        Wip::<'facet, 'static>::alloc::<Wrapper<'facet>>()?
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
