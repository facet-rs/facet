use dibs::schema::collect_schema;
use facet::Facet;

#[derive(Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "users")]
struct User {
    #[facet(dibs::pk)]
    id: i64,
    #[facet(dibs::unique)]
    email: String,
    name: String,
    bio: Option<String>,
    #[facet(dibs::fk = "tenants.id")]
    tenant_id: i64,
}

#[derive(Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "tenants")]
struct Tenant {
    #[facet(dibs::pk)]
    id: i64,
    name: String,
}

#[test]
fn test_schema_collect() {
    let schema = collect_schema();
    assert!(!schema.tables.is_empty(), "Schema should have tables");
}

#[test]
fn test_user_table() {
    let schema = collect_schema();
    let user_table = schema.tables.values().find(|t| t.name == "users");
    assert!(user_table.is_some(), "Should have users table");

    let user = user_table.unwrap();
    assert_eq!(user.columns.len(), 5);

    // Check pk
    let id_col = user.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.primary_key);

    // Check unique
    let email_col = user.columns.iter().find(|c| c.name == "email").unwrap();
    assert!(email_col.unique);

    // Check fk (foreign keys are on the table, not column)
    assert_eq!(user.foreign_keys.len(), 1);
    let fk = &user.foreign_keys[0];
    assert_eq!(fk.columns, vec!["tenant_id"]);
    assert_eq!(fk.references_table, "tenants");
    assert_eq!(fk.references_columns, vec!["id"]);
}
