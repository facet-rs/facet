#[cfg(feature = "function")]
use facet::{facet_fn, fn_shape};

#[cfg(feature = "function")]
#[test]
fn simple_function_with_basic_types() {
    #[facet_fn]
    fn add(x: i32, y: i32) -> i32 {
        x + y
    }

    // Test function works normally
    assert_eq!(add(2, 3), 5);

    // Test shape metadata
    let shape = fn_shape!(add);
    assert_eq!(shape.name, "add");
    assert_eq!(shape.param_count, 2);
    assert_eq!(shape.param_names, &["x", "y"]);
}

#[cfg(feature = "function")]
#[test]
fn function_with_string_parameter() {
    #[facet_fn]
    fn greet(name: String) -> String {
        format!("Hello, {}!", name)
    }

    // Test function works normally
    assert_eq!(greet("World".to_string()), "Hello, World!");

    // Test shape metadata
    let shape = fn_shape!(greet);
    assert_eq!(shape.name, "greet");
    assert_eq!(shape.param_count, 1);
    assert_eq!(shape.param_names, &["name"]);
}

#[cfg(feature = "function")]
#[test]
fn function_with_no_parameters() {
    #[facet_fn]
    fn no_params() -> &'static str {
        "No parameters here!"
    }

    // Test function works normally
    assert_eq!(no_params(), "No parameters here!");

    // Test shape metadata
    let shape = fn_shape!(no_params);
    assert_eq!(shape.name, "no_params");
    assert_eq!(shape.param_count, 0);
    assert!(shape.param_names.is_empty());
}

#[cfg(feature = "function")]
#[test]
fn function_with_no_return_type() {
    #[facet_fn]
    fn side_effect_only(x: i32) {
        println!("Side effect: {}", x);
    }

    // Test function works normally
    side_effect_only(42);

    // Test shape metadata
    let shape = fn_shape!(side_effect_only);
    assert_eq!(shape.name, "side_effect_only");
    assert_eq!(shape.param_count, 1);
    assert_eq!(shape.param_names, &["x"]);
}

#[cfg(feature = "function")]
#[test]
fn generic_function_with_simple_bounds() {
    #[facet_fn]
    fn generic_add<T: core::ops::Add<Output = T>>(x: T, y: T) -> T {
        x + y
    }

    // Test function works normally with different types
    assert_eq!(generic_add::<i32>(4, 5), 9);
    assert_eq!(generic_add::<i64>(10, 20), 30);
    assert_eq!(generic_add::<usize>(7, 8), 15);

    // Test shape metadata for different instantiations
    let shape_i32 = fn_shape!(generic_add<i32>);
    assert_eq!(shape_i32.name, "generic_add");
    assert_eq!(shape_i32.param_count, 2);
    assert_eq!(shape_i32.param_names, &["x", "y"]);

    let shape_i64 = fn_shape!(generic_add<i64>);
    assert_eq!(shape_i64.name, "generic_add");
    assert_eq!(shape_i64.param_count, 2);
    assert_eq!(shape_i64.param_names, &["x", "y"]);

    let shape_usize = fn_shape!(generic_add<usize>);
    assert_eq!(shape_usize.name, "generic_add");
    assert_eq!(shape_usize.param_count, 2);
    assert_eq!(shape_usize.param_names, &["x", "y"]);
}

#[cfg(feature = "function")]
#[test]
fn function_shape_debug_output() {
    #[facet_fn]
    fn debug_test(a: u32, b: String) -> bool {
        true
    }

    let shape = fn_shape!(debug_test);
    let debug_output = format!("{:?}", shape);

    // Check that debug output contains expected information
    assert!(debug_output.contains("debug_test"));
    assert!(debug_output.contains("param_count: 2"));
    assert!(debug_output.contains("param_names"));
}

#[cfg(feature = "function")]
#[test]
fn function_shape_clone() {
    #[facet_fn]
    fn clone_test() -> i32 {
        42
    }

    let shape1 = fn_shape!(clone_test);
    let shape2 = shape1.clone();

    assert_eq!(shape1.name, shape2.name);
    assert_eq!(shape1.param_count, shape2.param_count);
    assert_eq!(shape1.param_names, shape2.param_names);
}

// #[cfg(feature = "function")]
// #[test]
// fn function_with_complex_parameter_types() {
//     #[facet_fn]
//     fn complex_params(
//         callback: fn(i32) -> String,
//         data: Vec<Option<u64>>,
//         result: Result<String, Box<dyn std::error::Error>>
//     ) -> HashMap<String, Vec<i32>> {
//         HashMap::new()
//     }
//
//     // Test function works normally
//     let _map = complex_params(
//         |x| x.to_string(),
//         vec![Some(42), None],
//         Ok("test".to_string())
//     );
//
//     // Test shape metadata
//     let shape = fn_shape!(complex_params);
//     assert_eq!(shape.name, "complex_params");
//     assert_eq!(shape.param_count, 3);
//     assert_eq!(shape.param_names, &["callback", "data", "result"]);
// }

// #[cfg(feature = "function")]
// #[test]
// fn function_with_lifetime_parameters() {
//     #[facet_fn]
//     fn with_lifetimes<'a>(s: &'a str, data: &'a [u8]) -> &'a str {
//         s
//     }
//
//     // Test function works normally
//     let test_str = "hello world";
//     let test_bytes = test_str.as_bytes();
//     assert_eq!(with_lifetimes(test_str, test_bytes), test_str);
//
//     // Test shape metadata
//     let shape = fn_shape!(with_lifetimes);
//     assert_eq!(shape.name, "with_lifetimes");
//     assert_eq!(shape.param_count, 2);
//     assert_eq!(shape.param_names, &["s", "data"]);
// }

// #[cfg(feature = "function")]
// #[test]
// fn function_with_mutable_references() {
//     #[facet_fn]
//     fn with_mut_refs(x: &mut i32, y: &mut Vec<String>) -> usize {
//         *x += 1;
//         y.len()
//     }
//
//     // Test function works normally
//     let mut test_int = 5;
//     let mut test_vec = vec!["hello".to_string(), "world".to_string()];
//     let result = with_mut_refs(&mut test_int, &mut test_vec);
//     assert_eq!(test_int, 6);
//     assert_eq!(result, 2);
//
//     // Test shape metadata
//     let shape = fn_shape!(with_mut_refs);
//     assert_eq!(shape.name, "with_mut_refs");
//     assert_eq!(shape.param_count, 2);
//     assert_eq!(shape.param_names, &["x", "y"]);
// }

// #[cfg(feature = "function")]
// #[test]
// fn generic_function_with_complex_bounds() {
//     #[facet_fn]
//     fn bounded_generics<T: Clone + Send, U: Iterator<Item = T>>(data: U) -> Vec<T> {
//         data.collect()
//     }
//
//     // Test function works normally
//     let input = vec![1, 2, 3, 4, 5];
//     let result = bounded_generics::<i32, std::vec::IntoIter<i32>>(input.into_iter());
//     assert_eq!(result, vec![1, 2, 3, 4, 5]);
//
//     // Test shape metadata
//     let shape = fn_shape!(bounded_generics<i32, std::vec::IntoIter<i32>>);
//     assert_eq!(shape.name, "bounded_generics");
//     assert_eq!(shape.param_count, 1);
//     assert_eq!(shape.param_names, &["data"]);
// }
