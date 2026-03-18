#[path = "cases/schema_compat.rs"]
mod schema_compat;

#[test]
fn lang_rust_to_typescript_transport_tcp_schema_compat_added_optional_field() {
    schema_compat::run_schema_compat_added_optional_field();
}

#[test]
fn lang_rust_to_typescript_transport_tcp_schema_compat_reordered_fields() {
    schema_compat::run_schema_compat_reordered_fields();
}

#[test]
fn lang_rust_to_typescript_transport_tcp_schema_compat_added_enum_variant() {
    schema_compat::run_schema_compat_added_enum_variant();
}

#[test]
fn lang_rust_to_typescript_transport_tcp_schema_compat_removed_field() {
    schema_compat::run_schema_compat_removed_field();
}

#[test]
fn lang_rust_to_typescript_transport_tcp_schema_compat_incompatible_type_change() {
    schema_compat::run_schema_compat_incompatible_type_change();
}

#[test]
fn lang_rust_to_typescript_transport_tcp_schema_compat_missing_required_field() {
    schema_compat::run_schema_compat_missing_required_field();
}
