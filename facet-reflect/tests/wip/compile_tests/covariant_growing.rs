use facet::Facet;
use facet_reflect::{ReflectError, Wip};

#[derive(Debug, Facet)]
struct CovariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn() -> &'a ()>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: CovariantLifetime<'a>,
    }

    fn scope<'a>(token: CovariantLifetime<'a>) -> Result<Wrapper<'static>, ReflectError> {
        Wip::<'static>::alloc::<Wrapper<'static>>()
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
