use facet::Facet;

#[test]
fn schema_error_for_tuple_shape_uses_shape_display() {
    let shape = <(&'static String, String) as Facet>::SHAPE;

    assert_eq!(shape.type_identifier, "(…)");
    assert_eq!(shape.to_string(), "(&String, String)");

    let err = vox_phon::schema_bytes_for_shape(shape)
        .expect_err("&String is a Facet reference shape Phon does not derive yet")
        .to_string();

    assert!(
        err.contains("derive (&String, String):"),
        "schema error should include Shape display, got: {err}"
    );
    assert!(
        !err.contains("derive (…):"),
        "schema error must not use Shape::type_identifier, got: {err}"
    );
}
