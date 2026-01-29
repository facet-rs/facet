#![allow(clippy::needless_lifetimes)]

use bumpalo::Bump;
use facet::Facet;
use facet_reflect::Partial;
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

    fn scope<'a>(token: CovariantLifetime<'a>) -> Wrapper<'a> {
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
            .unwrap()
    }
    let _ = scope(CovariantLifetime {
        _pd: std::marker::PhantomData,
    });
}

#[test]
fn contravariant_works() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: ContravariantLifetime<'a>,
    }

    fn scope<'a>(token: ContravariantLifetime<'a>) -> Wrapper<'a> {
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
            .unwrap()
    }
    let _ = scope(ContravariantLifetime {
        _pd: std::marker::PhantomData,
    });
}

#[test]
fn invariant_works() {
    #[derive(Debug, Facet)]
    struct Wrapper<'a> {
        token: InvariantLifetime<'a>,
    }

    fn scope<'a>(token: InvariantLifetime<'a>) -> Wrapper<'a> {
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
            .unwrap()
    }
    let _ = scope(InvariantLifetime {
        _pd: std::marker::PhantomData,
    });
}
