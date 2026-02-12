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
    KStr = "str";
    KBuiltin = "builtin";
    KMakeT = "make_t";
    KOr = "or";
    KDefault = "default";
    KTy = "ty";
    KPredicate = "predicate";
    KValidator = "validator";
    KFnPtr = "fn_ptr";
    KShapeType = "shape_type";
    KArbitrary = "arbitrary";
    KPassthrough = "passthrough";
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
    /// Dollar sign $
    Dollar = "$";
}

unsynn! {
    /// The complete grammar definition
    ///
    /// Format:
    /// ```ignore
    /// ns "xml";
    /// crate_path ::facet_xml;
    /// pub enum Attr { ... }
    /// pub struct Column { ... }
    /// ```
    ///
    /// For built-in attrs defined inside the facet crate itself:
    /// ```ignore
    /// builtin;
    /// ns "";
    /// crate_path ::facet::builtin;
    /// pub enum Attr { ... }
    /// ```
    struct Grammar {
        builtin_decl: Option<BuiltinDecl>,
        ns_decl: Option<NsDecl>,
        crate_path_decl: Option<CratePathDecl>,
        items: Vec<GrammarItem>,
    }

    /// Builtin declaration: `builtin;`
    /// Indicates this grammar is defined inside the facet crate itself.
    /// This changes code generation to use `crate::` for definition-time
    /// references instead of `::facet::`.
    struct BuiltinDecl {
        _kw: KBuiltin,
        _semi: Semicolon,
    }

    /// Namespace declaration: `ns "xml";`
    struct NsDecl {
        _kw: KNs,
        ns_literal: Literal,
        _semi: Semicolon,
    }

    /// Crate path declaration: `crate_path ::facet_xml;`
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

    /// What's inside the variant parens - captures all tokens for analysis
    struct VariantPayload {
        /// All tokens in the payload - we analyze these in to_parsed()
        tokens: Vec<TokenTree>,
    }

    /// A static str reference: `&'static str`
    struct StaticStrRef {
        _amp: Amp,
        _apos: Apos,
        _static: KStatic,
        _str: KStr,
    }

    /// A static reference to some other type: `&'static SomeType`
    struct StaticRef {
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

    /// Option<&'static str> type
    struct OptionStaticStrType {
        _option: KOption,
        _lt: Lt,
        _inner: StaticStrRef,
        _gt: Gt,
    }

    /// `predicate TypeName` payload
    struct PredicatePayload {
        _kw: KPredicate,
        type_name: Ident,
    }

    /// `validator TypeName` payload
    struct ValidatorPayload {
        _kw: KValidator,
        type_name: Ident,
    }

    /// `fn_ptr TypeName` payload
    struct FnPtrPayload {
        _kw: KFnPtr,
        type_name: Ident,
    }

    /// `shape_type` payload (just the keyword)
    struct ShapeTypePayload {
        _kw: KShapeType,
    }

    /// `arbitrary` payload (just the keyword)
    struct ArbitraryPayload {
        _kw: KArbitrary,
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

    /// make_t payload: `make_t` or `make_t or $ty::default()`
    struct MakeTPayload {
        _kw: KMakeT,
        fallback: Option<MakeTFallback>,
    }

    /// The `or $ty::default()` part of make_t
    struct MakeTFallback {
        _or: KOr,
        _dollar: Dollar,
        _ty: KTy,
        _sep: PathSep,
        _default: KDefault,
        _parens: ParenthesisGroup,
    }
}

// ============================================================================
// PARSED STRUCTURES FOR CODE GENERATION
// ============================================================================

/// The parsed grammar for code generation
struct ParsedGrammar {
    /// Whether this grammar is for built-in attrs defined inside the facet crate.
    /// When true, definition-time code uses `crate::` instead of `::facet::`.
    builtin: bool,
    /// Namespace string (e.g., "xml"), or empty string for built-in attrs
    ns: Option<String>,
    /// Crate path tokens (e.g., `::facet_xml`), required for non-unit variants
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

/// Where an attribute can be used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AttrTarget {
    /// Valid on both fields and containers (default)
    #[default]
    Both,
    /// Only valid on fields (has access to $ty)
    Field,
    /// Only valid on containers
    Container,
}

/// How an attribute should be stored at runtime.
///
/// By default, attributes go into the `attributes` slice which requires O(n) lookup.
/// With `#[storage(...)]` annotations, attributes can be stored in dedicated fields
/// for O(1) access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Storage {
    /// Store in the attributes slice (default, O(n) lookup)
    #[default]
    Attr,
    /// Store as a flag bit in `FieldFlags` (O(1) lookup, for Unit variants only)
    Flag,
    /// Store in a dedicated field on `Field` struct (O(1) lookup)
    /// The field name is derived from the variant name (snake_case)
    Field,
}

struct ParsedVariant {
    attrs: Vec<TokenStream2>,
    name: proc_macro2::Ident,
    kind: VariantKind,
    /// Where this attribute can be used
    target: AttrTarget,
    /// How this attribute should be stored
    /// Note: Currently used for documentation/validation only. The actual
    /// storage routing is hardcoded in process_struct.rs for builtin attrs.
    #[allow(dead_code)]
    storage: Storage,
}

enum VariantKind {
    Unit,
    Newtype(TokenStream2),
    /// Newtype holding `&'static str` - stored directly for facet-core access.
    /// Used for attributes like `tag`, `content`, `rename`, `rename_all`, `alias`.
    NewtypeStr,
    NewtypeOptionChar,
    /// Newtype holding `i64` - for numeric validation attributes like `min`, `max`.
    NewtypeI64,
    /// Newtype holding `usize` - for length validation attributes like `min_length`, `max_length`.
    NewtypeUsize,
    Struct(proc_macro2::Ident),
    /// Arbitrary type like `Option<DefaultInPlaceFn>` - the tokens are passed through as-is
    ArbitraryType(TokenStream2),
    /// Expression that "makes a T" - wrapped in `|ptr| unsafe { ptr.put(#expr) }`.
    /// Generic mechanism for any attribute that needs to produce a value.
    /// Grammar syntax: `Default(make_t)` or `Default(make_t or $ty::default())`
    /// When fallback is true, bare usage generates `$ty::default()` where $ty is the field type.
    MakeT {
        /// If true, generate `<$ty as ::core::default::Default>::default()` when no value provided.
        /// This is used for field attributes where $ty refers to the field type.
        use_ty_default_fallback: bool,
    },
    /// Predicate function - user provides `fn(&T) -> bool`, wrapped in type-erased closure.
    /// The expression is wrapped in `|ptr| unsafe { expr(ptr.get::<T>()) }`.
    /// Grammar syntax: `SkipSerializingIf(predicate SkipSerializingIfFn)`
    Predicate(TokenStream2),
    /// Validator function - user provides `fn(&T) -> Result<(), String>`, wrapped in type-erased closure.
    /// The expression is wrapped in `|ptr| unsafe { expr(ptr.get::<T>()) }`.
    /// Grammar syntax: `Custom(validator ValidatorFn)`
    Validator(TokenStream2),
    /// Expression that is stored directly as a function pointer.
    /// Used for attributes where the expression is already the correct type-erased signature.
    /// Grammar syntax: `Foo(fn_ptr FooFn)`
    FnPtr(TokenStream2),
    /// Type that is converted to a Shape reference.
    /// The user's type is converted to `<Type as Facet>::SHAPE`.
    /// Grammar syntax: `Proxy(shape_type)`
    ShapeType,
    /// Arbitrary value - accepts any tokens without validation.
    /// Used for compile-time-only attributes where the value is read from raw tokens.
    /// Grammar syntax: `FromRef(arbitrary)`
    Arbitrary,
    /// Optional `&'static str` - can be used with or without a value.
    /// Grammar syntax: `Children(Option<&'static str>)`
    /// - `#[facet(xml::children)]` → None
    /// - `#[facet(xml::children = "kiddo")]` → Some("kiddo")
    OptionalStr,
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
        // Check for `builtin;` declaration
        let builtin = self.builtin_decl.is_some();

        // Extract namespace from `ns "xml";` declaration
        let ns = self.ns_decl.as_ref().map(|decl| {
            // Strip quotes from the literal
            let s = decl.ns_literal.to_string();
            s.trim_matches('"').to_string()
        });

        // Extract crate path from `crate_path ::facet_xml;` declaration
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
            builtin,
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
                let tokens = &paren_group.content.tokens;
                analyze_variant_payload(tokens)?
            }
        };

        // Parse #[target(field)] or #[target(container)] attribute
        let target = parse_target_attr(&self.attrs)?;

        // Parse #[storage(flag)] or #[storage(field)] attribute
        let storage = parse_storage_attr(&self.attrs)?;

        // Validate storage attribute compatibility with variant kind
        if storage == Storage::Flag && !matches!(kind, VariantKind::Unit) {
            return Err(format!(
                "#[storage(flag)] can only be used with unit variants, but {} has a payload",
                self.name
            ));
        }

        // Filter out the target and storage attributes from the attrs we pass through
        let attrs = self
            .attrs
            .iter()
            .filter(|a| !is_target_attr(a) && !is_storage_attr(a))
            .map(|a| {
                let content = a.content.0.stream();
                quote! { #[#content] }
            })
            .collect();

        Ok(ParsedVariant {
            attrs,
            name: convert_ident(&self.name),
            kind,
            target,
            storage,
        })
    }
}

/// Check if an outer attribute is #[target(...)]
fn is_target_attr(attr: &OuterAttr) -> bool {
    let content = attr.content.0.stream().to_string();
    content.starts_with("target")
}

/// Check if an outer attribute is #[storage(...)]
fn is_storage_attr(attr: &OuterAttr) -> bool {
    let content = attr.content.0.stream().to_string();
    content.starts_with("storage")
}

/// Parse the #[target(field)] or #[target(container)] attribute
fn parse_target_attr(attrs: &[OuterAttr]) -> std::result::Result<AttrTarget, String> {
    for attr in attrs {
        let content = attr.content.0.stream().to_string();
        if content.starts_with("target") {
            // Parse "target(field)" or "target(container)"
            if content.contains("field") {
                return Ok(AttrTarget::Field);
            } else if content.contains("container") {
                return Ok(AttrTarget::Container);
            } else {
                return Err(format!(
                    "invalid target attribute: expected #[target(field)] or #[target(container)], got #[{content}]"
                ));
            }
        }
    }
    Ok(AttrTarget::Both)
}

/// Parse the #[storage(flag)] or #[storage(field)] attribute
fn parse_storage_attr(attrs: &[OuterAttr]) -> std::result::Result<Storage, String> {
    for attr in attrs {
        let content = attr.content.0.stream().to_string();
        if content.starts_with("storage") {
            // Parse "storage(flag)" or "storage(field)"
            if content.contains("flag") {
                return Ok(Storage::Flag);
            } else if content.contains("field") {
                return Ok(Storage::Field);
            } else {
                return Err(format!(
                    "invalid storage attribute: expected #[storage(flag)] or #[storage(field)], got #[{content}]"
                ));
            }
        }
    }
    Ok(Storage::Attr)
}

/// Analyze variant payload tokens to determine the variant kind.
///
/// Uses unsynn grammar parsing to handle:
/// - `&'static str` → NewtypeStr
/// - `&'static SomeType` → Newtype
/// - `Option<char>` → NewtypeOptionChar
/// - `Option<&'static str>` → OptionalStr
/// - `make_t` or `make_t or $ty::default()` → MakeT
/// - `predicate TypeName` → Predicate
/// - `fn_ptr TypeName` → FnPtr
/// - `shape_type` → ShapeType
/// - Single identifier like `Column` → Struct reference
/// - Everything else → ArbitraryType
fn analyze_variant_payload(tokens: &[TokenTree]) -> std::result::Result<VariantKind, String> {
    let token_stream: TokenStream2 = tokens.iter().cloned().collect();

    // Try each grammar in order of specificity

    // &'static str → NewtypeStr (must come before StaticRef)
    {
        let mut iter = token_stream.clone().to_token_iter();
        if iter.parse::<StaticStrRef>().is_ok() && iter.next().is_none() {
            return Ok(VariantKind::NewtypeStr);
        }
    }

    // &'static SomeType → Newtype
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(parsed) = iter.parse::<StaticRef>()
            && iter.next().is_none()
        {
            let typ = convert_ident(&parsed.typ);
            return Ok(VariantKind::Newtype(quote! { &'static #typ }));
        }
    }

    // Option<&'static str> → OptionalStr (must come before OptionCharType)
    {
        let mut iter = token_stream.clone().to_token_iter();
        if iter.parse::<OptionStaticStrType>().is_ok() && iter.next().is_none() {
            return Ok(VariantKind::OptionalStr);
        }
    }

    // Option<char> → NewtypeOptionChar
    {
        let mut iter = token_stream.clone().to_token_iter();
        if iter.parse::<OptionCharType>().is_ok() && iter.next().is_none() {
            return Ok(VariantKind::NewtypeOptionChar);
        }
    }

    // make_t or make_t or $ty::default() → MakeT
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(make_t) = iter.parse::<MakeTPayload>()
            && iter.next().is_none()
        {
            let use_ty_default_fallback = make_t.fallback.is_some();
            return Ok(VariantKind::MakeT {
                use_ty_default_fallback,
            });
        }
    }

    // predicate TypeName → Predicate
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(parsed) = iter.parse::<PredicatePayload>()
            && iter.next().is_none()
        {
            let type_name = convert_ident(&parsed.type_name);
            return Ok(VariantKind::Predicate(quote! { #type_name }));
        }
    }

    // validator TypeName → Validator
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(parsed) = iter.parse::<ValidatorPayload>()
            && iter.next().is_none()
        {
            let type_name = convert_ident(&parsed.type_name);
            return Ok(VariantKind::Validator(quote! { #type_name }));
        }
    }

    // fn_ptr TypeName → FnPtr
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(parsed) = iter.parse::<FnPtrPayload>()
            && iter.next().is_none()
        {
            let type_name = convert_ident(&parsed.type_name);
            return Ok(VariantKind::FnPtr(quote! { #type_name }));
        }
    }

    // shape_type → ShapeType
    {
        let mut iter = token_stream.clone().to_token_iter();
        if iter.parse::<ShapeTypePayload>().is_ok() && iter.next().is_none() {
            return Ok(VariantKind::ShapeType);
        }
    }

    // arbitrary → Arbitrary
    {
        let mut iter = token_stream.clone().to_token_iter();
        if iter.parse::<ArbitraryPayload>().is_ok() && iter.next().is_none() {
            return Ok(VariantKind::Arbitrary);
        }
    }

    // i64 → NewtypeI64 (for numeric bounds like min, max)
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(ref ident) = iter.parse::<Ident>()
            && iter.next().is_none()
            && ident == "i64"
        {
            return Ok(VariantKind::NewtypeI64);
        }
    }

    // usize → NewtypeUsize (for length bounds like min_length, max_length)
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(ref ident) = iter.parse::<Ident>()
            && iter.next().is_none()
            && ident == "usize"
        {
            return Ok(VariantKind::NewtypeUsize);
        }
    }

    // Single PascalCase identifier → Struct reference
    // Only treat identifiers starting with uppercase as struct references.
    // Lowercase identifiers like `bool` fall through to ArbitraryType.
    {
        let mut iter = token_stream.clone().to_token_iter();
        if let Ok(ident) = iter.parse::<Ident>()
            && iter.next().is_none()
        {
            let ident_str = ident.to_string();
            // Check if it starts with uppercase (PascalCase = struct reference)
            if ident_str
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
            {
                return Ok(VariantKind::Struct(convert_ident(&ident)));
            }
        }
    }

    // Everything else is an arbitrary type - collect tokens directly
    let type_tokens: TokenStream2 = tokens.iter().cloned().collect();
    Ok(VariantKind::ArbitraryType(type_tokens))
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
        let struct_defs: Vec<_> = self
            .structs
            .iter()
            .map(|s| s.generate(self.builtin))
            .collect();

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
                let ParsedVariant {
                    attrs, name, kind, ..
                } = v;
                match kind {
                    VariantKind::Unit => quote! { #(#attrs)* #name },
                    VariantKind::Newtype(ty) => quote! { #(#attrs)* #name(#ty) },
                    VariantKind::NewtypeStr => quote! { #(#attrs)* #name(&'static str) },
                    VariantKind::NewtypeOptionChar => quote! { #(#attrs)* #name(Option<char>) },
                    VariantKind::NewtypeI64 => quote! { #(#attrs)* #name(i64) },
                    VariantKind::NewtypeUsize => quote! { #(#attrs)* #name(usize) },
                    VariantKind::Struct(struct_name) => {
                        quote! { #(#attrs)* #name(#struct_name) }
                    }
                    VariantKind::ArbitraryType(ty) => quote! { #(#attrs)* #name(#ty) },
                    VariantKind::MakeT { .. } => {
                        quote! { #(#attrs)* #name(Option<DefaultInPlaceFn>) }
                    }
                    VariantKind::Predicate(ty) => {
                        quote! { #(#attrs)* #name(Option<#ty>) }
                    }
                    VariantKind::Validator(ty) => {
                        quote! { #(#attrs)* #name(Option<#ty>) }
                    }
                    VariantKind::FnPtr(ty) => {
                        quote! { #(#attrs)* #name(Option<#ty>) }
                    }
                    VariantKind::ShapeType => {
                        // Generates a newtype variant holding a Shape reference
                        // Use crate:: path in builtin mode, ::facet:: otherwise
                        let shape_path = if self.builtin {
                            quote! { crate::Shape }
                        } else {
                            quote! { ::facet::Shape }
                        };
                        quote! { #(#attrs)* #name(&'static #shape_path) }
                    }
                    VariantKind::Arbitrary => {
                        // Arbitrary variants are compile-time only - they don't store runtime data.
                        // The value is read from raw tokens by the derive macro.
                        // We use a unit variant since nothing is stored.
                        quote! { #(#attrs)* #name }
                    }
                    VariantKind::OptionalStr => {
                        quote! { #(#attrs)* #name(Option<&'static str>) }
                    }
                }
            })
            .collect();

        // Check if any variant contains function pointers (can't implement Facet)
        let has_fn_ptr_variants = variants.iter().any(|v| {
            matches!(
                v.kind,
                VariantKind::Predicate(_)
                    | VariantKind::Validator(_)
                    | VariantKind::FnPtr(_)
                    | VariantKind::MakeT { .. }
            )
        });

        // For builtin mode, skip deriving Facet entirely because the derive macro
        // generates ::facet:: paths which don't work inside the facet crate.
        // Builtin attrs will have Facet implemented manually via a blanket impl.
        //
        // Also skip deriving Facet for grammars with function pointer variants,
        // since function pointers can't implement Facet.
        //
        // We don't derive PartialEq because some variants contain function pointers.
        // Instead, we generate a manual impl that uses fn_addr_eq for those variants.
        let derive_attr = if self.builtin || has_fn_ptr_variants {
            quote! { #[derive(Debug, Clone)] }
        } else {
            quote! { #[derive(Debug, Clone, ::facet::Facet)] }
        };

        // Generate manual PartialEq implementation
        let partial_eq_impl = self.generate_partial_eq_impl();

        quote! {
            #(#attrs)*
            #derive_attr
            #[repr(u8)]
            #vis_tokens enum #name {
                #(#variant_defs),*
            }

            #partial_eq_impl
        }
    }

    /// Generate a manual PartialEq implementation for the enum.
    ///
    /// Function pointer variants use `core::ptr::fn_addr_eq` for comparison,
    /// which is more explicit about the semantics (addresses may vary across
    /// codegen units, but within the same unit they should be stable).
    fn generate_partial_eq_impl(&self) -> TokenStream2 {
        let name = &self.attr_enum.name;

        let match_arms: Vec<_> = self
            .attr_enum
            .variants
            .iter()
            .map(|v| {
                let variant_name = &v.name;
                match &v.kind {
                    // Unit variants: simple equality
                    VariantKind::Unit => {
                        quote! {
                            (Self::#variant_name, Self::#variant_name) => true
                        }
                    }
                    // Simple value types: use regular equality
                    VariantKind::NewtypeStr | VariantKind::NewtypeOptionChar | VariantKind::OptionalStr => {
                        quote! {
                            (Self::#variant_name(a), Self::#variant_name(b)) => a == b
                        }
                    }
                    // Newtype with non-fn-ptr type: use regular equality
                    VariantKind::Newtype(_) => {
                        quote! {
                            (Self::#variant_name(a), Self::#variant_name(b)) => a == b
                        }
                    }
                    // Struct variants: use regular equality (structs derive PartialEq)
                    VariantKind::Struct(_) => {
                        quote! {
                            (Self::#variant_name(a), Self::#variant_name(b)) => a == b
                        }
                    }
                    // ArbitraryType: assume it has PartialEq
                    VariantKind::ArbitraryType(_) => {
                        quote! {
                            (Self::#variant_name(a), Self::#variant_name(b)) => a == b
                        }
                    }
                    // NewtypeI64 and NewtypeUsize: simple numeric equality
                    VariantKind::NewtypeI64 | VariantKind::NewtypeUsize => {
                        quote! {
                            (Self::#variant_name(a), Self::#variant_name(b)) => a == b
                        }
                    }
                    // ShapeType: comparing &'static Shape by pointer equality is fine
                    VariantKind::ShapeType => {
                        quote! {
                            (Self::#variant_name(a), Self::#variant_name(b)) => ::core::ptr::eq(*a, *b)
                        }
                    }
                    // Arbitrary: unit variant, simple equality
                    VariantKind::Arbitrary => {
                        quote! {
                            (Self::#variant_name, Self::#variant_name) => true
                        }
                    }
                    // Function pointer variants: always return false
                    // Function pointer comparison is unreliable across codegen units,
                    // so we don't even try - two function pointers are never considered equal.
                    VariantKind::MakeT { .. } | VariantKind::Predicate(_) | VariantKind::Validator(_) | VariantKind::FnPtr(_) => {
                        quote! {
                            (Self::#variant_name(_), Self::#variant_name(_)) => false
                        }
                    }
                }
            })
            .collect();

        quote! {
            impl ::core::cmp::PartialEq for #name {
                fn eq(&self, other: &Self) -> bool {
                    match (self, other) {
                        #(#match_arms,)*
                        // Different variants are never equal
                        _ => false,
                    }
                }
            }
        }
    }

    fn generate_reexports(&self) -> TokenStream2 {
        // Definition-time: use crate:: when builtin, ::facet:: otherwise
        let facet_path: TokenStream2 = if self.builtin {
            quote! { crate }
        } else {
            quote! { ::facet }
        };

        quote! {
            #[doc(hidden)]
            pub use #facet_path::__attr_error as __attr_error_proc_macro;
            #[doc(hidden)]
            pub use #facet_path::__build_struct_fields;
            #[doc(hidden)]
            pub use #facet_path::__dispatch_attr;
            #[doc(hidden)]
            pub use #facet_path::__field_error as __field_error_proc_macro;
            #[doc(hidden)]
            pub use #facet_path::__spanned_error;
        }
    }

    fn generate_attr_macro(&self) -> TokenStream2 {
        let enum_name = &self.attr_enum.name;
        let ns_str = self.ns.as_deref().unwrap_or("");
        // Generate the namespace expression: None for builtins, Some("ns") for namespaced
        let ns_expr = if ns_str.is_empty() {
            quote! { 𝟋None }
        } else {
            quote! { 𝟋Some(#ns_str) }
        };

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
                    VariantKind::NewtypeStr => quote! { #name: newtype_str },
                    VariantKind::NewtypeOptionChar => quote! { #name: newtype_opt_char },
                    VariantKind::NewtypeI64 => quote! { #name: newtype_i64 },
                    VariantKind::NewtypeUsize => quote! { #name: newtype_usize },
                    VariantKind::ArbitraryType(_) => quote! { #name: arbitrary },
                    VariantKind::MakeT { .. } => quote! { #name: make_t },
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
                    VariantKind::Predicate(_) => quote! { #name: predicate },
                    VariantKind::Validator(_) => quote! { #name: validator },
                    VariantKind::FnPtr(_) => quote! { #name: fn_ptr },
                    VariantKind::ShapeType => quote! { #name: shape_type },
                    VariantKind::Arbitrary => quote! { #name: arbitrary },
                    VariantKind::OptionalStr => quote! { #name: opt_str },
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
                        // Pre-compute the full attribute name for error messages
                        let full_attr_name = if ns_str.is_empty() {
                            key_str.clone()
                        } else {
                            format!("{ns_str}::{key_str}")
                        };
                        quote! {
                            // Field-level: @ns { path } attr { field : Type }
                            // Note: $field is tt not ident because tuple struct fields are literals (0, 1, etc.)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                static __UNIT: () = ();
                                ::facet::Attr::new(#ns_expr, #key_str, &__UNIT)
                            }};
                            // Field-level with args: not expected for unit variants
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $first:tt $($rest:tt)* }) => {{
                                ::facet::__no_args!(#full_attr_name, $first)
                            }};
                            // Container-level: @ns { path } attr { }
                            (@ns { $ns:path } #key_ident { }) => {{
                                static __UNIT: () = ();
                                ::facet::Attr::new(#ns_expr, #key_str, &__UNIT)
                            }};
                            // Container-level with args: not expected for unit variants
                            (@ns { $ns:path } #key_ident { | $first:tt $($rest:tt)* }) => {{
                                ::facet::__no_args!(#full_attr_name, $first)
                            }};
                        }
                    }
                    VariantKind::ShapeType => {
                        // ShapeType variants store just &'static Shape, not wrapped in Attr enum.
                        // This allows efficient runtime access via proxy_shape() method.
                        // We use new_shape() which bypasses the T: Facet bound since Shape
                        // doesn't implement Facet.
                        let crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for shape_type variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        quote! {
                            // Field-level: @ns { path } attr { field : Type } - no args is an error for shape_type
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!("`", stringify!(#key_ident), "` requires a type argument"))
                            }};
                            // Field-level with args: parse type and store just the Shape
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $($args:tt)* }) => {{
                                ::facet::Attr::new_shape(
                                    #ns_expr,
                                    #key_str,
                                    ::facet::__dispatch_attr!{
                                        @crate_path { #crate_path }
                                        @enum_name { #enum_name }
                                        @variants { #(#variants_meta),* }
                                        @name { #key_ident }
                                        @rest { $($args)* }
                                    }
                                )
                            }};
                            // Container-level: @ns { path } attr { } - no args is an error for shape_type
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!("`", stringify!(#key_ident), "` requires a type argument"))
                            }};
                            // Container-level with args: parse type and store just the Shape
                            (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                ::facet::Attr::new_shape(
                                    #ns_expr,
                                    #key_str,
                                    ::facet::__dispatch_attr!{
                                        @crate_path { #crate_path }
                                        @enum_name { #enum_name }
                                        @variants { #(#variants_meta),* }
                                        @name { #key_ident }
                                        @rest { $($args)* }
                                    }
                                )
                            }};
                        }
                    }
                    VariantKind::Predicate(target_ty) => {
                        // For predicate variants, we generate a wrapper function in the attribute macro
                        // because we need access to $ty which __dispatch_attr doesn't have.
                        //
                        // IMPORTANT: We store the function pointer directly, NOT wrapped in an Attr enum.
                        // This is because the retrieval code (e.g., skip_serializing_if_fn()) uses data_ref
                        // to read the raw function pointer.
                        //
                        // We generate a wrapper function instead of transmuting directly so that
                        // auto-deref works at the call site. This allows users to write:
                        //   fn is_empty(s: &str) -> bool { ... }
                        // for a String field, instead of requiring the exact type:
                        //   fn is_empty(s: &String) -> bool { ... }
                        let _crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for predicate variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        // Qualify the target type - use ::facet:: since these types are re-exported there
                        let qualified_target_ty = quote! { ::facet::#target_ty };
                        quote! {
                            // Field-level with args: wrap the user's predicate in a function that
                            // enables auto-deref at the call site.
                            // Store the function pointer directly (not wrapped in Attr enum)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $($args:tt)* }) => {{
                                ::facet::Attr {
                                    ns: #ns_expr,
                                    key: #key_str,
                                    // SAFETY: Static const block pointer is valid for 'static
                                    data: unsafe { ::facet::OxRef::new(
                                        ::facet::PtrConst::new_sized(&const {
                                            // Define a wrapper function that calls the user's predicate.
                                            // The call site `predicate(ptr.get::<$ty>())` enables auto-deref,
                                            // so `fn(&str) -> bool` works for a `String` field.
                                            unsafe fn __predicate_wrapper(ptr: ::facet::PtrConst) -> bool {
                                                let predicate = ($($args)*);
                                                predicate(ptr.get::<$ty>())
                                            }
                                            // Coerce function item to function pointer
                                            __predicate_wrapper as #qualified_target_ty
                                        } as *const #qualified_target_ty as *const ()),
                                        <() as ::facet::Facet>::SHAPE
                                    ) },
                                }
                            }};
                            // Field-level: @ns { path } attr { field : Type } - no args is an error for predicate
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a function argument: `",
                                    stringify!(#key_ident),
                                    " = your_fn`"
                                ))
                            }};
                            // Container-level: not supported for predicate (no $ty available)
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!(
                                    "Container-level predicate attributes like `",
                                    stringify!(#key_ident),
                                    "` are not supported"
                                ))
                            }};
                            (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                compile_error!(concat!(
                                    "Container-level predicate attributes like `",
                                    stringify!(#key_ident),
                                    "` are not supported"
                                ))
                            }};
                        }
                    }
                    VariantKind::Validator(target_ty) => {
                        // For validator variants, we generate a wrapper function in the attribute macro
                        // because we need access to $ty which __dispatch_attr doesn't have.
                        //
                        // Similar to predicate but returns Result<(), String> instead of bool.
                        // This allows validators to provide meaningful error messages.
                        //
                        // We generate a wrapper function instead of transmuting directly so that
                        // auto-deref works at the call site. This allows users to write:
                        //   fn validate_email(s: &str) -> Result<(), String> { ... }
                        // for a String field, instead of requiring the exact type:
                        //   fn validate_email(s: &String) -> Result<(), String> { ... }
                        let _crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for validator variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        // Qualify the target type - use ::facet:: since these types are re-exported there
                        let qualified_target_ty = quote! { ::facet::#target_ty };
                        quote! {
                            // Field-level with args: wrap the user's validator in a function that
                            // enables auto-deref at the call site.
                            // Store the function pointer directly (not wrapped in Attr enum)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $($args:tt)* }) => {{
                                ::facet::Attr {
                                    ns: #ns_expr,
                                    key: #key_str,
                                    // SAFETY: Static const block pointer is valid for 'static
                                    data: unsafe { ::facet::OxRef::new(
                                        ::facet::PtrConst::new_sized(&const {
                                            // Define a wrapper function that calls the user's validator.
                                            // The call site `validator(ptr.get::<$ty>())` enables auto-deref,
                                            // so `fn(&str) -> Result<(), String>` works for a `String` field.
                                            unsafe fn __validator_wrapper(ptr: ::facet::PtrConst) -> ::core::result::Result<(), ::std::string::String> {
                                                let validator = ($($args)*);
                                                validator(ptr.get::<$ty>())
                                            }
                                            // Coerce function item to function pointer
                                            __validator_wrapper as #qualified_target_ty
                                        } as *const #qualified_target_ty as *const ()),
                                        <() as ::facet::Facet>::SHAPE
                                    ) },
                                }
                            }};
                            // Field-level: @ns { path } attr { field : Type } - no args is an error for validator
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a function argument: `",
                                    stringify!(#key_ident),
                                    " = your_fn`"
                                ))
                            }};
                            // Container-level: not supported for validator (no $ty available)
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!(
                                    "Container-level validator attributes like `",
                                    stringify!(#key_ident),
                                    "` are not supported"
                                ))
                            }};
                            (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                compile_error!(concat!(
                                    "Container-level validator attributes like `",
                                    stringify!(#key_ident),
                                    "` are not supported"
                                ))
                            }};
                        }
                    }
                    VariantKind::MakeT { use_ty_default_fallback } => {
                        // MakeT variants store Option<DefaultInPlaceFn> directly (not wrapped in Attr).
                        // This is necessary because facet-core needs to access default_fn() but can't
                        // import the Attr enum (dependency direction: facet depends on facet-core).
                        //
                        // - `default` (no args) → use $ty::default() if fallback enabled, otherwise None
                        // - `default = expr` → Some(|ptr| ptr.put(expr))
                        let _crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for make_t variants; add `crate_path ::your_crate;` to the grammar"
                        );

                        let target = v.target;

                        // Generate field-level arms (only if target allows fields)
                        let field_arms = if target != AttrTarget::Container {
                            // For field-level no-args: use $ty::default() if fallback enabled, otherwise None
                            let no_args_body = if *use_ty_default_fallback {
                                // Use <$ty as Default>::default() - $ty is already a metavariable
                                // in the generated macro, so it will reference the field type
                                quote! {
                                    ::facet::Attr {
                                        ns: #ns_expr,
                                        key: #key_str,
                                        // SAFETY: Static const block pointer is valid for 'static
                                        data: unsafe { ::facet::OxRef::new(
                                            ::facet::PtrConst::new_sized(&const {
                                                𝟋Some(
                                                    (|__ptr: ::facet::PtrUninit| unsafe {
                                                        __ptr.put(<$ty as ::core::default::Default>::default())
                                                    }) as ::facet::DefaultInPlaceFn
                                                )
                                            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const ()),
                                            <() as ::facet::Facet>::SHAPE
                                        ) },
                                    }
                                }
                            } else {
                                // No fallback - use None (runtime Default trait lookup)
                                quote! {
                                    ::facet::Attr {
                                        ns: #ns_expr,
                                        key: #key_str,
                                        // SAFETY: Static const block pointer is valid for 'static
                                        data: unsafe { ::facet::OxRef::new(
                                            ::facet::PtrConst::new_sized(&const {
                                                ::core::option::Option::<::facet::DefaultInPlaceFn>::None
                                            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const ()),
                                            <() as ::facet::Facet>::SHAPE
                                        ) },
                                    }
                                }
                            };

                            quote! {
                                // Field-level: no args
                                (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                    #no_args_body
                                }};
                                // Field-level with `= expr`: wrap in closure
                                (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $expr:expr }) => {{
                                    ::facet::Attr {
                                        ns: #ns_expr,
                                        key: #key_str,
                                        // SAFETY: Static const block pointer is valid for 'static
                                        data: unsafe { ::facet::OxRef::new(
                                            ::facet::PtrConst::new_sized(&const {
                                                𝟋Some(
                                                    (|__ptr: ::facet::PtrUninit| unsafe { __ptr.put($expr) })
                                                        as ::facet::DefaultInPlaceFn
                                                )
                                            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const ()),
                                            <() as ::facet::Facet>::SHAPE
                                        ) },
                                    }
                                }};
                                // Field-level with just expr (no =): also wrap in closure
                                (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $expr:expr }) => {{
                                    ::facet::Attr {
                                        ns: #ns_expr,
                                        key: #key_str,
                                        // SAFETY: Static const block pointer is valid for 'static
                                        data: unsafe { ::facet::OxRef::new(
                                            ::facet::PtrConst::new_sized(&const {
                                                𝟋Some(
                                                    (|__ptr: ::facet::PtrUninit| unsafe { __ptr.put($expr) })
                                                        as ::facet::DefaultInPlaceFn
                                                )
                                            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const ()),
                                            <() as ::facet::Facet>::SHAPE
                                        ) },
                                    }
                                }};
                            }
                        } else {
                            // Container-only attribute used on field: error
                            quote! {
                                (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                    compile_error!(concat!(
                                        "Attribute `",
                                        stringify!(#key_ident),
                                        "` can only be used on containers, not fields"
                                    ))
                                }};
                                (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $($args:tt)* }) => {{
                                    compile_error!(concat!(
                                        "Attribute `",
                                        stringify!(#key_ident),
                                        "` can only be used on containers, not fields"
                                    ))
                                }};
                            }
                        };

                        // Generate container-level arms (only if target allows containers)
                        let container_arms = if target != AttrTarget::Field {
                            quote! {
                                // Container-level: no args means use Default trait (no fallback - no $ty available)
                                (@ns { $ns:path } #key_ident { }) => {{
                                    ::facet::Attr {
                                        ns: #ns_expr,
                                        key: #key_str,
                                        // SAFETY: Static const block pointer is valid for 'static
                                        data: unsafe { ::facet::OxRef::new(
                                            ::facet::PtrConst::new_sized(&const {
                                                ::core::option::Option::<::facet::DefaultInPlaceFn>::None
                                            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const ()),
                                            <() as ::facet::Facet>::SHAPE
                                        ) },
                                    }
                                }};
                                // Container-level with args: not typical, error
                                (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                    compile_error!(concat!(
                                        "Container-level `",
                                        stringify!(#key_ident),
                                        "` with arguments is not supported"
                                    ))
                                }};
                            }
                        } else {
                            // Field-only attribute used on container: error
                            quote! {
                                (@ns { $ns:path } #key_ident { }) => {{
                                    compile_error!(concat!(
                                        "Attribute `",
                                        stringify!(#key_ident),
                                        "` can only be used on fields, not containers"
                                    ))
                                }};
                                (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                    compile_error!(concat!(
                                        "Attribute `",
                                        stringify!(#key_ident),
                                        "` can only be used on fields, not containers"
                                    ))
                                }};
                            }
                        };

                        quote! {
                            #field_arms
                            #container_arms
                        }
                    }
                    VariantKind::NewtypeStr => {
                        // NewtypeStr stores &'static str directly (not wrapped in Attr).
                        // This is necessary because facet-core needs to access tag/content/rename
                        // via get_builtin_attr_value but can't import the Attr enum.
                        let _crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for newtype_str variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        quote! {
                            // Field-level: no args is an error
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a string value: `",
                                    stringify!(#key_ident),
                                    " = \"value\"`"
                                ))
                            }};
                            // Field-level with `= "value"`: store string directly
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &$val)
                            }};
                            // Field-level with just expr
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &$val)
                            }};
                            // Container-level: no args is an error
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a string value: `",
                                    stringify!(#key_ident),
                                    " = \"value\"`"
                                ))
                            }};
                            // Container-level with `= "value"`: store string directly
                            (@ns { $ns:path } #key_ident { | = $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &$val)
                            }};
                            // Container-level with just expr
                            (@ns { $ns:path } #key_ident { | $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &$val)
                            }};
                        }
                    }
                    VariantKind::OptionalStr => {
                        // OptionalStr stores Option<&'static str> directly.
                        // - No args → None
                        // - `= "value"` → Some("value")
                        let crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for opt_str variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        let variant_name = &v.name;
                        quote! {
                            // Field-level: no args → None
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                static __ATTR_DATA: #crate_path::Attr = #crate_path::Attr::#variant_name(𝟋None);
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            // Field-level with `= "value"` → Some(value)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $val:expr }) => {{
                                static __ATTR_DATA: #crate_path::Attr = #crate_path::Attr::#variant_name(𝟋Some($val));
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            // Field-level with just expr → Some(value)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $val:expr }) => {{
                                static __ATTR_DATA: #crate_path::Attr = #crate_path::Attr::#variant_name(𝟋Some($val));
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            // Container-level: no args → None
                            (@ns { $ns:path } #key_ident { }) => {{
                                static __ATTR_DATA: #crate_path::Attr = #crate_path::Attr::#variant_name(𝟋None);
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            // Container-level with `= "value"` → Some(value)
                            (@ns { $ns:path } #key_ident { | = $val:expr }) => {{
                                static __ATTR_DATA: #crate_path::Attr = #crate_path::Attr::#variant_name(𝟋Some($val));
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            // Container-level with just expr → Some(value)
                            (@ns { $ns:path } #key_ident { | $val:expr }) => {{
                                static __ATTR_DATA: #crate_path::Attr = #crate_path::Attr::#variant_name(𝟋Some($val));
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};

                            // Generic-safe dispatch: use const payload generation.
                            (@const @ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                ::facet::Attr::new(
                                    #ns_expr,
                                    #key_str,
                                    &const { #crate_path::Attr::#variant_name(𝟋None) }
                                )
                            }};
                            (@const @ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $val:expr }) => {{
                                ::facet::Attr::new(
                                    #ns_expr,
                                    #key_str,
                                    &const { #crate_path::Attr::#variant_name(𝟋Some($val)) }
                                )
                            }};
                            (@const @ns { $ns:path } #key_ident { $field:tt : $ty:ty | $val:expr }) => {{
                                ::facet::Attr::new(
                                    #ns_expr,
                                    #key_str,
                                    &const { #crate_path::Attr::#variant_name(𝟋Some($val)) }
                                )
                            }};
                            (@const @ns { $ns:path } #key_ident { }) => {{
                                ::facet::Attr::new(
                                    #ns_expr,
                                    #key_str,
                                    &const { #crate_path::Attr::#variant_name(𝟋None) }
                                )
                            }};
                            (@const @ns { $ns:path } #key_ident { | = $val:expr }) => {{
                                ::facet::Attr::new(
                                    #ns_expr,
                                    #key_str,
                                    &const { #crate_path::Attr::#variant_name(𝟋Some($val)) }
                                )
                            }};
                            (@const @ns { $ns:path } #key_ident { | $val:expr }) => {{
                                ::facet::Attr::new(
                                    #ns_expr,
                                    #key_str,
                                    &const { #crate_path::Attr::#variant_name(𝟋Some($val)) }
                                )
                            }};
                        }
                    }
                    VariantKind::NewtypeI64 => {
                        // NewtypeI64 stores i64 directly (for numeric validation like min, max).
                        // We store the raw i64 value directly, not wrapped in an Attr enum.
                        // The deserializer uses the `key` field to know which validator to apply.
                        let _crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for newtype_i64 variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        quote! {
                            // Field-level: no args is an error (need a value)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a numeric value: `",
                                    stringify!(#key_ident),
                                    " = 42`"
                                ))
                            }};
                            // Field-level with `= value`: store i64 directly
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: i64 = $val; __V })
                            }};
                            // Field-level with just expr
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: i64 = $val; __V })
                            }};
                            // Container-level: no args is an error
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a numeric value: `",
                                    stringify!(#key_ident),
                                    " = 42`"
                                ))
                            }};
                            // Container-level with `= value`: store i64 directly
                            (@ns { $ns:path } #key_ident { | = $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: i64 = $val; __V })
                            }};
                            // Container-level with just expr
                            (@ns { $ns:path } #key_ident { | $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: i64 = $val; __V })
                            }};
                        }
                    }
                    VariantKind::NewtypeUsize => {
                        // NewtypeUsize stores usize directly (for length validation like min_length, max_length).
                        // We store the raw usize value directly, not wrapped in an Attr enum.
                        // The deserializer uses the `key` field to know which validator to apply.
                        let _crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for newtype_usize variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        quote! {
                            // Field-level: no args is an error (need a value)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a numeric value: `",
                                    stringify!(#key_ident),
                                    " = 42`"
                                ))
                            }};
                            // Field-level with `= value`: store usize directly
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: usize = $val; __V })
                            }};
                            // Field-level with just expr
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: usize = $val; __V })
                            }};
                            // Container-level: no args is an error
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    stringify!(#key_ident),
                                    "` requires a numeric value: `",
                                    stringify!(#key_ident),
                                    " = 42`"
                                ))
                            }};
                            // Container-level with `= value`: store usize directly
                            (@ns { $ns:path } #key_ident { | = $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: usize = $val; __V })
                            }};
                            // Container-level with just expr
                            (@ns { $ns:path } #key_ident { | $val:expr }) => {{
                                ::facet::Attr::new(#ns_expr, #key_str, &{ const __V: usize = $val; __V })
                            }};
                        }
                    }
                    VariantKind::Arbitrary => {
                        // Arbitrary variants are compile-time only - they don't store runtime data.
                        // The derive macro reads the raw tokens from the attribute directly.
                        // At runtime we just store a unit marker so the attribute exists.
                        // Pre-compute the full attribute name for error messages
                        let full_attr_name = if ns_str.is_empty() {
                            key_str.clone()
                        } else {
                            format!("{ns_str}::{key_str}")
                        };
                        quote! {
                            // Field-level: no args is an error (arbitrary needs a value)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    #full_attr_name,
                                    "` requires a value"
                                ))
                            }};
                            // Field-level with args: store unit marker, derive macro reads raw tokens
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $($args:tt)* }) => {{
                                static __UNIT: () = ();
                                ::facet::Attr::new(#ns_expr, #key_str, &__UNIT)
                            }};
                            // Container-level: no args is an error (arbitrary needs a value)
                            (@ns { $ns:path } #key_ident { }) => {{
                                compile_error!(concat!(
                                    "Attribute `",
                                    #full_attr_name,
                                    "` requires a value"
                                ))
                            }};
                            // Container-level with args: store unit marker, derive macro reads raw tokens
                            (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                static __UNIT: () = ();
                                ::facet::Attr::new(#ns_expr, #key_str, &__UNIT)
                            }};
                        }
                    }
                    VariantKind::Newtype(_) | VariantKind::NewtypeOptionChar | VariantKind::ArbitraryType(_) | VariantKind::Struct(_) | VariantKind::FnPtr(_) => {
                        // For non-unit variants, we need the crate_path to generate proper type references.
                        // The crate_path is passed to the proc macro so it can output e.g. `::figue::Attr::Short(...)`
                        let crate_path = self.crate_path.as_ref().expect(
                            "crate_path is required for non-unit variants; add `crate_path ::your_crate;` to the grammar"
                        );
                        quote! {
                            // Field-level: @ns { path } attr { field : Type } or with args
                            // Note: $field is tt not ident because tuple struct fields are literals (0, 1, etc.)
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { }
                                };
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | $($args:tt)* }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { $($args)* }
                                };
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
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
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
                            }};
                            (@ns { $ns:path } #key_ident { | $($args:tt)* }) => {{
                                static __ATTR_DATA: #crate_path::Attr = ::facet::__dispatch_attr!{
                                    @crate_path { #crate_path }
                                    @enum_name { #enum_name }
                                    @variants { #(#variants_meta),* }
                                    @name { #key_ident }
                                    @rest { $($args)* }
                                };
                                ::facet::Attr::new(#ns_expr, #key_str, &__ATTR_DATA)
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
            /// Input format: `@ns { namespace_path } attr_name { ... }` (optionally prefixed with `@const`)
            #[macro_export]
            #[doc(hidden)]
            macro_rules! __attr {
                #(#variant_arms)*

                // Generic-context marker. Variants that need custom const dispatch
                // can match `@const` explicitly; everything else falls back to
                // normal dispatch.
                (@const @ns { $ns:path } $name:ident $($rest:tt)*) => {
                    $crate::__attr!(@ns { $ns } $name $($rest)*)
                };

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
    fn generate(&self, builtin: bool) -> TokenStream2 {
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

        // For builtin mode, skip deriving Facet entirely because the derive macro
        // generates ::facet:: paths which don't work inside the facet crate.
        // Builtin attrs will have Facet implemented manually via a blanket impl.
        let derive_attr = if builtin {
            quote! { #[derive(Debug, Clone, PartialEq, Default)] }
        } else {
            quote! { #[derive(Debug, Clone, PartialEq, Default, ::facet::Facet)] }
        };

        quote! {
            #(#attrs)*
            #derive_attr
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
