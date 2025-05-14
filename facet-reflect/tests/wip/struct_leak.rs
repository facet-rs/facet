use facet::Facet;
use facet_reflect::Wip;
use facet_testhelpers::test;

#[derive(Facet, PartialEq, Eq, Debug)]
struct Outer {
    name: String,
    inner: Inner,
}

#[derive(Facet, PartialEq, Eq, Debug)]
struct Inner {
    x: i32,
    b: i32,
}

#[test]
fn wip_struct_testleak1() {
    let v = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .build()?
        .materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );
}

#[test]
fn wip_struct_testleak2() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .build()?;
}

#[test]
fn wip_struct_testleak3() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?;
}

#[test]
fn wip_struct_testleak4() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?;
}

#[test]
fn wip_struct_testleak5() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?;
}

#[test]
fn wip_struct_testleak6() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?;
}

#[test]
fn wip_struct_testleak7() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?;
}

#[test]
fn wip_struct_testleak8() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?;
}

#[test]
fn wip_struct_testleak9() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?;
}

#[test]
fn wip_struct_testleak10() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?;
}

#[test]
fn wip_struct_testleak11() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?;
}

#[test]
fn wip_struct_testleak12() {
    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?;
}

#[test]
fn wip_struct_testleak13() {
    let _ = Wip::alloc::<Outer>()?.field_named("name")?;
}

#[test]
fn wip_struct_testleak14() {
    let _ = Wip::alloc::<Outer>()?;
}
