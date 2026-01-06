#![allow(clippy::needless_lifetimes)]
use facet::Facet;
use facet_reflect::{Partial, ReflectError};
use facet_testhelpers::test;

#[derive(Debug, Facet)]
struct CovariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn() -> &'a ()>,
}

#[derive(Debug, Facet)]
struct ContravariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ())>,
}

#[derive(Debug, Facet)]
struct InvariantLifetime<'a> {
    _pd: std::marker::PhantomData<fn(&'a ()) -> &'a ()>,
}

#[test]
fn covariant_works() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: CovariantLifetime<'a>,
    }

    fn scope<'a>(token: CovariantLifetime<'a>) -> Result<Wrapper<'a>, ReflectError> {
        // SAFETY: Wrapper::<'a>::SHAPE comes from the derived Facet implementation
        let partial: Partial<'a> = unsafe { Partial::alloc_shape(Wrapper::<'a>::SHAPE) }.unwrap();
        partial
            .begin_field("token")
            .unwrap()
            .set(token)
            .unwrap()
            .end()
            .unwrap()
            .build()
            .unwrap()
            .materialize::<Wrapper>()
    }
    scope(CovariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap();
}

#[test]
fn contravariant_works() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: ContravariantLifetime<'a>,
    }

    fn scope<'a>(token: ContravariantLifetime<'a>) -> Result<Wrapper<'a>, ReflectError> {
        // SAFETY: Wrapper::<'a>::SHAPE comes from the derived Facet implementation
        let partial: Partial<'a> = unsafe { Partial::alloc_shape(Wrapper::<'a>::SHAPE) }.unwrap();
        partial
            .begin_field("token")
            .unwrap()
            .set(token)
            .unwrap()
            .end()
            .unwrap()
            .build()
            .unwrap()
            .materialize::<Wrapper>()
    }
    scope(ContravariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap();
}

#[test]
fn invariant_works() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: InvariantLifetime<'a>,
    }

    fn scope<'a>(token: InvariantLifetime<'a>) -> Result<Wrapper<'a>, ReflectError> {
        // SAFETY: Wrapper::<'a>::SHAPE comes from the derived Facet implementation
        let partial: Partial<'a> = unsafe { Partial::alloc_shape(Wrapper::<'a>::SHAPE) }.unwrap();
        partial
            .begin_field("token")
            .unwrap()
            .set(token)
            .unwrap()
            .end()
            .unwrap()
            .build()
            .unwrap()
            .materialize::<Wrapper>()
    }
    scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    })
    .unwrap();
}
