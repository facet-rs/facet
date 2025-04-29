use crate::{BoundedGenericParams, RenameRule};
use facet_derive_parse::{Ident, ReprInner, ToTokens, TokenStream};
use quote::quote;

/// For struct fields, they can either be identifiers (`my_struct.foo`)
/// or literals (`my_struct.2`) — for tuple structs.
#[derive(Clone)]
pub enum IdentOrLiteral {
    Ident(Ident),
    Literal(usize),
}

impl quote::ToTokens for IdentOrLiteral {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            IdentOrLiteral::Ident(ident) => tokens.extend(quote::quote! { #ident }),
            IdentOrLiteral::Literal(lit) => {
                let unsuffixed = facet_derive_parse::Literal::usize_unsuffixed(*lit);
                tokens.extend(quote! { #unsuffixed })
            }
        }
    }
}

/// All the supported facet attributes, e.g. `#[facet(sensitive)]` `#[facet(rename_all)]`, etc.
///
/// Stands for `parsed facet attr`
#[derive(Clone)]
pub enum PFacetAttr {
    /// Valid in field
    /// `#[facet(sensitive)]` — must be censored in debug outputs
    Sensitive,

    /// Valid in container
    /// `#[facet(opaque)]` — the inner field does not have to implement
    /// `Facet`
    Opaque,

    /// Valid in container
    /// `#[facet(transparent)]` — applied on things like `NonZero<T>`, `Utf8PathBuf`,
    /// etc. — when you're doing the newtype pattern. `de/ser` is forwarded.
    Transparent,

    /// Valid in container
    /// `#[facet(invariants = "Self::invariants_func")]` — returns a bool, is called
    /// when doing `Wip::build`
    Invariants { expr: TokenStream },

    /// Valid in container
    /// `#[facet(deny_unknown_fields)]`
    DenyUnknownFields,

    /// Valid in field
    /// `#[facet(default = expr)]` — when deserializing and missing, use `fn_name` to provide a default value
    DefaultEquals { expr: TokenStream },

    /// Valid in field
    /// `#[facet(default)]` — when deserializing and missing, use the field's value from
    /// the container's `Default::default()`
    Default,

    /// Valid in field, enum variant, container
    /// An arbitrary/unknown string, like,
    /// `#[facet(bleh)]`
    Arbitrary { content: String },

    /// Valid in container
    /// `#[facet(rename_all = "rule")]` — rename all fields following a rule
    RenameAll { rule: RenameRule },

    /// Valid in field, enum variant, or container
    /// `#[facet(skip_serializing)]` — skip serializing this field. Like serde.
    SkipSerializing,

    /// Valid in field, enum variant, or container
    /// `#[facet(skip_serializing_if = "func")]` — skip serializing if the function returns true.
    SkipSerializingIf { expr: TokenStream },
}

impl PFacetAttr {
    /// Parse a `FacetAttr` attribute into a `PFacetAttr`.
    /// Pushes to `dest` for each parsed attribute.
    pub fn parse(
        facet_attr: &facet_derive_parse::FacetAttr,
        display_name: &mut String,
        dest: &mut Vec<PFacetAttr>,
    ) {
        use facet_derive_parse::FacetInner;

        for attr in facet_attr.inner.content.0.iter().map(|d| &d.value) {
            match attr {
                FacetInner::Sensitive(_) => dest.push(PFacetAttr::Sensitive),
                FacetInner::Opaque(_) => dest.push(PFacetAttr::Opaque),
                FacetInner::Transparent(_) => dest.push(PFacetAttr::Transparent),
                FacetInner::Invariants(invariant) => {
                    let expr = invariant.expr.to_token_stream();
                    dest.push(PFacetAttr::Invariants { expr });
                }
                FacetInner::DenyUnknownFields(_) => dest.push(PFacetAttr::DenyUnknownFields),
                FacetInner::DefaultEquals(default_equals) => dest.push(PFacetAttr::DefaultEquals {
                    expr: default_equals.expr.to_token_stream(),
                }),
                FacetInner::Default(_) => dest.push(PFacetAttr::Default),
                FacetInner::Rename(rename) => {
                    *display_name = rename.value.as_str().to_string();
                }
                FacetInner::RenameAll(rename_all) => {
                    let rule_str = rename_all.value.as_str();
                    if let Some(rule) = RenameRule::from_str(rule_str) {
                        dest.push(PFacetAttr::RenameAll { rule });
                    } else {
                        panic!("Unknown #[facet(rename_all = ...)] rule: {}", rule_str);
                    }
                }
                FacetInner::Arbitrary(tt) => {
                    dest.push(PFacetAttr::Arbitrary {
                        content: tt.tokens_to_string(),
                    });
                }
                FacetInner::SkipSerializing(_) => {
                    dest.push(PFacetAttr::SkipSerializing);
                }
                FacetInner::SkipSerializingIf(skip_if) => {
                    dest.push(PFacetAttr::SkipSerializingIf {
                        expr: skip_if.expr.to_token_stream(),
                    });
                }
            }
        }
    }
}

/// Parsed attr
pub enum PAttr {
    /// A single line of doc comments
    /// `#[doc = "Some doc"], or `/// Some doc`, same thing
    Doc { line: String },

    /// A representation attribute
    Repr { repr: PRepr },

    /// A facet attribute
    Facet { name: String },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PRepr {
    Transparent,
    Rust(Option<PrimitiveRepr>),
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

        let items = s.attr.content.0.as_slice();
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
                        "Unknown token '{}' in #[repr(...)]. Only C, Rust, transparent, or primitive integer types allowed.",
                        unknown
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
    pub doc: Vec<TokenStream>,

    /// Facet attributes specifically
    pub facet: Vec<PFacetAttr>,

    /// Representation of the facet
    pub repr: PRepr,

    /// rename_all rule (if any)
    pub rename_all: Option<RenameRule>,
}

impl PAttrs {
    fn parse(attrs: &[facet_derive_parse::Attribute], display_name: &mut String) -> Self {
        let mut doc_lines: Vec<TokenStream> = Vec::new();
        let mut facet_attrs: Vec<PFacetAttr> = Vec::new();
        let mut repr: Option<PRepr> = None;
        let mut rename_all: Option<RenameRule> = None;

        for attr in attrs {
            match &attr.body.content {
                facet_derive_parse::AttributeInner::Doc(doc_attr) => {
                    // Handle doc comments
                    doc_lines.push(doc_attr.value.to_token_stream());
                }
                facet_derive_parse::AttributeInner::Repr(repr_attr) => {
                    if repr.is_some() {
                        panic!("Multiple #[repr] attributes found");
                    }

                    // Parse repr attribute, e.g. #[repr(C)], #[repr(transparent)], #[repr(u8)]
                    // repr_attr.attr.content is a Vec<Delimited<Ident, Operator<','>>>
                    // which represents something like ["C"], or ["u8"], or ["transparent"]
                    //
                    // We should parse each possible repr kind. But usually there's only one item.
                    //
                    // We'll take the first one and parse it, ignoring the rest.
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
                facet_derive_parse::AttributeInner::Facet(facet_attr) => {
                    PFacetAttr::parse(facet_attr, display_name, &mut facet_attrs);
                }
                _ => {
                    // Ignore unknown AttributeInner types
                }
            }
        }

        for attr in &facet_attrs {
            if let PFacetAttr::RenameAll { rule } = attr {
                rename_all = Some(*rule);
            }
        }

        Self {
            doc: doc_lines,
            facet: facet_attrs,
            repr: repr.unwrap_or(PRepr::Rust(None)),
            rename_all,
        }
    }

    pub(crate) fn is_transparent(&self) -> bool {
        self.facet
            .iter()
            .any(|attr| matches!(attr, PFacetAttr::Transparent))
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
    /// Parse a `facet_derive_parse::Enum` into a `PEnum`.
    pub fn parse(e: &facet_derive_parse::Enum) -> Self {
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
            .0
            .iter()
            .map(|delim| PVariant::parse(&delim.value, container_rename_all_rule))
            .collect();

        // Get the repr attribute if present, or default to Rust(None)
        let mut repr = None;
        for attr in &e.attributes {
            if let facet_derive_parse::AttributeInner::Repr(repr_attr) = &attr.body.content {
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
        f: &facet_derive_parse::StructField,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use facet_derive_parse::ToTokens;
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
        attrs: &[facet_derive_parse::Attribute],
        idx: usize,
        typ: &facet_derive_parse::VerbatimUntil<facet_derive_parse::Comma>,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use facet_derive_parse::ToTokens;
        // Create an Ident from the index, using `_` prefix convention for tuple fields
        let ty = typ.to_token_stream(); // Convert to TokenStream
        Self::parse(attrs, IdentOrLiteral::Literal(idx), ty, rename_all_rule)
    }

    /// Central parse function used by both `from_struct_field` and `from_enum_field`.
    fn parse(
        attrs: &[facet_derive_parse::Attribute],
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
    Struct { fields: Vec<PStructField> },
    /// A tuple struct.
    TupleStruct { fields: Vec<PStructField> },
    /// A unit struct.
    UnitStruct,
}

impl PStructKind {
    /// Parse a `facet_derive_parse::StructKind` into a `PStructKind`.
    /// Passes rename_all_rule through to all PStructField parsing.
    pub fn parse(
        kind: &facet_derive_parse::StructKind,
        rename_all_rule: Option<RenameRule>,
    ) -> Self {
        match kind {
            facet_derive_parse::StructKind::Struct { clauses: _, fields } => {
                let parsed_fields = fields
                    .content
                    .0
                    .iter()
                    .map(|delim| PStructField::from_struct_field(&delim.value, rename_all_rule))
                    .collect();
                PStructKind::Struct {
                    fields: parsed_fields,
                }
            }
            facet_derive_parse::StructKind::TupleStruct {
                fields,
                clauses: _,
                semi: _,
            } => {
                let parsed_fields = fields
                    .content
                    .0
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
            facet_derive_parse::StructKind::UnitStruct {
                clauses: _,
                semi: _,
            } => PStructKind::UnitStruct,
        }
    }
}

impl PStruct {
    pub fn parse(s: &facet_derive_parse::Struct) -> Self {
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
    Tuple { fields: Vec<PStructField> },
    /// Struct variant, e.g., `Variant { field1: u32, field2: String }`.
    Struct { fields: Vec<PStructField> },
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
    pub discriminant: Option<facet_derive_parse::Literal>,
}

impl PVariant {
    /// Parses an `EnumVariantLike` from `facet_derive_parse` into a `PVariant`.
    ///
    /// Requires the container-level `rename_all` rule to correctly determine the
    /// effective name of the variant itself. The variant's own `rename_all` rule
    /// (if present) will be stored in `attrs.rename_all` and used for its fields.
    fn parse(
        var_like: &facet_derive_parse::EnumVariantLike,
        container_rename_all_rule: Option<RenameRule>,
    ) -> Self {
        use facet_derive_parse::{EnumVariantData, StructVariant, TupleVariant, UnitVariant};

        let (raw_name_ident, attributes) = match &var_like.variant {
            // Fix: Changed var_like.value.variant to var_like.variant
            EnumVariantData::Unit(UnitVariant { name, attributes })
            | EnumVariantData::Tuple(TupleVariant {
                name, attributes, ..
            })
            | EnumVariantData::Struct(StructVariant {
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
                    .0
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
            EnumVariantData::Struct(StructVariant { fields, .. }) => {
                let parsed_fields = fields
                    .content
                    .0
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
        let discriminant = var_like.discriminant.as_ref().map(|d| d.second.clone());

        PVariant {
            name,
            attrs,
            kind,
            discriminant,
        }
    }
}
