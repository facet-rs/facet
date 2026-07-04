use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{WeavyParseError, WeavyParsePlan, parse_prepared_weavy_with_report},
    parser::{ParseTable, ParserGrammar, TraceEvent},
    validated::ValidatedGrammar,
};

struct PreparedFableParser {
    parser: ParserGrammar,
    table: ParseTable,
    plan: WeavyParsePlan,
}

impl PreparedFableParser {
    fn new() -> Self {
        let raw = RawGrammarJson::from_tree_sitter_json_str(fable::GRAMMAR_JSON)
            .expect("embedded fable grammar imports");
        let validated = ValidatedGrammar::from_raw(&raw).expect("embedded fable grammar validates");
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .expect("embedded fable grammar normalizes")
            .prepare_productions_for_items()
            .expect("embedded fable grammar prepares productions");
        let table =
            ParseTable::from_grammar(&parser).expect("embedded fable grammar builds tables");
        let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("weavy parse plan");

        Self {
            parser,
            table,
            plan,
        }
    }

    fn strict_branch_step_limit(&self, input: &str) -> usize {
        10_000usize.max(
            input
                .len()
                .saturating_mul(4096)
                .saturating_add(self.table.states().len().saturating_mul(64)),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParseMetrics {
    bytes: usize,
    limit: usize,
    states: usize,
    conflicts: usize,
    splits: usize,
    accepted: usize,
    failures: usize,
    max_live_versions: usize,
}

fn parse_metrics(prepared: &PreparedFableParser, input: &str) -> ParseMetrics {
    let report =
        parse_prepared_weavy_with_report(&prepared.plan, &prepared.parser, &prepared.table, input)
            .expect("diagnostic query parses");
    ParseMetrics {
        bytes: input.len(),
        limit: prepared.strict_branch_step_limit(input),
        states: prepared.table.states().len(),
        conflicts: prepared.table.conflicts().len(),
        splits: report
            .trace_events()
            .iter()
            .filter(|event| matches!(event, TraceEvent::GlrSplit { .. }))
            .count(),
        accepted: report.accepted_count(),
        failures: report.failure_count(),
        max_live_versions: report.max_live_versions(),
    }
}

fn declaration_walker_query() -> &'static str {
    r#"
fn walk_name(name: Name) -> usize {
  match name {
    Name::Ident { ident } => { 1; },
    Name::TypeIdent { type_ident } => { 1; },
  };
}

fn walk_type(ty: TypeExpr) -> usize {
  match ty {
    TypeExpr::Scalar { scalar } => { 1; },
    TypeExpr::Declared { declared } => { walk_name(declared.name); },
  };
}

fn walk_type_fields(fields: TypeFieldList) -> usize {
  walk_type_fields_from(fields, 0);
}

fn walk_type_fields_from(fields: TypeFieldList, index: usize) -> usize {
  if index >= len(fields.fields) {
    0;
  } else {
    walk_name(fields.fields[index].name) + walk_type(fields.fields[index].ty) + walk_type_fields_from(fields, index + 1);
  }
}

fn walk_struct(decl: StructDecl) -> usize {
  walk_name(decl.name) + walk_type_fields(decl.fields);
}

fn walk_enum_variants(decl: EnumDecl, index: usize) -> usize {
  if index >= len(decl.variants) {
    0;
  } else {
    walk_name(decl.variants[index].name) + walk_enum_variants(decl, index + 1);
  }
}

fn walk_enum(decl: EnumDecl) -> usize {
  walk_name(decl.name) + walk_enum_variants(decl, 0);
}

fn walk_items(index: usize) -> usize {
  if index >= len(root.items) {
    0;
  } else {
    match root.items[index] {
      Item::Struct { struct_decl } => { walk_struct(struct_decl) + walk_items(index + 1); },
      Item::Enum { enum_decl } => { walk_enum(enum_decl) + walk_items(index + 1); },
      Item::Fn { fn_decl } => { walk_name(fn_decl.name) + walk_items(index + 1); },
      Item::Stmt { stmt } => { walk_items(index + 1); },
    };
  }
}

walk_items(0);
"#
}

fn all_node_walker_query(dummy_function_count: usize) -> String {
    let mut source = String::new();
    source.push_str(
        r#"
fn walk_name(name: Name) -> usize {
  match name {
    Name::Ident { ident } => { 1; },
    Name::TypeIdent { type_ident } => { 1; },
  };
}

fn walk_type(ty: TypeExpr) -> usize {
  match ty {
    TypeExpr::Scalar { scalar } => { 1; },
    TypeExpr::Declared { declared } => { walk_name(declared.name); },
  };
}

fn walk_type_fields(fields: TypeFieldList) -> usize {
  walk_type_fields_from(fields, 0);
}

fn walk_type_fields_from(fields: TypeFieldList, index: usize) -> usize {
  if index >= len(fields.fields) {
    0;
  } else {
    walk_name(fields.fields[index].name) + walk_type(fields.fields[index].ty) + walk_type_fields_from(fields, index + 1);
  }
}

fn walk_struct(decl: StructDecl) -> usize {
  walk_name(decl.name) + walk_type_fields(decl.fields);
}

fn walk_enum_variants(decl: EnumDecl, index: usize) -> usize {
  if index >= len(decl.variants) {
    0;
  } else {
    walk_name(decl.variants[index].name) + walk_enum_variants(decl, index + 1);
  }
}

fn walk_enum(decl: EnumDecl) -> usize {
  walk_name(decl.name) + walk_enum_variants(decl, 0);
}

fn walk_block(block: Block) -> usize {
  walk_stmts(block, 0);
}

fn walk_stmts(block: Block, index: usize) -> usize {
  if index >= len(block.stmts) {
    0;
  } else {
    walk_stmt(block.stmts[index]) + walk_stmts(block, index + 1);
  }
}

fn walk_else_clause(else_clause: ElseClause) -> usize {
  1;
}

fn walk_stmt(stmt: Stmt) -> usize {
  match stmt {
    Stmt::If { if_stmt } => {
      walk_expr(if_stmt.condition) + walk_block(if_stmt.then) + walk_else_clause(if_stmt.else_clause);
    },
    Stmt::Let { let_stmt } => {
      walk_name(let_stmt.name) + walk_expr(let_stmt.value);
    },
    Stmt::Assign { assign_stmt } => {
      walk_expr(assign_stmt.target) + walk_expr(assign_stmt.value);
    },
    Stmt::Expr { expr_stmt } => {
      walk_expr(expr_stmt.expr);
    },
  };
}

fn walk_args(args: ArgList, index: usize) -> usize {
  if index >= len(args.args) {
    0;
  } else {
    walk_expr(args.args[index].expr) + walk_args(args, index + 1);
  }
}

fn walk_struct_fields(fields: StructFieldList, index: usize) -> usize {
  if index >= len(fields.fields) {
    0;
  } else {
    walk_name(fields.fields[index].name) + walk_expr(fields.fields[index].value) + walk_struct_fields(fields, index + 1);
  }
}

fn walk_match_arms(match_expr: MatchExpr, index: usize) -> usize {
  if index >= len(match_expr.arms) {
    0;
  } else {
    walk_block(match_expr.arms[index].body) + walk_match_arms(match_expr, index + 1);
  }
}

fn walk_expr(expr: Expr) -> usize {
  match expr {
    Expr::Binary { binary } => {
      walk_expr(binary.lhs) + walk_expr(binary.rhs);
    },
    Expr::Unary { unary } => {
      walk_expr(unary.operand);
    },
    Expr::Field { field } => {
      walk_expr(field.base) + walk_name(field.field_name);
    },
    Expr::Index { index } => {
      walk_expr(index.base) + walk_expr(index.index);
    },
    Expr::Call { call } => {
      walk_expr(call.callee) + walk_args(call.args, 0);
    },
    Expr::StructLiteral { struct_literal } => {
      walk_name(struct_literal.type_name) + walk_struct_fields(struct_literal.fields, 0);
    },
    Expr::EnumVariant { enum_variant } => {
      walk_name(enum_variant.path.type_name) + walk_name(enum_variant.path.variant_name);
    },
    Expr::Match { match_expr } => {
      walk_expr(match_expr.scrutinee) + walk_match_arms(match_expr, 0);
    },
    Expr::Paren { paren } => {
      walk_expr(paren.expr);
    },
    Expr::Var { var } => {
      walk_name(var.name);
    },
    Expr::Literal { literal } => {
      1;
    },
  };
}

fn walk_items(index: usize) -> usize {
  if index >= len(root.items) {
    0;
  } else {
    match root.items[index] {
      Item::Struct { struct_decl } => { walk_struct(struct_decl) + walk_items(index + 1); },
      Item::Enum { enum_decl } => { walk_enum(enum_decl) + walk_items(index + 1); },
      Item::Fn { fn_decl } => { walk_name(fn_decl.name) + walk_block(fn_decl.body) + walk_items(index + 1); },
      Item::Stmt { stmt } => { walk_stmt(stmt) + walk_items(index + 1); },
    };
  }
}
"#,
    );

    for index in 0..dummy_function_count {
        source.push_str(&format!(
            "\nfn walker_padding_{index}(value: usize) -> usize {{\n  if value == 0 {{\n    {index};\n  }} else {{\n    walker_padding_{index}(value - 1);\n  }}\n}}\n"
        ));
    }

    source.push_str("\nwalk_items(0);\n");
    source
}

#[test]
fn declaration_walker_control_metrics_are_stable() {
    let prepared = PreparedFableParser::new();
    let query = declaration_walker_query();
    let metrics = parse_metrics(&prepared, query);

    assert_eq!(
        metrics,
        ParseMetrics {
            bytes: 1533,
            limit: 6_351_104,
            states: 1124,
            conflicts: 18,
            splits: 0,
            accepted: 1,
            failures: 202,
            max_live_versions: 92,
        }
    );
}

#[test]
#[ignore = "diagnostic repro for the BranchStepLimit ceiling; expands 16,857,344 GLR branch steps"]
fn all_node_walker_hits_branch_step_limit() {
    let prepared = PreparedFableParser::new();
    let query = all_node_walker_query(0);
    let error =
        parse_prepared_weavy_with_report(&prepared.plan, &prepared.parser, &prepared.table, &query)
            .expect_err("diagnostic query should exceed the strict branch budget");

    assert_eq!(
        error,
        WeavyParseError::BranchStepLimit {
            limit: prepared.strict_branch_step_limit(&query),
        }
    );
}
