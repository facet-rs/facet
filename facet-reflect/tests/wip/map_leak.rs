use std::collections::HashMap;

use facet_reflect::Wip;

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?
        .pop()?
        .build()?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?
        .pop()?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest4() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest5() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest6() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest7() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?.begin_map_insert()?;
    drop(wip);

    Ok(())
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest8() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let wip = Wip::alloc::<HashMap<String, String>>()?;
    drop(wip);

    Ok(())
}
