//! Parser for the template language
//!
//! Transforms token stream into AST with full span information.

use super::ast::*;
use super::error::{SyntaxError, TemplateSource};
use super::lexer::{Lexer, Token, TokenKind};
use miette::Result;
use std::sync::Arc;

/// Parsed call arguments: (positional args, keyword args)
type CallArgs = (Vec<Expr>, Vec<(Ident, Expr)>);

/// Parser state
pub struct Parser {
    lexer: Lexer,
    source: TemplateSource,
    /// Current token
    current: Token,
    /// Previous token (for span info)
    previous: Token,
    /// Pending token (for lookahead pushback)
    pending: Option<Token>,
}

impl Parser {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        let source_str: String = source.into();
        let source_arc = Arc::new(source_str.clone());
        let template_source = TemplateSource::new(name, source_str);

        let mut lexer = Lexer::new(source_arc);
        let current = lexer.next_token();
        Self {
            lexer,
            source: template_source,
            current: current.clone(),
            previous: current,
            pending: None,
        }
    }

    /// Parse the full template
    pub fn parse(mut self) -> Result<Template> {
        let start = self.current.span;
        let body = self.parse_body(&[])?;
        let end = self.previous.span;

        Ok(Template {
            body,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        })
    }

    /// Parse template body until we hit a terminator
    fn parse_body(&mut self, terminators: &[TokenKind]) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();

        loop {
            if self.is_at_end() {
                break;
            }

            // Check for terminators - they come after {% so we need to peek
            if self.check(&TokenKind::TagOpen) {
                // Look at what comes after {%
                let next = self
                    .pending
                    .take()
                    .unwrap_or_else(|| self.lexer.next_token());
                let is_terminator = terminators
                    .iter()
                    .any(|t| std::mem::discriminant(&next.kind) == std::mem::discriminant(t));

                if is_terminator {
                    // Save the terminator token for later - caller will consume it
                    self.pending = Some(next);
                    break;
                }

                // Not a terminator - we need to continue parsing as a tag
                // But we already consumed the next token, so save the current and restore
                let saved_current = std::mem::replace(&mut self.current, next);
                self.previous = saved_current;
                // Now current is the token after {% (like If, For, etc.)
                // We need to parse the tag body
                let node = self.parse_tag_body()?;
                nodes.push(node);
                continue;
            }

            nodes.push(self.parse_node()?);
        }

        Ok(nodes)
    }

    /// Parse tag body after we've seen {% and consumed the keyword
    fn parse_tag_body(&mut self) -> Result<Node> {
        let start = self.previous.span; // The {% token

        match &self.current.kind {
            TokenKind::If => self.parse_if(start),
            TokenKind::For => self.parse_for(start),
            TokenKind::Block => self.parse_block(start),
            TokenKind::Extends => self.parse_extends(start),
            TokenKind::Include => self.parse_include(start),
            TokenKind::Import => self.parse_import(start),
            TokenKind::Macro => self.parse_macro(start),
            TokenKind::Set => self.parse_set(start),
            _ => {
                let span = self.current.span;
                let found = format!("{:?}", self.current.kind);
                Err(SyntaxError {
                    found,
                    expected: "if, for, block, extends, include, import, macro, or set".to_string(),
                    span,
                    src: self.source.named_source(),
                })?
            }
        }
    }

    /// Parse a single node
    fn parse_node(&mut self) -> Result<Node> {
        match &self.current.kind {
            TokenKind::Text(text) => {
                let text = text.clone();
                let span = self.current.span;
                self.advance();
                Ok(Node::Text(TextNode { text, span }))
            }
            TokenKind::ExprOpen => self.parse_print(),
            TokenKind::TagOpen => self.parse_tag(),
            _ => {
                let span = self.current.span;
                let found = format!("{:?}", self.current.kind);
                Err(SyntaxError {
                    found,
                    expected: "text, {{ or {%".to_string(),
                    span,
                    src: self.source.named_source(),
                })?
            }
        }
    }

    /// Parse expression print: {{ expr }}
    fn parse_print(&mut self) -> Result<Node> {
        let start = self.current.span;
        self.expect(&TokenKind::ExprOpen)?;

        let expr = self.parse_expr()?;

        self.expect(&TokenKind::ExprClose)?;
        let end = self.previous.span;

        Ok(Node::Print(PrintNode {
            expr,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse a tag: {% ... %}
    fn parse_tag(&mut self) -> Result<Node> {
        let start = self.current.span;
        self.expect(&TokenKind::TagOpen)?;

        let node = match &self.current.kind {
            TokenKind::If => self.parse_if(start)?,
            TokenKind::For => self.parse_for(start)?,
            TokenKind::Block => self.parse_block(start)?,
            TokenKind::Extends => self.parse_extends(start)?,
            TokenKind::Include => self.parse_include(start)?,
            TokenKind::Import => self.parse_import(start)?,
            TokenKind::Macro => self.parse_macro(start)?,
            TokenKind::Set => self.parse_set(start)?,
            _ => {
                let span = self.current.span;
                let found = format!("{:?}", self.current.kind);
                return Err(SyntaxError {
                    found,
                    expected: "if, for, block, extends, include, import, macro, or set".to_string(),
                    span,
                    src: self.source.named_source(),
                })?;
            }
        };

        Ok(node)
    }

    /// Parse if statement
    fn parse_if(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::If)?;

        let condition = self.parse_expr()?;
        self.expect(&TokenKind::TagClose)?;

        let then_body = self.parse_body(&[TokenKind::Elif, TokenKind::Else, TokenKind::Endif])?;

        // Parse elif branches - peek at pending to check what follows TagOpen
        let mut elif_branches = Vec::new();
        while self.check(&TokenKind::TagOpen)
            && self
                .pending
                .as_ref()
                .is_some_and(|t| matches!(t.kind, TokenKind::Elif))
        {
            let elif_start = self.current.span;
            self.advance(); // consume TagOpen
            self.advance(); // consume Elif (from pending)
            let elif_cond = self.parse_expr()?;
            self.expect(&TokenKind::TagClose)?;

            let elif_body =
                self.parse_body(&[TokenKind::Elif, TokenKind::Else, TokenKind::Endif])?;

            let elif_end = self.previous.span;
            elif_branches.push(ElifBranch {
                condition: elif_cond,
                body: elif_body,
                span: span(
                    elif_start.offset(),
                    elif_end.offset() + elif_end.len() - elif_start.offset(),
                ),
            });
        }

        // Parse else - peek at pending to check
        let else_body = if self.check(&TokenKind::TagOpen)
            && self
                .pending
                .as_ref()
                .is_some_and(|t| matches!(t.kind, TokenKind::Else))
        {
            self.advance(); // consume TagOpen
            self.advance(); // consume Else (from pending)
            self.expect(&TokenKind::TagClose)?;
            Some(self.parse_body(&[TokenKind::Endif])?)
        } else {
            None
        };

        // Expect {% endif %}
        self.expect(&TokenKind::TagOpen)?;
        self.expect(&TokenKind::Endif)?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::If(IfNode {
            condition,
            then_body,
            elif_branches,
            else_body,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse for loop
    fn parse_for(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::For)?;

        let target = self.parse_target()?;

        self.expect(&TokenKind::In)?;

        let iter = self.parse_expr()?;
        self.expect(&TokenKind::TagClose)?;

        let body = self.parse_body(&[TokenKind::Else, TokenKind::Endfor])?;

        // Parse else - peek at pending to see if it's else or endfor
        let else_body = if self.check(&TokenKind::TagOpen) {
            if self
                .pending
                .as_ref()
                .is_some_and(|t| matches!(t.kind, TokenKind::Else))
            {
                self.advance(); // consume TagOpen
                self.advance(); // consume Else (from pending)
                self.expect(&TokenKind::TagClose)?;
                Some(self.parse_body(&[TokenKind::Endfor])?)
            } else {
                None
            }
        } else {
            None
        };

        // Expect {% endfor %}
        self.expect(&TokenKind::TagOpen)?;
        self.expect(&TokenKind::Endfor)?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::For(ForNode {
            target,
            iter,
            body,
            else_body,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse loop target
    fn parse_target(&mut self) -> Result<Target> {
        let start = self.current.span;

        let first_name = self.expect_ident()?;

        if self.check(&TokenKind::Comma) {
            // Tuple unpacking
            let mut names = vec![(first_name.name, first_name.span)];
            while self.check(&TokenKind::Comma) {
                self.advance();
                let ident = self.expect_ident()?;
                names.push((ident.name, ident.span));
            }
            let end = self.previous.span;
            Ok(Target::Tuple {
                names,
                span: span(start.offset(), end.offset() + end.len() - start.offset()),
            })
        } else {
            Ok(Target::Single {
                name: first_name.name,
                span: first_name.span,
            })
        }
    }

    /// Parse block definition
    fn parse_block(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::Block)?;

        let name = self.expect_ident()?;
        self.expect(&TokenKind::TagClose)?;

        let body = self.parse_body(&[TokenKind::Endblock])?;

        self.expect(&TokenKind::TagOpen)?;
        self.expect(&TokenKind::Endblock)?;
        // Optional block name after endblock (e.g., {% endblock title %})
        if matches!(&self.current.kind, TokenKind::Ident(_)) {
            self.advance();
        }
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::Block(BlockNode {
            name,
            body,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse extends
    fn parse_extends(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::Extends)?;

        let path = self.expect_string()?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::Extends(ExtendsNode {
            path,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse include
    fn parse_include(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::Include)?;

        let path = self.expect_string()?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::Include(IncludeNode {
            path,
            context: None,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse set statement: {% set var = expr %}
    fn parse_set(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::Set)?;

        let name = self.expect_ident()?;
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr()?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::Set(SetNode {
            name,
            value,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse import statement: {% import "path" as name %}
    fn parse_import(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::Import)?;

        let path = self.expect_string()?;
        self.expect(&TokenKind::As)?;
        let alias = self.expect_ident()?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::Import(ImportNode {
            path,
            alias,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse macro definition: {% macro name(args) %}...{% endmacro %}
    fn parse_macro(&mut self, start: Span) -> Result<Node> {
        self.expect(&TokenKind::Macro)?;

        let name = self.expect_ident()?;

        // Parse parameters
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_macro_params()?;
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::TagClose)?;

        // Parse body
        let body = self.parse_body(&[TokenKind::Endmacro])?;

        // Expect {% endmacro %}
        self.expect(&TokenKind::TagOpen)?;
        self.expect(&TokenKind::Endmacro)?;
        self.expect(&TokenKind::TagClose)?;

        let end = self.previous.span;

        Ok(Node::Macro(MacroNode {
            name,
            params,
            body,
            span: span(start.offset(), end.offset() + end.len() - start.offset()),
        }))
    }

    /// Parse macro parameters with optional defaults
    fn parse_macro_params(&mut self) -> Result<Vec<MacroParam>> {
        let mut params = Vec::new();

        if !self.check(&TokenKind::RParen) {
            loop {
                let name = self.expect_ident()?;
                let default = if self.check(&TokenKind::Assign) {
                    self.advance();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                params.push(MacroParam { name, default });

                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
                if self.check(&TokenKind::RParen) {
                    break;
                }
            }
        }

        Ok(params)
    }

    // ========================================================================
    // Expression parsing (precedence climbing)
    // ========================================================================

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr> {
        let value = self.parse_or()?;

        if self.check(&TokenKind::If) {
            self.advance();
            let condition = self.parse_or()?;
            self.expect(&TokenKind::Else)?;
            let otherwise = self.parse_ternary()?;

            let span = span(
                value.span().offset(),
                otherwise.span().offset() + otherwise.span().len() - value.span().offset(),
            );

            Ok(Expr::Ternary(TernaryExpr {
                value: Box::new(value),
                condition: Box::new(condition),
                otherwise: Box::new(otherwise),
                span,
            }))
        } else {
            Ok(value)
        }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;

        while self.check(&TokenKind::Or) {
            self.advance();
            let right = self.parse_and()?;
            let span = span(
                left.span().offset(),
                right.span().offset() + right.span().len() - left.span().offset(),
            );
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
                span,
            });
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_not()?;

        while self.check(&TokenKind::And) {
            self.advance();
            let right = self.parse_not()?;
            let span = span(
                left.span().offset(),
                right.span().offset() + right.span().len() - left.span().offset(),
            );
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
                span,
            });
        }

        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr> {
        if self.check(&TokenKind::Not) {
            let start = self.current.span;
            self.advance();
            let expr = self.parse_not()?;
            let span = span(
                start.offset(),
                expr.span().offset() + expr.span().len() - start.offset(),
            );
            Ok(Expr::Unary(UnaryExpr {
                op: UnaryOp::Not,
                expr: Box::new(expr),
                span,
            }))
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut left = self.parse_add()?;

        loop {
            // Check for "is" test expressions
            if self.check(&TokenKind::Is) {
                self.advance();
                // Check for "is not"
                let negated = if self.check(&TokenKind::Not) {
                    self.advance();
                    true
                } else {
                    false
                };
                // Parse test name
                let test_name = self.expect_ident()?;
                // Parse optional args: test_name(arg1, arg2)
                let args = if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check(&TokenKind::RParen) {
                        args.push(self.parse_expr()?);
                        while self.check(&TokenKind::Comma) {
                            self.advance();
                            args.push(self.parse_expr()?);
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    args
                } else {
                    Vec::new()
                };
                let end = self.previous.span;
                let expr_span = span(
                    left.span().offset(),
                    end.offset() + end.len() - left.span().offset(),
                );
                left = Expr::Test(TestExpr {
                    expr: Box::new(left),
                    test_name,
                    args,
                    negated,
                    span: expr_span,
                });
                continue;
            }

            let op = match &self.current.kind {
                TokenKind::Eq => Some(BinaryOp::Eq),
                TokenKind::Ne => Some(BinaryOp::Ne),
                TokenKind::Lt => Some(BinaryOp::Lt),
                TokenKind::Le => Some(BinaryOp::Le),
                TokenKind::Gt => Some(BinaryOp::Gt),
                TokenKind::Ge => Some(BinaryOp::Ge),
                TokenKind::In => Some(BinaryOp::In),
                TokenKind::Not => {
                    // Check for "not in"
                    let saved = self.current.clone();
                    self.advance();
                    if self.check(&TokenKind::In) {
                        self.advance();
                        let right = self.parse_add()?;
                        let span = span(
                            left.span().offset(),
                            right.span().offset() + right.span().len() - left.span().offset(),
                        );
                        left = Expr::Binary(BinaryExpr {
                            left: Box::new(left),
                            op: BinaryOp::NotIn,
                            right: Box::new(right),
                            span,
                        });
                        continue;
                    } else {
                        self.current = saved;
                        break;
                    }
                }
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_add()?;
                let span = span(
                    left.span().offset(),
                    right.span().offset() + right.span().len() - left.span().offset(),
                );
                left = Expr::Binary(BinaryExpr {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut left = self.parse_mul()?;

        loop {
            let op = match &self.current.kind {
                TokenKind::Plus => Some(BinaryOp::Add),
                TokenKind::Minus => Some(BinaryOp::Sub),
                TokenKind::Tilde => Some(BinaryOp::Concat),
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_mul()?;
                let span = span(
                    left.span().offset(),
                    right.span().offset() + right.span().len() - left.span().offset(),
                );
                left = Expr::Binary(BinaryExpr {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;

        loop {
            let op = match &self.current.kind {
                TokenKind::Star => Some(BinaryOp::Mul),
                TokenKind::Slash => Some(BinaryOp::Div),
                TokenKind::DoubleSlash => Some(BinaryOp::FloorDiv),
                TokenKind::Percent => Some(BinaryOp::Mod),
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_unary()?;
                let span = span(
                    left.span().offset(),
                    right.span().offset() + right.span().len() - left.span().offset(),
                );
                left = Expr::Binary(BinaryExpr {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        let start = self.current.span;

        if self.check(&TokenKind::Minus) {
            self.advance();
            let expr = self.parse_unary()?;
            let span = span(
                start.offset(),
                expr.span().offset() + expr.span().len() - start.offset(),
            );
            Ok(Expr::Unary(UnaryExpr {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
                span,
            }))
        } else if self.check(&TokenKind::Plus) {
            self.advance();
            let expr = self.parse_unary()?;
            let span = span(
                start.offset(),
                expr.span().offset() + expr.span().len() - start.offset(),
            );
            Ok(Expr::Unary(UnaryExpr {
                op: UnaryOp::Pos,
                expr: Box::new(expr),
                span,
            }))
        } else {
            self.parse_power()
        }
    }

    fn parse_power(&mut self) -> Result<Expr> {
        let base = self.parse_filter()?;

        if self.check(&TokenKind::DoubleStar) {
            self.advance();
            let exp = self.parse_unary()?;
            let span = span(
                base.span().offset(),
                exp.span().offset() + exp.span().len() - base.span().offset(),
            );
            Ok(Expr::Binary(BinaryExpr {
                left: Box::new(base),
                op: BinaryOp::Pow,
                right: Box::new(exp),
                span,
            }))
        } else {
            Ok(base)
        }
    }

    fn parse_filter(&mut self) -> Result<Expr> {
        let mut expr = self.parse_postfix()?;

        while self.check(&TokenKind::Pipe) {
            self.advance();
            let filter = self.expect_ident()?;

            // Optional filter arguments (with kwargs support)
            let (args, kwargs) = if self.check(&TokenKind::LParen) {
                self.advance();
                let result = self.parse_call_args()?;
                self.expect(&TokenKind::RParen)?;
                result
            } else {
                (Vec::new(), Vec::new())
            };

            let span = span(
                expr.span().offset(),
                self.previous.span.offset() + self.previous.span.len() - expr.span().offset(),
            );

            expr = Expr::Filter(FilterExpr {
                expr: Box::new(expr),
                filter,
                args,
                kwargs,
                span,
            });
        }

        Ok(expr)
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let field = self.expect_ident()?;
                let span = span(
                    expr.span().offset(),
                    field.span.offset() + field.span.len() - expr.span().offset(),
                );
                expr = Expr::Field(FieldExpr {
                    base: Box::new(expr),
                    field,
                    span,
                });
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(&TokenKind::RBracket)?;
                let span = span(
                    expr.span().offset(),
                    self.previous.span.offset() + self.previous.span.len() - expr.span().offset(),
                );
                expr = Expr::Index(IndexExpr {
                    base: Box::new(expr),
                    index: Box::new(index),
                    span,
                });
            } else if self.check(&TokenKind::LParen) {
                self.advance();
                let (args, kwargs) = self.parse_call_args()?;
                self.expect(&TokenKind::RParen)?;
                let span = span(
                    expr.span().offset(),
                    self.previous.span.offset() + self.previous.span.len() - expr.span().offset(),
                );
                expr = Expr::Call(CallExpr {
                    func: Box::new(expr),
                    args,
                    kwargs,
                    span,
                });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.current.clone();

        match &token.kind {
            TokenKind::Int(v) => {
                let v = *v;
                self.advance();
                Ok(Expr::Literal(Literal::Int(IntLit {
                    value: v,
                    span: token.span,
                })))
            }
            TokenKind::Float(v) => {
                let v = *v;
                self.advance();
                Ok(Expr::Literal(Literal::Float(FloatLit {
                    value: v,
                    span: token.span,
                })))
            }
            TokenKind::String(v) => {
                let v = v.clone();
                self.advance();
                Ok(Expr::Literal(Literal::String(StringLit {
                    value: v,
                    span: token.span,
                })))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(BoolLit {
                    value: true,
                    span: token.span,
                })))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(BoolLit {
                    value: false,
                    span: token.span,
                })))
            }
            TokenKind::None => {
                self.advance();
                Ok(Expr::Literal(Literal::None(NoneLit { span: token.span })))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                let start_span = token.span;
                self.advance();

                // Check for macro call: namespace::macro_name(args)
                if self.check(&TokenKind::DoubleColon) {
                    self.advance(); // consume ::
                    let macro_name = self.expect_ident()?;
                    self.expect(&TokenKind::LParen)?;
                    let (args, kwargs) = self.parse_call_args()?;
                    self.expect(&TokenKind::RParen)?;

                    let end = self.previous.span;
                    Ok(Expr::MacroCall(MacroCallExpr {
                        namespace: Ident {
                            name,
                            span: start_span,
                        },
                        macro_name,
                        args,
                        kwargs,
                        span: span(
                            start_span.offset(),
                            end.offset() + end.len() - start_span.offset(),
                        ),
                    }))
                } else {
                    Ok(Expr::Var(Ident {
                        name,
                        span: token.span,
                    }))
                }
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::LBracket => {
                self.advance();
                let elements = self.parse_list_elements()?;
                self.expect(&TokenKind::RBracket)?;
                let span = span(
                    token.span.offset(),
                    self.previous.span.offset() + self.previous.span.len() - token.span.offset(),
                );
                Ok(Expr::Literal(Literal::List(ListLit { elements, span })))
            }
            TokenKind::LBrace => {
                self.advance();
                let entries = self.parse_dict_entries()?;
                self.expect(&TokenKind::RBrace)?;
                let span = span(
                    token.span.offset(),
                    self.previous.span.offset() + self.previous.span.len() - token.span.offset(),
                );
                Ok(Expr::Literal(Literal::Dict(DictLit { entries, span })))
            }
            _ => Err(SyntaxError {
                found: format!("{:?}", token.kind),
                expected: "expression".to_string(),
                span: token.span,
                src: self.source.named_source(),
            })?,
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>> {
        let mut args = Vec::new();

        if !self.check(&TokenKind::RParen) {
            args.push(self.parse_expr()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RParen) {
                    break;
                }
                args.push(self.parse_expr()?);
            }
        }

        Ok(args)
    }

    fn parse_call_args(&mut self) -> Result<CallArgs> {
        let mut args = Vec::new();
        let mut kwargs = Vec::new();

        if !self.check(&TokenKind::RParen) {
            loop {
                // Check for kwarg: name=value
                if let TokenKind::Ident(name) = &self.current.kind {
                    let name = name.clone();
                    let name_span = self.current.span;

                    // Peek to see if this is a kwarg
                    self.advance();
                    if self.check(&TokenKind::Assign) {
                        self.advance();
                        let value = self.parse_expr()?;
                        kwargs.push((
                            Ident {
                                name,
                                span: name_span,
                            },
                            value,
                        ));
                    } else {
                        // It's a positional arg that's a variable
                        // We already consumed the identifier, need to create expr
                        args.push(Expr::Var(Ident {
                            name,
                            span: name_span,
                        }));
                    }
                } else {
                    args.push(self.parse_expr()?);
                }

                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
                if self.check(&TokenKind::RParen) {
                    break;
                }
            }
        }

        Ok((args, kwargs))
    }

    fn parse_list_elements(&mut self) -> Result<Vec<Expr>> {
        let mut elements = Vec::new();

        if !self.check(&TokenKind::RBracket) {
            elements.push(self.parse_expr()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RBracket) {
                    break;
                }
                elements.push(self.parse_expr()?);
            }
        }

        Ok(elements)
    }

    fn parse_dict_entries(&mut self) -> Result<Vec<(Expr, Expr)>> {
        let mut entries = Vec::new();

        if !self.check(&TokenKind::RBrace) {
            let key = self.parse_expr()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            entries.push((key, value));

            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RBrace) {
                    break;
                }
                let key = self.parse_expr()?;
                self.expect(&TokenKind::Colon)?;
                let value = self.parse_expr()?;
                entries.push((key, value));
            }
        }

        Ok(entries)
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn advance(&mut self) {
        let next = self
            .pending
            .take()
            .unwrap_or_else(|| self.lexer.next_token());
        self.previous = std::mem::replace(&mut self.current, next);
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current.kind) == std::mem::discriminant(kind)
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current.kind, TokenKind::Eof)
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<()> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            Err(SyntaxError {
                found: format!("{:?}", self.current.kind),
                expected: format!("{kind:?}"),
                span: self.current.span,
                src: self.source.named_source(),
            })?
        }
    }

    fn expect_ident(&mut self) -> Result<Ident> {
        if let TokenKind::Ident(name) = &self.current.kind {
            let name = name.clone();
            let span = self.current.span;
            self.advance();
            Ok(Ident { name, span })
        } else {
            Err(SyntaxError {
                found: format!("{:?}", self.current.kind),
                expected: "identifier".to_string(),
                span: self.current.span,
                src: self.source.named_source(),
            })?
        }
    }

    fn expect_string(&mut self) -> Result<StringLit> {
        if let TokenKind::String(value) = &self.current.kind {
            let value = value.clone();
            let span = self.current.span;
            self.advance();
            Ok(StringLit { value, span })
        } else {
            Err(SyntaxError {
                found: format!("{:?}", self.current.kind),
                expected: "string".to_string(),
                span: self.current.span,
                src: self.source.named_source(),
            })?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Template> {
        Parser::new("test", s).parse()
    }

    #[test]
    fn test_parse_text() {
        let template = parse("Hello, world!").unwrap();
        assert_eq!(template.body.len(), 1);
        assert!(matches!(&template.body[0], Node::Text(t) if t.text == "Hello, world!"));
    }

    #[test]
    fn test_parse_print() {
        let template = parse("{{ name }}").unwrap();
        assert_eq!(template.body.len(), 1);
        assert!(matches!(&template.body[0], Node::Print(_)));
    }

    #[test]
    fn test_parse_if() {
        let template = parse("{% if true %}yes{% endif %}").unwrap();
        assert_eq!(template.body.len(), 1);
        assert!(matches!(&template.body[0], Node::If(_)));
    }

    #[test]
    fn test_parse_for() {
        let template = parse("{% for item in items %}{{ item }}{% endfor %}").unwrap();
        assert_eq!(template.body.len(), 1);
        assert!(matches!(&template.body[0], Node::For(_)));
    }

    #[test]
    fn test_parse_field_access() {
        let template = parse("{{ user.name }}").unwrap();
        if let Node::Print(print) = &template.body[0] {
            assert!(matches!(&print.expr, Expr::Field(_)));
        } else {
            panic!("Expected print node");
        }
    }

    #[test]
    fn test_parse_filter() {
        let template = parse("{{ name | upper }}").unwrap();
        if let Node::Print(print) = &template.body[0] {
            assert!(matches!(&print.expr, Expr::Filter(_)));
        } else {
            panic!("Expected print node");
        }
    }

    #[test]
    fn test_parse_import() {
        let template = parse(r#"{% import "macros.html" as macros %}"#).unwrap();
        assert_eq!(template.body.len(), 1);
        if let Node::Import(import) = &template.body[0] {
            assert_eq!(import.path.value, "macros.html");
            assert_eq!(import.alias.name, "macros");
        } else {
            panic!("Expected import node");
        }
    }

    #[test]
    fn test_parse_macro() {
        let template = parse(
            r#"{% macro button(text, class="btn") %}
<button class="{{ class }}">{{ text }}</button>
{% endmacro %}"#,
        )
        .unwrap();
        assert_eq!(template.body.len(), 1);
        if let Node::Macro(m) = &template.body[0] {
            assert_eq!(m.name.name, "button");
            assert_eq!(m.params.len(), 2);
            assert_eq!(m.params[0].name.name, "text");
            assert!(m.params[0].default.is_none());
            assert_eq!(m.params[1].name.name, "class");
            assert!(m.params[1].default.is_some());
        } else {
            panic!("Expected macro node");
        }
    }

    #[test]
    fn test_parse_macro_call() {
        let template = parse(r#"{{ macros::button(text="Click me") }}"#).unwrap();
        assert_eq!(template.body.len(), 1);
        if let Node::Print(print) = &template.body[0] {
            if let Expr::MacroCall(call) = &print.expr {
                assert_eq!(call.namespace.name, "macros");
                assert_eq!(call.macro_name.name, "button");
                assert_eq!(call.kwargs.len(), 1);
                assert_eq!(call.kwargs[0].0.name, "text");
            } else {
                panic!("Expected macro call expression");
            }
        } else {
            panic!("Expected print node");
        }
    }

    #[test]
    fn test_parse_self_macro_call() {
        let template = parse(r#"{{ self::helper() }}"#).unwrap();
        assert_eq!(template.body.len(), 1);
        if let Node::Print(print) = &template.body[0] {
            if let Expr::MacroCall(call) = &print.expr {
                assert_eq!(call.namespace.name, "self");
                assert_eq!(call.macro_name.name, "helper");
            } else {
                panic!("Expected macro call expression");
            }
        } else {
            panic!("Expected print node");
        }
    }
}
