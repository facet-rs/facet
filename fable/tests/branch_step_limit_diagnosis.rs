use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyGlrDiagnostics, WeavyParsePlan, parse_prepared_weavy_glr_diagnostics,
        parse_prepared_weavy_with_report,
    },
    parser::{LookaheadSymbol, ParseTable, ParserGrammar, TraceEvent},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourcePopSnapshot {
    conflict: u32,
    state: u32,
    source_byte: usize,
    line: usize,
    column: usize,
    lookahead: LookaheadSnapshot,
    branch_pops: usize,
    split_count: usize,
    created_branch_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PositionPopSnapshot {
    conflict: u32,
    state: u32,
    source_byte: usize,
    current_byte: usize,
    current_line: usize,
    current_column: usize,
    branch_pops: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConflictPopSnapshot {
    conflict: u32,
    branch_pops: usize,
    split_count: usize,
    created_branch_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LookaheadSnapshot {
    Terminal(u32),
    Eof,
    ReservedWord { terminal: u32, context: u32 },
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GlrAttributionSnapshot {
    total_branch_pops: usize,
    root_branch_pops: usize,
    top_conflicts: Vec<ConflictPopSnapshot>,
    top_sources: Vec<SourcePopSnapshot>,
    top_positions: Vec<PositionPopSnapshot>,
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

fn glr_attribution_snapshot(
    input: &str,
    diagnostics: &WeavyGlrDiagnostics,
) -> GlrAttributionSnapshot {
    let mut conflict_totals = std::collections::BTreeMap::<u32, (usize, usize, usize)>::new();
    for source in diagnostics.source_pops() {
        let entry = conflict_totals
            .entry(source.source().conflict().get())
            .or_default();
        entry.0 += source.branch_pops();
        entry.1 += source.split_count();
        entry.2 += source.created_branch_count();
    }
    let mut top_conflicts = conflict_totals
        .into_iter()
        .map(
            |(conflict, (branch_pops, split_count, created_branch_count))| ConflictPopSnapshot {
                conflict,
                branch_pops,
                split_count,
                created_branch_count,
            },
        )
        .collect::<Vec<_>>();
    top_conflicts.sort_by(|left, right| {
        right
            .branch_pops
            .cmp(&left.branch_pops)
            .then_with(|| left.conflict.cmp(&right.conflict))
    });

    GlrAttributionSnapshot {
        total_branch_pops: diagnostics.total_branch_pops(),
        root_branch_pops: diagnostics.root_branch_pops(),
        top_conflicts,
        top_sources: diagnostics
            .source_pops()
            .iter()
            .take(8)
            .map(|entry| {
                let source = entry.source();
                let (line, column) = line_column(input, source.byte_position());
                SourcePopSnapshot {
                    conflict: source.conflict().get(),
                    state: source.state().get(),
                    source_byte: source.byte_position(),
                    line,
                    column,
                    lookahead: lookahead_snapshot(source.lookahead()),
                    branch_pops: entry.branch_pops(),
                    split_count: entry.split_count(),
                    created_branch_count: entry.created_branch_count(),
                }
            })
            .collect(),
        top_positions: diagnostics
            .position_pops()
            .iter()
            .take(8)
            .map(|entry| {
                let source = entry.source();
                let (current_line, current_column) =
                    line_column(input, entry.current_byte_position());
                PositionPopSnapshot {
                    conflict: source.conflict().get(),
                    state: source.state().get(),
                    source_byte: source.byte_position(),
                    current_byte: entry.current_byte_position(),
                    current_line,
                    current_column,
                    branch_pops: entry.branch_pops(),
                }
            })
            .collect(),
    }
}

const fn lookahead_snapshot(lookahead: LookaheadSymbol) -> LookaheadSnapshot {
    match lookahead {
        LookaheadSymbol::Terminal(terminal) => LookaheadSnapshot::Terminal(terminal.get()),
        LookaheadSymbol::Eof => LookaheadSnapshot::Eof,
        LookaheadSymbol::ReservedWord { terminal, context } => LookaheadSnapshot::ReservedWord {
            terminal: terminal.get(),
            context: context.get(),
        },
        LookaheadSymbol::External(_) | LookaheadSymbol::ErrorRecovery(_) | _ => {
            LookaheadSnapshot::Other
        }
    }
}

fn line_column(input: &str, byte_position: usize) -> (usize, usize) {
    let prefix = &input[..byte_position.min(input.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len())
        + 1;
    (line, column)
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

/*
BranchStepLimit attribution findings, measured with Snark's glr-diagnostics
feature on the declaration-walker control and all-node walker repro.

Conflict catalog:
- conflict 3, state 41, lookahead terminal#7 "(":
  reduce p75 _expr -> var_ref
  reduce p92 _call_callee -> var_ref
- conflict 13, state 560, lookahead terminal#7 "(":
  reduce p75 _expr -> var_ref
  reduce p92 _call_callee -> var_ref

The 18 retained table conflicts are not equally involved in this workload.
Every attributed branch pop in both measured shapes comes from conflicts 3 and
13, both from the same semantic ambiguity: at a var_ref followed by "(", the
parser keeps both reductions, `_expr -> var_ref` and `_call_callee -> var_ref`.

Declaration-walker control before grammar disambiguation:
- total GLR branch pops: 2,417,938
- conflict 13: 1,811,488 pops
- conflict 3: 606,208 pops

Declaration-walker control after grammar disambiguation:
- total GLR branch pops: 1,430
- attributed fork pops: 0
- table conflicts: 14

All-node walker repro before grammar disambiguation:
- total GLR branch pops: 16,857,345; this is limit + 1 for the 4,098 byte input
- conflict 13: 12,122,251 pops, 128,324 splits, 256,648 child versions
- conflict 3: 4,734,852 pops, 127,970 splits, 255,940 child versions

All-node walker repro after grammar disambiguation:
- total GLR branch pops: 3,700
- attributed fork pops: 0
- parses under the 16,857,344 branch-step limit

Implemented grammar half:
- call_expr still uses `_call_callee`, preserving the generated
  `CallExpr { callee: Expr }` AST surface.
- `_call_callee` now gives its `var_ref` alternative postfix static precedence,
  so on `foo(` the reduce/reduce conflict picks `_call_callee -> var_ref` over
  `_expr -> var_ref`.
- The semantic choice preserved is that calls bind tighter than binary/unary
  expression forms and that only the explicit callee subset can be called.
- GLR-side candidate: the branch worklist does not pack/merge equivalent reduce
  alternatives for `_expr -> var_ref` vs `_call_callee -> var_ref`, so each call
  site doubles many later stack versions. Even with grammar disambiguation,
  GLR should eventually merge or share equivalent branch suffixes earlier,
  because repeated source positions grow split counts into powers of two.
- Best classification: both. The direct offender is a Fable grammar ambiguity,
  but the substrate turns that small ambiguity into millions of branch pops.
*/

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
            conflicts: 14,
            splits: 0,
            accepted: 1,
            failures: 0,
            max_live_versions: 1,
        }
    );
}

#[test]
fn declaration_walker_glr_attribution_is_stable() {
    let prepared = PreparedFableParser::new();
    let query = declaration_walker_query();
    let run = parse_prepared_weavy_glr_diagnostics(
        &prepared.plan,
        &prepared.parser,
        &prepared.table,
        query,
    );

    assert!(run.result().is_ok());
    assert_eq!(
        glr_attribution_snapshot(query, run.diagnostics()),
        GlrAttributionSnapshot {
            total_branch_pops: 1430,
            root_branch_pops: 1430,
            top_conflicts: Vec::new(),
            top_sources: Vec::new(),
            top_positions: Vec::new(),
        }
    );
}

#[test]
fn all_node_walker_parses_under_branch_step_limit() {
    let prepared = PreparedFableParser::new();
    let query = all_node_walker_query(0);
    let run = parse_prepared_weavy_glr_diagnostics(
        &prepared.plan,
        &prepared.parser,
        &prepared.table,
        &query,
    );
    assert!(run.result().is_ok());
    assert_eq!(
        glr_attribution_snapshot(&query, run.diagnostics()),
        GlrAttributionSnapshot {
            total_branch_pops: 3700,
            root_branch_pops: 3700,
            top_conflicts: Vec::new(),
            top_sources: Vec::new(),
            top_positions: Vec::new(),
        }
    );
}
