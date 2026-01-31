use facet::Facet;
use facet_reflect2::{Op, Partial};

// =============================================================================
// Result with Imm (moving complete values)
// =============================================================================

#[test]
fn build_result_ok_with_imm() {
    let mut partial = Partial::alloc::<Result<u32, String>>().unwrap();

    let mut val: Result<u32, String> = Ok(42);
    partial.apply(&[Op::set().imm(&mut val)]).unwrap();

    let result: Result<u32, String> = partial.build().unwrap();
    assert_eq!(result, Ok(42));
}

#[test]
fn build_result_err_with_imm() {
    let mut partial = Partial::alloc::<Result<u32, String>>().unwrap();

    let mut val: Result<u32, String> = Err(String::from("error"));
    partial.apply(&[Op::set().imm(&mut val)]).unwrap();

    let result: Result<u32, String> = partial.build().unwrap();
    assert_eq!(result, Err(String::from("error")));
}

// =============================================================================
// Result with Build (building Ok/Err with complex inner)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct SuccessData {
    id: u64,
    message: String,
}

#[derive(Debug, PartialEq, Facet)]
struct ErrorData {
    code: i32,
    details: String,
}

#[test]
fn build_result_ok_struct_with_build() {
    let mut partial = Partial::alloc::<Result<SuccessData, ErrorData>>().unwrap();

    let mut id = 123u64;
    let mut message = String::from("success!");

    partial
        .apply(&[
            Op::set().at(0).build(), // variant 0 = Ok, enter SuccessData
            Op::set().at(0).imm(&mut id),
            Op::set().at(1).imm(&mut message),
            Op::end(),
        ])
        .unwrap();

    let result: Result<SuccessData, ErrorData> = partial.build().unwrap();
    assert_eq!(
        result,
        Ok(SuccessData {
            id: 123,
            message: String::from("success!")
        })
    );
}

#[test]
fn build_result_err_struct_with_build() {
    let mut partial = Partial::alloc::<Result<SuccessData, ErrorData>>().unwrap();

    let mut code = -1i32;
    let mut details = String::from("something went wrong");

    partial
        .apply(&[
            Op::set().at(1).build(), // variant 1 = Err, enter ErrorData
            Op::set().at(0).imm(&mut code),
            Op::set().at(1).imm(&mut details),
            Op::end(),
        ])
        .unwrap();

    let result: Result<SuccessData, ErrorData> = partial.build().unwrap();
    assert_eq!(
        result,
        Err(ErrorData {
            code: -1,
            details: String::from("something went wrong")
        })
    );
}

// =============================================================================
// Result with scalar types via Build
// =============================================================================

#[test]
fn build_result_ok_scalar_with_build() {
    let mut partial = Partial::alloc::<Result<i32, String>>().unwrap();

    let mut val = 42i32;

    partial
        .apply(&[
            Op::set().at(0).build(), // Ok variant
            Op::set().imm(&mut val),
            Op::end(),
        ])
        .unwrap();

    let result: Result<i32, String> = partial.build().unwrap();
    assert_eq!(result, Ok(42));
}

#[test]
fn build_result_err_string_with_build() {
    let mut partial = Partial::alloc::<Result<i32, String>>().unwrap();

    let mut err = String::from("error message");

    partial
        .apply(&[
            Op::set().at(1).build(), // Err variant
            Op::set().imm(&mut err),
            Op::end(),
        ])
        .unwrap();

    let result: Result<i32, String> = partial.build().unwrap();
    assert_eq!(result, Err(String::from("error message")));
}

// =============================================================================
// Result as struct field
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct ApiResponse {
    request_id: String,
    result: Result<String, String>,
}

#[test]
fn build_struct_with_result_field_ok() {
    let mut partial = Partial::alloc::<ApiResponse>().unwrap();

    let mut request_id = String::from("req-123");
    let mut data = String::from("response data");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut request_id),
            Op::set().at(1).build(), // enter Result field
            Op::set().at(0).build(), // Ok variant
            Op::set().imm(&mut data),
            Op::end(), // end Ok
            Op::end(), // end Result
        ])
        .unwrap();

    let result: ApiResponse = partial.build().unwrap();
    assert_eq!(
        result,
        ApiResponse {
            request_id: String::from("req-123"),
            result: Ok(String::from("response data"))
        }
    );
}

#[test]
fn build_struct_with_result_field_err() {
    let mut partial = Partial::alloc::<ApiResponse>().unwrap();

    let mut request_id = String::from("req-456");
    let mut error = String::from("not found");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut request_id),
            Op::set().at(1).build(), // enter Result field
            Op::set().at(1).build(), // Err variant
            Op::set().imm(&mut error),
            Op::end(), // end Err
            Op::end(), // end Result
        ])
        .unwrap();

    let result: ApiResponse = partial.build().unwrap();
    assert_eq!(
        result,
        ApiResponse {
            request_id: String::from("req-456"),
            result: Err(String::from("not found"))
        }
    );
}

// =============================================================================
// Nested Results
// =============================================================================

#[test]
fn build_nested_result_ok_ok() {
    let mut partial = Partial::alloc::<Result<Result<u32, String>, String>>().unwrap();

    let mut val = 42u32;

    partial
        .apply(&[
            Op::set().at(0).build(), // outer Ok
            Op::set().at(0).build(), // inner Ok
            Op::set().imm(&mut val),
            Op::end(), // end inner Result
            Op::end(), // end outer Result
        ])
        .unwrap();

    let result: Result<Result<u32, String>, String> = partial.build().unwrap();
    assert_eq!(result, Ok(Ok(42)));
}

#[test]
fn build_nested_result_ok_err() {
    let mut partial = Partial::alloc::<Result<Result<u32, String>, String>>().unwrap();

    let mut err = String::from("inner error");

    partial
        .apply(&[
            Op::set().at(0).build(), // outer Ok
            Op::set().at(1).build(), // inner Err
            Op::set().imm(&mut err),
            Op::end(), // end inner Result
            Op::end(), // end outer Result
        ])
        .unwrap();

    let result: Result<Result<u32, String>, String> = partial.build().unwrap();
    assert_eq!(result, Ok(Err(String::from("inner error"))));
}

#[test]
fn build_nested_result_err() {
    let mut partial = Partial::alloc::<Result<Result<u32, String>, String>>().unwrap();

    let mut err = String::from("outer error");

    partial
        .apply(&[
            Op::set().at(1).build(), // outer Err
            Op::set().imm(&mut err),
            Op::end(), // end outer Result
        ])
        .unwrap();

    let result: Result<Result<u32, String>, String> = partial.build().unwrap();
    assert_eq!(result, Err(String::from("outer error")));
}

// =============================================================================
// Vec of Results
// =============================================================================

#[test]
fn build_vec_of_results_with_build() {
    let mut partial = Partial::alloc::<Vec<Result<i32, String>>>().unwrap();

    let mut val1 = 1i32;
    let mut err = String::from("failed");
    let mut val2 = 3i32;

    partial
        .apply(&[
            Op::set().build(), // initialize Vec
            // First: Ok(1)
            Op::push().build(),
            Op::set().at(0).build(),
            Op::set().imm(&mut val1),
            Op::end(), // end Ok
            Op::end(), // end Result
            // Second: Err("failed")
            Op::push().build(),
            Op::set().at(1).build(),
            Op::set().imm(&mut err),
            Op::end(), // end Err
            Op::end(), // end Result
            // Third: Ok(3)
            Op::push().build(),
            Op::set().at(0).build(),
            Op::set().imm(&mut val2),
            Op::end(), // end Ok
            Op::end(), // end Result
        ])
        .unwrap();

    let result: Vec<Result<i32, String>> = partial.build().unwrap();
    assert_eq!(result, vec![Ok(1), Err(String::from("failed")), Ok(3)]);
}
