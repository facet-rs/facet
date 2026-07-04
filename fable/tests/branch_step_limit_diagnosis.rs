use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyGlrDiagnostics, WeavyParseError, WeavyParsePlan, parse_prepared_weavy_glr_diagnostics,
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

Declaration-walker control:
- total GLR branch pops: 2,417,938
- unforked/root pops: 242
- conflict 13: 1,811,488 pops, 16,585 splits, 33,170 child versions
- conflict 3: 606,208 pops, 16,384 splits, 32,768 child versions
- densest current positions:
  - conflict 3 source byte 1528 -> current bytes 1530 and 1533:
    163,840 pops each
  - conflict 13 source byte 1488 -> current byte 1498: 114,688 pops

All-node walker repro:
- total GLR branch pops: 16,857,345; this is limit + 1 for the 4,098 byte input
- unforked/root pops: 242
- conflict 13: 12,122,251 pops, 128,324 splits, 256,648 child versions
- conflict 3: 4,734,852 pops, 127,970 splits, 255,940 child versions
- densest current positions:
  - conflict 3 source byte 4093 -> current bytes 4095 and 4098:
    1,279,692 and 1,279,690 pops
  - conflict 13 source byte 4053 -> current byte 4063:
    895,830 pops

Classification:
- Fable grammar candidate: call_expr currently uses `_call_callee` as a hidden
  subset of `_expr`, but `_call_callee` includes var_ref, field_expr, index_expr,
  and paren_expr. On `foo(` this leaves `var_ref` reducible as both `_expr` and
  `_call_callee`. A grammar fix would pin call syntax so a callee followed by
  `arg_list` reduces through only the call-callee path. Candidate rules:
  `call_expr`, `_call_callee`, `var_ref`, and postfix `field_expr`/`index_expr`.
  The semantic choice to preserve is that calls bind tighter than binary/unary
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
            conflicts: 18,
            splits: 0,
            accepted: 1,
            failures: 202,
            max_live_versions: 92,
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
            total_branch_pops: 2_417_938,
            root_branch_pops: 242,
            top_conflicts: vec![
                ConflictPopSnapshot {
                    conflict: 13,
                    branch_pops: 1_811_488,
                    split_count: 16_585,
                    created_branch_count: 33_170,
                },
                ConflictPopSnapshot {
                    conflict: 3,
                    branch_pops: 606_208,
                    split_count: 16_384,
                    created_branch_count: 32_768,
                },
            ],
            top_sources: vec![
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1488,
                    line: 52,
                    column: 42,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 950_272,
                    split_count: 8192,
                    created_branch_count: 16_384,
                },
                SourcePopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 1528,
                    line: 57,
                    column: 11,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 606_208,
                    split_count: 16_384,
                    created_branch_count: 32_768,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1431,
                    line: 51,
                    column: 69,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 524_288,
                    split_count: 4096,
                    created_branch_count: 8192,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1347,
                    line: 50,
                    column: 70,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 131_072,
                    split_count: 1024,
                    created_branch_count: 2048,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1404,
                    line: 51,
                    column: 42,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 106_496,
                    split_count: 2048,
                    created_branch_count: 4096,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1262,
                    line: 49,
                    column: 78,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 32_768,
                    split_count: 256,
                    created_branch_count: 512,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1052,
                    line: 41,
                    column: 44,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 22_912,
                    split_count: 128,
                    created_branch_count: 256,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1323,
                    line: 50,
                    column: 46,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 20_480,
                    split_count: 512,
                    created_branch_count: 1024,
                },
            ],
            top_positions: vec![
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 1528,
                    current_byte: 1530,
                    current_line: 57,
                    current_column: 13,
                    branch_pops: 163_840,
                },
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 1528,
                    current_byte: 1533,
                    current_line: 58,
                    current_column: 1,
                    branch_pops: 163_840,
                },
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 1528,
                    current_byte: 1531,
                    current_line: 57,
                    current_column: 14,
                    branch_pops: 114_688,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1488,
                    current_byte: 1498,
                    current_line: 52,
                    current_column: 52,
                    branch_pops: 114_688,
                },
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 1528,
                    current_byte: 1528,
                    current_line: 57,
                    current_column: 11,
                    branch_pops: 98_304,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1488,
                    current_byte: 1515,
                    current_line: 55,
                    current_column: 1,
                    branch_pops: 98_304,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1488,
                    current_byte: 1495,
                    current_line: 52,
                    current_column: 49,
                    branch_pops: 81_920,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 1488,
                    current_byte: 1518,
                    current_line: 57,
                    current_column: 1,
                    branch_pops: 81_920,
                },
            ],
        }
    );
}

#[test]
#[ignore = "diagnostic repro for the BranchStepLimit ceiling; expands 16,857,344 GLR branch steps"]
fn all_node_walker_hits_branch_step_limit() {
    let prepared = PreparedFableParser::new();
    let query = all_node_walker_query(0);
    let run = parse_prepared_weavy_glr_diagnostics(
        &prepared.plan,
        &prepared.parser,
        &prepared.table,
        &query,
    );
    let error = run
        .result()
        .as_ref()
        .expect_err("diagnostic query should exceed the strict branch budget");

    assert_eq!(
        *error,
        WeavyParseError::BranchStepLimit {
            limit: prepared.strict_branch_step_limit(&query),
        }
    );
    assert_eq!(
        glr_attribution_snapshot(&query, run.diagnostics()),
        GlrAttributionSnapshot {
            total_branch_pops: 16_857_345,
            root_branch_pops: 242,
            top_conflicts: vec![
                ConflictPopSnapshot {
                    conflict: 13,
                    branch_pops: 12_122_251,
                    split_count: 128_324,
                    created_branch_count: 256_648,
                },
                ConflictPopSnapshot {
                    conflict: 3,
                    branch_pops: 4_734_852,
                    split_count: 127_970,
                    created_branch_count: 255_940,
                },
            ],
            top_sources: vec![
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 4053,
                    line: 147,
                    column: 60,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 7_678_343,
                    split_count: 63_990,
                    created_branch_count: 127_980,
                },
                SourcePopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 4093,
                    line: 152,
                    column: 11,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 4_734_852,
                    split_count: 127_970,
                    created_branch_count: 255_940,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 3978,
                    line: 146,
                    column: 96,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 2_047_852,
                    split_count: 16_000,
                    created_branch_count: 32_000,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 4034,
                    line: 147,
                    column: 41,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 1_279_800,
                    split_count: 31_996,
                    created_branch_count: 63_992,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 3951,
                    line: 146,
                    column: 69,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 448_000,
                    split_count: 8000,
                    created_branch_count: 16_000,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 3867,
                    line: 145,
                    column: 70,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 256_000,
                    split_count: 2000,
                    created_branch_count: 4000,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 3924,
                    line: 146,
                    column: 42,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 208_000,
                    split_count: 4000,
                    created_branch_count: 8000,
                },
                SourcePopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 3782,
                    line: 144,
                    column: 78,
                    lookahead: LookaheadSnapshot::Terminal(7),
                    branch_pops: 64_256,
                    split_count: 506,
                    created_branch_count: 1012,
                },
            ],
            top_positions: vec![
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 4093,
                    current_byte: 4095,
                    current_line: 152,
                    current_column: 13,
                    branch_pops: 1_279_692,
                },
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 4093,
                    current_byte: 4098,
                    current_line: 153,
                    current_column: 1,
                    branch_pops: 1_279_690,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 4053,
                    current_byte: 4063,
                    current_line: 147,
                    current_column: 70,
                    branch_pops: 895_830,
                },
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 4093,
                    current_byte: 4096,
                    current_line: 152,
                    current_column: 14,
                    branch_pops: 895_784,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 4053,
                    current_byte: 4080,
                    current_line: 150,
                    current_column: 1,
                    branch_pops: 767_820,
                },
                PositionPopSnapshot {
                    conflict: 3,
                    state: 41,
                    source_byte: 4093,
                    current_byte: 4093,
                    current_line: 152,
                    current_column: 11,
                    branch_pops: 767_810,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 4053,
                    current_byte: 4060,
                    current_line: 147,
                    current_column: 67,
                    branch_pops: 639_875,
                },
                PositionPopSnapshot {
                    conflict: 13,
                    state: 560,
                    source_byte: 4053,
                    current_byte: 4083,
                    current_line: 152,
                    current_column: 1,
                    branch_pops: 639_850,
                },
            ],
        }
    );
}
