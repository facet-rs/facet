use facet_reflect::PokeValueUninit;

#[test]
fn build_u64() {
    facet_testhelpers::setup();

    let (pu, _guard) = PokeValueUninit::alloc::<u64>();
    let pv = pu.put(42u64).unwrap();

    let value = *pv.get::<u64>();

    // Verify the value was set correctly
    assert_eq!(value, 42);
}
