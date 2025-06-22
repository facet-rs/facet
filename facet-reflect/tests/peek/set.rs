use facet_reflect::Peek;
use facet_testhelpers::test;
use std::collections::HashSet;

#[test]
fn test_peek_set_basics() {
    let mut source = HashSet::new();
    source.insert("a");
    source.insert("b");

    let peek_value = Peek::new(&source);
    let peek_map = peek_value.into_set()?;
    assert_eq!(peek_map.len(), 2);
}

#[test]
fn test_peek_set_iteration() {
    let mut source = HashSet::new();
    source.insert("a");
    source.insert("b");

    let peek_value = Peek::new(&source);
    let peek_map = peek_value.into_set()?;
    let mut entries: Vec<_> = peek_map.iter().map(|v| *v.get::<&str>().unwrap()).collect();
    entries.sort_by(|a, b| a.cmp(&b));

    assert_eq!(entries, vec!["a", "b"]);
}
