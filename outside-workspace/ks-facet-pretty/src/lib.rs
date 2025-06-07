use facet_pretty::FacetPretty;
use ks_types::Catalog;

pub fn pretty_print(catalog: &Catalog) {
    eprintln!("{}", catalog.pretty());
}
