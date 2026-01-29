use bumpalo::Bump;
use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn test_result_building_ok() {
    let mut wip = Partial::alloc::<Result<String, i32>>().unwrap();

    // Build Ok("hello")
    wip = wip.begin_ok().unwrap();
    wip = wip.set("hello".to_string()).unwrap();
    wip = wip.end().unwrap();

    let result_value = wip
        .build()
        .unwrap()
        .materialize::<Result<String, i32>>()
        .unwrap();
    assert_eq!(result_value, Ok("hello".to_string()));
}

#[test]
fn test_result_building_err() {
    let mut wip = Partial::alloc::<Result<String, i32>>().unwrap();

    // Build Err(42)
    wip = wip.begin_err().unwrap();
    wip = wip.set(42i32).unwrap();
    wip = wip.end().unwrap();

    let result_value = wip
        .build()
        .unwrap()
        .materialize::<Result<String, i32>>()
        .unwrap();
    assert_eq!(result_value, Err(42));
}

#[test]
fn test_result_building_ok_complex_type() {
    let mut wip = Partial::alloc::<Result<Vec<i32>, String>>().unwrap();

    // Build Ok(vec![1, 2, 3])
    wip = wip.begin_ok().unwrap();
    wip = wip.set(vec![1, 2, 3]).unwrap();
    wip = wip.end().unwrap();

    let result_value = wip
        .build()
        .unwrap()
        .materialize::<Result<Vec<i32>, String>>()
        .unwrap();
    assert_eq!(result_value, Ok(vec![1, 2, 3]));
}

#[test]
fn test_result_building_err_complex_type() {
    let mut wip = Partial::alloc::<Result<i32, Vec<String>>>().unwrap();

    // Build Err(vec!["error1", "error2"])
    wip = wip.begin_err().unwrap();
    wip = wip
        .set(vec!["error1".to_string(), "error2".to_string()])
        .unwrap();
    wip = wip.end().unwrap();

    let result_value = wip
        .build()
        .unwrap()
        .materialize::<Result<i32, Vec<String>>>()
        .unwrap();
    assert_eq!(
        result_value,
        Err(vec!["error1".to_string(), "error2".to_string()])
    );
}

#[test]
fn test_result_in_struct() {
    #[derive(facet::Facet, Debug, PartialEq)]
    struct TestStruct {
        success: Result<String, i32>,
        failure: Result<String, i32>,
    }

    let bump = Bump::new(); let mut wip = Partial::alloc::<TestStruct>(&bump).unwrap();

    // Build the success field as Ok
    wip = wip.begin_nth_field(0).unwrap();
    wip = wip.begin_ok().unwrap();
    wip = wip.set("success".to_string()).unwrap();
    wip = wip.end().unwrap();
    wip = wip.end().unwrap();

    // Build the failure field as Err
    wip = wip.begin_nth_field(1).unwrap();
    wip = wip.begin_err().unwrap();
    wip = wip.set(404i32).unwrap();
    wip = wip.end().unwrap();
    wip = wip.end().unwrap();

    let struct_value = wip.build().unwrap().materialize::<TestStruct>().unwrap();
    assert_eq!(
        struct_value,
        TestStruct {
            success: Ok("success".to_string()),
            failure: Err(404),
        }
    );
}

#[test]
fn explore_result_shape() {
    // Explore the shape of Result<String, i32> to understand its structure
    let wip = Partial::alloc::<Result<String, i32>>().unwrap();

    println!("Result<String, i32> shape: {:?}", wip.shape());

    if let facet_core::Def::Result(result_def) = wip.shape().def {
        println!("Ok type: {:?}", result_def.t());
        println!("Err type: {:?}", result_def.e());
        println!("Result vtable: {:?}", result_def.vtable);
    }
}

use facet_testhelpers::IPanic;

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*)
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {{ let _ = $($tt)*; }};
}

#[test]
fn result_uninit() -> Result<(), IPanic> {
    assert_snapshot!(
        Partial::alloc::<Result<f64, String>>()?
            .build()
            .unwrap_err()
    );
    Ok(())
}
