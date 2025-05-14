use facet_reflect::Wip;
use facet_testhelpers::test;

#[test]
fn wip_option_testleak1() {
    let _ = Wip::alloc::<Option<String>>()?
        .push_some()?
        .put(String::from("Hello, world!"))?
        .pop()?
        .build()?
        .materialize::<Option<String>>();
}

#[test]
fn wip_option_testleak2() {
    let wip = Wip::alloc::<Option<String>>()?;
    let wip = wip.push_some()?;
    let wip = wip.put(String::from("Hello, world!"))?;
    let wip = wip.pop()?;
    let _wip = wip.build()?;
}

#[test]
fn wip_option_testleak3() {
    Wip::alloc::<Option<String>>()?
        .push_some()?
        .put(String::from("Hello, world!"))?
        .pop()?;
}

#[test]
fn wip_option_testleak4() {
    let _ = Wip::alloc::<Option<String>>()?
        .push_some()?
        .put(String::from("Hello, world!"));
}

#[test]
fn wip_option_testleak5() {
    Wip::alloc::<Option<String>>()?.push_some()?;
}

#[test]
fn wip_option_testleak6() {
    Wip::alloc::<Option<String>>()?;
}
