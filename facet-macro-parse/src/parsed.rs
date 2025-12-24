use crate::{BoundedGenericParams, RenameRule, unescape};
use crate::{Ident, ReprInner, ToTokens, TokenStream};
use proc_macro2::Span;
use quote::{quote, quote_spanned};

/// Errors that can occur during parsing of derive macro attributes.
///
/// Some errors are caught by rustc itself, so we don't emit duplicate diagnostics.
/// Others are facet-specific and need our own compile_error!.
#[derive(Debug)]
pub enum ParseError {
    /// An error that rustc will catch on its own - we don't emit a diagnostic.
    ///
    /// We track these so the code is explicit about why we're not panicking,
    /// and to document what rustc catches.
    RustcWillCatch {
        /// Description of what rustc will catch (for documentation purposes)
        reason: &'static str,
    },

    /// A facet-specific error that rustc won't catch - we emit compile_error!
    FacetError {
        /// The error message to display
        message: String,
        /// The span to point the error at
        span: Span,
    },
}

impl ParseError {
    /// Create a "rustc will catch this" error.
    ///
    /// Use this when we detect an error that rustc will also catch,
    /// so we avoid duplicate diagnostics.
    pub fn rustc_will_catch(reason: &'static str) -> Self {
        ParseError::RustcWillCatch { reason }
    }

    /// Create a facet-specific error with a span.
    pub fn facet_error(message: impl Into<String>, span: Span) -> Self {
        ParseError::FacetError {
            message: message.into(),
            span,
        }
    }

    /// Convert to a compile_error! TokenStream, or None if rustc will catch it.
    pub fn to_compile_error(&self) -> Option<TokenStream> {
        match self {
            ParseError::RustcWillCatch { .. } => None,
            ParseError::FacetError { message, span } => {
                Some(quote_spanned! { *span => compile_error!(#message); })
            }
        }
    }
}

/// For struct fields, they can either be identifiers (`my_struct.foo`)
/// or literals (`my_struct.2`) — for tuple structs.
#[derive(Clone)]
pub enum IdentOrLiteral {
    /// Named field identifier
    Ident(Ident),
    /// Tuple field index
    Literal(usize),
}

impl quote::ToTokens for IdentOrLiteral {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            IdentOrLiteral::Ident(ident) => tokens.extend(quote::quote! { #ident }),
            IdentOrLiteral::Literal(lit) => {
                let unsuffixed = crate::Literal::usize_unsuffixed(*lit);
                tokens.extend(quote! { #unsuffixed })
            }
        }
    }
}

/// A parsed facet attribute.
///
/// All attributes are now stored uniformly - either with a namespace (`kdl::child`)
/// or without (`sensitive`). The grammar system handles validation and semantics.
#[derive(Clone)]
pub struct PFacetAttr {
    /// The namespace (e.g., "kdl", "args"). None for builtin attributes.
    pub ns: Option<Ident>,
    /// The key (e.g., "child", "sensitive", "rename")
    pub key: Ident,
    /// The arguments as a TokenStream
    pub args: TokenStream,
}

impl PFacetAttr {
    /// Parse a `FacetAttr` attribute into `PFacetAttr` entries.
    ///
    /// All attributes are captured uniformly as ns/key/args.
    /// The grammar system handles validation - we just capture the tokens.
    pub fn parse(facet_attr: &crate::FacetAttr, dest: &mut Vec<PFacetAttr>) {
        use crate::{AttrArgs, FacetInner, ToTokens};

        for attr in facet_attr.inner.content.iter().map(|d| &d.value) {
            match attr {
                // Namespaced attributes like `kdl::child` or `args::short = 'v'`
                FacetInner::Namespaced(ext) => {
                    let args = match &ext.args {
                        Some(AttrArgs::Parens(p)) => p.content.to_token_stream(),
                        Some(AttrArgs::Equals(e)) => e.value.to_token_stream(),
                        None => TokenStream::new(),
                    };
                    dest.push(PFacetAttr {
                        ns: Some(ext.ns.clone()),
                        key: ext.key.clone(),
                        args,
                    });
                }

                // Simple (builtin) attributes like `sensitive` or `rename = "foo"`
                FacetInner::Simple(simple) => {
                    let args = match &simple.args {
                        Some(AttrArgs::Parens(p)) => p.content.to_token_stream(),
                        Some(AttrArgs::Equals(e)) => e.value.to_token_stream(),
                        None => TokenStream::new(),
                    };
                    dest.push(PFacetAttr {
                        ns: None,
                        key: simple.key.clone(),
                        args,
                    });
                }
            }
        }
    }

    /// Returns true if this is a builtin attribute (no namespace)
    pub fn is_builtin(&self) -> bool {
        self.ns.is_none()
    }

    /// Returns the key as a string
    pub fn key_str(&self) -> String {
        self.key.to_string()
    }
}

/// Parsed attr
pub enum PAttr {
    /// A single line of doc comments
    /// `#[doc = "Some doc"], or `/// Some doc`, same thing
    Doc {
        /// The doc comment text
        line: String,
    },

    /// A representation attribute
    Repr {
        /// The parsed repr
        repr: PRepr,
    },

    /// A facet attribute
    Facet {
        /// The facet attribute name
        name: String,
    },
}

/// A parsed name, which includes the raw name and the
/// effective name.
///
/// Examples:
///
///   raw = "foo_bar", no rename rule, effective = "foo_bar"
///   raw = "foo_bar", #[facet(rename = "kiki")], effective = "kiki"
///   raw = "foo_bar", #[facet(rename_all = camelCase)], effective = "fooBar"
///   raw = "r#type", no rename rule, effective = "type"
///
#[derive(Clone)]
pub struct PName {
    /// The raw identifier, as we found it in the source code. It might
    /// be _actually_ raw, as in "r#keyword".
    pub raw: IdentOrLiteral,

    /// The name after applying rename rules, which might not be a valid identifier in Rust.
    /// It could be a number. It could be a kebab-case thing.
    pub effective: String,
}

impl PName {
    /// Constructs a new `PName` with the given raw name, an optional container-level rename rule,
    /// an optional field-level rename rule, and a raw identifier.
    ///
    /// Precedence:
    ///   - If field_rename_rule is Some, use it on raw for effective name
    ///   - Else if container_rename_rule is Some, use it on raw for effective name
    ///   - Else, strip raw ("r#" if present) for effective name
    pub fn new(container_rename_rule: Option<RenameRule>, raw: IdentOrLiteral) -> Self {
        // Remove Rust's raw identifier prefix, e.g. r#type -> type
        let norm_raw_str = match &raw {
            IdentOrLiteral::Ident(ident) => ident
                .tokens_to_string()
                .trim_start_matches("r#")
                .to_string(),
            IdentOrLiteral::Literal(l) => l.to_string(),
        };

        let effective = if let Some(container_rule) = container_rename_rule {
            container_rule.apply(&norm_raw_str)
        } else {
            norm_raw_str // Use the normalized string (without r#)
        };

        Self {
            raw: raw.clone(), // Keep the original raw identifier
            effective,
        }
    }
}

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
    /// We use this sentinel to avoid emitting our own misleading errors.
    RustcWillCatch,
}

impl PRepr {
    /// Parse a `&str` (for example a value coming from #[repr(...)] attribute)
    /// into a `PRepr` variant.
    ///
    /// Returns `Err(ParseError::RustcWillCatch { .. })` for errors that rustc
    /// will catch on its own (conflicting repr hints). Returns
    /// `Err(ParseError::FacetError { .. })` for facet-specific errors like
    /// unsupported repr types (e.g., `packed`).
    pub fn parse(s: &ReprInner) -> Result<Option<Self>, ParseError> {
        enum ReprKind {
            Rust,
            C,
        }

        let items = s.attr.content.as_slice();
        let mut repr_kind: Option<ReprKind> = None;
        let mut primitive_repr: Option<PrimitiveRepr> = None;
        let mut is_transparent = false;

        for token_delimited in items {
            let token_str = token_delimited.value.to_string();
            let token_span = token_delimited.value.span();

            match token_str.as_str() {
                "C" | "c" => {
                    if repr_kind.is_some() && !matches!(repr_kind, Some(ReprKind::C)) {
                        // rustc emits E0566: conflicting representation hints
                        return Err(ParseError::rustc_will_catch(
                            "E0566: conflicting representation hints (C vs Rust)",
                        ));
                    }
                    if is_transparent {
                        // rustc emits E0692: transparent struct/enum cannot have other repr hints
                        return Err(ParseError::rustc_will_catch(
                            "E0692: transparent cannot have other repr hints",
                        ));
                    }
                    repr_kind = Some(ReprKind::C);
                }
                "Rust" | "rust" => {
                    if repr_kind.is_some() && !matches!(repr_kind, Some(ReprKind::Rust)) {
                        // rustc emits E0566: conflicting representation hints
                        return Err(ParseError::rustc_will_catch(
                            "E0566: conflicting representation hints (Rust vs C)",
                        ));
                    }
                    if is_transparent {
                        // rustc emits E0692: transparent struct/enum cannot have other repr hints
                        return Err(ParseError::rustc_will_catch(
                            "E0692: transparent cannot have other repr hints",
                        ));
                    }
                    repr_kind = Some(ReprKind::Rust);
                }
                "transparent" => {
                    if repr_kind.is_some() || primitive_repr.is_some() {
                        // rustc emits E0692: transparent struct/enum cannot have other repr hints
                        return Err(ParseError::rustc_will_catch(
                            "E0692: transparent cannot have other repr hints",
                        ));
                    }
                    is_transparent = true;
                }
                prim_str @ ("u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32"
                | "i64" | "i128" | "usize" | "isize") => {
                    let current_prim = match prim_str {
                        "u8" => PrimitiveRepr::U8,
                        "u16" => PrimitiveRepr::U16,
                        "u32" => PrimitiveRepr::U32,
                        "u64" => PrimitiveRepr::U64,
                        "u128" => PrimitiveRepr::U128,
                        "i8" => PrimitiveRepr::I8,
                        "i16" => PrimitiveRepr::I16,
                        "i32" => PrimitiveRepr::I32,
                        "i64" => PrimitiveRepr::I64,
                        "i128" => PrimitiveRepr::I128,
                        "usize" => PrimitiveRepr::Usize,
                        "isize" => PrimitiveRepr::Isize,
                        _ => unreachable!(),
                    };
                    if is_transparent {
                        // rustc emits E0692: transparent struct/enum cannot have other repr hints
                        return Err(ParseError::rustc_will_catch(
                            "E0692: transparent cannot have other repr hints",
                        ));
                    }
                    if primitive_repr.is_some() {
                        // rustc emits E0566: conflicting representation hints
                        return Err(ParseError::rustc_will_catch(
                            "E0566: conflicting representation hints (multiple primitives)",
                        ));
                    }
                    primitive_repr = Some(current_prim);
                }
                unknown => {
                    // This is a facet-specific error: rustc accepts things like `packed`,
                    // `align(N)`, etc., but facet doesn't support them.
                    return Err(ParseError::facet_error(
                        format!(
                            "unsupported repr `{unknown}` - facet only supports \
                             C, Rust, transparent, and primitive integer types"
                        ),
                        token_span,
                    ));
                }
            }
        }

        // Final construction
        if is_transparent {
            debug_assert!(
                repr_kind.is_none() && primitive_repr.is_none(),
                "internal error: transparent repr mixed with other kinds after parsing"
            );
            Ok(Some(PRepr::Transparent))
        } else {
            let final_kind = repr_kind.unwrap_or(ReprKind::Rust);
            Ok(Some(match final_kind {
                ReprKind::Rust => PRepr::Rust(primitive_repr),
                ReprKind::C => PRepr::C(primitive_repr),
            }))
        }
    }
}

/// Primitive repr types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveRepr {
    /// `u8`
    U8,
    /// `u16`
    U16,
    /// `u32`
    U32,
    /// `u64`
    U64,
    /// `u128`
    U128,
    /// `i8`
    I8,
    /// `i16`
    I16,
    /// `i32`
    I32,
    /// `i64`
    I64,
    /// `i128`
    I128,
    /// `isize`
    Isize,
    /// `usize`
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

/// A compile error to be emitted during code generation
#[derive(Clone)]
pub struct CompileError {
    /// The error message
    pub message: String,
    /// The span where the error occurred
    pub span: Span,
}

/// Tracks which traits are explicitly declared via `#[facet(traits(...))]`.
///
/// When this is present, we skip all `impls!` checks and only generate
/// vtable entries for the declared traits.
#[derive(Clone, Default)]
pub struct DeclaredTraits {
    /// Display trait declared
    pub display: bool,
    /// Debug trait declared
    pub debug: bool,
    /// Clone trait declared
    pub clone: bool,
    /// Copy trait declared (marker)
    pub copy: bool,
    /// PartialEq trait declared
    pub partial_eq: bool,
    /// Eq trait declared (marker)
    pub eq: bool,
    /// PartialOrd trait declared
    pub partial_ord: bool,
    /// Ord trait declared
    pub ord: bool,
    /// Hash trait declared
    pub hash: bool,
    /// Default trait declared
    pub default: bool,
    /// Send trait declared (marker)
    pub send: bool,
    /// Sync trait declared (marker)
    pub sync: bool,
    /// Unpin trait declared (marker)
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

    /// Parse traits from a token stream like `Debug, PartialEq, Clone, Send`
    pub fn parse_from_tokens(tokens: &TokenStream, errors: &mut Vec<CompileError>) -> Self {
        let mut result = DeclaredTraits::default();

        for token in tokens.clone() {
            if let proc_macro2::TokenTree::Ident(ident) = token {
                let name = ident.to_string();
                match name.as_str() {
                    "Display" => result.display = true,
                    "Debug" => result.debug = true,
                    "Clone" => result.clone = true,
                    "Copy" => result.copy = true,
                    "PartialEq" => result.partial_eq = true,
                    "Eq" => result.eq = true,
                    "PartialOrd" => result.partial_ord = true,
                    "Ord" => result.ord = true,
                    "Hash" => result.hash = true,
                    "Default" => result.default = true,
                    "Send" => result.send = true,
                    "Sync" => result.sync = true,
                    "Unpin" => result.unpin = true,
                    unknown => {
                        errors.push(CompileError {
                            message: format!(
                                "unknown trait `{unknown}` in #[facet(traits(...))]. \
                                 Valid traits: Display, Debug, Clone, Copy, PartialEq, Eq, \
                                 PartialOrd, Ord, Hash, Default, Send, Sync, Unpin"
                            ),
                            span: ident.span(),
                        });
                    }
                }
            }
        }

        result
    }
}

/// Parsed attributes
#[derive(Clone)]
pub struct PAttrs {
    /// An array of doc lines
    pub doc: Vec<String>,

    /// Facet attributes specifically
    pub facet: Vec<PFacetAttr>,

    /// Representation of the facet
    pub repr: PRepr,

    /// rename_all rule (if any)
    pub rename_all: Option<RenameRule>,

    /// Custom crate path (if any), e.g., `::my_crate::facet`
    pub crate_path: Option<TokenStream>,

    /// Errors to be emitted as compile_error! during code generation
    pub errors: Vec<CompileError>,

    /// Explicitly declared traits via `#[facet(traits(...))]`
    /// When present, we skip all `impls!` checks and only generate vtable
    /// entries for the declared traits.
    pub declared_traits: Option<DeclaredTraits>,

    /// Whether `#[facet(auto_traits)]` is present
    /// When true, we use the old specialization-based detection.
    /// When false (and no declared_traits), we generate an empty vtable.
    pub auto_traits: bool,
}

impl PAttrs {
    /// Parse attributes from a list of `Attribute`s
    pub fn parse(attrs: &[crate::Attribute], display_name: &mut String) -> Self {
        let mut doc_lines: Vec<String> = Vec::new();
        let mut facet_attrs: Vec<PFacetAttr> = Vec::new();
        let mut repr: Option<PRepr> = None;
        let mut rename_all: Option<RenameRule> = None;
        let mut crate_path: Option<TokenStream> = None;
        let mut errors: Vec<CompileError> = Vec::new();

        for attr in attrs {
            match &attr.body.content {
                crate::AttributeInner::Doc(doc_attr) => {
                    let unescaped_text =
                        unescape(doc_attr).expect("invalid escape sequence in doc string");
                    doc_lines.push(unescaped_text);
                }
                crate::AttributeInner::Repr(repr_attr) => {
                    if repr.is_some() {
                        // rustc emits E0566: conflicting representation hints
                        // for multiple #[repr] attributes - use sentinel
                        repr = Some(PRepr::RustcWillCatch);
                        continue;
                    }

                    match PRepr::parse(repr_attr) {
                        Ok(Some(parsed)) => repr = Some(parsed),
                        Ok(None) => { /* empty repr, use default */ }
                        Err(ParseError::RustcWillCatch { .. }) => {
                            // rustc will emit the error - use sentinel so we don't
                            // emit misleading "missing repr" errors later
                            repr = Some(PRepr::RustcWillCatch);
                        }
                        Err(ParseError::FacetError { message, span }) => {
                            errors.push(CompileError { message, span });
                        }
                    }
                }
                crate::AttributeInner::Facet(facet_attr) => {
                    PFacetAttr::parse(facet_attr, &mut facet_attrs);
                }
                // Note: Rust strips #[derive(...)] attributes before passing to derive macros,
                // so we cannot detect them here. Users must use #[facet(traits(...))] instead.
                crate::AttributeInner::Any(tokens) => {
                    // WORKAROUND: Doc comments with raw string literals (r"...") are not
                    // recognized by the DocInner parser, so they end up as Any attributes.
                    // Parse them manually here: doc = <string literal>
                    if tokens.len() == 3
                        && let Some(proc_macro2::TokenTree::Ident(id)) = tokens.first()
                        && id == "doc"
                        && let Some(proc_macro2::TokenTree::Literal(lit)) = tokens.get(2)
                    {
                        // Extract the string value from the literal
                        let lit_str = lit.to_string();
                        // Handle both regular strings "..." and raw strings r"..."
                        let content = if lit_str.starts_with("r#") {
                            // Raw string with hashes: r#"..."#, r##"..."##, etc.
                            let hash_count =
                                lit_str.chars().skip(1).take_while(|&c| c == '#').count();
                            let start = 2 + hash_count + 1; // r + hashes + "
                            let end = lit_str.len() - 1 - hash_count; // " + hashes
                            lit_str[start..end].to_string()
                        } else if lit_str.starts_with('r') {
                            // Simple raw string: r"..."
                            let content = &lit_str[2..lit_str.len() - 1];
                            content.to_string()
                        } else {
                            // Regular string: "..." - needs unescaping
                            let trimmed = lit_str.trim_matches('"');
                            match crate::unescape_inner(trimmed) {
                                Ok(s) => s,
                                Err(_) => continue, // Skip malformed strings
                            }
                        };
                        doc_lines.push(content);
                    }
                }
            }
        }

        // Extract rename, rename_all, crate, traits, and auto_traits from parsed attrs
        let mut declared_traits: Option<DeclaredTraits> = None;
        let mut auto_traits = false;

        for attr in &facet_attrs {
            if attr.is_builtin() {
                match attr.key_str().as_str() {
                    "rename" => {
                        let s = attr.args.to_string();
                        let trimmed = s.trim().trim_matches('"');
                        *display_name = trimmed.to_string();
                    }
                    "rename_all" => {
                        let s = attr.args.to_string();
                        let rule_str = s.trim().trim_matches('"');
                        if let Some(rule) = RenameRule::parse(rule_str) {
                            rename_all = Some(rule);
                        } else {
                            errors.push(CompileError {
                                message: format!(
                                    "unknown #[facet(rename_all = \"...\")] rule: `{rule_str}`. \
                                     Valid options: camelCase, snake_case, kebab-case, \
                                     PascalCase, SCREAMING_SNAKE_CASE, SCREAMING-KEBAB-CASE, \
                                     lowercase, UPPERCASE"
                                ),
                                span: attr.key.span(),
                            });
                        }
                    }
                    "crate" => {
                        // Store the crate path tokens directly
                        crate_path = Some(attr.args.clone());
                    }
                    "traits" => {
                        // Parse #[facet(traits(Debug, PartialEq, Clone, ...))]
                        declared_traits =
                            Some(DeclaredTraits::parse_from_tokens(&attr.args, &mut errors));
                    }
                    "auto_traits" => {
                        // #[facet(auto_traits)] enables specialization-based detection
                        auto_traits = true;
                    }
                    _ => {}
                }
            }
        }

        // Validate: traits(...) and auto_traits are mutually exclusive
        if declared_traits.is_some()
            && auto_traits
            && let Some(span) = facet_attrs
                .iter()
                .find(|a| a.is_builtin() && a.key_str() == "auto_traits")
                .map(|a| a.key.span())
        {
            errors.push(CompileError {
                message: "cannot use both #[facet(traits(...))] and #[facet(auto_traits)] \
                              on the same type"
                    .to_string(),
                span,
            });
        }

        Self {
            doc: doc_lines,
            facet: facet_attrs,
            repr: repr.unwrap_or(PRepr::Rust(None)),
            rename_all,
            crate_path,
            errors,
            declared_traits,
            auto_traits,
        }
    }

    /// Check if a builtin attribute with the given key exists
    pub fn has_builtin(&self, key: &str) -> bool {
        self.facet
            .iter()
            .any(|a| a.is_builtin() && a.key_str() == key)
    }

    /// Check if `#[repr(transparent)]` is present
    pub fn is_repr_transparent(&self) -> bool {
        matches!(self.repr, PRepr::Transparent)
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

    /// Check if any namespaced attribute exists (e.g., `kdl::child`, `args::short`)
    ///
    /// When a namespaced attribute is present, `rename` on a container may be valid
    /// because it controls how the type appears in that specific context.
    pub fn has_any_namespaced(&self) -> bool {
        self.facet.iter().any(|a| a.ns.is_some())
    }

    /// Get the span of a builtin attribute with the given key (if present)
    pub fn get_builtin_span(&self, key: &str) -> Option<Span> {
        self.facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == key)
            .map(|a| a.key.span())
    }
}

/// Parsed container
pub struct PContainer {
    /// Name of the container (could be a struct, an enum variant, etc.)
    pub name: Ident,

    /// Attributes of the container
    pub attrs: PAttrs,

    /// Generic parameters of the container
    pub bgp: BoundedGenericParams,
}

/// Parse struct
pub struct PStruct {
    /// Container information
    pub container: PContainer,

    /// Kind of struct
    pub kind: PStructKind,
}

/// Parsed enum (given attributes etc.)
pub struct PEnum {
    /// Container information
    pub container: PContainer,
    /// The variants of the enum, in parsed form
    pub variants: Vec<PVariant>,
    /// The representation (repr) for the enum (e.g., C, u8, etc.)
    pub repr: PRepr,
}

impl PEnum {
    /// Parse a `crate::Enum` into a `PEnum`.
    pub fn parse(e: &crate::Enum) -> Self {
        let mut container_display_name = e.name.to_string();

        // Parse container-level attributes (including repr and any errors)
        let attrs = PAttrs::parse(&e.attributes, &mut container_display_name);

        // Get the container-level rename_all rule
        let container_rename_all_rule = attrs.rename_all;

        // Get repr from already-parsed attrs
        let repr = attrs.repr;

        // Build PContainer
        let container = PContainer {
            name: e.name.clone(),
            attrs,
            bgp: BoundedGenericParams::parse(e.generics.as_ref()),
        };

        // Parse variants, passing the container's rename_all rule
        let variants = e
            .body
            .content
            .iter()
            .map(|delim| PVariant::parse(&delim.value, container_rename_all_rule))
            .collect();

        PEnum {
            container,
            variants,
            repr,
        }
    }
}

/// Parsed field
#[derive(Clone)]
pub struct PStructField {
    /// The field's name (with rename rules applied)
    pub name: PName,

    /// The field's type
    pub ty: TokenStream,

    /// The field's offset (can be an expression, like `offset_of!(self, field)`)
    pub offset: TokenStream,

    /// The field's attributes
    pub attrs: PAttrs,
}

impl PStructField {
    /// Parse a named struct field (usual struct).
    pub fn from_struct_field(f: &crate::StructField, rename_all_rule: Option<RenameRule>) -> Self {
        use crate::ToTokens;
        Self::parse_field(
            &f.attributes,
            IdentOrLiteral::Ident(f.name.clone()),
            f.typ.to_token_stream(),
            rename_all_rule,
        )
    }

    /// Parse a tuple (unnamed) field for tuple structs or enum tuple variants.
    /// The index is converted to an identifier like `_0`, `_1`, etc.
    pub fn from_enum_field(
        attrs: &[crate::Attribute],
        idx: usize,
        typ: &crate::VerbatimUntil<crate::Comma>,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use crate::ToTokens;
        // Create an Ident from the index, using `_` prefix convention for tuple fields
        let ty = typ.to_token_stream(); // Convert to TokenStream
        Self::parse_field(attrs, IdentOrLiteral::Literal(idx), ty, rename_all_rule)
    }

    /// Central parse function used by both `from_struct_field` and `from_enum_field`.
    fn parse_field(
        attrs: &[crate::Attribute],
        name: IdentOrLiteral,
        ty: TokenStream,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        let initial_display_name = quote::ToTokens::to_token_stream(&name).tokens_to_string();
        let mut display_name = initial_display_name.clone();

        // Parse attributes for the field
        let attrs = PAttrs::parse(attrs, &mut display_name);

        // Name resolution:
        // Precedence:
        //   1. Field-level #[facet(rename = "...")]
        //   2. rename_all_rule argument (container-level rename_all, passed in)
        //   3. Raw field name (after stripping "r#")
        let raw = name.clone();

        let p_name = if display_name != initial_display_name {
            // If #[facet(rename = "...")] is present, use it directly as the effective name.
            // Preserve the span of the original identifier.
            PName {
                raw: raw.clone(),
                effective: display_name,
            }
        } else {
            // Use PName::new logic with container_rename_rule as the rename_all_rule argument.
            // PName::new handles the case where rename_all_rule is None.
            PName::new(rename_all_rule, raw)
        };

        // Field type as TokenStream (already provided as argument)
        let ty = ty.clone();

        // Offset string -- we don't know the offset here in generic parsing, so just default to empty
        let offset = quote! {};

        PStructField {
            name: p_name,
            ty,
            offset,
            attrs,
        }
    }
}
/// Parsed struct kind, modeled after `StructKind`.
pub enum PStructKind {
    /// A regular struct with named fields.
    Struct {
        /// The struct fields
        fields: Vec<PStructField>,
    },
    /// A tuple struct.
    TupleStruct {
        /// The tuple fields
        fields: Vec<PStructField>,
    },
    /// A unit struct.
    UnitStruct,
}

impl PStructKind {
    /// Parse a `crate::StructKind` into a `PStructKind`.
    /// Passes rename_all_rule through to all PStructField parsing.
    pub fn parse(kind: &crate::StructKind, rename_all_rule: Option<RenameRule>) -> Self {
        match kind {
            crate::StructKind::Struct { clauses: _, fields } => {
                let parsed_fields = fields
                    .content
                    .iter()
                    .map(|delim| PStructField::from_struct_field(&delim.value, rename_all_rule))
                    .collect();
                PStructKind::Struct {
                    fields: parsed_fields,
                }
            }
            crate::StructKind::TupleStruct {
                fields,
                clauses: _,
                semi: _,
            } => {
                let parsed_fields = fields
                    .content
                    .iter()
                    .enumerate()
                    .map(|(idx, delim)| {
                        PStructField::from_enum_field(
                            &delim.value.attributes,
                            idx,
                            &delim.value.typ,
                            rename_all_rule,
                        )
                    })
                    .collect();
                PStructKind::TupleStruct {
                    fields: parsed_fields,
                }
            }
            crate::StructKind::UnitStruct {
                clauses: _,
                semi: _,
            } => PStructKind::UnitStruct,
        }
    }
}

impl PStruct {
    /// Parse a struct into its parsed representation
    pub fn parse(s: &crate::Struct) -> Self {
        let original_name = s.name.to_string();
        let mut container_display_name = original_name.clone();

        // Parse top-level (container) attributes for the struct.
        let attrs = PAttrs::parse(&s.attributes, &mut container_display_name);

        // Note: #[facet(rename = "...")] on structs is allowed. While for formats like JSON
        // the container name is determined by the parent field, formats like XML and KDL
        // use the container's rename as the element/node name (especially for root elements).
        // See: https://github.com/facet-rs/facet/issues/1018

        // Extract the rename_all rule *after* parsing all attributes.
        let rename_all_rule = attrs.rename_all;

        // Build PContainer from struct's name and attributes.
        let container = PContainer {
            name: s.name.clone(),
            attrs, // Use the parsed attributes (which includes rename_all implicitly)
            bgp: BoundedGenericParams::parse(s.generics.as_ref()),
        };

        // Pass the container's rename_all rule (extracted above) as argument to PStructKind::parse
        let kind = PStructKind::parse(&s.kind, rename_all_rule);

        PStruct { container, kind }
    }
}

/// Parsed enum variant kind
pub enum PVariantKind {
    /// Unit variant, e.g., `Variant`.
    Unit,
    /// Tuple variant, e.g., `Variant(u32, String)`.
    Tuple {
        /// The tuple variant fields
        fields: Vec<PStructField>,
    },
    /// Struct variant, e.g., `Variant { field1: u32, field2: String }`.
    Struct {
        /// The struct variant fields
        fields: Vec<PStructField>,
    },
}

/// Parsed enum variant
pub struct PVariant {
    /// Name of the variant (with rename rules applied)
    pub name: PName,
    /// Attributes of the variant
    pub attrs: PAttrs,
    /// Kind of the variant (unit, tuple, or struct)
    pub kind: PVariantKind,
    /// Optional explicit discriminant (`= literal`)
    pub discriminant: Option<TokenStream>,
}

impl PVariant {
    /// Parses an `EnumVariantLike` from `facet_macros_parse` into a `PVariant`.
    ///
    /// Requires the container-level `rename_all` rule to correctly determine the
    /// effective name of the variant itself. The variant's own `rename_all` rule
    /// (if present) will be stored in `attrs.rename_all` and used for its fields.
    fn parse(
        var_like: &crate::EnumVariantLike,
        container_rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use crate::{EnumVariantData, StructEnumVariant, TupleVariant, UnitVariant};

        let (raw_name_ident, attributes) = match &var_like.variant {
            // Fix: Changed var_like.value.variant to var_like.variant
            EnumVariantData::Unit(UnitVariant { name, attributes })
            | EnumVariantData::Tuple(TupleVariant {
                name, attributes, ..
            })
            | EnumVariantData::Struct(StructEnumVariant {
                name, attributes, ..
            }) => (name, attributes),
        };

        let initial_display_name = raw_name_ident.to_string();
        let mut display_name = initial_display_name.clone();

        // Parse variant attributes, potentially modifying display_name if #[facet(rename=...)] is found
        let attrs = PAttrs::parse(attributes.as_slice(), &mut display_name); // Fix: Pass attributes as a slice

        // Determine the variant's effective name
        let name = if display_name != initial_display_name {
            // #[facet(rename=...)] was present on the variant
            PName {
                raw: IdentOrLiteral::Ident(raw_name_ident.clone()),
                effective: display_name,
            }
        } else {
            // Use container's rename_all rule if no variant-specific rename found
            PName::new(
                container_rename_all_rule,
                IdentOrLiteral::Ident(raw_name_ident.clone()),
            )
        };

        // Extract the variant's own rename_all rule to apply to its fields
        let variant_field_rename_rule = attrs.rename_all;

        // Parse the variant kind and its fields
        let kind = match &var_like.variant {
            // Fix: Changed var_like.value.variant to var_like.variant
            EnumVariantData::Unit(_) => PVariantKind::Unit,
            EnumVariantData::Tuple(TupleVariant { fields, .. }) => {
                let parsed_fields = fields
                    .content
                    .iter()
                    .enumerate()
                    .map(|(idx, delim)| {
                        PStructField::from_enum_field(
                            &delim.value.attributes,
                            idx,
                            &delim.value.typ,
                            variant_field_rename_rule, // Use variant's rule for its fields
                        )
                    })
                    .collect();
                PVariantKind::Tuple {
                    fields: parsed_fields,
                }
            }
            EnumVariantData::Struct(StructEnumVariant { fields, .. }) => {
                let parsed_fields = fields
                    .content
                    .iter()
                    .map(|delim| {
                        PStructField::from_struct_field(
                            &delim.value,
                            variant_field_rename_rule, // Use variant's rule for its fields
                        )
                    })
                    .collect();
                PVariantKind::Struct {
                    fields: parsed_fields,
                }
            }
        };

        // Extract the discriminant literal if present
        let discriminant = var_like
            .discriminant
            .as_ref()
            .map(|d| d.second.to_token_stream());

        PVariant {
            name,
            attrs,
            kind,
            discriminant,
        }
    }
}
