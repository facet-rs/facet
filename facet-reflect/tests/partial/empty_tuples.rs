use facet::Facet;
use facet_reflect::Partial;

#[test]
fn test_empty_tuple_always_initialized() {
    // Empty tuple should always be considered initialized
    let mut partial = Partial::alloc_shape(<()>::SHAPE).unwrap();

    // Check the shape
    let shape = partial.shape();
    if let facet_core::Type::User(facet_core::UserType::Struct(st)) = shape.ty {
        assert_eq!(st.fields.len(), 0, "Empty tuple should have 0 fields");
    }

    // Build should succeed immediately
    let built = partial.build().unwrap();
    let value: () = built.materialize().unwrap();
}

#[test]
fn test_nested_empty_tuple_field_check() {
    // (()) - tuple with one field that is ()
    let mut partial = Partial::alloc_shape(<((),)>::SHAPE).unwrap();

    // Check if field 0 is initialized - this should be true!
    let field_0_initialized = partial.is_field_set(0).unwrap();
    println!("Is field 0 of (()) initialized? {}", field_0_initialized);

    // Debug: Check the shape
    let shape = partial.shape();
    println!("Shape type: {:?}", shape.ty);

    if !field_0_initialized {
        // If not initialized, try to navigate to it
        partial.begin_nth_field(0).unwrap();

        // Now we're at type (), which should be considered complete
        let inner_shape = partial.innermost_shape();
        println!("Inner shape: {:?}", inner_shape);

        // No need to set anything - it's a ZST
        partial.end().unwrap();
    }

    // Build should succeed
    let built = partial.build().unwrap();
    let value: ((),) = built.materialize().unwrap();
    assert_eq!(value, ((),));
}

#[test]
fn test_double_empty_tuple() {
    // ((), ()) - tuple with two fields, both empty tuples
    let mut partial = Partial::alloc_shape(<((), ())>::SHAPE).unwrap();

    // Check if fields are initialized
    let field_0_initialized = partial.is_field_set(0).unwrap();
    let field_1_initialized = partial.is_field_set(1).unwrap();

    println!("Field 0 initialized: {}", field_0_initialized);
    println!("Field 1 initialized: {}", field_1_initialized);

    // If not initialized, try setting them
    if !field_0_initialized {
        partial.begin_nth_field(0).unwrap();
        partial.end().unwrap();
    }
    if !field_1_initialized {
        partial.begin_nth_field(1).unwrap();
        partial.end().unwrap();
    }

    // Build should succeed
    let built = partial.build().unwrap();
    let value: ((), ()) = built.materialize().unwrap();
    assert_eq!(value, ((), ()));
}

#[test]
fn test_deeply_nested_empty_tuple() {
    // (((),),) - deeply nested
    let mut partial = Partial::alloc_shape(<(((),),)>::SHAPE).unwrap();

    // Check if field 0 is initialized
    let field_0_initialized = partial.is_field_set(0).unwrap();
    println!(
        "Is field 0 of (((),),) initialized? {}",
        field_0_initialized
    );

    if !field_0_initialized {
        partial.begin_nth_field(0).unwrap(); // Now at ((),)

        let inner_field_0_initialized = partial.is_field_set(0).unwrap();
        println!(
            "Is field 0 of ((),) initialized? {}",
            inner_field_0_initialized
        );

        if !inner_field_0_initialized {
            partial.begin_nth_field(0).unwrap(); // Now at ()
            partial.end().unwrap(); // Back to ((),)
        }

        partial.end().unwrap(); // Back to (((),),)
    }

    // Build should succeed
    let built = partial.build().unwrap();
    let value: (((),),) = built.materialize().unwrap();
    assert_eq!(value, (((),),));
}

#[test]
fn test_is_field_set_for_nested_empty_tuples() {
    // Test various nested empty tuple configurations

    // (((),),) - field 0 is ((),) which contains only empty tuples
    let partial = Partial::alloc_shape(<(((),),)>::SHAPE).unwrap();
    let field_0_initialized = partial.is_field_set(0).unwrap();
    println!(
        "Is field 0 of (((),),) initialized? {}",
        field_0_initialized
    );
    assert!(
        !field_0_initialized,
        "Field 0 of (((),),) should NOT be considered initialized - it needs to be explicitly set"
    );

    // ((((),),),) - even deeper nesting
    let partial = Partial::alloc_shape(<((((),),),)>::SHAPE).unwrap();
    let field_0_initialized = partial.is_field_set(0).unwrap();
    println!(
        "Is field 0 of ((((),),),) initialized? {}",
        field_0_initialized
    );
    assert!(
        !field_0_initialized,
        "Field 0 of ((((),),),) should NOT be considered initialized"
    );

    // ((), (), ()) - multiple empty tuple fields
    let partial = Partial::alloc_shape(<((), (), ())>::SHAPE).unwrap();
    let field_0_initialized = partial.is_field_set(0).unwrap();
    let field_1_initialized = partial.is_field_set(1).unwrap();
    let field_2_initialized = partial.is_field_set(2).unwrap();
    println!(
        "Fields of ((), (), ()): {}, {}, {}",
        field_0_initialized, field_1_initialized, field_2_initialized
    );
    assert!(
        !field_0_initialized && !field_1_initialized && !field_2_initialized,
        "All fields should NOT be initialized - they need to be explicitly set"
    );

    // (((), ()), ()) - mixed nesting
    let partial = Partial::alloc_shape(<(((), ()), ())>::SHAPE).unwrap();
    let field_0_initialized = partial.is_field_set(0).unwrap();
    let field_1_initialized = partial.is_field_set(1).unwrap();
    println!(
        "Field 0 of (((), ()), ()) initialized? {}",
        field_0_initialized
    );
    println!(
        "Field 1 of (((), ()), ()) initialized? {}",
        field_1_initialized
    );
    assert!(
        !field_0_initialized,
        "Field 0 should NOT be initialized - it's a non-empty tuple"
    );
    assert!(
        !field_1_initialized,
        "Field 1 should NOT be initialized - even though it's an empty tuple"
    );
}

#[test]
fn test_building_nested_empty_tuples_without_navigation() {
    // With the new behavior, we should be able to build trivially constructible tuples

    // ((),) - should now work without navigation
    let mut partial = Partial::alloc_shape(<((),)>::SHAPE).unwrap();
    let result = partial.build();
    // This will still fail because build() checks actual initialization, not just is_field_set
    assert!(
        result.is_err(),
        "Still fails - build() requires actual initialization"
    );

    // Navigation is still required for build to succeed
    let mut partial = Partial::alloc_shape(<((),)>::SHAPE).unwrap();
    partial.begin_nth_field(0).unwrap();
    partial.end().unwrap();
    let built = partial.build().unwrap();
    let value: ((),) = built.materialize().unwrap();
    assert_eq!(value, ((),));
}
