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
    // Verify assumption: rename_all transforms are stored in field.rename
    // and effective_name() returns the renamed version
    #[derive(Facet)]
    #[facet(rename_all = "kebab-case")]
    struct TestStruct {
        my_field: u32,
    }

    let fields = get_struct_fields::<TestStruct>();
    let field = &fields[0];

    // name is the original Rust identifier
    assert_eq!(field.name, "my_field");
    // rename contains the kebab-case transformation
    assert_eq!(field.rename, Some("my-field"));
    // effective_name() returns rename if present, else name
    assert_eq!(field.effective_name(), "my-field");
}

#[test]
fn rename_all_on_enum_does_not_affect_variant_fields_in_facet_derive() {
    // Document current behavior: facet-derive does NOT propagate rename_all
    // to enum variant fields. The facet-dom deserializer handles this at runtime instead.
    #[derive(Facet)]
    #[facet(rename_all = "PascalCase")]
    #[repr(C)]
    #[allow(dead_code)] // Fields are accessed via reflection, not directly
    enum MyTag {
        TagFoo {
            name: String,
            value: u32,
        },
    }

    let Type::User(UserType::Enum(enum_type)) = MyTag::SHAPE.ty else {
        panic!("expected enum type");
    };

    let variant = &enum_type.variants[0];
    // Variant name IS transformed by rename_all
    assert_eq!(variant.name, "TagFoo");
    assert_eq!(variant.rename, Some("TagFoo"));
    assert_eq!(variant.effective_name(), "TagFoo");

    // However, fields within the variant are NOT transformed by facet-derive
    // This is the current limitation that facet-dom works around
    let fields = &variant.data.fields;
    let name_field = &fields[0];
    assert_eq!(name_field.name, "name");
    assert_eq!(name_field.rename, None); // NOT transformed!
    assert_eq!(name_field.effective_name(), "name");

    let value_field = &fields[1];
    assert_eq!(value_field.name, "value");
    assert_eq!(value_field.rename, None); // NOT transformed!
    assert_eq!(value_field.effective_name(), "value");
}
