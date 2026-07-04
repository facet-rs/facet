//! Oracle: the lua.vix sketch (the playground corpus, the first vix design target)
//! parses into the grammar-derived typed AST with the structure the sketch means.

use vix::ast::{Arg, ArrayElem, CommandPart, Expr, Item, Pattern, Stmt, Type};

fn lua_source() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/lua.vix"
    ))
    .expect("read lua.vix corpus")
}

#[test]
fn lua_sketch_lowers_to_typed_ast() {
    let parser = vix::VixParser::new();
    let source = lua_source();
    let file = parser.parse(&source).expect("lua.vix parses");

    // use vix::{Tree, Path, Target}; use caps::{Cc, Ar}; fn sources; fn object; pub fn lua
    assert_eq!(file.items.len(), 5);
    let Item::Use(use_vix) = &file.items[0] else {
        panic!("item 0 is `use vix::…`");
    };
    let names = |xs: &[vix::support::Spanned<String>]| {
        xs.iter().map(|s| s.value.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(&use_vix.tree.segments), ["vix"]);
    assert_eq!(names(&use_vix.tree.leaves), ["Tree", "Path", "Target"]);

    let fns: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Fn(f) => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(fns.len(), 3);
    let (sources, object, lua) = (&fns[0], &fns[1], &fns[2]);

    // fn sources() -> Tree { let tar = …; extract(tar) / p"lua-5.4.8/src" }
    assert_eq!(sources.name.value, "sources");
    assert_eq!(sources.vis, None);
    assert_eq!(sources.body.stmts.len(), 1);
    let Some(Expr::Binary(join)) = &sources.body.tail else {
        panic!("sources tail is a `/` join");
    };
    assert_eq!(join.op, "/");
    let Expr::Path(p) = &join.right else {
        panic!("join rhs is a path literal");
    };
    assert_eq!(p.value, "lua-5.4.8/src");

    // fn object(cc: Cc, src: Tree, unit: Path, defines: [Flag]) -> Tree { cc! { … } }
    assert_eq!(object.name.value, "object");
    let params = &object.params.params;
    assert_eq!(
        params
            .iter()
            .map(|p| p.name.value.as_str())
            .collect::<Vec<_>>(),
        ["cc", "src", "unit", "defines"]
    );
    let Type::Array(defines_ty) = &params[3].ty else {
        panic!("defines is [Flag]");
    };
    let Type::Path(flag) = &defines_ty.elem else {
        panic!("array elem is a path type");
    };
    assert_eq!(names(&flag.segments), ["Flag"]);
    assert!(object.body.stmts.is_empty());
    let Some(Expr::Command(cc)) = &object.body.tail else {
        panic!("object tail is cc! {{…}}");
    };
    assert_eq!(cc.command.value, "cc");
    // -O2 -Wall {defines} -I {src} -c {src / unit} -o {unit.with_ext("o")}
    assert_eq!(cc.parts.len(), 9);
    assert_eq!(
        cc.parts
            .iter()
            .filter(|p| matches!(p, CommandPart::Splice(_)))
            .count(),
        4
    );

    // pub fn lua(target: Target) -> Tree
    assert_eq!(lua.name.value, "lua");
    assert_eq!(
        lua.vis
            .as_ref()
            .map(|span| &source[span.start as usize..span.end as usize]),
        Some("pub")
    );
    assert_eq!(lua.body.stmts.len(), 8);

    // let defines = match target.os { Linux => [-DLUA_USE_LINUX], Macos => …, _ => [] };
    let Stmt::Let(defines) = &lua.body.stmts[3] else {
        panic!("stmt 3 is `let defines`");
    };
    assert_eq!(defines.name.value, "defines");
    let Expr::Match(m) = &defines.value else {
        panic!("defines is a match");
    };
    assert_eq!(m.arms.len(), 3);
    let Pattern::Identifier(first) = &m.arms[0].pattern else {
        panic!("first arm matches Linux");
    };
    assert_eq!(first.value, "Linux");
    let Expr::Array(linux_flags) = &m.arms[0].value else {
        panic!("Linux arm yields an array");
    };
    assert_eq!(linux_flags.elems.len(), 1);
    let ArrayElem::Flag(flag) = &linux_flags.elems[0] else {
        panic!("array elem is a flag");
    };
    assert_eq!(flag.value, "-DLUA_USE_LINUX");
    assert!(matches!(m.arms[2].pattern, Pattern::Wildcard(_)));
    let Expr::Array(empty) = &m.arms[2].value else {
        panic!("wildcard arm yields an array");
    };
    assert!(empty.elems.is_empty());

    // let units = src.glob("*.c").filter(|u| u != p"lua.c" && …);
    let Stmt::Let(units) = &lua.body.stmts[4] else {
        panic!("stmt 4 is `let units`");
    };
    let Expr::MethodCall(filter) = &units.value else {
        panic!("units is a method-call chain");
    };
    assert_eq!(filter.name.value, "filter");
    let Some(Arg::Expr(Expr::Closure(pred))) = filter.args.args.first() else {
        panic!("filter takes a closure");
    };
    assert_eq!(names(&pred.params), ["u"]);

    // tail: cc! { -o lua {main / p"lua.o"} {lib / p"liblua.a"} -lm }
    let Some(Expr::Command(link)) = &lua.body.tail else {
        panic!("lua tail is cc! {{…}}");
    };
    assert_eq!(link.parts.len(), 5);
    let CommandPart::Splice(main_splice) = &link.parts[2] else {
        panic!("part 2 splices {{main / p\"lua.o\"}}");
    };
    let Expr::Binary(main_join) = &main_splice.expr else {
        panic!("splice holds a `/` join");
    };
    assert_eq!(main_join.op, "/");
}
