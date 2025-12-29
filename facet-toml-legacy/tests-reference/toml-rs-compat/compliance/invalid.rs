use snapbox::assert_data_eq;
use snapbox::prelude::*;
use snapbox::str;

#[track_caller]
fn t(toml: &str, expected: impl IntoData) {
    dbg!(toml);
    match toml.parse::<crate::RustDocument>() {
        Ok(s) => panic!("parsed to: {s:#?}"),
        Err(e) => assert_data_eq!(e.to_string(), expected.raw()),
    }
}

#[test]
fn basic_string_escape() {
    t(
        "a = \"\u{7f}\"",
        str![[r#"
TOML parse error at line 1, column 6
  |
1 | a = ""
  |      ^
invalid basic string, expected non-double-quote visible characters, `\`

"#]],
    );
}

#[test]
fn literal_escape() {
    t(
        "a = '\u{7f}'",
        str![[r#"
TOML parse error at line 1, column 6
  |
1 | a = ''
  |      ^
invalid literal string, expected non-single-quote visible characters

"#]],
    );
}

#[test]
fn stray_cr() {
    t(
        "\r",
        str![[r#"
TOML parse error at line 1, column 2
  |
1 | 
  |  ^
carriage return must be followed by newline, expected newline

"#]],
    );
    t(
        "a = [ \r ]",
        str![[r#"
TOML parse error at line 1, column 8
  |
1 | a = [ 
 ]
  |        ^
carriage return must be followed by newline, expected newline

"#]],
    );
    t(
        "a = \"\"\"\r\"\"\"",
        str![[r#"
TOML parse error at line 1, column 9
  |
1 | a = """
"""
  |         ^
carriage return must be followed by newline, expected newline

"#]],
    );
    t(
        "a = \"\"\"\\  \r  \"\"\"",
        str![[r#"
TOML parse error at line 1, column 12
  |
1 | a = """\  
  """
  |            ^
carriage return must be followed by newline, expected newline

"#]],
    );
    t(
        "a = '''\r'''",
        str![[r#"
TOML parse error at line 1, column 9
  |
1 | a = '''
'''
  |         ^
carriage return must be followed by newline, expected newline

"#]],
    );
    t(
        "a = '\r'",
        str![[r#"
TOML parse error at line 1, column 6
  |
1 | a = '
'
  |      ^
invalid literal string, expected non-single-quote visible characters

"#]],
    );
    t(
        "a = \"\r\"",
        str![[r#"
TOML parse error at line 1, column 6
  |
1 | a = "
"
  |      ^
invalid basic string, expected non-double-quote visible characters, `\`

"#]],
    );
}

#[test]
fn duplicate_key_with_crlf() {
    t(
        "\r\n\
         [t1]\r\n\
         [t2]\r\n\
         a = 1\r\n\
         a = 2\r\n\
         ",
        str![[r#"
TOML parse error at line 5, column 1
  |
5 | a = 2
  | ^
duplicate key

"#]],
    );
}

#[test]
fn emoji_error_span() {
    let input = "key = 😀";
    dbg!(input);
    let err = input.parse::<crate::RustDocument>().unwrap_err();
    dbg!(&err);
    let actual = &input[err.span().unwrap()];
    assert_eq!(actual, "😀");
}

#[test]
fn text_error_span() {
    let input = "key = asdf";
    dbg!(input);
    let err = input.parse::<crate::RustDocument>().unwrap_err();
    dbg!(&err);
    let actual = &input[err.span().unwrap()];
    assert_eq!(actual, "asdf");
}

#[test]
fn fuzzed_68144_error_span() {
    let input = "key = \"\\ᾂr\"";
    dbg!(input);
    let err = input.parse::<crate::RustDocument>().unwrap_err();
    dbg!(&err);
    let actual = &input[err.span().unwrap()];
    // atm bad escape values are reported as missing escape values
    assert_eq!(actual, "");
}
