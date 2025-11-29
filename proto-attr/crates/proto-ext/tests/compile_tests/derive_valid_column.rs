// Valid: derive with column attribute on field
// Demonstrates realistic ORM column configurations
use proto_attr::Faket;

#[derive(Faket)]
struct User {
    /// Primary key with auto-increment
    #[faket(proto_ext::column(name = "id", primary_key, auto_increment))]
    id: i64,

    /// Custom column name
    #[faket(proto_ext::column(name = "user_name"))]
    name: String,

    /// Nullable field with custom SQL type
    #[faket(proto_ext::column(nullable, sql_type = "TEXT"))]
    bio: Option<String>,

    /// Explicit non-nullable with specific SQL type
    #[faket(proto_ext::column(nullable = false, sql_type = "TIMESTAMP"))]
    created_at: i64,
}

fn main() {}
