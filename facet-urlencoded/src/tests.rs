use crate::from_str;
use eyre::Result;
use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
struct SearchParams {
    query: String,
    page: u64,
}

#[derive(Debug, Facet, PartialEq)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Facet, PartialEq)]
struct User {
    name: String,
    age: u64,
    address: Address,
}

#[derive(Debug, Facet, PartialEq)]
struct OrderForm {
    product_id: String,
    quantity: u64,
    user: User,
}

#[test]
fn test_basic_urlencoded() -> Result<()> {
    facet_testhelpers::setup();

    let query_string = "query=rust+programming&page=2";

    let params: SearchParams = from_str(query_string)?;
    assert_eq!(
        params,
        SearchParams {
            query: "rust programming".to_string(),
            page: 2
        }
    );

    Ok(())
}

#[test]
fn test_encoded_characters() -> Result<()> {
    facet_testhelpers::setup();

    let query_string = "query=rust%20programming%21&page=3";

    let params: SearchParams = from_str(query_string)?;
    assert_eq!(
        params,
        SearchParams {
            query: "rust programming!".to_string(),
            page: 3
        }
    );

    Ok(())
}

#[test]
fn test_missing_field_light() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct TestStruct {
        field1: String,
        field2: String,
    }

    let query_string = "field1=value";

    // This should return an error because field2 is not initialized
    let result = from_str::<TestStruct>(query_string);

    assert!(result.is_err());
    if let Err(err) = result {
        match err {
            crate::UrlEncodedError::ReflectError(reflect_err) => {
                // Convert to string and check if it contains the expected message
                let err_msg = format!("{}", reflect_err);
                assert!(
                    err_msg.contains("Field 'TestStruct::field2' was not initialized"),
                    "Expected error about uninitialized field, got: {}",
                    err_msg
                );
            }
            _ => panic!("Expected ReflectError, got: {:?}", err),
        }
    }

    Ok(())
}

#[test]
fn test_unknown_field() -> Result<()> {
    facet_testhelpers::setup();

    let query_string = "query=rust+programming&page=2&unknown=value";

    let params: SearchParams = from_str(query_string)?;
    assert_eq!(
        params,
        SearchParams {
            query: "rust programming".to_string(),
            page: 2
        }
    );

    Ok(())
}

#[test]
fn test_invalid_number() -> Result<()> {
    facet_testhelpers::setup();

    let query_string = "query=rust+programming&page=not_a_number";

    let result = from_str::<SearchParams>(query_string);

    assert!(result.is_err());
    if let Err(err) = result {
        match err {
            crate::UrlEncodedError::InvalidNumber(field, value) => {
                assert_eq!(field, "page");
                assert_eq!(value, "not_a_number");
            }
            _ => panic!("Expected InvalidNumber error"),
        }
    }

    Ok(())
}

#[test]
fn test_nested_struct() -> Result<()> {
    facet_testhelpers::setup();

    let query_string = "user[name]=John+Doe&user[age]=30&user[address][street]=123+Main+St&user[address][city]=Anytown&user[address][zip]=12345&product_id=ABC123&quantity=2";

    let order: OrderForm = from_str(query_string)?;

    assert_eq!(
        order,
        OrderForm {
            product_id: "ABC123".to_string(),
            quantity: 2,
            user: User {
                name: "John Doe".to_string(),
                age: 30,
                address: Address {
                    street: "123 Main St".to_string(),
                    city: "Anytown".to_string(),
                    zip: "12345".to_string(),
                },
            },
        }
    );

    Ok(())
}

#[test]
fn test_partial_nested_struct() -> Result<()> {
    facet_testhelpers::setup();

    // Missing some nested fields
    let query_string = "user[name]=John+Doe&user[age]=30&user[address][street]=123+Main+St&user[address][zip]=12345&product_id=ABC123&quantity=2";

    // This should return an error because the city field is not initialized
    let result = from_str::<OrderForm>(query_string);

    assert!(result.is_err());
    if let Err(err) = result {
        match err {
            crate::UrlEncodedError::ReflectError(reflect_err) => {
                // Convert to string and check if it contains the expected message
                let err_msg = format!("{}", reflect_err);
                assert!(
                    err_msg.contains("Field 'OrderForm::user' was not initialized"),
                    "Expected error about uninitialized field, got: {}",
                    err_msg
                );
            }
            _ => panic!("Expected ReflectError, got: {:?}", err),
        }
    }

    Ok(())
}

#[test]
fn test_deep_nesting() -> Result<()> {
    let query_string = "very[very][deeply][nested][field]=value&simple=data";

    #[derive(Debug, Facet, PartialEq)]
    struct DeepNested {
        field: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Nested {
        nested: DeepNested,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Deeply {
        deeply: Nested,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Very {
        very: Deeply,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct DeepTest {
        very: Very,
        simple: String,
    }

    let deep_test: DeepTest = from_str(query_string)?;

    assert_eq!(
        deep_test,
        DeepTest {
            very: Very {
                very: Deeply {
                    deeply: Nested {
                        nested: DeepNested {
                            field: "value".to_string(),
                        }
                    }
                }
            },
            simple: "data".to_string(),
        }
    );

    Ok(())
}
