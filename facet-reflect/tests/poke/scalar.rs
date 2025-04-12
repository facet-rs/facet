use facet_reflect::{Poke, PokeUninit};

#[test]
fn build_u64() {
    facet_testhelpers::setup();

    let (pu, _guard) = PokeUninit::alloc::<u64>();
    let pv = pu.into_scalar().put(42u64);

    let value = *pv.get::<u64>();

    // Verify the value was set correctly
    assert_eq!(value, 42);
}

#[test]
fn mutate_u64() {
    facet_testhelpers::setup();

    let mut value = 41;
    assert_eq!(value, 41);
    {
        let p = Poke::new(&mut value);
        let pv = p.into_scalar();
        pv.replace(99);
    }
    assert_eq!(value, 99);
}
