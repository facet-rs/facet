use facet_reflect::ConstValue;

#[test]
fn peek_list() {
    // Create test Vec instance
    let test_list = vec![1, 2, 3, 4, 5];
    let peek_value = ConstValue::new(&test_list);

    // Convert to list and check we can convert to PeekList
    let peek_list = peek_value
        .into_list()
        .expect("Should be convertible to list");

    // Test length
    assert_eq!(peek_list.len(), 5);

    // Test index access
    let first = peek_list.get(0).expect("Should have first element");
    let third = peek_list.get(2).expect("Should have third element");
    let last = peek_list.get(4).expect("Should have last element");

    // Test element values
    let first_value = first.get::<i32>();
    assert_eq!(*first_value, 1);

    let third_value = third.get::<i32>();
    assert_eq!(*third_value, 3);

    let last_value = last.get::<i32>();
    assert_eq!(*last_value, 5);

    // Test out of bounds
    assert!(peek_list.get(5).is_none());
}
