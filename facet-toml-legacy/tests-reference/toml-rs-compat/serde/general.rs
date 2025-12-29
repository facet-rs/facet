use std::collections::BTreeMap;

use facet::Facet;
use snapbox::assert_data_eq;
use snapbox::prelude::*;
use snapbox::str;

macro_rules! equivalent {
    ($literal:expr, $toml:expr,) => {{
        let toml = $toml;
        let literal = $literal;

        // Through a string equivalent
        println!("to_string");
        assert_data_eq!(
            t!(crate::to_string(&toml)),
            t!(crate::to_string(&literal)).raw()
        );

        println!("to_string_pretty");
        assert_data_eq!(
            t!(crate::to_string_pretty(&toml)),
            t!(crate::to_string_pretty(&literal)).raw()
        );

        println!("literal, from_str(toml.to_string())");
        assert_eq!(literal, t!(crate::from_str(&t!(crate::to_string(&toml)))));

        println!("literal, from_str(toml.to_string_pretty())");
        assert_eq!(
            literal,
            t!(crate::from_str(&t!(crate::to_string_pretty(&toml))))
        );

        println!("toml, from_str(literal.to_string())");
        assert_eq!(toml, t!(crate::from_str(&t!(crate::to_string(&literal)))));

        println!("toml, from_str(literal.to_string_pretty())");
        assert_eq!(
            toml,
            t!(crate::from_str(&t!(crate::to_string_pretty(&literal))))
        );

        println!("toml.try_into()");
        assert_eq!(literal, t!(toml.clone().try_into()));

        // println!("Value::Table(toml).try_into()");
        // assert_eq!(literal, t!(toml::Value::Table(toml.clone()).try_into()));
    }};
}

macro_rules! error {
    ($ty:ty, $toml:expr, $msg_parse:expr, $msg_decode:expr) => {{
        println!("attempting parsing");
        match crate::from_str::<$ty>(&crate::to_string(&$toml).unwrap()) {
            Ok(_) => panic!("successful"),
            Err(e) => assert_data_eq!(e.to_string(), $msg_parse.raw()),
        }

        println!("attempting parsing of pretty");
        match crate::from_str::<$ty>(&crate::to_string_pretty(&$toml).unwrap()) {
            Ok(_) => panic!("successful"),
            Err(_) => {}
        }

        println!("attempting toml decoding");
        match $toml.try_into::<$ty>() {
            Ok(_) => panic!("successful"),
            Err(e) => assert_data_eq!(e.to_string(), $msg_decode.raw()),
        }
    }};
}

macro_rules! map( ($($k:ident: $v:expr),*) => ({
    let mut _m = crate::SerdeTable::new();
    $(_m.insert(stringify!($k).to_owned(), t!(crate::SerdeValue::try_from($v)));)*
    _m
}) );

#[test]
fn smoke() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: isize,
    }

    equivalent!(Foo { a: 2 }, map! { a: crate::SerdeValue::Integer(2) },);
}

#[test]
fn smoke_hyphen() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: isize,
    }

    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo2 {
        #[facet(rename = "a-b")]
        a_b: isize,
    }

    equivalent! {
        Foo { a_b: 2 },
        map! { a_b: crate::SerdeValue::Integer(2)},
    }

    let mut m = crate::SerdeTable::new();
    m.insert("a-b".to_owned(), crate::SerdeValue::Integer(2));
    equivalent! {
        Foo2 { a_b: 2 },
        m,
    }
}

#[test]
fn nested() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: isize,
        b: Bar,
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Bar {
        a: String,
    }

    equivalent! {
        Foo { a: 2, b: Bar { a: "test".to_owned() } },
        map! {
            a: crate::SerdeValue::Integer(2),
            b: map! {
                a: crate::SerdeValue::String("test".to_owned())
            }
        },
    }
}

#[test]
fn application_decode_error() {
    todo!("needs deser_with or _some sort_ of validation")
    //     #[derive(PartialEq, Debug)]
    //     struct Range10(usize);
    //     impl<'de> Deserialize<'de> for Range10 {
    //         fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
    //             let x: usize = Deserialize::deserialize(d)?;
    //             if x > 10 {
    //                 Err(serde::de::Error::custom("more than 10"))
    //             } else {
    //                 Ok(Self(x))
    //             }
    //         }
    //     }
    //     let d_good = crate::SerdeValue::Integer(5);
    //     let d_bad1 = crate::SerdeValue::String("not an isize".to_owned());
    //     let d_bad2 = crate::SerdeValue::Integer(11);

    //     assert_data_eq!(
    //         d_good.try_into::<Range10>().unwrap().to_debug(),
    //         str![[r#"
    // Range10(
    //     5,
    // )

    // "#]]
    //     );

    //     let err1: Result<Range10, _> = d_bad1.try_into();
    //     assert!(err1.is_err());
    //     let err2: Result<Range10, _> = d_bad2.try_into();
    //     assert!(err2.is_err());
}

#[test]
fn array() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: Vec<isize>,
    }

    equivalent! {
        Foo { a: vec![1, 2, 3, 4] },
        map! {
            a: crate::SerdeValue::Array(vec![
                crate::SerdeValue::Integer(1),
                crate::SerdeValue::Integer(2),
                crate::SerdeValue::Integer(3),
                crate::SerdeValue::Integer(4)
            ])
        },
    };
}

#[test]
fn hashmap() {
    use std::collections::HashSet;

    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        map: BTreeMap<String, isize>,
        set: HashSet<char>,
    }

    equivalent! {
        Foo {
            set: {
                let mut s = HashSet::new();
                s.insert('a');
                s
            },
            map: {
                let mut m = BTreeMap::new();
                m.insert("bar".to_owned(), 4);
                m.insert("foo".to_owned(), 10);
                m
            }
        },
        map! {
            map: map! {
                bar: crate::SerdeValue::Integer(4),
                foo: crate::SerdeValue::Integer(10)
            },
            set: crate::SerdeValue::Array(vec![crate::SerdeValue::String("a".to_owned())])
        },
    }
}

#[test]
fn table_array() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: Vec<Bar>,
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Bar {
        a: isize,
    }

    equivalent! {
        Foo { a: vec![Bar { a: 1 }, Bar { a: 2 }] },
        map! {
            a: crate::SerdeValue::Array(vec![
                crate::SerdeValue::Table(map!{ a: crate::SerdeValue::Integer(1) }),
                crate::SerdeValue::Table(map!{ a: crate::SerdeValue::Integer(2) }),
            ])
        },
    }
}

#[test]
fn type_errors() {
    #[derive(Facet)]
    #[allow(dead_code)]
    struct Foo {
        bar: isize,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Bar {
        foo: Foo,
    }

    error! {
        Foo,
        map! {
            bar: crate::SerdeValue::String("a".to_owned())
        },
        str![[r#"
TOML parse error at line 1, column 7
  |
1 | bar = "a"
  |       ^^^
invalid type: string "a", expected isize

"#]],
        str![[r#"
invalid type: string "a", expected isize
in `bar`

"#]]
    }

    error! {
        Bar,
        map! {
            foo: map! {
                bar: crate::SerdeValue::String("a".to_owned())
            }
        },
        str![[r#"
TOML parse error at line 2, column 7
  |
2 | bar = "a"
  |       ^^^
invalid type: string "a", expected isize

"#]],
        str![[r#"
invalid type: string "a", expected isize
in `foo.bar`

"#]]
    }
}

#[test]
fn missing_errors() {
    #[derive(Facet, PartialEq, Debug)]
    struct Foo {
        bar: isize,
    }

    error! {
        Foo,
        map! { },
        str![[r#"
TOML parse error at line 1, column 1
  |
1 |
  | ^
missing field `bar`

"#]],
        str![[r#"
missing field `bar`

"#]]
    }
}

#[test]
fn parse_enum() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: E,
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    #[facet(untagged)]
    enum E {
        Bar(isize),
        Baz(String),
        Last(Foo2),
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo2 {
        test: String,
    }

    equivalent! {
        Foo { a: E::Bar(10) },
        map! { a: crate::SerdeValue::Integer(10) },
    }

    equivalent! {
        Foo { a: E::Baz("foo".to_owned()) },
        map! { a: crate::SerdeValue::String("foo".to_owned()) },
    }

    equivalent! {
        Foo { a: E::Last(Foo2 { test: "test".to_owned() }) },
        map! { a: map! { test: crate::SerdeValue::String("test".to_owned()) } },
    }
}

#[test]
fn parse_enum_string() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: Sort,
    }

    #[derive(Facet, PartialEq, Debug, Clone)]
    #[facet(rename_all = "lowercase")]
    enum Sort {
        Asc,
        Desc,
    }

    equivalent! {
        Foo { a: Sort::Desc },
        map! { a: crate::SerdeValue::String("desc".to_owned()) },
    }
}

#[test]
fn parse_tuple_variant() {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Document {
        inner: Vec<Enum>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    enum Enum {
        Int(i32, i32),
        String(String, String),
    }

    let input = Document {
        inner: vec![
            Enum::Int(1, 1),
            Enum::String("2".to_owned(), "2".to_owned()),
        ],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[inner]]
Int = [1, 1]

[[inner]]
String = ["2", "2"]

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[inner]]
Int = [
    1,
    1,
]

[[inner]]
String = [
    "2",
    "2",
]

"#]]
        .raw()
    );

    equivalent! {
        Document {
            inner: vec![
                Enum::Int(1, 1),
                Enum::String("2".to_owned(), "2".to_owned()),
            ],
        },
        map! {
            inner: vec![
                map! { Int: [1, 1] },
                map! { String: ["2".to_owned(), "2".to_owned()] },
            ]
        },
    }
}

#[test]
fn parse_struct_variant() {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Document {
        inner: Vec<Enum>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    enum Enum {
        Int { first: i32, second: i32 },
        String { first: String, second: String },
    }

    let input = Document {
        inner: vec![
            Enum::Int {
                first: 1,
                second: 1,
            },
            Enum::String {
                first: "2".to_owned(),
                second: "2".to_owned(),
            },
        ],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[inner]]

[inner.Int]
first = 1
second = 1

[[inner]]

[inner.String]
first = "2"
second = "2"

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[inner]]

[inner.Int]
first = 1
second = 1

[[inner]]

[inner.String]
first = "2"
second = "2"

"#]]
        .raw()
    );

    equivalent! {
        Document {
            inner: vec![
                Enum::Int { first: 1, second: 1 },
                Enum::String { first: "2".to_owned(), second: "2".to_owned() },
            ],
        },
        map! {
            inner: vec![
                map! { Int: map! { first: 1, second: 1 } },
                map! { String: map! { first: "2".to_owned(), second: "2".to_owned() } },
            ]
        },
    }
}

#[test]
#[cfg(feature = "preserve_order")]
fn map_key_unit_variants() {
    #[derive(Facet, PartialEq, Eq, Debug, Clone, PartialOrd, Ord)]
    enum Sort {
        #[facet(rename = "ascending")]
        Asc,
        Desc,
    }

    let mut map = BTreeMap::new();
    map.insert(Sort::Asc, 1);
    map.insert(Sort::Desc, 2);

    equivalent! {
        map,
        map! { ascending: crate::SerdeValue::Integer(1), Desc: crate::SerdeValue::Integer(2) },
    }
}

// #[test]
// fn unused_fields() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: isize }
//
//     let v = Foo { a: 2 };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, Integer(2),
//         b, Integer(5)
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, Some(crate::SerdeTable(map! {
//         b, Integer(5)
//     })));
// }
//
// #[test]
// fn unused_fields2() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: Bar }
//     #[derive(Facet, PartialEq, Debug)]
//     struct Bar { a: isize }
//
//     let v = Foo { a: Bar { a: 2 } };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, crate::SerdeTable(map! {
//             a, Integer(2),
//             b, Integer(5)
//         })
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, Some(crate::SerdeTable(map! {
//         a, crate::SerdeTable(map! {
//             b, Integer(5)
//         })
//     })));
// }
//
// #[test]
// fn unused_fields3() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: Bar }
//     #[derive(Facet, PartialEq, Debug)]
//     struct Bar { a: isize }
//
//     let v = Foo { a: Bar { a: 2 } };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, crate::SerdeTable(map! {
//             a, Integer(2)
//         })
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, None);
// }
//
// #[test]
// fn unused_fields4() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: BTreeMap<String, String> }
//
//     let v = Foo { a: map! { a, "foo".to_owned() } };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, crate::SerdeTable(map! {
//             a, crate::SerdeValue::String("foo".to_owned())
//         })
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, None);
// }
//
// #[test]
// fn unused_fields5() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: Vec<String> }
//
//     let v = Foo { a: vec!["a".to_owned()] };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, Array(vec![crate::SerdeValue::String("a".to_owned())])
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, None);
// }
//
// #[test]
// fn unused_fields6() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: Option<Vec<String>> }
//
//     let v = Foo { a: Some(vec![]) };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, Array(vec![])
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, None);
// }
//
// #[test]
// fn unused_fields7() {
//     #[derive(Facet, PartialEq, Debug)]
//     struct Foo { a: Vec<Bar> }
//     #[derive(Facet, PartialEq, Debug)]
//     struct Bar { a: isize }
//
//     let v = Foo { a: vec![Bar { a: 1 }] };
//     let mut d = Decoder::new(crate::SerdeTable(map! {
//         a, Array(vec![crate::SerdeTable(map! {
//             a, Integer(1),
//             b, Integer(2)
//         })])
//     }));
//     assert_eq!(v, t!(Deserialize::deserialize(&mut d)));
//
//     assert_eq!(d.toml, Some(crate::SerdeTable(map! {
//         a, Array(vec![crate::SerdeTable(map! {
//             b, Integer(2)
//         })])
//     })));
// }

#[test]
fn empty_arrays() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: Vec<Bar>,
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Bar;

    equivalent! {
        Foo { a: vec![] },
        map! {a: crate::SerdeValue::Array(Vec::new())},
    }
}

#[test]
fn extra_keys() {
    #[derive(Facet)]
    struct Foo {
        a: isize,
    }

    let toml = map! { a: crate::SerdeValue::Integer(2), b: crate::SerdeValue::Integer(2) };
    assert!(toml.clone().try_into::<Foo>().is_ok());
    assert!(crate::from_str::<Foo>(&crate::to_string(&toml).unwrap()).is_ok());
    assert!(crate::from_str::<Foo>(&crate::to_string_pretty(&toml).unwrap()).is_ok());
}

#[test]
fn newtypes() {
    #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
    struct A {
        b: B,
    }

    #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
    struct B(u32);

    equivalent! {
        A { b: B(2) },
        map! { b: crate::SerdeValue::Integer(2) },
    }
}

#[test]
fn newtypes2() {
    #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
    struct A {
        b: B,
    }

    #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
    struct B(Option<C>);

    #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
    struct C {
        x: u32,
        y: u32,
        z: u32,
    }

    equivalent! {
        A { b: B(Some(C { x: 0, y: 1, z: 2 })) },
        map! {
            b: map! {
                x: crate::SerdeValue::Integer(0),
                y: crate::SerdeValue::Integer(1),
                z: crate::SerdeValue::Integer(2)
            }
        },
    }
}

#[test]
fn newtype_variant() {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Struct {
        field: Enum,
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    enum Enum {
        Variant(u8),
    }

    equivalent! {
        Struct { field: Enum::Variant(21) },
        map! {
            field: map! {
                Variant: crate::SerdeValue::Integer(21)
            }
        },
    }
}

#[test]
fn newtype_key() {
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Serialize, Deserialize)]
    struct NewType(String);

    type CustomKeyMap = BTreeMap<NewType, u32>;

    equivalent! {
        [
            (NewType("x".to_owned()), 1),
            (NewType("y".to_owned()), 2),
        ].into_iter().collect::<CustomKeyMap>(),
        map! {
            x: crate::SerdeValue::Integer(1),
            y: crate::SerdeValue::Integer(2)
        },
    }
}

#[test]
fn fixed_size_array() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Entity {
        pos: [i32; 2],
    }

    equivalent! {
        Entity { pos: [1, 2] },
        map! {
            pos: crate::SerdeValue::Array(vec![
                crate::SerdeValue::Integer(1),
                crate::SerdeValue::Integer(2),
            ])
        },
    }
}

#[test]
fn homogeneous_tuple() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Collection {
        elems: (i64, i64, i64),
    }

    equivalent! {
        Collection { elems: (0, 1, 2) },
        map! {
            elems: crate::SerdeValue::Array(vec![
                crate::SerdeValue::Integer(0),
                crate::SerdeValue::Integer(1),
                crate::SerdeValue::Integer(2),
            ])
        },
    }
}

#[test]
fn homogeneous_tuple_struct() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Object(Vec<String>, Vec<String>, Vec<String>);

    equivalent! {
        map! {
            obj: Object(vec!["foo".to_owned()], vec![], vec!["bar".to_owned(), "baz".to_owned()])
        },
        map! {
            obj: crate::SerdeValue::Array(vec![
                crate::SerdeValue::Array(vec![
                    crate::SerdeValue::String("foo".to_owned()),
                ]),
                crate::SerdeValue::Array(vec![]),
                crate::SerdeValue::Array(vec![
                    crate::SerdeValue::String("bar".to_owned()),
                    crate::SerdeValue::String("baz".to_owned()),
                ]),
            ])
        },
    }
}

#[test]
fn json_interoperability() {
    #[derive(Facet)]
    struct Foo {
        any: crate::SerdeValue,
    }

    let _foo: Foo = serde_json::from_str(
        r#"
        {"any":1}
    "#,
    )
    .unwrap();
}

#[test]
fn error_includes_key() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Package {
        name: String,
        version: String,
        authors: Vec<String>,
        profile: Profile,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Profile {
        dev: Dev,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Dev {
        debug: U32OrBool,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
    #[facet(untagged, expecting = "expected a boolean or an integer")]
    pub(crate) enum U32OrBool {
        U32(u32),
        Bool(bool),
    }

    let res: Result<Package, _> = crate::from_str(
        r#"
[package]
name = "foo"
version = "0.0.0"
authors = []

[profile.dev]
debug = 'a'
"#,
    );
    let err = res.unwrap_err();
    assert_data_eq!(
        err.to_string(),
        str![[r#"
TOML parse error at line 8, column 9
  |
8 | debug = 'a'
  |         ^^^
expected a boolean or an integer

"#]]
    );

    let res: Result<Package, _> = crate::from_str(
        r#"
[package]
name = "foo"
version = "0.0.0"
authors = []

[profile]
dev = { debug = 'a' }
"#,
    );
    let err = res.unwrap_err();
    assert_data_eq!(
        err.to_string(),
        str![[r#"
TOML parse error at line 8, column 17
  |
8 | dev = { debug = 'a' }
  |                 ^^^
expected a boolean or an integer

"#]]
    );
}

#[test]
fn newline_key_value() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Package {
        name: String,
    }

    let package = Package {
        name: "foo".to_owned(),
    };
    let raw = crate::to_string(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
name = "foo"

"#]]
    );
    let raw = crate::to_string_pretty(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
name = "foo"

"#]]
    );
}

#[test]
fn newline_table() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Manifest {
        package: Package,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Package {
        name: String,
    }

    let package = Manifest {
        package: Package {
            name: "foo".to_owned(),
        },
    };
    let raw = crate::to_string(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[package]
name = "foo"

"#]]
    );
    let raw = crate::to_string_pretty(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[package]
name = "foo"

"#]]
    );
}

#[test]
fn newline_dotted_table() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Manifest {
        profile: Profile,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Profile {
        dev: Dev,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Dev {
        debug: U32OrBool,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
    #[facet(untagged, expecting = "expected a boolean or an integer")]
    pub(crate) enum U32OrBool {
        U32(u32),
        Bool(bool),
    }

    let package = Manifest {
        profile: Profile {
            dev: Dev {
                debug: U32OrBool::Bool(true),
            },
        },
    };
    let raw = crate::to_string(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[profile.dev]
debug = true

"#]]
    );
    let raw = crate::to_string_pretty(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[profile.dev]
debug = true

"#]]
    );
}

#[test]
fn newline_mixed_tables() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Manifest {
        cargo_features: Vec<String>,
        package: Package,
        profile: Profile,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Package {
        name: String,
        version: String,
        authors: Vec<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Profile {
        dev: Dev,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Dev {
        debug: U32OrBool,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
    #[facet(untagged, expecting = "expected a boolean or an integer")]
    pub(crate) enum U32OrBool {
        U32(u32),
        Bool(bool),
    }

    let package = Manifest {
        cargo_features: vec![],
        package: Package {
            name: "foo".to_owned(),
            version: "1.0.0".to_owned(),
            authors: vec![],
        },
        profile: Profile {
            dev: Dev {
                debug: U32OrBool::Bool(true),
            },
        },
    };
    let raw = crate::to_string(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
cargo_features = []

[package]
name = "foo"
version = "1.0.0"
authors = []

[profile.dev]
debug = true

"#]]
    );
    let raw = crate::to_string_pretty(&package).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
cargo_features = []

[package]
name = "foo"
version = "1.0.0"
authors = []

[profile.dev]
debug = true

"#]]
    );
}

#[test]
fn i64_min() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: i64,
    }

    equivalent! {
        Foo { a_b: i64::MIN },
        map! { a_b: crate::SerdeValue::Integer(i64::MIN) },
    }
}

#[test]
fn i64_max() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: i64,
    }

    equivalent! {
        Foo { a_b: i64::MAX },
        map! { a_b: crate::SerdeValue::Integer(i64::MAX) },
    }
}

#[test]
fn u64_max() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: u64,
    }

    let value = u64::MAX;
    let literal = Foo { a_b: value };
    let encoded = format!(
        "
a_b = {value}
"
    );

    // Through a string equivalent
    println!("to_string");
    assert_data_eq!(
        crate::to_string(&literal).unwrap_err().to_string(),
        str!["out-of-range value for u64 type"].raw()
    );
    println!("to_string_pretty");
    assert_data_eq!(
        crate::to_string_pretty(&literal).unwrap_err().to_string(),
        str!["out-of-range value for u64 type"].raw()
    );
    println!("literal, from_str(toml)");
    assert_data_eq!(
        crate::from_str::<Foo>(&encoded).unwrap().to_debug(),
        str![[r#"
Foo {
    a_b: 18446744073709551615,
}

"#]]
        .raw()
    );

    // In/out of Value is equivalent
    println!("Table::try_from(literal)");
    assert_data_eq!(
        toml::Table::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["u64 value was too large"].raw()
    );
    println!("Value::try_from(literal)");
    assert_data_eq!(
        toml::Value::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["u64 value was too large"].raw()
    );
}

#[test]
fn i128_min() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: i128,
    }

    let value = i128::MIN;
    let literal = Foo { a_b: value };
    let encoded = format!(
        "
a_b = {value}
"
    );

    // Through a string equivalent
    println!("to_string");
    assert_data_eq!(
        crate::to_string(&literal).unwrap_err().to_string(),
        str!["i128 is not supported"].raw()
    );
    println!("to_string_pretty");
    assert_data_eq!(
        crate::to_string_pretty(&literal).unwrap_err().to_string(),
        str!["i128 is not supported"].raw()
    );
    println!("literal, from_str(toml)");
    assert_data_eq!(
        crate::from_str::<Foo>(&encoded).unwrap().to_debug(),
        str![[r#"
Foo {
    a_b: -170141183460469231731687303715884105728,
}

"#]]
        .raw()
    );

    // In/out of Value is equivalent
    println!("Table::try_from(literal)");
    assert_data_eq!(
        toml::Table::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["i128 is not supported"].raw()
    );
    println!("Value::try_from(literal)");
    assert_data_eq!(
        toml::Value::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["i128 is not supported"].raw()
    );
}

#[test]
fn i128_max() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: i128,
    }

    let value = i128::MAX;
    let literal = Foo { a_b: value };
    let encoded = format!(
        "
a_b = {value}
"
    );

    // Through a string equivalent
    println!("to_string");
    assert_data_eq!(
        crate::to_string(&literal).unwrap_err().to_string(),
        str!["i128 is not supported"].raw()
    );
    println!("to_string_pretty");
    assert_data_eq!(
        crate::to_string_pretty(&literal).unwrap_err().to_string(),
        str!["i128 is not supported"].raw()
    );
    println!("literal, from_str(toml)");
    assert_data_eq!(
        crate::from_str::<Foo>(&encoded).unwrap().to_debug(),
        str![[r#"
Foo {
    a_b: 170141183460469231731687303715884105727,
}

"#]]
        .raw()
    );

    // In/out of Value is equivalent
    println!("Table::try_from(literal)");
    assert_data_eq!(
        toml::Table::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["i128 is not supported"].raw()
    );
    println!("Value::try_from(literal)");
    assert_data_eq!(
        toml::Value::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["i128 is not supported"].raw()
    );
}

#[test]
fn u128_max() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: u128,
    }

    let value = u128::MAX;
    let literal = Foo { a_b: value };
    let encoded = format!(
        "
a_b = {value}
"
    );

    // Through a string equivalent
    println!("to_string");
    assert_data_eq!(
        crate::to_string(&literal).unwrap_err().to_string(),
        str!["u128 is not supported"].raw()
    );
    println!("to_string_pretty");
    assert_data_eq!(
        crate::to_string_pretty(&literal).unwrap_err().to_string(),
        str!["u128 is not supported"].raw()
    );
    println!("literal, from_str(toml)");
    assert_data_eq!(
        crate::from_str::<Foo>(&encoded).unwrap().to_debug(),
        str![[r#"
Foo {
    a_b: 340282366920938463463374607431768211455,
}

"#]]
        .raw()
    );

    // In/out of Value is equivalent
    println!("Table::try_from(literal)");
    assert_data_eq!(
        toml::Table::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["u128 is not supported"].raw()
    );
    println!("Value::try_from(literal)");
    assert_data_eq!(
        toml::Value::try_from(literal.clone())
            .unwrap_err()
            .to_string(),
        str!["u128 is not supported"].raw()
    );
}

#[test]
fn float_min() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: f64,
    }

    equivalent! {
        Foo { a_b: f64::MIN },
        map! { a_b: crate::SerdeValue::Float(f64::MIN) },
    }
}

#[test]
fn float_max() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a_b: f64,
    }

    equivalent! {
        Foo { a_b: f64::MAX },
        map! { a_b: crate::SerdeValue::Float(f64::MAX) },
    }
}

#[test]
fn unsupported_root_type() {
    let native = "value";
    let err = crate::to_string(&native).unwrap_err();
    assert_data_eq!(err.to_string(), str!["unsupported str type"].raw());
    let err = crate::to_string_pretty(&native).unwrap_err();
    assert_data_eq!(err.to_string(), str!["unsupported str type"].raw());
}

#[test]
fn unsupported_nested_type() {
    #[derive(Debug, Serialize, Deserialize)]
    struct Foo {
        unused: (),
    }

    let native = Foo { unused: () };
    let err = crate::to_string(&native).unwrap_err();
    assert_data_eq!(err.to_string(), str!["unsupported unit type"].raw());
    let err = crate::to_string_pretty(&native).unwrap_err();
    assert_data_eq!(err.to_string(), str!["unsupported unit type"].raw());
}

#[test]
fn table_type_enum_regression_issue_388() {
    #[derive(Facet)]
    struct DataFile {
        #[allow(dead_code)]
        data: Compare,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    enum Compare {
        Gt(u32),
    }

    let dotted_table = r#"
        data.Gt = 5
        "#;
    assert!(crate::from_str::<DataFile>(dotted_table).is_ok());

    let inline_table = r#"
        data = { Gt = 5 }
        "#;
    assert!(crate::from_str::<DataFile>(inline_table).is_ok());
}

#[test]
fn serialize_datetime_issue_333() {
    #[derive(Serialize)]
    struct Struct {
        date: crate::Datetime,
    }

    let input = Struct {
        date: crate::Datetime {
            date: Some(crate::Date {
                year: 2022,
                month: 1,
                day: 1,
            }),
            time: None,
            offset: None,
        },
    };

    let toml = crate::to_string(&input).unwrap();
    assert_data_eq!(
        toml,
        str![[r#"
date = 2022-01-01

"#]]
    );
    let toml = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        toml,
        str![[r#"
date = 2022-01-01

"#]]
    );

    let toml = crate::to_string_value(&input.date).unwrap();
    assert_data_eq!(toml, str!["2022-01-01"]);
}

#[test]
fn serialize_date() {
    #[derive(Serialize)]
    struct Document {
        date: crate::Date,
    }

    let input = Document {
        date: crate::Date {
            year: 2024,
            month: 1,
            day: 1,
        },
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
date = 2024-01-01

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
date = 2024-01-01

"#]]
        .raw()
    );

    let toml = crate::to_string_value(&input.date).unwrap();
    assert_data_eq!(toml, str!["2024-01-01"]);
}

#[test]
fn serialize_time() {
    #[derive(Serialize)]
    struct Document {
        date: crate::Time,
    }

    let input = Document {
        date: crate::Time {
            hour: 5,
            minute: 0,
            second: 0,
            nanosecond: 0,
        },
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
date = 05:00:00

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
date = 05:00:00

"#]]
        .raw()
    );

    let toml = crate::to_string_value(&input.date).unwrap();
    assert_data_eq!(toml, str!["05:00:00"]);
}

#[test]
fn deserialize_date() {
    #[derive(Debug, Deserialize)]
    struct Document {
        date: crate::Date,
    }

    let document = crate::from_str::<Document>("date = 2024-01-01").unwrap();
    assert_data_eq!(
        document.date.to_debug(),
        str![[r#"
Date {
    year: 2024,
    month: 1,
    day: 1,
}

"#]],
    );

    let err = crate::from_str::<Document>("date = 2024-01-01T05:00:00").unwrap_err();
    assert_data_eq!(
        err.message(),
        str!["invalid type: local datetime, expected local date"]
    );
}

#[test]
fn deserialize_time() {
    #[derive(Debug, Deserialize)]
    struct Document {
        time: crate::Time,
    }

    let document = crate::from_str::<Document>("time = 05:00:00").unwrap();
    assert_data_eq!(
        document.time.to_debug(),
        str![[r#"
Time {
    hour: 5,
    minute: 0,
    second: 0,
    nanosecond: 0,
}

"#]],
    );

    let err = crate::from_str::<Document>("time = 2024-01-01T05:00:00").unwrap_err();
    assert_data_eq!(
        err.message(),
        str!["invalid type: local datetime, expected local time"]
    );
}

#[test]
fn serialize_array_with_none_value() {
    #[derive(Serialize)]
    struct Document {
        values: Vec<Option<usize>>,
    }

    let input = Document {
        values: vec![Some(1), Some(2), Some(3)],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
values = [1, 2, 3]

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
values = [
    1,
    2,
    3,
]

"#]]
        .raw()
    );

    let input = Document {
        values: vec![Some(1), None, Some(3)],
    };
    let err = crate::to_string(&input).unwrap_err();
    assert_data_eq!(err.to_string(), str!["unsupported None value"].raw());
    let err = crate::to_string_pretty(&input).unwrap_err();
    assert_data_eq!(err.to_string(), str!["unsupported None value"].raw());
}

#[test]
fn serialize_array_with_optional_struct_field() {
    #[derive(Debug, Deserialize, Serialize)]
    struct Document {
        values: Vec<OptionalField>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct OptionalField {
        x: u8,
        y: Option<u8>,
    }

    let input = Document {
        values: vec![
            OptionalField { x: 0, y: Some(4) },
            OptionalField { x: 2, y: Some(5) },
            OptionalField { x: 3, y: Some(7) },
        ],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[values]]
x = 0
y = 4

[[values]]
x = 2
y = 5

[[values]]
x = 3
y = 7

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[values]]
x = 0
y = 4

[[values]]
x = 2
y = 5

[[values]]
x = 3
y = 7

"#]]
        .raw()
    );

    let input = Document {
        values: vec![
            OptionalField { x: 0, y: Some(4) },
            OptionalField { x: 2, y: None },
            OptionalField { x: 3, y: Some(7) },
        ],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[values]]
x = 0
y = 4

[[values]]
x = 2

[[values]]
x = 3
y = 7

"#]]
        .raw()
    );
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
[[values]]
x = 0
y = 4

[[values]]
x = 2

[[values]]
x = 3
y = 7

"#]]
        .raw()
    );
}

#[test]
fn serialize_array_with_enum_of_optional_struct_field() {
    #[derive(Debug, Deserialize, Serialize)]
    struct Document {
        values: Vec<Choice>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    enum Choice {
        Optional(OptionalField),
        Empty,
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct OptionalField {
        x: u8,
        y: Option<u8>,
    }

    let input = Document {
        values: vec![
            Choice::Optional(OptionalField { x: 0, y: Some(4) }),
            Choice::Empty,
            Choice::Optional(OptionalField { x: 2, y: Some(5) }),
            Choice::Optional(OptionalField { x: 3, y: Some(7) }),
        ],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(raw, str![[r#"
values = [{ Optional = { x = 0, y = 4 } }, "Empty", { Optional = { x = 2, y = 5 } }, { Optional = { x = 3, y = 7 } }]

"#]].raw());
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
values = [
    { Optional = { x = 0, y = 4 } },
    "Empty",
    { Optional = { x = 2, y = 5 } },
    { Optional = { x = 3, y = 7 } },
]

"#]]
        .raw()
    );

    let input = Document {
        values: vec![
            Choice::Optional(OptionalField { x: 0, y: Some(4) }),
            Choice::Empty,
            Choice::Optional(OptionalField { x: 2, y: None }),
            Choice::Optional(OptionalField { x: 3, y: Some(7) }),
        ],
    };
    let raw = crate::to_string(&input).unwrap();
    assert_data_eq!(raw, str![[r#"
values = [{ Optional = { x = 0, y = 4 } }, "Empty", { Optional = { x = 2 } }, { Optional = { x = 3, y = 7 } }]

"#]].raw());
    let raw = crate::to_string_pretty(&input).unwrap();
    assert_data_eq!(
        raw,
        str![[r#"
values = [
    { Optional = { x = 0, y = 4 } },
    "Empty",
    { Optional = { x = 2 } },
    { Optional = { x = 3, y = 7 } },
]

"#]]
        .raw()
    );
}

#[test]
fn serialize_struct_with_none_string() {
    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct CanBeEmpty {
        a: Option<String>,
        b: Option<String>,
    }

    let input = "[bar]

[baz]

[bazv]
a = \"foo\"

[foo]";
    let value: BTreeMap<String, CanBeEmpty> = crate::from_str(input).unwrap();

    assert_data_eq!(
        value.to_debug(),
        str![[r#"
{
    "bar": CanBeEmpty {
        a: None,
        b: None,
    },
    "baz": CanBeEmpty {
        a: None,
        b: None,
    },
    "bazv": CanBeEmpty {
        a: Some(
            "foo",
        ),
        b: None,
    },
    "foo": CanBeEmpty {
        a: None,
        b: None,
    },
}

"#]]
    );
    assert_data_eq!(
        crate::to_string(&value).unwrap(),
        str![[r#"
[bar]

[baz]

[bazv]
a = "foo"

[foo]

"#]]
        .raw()
    );
    assert_data_eq!(
        crate::to_string_pretty(&value).unwrap(),
        str![[r#"
[bar]

[baz]

[bazv]
a = "foo"

[foo]

"#]]
        .raw()
    );
}

#[test]
fn serialize_struct_with_none_vec() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: Option<Vec<Bar>>,
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Bar;

    equivalent! {
        Foo { a: None },
        map! {},
    }

    equivalent! {
        Foo { a: Some(vec![]) },
        map! { a: crate::SerdeValue::Array(vec![]) },
    }
}

#[test]
fn serialize_struct_with_none_struct() {
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Foo {
        a: Option<Box<Foo>>,
        b: Bar,
    }
    #[derive(Facet, PartialEq, Debug, Clone)]
    struct Bar {
        a: String,
        b: f64,
    }

    equivalent! {
        Foo {
            a: Some(Box::new(Foo {
                a: None,
                b: Bar { a: "foo".to_owned(), b: 4.5 },
            })),
            b: Bar { a: "bar".to_owned(), b: 1.0 },
        },
        map! {
            a: map! {
                b: map! {
                    a: crate::SerdeValue::String("foo".to_owned()),
                    b: crate::SerdeValue::Float(4.5)
                }
            },
            b: map! {
                a: crate::SerdeValue::String("bar".to_owned()),
                b: crate::SerdeValue::Float(1.0)
            }
        },
    }
}

#[test]
fn serialize_struct_with_newtype_with_none() {
    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct CanBeEmpty {
        #[facet(default)]
        a: NewType,
        #[facet(default)]
        b: NewType,
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct NewType(Option<String>);

    let input = "[bar]

[baz]

[bazv]
a = \"foo\"

[foo]";
    let value: BTreeMap<String, CanBeEmpty> = crate::from_str(input).unwrap();

    assert_data_eq!(
        value.to_debug(),
        str![[r#"
{
    "bar": CanBeEmpty {
        a: NewType(
            None,
        ),
        b: NewType(
            None,
        ),
    },
    "baz": CanBeEmpty {
        a: NewType(
            None,
        ),
        b: NewType(
            None,
        ),
    },
    "bazv": CanBeEmpty {
        a: NewType(
            Some(
                "foo",
            ),
        ),
        b: NewType(
            None,
        ),
    },
    "foo": CanBeEmpty {
        a: NewType(
            None,
        ),
        b: NewType(
            None,
        ),
    },
}

"#]]
    );
    assert_data_eq!(
        crate::to_string(&value).unwrap(),
        str![[r#"
[bar]

[baz]

[bazv]
a = "foo"

[foo]

"#]]
        .raw()
    );
    assert_data_eq!(
        crate::to_string_pretty(&value).unwrap(),
        str![[r#"
[bar]

[baz]

[bazv]
a = "foo"

[foo]

"#]]
        .raw()
    );
}

#[test]
fn span_for_sequence_as_map() {
    #[allow(dead_code)]
    #[derive(Facet)]
    struct Manifest {
        package: Package,
        bench: Vec<Bench>,
    }

    #[derive(Facet)]
    struct Package {}

    #[derive(Facet)]
    struct Bench {}

    let raw = r#"
[package]
name = "foo"
version = "0.1.0"
edition = "2021"
[[bench.foo]]
"#;
    let err = match crate::from_str::<Manifest>(raw) {
        Ok(_) => panic!("should fail"),
        Err(err) => err,
    };
    assert_data_eq!(
        err.span().to_debug(),
        str![[r#"
Some(
    61..66,
)

"#]]
    );
}
