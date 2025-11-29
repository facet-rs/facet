//! Generated extension crate using the grammar compiler.
//!
//! This crate tests that `define_attr_grammar!` produces the same
//! functionality as the hand-written code in proto-ext.

// Invoke proc-macro directly to get correct $crate resolution
proto_attr_macros::__make_parse_attr! {
    /// ORM attributes for field and struct configuration.
    pub enum Attr {
        /// Skip this field entirely
        Skip,
        /// Rename this field/struct
        Rename(&'static str),
        /// Column configuration
        Column(Column),
    }

    /// Column configuration for ORM mapping.
    ///
    /// This attribute customizes how a field maps to a database column.
    pub struct Column {
        /// Override the column name (defaults to field name)
        pub name: Option<&'static str>,
        /// Column is nullable (default: inferred from Option<T>)
        pub nullable: Option<bool>,
        /// Custom SQL type override (e.g., "BIGINT UNSIGNED", "TEXT")
        pub sql_type: Option<&'static str>,
        /// This is a primary key
        pub primary_key: bool,
        /// Auto-increment this column
        pub auto_increment: bool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create default Column for test assertions
    fn default_column() -> Column {
        Column {
            name: None,
            nullable: None,
            sql_type: None,
            primary_key: false,
            auto_increment: false,
        }
    }

    #[test]
    fn test_skip() {
        let attr = __parse_attr!(skip);
        assert_eq!(attr, Attr::Skip);
    }

    #[test]
    fn test_rename_parens() {
        let attr = __parse_attr!(rename("new_name"));
        assert_eq!(attr, Attr::Rename("new_name"));
    }

    #[test]
    fn test_rename_equals() {
        let attr = __parse_attr!(rename = "new_name");
        assert_eq!(attr, Attr::Rename("new_name"));
    }

    #[test]
    fn test_column_empty() {
        let attr = __parse_attr!(column());
        assert_eq!(attr, Attr::Column(default_column()));
    }

    #[test]
    fn test_column_no_parens() {
        let attr = __parse_attr!(column);
        assert_eq!(attr, Attr::Column(default_column()));
    }

    #[test]
    fn test_column_name_only() {
        let attr = __parse_attr!(column(name = "user_id"));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("user_id"),
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_primary_key_flag() {
        let attr = __parse_attr!(column(primary_key));
        assert_eq!(
            attr,
            Attr::Column(Column {
                primary_key: true,
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_primary_key_explicit() {
        let attr = __parse_attr!(column(primary_key = true));
        assert_eq!(
            attr,
            Attr::Column(Column {
                primary_key: true,
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_name_and_primary_key() {
        let attr = __parse_attr!(column(name = "user_id", primary_key));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("user_id"),
                primary_key: true,
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_order_independent() {
        let attr = __parse_attr!(column(primary_key, name = "user_id"));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("user_id"),
                primary_key: true,
                ..default_column()
            })
        );
    }

    // New tests for extended Column fields

    #[test]
    fn test_column_auto_increment() {
        let attr = __parse_attr!(column(primary_key, auto_increment));
        assert_eq!(
            attr,
            Attr::Column(Column {
                primary_key: true,
                auto_increment: true,
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_nullable_flag() {
        let attr = __parse_attr!(column(nullable));
        assert_eq!(
            attr,
            Attr::Column(Column {
                nullable: Some(true),
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_sql_type() {
        let attr = __parse_attr!(column(sql_type = "BIGINT UNSIGNED"));
        assert_eq!(
            attr,
            Attr::Column(Column {
                sql_type: Some("BIGINT UNSIGNED"),
                ..default_column()
            })
        );
    }

    #[test]
    fn test_column_full_realistic() {
        // Realistic ORM column: primary key with auto-increment
        let attr = __parse_attr!(column(name = "id", primary_key, auto_increment));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("id"),
                nullable: None,
                sql_type: None,
                primary_key: true,
                auto_increment: true,
            })
        );
    }

    #[test]
    fn test_column_nullable_text() {
        // Realistic ORM column: nullable TEXT field
        let attr = __parse_attr!(column(name = "bio", nullable, sql_type = "TEXT"));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("bio"),
                nullable: Some(true),
                sql_type: Some("TEXT"),
                primary_key: false,
                auto_increment: false,
            })
        );
    }
}
