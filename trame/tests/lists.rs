use facet::Facet;
use facet_reflect2::{Op, Partial};

// =============================================================================
// Basic Vec tests
// =============================================================================

#[test]
fn build_empty_vec() {
    let mut partial = Partial::alloc::<Vec<u32>>().unwrap();

    // For root-level lists, no End needed - Build initializes the list
    partial.apply(&[Op::set().stage()]).unwrap();

    let result: Vec<u32> = partial.build().unwrap();
    assert!(result.is_empty());
}

#[test]
fn build_vec_with_imm_elements() {
    let mut partial = Partial::alloc::<Vec<u32>>().unwrap();

    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;

    // For root-level lists, no End needed
    partial
        .apply(&[
            Op::set().stage_with_capacity(3),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
        ])
        .unwrap();

    let result: Vec<u32> = partial.build().unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn build_vec_with_default_elements() {
    let mut partial = Partial::alloc::<Vec<u32>>().unwrap();

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().default(),
            Op::set().append().default(),
        ])
        .unwrap();

    let result: Vec<u32> = partial.build().unwrap();
    assert_eq!(result, vec![0, 0]);
}

#[test]
fn build_vec_of_strings() {
    let mut partial = Partial::alloc::<Vec<String>>().unwrap();

    let mut hello = String::from("hello");
    let mut world = String::from("world");

    partial
        .apply(&[
            Op::set().stage_with_capacity(2),
            Op::set().append().imm(&mut hello),
            Op::set().append().imm(&mut world),
        ])
        .unwrap();
    std::mem::forget(hello);
    std::mem::forget(world);

    let result: Vec<String> = partial.build().unwrap();
    assert_eq!(result, vec!["hello", "world"]);
}

// =============================================================================
// Vec with complex elements (Push with Build)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn build_vec_of_structs_with_build() {
    let mut partial = Partial::alloc::<Vec<Point>>().unwrap();

    let mut x1 = 1i32;
    let mut y1 = 2i32;
    let mut x2 = 3i32;
    let mut y2 = 4i32;

    partial
        .apply(&[
            Op::set().stage_with_capacity(2),
            // First Point - build field by field
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut x1),
            Op::set().at(1).imm(&mut y1),
            Op::End,
            // Second Point - build field by field
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut x2),
            Op::set().at(1).imm(&mut y2),
            Op::End,
            // No End for root-level Vec
        ])
        .unwrap();

    let result: Vec<Point> = partial.build().unwrap();
    assert_eq!(result, vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]);
}

#[test]
fn build_vec_of_strings_with_build() {
    // Build String elements using Default (empty string)
    let mut partial = Partial::alloc::<Vec<String>>().unwrap();

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().stage(),
            Op::set().default(), // String::default() = ""
            Op::End,
            Op::set().append().stage(),
            Op::set().default(),
            Op::End,
            // No End for root-level Vec
        ])
        .unwrap();

    let result: Vec<String> = partial.build().unwrap();
    assert_eq!(result, vec![String::new(), String::new()]);
}

// =============================================================================
// Struct with Vec field
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Config {
    name: String,
    servers: Vec<String>,
}

#[test]
fn build_struct_with_vec_field() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let mut name = String::from("my-config");
    let mut server1 = String::from("server1.example.com");
    let mut server2 = String::from("server2.example.com");

    partial
        .apply(&[
            // Set name field
            Op::set().at(0).imm(&mut name),
            // Build servers field
            Op::set().at(1).stage_with_capacity(2),
            Op::set().append().imm(&mut server1),
            Op::set().append().imm(&mut server2),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(name);
    std::mem::forget(server1);
    std::mem::forget(server2);

    let result: Config = partial.build().unwrap();
    assert_eq!(
        result,
        Config {
            name: String::from("my-config"),
            servers: vec![
                String::from("server1.example.com"),
                String::from("server2.example.com")
            ],
        }
    );
}

#[test]
fn build_struct_with_empty_vec_field() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let mut name = String::from("empty-config");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut name),
            Op::set().at(1).stage(), // empty vec
            Op::End,
        ])
        .unwrap();
    std::mem::forget(name);

    let result: Config = partial.build().unwrap();
    assert_eq!(
        result,
        Config {
            name: String::from("empty-config"),
            servers: vec![],
        }
    );
}

#[derive(Debug, PartialEq, Facet)]
struct Team {
    name: String,
    members: Vec<Person>,
}

#[derive(Debug, PartialEq, Facet)]
struct Person {
    name: String,
    age: u32,
}

#[test]
fn build_struct_with_vec_of_structs() {
    let mut partial = Partial::alloc::<Team>().unwrap();

    let mut team_name = String::from("Engineering");
    let mut alice = String::from("Alice");
    let mut bob = String::from("Bob");
    let mut age1 = 30u32;
    let mut age2 = 25u32;

    partial
        .apply(&[
            Op::set().at(0).imm(&mut team_name),
            Op::set().at(1).stage_with_capacity(2),
            // First person - build field by field
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut alice),
            Op::set().at(1).imm(&mut age1),
            Op::End,
            // Second person - build field by field
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut bob),
            Op::set().at(1).imm(&mut age2),
            Op::End,
            Op::End,
        ])
        .unwrap();
    std::mem::forget(team_name);
    std::mem::forget(alice);
    std::mem::forget(bob);

    let result: Team = partial.build().unwrap();
    assert_eq!(
        result,
        Team {
            name: String::from("Engineering"),
            members: vec![
                Person {
                    name: String::from("Alice"),
                    age: 30
                },
                Person {
                    name: String::from("Bob"),
                    age: 25
                },
            ],
        }
    );
}

// =============================================================================
// Nested Vec
// =============================================================================

#[test]
fn build_vec_of_vecs() {
    let mut partial = Partial::alloc::<Vec<Vec<u32>>>().unwrap();

    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;
    let mut d = 4u32;

    partial
        .apply(&[
            Op::set().stage_with_capacity(2),
            // First inner vec
            Op::set().append().stage(),
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::End,
            // Second inner vec
            Op::set().append().stage(),
            Op::set().append().imm(&mut c),
            Op::set().append().imm(&mut d),
            Op::End,
            // No End for root-level Vec
        ])
        .unwrap();

    let result: Vec<Vec<u32>> = partial.build().unwrap();
    assert_eq!(result, vec![vec![1, 2], vec![3, 4]]);
}

#[test]
fn build_vec_of_vecs_empty_inner() {
    let mut partial = Partial::alloc::<Vec<Vec<u32>>>().unwrap();

    let mut a = 1u32;

    partial
        .apply(&[
            Op::set().stage(),
            // First inner vec - empty
            Op::set().append().stage(),
            Op::End,
            // Second inner vec - one element
            Op::set().append().stage(),
            Op::set().append().imm(&mut a),
            Op::End,
            // Third inner vec - empty
            Op::set().append().stage(),
            Op::End,
            // No End for root-level Vec
        ])
        .unwrap();

    let result: Vec<Vec<u32>> = partial.build().unwrap();
    assert_eq!(result, vec![vec![], vec![1], vec![]]);
}

// =============================================================================
// Vec with Option elements
// =============================================================================

#[test]
fn build_vec_of_options_with_imm() {
    let mut partial = Partial::alloc::<Vec<Option<u32>>>().unwrap();

    let mut some_val1 = Some(42u32);
    let mut none_val: Option<u32> = None;
    let mut some_val2 = Some(42u32);

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut some_val1),
            Op::set().append().imm(&mut none_val),
            Op::set().append().imm(&mut some_val2),
        ])
        .unwrap();

    let result: Vec<Option<u32>> = partial.build().unwrap();
    assert_eq!(result, vec![Some(42), None, Some(42)]);
}

// =============================================================================
// Vec with enum elements
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
    Pending(u32),
}

#[test]
fn build_vec_of_enums_with_imm() {
    let mut partial = Partial::alloc::<Vec<Status>>().unwrap();

    let mut active = Status::Active;
    let mut pending = Status::Pending(5);

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut active),
            Op::set().append().imm(&mut pending),
        ])
        .unwrap();

    let result: Vec<Status> = partial.build().unwrap();
    assert_eq!(result, vec![Status::Active, Status::Pending(5)]);
}

#[test]
fn build_vec_of_enums_with_build() {
    let mut partial = Partial::alloc::<Vec<Status>>().unwrap();

    let mut count = 10u32;

    partial
        .apply(&[
            Op::set().stage(),
            // Active variant (unit)
            Op::set().append().stage(),
            Op::set().at(0).default(), // Select variant 0
            Op::End,
            // Pending variant with data
            Op::set().append().stage(),
            Op::set().at(2).imm(&mut count), // Select variant 2, set field
            Op::End,
            // No End for root-level Vec
        ])
        .unwrap();

    let result: Vec<Status> = partial.build().unwrap();
    assert_eq!(result, vec![Status::Active, Status::Pending(10)]);
}

// =============================================================================
// Drop/cleanup tests
// =============================================================================

#[test]
fn drop_partial_vec_mid_construction() {
    // Start building a Vec<String>, push some elements, then drop without finishing
    let mut partial = Partial::alloc::<Vec<String>>().unwrap();

    let mut hello = String::from("hello");
    let mut world = String::from("world");

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().imm(&mut hello),
            Op::set().append().imm(&mut world),
            // Don't call End - just drop
        ])
        .unwrap();
    std::mem::forget(hello);
    std::mem::forget(world);

    // Drop the partial - should clean up the Vec and its elements
    drop(partial);
    // If we get here without Miri complaining, we're good
}

#[test]
fn drop_partial_vec_of_structs_mid_element() {
    // Start building a Vec<Point>, start building an element, then drop
    let mut partial = Partial::alloc::<Vec<Point>>().unwrap();

    let mut x = 1i32;

    let result = partial.apply(&[
        Op::set().stage(),
        Op::set().append().stage(),
        Op::set().at(0).imm(&mut x),
        // Don't set y, don't End - just drop
    ]);

    // This might error (incomplete element) or succeed depending on implementation
    // Either way, dropping should be safe
    let _ = result;
    drop(partial);
}

// =============================================================================
// Error cases
// =============================================================================

#[test]
fn push_on_non_list_errors() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let mut val = 42u32;
    let err = partial
        .apply(&[Op::set().append().imm(&mut val)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        facet_reflect2::ReflectErrorKind::NotAList
    ));
}

#[test]
fn push_wrong_element_type_errors() {
    let mut partial = Partial::alloc::<Vec<u32>>().unwrap();

    let mut wrong_type = String::from("not a u32");

    partial.apply(&[Op::set().stage()]).unwrap();
    let err = partial
        .apply(&[Op::set().append().imm(&mut wrong_type)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        facet_reflect2::ReflectErrorKind::ShapeMismatch { .. }
    ));
}

// Regression test for double-free bug found by AFL fuzzer.
// When overwriting a Vec field with Build, we must drop the old Vec first.
// The bug was: Source::Build didn't call prepare_field_for_overwrite(),
// so the old Vec wasn't dropped before creating the new one. Then when
// dropping the partial, both the old and new Vec would be dropped.
#[test]
fn overwrite_vec_field_with_build_no_double_free() {
    #[derive(Debug, Facet)]
    struct HasVec {
        items: Vec<u32>,
    }

    let mut partial = Partial::alloc::<HasVec>().unwrap();

    // Step 1: Build the Vec field and complete it
    partial
        .apply(&[Op::set().at(0).stage(), Op::end()])
        .unwrap();

    // Step 2: Build the same field again - this should drop the old Vec
    partial
        .apply(&[Op::set().at(0).stage(), Op::end()])
        .unwrap();

    // Step 3: Drop - should not double-free
    drop(partial);
}
