//! Tests verifying assumptions about how field.name and field.effective_name() work.
//!
//! These tests document the contract that facet-dom relies on:
//! - When rename is set, effective_name() returns the rename value
//! - When no rename, effective_name() equals name

use facet::Facet;
use facet_core::{Type, UserType};

fn get_struct_fields<T: for<'a> Facet<'a>>() -> &'static [facet_core::Field] {
    let Type::User(UserType::Struct(struct_type)) = T::SHAPE.ty else {
        panic!("expected struct type");
    };
    struct_type.fields
}

#[test]
fn field_rename_sets_effective_name() {
    // Verify assumption: when rename is set, effective_name returns the rename value
    #[derive(Facet)]
    struct TestStruct {
        #[facet(rename = "customName")]
        my_field: u32,
    }

    let fields = get_struct_fields::<TestStruct>();
    let field = &fields[0];

    eprintln!("field.name = {:?}", field.name);
    eprintln!("field.rename = {:?}", field.rename);
    eprintln!("field.effective_name() = {:?}", field.effective_name());
    eprintln!("field.attributes = {:?}", field.attributes);
    assert_eq!(field.name, "my_field");
    assert_eq!(field.rename, Some("customName"));
    assert_eq!(field.effective_name(), "customName");
    assert_ne!(field.name, field.effective_name());
}

#[test]
fn field_no_rename_effective_equals_name() {
    // Verify assumption: when no rename, effective_name equals name
    #[derive(Facet)]
    struct TestStruct {
        my_field: u32,
    }

    let fields = get_struct_fields::<TestStruct>();
    let field = &fields[0];

    eprintln!("no_rename: field.name = {:?}", field.name);
    eprintln!("no_rename: field.rename = {:?}", field.rename);
    eprintln!(
        "no_rename: field.effective_name() = {:?}",
        field.effective_name()
    );
    assert_eq!(field.name, "my_field");
    assert_eq!(field.effective_name(), "my_field");
    assert_eq!(field.name, field.effective_name());
}

#[test]
fn rename_all_sets_effective_name() {
    // Verify assumption: rename_all transforms are stored in effective_name
    #[derive(Facet)]
    #[facet(rename_all = "kebab-case")]
    struct TestStruct {
        my_field: u32,
    }

    let fields = get_struct_fields::<TestStruct>();
    let field = &fields[0];

    panic!(
        "rename_all: field.name={:?}, field.rename={:?}",
        field.name, field.rename
    );
}
