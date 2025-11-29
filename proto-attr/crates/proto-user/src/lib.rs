//! Test crate to validate cross-crate attribute usage.
//!
//! This simulates a user crate that depends on proto-ext and uses its attributes.

use proto_ext::Attr;
#[cfg(test)]
use proto_ext::Column;

/// Demonstrates cross-crate attribute parsing.
pub fn demo() -> Vec<Attr> {
    vec![
        // Unit variant
        proto_ext::__parse_attr!(skip),
        // Newtype variants
        proto_ext::__parse_attr!(rename("user_name")),
        proto_ext::__parse_attr!(rename = "user_name"),
        // Struct variant with various field combinations
        proto_ext::__parse_attr!(column),
        proto_ext::__parse_attr!(column()),
        proto_ext::__parse_attr!(column(name = "id")),
        proto_ext::__parse_attr!(column(primary_key)),
        proto_ext::__parse_attr!(column(name = "id", primary_key)),
        proto_ext::__parse_attr!(column(primary_key, name = "id")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_crate_parsing() {
        let attrs = demo();

        assert_eq!(attrs[0], Attr::Skip);
        assert_eq!(attrs[1], Attr::Rename("user_name"));
        assert_eq!(attrs[2], Attr::Rename("user_name"));
        assert_eq!(
            attrs[3],
            Attr::Column(Column {
                name: None,
                primary_key: false
            })
        );
        assert_eq!(
            attrs[4],
            Attr::Column(Column {
                name: None,
                primary_key: false
            })
        );
        assert_eq!(
            attrs[5],
            Attr::Column(Column {
                name: Some("id"),
                primary_key: false
            })
        );
        assert_eq!(
            attrs[6],
            Attr::Column(Column {
                name: None,
                primary_key: true
            })
        );
        assert_eq!(
            attrs[7],
            Attr::Column(Column {
                name: Some("id"),
                primary_key: true
            })
        );
        assert_eq!(
            attrs[8],
            Attr::Column(Column {
                name: Some("id"),
                primary_key: true
            })
        );
    }
}
