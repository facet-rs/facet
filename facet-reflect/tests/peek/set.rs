use facet_reflect::Peek;
use facet_testhelpers::test;
use std::collections::HashSet;

#[test]
fn test_peek_set_basics() {
    let mut source = HashSet::new();
    source.insert("a");
    source.insert("b");

    let peek_value = Peek::new(&source);
    let peek_set = peek_value.into_set().unwrap();

    assert_eq!(peek_set.len(), 2);
    assert!(!peek_set.is_empty());
}

#[test]
fn test_peek_set_iteration() {
    let mut source = HashSet::new();
    source.insert("a");
    source.insert("b");

    let peek_value = Peek::new(&source);
    let peek_set = peek_value.into_set().unwrap();
    let mut entries: Vec<_> = peek_set.iter().map(|v| *v.get::<&str>().unwrap()).collect();
    entries.sort();

    assert_eq!(entries, vec!["a", "b"]);
}
