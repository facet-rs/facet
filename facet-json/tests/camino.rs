use camino::Utf8PathBuf;
use eyre::Result;
use facet::Facet;
use facet_json::from_str;
use facet_json::to_string;

#[test]
fn json_read_utf8pathbuf() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        path: Utf8PathBuf,
    }

    let json = r#"{"path":"foo/bar"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            path: Utf8PathBuf::from("foo/bar"),
        }
    );

    Ok(())
}

#[test]
fn json_write_utf8pathbuf_1() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        path: Utf8PathBuf,
    }

    let original = FooBar {
        path: Utf8PathBuf::from("foo/bar/baz"),
    };

    let _json = facet_json::to_string(&original);

    Ok(())
}

#[test]
fn json_write_utf8pathbuf_2() -> Result<()> {
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
