use camino::Utf8PathBuf;
use eyre::Result;
use facet::Facet;
use facet_json::to_string;

#[test]
fn json_write_utf8pathbuf() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        path: Utf8PathBuf,
    }

    let value = FooBar {
        path: Utf8PathBuf::from("foo/bar"),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"path":"foo/bar"}"#);

    Ok(())
}
