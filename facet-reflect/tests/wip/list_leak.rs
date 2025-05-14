use facet_reflect::Wip;
use facet_testhelpers::test;

#[test]
fn wip_list_leaktest1() {
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
}

#[test]
fn wip_list_leaktest2() {
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
}

#[test]
fn wip_list_leaktest3() {
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
}

#[test]
fn wip_list_leaktest4() {
    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?
        .push()?;
}

#[test]
fn wip_list_leaktest5() {
    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?
        .pop()?;
}

#[test]
fn wip_list_leaktest6() {
    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?
        .put(20)?;
}

#[test]
fn wip_list_leaktest7() {
    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?
        .push()?;
}

#[test]
fn wip_list_leaktest8() {
    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?
        .pop()?;
}

#[test]
fn wip_list_leaktest9() {
    let _ = Wip::alloc::<Vec<i32>>()?
        .begin_pushback()?
        .push()?
        .put(10)?;
}

#[test]
fn wip_list_leaktest10() {
    let _ = Wip::alloc::<Vec<i32>>()?.begin_pushback()?.push()?;
}

#[test]
fn wip_list_leaktest11() {
    let _ = Wip::alloc::<Vec<i32>>()?.begin_pushback()?;
}

#[test]
fn wip_list_leaktest12() {
    let _ = Wip::alloc::<Vec<i32>>()?;
}
