// AST enrichment for the ratchet-facing Vix grammar. Structure remains
// grammar-derived; this file only names hidden enums, variants, and decoders.
ast({
  _item: { enum: "Item" },
  _statement: { enum: "Stmt" },
  _expr: { enum: "Expr" },
  _type: { enum: "Type" },
  _pattern: { enum: "Pattern" },
  _variant_type_payload: { enum: "VariantTypePayload" },
  _variant_pattern_payload: { enum: "VariantPatternPayload" },

  fn_item: { as: "Fn" },
  struct_item: { as: "Struct" },
  enum_item: { as: "Enum" },
  let_statement: { as: "Let", struct: "LetStmt" },
  yield_statement: { as: "Yield", struct: "YieldStmt" },
  field_access: {
    as: "Field",
    fields: { name: { enum: "Member" } },
  },
  record_expr: { as: "Record" },
  variant_expr: { as: "Variant" },
  match_expr: { as: "Match" },
  variant_pattern: { as: "Variant" },
  variant_tuple_type: { as: "Tuple" },
  record_field_list: { as: "Record" },
  tuple_pattern: { as: "Tuple" },
  record_pattern: { as: "Record" },
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
