use super::*;
use dibs_jsonb::Jsonb;

#[test]
fn test_jsonb_shape_to_pg_type() {
    // Test that Jsonb<T> maps to PgType::Jsonb regardless of T
    let shape_unit = Jsonb::<()>::SHAPE;
    let shape_string = Jsonb::<String>::SHAPE;

    assert_eq!(
        shape_to_pg_type(shape_unit),
        Some(PgType::Jsonb),
        "Jsonb<()> should map to Jsonb"
    );
    assert_eq!(
        shape_to_pg_type(shape_string),
        Some(PgType::Jsonb),
        "Jsonb<String> should map to Jsonb"
    );

    // Verify decl_ids match
    assert_eq!(
        shape_unit.decl_id, shape_string.decl_id,
        "Jsonb<()> and Jsonb<String> should have the same decl_id"
    );
}

#[test]
fn test_unwrap_option() {
    // Test unwrap_option with various types
    let (inner, nullable) = unwrap_option(Option::<String>::SHAPE);
    assert!(nullable, "Option<String> should be nullable");
    assert_eq!(inner, String::SHAPE, "inner should be String");

    let (inner, nullable) = unwrap_option(String::SHAPE);
    assert!(!nullable, "String should not be nullable");
    assert_eq!(inner, String::SHAPE, "inner should be String");

    // Test with Jsonb
    let (inner, nullable) = unwrap_option(Option::<Jsonb<String>>::SHAPE);
    assert!(nullable, "Option<Jsonb<String>> should be nullable");
    assert_eq!(
        inner.decl_id,
        Jsonb::<()>::SHAPE.decl_id,
        "inner should be Jsonb"
    );
}

#[test]
fn test_option_shape_has_inner() {
    // Verify that Option shapes have the inner field set
    let shape = Option::<String>::SHAPE;
    assert!(
        shape.inner.is_some(),
        "Option<String>::SHAPE.inner should be Some"
    );
    assert_eq!(shape.inner, Some(String::SHAPE));

    let shape = Option::<Jsonb<String>>::SHAPE;
    assert!(
        shape.inner.is_some(),
        "Option<Jsonb<String>>::SHAPE.inner should be Some"
    );

    // Test with facet_value::Value - this is what the example uses
    let shape = Option::<Jsonb<facet_value::Value>>::SHAPE;
    assert!(
        shape.inner.is_some(),
        "Option<Jsonb<Value>>::SHAPE.inner should be Some"
    );

    // Verify the inner is Jsonb
    let inner = shape.inner.unwrap();
    assert_eq!(
        inner.decl_id,
        Jsonb::<()>::SHAPE.decl_id,
        "inner should be Jsonb"
    );

    // Verify shape_to_pg_type works
    assert_eq!(
        shape_to_pg_type(inner),
        Some(PgType::Jsonb),
        "Jsonb<Value> should map to Jsonb"
    );
}

#[test]
fn test_parse_fk_reference_dot_format() {
    assert_eq!(parse_fk_reference("users.id"), Some(("users", "id")));
    assert_eq!(parse_fk_reference("shop.id"), Some(("shop", "id")));
    assert_eq!(
        parse_fk_reference("category.parent_id"),
        Some(("category", "parent_id"))
    );
}

#[test]
fn test_parse_fk_reference_paren_format() {
    assert_eq!(parse_fk_reference("users(id)"), Some(("users", "id")));
    assert_eq!(parse_fk_reference("shop(id)"), Some(("shop", "id")));
    assert_eq!(
        parse_fk_reference("category(parent_id)"),
        Some(("category", "parent_id"))
    );
}

#[test]
fn test_parse_fk_reference_invalid() {
    assert_eq!(parse_fk_reference(""), None);
    assert_eq!(parse_fk_reference("users"), None);
    assert_eq!(parse_fk_reference(".id"), None);
    assert_eq!(parse_fk_reference("users."), None);
    assert_eq!(parse_fk_reference("(id)"), None);
    assert_eq!(parse_fk_reference("users("), None);
    assert_eq!(parse_fk_reference("users()"), None);
    assert_eq!(parse_fk_reference("()"), None);
}

#[test]
fn test_index_column_parse_simple() {
    let col = IndexColumn::parse("name");
    assert_eq!(col.name, "name");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::Default);
}

#[test]
fn test_index_column_parse_desc() {
    let col = IndexColumn::parse("created_at DESC");
    assert_eq!(col.name, "created_at");
    assert_eq!(col.order, SortOrder::Desc);
    assert_eq!(col.nulls, NullsOrder::Default);
}

#[test]
fn test_index_column_parse_asc() {
    let col = IndexColumn::parse("id ASC");
    assert_eq!(col.name, "id");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::Default);
}

#[test]
fn test_index_column_parse_nulls_first() {
    let col = IndexColumn::parse("reminder_sent_at NULLS FIRST");
    assert_eq!(col.name, "reminder_sent_at");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::First);
}

#[test]
fn test_index_column_parse_nulls_last() {
    let col = IndexColumn::parse("score NULLS LAST");
    assert_eq!(col.name, "score");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::Last);
}

#[test]
fn test_index_column_parse_desc_nulls_first() {
    let col = IndexColumn::parse("priority DESC NULLS FIRST");
    assert_eq!(col.name, "priority");
    assert_eq!(col.order, SortOrder::Desc);
    assert_eq!(col.nulls, NullsOrder::First);
}

#[test]
fn test_index_column_parse_desc_nulls_last() {
    let col = IndexColumn::parse("updated_at DESC NULLS LAST");
    assert_eq!(col.name, "updated_at");
    assert_eq!(col.order, SortOrder::Desc);
    assert_eq!(col.nulls, NullsOrder::Last);
}

#[test]
fn test_index_column_parse_asc_nulls_first() {
    let col = IndexColumn::parse("nullable_col ASC NULLS FIRST");
    assert_eq!(col.name, "nullable_col");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::First);
}
