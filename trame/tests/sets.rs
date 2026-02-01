use facet::Facet;
use std::collections::{BTreeSet, HashSet};
use trame::{Op, Partial};

// =============================================================================
// HashSet tests
// =============================================================================

#[test]
fn build_empty_hashset() {
    let mut partial = Partial::alloc::<HashSet<u32>>().unwrap();

    partial.apply(&[Op::set().stage()]).unwrap();

    let result: HashSet<u32> = partial.build().unwrap();
    assert!(result.is_empty());
}

#[test]
fn build_hashset_with_imm_elements() {
    let mut partial = Partial::alloc::<HashSet<u32>>().unwrap();

    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
        ])
        .unwrap();

    let result: HashSet<u32> = partial.build().unwrap();
    assert_eq!(result.len(), 3);
    assert!(result.contains(&1));
    assert!(result.contains(&2));
    assert!(result.contains(&3));
}

#[test]
fn build_hashset_with_default_elements() {
    let mut partial = Partial::alloc::<HashSet<u32>>().unwrap();

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().default(),
            Op::set().append().default(),
        ])
        .unwrap();

    let result: HashSet<u32> = partial.build().unwrap();
    // Two default u32s (both 0) should deduplicate to one element
    assert_eq!(result.len(), 1);
    assert!(result.contains(&0));
}

#[test]
fn build_hashset_of_strings() {
    let mut partial = Partial::alloc::<HashSet<String>>().unwrap();

    let mut a = String::from("apple");
    let mut b = String::from("banana");
    let mut c = String::from("cherry");

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
        ])
        .unwrap();

    // Imm moves the values - prevent double-free
    std::mem::forget(a);
    std::mem::forget(b);
    std::mem::forget(c);

    let result: HashSet<String> = partial.build().unwrap();
    assert_eq!(result.len(), 3);
    assert!(result.contains("apple"));
    assert!(result.contains("banana"));
    assert!(result.contains("cherry"));
}

#[test]
fn build_hashset_with_duplicates() {
    let mut partial = Partial::alloc::<HashSet<u32>>().unwrap();

    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 1u32; // duplicate

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
        ])
        .unwrap();

    let result: HashSet<u32> = partial.build().unwrap();
    assert_eq!(result.len(), 2); // duplicates are deduplicated
    assert!(result.contains(&1));
    assert!(result.contains(&2));
}

// =============================================================================
// BTreeSet tests
// =============================================================================

#[test]
fn build_empty_btreeset() {
    let mut partial = Partial::alloc::<BTreeSet<u32>>().unwrap();

    partial.apply(&[Op::set().stage()]).unwrap();

    let result: BTreeSet<u32> = partial.build().unwrap();
    assert!(result.is_empty());
}

#[test]
fn build_btreeset_with_imm_elements() {
    let mut partial = Partial::alloc::<BTreeSet<u32>>().unwrap();

    let mut a = 3u32;
    let mut b = 1u32;
    let mut c = 2u32;

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
        ])
        .unwrap();

    let result: BTreeSet<u32> = partial.build().unwrap();
    assert_eq!(result.len(), 3);
    // BTreeSet maintains order
    let vec: Vec<_> = result.into_iter().collect();
    assert_eq!(vec, vec![1, 2, 3]);
}

#[test]
fn build_btreeset_of_strings() {
    let mut partial = Partial::alloc::<BTreeSet<String>>().unwrap();

    let mut a = String::from("cherry");
    let mut b = String::from("apple");
    let mut c = String::from("banana");

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
        ])
        .unwrap();

    // Imm moves the values - prevent double-free
    std::mem::forget(a);
    std::mem::forget(b);
    std::mem::forget(c);

    let result: BTreeSet<String> = partial.build().unwrap();
    assert_eq!(result.len(), 3);
    // BTreeSet maintains sorted order
    let vec: Vec<_> = result.into_iter().collect();
    assert_eq!(vec, vec!["apple", "banana", "cherry"]);
}

// =============================================================================
// Sets as struct fields
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct TaggedItem {
    name: String,
    tags: HashSet<String>,
}

#[test]
fn build_struct_with_hashset_field() {
    let mut partial = Partial::alloc::<TaggedItem>().unwrap();

    let mut name = String::from("item1");
    let mut tag1 = String::from("important");
    let mut tag2 = String::from("urgent");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut name),
            Op::set().at(1).stage(),
            Op::set().append().imm(&mut tag1),
            Op::set().append().imm(&mut tag2),
            Op::end(),
        ])
        .unwrap();

    // Imm moves the values - prevent double-free
    std::mem::forget(name);
    std::mem::forget(tag1);
    std::mem::forget(tag2);

    let result: TaggedItem = partial.build().unwrap();
    assert_eq!(result.name, "item1");
    assert_eq!(result.tags.len(), 2);
    assert!(result.tags.contains("important"));
    assert!(result.tags.contains("urgent"));
}

#[derive(Debug, PartialEq, Facet)]
struct SortedCollection {
    items: BTreeSet<i32>,
}

#[test]
fn build_struct_with_btreeset_field() {
    let mut partial = Partial::alloc::<SortedCollection>().unwrap();

    let mut a = 30i32;
    let mut b = 10i32;
    let mut c = 20i32;

    partial
        .apply(&[
            Op::set().at(0).stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
            Op::end(),
        ])
        .unwrap();

    let result: SortedCollection = partial.build().unwrap();
    let vec: Vec<_> = result.items.into_iter().collect();
    assert_eq!(vec, vec![10, 20, 30]);
}
