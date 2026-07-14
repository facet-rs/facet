fn tryp(name: &str, src: &str) {
    let p = vix::surface::SurfaceParser::new();
    match p.parse(src) {
        Ok(_) => eprintln!("{name}: PARSED OK"),
        Err(e) => eprintln!("{name}: ERR {:?}", e.entries.iter().map(|x| format!("{:?}", x.code)).collect::<Vec<_>>()),
    }
}
#[test]
fn dbg_parse_bisect() {
    tryp("turbofish-let", "#[test]\nfn f() -> Stream<Check> {\n  let bad = try_json_decode<PkgRow>(\"x\");\n  yield expect(true);\n}\n");
    tryp("match-okerr", "#[test]\nfn f() -> Stream<Check> {\n  yield match bad {\n    Ok(_) => expect(false),\n    Err(e) => expect(true),\n  };\n}\n");
    tryp("emsg", "#[test]\nfn f() -> Stream<Check> {\n  yield expect(e.message.contains(\"name\"));\n}\n");
    tryp("turbofish+match", "#[test]\nfn f() -> Stream<Check> {\n  let bad = try_json_decode<PkgRow>(\"x\");\n  yield match bad {\n    Ok(_) => expect(false),\n    Err(e) => expect(true),\n  };\n}\n");
}
