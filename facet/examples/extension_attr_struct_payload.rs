//! End-to-end example for structured extension-attribute payloads.
//!
//! Run with:
//! `cargo run -p facet --example extension_attr_struct_payload`

use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
struct IndexedUser {
    #[facet(testattrs::column(rename = "user_name", indexed))]
    username: String,
}

fn first_field() -> &'static facet::Field {
    let Type::User(UserType::Struct(StructType { fields, .. })) = IndexedUser::SHAPE.ty else {
        panic!("expected struct");
    };
    &fields[0]
}

fn main() {
    let field = first_field();
    let attr = field
        .get_attr(Some("testattrs"), "column")
        .expect("column attribute should be present");

    let decoded = attr
        .get_as::<testattrs::Attr>()
        .expect("column payload is wrapped in testattrs::Attr");

    let testattrs::Attr::Column(column) = decoded else {
        panic!("unexpected payload variant");
    };

    println!(
        "column payload: rename={:?}, indexed={}",
        column.rename, column.indexed
    );
}
