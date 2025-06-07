use ks_types::Catalog;

pub fn catalog_from_json(json: &str) -> Catalog {
    serde_json::from_str(json).unwrap()
}
