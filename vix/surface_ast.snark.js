// AST enrichment for the ratchet-facing Vix grammar. Structure remains
// grammar-derived; this file only names hidden enums, variants, and decoders.
ast({
  _item: { enum: "Item" },
  _statement: { enum: "Stmt" },
  _expr: { enum: "Expr" },
  _type: { enum: "Type" },

  fn_item: { as: "Fn" },
  let_statement: { as: "Let", struct: "LetStmt" },
  yield_statement: { as: "Yield", struct: "YieldStmt" },
  field_access: {
    as: "Field",
    fields: { name: { enum: "Member" } },
  },
  tuple_expr: { as: "Tuple" },
  generic_type: { as: "Generic" },
  tuple_type: { as: "Tuple" },
  type_path: { as: "Path" },

  identifier: { as: "Identifier", decode: "text" },
  string: { as: "Str", decode: "string" },
  number: { as: "Number", decode: "text" },
  tuple_index: { as: "Index", decode: "text" },
  boolean: { as: "Bool", decode: "bool" },
});
