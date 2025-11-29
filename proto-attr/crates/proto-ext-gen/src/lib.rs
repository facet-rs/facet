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
    pub struct Column {
        /// Override the column name
        pub name: Option<&'static str>,
        /// Is this a primary key?
        pub primary_key: bool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: None,
                primary_key: false,
            })
        );
    }

    #[test]
    fn test_column_no_parens() {
        let attr = __parse_attr!(column);
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: None,
                primary_key: false,
            })
        );
    }

    #[test]
    fn test_column_name_only() {
        let attr = __parse_attr!(column(name = "user_id"));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("user_id"),
                primary_key: false,
            })
        );
    }

    #[test]
    fn test_column_primary_key_flag() {
        let attr = __parse_attr!(column(primary_key));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: None,
                primary_key: true,
            })
        );
    }

    #[test]
    fn test_column_primary_key_explicit() {
        let attr = __parse_attr!(column(primary_key = true));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: None,
                primary_key: true,
            })
        );
    }

    #[test]
    fn test_column_full() {
        let attr = __parse_attr!(column(name = "user_id", primary_key));
        assert_eq!(
            attr,
            Attr::Column(Column {
                name: Some("user_id"),
                primary_key: true,
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
            })
        );
    }
}
