fn main() {
    let catalog = ks_mock::generate_mock_catalog();

    // Serialize the catalog to JSON
    let serialized = ks_serde_json_write::catalog_to_json(&catalog);
    eprintln!("Serialized catalog JSON:\n{}", &serialized);

    let deserialized = ks_serde_json_read::catalog_from_json(&serialized);
    ks_debug::pretty_print(&deserialized);
}
