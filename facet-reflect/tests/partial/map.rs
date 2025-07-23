use facet_reflect::Partial;
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn wip_map_trivial() {
    let mut partial = Partial::alloc::<HashMap<String, String>>().unwrap();
    partial.begin_map().unwrap();

    partial.begin_key().unwrap();
    partial.set::<String>("key".into()).unwrap();
    partial.end().unwrap();
    partial.begin_value().unwrap();
    partial.set::<String>("value".into()).unwrap();
    partial.end().unwrap();
    let wip: HashMap<String, String> = *partial.build().unwrap();

    assert_eq!(
        wip,
        HashMap::from([("key".to_string(), "value".to_string())])
    );
}
