use crate::{BoundedGenericParams, RenameRule, unescaping::unescape};
use crate::{Ident, ReprInner, ToTokens, TokenStream};
use quote::quote;

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
}

impl PRepr {
    /// Parse a `&str` (for example a value coming from #[repr(...)] attribute)
    /// into a `PRepr` variant.
    pub fn parse(s: &ReprInner) -> Option<Self> {
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
            match token_str.as_str() {
                "C" | "c" => {
                    if repr_kind.is_some() && !matches!(repr_kind, Some(ReprKind::C)) {
                        panic!(
                            "Conflicting repr kinds found in #[repr(...)]. Cannot mix C/c and Rust/rust."
                        );
                    }
                    if is_transparent {
                        panic!(
                            "Conflicting repr kinds found in #[repr(...)]. Cannot mix C/c and transparent."
                        );
                    }
                    // If primitive is already set, and kind is not already C, ensure kind becomes C.
                    // Example: #[repr(u8, C)] is valid.
                    repr_kind = Some(ReprKind::C);
                }
                "Rust" | "rust" => {
                    if repr_kind.is_some() && !matches!(repr_kind, Some(ReprKind::Rust)) {
                        panic!(
                            "Conflicting repr kinds found in #[repr(...)]. Cannot mix Rust/rust and C/c."
                        );
                    }
                    if is_transparent {
                        panic!(
                            "Conflicting repr kinds found in #[repr(...)]. Cannot mix Rust/rust and transparent."
                        );
                    }
                    // If primitive is already set, and kind is not already Rust, ensure kind becomes Rust.
                    // Example: #[repr(i32, Rust)] is valid.
                    repr_kind = Some(ReprKind::Rust);
                }
                "transparent" => {
                    if repr_kind.is_some() || primitive_repr.is_some() {
                        panic!(
                            "Conflicting repr kinds found in #[repr(...)]. Cannot mix transparent with C/c, Rust/rust, or primitive types."
                        );
                    }
                    // Allow duplicate "transparent", although weird.
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
                        _ => unreachable!(), // Already matched by outer pattern
                    };
                    if is_transparent {
                        panic!(
                            "Conflicting repr kinds found in #[repr(...)]. Cannot mix primitive types and transparent."
                        );
                    }
                    if primitive_repr.is_some() {
                        panic!("Multiple primitive types specified in #[repr(...)].");
                    }
                    primitive_repr = Some(current_prim);
                }
                unknown => {
                    // Standard #[repr] only allows specific identifiers.
                    panic!(
                        "Unknown token '{unknown}' in #[repr(...)]. Only C, Rust, transparent, or primitive integer types allowed."
                    );
                }
            }
        }

        // Final construction
        if is_transparent {
            if repr_kind.is_some() || primitive_repr.is_some() {
                // This check should be redundant due to checks inside the loop, but added for safety.
                panic!("Internal error: transparent repr mixed with other kinds after parsing.");
            }
            Some(PRepr::Transparent)
        } else {
            // Default to Rust if only a primitive type is provided (e.g., #[repr(u8)]) or if nothing is specified.
            // If C/c or Rust/rust was specified, use that.
            let final_kind = repr_kind.unwrap_or(ReprKind::Rust);
            match final_kind {
                ReprKind::Rust => Some(PRepr::Rust(primitive_repr)),
                ReprKind::C => Some(PRepr::C(primitive_repr)),
            }
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
}

impl PAttrs {
    fn parse(attrs: &[crate::Attribute], display_name: &mut String) -> Self {
        let mut doc_lines: Vec<String> = Vec::new();
        let mut facet_attrs: Vec<PFacetAttr> = Vec::new();
        let mut repr: Option<PRepr> = None;
        let mut rename_all: Option<RenameRule> = None;

        for attr in attrs {
            match &attr.body.content {
                crate::AttributeInner::Doc(doc_attr) => {
                    let unescaped_text =
                        unescape(doc_attr).expect("invalid escape sequence in doc string");
                    doc_lines.push(unescaped_text);
                }
                crate::AttributeInner::Repr(repr_attr) => {
                    if repr.is_some() {
                        panic!("Multiple #[repr] attributes found");
                    }

                    repr = match PRepr::parse(repr_attr) {
                        Some(parsed) => Some(parsed),
                        None => {
                            panic!(
                                "Unknown #[repr] attribute: {}",
                                repr_attr.tokens_to_string()
                            );
                        }
                    };
                }
                crate::AttributeInner::Facet(facet_attr) => {
                    PFacetAttr::parse(facet_attr, &mut facet_attrs);
                }
                _ => {
                    // Ignore unknown AttributeInner types
                }
            }
        }

        // Extract rename and rename_all from parsed attrs
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
                        if let Some(rule) = RenameRule::from_str(rule_str) {
                            rename_all = Some(rule);
                        } else {
                            panic!("Unknown #[facet(rename_all = ...)] rule: {rule_str}");
                        }
                    }
                    _ => {}
                }
            }
        }

        Self {
            doc: doc_lines,
            facet: facet_attrs,
            repr: repr.unwrap_or(PRepr::Rust(None)),
            rename_all,
        }
    }

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

        // Parse container-level attributes
        let attrs = PAttrs::parse(&e.attributes, &mut container_display_name);

        // Get the container-level rename_all rule
        let container_rename_all_rule = attrs.rename_all;

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

        // Get the repr attribute if present, or default to Rust(None)
        let mut repr = None;
        for attr in &e.attributes {
            if let crate::AttributeInner::Repr(repr_attr) = &attr.body.content {
                // Parse repr attribute, will panic if invalid, just like struct repr parser
                repr = match PRepr::parse(repr_attr) {
                    Some(parsed) => Some(parsed),
                    None => panic!(
                        "Unknown #[repr] attribute: {}",
                        repr_attr.tokens_to_string()
                    ),
                };
                break; // Only use the first #[repr] attribute
            }
        }
        // Default to Rust(None) if not present, to match previous behavior, but enums will typically require repr(C) or a primitive in process_enum
        let repr = repr.unwrap_or(PRepr::Rust(None));

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
    pub(crate) fn from_struct_field(
        f: &crate::StructField,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use crate::ToTokens;
        Self::parse(
            &f.attributes,
            IdentOrLiteral::Ident(f.name.clone()),
            f.typ.to_token_stream(),
            rename_all_rule,
        )
    }

    /// Parse a tuple (unnamed) field for tuple structs or enum tuple variants.
    /// The index is converted to an identifier like `_0`, `_1`, etc.
    pub(crate) fn from_enum_field(
        attrs: &[crate::Attribute],
        idx: usize,
        typ: &crate::VerbatimUntil<crate::Comma>,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use crate::ToTokens;
        // Create an Ident from the index, using `_` prefix convention for tuple fields
        let ty = typ.to_token_stream(); // Convert to TokenStream
        Self::parse(attrs, IdentOrLiteral::Literal(idx), ty, rename_all_rule)
    }

    /// Central parse function used by both `from_struct_field` and `from_enum_field`.
    fn parse(
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
        // Create a mutable string to pass to PAttrs::parse.
        // While #[facet(rename = "...")] isn't typically used directly on the struct
        // definition itself in the same way as fields, the parse function expects
        // a mutable string to potentially modify if such an attribute is found.
        // We initialize it with the struct's name, although its value isn't
        // directly used for the container's name after parsing attributes.
        let mut container_display_name = s.name.to_string();

        // Parse top-level (container) attributes for the struct.
        let attrs = PAttrs::parse(&s.attributes, &mut container_display_name);

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
