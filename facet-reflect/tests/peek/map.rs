use facet_reflect::Peek;
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn test_peek_map_basics() {
    let mut source = HashMap::new();
    source.insert("a", 1);
    source.insert("b", 2);
    source.insert("c", 3);

    let peek_value = Peek::new(&source);
    let peek_map = peek_value.into_map()?;
    assert_eq!(peek_map.len(), 3);
    assert!(!peek_map.is_empty());

    assert!(peek_map.contains_key(&"a"));
    assert!(peek_map.contains_key(&"b"));
    assert!(peek_map.contains_key(&"c"));
    assert!(!peek_map.contains_key(&"d"));

    assert_eq!(peek_map.get(&"a").unwrap().get::<i32>()?, &1);
    assert_eq!(peek_map.get(&"b").unwrap().get::<i32>()?, &2);
    assert_eq!(peek_map.get(&"c").unwrap().get::<i32>()?, &3);
    assert!(peek_map.get(&"d").is_none());
}

#[test]
fn test_peek_map_empty() {
    let source: HashMap<&str, i32> = HashMap::new();
    let peek_value = Peek::new(&source);
    let peek_map = peek_value.into_map()?;
    assert_eq!(peek_map.len(), 0);
    assert!(peek_map.is_empty());
    assert!(!peek_map.contains_key(&"anything"));
    assert!(peek_map.get(&"anything").is_none());
}

#[test]
fn test_peek_map_iteration() {
    let mut source = HashMap::new();
    source.insert("a", 1);
    source.insert("b", 2);

    let peek_value = Peek::new(&source);
    let peek_map = peek_value.into_map()?;
    let mut entries: Vec<_> = peek_map
        .iter()
        .map(|(k, v)| {
            (
                k.get::<&str>().unwrap().to_string(),
                *v.get::<i32>().unwrap(),
            )
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(entries, vec![("a".to_string(), 1), ("b".to_string(), 2),]);
}

#[test]
fn test_peek_map_different_types() {
    let mut source = HashMap::new();
    source.insert(1, "one");
    source.insert(2, "two");

    let peek_value = Peek::new(&source);
    let peek_map = peek_value.into_map()?;
    assert_eq!(peek_map.len(), 2);

    assert!(peek_map.contains_key(&1));
    assert!(peek_map.contains_key(&2));
    assert!(!peek_map.contains_key(&3));

    assert_eq!(peek_map.get(&1).unwrap().get::<&str>()?, &"one");
    assert_eq!(peek_map.get(&2).unwrap().get::<&str>()?, &"two");
    assert!(peek_map.get(&3).is_none());
}
