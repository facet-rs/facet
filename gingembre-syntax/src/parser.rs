//! Recursive-descent + precedence parser that builds a lossless cstree from the lexer's
//! token stream, with error recovery (it never bails — unexpected input becomes an
//! `Error` token and parsing continues, which is what the LSP needs).
//!
//! Precedence (loosest → tightest), matching the legacy gingembre parser so lowering
//! yields an identical AST:
//! `ternary > or > and > not > comparison > add(+ - ~) > mul(* / // %) > unary(-) >
//!  power(**) > filter(|) > postfix(. [] () ?) > primary`.

use cstree::build::GreenNodeBuilder;
use cstree::syntax::ResolvedNode;

use crate::SyntaxKind::{self, *};
use crate::lexer::{Lexeme, lex};

/// A parse error: a message and the byte offset where it was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

/// The result of parsing: the resolved (red) tree plus any recovered errors.
pub struct Parse {
    root: ResolvedNode<SyntaxKind>,
    pub errors: Vec<ParseError>,
}

impl Parse {
    /// The root syntax node (text-resolving, so it `Display`s as the source).
    pub fn syntax(&self) -> &ResolvedNode<SyntaxKind> {
        &self.root
    }
}

/// Parse a template source into a lossless CST.
pub fn parse(src: &str) -> Parse {
    let tokens = lex(src);
    let mut p = Parser {
        tokens,
        pos: 0,
        offset: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
    };
    p.parse_template();
    let (green, cache) = p.builder.finish();
    let interner = cache.unwrap().into_interner().unwrap();
    let root = ResolvedNode::new_root_with_resolver(green, interner);
    Parse { root, errors: p.errors }
}

/// Convenience for parsing a bare expression (e.g. tests), wrapped in a `Template`.
pub fn parse_expr_str(src: &str) -> Parse {
    parse(&format!("{{{{ {src} }}}}"))
}

struct Parser<'src> {
    tokens: Vec<Lexeme<'src>>,
    pos: usize,
    offset: usize,
    builder: GreenNodeBuilder<'static, 'static, SyntaxKind>,
    errors: Vec<ParseError>,
}

impl<'src> Parser<'src> {
    // ----- token cursor (trivia-skipping for decisions, trivia-preserving in the tree) -----

    fn is_trivia(k: SyntaxKind) -> bool {
        matches!(k, Whitespace | Comment)
    }

    /// Kind of the n-th *non-trivia* token from the cursor, without consuming.
    fn nth(&self, n: usize) -> Option<SyntaxKind> {
        let mut seen = 0;
        for t in &self.tokens[self.pos..] {
            if Self::is_trivia(t.kind) {
                continue;
            }
            if seen == n {
                return Some(t.kind);
            }
            seen += 1;
        }
        None
    }

    fn at(&self, k: SyntaxKind) -> bool {
        self.nth(0) == Some(k)
    }

    fn at_end(&self) -> bool {
        self.nth(0).is_none()
    }

    /// Emit any pending trivia tokens into the tree.
    fn eat_trivia(&mut self) {
        while let Some(t) = self.tokens.get(self.pos) {
            if !Self::is_trivia(t.kind) {
                break;
            }
            self.builder.token(t.kind, t.text);
            self.offset += t.text.len();
            self.pos += 1;
        }
    }

    /// Emit pending trivia, then the current (non-trivia) token, and advance.
    fn bump(&mut self) {
        self.eat_trivia();
        if let Some(t) = self.tokens.get(self.pos) {
            self.builder.token(t.kind, t.text);
            self.offset += t.text.len();
            self.pos += 1;
        }
    }

    fn eat(&mut self, k: SyntaxKind) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, k: SyntaxKind) {
        if !self.eat(k) {
            self.error(format!("expected {k:?}, found {:?}", self.nth(0)));
        }
    }

    fn error(&mut self, message: String) {
        self.eat_trivia();
        let offset = self.offset;
        self.errors.push(ParseError { message, offset });
        // Wrap the offending token (if any) in an Error node so the tree stays lossless.
        if self.tokens.get(self.pos).is_some() {
            self.builder.start_node(Error);
            self.bump();
            self.builder.finish_node();
        }
    }

    fn start(&mut self, k: SyntaxKind) {
        self.eat_trivia();
        self.builder.start_node(k);
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }

    // ----- template structure -----

    fn parse_template(&mut self) {
        self.builder.start_node(Template);
        while !self.tokens.is_empty() && self.pos < self.tokens.len() {
            match self.tokens[self.pos].kind {
                Whitespace | Comment | Text => {
                    let t = self.tokens[self.pos];
                    self.builder.token(t.kind, t.text);
                    self.offset += t.text.len();
                    self.pos += 1;
                }
                OpenExpr | OpenExprTrim => self.parse_interpolation(),
                OpenStmt | OpenStmtTrim => self.parse_statement(),
                _ => {
                    // Stray close-delimiter or other token in text position.
                    let t = self.tokens[self.pos];
                    self.builder.token(t.kind, t.text);
                    self.offset += t.text.len();
                    self.pos += 1;
                }
            }
        }
        self.builder.finish_node();
    }

    fn parse_interpolation(&mut self) {
        self.builder.start_node(Interpolation);
        self.bump(); // {{ or {{-
        if !self.at(CloseExpr) && !self.at(CloseExprTrim) {
            self.parse_expr();
        }
        if !self.eat(CloseExpr) {
            self.expect(CloseExprTrim);
        }
        self.finish();
    }

    /// True if the next thing is `{%[-]` immediately followed by one of `kws`.
    fn at_stmt_kw(&self, kws: &[SyntaxKind]) -> bool {
        matches!(self.nth(0), Some(OpenStmt | OpenStmtTrim)) && matches!(self.nth(1), Some(k) if kws.contains(&k))
    }

    fn parse_statement(&mut self) {
        let kw = self.nth(1);
        match kw {
            Some(IfKw) => self.parse_if(),
            Some(ForKw) => self.parse_for(),
            Some(SetKw) => self.parse_set(),
            Some(BlockKw) => self.parse_block(),
            Some(MacroKw) => self.parse_macro(),
            Some(ExtendsKw) => self.parse_simple_stmt(ExtendsStmt),
            Some(IncludeKw) => self.parse_simple_stmt(IncludeStmt),
            Some(ImportKw) => self.parse_simple_stmt(ImportStmt),
            Some(BreakKw) => self.parse_keyword_stmt(BreakStmt),
            Some(ContinueKw) => self.parse_keyword_stmt(ContinueStmt),
            _ => {
                // Unknown / stray statement — consume the tag so we make progress.
                self.builder.start_node(Statement);
                self.open_tag();
                self.close_tag();
                self.finish();
            }
        }
    }

    /// Consume `{%` / `{%-` plus the keyword token.
    fn open_tag(&mut self) {
        self.bump(); // {% or {%-
        self.bump(); // keyword
    }

    /// Consume `%}` / `-%}`.
    fn close_tag(&mut self) {
        if !self.eat(CloseStmt) {
            self.expect(CloseStmtTrim);
        }
    }

    fn parse_body(&mut self, terminators: &[SyntaxKind]) {
        self.start(Body);
        while self.pos < self.tokens.len() {
            if self.at_stmt_kw(terminators) {
                break;
            }
            match self.tokens[self.pos].kind {
                Whitespace | Comment | Text => {
                    let t = self.tokens[self.pos];
                    self.builder.token(t.kind, t.text);
                    self.offset += t.text.len();
                    self.pos += 1;
                }
                OpenExpr | OpenExprTrim => self.parse_interpolation(),
                OpenStmt | OpenStmtTrim => self.parse_statement(),
                _ => {
                    let t = self.tokens[self.pos];
                    self.builder.token(t.kind, t.text);
                    self.offset += t.text.len();
                    self.pos += 1;
                }
            }
        }
        self.finish();
    }

    fn parse_if(&mut self) {
        self.start(IfStmt);
        self.open_tag(); // {% if
        self.parse_expr();
        self.close_tag();
        self.parse_body(&[ElifKw, ElseKw, EndifKw]);
        while self.at_stmt_kw(&[ElifKw]) {
            self.start(ElifClause);
            self.open_tag(); // {% elif
            self.parse_expr();
            self.close_tag();
            self.parse_body(&[ElifKw, ElseKw, EndifKw]);
            self.finish();
        }
        if self.at_stmt_kw(&[ElseKw]) {
            self.start(ElseClause);
            self.open_tag(); // {% else
            self.close_tag();
            self.parse_body(&[EndifKw]);
            self.finish();
        }
        if self.at_stmt_kw(&[EndifKw]) {
            self.open_tag();
            self.close_tag();
        } else {
            self.error("unclosed {% if %} (expected {% endif %})".into());
        }
        self.finish();
    }

    fn parse_for(&mut self) {
        self.start(ForStmt);
        self.open_tag(); // {% for
        self.expect(Ident); // loop var (could be `a, b` — accept a comma chain)
        while self.eat(Comma) {
            self.expect(Ident);
        }
        self.expect(InKw);
        self.parse_expr();
        self.close_tag();
        self.parse_body(&[ElseKw, EndforKw]);
        if self.at_stmt_kw(&[ElseKw]) {
            self.start(ElseClause);
            self.open_tag();
            self.close_tag();
            self.parse_body(&[EndforKw]);
            self.finish();
        }
        if self.at_stmt_kw(&[EndforKw]) {
            self.open_tag();
            self.close_tag();
        } else {
            self.error("unclosed {% for %} (expected {% endfor %})".into());
        }
        self.finish();
    }

    fn parse_set(&mut self) {
        self.start(SetStmt);
        self.open_tag(); // {% set
        self.expect(Ident);
        if self.eat(Assign) {
            // `{% set x = expr %}`
            self.parse_expr();
            self.close_tag();
        } else {
            // Block form: `{% set x %} … {% endset %}`
            self.close_tag();
            self.parse_body(&[EndsetKw]);
            if self.at_stmt_kw(&[EndsetKw]) {
                self.open_tag();
                self.close_tag();
            } else {
                self.error("unclosed {% set %} block (expected {% endset %})".into());
            }
        }
        self.finish();
    }

    fn parse_block(&mut self) {
        self.start(BlockStmt);
        self.open_tag(); // {% block
        self.expect(Ident);
        self.close_tag();
        self.parse_body(&[EndblockKw]);
        if self.at_stmt_kw(&[EndblockKw]) {
            self.open_tag();
            // optional trailing name before %}
            self.eat(Ident);
            self.close_tag();
        } else {
            self.error("unclosed {% block %} (expected {% endblock %})".into());
        }
        self.finish();
    }

    fn parse_macro(&mut self) {
        self.start(MacroStmt);
        self.open_tag(); // {% macro
        self.expect(Ident);
        self.parse_param_list();
        self.close_tag();
        self.parse_body(&[EndmacroKw]);
        if self.at_stmt_kw(&[EndmacroKw]) {
            self.open_tag();
            self.close_tag();
        } else {
            self.error("unclosed {% macro %} (expected {% endmacro %})".into());
        }
        self.finish();
    }

    fn parse_param_list(&mut self) {
        self.start(ParamList);
        self.expect(LParen);
        while !self.at(RParen) && !self.at_close_tag() && !self.at_end() {
            self.start(Param);
            self.expect(Ident);
            if self.eat(Assign) {
                self.parse_expr();
            }
            self.finish();
            if !self.eat(Comma) {
                break;
            }
        }
        self.expect(RParen);
        self.finish();
    }

    fn at_close_tag(&self) -> bool {
        self.at(CloseStmt) || self.at(CloseStmtTrim)
    }

    /// `{% break %}` / `{% continue %}` — keyword-only statements.
    fn parse_keyword_stmt(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.open_tag();
        self.close_tag();
        self.finish();
    }

    /// `{% extends/include/import EXPR [as name] %}`
    fn parse_simple_stmt(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.open_tag();
        if !self.at_close_tag() && !self.at_end() {
            self.parse_expr();
            if self.eat(AsKw) {
                self.expect(Ident);
            }
        }
        self.close_tag();
        self.finish();
    }

    // ----- expressions (precedence climbing) -----

    fn parse_expr(&mut self) {
        self.parse_ternary();
    }

    fn parse_ternary(&mut self) {
        let cp = self.builder.checkpoint();
        self.parse_or();
        if self.at(IfKw) {
            self.start_at(cp, TernaryExpr);
            self.bump(); // if
            self.parse_or();
            if self.eat(ElseKw) {
                self.parse_expr();
            }
            self.finish();
        }
    }

    /// Left-associative binary level: each `kinds` op wraps the running expr.
    fn parse_binary(&mut self, kinds: &[SyntaxKind], next: fn(&mut Self)) {
        let cp = self.builder.checkpoint();
        next(self);
        while self.nth(0).is_some_and(|k| kinds.contains(&k)) {
            self.start_at(cp, BinaryExpr);
            self.bump(); // operator
            next(self);
            self.finish();
        }
    }

    fn parse_or(&mut self) {
        self.parse_binary(&[OrKw], Self::parse_and);
    }

    fn parse_and(&mut self) {
        self.parse_binary(&[AndKw], Self::parse_not);
    }

    fn parse_not(&mut self) {
        if self.at(NotKw) {
            self.start(UnaryExpr);
            self.bump();
            self.parse_not();
            self.finish();
        } else {
            self.parse_comparison();
        }
    }

    fn parse_comparison(&mut self) {
        let cp = self.builder.checkpoint();
        self.parse_add();
        loop {
            match self.nth(0) {
                Some(EqEq | Neq | Lt | Gt | Le | Ge) => {
                    self.start_at(cp, BinaryExpr);
                    self.bump();
                    self.parse_add();
                    self.finish();
                }
                Some(InKw) => {
                    self.start_at(cp, BinaryExpr);
                    self.bump();
                    self.parse_add();
                    self.finish();
                }
                Some(NotKw) if self.nth(1) == Some(InKw) => {
                    self.start_at(cp, BinaryExpr);
                    self.bump(); // not
                    self.bump(); // in
                    self.parse_add();
                    self.finish();
                }
                Some(IsKw) => {
                    self.start_at(cp, TestExpr);
                    self.bump(); // is
                    self.eat(NotKw); // optional `not`
                    // Test name is usually an Ident, but `none` lexes as a keyword.
                    if !self.eat(Ident) && !self.eat(NoneKw) {
                        self.error("expected a test name".into());
                    }
                    // optional test args `(...)`
                    if self.at(LParen) {
                        self.parse_arg_list();
                    }
                    self.finish();
                }
                _ => break,
            }
        }
    }

    fn parse_add(&mut self) {
        self.parse_binary(&[Plus, Minus, Tilde], Self::parse_mul);
    }

    fn parse_mul(&mut self) {
        self.parse_binary(&[Star, Slash, SlashSlash, Percent], Self::parse_unary);
    }

    fn parse_unary(&mut self) {
        if self.at(Minus) {
            self.start(UnaryExpr);
            self.bump();
            self.parse_unary();
            self.finish();
        } else {
            self.parse_power();
        }
    }

    fn parse_power(&mut self) {
        let cp = self.builder.checkpoint();
        self.parse_filter();
        if self.at(StarStar) {
            self.start_at(cp, BinaryExpr);
            self.bump();
            self.parse_unary(); // right-assoc-ish
            self.finish();
        }
    }

    fn parse_filter(&mut self) {
        let cp = self.builder.checkpoint();
        self.parse_postfix();
        while self.at(Pipe) {
            self.start_at(cp, FilterExpr);
            self.bump(); // |
            self.expect(Ident); // filter name
            if self.at(LParen) {
                self.parse_arg_list();
            }
            self.finish();
        }
    }

    fn parse_postfix(&mut self) {
        let cp = self.builder.checkpoint();
        self.parse_primary();
        loop {
            match self.nth(0) {
                Some(Dot) => {
                    self.start_at(cp, FieldExpr);
                    self.bump(); // .
                    self.expect(Ident);
                    self.finish();
                }
                Some(LBracket) => {
                    // index or slice
                    self.start_at(cp, IndexExpr);
                    self.bump(); // [
                    self.parse_subscript();
                    self.expect(RBracket);
                    self.finish();
                }
                Some(LParen) => {
                    self.start_at(cp, CallExpr);
                    self.parse_arg_list();
                    self.finish();
                }
                Some(Question) => {
                    self.start_at(cp, OptionalExpr);
                    self.bump(); // ?
                    self.finish();
                }
                _ => break,
            }
        }
    }

    /// Inside `[ … ]`: an index `expr`, or a slice `expr? : expr?`.
    fn parse_subscript(&mut self) {
        if !self.at(Colon) {
            self.parse_expr();
        }
        if self.at(Colon) {
            // turn the enclosing node into a slice by tagging a SliceExpr child marker:
            // we simply consume `:` and an optional end expr; the IndexExpr node holding a
            // Colon child is recognised as a slice during lowering.
            self.bump(); // :
            if !self.at(RBracket) {
                self.parse_expr();
            }
        }
    }

    fn parse_arg_list(&mut self) {
        self.start(ArgList);
        self.expect(LParen);
        while !self.at(RParen) && !self.at_end() {
            // kwarg = `ident = expr`, detected by peeking `ident =`.
            if self.nth(0) == Some(Ident) && self.nth(1) == Some(Assign) {
                self.start(KwArg);
                self.bump(); // ident
                self.bump(); // =
                self.parse_expr();
                self.finish();
            } else {
                self.start(Arg);
                self.parse_expr();
                self.finish();
            }
            if !self.eat(Comma) {
                break;
            }
        }
        self.expect(RParen);
        self.finish();
    }

    fn parse_primary(&mut self) {
        match self.nth(0) {
            Some(Int | Float | Str | True | False | NoneKw) => {
                self.start(Literal);
                self.bump();
                self.finish();
            }
            Some(Ident) if self.nth(1) == Some(ColonColon) => {
                // `namespace::macro(args)` explicit macro call.
                self.start(MacroCallExpr);
                self.bump(); // namespace
                self.bump(); // ::
                self.expect(Ident); // macro name
                if self.at(LParen) {
                    self.parse_arg_list();
                }
                self.finish();
            }
            Some(Ident) => {
                self.start(VarRef);
                self.bump();
                self.finish();
            }
            Some(LParen) => {
                self.start(ParenExpr);
                self.bump(); // (
                self.parse_expr();
                self.expect(RParen);
                self.finish();
            }
            Some(LBracket) => self.parse_list(),
            Some(LBrace) => self.parse_dict(),
            Some(Minus | NotKw) => {
                // unary fell through (e.g. `-x` reached here): handle gracefully.
                self.start(UnaryExpr);
                self.bump();
                self.parse_primary();
                self.finish();
            }
            _ => self.error("expected an expression".into()),
        }
    }

    fn parse_list(&mut self) {
        self.start(ListLit);
        self.expect(LBracket);
        while !self.at(RBracket) && !self.at_end() {
            self.parse_expr();
            if !self.eat(Comma) {
                break;
            }
        }
        self.expect(RBracket);
        self.finish();
    }

    fn parse_dict(&mut self) {
        self.start(DictLit);
        self.expect(LBrace);
        while !self.at(RBrace) && !self.at_end() {
            self.parse_expr(); // key
            self.expect(Colon);
            self.parse_expr(); // value
            if !self.eat(Comma) {
                break;
            }
        }
        self.expect(RBrace);
        self.finish();
    }

    fn start_at(&mut self, cp: cstree::build::Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(cp, kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render the tree shape compactly for snapshotting.
    fn tree(src: &str) -> String {
        let p = parse(src);
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        // Lossless check: the resolved tree Displays as the source.
        assert_eq!(p.syntax().to_string(), src, "tree not lossless");
        format!("{:#?}", p.syntax())
    }

    #[test]
    fn lossless_roundtrip() {
        for src in [
            "hello",
            "a {{ x }} b",
            "{%- if a is not defined -%}x{%- endif -%}",
            "{{ macros.youtube_embed(id, alt=x) }}",
            "{{ a.b.c | upper | safe }}",
            "{{ items[:3] }}",
            "{{ a + b * c ~ \"x\" }}",
            "{{ a if cond else b }}",
            "{% for x in xs %}{{ x }}{% else %}none{% endfor %}",
            "{% set n = 1 %}",
            "{% block content %}hi{% endblock %}",
            "{% macro m(a, b=1) %}{{ a }}{% endmacro %}",
            "{{ latest_article.reading_time }}",
            "{{ f(a.b, k=c.d) }}",
        ] {
            let p = parse(src);
            assert_eq!(p.syntax().to_string(), src, "not lossless: {src:?}");
            assert!(p.errors.is_empty(), "errors for {src:?}: {:?}", p.errors);
        }
    }

    #[test]
    fn field_access_in_call_arg() {
        // The exact construct the old parser choked on.
        let p = parse("{{ render_reading_time(latest_article.reading_time) }}");
        assert!(p.errors.is_empty(), "errors: {:?}", p.errors);
    }

    #[test]
    fn snapshot_binary_precedence() {
        // a + b * c  →  Binary(a, +, Binary(b, *, c))
        let t = tree("{{ a + b * c }}");
        assert!(t.contains("BinaryExpr"), "{t}");
    }
}
