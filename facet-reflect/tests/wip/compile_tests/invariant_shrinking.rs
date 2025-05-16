use facet::Facet;
use facet_reflect::{ReflectError, Wip};

#[derive(Debug, Facet)]
struct InvariantLifetime<'facet> {
    _pd: std::marker::PhantomData<fn(&'facet ()) -> &'facet ()>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'facet> {
        token: InvariantLifetime<'facet>,
    }

    fn scope<'facet>(
        token: InvariantLifetime<'static>,
    ) -> Result<Wrapper<'facet>, ReflectError<'static>> {
        Wip::<'facet, 'static>::alloc::<Wrapper<'facet>>()?
            .field_named("token")?
            .put(token)?
            .pop()?
            .build()?
            .materialize::<Wrapper>()
    }
    scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
