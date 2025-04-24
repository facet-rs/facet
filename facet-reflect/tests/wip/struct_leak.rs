use facet::Facet;
use facet_reflect::Wip;

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
fn wip_struct_testleak1() -> eyre::Result<()> {
    facet_testhelpers::setup();

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

    Ok(())
}

#[test]
fn wip_struct_testleak2() -> eyre::Result<()> {
    facet_testhelpers::setup();

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
        .build()?; // Removed .build()?.materialize()?

    Ok(())
}

#[test]
fn wip_struct_testleak3() -> eyre::Result<()> {
    facet_testhelpers::setup();

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
        .pop()?; // Removed .build()?

    Ok(())
}

#[test]
fn wip_struct_testleak4() -> eyre::Result<()> {
    facet_testhelpers::setup();

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
        .pop()?; // Removed .pop()?

    Ok(())
}

#[test]
fn wip_struct_testleak5() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?; // Removed .pop()?

    Ok(())
}

#[test]
fn wip_struct_testleak6() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?; // Removed .put(43)?

    Ok(())
}

#[test]
fn wip_struct_testleak7() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?; // Removed .field_named("b")?

    Ok(())
}

#[test]
fn wip_struct_testleak8() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?; // Removed .pop()?

    Ok(())
}

#[test]
fn wip_struct_testleak9() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?; // Removed .put(42)?

    Ok(())
}

#[test]
fn wip_struct_testleak10() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?; // Removed .field_named("x")?

    Ok(())
}

#[test]
fn wip_struct_testleak11() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?; // Removed .field_named("inner")?

    Ok(())
}

#[test]
fn wip_struct_testleak12() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?
        .field_named("name")?
        .put(String::from("Hello, world!"))?; // Removed .pop()?

    Ok(())
}

#[test]
fn wip_struct_testleak13() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?.field_named("name")?; // Removed .put(String::from("Hello, world!"))?

    Ok(())
}

#[test]
fn wip_struct_testleak14() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Outer>()?; // Removed .field_named("name")?

    Ok(())
}
