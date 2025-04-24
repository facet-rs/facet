use facet_reflect::Wip;

#[test]
fn put_vec_leaktest1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let w = Wip::alloc::<Vec<String>>()?;
    let w = w.put(vec!["a".to_string()])?;
    drop(w);
    Ok(())
}

#[test]
fn put_vec_leaktest2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let w = Wip::alloc::<Vec<String>>()?;
    let w = w.put(vec!["a".to_string()])?;
    let w = w.build()?;
    // let it drop: the entire value should be deinitialized, and the memory for the Wip should be freed
    drop(w);
    Ok(())
}

#[test]
fn put_vec_leaktest3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let w = Wip::alloc::<Vec<String>>()?;
    let w = w.put(vec!["a".to_string()])?;
    let w = w.build()?;
    let v = w.materialize::<Vec<String>>()?;
    assert_eq!(v, vec!["a".to_string()]);
    Ok(())
}
