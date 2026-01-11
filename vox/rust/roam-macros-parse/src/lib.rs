//! Parser grammar for roam RPC service trait definitions.
//!
//! # This Is Just a Grammar
//!
//! This crate contains **only** the [unsynn] grammar for parsing Rust trait definitions
//! that define roam RPC services. It does not:
//!
//! - Generate any code
//! - Perform validation
//! - Know anything about roam's wire protocol
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
//! 3. **Separation of concerns** — The grammar is pure parsing; [`roam-macros`] handles
//!    the proc-macro machinery; [`roam-codegen`] handles actual code generation.
//!
//! # The Bigger Picture
//!
//! ```text
//! roam-macros-parse     roam-macros              roam-codegen
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
//! [`roam-macros`]: https://docs.rs/roam-service-macros
//! [`roam-codegen`]: https://docs.rs/roam-codegen

pub use unsynn::Error as ParseError;
pub use unsynn::ToTokens;

use proc_macro2::TokenStream as TokenStream2;
use unsynn::operator::names::{Assign, Colon, Comma, Gt, Lt, PathSep, Pound, RArrow, Semicolon};
use unsynn::{
    Any, BraceGroupContaining, BracketGroupContaining, CommaDelimitedVec, Cons, Either,
    EndOfStream, Except, Ident, LiteralString, Many, Optional, ParenthesisGroupContaining, Parse,
    ToTokenIter, TokenStream, keyword, operator, unsynn,
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

operator! {
    pub Apostrophe = "'";
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
    pub enum Type {
        Reference(TypeRef),
        Tuple(TypeTuple),
        PathWithGenerics(PathWithGenerics),
        Path(TypePath),
    }

    #[derive(Clone)]
    pub struct TypeRef {
        pub _amp: unsynn::operator::names::And,
        pub lifetime: Option<Cons<Apostrophe, Ident>>,
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
        pub args: CommaDelimitedVec<Type>,
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
// Helper methods for Type
// ============================================================================

impl Type {
    /// Extract Ok and Err types if this is Result<T, E>
    pub fn as_result(&self) -> Option<(&Type, &Type)> {
        match self {
            Type::PathWithGenerics(PathWithGenerics { path, args, .. })
                if path.last_segment().as_str() == "Result" && args.len() == 2 =>
            {
                let types = args.as_slice();
                Some((&types[0].value, &types[1].value))
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
            let line = doc_attr.value.as_str().replace("\\\"", "\"");
            docs.push(line);
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

/// Parse a trait definition from a token stream.
#[allow(clippy::result_large_err)] // unsynn::Error is external, we can't box it
pub fn parse_trait(tokens: &TokenStream2) -> Result<ServiceTrait, unsynn::Error> {
    let mut iter = tokens.clone().to_token_iter();
    ServiceTrait::parse(&mut iter)
}
