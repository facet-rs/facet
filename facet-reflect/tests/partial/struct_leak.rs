use facet::Facet;
use facet_reflect::Partial;
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
    let v = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?
        .begin_field("b")?
        .set(43)?
        .end()?
        .end()?
        .build()?;

    assert_eq!(
        *v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );
}

#[test]
fn wip_struct_testleak2() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?
        .begin_field("b")?
        .set(43)?
        .end()?
        .end()?
        .build()?;
}

#[test]
fn wip_struct_testleak3() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?
        .begin_field("b")?
        .set(43)?
        .end()?
        .end()?;
}

#[test]
fn wip_struct_testleak4() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?
        .begin_field("b")?
        .set(43)?
        .end()?;
}

#[test]
fn wip_struct_testleak5() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?
        .begin_field("b")?
        .set(43)?;
}

#[test]
fn wip_struct_testleak6() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?
        .begin_field("b")?;
}

#[test]
fn wip_struct_testleak7() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?
        .end()?;
}

#[test]
fn wip_struct_testleak8() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?
        .set(42)?;
}

#[test]
fn wip_struct_testleak9() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?
        .begin_field("x")?;
}

#[test]
fn wip_struct_testleak10() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?
        .begin_field("inner")?;
}

#[test]
fn wip_struct_testleak11() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?
        .end()?;
}

#[test]
fn wip_struct_testleak12() {
    let _ = Partial::alloc::<Outer>()?
        .begin_field("name")?
        .set(String::from("Hello, world!"))?;
}

#[test]
fn wip_struct_testleak13() {
    let _ = Partial::alloc::<Outer>()?.begin_field("name")?;
}

#[test]
fn wip_struct_testleak14() {
    let _ = Partial::alloc::<Outer>()?;
}
