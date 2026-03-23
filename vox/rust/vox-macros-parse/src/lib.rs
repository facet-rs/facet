#![allow(clippy::result_large_err)]
//! Parser grammar for vox RPC service trait definitions.
//!
//! # This Is Just a Grammar
//!
//! This crate contains **only** the [unsynn] grammar for parsing Rust trait definitions
//! that define vox RPC services. It does not:
//!
//! - Generate any code
//! - Perform validation
//! - Know anything about vox's wire protocol
//! - Have opinions about how services should be implemented
//!
//! It simply parses syntax like:
//!
//! ```ignore
//! pub trait Calculator {
//!     /// Add two numbers.
//!     async fn add(&self, a: i32, b: i32) -> i32;
//! }
//! ```
//!
//! ...and produces an AST ([`ServiceTrait`]) that downstream crates can inspect.
//!
//! # Why a Separate Crate?
//!
//! The grammar is extracted into its own crate so that:
//!
//! 1. **It can be tested independently** — We use [datatest-stable] + [insta] for
//!    snapshot testing the parsed AST, which isn't possible in a proc-macro crate.
//!
//! 2. **It's reusable** — Other tools (linters, documentation generators, IDE plugins)
//!    can parse service definitions without pulling in proc-macro dependencies.
//!
//! 3. **Separation of concerns** — The grammar is pure parsing; [`vox-macros`] handles
//!    the proc-macro machinery; [`vox-codegen`] handles actual code generation.
//!
//! # The Bigger Picture
//!
//! ```text
//! vox-macros-parse     vox-macros              vox-codegen
//! ┌──────────────┐     ┌──────────────┐         ┌──────────────┐
//! │              │     │              │         │              │
//! │  unsynn      │────▶│  #[service]  │────────▶│  build.rs    │
//! │  grammar     │     │  proc macro  │         │  code gen    │
//! │              │     │              │         │              │
//! └──────────────┘     └──────────────┘         └──────────────┘
//!    just parsing         emit metadata          Rust, TS, Go...
//! ```
//!
//! [unsynn]: https://docs.rs/unsynn
//! [datatest-stable]: https://docs.rs/datatest-stable
//! [insta]: https://docs.rs/insta
//! [`vox-macros`]: https://docs.rs/vox-service-macros
//! [`vox-codegen`]: https://docs.rs/vox-codegen

pub use unsynn::Error as ParseError;
pub use unsynn::ToTokens;

use proc_macro2::TokenStream as TokenStream2;
use unsynn::operator::names::{
    Assign, Colon, Comma, Gt, LifetimeTick, Lt, PathSep, Pound, RArrow, Semicolon,
};
use unsynn::{
    Any, BraceGroupContaining, BracketGroupContaining, CommaDelimitedVec, Cons, Either,
    EndOfStream, Except, Ident, LiteralString, Many, Optional, ParenthesisGroupContaining, Parse,
    ToTokenIter, TokenStream, keyword, unsynn,
};

keyword! {
    pub KAsync = "async";
    pub KFn = "fn";
    pub KTrait = "trait";
    pub KSelfKw = "self";
    pub KMut = "mut";
    pub KDoc = "doc";
    pub KPub = "pub";
    pub KWhere = "where";
}

/// Parses tokens and groups until `C` is found, handling `<...>` correctly.
type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping.
    #[derive(Clone)]
    pub struct AngleTokenTree(
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, unsynn::TokenTree>,
    );

    pub struct RawAttribute {
        pub _pound: Pound,
        pub body: BracketGroupContaining<TokenStream>,
    }

    pub struct DocAttribute {
        pub _doc: KDoc,
        pub _assign: Assign,
        pub value: LiteralString,
    }

    pub enum Visibility {
        Pub(KPub),
        PubRestricted(Cons<KPub, ParenthesisGroupContaining<TokenStream>>),
    }

    pub struct RefSelf {
        pub _amp: unsynn::operator::names::And,
        pub mutability: Option<KMut>,
        pub name: KSelfKw,
    }

    pub struct MethodParam {
        pub name: Ident,
        pub _colon: Colon,
        pub ty: Type,
    }

    pub struct GenericParams {
        pub _lt: Lt,
        pub params: VerbatimUntil<Gt>,
        pub _gt: Gt,
    }

    #[derive(Clone)]
    pub struct TypePath {
        pub leading: Option<PathSep>,
        pub first: Ident,
        pub rest: Any<Cons<PathSep, Ident>>,
    }

    #[derive(Clone)]
    pub struct Lifetime {
        pub _apo: LifetimeTick,
        pub ident: Ident,
    }

    #[derive(Clone)]
    pub enum GenericArgument {
        Lifetime(Lifetime),
        Type(Type),
    }

    #[derive(Clone)]
    pub enum Type {
        Reference(TypeRef),
        Tuple(TypeTuple),
        PathWithGenerics(PathWithGenerics),
        Path(TypePath),
    }

    #[derive(Clone)]
    pub struct TypeRef {
        pub _amp: unsynn::operator::names::And,
        pub lifetime: Option<Cons<LifetimeTick, Ident>>,
        pub mutable: Option<KMut>,
        pub inner: Box<Type>,
    }

    #[derive(Clone)]
    pub struct TypeTuple(
        pub ParenthesisGroupContaining<CommaDelimitedVec<Type>>,
    );

    #[derive(Clone)]
    pub struct PathWithGenerics {
        pub path: TypePath,
        pub _lt: Lt,
        pub args: CommaDelimitedVec<GenericArgument>,
        pub _gt: Gt,
    }

    pub struct ReturnType {
        pub _arrow: RArrow,
        pub ty: Type,
    }

    pub struct WhereClause {
        pub _where: KWhere,
        pub bounds: VerbatimUntil<Semicolon>,
    }

    pub struct MethodParams {
        pub receiver: RefSelf,
        pub rest: Optional<Cons<Comma, CommaDelimitedVec<MethodParam>>>,
    }

    pub struct ServiceMethod {
        pub attributes: Any<RawAttribute>,
        pub _async: KAsync,
        pub _fn: KFn,
        pub name: Ident,
        pub generics: Optional<GenericParams>,
        pub params: ParenthesisGroupContaining<MethodParams>,
        pub return_type: Optional<ReturnType>,
        pub where_clause: Optional<WhereClause>,
        pub _semi: Semicolon,
    }

    pub struct ServiceTrait {
        pub attributes: Any<RawAttribute>,
        pub vis: Optional<Visibility>,
        pub _trait: KTrait,
        pub name: Ident,
        pub generics: Optional<GenericParams>,
        pub body: BraceGroupContaining<Any<ServiceMethod>>,
        pub _eos: EndOfStream,
    }
}

// ============================================================================
// Helper methods for GenericArgument
// ============================================================================

impl GenericArgument {
    pub fn has_lifetime(&self) -> bool {
        match self {
            GenericArgument::Lifetime(_) => true,
            GenericArgument::Type(ty) => ty.has_lifetime(),
        }
    }

    pub fn has_named_lifetime(&self, name: &str) -> bool {
        match self {
            GenericArgument::Lifetime(lifetime) => lifetime.ident == name,
            GenericArgument::Type(ty) => ty.has_named_lifetime(name),
        }
    }

    pub fn has_non_named_lifetime(&self, name: &str) -> bool {
        match self {
            GenericArgument::Lifetime(lifetime) => lifetime.ident != name,
            GenericArgument::Type(ty) => ty.has_non_named_lifetime(name),
        }
    }

    pub fn has_elided_reference_lifetime(&self) -> bool {
        match self {
            GenericArgument::Lifetime(_) => false,
            GenericArgument::Type(ty) => ty.has_elided_reference_lifetime(),
        }
    }

    pub fn contains_channel(&self) -> bool {
        match self {
            GenericArgument::Lifetime(_) => false,
            GenericArgument::Type(ty) => ty.contains_channel(),
        }
    }
}

// ============================================================================
// Helper methods for Type
// ============================================================================

impl Type {
    /// Extract Ok and Err types if this is Result<T, E>
    pub fn as_result(&self) -> Option<(&Type, &Type)> {
        match self {
            Type::PathWithGenerics(PathWithGenerics { path, args, .. })
                if path.last_segment().as_str() == "Result" && args.len() == 2 =>
            {
                let args_slice = args.as_slice();
                match (&args_slice[0].value, &args_slice[1].value) {
                    (GenericArgument::Type(ok), GenericArgument::Type(err)) => Some((ok, err)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Check if type contains a lifetime anywhere in the tree
    pub fn has_lifetime(&self) -> bool {
        match self {
            Type::Reference(TypeRef {
                lifetime: Some(_), ..
            }) => true,
            Type::Reference(TypeRef { inner, .. }) => inner.has_lifetime(),
            Type::PathWithGenerics(PathWithGenerics { args, .. }) => {
                args.iter().any(|t| t.value.has_lifetime())
            }
            Type::Tuple(TypeTuple(group)) => group.content.iter().any(|t| t.value.has_lifetime()),
            Type::Path(_) => false,
        }
    }

    /// Check if type contains the named lifetime anywhere in the tree.
    pub fn has_named_lifetime(&self, name: &str) -> bool {
        match self {
            Type::Reference(TypeRef {
                lifetime: Some(lifetime),
                ..
            }) => lifetime.second == name,
            Type::Reference(TypeRef { inner, .. }) => inner.has_named_lifetime(name),
            Type::PathWithGenerics(PathWithGenerics { args, .. }) => {
                args.iter().any(|t| t.value.has_named_lifetime(name))
            }
            Type::Tuple(TypeTuple(group)) => group
                .content
                .iter()
                .any(|t| t.value.has_named_lifetime(name)),
            Type::Path(_) => false,
        }
    }

    /// Check if type contains any named lifetime other than `name`.
    pub fn has_non_named_lifetime(&self, name: &str) -> bool {
        match self {
            Type::Reference(TypeRef {
                lifetime: Some(lifetime),
                ..
            }) => lifetime.second != name,
            Type::Reference(TypeRef { inner, .. }) => inner.has_non_named_lifetime(name),
            Type::PathWithGenerics(PathWithGenerics { args, .. }) => {
                args.iter().any(|t| t.value.has_non_named_lifetime(name))
            }
            Type::Tuple(TypeTuple(group)) => group
                .content
                .iter()
                .any(|t| t.value.has_non_named_lifetime(name)),
            Type::Path(_) => false,
        }
    }

    /// Check if type contains any `&T` reference without an explicit lifetime.
    ///
    /// We require explicit `'vox` for borrowed RPC return payloads.
    pub fn has_elided_reference_lifetime(&self) -> bool {
        match self {
            Type::Reference(TypeRef { lifetime: None, .. }) => true,
            Type::Reference(TypeRef { inner, .. }) => inner.has_elided_reference_lifetime(),
            Type::PathWithGenerics(PathWithGenerics { args, .. }) => {
                args.iter().any(|t| t.value.has_elided_reference_lifetime())
            }
            Type::Tuple(TypeTuple(group)) => group
                .content
                .iter()
                .any(|t| t.value.has_elided_reference_lifetime()),
            Type::Path(_) => false,
        }
    }

    /// Check if type contains Tx or Rx at any nesting level
    ///
    /// Note: This is a heuristic based on type names. Proper validation should
    /// happen at codegen time when we can resolve types properly.
    pub fn contains_channel(&self) -> bool {
        match self {
            Type::Reference(TypeRef { inner, .. }) => inner.contains_channel(),
            Type::Tuple(TypeTuple(group)) => {
                group.content.iter().any(|t| t.value.contains_channel())
            }
            Type::PathWithGenerics(PathWithGenerics { path, args, .. }) => {
                let seg = path.last_segment();
                if seg == "Tx" || seg == "Rx" {
                    return true;
                }
                args.iter().any(|t| t.value.contains_channel())
            }
            Type::Path(path) => {
                let seg = path.last_segment();
                seg == "Tx" || seg == "Rx"
            }
        }
    }
}

// ============================================================================
// Helper methods for TypePath
// ============================================================================

impl TypePath {
    /// Get the last segment (e.g., "Result" from "std::result::Result")
    pub fn last_segment(&self) -> String {
        self.rest
            .iter()
            .last()
            .map(|seg| seg.value.second.to_string())
            .unwrap_or_else(|| self.first.to_string())
    }
}

// ============================================================================
// Helper methods for ServiceTrait
// ============================================================================

impl ServiceTrait {
    /// Get the trait name as a string.
    pub fn name(&self) -> String {
        self.name.to_string()
    }

    /// Get the trait's doc string (collected from #[doc = "..."] attributes).
    pub fn doc(&self) -> Option<String> {
        collect_doc_string(&self.attributes)
    }

    /// Get an iterator over the methods.
    pub fn methods(&self) -> impl Iterator<Item = &ServiceMethod> {
        self.body.content.iter().map(|entry| &entry.value)
    }
}

// ============================================================================
// Helper methods for ServiceMethod
// ============================================================================

impl ServiceMethod {
    /// Get the method name as a string.
    pub fn name(&self) -> String {
        self.name.to_string()
    }

    /// Get the method's doc string (collected from #[doc = "..."] attributes).
    pub fn doc(&self) -> Option<String> {
        collect_doc_string(&self.attributes)
    }

    /// Get an iterator over the method's parameters (excluding &self).
    pub fn args(&self) -> impl Iterator<Item = &MethodParam> {
        self.params
            .content
            .rest
            .iter()
            .flat_map(|rest| rest.value.second.iter().map(|entry| &entry.value))
    }

    /// Get the return type, defaulting to () if not specified.
    pub fn return_type(&self) -> Type {
        self.return_type
            .iter()
            .next()
            .map(|r| r.value.ty.clone())
            .unwrap_or_else(unit_type)
    }

    /// Check if receiver is &mut self (not allowed for service methods).
    pub fn is_mut_receiver(&self) -> bool {
        self.params.content.receiver.mutability.is_some()
    }

    /// Check if method has generics.
    pub fn has_generics(&self) -> bool {
        !self.generics.is_empty()
    }

    /// Check whether this method explicitly opts into request context injection.
    pub fn wants_context(&self) -> bool {
        has_attr_path(&self.attributes, &["vox", "context"])
    }

    /// Check whether this method explicitly declares rerun-safe semantics.
    pub fn is_idem(&self) -> bool {
        has_attr_helper(&self.attributes, &["vox"], "idem")
    }

    /// Check whether this method explicitly declares persistent admission.
    pub fn is_persist(&self) -> bool {
        has_attr_helper(&self.attributes, &["vox"], "persist")
    }
}

// ============================================================================
// Helper methods for MethodParam
// ============================================================================

impl MethodParam {
    /// Get the parameter name as a string.
    pub fn name(&self) -> String {
        self.name.to_string()
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Extract Ok and Err types from a return type.
/// Returns (ok_type, Some(err_type)) for Result<T, E>, or (type, None) otherwise.
pub fn method_ok_and_err_types(return_ty: &Type) -> (&Type, Option<&Type>) {
    if let Some((ok, err)) = return_ty.as_result() {
        (ok, Some(err))
    } else {
        (return_ty, None)
    }
}

/// Returns the unit type `()`.
fn unit_type() -> Type {
    let mut iter = "()".to_token_iter();
    Type::parse(&mut iter).expect("unit type should always parse")
}

/// Collect doc strings from attributes.
fn collect_doc_string(attrs: &Any<RawAttribute>) -> Option<String> {
    let mut docs = Vec::new();

    for attr in attrs.iter() {
        let mut body_iter = attr.value.body.content.clone().to_token_iter();
        if let Ok(doc_attr) = DocAttribute::parse(&mut body_iter) {
            let line = doc_attr
                .value
                .as_str()
                .replace("\\\"", "\"")
                .replace("\\'", "'");
            docs.push(line);
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

fn has_attr_path(attrs: &Any<RawAttribute>, expected: &[&str]) -> bool {
    attrs
        .iter()
        .any(|attr| attr_path_matches(&attr.value, expected))
}

fn has_attr_helper(attrs: &Any<RawAttribute>, path: &[&str], helper: &str) -> bool {
    attrs
        .iter()
        .any(|attr| attr_helper_matches(&attr.value, path, helper))
}

fn attr_path_matches(attr: &RawAttribute, expected: &[&str]) -> bool {
    let mut iter = attr.body.content.clone().to_token_iter();
    let Ok(path) = TypePath::parse(&mut iter) else {
        return false;
    };
    if EndOfStream::parse(&mut iter).is_err() {
        return false;
    }
    path_matches(&path, expected)
}

fn attr_helper_matches(attr: &RawAttribute, expected_path: &[&str], expected_helper: &str) -> bool {
    let mut iter = attr.body.content.clone().to_token_iter();
    let Ok(path) = TypePath::parse(&mut iter) else {
        return false;
    };
    if !path_matches(&path, expected_path) {
        return false;
    }

    let Ok(group) = ParenthesisGroupContaining::<TokenStream>::parse(&mut iter) else {
        return false;
    };
    if EndOfStream::parse(&mut iter).is_err() {
        return false;
    }

    let mut inner = group.content.to_token_iter();
    let Ok(helper) = Ident::parse(&mut inner) else {
        return false;
    };
    if EndOfStream::parse(&mut inner).is_err() {
        return false;
    }
    helper == expected_helper
}

fn path_matches(path: &TypePath, expected: &[&str]) -> bool {
    let actual = std::iter::once(path.first.to_string())
        .chain(path.rest.iter().map(|seg| seg.value.second.to_string()))
        .collect::<Vec<_>>();

    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected.iter())
            .all(|(actual, expected)| actual == expected)
}

/// Parse a trait definition from a token stream.
#[allow(clippy::result_large_err)] // unsynn::Error is external, we can't box it
pub fn parse_trait(tokens: &TokenStream2) -> Result<ServiceTrait, unsynn::Error> {
    let mut iter = tokens.clone().to_token_iter();
    ServiceTrait::parse(&mut iter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ServiceTrait {
        let ts: TokenStream2 = src.parse().expect("tokenstream parse");
        parse_trait(&ts).expect("trait parse")
    }

    #[test]
    fn parse_trait_exposes_docs_methods_and_args() {
        let trait_def = parse(
            r#"
            #[doc = "Calculator service."]
            pub trait Calculator {
                #[doc = "Adds two numbers."]
                async fn add(&self, a: i32, b: i32) -> Result<i64, String>;
            }
            "#,
        );

        assert_eq!(trait_def.name(), "Calculator");
        assert_eq!(trait_def.doc(), Some("Calculator service.".to_string()));

        let method = trait_def.methods().next().expect("method");
        assert_eq!(method.name(), "add");
        assert_eq!(method.doc(), Some("Adds two numbers.".to_string()));
        assert_eq!(
            method.args().map(|arg| arg.name()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        let ret = method.return_type();
        let (ok, err) = method_ok_and_err_types(&ret);
        assert!(ok.as_result().is_none());
        assert!(err.is_some());
    }

    #[test]
    fn return_type_defaults_to_unit_when_omitted() {
        let trait_def = parse(
            r#"
            trait Svc {
                async fn ping(&self);
            }
            "#,
        );
        let method = trait_def.methods().next().expect("method");
        let ret = method.return_type();
        match ret {
            Type::Tuple(TypeTuple(group)) => assert!(group.content.is_empty()),
            other => panic!(
                "expected unit tuple return, got {}",
                other.to_token_stream()
            ),
        }
    }

    #[test]
    fn method_helpers_detect_generics_and_mut_receiver() {
        let trait_def = parse(
            r#"
            trait Svc {
                async fn bad<T>(&mut self, value: T) -> T;
            }
            "#,
        );
        let method = trait_def.methods().next().expect("method");
        assert!(method.has_generics());
        assert!(method.is_mut_receiver());
    }

    #[test]
    fn method_helpers_detect_explicit_request_context_opt_in() {
        let trait_def = parse(
            r#"
            trait Svc {
                #[vox::context]
                async fn contextual(&self) -> u32;

                async fn plain(&self) -> u32;
            }
            "#,
        );
        let mut methods = trait_def.methods();
        assert!(methods.next().expect("contextual method").wants_context());
        assert!(!methods.next().expect("plain method").wants_context());
    }

    #[test]
    fn method_helpers_detect_retry_helper_attributes() {
        let trait_def = parse(
            r#"
            trait Svc {
                #[vox(idem)]
                async fn cached(&self) -> u32;

                #[vox(persist)]
                async fn durable(&self) -> u32;

                async fn plain(&self) -> u32;
            }
            "#,
        );
        let mut methods = trait_def.methods();
        let cached = methods.next().expect("cached");
        assert!(cached.is_idem());
        assert!(!cached.is_persist());

        let durable = methods.next().expect("durable");
        assert!(!durable.is_idem());
        assert!(durable.is_persist());

        let plain = methods.next().expect("plain");
        assert!(!plain.is_idem());
        assert!(!plain.is_persist());
    }

    #[test]
    fn type_helpers_detect_result_lifetime_and_channel_nesting() {
        let trait_def = parse(
            r#"
            trait Svc {
                async fn stream(&self, input: &'static str) -> Result<Option<Tx<Vec<u8>>>, Rx<u32>>;
            }
            "#,
        );
        let method = trait_def.methods().next().expect("method");
        let arg = method.args().next().expect("arg");
        assert!(arg.ty.has_lifetime());
        assert!(!arg.ty.contains_channel());

        let ret = method.return_type();
        let (ok, err) = method_ok_and_err_types(&ret);
        assert!(ok.contains_channel());
        assert!(err.expect("result err type").contains_channel());
    }

    #[test]
    fn type_helpers_detect_named_and_elided_lifetimes() {
        let trait_def = parse(
            r#"
            trait Svc {
                async fn borrowed(&self) -> Result<&'vox str, Error>;
                async fn bad_lifetime(&self) -> Result<&'a str, Error>;
                async fn elided(&self) -> Result<&str, Error>;
            }
            "#,
        );
        let mut methods = trait_def.methods();

        let borrowed = methods.next().expect("borrowed method").return_type();
        let (borrowed_ok, _) = method_ok_and_err_types(&borrowed);
        assert!(borrowed_ok.has_named_lifetime("vox"));
        assert!(!borrowed_ok.has_non_named_lifetime("vox"));
        assert!(!borrowed_ok.has_elided_reference_lifetime());

        let bad_lifetime = methods.next().expect("bad_lifetime method").return_type();
        let (bad_ok, _) = method_ok_and_err_types(&bad_lifetime);
        assert!(!bad_ok.has_named_lifetime("vox"));
        assert!(bad_ok.has_non_named_lifetime("vox"));
        assert!(!bad_ok.has_elided_reference_lifetime());

        let elided = methods.next().expect("elided method").return_type();
        let (elided_ok, _) = method_ok_and_err_types(&elided);
        assert!(!elided_ok.has_named_lifetime("vox"));
        assert!(!elided_ok.has_non_named_lifetime("vox"));
        assert!(elided_ok.has_elided_reference_lifetime());
    }

    #[test]
    fn type_path_last_segment_uses_trailing_segment() {
        let trait_def = parse(
            r#"
            trait Svc {
                async fn f(&self) -> std::result::Result<u8, u8>;
            }
            "#,
        );
        let method = trait_def.methods().next().expect("method");
        let ret = method.return_type();
        let Type::PathWithGenerics(path_with_generics) = ret else {
            panic!("expected path with generics");
        };
        assert_eq!(path_with_generics.path.last_segment(), "Result");
    }
}
