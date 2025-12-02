use facet_reflect::Peek;
use facet_testhelpers::test;

#[test]
fn peek_result_ok() {
    // Test with Ok value
    let ok_value: Result<i32, String> = Ok(42);
    let peek_value = Peek::new(&ok_value);

    // Convert to result
    let peek_result = peek_value
        .into_result()
        .expect("Should be convertible to result");

    // Check the Ok variant methods
    assert!(peek_result.is_ok());
    assert!(!peek_result.is_err());

    // Get the Ok value
    let ok_inner = peek_result.ok().expect("Should have an Ok value");
    let value = ok_inner.get::<i32>().unwrap();
    assert_eq!(*value, 42);

    // Err should be None
    assert!(peek_result.err().is_none());
}

#[test]
fn peek_result_err() {
    // Test with Err value
    let err_value: Result<i32, String> = Err("error message".to_string());
    let peek_value = Peek::new(&err_value);

    // Convert to result
    let peek_result = peek_value
        .into_result()
        .expect("Should be convertible to result");

    // Check the Err variant methods
    assert!(!peek_result.is_ok());
    assert!(peek_result.is_err());

    // Get the Err value
    let err_inner = peek_result.err().expect("Should have an Err value");
    let value = err_inner.get::<String>().unwrap();
    assert_eq!(value, "error message");

    // Ok should be None
    assert!(peek_result.ok().is_none());
}

#[test]
fn peek_result_with_complex_types() {
    // Test with complex Ok type
    let ok_value: Result<Vec<i32>, &str> = Ok(vec![1, 2, 3]);
    let peek_value = Peek::new(&ok_value);
    let peek_result = peek_value.into_result().unwrap();

    assert!(peek_result.is_ok());
    let ok_inner = peek_result.ok().unwrap();
    let value = ok_inner.get::<Vec<i32>>().unwrap();
    assert_eq!(value, &vec![1, 2, 3]);

    // Test with complex Err type
    let err_value: Result<i32, Vec<String>> = Err(vec!["error1".to_string(), "error2".to_string()]);
    let peek_value = Peek::new(&err_value);
    let peek_result = peek_value.into_result().unwrap();

    assert!(peek_result.is_err());
    let err_inner = peek_result.err().unwrap();
    let value = err_inner.get::<Vec<String>>().unwrap();
    assert_eq!(value, &vec!["error1".to_string(), "error2".to_string()]);
}
