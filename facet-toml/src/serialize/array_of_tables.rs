use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, PeekListLike, PeekStruct};
use toml_edit::{ArrayOfTables, Item, Table};

/// Check if a Peek value represents an array of structs/tables
pub fn is_array_of_tables(peek: &Peek) -> bool {
    match peek.shape().def {
        Def::List(ld) => {
            // Check if the element type is a struct (not tuple or unit)
            matches!(
                ld.t().ty,
                Type::User(UserType::Struct(sd)) if !matches!(sd.kind, StructKind::Tuple | StructKind::Unit)
            )
        }
        Def::Array(ad) => {
            // Check if the element type is a struct (not tuple or unit)
            matches!(
                ad.t().ty,
                Type::User(UserType::Struct(sd)) if !matches!(sd.kind, StructKind::Tuple | StructKind::Unit)
            )
        }
        _ => false,
    }
}

/// Serialize an array of tables to TOML array of tables format
pub fn serialize_array_of_tables<'mem, 'facet, 'shape>(
    list: PeekListLike<'mem, 'facet, 'shape>,
) -> Result<ArrayOfTables, super::TomlSerError> {
    let mut array_of_tables = ArrayOfTables::new();

    for item in list.iter() {
        // Each item should be a struct that we convert to a table
        if let Ok(struct_peek) = item.into_struct() {
            let table = serialize_struct_as_table(struct_peek)?;
            array_of_tables.push(table);
        } else {
            return Err(super::TomlSerError::InvalidArrayOfTables);
        }
    }

    Ok(array_of_tables)
}

/// Serialize a struct as a TOML table
fn serialize_struct_as_table<'mem, 'facet, 'shape>(
    struct_peek: PeekStruct<'mem, 'facet, 'shape>,
) -> Result<Table, super::TomlSerError> {
    let mut table = Table::new();

    // Serialize each field
    for (field, value) in struct_peek.fields_for_serialize() {
        // Serialize the field value to a TOML value
        let toml_value = serialize_value_to_toml(value)?;
        table.insert(field.name, toml_value);
    }

    Ok(table)
}

/// Helper to serialize a value to a TOML Item
fn serialize_value_to_toml<'mem, 'facet, 'shape>(
    value: Peek<'mem, 'facet, 'shape>,
) -> Result<Item, super::TomlSerError> {
    // Create a temporary serializer to serialize just this value
    let mut temp_serializer = super::TomlSerializer::new();
    facet_serialize::serialize_iterative(value, &mut temp_serializer)?;

    // Get the serialized document
    let doc = temp_serializer.into_raw_document();

    // The document should contain exactly one value at the root
    // Return it as an Item
    Ok(doc.as_item().clone())
}
