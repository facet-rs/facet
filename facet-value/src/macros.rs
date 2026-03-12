//! Macros for constructing `Value` instances.

/// Creates a [`Value`](crate::Value) from a JSON-like syntax.
///
/// # Examples
///
/// ```
/// use facet_value::{Value, value};
///
/// // Null
/// let v = value!(null);
/// assert!(v.is_null());
///
/// // Booleans
/// let v = value!(true);
/// assert_eq!(v.as_bool(), Some(true));
///
/// // Numbers
/// let v = value!(42);
/// assert_eq!(v.as_number().unwrap().to_i64(), Some(42));
///
/// // Strings
/// let v = value!("hello");
/// assert_eq!(v.as_string().unwrap().as_str(), "hello");
///
/// // Arrays
/// let v = value!([1, 2, 3]);
/// assert_eq!(v.as_array().unwrap().len(), 3);
///
/// // Objects
/// let v = value!({
///     "name": "Alice",
///     "age": 30,
///     "active": true
/// });
/// assert_eq!(v.as_object().unwrap().get("name").unwrap().as_string().unwrap().as_str(), "Alice");
///
/// // Nested structures
/// let v = value!({
///     "users": [
///         { "name": "Alice", "age": 30 },
///         { "name": "Bob", "age": 25 }
///     ],
///     "count": 2
/// });
/// ```
///
/// # Variable interpolation
///
/// You can interpolate variables using parentheses:
///
/// ```
/// use facet_value::{Value, value};
///
/// let name = "Alice";
/// let age = 30;
///
/// let v = value!({
///     "name": (name),
///     "age": (age)
/// });
/// ```
#[macro_export]
macro_rules! value {
    // Null
    (null) => {
        $crate::Value::NULL
    };

    // Boolean true
    (true) => {
        $crate::Value::TRUE
    };

    // Boolean false
    (false) => {
        $crate::Value::FALSE
    };

    // Empty array
    ([]) => {
        $crate::Value::from($crate::VArray::new())
    };

    // Array with elements
    ([ $($elem:tt),* $(,)? ]) => {{
        let mut arr = $crate::VArray::new();
        $(
            arr.push($crate::value!($elem));
        )*
        $crate::Value::from(arr)
    }};

    // Empty object
    ({}) => {
        $crate::Value::from($crate::VObject::new())
    };

    // Object with key-value pairs
    ({ $($key:tt : $value:tt),* $(,)? }) => {{
        let mut obj = $crate::VObject::new();
        $(
            obj.insert($key, $crate::value!($value));
        )*
        $crate::Value::from(obj)
    }};

    // Parenthesized expression (variable interpolation)
    (( $expr:expr )) => {
        $crate::Value::from($expr)
    };

    // Literal expression (numbers, strings, etc.)
    ($other:expr) => {
        $crate::Value::from($other)
    };
}

#[cfg(test)]
mod tests {
    use crate::{VArray, Value};

    #[test]
    fn test_null() {
        let v = value!(null);
        assert!(v.is_null());
    }

    #[test]
    fn test_booleans() {
        assert_eq!(value!(true), Value::TRUE);
        assert_eq!(value!(false), Value::FALSE);
    }

    #[test]
    fn test_numbers() {
        let v = value!(42);
        assert_eq!(v.as_number().unwrap().to_i64(), Some(42));

        let v = value!(-17);
        assert_eq!(v.as_number().unwrap().to_i64(), Some(-17));

        let v = value!(1.5);
        assert!((v.as_number().unwrap().to_f64().unwrap() - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_strings() {
        let v = value!("hello");
        assert_eq!(v.as_string().unwrap().as_str(), "hello");

        let v = value!("hello world with spaces");
        assert_eq!(v.as_string().unwrap().as_str(), "hello world with spaces");
    }

    #[test]
    fn test_empty_array() {
        let v = value!([]);
        assert!(v.is_array());
        assert!(v.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_array_of_numbers() {
        let v = value!([1, 2, 3]);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_number().unwrap().to_i64(), Some(1));
        assert_eq!(arr[1].as_number().unwrap().to_i64(), Some(2));
        assert_eq!(arr[2].as_number().unwrap().to_i64(), Some(3));
    }

    #[test]
    fn test_array_mixed_types() {
        let v = value!([1, "two", true, null]);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0].as_number().unwrap().to_i64(), Some(1));
        assert_eq!(arr[1].as_string().unwrap().as_str(), "two");
        assert_eq!(arr[2].as_bool(), Some(true));
        assert!(arr[3].is_null());
    }

    #[test]
    fn test_empty_object() {
        let v = value!({});
        assert!(v.is_object());
        assert!(v.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_object_simple() {
        let v = value!({
            "name": "Alice",
            "age": 30,
            "active": true
        });
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(obj["name"].as_string().unwrap().as_str(), "Alice");
        assert_eq!(obj["age"].as_number().unwrap().to_i64(), Some(30));
        assert_eq!(obj["active"].as_bool(), Some(true));
    }

    #[test]
    fn test_nested_structure() {
        let v = value!({
            "users": [
                { "name": "Alice", "age": 30 },
                { "name": "Bob", "age": 25 }
            ],
            "count": 2
        });

        let obj = v.as_object().unwrap();
        assert_eq!(obj["count"].as_number().unwrap().to_i64(), Some(2));

        let users = obj["users"].as_array().unwrap();
        assert_eq!(users.len(), 2);

        let alice = users[0].as_object().unwrap();
        assert_eq!(alice["name"].as_string().unwrap().as_str(), "Alice");
        assert_eq!(alice["age"].as_number().unwrap().to_i64(), Some(30));

        let bob = users[1].as_object().unwrap();
        assert_eq!(bob["name"].as_string().unwrap().as_str(), "Bob");
        assert_eq!(bob["age"].as_number().unwrap().to_i64(), Some(25));
    }

    #[test]
    fn test_variable_interpolation() {
        let name = "Alice";
        let age = 30i64;

        let v = value!({
            "name": (name),
            "age": (age)
        });

        let obj = v.as_object().unwrap();
        assert_eq!(obj["name"].as_string().unwrap().as_str(), "Alice");
        assert_eq!(obj["age"].as_number().unwrap().to_i64(), Some(30));
    }

    #[test]
    fn test_array_interpolation() {
        let items = vec![1i64, 2, 3];
        let arr: VArray = items.into_iter().collect();

        let v = value!({
            "items": (arr)
        });

        let obj = v.as_object().unwrap();
        assert_eq!(obj["items"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_trailing_comma() {
        // Arrays with trailing comma
        let v = value!([1, 2, 3,]);
        assert_eq!(v.as_array().unwrap().len(), 3);

        // Objects with trailing comma
        let v = value!({
            "a": 1,
            "b": 2,
        });
        assert_eq!(v.as_object().unwrap().len(), 2);
    }

    #[test]
    fn test_deeply_nested() {
        let v = value!({
            "level1": {
                "level2": {
                    "level3": {
                        "data": [1, 2, 3]
                    }
                }
            }
        });

        let data = &v.as_object().unwrap()["level1"].as_object().unwrap()["level2"]
            .as_object()
            .unwrap()["level3"]
            .as_object()
            .unwrap()["data"];

        assert_eq!(data.as_array().unwrap().len(), 3);
    }
}
