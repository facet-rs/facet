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
        token: InvariantLifetime<'static>,
    ) -> Result<Wrapper<'facet>, ReflectError<'static>> {
        Partial::<'facet, 'static>::alloc_shape(Wrapper::<'facet>::SHAPE)?
            .set_field("token", token)?
            .build()?
            .materialize()
    }
    scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
