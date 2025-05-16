use facet::Facet;
use facet_reflect::{ReflectError, Wip};

#[derive(Debug, Facet)]
struct CovariantLifetime<'facet> {
    _pd: std::marker::PhantomData<fn() -> &'facet ()>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'facet> {
        token: CovariantLifetime<'facet>,
    }

    fn scope<'facet>(
        token: CovariantLifetime<'facet>,
    ) -> Result<Wrapper<'static>, ReflectError<'static>> {
        Wip::<'static, 'static>::alloc::<Wrapper<'static>>()?
            .field_named("token")?
            .put(token)?
            .pop()?
            .build()?
            .materialize::<Wrapper>()
    }
    scope(CovariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
