use ks_types::Catalog;

pub fn catalog_to_json(catalog: &Catalog) -> String {
    serde_json::to_string(catalog).unwrap()
}
