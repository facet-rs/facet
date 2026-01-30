//! DO NOT DELETE ANY TESTS OR WEAKEN THEM IN ANY WAY

use super::*;
use crate::{ParseErrorKind, ScalarKind};
use facet_testhelpers::test;
use styx_testhelpers::{ActualError, assert_annotated_errors, source_without_annotations};
use tracing::trace;

fn parse(source: &str) -> Vec<Event<'_>> {
    Parser::new(source).parse_to_vec()
}

fn error_kind_name(kind: &ParseErrorKind) -> &'static str {
    match kind {
        ParseErrorKind::UnexpectedToken => "UnexpectedToken",
        ParseErrorKind::UnclosedObject => "UnclosedObject",
        ParseErrorKind::UnclosedSequence => "UnclosedSequence",
        ParseErrorKind::InvalidEscape(_) => "InvalidEscape",
        ParseErrorKind::ExpectedKey => "ExpectedKey",
        ParseErrorKind::ExpectedValue => "ExpectedValue",
        ParseErrorKind::UnexpectedEof => "UnexpectedEof",
        ParseErrorKind::DuplicateKey { .. } => "DuplicateKey",
        ParseErrorKind::InvalidTagName => "InvalidTagName",
        ParseErrorKind::InvalidKey => "InvalidKey",
        ParseErrorKind::DanglingDocComment => "DanglingDocComment",
        ParseErrorKind::TooManyAtoms => "TooManyAtoms",
        ParseErrorKind::ReopenedPath { .. } => "ReopenedPath",
        ParseErrorKind::NestIntoTerminal { .. } => "NestIntoTerminal",
        ParseErrorKind::CommaInSequence => "CommaInSequence",
        ParseErrorKind::MissingWhitespaceBeforeBlock => "MissingWhitespaceBeforeBlock",
        ParseErrorKind::TrailingContent => "TrailingContent",
    }
}

fn assert_parse_errors(annotated_source: &str) {
    let source = source_without_annotations(annotated_source);
    let events = parse(&source);
    let actual_errors: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Error { span, kind } => Some(ActualError {
                span: (*span).into(),
                kind: error_kind_name(kind).to_string(),
            }),
            _ => None,
        })
        .collect();
    assert_annotated_errors(annotated_source, actual_errors);
}

#[test]
fn test_empty_document() {
    let events = parse("");
    assert!(events.contains(&Event::DocumentStart));
    assert!(events.contains(&Event::DocumentEnd));
}

#[test]
fn test_simple_entry() {
    let events = parse("foo bar");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Key { payload: Some(v), .. } if v == "foo"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "bar"))
    );
}

#[test]
fn test_key_only() {
    let events = parse("foo");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Key { payload: Some(v), .. } if v == "foo"))
    );
    assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
}

#[test]
fn test_multiple_entries() {
    let events = parse("foo bar\nbaz qux");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(v), ..
            } => Some(v.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec!["foo", "baz"]);
}

#[test]
fn test_quoted_string() {
    let events = parse(r#"name "hello world""#);
    assert!(events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, kind: ScalarKind::Quoted, .. } if value == "hello world")));
}

#[test]
fn test_quoted_escape() {
    let events = parse(r#"msg "hello\nworld""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "hello\nworld"))
    );
}

#[test]
fn test_too_many_atoms() {
    let events = parse("a b c");
    eprintln!("EVENTS:");
    for (i, e) in events.iter().enumerate() {
        eprintln!("  {i}: {e:?}");
    }
    assert_parse_errors(
        r#"
a b c
    ^ TooManyAtoms
"#,
    );
}

#[test]
fn test_too_many_atoms_in_object() {
    assert_parse_errors(
        r#"
{label ": BIGINT" line 4}
                  ^^^^ TooManyAtoms
"#,
    );
}

#[test]
fn test_unit_value() {
    let events = parse("flag @");
    for _e in &events {
        trace!(?_e, "event");
    }
    assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
}

#[test]
fn test_unit_key() {
    let events = parse("@ value");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Key {
            payload: None,
            tag: None,
            ..
        }
    )));
}

#[test]
fn test_tag() {
    let events = parse("type @user");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "user"))
    );
}

#[test]
fn test_comments() {
    let events = parse("// comment\nfoo bar");
    assert!(events.iter().any(|e| matches!(e, Event::Comment { .. })));
}

#[test]
fn test_doc_comments() {
    let events = parse("/// doc\nfoo bar");
    assert!(events.iter().any(|e| matches!(e, Event::DocComment { .. })));
}

#[test]
fn test_doc_comment_at_eof_error() {
    assert_parse_errors(
        r#"
foo bar
/// dangling
^^^^^^^^^^^^ DanglingDocComment
"#,
    );
}

#[test]
fn test_nested_object() {
    let events = parse("outer {inner {x 1}}");
    let obj_starts = events
        .iter()
        .filter(|e| matches!(e, Event::ObjectStart { .. }))
        .count();
    assert!(obj_starts >= 2);
}

#[test]
fn test_sequence_elements() {
    let events = parse("items (a b c)");
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Scalar { value, .. } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(scalars.contains(&"a"));
    assert!(scalars.contains(&"b"));
    assert!(scalars.contains(&"c"));
}

#[test]
fn test_tagged_object() {
    let events = parse("result @err{message oops}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "err"))
    );
}

#[test]
fn test_tagged_explicit_unit() {
    let events = parse("nothing @empty@");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "empty"))
    );
}

#[test]
fn test_simple_attribute() {
    let events = parse("server host>localhost");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(v), ..
            } => Some(v.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"server"));
    assert!(keys.contains(&"host"));
}

#[test]
fn test_multiple_attributes() {
    let events = parse("server host>localhost port>8080");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(v), ..
            } => Some(v.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"server"));
    assert!(keys.contains(&"host"));
    assert!(keys.contains(&"port"));
}

#[test]
fn test_attribute_with_object_value() {
    let events = parse("config opts>{x 1}");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(v), ..
            } => Some(v.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"config"));
    assert!(keys.contains(&"opts"));
    assert!(keys.contains(&"x"));
}

#[test]
fn test_attribute_with_sequence_value() {
    let events = parse("config tags>(a b c)");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SequenceStart { .. }))
    );
}

#[test]
fn test_attribute_with_tag_value() {
    let events = parse("config status>@ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "ok"))
    );
}

#[test]
fn test_tag_with_dot_invalid() {
    assert_parse_errors(
        r#"
@Some.Type
^^^^^^^^^^ InvalidTagName
"#,
    );
}

#[test]
fn test_invalid_tag_name_starts_with_digit() {
    assert_parse_errors(
        r#"
x @123
  ^^^^ InvalidTagName
"#,
    );
}

#[test]
fn test_unicode_escape_braces() {
    let events = parse(r#"x "\u{1F600}""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "😀"))
    );
}

#[test]
fn test_unicode_escape_4digit() {
    let events = parse(r#"x "\u0041""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "A"))
    );
}

#[test]
fn test_heredoc_key_rejected() {
    assert_parse_errors(
        r#"
<<EOF
^^^^^^ InvalidKey
key
EOF
 value
"#,
    );
}

#[test]
fn test_missing_comma_rejected() {
    assert_parse_errors(
        r#"
{server {host localhost port 8080}}
                        ^^^^ TooManyAtoms
"#,
    );
}

#[test]
fn test_bare_scalar_is_string() {
    let events = parse("port 8080");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "8080"))
    );
}

#[test]
fn test_bool_like_is_string() {
    let events = parse("enabled true");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "true"))
    );
}

// Additional tests from old Parser

#[test]
fn test_doc_comment_followed_by_entry_ok() {
    assert_parse_errors("/// documentation\nkey value");
}

#[test]
fn test_doc_comment_before_closing_brace_error() {
    assert_parse_errors(
        r#"
{foo bar
/// dangling
^^^^^^^^^^^^ DanglingDocComment
}
"#,
    );
}

#[test]
fn test_multiple_doc_comments_before_entry_ok() {
    assert_parse_errors("/// line 1\n/// line 2\nkey value");
}

#[test]
fn test_multiline_doc_comment_in_object() {
    // Multiple consecutive doc comments inside a braced object should be joined into one event
    let source = "schema {\n    /// First line\n    /// Second line\n    /// Third line\n    field @string\n}";
    let events = parse(source);
    let doc_comments: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::DocComment { lines, .. } => Some(lines.clone()),
            _ => None,
        })
        .collect();
    // Should be one event with all lines, without the `/// ` prefix
    assert_eq!(
        doc_comments,
        vec![vec!["First line", "Second line", "Third line"]],
        "consecutive doc comments should be joined into one event"
    );
}

#[test]
fn test_object_with_entries() {
    let events = parse("config {host localhost, port 8080}");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"config"));
    assert!(keys.contains(&"host"));
    assert!(keys.contains(&"port"));
}

#[test]
fn test_nested_sequences() {
    let events = parse("matrix ((1 2) (3 4))");
    let seq_starts = events
        .iter()
        .filter(|e| matches!(e, Event::SequenceStart { .. }))
        .count();
    assert_eq!(seq_starts, 3);
}

#[test]
fn test_tagged_sequence() {
    let events = parse("color @rgb(255 128 0)");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "rgb"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SequenceStart { .. }))
    );
}

#[test]
fn test_tagged_scalar() {
    let events = parse(r#"name @nickname"Bob""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "nickname"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "Bob"))
    );
}

#[test]
fn test_tag_whitespace_gap() {
    let events = parse("x @tag\ny {a b}");
    let tag_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::TagStart { .. } | Event::TagEnd))
        .collect();
    assert_eq!(tag_events.len(), 2);
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"x"));
    assert!(keys.contains(&"y"));
}

#[test]
fn test_object_in_sequence() {
    let events = parse("servers ({host a} {host b})");
    let obj_starts = events
        .iter()
        .filter(|e| matches!(e, Event::ObjectStart { .. }))
        .count();
    // 2 = 2 objects in sequence (implicit root doesn't emit ObjectStart)
    assert_eq!(obj_starts, 2);
}

#[test]
fn test_attribute_values() {
    let events = parse("config name>app tags>(a b) opts>{x 1}");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"config"));
    assert!(keys.contains(&"name"));
    assert!(keys.contains(&"tags"));
    assert!(keys.contains(&"opts"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SequenceStart { .. }))
    );
}

#[test]
fn test_too_many_atoms_with_attributes() {
    assert_parse_errors(
        r#"
spec selector matchLabels app>web tier>frontend
              ^^^^^^^^^^^ TooManyAtoms
"#,
    );
}

#[test]
fn test_attribute_no_spaces() {
    let events = parse("x > y");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"x"));
}

#[test]
fn test_explicit_root_after_comment() {
    let events = parse("// comment\n{a 1}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ObjectStart { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Key { payload: Some(value), .. } if value == "a"))
    );
}

#[test]
fn test_explicit_root_after_doc_comment() {
    let events = parse("/// doc comment\n{a 1}");
    assert!(events.iter().any(|e| matches!(e, Event::DocComment { .. })));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ObjectStart { .. }))
    );
}

#[test]
fn test_duplicate_bare_key() {
    assert_parse_errors(
        r#"
{a 1, a 2}
      ^ DuplicateKey
"#,
    );
}

#[test]
fn test_duplicate_quoted_key() {
    assert_parse_errors(
        r#"
{"key" 1, "key" 2}
          ^^^^^ DuplicateKey
"#,
    );
}

#[test]
fn test_duplicate_key_escape_normalized() {
    assert_parse_errors(
        r#"
{"ab" 1, "a\u{62}" 2}
         ^^^^^^^^^ DuplicateKey
"#,
    );
}

#[test]
fn test_duplicate_unit_key() {
    assert_parse_errors(
        r#"
{@ 1, @ 2}
      ^ DuplicateKey
"#,
    );
}

#[test]
fn test_duplicate_tagged_key() {
    assert_parse_errors(
        r#"
{@foo 1, @foo 2}
         ^^^^ DuplicateKey
"#,
    );
}

#[test]
fn test_different_keys_ok() {
    assert_parse_errors(r#"{a 1, b 2, c 3}"#);
}

#[test]
fn test_duplicate_key_at_root() {
    assert_parse_errors(
        r#"
a 1
a 2
^ DuplicateKey
"#,
    );
}

#[test]
fn test_mixed_separators_allowed() {
    // Mixed separators (commas and newlines) are allowed
    assert_parse_errors(
        r#"
{a 1, b 2
c 3}
"#,
    );
    assert_parse_errors(
        r#"
{a 1
b 2, c 3}
"#,
    );
}

#[test]
fn test_consistent_comma_separators() {
    assert_parse_errors(r#"{a 1, b 2, c 3}"#);
}

#[test]
fn test_consistent_newline_separators() {
    assert_parse_errors(
        r#"{a 1
b 2
c 3}"#,
    );
}

#[test]
fn test_valid_tag_names() {
    assert_parse_errors("@foo");
    assert_parse_errors("@_private");
    assert_parse_errors("@my-tag");
    assert_parse_errors("@Type123");
}

#[test]
fn test_invalid_tag_name_starts_with_hyphen() {
    assert_parse_errors(
        r#"
x @-foo
  ^^^^^ InvalidTagName
"#,
    );
}

#[test]
fn test_invalid_tag_name_starts_with_dot() {
    assert_parse_errors(
        r#"
x @.foo
  ^^^^^ InvalidTagName
"#,
    );
}

#[test]
fn test_unicode_escape_4digit_accented() {
    let events = parse(r#"x "\u00E9""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "é"))
    );
}

#[test]
fn test_unicode_escape_mixed() {
    let events = parse(r#"x "\u0048\u{65}\u006C\u{6C}\u006F""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "Hello"))
    );
}

#[test]
fn test_invalid_escape_null() {
    assert_parse_errors(
        r#"
x "\0"
   ^^ InvalidEscape
"#,
    );
}

#[test]
fn test_invalid_escape_unknown() {
    assert_parse_errors(
        r#"
x "\q"
   ^^ InvalidEscape
"#,
    );
}

#[test]
fn test_invalid_escape_multiple() {
    assert_parse_errors(
        r#"
x "\0\q\?"
   ^^ InvalidEscape
     ^^ InvalidEscape
       ^^ InvalidEscape
"#,
    );
}

#[test]
fn test_valid_escapes_still_work() {
    let events = parse(r#"x "a\nb\tc\\d\"e""#);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "a\nb\tc\\d\"e"))
    );
    assert_parse_errors(r#"x "a\nb\tc\\d\"e""#);
}

#[test]
fn test_invalid_escape_in_key() {
    assert_parse_errors(
        r#"
"\0" value
 ^^ InvalidEscape
"#,
    );
}

#[test]
fn test_simple_key_value_with_attributes() {
    let events = parse("server host>localhost port>8080");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"server"));
    assert!(keys.contains(&"host"));
    assert!(keys.contains(&"port"));
    assert_parse_errors(r#"server host>localhost port>8080"#);
}

#[test]
fn test_dotted_path_simple() {
    let events = parse("a.b value");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec!["a", "b"]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ObjectStart { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "value"))
    );
    assert_parse_errors(r#"a.b value"#);
}

#[test]
fn test_dotted_path_three_segments() {
    let events = parse("a.b.c deep");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec!["a", "b", "c"]);
    let obj_starts: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::ObjectStart { .. }))
        .collect();
    // 2 = 2 from dotted path (a { b { c deep } }) - implicit root doesn't emit ObjectStart
    assert_eq!(obj_starts.len(), 2);
    assert_parse_errors(r#"a.b.c deep"#);
}

#[test]
fn test_dotted_path_with_implicit_unit() {
    let events = parse("a.b");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec!["a", "b"]);
    assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
}

#[test]
fn test_dotted_path_empty_segment() {
    assert_parse_errors(
        r#"
a..b value
^^^^ InvalidKey
"#,
    );
}

#[test]
fn test_dotted_path_trailing_dot() {
    assert_parse_errors(
        r#"
a.b. value
^^^^ InvalidKey
"#,
    );
}

#[test]
fn test_dotted_path_leading_dot() {
    assert_parse_errors(
        r#"
.a.b value
^^^^ InvalidKey
"#,
    );
}

#[test]
fn test_dotted_path_with_object_value() {
    let events = parse("a.b { c d }");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"a"));
    assert!(keys.contains(&"b"));
    assert!(keys.contains(&"c"));
    assert_parse_errors(r#"a.b { c d }"#);
}

#[test]
fn test_dotted_path_with_attributes_value() {
    let events = parse("selector.matchLabels app>web");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"selector"));
    assert!(keys.contains(&"matchLabels"));
    assert!(keys.contains(&"app"));
    assert_parse_errors(r#"selector.matchLabels app>web"#);
}

#[test]
fn test_dot_in_value_is_literal() {
    let events = parse("key example.com");
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec!["key"]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value == "example.com"))
    );
    assert_parse_errors(r#"key example.com"#);
}

#[test]
fn test_sibling_dotted_paths() {
    let events = parse("foo.bar.x value1\nfoo.bar.y value2\nfoo.baz value3");
    assert_parse_errors(
        r#"foo.bar.x value1
foo.bar.y value2
foo.baz value3"#,
    );
    let keys: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                payload: Some(value),
                ..
            } => Some(value.as_ref()),
            _ => None,
        })
        .collect();
    assert!(keys.contains(&"foo"));
    assert!(keys.contains(&"bar"));
    assert!(keys.contains(&"baz"));
    assert!(keys.contains(&"x"));
    assert!(keys.contains(&"y"));
}

#[test]
fn test_reopen_closed_path_error() {
    assert_parse_errors(
        r#"
foo.bar {}
foo.baz {}
foo.bar.x value
^^^^^^^^^ ReopenedPath
"#,
    );
}

#[test]
fn test_reopen_nested_closed_path_error() {
    assert_parse_errors(
        r#"
a.b.c {}
a.b.d {}
a.x {}
a.b.e {}
^^^^^ ReopenedPath
"#,
    );
}

#[test]
fn test_nest_into_scalar_error() {
    assert_parse_errors(
        r#"
a.b value
a.b.c deep
^^^^^ NestIntoTerminal
"#,
    );
}

#[test]
fn test_different_top_level_paths_ok() {
    assert_parse_errors(
        r#"server.host localhost
database.port 5432"#,
    );
}

#[test]
fn test_bare_key_requires_whitespace_before_brace() {
    assert_parse_errors(
        r#"
config{}
      ^ MissingWhitespaceBeforeBlock
"#,
    );
}

#[test]
fn test_bare_key_requires_whitespace_before_paren() {
    assert_parse_errors(
        r#"
items(1 2 3)
     ^ MissingWhitespaceBeforeBlock
"#,
    );
}

#[test]
fn test_bare_key_with_whitespace_before_brace_ok() {
    assert_parse_errors("config {}");
}

#[test]
fn test_bare_key_with_whitespace_before_paren_ok() {
    assert_parse_errors("items (1 2 3)");
}

#[test]
fn test_tag_with_brace_no_whitespace_ok() {
    assert_parse_errors("config @object{}");
}

#[test]
fn test_quoted_key_no_whitespace_ok() {
    assert_parse_errors(r#""config"{}"#);
}

#[test]
fn test_minified_styx_with_whitespace() {
    assert_parse_errors("{server {host localhost,port 8080}}");
}

#[test]
fn test_invalid_escape_annotated() {
    assert_parse_errors(
        r#"
x "\0"
   ^^ InvalidEscape
"#,
    );
}

#[test]
fn test_invalid_tag_name_annotated() {
    assert_parse_errors(
        r#"
x @123
  ^^^^ InvalidTagName
"#,
    );
}

#[test]
fn test_dangling_doc_comment_annotated() {
    assert_parse_errors(
        r#"
foo bar
/// dangling
^^^^^^^^^^^^ DanglingDocComment
"#,
    );
}

#[test]
fn test_nested_tag_in_object() {
    // Complex nested structure with unit keys and tag values
    let input = r#"meta {id test}
schema {
    @ @object{
        name @optional(@string)
    }
}"#;
    let events = parse(input);
    let errors: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Error { .. }))
        .collect();
    assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
}

#[test]
fn test_tag_with_seq_containing_tag() {
    // Tag with sequence payload containing another tag
    let input = "x @outer(@inner)";
    let events = parse(input);
    let errors: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Error { .. }))
        .collect();
    assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
}

#[test]
fn test_tag_with_typed_literal_in_seq() {
    // This is the format used by schema_gen: @default(true @bool)
    // It's a sequence with two elements: true and @bool (unit tag)
    // We wrap it in an entry to test it as a value
    let input = "key @default(true @bool)";
    let events = parse(input);
    let errors: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Error { .. }))
        .collect();
    assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
}

#[test]
fn test_schema_with_comma_separated_entries() {
    // This is from the schema_gen tests - multiple entries in an object separated by commas
    let input = r#"schema {@ @object{inner @Inner}, Inner @object{enabled @bool}}"#;
    let events = parse(input);
    for event in &events {
        eprintln!("{:?}", event);
    }
    let errors: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Error { .. }))
        .collect();
    assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
}

#[test]
fn test_tag_unit_object_is_too_many_atoms() {
    // @a @ {} is three atoms: @a (tag), @ (unit), {} (object)
    // An entry can only have key + value (2 atoms max), so this is TooManyAtoms
    let input = "@a @ {}";
    let events = parse(input);

    let errors: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Error { kind, .. } => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], ParseErrorKind::TooManyAtoms));
}
