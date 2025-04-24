use facet_reflect::Wip;

#[test]
fn wip_option_testleak1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Option<String>>()?
        .push_some()?
        .put(String::from("Hello, world!"))?
        .pop()?
        .build()?
        .materialize::<Option<String>>();

    Ok(())
}

#[test]
fn wip_option_testleak2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<Option<String>>()?;
    let wip = wip.push_some()?;
    let wip = wip.put(String::from("Hello, world!"))?;
    let wip = wip.pop()?;
    let _wip = wip.build()?;

    Ok(())
}

#[test]
fn wip_option_testleak3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    Wip::alloc::<Option<String>>()?
        .push_some()?
        .put(String::from("Hello, world!"))?
        .pop()?;

    Ok(())
}

#[test]
fn wip_option_testleak4() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Option<String>>()?
        .push_some()?
        .put(String::from("Hello, world!"));

    Ok(())
}

#[test]
fn wip_option_testleak5() -> eyre::Result<()> {
    facet_testhelpers::setup();

    Wip::alloc::<Option<String>>()?.push_some()?;

    Ok(())
}

#[test]
fn wip_option_testleak6() -> eyre::Result<()> {
    facet_testhelpers::setup();

    Wip::alloc::<Option<String>>()?;

    Ok(())
}
