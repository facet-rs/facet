use facet_reflect::Partial;
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn wip_map_trivial() {
    let mut partial = Partial::alloc::<HashMap<String, String>>()?;
    partial.begin_map()?;

    partial.begin_key()?;
    partial.set::<String>("key".into())?;
    partial.end()?;
    partial.begin_value()?;
    partial.set::<String>("value".into())?;
    partial.end()?;
    let wip: HashMap<String, String> = *partial.build()?;

    assert_eq!(
        wip,
        HashMap::from([("key".to_string(), "value".to_string())])
    );
}
