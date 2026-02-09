use facet_core::Facet;
use facet_testhelpers::test;

#[test]
fn option_type_name_includes_generic_parameter() {
    let shape = <Option<String> as Facet>::SHAPE;
    assert_eq!(shape.to_string(), "Option<String>");
}

#[test]
fn result_type_name_includes_both_generic_parameters() {
    let shape = <Result<String, u32> as Facet>::SHAPE;
    assert_eq!(shape.to_string(), "Result<String, u32>");
}
