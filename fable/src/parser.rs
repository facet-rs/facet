//! Recursive-descent parser for the initial Fable grammar.

use cstree::build::GreenNodeBuilder;
use cstree::syntax::ResolvedNode;

use crate::SyntaxKind::{self, *};
use crate::lexer::{Lexeme, lex};

/// A parse error with the byte offset where it was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

/// A parsed source file.
#[derive(Debug, Clone)]
pub struct Parse {
    root: ResolvedNode<SyntaxKind>,
    errors: Vec<ParseError>,
}

impl Parse {
    /// The resolved concrete syntax tree root.
    #[must_use]
    pub fn syntax(&self) -> &ResolvedNode<SyntaxKind> {
        &self.root
    }

    /// Non-fatal parse errors collected while recovering.
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
}

/// Parse Fable source into a lossless CST.
#[must_use]
pub fn parse(src: &str) -> Parse {
    let tokens = lex(src);
    let mut parser = Parser {
        tokens,
        pos: 0,
        offset: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
    };
    parser.parse_root();
    let (green, cache) = parser.builder.finish();
    let interner = cache.unwrap().into_interner().unwrap();
    let root = ResolvedNode::new_root_with_resolver(green, interner);
    Parse {
        root,
        errors: parser.errors,
    }
}

struct Parser<'src> {
    tokens: Vec<Lexeme<'src>>,
    pos: usize,
    offset: usize,
    builder: GreenNodeBuilder<'static, 'static, SyntaxKind>,
    errors: Vec<ParseError>,
}

impl<'src> Parser<'src> {
    fn nth(&self, n: usize) -> Option<SyntaxKind> {
        let mut seen = 0usize;
        for token in &self.tokens[self.pos..] {
            if token.kind.is_trivia() {
                continue;
            }
            if seen == n {
                return Some(token.kind);
            }
            seen += 1;
        }
        None
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.nth(0) == Some(kind)
    }

    fn at_end(&self) -> bool {
        self.nth(0).is_none()
    }

    fn eat_trivia(&mut self) {
        while let Some(token) = self.tokens.get(self.pos) {
            if !token.kind.is_trivia() {
                break;
            }
            self.builder.token(token.kind, token.text);
            self.offset += token.text.len();
            self.pos += 1;
        }
    }

    fn bump(&mut self) {
        self.eat_trivia();
        if let Some(token) = self.tokens.get(self.pos) {
            self.builder.token(token.kind, token.text);
            self.offset += token.text.len();
            self.pos += 1;
        }
    }

    fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: SyntaxKind) {
        if !self.eat(kind) {
            self.error(format!("expected {kind:?}, found {:?}", self.nth(0)));
        }
    }

    fn error(&mut self, message: String) {
        self.eat_trivia();
        self.errors.push(ParseError {
            message,
            offset: self.offset,
        });
        if self.tokens.get(self.pos).is_some() {
            self.builder.start_node(Error);
            self.bump();
            self.builder.finish_node();
        }
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        self.builder.start_node(kind);
    }

    fn start_at(&mut self, checkpoint: cstree::build::Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(checkpoint, kind);
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }

    fn parse_root(&mut self) {
        self.builder.start_node(Root);
        while !self.at_end() {
            if self.at(RBrace) {
                self.error("unexpected closing brace".into());
            } else {
                self.parse_stmt();
            }
        }
        self.eat_trivia();
        self.builder.finish_node();
    }

    fn parse_stmt(&mut self) {
        if self.at(IfKw) {
            self.parse_if_stmt();
        } else {
            self.parse_assignment_or_expr_stmt();
        }
    }

    fn parse_if_stmt(&mut self) {
        self.start(IfStmt);
        self.expect(IfKw);
        self.parse_expr();
        self.parse_block();

        if self.at(ElseKw) {
            self.start(ElseClause);
            self.bump();
            if self.at(IfKw) {
                self.parse_if_stmt();
            } else {
                self.parse_block();
            }
            self.finish();
        }

        self.finish();
    }

    fn parse_block(&mut self) {
        self.start(Block);
        self.expect(LBrace);
        while !self.at(RBrace) && !self.at_end() {
            self.parse_stmt();
        }
        self.expect(RBrace);
        self.finish();
    }

    fn parse_assignment_or_expr_stmt(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_expr();

        if self.at(Assign) {
            self.start_at(checkpoint, AssignStmt);
            self.bump();
            self.parse_expr();
            self.eat(Semicolon);
            self.finish();
        } else {
            self.start_at(checkpoint, ExprStmt);
            self.eat(Semicolon);
            self.finish();
        }
    }

    fn parse_expr(&mut self) {
        self.parse_or();
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
            self.parse_equality();
        }
    }

    fn parse_equality(&mut self) {
        self.parse_binary(&[EqEq, Neq], Self::parse_comparison);
    }

    fn parse_comparison(&mut self) {
        self.parse_binary(&[Lt, Gt, Le, Ge], Self::parse_add);
    }

    fn parse_add(&mut self) {
        self.parse_binary(&[Plus, Minus], Self::parse_unary);
    }

    fn parse_binary(&mut self, kinds: &[SyntaxKind], next: fn(&mut Self)) {
        let checkpoint = self.builder.checkpoint();
        next(self);
        while self.nth(0).is_some_and(|kind| kinds.contains(&kind)) {
            self.start_at(checkpoint, BinaryExpr);
            self.bump();
            next(self);
            self.finish();
        }
    }

    fn parse_unary(&mut self) {
        if self.at(Minus) {
            self.start(UnaryExpr);
            self.bump();
            self.parse_unary();
            self.finish();
        } else {
            self.parse_postfix();
        }
    }

    fn parse_postfix(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_primary();

        loop {
            match self.nth(0) {
                Some(Dot) => {
                    self.start_at(checkpoint, FieldExpr);
                    self.bump();
                    self.expect(Ident);
                    self.finish();
                }
                Some(LBracket) => {
                    self.start_at(checkpoint, IndexExpr);
                    self.bump();
                    self.parse_expr();
                    self.expect(RBracket);
                    self.finish();
                }
                Some(LParen) => {
                    self.start_at(checkpoint, CallExpr);
                    self.parse_arg_list();
                    self.finish();
                }
                _ => break,
            }
        }
    }

    fn parse_arg_list(&mut self) {
        self.start(ArgList);
        self.expect(LParen);
        while !self.at(RParen) && !self.at_end() {
            self.start(Arg);
            self.parse_expr();
            self.finish();
            if !self.eat(Comma) {
                break;
            }
        }
        self.expect(RParen);
        self.finish();
    }

    fn parse_primary(&mut self) {
        match self.nth(0) {
            Some(Int | Float | Str | True | False | Null) => {
                self.start(Literal);
                self.bump();
                self.finish();
            }
            Some(Ident) => {
                self.start(VarRef);
                self.bump();
                self.finish();
            }
            Some(LParen) => {
                self.start(ParenExpr);
                self.bump();
                self.parse_expr();
                self.expect(RParen);
                self.finish();
            }
            _ => self.error("expected an expression".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(src: &str) -> String {
        let parsed = parse(src);
        assert!(
            parsed.errors().is_empty(),
            "unexpected errors: {:?}",
            parsed.errors()
        );
        format!("{:#?}", parsed.syntax())
    }

    #[test]
    fn parses_assignments_and_paths() {
        let tree = tree(r#"root.user.name = "Ada";"#);
        assert!(tree.contains("AssignStmt"));
        assert!(tree.contains("FieldExpr"));
        assert!(tree.contains("Str"));
    }

    #[test]
    fn parses_if_else_blocks() {
        let tree = tree(
            r#"
if root.user.age >= 18 {
    root.user.adult = true
} else {
    root.user.adult = false
}
"#,
        );
        assert!(tree.contains("IfStmt"));
        assert!(tree.contains("ElseClause"));
        assert!(tree.contains("BinaryExpr"));
    }

    #[test]
    fn parses_call_and_index_expressions() {
        let tree = tree("root.total = add(root.items[0].price, 3)");
        assert!(tree.contains("CallExpr"));
        assert!(tree.contains("IndexExpr"));
        assert!(tree.contains("ArgList"));
    }

    #[test]
    fn recovers_from_missing_expression() {
        let parsed = parse("root.age = ; root.ok = true");
        assert_eq!(parsed.errors().len(), 1);
        assert_eq!(parsed.errors()[0].message, "expected an expression");
        assert_eq!(parsed.syntax().kind(), Root);
    }
}
