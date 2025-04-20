use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_bool() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct BoolStruct {
        yes: bool,
        no: bool,
    }

    let json = r#"{"yes": true, "no": false}"#;

    let s: BoolStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(
        s,
        BoolStruct {
            yes: true,
            no: false
        }
    );
}
