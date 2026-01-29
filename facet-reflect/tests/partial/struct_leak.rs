use bumpalo::Bump;
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
    let v = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        .begin_field("b")
        .unwrap()
        .set(43)
        .unwrap()
        .end()
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap()
        .materialize::<Outer>()
        .unwrap();

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
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        .begin_field("b")
        .unwrap()
        .set(43)
        .unwrap()
        .end()
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap();
}

#[test]
fn wip_struct_testleak3() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        .begin_field("b")
        .unwrap()
        .set(43)
        .unwrap()
        .end()
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_struct_testleak4() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        .begin_field("b")
        .unwrap()
        .set(43)
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_struct_testleak5() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        .begin_field("b")
        .unwrap()
        .set(43)
        .unwrap();
}

#[test]
fn wip_struct_testleak6() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap()
        .begin_field("b")
        .unwrap();
}

#[test]
fn wip_struct_testleak7() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_struct_testleak8() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap()
        .set(42)
        .unwrap();
}

#[test]
fn wip_struct_testleak9() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap()
        .begin_field("x")
        .unwrap();
}

#[test]
fn wip_struct_testleak10() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap()
        .begin_field("inner")
        .unwrap();
}

#[test]
fn wip_struct_testleak11() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_struct_testleak12() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap()
        .set(String::from("Hello, world!"))
        .unwrap();
}

#[test]
fn wip_struct_testleak13() {
    let _ = Partial::alloc::<Outer>()
        .unwrap()
        .begin_field("name")
        .unwrap();
}

#[test]
fn wip_struct_testleak14() {
    let bump = Bump::new(); let _ = Partial::alloc::<Outer>(&bump).unwrap();
}
