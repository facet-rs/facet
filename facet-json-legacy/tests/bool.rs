use facet::Facet;
use facet_json_legacy::from_str;
use facet_testhelpers::test;

#[test]
fn json_read_bool() {
    #[derive(Facet, Debug, PartialEq)]
    struct BoolStruct {
        yes: bool,
        no: bool,
    }

    let json = r#"{"yes": true, "no": false}"#;

    let s: BoolStruct = from_str(json).unwrap();
    assert_eq!(
        s,
        BoolStruct {
            yes: true,
            no: false
        }
    );
}
