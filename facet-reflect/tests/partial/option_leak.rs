use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn wip_option_testleak1() {
    let wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    let _ = wip.build().unwrap();
}

#[test]
fn wip_option_testleak2() {
    let wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    let _wip = wip.build().unwrap();
}

#[test]
fn wip_option_testleak3() {
    let _wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak4() {
    let _wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak5() {
    let _ = Partial::alloc::<Option<String>>().unwrap();
    // Just allocate without setting a value
}

#[test]
fn wip_option_testleak6() {
    let _ = Partial::alloc::<Option<String>>().unwrap();
}
