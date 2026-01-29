use facet::Facet;
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct CovariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn() -> &'a ()>,
}

fn main() {
    fn scope<'l, 'a>(token: &'l CovariantLifetime<'a>) -> &'l CovariantLifetime<'static> {
        Peek::new(token)
            .get::<CovariantLifetime<'static>>()
            .unwrap()
    }
    scope(&CovariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
