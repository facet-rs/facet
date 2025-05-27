use facet::Facet;
use facet_testhelpers::test;

use super::Wip;

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*);
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        /* no-op under miri */
    };
}

#[test]
fn f64_uninit() {
    let wip = Wip::alloc::<f64>()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn f64_init() {
    let mut wip = Wip::alloc::<f64>()?;
    wip.put::<f64>(6.241)?;
    let hv = wip.build()?;
    assert_eq!(*hv, 6.241);
}

#[test]
fn option_uninit() {
    let wip = Wip::alloc::<Option<f64>>()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn option_init() {
    let mut wip = Wip::alloc::<Option<f64>>()?;
    wip.put::<Option<f64>>(Some(6.241))?;
    let hv = wip.build()?;
    assert_eq!(*hv, Some(6.241));
}

#[test]
fn struct_fully_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let wip = Wip::alloc::<FooBar>()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn struct_partially_uninit() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let mut wip = Wip::alloc::<FooBar>()?;
    wip.push_field("foo")?;
    wip.put::<u64>(42)?;
    wip.pop()?;
    assert_snapshot!(wip.build().unwrap_err());
}

#[test]
fn struct_fully_init() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        foo: u64,
        bar: bool,
    }

    let mut wip = Wip::alloc::<FooBar>()?;
    wip.push_field("foo")?;
    wip.put::<u64>(42)?;
    wip.pop()?;
    wip.push_field("bar")?;
    wip.put::<bool>(true)?;
    wip.pop()?;
    let hv = wip.build()?;
    assert_eq!(hv.foo, 42u64);
    assert_eq!(hv.bar, true);
}
