use facet_peek::Peek;

#[test]
fn peek_option() {
    // Test with Some value
    let some_value = Some(42);
    let peek = Peek::new(&some_value);

    // Check we get the right variant
    if let Peek::Option(peek_option) = peek {
        assert!(peek_option.is_some());
        assert!(!peek_option.is_none());

        // Check the inner value
        let inner = peek_option.value().unwrap();
        if let Peek::Value(peek_value) = inner {
            let value = unsafe { peek_value.data().as_ref::<i32>() };
            assert_eq!(*value, 42);
        } else {
            panic!("Expected inner value to be a PeekValue");
        }
    } else {
        panic!("Expected a PeekOption");
    }

    // Test with None value
    let none_value: Option<i32> = None;
    let peek = Peek::new(&none_value);

    if let Peek::Option(peek_option) = peek {
        assert!(!peek_option.is_some());
        assert!(peek_option.is_none());
        assert!(peek_option.value().is_none());
    } else {
        panic!("Expected a PeekOption");
    }
}
