use facet_reflect::Wip;
use facet_testhelpers::test;

#[test]
fn put_vec_leaktest1() {
    let w = Wip::alloc::<Vec<String>>()?;
    let w = w.put(vec!["a".to_string()])?;
    drop(w);
}

#[test]
fn put_vec_leaktest2() {
    let w = Wip::alloc::<Vec<String>>()?;
    let w = w.put(vec!["a".to_string()])?;
    let w = w.build()?;
    // let it drop: the entire value should be deinitialized, and the memory for the Wip should be freed
    drop(w);
}

#[test]
fn put_vec_leaktest3() {
    let w = Wip::alloc::<Vec<String>>()?;
    let w = w.put(vec!["a".to_string()])?;
    let w = w.build()?;
    let v = w.materialize::<Vec<String>>()?;
    assert_eq!(v, vec!["a".to_string()]);
}
