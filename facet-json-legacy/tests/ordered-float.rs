use facet::Facet;
use facet_testhelpers::test;
use ordered_float::OrderedFloat;

/**
 * This test verifies that Facet can properly serialize and deserialize
 * enum struct variants.
 */

#[test]
fn transparent_ordered_float_1() {
    #[derive(Facet)]
    #[facet(transparent)]
    struct PixelDensity(f32);

    let deser: PixelDensity = facet_json_legacy::from_str(r#"1.4"#).unwrap();
    assert_eq!(deser.0, 1.4);
}

#[test]
fn transparent_ordered_float_2() {
    type PixelDensity = OrderedFloat<f32>;

    let deser: PixelDensity = facet_json_legacy::from_str(r#"1.4"#).unwrap();
    assert_eq!(deser.0, 1.4);
}

#[test]
fn transparent_ordered_float_3() {
    #[derive(Facet)]
    #[facet(transparent)]
    struct PixelDensity(OrderedFloat<f32>);

    let deser: PixelDensity = facet_json_legacy::from_str(r#"1.4"#).unwrap();
    assert_eq!(deser.0, OrderedFloat(1.4));
}
