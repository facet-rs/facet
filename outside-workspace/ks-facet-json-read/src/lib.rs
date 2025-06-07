use ks_types::Catalog;

pub fn catalog_from_json(json: &str) -> Catalog {
    facet_json::from_str(json).unwrap()
}
