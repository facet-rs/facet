use facet::Facet;
use facet_reflect::{ReflectError, Wip};

#[derive(Debug, Facet)]
struct InvariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ()) -> &'a ()>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: InvariantLifetime<'a>,
    }

    fn scope<'a>(token: InvariantLifetime<'static>) -> Result<Wrapper<'a>, ReflectError> {
        Wip::<'a>::alloc::<Wrapper<'a>>()
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
