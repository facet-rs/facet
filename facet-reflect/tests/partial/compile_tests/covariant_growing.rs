use facet::Facet;
use facet_reflect::Partial;

#[derive(Debug, Facet)]
struct CovariantLifetime<'facet> {
    _pd: std::marker::PhantomData<fn() -> &'facet ()>,
}

fn main() {
    #[derive(Debug, Facet)]
    struct Wrapper<'facet> {
        token: CovariantLifetime<'facet>,
    }

    fn scope<'facet>(token: CovariantLifetime<'facet>) -> Wrapper<'static> {
        // SAFETY: Wrapper::<'static>::SHAPE comes from the derived Facet implementation
        unsafe { Partial::<'static>::alloc_shape(Wrapper::<'static>::SHAPE) }
            .unwrap()
            .set_field("token", token)
            .unwrap()
            .build()
            .unwrap()
            .materialize()
            .unwrap()
    }
    let _ = scope(CovariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
