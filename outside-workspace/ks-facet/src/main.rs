fn main() {
    let catalog = ks_mock::generate_mock_catalog();

    // Serialize the catalog to JSON
    let serialized = ks_facet_json_write::catalog_to_json(&catalog);
    eprintln!("Serialized catalog JSON:\n{}", &serialized);

    let deserialized = ks_facet_json_read::catalog_from_json(&serialized);
    ks_facet_pretty::pretty_print(&deserialized);
}
