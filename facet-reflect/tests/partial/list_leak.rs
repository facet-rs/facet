use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn wip_list_leaktest1() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?
        .set(20)?
        .end()?
        .begin_list_item()?
        .set(30)?
        .end()?
        .build()?;
}

#[test]
fn wip_list_leaktest2() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?
        .set(20)?
        .end()?
        .begin_list_item()?
        .set(30)?
        .end()?;
}

#[test]
fn wip_list_leaktest3() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?
        .set(20)?
        .end()?
        .begin_list_item()?
        .set(30)?;
}

#[test]
fn wip_list_leaktest4() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?
        .set(20)?
        .end()?
        .begin_list_item()?;
}

#[test]
fn wip_list_leaktest5() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?
        .set(20)?
        .end()?;
}

#[test]
fn wip_list_leaktest6() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?
        .set(20)?;
}

#[test]
fn wip_list_leaktest7() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?
        .begin_list_item()?;
}

#[test]
fn wip_list_leaktest8() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?
        .end()?;
}

#[test]
fn wip_list_leaktest9() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?
        .set(10)?;
}

#[test]
fn wip_list_leaktest10() {
    let _ = Partial::alloc::<Vec<i32>>()?
        .begin_list()?
        .begin_list_item()?;
}

#[test]
fn wip_list_leaktest11() {
    let _ = Partial::alloc::<Vec<i32>>()?.begin_list()?;
}

#[test]
fn wip_list_leaktest12() {
    let _ = Partial::alloc::<Vec<i32>>()?;
}
