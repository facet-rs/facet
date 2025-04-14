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

    let wip: MyNonZeroU8 = Wip::alloc::<MyNonZeroU8>()
        .put(MyNonZeroU8(42))?
        .build()?
        .materialize()?;
    assert_eq!(wip, MyNonZeroU8(42));

    let result = Wip::alloc::<MyNonZeroU8>().put(MyNonZeroU8(0))?.build();
    assert!(result.is_err());

    Ok(())
}
