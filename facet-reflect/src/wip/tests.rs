use facet_testhelpers::test;

use super::Wip;

#[test]
fn f64_uninit() {
    let wip = Wip::alloc::<f64>()?;
    insta::assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn f64_init() {
    let wip = Wip::alloc::<f64>()?;
    let wip = wip.put::<f64>(6.241)?;
    let hv = wip.build()?;
    assert_eq!(*hv, 6.241);
}
