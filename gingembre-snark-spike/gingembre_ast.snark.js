// AST enrichment for the gingembre expression grammar, in the `ast()` DSL.
//
// This is the SINGLE source of the annotations: build.rs consumes it to CODEGEN the
// `#[derive(Facet)]` AST (structure derived from the grammar; this only adds field names,
// node->variant renames, and the scalar-decode choice), and main.rs consumes the SAME file
// to drive the reflection/Weavy builder. Field TYPES are not written here — they're derived
// from the grammar (a named child is an expression -> the enum; a token -> String).
ast({
  _expr:    { enum: "Expr" },
  binary:   { as: "Binary", struct: "Binary",
              fields: { left: { from: "named:0" }, op: { from: "token" }, right: { from: "named:1" } } },
  variable: { as: "Variable", scalar: "String" },
  literal:  { transparent: true },
  number:   { as: "Number", scalar: "i64" },
});
