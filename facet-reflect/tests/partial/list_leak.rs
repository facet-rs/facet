use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn wip_list_leaktest1() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(20)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(30)
        .unwrap()
        .end()
        .unwrap()
        .build()
        .unwrap();
}

#[test]
fn wip_list_leaktest2() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(20)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(30)
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_list_leaktest3() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(20)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(30)
        .unwrap();
}

#[test]
fn wip_list_leaktest4() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(20)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap();
}

#[test]
fn wip_list_leaktest5() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(20)
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_list_leaktest6() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(20)
        .unwrap();
}

#[test]
fn wip_list_leaktest7() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap()
        .begin_list_item()
        .unwrap();
}

#[test]
fn wip_list_leaktest8() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap()
        .end()
        .unwrap();
}

#[test]
fn wip_list_leaktest9() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap()
        .set(10)
        .unwrap();
}

#[test]
fn wip_list_leaktest10() {
    let _ = Partial::alloc::<Vec<i32>>()
        .unwrap()
        .init_list()
        .unwrap()
        .begin_list_item()
        .unwrap();
}

#[test]
fn wip_list_leaktest11() {
    let _ = Partial::alloc::<Vec<i32>>().unwrap().init_list().unwrap();
}

#[test]
fn wip_list_leaktest12() {
    let _ = Partial::alloc::<Vec<i32>>().unwrap();
}
