use facet_json_legacy::to_string;
use facet_testhelpers::test;

#[test]
fn test_set() {
    let mut set = std::collections::HashSet::new();
    set.insert(3);

    let json = to_string(&set);

    assert_eq!(json, "[3]");
}

#[test]
fn test_set_with_multiple_entries() {
    let mut set = std::collections::HashSet::new();
    set.insert(3);
    set.insert(4);

    let json = to_string(&set);

    assert!(json == "[3,4]" || json == "[4,3]");
}
