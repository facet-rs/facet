//! # Facet Macro Types
//!
//! Shared parsed type representations for the facet macro ecosystem.
//!
//! This crate provides the core types that represent parsed Rust types
//! (structs, enums, fields, variants) in a form suitable for code generation.
//!
//! These types are used by:
//! - `facet-macros-impl`: Parses Rust source into these types
//! - `facet-macro-template`: Evaluates templates against these types

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};

// =============================================================================
// Field/Variant Name
// =============================================================================

/// For struct fields, they can either be identifiers (`my_struct.foo`)
/// or literals (`my_struct.2`) — for tuple structs.
#[derive(Debug, Clone)]
pub enum IdentOrLiteral {
    /// Named field identifier
    Ident(proc_macro2::Ident),
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
// Attributes
// =============================================================================

/// A parsed facet attribute.
///
/// All attributes are stored uniformly - either with a namespace (`kdl::child`)
/// or without (`sensitive`).
#[derive(Debug, Clone)]
pub struct PFacetAttr {
    /// The namespace (e.g., "kdl", "args"). None for builtin attributes.
    pub ns: Option<proc_macro2::Ident>,
    /// The key (e.g., "child", "sensitive", "rename")
    pub key: proc_macro2::Ident,
    /// The arguments as a TokenStream
    pub args: TokenStream,
}

impl PFacetAttr {
    /// Returns true if this is a builtin attribute (no namespace)
    pub fn is_builtin(&self) -> bool {
        self.ns.is_none()
    }

    /// Returns the key as a string
    pub fn key_str(&self) -> String {
        self.key.to_string()
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

/// Tracks which standard derives are visible on the type.
#[derive(Debug, Clone, Default)]
pub struct KnownDerives {
    pub debug: bool,
    pub clone: bool,
    pub copy: bool,
    pub partial_eq: bool,
    pub eq: bool,
    pub partial_ord: bool,
    pub ord: bool,
    pub hash: bool,
    pub default: bool,
}

/// Tracks which traits are explicitly declared via `#[facet(traits(...))]`.
#[derive(Debug, Clone, Default)]
pub struct DeclaredTraits {
    pub display: bool,
    pub debug: bool,
    pub clone: bool,
    pub copy: bool,
    pub partial_eq: bool,
    pub eq: bool,
    pub partial_ord: bool,
    pub ord: bool,
    pub hash: bool,
    pub default: bool,
    pub send: bool,
    pub sync: bool,
    pub unpin: bool,
}

impl DeclaredTraits {
    /// Returns true if any trait is declared
    pub fn has_any(&self) -> bool {
        self.display
            || self.debug
            || self.clone
            || self.copy
            || self.partial_eq
            || self.eq
            || self.partial_ord
            || self.ord
            || self.hash
            || self.default
            || self.send
            || self.sync
            || self.unpin
    }
}

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
        match s {
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
        // Split the name into words (handling snake_case, camelCase, etc.)
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
            // camelCase boundary
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

/// Parsed attributes for a container, field, or variant
#[derive(Debug, Clone)]
pub struct PAttrs {
    /// Doc comment lines
    pub doc: Vec<String>,
    /// Facet attributes
    pub facet: Vec<PFacetAttr>,
    /// Representation
    pub repr: PRepr,
    /// Container-level rename_all rule
    pub rename_all: Option<RenameRule>,
    /// Custom crate path (if any)
    pub crate_path: Option<TokenStream>,
    /// Errors to emit
    pub errors: Vec<CompileError>,
    /// Known derives on the type
    pub known_derives: KnownDerives,
    /// Explicitly declared traits
    pub declared_traits: Option<DeclaredTraits>,
    /// Whether auto_traits is enabled
    pub auto_traits: bool,
}

impl Default for PAttrs {
    fn default() -> Self {
        Self {
            doc: Vec::new(),
            facet: Vec::new(),
            repr: PRepr::Rust(None),
            rename_all: None,
            crate_path: None,
            errors: Vec::new(),
            known_derives: KnownDerives::default(),
            declared_traits: None,
            auto_traits: false,
        }
    }
}

impl PAttrs {
    /// Check if a builtin attribute with the given key exists
    pub fn has_builtin(&self, key: &str) -> bool {
        self.facet
            .iter()
            .any(|a| a.is_builtin() && a.key_str() == key)
    }

    /// Get the args of a builtin attribute with the given key (if present)
    pub fn get_builtin_args(&self, key: &str) -> Option<String> {
        self.facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == key)
            .map(|a| a.args.to_string().trim().trim_matches('"').to_string())
    }

    /// Get the facet crate path, defaulting to `::facet` if not specified
    pub fn facet_crate(&self) -> TokenStream {
        self.crate_path
            .clone()
            .unwrap_or_else(|| quote! { ::facet })
    }

    /// Get the full doc comment as a single string
    pub fn doc_string(&self) -> String {
        self.doc.join(" ")
    }
}

// =============================================================================
// Generics
// =============================================================================

/// Bounded generic parameters (simplified)
#[derive(Debug, Clone, Default)]
pub struct BoundedGenericParams {
    /// The full generic parameter list as tokens (e.g., `<T: Clone, U>`)
    pub params: TokenStream,
    /// Just the type parameter names (e.g., `T, U`)
    pub type_params: TokenStream,
    /// Where clause tokens (if any)
    pub where_clause: Option<TokenStream>,
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
    TupleStruct { fields: Vec<PStructField> },
    /// A unit struct
    UnitStruct,
}

impl PStructKind {
    /// Get all fields (empty for unit struct)
    pub fn fields(&self) -> &[PStructField] {
        match self {
            PStructKind::Struct { fields } | PStructKind::TupleStruct { fields } => fields,
            PStructKind::UnitStruct => &[],
        }
    }

    /// Is this a unit struct?
    pub fn is_unit(&self) -> bool {
        matches!(self, PStructKind::UnitStruct)
    }

    /// Is this a tuple struct?
    pub fn is_tuple(&self) -> bool {
        matches!(self, PStructKind::TupleStruct { .. })
    }

    /// Is this a named struct?
    pub fn is_named(&self) -> bool {
        matches!(self, PStructKind::Struct { .. })
    }
}

/// Parsed container (shared between struct and enum)
#[derive(Debug, Clone)]
pub struct PContainer {
    /// Name of the container
    pub name: proc_macro2::Ident,
    /// Attributes
    pub attrs: PAttrs,
    /// Generic parameters
    pub bgp: BoundedGenericParams,
}

/// A parsed struct
#[derive(Debug, Clone)]
pub struct PStruct {
    /// Container information
    pub container: PContainer,
    /// Kind of struct
    pub kind: PStructKind,
}

impl PStruct {
    /// Get the struct name as an identifier
    pub fn name(&self) -> &proc_macro2::Ident {
        &self.container.name
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        self.container.attrs.doc_string()
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
    pub fn raw_ident(&self) -> &proc_macro2::Ident {
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
    /// Container information
    pub container: PContainer,
    /// The variants
    pub variants: Vec<PVariant>,
    /// The representation
    pub repr: PRepr,
}

impl PEnum {
    /// Get the enum name as an identifier
    pub fn name(&self) -> &proc_macro2::Ident {
        &self.container.name
    }

    /// Get the doc comment
    pub fn doc(&self) -> String {
        self.container.attrs.doc_string()
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
    pub fn name(&self) -> &proc_macro2::Ident {
        match self {
            PType::Struct(s) => s.name(),
            PType::Enum(e) => e.name(),
        }
    }

    /// Get the container
    pub fn container(&self) -> &PContainer {
        match self {
            PType::Struct(s) => &s.container,
            PType::Enum(e) => &e.container,
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
