use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn put_vec_leaktest1() {
    let w = Partial::alloc::<Vec<String>>()
        .unwrap()
        .set(vec!["a".to_string()])
        .unwrap();
    drop(w);
}

#[test]
fn put_vec_leaktest2() {
    let w = Partial::alloc::<Vec<String>>()
        .unwrap()
        .set(vec!["a".to_string()])
        .unwrap()
        .build()
        .unwrap();
    // let it drop: the entire value should be deinitialized, and the memory for the Partial should be freed
    drop(w);
}

#[test]
fn put_vec_leaktest3() {
    let v = Partial::alloc::<Vec<String>>()
        .unwrap()
        .set(vec!["a".to_string()])
        .unwrap()
        .build()
        .unwrap()
        .materialize::<Vec<String>>()
        .unwrap();
    assert_eq!(v, vec!["a".to_string()]);
}
