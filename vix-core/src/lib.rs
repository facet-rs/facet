//! vix — the demand-driven build language.
//!
//! The compiler starts at [`surface`], lowers through [`vir`] and [`lowering`],
//! and runs through [`runtime`]. The surface AST is generated from its Snark
//! grammar.

// So this crate's own wire types can spell `#[facet(vix::wire_extern = "…")]`
// exactly as an embedder does (see [`wire`]).
extern crate self as vix;

pub mod binding;
pub mod compiler;
pub mod decode;
pub mod diagnostic;
pub mod exec;
pub mod fetch;
pub mod lowering;
pub mod modules;
pub mod prelude;
pub mod reloc_selection;
pub mod runtime;
pub mod schema;
pub mod surface;
pub mod vir;

// The `vix::` facet extension-attribute grammar (`#[facet(vix::wire_extern =
// "…")]` — a wire type declaring which vix extern it maps to). Declared in
// `vix-wire` and re-exported here so both this crate's own wire types and any
// embedder's spell the same `vix::` prefix; the derive resolves the grammar's
// dispatch macros (`__attr` / `__parse_attr`) and its `Attr` enum through this
// root, exactly as `dibs` re-exports `dibs-db-schema`.
pub use vix_wire::{__attr, __parse_attr, Attr};

/// Runtime support for the generated lowering.
pub mod support {
    use snark::parser::ResolvedCstNode;

    /// Half-open byte range into the source, carried by every AST node.
    #[derive(facet::Facet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Span {
        pub start: u32,
        pub end: u32,
    }

    impl Span {
        pub fn contains(&self, offset: u32) -> bool {
            self.start <= offset && offset < self.end
        }
    }

    /// A decoded leaf value plus where it came from — rename/go-to-def need the
    /// span of every identifier OCCURRENCE, not just of enclosing nodes.
    #[derive(facet::Facet, Debug, Clone, PartialEq)]
    pub struct Spanned<T> {
        pub value: T,
        pub span: Span,
    }

    impl<T> core::ops::Deref for Spanned<T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.value
        }
    }

    impl PartialEq<&str> for Spanned<String> {
        fn eq(&self, other: &&str) -> bool {
            self.value == *other
        }
    }

    /// This node's source span.
    pub fn span(n: &ResolvedCstNode) -> Span {
        let bytes = n.bytes();
        Span {
            start: bytes.start().get(),
            end: bytes.end().get(),
        }
    }

    /// All children carrying the given field.
    pub fn fields<'a>(
        n: &'a ResolvedCstNode,
        name: &'static str,
    ) -> impl Iterator<Item = &'a ResolvedCstNode> {
        n.children().iter().filter(move |c| c.field() == Some(name))
    }

    /// The single child carrying the given field; the grammar guarantees exactly one,
    /// so absence is a codegen/parser bug.
    pub fn field_one<'a>(n: &'a ResolvedCstNode, name: &str, kind: &str) -> &'a ResolvedCstNode {
        field_opt(n, name).unwrap_or_else(|| panic!("missing field `{name}` on `{kind}` node"))
    }

    pub fn field_opt<'a>(n: &'a ResolvedCstNode, name: &str) -> Option<&'a ResolvedCstNode> {
        n.children().iter().find(|c| c.field() == Some(name))
    }

    /// Raw source text of a node: its own token text, or its non-extra descendants'
    /// token texts concatenated in order (leaf nodes wrap one anonymous token).
    fn raw_text(n: &ResolvedCstNode) -> String {
        fn collect(n: &ResolvedCstNode, out: &mut String) {
            if let Some(t) = n.text() {
                out.push_str(t);
                return;
            }
            for c in n.children() {
                if !c.extra() {
                    collect(c, out);
                }
            }
        }
        let mut out = String::new();
        collect(n, &mut out);
        out
    }

    /// Raw-text leaf (identifier, number, flag, command token).
    pub fn node_text(n: &ResolvedCstNode) -> Spanned<String> {
        Spanned {
            value: raw_text(n),
            span: span(n),
        }
    }

    /// Token-valued fields (`field("op", …)`, `field("vis", "pub")`): snark drops
    /// `Field { child: None }` for anonymous token steps, so the generated lowering
    /// scans the node's direct anonymous children against the grammar-derived token
    /// set instead. The sets never collide with a node's other anonymous tokens.
    pub fn token_field(n: &ResolvedCstNode, set: &[&str]) -> Option<String> {
        n.children()
            .iter()
            .filter(|c| !c.named() && !c.extra())
            .find_map(|c| c.text().filter(|t| set.contains(t)).map(str::to_owned))
    }

    /// `"…"` string literal -> contents, escapes processed.
    pub fn decode_string(n: &ResolvedCstNode) -> Spanned<String> {
        Spanned {
            value: unquote(&raw_text(n)),
            span: span(n),
        }
    }

    /// `p"…"` path literal -> contents, escapes processed.
    pub fn decode_path(n: &ResolvedCstNode) -> Spanned<String> {
        let raw = raw_text(n);
        Spanned {
            value: unquote(raw.strip_prefix('p').unwrap_or(&raw)),
            span: span(n),
        }
    }

    /// `tmpl"…"` template literal -> contents, escapes processed.
    pub fn decode_template(n: &ResolvedCstNode) -> Spanned<String> {
        let raw = raw_text(n);
        Spanned {
            value: unquote(raw.strip_prefix("tmpl").unwrap_or(&raw)),
            span: span(n),
        }
    }

    pub fn decode_bool(n: &ResolvedCstNode) -> Spanned<bool> {
        Spanned {
            value: raw_text(n) == "true",
            span: span(n),
        }
    }

    fn unquote(raw: &str) -> String {
        let inner = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw);
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some(other) => out.push(other),
                    None => {}
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PathMissing {
        pub path: String,
    }

    impl PathMissing {
        pub fn diagnostic(&self) -> String {
            format!("path `{}` is missing from resolved tree", self.path)
        }
    }
}
