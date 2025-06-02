use bigapi_types_facet::generate_mock_catalog;
use facet_pretty::FacetPretty;

pub fn do_ser_stuff() {
    // Generate a mock catalog
    let catalog = generate_mock_catalog();

    // Serialize the catalog to JSON
    let serialized = facet_json::to_string(&catalog);

    let serialized = std::fs::read_to_string("/tmp/blah.json").unwrap();

    println!("Serialized catalog JSON.\n{}", serialized);

    // Deserialize back to a Catalog struct
    let deserialized: bigapi_types_facet::Catalog =
        facet_json::from_str(&serialized).expect("Failed to deserialize catalog!");

    println!("Deserialized catalog struct:\n{}", deserialized.pretty());
}
