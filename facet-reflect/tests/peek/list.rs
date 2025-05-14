use facet_reflect::Peek;
use facet_testhelpers::test;

#[test]
fn peek_list() {
    // Create test Vec instance
    let test_list = vec![1, 2, 3, 4, 5];
    let peek_value = Peek::new(&test_list);

    // Convert to list and check we can convert to PeekList
    let peek_list = peek_value.into_list()?;

    // Test length
    assert_eq!(peek_list.len(), 5);

    // Test index access
    let first = peek_list.get(0).unwrap();
    let third = peek_list.get(2).unwrap();
    let last = peek_list.get(4).unwrap();

    // Test element values
    let first_value = *first.get::<i32>()?;
    assert_eq!(first_value, 1);

    let third_value = *third.get::<i32>()?;
    assert_eq!(third_value, 3);

    let last_value = *last.get::<i32>()?;
    assert_eq!(last_value, 5);

    // Test out of bounds
    assert!(peek_list.get(5).is_none());
}
