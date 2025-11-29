//! Implementation of `__field_error!` proc-macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
#[cfg(not(feature = "nightly"))]
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

pub fn field_error(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed: FieldErrorInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); }.into();
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

    #[cfg(feature = "nightly")]
    {
        use proc_macro::{Diagnostic, Level};

        let error_msg = format!("unknown field `{}` in `{}`", got_name_str, struct_name_str);
        let mut diag = Diagnostic::spanned(got_span.unwrap(), Level::Error, error_msg);

        diag = diag.note(format!("expected {}", known_str));

        if let Some((suggestion, _)) = best_suggestion {
            diag = diag.help(format!("did you mean `{}`?", suggestion));
        }

        diag.emit();

        // Return a dummy valid value to satisfy type inference
        // The error is already shown; this just prevents cascading errors
        "proto_ext::Attr::Skip".parse().unwrap()
    }

    #[cfg(not(feature = "nightly"))]
    {
        let message = match best_suggestion {
            Some((suggestion, _)) => {
                format!(
                    "unknown field `{}` in `{}`, did you mean `{}`?\navailable fields: {}",
                    got_name_str, struct_name_str, suggestion, known_str
                )
            }
            None => {
                format!(
                    "unknown field `{}` in `{}`\navailable fields: {}",
                    got_name_str, struct_name_str, known_str
                )
            }
        };

        let expanded = quote_spanned! { got_span =>
            compile_error!(#message)
        };

        expanded.into()
    }
}
