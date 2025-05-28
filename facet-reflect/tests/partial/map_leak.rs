use std::collections::HashMap;

use facet_reflect::Partial;
use facet_testhelpers::test;

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest1() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?
        .begin_insert()?
        .begin_key()?
        .set("key".to_string())?
        .end()?
        .begin_value()?
        .set("value".to_string())?
        .end()?;
    let wip = wip.build()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest2() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?
        .begin_insert()?
        .begin_key()?
        .set("key".to_string())?
        .end()?
        .begin_value()?
        .set("value".to_string())?
        .end()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest3() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?
        .begin_insert()?
        .begin_key()?
        .set("key".to_string())?
        .end()?
        .begin_value()?
        .set("value".to_string())?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest4() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?
        .begin_insert()?
        .begin_key()?
        .set("key".to_string())?
        .end()?
        .begin_value()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest5() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?
        .begin_insert()?
        .begin_key()?
        .set("key".to_string())?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest6() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?.begin_insert()?.begin_key()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest7() {
    let mut wip = Partial::alloc::<HashMap<String, String>>()?;
    wip.begin_map()?.begin_insert()?;
    drop(wip);
}

// If we partially initialize a map, do we leak memory?
#[test]
fn wip_map_leaktest8() {
    let wip = Partial::alloc::<HashMap<String, String>>()?;
    drop(wip);
}
