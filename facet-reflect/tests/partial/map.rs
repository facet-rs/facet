use facet::Facet;
use facet_reflect::Wip;
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn wip_map_trivial() {
    let wip = Wip::alloc::<HashMap<String, String>>()?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("key".into())?
        .push_map_value()?
        .put::<String>("value".into())?
        .pop()?
        .build()?
        .materialize::<HashMap<String, String>>()?;

    assert_eq!(
        wip,
        HashMap::from([("key".to_string(), "value".to_string())])
    );
}

#[test]
fn wip_map_twice() {
    #[derive(Facet)]
    struct MapWrap {
        map: HashMap<String, String>,
    }

    let _wip = Wip::alloc::<MapWrap>()?
        .field_named("map")?
        .begin_map_insert()?
        .push_map_key()?
        .put::<String>("first".into())?
        .push_map_value()?
        .put::<String>("uno".into())?
        .pop()?
        .pop()?
        .field_named("map")?
        .push_map_key()?
        .put::<String>("second".into())?
        .push_map_value()?
        .put::<String>("dos".into())?
        .pop()?
        .pop()?
        .build()?
        .materialize::<MapWrap>()?;
}
