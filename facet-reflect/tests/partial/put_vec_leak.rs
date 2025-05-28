use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn put_vec_leaktest1() {
    let mut w = Partial::alloc::<Vec<String>>()?;
    w.set(vec!["a".to_string()])?;
    drop(w);
}

#[test]
fn put_vec_leaktest2() {
    let mut w = Partial::alloc::<Vec<String>>()?;
    w.set(vec!["a".to_string()])?;
    let w = w.build()?;
    // let it drop: the entire value should be deinitialized, and the memory for the Partial should be freed
    drop(w);
}

#[test]
fn put_vec_leaktest3() {
    let mut w = Partial::alloc::<Vec<String>>()?;
    w.set(vec!["a".to_string()])?;
    let v = w.build()?;
    assert_eq!(*v, vec!["a".to_string()]);
}
