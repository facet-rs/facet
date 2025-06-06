use std::hash::{DefaultHasher, Hash, Hasher};

use facet_reflect::Peek;
use facet_testhelpers::test;

#[test]
fn test_peek_value_twoints() {
    let a = 42_i32;
    let b = 42_i32;

    let av = Peek::new(&a);
    let bv = Peek::new(&b);

    assert_eq!(av, bv);
    assert_eq!(av.to_string(), "42");

    let mut h = DefaultHasher::new();
    a.hash(&mut h);
    let h1 = h.finish();

    let mut h = DefaultHasher::new();
    av.hash(&mut h).unwrap();
    let h2 = h.finish();

    assert_eq!(h1, h2);
}

#[test]
fn test_peek_value_twostrings() {
    let a = Some(42_i32);
    let av = Peek::new(&a);

    assert_eq!(av.to_string(), "⟨Option<i32>⟩");
    assert_eq!(format!("{a:?}"), format!("{av:?}"));
}
