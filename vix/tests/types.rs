//! Type-surface oracle: types.vix (the tour) and eval.vix (the self-hosting
//! shape) parse into the generated AST and bind with ZERO unresolved names —
//! every type is declared, imported, or a builtin scalar.

use vix::VixParser;
use vix::ast::{Expr, Item, Pattern, Stmt, Type};
use vix::binder::{self, SymbolKind};

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample")
}

#[test]
fn types_tour_parses_and_binds_clean() {
    let src = sample("types.vix");
    let file = VixParser::new().parse(&src).expect("types.vix parses");
    let b = binder::bind(&file);

    assert_eq!(
        b.unresolved().len(),
        0,
        "everything declared/imported/builtin: {:?}",
        b.unresolved()
    );

    // 4 structs (Toolchain, Sha256, Probe, Pair), 2 enums (Os, Artifact).
    let structs: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Struct(s) => Some(s),
            _ => None,
        })
        .collect();
    let enums: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) => Some(e),
            _ => None,
        })
        .collect();
    assert_eq!(structs.len(), 4);
    assert_eq!(enums.len(), 2);

    // Toolchain: record struct, `opt` has a default.
    let toolchain = &structs[0];
    assert_eq!(toolchain.name.value, "Toolchain");
    let fields = &toolchain.fields.as_ref().expect("record struct").fields;
    assert_eq!(fields[2].name.value, "opt");
    assert!(fields[2].default.is_some());

    // Sha256: tuple struct; Probe: unit struct.
    assert!(structs[1].tuple.is_some() && structs[1].fields.is_none());
    assert!(structs[2].tuple.is_none() && structs[2].fields.is_none());

    // Artifact: unit + tuple + record variants, in declaration order.
    let artifact = &enums[1];
    assert_eq!(artifact.name.value, "Artifact");
    assert_eq!(artifact.variants.len(), 3);
    assert!(artifact.variants[0].tuple.is_some());
    assert!(artifact.variants[1].fields.is_some());
    assert!(artifact.variants[2].tuple.is_none() && artifact.variants[2].fields.is_none());

    // Pair<A, B>: generic params bind as TypeParam (fields resolve to them).
    let pair = &structs[3];
    let generics = pair.generics.as_ref().expect("Pair is generic");
    assert_eq!(
        generics.params.iter().map(|p| p.value.as_str()).collect::<Vec<_>>(),
        ["A", "B"]
    );
    let a_param = b.symbol_at(generics.params[0].span.start).expect("A defined");
    assert_eq!(b.symbol(a_param).kind, SymbolKind::TypeParam);
    assert_eq!(b.references(a_param).len(), 1); // `first: A`

    // classify: match with guard, struct pattern shorthand + rest, unit variant.
    let fns: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Fn(f) => Some(f),
            _ => None,
        })
        .collect();
    let classify = fns.iter().find(|f| f.name.value == "classify").unwrap();
    let Some(Expr::Match(m)) = &classify.body.tail else {
        panic!("classify body is a match");
    };
    assert!(m.arms[0].guard.is_some());
    let Pattern::Struct(archive) = &m.arms[2].pattern else {
        panic!("third arm is a struct pattern");
    };
    assert_eq!(archive.fields[0].name.value, "name");
    assert!(archive.fields[0].pattern.is_none(), "shorthand binds");
    assert_eq!(archive.rests.len(), 1);
    // The shorthand-bound `name` is referenced by the arm value.
    let name_binding = b.symbol_at(archive.fields[0].name.span.start).unwrap();
    assert_eq!(b.symbol(name_binding).kind, SymbolKind::Binding);
    assert_eq!(b.references(name_binding).len(), 1);

    // apply: fn type in a param.
    let apply = fns.iter().find(|f| f.name.value == "apply").unwrap();
    assert!(matches!(apply.params.params[0].ty, Type::Fn(_)));

    // toolchain: struct literal + record update + tuple + tuple index.
    let toolchain_fn = fns.iter().find(|f| f.name.value == "toolchain").unwrap();
    let Stmt::Let(base) = &toolchain_fn.body.stmts[0] else {
        panic!("first stmt is let base");
    };
    let Expr::StructLit(lit) = &base.value else {
        panic!("base is a struct literal");
    };
    assert_eq!(lit.fields.len(), 2);
    assert!(lit.spreads.is_empty());
    let Stmt::Let(tuned) = &toolchain_fn.body.stmts[1] else {
        panic!("second stmt is let tuned");
    };
    let Expr::Match(m) = &tuned.value else {
        panic!("tuned is a match");
    };
    let Expr::StructLit(update) = &m.arms[0].value else {
        panic!("windows arm is a record update");
    };
    assert_eq!(update.spreads.len(), 1);
    // tail: `Toolchain { env: flags, ..pair.0 }` — map-valued field + record
    // update whose base is a tuple index.
    let Some(Expr::StructLit(tail)) = &toolchain_fn.body.tail else {
        panic!("tail is a record update");
    };
    assert_eq!(tail.fields.len(), 1);
    assert_eq!(tail.spreads.len(), 1);
    let Some(Expr::Field(idx)) = &tail.spreads[0].base else {
        panic!("spread base is pair.0");
    };
    assert!(matches!(&idx.name, vix::ast::Member::Index(n) if n.value == "0"));

    // `let flags = { "CFLAGS": …, "LDFLAGS": … };` — a map literal.
    let Stmt::Let(flags) = &toolchain_fn.body.stmts[3] else {
        panic!("fourth stmt is let flags");
    };
    let Expr::Map(map) = &flags.value else {
        panic!("flags is a map literal");
    };
    assert_eq!(map.entries.len(), 2);

    // partials: `scaled(k: 2, ..)` is a partial call.
    let partials = fns.iter().find(|f| f.name.value == "partials").unwrap();
    let Stmt::Let(double) = &partials.body.stmts[0] else {
        panic!("let double");
    };
    let Expr::Call(call) = &double.value else {
        panic!("double is a call");
    };
    assert!(
        call.args
            .args
            .iter()
            .any(|a| matches!(a, vix::ast::Arg::Partial(_))),
        "trailing `..` marks the call partial"
    );

    // depths: `deep.0.1` nests tuple indices (no float token forms).
    let depths = fns.iter().find(|f| f.name.value == "depths").unwrap();
    let Some(Expr::Field(outer)) = &depths.body.tail else {
        panic!("tail is deep.0.1");
    };
    assert!(matches!(&outer.name, vix::ast::Member::Index(n) if n.value == "1"));
    let Expr::Field(inner) = &outer.receiver else {
        panic!("receiver is deep.0");
    };
    assert!(matches!(&inner.name, vix::ast::Member::Index(n) if n.value == "0"));
}

#[test]
fn eval_self_hosting_shape_binds_clean() {
    let src = sample("eval.vix");
    let file = VixParser::new().parse(&src).expect("eval.vix parses");
    let b = binder::bind(&file);

    assert_eq!(
        b.unresolved().len(),
        0,
        "self-hosting shape resolves fully: {:?}",
        b.unresolved()
    );

    // The recursive enum: Expr referenced from its own variants.
    let expr_ty = b
        .symbols()
        .find(|(_, s)| s.name == "Expr" && s.kind == SymbolKind::Type)
        .map(|(id, _)| id)
        .expect("enum Expr declared");
    assert!(
        b.references(expr_ty).len() >= 5,
        "Expr is referenced recursively + in eval's signature"
    );

    // Pattern payload bindings: `Expr::Let { name, value, body }` binds three
    // names, each used in the arm value.
    let fns: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Fn(f) => Some(f),
            _ => None,
        })
        .collect();
    let eval_fn = fns.iter().find(|f| f.name.value == "eval").unwrap();
    let Some(Expr::Match(m)) = &eval_fn.body.tail else {
        panic!("eval body is a match");
    };
    let Pattern::Struct(let_pat) = &m.arms[4].pattern else {
        panic!("last arm is Expr::Let {{ … }}");
    };
    for f in &let_pat.fields {
        let binding = b.symbol_at(f.name.span.start).expect("shorthand binds");
        assert_eq!(b.symbol(binding).kind, SymbolKind::Binding);
        assert!(
            !b.references(binding).is_empty(),
            "`{}` is used in the arm value",
            f.name.value
        );
    }
}
