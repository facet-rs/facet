use facet::Facet;
use facet_reflect::{Partial, ReflectError};

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
        Partial::<'facet, 'static>::alloc_shape(Wrapper::<'facet>::SHAPE)?
            .set_field("token", token)?
            .build()?
            .materialize()
    }
    scope(ContravariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap_err();
}
