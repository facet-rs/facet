use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, PeekListLike, PeekStruct};
use toml_edit::{ArrayOfTables, Table};

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
pub fn serialize_array_of_tables<'mem, 'facet>(
    list: PeekListLike<'mem, 'facet>,
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
fn serialize_struct_as_table<'mem, 'facet>(
    struct_peek: PeekStruct<'mem, 'facet>,
) -> Result<Table, super::TomlSerError> {
    let mut table = Table::new();

    // Serialize each field
    for (field, value) in struct_peek.fields_for_serialize() {
        // Skip None values
        if let Def::Option(_) = value.shape().def {
            let opt = value.into_option().unwrap();
            if let Some(inner) = opt.value() {
                let toml_item = super::serialize_to_item(inner)?;
                table.insert(field.name, toml_item);
            }
            // Skip None
        } else {
            let toml_item = super::serialize_to_item(value)?;
            table.insert(field.name, toml_item);
        }
    }

    Ok(table)
}
