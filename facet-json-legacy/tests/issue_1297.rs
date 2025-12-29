use facet::Facet;
use facet_json_legacy::from_str;

#[test]
fn flatten_default_field_missing() {
    #[derive(Facet, Default, Debug, PartialEq)]
    struct Inner {
        foo: u8,
        #[facet(default)]
        color_code: u8,
    }

    #[derive(Facet, Default, Debug, PartialEq)]
    struct Outer {
        #[facet(flatten)]
        inner: Inner,
    }

    let json = r#"{"foo":1}"#;
    let data: Outer = from_str(json).expect("flatten default field should deserialize");
    assert_eq!(
        data,
        Outer {
            inner: Inner {
                foo: 1,
                color_code: 0
            }
        }
    );
}
