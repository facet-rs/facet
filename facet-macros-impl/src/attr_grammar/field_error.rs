//! Implementation of `__field_error!` proc-macro.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use unsynn::*;

keyword! {
    KStructName = "struct_name";
    KKnownFields = "known_fields";
    KGotName = "got_name";
    KGotRest = "got_rest";
}

operator! {
    At = "@";
}

unsynn! {
    /// Input format:
    /// ```ignore
    /// @struct_name { Column }
    /// @known_fields { name, primary_key }
    /// @got_name { nam }
    /// @got_rest { = "..." }
    /// ```
    struct FieldErrorInput {
        struct_name_section: StructNameSection,
        known_fields_section: KnownFieldsSection,
        got_name_section: GotNameSection,
        #[allow(dead_code)]
        got_rest_section: GotRestSection,
    }

    struct StructNameSection {
        _at: At,
        _kw: KStructName,
        content: BraceGroupContaining<Ident>,
    }

    struct KnownFieldsSection {
        _at: At,
        _kw: KKnownFields,
        content: BraceGroupContaining<CommaDelimitedVec<Ident>>,
    }

    struct GotNameSection {
        _at: At,
        _kw: KGotName,
        content: BraceGroupContaining<Ident>,
    }

    struct GotRestSection {
        _at: At,
        _kw: KGotRest,
        content: BraceGroup,
    }
}

/// Generates compile errors for unknown struct fields with helpful suggestions and lists available fields.
pub fn field_error(input: TokenStream2) -> TokenStream2 {
    let mut iter = input.to_token_iter();

    let parsed: FieldErrorInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); };
        }
    };

    let struct_name = &parsed.struct_name_section.content.content;
    let struct_name_str = struct_name.to_string();

    let known_fields: Vec<_> = parsed
        .known_fields_section
        .content
        .content
        .iter()
        .map(|d| d.value.clone())
        .collect();
    let got_name = &parsed.got_name_section.content.content;
    let got_name_str = got_name.to_string();
    let got_span = got_name.span();

    // Find best suggestion using strsim
    let mut best_suggestion: Option<(&Ident, f64)> = None;
    for known in &known_fields {
        let score = strsim::jaro_winkler(&got_name_str, &known.to_string());
        if score > 0.7 {
            match &best_suggestion {
                None => best_suggestion = Some((known, score)),
                Some((_, best_score)) if score > *best_score => {
                    best_suggestion = Some((known, score))
                }
                _ => {}
            }
        }
    }

    let known_list: Vec<_> = known_fields.iter().map(|i| i.to_string()).collect();
    let known_str = known_list.join(", ");

    let message = match best_suggestion {
        Some((suggestion, _)) => {
            format!(
                "unknown field `{got_name_str}` in `{struct_name_str}`, did you mean `{suggestion}`?\navailable fields: {known_str}"
            )
        }
        None => {
            format!(
                "unknown field `{got_name_str}` in `{struct_name_str}`\navailable fields: {known_str}"
            )
        }
    };

    quote_spanned! { got_span =>
        compile_error!(#message)
    }
}
