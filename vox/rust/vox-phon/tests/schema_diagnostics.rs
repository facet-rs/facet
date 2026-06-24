use facet::Facet;

#[test]
fn schema_for_reference_tuple_uses_shape_display_not_identifier() {
    let shape = <(&'static String, String) as Facet>::SHAPE;

    assert_eq!(shape.type_identifier, "(…)");
    assert_eq!(shape.to_string(), "(&String, String)");

    let bytes = vox_phon::schema_bytes_for_shape(shape)
        .expect("&String tuple should derive through its pointee schema");

    assert!(
        !bytes.is_empty(),
        "schema bytes should be emitted for reference tuple"
    );
}

#[test]
fn reference_tuple_encodes_and_decodes() {
    let name = String::from("facet");
    let value = (&name, String::from("phon"));
    let writer =
        vox_phon::parse_schema_bytes(&vox_phon::schema_bytes::<(&String, String)>().unwrap())
            .unwrap();
    let bytes = vox_phon::to_vec(&value).unwrap();
    let program = vox_phon::build_decode_program::<(&String, String)>(&writer).unwrap();

    let decoded =
        vox_phon::decode_with_program_retained::<(&String, String)>(&program, &bytes).unwrap();
    let decoded = decoded.get();

    assert_eq!(decoded.0, "facet");
    assert_eq!(decoded.1, "phon");
}
