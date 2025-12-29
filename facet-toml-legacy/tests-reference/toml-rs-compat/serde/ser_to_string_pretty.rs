use serde::Deserialize;
use serde::Serialize;
use snapbox::assert_data_eq;
use snapbox::prelude::*;
use snapbox::str;

#[track_caller]
fn t(toml: &str, data: impl IntoData) {
    let value: crate::SerdeDocument = crate::from_str(toml).unwrap();
    let result = crate::to_string_pretty(&value).unwrap();
    assert_data_eq!(result, data.raw());
}

#[test]
fn no_unnecessary_newlines_array() {
    #[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
    struct Users {
        pub(crate) user: Vec<User>,
    }

    #[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
    struct User {
        pub(crate) name: String,
        pub(crate) surname: String,
    }

    assert!(
        !crate::to_string_pretty(&Users {
            user: vec![
                User {
                    name: "John".to_owned(),
                    surname: "Doe".to_owned(),
                },
                User {
                    name: "Jane".to_owned(),
                    surname: "Dough".to_owned(),
                },
            ],
        })
        .unwrap()
        .starts_with('\n')
    );
}

#[test]
fn no_unnecessary_newlines_table() {
    #[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
    struct TwoUsers {
        pub(crate) user0: User,
        pub(crate) user1: User,
    }

    #[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
    struct User {
        pub(crate) name: String,
        pub(crate) surname: String,
    }

    assert!(
        !crate::to_string_pretty(&TwoUsers {
            user0: User {
                name: "John".to_owned(),
                surname: "Doe".to_owned(),
            },
            user1: User {
                name: "Jane".to_owned(),
                surname: "Dough".to_owned(),
            },
        })
        .unwrap()
        .starts_with('\n')
    );
}

#[test]
fn basic() {
    t(
        "\
[example]
array = [\"item 1\", \"item 2\"]
empty = []
oneline = \"this has no newlines.\"
text = '''

this is the first line\\nthis is the second line
'''
",
        str![[r#"
[example]
array = [
    "item 1",
    "item 2",
]
empty = []
oneline = "this has no newlines."
text = '''

this is the first line\nthis is the second line
'''

"#]],
    );
}

#[test]
fn tricky() {
    t(
        r#"[example]
f = "\f"
glass = """
Nothing too unusual, except that I can eat glass in:
- Greek: Μπορώ να φάω σπασμένα γυαλιά χωρίς να πάθω τίποτα. 
- Polish: Mogę jeść szkło, i mi nie szkodzi. 
- Hindi: मैं काँच खा सकता हूँ, मुझे उस से कोई पीडा नहीं होती. 
- Japanese: 私はガラスを食べられます。それは私を傷つけません。 
"""
r = "\r"
r_newline = """
\r
"""
single = "this is a single line but has '' cuz it's tricky"
single_tricky = "single line with ''' in it"
tabs = """
this is pretty standard
\texcept for some   \ttabs right here
"""
text = """
this is the first line.
This has a ''' in it and ""\" cuz it's tricky yo
Also ' and " because why not
this is the fourth line
"""
"#,
        str![[r#"
[example]
f = "\f"
glass = """
Nothing too unusual, except that I can eat glass in:
- Greek: Μπορώ να φάω σπασμένα γυαλιά χωρίς να πάθω τίποτα. 
- Polish: Mogę jeść szkło, i mi nie szkodzi. 
- Hindi: मैं काँच खा सकता हूँ, मुझे उस से कोई पीडा नहीं होती. 
- Japanese: 私はガラスを食べられます。それは私を傷つけません。 
"""
r = "\r"
r_newline = """
\r
"""
single = "this is a single line but has '' cuz it's tricky"
single_tricky = "single line with ''' in it"
tabs = """
this is pretty standard
\texcept for some   \ttabs right here
"""
text = """
this is the first line.
This has a ''' in it and ""\" cuz it's tricky yo
Also ' and " because why not
this is the fourth line
"""

"#]],
    );
}

#[test]
fn table_array() {
    t(
        r#"
[abc]
doc = "this is a table"

[[array]]
key = "foo"

[[array]]
key = "bar"

[example]
single = "this is a single line string"
"#,
        str![[r#"
[abc]
doc = "this is a table"

[[array]]
key = "foo"

[[array]]
key = "bar"

[example]
single = "this is a single line string"

"#]],
    );
}

#[test]
fn empty_table() {
    t(
        r#"[example]
"#,
        str![[r#"
[example]

"#]],
    );
}

#[test]
fn implicit_tables() {
    t(
        r#"
authors = []
name = "foo"
version = "0.0.0"

[profile.dev]
debug = true
"#,
        str![[r#"
authors = []
name = "foo"
version = "0.0.0"

[profile.dev]
debug = true

"#]],
    );
}
