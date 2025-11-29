//! Uncomment lines to see error messages

fn main() {
    // === UNKNOWN ATTRIBUTE (typo detection) ===
    // let _ = proto_ext::__parse_attr!(skp);
    // → "unknown attribute `skp`, did you mean `skip`?"

    // === SKIP ERRORS ===
    // let _ = proto_ext::__parse_attr!(skip("foo"));
    // → "`skip` does not take arguments; use just `skip`"  ✓

    // let _ = proto_ext::__parse_attr!(rename);
    // → "`rename` requires a string value..."  ✓

    // let _ = proto_ext::__parse_attr!(column(nam = "id"));
    // → "unknown field `nam`, did you mean `name`?"  ✓

    // let _ = proto_ext::__parse_attr!(skip = true);
    // → "`skip` does not take a value; use just `skip`"

    // === RENAME ERRORS ===
    // let _ = proto_ext::__parse_attr!(rename);
    // → "`rename` requires a string value: `rename(\"name\")` or `rename = \"name\"`"

    // let _ = proto_ext::__parse_attr!(rename());
    // → "`rename` requires a string value: `rename(\"name\")`"

    // let _ = proto_ext::__parse_attr!(rename =);
    // → "`rename` requires a value after `=`: `rename = \"name\"`"

    // let _ = proto_ext::__parse_attr!(rename(foo, bar));
    // → "`rename` takes exactly one string literal: `rename(\"name\")`"

    // === COLUMN ERRORS ===
    // let _ = proto_ext::__parse_attr!(column = "foo");
    // → "`column` uses parentheses syntax: `column(name = \"...\", primary_key)`"

    // let _ = proto_ext::__parse_attr!(column(nam = "id"));
    // → "unknown field `nam` in `Column`, did you mean `name`?"

    // let _ = proto_ext::__parse_attr!(column(name));
    // → "`name` requires a string value: `name = \"column_name\"`"

    // let _ = proto_ext::__parse_attr!(column(name("foo")));
    // → "`name` uses equals syntax: `name = \"column_name\"`, not `name(...)`"

    println!("Uncomment lines above to test error messages");
}
