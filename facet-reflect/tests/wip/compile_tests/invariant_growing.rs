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
        token: InvariantLifetime<'facet>,
    ) -> Result<Wrapper<'static>, ReflectError<'static>> {
        Wip::<'static, 'static>::alloc::<Wrapper<'static>>()?
            .field_named("token")?
            .put(token)?
            .pop()?
            .build()?
            .materialize::<Wrapper<'static>>()
    }
    scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
