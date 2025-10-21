use facet_reflect::Peek;

#[test]
fn test_peek_dst_str() {
    let s = "abc";
    let peek = Peek::new(s);
    assert_eq!(format!("{peek}"), format!("{s}"));
    assert_eq!(format!("{peek:?}"), format!("{s:?}"));
}

#[test]
fn test_peek_dst_slice() {
    let s = [1, 2, 3].as_slice();
    let peek = Peek::new(s);
    assert_eq!(format!("{peek:?}"), format!("{s:?}"));
}
