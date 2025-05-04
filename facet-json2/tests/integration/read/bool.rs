use eyre::Result;
use facet::Facet;
use facet_json2::from_str;

#[test]
fn json_read_bool() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct BoolStruct {
        yes: bool,
        no: bool,
    }

    let json = r#"{"yes": true, "no": false}"#;

    let s: BoolStruct = from_str(json)?;
    assert_eq!(
        s,
        BoolStruct {
            yes: true,
            no: false
        }
    );

    Ok(())
}
