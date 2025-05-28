use std::collections::HashMap;

use facet_reflect::Wip;
use facet_testhelpers::test;

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest1() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?
        .pop()?
        .build()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest2() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?
        .pop()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest3() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest4() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest5() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest6() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest7() {
    let wip = Wip::alloc::<HashMap<String, String>>()?.begin_map_insert()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest8() {
    let wip = Wip::alloc::<HashMap<String, String>>()?;
    drop(wip);
}
