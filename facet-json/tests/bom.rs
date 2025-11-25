use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

#[test]
fn test_basic_with_bom() {
    #[derive(Debug, Facet)]
    struct HelloBom {
        hello: &'static str,
    }
    let json = "\u{feff}{\"hello\": \"hi!\"}";

    let result = from_str::<HelloBom>(json);
    match result {
        Ok(data) => {
            assert_eq!(data.hello, "hi!");
        }
        Err(e) => {
            panic!("Failed to parse JSON: {e}");
        }
    }
}
