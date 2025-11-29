//! Test crate to validate cross-crate attribute usage.
//!
//! This simulates a user crate that depends on proto-ext and uses its attributes.

use proto_attr::Faket;
use proto_ext::Attr;
#[cfg(test)]
use proto_ext::Column;

/// Internal cache struct we want to skip from ORM generation
#[derive(Faket)]
#[faket(proto_ext::skip)]
pub struct InternalCache {
    pub data: Vec<u8>,
}

/// User model with full ORM attribute configuration
#[derive(Faket)]
pub struct User {
    /// Primary key with auto-increment
    #[faket(proto_ext::column(name = "id", primary_key, auto_increment))]
    pub id: i64,

    /// Custom column name mapping
    #[faket(proto_ext::column(name = "user_name"))]
    pub name: String,

    /// Nullable TEXT field
    #[faket(proto_ext::column(nullable, sql_type = "TEXT"))]
    pub bio: Option<String>,

    /// Skip sensitive field from serialization
    #[faket(proto_ext::skip)]
    pub password_hash: String,

    /// Rename for API compatibility
    #[faket(proto_ext::rename("email_address"))]
    pub email: String,
}

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

    fn default_column() -> Column {
        Column::default()
    }

    #[test]
    fn test_cross_crate_parsing() {
        let attrs = demo();

        assert_eq!(attrs[0], Attr::Skip);
        assert_eq!(attrs[1], Attr::Rename("user_name"));
        assert_eq!(attrs[2], Attr::Rename("user_name"));
        assert_eq!(attrs[3], Attr::Column(default_column()));
        assert_eq!(attrs[4], Attr::Column(default_column()));
        assert_eq!(
            attrs[5],
            Attr::Column(Column {
                name: Some("id"),
                ..default_column()
            })
        );
        assert_eq!(
            attrs[6],
            Attr::Column(Column {
                primary_key: true,
                ..default_column()
            })
        );
        assert_eq!(
            attrs[7],
            Attr::Column(Column {
                name: Some("id"),
                primary_key: true,
                ..default_column()
            })
        );
        assert_eq!(
            attrs[8],
            Attr::Column(Column {
                name: Some("id"),
                primary_key: true,
                ..default_column()
            })
        );
    }
}
