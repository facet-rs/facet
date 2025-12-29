#![allow(renamed_and_removed_lints)]
#![allow(clippy::blacklisted_name)]

use std::collections::HashMap;
use std::fmt::Debug;

use serde::Deserialize;
use serde::de::{Deserializer, MapAccess};
use serde_untagged::UntaggedEnumVisitor;
use snapbox::assert_data_eq;
use snapbox::prelude::*;
use snapbox::str;

use crate::Datetime;
use crate::Spanned;

#[test]
fn test_spanned_field() {
    #[derive(Deserialize, Debug)]
    struct Foo<T> {
        foo: Spanned<T>,
    }

    #[derive(Deserialize, Debug)]
    struct BareFoo<T> {
        foo: T,
    }

    #[track_caller]
    fn good<T>(input: &str, expected: &str, end: Option<usize>)
    where
        T: serde::de::DeserializeOwned + Debug + PartialEq,
    {
        dbg!(input);
        let foo: Foo<T> = crate::from_str(input).unwrap();
        dbg!(&foo);

        assert_eq!(
            &input[foo.foo.span()],
            expected,
            "incorrect `foo.foo.span()`",
        );
        assert_eq!(foo.foo.span().start, 6, "incorrect `foo.foo.span().start`");
        if let Some(end) = end {
            assert_eq!(foo.foo.span().end, end, "incorrect `foo.foo.span().end`");
        } else {
            assert_eq!(
                foo.foo.span().end,
                input.len(),
                "incorrect `foo.foo.span().end`"
            );
        }

        // Test for Spanned<> at the top level
        let foo_outer: Spanned<BareFoo<T>> = crate::from_str(input).unwrap();
        dbg!(&foo_outer);

        assert_eq!(
            foo_outer.span().start,
            0,
            "incorrect `foo_outer.span().start`"
        );
        assert_eq!(foo_outer.span().end, 0, "incorrect `foo_outer.span().end`");
        assert_eq!(
            foo.foo.into_inner(),
            foo_outer.into_inner().foo,
            "deserialized incorrectly"
        );
    }

    good::<String>("foo = \"foo\"", "\"foo\"", None);
    good::<u32>("foo = 42", "42", None);
    // leading plus
    good::<u32>("foo = +42", "+42", None);
    // table
    good::<HashMap<String, u32>>(
        "foo = {\"foo\" = 42, \"bar\" = 42}",
        "{\"foo\" = 42, \"bar\" = 42}",
        None,
    );
    // array
    good::<Vec<u32>>("foo = [0, 1, 2, 3, 4]", "[0, 1, 2, 3, 4]", None);
    // datetime
    good::<String>(
        "foo = \"1997-09-09T09:09:09Z\"",
        "\"1997-09-09T09:09:09Z\"",
        None,
    );

    let good_datetimes = [
        "1997-09-09T09:09:09Z",
        "1997-09-09T09:09:09+09:09",
        "1997-09-09T09:09:09-09:09",
        "1997-09-09T09:09:09",
        "1997-09-09",
        "09:09:09",
        "1997-09-09T09:09:09.09Z",
        "1997-09-09T09:09:09.09+09:09",
        "1997-09-09T09:09:09.09-09:09",
        "1997-09-09T09:09:09.09",
        "09:09:09.09",
    ];
    for expected in good_datetimes {
        let s = format!("foo = {expected}");
        good::<Datetime>(&s, expected, None);
    }
    // ending at something other than the absolute end
    good::<u32>("foo = 42\nnoise = true", "42", Some(8));
}

#[test]
fn test_inner_spanned_table() {
    #[derive(Deserialize, Debug)]
    struct Foo {
        foo: Spanned<HashMap<Spanned<String>, Spanned<String>>>,
    }

    #[track_caller]
    fn good(input: &str, zero: bool) {
        dbg!(input);
        let foo: Foo = crate::from_str(input).unwrap();
        dbg!(&foo);

        if zero {
            assert_eq!(foo.foo.span().start, 0, "invalid `foo.foo.span().start`");
            assert_eq!(foo.foo.span().end, 5, "invalid `foo.foo.span().end`");
        } else {
            assert_eq!(
                foo.foo.span().start,
                input.find('{').unwrap(),
                "invalid `foo.foo.span().start`"
            );
            assert_eq!(
                foo.foo.span().end,
                input.find('}').unwrap() + 1,
                "invalid `foo.foo.span().end`"
            );
        }
        for (k, v) in foo.foo.as_ref().iter() {
            dbg!(&k);
            dbg!(&v);
            assert_eq!(
                &input[k.span().start..k.span().end],
                k.as_ref(),
                "invalid key"
            );
            assert_eq!(
                &input[(v.span().start + 1)..(v.span().end - 1)],
                v.as_ref(),
                "invalid value"
            );
        }
    }

    good(
        "\
        [foo]
        a = 'b'
        bar = 'baz'
        c = 'd'
        e = \"f\"
    ",
        true,
    );

    good(
        "
        foo = { a = 'b', bar = 'baz', c = 'd', e = \"f\" }",
        false,
    );
}

#[test]
fn test_outer_spanned_table() {
    #[derive(Facet)]
    struct Foo {
        foo: HashMap<Spanned<String>, Spanned<String>>,
    }

    fn good(s: &str) {
        let foo: Foo = crate::from_str(s).unwrap();

        for (k, v) in foo.foo.iter() {
            assert_eq!(&s[k.span().start..k.span().end], k.as_ref());
            assert_eq!(&s[(v.span().start + 1)..(v.span().end - 1)], v.as_ref());
        }
    }

    good(
        "
        [foo]
        a = 'b'
        bar = 'baz'
        c = 'd'
        e = \"f\"
    ",
    );

    good(
        "
        foo = { a = 'b', bar = 'baz', c = 'd', e = \"f\" }
    ",
    );
}

#[test]
fn test_spanned_nested() {
    #[derive(Facet)]
    struct Foo {
        foo: HashMap<Spanned<String>, HashMap<Spanned<String>, Spanned<String>>>,
    }

    fn good(s: &str) {
        let foo: Foo = crate::from_str(s).unwrap();

        for (k, v) in foo.foo.iter() {
            assert_eq!(&s[k.span().start..k.span().end], k.as_ref());
            for (n_k, n_v) in v.iter() {
                assert_eq!(&s[n_k.span().start..n_k.span().end], n_k.as_ref());
                assert_eq!(
                    &s[(n_v.span().start + 1)..(n_v.span().end - 1)],
                    n_v.as_ref()
                );
            }
        }
    }

    good(
        "
        [foo.a]
        a = 'b'
        c = 'd'
        e = \"f\"
        [foo.bar]
        baz = 'true'
    ",
    );

    good(
        "
        [foo]
        foo = { a = 'b', bar = 'baz', c = 'd', e = \"f\" }
        bazz = {}
        g = { h = 'i' }
    ",
    );
}

#[test]
fn test_spanned_array() {
    #[derive(Facet)]
    struct Foo {
        foo: Vec<Spanned<HashMap<Spanned<String>, Spanned<String>>>>,
    }

    let toml = "\
        [[foo]]
        a = 'b'
        bar = 'baz'
        c = 'd'
        e = \"f\"
        [[foo]]
        a = 'c'
        bar = 'baz'
        c = 'g'
        e = \"h\"
    ";
    let foo_list: Foo = crate::from_str(toml).unwrap();

    for (foo, expected) in foo_list.foo.iter().zip([0..7, 84..91]) {
        assert_eq!(foo.span(), expected);
        for (k, v) in foo.as_ref().iter() {
            assert_eq!(&toml[k.span().start..k.span().end], k.as_ref());
            assert_eq!(&toml[(v.span().start + 1)..(v.span().end - 1)], v.as_ref());
        }
    }
}

#[test]
fn deny_unknown_fields() {
    #[derive(Debug, serde::Deserialize)]
    #[facet(deny_unknown_fields)]
    struct Example {
        #[allow(dead_code)]
        real: u32,
    }

    let error = crate::from_str::<Example>(
        r#"# my comment
# bla bla bla
fake = 1"#,
    )
    .unwrap_err();
    assert_data_eq!(
        error.to_string(),
        str![[r#"
TOML parse error at line 3, column 1
  |
3 | fake = 1
  | ^^^^
unknown field `fake`, expected `real`

"#]]
        .raw()
    );
}

#[test]
fn implicit_tables() {
    #[derive(Debug)]
    #[allow(dead_code)]
    enum SpannedValue {
        String(String),
        Map(Vec<(String, Spanned<SpannedValue>)>),
    }

    impl<'de> Deserialize<'de> for SpannedValue {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let data = UntaggedEnumVisitor::new()
                .string(|str| Ok(Self::String(str.into())))
                .map(|mut map| {
                    let mut result = Vec::new();

                    while let Some((k, v)) = map.next_entry()? {
                        result.push((k, v));
                    }

                    Ok(Self::Map(result))
                })
                .deserialize(deserializer)?;

            Ok(data)
        }
    }

    const INPUT: &str = r#"
[foo.bar]
alice.bob = { one.two = "qux" }
"#;

    let result = crate::from_str::<SpannedValue>(INPUT);
    assert_data_eq!(
        result.unwrap().to_debug(),
        str![[r#"
Map(
    [
        (
            "foo",
            Spanned {
                span: 2..5,
                value: Map(
                    [
                        (
                            "bar",
                            Spanned {
                                span: 1..10,
                                value: Map(
                                    [
                                        (
                                            "alice",
                                            Spanned {
                                                span: 11..16,
                                                value: Map(
                                                    [
                                                        (
                                                            "bob",
                                                            Spanned {
                                                                span: 23..42,
                                                                value: Map(
                                                                    [
                                                                        (
                                                                            "one",
                                                                            Spanned {
                                                                                span: 25..28,
                                                                                value: Map(
                                                                                    [
                                                                                        (
                                                                                            "two",
                                                                                            Spanned {
                                                                                                span: 35..40,
                                                                                                value: String(
                                                                                                    "qux",
                                                                                                ),
                                                                                            },
                                                                                        ),
                                                                                    ],
                                                                                ),
                                                                            },
                                                                        ),
                                                                    ],
                                                                ),
                                                            },
                                                        ),
                                                    ],
                                                ),
                                            },
                                        ),
                                    ],
                                ),
                            },
                        ),
                    ],
                ),
            },
        ),
    ],
)

"#]]
    );
}
