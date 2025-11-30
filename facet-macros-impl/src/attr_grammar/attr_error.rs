//! Implementation of `__attr_error!` proc-macro.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use unsynn::*;

keyword! {
    KKnownAttrs = "known_attrs";
    KGotName = "got_name";
    KGotRest = "got_rest";
}

operator! {
    At = "@";
}

unsynn! {
    /// Input format:
    /// ```ignore
    /// @known_attrs { skip, rename, column }
    /// @got_name { colum }
    /// @got_rest { (...) }
    /// ```
    struct AttrErrorInput {
        known_attrs_section: KnownAttrsSection,
        got_name_section: GotNameSection,
        #[allow(dead_code)]
        got_rest_section: GotRestSection,
    }

    struct KnownAttrsSection {
        _at: At,
        _kw: KKnownAttrs,
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

/// Generates compile errors for unknown attributes with helpful suggestions based on string similarity.
pub fn attr_error(input: TokenStream2) -> TokenStream2 {
    let mut iter = input.to_token_iter();

    let parsed: AttrErrorInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); };
        }
    };

    let known_attrs: Vec<_> = parsed
        .known_attrs_section
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
    for known in &known_attrs {
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

    let known_list: Vec<_> = known_attrs.iter().map(|i| i.to_string()).collect();
    let known_str = known_list.join(", ");

    let message = match best_suggestion {
        Some((suggestion, _)) => {
            format!(
                "unknown attribute `{got_name_str}`, did you mean `{suggestion}`?\navailable attributes: {known_str}"
            )
        }
        None => {
            format!("unknown attribute `{got_name_str}`\navailable attributes: {known_str}")
        }
    };

    quote_spanned! { got_span =>
        compile_error!(#message)
    }
}
