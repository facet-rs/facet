// AST enrichment for the fable grammar, in the `ast()` DSL.
//
// Structure is derived from playgrounds/snark/src/bundled/fable/grammar.js:
// every AST-relevant child carries field(), and cardinality comes from grammar
// context. This file only supplies Rust-facing names and leaf decode choices.
ast({
  _statement: { enum: "Stmt" },
  _expr: { enum: "Expr" },
  _call_callee: { enum: "Expr" },
  _literal: { enum: "Literal", as: "Literal" },
  _name: { enum: "Name" },

  let_statement: { as: "Let", struct: "LetStmt" },
  assign_statement: { as: "Assign", struct: "AssignStmt" },
  expr_statement: { as: "Expr", struct: "ExprStmt" },
  if_statement: { as: "If", struct: "IfStmt" },

  binary_expr: { as: "Binary", struct: "BinaryExpr" },
  unary_expr: { as: "Unary", struct: "UnaryExpr" },
  field_expr: { as: "Field", struct: "FieldExpr" },
  index_expr: { as: "Index", struct: "IndexExpr" },
  call_expr: { as: "Call", struct: "CallExpr" },
  struct_literal: { as: "StructLiteral" },
  paren_expr: { as: "Paren", struct: "ParenExpr" },
  var_ref: { as: "Var", struct: "VarRef" },

  identifier: { as: "Ident", decode: "text" },
  type_identifier: { as: "TypeIdent", decode: "text" },
  int_literal: { as: "Int", decode: "text" },
  float_literal: { as: "Float", decode: "text" },
  string_literal: { as: "Str", decode: "string" },
  true_literal: { as: "True", decode: "bool" },
  false_literal: { as: "False", decode: "bool" },
  null_literal: { as: "Null", decode: "text" },
});
