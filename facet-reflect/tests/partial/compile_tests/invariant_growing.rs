use facet::Facet;
use facet_reflect::{Partial, ReflectError};

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
        Partial::<'static, 'static>::alloc_shape(Wrapper::<'static>::SHAPE)?
            .set_field("token", token)?
            .build()?
            .materialize()
    }
    scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
