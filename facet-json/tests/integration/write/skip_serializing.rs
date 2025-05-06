#![cfg(feature = "std")]

use facet::Facet;
use facet_json::to_string;

#[test]
fn test_skip_serializing() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Greetings {
        hello: &'static str,
        #[facet(skip_serializing)]
        goodbye: &'static str,
    }
    let test_struct1 = Greetings {
        hello: "monde",
        goodbye: "world",
    };
    let json = to_string(&test_struct1);
    assert_eq!(json, r#"{"hello":"monde"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Salutations(&'static str, #[facet(skip_serializing)] &'static str);
    let test_struct2 = Salutations("groetjes", "wereld");
    let json = to_string(&test_struct2);
    assert_eq!(json, r#"["groetjes"]"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    #[repr(C)]
    enum Gruesse {
        Tschuess {
            auf_wiedersehen: i32,
            #[facet(skip_serializing)]
            bis_spaeter: i32,
        },
    }
    let test_struct3 = Gruesse::Tschuess {
        auf_wiedersehen: 1,
        bis_spaeter: 2,
    };
    let json = to_string(&test_struct3);
    assert_eq!(json, r#"{"Tschuess":{"auf_wiedersehen":1}}"#);
}

#[test]
fn test_skip_serializing_if() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Greetings {
        hello: &'static str,
        #[facet(skip_serializing_if = Option::is_some)]
        goodbye: Option<&'static str>,
    }
    let test_struct1 = Greetings {
        hello: "monde",
        goodbye: Some("world"),
    };
    let json = to_string(&test_struct1);
    assert_eq!(json, r#"{"hello":"monde"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Salutations(
        &'static str,
        #[facet(skip_serializing_if = String::is_empty)] String,
    );
    let test_struct2 = Salutations("groetjes", "".to_string());
    let json = to_string(&test_struct2);
    assert_eq!(json, r#"["groetjes"]"#);
}
