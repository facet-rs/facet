//! # Facet Macro Types
//!
//! Shared parsed type representations for the facet macro ecosystem.
//!
//! This crate provides the core types that represent parsed Rust types
//! (structs, enums, fields, variants) in a form suitable for code generation.
//!
//! These types are used by:
//! - `facet-macro-parse`: Parses Rust source into these types
//! - `facet-macro-template`: Evaluates templates against these types
//! - `facet-macros-impl`: Uses these types for code generation

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};

// =============================================================================
// Field/Variant Name
// =============================================================================

/// For struct fields, they can either be identifiers (`my_struct.foo`)
/// or literals (`my_struct.2`) — for tuple structs.
#[derive(Debug, Clone)]
pub enum IdentOrLiteral {
    /// Named field identifier
    Ident(Ident),
    /// Tuple field index
    Literal(usize),
}

impl ToTokens for IdentOrLiteral {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            IdentOrLiteral::Ident(ident) => tokens.extend(quote! { #ident }),
            IdentOrLiteral::Literal(lit) => {
                let unsuffixed = proc_macro2::Literal::usize_unsuffixed(*lit);
                tokens.extend(quote! { #unsuffixed })
            }
        }
    }
}

impl std::fmt::Display for IdentOrLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentOrLiteral::Ident(ident) => write!(f, "{ident}"),
            IdentOrLiteral::Literal(lit) => write!(f, "{lit}"),
        }
    }
}

/// A parsed name, which includes the raw name and the effective name.
///
/// Examples:
///
///   raw = "foo_bar", no rename rule, effective = "foo_bar"
///   raw = "foo_bar", #[facet(rename = "kiki")], effective = "kiki"
///   raw = "foo_bar", #[facet(rename_all = camelCase)], effective = "fooBar"
///   raw = "r#type", no rename rule, effective = "type"
#[derive(Debug, Clone)]
pub struct PName {
    /// The raw identifier, as found in the source code.
    /// It might be raw, as in "r#keyword".
    pub raw: IdentOrLiteral,

    /// The name after applying rename rules.
    /// This might not be a valid Rust identifier (e.g., kebab-case).
    pub effective: String,
}

impl PName {
    /// Construct a PName with an optional rename rule applied
    pub fn new(rename_rule: Option<RenameRule>, raw: IdentOrLiteral) -> Self {
        // Get the normalized raw string (strip r# prefix for raw identifiers)
        let norm_raw = match &raw {
            IdentOrLiteral::Ident(ident) => ident.to_string().trim_start_matches("r#").to_string(),
            IdentOrLiteral::Literal(lit) => lit.to_string(),
        };

        let effective = if let Some(rule) = rename_rule {
            rule.apply(&norm_raw)
        } else {
            norm_raw
        };

        Self { raw, effective }
    }
}

// =============================================================================
// Representation
// =============================================================================

/// Parsed representation attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PRepr {
    /// `#[repr(transparent)]`
    Transparent,
    /// `#[repr(Rust)]` with optional primitive type
    Rust(Option<PrimitiveRepr>),
    /// `#[repr(C)]` with optional primitive type
    C(Option<PrimitiveRepr>),
    /// A repr error that rustc will catch (e.g., conflicting hints).
    RustcWillCatch,
}

/// Primitive repr types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveRepr {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    Usize,
}

impl PrimitiveRepr {
    /// Returns the type name as a token stream
    pub fn type_name(&self) -> TokenStream {
        match self {
            PrimitiveRepr::U8 => quote! { u8 },
            PrimitiveRepr::U16 => quote! { u16 },
            PrimitiveRepr::U32 => quote! { u32 },
            PrimitiveRepr::U64 => quote! { u64 },
            PrimitiveRepr::U128 => quote! { u128 },
            PrimitiveRepr::I8 => quote! { i8 },
            PrimitiveRepr::I16 => quote! { i16 },
            PrimitiveRepr::I32 => quote! { i32 },
            PrimitiveRepr::I64 => quote! { i64 },
            PrimitiveRepr::I128 => quote! { i128 },
            PrimitiveRepr::Isize => quote! { isize },
            PrimitiveRepr::Usize => quote! { usize },
        }
    }
}

// =============================================================================
// Rename Rules
// =============================================================================

/// Rename rule for field/variant names
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameRule {
    /// camelCase
    CamelCase,
    /// snake_case
    SnakeCase,
    /// kebab-case
    KebabCase,
    /// PascalCase
    PascalCase,
    /// SCREAMING_SNAKE_CASE
    ScreamingSnakeCase,
    /// SCREAMING-KEBAB-CASE
    ScreamingKebabCase,
    /// lowercase
    Lowercase,
    /// UPPERCASE
    Uppercase,
}

impl RenameRule {
    /// Parse a rename rule from a string
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().trim_matches('"') {
            "camelCase" => Some(RenameRule::CamelCase),
            "snake_case" => Some(RenameRule::SnakeCase),
            "kebab-case" => Some(RenameRule::KebabCase),
            "PascalCase" => Some(RenameRule::PascalCase),
            "SCREAMING_SNAKE_CASE" => Some(RenameRule::ScreamingSnakeCase),
            "SCREAMING-KEBAB-CASE" => Some(RenameRule::ScreamingKebabCase),
            "lowercase" => Some(RenameRule::Lowercase),
            "UPPERCASE" => Some(RenameRule::Uppercase),
            _ => None,
        }
    }

    /// Apply this rename rule to an identifier
    pub fn apply(&self, name: &str) -> String {
        let words = split_into_words(name);

        match self {
            RenameRule::CamelCase => {
                let mut result = String::new();
                for (i, word) in words.iter().enumerate() {
                    if i == 0 {
                        result.push_str(&word.to_lowercase());
                    } else {
                        let mut chars = word.chars();
                        if let Some(first) = chars.next() {
                            result.push(first.to_ascii_uppercase());
                            result.push_str(&chars.collect::<String>().to_lowercase());
                        }
                    }
                }
                result
            }
            RenameRule::SnakeCase => words
                .iter()
                .map(|w| w.to_lowercase())
                .collect::<Vec<_>>()
                .join("_"),
            RenameRule::KebabCase => words
                .iter()
                .map(|w| w.to_lowercase())
                .collect::<Vec<_>>()
                .join("-"),
            RenameRule::PascalCase => words
                .iter()
                .map(|w| {
                    let mut chars = w.chars();
                    match chars.next() {
                        Some(first) => {
                            first.to_ascii_uppercase().to_string()
                                + &chars.collect::<String>().to_lowercase()
                        }
                        None => String::new(),
                    }
                })
                .collect(),
            RenameRule::ScreamingSnakeCase => words
                .iter()
                .map(|w| w.to_uppercase())
                .collect::<Vec<_>>()
                .join("_"),
            RenameRule::ScreamingKebabCase => words
                .iter()
                .map(|w| w.to_uppercase())
                .collect::<Vec<_>>()
                .join("-"),
            RenameRule::Lowercase => name.to_lowercase(),
            RenameRule::Uppercase => name.to_uppercase(),
        }
    }
}

/// Split an identifier into words for rename rule processing
fn split_into_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current_word = String::new();

    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current_word.is_empty() {
                words.push(current_word);
                current_word = String::new();
            }
        } else if ch.is_uppercase() && !current_word.is_empty() {
            words.push(current_word);
            current_word = ch.to_string();
        } else {
            current_word.push(ch);
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
}

// =============================================================================
// Attributes
// =============================================================================

/// A parsed facet attribute.
#[derive(Debug, Clone)]
pub struct PFacetAttr {
    /// The namespace (e.g., "kdl", "args"). None for builtin attributes.
    pub ns: Option<String>,
    /// The key (e.g., "child", "sensitive", "rename")
    pub key: String,
    /// The arguments as a TokenStream
    pub args: TokenStream,
}

impl PFacetAttr {
    /// Returns true if this is a builtin attribute (no namespace)
    pub fn is_builtin(&self) -> bool {
        self.ns.is_none()
    }

    /// Returns the key as a string reference
    pub fn key_str(&self) -> &str {
        &self.key
    }

    /// Returns the args as a trimmed, unquoted string
    pub fn args_string(&self) -> String {
        self.args.to_string().trim().trim_matches('"').to_string()
    }
}

/// A compile error to be emitted during code generation
#[derive(Debug, Clone)]
pub struct CompileError {
    /// The error message
    pub message: String,
    /// The span where the error occurred
    pub span: Span,
}

/// Parsed attributes for a container, field, or variant
#[derive(Debug, Clone, Default)]
pub struct PAttrs {
    /// Doc comment lines
    pub doc: Vec<String>,
    /// Facet attributes
    pub facet: Vec<PFacetAttr>,
    /// Representation
    pub repr: PRepr,
    /// Container-level rename_all rule
    pub rename_all: Option<RenameRule>,
}

impl Default for PRepr {
    fn default() -> Self {
        PRepr::Rust(None)
    }
}

impl PAttrs {
    /// Check if a builtin attribute with the given key exists
    pub fn has_builtin(&self, key: &str) -> bool {
        self.facet.iter().any(|a| a.is_builtin() && a.key == key)
    }

    /// Get the args of a builtin attribute with the given key (if present)
    pub fn get_builtin_args(&self, key: &str) -> Option<String> {
        self.facet
            .iter()
            .find(|a| a.is_builtin() && a.key == key)
            .map(|a| a.args_string())
    }

    /// Get the full doc comment as a single string
    pub fn doc_string(&self) -> String {
        self.doc.join(" ")
    }
}

// =============================================================================
// Generics
// =============================================================================

/// Bounded generic parameters
#[derive(Debug, Clone, Default)]
pub struct BoundedGenericParams {
    /// Lifetime parameters (e.g., ["a", "b"])
    pub lifetimes: Vec<String>,
    /// Type parameters with their bounds (e.g., [(T, "Clone + Debug")])
    pub type_params: Vec<(Ident, TokenStream)>,
    /// Const parameters with their types (e.g., [(N, "usize")])
    pub const_params: Vec<(Ident, TokenStream)>,
}

impl BoundedGenericParams {
    /// Returns true if there are no generic parameters
    pub fn is_empty(&self) -> bool {
        self.lifetimes.is_empty() && self.type_params.is_empty() && self.const_params.is_empty()
    }

    /// Generate the generic parameter list (e.g., `<'a, T: Clone, const N: usize>`)
    pub fn to_param_tokens(&self) -> TokenStream {
        if self.is_empty() {
            return TokenStream::new();
        }

        let mut parts: Vec<TokenStream> = Vec::new();

        for lt in &self.lifetimes {
            let lt_ident = Ident::new(lt, Span::call_site());
            // Build lifetime token manually to avoid quote! escaping issues
            let tick = proc_macro2::Punct::new('\'', proc_macro2::Spacing::Joint);
            parts.push(quote! { #tick #lt_ident });
        }

        for (name, bounds) in &self.type_params {
            if bounds.is_empty() {
                parts.push(quote! { #name });
            } else {
                parts.push(quote! { #name: #bounds });
            }
        }

        for (name, ty) in &self.const_params {
            parts.push(quote! { const #name: #ty });
        }

        quote! { < #(#parts),* > }
    }

    /// Generate just the type arguments (e.g., `<'a, T, N>`)
    pub fn to_arg_tokens(&self) -> TokenStream {
        if self.is_empty() {
            return TokenStream::new();
        }

        let mut parts: Vec<TokenStream> = Vec::new();

        for lt in &self.lifetimes {
            let lt_ident = Ident::new(lt, Span::call_site());
            let tick = proc_macro2::Punct::new('\'', proc_macro2::Spacing::Joint);
            parts.push(quote! { #tick #lt_ident });
        }

        for (name, _) in &self.type_params {
            parts.push(quote! { #name });
        }

        for (name, _) in &self.const_params {
            parts.push(quote! { #name });
        }

        quote! { < #(#parts),* > }
    }
}

// =============================================================================
// Struct Types
// =============================================================================

/// A parsed struct field
#[derive(Debug, Clone)]
pub struct PStructField {
    /// The field's name (with rename rules applied)
    pub name: PName,
    /// The field's type as tokens
    pub ty: TokenStream,
    /// The field's attributes
    pub attrs: PAttrs,
}

impl PStructField {
    /// Get the raw field name for pattern matching
    pub fn raw_ident(&self) -> TokenStream {
        self.name.raw.to_token_stream()
    }

    /// Get the effective name as a string
    pub fn effective_name(&self) -> &str {
        &self.name.effective
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        self.attrs.doc_string()
    }
}

/// Parsed struct kind
#[derive(Debug, Clone)]
pub enum PStructKind {
    /// A regular struct with named fields
    Struct { fields: Vec<PStructField> },
    /// A tuple struct
    Tuple { fields: Vec<PStructField> },
    /// A unit struct
    Unit,
}

impl PStructKind {
    /// Get all fields (empty for unit struct)
    pub fn fields(&self) -> &[PStructField] {
        match self {
            PStructKind::Struct { fields } | PStructKind::Tuple { fields } => fields,
            PStructKind::Unit => &[],
        }
    }

    /// Is this a unit struct?
    pub fn is_unit(&self) -> bool {
        matches!(self, PStructKind::Unit)
    }

    /// Is this a tuple struct?
    pub fn is_tuple(&self) -> bool {
        matches!(self, PStructKind::Tuple { .. })
    }

    /// Is this a named struct?
    pub fn is_named(&self) -> bool {
        matches!(self, PStructKind::Struct { .. })
    }
}

/// A parsed struct
#[derive(Debug, Clone)]
pub struct PStruct {
    /// The struct name
    pub name: Ident,
    /// Attributes
    pub attrs: PAttrs,
    /// Kind of struct
    pub kind: PStructKind,
    /// Generic parameters
    pub generics: Option<BoundedGenericParams>,
}

impl PStruct {
    /// Get the struct name as an identifier
    pub fn name(&self) -> &Ident {
        &self.name
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        self.attrs.doc_string()
    }
}

// =============================================================================
// Enum Types
// =============================================================================

/// Parsed enum variant kind
#[derive(Debug, Clone)]
pub enum PVariantKind {
    /// Unit variant (e.g., `Variant`)
    Unit,
    /// Tuple variant (e.g., `Variant(u32, String)`)
    Tuple { fields: Vec<PStructField> },
    /// Struct variant (e.g., `Variant { field1: u32 }`)
    Struct { fields: Vec<PStructField> },
}

impl PVariantKind {
    /// Get all fields (empty for unit variant)
    pub fn fields(&self) -> &[PStructField] {
        match self {
            PVariantKind::Unit => &[],
            PVariantKind::Tuple { fields } | PVariantKind::Struct { fields } => fields,
        }
    }

    /// Is this a unit variant?
    pub fn is_unit(&self) -> bool {
        matches!(self, PVariantKind::Unit)
    }

    /// Is this a tuple variant?
    pub fn is_tuple(&self) -> bool {
        matches!(self, PVariantKind::Tuple { .. })
    }

    /// Is this a struct variant?
    pub fn is_struct(&self) -> bool {
        matches!(self, PVariantKind::Struct { .. })
    }
}

/// A parsed enum variant
#[derive(Debug, Clone)]
pub struct PVariant {
    /// Name of the variant (with rename rules applied)
    pub name: PName,
    /// Attributes
    pub attrs: PAttrs,
    /// Kind (unit, tuple, or struct)
    pub kind: PVariantKind,
    /// Optional explicit discriminant
    pub discriminant: Option<TokenStream>,
}

impl PVariant {
    /// Get the raw variant name for pattern matching
    pub fn raw_ident(&self) -> &Ident {
        match &self.name.raw {
            IdentOrLiteral::Ident(ident) => ident,
            IdentOrLiteral::Literal(_) => panic!("variant name cannot be a literal"),
        }
    }

    /// Get the effective name as a string
    pub fn effective_name(&self) -> &str {
        &self.name.effective
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        self.attrs.doc_string()
    }
}

/// A parsed enum
#[derive(Debug, Clone)]
pub struct PEnum {
    /// The enum name
    pub name: Ident,
    /// Attributes
    pub attrs: PAttrs,
    /// The variants
    pub variants: Vec<PVariant>,
    /// Generic parameters
    pub generics: Option<BoundedGenericParams>,
}

impl PEnum {
    /// Get the enum name as an identifier
    pub fn name(&self) -> &Ident {
        &self.name
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        self.attrs.doc_string()
    }
}

// =============================================================================
// Unified Type
// =============================================================================

/// A parsed type (struct or enum)
#[derive(Debug, Clone)]
pub enum PType {
    /// A struct
    Struct(PStruct),
    /// An enum
    Enum(PEnum),
}

impl PType {
    /// Get the type name
    pub fn name(&self) -> &Ident {
        match self {
            PType::Struct(s) => s.name(),
            PType::Enum(e) => e.name(),
        }
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        match self {
            PType::Struct(s) => s.doc(),
            PType::Enum(e) => e.doc(),
        }
    }
}
