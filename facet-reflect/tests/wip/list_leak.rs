use facet_reflect::Wip;

#[test]
fn wip_list_leaktest1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?
        .push()?
        .put(30)?
        .pop()?
        .build()?;
    Ok(())
}

#[test]
fn wip_list_leaktest2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?
        .push()?
        .put(30)?
        .pop()?;
    Ok(())
}

#[test]
fn wip_list_leaktest3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?
        .push()?
        .put(30)?;
    Ok(())
}

#[test]
fn wip_list_leaktest4() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?
        .push()?;
    Ok(())
}

#[test]
fn wip_list_leaktest5() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?;
    Ok(())
}

#[test]
fn wip_list_leaktest6() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?;
    Ok(())
}

#[test]
fn wip_list_leaktest7() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?;
    Ok(())
}

#[test]
fn wip_list_leaktest8() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?;
    Ok(())
}

#[test]
fn wip_list_leaktest9() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?;
    Ok(())
}

#[test]
fn wip_list_leaktest10() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?.begin_pushback()?.push()?;
    Ok(())
}

#[test]
fn wip_list_leaktest11() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?.begin_pushback()?;
    Ok(())
}

#[test]
fn wip_list_leaktest12() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let _ = Wip::alloc::<Vec<i32>>()?;
    Ok(())
}
