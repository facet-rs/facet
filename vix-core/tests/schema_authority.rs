//! Production-path certificates for Taxon-owned semantic schema identity.

use vix::compiler::Compiler;
use vix::runtime::{FramedNode, Store, StoreJournalError};
use vix::vir::Type;

fn record_schema(source: &str) -> vix::schema::SchemaRef {
    let module = Compiler::new()
        .compile(source)
        .expect("schema authority fixture compiles");
    module.records[0].schema.clone()
}

#[test]
fn same_nominal_name_with_different_structure_has_distinct_value_identity() {
    let int_packet = record_schema("struct Packet { value: Int }");
    let string_packet = record_schema("struct Packet { value: String }");

    assert_ne!(
        int_packet, string_packet,
        "Taxon structure, not the surface nominal name, identifies a declaration",
    );

    let bytes = b"same resident bytes";
    let int_value = FramedNode::leaf(int_packet, bytes.to_vec()).identity();
    let string_value = FramedNode::leaf(string_packet, bytes.to_vec()).identity();
    assert_ne!(
        int_value, string_value,
        "the full semantic SchemaRef participates in Store value identity",
    );
}

#[test]
fn generic_instantiations_retain_one_base_schema_and_distinct_arguments() {
    let module = Compiler::new()
        .compile(
            r#"
enum Outcome<T> { Ok(T), Err(String) }

fn bool_outcome(value: Bool) -> Outcome<Bool> { Outcome::Ok(value) }
fn string_outcome(value: String) -> Outcome<String> { Outcome::Ok(value) }
"#,
        )
        .expect("two concrete applications compile");

    let bool_outcome = module
        .enums
        .iter()
        .find(|enumeration| enumeration.name == "Outcome<Bool>")
        .expect("Bool application is materialized");
    let string_outcome = module
        .enums
        .iter()
        .find(|enumeration| enumeration.name == "Outcome<String>")
        .expect("String application is materialized");

    assert_eq!(
        bool_outcome.schema.id, string_outcome.schema.id,
        "generic applications retain their declaration's Taxon SchemaId",
    );
    assert_eq!(bool_outcome.schema.args, [Type::Bool.schema_ref()]);
    assert_eq!(string_outcome.schema.args, [Type::String.schema_ref()]);
    assert_ne!(bool_outcome.schema, string_outcome.schema);
}

#[test]
fn previous_identity_epoch_journals_are_rejected_without_a_migration_reader() {
    let mut store = Store::default();
    store.intern_realized(Type::String.schema_ref(), b"epoch body");

    let mut old_version = store.to_journal();
    old_version.version -= 1;
    assert!(matches!(
        Store::from_journal(old_version),
        Err(StoreJournalError::UnsupportedVersion { .. })
    ));

    let mut old_authority = store.to_journal();
    old_authority.authority.value_epoch = "vix.identity.value.framed.v1".to_owned();
    old_authority.authority.store_epoch = "vix.store.persistence.v1".to_owned();
    assert!(matches!(
        Store::from_journal(old_authority),
        Err(StoreJournalError::AuthorityMismatch { .. })
    ));
}
