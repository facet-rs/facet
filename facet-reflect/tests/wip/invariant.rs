use facet::Facet;
use facet_reflect::Wip;

#[test]
fn build_with_invariants() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    #[facet(invariants = "invariants")]
    struct MyNonZeroU8(u8);

    impl MyNonZeroU8 {
        fn invariants(&self) -> bool {
            self.0 != 0
        }
    }

    let wip: MyNonZeroU8 = Wip::alloc::<MyNonZeroU8>()?
        .field(0)?
        .put(42u8)?
        .pop()?
        .build()?
        .materialize()?;
    assert_eq!(wip, MyNonZeroU8(42));

    let result = Wip::alloc::<MyNonZeroU8>()?
        .field(0)?
        .put(0_u8)?
        .pop()?
        .build();
    assert!(result.is_err());

    Ok(())
}
