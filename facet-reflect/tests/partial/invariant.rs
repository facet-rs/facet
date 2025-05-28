use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn build_with_invariants() {
    #[derive(Facet, PartialEq, Debug)]
    #[facet(invariants = MyNonZeroU8::invariants)]
    struct MyNonZeroU8(u8);

    impl MyNonZeroU8 {
        fn invariants(&self) -> bool {
            self.0 != 0
        }
    }

    let mut partial = Partial::alloc::<MyNonZeroU8>()?;
    partial.begin_nth_field(0)?;
    partial.set(42u8)?;
    partial.end()?;
    let wip: MyNonZeroU8 = *partial.build()?;
    assert_eq!(wip, MyNonZeroU8(42));

    let mut partial = Partial::alloc::<MyNonZeroU8>()?;
    partial.begin_nth_field(0)?;
    partial.set(0_u8)?;
    partial.end()?;
    let result = partial.build();
    assert!(result.is_err());
}

// Enums don't support invariants, so we test with a struct containing an enum
#[test]
fn build_struct_with_enum_field() {
    #[derive(Facet, PartialEq, Debug)]
    #[repr(C)]
    enum Range {
        Low(u8),
        High(u8),
    }

    #[derive(Facet, PartialEq, Debug)]
    #[facet(invariants = ValidatedRange::invariants)]
    struct ValidatedRange {
        range: Range,
    }

    impl ValidatedRange {
        fn invariants(&self) -> bool {
            match &self.range {
                Range::Low(v) => *v <= 50,
                Range::High(v) => *v > 50,
            }
        }
    }

    // Valid Low variant
    let mut partial = Partial::alloc::<ValidatedRange>()?;
    partial.begin_field("range")?;
    partial.select_variant_named("Low")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(25u8)?;
    partial.end()?;
    partial.end()?;
    let value: ValidatedRange = *partial.build()?;
    assert_eq!(value.range, Range::Low(25));

    // Invalid Low variant (too high)
    let mut partial = Partial::alloc::<ValidatedRange>()?;
    partial.begin_field("range")?;
    partial.select_variant_named("Low")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(75u8)?;
    partial.end()?;
    partial.end()?;
    let result = partial.build();
    assert!(result.is_err());

    // Valid High variant
    let mut partial = Partial::alloc::<ValidatedRange>()?;
    partial.begin_field("range")?;
    partial.select_variant_named("High")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(75u8)?;
    partial.end()?;
    partial.end()?;
    let value: ValidatedRange = *partial.build()?;
    assert_eq!(value.range, Range::High(75));

    // Invalid High variant (too low)
    let mut partial = Partial::alloc::<ValidatedRange>()?;
    partial.begin_field("range")?;
    partial.select_variant_named("High")?;
    partial.begin_nth_enum_field(0)?;
    partial.set(25u8)?;
    partial.end()?;
    partial.end()?;
    let result = partial.build();
    assert!(result.is_err());
}

#[test]
fn build_nested_with_invariants() {
    #[derive(Facet, PartialEq, Debug)]
    #[facet(invariants = Point::invariants)]
    struct Point {
        x: i32,
        y: i32,
    }

    impl Point {
        fn invariants(&self) -> bool {
            // Point must be in first quadrant
            self.x >= 0 && self.y >= 0
        }
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Container {
        point: Point,
    }

    // Valid point
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("point")?;
    partial.set_field("x", 10i32)?;
    partial.set_field("y", 20i32)?;
    partial.end()?;
    let container: Container = *partial.build()?;
    assert_eq!(container.point, Point { x: 10, y: 20 });

    // Invalid point (negative x)
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("point")?;
    partial.set_field("x", -10i32)?;
    partial.set_field("y", 20i32)?;
    partial.end()?;
    let result = partial.build();
    // This should succeed because Container itself has no invariants,
    // only Point does, and we're not checking nested invariants yet
    assert!(result.is_ok());
}
