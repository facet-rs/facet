use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn wip_option_testleak1() {
    let mut wip = Partial::alloc::<Option<String>>()?;
    wip.set(Some(String::from("Hello, world!")))?;
    let _ = wip.build()?;
}

#[test]
fn wip_option_testleak2() {
    let mut wip = Partial::alloc::<Option<String>>()?;
    wip.set(Some(String::from("Hello, world!")))?;
    let _wip = wip.build()?;
}

#[test]
fn wip_option_testleak3() {
    let mut wip = Partial::alloc::<Option<String>>()?;
    wip.set(Some(String::from("Hello, world!")))?;
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak4() {
    let mut wip = Partial::alloc::<Option<String>>()?;
    wip.set(Some(String::from("Hello, world!")))?;
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak5() {
    let _ = Partial::alloc::<Option<String>>()?;
    // Just allocate without setting a value
}

#[test]
fn wip_option_testleak6() {
    let _ = Partial::alloc::<Option<String>>()?;
}
