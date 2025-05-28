use facet::Facet;
use facet_reflect::{Partial, ReflectError};

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
        Partial::<'static, 'static>::alloc_shape(Wrapper::<'static>::SHAPE)?
            .set_field("token", token)?
            .build()?
            .materialize()
    }
    scope(CovariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
