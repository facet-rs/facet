//! Typed views over the Fable CST.

use crate::SyntaxKind::{self, *};
use crate::{ResolvedNode, ResolvedToken};

/// A typed view over a CST node with a known kind.
pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: ResolvedNode) -> Option<Self>;
    fn syntax(&self) -> &ResolvedNode;
}

macro_rules! ast_node {
    ($name:ident, $kind:path) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(ResolvedNode);

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }

            fn cast(node: ResolvedNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some(Self(node))
                } else {
                    None
                }
            }

            fn syntax(&self) -> &ResolvedNode {
                &self.0
            }
        }
    };
}

fn typed_children<T: AstNode>(node: &ResolvedNode) -> impl Iterator<Item = T> + '_ {
    node.children().filter_map(|child| T::cast(child.clone()))
}

fn typed_child<T: AstNode>(node: &ResolvedNode) -> Option<T> {
    typed_children(node).next()
}

fn expr_children(node: &ResolvedNode) -> impl Iterator<Item = Expr> + '_ {
    node.children()
        .filter_map(|child| Expr::cast(child.clone()))
}

ast_node!(Root, SyntaxKind::Root);
ast_node!(Block, SyntaxKind::Block);
ast_node!(AssignStmt, SyntaxKind::AssignStmt);
ast_node!(LetStmt, SyntaxKind::LetStmt);
ast_node!(ExprStmt, SyntaxKind::ExprStmt);
ast_node!(IfStmt, SyntaxKind::IfStmt);
ast_node!(ElseClause, SyntaxKind::ElseClause);
ast_node!(Literal, SyntaxKind::Literal);
ast_node!(VarRef, SyntaxKind::VarRef);
ast_node!(FieldExpr, SyntaxKind::FieldExpr);
ast_node!(IndexExpr, SyntaxKind::IndexExpr);
ast_node!(StructLiteral, SyntaxKind::StructLiteral);
ast_node!(StructField, SyntaxKind::StructField);
ast_node!(CallExpr, SyntaxKind::CallExpr);
ast_node!(ArgList, SyntaxKind::ArgList);
ast_node!(Arg, SyntaxKind::Arg);
ast_node!(BinaryExpr, SyntaxKind::BinaryExpr);
ast_node!(UnaryExpr, SyntaxKind::UnaryExpr);
ast_node!(ParenExpr, SyntaxKind::ParenExpr);

/// A statement in a Fable block or root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Assign(AssignStmt),
    Let(LetStmt),
    Expr(ExprStmt),
    If(IfStmt),
}

impl Stmt {
    fn cast(node: ResolvedNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::AssignStmt => Some(Self::Assign(AssignStmt(node))),
            SyntaxKind::LetStmt => Some(Self::Let(LetStmt(node))),
            SyntaxKind::ExprStmt => Some(Self::Expr(ExprStmt(node))),
            SyntaxKind::IfStmt => Some(Self::If(IfStmt(node))),
            _ => None,
        }
    }

    /// The statement syntax node.
    #[must_use]
    pub fn syntax(&self) -> &ResolvedNode {
        match self {
            Self::Assign(node) => node.syntax(),
            Self::Let(node) => node.syntax(),
            Self::Expr(node) => node.syntax(),
            Self::If(node) => node.syntax(),
        }
    }
}

/// An expression node in Fable source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Literal),
    Var(VarRef),
    Field(FieldExpr),
    Index(IndexExpr),
    StructLiteral(StructLiteral),
    Call(CallExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Paren(ParenExpr),
}

impl Expr {
    fn cast(node: ResolvedNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::Literal => Some(Self::Literal(Literal(node))),
            SyntaxKind::VarRef => Some(Self::Var(VarRef(node))),
            SyntaxKind::FieldExpr => Some(Self::Field(FieldExpr(node))),
            SyntaxKind::IndexExpr => Some(Self::Index(IndexExpr(node))),
            SyntaxKind::StructLiteral => Some(Self::StructLiteral(StructLiteral(node))),
            SyntaxKind::CallExpr => Some(Self::Call(CallExpr(node))),
            SyntaxKind::BinaryExpr => Some(Self::Binary(BinaryExpr(node))),
            SyntaxKind::UnaryExpr => Some(Self::Unary(UnaryExpr(node))),
            SyntaxKind::ParenExpr => Some(Self::Paren(ParenExpr(node))),
            _ => None,
        }
    }

    /// The expression syntax node.
    #[must_use]
    pub fn syntax(&self) -> &ResolvedNode {
        match self {
            Self::Literal(node) => node.syntax(),
            Self::Var(node) => node.syntax(),
            Self::Field(node) => node.syntax(),
            Self::Index(node) => node.syntax(),
            Self::StructLiteral(node) => node.syntax(),
            Self::Call(node) => node.syntax(),
            Self::Binary(node) => node.syntax(),
            Self::Unary(node) => node.syntax(),
            Self::Paren(node) => node.syntax(),
        }
    }
}

impl Root {
    /// Top-level statements.
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ {
        self.0
            .children()
            .filter_map(|child| Stmt::cast(child.clone()))
    }
}

impl Block {
    /// Statements inside this block.
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ {
        self.0
            .children()
            .filter_map(|child| Stmt::cast(child.clone()))
    }
}

impl AssignStmt {
    /// Assignment target expression.
    #[must_use]
    pub fn target(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }

    /// Assigned value expression.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        expr_children(&self.0).nth(1)
    }
}

impl LetStmt {
    /// Binding name.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == Ident)
            .map(|token| token.text().to_owned())
    }

    /// Initial value expression.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }
}

impl ExprStmt {
    /// The expression evaluated by this statement.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }
}

impl IfStmt {
    /// The condition expression.
    #[must_use]
    pub fn condition(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }

    /// The `then` block.
    #[must_use]
    pub fn then_block(&self) -> Option<Block> {
        typed_child(&self.0)
    }

    /// Optional else clause.
    #[must_use]
    pub fn else_clause(&self) -> Option<ElseClause> {
        typed_child(&self.0)
    }
}

impl ElseClause {
    /// Nested `else if` statement, if present.
    #[must_use]
    pub fn if_stmt(&self) -> Option<IfStmt> {
        typed_child(&self.0)
    }

    /// Else block, if present.
    #[must_use]
    pub fn block(&self) -> Option<Block> {
        typed_child(&self.0)
    }
}

impl Literal {
    /// The literal token.
    #[must_use]
    pub fn token(&self) -> Option<ResolvedToken> {
        self.0.first_token().cloned()
    }
}

impl VarRef {
    /// Variable name text.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.0.first_token().map(|token| token.text().to_owned())
    }
}

impl FieldExpr {
    /// Base expression before the dot.
    #[must_use]
    pub fn base(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }

    /// Field name after the dot.
    #[must_use]
    pub fn field_name(&self) -> Option<String> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == Ident)
            .last()
            .map(|token| token.text().to_owned())
    }
}

impl IndexExpr {
    /// Base expression before the bracket.
    #[must_use]
    pub fn base(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }

    /// Index expression inside the bracket.
    #[must_use]
    pub fn index(&self) -> Option<Expr> {
        expr_children(&self.0).nth(1)
    }
}

impl StructLiteral {
    /// Type name before the literal body.
    #[must_use]
    pub fn type_name(&self) -> Option<String> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == Ident)
            .map(|token| token.text().to_owned())
    }

    /// Field initializers.
    pub fn fields(&self) -> impl Iterator<Item = StructField> + '_ {
        typed_children(&self.0)
    }
}

impl StructField {
    /// Field name before the colon.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == Ident)
            .map(|token| token.text().to_owned())
    }

    /// Field value expression.
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }
}

impl CallExpr {
    /// Callee expression.
    #[must_use]
    pub fn callee(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }

    /// Call argument list.
    #[must_use]
    pub fn args(&self) -> Option<ArgList> {
        typed_child(&self.0)
    }
}

impl ArgList {
    /// Positional arguments.
    pub fn args(&self) -> impl Iterator<Item = Arg> + '_ {
        typed_children(&self.0)
    }
}

impl Arg {
    /// Argument expression.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }
}

impl BinaryExpr {
    /// Left operand.
    #[must_use]
    pub fn lhs(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }

    /// Right operand.
    #[must_use]
    pub fn rhs(&self) -> Option<Expr> {
        expr_children(&self.0).nth(1)
    }
}

impl UnaryExpr {
    /// Unary operand.
    #[must_use]
    pub fn operand(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }
}

impl ParenExpr {
    /// Parenthesized inner expression.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        expr_children(&self.0).next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn root_exposes_statements() {
        let parsed = parse("root.age = 42\nif root.age >= 18 { root.adult = true }");
        assert!(parsed.errors().is_empty());

        let root = Root::cast(parsed.syntax().clone()).unwrap();
        let statements: Vec<_> = root.statements().collect();
        assert_eq!(statements.len(), 2);
        assert!(matches!(statements[0], Stmt::Assign(_)));
        assert!(matches!(statements[1], Stmt::If(_)));
    }

    #[test]
    fn assignment_exposes_target_and_value() {
        let parsed = parse("root.user.name = \"Ada\"");
        assert!(parsed.errors().is_empty());
        let root = Root::cast(parsed.syntax().clone()).unwrap();
        let Stmt::Assign(assign) = root.statements().next().unwrap() else {
            panic!("expected assignment");
        };

        assert!(matches!(assign.target(), Some(Expr::Field(_))));
        assert!(matches!(assign.value(), Some(Expr::Literal(_))));
    }

    #[test]
    fn let_stmt_exposes_name_and_value() {
        let parsed = parse("let next_age = root.user.age + 1");
        assert!(parsed.errors().is_empty());
        let root = Root::cast(parsed.syntax().clone()).unwrap();
        let Stmt::Let(let_stmt) = root.statements().next().unwrap() else {
            panic!("expected let statement");
        };

        assert_eq!(let_stmt.name().as_deref(), Some("next_age"));
        assert!(matches!(let_stmt.value(), Some(Expr::Binary(_))));
    }

    #[test]
    fn field_expr_exposes_final_field_name() {
        let parsed = parse("root.user.name");
        assert!(parsed.errors().is_empty());
        let root = Root::cast(parsed.syntax().clone()).unwrap();
        let Stmt::Expr(expr) = root.statements().next().unwrap() else {
            panic!("expected expression statement");
        };
        let Some(Expr::Field(field)) = expr.expr() else {
            panic!("expected field expression");
        };

        assert_eq!(field.field_name().as_deref(), Some("name"));
    }
}
