use facet_reflect::{Peek, ReflectErrorKind};
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

#[test]
fn test_peek_set_contains_peek() {
    let mut source = HashSet::new();
    source.insert("a");
    source.insert("b");
    source.insert("c");

    let peek_value = Peek::new(&source);
    let peek_set = peek_value.into_set().unwrap();

    // Test contains with values that exist
    let a = "a";
    assert_eq!(peek_set.contains_peek(Peek::new(&a)).unwrap(), true);

    let b = "b";
    assert_eq!(peek_set.contains_peek(Peek::new(&b)).unwrap(), true);

    // Test contains with value that doesn't exist
    let d = "d";
    assert_eq!(peek_set.contains_peek(Peek::new(&d)).unwrap(), false);
}

#[test]
fn test_peek_set_contains_peek_wrong_type() {
    let mut source = HashSet::new();
    source.insert("a");
    source.insert("b");

    let peek_value = Peek::new(&source);
    let peek_set = peek_value.into_set().unwrap();

    // Try to check for an integer in a string set
    let wrong_type = 42;
    let result = peek_set.contains_peek(Peek::new(&wrong_type));

    assert!(
        matches!(result, Err(ref err) if matches!(err.kind, ReflectErrorKind::WrongShape { .. }))
    );
}
