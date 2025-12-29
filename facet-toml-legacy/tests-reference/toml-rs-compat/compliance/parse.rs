use snapbox::assert_data_eq;
use snapbox::prelude::*;
use snapbox::str;

#[test]
fn test_value_from_str() {
    assert!(parse_value!("1979-05-27T00:32:00.999999-07:00").is_datetime());
    assert!(parse_value!("1979-05-27T00:32:00.999999Z").is_datetime());
    assert!(parse_value!("1979-05-27T00:32:00.999999").is_datetime());
    assert!(parse_value!("1979-05-27T00:32:00").is_datetime());
    assert!(parse_value!("1979-05-27").is_datetime());
    assert!(parse_value!("00:32:00").is_datetime());
    assert!(parse_value!("-239").is_integer());
    assert!(parse_value!("1e200").is_float());
    assert!(parse_value!("9_224_617.445_991_228_313").is_float());
    assert!(parse_value!(r#""basic string\nJos\u00E9\n""#).is_str());
    assert!(
        parse_value!(
            r#""""
multiline basic string
""""#
        )
        .is_str()
    );
    assert!(parse_value!(r"'literal string\ \'").is_str());
    assert!(
        parse_value!(
            r"'''multiline
literal \ \
string'''"
        )
        .is_str()
    );
    assert!(parse_value!(r#"{ hello = "world", a = 1}"#).is_table());
    assert!(
        parse_value!(r#"[ { x = 1, a = "2" }, {a = "a",b = "b",     c =    "c"} ]"#).is_array()
    );
    let wp = "C:\\Users\\appveyor\\AppData\\Local\\Temp\\1\\cargo-edit-test.YizxPxxElXn9";
    let lwp = "'C:\\Users\\appveyor\\AppData\\Local\\Temp\\1\\cargo-edit-test.YizxPxxElXn9'";
    assert_eq!(
        crate::RustValue::from(wp).as_str(),
        parse_value!(lwp).as_str()
    );
    assert!(parse_value!(r#""\\\"\b\f\n\r\t\u00E9\U000A0000""#).is_str());
}

#[test]
#[cfg(feature = "preserve_order")]
fn test_key_unification() {
    let toml = r#"
[a]
[a.'b'.c]
[a."b".c.e]
[a.b.c.d]
"#;
    let expected = str![[r#"
[a.b.c.e]

[a.b.c.d]

"#]];
    let doc = toml.parse::<crate::RustDocument>();
    assert!(doc.is_ok());
    let doc = doc.unwrap();

    assert_data_eq!(doc.to_string(), expected.raw());
}

#[test]
fn crlf() {
    "\
     [project]\r\n\
     \r\n\
     name = \"splay\"\r\n\
     version = \"0.1.0\"\r\n\
     authors = [\"alex@crichton.co\"]\r\n\
     \r\n\
     [[lib]]\r\n\
     \r\n\
     path = \"lib.rs\"\r\n\
     name = \"splay\"\r\n\
     description = \"\"\"\
     A Rust implementation of a TAR file reader and writer. This library does not\r\n\
     currently handle compression, but it is abstract over all I/O readers and\r\n\
     writers. Additionally, great lengths are taken to ensure that the entire\r\n\
     contents are never required to be entirely resident in memory all at once.\r\n\
     \"\"\"\
     "
    .parse::<crate::RustDocument>()
    .unwrap();
}

#[test]
fn fun_with_strings() {
    let table = r#"
bar = "\U00000000"
key1 = "One\nTwo"
key2 = """One\nTwo"""
key3 = """
One
Two"""

key4 = "The quick brown fox jumps over the lazy dog."
key5 = """
The quick brown \


fox jumps over \
the lazy dog."""
key6 = """\
   The quick brown \
   fox jumps over \
   the lazy dog.\
   """
# What you see is what you get.
winpath  = 'C:\Users\nodejs\templates'
winpath2 = '\\ServerX\admin$\system32\'
quoted   = 'Tom "Dubs" Preston-Werner'
regex    = '<\i\c*\s*>'

regex2 = '''I [dw]on't need \d{2} apples'''
lines  = '''
The first newline is
trimmed in raw strings.
All other whitespace
is preserved.
'''
"#
    .parse::<crate::RustDocument>()
    .unwrap();
    assert_eq!(table["bar"].as_str(), Some("\0"));
    assert_eq!(table["key1"].as_str(), Some("One\nTwo"));
    assert_eq!(table["key2"].as_str(), Some("One\nTwo"));
    assert_eq!(table["key3"].as_str(), Some("One\nTwo"));

    let msg = "The quick brown fox jumps over the lazy dog.";
    assert_eq!(table["key4"].as_str(), Some(msg));
    assert_eq!(table["key5"].as_str(), Some(msg));
    assert_eq!(table["key6"].as_str(), Some(msg));

    assert_eq!(
        table["winpath"].as_str(),
        Some(r"C:\Users\nodejs\templates")
    );
    assert_eq!(
        table["winpath2"].as_str(),
        Some(r"\\ServerX\admin$\system32\")
    );
    assert_eq!(
        table["quoted"].as_str(),
        Some(r#"Tom "Dubs" Preston-Werner"#)
    );
    assert_eq!(table["regex"].as_str(), Some(r"<\i\c*\s*>"));
    assert_eq!(
        table["regex2"].as_str(),
        Some(r"I [dw]on't need \d{2} apples")
    );
    assert_eq!(
        table["lines"].as_str(),
        Some(
            "The first newline is\n\
             trimmed in raw strings.\n\
             All other whitespace\n\
             is preserved.\n"
        )
    );
}

#[test]
fn tables_in_arrays() {
    let table = r#"
[[foo]]
#…
[foo.bar]
#…

[[foo]] # ...
#…
[foo.bar]
#...
"#
    .parse::<crate::RustDocument>()
    .unwrap();
    table["foo"][0]["bar"].as_table().unwrap();
    table["foo"][1]["bar"].as_table().unwrap();
}

#[test]
fn empty_table() {
    let table = r#"
[foo]"#
        .parse::<crate::RustDocument>()
        .unwrap();
    table["foo"].as_table().unwrap();
}

#[test]
fn mixed_table_issue_527() {
    let input = r#"
[package]
metadata.msrv = "1.65.0"

[package.metadata.release.pre-release-replacements]
"#;
    let expected = str![[r#"
[package.metadata]
msrv = "1.65.0"

[package.metadata.release.pre-release-replacements]

"#]];
    let document = input.parse::<crate::RustDocument>().unwrap();
    let actual = document.to_string();
    assert_data_eq!(actual, expected.raw());
}

#[test]
fn fruit() {
    let table = r#"
[[fruit]]
name = "apple"

[fruit.physical]
color = "red"
shape = "round"

[[fruit.variety]]
name = "red delicious"

[[fruit.variety]]
name = "granny smith"

[[fruit]]
name = "banana"

[[fruit.variety]]
name = "plantain"
"#
    .parse::<crate::RustDocument>()
    .unwrap();
    assert_eq!(table["fruit"][0]["name"].as_str(), Some("apple"));
    assert_eq!(table["fruit"][0]["physical"]["color"].as_str(), Some("red"));
    assert_eq!(
        table["fruit"][0]["physical"]["shape"].as_str(),
        Some("round")
    );
    assert_eq!(
        table["fruit"][0]["variety"][0]["name"].as_str(),
        Some("red delicious")
    );
    assert_eq!(
        table["fruit"][0]["variety"][1]["name"].as_str(),
        Some("granny smith")
    );
    assert_eq!(table["fruit"][1]["name"].as_str(), Some("banana"));
    assert_eq!(
        table["fruit"][1]["variety"][0]["name"].as_str(),
        Some("plantain")
    );
}

#[test]
fn blank_literal_string() {
    let table = "foo = ''".parse::<crate::RustDocument>().unwrap();
    assert_eq!(table["foo"].as_str(), Some(""));
}

#[test]
fn many_blank() {
    let table = "foo = \"\"\"\n\n\n\"\"\""
        .parse::<crate::RustDocument>()
        .unwrap();
    assert_eq!(table["foo"].as_str(), Some("\n\n"));
}

#[test]
fn literal_eats_crlf() {
    let table = "
        foo = \"\"\"\\\r\n\"\"\"
        bar = \"\"\"\\\r\n   \r\n   \r\n   a\"\"\"
    "
    .parse::<crate::RustDocument>()
    .unwrap();
    assert_eq!(table["foo"].as_str(), Some(""));
    assert_eq!(table["bar"].as_str(), Some("a"));
}

#[test]
fn floats() {
    macro_rules! t {
        ($actual:expr, $expected:expr) => {{
            let f = format!("foo = {}", $actual);
            println!("{}", f);
            let a = f.parse::<crate::RustDocument>().unwrap();
            assert_eq!(a["foo"].as_float().unwrap(), $expected);
        }};
    }

    t!("1.0", 1.0);
    t!("1.0e0", 1.0);
    t!("1.0e+0", 1.0);
    t!("1.0e-0", 1.0);
    t!("1E-0", 1.0);
    t!("1.001e-0", 1.001);
    t!("2e10", 2e10);
    t!("2e+10", 2e10);
    t!("2e-10", 2e-10);
    t!("2_0.0", 20.0);
    t!("2_0.0_0e1_0", 20.0e10);
    t!("2_0.1_0e1_0", 20.1e10);
}

#[test]
fn bare_key_names() {
    let a = "
        foo = 3
        foo_3 = 3
        foo_-2--3--r23f--4-f2-4 = 3
        _ = 3
        - = 3
        8 = 8
        \"a\" = 3
        \"!\" = 3
        \"a^b\" = 3
        \"\\\"\" = 3
        \"character encoding\" = \"value\"
        'ʎǝʞ' = \"value\"
    "
    .parse::<crate::RustDocument>()
    .unwrap();
    let _ = &a["foo"];
    let _ = &a["-"];
    let _ = &a["_"];
    let _ = &a["8"];
    let _ = &a["foo_3"];
    let _ = &a["foo_-2--3--r23f--4-f2-4"];
    let _ = &a["a"];
    let _ = &a["!"];
    let _ = &a["\""];
    let _ = &a["character encoding"];
    let _ = &a["ʎǝʞ"];
}

#[test]
fn table_names() {
    let a = "
        [a.\"b\"]
        [\"f f\"]
        [\"f.f\"]
        [\"\\\"\"]
        ['a.a']
        ['\"\"']
    "
    .parse::<crate::RustDocument>()
    .unwrap();
    println!("{a:?}");
    let _ = &a["a"]["b"];
    let _ = &a["f f"];
    let _ = &a["f.f"];
    let _ = &a["\""];
    let _ = &a["\"\""];
}

#[test]
fn inline_tables() {
    "a = {}".parse::<crate::RustDocument>().unwrap();
    "a = {b=1}".parse::<crate::RustDocument>().unwrap();
    "a = {   b   =   1    }"
        .parse::<crate::RustDocument>()
        .unwrap();
    "a = {a=1,b=2}".parse::<crate::RustDocument>().unwrap();
    "a = {a=1,b=2,c={}}".parse::<crate::RustDocument>().unwrap();
    "a = {a=[\n]}".parse::<crate::RustDocument>().unwrap();
    "a = {\"a\"=[\n]}".parse::<crate::RustDocument>().unwrap();
    "a = [\n{},\n{},\n]".parse::<crate::RustDocument>().unwrap();
}

#[test]
fn number_underscores() {
    macro_rules! t {
        ($actual:expr, $expected:expr) => {{
            let f = format!("foo = {}", $actual);
            let table = f.parse::<crate::RustDocument>().unwrap();
            assert_eq!(table["foo"].as_integer().unwrap(), $expected);
        }};
    }

    t!("1_0", 10);
    t!("1_0_0", 100);
    t!("1_000", 1000);
    t!("+1_000", 1000);
    t!("-1_000", -1000);
}

#[test]
fn empty_string() {
    assert_eq!(
        "foo = \"\"".parse::<crate::RustDocument>().unwrap()["foo"]
            .as_str()
            .unwrap(),
        ""
    );
}

#[test]
fn datetimes() {
    macro_rules! t {
        ($actual:expr) => {{
            let f = format!("foo = {}", $actual);
            let toml = f
                .parse::<crate::RustDocument>()
                .expect(&format!("failed: {}", f));
            assert_eq!(toml["foo"].as_datetime().unwrap().to_string(), $actual);
        }};
    }

    t!("2016-09-09T09:09:09Z");
    t!("2016-09-09T09:09:09.1Z");
    t!("2016-09-09T09:09:09.2+10:00");
    t!("2016-09-09T09:09:09.123456789-02:00");
}

#[test]
fn dont_use_dotted_key_prefix_on_table_fuzz_57049() {
    // This could generate
    // ```toml
    // [
    // p.o]
    // ```
    let input = r#"
p.a=4
[p.o]
"#;
    let expected = str![[r#"
[p]
a = 4

[p.o]

"#]];
    let document = input.parse::<crate::RustDocument>().unwrap();
    let actual = document.to_string();
    assert_data_eq!(actual, expected.raw());
}

#[test]
#[cfg(feature = "preserve_order")]
fn dotted_key_comment_roundtrip() {
    let input = r###"
rust.unsafe_op_in_unsafe_fn = "deny"

rust.explicit_outlives_requirements = "warn"
# rust.unused_crate_dependencies = "warn"

clippy.cast_lossless = "warn"
clippy.doc_markdown = "warn"
clippy.exhaustive_enums = "warn"
"###;
    let expected = str![[r#"
[rust]
unsafe_op_in_unsafe_fn = "deny"
explicit_outlives_requirements = "warn"

[clippy]
cast_lossless = "warn"
doc_markdown = "warn"
exhaustive_enums = "warn"

"#]];

    let manifest: crate::RustDocument = input.parse().unwrap();
    let actual = manifest.to_string();

    assert_data_eq!(actual, expected.raw());
}

#[test]
fn string_repr_roundtrip() {
    assert_string_repr_roundtrip(r#""""#, str![[r#""""#]]);
    assert_string_repr_roundtrip(r#""a""#, str![[r#""a""#]]);

    assert_string_repr_roundtrip(r#""tab \t tab""#, str![[r#""tab \t tab""#]]);
    assert_string_repr_roundtrip(
        r#""lf \n lf""#,
        str![[r#"
"""
lf 
 lf"""
"#]],
    );
    assert_string_repr_roundtrip(
        r#""crlf \r\n crlf""#,
        str![[r#"
"""
crlf \r
 crlf"""
"#]],
    );
    assert_string_repr_roundtrip(r#""bell \b bell""#, str![[r#""bell \b bell""#]]);
    assert_string_repr_roundtrip(r#""feed \f feed""#, str![[r#""feed \f feed""#]]);
    assert_string_repr_roundtrip(
        r#""backslash \\ backslash""#,
        str![[r#"'backslash \ backslash'"#]],
    );

    assert_string_repr_roundtrip(r#""squote ' squote""#, str![[r#""squote ' squote""#]]);
    assert_string_repr_roundtrip(
        r#""triple squote ''' triple squote""#,
        str![[r#""triple squote ''' triple squote""#]],
    );
    assert_string_repr_roundtrip(r#""end squote '""#, str![[r#""end squote '""#]]);

    assert_string_repr_roundtrip(r#""quote \" quote""#, str![[r#"'quote " quote'"#]]);
    assert_string_repr_roundtrip(
        r#""triple quote \"\"\" triple quote""#,
        str![[r#"'triple quote """ triple quote'"#]],
    );
    assert_string_repr_roundtrip(r#""end quote \"""#, str![[r#"'end quote "'"#]]);
    assert_string_repr_roundtrip(
        r#""quoted \"content\" quoted""#,
        str![[r#"'quoted "content" quoted'"#]],
    );
    assert_string_repr_roundtrip(
        r#""squoted 'content' squoted""#,
        str![[r#""squoted 'content' squoted""#]],
    );
    assert_string_repr_roundtrip(
        r#""mixed quoted \"start\" 'end'' mixed quote""#,
        str![[r#""""mixed quoted "start" 'end'' mixed quote""""#]],
    );
}

#[track_caller]
fn assert_string_repr_roundtrip(input: &str, expected: impl IntoData) {
    let value = parse_value!(input);
    let actual = value.to_string();
    let _ = parse_value!(&actual);
    assert_data_eq!(actual, expected.raw());
}

#[test]
fn string_value_roundtrip() {
    assert_string_value_roundtrip(r#""""#, str![[r#""""#]]);
    assert_string_value_roundtrip(r#""a""#, str![[r#""a""#]]);

    assert_string_value_roundtrip(r#""tab \t tab""#, str![[r#""tab \t tab""#]]);
    assert_string_value_roundtrip(
        r#""lf \n lf""#,
        str![[r#"
"""
lf 
 lf"""
"#]],
    );
    assert_string_value_roundtrip(
        r#""crlf \r\n crlf""#,
        str![[r#"
"""
crlf \r
 crlf"""
"#]],
    );
    assert_string_value_roundtrip(r#""bell \b bell""#, str![[r#""bell \b bell""#]]);
    assert_string_value_roundtrip(r#""feed \f feed""#, str![[r#""feed \f feed""#]]);
    assert_string_value_roundtrip(
        r#""backslash \\ backslash""#,
        str![[r#"'backslash \ backslash'"#]],
    );

    assert_string_value_roundtrip(r#""squote ' squote""#, str![[r#""squote ' squote""#]]);
    assert_string_value_roundtrip(
        r#""triple squote ''' triple squote""#,
        str![[r#""triple squote ''' triple squote""#]],
    );
    assert_string_value_roundtrip(r#""end squote '""#, str![[r#""end squote '""#]]);

    assert_string_value_roundtrip(r#""quote \" quote""#, str![[r#"'quote " quote'"#]]);
    assert_string_value_roundtrip(
        r#""triple quote \"\"\" triple quote""#,
        str![[r#"'triple quote """ triple quote'"#]],
    );
    assert_string_value_roundtrip(r#""end quote \"""#, str![[r#"'end quote "'"#]]);
    assert_string_value_roundtrip(
        r#""quoted \"content\" quoted""#,
        str![[r#"'quoted "content" quoted'"#]],
    );
    assert_string_value_roundtrip(
        r#""squoted 'content' squoted""#,
        str![[r#""squoted 'content' squoted""#]],
    );
    assert_string_value_roundtrip(
        r#""mixed quoted \"start\" 'end'' mixed quote""#,
        str![[r#""""mixed quoted "start" 'end'' mixed quote""""#]],
    );
}

#[track_caller]
fn assert_string_value_roundtrip(input: &str, expected: impl IntoData) {
    let value = parse_value!(input);
    let value = crate::RustValue::from(value.as_str().unwrap()); // Remove repr
    let actual = value.to_string();
    let _ = parse_value!(&actual);
    assert_data_eq!(actual, expected.raw());
}

#[test]
#[cfg(not(feature = "unbounded"))]
fn array_recursion_limit() {
    let depths = [(1, true), (20, true), (300, false)];
    for (depth, is_ok) in depths {
        let input = format!("x={}{}", &"[".repeat(depth), &"]".repeat(depth));
        let document = input.parse::<crate::RustDocument>();
        assert_eq!(document.is_ok(), is_ok, "depth: {depth}");
    }
}

#[test]
#[cfg(not(feature = "unbounded"))]
fn inline_table_recursion_limit() {
    let depths = [(1, true), (20, true), (300, false)];
    for (depth, is_ok) in depths {
        let input = format!("x={}true{}", &"{ x = ".repeat(depth), &"}".repeat(depth));
        let document = input.parse::<crate::RustDocument>();
        assert_eq!(document.is_ok(), is_ok, "depth: {depth}");
    }
}

#[test]
#[cfg(not(feature = "unbounded"))]
fn table_key_recursion_limit() {
    let depths = [(1, true), (20, true), (300, false)];
    for (depth, is_ok) in depths {
        let input = format!("[x{}]", &".x".repeat(depth));
        let document = input.parse::<crate::RustDocument>();
        assert_eq!(document.is_ok(), is_ok, "depth: {depth}");
    }
}

#[test]
#[cfg(not(feature = "unbounded"))]
fn dotted_key_recursion_limit() {
    let depths = [(1, true), (20, true), (300, false)];
    for (depth, is_ok) in depths {
        let input = format!("x{} = true", &".x".repeat(depth));
        let document = input.parse::<crate::RustDocument>();
        assert_eq!(document.is_ok(), is_ok, "depth: {depth}");
    }
}

#[test]
#[cfg(not(feature = "unbounded"))]
fn inline_dotted_key_recursion_limit() {
    let depths = [(1, true), (20, true), (300, false)];
    for (depth, is_ok) in depths {
        let input = format!("x = {{ x{} = true }}", &".x".repeat(depth));
        let document = input.parse::<crate::RustDocument>();
        assert_eq!(document.is_ok(), is_ok, "depth: {depth}");
    }
}
