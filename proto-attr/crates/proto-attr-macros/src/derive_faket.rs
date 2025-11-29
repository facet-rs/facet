//! Implementation of `#[derive(Faket)]` proc-macro.
//!
//! This processes `#[faket(...)]` attributes and dispatches them to the
//! appropriate `__parse_attr!` macros based on namespace.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use unsynn::*;

keyword! {
    KStruct = "struct";
    KEnum = "enum";
    KPub = "pub";
    KFaket = "faket";
}

operator! {
    PathSep = "::";
    Eq = "=";
}

unsynn! {
    /// Visibility: `pub` or nothing
    enum Vis {
        PubIn(Cons<KPub, ParenthesisGroup>),
        Pub(KPub),
    }

    /// An attribute: `#[...]`
    struct Attribute {
        _pound: Pound,
        content: BracketGroup,
    }

    /// The top-level derive input (simplified for our needs)
    enum DeriveInput {
        Struct(StructDef),
        Enum(EnumDef),
    }

    /// A struct definition
    struct StructDef {
        attrs: Vec<Attribute>,
        vis: Option<Vis>,
        _kw_struct: KStruct,
        name: Ident,
        body: StructBody,
    }

    /// Struct body - braces with fields or semicolon for unit struct
    enum StructBody {
        Named(BraceGroupContaining<CommaDelimitedVec<StructField>>),
        Tuple(Cons<ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>, Semicolon>),
        Unit(Semicolon),
    }

    /// A named struct field
    struct StructField {
        attrs: Vec<Attribute>,
        vis: Option<Vis>,
        name: Ident,
        _colon: Colon,
        ty: FieldType,
    }

    /// A tuple struct field
    struct TupleField {
        attrs: Vec<Attribute>,
        vis: Option<Vis>,
        ty: FieldType,
    }

    /// Field type - collect tokens until comma
    struct FieldType {
        tokens: Any<Cons<Except<Comma>, TokenTree>>,
    }

    /// An enum definition
    struct EnumDef {
        attrs: Vec<Attribute>,
        vis: Option<Vis>,
        _kw_enum: KEnum,
        name: Ident,
        body: BraceGroup,
    }

    /// Attribute content (for parsing faket args)
    struct FaketAttrContent {
        namespace: Ident,
        _sep: PathSep,
        attr_name: Ident,
        rest: Vec<TokenTree>,
    }
}

/// Check if an attribute is a faket attribute
fn is_faket_attr(attr: &Attribute) -> bool {
    let stream = attr.content.0.stream();
    let tokens: Vec<_> = stream.into_iter().collect();
    if let Some(proc_macro2::TokenTree::Ident(ident)) = tokens.first() {
        return ident.to_string() == "faket";
    }
    false
}

/// Extract faket attributes from a list of attributes
fn extract_faket_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs.iter().filter(|attr| is_faket_attr(attr)).collect()
}

/// Generate the __parse_attr! call for a single attribute
fn generate_parse_call(attr: &Attribute) -> std::result::Result<TokenStream2, String> {
    let stream = attr.content.0.stream();

    // Parse the attribute content: faket(ns::attr_name ...)
    let tokens: Vec<proc_macro2::TokenTree> = stream.into_iter().collect();

    // Get span from the first token, or use call_site if empty
    let span = tokens
        .first()
        .map(|t| t.span())
        .unwrap_or_else(proc_macro2::Span::call_site);

    // First token should be "faket"
    if tokens.is_empty() {
        return Err("expected faket attribute content".to_string());
    }

    // Find the parenthesis group after "faket"
    let paren_group = tokens.get(1);
    let inner_stream = match paren_group {
        Some(proc_macro2::TokenTree::Group(g))
            if g.delimiter() == proc_macro2::Delimiter::Parenthesis =>
        {
            g.stream()
        }
        _ => {
            return Err("expected faket(...)".to_string());
        }
    };

    let inner_tokens: Vec<proc_macro2::TokenTree> = inner_stream.into_iter().collect();

    // Parse: ns::attr_name rest
    if inner_tokens.is_empty() {
        return Err("expected attribute content inside faket(...)".to_string());
    }

    // First should be namespace ident
    let ns = match &inner_tokens[0] {
        proc_macro2::TokenTree::Ident(i) => i.clone(),
        _ => {
            return Err("expected namespace identifier".to_string());
        }
    };

    // Check for ::
    if inner_tokens.len() < 3 {
        return Ok(quote_spanned! { span =>
            compile_error!("unprefixed attributes not yet supported in prototype; use `ns::attr` syntax")
        });
    }

    let is_path_sep = matches!(
        (&inner_tokens.get(1), &inner_tokens.get(2)),
        (Some(proc_macro2::TokenTree::Punct(p1)), Some(proc_macro2::TokenTree::Punct(p2)))
        if p1.as_char() == ':' && p2.as_char() == ':'
    );

    if !is_path_sep {
        return Ok(quote_spanned! { span =>
            compile_error!("unprefixed attributes not yet supported in prototype; use `ns::attr` syntax")
        });
    }

    // Attribute name
    let attr_name = match &inner_tokens.get(3) {
        Some(proc_macro2::TokenTree::Ident(i)) => i.clone(),
        _ => {
            return Err("expected attribute name after ::".to_string());
        }
    };

    // Rest of tokens
    let rest: TokenStream2 = inner_tokens.into_iter().skip(4).collect();

    Ok(quote_spanned! { span =>
        #ns::__parse_attr!(#attr_name #rest)
    })
}

/// Process a struct and generate the Faket impl
fn process_struct(def: &StructDef) -> std::result::Result<TokenStream2, String> {
    let name = &def.name;

    // Collect struct-level attributes
    let struct_attrs = extract_faket_attrs(&def.attrs);
    let struct_attr_calls: Vec<TokenStream2> = struct_attrs
        .iter()
        .map(|a| generate_parse_call(a))
        .collect::<std::result::Result<_, _>>()?;

    // Collect field-level attributes
    let mut field_attr_sections = Vec::new();

    match &def.body {
        StructBody::Named(fields) => {
            for field in fields.content.iter() {
                let field_name = &field.value.name;
                let field_name_str = field_name.to_string();
                let field_attrs = extract_faket_attrs(&field.value.attrs);
                let field_attr_calls: Vec<TokenStream2> = field_attrs
                    .iter()
                    .map(|a| generate_parse_call(a))
                    .collect::<std::result::Result<_, _>>()?;

                if !field_attr_calls.is_empty() {
                    field_attr_sections.push(quote! {
                        (#field_name_str, &[#(#field_attr_calls),*])
                    });
                }
            }
        }
        StructBody::Tuple(tuple) => {
            for (idx, field) in tuple.first.content.iter().enumerate() {
                let field_attrs = extract_faket_attrs(&field.value.attrs);
                let field_attr_calls: Vec<TokenStream2> = field_attrs
                    .iter()
                    .map(|a| generate_parse_call(a))
                    .collect::<std::result::Result<_, _>>()?;

                if !field_attr_calls.is_empty() {
                    field_attr_sections.push(quote! {
                        (#idx, &[#(#field_attr_calls),*])
                    });
                }
            }
        }
        StructBody::Unit(_) => {}
    }

    // Generate a simple trait impl that holds the parsed attributes
    Ok(quote! {
        impl #name {
            /// Returns the parsed struct-level attributes (prototype)
            #[allow(dead_code)]
            pub const STRUCT_ATTRS: &'static [proto_ext::Attr] = &[
                #(#struct_attr_calls),*
            ];
        }

        // Force evaluation of field attributes at compile time
        const _: () = {
            #(let _ = #field_attr_sections;)*
        };
    })
}

pub fn derive_faket(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed: DeriveInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote! { compile_error!(#msg); }.into();
        }
    };

    let expanded = match parsed {
        DeriveInput::Struct(def) => process_struct(&def),
        DeriveInput::Enum(_def) => Err(format!("Faket derive not yet implemented for enums")),
    };

    match expanded {
        Ok(tokens) => tokens.into(),
        Err(err) => quote! { compile_error!(#err); }.into(),
    }
}
