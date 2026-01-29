use facet::Facet;
use facet_reflect::Partial;

#[derive(Debug, Facet)]
struct InvariantLifetime<'facet> {
    _pd: std::marker::PhantomData<fn(&'facet ()) -> &'facet ()>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'facet> {
        token: InvariantLifetime<'facet>,
    }

    fn scope<'facet>(token: InvariantLifetime<'static>) -> Wrapper<'facet> {
        // SAFETY: Wrapper::<'facet>::SHAPE comes from the derived Facet implementation
        unsafe { Partial::<'facet>::alloc_shape(Wrapper::<'facet>::SHAPE) }
            .unwrap()
            .set_field("token", token)
            .unwrap()
            .build()
            .unwrap()
            .materialize()
            .unwrap()
    }
    let _ = scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
