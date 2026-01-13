fn main() {
    let cases = [
        ("unit @", "foo @"),
        ("tag no payload", "foo @blah"),
        ("tag with @ payload", "foo @blah@"),
        ("tag with seq payload", "foo @blah(x)"),
        ("tag with obj payload", "foo @blah{x y}"),
    ];

    for (name, source) in cases {
        println!("=== {name}: `{source}` ===");
        match styx_tree::parse(source) {
            Ok(v) => println!("{v:#?}\n"),
            Err(e) => println!("Error: {e:?}\n"),
        }
    }
}
