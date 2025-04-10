use super::*;
use unsynn::*;

/// Processes a tuple struct to implement Facet
///
/// Example input:
/// ```rust
/// struct Point(f32, f32);
/// ```
pub(crate) fn process_tuple_struct(parsed: TupleStruct) -> proc_macro::TokenStream {
    let struct_name = parsed.name.to_string();

    // Generate field names for tuple elements (0, 1, 2, etc.)
    let fields = parsed
        .body
        .content
        .0
        .iter()
        .enumerate()
        .map(|(idx, _)| idx.to_string())
        .collect::<Vec<String>>();

    // Create the fields string for struct_fields! macro
    let fields_str = fields.join(", ");

    // Generate the impl
    let output = format!(
        r#"
#[automatically_derived]
unsafe impl facet::Facet for {struct_name} {{
    const SHAPE: &'static facet::Shape = &const {{
        facet::Shape::builder()
            .layout(core::alloc::Layout::new::<Self>())
            .vtable(facet::value_vtable!(
                {struct_name},
                |f, _opts| core::fmt::Write::write_str(f, "{struct_name}")
            ))
            .def(facet::Def::Struct(facet::StructDef::builder()
                .kind(facet::StructKind::TupleStruct)
                .fields(facet::struct_fields!({struct_name}, ({fields_str})))
                .build()
            ))
            .build()
    }};
}}
    "#
    );
    output.into_token_stream().into()
}
