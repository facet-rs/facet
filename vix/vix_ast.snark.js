// AST enrichment for the vix grammar, in the `ast()` DSL.
//
// Structure (which fields a node has, their types, their cardinality) is DERIVED from
// the grammar: every AST-relevant child carries a `field()`, and the optional/repeat
// context it sits in becomes Option/Vec. This file only adds what the grammar cannot
// express: enum names for hidden choice rules, variant/struct renames, leaf decode
// choices, and names for ad-hoc enums arising from mixed-alternative fields.
ast({
  // hidden choice rules -> Rust enums
  _item: { enum: "Item" },
  _statement: { enum: "Stmt" },
  _expr: { enum: "Expr" },
  // Syntactic restriction of _expr (no bare struct literal before a match
  // body's `{`) — same AST enum, deduped by the codegen.
  _scrutinee: { enum: "Expr" },
  _type: { enum: "Type" },
  _pattern: { enum: "Pattern" },
  _arg: { enum: "Arg" },

  // items
  use_item: { as: "Use" },
  fn_item: { as: "Fn" },
  struct_item: { as: "Struct" },
  enum_item: { as: "Enum" },

  // statements
  let_statement: { as: "Let", struct: "LetStmt" },
  expr_statement: { as: "Expr", struct: "ExprStmt" },

  // expressions
  match_expr: { as: "Match" },
  command_block: {
    as: "Command",
    fields: { part: { enum: "CommandPart" } },
  },
  method_call: { as: "MethodCall" },
  field_access: {
    as: "Field",
    // `.name` field access | `.0` tuple index share the node
    fields: { name: { enum: "Member" } },
  },
  scoped_identifier: { as: "Scoped" },
  call: { fields: { callee: { enum: "PathRef" } } },
  array: { fields: { elem: { enum: "ArrayElem" } } },
  struct_literal: {
    as: "StructLit",
    fields: { path: { enum: "PathRef" } },
  },
  map_literal: { as: "Map", struct: "MapLiteral" },
  tuple_expr: { as: "Tuple" },
  partial: { as: "Partial" },
  tuple_index: { as: "Index", decode: "text" },

  // types
  array_type: { as: "Array" },
  generic_type: { as: "Generic" },
  tuple_type: { as: "Tuple" },
  fn_type: { as: "Fn" },
  type_path: { as: "Path" },

  // patterns
  wildcard_pattern: { as: "Wildcard" },
  variant_pattern: { as: "Variant", fields: { path: { enum: "PathRef" } } },
  struct_pattern: { as: "Struct", fields: { path: { enum: "PathRef" } } },
  tuple_pattern: { as: "Tuple" },

  // leaves: decode choice ("text" = raw source text)
  identifier: { as: "Identifier", decode: "text" },
  string: { as: "Str", decode: "string" },
  path_literal: { as: "Path", decode: "path" },
  number: { as: "Number", decode: "text" },
  boolean: { as: "Bool", decode: "bool" },
  flag: { as: "Flag", decode: "text" },
  command_token: { as: "Token", decode: "text" },
});
