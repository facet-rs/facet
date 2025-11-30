//! Implementation of `__make_parse_attr!` proc-macro.
//!
//! This is the grammar compiler - it takes a grammar DSL and generates:
//! 1. Type definitions (enum + structs)
//! 2. Proc-macro re-exports
//! 3. `__attr!` dispatcher macro that returns `ExtensionAttr`

use std::collections::HashMap;

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use unsynn::*;

// ============================================================================
// UNSYNN TYPE DEFINITIONS
// ============================================================================

keyword! {
    KEnum = "enum";
    KStruct = "struct";
    KPub = "pub";
    KBool = "bool";
    KOption = "Option";
    KStatic = "static";
    KNs = "ns";
    KCratePath = "crate_path";
    KChar = "char";
}

operator! {
    /// Represents the '=' operator.
    Eq = "=";
    /// Represents the ':' operator.
    Col = ":";
    /// Represents the '&' operator.
    Amp = "&";
    /// Represents the ''' operator.
    Apos = "'";
    /// Path separator ::
    PathSep = "::";
    /// Less than <
    Lt = "<";
    /// Greater than >
    Gt = ">";
}

unsynn! {
    /// The complete grammar definition
    ///
    /// Format:
    /// ```ignore
    /// ns "kdl";
    /// crate_path ::facet_kdl;
    /// pub enum Attr { ... }
    /// pub struct Column { ... }
    /// ```
    struct Grammar {
        ns_decl: Option<NsDecl>,
        crate_path_decl: Option<CratePathDecl>,
        items: Vec<GrammarItem>,
    }

    /// Namespace declaration: `ns "kdl";`
    struct NsDecl {
        _kw: KNs,
        ns_literal: Literal,
        _semi: Semicolon,
    }

    /// Crate path declaration: `crate_path ::facet_kdl;`
    /// The path is captured as raw tokens until the semicolon.
    struct CratePathDecl {
        _kw: KCratePath,
        path_tokens: Any<Cons<Except<Semicolon>, TokenTree>>,
        _semi: Semicolon,
    }

    /// Either an enum or a struct definition
    enum GrammarItem {
        Enum(AttrEnum),
        Struct(StructDef),
    }

    /// Visibility: `pub` or nothing
    enum Vis {
        Pub(KPub),
    }

    /// An enum definition: `pub enum Attr { ... }`
    struct AttrEnum {
        attrs: Vec<OuterAttr>,
        vis: Option<Vis>,
        _kw_enum: KEnum,
        name: Ident,
        body: BraceGroupContaining<CommaDelimitedVec<EnumVariant>>,
    }

    /// A variant in the enum: `Skip` or `Rename(&'static str)` or `Column(Column)`
    struct EnumVariant {
        attrs: Vec<OuterAttr>,
        name: Ident,
        payload: Option<ParenthesisGroupContaining<VariantPayload>>,
    }

    /// What's inside the variant parens - either a type or just an ident (struct ref)
    enum VariantPayload {
        /// Reference type like `&'static str`
        RefType(RefType),
        /// Option<char> type
        OptionChar(OptionCharType),
        /// Just an identifier (struct reference)
        Ident(Ident),
    }

    /// A reference type: `&'static str`
    struct RefType {
        _amp: Amp,
        _apos: Apos,
        _static: KStatic,
        typ: Ident,
    }

    /// Option<char> type
    struct OptionCharType {
        _option: KOption,
        _lt: Lt,
        _char: KChar,
        _gt: Gt,
    }

    /// A struct definition: `pub struct Column { ... }`
    struct StructDef {
        attrs: Vec<OuterAttr>,
        vis: Option<Vis>,
        _kw_struct: KStruct,
        name: Ident,
        body: BraceGroupContaining<CommaDelimitedVec<StructField>>,
    }

    /// A field in a struct: `pub name: Option<&'static str>`
    struct StructField {
        attrs: Vec<OuterAttr>,
        vis: Option<Vis>,
        name: Ident,
        _colon: Col,
        ty: FieldTypeTokens,
    }

    /// Outer attribute: `#[...]`
    struct OuterAttr {
        _pound: Pound,
        content: BracketGroup,
    }

    /// Field type tokens - we collect them and parse separately
    struct FieldTypeTokens {
        tokens: Any<Cons<Except<Comma>, TokenTree>>,
    }
}

// ============================================================================
// PARSED STRUCTURES FOR CODE GENERATION
// ============================================================================

/// The parsed grammar for code generation
struct ParsedGrammar {
    /// Namespace string (e.g., "kdl"), or None for built-in attrs
    ns: Option<String>,
    /// Crate path tokens (e.g., `::facet_kdl`), required for non-unit variants
    crate_path: Option<TokenStream2>,
    attr_enum: ParsedEnum,
    structs: Vec<ParsedStruct>,
}

struct ParsedEnum {
    attrs: Vec<TokenStream2>,
    is_pub: bool,
    name: proc_macro2::Ident,
    variants: Vec<ParsedVariant>,
}

struct ParsedVariant {
    attrs: Vec<TokenStream2>,
    name: proc_macro2::Ident,
    kind: VariantKind,
}

enum VariantKind {
    Unit,
    Newtype(TokenStream2),
    NewtypeOptionChar,
    Struct(proc_macro2::Ident),
}

struct ParsedStruct {
    attrs: Vec<TokenStream2>,
    is_pub: bool,
    name: proc_macro2::Ident,
    fields: Vec<ParsedField>,
}

struct ParsedField {
    attrs: Vec<TokenStream2>,
    is_pub: bool,
    name: proc_macro2::Ident,
    ty: FieldType,
}

/// Supported field types
enum FieldType {
    Bool,
    StaticStr,
    OptionStaticStr,
    OptionBool,
    OptionChar,
}

// ============================================================================
// CONVERSION FROM UNSYNN TYPES TO PARSED TYPES
// ============================================================================

impl Grammar {
    fn to_parsed(&self) -> std::result::Result<ParsedGrammar, String> {
        // Extract namespace from `ns "kdl";` declaration
        let ns = self.ns_decl.as_ref().map(|decl| {
            // Strip quotes from the literal
            let s = decl.ns_literal.to_string();
            s.trim_matches('"').to_string()
        });

        // Extract crate path from `crate_path ::facet_kdl;` declaration
        let crate_path = self.crate_path_decl.as_ref().map(|decl| {
            let mut tokens = TokenStream2::new();
            for item in decl.path_tokens.iter() {
                tokens.extend(std::iter::once(item.value.second.clone()));
            }
            tokens
        });

        let mut attr_enum: Option<ParsedEnum> = None;
        let mut structs = Vec::new();

        for item in &self.items {
            match item {
                GrammarItem::Enum(e) => {
                    if attr_enum.is_some() {
                        return Err("only one enum is allowed in the grammar".to_string());
                    }
                    attr_enum = Some(e.to_parsed()?);
                }
                GrammarItem::Struct(s) => {
                    structs.push(s.to_parsed()?);
                }
            }
        }

        let attr_enum = attr_enum.ok_or_else(|| "expected an enum definition".to_string())?;

        Ok(ParsedGrammar {
            ns,
            crate_path,
            attr_enum,
            structs,
        })
    }
}

fn convert_attrs(attrs: &[OuterAttr]) -> Vec<TokenStream2> {
    attrs
        .iter()
        .map(|a| {
            let content = a.content.0.stream();
            quote! { #[#content] }
        })
        .collect()
}

fn convert_ident(ident: &Ident) -> proc_macro2::Ident {
    proc_macro2::Ident::new(&ident.to_string(), Span::call_site())
}

impl AttrEnum {
    fn to_parsed(&self) -> std::result::Result<ParsedEnum, String> {
        let variants: std::result::Result<Vec<_>, _> = self
            .body
            .content
            .iter()
            .map(|d| d.value.to_parsed())
            .collect();

        Ok(ParsedEnum {
            attrs: convert_attrs(&self.attrs),
            is_pub: self.vis.is_some(),
            name: convert_ident(&self.name),
            variants: variants?,
        })
    }
}

impl EnumVariant {
    fn to_parsed(&self) -> std::result::Result<ParsedVariant, String> {
        let kind = match &self.payload {
            None => VariantKind::Unit,
            Some(paren_group) => {
                match &paren_group.content {
                    VariantPayload::Ident(ident) => VariantKind::Struct(convert_ident(ident)),
                    VariantPayload::RefType(ref_type) => {
                        // Reconstruct the type tokens
                        let typ = convert_ident(&ref_type.typ);
                        VariantKind::Newtype(quote! { &'static #typ })
                    }
                    VariantPayload::OptionChar(_) => VariantKind::NewtypeOptionChar,
                }
            }
        };

        Ok(ParsedVariant {
            attrs: convert_attrs(&self.attrs),
            name: convert_ident(&self.name),
            kind,
        })
    }
}

impl StructDef {
    fn to_parsed(&self) -> std::result::Result<ParsedStruct, String> {
        let fields: std::result::Result<Vec<_>, _> = self
            .body
            .content
            .iter()
            .map(|d| d.value.to_parsed())
            .collect();

        Ok(ParsedStruct {
            attrs: convert_attrs(&self.attrs),
            is_pub: self.vis.is_some(),
            name: convert_ident(&self.name),
            fields: fields?,
        })
    }
}

impl StructField {
    fn to_parsed(&self) -> std::result::Result<ParsedField, String> {
        let ty = parse_field_type(&self.ty)?;

        Ok(ParsedField {
            attrs: convert_attrs(&self.attrs),
            is_pub: self.vis.is_some(),
            name: convert_ident(&self.name),
            ty,
        })
    }
}

fn parse_field_type(ty: &FieldTypeTokens) -> std::result::Result<FieldType, String> {
    // Collect tokens into a string for comparison
    let mut ty_str = String::new();
    for item in ty.tokens.iter() {
        ty_str.push_str(&item.value.second.to_string());
    }
    ty_str = ty_str.replace(' ', "");

    match ty_str.as_str() {
        "bool" => Ok(FieldType::Bool),
        "&'staticstr" => Ok(FieldType::StaticStr),
        "Option<&'staticstr>" => Ok(FieldType::OptionStaticStr),
        "Option<bool>" => Ok(FieldType::OptionBool),
        "Option<char>" => Ok(FieldType::OptionChar),
        _ => Err(format!(
            "unsupported field type: {ty_str}. Supported types: bool, &'static str, Option<&'static str>, Option<bool>, Option<char>"
        )),
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Convert CamelCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

// ============================================================================
// CODE GENERATION
// ============================================================================

impl ParsedGrammar {
    fn generate(&self) -> TokenStream2 {
        let type_defs = self.generate_types();
        let reexports = self.generate_reexports();
        let attr_macro = self.generate_attr_macro();

        quote! {
            #type_defs
            #reexports
            #attr_macro
        }
    }

    fn generate_types(&self) -> TokenStream2 {
        let enum_def = self.generate_enum();
        let struct_defs: Vec<_> = self.structs.iter().map(|s| s.generate()).collect();

        quote! {
            #enum_def
            #(#struct_defs)*
        }
    }

    fn generate_enum(&self) -> TokenStream2 {
        let ParsedEnum {
            attrs,
            is_pub,
            name,
            variants,
        } = &self.attr_enum;

        let vis_tokens = if *is_pub { Some(quote! { pub }) } else { None };

        let variant_defs: Vec<_> = variants
            .iter()
            .map(|v| {
                let ParsedVariant { attrs, name, kind } = v;
                match kind {
                    VariantKind::Unit => quote! { #(#attrs)* #name },
                    VariantKind::Newtype(ty) => quote! { #(#attrs)* #name(#ty) },
                    VariantKind::NewtypeOptionChar => quote! { #(#attrs)* #name(Option<char>) },
                    VariantKind::Struct(struct_name) => {
                        quote! { #(#attrs)* #name(#struct_name) }
                    }
                }
            })
            .collect();

        quote! {
            #(#attrs)*
            #[derive(Debug, Clone, PartialEq, ::facet::Facet)]
            #[repr(u8)]
            #vis_tokens enum #name {
                #(#variant_defs),*
            }
        }
    }

    fn generate_reexports(&self) -> TokenStream2 {
        quote! {
            #[doc(hidden)]
            pub use ::facet::__attr_error as __attr_error_proc_macro;
            #[doc(hidden)]
            pub use ::facet::__build_struct_fields;
            #[doc(hidden)]
            pub use ::facet::__dispatch_attr;
            #[doc(hidden)]
            pub use ::facet::__field_error as __field_error_proc_macro;
            #[doc(hidden)]
            pub use ::facet::__spanned_error;
        }
    }

    fn generate_attr_macro(&self) -> TokenStream2 {
        let enum_name = &self.attr_enum.name;
        let ns_str = self.ns.as_deref().unwrap_or("");

        // Build a map from struct name to struct definition for O(1) lookup
        let struct_map: HashMap<String, &ParsedStruct> = self
            .structs
            .iter()
            .map(|s| (s.name.to_string(), s))
            .collect();

        // Generate variant metadata for the unified dispatcher
        let variants_meta: Vec<_> = self
            .attr_enum
            .variants
            .iter()
            .map(|v| {
                let name =
                    proc_macro2::Ident::new(&to_snake_case(&v.name.to_string()), Span::call_site());
                match &v.kind {
                    VariantKind::Unit => quote! { #name: unit },
                    VariantKind::Newtype(_) => quote! { #name: newtype },
                    VariantKind::NewtypeOptionChar => quote! { #name: newtype_opt_char },
                    VariantKind::Struct(struct_name) => {
                        // Find the struct definition
                        let struct_def = struct_map
                            .get(&struct_name.to_string())
                            .expect("struct variant must reference a defined struct");

                        // Generate field metadata
                        let fields_meta: Vec<_> = struct_def
                            .fields
                            .iter()
                            .map(|f| {
                                let field_name = &f.name;
                                let kind = match f.ty {
                                    FieldType::Bool => quote! { bool },
                                    FieldType::StaticStr => quote! { string },
                                    FieldType::OptionStaticStr => quote! { opt_string },
                                    FieldType::OptionBool => quote! { opt_bool },
                                    FieldType::OptionChar => quote! { opt_char },
                                };
                                quote! { #field_name: #kind }
                            })
                            .collect();

                        // Use "rec" (record) instead of "struct" since "struct" is a keyword
                        quote! { #name: rec #struct_name { #(#fields_meta),* } }
                    }
                }
            })
            .collect();

        // Generate match arms for each variant that return ExtensionAttr
        let variant_arms: Vec<_> = self
            .attr_enum
            .variants
            .iter()
            .map(|v| {
                let key_str = to_snake_case(&v.name.to_string());
                let key_ident = proc_macro2::Ident::new(&key_str, Span::call_site());

                // For unit variants, use () as data (simple and avoids $crate issues)
                // For complex variants, use the dispatch system
                // All arms now accept @ns { $ns:path } prefix from __ext!
                // The $ns path is used to import Attr from the correct crate at the call site
                match &v.kind {
                    VariantKind::Unit => {
                        quote! {
                            // Field-level: @ns { path } attr { field : Type }
                            (@ns { $ns:path } #key_ident { $field:ident : $ty:ty }) => {{
                                static __UNIT: () = ();
                                ::facet::ExtensionAttr::new(#ns_str, #key_str, &__UNIT)
                            }};
                            // Field-level with args: not expected for unit variants
                            (@ns { $ns:path } #key_ident { $field:ident : $ty:ty | $first:tt $($rest:tt)* }) => {{
                                ::facet::__no_args!(concat!(#ns_str, "::", #key_str), $first)
                            }};
                            // Container-level: @ns { path } attr { }
                            (@ns { $ns:path } #key_ident { }) => {{
                                static __UNIT: () = ();
                                ::facet::ExtensionAttr::new(#ns_str, #key_str, &__UNIT)
                            }};
                            // Container-level with args: not expected for unit variants
                            (@ns { $ns:path } #key_ident { | $first:tt $($rest:tt)* }) => {{
                                ::facet::__no_args!(concat!(#ns_str, "::", #key_str), $first)
                            }};
                        }
                    }
                    VariantKind::Newtype(_) | VariantKind::NewtypeOptionChar | VariantKind::Struct(_) => {
                        // For non-unit variants, we need the crate_path to generate proper type references.
                        // The crate_path is passed to the proc macro so it can output e.g. `::facet_args::Attr::Short(...)`
                        let crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for non-unit variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        quote! {
                            // Field-level: @ns { path } attr { field : Type } or with args
                            (@ns { $ns:path } #key_ident { $field:ident : $ty:ty }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { }
                                };
                                ::facet::ExtensionAttr::new(#ns_str, #key_str, &__ATTR_DATA)
                            }};
                            (@ns { $ns:path } #key_ident { $field:ident : $ty:ty | $($args:tt)* }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { $($args)* }
                                };
                                ::facet::ExtensionAttr::new(#ns_str, #key_str, &__ATTR_DATA)
                            }};
                            // Container-level: @ns { path } attr { } or with args
                            (@ns { $ns:path } #key_ident { }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { }
                                };
                                ::facet::ExtensionAttr::new(#ns_str, #key_str, &__ATTR_DATA)
                            }};
                            (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { $($args)* }
                                };
                                ::facet::ExtensionAttr::new(#ns_str, #key_str, &__ATTR_DATA)
                            }};
                        }
                    }
                }
            })
            .collect();

        // Generate list of known attribute names for error messages
        let known_attrs: Vec<_> = self
            .attr_enum
            .variants
            .iter()
            .map(|v| {
                proc_macro2::Ident::new(&to_snake_case(&v.name.to_string()), Span::call_site())
            })
            .collect();

        quote! {
            /// Dispatcher macro for extension attributes.
            ///
            /// Called by the derive macro via `__ext!`. Returns `ExtensionAttr` values.
            /// Input format: `@ns { namespace_path } attr_name { ... }`
            #[macro_export]
            #[doc(hidden)]
            macro_rules! __attr {
                #(#variant_arms)*

                // Unknown attribute: use __attr_error! for typo suggestions
                (@ns { $ns:path } $unknown:ident $($tt:tt)*) => {
                    ::facet::__attr_error!(
                        @known_attrs { #(#known_attrs),* }
                        @got_name { $unknown }
                        @got_rest { $($tt)* }
                    )
                };
            }

            /// Parse an attribute into an `Attr` value (for internal use/testing).
            ///
            /// Uses proc-macro dispatcher to preserve spans for error messages.
            #[macro_export]
            #[doc(hidden)]
            macro_rules! __parse_attr {
                // Dispatch via proc-macro to handle all variant types
                ($name:ident $($rest:tt)*) => {
                    $crate::__dispatch_attr!{
                        @namespace { $crate }
                        @enum_name { #enum_name }
                        @variants { #(#variants_meta),* }
                        @name { $name }
                        @rest { $($rest)* }
                    }
                };

                // Error: completely empty
                () => {
                    compile_error!("expected an attribute name")
                };
            }
        }
    }
}

impl ParsedStruct {
    fn generate(&self) -> TokenStream2 {
        let ParsedStruct {
            attrs,
            is_pub,
            name,
            fields,
        } = self;

        let vis_tokens = if *is_pub { Some(quote! { pub }) } else { None };

        let field_defs: Vec<_> = fields
            .iter()
            .map(|f| {
                let ParsedField {
                    attrs,
                    is_pub,
                    name,
                    ty,
                } = f;
                let vis_tokens = if *is_pub { Some(quote! { pub }) } else { None };
                let ty_tokens = ty.to_tokens();
                quote! { #(#attrs)* #vis_tokens #name: #ty_tokens }
            })
            .collect();

        quote! {
            #(#attrs)*
            #[derive(Debug, Clone, PartialEq, Default, ::facet::Facet)]
            #vis_tokens struct #name {
                #(#field_defs),*
            }
        }
    }
}

impl FieldType {
    fn to_tokens(&self) -> TokenStream2 {
        match self {
            FieldType::Bool => quote! { bool },
            FieldType::StaticStr => quote! { &'static str },
            FieldType::OptionStaticStr => quote! { Option<&'static str> },
            FieldType::OptionBool => quote! { Option<bool> },
            FieldType::OptionChar => quote! { Option<char> },
        }
    }
}

// ============================================================================
// ENTRY POINT
// ============================================================================

/// Grammar compiler that transforms attribute grammar DSL into type definitions, proc-macro re-exports, and dispatcher macros.
pub fn make_parse_attr(input: TokenStream2) -> TokenStream2 {
    let mut iter = input.to_token_iter();

    let grammar: Grammar = match iter.parse() {
        Ok(g) => g,
        Err(e) => {
            let msg = e.to_string();
            return quote! { compile_error!(#msg); };
        }
    };

    let parsed = match grammar.to_parsed() {
        Ok(p) => p,
        Err(e) => {
            return quote! { compile_error!(#e); };
        }
    };

    parsed.generate()
}
